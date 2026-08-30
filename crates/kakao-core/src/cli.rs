//! CLI surface. Mirrors `docs/command-spec.md`.
//!
//! kakao-cli is an interactive terminal chat client. Running it with no
//! subcommand launches the TUI; the only subcommand is `doctor`.

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "kakao-cli",
    about = "카카오톡을 터미널에서 쓰는 대화형 채팅 클라이언트",
    long_about = "카카오톡 데스크톱 앱을 접근성 API로 조작하는 개인용 터미널 채팅 클라이언트.\n\
                  인자 없이 실행하면 채팅 화면이 열립니다.  진단은 `kakao-cli doctor`.",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 카카오톡 실행 상태와 자동화 권한 진단
    Doctor,
}
