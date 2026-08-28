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
    /// "손쉬운 사용" list. Used by `doctor` only, never on the read path.
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

    @discardableResult
    static func setValue(_ element: AXUIElement, _ attribute: String, _ value: CFTypeRef) -> Bool {
        AXUIElementSetAttributeValue(element, attribute as CFString, value) == .success
    }

    @discardableResult
    static func perform(_ element: AXUIElement, _ action: String) -> Bool {
        AXUIElementPerformAction(element, action as CFString) == .success
    }
}

/// Live `UINode` backed by an `AXUIElement`.
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
}
