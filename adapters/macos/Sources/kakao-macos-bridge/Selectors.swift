import Foundation

/// Version-specific accessibility selectors for KakaoTalk Mac. UI updates are
/// the project's largest risk, so every selector lives here keyed by app
/// version instead of being scattered through the parsers. `healthCheck`
/// reports `APP_VERSION_UNSUPPORTED` when no entry matches.
///
/// Values below are from a real `--dump-tree` of KakaoTalk 26.6.1 (see
/// `_workspace/ax-dumps/`, gitignored). KakaoTalk reuses the same `_NS:*`
/// identifiers across every row, so identifiers alone don't identify a row —
/// they DO reliably tag the field *within* a row/window, which is how the
/// parsers use them.
struct SelectorMap {
    // --- main window (chat list) ---
    /// `AXIdentifier` of the main window.
    var mainWindowIdentifier: String
    /// Title of the main window (fallback when the identifier is absent).
    var mainWindowTitle: String
    /// `AXIdentifier` of the left-rail "채팅"(chat list) tab button. The chat
    /// table is only in the AX tree while this tab is selected.
    var chatListTabIdentifier: String
    /// `AXIdentifier` of the AXTable holding chat rows.
    var roomTableIdentifier: String
    /// Within a room `AXCell`: identifier of the title static text.
    var roomTitleIdentifier: String
    /// Within a room `AXCell`: identifier of the member-count static text
    /// (present for group chats only).
    var roomMemberCountIdentifier: String
    /// Within a room `AXCell`: identifier of the timestamp static text.
    var roomTimestampIdentifier: String
    /// Within a room `AXCell`: identifier of the last-message preview text area.
    var roomPreviewIdentifier: String

    // --- conversation window ---
    /// `AXIdentifier` of the AXScrollArea wrapping the message table.
    var messageScrollAreaIdentifier: String
    /// `AXIdentifier` of the AXTable holding message rows.
    var messageTableIdentifier: String
    /// `AXIdentifier` of the compose text area.
    var composeFieldIdentifier: String
    /// `AXDescription` of the compose text area (fallback / cross-check).
    var composeFieldDescription: String
    /// `AXTitle` of the send button.
    var sendButtonTitle: String

    static func forVersion(_ version: String?) -> SelectorMap? {
        guard let version else { return known["*"] }
        if let exact = known[version] { return exact }
        let parts = version.split(separator: ".")
        if parts.count >= 2 {
            let prefix = "\(parts[0]).\(parts[1])"
            if let byPrefix = known.first(where: { $0.key.hasPrefix(prefix) })?.value {
                return byPrefix
            }
        }
        return known["*"]
    }

    static let v26_6 = SelectorMap(
        mainWindowIdentifier: "Main Window",
        mainWindowTitle: "카카오톡",
        chatListTabIdentifier: "chatrooms",
        roomTableIdentifier: "_NS:63",
        roomTitleIdentifier: "_NS:40",
        roomMemberCountIdentifier: "Count Label",
        roomTimestampIdentifier: "_NS:69",
        roomPreviewIdentifier: "_NS:91",
        messageScrollAreaIdentifier: "_NS:29",
        messageTableIdentifier: "_NS:33",
        composeFieldIdentifier: "_NS:51",
        composeFieldDescription: "메시지 입력",
        sendButtonTitle: "전송"
    )

    /// KakaoTalk 26.5.0 — verified with `--probe-rooms` against a real install.
    /// Chat-list selectors are identical to 26.6; the visible difference is that
    /// the chat table is absent from the tree unless the 채팅 tab is selected,
    /// which the bridge now does before reading. Message-window / compose
    /// selectors are inherited from 26.6 and still need a live conversation dump.
    static let v26_5 = SelectorMap(
        mainWindowIdentifier: "Main Window",
        mainWindowTitle: "카카오톡",
        chatListTabIdentifier: "chatrooms",
        roomTableIdentifier: "_NS:63",
        roomTitleIdentifier: "_NS:40",
        roomMemberCountIdentifier: "Count Label",
        roomTimestampIdentifier: "_NS:69",
        roomPreviewIdentifier: "_NS:91",
        messageScrollAreaIdentifier: "_NS:29",
        messageTableIdentifier: "_NS:33",
        composeFieldIdentifier: "_NS:51",
        composeFieldDescription: "메시지 입력",
        sendButtonTitle: "전송"
    )

    /// `"*"` = development default / fallback for unknown versions.
    static let known: [String: SelectorMap] = [
        "*": v26_6,
        "26.6": v26_6,
        "26.5": v26_5,
    ]
}
