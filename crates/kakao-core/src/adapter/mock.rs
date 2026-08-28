//! In-process mock adapter driven by a fixtures JSON file. Used by unit tests
//! and by `KAKAO_CLI_MOCK=<file>` for local end-to-end runs without KakaoTalk.
//!
//! It is NOT a substitute for the real adapter in QA parity checks — it just
//! lets the core's own logic (resolution, state machine, rendering, DB) be
//! exercised deterministically.

use std::collections::HashMap;
use std::path::Path;

use kakao_contract::{
    ErrorCode, Health, ListRoomsData, ReadRecentData, SendResult, SendStatus,
};
use serde::Deserialize;

use crate::adapter::Adapter;
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    #[serde(default)]
    list_rooms: Option<ListRoomsData>,
    /// keyed by roomId, plus optional `"*"` fallback
    #[serde(default)]
    read_recent: HashMap<String, ReadRecentData>,
    #[serde(default)]
    open_room: HashMap<String, OpenRoomOutcome>,
    #[serde(default)]
    send_text: Option<SendResult>,
    #[serde(default)]
    health_check: Option<Health>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenRoomOutcome {
    #[serde(default)]
    error: Option<ErrorCode>,
}

pub struct MockAdapter {
    fixture: Fixture,
}

impl MockAdapter {
    pub fn from_fixture_file(path: &Path) -> AppResult<Self> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| AppError::internal(format!("목 fixture 읽기 실패 ({}): {e}", path.display())))?;
        Self::from_fixture_str(&raw)
    }

    pub fn from_fixture_str(raw: &str) -> AppResult<Self> {
        let fixture: Fixture = serde_json::from_str(raw)
            .map_err(|e| AppError::internal(format!("목 fixture 파싱 실패: {e}")))?;
        Ok(Self { fixture })
    }
}

impl Adapter for MockAdapter {
    fn list_rooms(&self) -> AppResult<ListRoomsData> {
        self.fixture
            .list_rooms
            .clone()
            .ok_or_else(|| AppError::internal("목 fixture에 listRooms가 없습니다"))
    }

    fn open_room(&self, room_id: &str) -> AppResult<()> {
        if let Some(outcome) = self
            .fixture
            .open_room
            .get(room_id)
            .or_else(|| self.fixture.open_room.get("*"))
        {
            if let Some(code) = outcome.error {
                return Err(AppError::adapter(code));
            }
        }
        Ok(())
    }

    fn read_recent(&self, room_id: &str, limit: u32) -> AppResult<ReadRecentData> {
        let data = self
            .fixture
            .read_recent
            .get(room_id)
            .or_else(|| self.fixture.read_recent.get("*"))
            .cloned()
            .ok_or_else(|| AppError::adapter(ErrorCode::RoomNotFound))?;
        let messages = data
            .messages
            .into_iter()
            .rev()
            .take(limit as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Ok(ReadRecentData { messages })
    }

    fn send_text(&self, _room_id: &str, _text: &str) -> AppResult<SendResult> {
        Ok(self.fixture.send_text.clone().unwrap_or(SendResult {
            status: SendStatus::Sent,
            at: Some(crate::time_util::now_utc_iso()),
            error: None,
        }))
    }

    fn health_check(&self) -> AppResult<Health> {
        self.fixture.health_check.clone().ok_or_else(|| {
            AppError::internal("목 fixture에 healthCheck가 없습니다")
        })
    }
}
