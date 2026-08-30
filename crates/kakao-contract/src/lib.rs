//! Shared contract types for kakao-cli.
//!
//! This crate is the single definition of the common-core <-> OS-adapter
//! boundary. Both `kakao-core` and the Windows bridge depend on it so the JSON
//! shapes and error codes cannot drift. See `docs/adapter-contract.md` (v2.0.0).
//!
//! JSON boundary is camelCase. All timestamps are ISO 8601 UTC strings.
//!
//! Two transports share these types:
//!   * **serve mode** (primary) — a long-lived `kakao-<os>-bridge serve` process
//!     speaking newline-delimited JSON: [`ServeRequest`] in, [`ServeResponse`] /
//!     [`ServeEvent`] out. Drives the interactive TUI.
//!   * **one-shot** (retained) — `kakao-<os>-bridge <method> <argsJson>` writing a
//!     single [`AdapterResponse`] line. Used only by `doctor` (healthCheck) and
//!     the bridge self-tests.

use serde::{Deserialize, Serialize};

/// Contract version this crate implements. Bump together with
/// `docs/adapter-contract.md`.
pub const CONTRACT_VERSION: &str = "2.0.0";

// ===========================================================================
// Error codes (closed enum). Adapters MUST NOT return any string outside this.
// ===========================================================================

/// Closed set of adapter error codes. `ROOM_AMBIGUOUS` is intentionally absent:
/// same-name rooms are resolved by the core, never reported by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    KakaoNotRunning,
    /// The app is running but no window is accessible (minimized, hidden, or
    /// on another Space). Added in contract 1.1.0.
    KakaoWindowNotVisible,
    AccessibilityPermissionDenied,
    AppVersionUnsupported,
    RoomNotFound,
    UiElementNotFound,
    SendInputFailed,
    SendVerifyTimeout,
    EmptyMessage,
    MessageTooLong,
}

impl ErrorCode {
    /// Process exit code the CLI returns when this error surfaces to the user.
    /// Mirrors `docs/adapter-contract.md` §4 and `docs/command-spec.md`.
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::KakaoNotRunning => 4,
            ErrorCode::KakaoWindowNotVisible => 4,
            ErrorCode::AccessibilityPermissionDenied => 3,
            ErrorCode::AppVersionUnsupported => 3,
            ErrorCode::RoomNotFound => 2,
            ErrorCode::UiElementNotFound => 3,
            ErrorCode::SendInputFailed => 7,
            ErrorCode::SendVerifyTimeout => 6,
            ErrorCode::EmptyMessage => 8,
            ErrorCode::MessageTooLong => 8,
        }
    }

    /// Stable machine-readable string form (matches the JSON wire value).
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::KakaoNotRunning => "KAKAO_NOT_RUNNING",
            ErrorCode::KakaoWindowNotVisible => "KAKAO_WINDOW_NOT_VISIBLE",
            ErrorCode::AccessibilityPermissionDenied => "ACCESSIBILITY_PERMISSION_DENIED",
            ErrorCode::AppVersionUnsupported => "APP_VERSION_UNSUPPORTED",
            ErrorCode::RoomNotFound => "ROOM_NOT_FOUND",
            ErrorCode::UiElementNotFound => "UI_ELEMENT_NOT_FOUND",
            ErrorCode::SendInputFailed => "SEND_INPUT_FAILED",
            ErrorCode::SendVerifyTimeout => "SEND_VERIFY_TIMEOUT",
            ErrorCode::EmptyMessage => "EMPTY_MESSAGE",
            ErrorCode::MessageTooLong => "MESSAGE_TOO_LONG",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ===========================================================================
// Interface function payloads (docs/adapter-contract.md §2)
// ===========================================================================

/// A chat room as returned by `listRooms`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    /// Opaque, session-stable handle. The core never parses this.
    pub room_id: String,
    pub title: String,
    /// `null` when the adapter cannot read it without opening the room.
    /// Parity rule: if one adapter returns null here, the other must too.
    pub member_count: Option<u32>,
    #[serde(default)]
    pub unread_count: u32,
    pub last_message: Option<LastMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastMessage {
    pub text: String,
    /// ISO 8601 UTC.
    pub at: String,
    pub sender: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListRoomsData {
    pub rooms: Vec<Room>,
}

/// A single message from `readRecent`. Order: oldest -> newest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub sender: String,
    pub text: String,
    /// ISO 8601 UTC.
    pub at: String,
    pub outgoing: bool,
    pub kind: MessageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    Text,
    /// Photo / file / emoticon / special message. `text` is "" in this case.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadRecentData {
    pub messages: Vec<Message>,
}

/// `sendText` result. Status transitions are governed by the send state
/// machine (`docs/adapter-contract.md` §3). The adapter never retries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendResult {
    pub status: SendStatus,
    /// Set only when `status == Sent` (ISO 8601 UTC verification time).
    pub at: Option<String>,
    /// Set only when `status` is `Failed` or `Unknown`.
    pub error: Option<ErrorCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SendStatus {
    Sent,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub kakao_running: bool,
    /// macOS = TCC trusted; Windows = UIA can read the main window element.
    pub accessibility_granted: bool,
    pub app_version: Option<String>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub code: ErrorCode,
    /// Short default guidance. The core may override this from `docs/errors.md`.
    pub recovery: String,
}

// ===========================================================================
// IPC envelope (docs/adapter-contract.md §1)
// ===========================================================================

/// One-line JSON response an adapter writes to stdout.
///
/// Success carries `data` shaped per method; failure carries `error`.
/// `openRoom` success uses `data = {}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdapterResponse {
    Ok { ok: OkTrue, data: serde_json::Value },
    Err { ok: OkFalse, error: ErrorCode },
}

/// Serializes/deserializes only as the literal `true`.
#[derive(Debug, Clone, Copy)]
pub struct OkTrue;
/// Serializes/deserializes only as the literal `false`.
#[derive(Debug, Clone, Copy)]
pub struct OkFalse;

impl Serialize for OkTrue {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}
impl<'de> Deserialize<'de> for OkTrue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match bool::deserialize(d)? {
            true => Ok(OkTrue),
            false => Err(serde::de::Error::custom("expected ok:true")),
        }
    }
}
impl Serialize for OkFalse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(false)
    }
}
impl<'de> Deserialize<'de> for OkFalse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match bool::deserialize(d)? {
            false => Ok(OkFalse),
            true => Err(serde::de::Error::custom("expected ok:false")),
        }
    }
}

impl AdapterResponse {
    pub fn ok(data: serde_json::Value) -> Self {
        AdapterResponse::Ok { ok: OkTrue, data }
    }
    pub fn err(error: ErrorCode) -> Self {
        AdapterResponse::Err { ok: OkFalse, error }
    }
}

/// The contract methods, used to build a request in either transport.
///
/// `ListRooms` / `OpenRoom` / `ReadRecent` / `SendText` / `HealthCheck` exist in
/// both transports; `Watch` / `Unwatch` / `Shutdown` are serve-mode only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    ListRooms,
    OpenRoom,
    ReadRecent,
    SendText,
    HealthCheck,
    Watch,
    Unwatch,
    Shutdown,
}

impl Method {
    pub fn wire_name(self) -> &'static str {
        match self {
            Method::ListRooms => "listRooms",
            Method::OpenRoom => "openRoom",
            Method::ReadRecent => "readRecent",
            Method::SendText => "sendText",
            Method::HealthCheck => "healthCheck",
            Method::Watch => "watch",
            Method::Unwatch => "unwatch",
            Method::Shutdown => "shutdown",
        }
    }

    pub fn from_wire(name: &str) -> Option<Self> {
        Some(match name {
            "listRooms" => Method::ListRooms,
            "openRoom" => Method::OpenRoom,
            "readRecent" => Method::ReadRecent,
            "sendText" => Method::SendText,
            "healthCheck" => Method::HealthCheck,
            "watch" => Method::Watch,
            "unwatch" => Method::Unwatch,
            "shutdown" => Method::Shutdown,
            _ => return None,
        })
    }

    /// Per-call IPC timeout in milliseconds for the **one-shot** transport
    /// (docs/adapter-contract.md §1). The non-send budget accommodates a cold
    /// accessibility-tree walk on a large KakaoTalk window. Serve mode does not
    /// use this — its requests are answered from a warm context.
    pub fn timeout_ms(self) -> u64 {
        match self {
            Method::SendText => 12_000,
            _ => 8_000,
        }
    }
}

// ===========================================================================
// Serve-mode framing (docs/adapter-contract.md §5)
// ===========================================================================

/// One newline-delimited request the core writes to the serve process's stdin.
///
/// `id` correlates the matching [`ServeResponse`]. `id` 0 is reserved for
/// `shutdown` (which gets no response). `params` shape is per method, mirroring
/// the one-shot `argsJson` (e.g. `{"roomId": "...", "limit": 40}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl ServeRequest {
    pub fn new(id: u64, method: Method, params: serde_json::Value) -> Self {
        Self { id, method: method.wire_name().to_string(), params }
    }
}

/// A reply to exactly one [`ServeRequest`], correlated by `id`.
///
/// Success: `{"id":3,"ok":true,"data":<shape>}`.
/// Failure: `{"id":4,"ok":false,"error":"<CODE>"}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorCode>,
}

impl ServeResponse {
    pub fn ok(id: u64, data: serde_json::Value) -> Self {
        Self { id, ok: true, data: Some(data), error: None }
    }
    pub fn err(id: u64, error: ErrorCode) -> Self {
        Self { id, ok: false, data: None, error: Some(error) }
    }
}

/// An unsolicited message the serve process pushes between responses.
///
/// * `message` — a newly appended message in the watched room. The bridge owns
///   de-duplication; the core appends every one it receives.
/// * `roomClosed` — the watched conversation is no longer open in KakaoTalk
///   (the user navigated away). The core stops expecting messages until the
///   next `watch`.
/// * `error` — a transient condition while watching (e.g. window minimized).
///   Advisory; the bridge keeps retrying.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum ServeEvent {
    #[serde(rename = "message", rename_all = "camelCase")]
    Message { room_id: String, message: Message },
    #[serde(rename = "roomClosed", rename_all = "camelCase")]
    RoomClosed { room_id: String },
    #[serde(rename = "error")]
    Error { code: ErrorCode },
}

/// Anything the core reads from the serve process's stdout: a correlated
/// [`ServeResponse`] or an unsolicited [`ServeEvent`].
#[derive(Debug, Clone)]
pub enum ServeMessage {
    Response(ServeResponse),
    Event(ServeEvent),
}

impl ServeMessage {
    /// Parse one stdout line. An object carrying an `"event"` key is an event;
    /// anything else is a response.
    pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_str(line)?;
        if value.get("event").is_some() {
            Ok(ServeMessage::Event(serde_json::from_value(value)?))
        } else {
            Ok(ServeMessage::Response(serde_json::from_value(value)?))
        }
    }
}
