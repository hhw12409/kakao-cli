//! Subprocess adapter: runs `kakao-<os>-bridge <method> <argsJson>` and reads
//! one line of JSON from stdout. Contract: `docs/adapter-contract.md` §1.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use kakao_contract::{
    AdapterResponse, ErrorCode, Health, ListRoomsData, Method, ReadRecentData, SendResult,
    SendStatus,
};
use serde_json::json;

use crate::adapter::Adapter;
use crate::error::{AppError, AppResult};

pub struct SubprocessAdapter {
    bin: PathBuf,
}

impl SubprocessAdapter {
    pub fn new(bin: PathBuf) -> Self {
        Self { bin }
    }

    /// Run one method. Returns the `data` value on success, maps a contract
    /// error code to `AppError::Adapter`, and treats anything else (non-zero
    /// exit, unparseable output, timeout) as an internal error — except for
    /// `send_text`, where a timeout is `unknown`, handled by the caller.
    fn call(&self, method: Method, args: serde_json::Value) -> AppResult<serde_json::Value> {
        let args_str = serde_json::to_string(&args)
            .map_err(|e| AppError::internal(format!("요청 직렬화 실패: {e}")))?;

        let mut child = Command::new(&self.bin)
            .arg(method.wire_name())
            .arg(&args_str)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                AppError::internal(format!(
                    "어댑터 실행 실패 ({}): {e}",
                    self.bin.display()
                ))
            })?;

        let mut stdout = child.stdout.take().expect("piped");
        let mut stderr = child.stderr.take().expect("piped");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut out = String::new();
            let _ = stdout.read_to_string(&mut out);
            let _ = tx.send(out);
        });

        let timeout = Duration::from_millis(method.timeout_ms());
        let out = match rx.recv_timeout(timeout) {
            Ok(out) => out,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Timeout(method));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::internal("어댑터 출력 스트림이 끊겼습니다"));
            }
        };

        let status = child
            .wait()
            .map_err(|e| AppError::internal(format!("어댑터 종료 대기 실패: {e}")))?;

        let mut err_log = String::new();
        let _ = stderr.read_to_string(&mut err_log);
        let last_diag = err_log.lines().last().unwrap_or("").trim().to_string();

        if !status.success() {
            return Err(AppError::internal(format!(
                "어댑터가 비정상 종료했습니다 (code {:?}). 진단: {last_diag}",
                status.code()
            )));
        }

        let line = out.lines().next().unwrap_or("").trim();
        if line.is_empty() {
            return Err(AppError::internal("어댑터가 빈 응답을 반환했습니다"));
        }

        let resp: AdapterResponse = serde_json::from_str(line).map_err(|e| {
            AppError::internal(format!("어댑터 응답을 파싱할 수 없습니다: {e}"))
        })?;

        match resp {
            AdapterResponse::Ok { data, .. } => Ok(data),
            AdapterResponse::Err { error, .. } => Err(AppError::adapter(error)),
        }
    }
}

/// Runtime-validate a `data` value against the expected shape and fail loudly
/// on a contract violation rather than casting past it.
fn parse_data<T: serde::de::DeserializeOwned>(
    method: Method,
    data: serde_json::Value,
) -> AppResult<T> {
    serde_json::from_value(data).map_err(|e| {
        AppError::internal(format!(
            "어댑터 계약 위반 ({}): {e}",
            method.wire_name()
        ))
    })
}

impl Adapter for SubprocessAdapter {
    fn list_rooms(&self) -> AppResult<ListRoomsData> {
        let data = self.call(Method::ListRooms, json!({}))?;
        parse_data(Method::ListRooms, data)
    }

    fn open_room(&self, room_id: &str) -> AppResult<()> {
        // Success data is `{}`; a failure is already an `AppError::Adapter`.
        self.call(Method::OpenRoom, json!({ "roomId": room_id }))?;
        Ok(())
    }

    fn read_recent(&self, room_id: &str, limit: u32) -> AppResult<ReadRecentData> {
        let data = self.call(
            Method::ReadRecent,
            json!({ "roomId": room_id, "limit": limit }),
        )?;
        parse_data(Method::ReadRecent, data)
    }

    fn send_text(&self, room_id: &str, text: &str) -> AppResult<SendResult> {
        match self.call(Method::SendText, json!({ "roomId": room_id, "text": text })) {
            Ok(data) => parse_data(Method::SendText, data),
            // IPC timeout on send is `unknown`, never a retry (contract §1, §3).
            Err(AppError::Timeout(Method::SendText)) => Ok(SendResult {
                status: SendStatus::Unknown,
                at: None,
                error: Some(ErrorCode::SendVerifyTimeout),
            }),
            Err(other) => Err(other),
        }
    }

    fn health_check(&self) -> AppResult<Health> {
        let data = self.call(Method::HealthCheck, json!({}))?;
        parse_data(Method::HealthCheck, data)
    }
}
