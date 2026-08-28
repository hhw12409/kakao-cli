//! Command dispatch. One function per subcommand; the heavy logic lives in
//! `resolve`, `send`, `db`, `render`.

use kakao_contract::ErrorCode;
use rusqlite::Connection;

use crate::adapter::{self, Adapter};
use crate::cli::{
    AliasCommand, CacheCommand, Cli, Command, InboxArgs, OpenArgs, RoomsArgs, SearchArgs,
    SendArgs,
};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::render;
use crate::resolve::{self, Interactivity};

pub fn run(cli: Cli) -> AppResult<()> {
    match cli.command {
        Command::Inbox(a) => inbox(a),
        Command::Rooms(a) => rooms(a),
        Command::Open(a) => open(a),
        Command::Send(a) => send(a),
        Command::Search(a) => search(a),
        Command::Doctor => doctor(),
        Command::Alias(c) => alias(c),
        Command::Cache(c) => cache(c),
    }
}

// --------------------------------------------------------------------------
// helpers
// --------------------------------------------------------------------------

/// Build the adapter and confirm the environment is usable. A missing bridge,
/// KakaoTalk not running, or missing permission all fall back to a
/// `doctor`-level message instead of a stack trace (`docs/command-spec.md`
/// "첫 실행 온보딩").
fn ready_adapter() -> AppResult<Box<dyn Adapter>> {
    let adapter = adapter::for_current_env().map_err(onboarding_from_internal)?;
    match adapter.health_check() {
        Ok(h) if h.kakao_running && h.accessibility_granted => Ok(adapter),
        Ok(h) => {
            let code = h
                .issues
                .first()
                .map(|i| i.code)
                .unwrap_or(ErrorCode::KakaoNotRunning);
            Err(AppError::Onboarding {
                code,
                rendered: onboarding_text(&render::render_doctor(&h)),
            })
        }
        // healthCheck itself failed to run — treat as environment-not-ready.
        Err(AppError::Adapter { code, .. }) => Err(AppError::Onboarding {
            code,
            rendered: onboarding_text(&render::error_message(code)),
        }),
        Err(other) => Err(other),
    }
}

fn onboarding_from_internal(e: AppError) -> AppError {
    match e {
        AppError::Internal(msg) => AppError::Onboarding {
            code: ErrorCode::UiElementNotFound,
            rendered: onboarding_text(&msg),
        },
        other => other,
    }
}

fn onboarding_text(detail: &str) -> String {
    format!(
        "kakao-cli 를 사용하려면 먼저 설정이 필요합니다.\n\n{detail}\n\n자세히:  kakao-cli doctor"
    )
}

fn open_db() -> AppResult<Connection> {
    db::open()
}

fn resolve_interactivity(assume_yes: bool) -> Interactivity {
    if render::is_interactive() && !assume_yes {
        Interactivity::Interactive
    } else {
        Interactivity::NonInteractive
    }
}

// --------------------------------------------------------------------------
// commands
// --------------------------------------------------------------------------

fn inbox(args: InboxArgs) -> AppResult<()> {
    let adapter = ready_adapter()?;
    let conn = open_db()?;
    let mut listing = adapter.list_rooms()?;
    db::upsert_rooms(&conn, &listing.rooms)?;

    // Unread first, then original (recent-activity) order.
    listing
        .rooms
        .sort_by_key(|r| if r.unread_count > 0 { 0 } else { 1 });

    if args.json {
        println!("{}", serde_json::to_string(&listing).unwrap());
    } else {
        println!("{}", render::render_inbox(&listing.rooms));
    }
    Ok(())
}

fn rooms(args: RoomsArgs) -> AppResult<()> {
    let adapter = ready_adapter()?;
    let conn = open_db()?;
    let listing = adapter.list_rooms()?;
    db::upsert_rooms(&conn, &listing.rooms)?;

    let filtered: Vec<_> = match &args.query {
        None => listing.rooms.clone(),
        Some(q) => {
            let needle = q.to_lowercase();
            listing
                .rooms
                .iter()
                .filter(|r| {
                    if args.exact {
                        &r.title == q
                    } else {
                        r.title.to_lowercase().contains(&needle)
                    }
                })
                .cloned()
                .collect()
        }
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string(&kakao_contract::ListRoomsData { rooms: filtered }).unwrap()
        );
    } else {
        println!("{}", render::render_rooms(&filtered));
    }
    Ok(())
}

fn open(args: OpenArgs) -> AppResult<()> {
    let adapter = ready_adapter()?;
    let conn = open_db()?;
    let room = resolve::resolve_room(
        adapter.as_ref(),
        &conn,
        &args.room,
        args.exact,
        resolve_interactivity(false),
    )?;

    let recent = adapter.read_recent(&room.room_id, args.limit)?;
    db::ensure_room(&conn, &room.room_id, &room.title)?;
    db::insert_messages(&conn, &room.room_id, &recent.messages)?;

    if args.json {
        println!("{}", serde_json::to_string(&recent).unwrap());
    } else {
        println!("{}", render::render_messages(&recent.messages));
    }
    Ok(())
}

fn send(args: SendArgs) -> AppResult<()> {
    let adapter = ready_adapter()?;
    let conn = open_db()?;
    crate::send::run_send(adapter.as_ref(), &conn, args)
}

fn search(args: SearchArgs) -> AppResult<()> {
    // Local-only: no adapter, no environment requirement.
    let conn = open_db()?;

    let room_id = match &args.room {
        None => None,
        Some(q) => {
            // Resolve against the cached room list only (offline).
            let title_like = format!("%{}%", q.to_lowercase());
            let found: Option<String> = conn
                .query_row(
                    "SELECT room_id FROM rooms WHERE lower(title) LIKE ?1 ORDER BY list_order LIMIT 1",
                    rusqlite::params![title_like],
                    |r| r.get(0),
                )
                .ok();
            match found {
                Some(id) => Some(id),
                None => {
                    return Err(AppError::RoomNotFound {
                        query: q.clone(),
                        near: Vec::new(),
                    })
                }
            }
        }
    };

    if db::is_message_cache_empty(&conn)? {
        println!(
            "검색할 메시지가 없습니다. 먼저 kakao-cli open <방> 으로 대화를 불러오세요."
        );
        return Ok(());
    }

    let hits = db::search(&conn, &args.query, room_id.as_deref())?;
    if args.json {
        let arr: Vec<_> = hits
            .iter()
            .map(|h| {
                serde_json::json!({
                    "roomTitle": h.room_title,
                    "sender": h.sender,
                    "at": h.at,
                    "snippet": h.snippet,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&arr).unwrap());
    } else {
        println!("{}", render::render_search(&hits));
    }
    Ok(())
}

fn doctor() -> AppResult<()> {
    let adapter = adapter::for_current_env().map_err(onboarding_from_internal)?;
    let health = adapter.health_check()?;
    println!("{}", render::render_doctor(&health));

    if health.issues.iter().any(|i| i.code == ErrorCode::KakaoNotRunning) {
        return Err(AppError::adapter(ErrorCode::KakaoNotRunning));
    }
    if health
        .issues
        .iter()
        .any(|i| i.code == ErrorCode::AccessibilityPermissionDenied)
    {
        return Err(AppError::adapter(ErrorCode::AccessibilityPermissionDenied));
    }
    Ok(())
}

fn alias(cmd: AliasCommand) -> AppResult<()> {
    let conn = open_db()?;
    match cmd {
        AliasCommand::Add { name, room_query } => {
            db::alias_add(&conn, &name, &room_query)?;
            println!("별칭 추가: @{name} → {room_query}");
        }
        AliasCommand::List => {
            let rows = db::alias_list(&conn)?;
            if rows.is_empty() {
                println!("등록된 별칭이 없습니다.");
            } else {
                for (name, q) in rows {
                    println!("@{name} → {q}");
                }
            }
        }
        AliasCommand::Remove { name } => {
            if db::alias_remove(&conn, &name)? {
                println!("별칭 삭제: @{name}");
            } else {
                eprintln!("별칭 @{name} 은(는) 없습니다.");
            }
        }
    }
    Ok(())
}

fn cache(cmd: CacheCommand) -> AppResult<()> {
    let conn = open_db()?;
    match cmd {
        CacheCommand::Clear { yes } => {
            if !yes && render::is_interactive() {
                let ok = render::confirm(
                    "캐시(방 목록·메시지)를 삭제합니다. 별칭과 전송 기록은 유지됩니다.",
                )?;
                if !ok {
                    return Err(AppError::Aborted);
                }
            }
            db::clear_cache(&conn)?;
            println!("캐시를 삭제했습니다.");
        }
    }
    Ok(())
}
