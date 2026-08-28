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

    /// Message cell layout (KakaoTalk 26.6.1 group chat):
    ///
    ///   [AXButton desc="프로필"]     — only on the first msg of a sender run
    ///   [AXStaticText  time]          — only on the first msg of a time run ("오후 11:07",
    ///                                   sometimes "1\n오전 1:01")
    ///   [AXStaticText  sender name]   — only alongside the profile button
    ///   AXImage
    ///   AXTextArea  body              — absent -> file/photo/sticker = unsupported
    ///
    /// Time and sender both carry forward to following rows that omit them.
    static func messages(
        in conversationWindow: UINode,
        selectors: SelectorMap,
        myName: String?,
        windowMinX: Double? = nil,
        now: Date = Date()
    ) -> [Message] {
        guard let table = conversationWindow.firstDescendant(where: {
            $0.identifier == selectors.messageTableIdentifier
        }) else { return [] }

        var out: [Message] = []
        var currentAt = ""
        var currentSender = ""

        for row in table.children where row.role == "AXRow" {
            guard let cell = row.firstDescendant(where: { $0.role == "AXCell" }) else { continue }

            let statics = cell.children.filter { $0.role == "AXStaticText" }
            let hasProfile = cell.children.contains {
                $0.role == "AXButton" && $0.descriptionText == "프로필"
            }

            // The static text whose last line is a clock is the timestamp.
            if let tsText = statics.compactMap({ $0.anyText }).first(where: {
                KoreanTime.parseHourMinute($0) != nil
            }), let iso = optional(KoreanTime.toISO(tsText, now: now)) {
                currentAt = iso
            }
            // A profile button means a new sender run starts here; its name is
            // the non-clock static text.
            if hasProfile, let name = statics.compactMap({ $0.anyText }).first(where: {
                KoreanTime.parseHourMinute($0) == nil
            }) {
                currentSender = name
            }

            let bodyNode = cell.children.first(where: { $0.role == "AXTextArea" })
            let body = bodyNode?.anyText

            if let body, !body.isEmpty {
                let outgoing = isOutgoing(
                    bubbleMinX: bodyNode?.frameMinX,
                    windowMinX: windowMinX,
                    hasProfile: hasProfile,
                    sender: currentSender,
                    myName: myName
                )
                out.append(Message(
                    sender: outgoing ? "" : currentSender,   // my messages carry no sender label
                    text: body,
                    at: currentAt,
                    outgoing: outgoing,
                    kind: .text
                ))
            } else if isMediaCell(cell) {
                out.append(Message(
                    sender: currentSender, text: "", at: currentAt, outgoing: false, kind: .unsupported
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

    /// Outgoing = the message bubble is right-aligned. Incoming bubbles sit at
    /// `windowMinX + ~60`; outgoing bubbles are pushed right and their left
    /// edge is well past window-left + a threshold (verified: incoming taX ≈
    /// win+60, outgoing taX ≥ win+150). Falls back to `sender == myName` when
    /// geometry is unavailable, then to "has a profile button ⇒ incoming".
    static func isOutgoing(
        bubbleMinX: Double?,
        windowMinX: Double?,
        hasProfile: Bool,
        sender: String,
        myName: String?
    ) -> Bool {
        if let bx = bubbleMinX, let wx = windowMinX {
            return (bx - wx) > 120
        }
        if let myName, !sender.isEmpty { return sender == myName }
        return false   // conservative: unknown -> treat as incoming
    }

    // MARK: helpers

    private static func optional(_ s: String) -> String? { s.isEmpty ? nil : s }
}
