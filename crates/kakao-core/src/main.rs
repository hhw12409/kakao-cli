use std::process::ExitCode;

use clap::Parser;
use kakao_core::cli::Cli;
use kakao_core::error::AppError;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match kakao_core::commands::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // User-facing message goes to stderr. Never contains message bodies.
            eprintln!("{}", err.user_message());
            if let Some(hint) = err.recovery_hint() {
                eprintln!();
                eprintln!("{hint}");
            }
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

// Keep the `AppError` symbol referenced from the binary so `use` stays honest
// even when the match above is edited.
#[allow(dead_code)]
fn _assert_error_type(e: AppError) -> i32 {
    e.exit_code()
}
