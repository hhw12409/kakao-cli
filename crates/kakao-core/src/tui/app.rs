//! TUI state and the fold of one [`UiEvent`] into it. No rendering, no I/O —
//! kept plain so `tests/tui_smoke.rs` can drive it headless.

use kakao_contract::{Message, MessageKind, Room};

use super::worker::{Job, UiEvent};
use crate::resolve::candidate_line;

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
    pub lines: Vec<Line>,
    pub input: String,
    /// Rows scrolled up from the bottom. 0 == following the latest message.
    pub scrollback: u16,
    pub room_title: Option<String>,
    pub active_room_id: Option<String>,
    pub status: String,
    pub picker: Option<Picker>,
    pub connected: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            connected: true,
            status: "방을 선택하세요 — /switch <방>".into(),
            ..Default::default()
        };
        app.push_system(
            "kakao-cli 채팅.  /switch <방> 으로 시작,  /help 로 명령 목록,  /quit 로 종료.",
        );
        app
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

    /// Fold one worker event into the state.
    pub fn apply(&mut self, event: UiEvent) {
        match event {
            UiEvent::Rooms(rooms) => {
                if rooms.is_empty() {
                    self.push_system("방 목록이 비어 있습니다.");
                } else {
                    self.push_system("채팅방:");
                    for r in &rooms {
                        let unread = if r.unread_count > 0 {
                            format!("  ({}건 안 읽음)", r.unread_count)
                        } else {
                            String::new()
                        };
                        self.push_system(format!("  • {}{unread}", r.title));
                    }
                    self.push_system("/switch <이름> 으로 이동");
                }
            }
            UiEvent::Switched { room, history } => {
                self.lines.clear();
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
                self.status = format!("연결됨 · {}", room.title);
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
         \x20 /rooms                 방 목록\n\
         \x20 /switch <이름|@별칭>    방 이동 (동명이면 번호 선택)\n\
         \x20 /alias add <이름> <검색어>   별칭 추가\n\
         \x20 /alias list            별칭 목록\n\
         \x20 /alias rm <이름>        별칭 삭제\n\
         \x20 /help                  이 도움말\n\
         \x20 /quit                  종료\n\
         그 밖의 입력 + Enter = 현재 방으로 전송.  PgUp/PgDn 스크롤,  Ctrl-C 종료."
    }
}
