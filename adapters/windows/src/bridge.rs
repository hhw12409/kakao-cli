//! The five contract functions, live against KakaoTalk Windows via UIA.
//!
//! Same shape as the macOS bridge: preflight (process + UIA reachable + version
//! -> selector map), navigate to the room list / message list, snapshot the
//! relevant subtree, run the shared `parsers`. Writes are best-effort with
//! Invoke/Value patterns; send is verified by polling for our own message.

#![cfg(windows)]

use std::time::{Duration, Instant};

use kakao_contract::{
    ErrorCode, Health, Issue, ListRoomsData, Method, ReadRecentData, SendResult, SendStatus,
};
use serde::Deserialize;

use crate::envelope::{self, BridgeError, BridgeResult};
use crate::kakao_app;
use crate::korean_time;
use crate::node::{first_descendant, FixtureNode};
use crate::parsers;
use crate::selectors::{self, SelectorMap};
use crate::uia::{self, Uia, UiaElement};

const ROW_SCAN_LIMIT: usize = 40;

struct Ctx {
    uia: Uia,
    pid: u32,
    sel: &'static SelectorMap,
}

fn context() -> BridgeResult<Ctx> {
    let running =
        kakao_app::running().ok_or_else(|| BridgeError::new(ErrorCode::KakaoNotRunning))?;
    let uia = Uia::new()?;
    // If we can't read the root at all, treat as a permission / integrity issue.
    let _ = uia.root()?;
    let version = kakao_app::version(&running.exe_path);
    let sel = selectors::for_version(version.as_deref())
        .ok_or_else(|| BridgeError::new(ErrorCode::AppVersionUnsupported))?;
    Ok(Ctx {
        uia,
        pid: running.pid,
        sel,
    })
}

fn windows(ctx: &Ctx) -> BridgeResult<Vec<UiaElement>> {
    let ws = uia::windows_of(&ctx.uia, ctx.pid)?;
    if ws.is_empty() {
        return Err(BridgeError::with(
            ErrorCode::KakaoWindowNotVisible,
            "no KakaoTalk windows",
        ));
    }
    Ok(ws)
}

fn main_window(ctx: &Ctx) -> BridgeResult<UiaElement> {
    let ws = windows(ctx)?;
    ws.iter()
        .find(|w| {
            w.class_name().as_deref() == Some(ctx.sel.main_window_class)
                || w.name().as_deref() == Some(ctx.sel.main_window_name)
        })
        .cloned()
        .or_else(|| ws.into_iter().next())
        .ok_or_else(|| BridgeError::with(ErrorCode::UiElementNotFound, "main window"))
}

fn conversation_window(ctx: &Ctx, title: &str) -> Option<UiaElement> {
    windows(ctx).ok()?.into_iter().find(|w| {
        w.name().as_deref() == Some(title)
            && w.class_name().as_deref() != Some(ctx.sel.main_window_class)
    })
}

// --- listRooms -----------------------------------------------------------

fn list_rooms(ctx: &Ctx) -> BridgeResult<ListRoomsData> {
    let main = main_window(ctx)?;
    let list = main
        .find_first(&ctx.uia, 400, &|e| {
            e.control_type() == ctx.sel.room_list_control_type
        })
        .ok_or_else(|| BridgeError::with(ErrorCode::UiElementNotFound, "room list"))?;

    let items: Vec<UiaElement> = list
        .children(&ctx.uia)
        .into_iter()
        .filter(|e| e.control_type() == ctx.sel.room_item_control_type)
        .take(ROW_SCAN_LIMIT)
        .collect();

    let snap_items: Vec<FixtureNode> = items.iter().map(|it| it.snapshot(&ctx.uia, 4)).collect();
    let synthetic = FixtureNode {
        control_type: "Window".into(),
        children: vec![FixtureNode {
            control_type: ctx.sel.room_list_control_type.into(),
            children: snap_items,
            ..blank()
        }],
        ..blank()
    };

    Ok(ListRoomsData {
        rooms: parsers::rooms(&synthetic, ctx.sel, korean_time::now()),
    })
}

// --- resolveRoom / openRoom --------------------------------------------

fn resolve_room(ctx: &Ctx, room_id: &str) -> BridgeResult<(UiaElement, String)> {
    let idx: usize = room_id
        .strip_prefix("row:")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| BridgeError::with(ErrorCode::RoomNotFound, "bad roomId"))?;

    let main = main_window(ctx)?;
    let list = main
        .find_first(&ctx.uia, 400, &|e| {
            e.control_type() == ctx.sel.room_list_control_type
        })
        .ok_or_else(|| BridgeError::with(ErrorCode::UiElementNotFound, "room list"))?;
    let items: Vec<UiaElement> = list
        .children(&ctx.uia)
        .into_iter()
        .filter(|e| e.control_type() == ctx.sel.room_item_control_type)
        .collect();
    let item = items
        .get(idx)
        .cloned()
        .ok_or_else(|| BridgeError::with(ErrorCode::RoomNotFound, "row index"))?;

    let snap = item.snapshot(&ctx.uia, 3);
    let title = first_descendant(&snap, &|n| {
        n.automation_id() == Some(ctx.sel.room_title_automation_id)
    })
    .and_then(crate::node::any_text)
    .unwrap_or_default();
    Ok((item, title))
}

fn open_room(ctx: &Ctx, room_id: &str) -> BridgeResult<()> {
    let (item, _title) = resolve_room(ctx, room_id)?;
    if !activate_item(ctx, &item) {
        return Err(BridgeError::with(
            ErrorCode::UiElementNotFound,
            "activate room row",
        ));
    }
    Ok(())
}

/// Open a chat row. KakaoTalk list items are usually invokable; fall back to a
/// double Invoke (some builds only open on double activation).
fn activate_item(ctx: &Ctx, item: &UiaElement) -> bool {
    if item.invoke() {
        return true;
    }
    // Some rows expose Invoke on a child button.
    if let Some(btn) = item.find_first(&ctx.uia, 30, &|e| e.control_type() == "Button") {
        return btn.invoke();
    }
    false
}

// --- readRecent --------------------------------------------------------

fn read_recent(ctx: &Ctx, room_id: &str, limit: u32) -> BridgeResult<ReadRecentData> {
    let (item, title) = resolve_room(ctx, room_id)?;
    let conv = open_conversation(ctx, &item, &title)?;

    let list = conv
        .find_first(&ctx.uia, 200, &|e| {
            e.control_type() == ctx.sel.message_list_control_type
        })
        .ok_or_else(|| BridgeError::with(ErrorCode::UiElementNotFound, "message list"))?;

    let want = limit.max(1) as usize;
    let rows: Vec<UiaElement> = list.children(&ctx.uia);
    let tail = rows.iter().skip(rows.len().saturating_sub(want + 8));
    let snap_items: Vec<FixtureNode> = tail.map(|r| r.snapshot(&ctx.uia, 4)).collect();
    let synthetic = FixtureNode {
        control_type: "Window".into(),
        children: vec![FixtureNode {
            control_type: ctx.sel.message_list_control_type.into(),
            children: snap_items,
            ..blank()
        }],
        ..blank()
    };

    let window_left = conv.bounding_left();
    let mut all = parsers::messages(&synthetic, ctx.sel, window_left, korean_time::now());
    if all.len() > want {
        all = all.split_off(all.len() - want);
    }
    Ok(ReadRecentData { messages: all })
}

fn open_conversation(ctx: &Ctx, item: &UiaElement, title: &str) -> BridgeResult<UiaElement> {
    if let Some(w) = conversation_window(ctx, title) {
        return Ok(w);
    }
    activate_item(ctx, item);
    let deadline = Instant::now() + Duration::from_millis(2500);
    while Instant::now() < deadline {
        if let Some(w) = conversation_window(ctx, title) {
            return Ok(w);
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    // Some builds render the conversation inside the main window.
    main_window(ctx)
}

// --- sendText ---------------------------------------------------------

fn send_text(ctx: &Ctx, room_id: &str, text: &str) -> SendResult {
    if text.is_empty() {
        return failed(ErrorCode::EmptyMessage);
    }
    let (item, title) = match resolve_room(ctx, room_id) {
        Ok(v) => v,
        Err(e) => return failed(e.code),
    };
    let conv = match open_conversation(ctx, &item, &title) {
        Ok(c) => c,
        Err(e) => return failed(e.code),
    };

    let Some(field) = conv.find_first(&ctx.uia, 300, &|e| {
        e.control_type() == ctx.sel.compose_control_type
            && (e.automation_id().as_deref() == Some(ctx.sel.compose_automation_id)
                || e.control_type() == "Edit")
    }) else {
        return failed(ErrorCode::SendInputFailed);
    };

    if !field.set_value(text) {
        // TODO(PoC): clipboard paste fallback (OpenClipboard/SetClipboardData + Ctrl-V).
        return failed(ErrorCode::SendInputFailed);
    }

    let sent_click = conv
        .find_first(&ctx.uia, 300, &|e| {
            e.control_type() == "Button" && e.name().as_deref() == Some(ctx.sel.send_button_name)
        })
        .map(|b| b.invoke())
        .unwrap_or(false);
    if !sent_click {
        // Enter-to-send fallback would go here (SendInput VK_RETURN on the field).
        return SendResult {
            status: SendStatus::Unknown,
            at: None,
            error: Some(ErrorCode::SendVerifyTimeout),
        };
    }

    verify_send(ctx, &conv, text)
}

fn verify_send(ctx: &Ctx, conv: &UiaElement, text: &str) -> SendResult {
    let needle = text.trim();
    let deadline = Instant::now() + Duration::from_millis(3000);
    while Instant::now() < deadline {
        if let Some(list) = conv.find_first(&ctx.uia, 200, &|e| {
            e.control_type() == ctx.sel.message_list_control_type
        }) {
            let rows = list.children(&ctx.uia);
            let tail: Vec<FixtureNode> = rows
                .iter()
                .skip(rows.len().saturating_sub(8))
                .map(|r| r.snapshot(&ctx.uia, 4))
                .collect();
            let synthetic = FixtureNode {
                control_type: "Window".into(),
                children: vec![FixtureNode {
                    control_type: ctx.sel.message_list_control_type.into(),
                    children: tail,
                    ..blank()
                }],
                ..blank()
            };
            let msgs = parsers::messages(&synthetic, ctx.sel, None, korean_time::now());
            if msgs.iter().any(|m| m.text.contains(needle)) {
                return SendResult {
                    status: SendStatus::Sent,
                    at: Some(iso_now()),
                    error: None,
                };
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    SendResult {
        status: SendStatus::Unknown,
        at: None,
        error: Some(ErrorCode::SendVerifyTimeout),
    }
}

// --- healthCheck -----------------------------------------------------

fn health_check() -> Health {
    let Some(running) = kakao_app::running() else {
        return Health {
            kakao_running: false,
            accessibility_granted: false,
            app_version: None,
            issues: vec![Issue {
                code: ErrorCode::KakaoNotRunning,
                recovery: "카카오톡 데스크톱 앱을 실행하세요.".into(),
            }],
        };
    };
    let uia_ok = Uia::new().and_then(|u| u.root().map(|_| ())).is_ok();
    let version = kakao_app::version(&running.exe_path);
    let mut issues = Vec::new();
    if !uia_ok {
        issues.push(Issue {
            code: ErrorCode::AccessibilityPermissionDenied,
            recovery: "카카오톡과 kakao-cli 의 권한 수준(관리자 여부)을 맞추세요.".into(),
        });
    }
    if uia_ok && selectors::for_version(version.as_deref()).is_none() {
        issues.push(Issue {
            code: ErrorCode::AppVersionUnsupported,
            recovery: "지원 버전이 아닙니다. 버전을 이슈로 알려주세요.".into(),
        });
    }
    Health {
        kakao_running: true,
        accessibility_granted: uia_ok,
        app_version: version,
        issues,
    }
}

// --- dispatch -------------------------------------------------------

#[derive(Deserialize)]
struct RoomIdArg {
    #[serde(rename = "roomId")]
    room_id: String,
}
#[derive(Deserialize)]
struct ReadRecentArg {
    #[serde(rename = "roomId")]
    room_id: String,
    limit: u32,
}
#[derive(Deserialize)]
struct SendTextArg {
    #[serde(rename = "roomId")]
    room_id: String,
    text: String,
}

pub fn dispatch(method_name: &str, args_json: &str) -> ! {
    let method = match method_name {
        "listRooms" => Method::ListRooms,
        "openRoom" => Method::OpenRoom,
        "readRecent" => Method::ReadRecent,
        "sendText" => Method::SendText,
        "healthCheck" => Method::HealthCheck,
        other => envelope::crash(&format!("unknown method: {other}")),
    };

    // healthCheck runs even without a full context.
    if method == Method::HealthCheck {
        envelope::ok(health_check());
    }

    let ctx = match context() {
        Ok(c) => c,
        Err(e) => envelope::finish(Err(e), method_name),
    };

    let result: BridgeResult<serde_json::Value> = match method {
        Method::ListRooms => list_rooms(&ctx).and_then(to_value),
        Method::OpenRoom => {
            let a: RoomIdArg = parse_args(args_json);
            open_room(&ctx, &a.room_id).map(|_| serde_json::json!({}))
        }
        Method::ReadRecent => {
            let a: ReadRecentArg = parse_args(args_json);
            read_recent(&ctx, &a.room_id, a.limit).and_then(to_value)
        }
        Method::SendText => {
            let a: SendTextArg = parse_args(args_json);
            to_value(send_text(&ctx, &a.room_id, &a.text))
        }
        Method::HealthCheck => unreachable!(),
        // serve-only methods never reach the one-shot name match above.
        Method::Watch | Method::Unwatch | Method::Shutdown => {
            envelope::crash("serve-only method in one-shot dispatch")
        }
    };
    envelope::finish(result, method_name);
}

// --- serve-mode entry points (contract §5) --------------------------------

/// Run one serve-mode request that is not `watch`/`unwatch`/`shutdown`.
/// Returns the `data` value on success or a contract error code.
pub fn serve_call(
    method: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, ErrorCode> {
    if method == "healthCheck" {
        return to_value(health_check()).map_err(|e| e.code);
    }

    let ctx = context().map_err(|e| e.code)?;
    let str_param = |k: &str| params.get(k).and_then(|v| v.as_str()).map(str::to_string);

    match method {
        "listRooms" => list_rooms(&ctx).and_then(to_value).map_err(|e| e.code),
        "openRoom" => {
            let room_id = str_param("roomId").ok_or(ErrorCode::RoomNotFound)?;
            open_room(&ctx, &room_id)
                .map(|_| serde_json::json!({}))
                .map_err(|e| e.code)
        }
        "readRecent" => {
            let room_id = str_param("roomId").ok_or(ErrorCode::RoomNotFound)?;
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(40) as u32;
            read_recent(&ctx, &room_id, limit)
                .and_then(to_value)
                .map_err(|e| e.code)
        }
        "sendText" => {
            let room_id = str_param("roomId").ok_or(ErrorCode::SendInputFailed)?;
            let text = str_param("text").ok_or(ErrorCode::SendInputFailed)?;
            to_value(send_text(&ctx, &room_id, &text)).map_err(|e| e.code)
        }
        _ => Err(ErrorCode::UiElementNotFound),
    }
}

/// A watch-poll read of the message tail. Never synthesises a click: throws
/// `UI_ELEMENT_NOT_FOUND` if the conversation is not already open, which the
/// poller turns into a `roomClosed` event.
pub fn watch_read(room_id: &str) -> Result<Vec<kakao_contract::Message>, ErrorCode> {
    let ctx = context().map_err(|e| e.code)?;
    let (_, title) = resolve_room(&ctx, room_id).map_err(|e| e.code)?;
    let conv = conversation_window(&ctx, &title).ok_or(ErrorCode::UiElementNotFound)?;
    let list = conv
        .find_first(&ctx.uia, 200, &|e| {
            e.control_type() == ctx.sel.message_list_control_type
        })
        .ok_or(ErrorCode::UiElementNotFound)?;
    let rows: Vec<UiaElement> = list.children(&ctx.uia);
    let tail: Vec<FixtureNode> = rows
        .iter()
        .skip(rows.len().saturating_sub(20))
        .map(|r| r.snapshot(&ctx.uia, 4))
        .collect();
    let synthetic = FixtureNode {
        control_type: "Window".into(),
        children: vec![FixtureNode {
            control_type: ctx.sel.message_list_control_type.into(),
            children: tail,
            ..blank()
        }],
        ..blank()
    };
    let window_left = conv.bounding_left();
    let mut all = parsers::messages(&synthetic, ctx.sel, window_left, korean_time::now());
    if all.len() > 12 {
        all = all.split_off(all.len() - 12);
    }
    Ok(all)
}

fn parse_args<T: for<'de> Deserialize<'de>>(json: &str) -> T {
    match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => envelope::crash(&format!("bad args: {e}")),
    }
}

fn to_value<T: serde::Serialize>(v: T) -> BridgeResult<serde_json::Value> {
    serde_json::to_value(v)
        .map_err(|e| BridgeError::with(ErrorCode::UiElementNotFound, format!("serialize: {e}")))
}

fn failed(code: ErrorCode) -> SendResult {
    SendResult {
        status: SendStatus::Failed,
        at: None,
        error: Some(code),
    }
}

fn iso_now() -> String {
    use time::format_description::well_known::Rfc3339;
    korean_time::now().format(&Rfc3339).unwrap_or_default()
}

fn blank() -> FixtureNode {
    FixtureNode {
        control_type: String::new(),
        name: None,
        automation_id: None,
        value: None,
        class_name: None,
        bounding_left: None,
        children: Vec::new(),
    }
}
