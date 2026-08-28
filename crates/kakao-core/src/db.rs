//! SQLite cache, aliases, send log, FTS5 search.
//!
//! Column names are snake_case; the adapter's camelCase JSON is converted here
//! on the way in. The DDL is kept in sync with `docs/db-schema.sql`.

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

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    text, sender UNINDEXED, room_id UNINDEXED,
    content = 'messages', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, text, sender, room_id)
    VALUES (new.id, new.text, new.sender, new.room_id);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text, sender, room_id)
    VALUES ('delete', old.id, old.text, old.sender, old.room_id);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text, sender, room_id)
    VALUES ('delete', old.id, old.text, old.sender, old.room_id);
    INSERT INTO messages_fts(rowid, text, sender, room_id)
    VALUES (new.id, new.text, new.sender, new.room_id);
END;

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

const SCHEMA_VERSION: i64 = 1;

/// Open the cache DB and ensure the schema exists.
pub fn open() -> AppResult<Connection> {
    let path = crate::config::db_path()?;
    let conn = Connection::open(&path)?;
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
    }
    // Future migrations: match on `current` and step up to SCHEMA_VERSION.
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

// --------------------------------------------------------------------------
// search (FTS5)
// --------------------------------------------------------------------------

pub struct SearchHit {
    pub room_title: String,
    pub sender: String,
    pub at: String,
    pub snippet: String,
}

pub fn search(
    conn: &Connection,
    query: &str,
    room_id: Option<&str>,
) -> AppResult<Vec<SearchHit>> {
    let fts_query = to_fts_query(query);
    let mut sql = String::from(
        "SELECT COALESCE(r.title, m.room_id), m.sender, m.at,
                snippet(messages_fts, 0, '[', ']', '…', 8)
         FROM messages_fts
         JOIN messages m ON m.id = messages_fts.rowid
         LEFT JOIN rooms r ON r.room_id = m.room_id
         WHERE messages_fts MATCH ?1",
    );
    if room_id.is_some() {
        sql.push_str(" AND m.room_id = ?2");
    }
    sql.push_str(" ORDER BY m.at DESC LIMIT 50");

    let mut stmt = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(SearchHit {
            room_title: row.get(0)?,
            sender: row.get(1)?,
            at: row.get(2)?,
            snippet: row.get(3)?,
        })
    };
    let rows = match room_id {
        Some(rid) => stmt
            .query_map(params![fts_query, rid], map_row)?
            .collect::<Result<Vec<_>, _>>()?,
        None => stmt
            .query_map(params![fts_query], map_row)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// Quote each whitespace-separated term so FTS5 treats punctuation-heavy user
/// input literally instead of as query syntax.
fn to_fts_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_message_cache_empty(conn: &Connection) -> AppResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    Ok(n == 0)
}

// --------------------------------------------------------------------------
// cache clear
// --------------------------------------------------------------------------

/// Clears `rooms` / `messages` / `messages_fts`. Leaves `aliases` and
/// `send_log` untouched (command-spec).
pub fn clear_cache(conn: &Connection) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM messages", [])?;
    tx.execute("DELETE FROM rooms", [])?;
    tx.execute("INSERT INTO messages_fts(messages_fts) VALUES ('rebuild')", [])?;
    tx.commit()?;
    Ok(())
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
