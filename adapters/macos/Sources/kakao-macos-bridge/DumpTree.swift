import Foundation
import ApplicationServices

/// `kakao-macos-bridge --dump-tree` — serialize the live KakaoTalk
/// accessibility tree to `FixtureNode` JSON on stdout. Used to capture
/// per-version fixtures for the parser tests. Not part of the contract.
///
/// KakaoTalk Mac uses a separate window for the chat list and for each open
/// conversation, so this dumps EVERY window (via kAXWindowsAttribute, which is
/// more complete than kAXChildrenAttribute on the app element) and reports the
/// window titles + which one is main/focused on stderr.
enum DumpTree {
    static func run() {
        guard let running = KakaoApp.running() else {
            FileHandle.standardError.write(Data("KakaoTalk is not running\n".utf8))
            exit(1)
        }
        guard AX.isTrusted() else {
            FileHandle.standardError.write(Data("accessibility permission not granted\n".utf8))
            exit(1)
        }

        let appRaw = AX.app(pid: running.pid)
        AX.setMessagingTimeout(appRaw, seconds: 2.0)

        // kAXWindowsAttribute is the canonical list but goes empty for KakaoTalk
        // when windows are minimized / the app is not frontmost. Fall back to
        // the app element's direct children filtered to windows.
        var windows = AX.elements(appRaw, kAXWindowsAttribute as String)
        var windowSource = "kAXWindowsAttribute"
        if windows.isEmpty {
            windows = AX.elements(appRaw, kAXChildrenAttribute as String)
                .filter { AX.string($0, kAXRoleAttribute as String) == "AXWindow" }
            windowSource = "kAXChildrenAttribute (fallback)"
        }

        var summary = "KakaoTalk \(KakaoApp.version(of: running) ?? "?") — "
            + "\(windows.count) window(s) via \(windowSource):\n"
        var dumped: [FixtureNode] = []
        for (i, w) in windows.enumerated() {
            let el = AXElement(raw: w)
            let title = el.title ?? "(no title)"
            let sub = el.subrole ?? "-"
            let childRoles = Set(el.children.map { $0.role }).sorted().joined(separator: ",")
            summary += "  [\(i)] \(sub)  title=\(title)  childRoles={\(childRoles)}\n"
            dumped.append(snapshot(el, depth: 0, maxDepth: 60))
        }
        FileHandle.standardError.write(Data(summary.utf8))

        // Also include whatever the app element exposes directly, for comparison.
        let root = FixtureNode(
            role: "AXApplication",
            title: "KakaoTalk",
            children: dumped
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .withoutEscapingSlashes, .sortedKeys]
        if let data = try? encoder.encode(root) {
            FileHandle.standardOutput.write(data)
            FileHandle.standardOutput.write(Data("\n".utf8))
        }
    }

    static func snapshot(_ node: UINode, depth: Int, maxDepth: Int) -> FixtureNode {
        let kids: [FixtureNode] = depth >= maxDepth
            ? []
            : node.children.map { snapshot($0, depth: depth + 1, maxDepth: maxDepth) }
        return FixtureNode(
            role: node.role,
            subrole: node.subrole,
            title: node.title,
            value: node.value,
            descriptionText: node.descriptionText,
            identifier: node.identifier,
            children: kids
        )
    }
}
