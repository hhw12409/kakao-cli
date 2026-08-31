//! In-process streaming mock. Lets the TUI (and `tests/tui_smoke.rs`) run with
//! no KakaoTalk and no subprocess.
//!
//! Enabled by `KAKAO_CLI_STREAM_MOCK=<file>`. Fixture shape:
//!
//! ```json
//! {
//!   "rooms": [ { "roomId": "row:0", "title": "가족", "memberCount": 4,
//!               "unreadCount": 0, "lastMessage": null } ],
//!   "history": { "row:0": [ { "sender": "엄마", "text": "밥 먹었니",
//!               "at": "2026-08-31T09:00:00Z", "outgoing": false, "kind": "text" } ] },
//!   "incoming": [ { "afterMs": 200, "roomId": "row:0",
//!               "message": { "sender": "아빠", "text": "치킨 어때",
//!               "at": "2026-08-31T09:01:00Z", "outgoing": false, "kind": "text" } } ]
//! }
//! ```
//!
//! `send_text` echoes the sent text back as an outgoing message event, the way
//! a real bridge's poll would once KakaoTalk shows it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use kakao_contract::{
    Health, ListRoomsData, Message, MessageKind, ReadRecentData, Room, SendResult, SendStatus,
};
use serde::Deserialize;

use crate::adapter::stream::{StreamAdapter, StreamEvent};
use crate::error::{AppError, AppResult};
use crate::time_util::now_utc_iso;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    #[serde(default)]
    rooms: Vec<Room>,
    #[serde(default)]
    history: HashMap<String, Vec<Message>>,
    #[serde(default)]
    incoming: Vec<Scripted>,
    #[serde(default)]
    health_check: Option<Health>,
    /// Force the result of every `send_text` (for state-machine tests).
    #[serde(default)]
    send_text: Option<SendResult>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Scripted {
    after_ms: u64,
    room_id: String,
    message: Message,
}

pub struct MockStreamAdapter {
    rooms: Vec<Room>,
    history: HashMap<String, Vec<Message>>,
    incoming: Vec<Scripted>,
    health: Health,
    forced_send: Option<SendResult>,
    /// When set, `list_rooms` / `open_room` fail with this code — used to
    /// exercise the offline / cached-view path. Shared so a test can flip it
    /// after the adapter has moved into the worker thread.
    unavailable: Arc<Mutex<Option<kakao_contract::ErrorCode>>>,
    watched: Option<String>,
    tx: Sender<StreamEvent>,
    rx: Receiver<StreamEvent>,
}

/// Handle a test keeps to toggle [`MockStreamAdapter`] availability at runtime.
#[derive(Clone)]
pub struct MockAvailability(Arc<Mutex<Option<kakao_contract::ErrorCode>>>);

impl MockAvailability {
    pub fn set(&self, code: Option<kakao_contract::ErrorCode>) {
        *self.0.lock().unwrap() = code;
    }
}

impl MockStreamAdapter {
    pub fn from_fixture_file(path: &Path) -> AppResult<Self> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            AppError::internal(format!("스트림 목 fixture 읽기 실패 ({}): {e}", path.display()))
        })?;
        Self::from_fixture_str(&raw)
    }

    pub fn from_fixture_str(raw: &str) -> AppResult<Self> {
        let fx: Fixture = serde_json::from_str(raw)
            .map_err(|e| AppError::internal(format!("스트림 목 fixture 파싱 실패: {e}")))?;
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            rooms: fx.rooms,
            history: fx.history,
            incoming: fx.incoming,
            health: fx.health_check.unwrap_or(Health {
                kakao_running: true,
                accessibility_granted: true,
                app_version: Some("mock".into()),
                issues: vec![],
            }),
            forced_send: fx.send_text,
            unavailable: Arc::new(Mutex::new(None)),
            watched: None,
            tx,
            rx,
        })
    }

    /// A handle to flip KakaoTalk availability from a test.
    pub fn availability(&self) -> MockAvailability {
        MockAvailability(Arc::clone(&self.unavailable))
    }
}

impl StreamAdapter for MockStreamAdapter {
    fn list_rooms(&mut self) -> AppResult<ListRoomsData> {
        if let Some(code) = *self.unavailable.lock().unwrap() {
            return Err(AppError::adapter(code));
        }
        Ok(ListRoomsData { rooms: self.rooms.clone() })
    }

    fn open_room(&mut self, room_id: &str) -> AppResult<()> {
        if let Some(code) = *self.unavailable.lock().unwrap() {
            return Err(AppError::adapter(code));
        }
        if self.rooms.iter().any(|r| r.room_id == room_id) {
            Ok(())
        } else {
            Err(AppError::adapter(kakao_contract::ErrorCode::RoomNotFound))
        }
    }

    fn read_recent(&mut self, room_id: &str, limit: u32) -> AppResult<ReadRecentData> {
        let all = self.history.get(room_id).cloned().unwrap_or_default();
        let start = all.len().saturating_sub(limit as usize);
        Ok(ReadRecentData { messages: all[start..].to_vec() })
    }

    fn send_text(&mut self, room_id: &str, text: &str) -> AppResult<SendResult> {
        if let Some(forced) = self.forced_send.clone() {
            return Ok(forced);
        }
        if text.trim().is_empty() {
            return Ok(SendResult {
                status: SendStatus::Failed,
                at: None,
                error: Some(kakao_contract::ErrorCode::EmptyMessage),
            });
        }
        let echo = Message {
            sender: String::new(),
            text: text.to_string(),
            at: now_utc_iso(),
            outgoing: true,
            kind: MessageKind::Text,
        };
        self.history.entry(room_id.to_string()).or_default().push(echo.clone());
        if self.watched.as_deref() == Some(room_id) {
            let tx = self.tx.clone();
            let room_id = room_id.to_string();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(120));
                let _ = tx.send(StreamEvent::Message { room_id, message: echo });
            });
        }
        Ok(SendResult { status: SendStatus::Sent, at: Some(now_utc_iso()), error: None })
    }

    fn watch(&mut self, room_id: &str) -> AppResult<()> {
        self.watched = Some(room_id.to_string());
        for s in self.incoming.iter().filter(|s| s.room_id == room_id).cloned() {
            let tx = self.tx.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(s.after_ms));
                let _ = tx.send(StreamEvent::Message {
                    room_id: s.room_id,
                    message: s.message,
                });
            });
        }
        Ok(())
    }

    fn unwatch(&mut self) -> AppResult<()> {
        self.watched = None;
        Ok(())
    }

    fn health_check(&mut self) -> AppResult<Health> {
        Ok(self.health.clone())
    }

    fn next_event(&mut self, timeout: Duration) -> Option<StreamEvent> {
        self.rx.recv_timeout(timeout).ok()
    }

    fn shutdown(&mut self) {
        self.watched = None;
    }
}
