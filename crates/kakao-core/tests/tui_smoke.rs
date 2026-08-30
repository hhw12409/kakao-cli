//! Headless drive of the TUI worker + app fold against the streaming mock.
//! No terminal, no KakaoTalk. Mirrors what a real session does:
//! `/rooms` -> `/switch` -> receive a pushed message -> send one.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use kakao_core::adapter::MockStreamAdapter;
use kakao_core::db;
use kakao_core::tui::app::{App, Line};
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
        let adapter = Box::new(MockStreamAdapter::from_fixture_str(FIXTURE).unwrap());
        let conn = db::open_in_memory().unwrap();
        let (job_tx, job_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let worker = thread::spawn(move || worker::run(adapter, conn, job_rx, evt_tx));
        Harness {
            jobs: job_tx,
            events: evt_rx,
            app: App::new(),
            worker: Some(worker),
        }
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
fn rooms_switch_receive_send() {
    let mut h = Harness::start();

    // /rooms
    h.jobs.send(Job::ListRooms).unwrap();
    h.pump_until("room list", |a| {
        a.lines.iter().any(|l| matches!(l, Line::System(s) if s.contains("개발팀")))
    });

    // /switch 가족  (unique match)
    h.jobs
        .send(Job::Switch { query: "가족".into(), exact: false })
        .unwrap();
    h.pump_until("switch to 가족", |a| a.room_title.as_deref() == Some("가족"));
    assert!(h.transcript().iter().any(|l| l.contains("치킨이요")), "history loaded");

    // pushed incoming message from the fixture
    h.pump_until("incoming 아빠", |a| {
        a.lines.iter().any(|l| matches!(l, Line::Msg { who, body, .. } if who == "아빠" && body == "좋아 치킨"))
    });

    // send a line
    h.jobs.send(Job::Send("나도 치킨".into())).unwrap();
    h.pump_until("send echoed", |a| {
        a.lines.iter().any(|l| matches!(l, Line::Msg { outgoing: true, body, .. } if body == "나도 치킨"))
    });
    assert!(h.app.status.starts_with('✓'), "status shows sent: {}", h.app.status);
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

    let job = h.app.pick(1).expect("pick candidate 1");
    h.jobs.send(job).unwrap();
    h.pump_until("switched after pick", |a| a.room_title.is_some());
}
