//! The adapter worker thread. Owns the live [`StreamAdapter`] and the SQLite
//! connection; the UI thread never blocks on accessibility I/O.
//!
//! It receives [`Job`]s from the UI over one channel and pushes [`UiEvent`]s
//! back over another. Between jobs it polls the adapter's event stream (the
//! `watch` message feed) on a short timeout, which also paces the loop.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;

use kakao_contract::{Message, Room, SendStatus};
use rusqlite::Connection;

use crate::adapter::{StreamAdapter, StreamEvent};
use crate::db;
use crate::render;
use crate::resolve::{self, Resolution};
use crate::send;

/// A request from the UI thread.
#[derive(Debug)]
pub enum Job {
    ListRooms,
    Switch { query: String, exact: bool },
    /// A room the user picked from the disambiguation overlay.
    SwitchTo(Room),
    Send(String),
    AliasAdd { name: String, query: String },
    AliasList,
    AliasRemove(String),
    Quit,
}

/// Something for the UI thread to fold into [`App`](super::app::App).
#[derive(Debug)]
pub enum UiEvent {
    Rooms(Vec<Room>),
    Switched { room: Room, history: Vec<Message> },
    Ambiguous(Vec<Room>),
    SwitchFailed { query: String, near: Vec<String> },
    Incoming(Message),
    Sent { at: Option<String> },
    SendFailed(String),
    SendUnknown,
    Notice(String),
    Warn(String),
    Disconnected(String),
}

pub fn run(
    mut adapter: Box<dyn StreamAdapter>,
    conn: Connection,
    jobs: Receiver<Job>,
    events: Sender<UiEvent>,
) {
    let mut active: Option<Room> = None;

    loop {
        match jobs.try_recv() {
            Ok(Job::Quit) | Err(TryRecvError::Disconnected) => {
                adapter.shutdown();
                return;
            }
            Ok(job) => handle(job, adapter.as_mut(), &conn, &mut active, &events),
            Err(TryRecvError::Empty) => {}
        }

        match adapter.next_event(Duration::from_millis(150)) {
            Some(StreamEvent::Message { room_id, message }) => {
                let _ = db::insert_messages(&conn, &room_id, std::slice::from_ref(&message));
                let _ = events.send(UiEvent::Incoming(message));
            }
            Some(StreamEvent::RoomClosed { .. }) => {
                let _ = events.send(UiEvent::Warn(
                    "카카오톡에서 대화가 닫혔습니다. /switch 로 다시 여세요.".into(),
                ));
            }
            Some(StreamEvent::Warn(code)) => {
                let _ = events.send(UiEvent::Warn(render::error_message(code)));
            }
            Some(StreamEvent::Disconnected(msg)) => {
                let _ = events.send(UiEvent::Disconnected(msg));
                return;
            }
            None => {}
        }
    }
}

fn handle(
    job: Job,
    adapter: &mut dyn StreamAdapter,
    conn: &Connection,
    active: &mut Option<Room>,
    events: &Sender<UiEvent>,
) {
    let emit = |e: UiEvent| {
        let _ = events.send(e);
    };

    match job {
        Job::ListRooms => match adapter.list_rooms() {
            Ok(listing) => {
                let _ = db::upsert_rooms(conn, &listing.rooms);
                emit(UiEvent::Rooms(listing.rooms));
            }
            Err(e) => emit(UiEvent::Warn(e.user_message())),
        },

        Job::Switch { query, exact } => {
            let rooms = match adapter.list_rooms() {
                Ok(l) => l.rooms,
                Err(e) => return emit(UiEvent::Warn(e.user_message())),
            };
            let _ = db::upsert_rooms(conn, &rooms);
            match resolve::resolve_in_list(&rooms, conn, &query, exact) {
                Ok(Resolution::One(room)) => do_switch(room, adapter, conn, active, events),
                Ok(Resolution::Many(candidates)) => emit(UiEvent::Ambiguous(candidates)),
                Ok(Resolution::None { query, near }) => {
                    emit(UiEvent::SwitchFailed { query, near })
                }
                Err(e) => emit(UiEvent::Warn(e.user_message())),
            }
        }

        Job::SwitchTo(room) => do_switch(room, adapter, conn, active, events),

        Job::Send(body) => {
            let Some(room) = active.clone() else {
                return emit(UiEvent::Notice("먼저 방을 선택하세요.".into()));
            };
            match send::send_in_room(adapter, conn, &room.room_id, &room.title, &body, 2000) {
                Ok(outcome) => match outcome.status {
                    SendStatus::Sent => emit(UiEvent::Sent { at: outcome.at }),
                    SendStatus::Failed => emit(UiEvent::SendFailed(
                        outcome
                            .error
                            .map(render::error_message)
                            .unwrap_or_else(|| "전송에 실패했습니다.".into()),
                    )),
                    SendStatus::Unknown => emit(UiEvent::SendUnknown),
                },
                Err(e) => emit(UiEvent::SendFailed(e.user_message())),
            }
        }

        Job::AliasAdd { name, query } => match db::alias_add(conn, &name, &query) {
            Ok(()) => emit(UiEvent::Notice(format!("별칭 추가: @{name} → {query}"))),
            Err(e) => emit(UiEvent::Warn(e.user_message())),
        },
        Job::AliasList => match db::alias_list(conn) {
            Ok(rows) if rows.is_empty() => {
                emit(UiEvent::Notice("등록된 별칭이 없습니다.".into()))
            }
            Ok(rows) => emit(UiEvent::Notice(
                rows.into_iter()
                    .map(|(n, q)| format!("@{n} → {q}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )),
            Err(e) => emit(UiEvent::Warn(e.user_message())),
        },
        Job::AliasRemove(name) => match db::alias_remove(conn, &name) {
            Ok(true) => emit(UiEvent::Notice(format!("별칭 삭제: @{name}"))),
            Ok(false) => emit(UiEvent::Notice(format!("별칭 @{name} 은(는) 없습니다."))),
            Err(e) => emit(UiEvent::Warn(e.user_message())),
        },

        Job::Quit => {}
    }
}

fn do_switch(
    room: Room,
    adapter: &mut dyn StreamAdapter,
    conn: &Connection,
    active: &mut Option<Room>,
    events: &Sender<UiEvent>,
) {
    let _ = adapter.unwatch();

    if let Err(e) = adapter.open_room(&room.room_id) {
        let _ = events.send(UiEvent::Warn(e.user_message()));
        return;
    }

    let history = match adapter.read_recent(&room.room_id, 40) {
        Ok(r) => r.messages,
        Err(e) => {
            let _ = events.send(UiEvent::Warn(e.user_message()));
            db::recent_messages(conn, &room.room_id, 40).unwrap_or_default()
        }
    };

    let _ = db::ensure_room(conn, &room.room_id, &room.title);
    let _ = db::insert_messages(conn, &room.room_id, &history);

    if let Err(e) = adapter.watch(&room.room_id) {
        let _ = events.send(UiEvent::Warn(e.user_message()));
    }

    *active = Some(room.clone());
    let _ = events.send(UiEvent::Switched { room, history });
}
