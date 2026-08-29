//! KakaoTalk's list and message views expose time as Korean locale labels
//! (`"오전 11:17"`, `"오후 12:12"`, `"어제"`), never ISO. Best-effort conversion
//! to an ISO 8601 UTC string; `""` when a label carries no usable time.
//!
//! Mirrors the macOS adapter's `KoreanTime` — the two adapters must produce
//! identical `at` values for identical labels.
//!
//! NOTE: KakaoTalk Windows locale strings need verification against a real
//! build via `--dump-tree`.

use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, Time, UtcOffset};

fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
}

fn iso_utc(dt: OffsetDateTime) -> String {
    dt.to_offset(UtcOffset::UTC)
        .replace_nanosecond(0)
        .unwrap_or(dt)
        .format(&Rfc3339)
        .unwrap_or_default()
}

/// Parse "오전 11:17" / "오후 12:12" (or "1\n오전 1:01") -> 24h (hour, minute).
pub fn parse_hour_minute(raw: &str) -> Option<(u8, u8)> {
    let line = raw.lines().last().unwrap_or(raw).trim();
    let (ampm, rest) = if let Some(r) = line.strip_prefix("오전") {
        (0u8, r)
    } else {
        let r = line.strip_prefix("오후")?;
        (12u8, r)
    };
    let (h, m) = rest.trim().split_once(':')?;
    let mut h: u8 = h.trim().parse().ok()?;
    let m: u8 = m.trim().parse().ok()?;
    if h == 12 {
        h = 0;
    }
    if h + ampm > 23 || m > 59 {
        return None;
    }
    Some((h + ampm, m))
}

/// `label` -> ISO 8601 UTC string, or `""`. `now` is injectable for tests.
pub fn to_iso(label: &str, now: OffsetDateTime) -> String {
    let line = label.lines().last().unwrap_or(label).trim();
    if line.is_empty() {
        return String::new();
    }
    let local_now = now.to_offset(local_offset());

    if let Some((h, m)) = parse_hour_minute(line) {
        let local = local_now.replace_time(Time::from_hms(h, m, 0).unwrap_or(Time::MIDNIGHT));
        return iso_utc(local);
    }

    if line == "어제" {
        let y = (local_now - Duration::days(1)).replace_time(Time::MIDNIGHT);
        return iso_utc(y);
    }

    // "YYYY년 M월 D일"
    let nums: Vec<i64> = line
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() == 3 {
        let month = u8::try_from(nums[1])
            .ok()
            .and_then(|m| time::Month::try_from(m).ok());
        let day = u8::try_from(nums[2]).ok();
        if let (Some(month), Some(day)) = (month, day) {
            if let Ok(date) = time::Date::from_calendar_date(nums[0] as i32, month, day) {
                return iso_utc(date.with_time(Time::MIDNIGHT).assume_offset(local_offset()));
            }
        }
    }

    String::new()
}

/// Current time (used by the live bridge).
pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}
