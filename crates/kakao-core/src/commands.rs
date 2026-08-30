//! Command dispatch. With no subcommand kakao-cli launches the chat TUI; the
//! only subcommand is `doctor`.

use kakao_contract::ErrorCode;

use crate::adapter;
use crate::cli::{Cli, Command};
use crate::error::{AppError, AppResult};
use crate::render;

pub fn run(cli: Cli) -> AppResult<()> {
    match cli.command {
        None => crate::tui::run(),
        Some(Command::Doctor) => doctor(),
    }
}

fn doctor() -> AppResult<()> {
    let adapter = adapter::for_current_env().map_err(onboarding_from_internal)?;
    let health = adapter.health_check()?;
    println!("{}", render::render_doctor(&health));

    if health
        .issues
        .iter()
        .any(|i| i.code == ErrorCode::KakaoNotRunning)
    {
        return Err(AppError::adapter(ErrorCode::KakaoNotRunning));
    }
    if health
        .issues
        .iter()
        .any(|i| i.code == ErrorCode::AccessibilityPermissionDenied)
    {
        return Err(AppError::adapter(ErrorCode::AccessibilityPermissionDenied));
    }
    Ok(())
}

fn onboarding_from_internal(e: AppError) -> AppError {
    match e {
        AppError::Internal(msg) => AppError::Onboarding {
            code: ErrorCode::UiElementNotFound,
            rendered: format!(
                "kakao-cli 를 사용하려면 먼저 설정이 필요합니다.\n\n{msg}\n\n자세히:  kakao-cli doctor"
            ),
        },
        other => other,
    }
}
