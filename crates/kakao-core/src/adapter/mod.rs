//! Adapter dispatch layer.
//!
//! Two transports, both returning shapes validated against
//! `docs/adapter-contract.md`:
//!
//! * [`StreamAdapter`] — the live `kakao-<os>-bridge serve` session the
//!   interactive TUI drives ([`ServeAdapter`]), or an in-process
//!   [`MockStreamAdapter`] for tests / `KAKAO_CLI_STREAM_MOCK`.
//! * [`Adapter`] — a one-shot subprocess ([`SubprocessAdapter`]) or in-process
//!   [`MockAdapter`]. Used only by `doctor` (a single `healthCheck`).

mod mock;
mod mock_stream;
mod serve;
mod stream;
mod subprocess;

pub use mock::MockAdapter;
pub use mock_stream::{MockAvailability, MockStreamAdapter};
pub use serve::ServeAdapter;
pub use stream::{StreamAdapter, StreamEvent};
pub use subprocess::SubprocessAdapter;

use kakao_contract::{Health, ListRoomsData, ReadRecentData, SendResult};

use crate::config;
use crate::error::AppResult;

/// The five one-shot contract functions. Errors are already mapped to
/// `AppError` (contract error codes -> `AppError::Adapter`).
pub trait Adapter {
    fn list_rooms(&self) -> AppResult<ListRoomsData>;
    fn open_room(&self, room_id: &str) -> AppResult<()>;
    fn read_recent(&self, room_id: &str, limit: u32) -> AppResult<ReadRecentData>;
    fn send_text(&self, room_id: &str, text: &str) -> AppResult<SendResult>;
    fn health_check(&self) -> AppResult<Health>;
}

/// One-shot adapter for `doctor`: mock when `KAKAO_CLI_MOCK` is set, otherwise
/// the OS bridge subprocess.
pub fn for_current_env() -> AppResult<Box<dyn Adapter>> {
    if let Some(fixture) = config::mock_fixture_path() {
        return Ok(Box::new(MockAdapter::from_fixture_file(&fixture)?));
    }
    Ok(Box::new(SubprocessAdapter::new(config::bridge_path()?)))
}

/// Streaming adapter for the TUI: in-process mock when `KAKAO_CLI_STREAM_MOCK`
/// is set, otherwise a spawned `kakao-<os>-bridge serve`.
pub fn stream_for_current_env() -> AppResult<Box<dyn StreamAdapter>> {
    if let Some(fixture) = config::mock_stream_fixture_path() {
        return Ok(Box::new(MockStreamAdapter::from_fixture_file(&fixture)?));
    }
    Ok(Box::new(ServeAdapter::spawn(config::bridge_path()?)?))
}
