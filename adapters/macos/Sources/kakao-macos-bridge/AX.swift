import Foundation
import ApplicationServices

/// Thin wrappers over the C Accessibility API. Element identification prefers
/// role / title / value / identifier over coordinates (skill guidance).

enum AX {
    /// Is this process trusted for the Accessibility API (TCC)? Silent check.
    static func isTrusted() -> Bool {
        AXIsProcessTrusted()
    }

    /// Trigger the one-time system prompt that adds this binary to the
    /// "손쉬운 사용" list. Called from `healthCheck` when not yet trusted
    /// (so `doctor` and first-run TUI both surface it); never on the read path.
    @discardableResult
    static func requestTrust() -> Bool {
        let key = "AXTrustedCheckOptionPrompt" as CFString
        let options = [key: kCFBooleanTrue as Any] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    static func app(pid: pid_t) -> AXUIElement {
        AXUIElementCreateApplication(pid)
    }

    /// Cap how long any single AX message to this element (and its subtree) may
    /// block. KakaoTalk is not always AX-cooperative; without this a full tree
    /// walk can stall for many seconds and blow the core's IPC timeout.
    static func setMessagingTimeout(_ element: AXUIElement, seconds: Float) {
        AXUIElementSetMessagingTimeout(element, seconds)
    }

    static func copyAttribute(_ element: AXUIElement, _ attribute: String) -> CFTypeRef? {
        var out: CFTypeRef?
        let err = AXUIElementCopyAttributeValue(element, attribute as CFString, &out)
        return err == .success ? out : nil
    }

    static func string(_ element: AXUIElement, _ attribute: String) -> String? {
        copyAttribute(element, attribute) as? String
    }

    static func elements(_ element: AXUIElement, _ attribute: String) -> [AXUIElement] {
        (copyAttribute(element, attribute) as? [AXUIElement]) ?? []
    }

    /// Read several attributes in ONE AX round trip. KakaoTalk's per-call AX
    /// latency is high (tens of ms), so batching role+identifier+value+... for
    /// an element cuts a 6-call element read to 1. Missing attributes come back
    /// as `nil` in the returned array (order matches `attributes`).
    static func multi(_ element: AXUIElement, _ attributes: [String]) -> [CFTypeRef?] {
        let cfAttrs = attributes.map { $0 as CFString } as CFArray
        var values: CFArray?
        let err = AXUIElementCopyMultipleAttributeValues(element, cfAttrs, [], &values)
        guard err == .success, let arr = values as? [AnyObject] else {
            return Array(repeating: nil, count: attributes.count)
        }
        // Absent attributes are represented as AXValue of type AXValueTypeIllegal
        // or as a wrapped error; treat anything not a plain value as nil.
        return arr.map { obj -> CFTypeRef? in
            if obj is NSNull { return nil }
            let tid = CFGetTypeID(obj)
            if tid == AXUIElementGetTypeID() || tid == CFStringGetTypeID()
                || tid == CFArrayGetTypeID() || tid == CFBooleanGetTypeID()
                || tid == CFNumberGetTypeID() {
                return obj
            }
            return nil
        }
    }

    @discardableResult
    static func setValue(_ element: AXUIElement, _ attribute: String, _ value: CFTypeRef) -> Bool {
        AXUIElementSetAttributeValue(element, attribute as CFString, value) == .success
    }

    @discardableResult
    static func perform(_ element: AXUIElement, _ action: String) -> Bool {
        AXUIElementPerformAction(element, action as CFString) == .success
    }

    /// Screen-coordinate frame of an element, or nil.
    static func frame(_ element: AXUIElement) -> CGRect? {
        guard let posVal = copyAttribute(element, kAXPositionAttribute as String),
              let sizeVal = copyAttribute(element, kAXSizeAttribute as String)
        else { return nil }
        var pos = CGPoint.zero
        var size = CGSize.zero
        guard AXValueGetValue(posVal as! AXValue, .cgPoint, &pos),
              AXValueGetValue(sizeVal as! AXValue, .cgSize, &size)
        else { return nil }
        return CGRect(origin: pos, size: size)
    }
}

/// Live `UINode` backed by an `AXUIElement`. Each property is a separate AX
/// round trip — fine for navigation, too slow for bulk reads (KakaoTalk's chat
/// list has 100+ rows). Use `snapshot(maxDepth:)` to pull a subtree in batched
/// reads and parse it in memory.
struct AXElement: UINode {
    let raw: AXUIElement

    var role: String { AX.string(raw, kAXRoleAttribute as String) ?? "" }
    var subrole: String? { AX.string(raw, kAXSubroleAttribute as String) }
    var title: String? { AX.string(raw, kAXTitleAttribute as String) }
    var value: String? { AX.string(raw, kAXValueAttribute as String) }
    var descriptionText: String? { AX.string(raw, kAXDescriptionAttribute as String) }
    var identifier: String? { AX.string(raw, kAXIdentifierAttribute as String) }
    var children: [UINode] {
        AX.elements(raw, kAXChildrenAttribute as String).map { AXElement(raw: $0) }
    }

    private static let snapshotAttrs = [
        kAXRoleAttribute, kAXSubroleAttribute, kAXTitleAttribute,
        kAXValueAttribute, kAXDescriptionAttribute, kAXIdentifierAttribute,
    ] as [String]

    /// Pull this element's subtree into `FixtureNode`s using one batched AX call
    /// per element. `maxDepth == 0` snapshots this element with no children.
    /// `childLimit` caps how many children are visited at each level.
    func snapshot(maxDepth: Int, childLimit: Int? = nil) -> FixtureNode {
        let v = AX.multi(raw, Self.snapshotAttrs)
        func str(_ i: Int) -> String? { v[i] as? String }

        var kids: [FixtureNode] = []
        if maxDepth > 0 {
            var childEls = AX.elements(raw, kAXChildrenAttribute as String)
            if let childLimit, childEls.count > childLimit {
                childEls = Array(childEls.prefix(childLimit))
            }
            kids = childEls.map {
                AXElement(raw: $0).snapshot(maxDepth: maxDepth - 1, childLimit: childLimit)
            }
        }
        // Capture the left edge only for text areas — the outgoing/incoming
        // signal is horizontal alignment of the message bubble.
        let minX: Double? = (str(0) == "AXTextArea") ? AX.frame(raw).map { Double($0.minX) } : nil

        return FixtureNode(
            role: str(0) ?? "",
            subrole: str(1),
            title: str(2),
            value: str(3),
            descriptionText: str(4),
            identifier: str(5),
            frameMinX: minX,
            children: kids
        )
    }
}
