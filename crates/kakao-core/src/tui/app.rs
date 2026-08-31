//! TUI state and the fold of one [`UiEvent`] into it. No rendering, no I/O —
//! kept plain so `tests/tui_smoke.rs` can drive it headless.

use kakao_contract::{Message, MessageKind, Room};

use super::worker::{Job, UiEvent};
use crate::resolve::candidate_line;

/// Which screen the TUI is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The room list — arrow keys move the highlight, `Enter` opens a room.
    #[default]
    Rooms,
    /// An open conversation — type to send, `Esc` returns to the room list.
    Chat,
}

/// One rendered row in the transcript.
#[derive(Debug, Clone)]
pub enum Line {
    Msg {
        at: String,
        who: String,
        body: String,
        outgoing: bool,
    },
    /// System note (room switched, errors, `/help`, …).
    System(String),
}

/// The disambiguation overlay shown when `/switch` matches several rooms.
#[derive(Debug, Clone)]
pub struct Picker {
    pub query: String,
    pub rooms: Vec<Room>,
}

#[derive(Debug, Default)]
pub struct App {
    pub screen: Screen,

    // --- room-list screen ---
    /// The last room listing the worker delivered.
    pub rooms: Vec<Room>,
    /// Highlighted row, indexed into [`App::filtered_rooms`].
    pub rooms_selected: usize,
    /// Incremental filter typed on the room-list screen.
    pub rooms_filter: String,
    /// A `listRooms` job is in flight and no rooms have arrived yet.
    pub rooms_loading: bool,

    // --- chat screen ---
    pub lines: Vec<Line>,
    pub input: String,
    /// Rows scrolled up from the bottom. 0 == following the latest message.
    pub scrollback: u16,
    pub room_title: Option<String>,
    pub active_room_id: Option<String>,
    pub status: String,
    pub picker: Option<Picker>,
    pub connected: bool,
    /// KakaoTalk unreachable — rooms/transcript are the cached copy, sending off.
    pub offline: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            connected: true,
            screen: Screen::Rooms,
            rooms_loading: true,
            status: String::new(),
            ..Default::default()
        }
    }

    pub fn push_system(&mut self, text: impl Into<String>) {
        for l in text.into().split('\n') {
            self.lines.push(Line::System(l.to_string()));
        }
        self.follow();
    }

    fn push_message(&mut self, m: &Message) {
        let who = if m.outgoing {
            "나".to_string()
        } else if m.sender.is_empty() {
            "상대".to_string()
        } else {
            m.sender.clone()
        };
        let body = match m.kind {
            MessageKind::Text => m.text.clone(),
            MessageKind::Unsupported => "(사진·파일·이모티콘 등 — 터미널에서 표시 불가)".to_string(),
        };
        self.lines.push(Line::Msg {
            at: m.at.clone(),
            who,
            body,
            outgoing: m.outgoing,
        });
        self.follow();
    }

    fn follow(&mut self) {
        self.scrollback = 0;
    }

    // --- room-list helpers -------------------------------------------------

    /// Rooms visible under the current filter, in listing order.
    pub fn filtered_rooms(&self) -> Vec<&Room> {
        let needle = self.rooms_filter.trim().to_lowercase();
        self.rooms
            .iter()
            .filter(|r| needle.is_empty() || r.title.to_lowercase().contains(&needle))
            .collect()
    }

    /// Move the highlight by `delta`, wrapping at both ends.
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.filtered_rooms().len();
        if n == 0 {
            self.rooms_selected = 0;
            return;
        }
        let cur = self.rooms_selected.min(n - 1) as i32;
        self.rooms_selected = (cur + delta).rem_euclid(n as i32) as usize;
    }

    /// Edit the filter, resetting the highlight to the top of the new list.
    pub fn edit_filter(&mut self, f: impl FnOnce(&mut String)) {
        f(&mut self.rooms_filter);
        self.rooms_selected = 0;
    }

    /// The job that opens the highlighted room, if any.
    pub fn enter_selected(&mut self) -> Option<Job> {
        let room = (*self.filtered_rooms().get(self.rooms_selected)?).clone();
        Some(Job::SwitchTo(room))
    }

    /// Leave the current conversation and show the room list again.
    pub fn open_room_list(&mut self) {
        self.screen = Screen::Rooms;
        self.rooms_loading = true;
        self.picker = None;
    }

    // --- event fold ------------------------------------------------------

    /// Fold one worker event into the state.
    pub fn apply(&mut self, event: UiEvent) {
        match event {
            UiEvent::Rooms(rooms) => {
                self.rooms = rooms;
                self.rooms_loading = false;
                let n = self.filtered_rooms().len();
                if n > 0 && self.rooms_selected >= n {
                    self.rooms_selected = n - 1;
                }
            }
            UiEvent::Switched { room, history } => {
                self.lines.clear();
                self.screen = Screen::Chat;
                self.rooms_filter.clear();
                self.room_title = Some(room.title.clone());
                self.active_room_id = Some(room.room_id.clone());
                self.picker = None;
                if history.is_empty() {
                    self.push_system(format!("{} — 최근 메시지가 없습니다.", room.title));
                } else {
                    self.push_system(format!("{} — 최근 {}개", room.title, history.len()));
                    for m in &history {
                        self.push_message(m);
                    }
                }
                if self.offline {
                    self.push_system("(캐시 — 카카오톡을 실행하면 최신 메시지를 불러옵니다)");
                    self.status = format!("캐시 · {}", room.title);
                } else {
                    self.status = format!("연결됨 · {}", room.title);
                }
            }
            UiEvent::Offline(reason) => {
                self.offline = true;
                self.connected = true;
                self.push_system(format!("오프라인: {reason}"));
                self.push_system("캐시된 방 목록을 표시합니다. 카카오톡을 실행하면 자동으로 연결됩니다.");
                self.status = "오프라인 — 캐시 (읽기 전용)".into();
            }
            UiEvent::Online => {
                if self.offline {
                    self.offline = false;
                    self.push_system("카카오톡에 연결되었습니다.");
                }
            }
            UiEvent::Ambiguous(rooms) => {
                self.picker = Some(Picker {
                    query: String::new(),
                    rooms: rooms.clone(),
                });
                self.push_system("여러 방이 일치합니다. 번호로 선택하세요 (Esc 취소):");
                for (i, r) in rooms.iter().enumerate() {
                    self.push_system(format!("  {}. {}", i + 1, candidate_line(r)));
                }
            }
            UiEvent::SwitchFailed { query, near } => {
                self.push_system(format!("'{query}' 와(과) 일치하는 방이 없습니다."));
                if !near.is_empty() {
                    self.push_system(format!("가까운 이름: {}", near.join(", ")));
                }
            }
            UiEvent::Incoming(m) => self.push_message(&m),
            UiEvent::Sent { at } => {
                let hm = at
                    .as_deref()
                    .and_then(crate::time_util::parse_iso)
                    .map(crate::time_util::local_hm)
                    .unwrap_or_else(|| {
                        crate::time_util::local_hm(time::OffsetDateTime::now_utc())
                    });
                self.status = format!("✓ 전송됨 {hm}");
            }
            UiEvent::SendFailed(msg) => {
                self.status = format!("✗ {msg}");
                self.push_system(format!("전송 실패: {msg}"));
            }
            UiEvent::SendUnknown => {
                self.status = "? 전송 확인 불가 — 카카오톡에서 직접 확인하세요".into();
                self.push_system(
                    "전송 여부를 확인할 수 없습니다. 카카오톡에서 직접 확인하세요. (재전송하지 않았습니다)",
                );
            }
            UiEvent::Notice(text) => self.push_system(text),
            UiEvent::Warn(text) => {
                self.status = text.clone();
                self.push_system(text);
            }
            UiEvent::Disconnected(msg) => {
                self.connected = false;
                self.status = format!("연결 끊김 — {msg}");
                self.push_system(format!("어댑터 연결이 끊겼습니다: {msg}"));
                self.push_system("/quit 로 종료 후 다시 실행하세요.");
            }
        }
    }

    /// Pick candidate `n` (1-based) from the overlay. Returns the job to run.
    pub fn pick(&mut self, n: usize) -> Option<Job> {
        let picker = self.picker.take()?;
        let room = picker.rooms.get(n.checked_sub(1)?)?.clone();
        Some(Job::SwitchTo(room))
    }

    pub fn help_text() -> &'static str {
        "명령:\n\
         \x20 /rooms                 방 목록으로 (Esc 와 동일)\n\
         \x20 /switch <이름|@별칭>    방 이동 (동명이면 번호 선택)\n\
         \x20 /alias add <이름> <검색어>   별칭 추가\n\
         \x20 /alias list            별칭 목록\n\
         \x20 /alias rm <이름>        별칭 삭제\n\
         \x20 /help                  이 도움말\n\
         \x20 /quit                  종료\n\
         그 밖의 입력 + Enter = 현재 방으로 전송.  Esc = 방 목록,  PgUp/PgDn 스크롤,  Ctrl-C 종료."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str, title: &str) -> Room {
        Room {
            room_id: id.into(),
            title: title.into(),
            member_count: None,
            unread_count: 0,
            last_message: None,
        }
    }

    #[test]
    fn offline_then_online_toggles_state_and_narrates() {
        let mut app = App::new();
        app.apply(UiEvent::Offline("카카오톡이 실행되지 않았습니다.".into()));
        assert!(app.offline);
        assert!(app.status.contains("오프라인"));

        // A cached room list still lands.
        app.apply(UiEvent::Rooms(vec![room("row:0", "가족")]));
        assert_eq!(app.rooms.len(), 1);

        // Opening a room while offline marks the transcript as cached.
        app.apply(UiEvent::Switched {
            room: room("row:0", "가족"),
            history: vec![],
        });
        assert_eq!(app.screen, Screen::Chat);
        assert!(app.status.starts_with("캐시"));
        assert!(app
            .lines
            .iter()
            .any(|l| matches!(l, Line::System(s) if s.contains("캐시"))));

        app.apply(UiEvent::Online);
        assert!(!app.offline);
    }
}
