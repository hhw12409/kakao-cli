//! `kakao-windows-bridge --self-test` — parser regression checks against the
//! bundled `scenario-basic` fixture. Host-independent (runs on macOS via
//! `cargo test` too — see `tests/`). Mirrors the macOS `--self-test`.

use time::macros::datetime;

use crate::node::FixtureNode;
use crate::{parsers, selectors};

const FIXTURE: &str = include_str!("../fixtures/scenario-basic.json");

pub fn run() -> ! {
    let failures = run_checks(|label, ok| {
        println!("  {} {label}", if ok { "ok  " } else { "FAIL" });
    });
    if failures == 0 {
        println!("\nall checks passed");
        std::process::exit(0);
    }
    println!("\n{failures} check(s) failed");
    std::process::exit(1);
}

/// Returns the number of failed checks. `report` is called per check.
pub fn run_checks(mut report: impl FnMut(&str, bool)) -> usize {
    let mut failures = 0;
    let mut check = |label: &str, cond: bool| {
        report(label, cond);
        if !cond {
            failures += 1;
        }
    };

    let now = datetime!(2026-08-29 00:00:00 UTC);
    let sel = selectors::for_version(None).unwrap();
    let tree: FixtureNode = serde_json::from_str(FIXTURE).expect("fixture parses");

    let main = tree
        .children
        .iter()
        .find(|w| w.name.as_deref() == Some("카카오톡"))
        .expect("main window");
    let conv = tree
        .children
        .iter()
        .find(|w| w.name.as_deref() == Some("개발팀"))
        .expect("conversation window");

    println!("scenario-basic — listRooms");
    let rooms = parsers::rooms(main, sel, now);
    check("3 rooms parsed (spacer skipped)", rooms.len() == 3);
    check(
        "titles in list order",
        rooms.iter().map(|r| r.title.as_str()).collect::<Vec<_>>()
            == ["개발팀", "엄마", "개발 공지"],
    );
    check(
        "opaque roomIds are row indices",
        rooms.iter().map(|r| r.room_id.as_str()).collect::<Vec<_>>() == ["row:0", "row:1", "row:2"],
    );
    check("unread badge -> 2", rooms[0].unread_count == 2);
    check("no unread badge -> 0", rooms[1].unread_count == 0);
    check("member count 18 (group)", rooms[0].member_count == Some(18));
    check(
        "no member count -> 1:1 -> 2",
        rooms[1].member_count == Some(2),
    );
    check(
        "last message preview",
        rooms[0].last_message.as_ref().map(|m| m.text.as_str()) == Some("배포 끝났어요?"),
    );
    check(
        "empty preview -> no lastMessage",
        rooms[2].last_message.is_none(),
    );
    check(
        "preview timestamp -> ISO UTC",
        rooms[0]
            .last_message
            .as_ref()
            .is_some_and(|m| m.at.ends_with('Z')),
    );

    println!("scenario-basic — readRecent");
    let msgs = parsers::messages(conv, sel, Some(1000.0), now);
    check("5 messages parsed (spacer skipped)", msgs.len() == 5);
    check("first message body", msgs[0].text == "배포 끝났어요?");
    check("sender from Text next to profile", msgs[0].sender == "민수");
    check("left-aligned bubble -> incoming", !msgs[0].outgoing);
    check("message time -> ISO", msgs[0].at.ends_with('Z'));
    check("sender carried to continuation", msgs[1].sender == "민수");
    check(
        "time inherited on no-timestamp row",
        msgs[1].at == msgs[0].at,
    );
    check("outgoing message body", msgs[2].text == "넵 확인했습니다");
    check("right-aligned bubble -> outgoing", msgs[2].outgoing);
    check("outgoing carries no sender", msgs[2].sender.is_empty());
    check("new incoming sender run", msgs[3].sender == "수빈");
    check(
        "multiline timestamp parsed (not inherited)",
        msgs[3].at != msgs[1].at,
    );
    check(
        "media item -> unsupported",
        matches!(msgs[4].kind, kakao_contract::MessageKind::Unsupported) && msgs[4].text.is_empty(),
    );

    println!("korean time parsing");
    check(
        "오전 11:17 -> (11,17)",
        crate::korean_time::parse_hour_minute("오전 11:17") == Some((11, 17)),
    );
    check(
        "오후 12:12 -> (12,12)",
        crate::korean_time::parse_hour_minute("오후 12:12") == Some((12, 12)),
    );
    check(
        "오전 12:30 -> (0,30)",
        crate::korean_time::parse_hour_minute("오전 12:30") == Some((0, 30)),
    );
    check(
        "어제 -> ISO",
        crate::korean_time::to_iso("어제", now).ends_with('Z'),
    );
    check(
        "empty -> empty",
        crate::korean_time::to_iso("", now).is_empty(),
    );

    println!("contract shape");
    let sr = kakao_contract::SendResult {
        status: kakao_contract::SendStatus::Unknown,
        at: None,
        error: Some(kakao_contract::ErrorCode::SendVerifyTimeout),
    };
    let json = serde_json::to_string(&sr).unwrap();
    check(
        "SendResult status wire value",
        json.contains("\"status\":\"unknown\""),
    );
    check(
        "ErrorCode wire value",
        json.contains("\"error\":\"SEND_VERIFY_TIMEOUT\""),
    );
    check(
        "ErrorCode raw values match contract",
        kakao_contract::ErrorCode::AccessibilityPermissionDenied.as_str()
            == "ACCESSIBILITY_PERMISSION_DENIED",
    );

    failures
}
