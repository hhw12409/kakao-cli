//! Room-name resolution — the core of misdelivery prevention
//! (`docs/command-spec.md` "이름 해석").
//!
//! Exactly one match proceeds. Several matches are returned as candidates with
//! NO default selection — the caller (the TUI `/switch` overlay) makes the
//! choice explicit. Zero matches carries a "did you mean" nudge.

use kakao_contract::Room;
use rusqlite::Connection;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::time_util;

/// Outcome of resolving a `/switch` query against the current room list.
#[derive(Debug, Clone)]
pub enum Resolution {
    /// Exactly one room matched.
    One(Room),
    /// Several rooms matched. The caller must disambiguate — never auto-pick.
    Many(Vec<Room>),
    /// Nothing matched.
    None { query: String, near: Vec<String> },
}

/// Resolve `query` (a search string or `@alias`) against an already-fetched
/// room list. `conn` is used only to expand aliases.
pub fn resolve_in_list(
    rooms: &[Room],
    conn: &Connection,
    query: &str,
    exact: bool,
) -> AppResult<Resolution> {
    let effective = expand_alias(conn, query)?;
    let needle = effective.to_lowercase();

    let matches: Vec<Room> = rooms
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

    Ok(match matches.len() {
        1 => Resolution::One(matches.into_iter().next().unwrap()),
        0 => Resolution::None {
            query: effective.clone(),
            near: near_names(rooms, &effective),
        },
        _ => Resolution::Many(matches),
    })
}

/// One-line description of a candidate room, for the disambiguation overlay.
pub fn candidate_line(r: &Room) -> String {
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

/// Cheap "did you mean" hint: titles sharing a character with the query.
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
