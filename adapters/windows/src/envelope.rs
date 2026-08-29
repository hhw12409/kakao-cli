//! Writes the single-line JSON response and exits. Contract §1 — identical
//! wire format to the macOS bridge.

use std::io::Write;
use std::process::exit;

use kakao_contract::{AdapterResponse, ErrorCode};
use serde::Serialize;

/// A bridge function failure carrying a contract error code. `diagnostic` is
/// stderr-only and never a message body.
#[derive(Debug)]
pub struct BridgeError {
    pub code: ErrorCode,
    pub diagnostic: Option<String>,
}

impl BridgeError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            diagnostic: None,
        }
    }
    pub fn with(code: ErrorCode, diagnostic: impl Into<String>) -> Self {
        Self {
            code,
            diagnostic: Some(diagnostic.into()),
        }
    }
}

pub type BridgeResult<T> = Result<T, BridgeError>;

fn emit(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(line.as_bytes());
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

pub fn ok<T: Serialize>(data: T) -> ! {
    let value = serde_json::to_value(data).unwrap_or(serde_json::Value::Null);
    let resp = AdapterResponse::ok(value);
    emit(&serde_json::to_string(&resp).unwrap_or_default());
    exit(0)
}

pub fn ok_empty() -> ! {
    ok(serde_json::json!({}))
}

pub fn error(code: ErrorCode) -> ! {
    let resp = AdapterResponse::err(code);
    emit(&serde_json::to_string(&resp).unwrap_or_default());
    exit(0)
}

/// The bridge itself failed (bad args, unknown method, unexpected error).
/// Non-zero exit so the core promotes it to an internal error.
pub fn crash(message: &str) -> ! {
    eprintln!("bridge crash: {message}");
    exit(70)
}

pub fn finish(result: BridgeResult<serde_json::Value>, method: &str) -> ! {
    match result {
        Ok(data) => ok(data),
        Err(e) => {
            if let Some(d) = &e.diagnostic {
                eprintln!("{method}: {d}");
            }
            error(e.code)
        }
    }
}
