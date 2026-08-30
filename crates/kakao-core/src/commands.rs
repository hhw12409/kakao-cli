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
    // `render_doctor` already prints each issue with its recovery block, so
    // exit straight from here rather than returning an Err (which would make
    // `main` print the same message a second time).
    println!("{}", render::render_doctor(&health));

    let blocking = health
        .issues
        .iter()
        .map(|i| i.code)
        .find(|c| {
            matches!(
                c,
                ErrorCode::KakaoNotRunning
                    | ErrorCode::KakaoWindowNotVisible
                    | ErrorCode::AccessibilityPermissionDenied
                    | ErrorCode::AppVersionUnsupported
            )
        });
    match blocking {
        Some(code) => std::process::exit(code.exit_code()),
        None => Ok(()),
    }
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
