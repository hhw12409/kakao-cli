//! kakao-cli Windows adapter.
//!
//! Drives the KakaoTalk Windows app via UI Automation (windows-rs) and
//! implements the five contract functions (`docs/adapter-contract.md`),
//! producing byte-identical output and error codes to the macOS adapter.
//!
//! Host-independent modules (`node`, `korean_time`, `selectors`, `parsers`,
//! `envelope`, `self_test`) type-check and test on any platform. The live UIA
//! layer (`uia`, `bridge`, `kakao_app`, `dump_tree`) is Windows-only.

pub mod envelope;
pub mod korean_time;
pub mod node;
pub mod parsers;
pub mod selectors;
pub mod self_test;

#[cfg(windows)]
pub mod bridge;
#[cfg(windows)]
pub mod dump_tree;
#[cfg(windows)]
pub mod kakao_app;
#[cfg(windows)]
pub mod uia;
