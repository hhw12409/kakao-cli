import Foundation

/// Version-specific accessibility selectors. KakaoTalk UI updates are the
/// project's largest risk, so selectors live in one map keyed by app version
/// instead of being scattered through the parsers. `healthCheck` reports
/// `APP_VERSION_UNSUPPORTED` when no entry matches.
///
/// The concrete values below are PLACEHOLDERS pending inspection of a real
/// KakaoTalk build with Accessibility Inspector / a `--dump-tree` capture.
/// They encode the *shape* of what each parser needs; the QA + PoC pass fills
/// in the real role/identifier strings.
struct SelectorMap {
    /// AXRole of the chat-list container (the "채팅" tab list).
    var roomListRole: String
    /// AXRole of a single row inside the chat list.
    var roomRowRole: String
    /// AXRole carrying the unread-count badge text within a row.
    var unreadBadgeRole: String
    /// AXRole of the message-area container inside an open room.
    var messageAreaRole: String
    /// AXRole of a single message bubble.
    var messageBubbleRole: String
    /// AXRole of the compose text field.
    var composeFieldRole: String
    /// AXRole/label of the send button (matched on title/description).
    var sendButtonLabel: String

    /// Lookup by `CFBundleShortVersionString`. Falls back to the newest known
    /// entry for a prefix match, else `nil`.
    static func forVersion(_ version: String?) -> SelectorMap? {
        guard let version else { return known["*"] }
        if let exact = known[version] { return exact }
        // Match on major.minor prefix (e.g. "3.8" for "3.8.4.xxxx").
        let parts = version.split(separator: ".")
        if parts.count >= 2 {
            let prefix = "\(parts[0]).\(parts[1])"
            if let byPrefix = known.first(where: { $0.key.hasPrefix(prefix) })?.value {
                return byPrefix
            }
        }
        return known["*"]
    }

    /// `"*"` is the development default so the parsers and fixture tests have
    /// something to run against before a real version is characterised.
    static let known: [String: SelectorMap] = [
        "*": SelectorMap(
            roomListRole: "AXList",
            roomRowRole: "AXRow",
            unreadBadgeRole: "AXStaticText",
            messageAreaRole: "AXScrollArea",
            messageBubbleRole: "AXGroup",
            composeFieldRole: "AXTextArea",
            sendButtonLabel: "전송"
        ),
    ]
}
