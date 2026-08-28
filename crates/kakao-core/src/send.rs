//! `send` — the product's centre. Fast, but hard to misfire.
//!
//! State machine (`docs/adapter-contract.md` §3): a `pending` row is written
//! immediately before `sendText`; the adapter's result resolves it to exactly
//! one of `sent | failed | unknown`. `--dry-run` never enters `pending`.
//! `unknown` is never retried.

use std::io::Read;

use kakao_contract::{ErrorCode, SendStatus};
use rusqlite::Connection;

use crate::adapter::Adapter;
use crate::cli::SendArgs;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::render;
use crate::resolve::{self, Interactivity};
use crate::time_util;

pub fn run_send(adapter: &dyn Adapter, conn: &Connection, args: SendArgs) -> AppResult<()> {
    // `--yes` or a non-tty makes the whole flow non-interactive: no editor,
    // no same-name prompt, no confirmation.
    let interactive = render::is_interactive() && !args.yes;
    let interactivity = if interactive {
        Interactivity::Interactive
    } else {
        Interactivity::NonInteractive
    };

    let (body, from_editor) = obtain_body(&args, interactive)?;
    validate_body(&body, args.max_chars)?;

    let room = resolve::resolve_room(adapter, conn, &args.room, args.exact, interactivity)?;

    // --dry-run: show target + message, call nothing, log nothing.
    if args.dry_run {
        println!("[dry-run] 받는 방: {}{}", room.title, member_suffix(&room));
        println!("[dry-run] 메시지: {}", first_line_preview(&body));
        return Ok(());
    }

    // Editor mode always previews and asks (unless --yes already made this
    // non-interactive). Arg/stdin mode with a single match sends straight away.
    if from_editor && interactive {
        eprintln!("받는 방: {}{}", room.title, member_suffix(&room));
        eprintln!("메시지: {}", first_line_preview(&body));
        eprintln!();
        if !render::confirm("전송할까요?")? {
            return Err(AppError::Aborted);
        }
    }

    // --- state machine: pending -> {sent|failed|unknown} -------------------
    let log_id = db::send_log_pending(conn, &room.room_id, &room.title, &body)?;

    let result = match adapter.send_text(&room.room_id, &body) {
        Ok(r) => r,
        Err(AppError::Adapter { code, .. }) => {
            // openRoom/input failure surfaced as a contract error -> failed.
            db::send_log_resolve(conn, log_id, SendStatus::Failed, Some(code))?;
            return Err(AppError::adapter(code));
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

    match result.status {
        SendStatus::Sent => {
            db::send_log_resolve(conn, log_id, SendStatus::Sent, None)?;
            let hm = result
                .at
                .as_deref()
                .and_then(time_util::parse_iso)
                .map(time_util::local_hm)
                .unwrap_or_else(|| time_util::local_hm(time::OffsetDateTime::now_utc()));
            println!("✓ {}에 전송됨  {hm}", room.title);
            Ok(())
        }
        SendStatus::Failed => {
            let code = result.error.unwrap_or(ErrorCode::SendInputFailed);
            db::send_log_resolve(conn, log_id, SendStatus::Failed, Some(code))?;
            Err(AppError::adapter(code))
        }
        SendStatus::Unknown => {
            db::send_log_resolve(
                conn,
                log_id,
                SendStatus::Unknown,
                Some(result.error.unwrap_or(ErrorCode::SendVerifyTimeout)),
            )?;
            Err(AppError::SendUnknown)
        }
    }
}

/// Returns `(body, came_from_editor)`.
fn obtain_body(args: &SendArgs, interactive: bool) -> AppResult<(String, bool)> {
    if let Some(msg) = &args.message {
        return Ok((msg.clone(), false));
    }
    if args.stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| AppError::internal(format!("표준 입력 읽기 실패: {e}")))?;
        // Preserve interior newlines; only trim a single trailing newline.
        let body = buf.strip_suffix('\n').unwrap_or(&buf).to_string();
        return Ok((body, false));
    }
    if !interactive {
        return Err(AppError::internal(
            "메시지가 없습니다. 인자로 전달하거나 --stdin 을 쓰세요 (비대화형에서는 편집기를 열 수 없습니다)",
        ));
    }
    Ok((open_editor()?, true))
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

fn open_editor() -> AppResult<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .map_err(|_| {
            AppError::internal("$VISUAL / $EDITOR 가 설정되어 있지 않습니다")
        })?;

    let path = std::env::temp_dir().join(format!("kakao-cli-{}.txt", std::process::id()));
    std::fs::write(&path, b"").ok();

    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = std::process::Command::new(program)
        .args(parts)
        .arg(&path)
        .status()
        .map_err(|e| AppError::internal(format!("편집기 실행 실패: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_file(&path);
        return Err(AppError::Aborted);
    }

    let body = std::fs::read_to_string(&path)
        .map_err(|e| AppError::internal(format!("편집기 파일 읽기 실패: {e}")))?;
    let _ = std::fs::remove_file(&path);
    Ok(body.strip_suffix('\n').unwrap_or(&body).to_string())
}

fn member_suffix(room: &kakao_contract::Room) -> String {
    match room.member_count {
        Some(n) => format!(" ({n}명)"),
        None => String::new(),
    }
}

fn first_line_preview(body: &str) -> String {
    let first = body.lines().next().unwrap_or("");
    let extra = body.lines().count().saturating_sub(1);
    if extra > 0 {
        format!("{first} … (+{extra}줄)")
    } else {
        first.to_string()
    }
}
