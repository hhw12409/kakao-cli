//! Behaviour tests for the common core against the streaming mock. No
//! KakaoTalk, no subprocess — these exercise room resolution, the send state
//! machine, and the DB.

use kakao_contract::{Message, MessageKind, Room, SendStatus};
use kakao_core::adapter::MockStreamAdapter;
use kakao_core::db;
use kakao_core::resolve::{resolve_in_list, Resolution};
use kakao_core::send::send_in_room;

fn room(id: &str, title: &str) -> Room {
    Room {
        room_id: id.into(),
        title: title.into(),
        member_count: Some(2),
        unread_count: 0,
        last_message: None,
    }
}

fn rooms() -> Vec<Room> {
    vec![
        room("r1", "개발팀"),
        room("r2", "개발 공지"),
        room("r3", "엄마"),
    ]
}

const SEND_FIXTURE: &str = r#"{
  "rooms": [ { "roomId": "r3", "title": "엄마", "memberCount": 2, "unreadCount": 0, "lastMessage": null } ],
  "history": { "r3": [] }
}"#;

// --- resolution ------------------------------------------------------------

#[test]
fn resolve_single_match() {
    let conn = db::open_in_memory().unwrap();
    match resolve_in_list(&rooms(), &conn, "엄마", false).unwrap() {
        Resolution::One(r) => assert_eq!(r.room_id, "r3"),
        other => panic!("expected One, got {other:?}"),
    }
}

#[test]
fn resolve_zero_matches() {
    let conn = db::open_in_memory().unwrap();
    match resolve_in_list(&rooms(), &conn, "없는방", false).unwrap() {
        Resolution::None { query, .. } => assert_eq!(query, "없는방"),
        other => panic!("expected None, got {other:?}"),
    }
}

#[test]
fn resolve_ambiguous_returns_all_candidates_no_default() {
    let conn = db::open_in_memory().unwrap();
    match resolve_in_list(&rooms(), &conn, "개발", false).unwrap() {
        Resolution::Many(v) => assert_eq!(v.len(), 2),
        other => panic!("expected Many, got {other:?}"),
    }
}

#[test]
fn resolve_exact_disambiguates() {
    let conn = db::open_in_memory().unwrap();
    match resolve_in_list(&rooms(), &conn, "개발팀", true).unwrap() {
        Resolution::One(r) => assert_eq!(r.room_id, "r1"),
        other => panic!("expected One, got {other:?}"),
    }
}

#[test]
fn alias_expands_before_resolution() {
    let conn = db::open_in_memory().unwrap();
    db::alias_add(&conn, "mom", "엄마").unwrap();
    match resolve_in_list(&rooms(), &conn, "@mom", false).unwrap() {
        Resolution::One(r) => assert_eq!(r.room_id, "r3"),
        other => panic!("expected One, got {other:?}"),
    }
}

// --- send state machine ---------------------------------------------------

fn adapter_with_send(status_json: &str) -> MockStreamAdapter {
    let fx = SEND_FIXTURE
        .trim_end_matches('}')
        .to_string()
        + &format!(", \"sendText\": {status_json} }}");
    MockStreamAdapter::from_fixture_str(&fx).unwrap()
}

#[test]
fn send_success_logs_sent() {
    let mut a = MockStreamAdapter::from_fixture_str(SEND_FIXTURE).unwrap();
    let conn = db::open_in_memory().unwrap();
    let out = send_in_room(&mut a, &conn, "r3", "엄마", "곧 도착해요", 2000).unwrap();
    assert_eq!(out.status, SendStatus::Sent);

    let status: String = conn
        .query_row("SELECT status FROM send_log ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "sent");
}

#[test]
fn send_unknown_is_logged_unknown_and_not_retried() {
    let mut a = adapter_with_send(
        r#"{ "status": "unknown", "at": null, "error": "SEND_VERIFY_TIMEOUT" }"#,
    );
    let conn = db::open_in_memory().unwrap();
    let out = send_in_room(&mut a, &conn, "r3", "엄마", "hi", 2000).unwrap();
    assert_eq!(out.status, SendStatus::Unknown);

    let status: String = conn
        .query_row("SELECT status FROM send_log ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "unknown");
}

#[test]
fn send_empty_message_never_logged() {
    let mut a = MockStreamAdapter::from_fixture_str(SEND_FIXTURE).unwrap();
    let conn = db::open_in_memory().unwrap();
    let err = send_in_room(&mut a, &conn, "r3", "엄마", "   ", 2000).unwrap_err();
    assert_eq!(err.exit_code(), 8);

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM send_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn send_too_long_never_logged() {
    let mut a = MockStreamAdapter::from_fixture_str(SEND_FIXTURE).unwrap();
    let conn = db::open_in_memory().unwrap();
    let body = "가".repeat(50);
    let err = send_in_room(&mut a, &conn, "r3", "엄마", &body, 10).unwrap_err();
    assert_eq!(err.exit_code(), 8);

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM send_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

// --- db -----------------------------------------------------------------

#[test]
fn recent_messages_roundtrips_oldest_first() {
    let conn = db::open_in_memory().unwrap();
    db::ensure_room(&conn, "r1", "개발팀").unwrap();
    db::insert_messages(
        &conn,
        "r1",
        &[
            Message {
                sender: "민수".into(),
                text: "먼저".into(),
                at: "2026-08-29T01:00:00Z".into(),
                outgoing: false,
                kind: MessageKind::Text,
            },
            Message {
                sender: "나".into(),
                text: "나중".into(),
                at: "2026-08-29T01:05:00Z".into(),
                outgoing: true,
                kind: MessageKind::Text,
            },
        ],
    )
    .unwrap();

    let got = db::recent_messages(&conn, "r1", 10).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].text, "먼저");
    assert_eq!(got[1].text, "나중");
}

#[test]
fn alias_conflict_is_exit_9() {
    let conn = db::open_in_memory().unwrap();
    db::alias_add(&conn, "dev", "개발팀").unwrap();
    let err = db::alias_add(&conn, "dev", "다른방").unwrap_err();
    assert_eq!(err.exit_code(), 9);
}

#[test]
fn cached_rooms_round_trips_for_offline_view() {
    let conn = db::open_in_memory().unwrap();
    let mut src = rooms();
    src[0].unread_count = 3;
    src[1].last_message = Some(kakao_contract::LastMessage {
        text: "회의 시작".into(),
        at: "2026-08-31T09:00:00Z".into(),
        sender: "팀장".into(),
    });
    db::upsert_rooms(&conn, &src).unwrap();

    let cached = db::cached_rooms(&conn).unwrap();
    assert_eq!(cached.len(), 3);
    // listing order preserved
    assert_eq!(cached[0].title, "개발팀");
    assert_eq!(cached[0].unread_count, 3);
    assert_eq!(
        cached[1].last_message.as_ref().map(|m| m.text.as_str()),
        Some("회의 시작")
    );
}
