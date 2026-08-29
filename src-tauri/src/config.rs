//! `~/.cooldown-bar/config.json`.
//!
//! Every key is optional and every missing key falls back to a default, so an
//! empty file, a partial file, or no file at all all work.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::env;

fn d_bar_width() -> f64 {
    62.0
}
fn d_concave_radius() -> f64 {
    31.0
}
fn d_top_offset() -> Option<f64> {
    None
}
fn d_edge_inset() -> f64 {
    0.0
}
fn d_ring_diameter() -> f64 {
    38.0
}
fn d_ring_line_width() -> f64 {
    3.0
}
fn d_item_gap() -> f64 {
    28.0
}
/// Persistent Codex reads; local Claude hook updates are checked separately.
fn d_refresh_seconds() -> u64 {
    10
}
fn d_stale_after_seconds() -> i64 {
    120
}
fn d_true() -> bool {
    true
}
fn d_custom_title() -> String {
    "Custom".into()
}
fn d_claude_color() -> String {
    "#FF5F2E".into()
}
fn d_codex_color() -> String {
    "#00E07A".into()
}
fn d_custom_color() -> String {
    "#E8E80A".into()
}

/// Which screen edge the bar clings to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
}

impl Edge {
    pub fn is_left(self) -> bool {
        matches!(self, Edge::Left)
    }
}

fn d_edge() -> Edge {
    Edge::Right
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Set by dragging the bar; persisted so it survives a restart.
    pub edge: Edge,
    pub bar_width: f64,
    pub concave_radius: f64,
    /// Gap between the top of the screen and the top of the bar, in points.
    ///
    /// `null` (the default) means "just below the menu bar", resolved at runtime
    /// from the live screen geometry so it adapts to notched and un-notched
    /// displays. An explicit `0` really does pin the bar to the top of the
    /// screen — but the menu bar will draw over it, because macOS composites the
    /// menu bar above every ordinary window level.
    pub top_offset: Option<f64>,
    pub edge_inset: f64,
    pub ring_diameter: f64,
    pub ring_line_width: f64,
    pub item_gap: f64,
    pub refresh_seconds: u64,
    pub stale_after_seconds: i64,
    pub show_claude: bool,
    pub show_codex: bool,
    pub custom_command: Option<String>,
    pub custom_title: String,
    pub claude_color: String,
    pub codex_color: String,
    pub custom_color: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            edge: d_edge(),
            bar_width: d_bar_width(),
            concave_radius: d_concave_radius(),
            top_offset: d_top_offset(),
            edge_inset: d_edge_inset(),
            ring_diameter: d_ring_diameter(),
            ring_line_width: d_ring_line_width(),
            item_gap: d_item_gap(),
            refresh_seconds: d_refresh_seconds(),
            stale_after_seconds: d_stale_after_seconds(),
            show_claude: d_true(),
            show_codex: d_true(),
            custom_command: None,
            custom_title: d_custom_title(),
            claude_color: d_claude_color(),
            codex_color: d_codex_color(),
            custom_color: d_custom_color(),
        }
    }
}

impl Config {
    /// Resolve `top_offset` against live screen geometry.
    ///
    /// Kept as a method rather than baked into `load()` so a display change is
    /// picked up without re-reading the file.
    pub fn resolved_top_offset(&self, menu_bar_height: f64) -> f64 {
        self.top_offset.unwrap_or(menu_bar_height)
    }

    pub fn path() -> Option<PathBuf> {
        env::preferred_app_file("config.json")
    }

    /// Read the config, falling back to defaults on anything unreadable.
    ///
    /// A malformed config must not stop the bar from rendering — the user would
    /// have no way to see the error, since there is no window to show it in.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };
        if !path.is_file() {
            return (Self::default(), None);
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return (Self::default(), Some(format!("config unreadable: {e}"))),
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(cfg) => (cfg.sanitised(), None),
            Err(e) => (
                Self::default(),
                Some(format!("config.json is not valid JSON: {e}")),
            ),
        }
    }

    /// Persist just the keys the user can change by dragging.
    ///
    /// Reads the file back as a generic JSON object first so any key we do not
    /// model — including ones a future version adds — survives the write.
    pub fn persist_position(edge: Edge, top_offset: f64) -> Result<(), String> {
        let Some(path) = Self::path() else {
            return Err("no home directory".into());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }

        let mut doc = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|e| format!("Refusing to overwrite invalid config: {e}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
            Err(error) => return Err(error.to_string()),
        };
        if !doc.is_object() {
            return Err("Refusing to overwrite a non-object config.".into());
        }

        if let Some(map) = doc.as_object_mut() {
            map.insert(
                "edge".into(),
                serde_json::Value::String(if edge.is_left() { "left" } else { "right" }.into()),
            );
            map.insert("topOffset".into(), serde_json::json!(top_offset.round()));
        }

        let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
        // Write-then-rename so a crash mid-write cannot leave a truncated config.
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let tmp = path.with_extension(format!("json.{}.{nonce}.tmp", std::process::id()));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    }

    /// Clamp values that would produce an unusable or invisible window.
    fn sanitised(mut self) -> Self {
        let d = Self::default();
        if !(24.0..=400.0).contains(&self.bar_width) {
            self.bar_width = d.bar_width;
        }
        if !(0.0..=80.0).contains(&self.concave_radius) {
            self.concave_radius = d.concave_radius;
        }
        if let Some(v) = self.top_offset {
            if !(-200.0..=2000.0).contains(&v) {
                self.top_offset = d.top_offset;
            }
        }
        if !(-100.0..=1000.0).contains(&self.edge_inset) {
            self.edge_inset = d.edge_inset;
        }
        if !(14.0..=200.0).contains(&self.ring_diameter) {
            self.ring_diameter = d.ring_diameter;
        }
        if !(0.5..=40.0).contains(&self.ring_line_width) {
            self.ring_line_width = d.ring_line_width;
        }
        if !(0.0..=200.0).contains(&self.item_gap) {
            self.item_gap = d.item_gap;
        }
        self.ring_diameter = self.ring_diameter.min((self.bar_width - 8.0).max(14.0));
        self.ring_line_width = self.ring_line_width.min(self.ring_diameter / 3.0);
        self.refresh_seconds = self.refresh_seconds.clamp(5, 3600);
        self.stale_after_seconds = self.stale_after_seconds.clamp(30, 86_400);
        self
    }
}
