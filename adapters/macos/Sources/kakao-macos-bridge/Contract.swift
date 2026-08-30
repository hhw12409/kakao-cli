import Foundation

/// Types mirroring `docs/adapter-contract.md` v2.0.0.
///
/// JSON boundary is camelCase (the default here). All timestamps are ISO 8601
/// UTC strings. `roomId` is an opaque string the core never parses.

let contractVersion = "2.0.0"

// MARK: - Error codes (closed set; must match kakao-contract::ErrorCode)

enum ErrorCode: String, Codable {
    case kakaoNotRunning = "KAKAO_NOT_RUNNING"
    case kakaoWindowNotVisible = "KAKAO_WINDOW_NOT_VISIBLE"
    case accessibilityPermissionDenied = "ACCESSIBILITY_PERMISSION_DENIED"
    case appVersionUnsupported = "APP_VERSION_UNSUPPORTED"
    case roomNotFound = "ROOM_NOT_FOUND"
    case uiElementNotFound = "UI_ELEMENT_NOT_FOUND"
    case sendInputFailed = "SEND_INPUT_FAILED"
    case sendVerifyTimeout = "SEND_VERIFY_TIMEOUT"
    case emptyMessage = "EMPTY_MESSAGE"
    case messageTooLong = "MESSAGE_TOO_LONG"
}

/// Thrown by bridge functions; `main` turns it into `{"ok":false,"error":...}`.
struct BridgeError: Error {
    let code: ErrorCode
    /// stderr-only diagnostic. NEVER a message body.
    let diagnostic: String?

    init(_ code: ErrorCode, _ diagnostic: String? = nil) {
        self.code = code
        self.diagnostic = diagnostic
    }
}

// MARK: - listRooms

struct Room: Codable, Equatable {
    let roomId: String
    let title: String
    /// null when unreadable without opening the room (parity rule with Windows).
    let memberCount: Int?
    let unreadCount: Int
    let lastMessage: LastMessage?
}

struct LastMessage: Codable, Equatable {
    let text: String
    let at: String       // ISO 8601 UTC
    let sender: String
}

struct ListRoomsData: Codable, Equatable {
    let rooms: [Room]
}

// MARK: - readRecent

enum MessageKind: String, Codable {
    case text
    case unsupported
}

struct Message: Codable, Equatable {
    let sender: String
    let text: String
    let at: String       // ISO 8601 UTC
    let outgoing: Bool
    let kind: MessageKind
}

struct ReadRecentData: Codable, Equatable {
    let messages: [Message]
}

// MARK: - sendText

enum SendStatus: String, Codable {
    case sent
    case failed
    case unknown
}

struct SendResult: Codable, Equatable {
    let status: SendStatus
    let at: String?          // set only when status == .sent
    let error: ErrorCode?    // set only when status == .failed | .unknown
}

// MARK: - healthCheck

struct Issue: Codable, Equatable {
    let code: ErrorCode
    let recovery: String
}

struct Health: Codable, Equatable {
    let kakaoRunning: Bool
    let accessibilityGranted: Bool
    let appVersion: String?
    let issues: [Issue]
}

// MARK: - serve-mode framing (docs/adapter-contract.md §5)

/// Success reply to one `ServeRequest`, correlated by `id`.
struct ServeOk<T: Encodable>: Encodable {
    let id: UInt64
    let ok = true
    let data: T
}

/// Failure reply to one `ServeRequest`.
struct ServeErr: Encodable {
    let id: UInt64
    let ok = false
    let error: ErrorCode
}

/// `data` payload for methods that just acknowledge (openRoom/watch/unwatch).
struct EmptyData: Encodable {}

/// Unsolicited: a newly appended message in the watched room.
struct MessageEvent: Encodable {
    let event = "message"
    let roomId: String
    let message: Message
}

/// Unsolicited: the watched conversation is no longer open in KakaoTalk.
struct RoomClosedEvent: Encodable {
    let event = "roomClosed"
    let roomId: String
}

/// Unsolicited: a transient watch condition (advisory).
struct ErrorEvent: Encodable {
    let event = "error"
    let code: ErrorCode
}

// MARK: - time

enum ISO8601 {
    static let formatter: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        f.timeZone = TimeZone(identifier: "UTC")
        return f
    }()

    static func now() -> String { formatter.string(from: Date()) }
    static func string(from date: Date) -> String { formatter.string(from: date) }
}
