//! Cooldown Bar — a vertical rate-limit bar pinned to the right edge of the screen.
//!
//! macOS only, by design: the whole thing is an `NSPanel` plus safe-area
//! geometry, and there is no meaningful cross-platform version of either.

#![cfg(target_os = "macos")]

mod config;
mod env;
mod layout;
mod motion;
mod mouse;
mod observers;
mod panel;
mod poller;
mod process;
mod providers;
mod screen;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{ActivationPolicy, AppHandle, Emitter, LogicalSize, Manager, Runtime};
use tauri_nspanel::ManagerExt;

use config::Config;
use layout::Layout;
use providers::ProviderSnapshot;

/// Everything the app needs to answer a command or reposition itself.
pub struct AppState {
    /// Serialises complete snapshots so bootstrap/reload cannot mix generations.
    pub updates: Mutex<()>,
    pub revision: AtomicU64,
    pub poller: Arc<poller::Poller>,
    pub mouse: Arc<mouse::MouseTracker>,
    pub motion: Mutex<motion::MotionState>,
    pub config: Mutex<Config>,
    pub layout: Mutex<Layout>,
    pub snapshots: Mutex<Vec<ProviderSnapshot>>,
    pub config_error: Mutex<Option<String>>,
    /// Live window position in **top-left** screen coordinates.
    ///
    /// Kept here rather than recomputed from the config on demand, because
    /// during a drag the window is somewhere the config knows nothing about and
    /// hit-testing still has to follow it.
    pub window_pos: Mutex<(f64, f64)>,
    /// True while the in-app context menu is showing.
    pub menu_open: std::sync::atomic::AtomicBool,
}

/// What the webview needs for a first paint, before any poll has landed.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub revision: u64,
    pub motion: motion::MotionState,
    pub layout: Layout,
    pub snapshots: Vec<ProviderSnapshot>,
    pub colors: Colors,
    pub config_error: Option<String>,
    /// Provider id -> icon to draw instead of the built-in mark.
    pub icons: std::collections::HashMap<String, IconSource>,
}

/// An icon file to render in place of a drawn mark.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IconSource {
    pub path: String,
    /// True for a file copied out of a vendor's app bundle.
    ///
    /// Those are full app icons — a dark glyph on a light rounded square — so
    /// the UI inverts them to get the flat light-on-dark mark the design wants.
    /// A file the user supplied is never touched, because it is presumably
    /// already the shape they want.
    pub vendor: bool,
    pub version: u64,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Colors {
    pub claude: String,
    pub codex: String,
    pub custom: String,
}

impl Colors {
    fn from_config(cfg: &Config) -> Self {
        Self {
            claude: cfg.claude_color.clone(),
            codex: cfg.codex_color.clone(),
            custom: cfg.custom_color.clone(),
        }
    }
}

#[tauri::command]
fn get_usage(state: tauri::State<'_, AppState>) -> Bootstrap {
    let _guard = state.updates.lock().expect("state update lock");
    bootstrap(&state)
}

fn bootstrap(state: &AppState) -> Bootstrap {
    let cfg = state.config.lock().map(|c| c.clone()).unwrap_or_default();
    Bootstrap {
        revision: state.revision.load(Ordering::SeqCst),
        motion: *state.motion.lock().expect("motion lock"),
        layout: state
            .layout
            .lock()
            .map(|l| *l)
            .unwrap_or_else(|_| Layout::compute(&cfg, 0)),
        snapshots: state
            .snapshots
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default(),
        colors: Colors::from_config(&cfg),
        config_error: state.config_error.lock().ok().and_then(|e| e.clone()),
        icons: icon_overrides(),
    }
}

/// Look for `~/.cooldown-bar/icons/<id>.png`.
///
/// Scanned on every call rather than cached, so dropping a file in takes effect
/// on the next reload instead of the next launch.
fn icon_overrides() -> std::collections::HashMap<String, IconSource> {
    let mut out = std::collections::HashMap::new();
    let dirs: Vec<_> = env::app_dirs()
        .into_iter()
        .map(|root| root.join("icons"))
        .collect();
    if dirs.is_empty() {
        return out;
    }
    for id in ["claude", "codex", "custom"] {
        let user = dirs
            .iter()
            .map(|dir| dir.join(format!("{id}.png")))
            .find(|path| path.is_file());
        if let Some(user) = user {
            out.insert(
                id.to_string(),
                IconSource {
                    path: user.to_string_lossy().into_owned(),
                    vendor: false,
                    version: icon_version(&user),
                },
            );
            continue;
        }
        let seeded = dirs
            .iter()
            .map(|dir| dir.join(format!("{id}{VENDOR_SUFFIX}")))
            .find(|path| path.is_file());
        if let Some(seeded) = seeded {
            out.insert(
                id.to_string(),
                IconSource {
                    path: seeded.to_string_lossy().into_owned(),
                    vendor: true,
                    version: icon_version(&seeded),
                },
            );
        }
    }
    out
}

fn icon_version(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|t| t.as_millis() as u64)
        .unwrap_or(0)
}

/// Name suffix for icons copied out of a vendor bundle, kept distinct from a
/// user-supplied `<id>.png` so the two can be treated differently.
const VENDOR_SUFFIX: &str = ".vendor.png";

/// Copy a vendor's own icon out of its installed app bundle, once.
///
/// No logo is bundled with Cooldown Bar or downloaded: the file already exists on
/// this machine because the vendor's app put it there, and it is copied into the
/// user's own config directory so the asset-protocol scope can stay pinned to
/// `~/.cooldown-bar/icons/**`.
///
/// Only fills gaps — a file the user has put there is never overwritten, so
/// replacing an icon is just a matter of dropping your own PNG in.
fn seed_vendor_icons() {
    let Some(dir) = env::app_dir().map(|root| root.join("icons")) else {
        return;
    };

    // (provider id, candidate source paths in priority order)
    const SOURCES: &[(&str, &[&str])] = &[(
        "codex",
        &[
            "/Applications/ChatGPT.app/Contents/Resources/icon-chatgpt.png",
            "/Applications/Codex.app/Contents/Resources/icon-chatgpt.png",
            "/Applications/ChatGPT.app/Contents/Resources/icon-codex-light.png",
        ],
    )];

    for (id, candidates) in SOURCES {
        let dest = dir.join(format!("{id}{VENDOR_SUFFIX}"));
        if dest.exists() {
            continue;
        }
        let Some(src) = candidates
            .iter()
            .map(std::path::Path::new)
            .find(|p| p.is_file())
        else {
            continue;
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        match std::fs::copy(src, &dest) {
            Ok(_) => eprintln!("[cooldown-bar] seeded {id} icon from {}", src.display()),
            Err(e) => eprintln!("[cooldown-bar] could not seed {id} icon: {e}"),
        }
    }
}

/// Tell the cursor loop to keep delivering events while the in-app menu is open.
///
/// # Why the menu is HTML and not an `NSMenu`
///
/// `Menu::popup` deadlocks this app. `-[NSMenu popUpMenuPositioningItem:...]`
/// opens an `NSMenuTrackingSession` event loop that only terminates once it sees
/// the events it is waiting for; with `ActivationPolicy::Accessory` and a panel
/// that refuses key status, those events never arrive. The main thread parks in
/// `nextEventMatchingMask:` forever and the whole UI freezes — confirmed with a
/// thread sample, not guessed.
///
/// Activating the app first would feed the loop, but stealing focus is the one
/// thing this app must never do. So the menu is drawn in the webview instead.
/// The only piece that needs native help is this: while it is open the window
/// must accept clicks outside the bar rect, where it is normally click-through.
#[tauri::command]
fn set_menu_open(state: tauri::State<'_, AppState>, open: bool) {
    state
        .menu_open
        .store(open, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn reload(app: AppHandle) {
    reload_config(&app);
}

#[tauri::command]
fn dock_nearest(state: tauri::State<'_, AppState>) {
    state.mouse.request_dock(false);
}

#[tauri::command]
fn motion_preferences(state: tauri::State<'_, AppState>, reduced: bool) {
    state.mouse.set_reduced_motion(reduced);
}

#[tauri::command]
fn refresh(app: AppHandle) {
    poller::refresh_now(&app);
}

pub fn run() {
    let _instance = match single_instance() {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            eprintln!("[cooldown-bar] another instance is already running");
            return;
        }
        Err(error) => {
            eprintln!("[cooldown-bar] cannot acquire instance lock: {error}");
            return;
        }
    };
    process::install_signal_handlers();
    let poller = Arc::new(poller::Poller::new());
    let (cfg, cfg_err) = Config::load();
    let layout = Layout::compute(&cfg, 0);
    let tracker = Arc::new(mouse::MouseTracker::new(&layout));
    let setup_poller = poller.clone();
    let setup_tracker = tracker.clone();
    let exit_poller = poller.clone();
    let exit_tracker = tracker.clone();
    let exit_code = tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .manage(AppState {
            updates: Mutex::new(()),
            revision: AtomicU64::new(0),
            poller: poller.clone(),
            mouse: tracker.clone(),
            motion: Mutex::new(motion::MotionState::docked(&layout)),
            config: Mutex::new(cfg),
            layout: Mutex::new(layout),
            snapshots: Mutex::new(Vec::new()),
            config_error: Mutex::new(cfg_err),
            window_pos: Mutex::new((0.0, 0.0)),
            menu_open: std::sync::atomic::AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            get_usage,
            set_menu_open,
            quit,
            reload,
            refresh,
            dock_nearest,
            motion_preferences
        ])
        .setup(move |app| {
            // No Dock icon, no app menu. Set before showing the panel, or macOS
            // briefly bounces an icon into the Dock.
            app.set_activation_policy(ActivationPolicy::Accessory);

            let window = app
                .get_webview_window("main")
                .ok_or("window `main` is missing from tauri.conf.json")?;

            seed_vendor_icons();
            panel::install(&window)?;
            sync_window(app.handle(), true);

            let handle = app.handle().clone();
            let exit_handle = handle.clone();
            std::thread::spawn(move || {
                while process::running() {
                    if process::terminated() {
                        exit_handle.exit(0);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });

            let poller = setup_poller.clone();
            let tracker = setup_tracker.clone();

            observers::install(handle.clone(), poller.clone());
            mouse::spawn(handle.clone(), tracker);
            poller::spawn(handle, poller);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Cooldown Bar")
        .run_return(move |_app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                exit_poller.stop();
                exit_tracker.stop();
            }
        });
    poller.stop();
    tracker.stop();
    poller.join();
    tracker.join();
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// A process-held lock prevents duplicate windows, polls, and config writers.
fn single_instance() -> std::io::Result<Option<Vec<std::fs::File>>> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let directories = env::app_dirs();
    if directories.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no home directory",
        ));
    }
    let mut locks = Vec::with_capacity(directories.len());
    for (index, directory) in directories.into_iter().enumerate() {
        if index > 0 && !directory.exists() {
            continue;
        }
        std::fs::create_dir_all(&directory)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(directory.join("instance.lock"))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            locks.push(file);
            continue;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(None);
        }
        return Err(error);
    }
    Ok(Some(locks))
}

/// Failed reloads retain the working config; successful ones publish all UI data.
pub fn reload_config(app: &AppHandle) {
    let (cfg, err) = Config::load();
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let (next, changed) = {
        let _guard = state.updates.lock().expect("state update lock");
        if err.is_none() {
            *state.config.lock().expect("config lock") = cfg.clone();
            state
                .snapshots
                .lock()
                .expect("snapshots lock")
                .retain(|s| match s.id.as_str() {
                    "claude" => cfg.show_claude,
                    "codex" => cfg.show_codex,
                    "custom" => cfg
                        .custom_command
                        .as_ref()
                        .is_some_and(|c| !c.trim().is_empty()),
                    _ => false,
                });
            // A new custom command may belong to an entirely different account.
            state
                .snapshots
                .lock()
                .expect("snapshots lock")
                .retain(|s| s.id != "custom");
            state.poller.reconfigure();
        }
        *state.config_error.lock().expect("config error lock") = err;
        let count = state.snapshots.lock().expect("snapshots lock").len();
        let changed = update_layout(&state, count);
        state.revision.fetch_add(1, Ordering::SeqCst);
        (bootstrap(&state), changed)
    };
    if state.mouse.is_undocked() {
        state.mouse.request_dock(true);
    }
    sync_window(app, changed);
    reposition(app);
    let _ = app.emit("state://update", next);
}

pub fn publish_snapshot(
    app: &AppHandle,
    id: &str,
    generation: u64,
    pause_epoch: u64,
    reading: Option<ProviderSnapshot>,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let (next, changed) = {
        let _guard = state.updates.lock().expect("state update lock");
        if !state.poller.can_publish(generation, pause_epoch) {
            return;
        }
        let ttl = state
            .config
            .lock()
            .expect("config lock")
            .stale_after_seconds;
        let count = {
            let mut snapshots = state.snapshots.lock().expect("snapshots lock");
            if let Some(reading) = reading {
                let previous = snapshots.iter().find(|s| s.id == id);
                let merged = providers::merge_snapshot(previous, reading, ttl);
                snapshots.retain(|s| s.id != id);
                snapshots.push(merged);
                snapshots.sort_by_key(|s| match s.id.as_str() {
                    "claude" => 0,
                    "codex" => 1,
                    _ => 2,
                });
            } else {
                snapshots.retain(|s| s.id != id);
            }
            snapshots.len()
        };
        let changed = update_layout(&state, count);
        state.revision.fetch_add(1, Ordering::SeqCst);
        (bootstrap(&state), changed)
    };
    sync_window(app, changed);
    let _ = app.emit("state://update", next);
}
fn update_layout(state: &AppState, item_count: usize) -> bool {
    let cfg = state.config.lock().expect("config lock");
    let next = Layout::compute(&cfg, item_count);
    let mut layout = state.layout.lock().expect("layout lock");
    let changed = *layout != next;
    *layout = next;
    changed
}
fn sync_window(app: &AppHandle, changed: bool) {
    if !changed {
        return;
    }
    let state = app.state::<AppState>();
    let next = *state.layout.lock().expect("layout lock");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(LogicalSize::new(next.window_width, next.window_height));
    }
    reposition(app);
}
pub fn relayout(app: &AppHandle, item_count: usize) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let (next, changed) = {
        let _guard = state.updates.lock().expect("state update lock");
        let changed = update_layout(&state, item_count);
        state.revision.fetch_add(1, Ordering::SeqCst);
        (bootstrap(&state), changed)
    };
    sync_window(app, changed);
    let _ = app.emit("state://update", next);
}

/// Park the panel against the configured screen edge.
///
/// The whole computation happens on the main thread. `main_screen_geometry()`
/// needs a `MainThreadMarker` and silently returns a placeholder screen without
/// one, so doing half the maths on a worker thread mixes a real screen with a
/// fake one and lands the window in the wrong place on both axes.
pub fn reposition(app: &AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let (layout, cfg) = {
            let _guard = state.updates.lock().expect("state update lock");
            let cfg = state.config.lock().expect("config lock").clone();
            let layout = *state.layout.lock().expect("layout lock");
            (layout, cfg)
        };
        let geo = screen::main_screen_geometry();

        // The rail stays centred in the webview on both sides. The unused
        // transparent half may extend off screen; hit-testing ignores it.
        if state.mouse.is_undocked() {
            return;
        }
        // `topOffset` names the top of the bar; the window starts above it by
        // the slack the bubble needs.
        let bar_top = cfg
            .resolved_top_offset(geo.menu_bar_height)
            .clamp(0.0, (geo.height() - layout.bar_height).max(0.0));
        let pos = motion::dock_position(&layout, &geo, &cfg, cfg.edge, bar_top);
        apply_window_pos(&app, &state, layout, pos);
    });
}

/// Main-thread half of the move. The only writer of `AppState::window_pos`, so
/// hit-testing and the actual window can never disagree.
pub(crate) fn apply_window_pos(app: &AppHandle, state: &AppState, layout: Layout, pos: (f64, f64)) {
    if let Ok(mut slot) = state.window_pos.lock() {
        *slot = pos;
    }
    let Ok(panel) = app.get_webview_panel("main") else {
        return;
    };
    let geo = screen::main_screen_geometry();
    // The one flip: top-left y -> AppKit bottom-left origin.
    let appkit_y = geo.min_y() + geo.height() - pos.1 - layout.window_height;
    screen::set_panel_frame(
        panel.as_panel(),
        pos.0,
        appkit_y,
        layout.window_width,
        layout.window_height,
    );
}

/// Keeps the generic bound honest for callers that hold a non-Wry runtime.
#[allow(dead_code)]
fn _assert_runtime<R: Runtime>() {}
