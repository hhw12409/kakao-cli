//! The Windows adapter's parsers must produce the same result as the macOS
//! adapter for the shared `scenario-basic` fixture. These run on any host
//! (the live UIA layer is Windows-only, the parsers are not).

use kakao_windows_bridge::self_test;

#[test]
fn scenario_basic_parses_identically_to_macos() {
    let failures = self_test::run_checks(|label, ok| {
        if !ok {
            eprintln!("FAILED: {label}");
        }
    });
    assert_eq!(failures, 0, "{failures} parser check(s) failed");
}
