//! Process environment helpers.
//!
//! A GUI app launched from Finder does **not** inherit the shell's `PATH`. It
//! gets a minimal `/usr/bin:/bin:/usr/sbin:/sbin`, which contains neither
//! `claude` nor `codex` on a normal developer machine. Everything in this module
//! exists to make provider CLIs findable anyway.

use crate::process::{run_bounded, CommandOutput};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Give up on the login shell quickly. A `.zshrc` that blocks on an SSH
/// passphrase or a bare `read` would otherwise hang app startup forever.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(4);

/// Resolved `PATH` of the user's login shell, computed once.
static LOGIN_PATH: OnceLock<String> = OnceLock::new();

/// Cache of binary name -> absolute path.
///
/// Only **successful** lookups are cached. Caching a miss would mean a user who
/// installs a CLI after launching Cooldown Bar keeps seeing "not found" until they
/// restart the app.
static BINARY_CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

fn binary_cache() -> &'static Mutex<HashMap<String, PathBuf>> {
    BINARY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The `PATH` a command typed in the user's terminal would see.
///
/// Runs the login shell as an interactive login shell and asks it to print
/// `PATH`. Falls back to the inherited `PATH` plus common install prefixes if
/// the shell misbehaves or times out.
pub fn login_path() -> &'static str {
    LOGIN_PATH.get_or_init(|| {
        if let Some(p) = probe_login_shell_path() {
            if !p.trim().is_empty() {
                return merge_with_fallbacks(&p);
            }
        }
        let inherited = std::env::var("PATH").unwrap_or_default();
        merge_with_fallbacks(&inherited)
    })
}

fn probe_login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    // `-l` so profile files run, `-i` because many users put their PATH edits in
    // `.zshrc` (interactive-only) rather than `.zprofile`.
    let mut cmd = Command::new(&shell);
    cmd.args(["-lic", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let out = run_bounded(cmd, None, LOGIN_SHELL_TIMEOUT).ok()?;
    out.success().then_some(out.stdout)
}

/// Directories a provider CLI plausibly lives in, beyond whatever `PATH` says.
fn fallback_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = home_dir();

    for p in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/opt/homebrew/sbin",
    ] {
        dirs.push(PathBuf::from(p));
    }

    if let Some(h) = home.as_ref() {
        for rel in [
            ".local/bin",
            ".bun/bin",
            ".cargo/bin",
            ".claude/local",
            ".codex/bin",
            "bin",
        ] {
            dirs.push(h.join(rel));
        }

        // nvm keeps one bin dir per installed Node version.
        let nvm = h.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm) {
            for e in entries.flatten() {
                dirs.push(e.path().join("bin"));
            }
        }

        // Volta / asdf shims.
        dirs.push(h.join(".volta/bin"));
        dirs.push(h.join(".asdf/shims"));
    }

    dirs
}

fn merge_with_fallbacks(path: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !s.is_empty() && !seen.iter().any(|x| x == &s) {
            seen.push(s);
        }
    };
    for part in path.split(':') {
        push(part.trim().to_string());
    }
    for d in fallback_dirs() {
        push(d.to_string_lossy().to_string());
    }
    seen.join(":")
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

pub const APP_DIRECTORY: &str = ".cooldown-bar";
pub const LEGACY_APP_DIRECTORY: &str = ".notchusage";

pub fn app_dir() -> Option<PathBuf> {
    Some(home_dir()?.join(APP_DIRECTORY))
}

pub fn app_dirs() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    vec![home.join(APP_DIRECTORY), home.join(LEGACY_APP_DIRECTORY)]
}

pub fn preferred_app_file(relative: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    Some(preferred_app_file_from(&home, relative, |path| {
        path.is_file()
    }))
}

fn preferred_app_file_from(home: &Path, relative: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let current = home.join(APP_DIRECTORY).join(relative);
    if exists(&current) {
        return current;
    }
    let legacy = home.join(LEGACY_APP_DIRECTORY).join(relative);
    if exists(&legacy) {
        return legacy;
    }
    current
}

/// Extra absolute locations to try for a specific CLI, when it is shipped inside
/// an application bundle rather than installed on `PATH`.
///
/// The Codex CLI is a good example: on this kind of machine it exists only as
/// `ChatGPT.app/Contents/Resources/codex`, with nothing on `PATH` at all.
fn bundled_candidates(name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if name == "codex" {
        for base in [
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
        ] {
            out.push(base);
        }
        if let Some(h) = home_dir() {
            out.push(h.join("Applications/ChatGPT.app/Contents/Resources/codex"));
            out.push(h.join("Applications/Codex.app/Contents/Resources/codex"));
        }
    }
    out
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

/// Find `name` on the login `PATH`, then in bundle-specific fallbacks.
///
/// Returns `None` when the CLI genuinely is not installed — that is the signal
/// the UI uses to hide a provider's ring entirely.
pub fn resolve_binary(name: &str) -> Option<PathBuf> {
    if let Ok(cache) = binary_cache().lock() {
        if let Some(hit) = cache.get(name) {
            // Re-verify: the user may have uninstalled since we cached it.
            if is_executable(hit) {
                return Some(hit.clone());
            }
        }
    }

    let mut found: Option<PathBuf> = None;
    for dir in login_path().split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(name);
        if is_executable(&candidate) {
            found = Some(candidate);
            break;
        }
    }
    if found.is_none() {
        found = bundled_candidates(name)
            .into_iter()
            .find(|p| is_executable(p));
    }

    // Cache positives only. See BINARY_CACHE.
    if let Some(ref p) = found {
        if let Ok(mut cache) = binary_cache().lock() {
            cache.insert(name.to_string(), p.clone());
        }
    }
    found
}

/// Spawn `program` with `args`, write `input` to stdin, and collect output under
/// both a time limit and a byte limit.
///
/// The child is killed if it outlives `timeout`; `timed_out` reports that so the
/// caller can surface an honest error instead of an empty reading.
pub fn run_bounded_input(
    program: &Path,
    args: &[&str],
    input: Option<&str>,
    timeout: Duration,
) -> Result<CommandOutput, std::io::Error> {
    let mut cmd = Command::new(program);
    cmd.args(args.iter().map(OsStr::new));
    cmd.env("PATH", login_path());
    run_bounded(cmd, input, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_app_file_has_priority() {
        let home = Path::new("/tmp/cooldown-home");
        let selected = preferred_app_file_from(home, "config.json", |path| {
            path.ends_with(".cooldown-bar/config.json") || path.ends_with(".notchusage/config.json")
        });
        assert_eq!(selected, home.join(".cooldown-bar/config.json"));
    }

    #[test]
    fn legacy_app_file_is_used_during_upgrade() {
        let home = Path::new("/tmp/cooldown-home");
        let selected = preferred_app_file_from(home, "config.json", |path| {
            path.ends_with(".notchusage/config.json")
        });
        assert_eq!(selected, home.join(".notchusage/config.json"));
    }

    #[test]
    fn new_app_file_is_default_for_fresh_install() {
        let home = Path::new("/tmp/cooldown-home");
        let selected = preferred_app_file_from(home, "config.json", |_| false);
        assert_eq!(selected, home.join(".cooldown-bar/config.json"));
    }
}
