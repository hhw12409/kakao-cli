//! Error type for the whole CLI. Every user-visible failure carries an exit
//! code (see `docs/command-spec.md`) and a message that never contains a
//! message body.

use kakao_contract::{ErrorCode, Method};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A contract error code surfaced from an adapter (or synthesised by the
    /// core for the same condition, e.g. empty message).
    #[error("{code}")]
    Adapter {
        code: ErrorCode,
        /// Optional extra context for stderr diagnostics. No message bodies.
        detail: Option<String>,
    },

    /// Room name matched nothing.
    #[error("room not found")]
    RoomNotFound { query: String, near: Vec<String> },

    /// Room name matched several rooms and we cannot prompt (non-interactive
    /// or `--yes`). Exit code 5.
    #[error("room ambiguous")]
    RoomAmbiguous { candidates: Vec<String> },

    /// Send verification could not confirm delivery. Exit code 6. No retry.
    #[error("send unknown")]
    SendUnknown,

    /// The user aborted an interactive prompt.
    #[error("aborted")]
    Aborted,

    /// `alias add` with a name that already exists. Exit code 9.
    #[error("alias conflict")]
    AliasConflict { name: String },

    /// First-run onboarding fallback: environment is not set up. Carries the
    /// most relevant contract code for the exit status.
    #[error("not set up")]
    Onboarding { code: ErrorCode, rendered: String },

    /// Adapter subprocess exceeded its IPC timeout. The dispatch layer converts
    /// this: `send_text` -> `unknown`, everything else -> `UI_ELEMENT_NOT_FOUND`.
    #[error("adapter timeout")]
    Timeout(Method),

    /// Adapter broke the contract, IPC crashed, DB failure, etc. Exit code 1.
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn adapter(code: ErrorCode) -> Self {
        AppError::Adapter { code, detail: None }
    }

    pub fn adapter_detail(code: ErrorCode, detail: impl Into<String>) -> Self {
        AppError::Adapter { code, detail: Some(detail.into()) }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        AppError::Internal(msg.into())
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::Adapter { code, .. } => code.exit_code(),
            AppError::RoomNotFound { .. } => 2,
            AppError::RoomAmbiguous { .. } => 5,
            AppError::SendUnknown => 6,
            AppError::Aborted => 0,
            AppError::AliasConflict { .. } => 9,
            AppError::Onboarding { code, .. } => code.exit_code(),
            AppError::Timeout(Method::SendText) => ErrorCode::SendVerifyTimeout.exit_code(),
            AppError::Timeout(_) => ErrorCode::UiElementNotFound.exit_code(),
            AppError::Internal(_) => 1,
        }
    }

    /// One-line (or short block) message for stderr. Never a stack trace, never
    /// a message body.
    pub fn user_message(&self) -> String {
        match self {
            AppError::Adapter { code, .. } => crate::render::error_message(*code),
            AppError::RoomNotFound { query, .. } => {
                format!("'{query}' 와(과) 일치하는 방이 없습니다.")
            }
            AppError::RoomAmbiguous { candidates } => {
                let mut s = String::from("여러 채팅방이 일치합니다. --exact 로 제목을 특정하거나 대화형으로 선택하세요.\n");
                for (i, c) in candidates.iter().enumerate() {
                    s.push_str(&format!("  {}. {}\n", i + 1, c));
                }
                s.trim_end().to_string()
            }
            AppError::SendUnknown => {
                "전송 여부를 확인할 수 없습니다. 카카오톡에서 직접 확인하세요.".to_string()
            }
            AppError::Aborted => "취소했습니다.".to_string(),
            AppError::AliasConflict { name } => {
                format!("별칭 '{name}' 은(는) 이미 있습니다. 먼저 alias remove 하세요.")
            }
            AppError::Onboarding { rendered, .. } => rendered.clone(),
            AppError::Timeout(Method::SendText) => {
                "전송 여부를 확인할 수 없습니다. 카카오톡에서 직접 확인하세요.".to_string()
            }
            AppError::Timeout(_) => crate::render::error_message(ErrorCode::UiElementNotFound),
            AppError::Internal(msg) => format!("내부 오류: {msg}"),
        }
    }

    /// Extra "do this next" line, when there is one.
    pub fn recovery_hint(&self) -> Option<String> {
        match self {
            AppError::Adapter { code, .. } => crate::render::recovery_hint(*code),
            AppError::RoomNotFound { near, .. } if !near.is_empty() => {
                Some(format!("가까운 이름: {}", near.join(", ")))
            }
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Internal(format!("로컬 DB: {e}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(format!("터미널 입출력: {e}"))
    }
}
