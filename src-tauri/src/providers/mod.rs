//! Provider abstraction.
//!
//! Every provider normalises to the same [`ProviderSnapshot`] so the UI never
//! branches on which CLI produced a number. Two very different protocols sit
//! behind this (a Claude Code control request, a Codex JSON-RPC app server), and
//! that difference must not leak into React.

pub mod claude;
pub mod codex;
pub mod custom;
pub mod parse;

use serde::Serialize;

/// One rate-limit window.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UsageWindow {
    /// 0..=100.
    pub percent: f64,
    /// Absolute unix seconds, or `None` when the provider did not say.
    pub resets_at: Option<i64>,
    pub label: String,
}

/// Where a reading came from. Surfaced in the UI so a fallback reading is never
/// mistaken for a live one.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Asked the provider's CLI directly. Authoritative, account-wide.
    Cli,
    /// Read from a file the provider left behind. Correct when written, but
    /// frozen once the provider stops running.
    File,
    /// Nothing readable.
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProviderSnapshot {
    /// `"claude" | "codex" | "custom"`.
    pub id: String,
    pub title: String,
    /// Drives the ring.
    pub primary: Option<UsageWindow>,
    pub secondary: Option<UsageWindow>,
    /// True when the reading is too old to trust.
    pub stale: bool,
    /// Human-readable reason the reading is missing or degraded.
    pub error: Option<String>,
    pub source: Source,
    /// False when the provider's CLI is not installed at all. The UI hides these
    /// instead of showing a permanently empty ring.
    pub available: bool,
    /// Plan name when the provider reports one ("max", "pro"). Shown in the bubble.
    pub plan: Option<String>,
    /// When the source actually supplied this reading, never the file scan time.
    pub observed_at: Option<i64>,
    pub checked_at: i64,
    pub stale_after_seconds: i64,
    #[serde(skip)]
    pub invalidate_previous: bool,
}

impl ProviderSnapshot {
    pub fn missing(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            primary: None,
            secondary: None,
            stale: false,
            error: None,
            source: Source::Unavailable,
            available: false,
            plan: None,
            observed_at: None,
            checked_at: parse::now_unix(),
            stale_after_seconds: 120,
            invalidate_previous: false,
        }
    }

    pub fn errored(id: &str, title: &str, message: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            primary: None,
            secondary: None,
            stale: true,
            error: Some(message.into()),
            source: Source::Unavailable,
            available: true,
            plan: None,
            observed_at: None,
            checked_at: parse::now_unix(),
            stale_after_seconds: 120,
            invalidate_previous: false,
        }
    }
}

/// A source of usage readings.
pub trait Provider: Send {
    fn id(&self) -> &str;
    fn title(&self) -> &str;
    /// Whether the underlying CLI exists on this machine. Called on every poll
    /// so a CLI installed after launch starts working without a restart.
    fn detect(&self) -> bool;
    /// Take a reading. Must never panic and must never block indefinitely.
    fn snapshot(&mut self) -> ProviderSnapshot;
    /// Nonblocking notifications or a newly written local statusline snapshot.
    fn poll_event(&mut self) -> Option<ProviderSnapshot> {
        None
    }
    fn disconnect(&mut self) {}
}

/// Keep the last good reading on a failed attempt, without rejuvenating its age.
pub fn merge_snapshot(
    previous: Option<&ProviderSnapshot>,
    mut next: ProviderSnapshot,
    ttl: i64,
) -> ProviderSnapshot {
    if !next.invalidate_previous {
        if let Some(old) = previous.filter(|s| s.primary.is_some() && s.id == next.id) {
            let older = next.observed_at.is_none() || next.observed_at < old.observed_at;
            let prefer_live = next.id == "codex"
                && old.source == Source::Cli
                && next.source == Source::File
                && !is_stale(old, parse::now_unix());
            if next.primary.is_none() || older || prefer_live {
                let error = next.error.clone();
                let checked_at = next.checked_at;
                next = old.clone();
                next.error = error.or_else(|| Some("Waiting for a newer reading.".into()));
                next.checked_at = checked_at;
            }
        }
    }
    next.stale_after_seconds = ttl;
    next.stale = next.stale || is_stale(&next, parse::now_unix());
    next
}

pub fn is_stale(snapshot: &ProviderSnapshot, now: i64) -> bool {
    snapshot.observed_at.map_or(true, |at| {
        at > now.saturating_add(5) || now.saturating_sub(at) >= snapshot.stale_after_seconds
    }) || snapshot
        .primary
        .as_ref()
        .and_then(|w| w.resets_at)
        .is_some_and(|at| at <= now)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_refresh_retains_age_and_last_value() {
        let mut old = ProviderSnapshot::errored("codex", "Codex", "");
        old.primary = Some(UsageWindow {
            percent: 12.0,
            resets_at: None,
            label: "Session".into(),
        });
        old.observed_at = Some(parse::now_unix() - 121);
        old.stale = false;
        let merged = merge_snapshot(
            Some(&old),
            ProviderSnapshot::errored("codex", "Codex", "offline"),
            120,
        );
        assert_eq!(merged.primary, old.primary);
        assert_eq!(merged.observed_at, old.observed_at);
        assert_eq!(merged.error.as_deref(), Some("offline"));
        assert!(merged.stale);
    }
    #[test]
    fn invalidation_does_not_reuse_another_accounts_reading() {
        let mut old = ProviderSnapshot::errored("codex", "Codex", "");
        old.primary = Some(UsageWindow {
            percent: 12.0,
            resets_at: None,
            label: "Session".into(),
        });
        let mut next = ProviderSnapshot::errored("codex", "Codex", "Account changed");
        next.invalidate_previous = true;
        assert!(merge_snapshot(Some(&old), next, 120).primary.is_none());
    }
    #[test]
    fn a_new_claude_hook_event_replaces_an_older_cli_reading() {
        let mut old = ProviderSnapshot::errored("claude", "Claude", "");
        old.primary = Some(UsageWindow {
            percent: 12.0,
            resets_at: None,
            label: "Session".into(),
        });
        old.source = Source::Cli;
        old.observed_at = Some(parse::now_unix() - 2);
        old.stale = false;
        let mut next = old.clone();
        next.source = Source::File;
        next.observed_at = Some(parse::now_unix());
        next.primary.as_mut().unwrap().percent = 13.0;
        let merged = merge_snapshot(Some(&old), next, 120);
        assert_eq!(merged.primary.unwrap().percent, 13.0);
        assert_eq!(merged.source, Source::File);
    }
}
