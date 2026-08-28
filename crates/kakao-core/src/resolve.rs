//! Room-name resolution — the core of misdelivery prevention
//! (`docs/command-spec.md` "이름 해석").
//!
//! Exactly one match proceeds. Several matches show a numbered list with NO
//! default selection; non-interactive callers get `RoomAmbiguous` (exit 5).
//! Zero matches is `RoomNotFound` (exit 2).

use kakao_contract::Room;
use rusqlite::Connection;

use crate::adapter::Adapter;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::render;
use crate::time_util;

/// How the caller may interact during resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interactivity {
    /// May prompt the user to pick from candidates.
    Interactive,
    /// Must not prompt (`--yes`, or stdin/stderr is not a tty). Ambiguity is an
    /// error.
    NonInteractive,
}

/// Resolve `query` (a search string or `@alias`) to exactly one room.
pub fn resolve_room(
    adapter: &dyn Adapter,
    conn: &Connection,
    query: &str,
    exact: bool,
    interactivity: Interactivity,
) -> AppResult<Room> {
    let effective = expand_alias(conn, query)?;

    let listing = adapter.list_rooms()?;
    db::upsert_rooms(conn, &listing.rooms)?;

    let needle = effective.to_lowercase();
    let mut matches: Vec<Room> = listing
        .rooms
        .iter()
        .filter(|r| {
            if exact {
                r.title == effective
            } else {
                r.title.to_lowercase().contains(&needle)
            }
        })
        .cloned()
        .collect();

    match matches.len() {
        1 => Ok(matches.pop().unwrap()),
        0 => Err(AppError::RoomNotFound {
            query: effective.clone(),
            near: near_names(&listing.rooms, &effective),
        }),
        _ => match interactivity {
            Interactivity::NonInteractive => Err(AppError::RoomAmbiguous {
                candidates: matches.iter().map(candidate_line).collect(),
            }),
            Interactivity::Interactive => {
                let lines: Vec<String> = matches.iter().map(candidate_line).collect();
                let idx = render::choose("여러 채팅방이 일치합니다.", &lines)?;
                Ok(matches.swap_remove(idx))
            }
        },
    }
}

/// `@dev` -> the alias's stored room_query. A non-alias string is returned
/// unchanged. An unknown alias is `RoomNotFound`.
fn expand_alias(conn: &Connection, query: &str) -> AppResult<String> {
    let Some(name) = query.strip_prefix('@') else {
        return Ok(query.to_string());
    };
    match db::alias_get(conn, name)? {
        Some(room_query) => Ok(room_query),
        None => Err(AppError::RoomNotFound {
            query: query.to_string(),
            near: Vec::new(),
        }),
    }
}

fn candidate_line(r: &Room) -> String {
    let members = match r.member_count {
        Some(n) => format!("{n}명"),
        None => "인원 미상".to_string(),
    };
    let last = match &r.last_message {
        Some(m) => match time_util::parse_iso(&m.at) {
            Some(dt) => format!("마지막 메시지 {}", time_util::relative_ko(dt)),
            None => "마지막 메시지 시각 미상".to_string(),
        },
        None => "메시지 없음".to_string(),
    };
    format!("{}  {members} · {last}", r.title)
}

/// Cheap "did you mean" hint: titles sharing a token or a common substring
/// with the query. Not a fuzzy matcher — just a nudge.
fn near_names(rooms: &[Room], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut out: Vec<String> = rooms
        .iter()
        .filter(|r| {
            let t = r.title.to_lowercase();
            q.chars().filter(|c| !c.is_whitespace()).any(|c| t.contains(c))
        })
        .map(|r| r.title.clone())
        .collect();
    out.truncate(5);
    out
}
