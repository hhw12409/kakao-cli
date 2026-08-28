//! CLI surface. Mirrors `docs/command-spec.md`.

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "kakao-cli",
    about = "카카오톡 텍스트 채팅을 터미널에서 처리하는 CLI",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 안 읽은 방 우선으로 목록 확인
    Inbox(InboxArgs),
    /// 채팅방 목록 / 이름 검색
    Rooms(RoomsArgs),
    /// 해당 방의 최근 텍스트 대화 출력
    Open(OpenArgs),
    /// 텍스트 메시지 전송
    Send(SendArgs),
    /// 로컬 캐시에서 메시지 검색
    Search(SearchArgs),
    /// 카카오톡 실행 상태와 자동화 권한 진단
    Doctor,
    /// 방 별칭 관리
    #[command(subcommand)]
    Alias(AliasCommand),
    /// 로컬 캐시 관리
    #[command(subcommand)]
    Cache(CacheCommand),
}

#[derive(Debug, Args)]
pub struct InboxArgs {
    /// 사람이 읽는 출력 대신 한 줄 JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RoomsArgs {
    /// 방 이름 검색어 (생략 시 전체 목록)
    pub query: Option<String>,
    /// 제목 완전 일치로 제한
    #[arg(long)]
    pub exact: bool,
    /// 사람이 읽는 출력 대신 한 줄 JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct OpenArgs {
    /// 방 검색어 또는 @별칭
    pub room: String,
    /// 제목 완전 일치로 검색 제한
    #[arg(long)]
    pub exact: bool,
    /// 가져올 최근 메시지 수
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
    /// 사람이 읽는 출력 대신 한 줄 JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SendArgs {
    /// 방 검색어 또는 @별칭
    pub room: String,
    /// 메시지 본문. 생략 시 --stdin 또는 편집기 모드
    pub message: Option<String>,
    /// 표준 입력에서 본문을 읽는다 (줄바꿈 보존, 빈 입력 거부)
    #[arg(long)]
    pub stdin: bool,
    /// 제목 완전 일치로 검색 제한
    #[arg(long)]
    pub exact: bool,
    /// 대화형 확인을 건너뛴다 (동명 방이 여럿이면 그래도 전송 안 함)
    #[arg(long)]
    pub yes: bool,
    /// 대상·메시지만 출력. sendText 미호출
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    /// 메시지 길이 상한
    #[arg(long = "max-chars", default_value_t = 2000)]
    pub max_chars: usize,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// FTS5 검색어
    pub query: String,
    /// 특정 방으로 한정 (이름 해석 거침)
    #[arg(long)]
    pub room: Option<String>,
    /// 사람이 읽는 출력 대신 한 줄 JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum AliasCommand {
    /// 별칭 추가
    Add { name: String, room_query: String },
    /// 별칭 목록
    List,
    /// 별칭 삭제
    Remove { name: String },
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// 방 목록·메시지 캐시 삭제 (별칭·전송 기록은 유지)
    Clear {
        #[arg(long)]
        yes: bool,
    },
}
