import Foundation

/// Pure tree -> contract-type parsers. They take a `UINode`, so the same code
/// runs against the live AX tree and against JSON fixtures in tests.
///
/// NOTE: the traversal heuristics below are written against the *expected*
/// KakaoTalk structure and the placeholder `SelectorMap`. Verifying them
/// against a real build (and adding per-version fixtures) is the PoC task.
enum Parsers {

    // MARK: listRooms

    static func rooms(in root: UINode, selectors: SelectorMap) -> [Room] {
        guard let list = root.firstDescendant(where: { $0.role == selectors.roomListRole })
        else { return [] }

        let rows = list.children.filter { $0.role == selectors.roomRowRole }
        return rows.enumerated().map { index, row in
            let title = roomTitle(of: row) ?? "(제목 없음)"
            return Room(
                roomId: row.identifier ?? "row:\(index)",
                title: title,
                memberCount: nil,                       // not available without opening the room
                unreadCount: unreadCount(of: row, selectors: selectors),
                lastMessage: lastMessage(of: row)
            )
        }
    }

    static func roomTitle(of row: UINode) -> String? {
        if let t = row.title?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty {
            return t
        }
        // First non-numeric static text is usually the room name.
        return row
            .descendants(where: { $0.role == "AXStaticText" })
            .compactMap { $0.anyText }
            .first(where: { !isBadgeNumber($0) })
    }

    static func unreadCount(of row: UINode, selectors: SelectorMap) -> Int {
        for node in row.descendants(where: { $0.role == selectors.unreadBadgeRole }) {
            if let t = node.anyText, isBadgeNumber(t), let n = Int(t) {
                return n
            }
        }
        return 0
    }

    static func lastMessage(of row: UINode) -> LastMessage? {
        // Preview text is typically the last static text that is not the title
        // and not a badge number. Timestamp/sender are often not exposed in the
        // list row; leave them empty rather than guessing wrong.
        let texts = row
            .descendants(where: { $0.role == "AXStaticText" })
            .compactMap { $0.anyText }
            .filter { !isBadgeNumber($0) }
        guard texts.count >= 2 else { return nil }
        return LastMessage(text: texts.last ?? "", at: "", sender: "")
    }

    // MARK: readRecent

    static func messages(in root: UINode, selectors: SelectorMap, myName: String?) -> [Message] {
        guard let area = root.firstDescendant(where: { $0.role == selectors.messageAreaRole })
        else { return [] }

        let bubbles = area.descendants(where: { $0.role == selectors.messageBubbleRole })
        return bubbles.compactMap { bubble in
            let texts = bubble
                .descendants(where: { $0.role == "AXStaticText" })
                .compactMap { $0.anyText }
            guard !texts.isEmpty else {
                // A bubble with no readable text = photo / file / emoticon.
                return Message(sender: "", text: "", at: "", outgoing: false, kind: .unsupported)
            }
            let sender = texts.count > 1 ? texts[0] : ""
            let body = texts.count > 1 ? texts[1...].joined(separator: "\n") : texts[0]
            let outgoing = myName.map { sender == $0 } ?? (bubble.subrole == "AXOutgoing")
            return Message(
                sender: sender,
                text: body,
                at: "",                      // timestamp extraction is version-specific; TODO in PoC
                outgoing: outgoing,
                kind: .text
            )
        }
    }

    // MARK: helpers

    static func isBadgeNumber(_ s: String) -> Bool {
        !s.isEmpty && s.allSatisfy { $0.isNumber }
    }
}
