//! Headless drive of the TUI worker + app fold against the streaming mock.
//! No terminal, no KakaoTalk. Mirrors what a real session does:
//! open on the room list -> highlight a room -> `Enter` -> receive -> send.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use kakao_core::adapter::{MockAvailability, MockStreamAdapter};
use kakao_core::db;
use kakao_core::tui::app::{App, Line, Screen};
use kakao_core::tui::worker::{self, Job, UiEvent};

const FIXTURE: &str = include_str!("fixtures/chat.json");

struct Harness {
    jobs: mpsc::Sender<Job>,
    events: mpsc::Receiver<UiEvent>,
    app: App,
    worker: Option<thread::JoinHandle<()>>,
}

impl Harness {
    fn start() -> Self {
        Self::start_returning_availability().0
    }

    fn start_returning_availability() -> (Self, MockAvailability) {
        let adapter = MockStreamAdapter::from_fixture_str(FIXTURE).unwrap();
        let availability = adapter.availability();
        let conn = db::open_in_memory().unwrap();
        let (job_tx, job_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let worker = thread::spawn(move || worker::run(Box::new(adapter), conn, job_rx, evt_tx));
        (
            Harness {
                jobs: job_tx,
                events: evt_rx,
                app: App::new(),
                worker: Some(worker),
            },
            availability,
        )
    }

    /// Pump events into the app until `pred` holds or we time out.
    fn pump_until(&mut self, what: &str, pred: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if pred(&self.app) {
                return;
            }
            match self.events.recv_timeout(Duration::from_millis(100)) {
                Ok(ev) => self.app.apply(ev),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        panic!("timed out waiting for: {what}");
    }

    fn transcript(&self) -> Vec<String> {
        self.app
            .lines
            .iter()
            .map(|l| match l {
                Line::System(s) => s.clone(),
                Line::Msg { who, body, .. } => format!("{who}: {body}"),
            })
            .collect()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.jobs.send(Job::Quit);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

#[test]
fn room_list_navigate_open_receive_send() {
    let mut h = Harness::start();
    assert_eq!(h.app.screen, Screen::Rooms, "opens on the room list");

    // The event loop fires this on startup.
    h.jobs.send(Job::ListRooms).unwrap();
    h.pump_until("room list loaded", |a| {
        !a.rooms_loading && a.rooms.iter().any(|r| r.title == "개발팀")
    });

    // Filter down to the unique "가족" room and open the highlighted row.
    h.app.edit_filter(|f| f.push_str("가족"));
    let job = h.app.enter_selected().expect("a room is highlighted");
    h.jobs.send(job).unwrap();

    h.pump_until("switched to 가족", |a| {
        a.screen == Screen::Chat && a.room_title.as_deref() == Some("가족")
    });
    assert!(
        h.transcript().iter().any(|l| l.contains("치킨이요")),
        "history loaded"
    );

    // Pushed incoming message from the fixture.
    h.pump_until("incoming 아빠", |a| {
        a.lines.iter().any(|l| matches!(l, Line::Msg { who, body, .. } if who == "아빠" && body == "좋아 치킨"))
    });

    // Send a line.
    h.jobs.send(Job::Send("나도 치킨".into())).unwrap();
    h.pump_until("send echoed", |a| {
        a.lines.iter().any(|l| matches!(l, Line::Msg { outgoing: true, body, .. } if body == "나도 치킨"))
    });
    assert!(h.app.status.starts_with('✓'), "status shows sent: {}", h.app.status);

    // Esc-equivalent: back to the room list.
    h.app.open_room_list();
    assert_eq!(h.app.screen, Screen::Rooms);
}

#[test]
fn offline_falls_back_to_cached_rooms_then_recovers() {
    use kakao_contract::ErrorCode;
    let (mut h, availability) = Harness::start_returning_availability();

    // Online sync populates the cache.
    h.jobs.send(Job::ListRooms).unwrap();
    h.pump_until("online room list", |a| {
        !a.rooms_loading && a.rooms.iter().any(|r| r.title == "가족")
    });

    // KakaoTalk goes away; the next list drops to the cached, read-only view.
    availability.set(Some(ErrorCode::KakaoNotRunning));
    h.jobs.send(Job::ListRooms).unwrap();
    h.pump_until("offline cached view", |a| a.offline && !a.rooms.is_empty());

    // Opening a room offline still shows cached history; sending is refused.
    let job = {
        h.app.edit_filter(|f| f.push_str("가족"));
        h.app.enter_selected().expect("cached room highlighted")
    };
    h.jobs.send(job).unwrap();
    h.pump_until("cached transcript", |a| {
        a.screen == Screen::Chat && a.status.starts_with("캐시")
    });
    h.jobs.send(Job::Send("보낼 수 있나".into())).unwrap();
    h.pump_until("send refused offline", |a| {
        a.status.starts_with('✗') || a.lines.iter().any(|l| matches!(l, Line::System(s) if s.contains("오프라인")))
    });

    // KakaoTalk comes back.
    availability.set(None);
    h.jobs.send(Job::ListRooms).unwrap();
    h.pump_until("back online", |a| !a.offline);
}

#[test]
fn switch_ambiguous_offers_picker_without_auto_selecting() {
    let mut h = Harness::start();
    h.jobs
        .send(Job::Switch { query: "개발".into(), exact: false })
        .unwrap();
    h.pump_until("picker shown", |a| a.picker.is_some());

    // No room was opened — the choice must stay with the user.
    assert!(h.app.room_title.is_none(), "must not auto-open a room");
    assert_eq!(h.app.screen, Screen::Rooms, "still on the room list");

    let job = h.app.pick(1).expect("pick candidate 1");
    h.jobs.send(job).unwrap();
    h.pump_until("switched after pick", |a| {
        a.screen == Screen::Chat && a.room_title.is_some()
    });
}
