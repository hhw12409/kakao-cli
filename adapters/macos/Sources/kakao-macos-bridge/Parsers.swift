import Foundation

/// Pure tree -> contract-type parsers. They take a `UINode`, so the same code
/// runs against the live AX tree and against JSON fixtures in tests.
///
/// Structure (KakaoTalk 26.6.1, from a real `--dump-tree`):
///
///   main window "카카오톡"
///     AXScrollArea > AXTable(roomTableIdentifier) > AXRow[AXTableRow] > AXCell
///       AXStaticText(roomTitleIdentifier)          room title
///       AXStaticText("Count Label")                member count (groups only)
///       AXStaticText(roomTimestampIdentifier)      "오전 2:06" / "어제"
///       AXScrollArea > AXTextArea(roomPreviewId)   last message preview
///       AXStaticText(no id, numeric)               unread badge (absent = 0)
///
///   conversation window (title == room title)
///     AXScrollArea > AXTable(messageTableIdentifier) > AXRow > AXCell
///       AXStaticText "오전 11:17"  (only on the first msg of a time group)
///       AXTextArea                 message body   (absent -> file/media = unsupported)
enum Parsers {

    // MARK: listRooms

    static func rooms(in mainWindow: UINode, selectors: SelectorMap, now: Date = Date()) -> [Room] {
        guard let table = mainWindow.firstDescendant(where: {
            $0.identifier == selectors.roomTableIdentifier
        }) else { return [] }

        let rows = table.children.filter { $0.role == "AXRow" }
        return rows.enumerated().compactMap { index, row in
            guard let cell = row.firstDescendant(where: { $0.role == "AXCell" }) else { return nil }

            let title = fieldValue(cell, id: selectors.roomTitleIdentifier)
            guard let title, !title.isEmpty else { return nil }  // spacer / divider rows

            let preview = fieldValue(cell, id: selectors.roomPreviewIdentifier) ?? ""
            let tsLabel = fieldValue(cell, id: selectors.roomTimestampIdentifier) ?? ""
            let at = KoreanTime.toISO(tsLabel, now: now)

            return Room(
                roomId: "row:\(index)",
                title: title,
                memberCount: memberCount(cell, selectors: selectors),
                unreadCount: unreadBadge(cell, selectors: selectors),
                lastMessage: preview.isEmpty
                    ? nil
                    : LastMessage(text: preview, at: at, sender: "")
            )
        }
    }

    /// Value of the `AXStaticText` / `AXTextArea` in this cell with `identifier == id`.
    static func fieldValue(_ cell: UINode, id: String) -> String? {
        cell.firstDescendant(where: { $0.identifier == id })?.anyText
    }

    /// `Count Label` present -> that number. Absent -> 2 (a 1:1 chat).
    static func memberCount(_ cell: UINode, selectors: SelectorMap) -> Int? {
        if let raw = fieldValue(cell, id: selectors.roomMemberCountIdentifier),
           let n = Int(raw.filter(\.isNumber)) {
            return n
        }
        return 2
    }

    /// The unread badge is a bare numeric `AXStaticText` with no identifier
    /// (the member-count label has `identifier == "Count Label"`, so it is
    /// excluded). Absent -> 0.
    static func unreadBadge(_ cell: UINode, selectors: SelectorMap) -> Int {
        for node in cell.descendants(where: { $0.role == "AXStaticText" }) {
            guard node.identifier == nil else { continue }
            if let t = node.anyText, !t.isEmpty, t.allSatisfy(\.isNumber), let n = Int(t) {
                return n
            }
        }
        return 0
    }

    // MARK: readRecent

    static func messages(
        in conversationWindow: UINode,
        selectors: SelectorMap,
        myName: String?,
        now: Date = Date()
    ) -> [Message] {
        guard let table = conversationWindow.firstDescendant(where: {
            $0.identifier == selectors.messageTableIdentifier
        }) else { return [] }

        var out: [Message] = []
        var lastSeenAt = ""   // KakaoTalk shows the time only on the first msg of a group

        for row in table.children where row.role == "AXRow" {
            guard let cell = row.firstDescendant(where: { $0.role == "AXCell" }) else { continue }

            // Timestamp label, if this row carries one.
            if let tsNode = cell.children.first(where: {
                $0.role == "AXStaticText" && KoreanTime.parseHourMinute($0.anyText ?? "") != nil
            }), let iso = optional(KoreanTime.toISO(tsNode.anyText ?? "", now: now)) {
                lastSeenAt = iso
            }

            let body = cell.children.first(where: { $0.role == "AXTextArea" })?.anyText

            if let body, !body.isEmpty {
                out.append(Message(
                    sender: "",                     // TODO: needs a group-chat dump to locate
                    text: body,
                    at: lastSeenAt,
                    outgoing: outgoingHeuristic(cell: cell, sender: "", myName: myName),
                    kind: .text
                ))
            } else if isMediaCell(cell) {
                out.append(Message(
                    sender: "", text: "", at: lastSeenAt, outgoing: false, kind: .unsupported
                ))
            }
            // else: spacer / date divider row -> skip
        }
        return out
    }

    /// File / photo / sticker cells have a `공유` (share) button and named
    /// static texts but no `AXTextArea`.
    static func isMediaCell(_ cell: UINode) -> Bool {
        let hasShare = cell.descendants(where: {
            $0.role == "AXButton" && $0.descriptionText == "공유"
        }).isEmpty == false
        let hasImage = cell.descendants(where: { $0.role == "AXImage" }).isEmpty == false
        return hasShare || hasImage
    }

    /// Placeholder: without a per-message sender or a captured alignment/position
    /// signal we cannot yet tell outgoing from incoming reliably. Resolve with a
    /// 1:1 + group dump (check `AXPosition` x, cell subrole, or a style attr).
    static func outgoingHeuristic(cell: UINode, sender: String, myName: String?) -> Bool {
        if let myName, !myName.isEmpty, sender == myName { return true }
        return false
    }

    // MARK: helpers

    private static func optional(_ s: String) -> String? { s.isEmpty ? nil : s }
}
