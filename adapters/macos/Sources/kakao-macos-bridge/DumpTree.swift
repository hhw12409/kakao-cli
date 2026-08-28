import Foundation
import ApplicationServices

/// `kakao-macos-bridge --dump-tree` — serialize the live KakaoTalk
/// accessibility tree to `FixtureNode` JSON on stdout. Used to capture
/// per-version fixtures for the parser tests. Not part of the contract.
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
        let app = AXElement(raw: AX.app(pid: running.pid))
        let node = snapshot(app, depth: 0, maxDepth: 40)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .withoutEscapingSlashes, .sortedKeys]
        if let data = try? encoder.encode(node) {
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
