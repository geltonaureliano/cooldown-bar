//! User-supplied provider.
//!
//! Runs an arbitrary shell command and expects a small JSON document on stdout:
//!
//! ```json
//! { "percent": 52, "resets_at": 1756400000, "label": "Session",
//!   "secondary_percent": 11 }
//! ```
//!
//! This is the only place Cooldown Bar runs something it did not write, and the
//! the user's command may itself contact external services. Built-in provider
//! CLIs also use their own network connections.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::parse::{percent_of, resets_at_of};
use super::{Provider, ProviderSnapshot, Source, UsageWindow};
use crate::env::run_bounded_input;

const TIMEOUT: Duration = Duration::from_secs(3);

pub struct CustomProvider {
    pub title: String,
    pub command: String,
}

impl CustomProvider {
    pub fn new(title: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            command: command.into(),
        }
    }
}

impl Provider for CustomProvider {
    fn id(&self) -> &str {
        "custom"
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn detect(&self) -> bool {
        !self.command.trim().is_empty()
    }

    fn snapshot(&mut self) -> ProviderSnapshot {
        if self.command.trim().is_empty() {
            return ProviderSnapshot::missing(self.id(), self.title());
        }

        let out = match run_bounded_input(
            &PathBuf::from("/bin/sh"),
            &["-c", &self.command],
            None,
            TIMEOUT,
        ) {
            Ok(o) => o,
            Err(e) => return ProviderSnapshot::errored(self.id(), self.title(), e.to_string()),
        };

        if out.timed_out {
            return ProviderSnapshot::errored(
                self.id(),
                self.title(),
                "customCommand did not finish within 3s.",
            );
        }

        if !out.success() {
            return ProviderSnapshot::errored(
                self.id(),
                self.title(),
                format!(
                    "customCommand failed (exit {}).",
                    out.status
                        .and_then(|s| s.code())
                        .map_or_else(|| "signal".into(), |c| c.to_string())
                ),
            );
        }

        let Ok(v) = serde_json::from_str::<Value>(out.stdout.trim()) else {
            return ProviderSnapshot::errored(
                self.id(),
                self.title(),
                "customCommand did not print valid JSON.",
            );
        };

        let Some(percent) = percent_of(&v) else {
            return ProviderSnapshot::errored(
                self.id(),
                self.title(),
                "customCommand JSON has no `percent` field.",
            );
        };

        let label = v
            .get("label")
            .and_then(|x| x.as_str())
            .unwrap_or("Session")
            .to_string();

        let secondary = v
            .get("secondary_percent")
            .and_then(|x| x.as_f64())
            .map(|p| UsageWindow {
                percent: p.clamp(0.0, 100.0),
                resets_at: v
                    .get("secondary_resets_at")
                    .and_then(super::parse::to_unix_seconds),
                label: v
                    .get("secondary_label")
                    .and_then(|x| x.as_str())
                    .unwrap_or("Secondary")
                    .to_string(),
            });

        ProviderSnapshot {
            id: self.id().to_string(),
            title: self.title.clone(),
            primary: Some(UsageWindow {
                percent,
                resets_at: resets_at_of(&v, Some(super::parse::now_unix())),
                label,
            }),
            secondary,
            stale: false,
            error: None,
            source: Source::Cli,
            available: true,
            plan: None,
            observed_at: Some(super::parse::now_unix()),
            checked_at: super::parse::now_unix(),
            stale_after_seconds: 120,
            invalidate_previous: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_json_from_failed_command_is_not_trusted() {
        let mut provider = CustomProvider::new("Custom", "printf '{\"percent\":52}'; exit 1");
        assert!(provider.snapshot().primary.is_none());
    }
}
