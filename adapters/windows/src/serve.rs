//! Serve mode — `kakao-windows-bridge serve`. Contract §5: a long-lived
//! process reading newline-delimited [`ServeRequest`]s from stdin and writing
//! [`ServeResponse`]s / [`ServeEvent`]s to stdout.
//!
//! The framing ([`ParsedRequest`], [`write_line`]) type-checks and tests on any
//! host so it stays in lockstep with the macOS bridge. The request loop that
//! actually drives KakaoTalk is Windows-only.

use std::io::Write;

use kakao_contract::{ServeRequest, ServeResponse};
use serde::Serialize;

/// A decoded request line.
pub struct ParsedRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

/// Parse one stdin line. `None` for blank lines or unparseable JSON.
pub fn parse_request(line: &str) -> Option<ParsedRequest> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let req: ServeRequest = serde_json::from_str(line).ok()?;
    Some(ParsedRequest {
        id: req.id,
        method: req.method,
        params: req.params,
    })
}

/// Write one framed line to stdout (a response or an event).
pub fn write_line<T: Serialize>(value: &T) {
    if let Ok(s) = serde_json::to_string(value) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(s.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }
}

/// Convenience: emit a `ServeResponse` for `id`.
pub fn respond_ok<T: Serialize>(id: u64, data: &T) {
    let value = serde_json::to_value(data).unwrap_or(serde_json::Value::Null);
    write_line(&ServeResponse::ok(id, value));
}

pub fn respond_err(id: u64, error: kakao_contract::ErrorCode) {
    write_line(&ServeResponse::err(id, error));
}

#[cfg(windows)]
pub use windows_impl::run;

#[cfg(not(windows))]
pub fn run() -> ! {
    crate::envelope::crash("serve mode needs the Windows UI Automation runtime")
}

#[cfg(windows)]
mod windows_impl {
    use super::{parse_request, respond_err, respond_ok, write_line};
    use kakao_contract::{ErrorCode, ServeEvent};
    use std::io::BufRead;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use crate::bridge;

    /// Read requests until stdin closes. `watch` starts a background poller that
    /// emits `message` events; every other method is a one-shot round-trip.
    pub fn run() -> ! {
        let stdin = std::io::stdin();
        let watch_stop = Arc::new(AtomicBool::new(false));
        let mut watch_handle: Option<std::thread::JoinHandle<()>> = None;

        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let Some(req) = parse_request(&line) else { continue };

            match req.method.as_str() {
                "shutdown" => break,
                "unwatch" => {
                    watch_stop.store(true, Ordering::SeqCst);
                    if let Some(h) = watch_handle.take() {
                        let _ = h.join();
                    }
                    respond_ok(req.id, &serde_json::json!({}));
                }
                "watch" => {
                    watch_stop.store(true, Ordering::SeqCst);
                    if let Some(h) = watch_handle.take() {
                        let _ = h.join();
                    }
                    let Some(room_id) = req.params.get("roomId").and_then(|v| v.as_str()) else {
                        respond_err(req.id, ErrorCode::RoomNotFound);
                        continue;
                    };
                    let room_id = room_id.to_string();
                    watch_stop.store(false, Ordering::SeqCst);
                    respond_ok(req.id, &serde_json::json!({}));
                    let stop = watch_stop.clone();
                    watch_handle = Some(std::thread::spawn(move || {
                        watch_loop(room_id, stop);
                    }));
                }
                other => one_shot(req.id, other, &req.params),
            }
        }

        watch_stop.store(true, Ordering::SeqCst);
        if let Some(h) = watch_handle.take() {
            let _ = h.join();
        }
        std::process::exit(0)
    }

    fn one_shot(id: u64, method: &str, params: &serde_json::Value) {
        match bridge::serve_call(method, params) {
            Ok(value) => write_line(&kakao_contract::ServeResponse::ok(id, value)),
            Err(code) => respond_err(id, code),
        }
    }

    fn watch_loop(room_id: String, stop: Arc<AtomicBool>) {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut seeded = false;
        let mut misses = 0u32;
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match bridge::watch_read(&room_id) {
                Ok(messages) => {
                    misses = 0;
                    let key = |m: &kakao_contract::Message| {
                        format!("{}\u{1}{}\u{1}{}\u{1}{}", m.at, m.sender, m.outgoing, m.text)
                    };
                    if !seeded {
                        seen = messages.iter().map(key).collect();
                        seeded = true;
                        continue;
                    }
                    for m in &messages {
                        if seen.insert(key(m)) {
                            write_line(&ServeEvent::Message {
                                room_id: room_id.clone(),
                                message: m.clone(),
                            });
                        }
                    }
                }
                Err(code) => {
                    misses += 1;
                    if code == ErrorCode::UiElementNotFound && misses >= 2 {
                        write_line(&ServeEvent::RoomClosed {
                            room_id: room_id.clone(),
                        });
                    } else {
                        write_line(&ServeEvent::Error { code });
                    }
                }
            }
        }
    }
}
