//! kakao-cli common core.
//!
//! Everything OS-independent: CLI parsing, output rendering, room-name
//! resolution, aliases, the SQLite cache + FTS search, the `send` safety
//! policy and its `pending -> sent | failed | unknown` state machine, and the
//! dispatch layer that calls an OS adapter as a subprocess.
//!
//! The core never touches the KakaoTalk UI. It assumes the adapter delivers
//! data shaped per `docs/adapter-contract.md` and validates that shape at
//! runtime.

pub mod adapter;
pub mod cli;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod render;
pub mod resolve;
pub mod send;
pub mod time_util;

pub use error::{AppError, AppResult};
