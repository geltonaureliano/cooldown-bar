//! Defensive JSON helpers.
//!
//! Provider payloads have moved between versions and will move again. Nothing in
//! here panics: a shape we do not recognise yields `None`, and the caller turns
//! that into a visible error rather than a confident wrong number.

use serde_json::Value;

/// Field names seen carrying a 0-100 utilisation figure.
const PERCENT_KEYS: &[&str] = &[
    "used_percentage",
    "used_percent",
    "usedPercent",
    "utilization",
    "utilisation",
    "percent",
    "percent_used",
];

/// Field names seen carrying an absolute reset instant.
const RESET_KEYS: &[&str] = &["resets_at", "resetsAt", "reset_at", "resetAt"];

/// Field names seen carrying a reset expressed as an offset from *now*.
const RESET_IN_KEYS: &[&str] = &[
    "resets_in_seconds",
    "resetsInSeconds",
    "reset_in_seconds",
    "seconds_until_reset",
];

/// Field names seen carrying a window length in minutes.
const WINDOW_MIN_KEYS: &[&str] = &["window_minutes", "windowDurationMins", "windowMinutes"];

/// Depth-first search for the first value under any key named `name`.
#[allow(dead_code)] // kept as the un-filtered counterpart of `find_key_non_null`
pub fn find_key<'a>(root: &'a Value, name: &str) -> Option<&'a Value> {
    match root {
        Value::Object(map) => {
            if let Some(v) = map.get(name) {
                return Some(v);
            }
            for v in map.values() {
                if let Some(found) = find_key(v, name) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|v| find_key(v, name)),
        _ => None,
    }
}

/// Like [`find_key`], but skips values that are `null` so a `"rate_limits": null`
/// (which Codex emits for `codex exec` runs) does not shadow a real one deeper in
/// the document.
pub fn find_key_non_null<'a>(root: &'a Value, name: &str) -> Option<&'a Value> {
    match root {
        Value::Object(map) => {
            if let Some(v) = map.get(name) {
                if !v.is_null() {
                    return Some(v);
                }
            }
            for v in map.values() {
                if let Some(found) = find_key_non_null(v, name) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|v| find_key_non_null(v, name)),
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Read a 0-100 percentage from an object, trying every spelling we have seen.
///
/// These fields are percentages, including values below 1. Never guess units
/// from magnitude: 1% is not 100%. Non-finite values are rejected.
pub fn percent_of(obj: &Value) -> Option<f64> {
    let map = obj.as_object()?;
    for key in PERCENT_KEYS {
        if let Some(raw) = map.get(*key).and_then(as_f64) {
            if !raw.is_finite() {
                continue;
            }
            return Some(raw.clamp(0.0, 100.0));
        }
    }
    None
}

/// Read an absolute reset instant as unix seconds.
///
/// Accepts unix seconds, unix milliseconds, and RFC 3339 / ISO-8601 strings.
/// Also accepts a relative `resets_in_seconds`, in which case `now_base` is the
/// instant the offset is relative to — for a log line that is the line's own
/// timestamp, not the current time. Getting that wrong drifts the countdown by
/// hours.
pub fn resets_at_of(obj: &Value, now_base: Option<i64>) -> Option<i64> {
    let map = obj.as_object()?;

    for key in RESET_KEYS {
        if let Some(v) = map.get(*key) {
            if let Some(ts) = to_unix_seconds(v) {
                return Some(ts);
            }
        }
    }
    for key in RESET_IN_KEYS {
        if let Some(offset) = map.get(*key).and_then(as_f64) {
            if offset.is_finite() {
                let base = now_base?;
                if !(0.0..=i64::MAX as f64).contains(&offset) {
                    return None;
                }
                return base.checked_add(offset as i64);
            }
        }
    }
    None
}

pub fn window_minutes_of(obj: &Value) -> Option<i64> {
    let map = obj.as_object()?;
    for key in WINDOW_MIN_KEYS {
        if let Some(v) = map.get(*key).and_then(as_f64) {
            if v.is_finite() && v > 0.0 {
                return Some(v as i64);
            }
        }
    }
    None
}

/// Turn a JSON scalar into unix seconds.
pub fn to_unix_seconds(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => {
            let f = n.as_f64()?;
            if !f.is_finite() || f <= 0.0 || f > 253_402_300_799_000.0 {
                return None;
            }
            // Anything past ~year 33658 in seconds is really milliseconds.
            Some(if f > 1e11 {
                (f / 1000.0) as i64
            } else {
                f as i64
            })
        }
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return None;
            }
            if let Ok(n) = t.parse::<f64>() {
                return to_unix_seconds(&Value::from(n));
            }
            parse_rfc3339(t)
        }
        _ => None,
    }
}

/// Minimal RFC 3339 / ISO-8601 parser.
///
/// Deliberately hand-rolled instead of pulling in `chrono`: we need exactly one
/// direction (string -> unix seconds) and zero timezone database.
///
/// Handles `2026-08-28T21:40:00.814169+00:00`, `...Z`, and a missing offset
/// (treated as UTC, which is what every provider we have seen actually means).
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19
        || !s.is_ascii()
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't' | b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse::<i64>().ok() };

    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let minute = num(14, 16)?;
    let second = num(17, 19)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    if year < 1970 || !(1..=days_in_month).contains(&day) || hour > 23 || minute > 59 || second > 59
    {
        return None;
    }

    let mut rest = &s[19..];
    // Skip fractional seconds; we round down to whole seconds.
    if rest.starts_with('.') {
        let end = rest[1..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        if end == 1 {
            return None;
        }
        rest = &rest[end..];
    }

    let offset_seconds = if rest.is_empty() || rest == "Z" || rest == "z" {
        0
    } else {
        let sign = match rest.as_bytes().first() {
            Some(b'+') => 1,
            Some(b'-') => -1,
            _ => return None,
        };
        if rest.len() != 5 && rest.len() != 6 {
            return None;
        }
        let oh: i64 = rest.get(1..3)?.parse().ok()?;
        // Both `+00:00` and `+0000` occur in the wild.
        let om: i64 = if rest.len() >= 6 && rest.as_bytes().get(3) == Some(&b':') {
            rest.get(4..6)?.parse().ok()?
        } else {
            rest.get(3..5)?.parse().ok()?
        };
        if oh > 23 || om > 59 {
            return None;
        }
        sign * (oh * 3600 + om * 60)
    };

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset_seconds)
}

/// Howard Hinnant's civil-date algorithm: days since 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Human label for a rate-limit window, derived from its length.
///
/// Derived rather than assumed: the same `primary` slot carries a 5-hour window
/// on some plans and a 7-day window on others, so hard-coding "5h" mislabels it.
pub fn label_for_window(minutes: Option<i64>, fallback: &str) -> String {
    match minutes {
        Some(m) if m <= 0 => fallback.to_string(),
        Some(m) if m < 60 => format!("{m} min window"),
        Some(m) if m < 60 * 24 => {
            let h = m / 60;
            if m % 60 == 0 {
                format!("{h}-hour window")
            } else {
                format!("{h}h{}m window", m % 60)
            }
        }
        Some(m) => {
            let d = m / (60 * 24);
            if d == 7 {
                "Weekly".to_string()
            } else if d == 1 {
                "Daily".to_string()
            } else {
                format!("{d}-day window")
            }
        }
        None => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_rfc3339_variants() {
        // Reference point: 2026-08-28T21:40:00Z
        let expected = 1787953200;
        assert_eq!(parse_rfc3339("2026-08-28T21:40:00Z"), Some(expected));
        assert_eq!(
            parse_rfc3339("2026-08-28T21:40:00.814169+00:00"),
            Some(expected)
        );
        assert_eq!(parse_rfc3339("2026-08-28T21:40:00"), Some(expected));
        // -03:00 means the same wall clock is three hours later in UTC.
        assert_eq!(
            parse_rfc3339("2026-08-28T21:40:00-03:00"),
            Some(expected + 3 * 3600)
        );
        assert_eq!(parse_rfc3339("garbage"), None);
        for invalid in [
            "2026-02-30T00:00:00Z",
            "2026-08-28T25:00:00Z",
            "2026-08-28T21:40:00Zjunk",
            "2026-08-28T21:40:00+99:00",
            "2026-08-28T21:40:00.Z",
            "2026-08-28T21:40:0💩",
        ] {
            assert_eq!(parse_rfc3339(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn reads_every_percent_spelling() {
        assert_eq!(percent_of(&json!({"utilization": 1})), Some(1.0));
        assert_eq!(percent_of(&json!({"utilization": 0.73})), Some(0.73));
        assert_eq!(percent_of(&json!({"utilization": 61})), Some(61.0));
        assert_eq!(percent_of(&json!({"usedPercent": 2.0})), Some(2.0));
        assert_eq!(percent_of(&json!({"used_percentage": 73})), Some(73.0));
        assert_eq!(percent_of(&json!({"nothing": 1})), None);
    }

    #[test]
    fn milliseconds_are_detected() {
        assert_eq!(to_unix_seconds(&json!(1788474061000i64)), Some(1788474061));
        assert_eq!(to_unix_seconds(&json!(1788474061i64)), Some(1788474061));
    }

    #[test]
    fn relative_reset_uses_the_supplied_base() {
        assert_eq!(resets_at_of(&json!({"resets_in_seconds": 600}), None), None);
        assert_eq!(
            resets_at_of(&json!({"resets_in_seconds": 600}), Some(i64::MAX)),
            None
        );
        let v = json!({"resets_in_seconds": 600});
        assert_eq!(resets_at_of(&v, Some(1_000_000)), Some(1_000_600));
    }

    #[test]
    fn null_rate_limits_do_not_shadow_a_real_one() {
        let doc = json!({"a": {"rate_limits": null}, "b": {"rate_limits": {"x": 1}}});
        assert!(find_key_non_null(&doc, "rate_limits").is_some());
    }

    #[test]
    fn window_labels_are_derived_not_assumed() {
        assert_eq!(label_for_window(Some(300), "x"), "5-hour window");
        assert_eq!(label_for_window(Some(10080), "x"), "Weekly");
    }
}
