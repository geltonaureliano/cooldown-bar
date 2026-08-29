//! Persistent Codex app-server connection. Polls reconcile notifications; no prompts.
use super::parse::{
    find_key_non_null, label_for_window, now_unix, percent_of, resets_at_of, to_unix_seconds,
    window_minutes_of,
};
use super::{Provider, ProviderSnapshot, Source, UsageWindow};
use crate::env::{home_dir, login_path, resolve_binary};
use crate::process::ChildProcess;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const INIT_TIMEOUT: Duration = Duration::from_secs(6);
const READ_TIMEOUT: Duration = Duration::from_secs(8);
const ROLLOUT_TAIL_BYTES: u64 = 512 * 1024;
const ROLLOUT_FILES_TO_SCAN: usize = 8;

pub struct CodexProvider {
    title: String,
    stale_after_seconds: i64,
    session: Option<Session>,
}
impl CodexProvider {
    pub fn new(title: impl Into<String>, stale_after_seconds: i64) -> Self {
        Self {
            title: title.into(),
            stale_after_seconds,
            session: None,
        }
    }
    fn finish(&self, raw: RawSnapshot, source: Source) -> ProviderSnapshot {
        ProviderSnapshot {
            id: self.id().into(),
            title: self.title.clone(),
            primary: raw.primary,
            secondary: raw.secondary,
            stale: source == Source::File,
            error: raw.error,
            source,
            available: true,
            plan: raw.plan,
            observed_at: raw.observed_at,
            checked_at: now_unix(),
            stale_after_seconds: self.stale_after_seconds,
            invalidate_previous: false,
        }
    }
    fn read_live(&mut self, bin: &Path) -> Result<RawSnapshot, String> {
        if self.session.is_none() {
            self.session = Some(Session::connect(bin)?);
        }
        let session = self.session.as_mut().expect("connected session");
        // Discard pre-request notifications; the response is the newer authority.
        session.latest = None;
        let result = session.request("account/rateLimits/read", Value::Null, READ_TIMEOUT)?;
        session.latest = None;
        session.headline_limit = result
            .get("rateLimits")
            .and_then(|v| v.get("limitId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        parse_live(&result)
    }
}
impl Provider for CodexProvider {
    fn id(&self) -> &str {
        "codex"
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn detect(&self) -> bool {
        resolve_binary("codex").is_some() || !newest_rollouts(1).is_empty()
    }
    fn disconnect(&mut self) {
        self.session = None;
    }
    fn snapshot(&mut self) -> ProviderSnapshot {
        let failure = if let Some(bin) = resolve_binary("codex") {
            match self.read_live(&bin) {
                Ok(raw) => return self.finish(raw, Source::Cli),
                Err(error) => {
                    self.session = None;
                    error
                }
            }
        } else {
            "`codex` was not found. Install it or open the desktop app.".into()
        };
        if failure.contains("account changed") {
            let mut next = ProviderSnapshot::errored(self.id(), self.title(), failure);
            next.invalidate_previous = true;
            return next;
        }
        if let Some(mut raw) = read_from_rollouts() {
            // Logs cannot establish which account is currently signed in.
            raw.error = Some(format!("{failure} Showing an unverified session log."));
            return self.finish(raw, Source::File);
        }
        ProviderSnapshot::errored(self.id(), self.title(), failure)
    }
    fn poll_event(&mut self) -> Option<ProviderSnapshot> {
        let session = self.session.as_mut()?;
        let result = session.poll();
        match result {
            Ok(Some(raw)) => Some(self.finish(raw, Source::Cli)),
            Ok(None) => None,
            Err(error) => {
                let changed = session.account_changed;
                self.session = None;
                let mut next = ProviderSnapshot::errored(self.id(), self.title(), error);
                next.invalidate_previous = changed;
                Some(next)
            }
        }
    }
}

#[derive(Default)]
struct RawSnapshot {
    primary: Option<UsageWindow>,
    secondary: Option<UsageWindow>,
    error: Option<String>,
    plan: Option<String>,
    observed_at: Option<i64>,
}
struct Session {
    process: ChildProcess,
    next_id: u64,
    latest: Option<Value>,
    account_changed: bool,
    headline_limit: Option<String>,
}
impl Session {
    fn connect(bin: &Path) -> Result<Self, String> {
        let mut command = Command::new(bin);
        command
            .args(["app-server", "--stdio"])
            .env("PATH", login_path());
        let process = ChildProcess::spawn(command, false)
            .map_err(|e| format!("Could not start Codex: {e}"))?;
        let mut session = Self {
            process,
            next_id: 0,
            latest: None,
            account_changed: false,
            headline_limit: None,
        };
        session.request("initialize", json!({"clientInfo":{"name":"Cooldown Bar","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}), INIT_TIMEOUT)?;
        session
            .process
            .send_json(
                &json!({"method":"initialized","params":{}}),
                Duration::from_secs(1),
            )
            .map_err(|e| e.to_string())?;
        Ok(session)
    }
    fn notification(&mut self, value: Value) {
        match value.get("method").and_then(Value::as_str) {
            Some("account/rateLimits/updated") => {
                let params = value.get("params");
                let id = params
                    .and_then(|v| v.get("rateLimits"))
                    .and_then(|v| v.get("limitId"))
                    .and_then(Value::as_str);
                // A model-specific notification must never replace the headline ring.
                if id.is_none()
                    || self
                        .headline_limit
                        .as_deref()
                        .map_or(true, |headline| Some(headline) == id)
                {
                    self.latest = params.cloned();
                }
            }
            Some("account/updated") => {
                self.account_changed = true;
                self.latest = None;
            }
            _ => {}
        }
    }
    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let deadline = Instant::now() + timeout;
        self.process
            .send_json(
                &json!({"id":id,"method":method,"params":params}),
                Duration::from_secs(1),
            )
            .map_err(|e| e.to_string())?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("Codex {method} timed out."));
            }
            let value = self
                .process
                .receive_json(remaining)
                .map_err(|e| format!("Codex connection: {e}"))?
                .ok_or_else(|| format!("Codex {method} timed out."))?;
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = value.get("error") {
                    return Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex request failed.")
                        .chars()
                        .take(240)
                        .collect());
                }
                if self.account_changed && method == "account/rateLimits/read" {
                    return Err("Codex account changed while reading. Refreshing usage.".into());
                }
                self.account_changed = false;
                return value
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "Codex returned an invalid response.".into());
            }
            self.notification(value);
        }
    }
    fn poll(&mut self) -> Result<Option<RawSnapshot>, String> {
        // Bound noisy notifications; unread messages remain buffered.
        for _ in 0..32 {
            match self.process.receive_json(Duration::ZERO) {
                Ok(Some(value)) => self.notification(value),
                Ok(None) => break,
                Err(e) => return Err(format!("Codex connection closed: {e}")),
            }
        }
        if self.account_changed {
            return Err("Codex account changed. Refreshing usage.".into());
        }
        self.latest.take().map(|v| parse_live(&v)).transpose()
    }
}

fn parse_live(result: &Value) -> Result<RawSnapshot, String> {
    let limits = result
        .get("rateLimits")
        .filter(|v| v.is_object())
        .ok_or("Codex reported no rate limits. Check the signed-in account.")?;
    let primary = window_from(limits.get("primary"), "Current window", Some(now_unix()));
    let secondary = window_from(limits.get("secondary"), "Weekly", Some(now_unix()))
        .or_else(|| extra_window(result, limits));
    if primary.is_none() && secondary.is_none() {
        return Err("Codex did not report a usage percentage for this account.".into());
    }
    Ok(RawSnapshot {
        primary: primary.or_else(|| secondary.clone()),
        secondary: if limits.get("primary").is_some_and(Value::is_object) {
            secondary
        } else {
            None
        },
        plan: limits
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_string),
        observed_at: Some(now_unix()),
        error: None,
    })
}
fn extra_window(result: &Value, headline: &Value) -> Option<UsageWindow> {
    let id = headline.get("limitId").and_then(Value::as_str);
    result
        .get("rateLimitsByLimitId")?
        .as_object()?
        .iter()
        .filter_map(|(key, entry)| {
            if Some(key.as_str()) == id || entry.get("primary") == headline.get("primary") {
                return None;
            }
            let primary = entry.get("primary")?;
            if window_minutes_of(primary).unwrap_or(i64::MAX) >= 1440 {
                return None;
            }
            let mut window = window_from(Some(primary), "Session", Some(now_unix()))?;
            let name = entry
                .get("limitName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(key);
            window.label = format!("{name} · {}", window.label);
            Some(window)
        })
        .max_by(|a, b| a.percent.total_cmp(&b.percent))
}
fn window_from(v: Option<&Value>, fallback: &str, base: Option<i64>) -> Option<UsageWindow> {
    let v = v?;
    Some(UsageWindow {
        percent: percent_of(v)?,
        resets_at: resets_at_of(v, base),
        label: label_for_window(window_minutes_of(v), fallback),
    })
}

/// Read the event timestamp, never the file mtime (later chat messages touch it).
fn rollout_snapshot(text: &str) -> Option<RawSnapshot> {
    text.lines()
        .rev()
        .filter(|line| line.contains("rate_limits"))
        .filter_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            let limits = find_key_non_null(&v, "rate_limits")?;
            let observed_at = v.get("timestamp").and_then(to_unix_seconds);
            let primary = window_from(limits.get("primary"), "Current window", observed_at);
            let secondary = window_from(limits.get("secondary"), "Weekly", observed_at);
            if primary.is_none() && secondary.is_none() {
                return None;
            }
            Some(RawSnapshot {
                primary,
                secondary,
                observed_at,
                error: None,
                plan: limits
                    .get("plan_type")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .max_by_key(|r| r.observed_at)
}
fn read_from_rollouts() -> Option<RawSnapshot> {
    newest_rollouts(ROLLOUT_FILES_TO_SCAN)
        .into_iter()
        .filter_map(|path| read_tail(&path, ROLLOUT_TAIL_BYTES).and_then(|t| rollout_snapshot(&t)))
        .max_by_key(|r| r.observed_at)
}

/// Newest rollout files by mtime, most recent first.
fn newest_rollouts(limit: usize) -> Vec<PathBuf> {
    let Some(root) = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".codex")))
        .map(|h| h.join("sessions"))
    else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    collect_rollouts(&root, &mut found, 0);
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().take(limit).map(|(_, p)| p).collect()
}

fn collect_rollouts(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            collect_rollouts(&path, out, depth + 1);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            if let Ok(m) = entry.metadata() {
                if let Ok(t) = m.modified() {
                    out.push((t, path));
                }
            }
        }
    }
}

/// Read at most the last `max_bytes` of a file, aligned to a line boundary.
fn read_tail(path: &Path, max_bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(max_bytes);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(max_bytes.min(len) as usize);
    f.take(max_bytes).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    // A mid-line start would produce one unparseable fragment; drop it.
    Some(if start > 0 {
        match text.find('\n') {
            Some(i) => text[i + 1..].to_string(),
            None => String::new(),
        }
    } else {
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_freshness_uses_the_usage_event_and_relative_reset_base() {
        let text = r#"{"timestamp":"2026-08-28T10:00:00Z","payload":{"rate_limits":{"primary":{"used_percent":1,"resets_in_seconds":300}}}}
{"timestamp":"2026-08-28T20:00:00Z","payload":{"rate_limits":null}}"#;
        let raw = rollout_snapshot(text).unwrap();
        let expected = to_unix_seconds(&json!("2026-08-28T10:00:00Z")).unwrap();
        assert_eq!(raw.observed_at, Some(expected));
        assert_eq!(raw.primary.unwrap().resets_at, Some(expected + 300));
    }
    #[test]
    fn extra_limit_is_named_and_does_not_duplicate_headline() {
        let headline =
            json!({"limitId":"codex", "primary":{"usedPercent":20,"windowDurationMins":300}});
        let only_headline = json!({"rateLimitsByLimitId":{"codex":headline}});
        assert!(extra_window(&only_headline, &headline).is_none());
        let result = json!({"rateLimitsByLimitId":{"model":{"limitName":"Model A","primary":{"usedPercent":40,"windowDurationMins":300}}}});
        assert_eq!(
            extra_window(&result, &headline).unwrap().label,
            "Model A · 5-hour window"
        );
    }
    #[test]
    fn rpc_matches_ids_and_keeps_notifications_between_requests() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", r#"read first; printf '%s\n' '{"method":"account/rateLimits/updated","params":{"rateLimits":{"primary":{"usedPercent":13}}}}' '{"id":99,"result":{}}' '{"id":1,"result":{"ok":true}}'; read second; printf '%s\n' '{"id":2,"result":{"ok":true}}'; sleep 1"#]);
        let mut session = Session {
            process: ChildProcess::spawn(command, false).unwrap(),
            next_id: 0,
            latest: None,
            account_changed: false,
            headline_limit: None,
        };
        assert_eq!(
            session
                .request("test", Value::Null, Duration::from_secs(1))
                .unwrap()["ok"],
            true
        );
        assert!(session.latest.is_some());
        assert_eq!(
            session
                .request("test", Value::Null, Duration::from_secs(1))
                .unwrap()["ok"],
            true
        );
        assert_eq!(
            session.poll().unwrap().unwrap().primary.unwrap().percent,
            13.0
        );
    }
    #[test]
    #[ignore = "Contacts the locally signed-in Codex account; run explicitly"]
    fn live_codex_connection_is_reused() {
        let mut provider = CodexProvider::new("Codex Usage", 120);
        let first = provider.snapshot();
        assert_eq!(
            first.source,
            Source::Cli,
            "{}",
            first.error.unwrap_or_default()
        );
        assert!(first.primary.is_some());
        assert_eq!(provider.session.as_ref().unwrap().next_id, 2);
        let second = provider.snapshot();
        assert_eq!(
            second.source,
            Source::Cli,
            "{}",
            second.error.unwrap_or_default()
        );
        assert_eq!(provider.session.as_ref().unwrap().next_id, 3);
        assert!(second.observed_at >= first.observed_at);
    }
}
