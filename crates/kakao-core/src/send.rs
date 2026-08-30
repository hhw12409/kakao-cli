//! `send` — the product's centre. Fast, but hard to misfire.
//!
//! State machine (`docs/adapter-contract.md` §3): a `pending` row is written
//! immediately before `sendText`; the adapter's result resolves it to exactly
//! one of `sent | failed | unknown`. `unknown` is never retried.
//!
//! In the TUI the active room is already chosen and pressing Enter is the
//! confirmation, so there is no room resolution, no editor, and no prompt here —
//! just validation and the state machine.

use kakao_contract::{ErrorCode, SendResult, SendStatus};
use rusqlite::Connection;

use crate::adapter::StreamAdapter;
use crate::db;
use crate::error::{AppError, AppResult};

/// What happened to one send attempt. `status` is authoritative; `at` is set on
/// `Sent`, `error` on `Failed`/`Unknown`.
#[derive(Debug, Clone)]
pub struct SendOutcome {
    pub status: SendStatus,
    pub at: Option<String>,
    pub error: Option<ErrorCode>,
}

/// Validate `body`, run the `pending -> {sent|failed|unknown}` state machine
/// against `adapter`, and record every transition in `send_log`.
///
/// Returns `Err` only for pre-send validation failures (empty / too long),
/// which never enter `pending`. A delivery that failed or could not be verified
/// comes back as `Ok(SendOutcome)` with the corresponding status.
pub fn send_in_room(
    adapter: &mut dyn StreamAdapter,
    conn: &Connection,
    room_id: &str,
    room_title: &str,
    body: &str,
    max_chars: usize,
) -> AppResult<SendOutcome> {
    validate_body(body, max_chars)?;

    let log_id = db::send_log_pending(conn, room_id, room_title, body)?;

    let result = match adapter.send_text(room_id, body) {
        Ok(r) => r,
        Err(AppError::Adapter { code, .. }) => {
            db::send_log_resolve(conn, log_id, SendStatus::Failed, Some(code))?;
            return Ok(SendOutcome { status: SendStatus::Failed, at: None, error: Some(code) });
        }
        Err(other) => {
            db::send_log_resolve(
                conn,
                log_id,
                SendStatus::Failed,
                Some(ErrorCode::SendInputFailed),
            )?;
            return Err(other);
        }
    };

    let SendResult { status, at, error } = result;
    match status {
        SendStatus::Sent => {
            db::send_log_resolve(conn, log_id, SendStatus::Sent, None)?;
        }
        SendStatus::Failed => {
            db::send_log_resolve(
                conn,
                log_id,
                SendStatus::Failed,
                Some(error.unwrap_or(ErrorCode::SendInputFailed)),
            )?;
        }
        SendStatus::Unknown => {
            db::send_log_resolve(
                conn,
                log_id,
                SendStatus::Unknown,
                Some(error.unwrap_or(ErrorCode::SendVerifyTimeout)),
            )?;
        }
    }
    Ok(SendOutcome { status, at, error })
}

fn validate_body(body: &str, max_chars: usize) -> AppResult<()> {
    if body.trim().is_empty() {
        return Err(AppError::adapter(ErrorCode::EmptyMessage));
    }
    if body.chars().count() > max_chars {
        return Err(AppError::adapter(ErrorCode::MessageTooLong));
    }
    Ok(())
}
