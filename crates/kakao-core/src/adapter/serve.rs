//! Serve-mode adapter: one long-lived `kakao-<os>-bridge serve` process spoken
//! to over newline-delimited JSON. Contract: `docs/adapter-contract.md` §5.
//!
//! A reader thread turns stdout lines into [`ServeMessage`]s on a channel; the
//! owning thread (the TUI worker) correlates responses by `id` and buffers any
//! events seen while waiting.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use kakao_contract::{
    ErrorCode, Health, ListRoomsData, Method, ReadRecentData, SendResult, SendStatus, ServeEvent,
    ServeMessage, ServeRequest,
};
use serde_json::{json, Value};

use crate::adapter::stream::{StreamAdapter, StreamEvent};
use crate::error::{AppError, AppResult};

/// Round-trip budget for a serve request. Generous: a cold accessibility walk
/// plus `openRoom`'s click-and-wait can take a while on a large window.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(20_000);

enum ReaderMsg {
    Line(ServeMessage),
    Closed(String),
}

pub struct ServeAdapter {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<ReaderMsg>,
    pending: VecDeque<StreamEvent>,
    next_id: u64,
    dead: Option<String>,
}

impl ServeAdapter {
    pub fn spawn(bin: PathBuf) -> AppResult<Self> {
        let mut child = Command::new(&bin)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                AppError::internal(format!("어댑터 실행 실패 ({}): {e}", bin.display()))
            })?;

        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) if l.trim().is_empty() => {}
                    Ok(l) => match ServeMessage::parse(&l) {
                        Ok(m) => {
                            if tx.send(ReaderMsg::Line(m)).is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(ReaderMsg::Closed(format!(
                                "어댑터 응답을 파싱할 수 없습니다: {e}"
                            )));
                            return;
                        }
                    },
                    Err(e) => {
                        let _ = tx
                            .send(ReaderMsg::Closed(format!("어댑터 출력 스트림 오류: {e}")));
                        return;
                    }
                }
            }
            let _ = tx.send(ReaderMsg::Closed("어댑터 프로세스가 종료되었습니다".into()));
        });

        // Drain stderr so a full pipe never blocks the child. Diagnostics only.
        if let Some(errpipe) = child.stderr.take() {
            thread::spawn(move || {
                let mut sink = String::new();
                let _ = BufReader::new(errpipe).read_to_string(&mut sink);
            });
        }

        Ok(Self {
            child,
            stdin,
            rx,
            pending: VecDeque::new(),
            next_id: 1,
            dead: None,
        })
    }

    fn mark_dead(&mut self, msg: String) -> AppError {
        if self.dead.is_none() {
            self.dead = Some(msg.clone());
        }
        AppError::internal(msg)
    }

    fn request(&mut self, method: Method, params: Value) -> AppResult<Value> {
        if let Some(msg) = &self.dead {
            return Err(AppError::internal(msg.clone()));
        }

        let id = self.next_id;
        self.next_id += 1;

        let req = ServeRequest::new(id, method, params);
        let mut line = serde_json::to_string(&req)
            .map_err(|e| AppError::internal(format!("요청 직렬화 실패: {e}")))?;
        line.push('\n');
        if let Err(e) = self.stdin.write_all(line.as_bytes()) {
            return Err(self.mark_dead(format!("어댑터 입력 쓰기 실패: {e}")));
        }
        let _ = self.stdin.flush();

        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AppError::Timeout(method))?;
            match self.rx.recv_timeout(remaining) {
                Ok(ReaderMsg::Line(ServeMessage::Response(r))) if r.id == id => {
                    return if r.ok {
                        Ok(r.data.unwrap_or(Value::Null))
                    } else {
                        Err(AppError::adapter(
                            r.error.unwrap_or(ErrorCode::UiElementNotFound),
                        ))
                    };
                }
                // A response for some other id — stale, ignore.
                Ok(ReaderMsg::Line(ServeMessage::Response(_))) => {}
                Ok(ReaderMsg::Line(ServeMessage::Event(e))) => {
                    self.pending.push_back(translate(e));
                }
                Ok(ReaderMsg::Closed(m)) => return Err(self.mark_dead(m)),
                Err(RecvTimeoutError::Timeout) => return Err(AppError::Timeout(method)),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(self.mark_dead("어댑터 스트림 연결이 끊겼습니다".into()))
                }
            }
        }
    }

    fn request_typed<T: serde::de::DeserializeOwned>(
        &mut self,
        method: Method,
        params: Value,
    ) -> AppResult<T> {
        let data = self.request(method, params)?;
        serde_json::from_value(data).map_err(|e| {
            AppError::internal(format!("어댑터 계약 위반 ({}): {e}", method.wire_name()))
        })
    }
}

fn translate(e: ServeEvent) -> StreamEvent {
    match e {
        ServeEvent::Message { room_id, message } => StreamEvent::Message { room_id, message },
        ServeEvent::RoomClosed { room_id } => StreamEvent::RoomClosed { room_id },
        ServeEvent::Error { code } => StreamEvent::Warn(code),
    }
}

impl StreamAdapter for ServeAdapter {
    fn list_rooms(&mut self) -> AppResult<ListRoomsData> {
        self.request_typed(Method::ListRooms, json!({}))
    }

    fn open_room(&mut self, room_id: &str) -> AppResult<()> {
        self.request(Method::OpenRoom, json!({ "roomId": room_id }))?;
        Ok(())
    }

    fn read_recent(&mut self, room_id: &str, limit: u32) -> AppResult<ReadRecentData> {
        self.request_typed(
            Method::ReadRecent,
            json!({ "roomId": room_id, "limit": limit }),
        )
    }

    fn send_text(&mut self, room_id: &str, text: &str) -> AppResult<SendResult> {
        match self.request_typed::<SendResult>(
            Method::SendText,
            json!({ "roomId": room_id, "text": text }),
        ) {
            Ok(r) => Ok(r),
            // A send whose verification we could not observe is `unknown`,
            // never a retry (contract §3).
            Err(AppError::Timeout(Method::SendText)) => Ok(SendResult {
                status: SendStatus::Unknown,
                at: None,
                error: Some(ErrorCode::SendVerifyTimeout),
            }),
            Err(other) => Err(other),
        }
    }

    fn watch(&mut self, room_id: &str) -> AppResult<()> {
        self.request(Method::Watch, json!({ "roomId": room_id }))?;
        Ok(())
    }

    fn unwatch(&mut self) -> AppResult<()> {
        self.request(Method::Unwatch, json!({}))?;
        Ok(())
    }

    fn health_check(&mut self) -> AppResult<Health> {
        self.request_typed(Method::HealthCheck, json!({}))
    }

    fn next_event(&mut self, timeout: Duration) -> Option<StreamEvent> {
        if let Some(ev) = self.pending.pop_front() {
            return Some(ev);
        }
        if let Some(msg) = self.dead.take() {
            return Some(StreamEvent::Disconnected(msg));
        }
        match self.rx.recv_timeout(timeout) {
            Ok(ReaderMsg::Line(ServeMessage::Event(e))) => Some(translate(e)),
            Ok(ReaderMsg::Line(ServeMessage::Response(_))) => None,
            Ok(ReaderMsg::Closed(m)) => Some(StreamEvent::Disconnected(m)),
            Err(_) => None,
        }
    }

    fn shutdown(&mut self) {
        let _ = self
            .stdin
            .write_all(b"{\"id\":0,\"method\":\"shutdown\",\"params\":{}}\n");
        let _ = self.stdin.flush();
        // Give the child a beat to exit on its own, then make sure.
        thread::sleep(Duration::from_millis(100));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ServeAdapter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
