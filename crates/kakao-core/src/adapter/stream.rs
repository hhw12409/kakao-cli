//! Streaming adapter interface — the transport the interactive TUI drives.
//!
//! Unlike the one-shot [`Adapter`](super::Adapter) (kept only for `doctor`), a
//! `StreamAdapter` owns a live `kakao-<os>-bridge serve` session. Request
//! methods are synchronous round-trips over that session; [`next_event`] drains
//! the unsolicited message stream that `watch` produces.
//!
//! [`next_event`]: StreamAdapter::next_event

use std::time::Duration;

use kakao_contract::{ErrorCode, Health, ListRoomsData, Message, ReadRecentData, SendResult};

use crate::error::AppResult;

/// Something the session surfaces between (or without) request responses.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A newly appended message in `room_id`. The bridge owns de-duplication;
    /// the core appends every one it receives.
    Message { room_id: String, message: Message },
    /// The watched conversation is no longer open in KakaoTalk.
    RoomClosed { room_id: String },
    /// A transient watch condition (e.g. window minimized). Advisory.
    Warn(ErrorCode),
    /// The session ended — process exited or the stream broke. Terminal.
    Disconnected(String),
}

pub trait StreamAdapter: Send {
    fn list_rooms(&mut self) -> AppResult<ListRoomsData>;
    fn open_room(&mut self, room_id: &str) -> AppResult<()>;
    fn read_recent(&mut self, room_id: &str, limit: u32) -> AppResult<ReadRecentData>;
    fn send_text(&mut self, room_id: &str, text: &str) -> AppResult<SendResult>;
    /// Start polling `room_id` for new messages. Replaces any previous watch.
    fn watch(&mut self, room_id: &str) -> AppResult<()>;
    /// Stop polling. Idempotent.
    fn unwatch(&mut self) -> AppResult<()>;
    fn health_check(&mut self) -> AppResult<Health>;
    /// Wait up to `timeout` for the next [`StreamEvent`]. `None` on timeout.
    fn next_event(&mut self, timeout: Duration) -> Option<StreamEvent>;
    /// Best-effort clean shutdown of the underlying session.
    fn shutdown(&mut self);
}
