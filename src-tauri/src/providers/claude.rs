//! Claude usage: prefer the official statusline feed when recent. Some CLI
//! versions expose get_usage; unsupported versions are remembered until the
//! binary changes. No credentials or private HTTP endpoints are used.
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::parse::{
    find_key_non_null, label_for_window, percent_of, resets_at_of, window_minutes_of,
};
use super::{Provider, ProviderSnapshot, Source, UsageWindow};
use crate::env::{resolve_binary, run_bounded_input};

const REQUEST_ID: &str = "cooldown-bar-get-usage";
const CLI_TIMEOUT: Duration = Duration::from_secs(12);

pub struct ClaudeProvider {
    pub title: String,
    pub stale_after_seconds: i64,
    unsupported_binary: Option<(PathBuf, Option<SystemTime>)>,
    last_file: Option<(SystemTime, u64)>,
    last_file_check: Instant,
}

impl ClaudeProvider {
    pub fn new(title: impl Into<String>, stale_after_seconds: i64) -> Self {
        Self {
            title: title.into(),
            stale_after_seconds,
            unsupported_binary: None,
            last_file: None,
            last_file_check: Instant::now() - Duration::from_secs(2),
        }
    }
}

impl Provider for ClaudeProvider {
    fn id(&self) -> &str {
        "claude"
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn detect(&self) -> bool {
        resolve_binary("claude").is_some() || statusline_file().is_some()
    }

    fn snapshot(&mut self) -> ProviderSnapshot {
        let file = read_from_statusline_file();
        if let Some(raw) = file.as_ref() {
            if raw.observed_at.is_some_and(|at| {
                super::parse::now_unix().saturating_sub(at) < self.stale_after_seconds
            }) {
                return self.finish(raw.clone(), Source::File);
            }
        }
        let fingerprint = resolve_binary("claude").map(|p| {
            let modified = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());
            (p, modified)
        });
        let error = if fingerprint.is_some() && fingerprint == self.unsupported_binary {
            "This Claude version does not support get_usage. Connect scripts/claude-statusline.sh to receive usage.".to_string()
        } else {
            match read_from_cli() {
                Ok(Some(raw)) => return self.finish(raw, Source::Cli),
                Ok(None) => "Claude returned no rate limits. Check the account or connect the statusline hook.".into(),
                Err(error) => {
                    if error.contains("Unsupported control request") { self.unsupported_binary = fingerprint; }
                    error
                }
            }
        };
        if let Some(mut raw) = file {
            raw.error = Some(error);
            return self.finish(raw, Source::File);
        }
        ProviderSnapshot::errored(self.id(), self.title(), error)
    }
    fn poll_event(&mut self) -> Option<ProviderSnapshot> {
        if self.last_file_check.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.last_file_check = Instant::now();
        let path = statusline_file()?;
        let metadata = std::fs::metadata(path).ok()?;
        let version = (metadata.modified().ok()?, metadata.len());
        if self.last_file == Some(version) {
            return None;
        }
        self.last_file = Some(version);
        read_from_statusline_file().map(|raw| self.finish(raw, Source::File))
    }
}

impl ClaudeProvider {
    fn finish(&self, raw: RawSnapshot, source: Source) -> ProviderSnapshot {
        ProviderSnapshot {
            id: self.id().to_string(),
            title: self.title.clone(),
            primary: raw.primary,
            secondary: raw.secondary,
            stale: false,
            observed_at: raw.observed_at,
            checked_at: super::parse::now_unix(),
            stale_after_seconds: self.stale_after_seconds,
            invalidate_previous: false,
            error: raw.error,
            source,
            available: true,
            plan: raw.plan,
        }
    }
}

#[derive(Default, Clone)]
struct RawSnapshot {
    primary: Option<UsageWindow>,
    secondary: Option<UsageWindow>,
    error: Option<String>,
    plan: Option<String>,
    observed_at: Option<i64>,
}

/// Ask the running Claude Code CLI for the current snapshot.
///
/// `Ok(None)` means the CLI ran but declined to report limits (a plan that does
/// not publish them). `Err` means we could not talk to it at all.
fn read_from_cli() -> Result<Option<RawSnapshot>, String> {
    let Some(bin) = resolve_binary("claude") else {
        return Err("`claude` was not found on your PATH.".to_string());
    };

    let request = format!(
        r#"{{"type":"control_request","request_id":"{REQUEST_ID}","request":{{"subtype":"get_usage"}}}}"#
    );

    let out = run_bounded_input(
        &bin,
        &[
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--no-session-persistence",
        ],
        Some(&format!("{request}\n")),
        CLI_TIMEOUT,
    )
    .map_err(|e| format!("Could not run `claude`: {e}"))?;

    if out.timed_out {
        return Err("`claude` did not answer within 12s.".to_string());
    }

    // The response is one line among several; match on request_id rather than
    // assuming a position in the stream.
    let mut payload: Option<Value> = None;
    for line in out.stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let resp = &v["response"];
        if resp["request_id"].as_str() == Some(REQUEST_ID) {
            if resp["subtype"].as_str() == Some("error") {
                let message = resp["error"]
                    .as_str()
                    .unwrap_or("Claude rejected the usage request.");
                return Err(format!(
                    "{} Connect the statusline hook if this version lacks usage support.",
                    message.chars().take(200).collect::<String>()
                ));
            }
            payload = Some(resp["response"].clone());
            break;
        }
    }

    if !out.success() {
        return Err(format!(
            "Claude usage command failed (exit {}).",
            out.status
                .and_then(|s| s.code())
                .map_or_else(|| "signal".into(), |c| c.to_string())
        ));
    }
    let Some(payload) = payload else {
        return Err("`claude` gave no answer to the usage request.".to_string());
    };

    // A plan that does not publish limits says so explicitly. Treat that as a
    // clear message instead of a zeroed ring.
    if payload["rate_limits_available"] == Value::Bool(false) {
        return Ok(Some(RawSnapshot {
            error: Some("This plan does not publish rate limits.".to_string()),
            plan: payload["subscription_type"].as_str().map(str::to_string),
            ..Default::default()
        }));
    }

    let limits = &payload["rate_limits"];
    if !limits.is_object() {
        return Ok(None);
    }

    Ok(Some(RawSnapshot {
        primary: window_from(
            limits.get("five_hour"),
            "Current session",
            Some(super::parse::now_unix()),
        ),
        secondary: weekly_window(limits),
        error: None,
        plan: payload["subscription_type"].as_str().map(str::to_string),
        observed_at: Some(super::parse::now_unix()),
    }))
}

/// The weekly figure, preferring the account-wide `seven_day` window.
///
/// When that is absent the limit is published per model family instead
/// (`seven_day_opus`, `seven_day_sonnet`, ...). In that case the binding
/// constraint is whichever family is furthest along, so we take the maximum.
fn weekly_window(limits: &Value) -> Option<UsageWindow> {
    if let Some(w) = window_from(limits.get("seven_day"), "All models", None) {
        return Some(w);
    }
    let map = limits.as_object()?;
    map.iter()
        .filter(|(k, _)| k.starts_with("seven_day_"))
        .filter_map(|(k, v)| {
            let label = k.trim_start_matches("seven_day_").replace('_', " ");
            window_from(Some(v), &title_case(&label), None)
        })
        .max_by(|a, b| a.percent.total_cmp(&b.percent))
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Build a window from a provider object, tolerating every field spelling.
fn window_from(v: Option<&Value>, fallback_label: &str, base: Option<i64>) -> Option<UsageWindow> {
    let v = v?;
    if !v.is_object() {
        return None;
    }
    let percent = percent_of(v)?;
    let label = label_for_window(window_minutes_of(v), fallback_label);
    Some(UsageWindow {
        percent,
        resets_at: resets_at_of(v, base),
        label,
    })
}

fn statusline_file() -> Option<PathBuf> {
    let p = crate::env::preferred_app_file("claude.json")?;
    p.is_file().then_some(p)
}

/// Bounded atomic hook snapshot. The timestamp is the hook write, not a scan.
fn read_from_statusline_file() -> Option<RawSnapshot> {
    use std::io::Read;
    let file = std::fs::File::open(statusline_file()?).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() > 1024 * 1024 {
        return None;
    }
    let at = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let mut text = String::new();
    file.take(1024 * 1024 + 1).read_to_string(&mut text).ok()?;
    if text.len() > 1024 * 1024 {
        return None;
    }
    parse_statusline(&serde_json::from_str::<Value>(&text).ok()?, at)
}
fn parse_statusline(root: &Value, at: i64) -> Option<RawSnapshot> {
    let limits = find_key_non_null(root, "rate_limits");
    let mut primary =
        limits.and_then(|v| window_from(v.get("five_hour"), "Current session", Some(at)));
    let mut secondary = limits.and_then(weekly_window);
    if primary.is_none() && secondary.is_none() {
        if let Some(items) = find_key_non_null(root, "limits").and_then(Value::as_array) {
            for item in items {
                let name = item
                    .get("limit_name")
                    .or_else(|| item.get("limit_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("Window");
                let Some(window) = window_from(Some(item), name, Some(at)) else {
                    continue;
                };
                if window_minutes_of(item).unwrap_or(0) >= 1440
                    || name.contains("seven")
                    || name.contains("week")
                {
                    secondary.get_or_insert(window);
                } else {
                    primary.get_or_insert(window);
                }
            }
        }
    }
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    Some(RawSnapshot {
        primary,
        secondary,
        error: None,
        observed_at: Some(at),
        plan: find_key_non_null(root, "subscription_type")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn official_statusline_percentages_include_zero_and_one_percent() {
        let raw = parse_statusline(&json!({"rate_limits":{"five_hour":{"used_percentage":0,"resets_at":1900000000},"seven_day":{"used_percentage":1}}}), 1800000000).unwrap();
        assert_eq!(raw.primary.unwrap().percent, 0.0);
        assert_eq!(raw.secondary.unwrap().percent, 1.0);
        assert_eq!(raw.observed_at, Some(1800000000));
    }
    #[test]
    fn array_fallback_does_not_require_rate_limits_wrapper() {
        let raw = parse_statusline(&json!({"limits":[{"limit_name":"Session","used_percentage":8,"resets_in_seconds":10}]}), 1800000000).unwrap();
        assert_eq!(raw.primary.unwrap().resets_at, Some(1800000010));
    }
}
