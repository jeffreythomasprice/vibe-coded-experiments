//! The one place this crate formats a timestamp. Every stored or reported
//! timestamp in this codebase — `turns.started_at`, a log line, a tool
//! decision's `decided_at` — uses this exact shape, so they all sort
//! together lexicographically.

use time::macros::format_description;

/// ISO8601 UTC, millisecond precision — e.g. `"2026-08-22T00:47:03.955Z"`.
pub fn now_iso8601() -> String {
    let format =
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");
    time::OffsetDateTime::now_utc()
        .format(&format)
        .expect("a static ISO8601 format description never fails to apply")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_iso8601_has_the_expected_shape() {
        let ts = now_iso8601();
        // "2026-08-22T00:47:03.955Z" — 24 characters, fixed width.
        assert_eq!(ts.len(), 24, "timestamp was {ts:?}");
        assert!(ts.ends_with('Z'), "timestamp was {ts:?}");
        assert_eq!(ts.chars().nth(4), Some('-'));
        assert_eq!(ts.chars().nth(10), Some('T'));
    }
}
