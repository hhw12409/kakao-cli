//! Adapter dispatch layer.
//!
//! The core calls exactly one `Adapter`. It does not know whether the call is
//! a subprocess (`SubprocessAdapter`) or an in-process mock (`MockAdapter`,
//! test/`KAKAO_CLI_MOCK` only). Both return shapes validated against
//! `docs/adapter-contract.md`.

mod mock;
mod subprocess;

pub use mock::MockAdapter;
pub use subprocess::SubprocessAdapter;

use kakao_contract::{Health, ListRoomsData, ReadRecentData, SendResult};

use crate::config;
use crate::error::AppResult;

/// The five contract functions. Errors are already mapped to `AppError`
/// (contract error codes -> `AppError::Adapter`).
pub trait Adapter {
    fn list_rooms(&self) -> AppResult<ListRoomsData>;
    fn open_room(&self, room_id: &str) -> AppResult<()>;
    fn read_recent(&self, room_id: &str, limit: u32) -> AppResult<ReadRecentData>;
    fn send_text(&self, room_id: &str, text: &str) -> AppResult<SendResult>;
    fn health_check(&self) -> AppResult<Health>;
}

/// Pick the adapter for this run: mock when `KAKAO_CLI_MOCK` is set, otherwise
/// the OS bridge subprocess.
pub fn for_current_env() -> AppResult<Box<dyn Adapter>> {
    if let Some(fixture) = config::mock_fixture_path() {
        return Ok(Box::new(MockAdapter::from_fixture_file(&fixture)?));
    }
    Ok(Box::new(SubprocessAdapter::new(config::bridge_path()?)))
}
