//! Timestamp helpers. Storage and the contract use ISO 8601 UTC; user output
//! uses the local timezone (`docs/command-spec.md`).

use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

/// Current time as an ISO 8601 UTC string (e.g. `2026-08-29T05:43:12Z`).
pub fn now_utc_iso() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .format(&Rfc3339)
        .unwrap_or_default()
}

/// Parse an ISO 8601 timestamp from the contract/DB.
pub fn parse_iso(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).ok()
}

/// Local wall-clock offset, falling back to UTC when the process cannot
/// determine it (common in multi-threaded contexts).
fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
}

/// `HH:MM` in local time, for the `✓ ...에 전송됨  14:32` line.
pub fn local_hm(dt: OffsetDateTime) -> String {
    let local = dt.to_offset(local_offset());
    format!("{:02}:{:02}", local.hour(), local.minute())
}

/// Human "n분 전 / n시간 전 / n일 전", relative to now.
pub fn relative_ko(dt: OffsetDateTime) -> String {
    let delta = OffsetDateTime::now_utc() - dt;
    let secs = delta.whole_seconds().max(0);
    match secs {
        0..=59 => "방금".to_string(),
        60..=3599 => format!("{}분 전", secs / 60),
        3600..=86399 => format!("{}시간 전", secs / 3600),
        _ => format!("{}일 전", secs / 86400),
    }
}

/// `YYYY-MM-DD HH:MM` local, for search results.
pub fn local_datetime(dt: OffsetDateTime) -> String {
    let l = dt.to_offset(local_offset());
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        l.year(),
        u8::from(l.month()),
        l.day(),
        l.hour(),
        l.minute()
    )
}
