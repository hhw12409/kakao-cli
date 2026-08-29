//! kakao-windows-bridge — subprocess adapter for kakao-cli.
//!
//! Invocation (contract §1):  kakao-windows-bridge <method> <argsJson>
//!   methods: listRooms | openRoom | readRecent | sendText | healthCheck
//!
//! Output: exactly one line of JSON on stdout:
//!   {"ok":true,"data":<...>}   or   {"ok":false,"error":"<CODE>"}

use kakao_windows_bridge::{envelope, self_test};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--self-test") => self_test::run(),
        Some("--dump-tree") => dump_tree(),
        Some(method) => dispatch(method, args.get(1).map(String::as_str).unwrap_or("{}")),
        None => envelope::crash("missing method argument"),
    }
}

#[cfg(windows)]
fn dump_tree() -> ! {
    kakao_windows_bridge::dump_tree::run()
}

#[cfg(not(windows))]
fn dump_tree() -> ! {
    envelope::crash("--dump-tree is only available on Windows")
}

#[cfg(windows)]
fn dispatch(method: &str, args_json: &str) -> ! {
    kakao_windows_bridge::bridge::dispatch(method, args_json)
}

#[cfg(not(windows))]
fn dispatch(method: &str, _args_json: &str) -> ! {
    // The parsers and self-test build everywhere; the live UIA calls do not.
    envelope::crash(&format!(
        "method '{method}' needs the Windows UI Automation runtime (build target x86_64-pc-windows-msvc)"
    ))
}
