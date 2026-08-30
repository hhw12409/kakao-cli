//! kakao-cli common core.
//!
//! Everything OS-independent: the interactive chat [`tui`], CLI parsing,
//! room-name resolution, aliases, the SQLite scrollback cache, the `send`
//! safety policy and its `pending -> sent | failed | unknown` state machine,
//! and the dispatch layer that speaks to an OS adapter (a long-lived
//! `kakao-<os>-bridge serve` process, or a one-shot subprocess for `doctor`).
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
pub mod tui;

pub use error::{AppError, AppResult};
