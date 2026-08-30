//! serve-mode wire framing (contract §5). These lock the exact JSON shapes the
//! bridges must produce and the core must accept.

use kakao_contract::{
    ErrorCode, Message, MessageKind, Method, ServeEvent, ServeMessage, ServeRequest, ServeResponse,
};

#[test]
fn request_roundtrips_and_names_methods() {
    let req = ServeRequest::new(3, Method::ReadRecent, serde_json::json!({ "roomId": "row:3", "limit": 40 }));
    let line = serde_json::to_string(&req).unwrap();
    assert_eq!(
        line,
        r#"{"id":3,"method":"readRecent","params":{"limit":40,"roomId":"row:3"}}"#
    );
    let back: ServeRequest = serde_json::from_str(&line).unwrap();
    assert_eq!(back.method, "readRecent");
    assert_eq!(Method::from_wire(&back.method), Some(Method::ReadRecent));
}

#[test]
fn response_shapes() {
    let ok = ServeResponse::ok(3, serde_json::json!({ "messages": [] }));
    assert_eq!(
        serde_json::to_string(&ok).unwrap(),
        r#"{"id":3,"ok":true,"data":{"messages":[]}}"#
    );
    let err = ServeResponse::err(4, ErrorCode::SendInputFailed);
    assert_eq!(
        serde_json::to_string(&err).unwrap(),
        r#"{"id":4,"ok":false,"error":"SEND_INPUT_FAILED"}"#
    );
}

#[test]
fn message_event_shape() {
    let ev = ServeEvent::Message {
        room_id: "row:3".into(),
        message: Message {
            sender: "홍길동".into(),
            text: "안녕".into(),
            at: "2026-08-31T09:00:00Z".into(),
            outgoing: false,
            kind: MessageKind::Text,
        },
    };
    let line = serde_json::to_string(&ev).unwrap();
    assert!(line.contains(r#""event":"message""#));
    assert!(line.contains(r#""roomId":"row:3""#));
    assert!(line.contains(r#""kind":"text""#));
}

#[test]
fn serve_message_parse_discriminates_events_from_responses() {
    match ServeMessage::parse(r#"{"id":3,"ok":true,"data":{}}"#).unwrap() {
        ServeMessage::Response(r) => assert_eq!(r.id, 3),
        other => panic!("expected Response, got {other:?}"),
    }
    match ServeMessage::parse(r#"{"event":"roomClosed","roomId":"row:1"}"#).unwrap() {
        ServeMessage::Event(ServeEvent::RoomClosed { room_id }) => assert_eq!(room_id, "row:1"),
        other => panic!("expected RoomClosed event, got {other:?}"),
    }
    match ServeMessage::parse(r#"{"event":"error","code":"KAKAO_WINDOW_NOT_VISIBLE"}"#).unwrap() {
        ServeMessage::Event(ServeEvent::Error { code }) => {
            assert_eq!(code, ErrorCode::KakaoWindowNotVisible)
        }
        other => panic!("expected Error event, got {other:?}"),
    }
}
