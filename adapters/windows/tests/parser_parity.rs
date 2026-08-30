//! The Windows adapter's parsers must produce the same result as the macOS
//! adapter for the shared `scenario-basic` fixture. These run on any host
//! (the live UIA layer is Windows-only, the parsers are not).

use kakao_windows_bridge::self_test;
use kakao_windows_bridge::serve;

#[test]
fn scenario_basic_parses_identically_to_macos() {
    let failures = self_test::run_checks(|label, ok| {
        if !ok {
            eprintln!("FAILED: {label}");
        }
    });
    assert_eq!(failures, 0, "{failures} parser check(s) failed");
}

#[test]
fn serve_request_parsing_matches_contract_shape() {
    let req = serve::parse_request(
        r#"{"id":7,"method":"readRecent","params":{"roomId":"row:3","limit":40}}"#,
    )
    .expect("valid request");
    assert_eq!(req.id, 7);
    assert_eq!(req.method, "readRecent");
    assert_eq!(req.params["roomId"], "row:3");

    assert!(serve::parse_request("   ").is_none());
    assert!(serve::parse_request("not json").is_none());
}
