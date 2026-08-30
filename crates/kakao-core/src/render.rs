//! User-facing output for the non-TUI surface (`doctor`) and shared error
//! copy. The TUI renders itself (`tui::ui`). Message bodies never go to logs or
//! telemetry.

use std::io::IsTerminal;

use kakao_contract::{ErrorCode, Health};

// --------------------------------------------------------------------------
// environment
// --------------------------------------------------------------------------

/// True when both stdin and stdout are a tty — i.e. it is sane to open the TUI.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

// --------------------------------------------------------------------------
// error copy  (final wording is docs/errors.md; kept in sync here)
// --------------------------------------------------------------------------

pub fn error_message(code: ErrorCode) -> String {
    match code {
        ErrorCode::KakaoNotRunning => "카카오톡이 실행되고 있지 않습니다.".into(),
        ErrorCode::KakaoWindowNotVisible => {
            "카카오톡 창을 찾을 수 없습니다. 창이 최소화되어 있을 수 있습니다.".into()
        }
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
            concat!(
                "kakao-cli 가 카카오톡 창을 읽으려면 접근성(자동화) 권한이 필요합니다.\n",
                "\n",
                "권장 — kakao-cli doctor 를 실행하면 시스템 권한 요청 창이 뜹니다:\n",
                "  1. [시스템 설정 열기] 를 누른다\n",
                "  2. 손쉬운 사용 목록에서 kakao-cli 항목의 토글을 켠다\n",
                "  3. 터미널 앱을 완전히 종료했다 다시 열고:  kakao-cli doctor\n",
                "\n",
                "창이 안 뜨거나 항목이 없으면 — 터미널 앱에 권한을 준다(가장 확실, 업그레이드에도 유지):\n",
                "  시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용 → '+' →\n",
                "  실행 중인 터미널 앱(iTerm / Terminal) 을 추가하고 토글을 켠다 → 터미널 재시작\n",
                "\n",
                "설정 바로 열기:\n",
                "  open \"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility\""
            )
            .into(),
        ),
        ErrorCode::AppVersionUnsupported => {
            Some("kakao-cli doctor 로 버전을 확인하고 업데이트를 알려주세요.".into())
        }
        ErrorCode::RoomNotFound => Some("채팅 화면에서 /rooms 로 방 목록을 다시 확인하세요.".into()),
        ErrorCode::UiElementNotFound => {
            Some("kakao-cli doctor 를 실행해 진단 결과를 확인하세요.".into())
        }
        ErrorCode::MessageTooLong => Some("메시지를 짧게 나눠 보내세요.".into()),
        _ => None,
    }
}

// --------------------------------------------------------------------------
// doctor
// --------------------------------------------------------------------------

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
        let recovery = recovery_hint(issue.code).unwrap_or_else(|| issue.recovery.clone());
        out.push('\n');
        out.push_str(&format!(
            "• {}\n  {}\n",
            error_message(issue.code),
            recovery.replace('\n', "\n  ")
        ));
    }
    if h.issues.is_empty() {
        out.push_str("\n모든 점검을 통과했습니다.");
    }
    out.trim_end().to_string()
}
