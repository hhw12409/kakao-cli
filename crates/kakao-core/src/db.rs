//! SQLite cache, aliases, send log.
//!
//! Column names are snake_case; the adapter's camelCase JSON is converted here
//! on the way in. The DDL is kept in sync with `docs/db-schema.sql`.
//!
//! The TUI writes each message it sees here (idempotent via the UNIQUE
//! constraint) for scrollback across sessions and a send audit trail.

use kakao_contract::{ErrorCode, Message, MessageKind, Room, SendStatus};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::time_util::now_utc_iso;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS rooms (
    room_id             TEXT PRIMARY KEY,
    title               TEXT NOT NULL,
    member_count        INTEGER,
    unread_count        INTEGER NOT NULL DEFAULT 0,
    last_message_text   TEXT,
    last_message_at     TEXT,
    last_message_sender TEXT,
    list_order          INTEGER NOT NULL DEFAULT 0,
    synced_at           TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rooms_title ON rooms(title);

CREATE TABLE IF NOT EXISTS messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    room_id    TEXT NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    sender     TEXT NOT NULL,
    text       TEXT NOT NULL,
    at         TEXT NOT NULL,
    outgoing   INTEGER NOT NULL DEFAULT 0,
    kind       TEXT NOT NULL DEFAULT 'text',
    synced_at  TEXT NOT NULL,
    UNIQUE(room_id, at, sender, text)
);
CREATE INDEX IF NOT EXISTS idx_messages_room_at ON messages(room_id, at);

CREATE TABLE IF NOT EXISTS aliases (
    name       TEXT PRIMARY KEY,
    room_query TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS send_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    room_id       TEXT NOT NULL,
    title_at_send TEXT NOT NULL,
    text          TEXT NOT NULL,
    status        TEXT NOT NULL,
    error_code    TEXT,
    created_at    TEXT NOT NULL,
    resolved_at   TEXT,
    CHECK (status IN ('pending','sent','failed','unknown'))
);
CREATE INDEX IF NOT EXISTS idx_send_log_created ON send_log(created_at);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const SCHEMA_VERSION: i64 = 2;

/// v1 -> v2: drop the FTS5 search objects (the `search` command is gone).
const MIGRATE_1_TO_2: &str = r#"
DROP TRIGGER IF EXISTS messages_ai;
DROP TRIGGER IF EXISTS messages_ad;
DROP TRIGGER IF EXISTS messages_au;
DROP TABLE IF EXISTS messages_fts;
"#;

/// Open the cache DB and ensure the schema exists.
pub fn open() -> AppResult<Connection> {
    let path = crate::config::db_path()?;
    let conn = Connection::open(&path)?;
    // `doctor` (or a second session) may hold the write lock briefly; wait
    // rather than failing the write outright.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&conn)?;
    Ok(conn)
}

/// In-memory DB for tests.
pub fn open_in_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current == 0 {
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        return Ok(());
    }
    if current < 2 {
        conn.execute_batch(MIGRATE_1_TO_2)?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    Ok(())
}

// --------------------------------------------------------------------------
// rooms
// --------------------------------------------------------------------------

pub fn upsert_rooms(conn: &Connection, rooms: &[Room]) -> AppResult<()> {
    let now = now_utc_iso();
    let tx = conn.unchecked_transaction()?;
    for (idx, r) in rooms.iter().enumerate() {
        let (text, at, sender) = match &r.last_message {
            Some(m) => (Some(m.text.as_str()), Some(m.at.as_str()), Some(m.sender.as_str())),
            None => (None, None, None),
        };
        tx.execute(
            "INSERT INTO rooms
                (room_id, title, member_count, unread_count,
                 last_message_text, last_message_at, last_message_sender,
                 list_order, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(room_id) DO UPDATE SET
                title = excluded.title,
                member_count = excluded.member_count,
                unread_count = excluded.unread_count,
                last_message_text = excluded.last_message_text,
                last_message_at = excluded.last_message_at,
                last_message_sender = excluded.last_message_sender,
                list_order = excluded.list_order,
                synced_at = excluded.synced_at",
            params![
                r.room_id,
                r.title,
                r.member_count,
                r.unread_count,
                text,
                at,
                sender,
                idx as i64,
                now,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// --------------------------------------------------------------------------
// messages
// --------------------------------------------------------------------------

/// Insert cached messages for a room (idempotent via the UNIQUE constraint).
/// The room row must already exist (call `upsert_rooms` first).
pub fn insert_messages(conn: &Connection, room_id: &str, messages: &[Message]) -> AppResult<()> {
    let now = now_utc_iso();
    let tx = conn.unchecked_transaction()?;
    for m in messages {
        let kind = match m.kind {
            MessageKind::Text => "text",
            MessageKind::Unsupported => "unsupported",
        };
        tx.execute(
            "INSERT OR IGNORE INTO messages
                (room_id, sender, text, at, outgoing, kind, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![room_id, m.sender, m.text, m.at, m.outgoing as i64, kind, now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Ensure a bare room row exists so `insert_messages` can reference it, even if
/// we have not run `listRooms` this session.
pub fn ensure_room(conn: &Connection, room_id: &str, title: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO rooms (room_id, title, synced_at) VALUES (?1, ?2, ?3)",
        params![room_id, title, now_utc_iso()],
    )?;
    Ok(())
}

/// Most recent cached messages for a room, oldest -> newest. Used to seed the
/// TUI transcript before the first live read returns.
pub fn recent_messages(conn: &Connection, room_id: &str, limit: u32) -> AppResult<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT sender, text, at, outgoing, kind FROM messages
         WHERE room_id = ?1 ORDER BY at DESC, id DESC LIMIT ?2",
    )?;
    let mut rows: Vec<Message> = stmt
        .query_map(params![room_id, limit], |r| {
            let kind: String = r.get(4)?;
            Ok(Message {
                sender: r.get(0)?,
                text: r.get(1)?,
                at: r.get(2)?,
                outgoing: r.get::<_, i64>(3)? != 0,
                kind: if kind == "unsupported" {
                    MessageKind::Unsupported
                } else {
                    MessageKind::Text
                },
            })
        })?
        .collect::<Result<_, _>>()?;
    rows.reverse();
    Ok(rows)
}

/// The last-synced room list, in listing order. Used for the read-only view
/// when KakaoTalk can't be reached.
pub fn cached_rooms(conn: &Connection) -> AppResult<Vec<Room>> {
    let mut stmt = conn.prepare(
        "SELECT room_id, title, member_count, unread_count,
                last_message_text, last_message_at, last_message_sender
         FROM rooms ORDER BY list_order, title",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let text: Option<String> = r.get(4)?;
            let at: Option<String> = r.get(5)?;
            let sender: Option<String> = r.get(6)?;
            let last_message = match (text, at) {
                (Some(text), Some(at)) => Some(kakao_contract::LastMessage {
                    text,
                    at,
                    sender: sender.unwrap_or_default(),
                }),
                _ => None,
            };
            Ok(Room {
                room_id: r.get(0)?,
                title: r.get(1)?,
                member_count: r.get(2)?,
                unread_count: r.get::<_, Option<i64>>(3)?.unwrap_or(0) as u32,
                last_message,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

// --------------------------------------------------------------------------
// aliases
// --------------------------------------------------------------------------

pub fn alias_add(conn: &Connection, name: &str, room_query: &str) -> AppResult<()> {
    let existing: Option<String> = conn
        .query_row("SELECT room_query FROM aliases WHERE name = ?1", params![name], |r| r.get(0))
        .optional()?;
    if existing.is_some() {
        return Err(AppError::AliasConflict { name: name.to_string() });
    }
    conn.execute(
        "INSERT INTO aliases (name, room_query, created_at) VALUES (?1, ?2, ?3)",
        params![name, room_query, now_utc_iso()],
    )?;
    Ok(())
}

pub fn alias_list(conn: &Connection) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT name, room_query FROM aliases ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn alias_remove(conn: &Connection, name: &str) -> AppResult<bool> {
    let n = conn.execute("DELETE FROM aliases WHERE name = ?1", params![name])?;
    Ok(n > 0)
}

pub fn alias_get(conn: &Connection, name: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT room_query FROM aliases WHERE name = ?1", params![name], |r| r.get(0))
        .optional()?)
}

// --------------------------------------------------------------------------
// send_log — the ONLY place `send_log.status` is written (state machine §3)
// --------------------------------------------------------------------------

/// Create the `pending` row right before calling `sendText`. Returns its id.
pub fn send_log_pending(
    conn: &Connection,
    room_id: &str,
    title: &str,
    text: &str,
) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO send_log (room_id, title_at_send, text, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        params![room_id, title, text, now_utc_iso()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Resolve a `pending` row to a terminal state. Only `pending -> {sent,
/// failed, unknown}` is allowed.
pub fn send_log_resolve(
    conn: &Connection,
    id: i64,
    status: SendStatus,
    error: Option<ErrorCode>,
) -> AppResult<()> {
    let status_str = match status {
        SendStatus::Sent => "sent",
        SendStatus::Failed => "failed",
        SendStatus::Unknown => "unknown",
    };
    let n = conn.execute(
        "UPDATE send_log SET status = ?1, error_code = ?2, resolved_at = ?3
         WHERE id = ?4 AND status = 'pending'",
        params![status_str, error.map(|c| c.as_str()), now_utc_iso(), id],
    )?;
    if n != 1 {
        return Err(AppError::internal(format!(
            "send_log 전이 실패: id {id} 가 pending 상태가 아닙니다"
        )));
    }
    Ok(())
}
