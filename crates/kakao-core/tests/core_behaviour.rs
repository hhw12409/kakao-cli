//! Behaviour tests for the common core against the mock adapter. No KakaoTalk,
//! no subprocess — these exercise resolution, the send state machine, and FTS.

use kakao_core::adapter::MockAdapter;
use kakao_core::cli::SendArgs;
use kakao_core::db;
use kakao_core::error::AppError;
use kakao_core::resolve::{resolve_room, Interactivity};

const FIXTURE: &str = r#"{
  "listRooms": {
    "rooms": [
      { "roomId": "r1", "title": "개발팀",    "memberCount": 18, "unreadCount": 2,
        "lastMessage": { "text": "배포 끝났어요?", "at": "2026-08-29T01:00:00Z", "sender": "민수" } },
      { "roomId": "r2", "title": "개발 공지",  "memberCount": 42, "unreadCount": 0, "lastMessage": null },
      { "roomId": "r3", "title": "엄마",       "memberCount": 2,  "unreadCount": 0,
        "lastMessage": { "text": "저녁 먹었니", "at": "2026-08-29T00:00:00Z", "sender": "엄마" } }
    ]
  },
  "readRecent": {
    "r1": { "messages": [
      { "sender": "민수", "text": "배포 끝났어요?", "at": "2026-08-29T01:00:00Z", "outgoing": false, "kind": "text" },
      { "sender": "나",   "text": "확인 중입니다",  "at": "2026-08-29T01:01:00Z", "outgoing": true,  "kind": "text" }
    ] }
  },
  "sendText": { "status": "sent", "at": "2026-08-29T02:00:00Z", "error": null },
  "healthCheck": { "kakaoRunning": true, "accessibilityGranted": true, "appVersion": "3.0.0", "issues": [] }
}"#;

fn mock() -> MockAdapter {
    MockAdapter::from_fixture_str(FIXTURE).unwrap()
}

fn send_args(room: &str, msg: Option<&str>) -> SendArgs {
    SendArgs {
        room: room.to_string(),
        message: msg.map(str::to_string),
        stdin: false,
        exact: false,
        yes: false,
        dry_run: false,
        max_chars: 2000,
    }
}

#[test]
fn resolve_exact_single_match() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let room = resolve_room(&a, &conn, "엄마", false, Interactivity::NonInteractive).unwrap();
    assert_eq!(room.room_id, "r3");
}

#[test]
fn resolve_zero_matches_is_room_not_found() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let err = resolve_room(&a, &conn, "없는방", false, Interactivity::NonInteractive).unwrap_err();
    assert!(matches!(err, AppError::RoomNotFound { .. }));
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn resolve_ambiguous_non_interactive_is_exit_5_and_lists_candidates() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let err = resolve_room(&a, &conn, "개발", false, Interactivity::NonInteractive).unwrap_err();
    match err {
        AppError::RoomAmbiguous { ref candidates } => {
            assert_eq!(candidates.len(), 2); // 개발팀, 개발 공지
        }
        other => panic!("expected RoomAmbiguous, got {other:?}"),
    }
    assert_eq!(err.exit_code(), 5);
}

#[test]
fn resolve_exact_flag_disambiguates() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let room = resolve_room(&a, &conn, "개발팀", true, Interactivity::NonInteractive).unwrap();
    assert_eq!(room.room_id, "r1");
}

#[test]
fn send_success_writes_sent_to_log() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    kakao_core::send::run_send(&a, &conn, send_args("엄마", Some("곧 도착해요"))).unwrap();

    let (status, err): (String, Option<String>) = conn
        .query_row(
            "SELECT status, error_code FROM send_log ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "sent");
    assert_eq!(err, None);
}

#[test]
fn send_dry_run_does_not_touch_send_log() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let mut args = send_args("엄마", Some("테스트"));
    args.dry_run = true;
    kakao_core::send::run_send(&a, &conn, args).unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM send_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "--dry-run must not enter the pending state");
}

#[test]
fn send_unknown_is_exit_6_and_logged_unknown() {
    let fixture = FIXTURE.replace(
        r#""sendText": { "status": "sent", "at": "2026-08-29T02:00:00Z", "error": null }"#,
        r#""sendText": { "status": "unknown", "at": null, "error": "SEND_VERIFY_TIMEOUT" }"#,
    );
    let a = MockAdapter::from_fixture_str(&fixture).unwrap();
    let conn = db::open_in_memory().unwrap();

    let err = kakao_core::send::run_send(&a, &conn, send_args("엄마", Some("hi"))).unwrap_err();
    assert!(matches!(err, AppError::SendUnknown));
    assert_eq!(err.exit_code(), 6);

    let status: String = conn
        .query_row("SELECT status FROM send_log ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "unknown");
}

#[test]
fn send_empty_message_is_exit_8_and_never_logged() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let err = kakao_core::send::run_send(&a, &conn, send_args("엄마", Some("   "))).unwrap_err();
    assert_eq!(err.exit_code(), 8);

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM send_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn send_ambiguous_room_never_sends() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    let err = kakao_core::send::run_send(&a, &conn, send_args("개발", Some("hi"))).unwrap_err();
    assert_eq!(err.exit_code(), 5);

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM send_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "ambiguous resolution must not reach sendText");
}

#[test]
fn alias_expands_before_resolution() {
    let a = mock();
    let conn = db::open_in_memory().unwrap();
    db::alias_add(&conn, "mom", "엄마").unwrap();
    let room = resolve_room(&a, &conn, "@mom", false, Interactivity::NonInteractive).unwrap();
    assert_eq!(room.room_id, "r3");
}

#[test]
fn fts_search_finds_cached_message() {
    let conn = db::open_in_memory().unwrap();
    db::ensure_room(&conn, "r1", "개발팀").unwrap();
    db::insert_messages(
        &conn,
        "r1",
        &[kakao_contract::Message {
            sender: "민수".into(),
            text: "배포 끝났어요?".into(),
            at: "2026-08-29T01:00:00Z".into(),
            outgoing: false,
            kind: kakao_contract::MessageKind::Text,
        }],
    )
    .unwrap();

    let hits = db::search(&conn, "배포", None).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].room_title, "개발팀");
    assert!(hits[0].snippet.contains('['));
}

#[test]
fn alias_conflict_is_exit_9() {
    let conn = db::open_in_memory().unwrap();
    db::alias_add(&conn, "dev", "개발팀").unwrap();
    let err = db::alias_add(&conn, "dev", "다른방").unwrap_err();
    assert_eq!(err.exit_code(), 9);
}
