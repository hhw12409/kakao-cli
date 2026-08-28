//! Shared contract types for kakao-cli.
//!
//! This crate is the single definition of the common-core <-> OS-adapter
//! boundary. Both `kakao-core` and the Windows bridge depend on it so the JSON
//! shapes and error codes cannot drift. See `docs/adapter-contract.md` (v1.0.0).
//!
//! JSON boundary is camelCase. All timestamps are ISO 8601 UTC strings.

use serde::{Deserialize, Serialize};

/// Contract version this crate implements. Bump together with
/// `docs/adapter-contract.md`.
pub const CONTRACT_VERSION: &str = "1.0.0";

// ===========================================================================
// Error codes (closed enum). Adapters MUST NOT return any string outside this.
// ===========================================================================

/// Closed set of adapter error codes. `ROOM_AMBIGUOUS` is intentionally absent:
/// same-name rooms are resolved by the core, never reported by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    KakaoNotRunning,
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

/// The five contract methods, used to build the argv request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    ListRooms,
    OpenRoom,
    ReadRecent,
    SendText,
    HealthCheck,
}

impl Method {
    pub fn wire_name(self) -> &'static str {
        match self {
            Method::ListRooms => "listRooms",
            Method::OpenRoom => "openRoom",
            Method::ReadRecent => "readRecent",
            Method::SendText => "sendText",
            Method::HealthCheck => "healthCheck",
        }
    }

    /// Per-method IPC timeout in milliseconds (docs/adapter-contract.md §1).
    /// The non-send budget accommodates a cold accessibility-tree walk on a
    /// large KakaoTalk window.
    pub fn timeout_ms(self) -> u64 {
        match self {
            Method::SendText => 12_000,
            _ => 8_000,
        }
    }
}
