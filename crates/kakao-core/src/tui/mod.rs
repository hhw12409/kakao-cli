//! The interactive chat TUI — what `kakao-cli` does with no subcommand.
//!
//! Threading: this (the UI thread) owns the terminal and never blocks on
//! accessibility I/O. A [`worker`] thread owns the [`StreamAdapter`] and the
//! SQLite connection, taking [`Job`]s and returning [`UiEvent`]s over channels.

pub mod app;
mod input;
mod ui;
pub mod worker;

pub use app::{App, Screen};
pub use worker::{Job, UiEvent};

use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use kakao_contract::{ErrorCode, Health};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::adapter;
use crate::db;
use crate::error::{AppError, AppResult};
use input::Action;

pub fn run() -> AppResult<()> {
    if !crate::render::is_interactive() {
        return Err(AppError::internal(
            "대화형 터미널이 아닙니다. kakao-cli 는 TTY 에서 실행하세요.",
        ));
    }

    let mut adapter = adapter::stream_for_current_env()?;

    // First-run gate: a stack trace helps nobody. Surface a doctor-level note.
    if let Ok(h) = adapter.health_check() {
        if let Some(err) = onboarding_gate(&h) {
            return Err(err);
        }
    }

    let conn = db::open()?;
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (evt_tx, evt_rx) = mpsc::channel::<UiEvent>();
    let worker = thread::spawn(move || worker::run(adapter, conn, job_rx, evt_tx));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = event_loop(&mut terminal, &job_tx, &evt_rx);

    disable_raw_mode().ok();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    let _ = job_tx.send(Job::Quit);
    let _ = worker.join();
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    job_tx: &mpsc::Sender<Job>,
    evt_rx: &mpsc::Receiver<UiEvent>,
) -> AppResult<()> {
    let mut app = App::new();
    // Open on the room list — load it right away.
    let _ = job_tx.send(Job::ListRooms);
    // Redraw only when something actually changed, so an idle chat doesn't
    // stream frames at the terminal (matters over SSH and fills pty buffers).
    let mut dirty = true;
    loop {
        if dirty {
            terminal.draw(|f| ui::render(f, &app))?;
            dirty = false;
        }

        while let Ok(ev) = evt_rx.try_recv() {
            app.apply(ev);
            dirty = true;
        }

        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    dirty = true;
                    match input::handle(&mut app, key) {
                        Action::None => {}
                        Action::Quit => {
                            let _ = job_tx.send(Job::Quit);
                            return Ok(());
                        }
                        Action::Job(job) => {
                            if job_tx.send(job).is_err() {
                                return Ok(());
                            }
                        }
                    }
                }
                Event::Resize(_, _) => dirty = true,
                _ => {}
            }
        }
    }
}

/// Mirrors `commands::ready_adapter`'s first-run handling for the TUI entry.
fn onboarding_gate(h: &Health) -> Option<AppError> {
    if h.kakao_running && h.accessibility_granted {
        return None;
    }
    let code = h
        .issues
        .first()
        .map(|i| i.code)
        .unwrap_or(ErrorCode::KakaoNotRunning);
    let detail = crate::render::render_doctor(h);
    Some(AppError::Onboarding {
        code,
        rendered: format!(
            "kakao-cli 채팅을 시작하려면 먼저 설정이 필요합니다.\n\n{detail}\n\n자세히:  kakao-cli doctor"
        ),
    })
}
