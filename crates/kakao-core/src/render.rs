//! User-facing output. Success is short and certain; failure carries the next
//! action. Message bodies never go to logs or telemetry (they do appear in
//! normal stdout output, which is the point of the tool).

use std::io::{IsTerminal, Write};

use kakao_contract::{ErrorCode, Health, Message, MessageKind, Room};

use crate::error::{AppError, AppResult};
use crate::time_util;

// --------------------------------------------------------------------------
// environment
// --------------------------------------------------------------------------

/// Interactive prompts are only shown when BOTH stdin and stderr are a tty.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

pub fn use_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

// --------------------------------------------------------------------------
// error copy  (final wording is docs/errors.md; kept in sync here)
// --------------------------------------------------------------------------

pub fn error_message(code: ErrorCode) -> String {
    match code {
        ErrorCode::KakaoNotRunning => "카카오톡이 실행되고 있지 않습니다.".into(),
        ErrorCode::KakaoWindowNotVisible => "카카오톡 창을 찾을 수 없습니다. 창이 최소화되어 있을 수 있습니다.".into(),
        ErrorCode::AccessibilityPermissionDenied => {
            "kakao-cli 에 접근성 권한이 없습니다.".into()
        }
        ErrorCode::AppVersionUnsupported => {
            "이 카카오톡 버전은 아직 지원하지 않습니다.".into()
        }
        ErrorCode::RoomNotFound => "그 방을 찾을 수 없습니다. 목록이 오래됐을 수 있습니다.".into(),
        ErrorCode::UiElementNotFound => {
            "카카오톡 화면에서 필요한 요소를 찾지 못했습니다. UI가 바뀌었을 수 있습니다.".into()
        }
        ErrorCode::SendInputFailed => "메시지를 입력창에 넣지 못했습니다. 전송하지 않았습니다.".into(),
        ErrorCode::SendVerifyTimeout => {
            "전송 여부를 확인할 수 없습니다. 카카오톡에서 직접 확인하세요.".into()
        }
        ErrorCode::EmptyMessage => "빈 메시지는 보낼 수 없습니다.".into(),
        ErrorCode::MessageTooLong => "메시지가 너무 깁니다.".into(),
    }
}

pub fn recovery_hint(code: ErrorCode) -> Option<String> {
    match code {
        ErrorCode::KakaoNotRunning => {
            Some("카카오톡 데스크톱 앱을 실행한 뒤 다시 시도하세요.".into())
        }
        ErrorCode::KakaoWindowNotVisible => {
            Some("Dock 의 카카오톡 아이콘을 클릭해 창을 열고 다시 시도하세요.".into())
        }
        ErrorCode::AccessibilityPermissionDenied => Some(
            "시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용에서 kakao-cli 를 켜세요.\n\
             설정 열기:  open \"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility\"\n\
             그다음:     kakao-cli doctor"
                .into(),
        ),
        ErrorCode::AppVersionUnsupported => {
            Some("kakao-cli doctor 로 버전을 확인하고 업데이트를 알려주세요.".into())
        }
        ErrorCode::RoomNotFound => Some("kakao-cli rooms 로 방 목록을 다시 확인하세요.".into()),
        ErrorCode::UiElementNotFound => {
            Some("kakao-cli doctor 를 실행해 진단 결과를 확인하세요.".into())
        }
        ErrorCode::MessageTooLong => Some("--max-chars 로 상한을 조정하거나 메시지를 줄이세요.".into()),
        _ => None,
    }
}

// --------------------------------------------------------------------------
// prompts
// --------------------------------------------------------------------------

fn read_line(prompt: &str) -> AppResult<String> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut buf = String::new();
    let n = std::io::stdin()
        .read_line(&mut buf)
        .map_err(|e| AppError::internal(format!("입력 읽기 실패: {e}")))?;
    if n == 0 {
        return Err(AppError::Aborted); // EOF
    }
    Ok(buf.trim().to_string())
}

/// `[y/N]` confirmation. Default (empty / anything not y) is No.
pub fn confirm(prompt: &str) -> AppResult<bool> {
    let ans = read_line(&format!("{prompt} [y/N] "))?;
    Ok(matches!(ans.to_lowercase().as_str(), "y" | "yes"))
}

/// Numbered selection with NO default. Returns the chosen 0-based index, or
/// `AppError::Aborted` on `q`/EOF.
pub fn choose(header: &str, items: &[String]) -> AppResult<usize> {
    eprintln!("{header}");
    for (i, item) in items.iter().enumerate() {
        eprintln!("  {}. {}", i + 1, item);
    }
    loop {
        let ans = read_line(&format!("선택 [1-{}, q]: ", items.len()))?;
        if ans.eq_ignore_ascii_case("q") {
            return Err(AppError::Aborted);
        }
        match ans.parse::<usize>() {
            Ok(n) if (1..=items.len()).contains(&n) => return Ok(n - 1),
            _ => eprintln!("1부터 {} 사이 번호 또는 q 를 입력하세요.", items.len()),
        }
    }
}

// --------------------------------------------------------------------------
// lists
// --------------------------------------------------------------------------

fn display_width(s: &str) -> usize {
    // Approximate: CJK code points take two columns.
    s.chars()
        .map(|c| if (c as u32) >= 0x1100 && !c.is_ascii() { 2 } else { 1 })
        .sum()
}

fn pad(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

pub fn render_inbox(rooms: &[Room]) -> String {
    if rooms.is_empty() {
        return "읽지 않은 메시지가 없습니다.".into();
    }
    let title_w = rooms
        .iter()
        .map(|r| display_width(&r.title))
        .max()
        .unwrap_or(0)
        .clamp(8, 24);

    let mut out = String::new();
    for r in rooms {
        let marker = if r.unread_count > 0 { "●" } else { " " };
        let unread = if r.unread_count > 0 {
            format!("{:>2}", r.unread_count)
        } else {
            "  ".into()
        };
        let preview = match &r.last_message {
            Some(m) if !m.text.is_empty() && !m.sender.is_empty() => {
                format!("{}: {}", m.sender, m.text)
            }
            Some(m) if !m.text.is_empty() => m.text.clone(),
            _ => String::new(),
        };
        out.push_str(&format!(
            "{marker} {}  {unread}  {preview}\n",
            pad(&r.title, title_w)
        ));
    }
    out.trim_end().to_string()
}

pub fn render_rooms(rooms: &[Room]) -> String {
    if rooms.is_empty() {
        return "일치하는 방이 없습니다.".into();
    }
    let title_w = rooms
        .iter()
        .map(|r| display_width(&r.title))
        .max()
        .unwrap_or(0)
        .clamp(8, 24);
    let mut out = String::new();
    for r in rooms {
        let members = match r.member_count {
            Some(n) => format!("{n}명"),
            None => "인원 미상".into(),
        };
        let last = match &r.last_message {
            Some(m) => match time_util::parse_iso(&m.at) {
                Some(dt) => format!("마지막 메시지 {}", time_util::relative_ko(dt)),
                None => "마지막 메시지 시각 미상".into(),
            },
            None => "메시지 없음".into(),
        };
        out.push_str(&format!("{}  {members} · {last}\n", pad(&r.title, title_w)));
    }
    out.trim_end().to_string()
}

pub fn render_messages(messages: &[Message]) -> String {
    if messages.is_empty() {
        return "표시할 메시지가 없습니다.".into();
    }
    let mut out = String::new();
    for m in messages {
        let hm = time_util::parse_iso(&m.at)
            .map(time_util::local_hm)
            .unwrap_or_else(|| "--:--".into());
        let body = match m.kind {
            MessageKind::Text => m.text.clone(),
            MessageKind::Unsupported => "(지원하지 않는 메시지)".into(),
        };
        // Outgoing messages carry no sender label from the adapter; show "나".
        let who = if m.outgoing && m.sender.is_empty() {
            "나"
        } else {
            m.sender.as_str()
        };
        out.push_str(&format!("[{hm}] {who}  {body}\n"));
    }
    out.trim_end().to_string()
}

pub fn render_search(hits: &[crate::db::SearchHit]) -> String {
    if hits.is_empty() {
        return "일치하는 메시지가 없습니다.".into();
    }
    let mut out = String::new();
    for h in hits {
        let when = time_util::parse_iso(&h.at)
            .map(time_util::local_datetime)
            .unwrap_or_else(|| h.at.clone());
        out.push_str(&format!(
            "{}   {}   {when}   {}\n",
            h.room_title, h.sender, h.snippet
        ));
    }
    out.trim_end().to_string()
}

pub fn render_doctor(h: &Health) -> String {
    let check = |ok: bool| if ok { "✓" } else { "✗" };
    let mut out = String::new();
    out.push_str(&format!("카카오톡 실행         {}\n", check(h.kakao_running)));
    out.push_str(&format!(
        "접근성 권한           {}\n",
        check(h.accessibility_granted)
    ));
    out.push_str(&format!(
        "앱 버전               {}\n",
        h.app_version.as_deref().unwrap_or("(확인 불가)")
    ));
    for issue in &h.issues {
        let recovery = recovery_hint(issue.code)
            .unwrap_or_else(|| issue.recovery.clone());
        out.push('\n');
        out.push_str(&format!("• {}\n  {}\n", error_message(issue.code), recovery.replace('\n', "\n  ")));
    }
    if h.issues.is_empty() {
        out.push_str("\n모든 점검을 통과했습니다.");
    }
    out.trim_end().to_string()
}
