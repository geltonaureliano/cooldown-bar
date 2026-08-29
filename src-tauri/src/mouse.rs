//! Native hit-testing and a single coalesced cursor poll. Transparent webview
//! regions must remain click-through: AppKit hit-tests windows, not CSS pixels.
//! No global monitor or Accessibility permission is needed for NSEvent polling.
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use objc2_app_kit::NSEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{Config, Edge};
use crate::layout::Layout;
use crate::motion::{Input, Motion, Phase};
use crate::poller::PauseReason;
use crate::screen::main_screen_geometry;

const IDLE_POLL: Duration = Duration::from_millis(33);
const MOTION_POLL: Duration = Duration::from_micros(16_667);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverState {
    pub index: i64,
    pub center_y: f64,
}

pub struct MouseTracker {
    hovered: AtomicI64,
    hover_center: AtomicU64,
    hover_tick: AtomicU64,
    pressed: AtomicBool,
    pending_tick: AtomicBool,
    accepting: AtomicBool,
    undocked: AtomicBool,
    animating: AtomicBool,
    running: AtomicBool,
    reduced_motion: AtomicBool,
    dock_request: AtomicU8,
    motion: Mutex<Motion>,
    last_tick: Mutex<Instant>,
    save_position: mpsc::Sender<Option<(Edge, f64)>>,
    writer: Mutex<Option<JoinHandle<()>>>,
}

impl MouseTracker {
    pub fn new(layout: &Layout) -> Self {
        let (save_position, receiver) = mpsc::channel::<Option<(Edge, f64)>>();
        // One ordered writer, never an fsync or a new thread on a motion frame.
        let writer = std::thread::spawn(move || {
            while let Ok(Some(mut position)) = receiver.recv() {
                let mut stop = false;
                while let Ok(next) = receiver.try_recv() {
                    match next {
                        Some(next) => position = next,
                        None => {
                            stop = true;
                            break;
                        }
                    }
                }
                if let Err(e) = Config::persist_position(position.0, position.1) {
                    eprintln!("[cooldown-bar] could not save position: {e}");
                }
                if stop {
                    break;
                }
            }
        });
        Self {
            hovered: AtomicI64::new(-1),
            hover_center: AtomicU64::new(0),
            hover_tick: AtomicU64::new(0),
            pressed: AtomicBool::new(false),
            pending_tick: AtomicBool::new(false),
            accepting: AtomicBool::new(false),
            undocked: AtomicBool::new(false),
            animating: AtomicBool::new(false),
            running: AtomicBool::new(true),
            reduced_motion: AtomicBool::new(false),
            dock_request: AtomicU8::new(0),
            motion: Mutex::new(Motion::new(layout)),
            last_tick: Mutex::new(Instant::now()),
            save_position,
            writer: Mutex::new(Some(writer)),
        }
    }
    pub fn is_undocked(&self) -> bool {
        self.undocked.load(Ordering::Relaxed)
    }
    pub fn request_dock(&self, configured_edge: bool) {
        self.dock_request
            .store(if configured_edge { 2 } else { 1 }, Ordering::Relaxed);
    }
    pub fn set_reduced_motion(&self, reduced: bool) {
        self.reduced_motion.store(reduced, Ordering::Relaxed);
    }
    pub fn stop(&self) {
        if self.running.swap(false, Ordering::Relaxed) {
            let _ = self.save_position.send(None);
        }
    }
    pub fn join(&self) {
        if let Some(writer) = self.writer.lock().ok().and_then(|mut w| w.take()) {
            let _ = writer.join();
        }
    }
}

pub fn spawn(app: AppHandle, tracker: Arc<MouseTracker>) {
    std::thread::spawn(move || {
        while tracker.running.load(Ordering::Relaxed) {
            if !tracker.pending_tick.swap(true, Ordering::Relaxed) {
                let app = app.clone();
                let t = tracker.clone();
                // At most one queued main-thread tick, even if AppKit is busy.
                if app
                    .clone()
                    .run_on_main_thread(move || {
                        if t.running.load(Ordering::Relaxed) {
                            tick(&app, &t);
                        }
                        t.pending_tick.store(false, Ordering::Relaxed);
                    })
                    .is_err()
                {
                    tracker.pending_tick.store(false, Ordering::Relaxed);
                }
            }
            std::thread::sleep(if tracker.animating.load(Ordering::Relaxed) {
                MOTION_POLL
            } else {
                IDLE_POLL
            });
        }
    });
}

fn tick(app: &AppHandle, tracker: &MouseTracker) {
    let Some(state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let Ok(mut motion) = tracker.motion.lock() else {
        return;
    };
    let p = NSEvent::mouseLocation();
    let geo = main_screen_geometry();
    let cursor = (p.x, geo.min_y() + geo.height() - p.y);
    let pressed = NSEvent::pressedMouseButtons() & 1 != 0;
    let just_pressed = pressed && !tracker.pressed.swap(pressed, Ordering::Relaxed);
    // swap must also run when the button is up (short-circuiting would stick it).
    if !pressed {
        tracker.pressed.store(false, Ordering::Relaxed);
    }
    let now = Instant::now();
    let dt = {
        let mut previous = tracker.last_tick.lock().expect("tick clock");
        let dt = now.duration_since(*previous).as_secs_f64();
        *previous = now;
        dt
    };
    let window_pos = *state.window_pos.lock().expect("window position lock");
    let mut menu_open = state.menu_open.load(Ordering::Relaxed);
    let (layout, inside, effects, changed) = {
        // Pause transitions share the publication gate: no in-flight reading can
        // slip through after detachment, even during a fast detach + reattach.
        let _guard = state.updates.lock().expect("state update lock");
        let mut layout = *state.layout.lock().expect("layout lock");
        let cfg = state.config.lock().expect("config lock").clone();
        let inside = motion.contains(&layout, cursor, window_pos);
        let outside_window = cursor.0 < window_pos.0
            || cursor.0 > window_pos.0 + layout.window_width
            || cursor.1 < window_pos.1
            || cursor.1 > window_pos.1 + layout.window_height;
        if menu_open && just_pressed && outside_window {
            state.menu_open.store(false, Ordering::Relaxed);
            menu_open = false;
            let _ = app.emit("menu://close", ());
        }
        let force_edge = match tracker.dock_request.swap(0, Ordering::Relaxed) {
            1 => Some(motion.nearest_edge(&layout, &geo, window_pos)),
            2 => Some(cfg.edge),
            _ => None,
        };
        let effects = motion.step(
            Input {
                cursor,
                position: window_pos,
                pressed,
                just_pressed,
                inside: inside && !menu_open,
                dt,
                reduced_motion: tracker.reduced_motion.load(Ordering::Relaxed),
                force_edge,
            },
            &layout,
            &geo,
            &cfg,
        );
        if let Some(paused) = effects.pause {
            state.poller.set_paused(PauseReason::Floating, paused);
        }
        if let Some(edge) = effects.new_edge {
            // Size and rail centre stay constant; only the silhouette flips.
            state.config.lock().expect("config lock").edge = edge;
            layout.edge = edge;
            state.layout.lock().expect("layout lock").edge = edge;
        }
        if let Some((edge, top)) = effects.docked {
            let mut cfg = state.config.lock().expect("config lock");
            cfg.edge = edge;
            cfg.top_offset = Some(top);
            let _ = tracker.save_position.send(Some((edge, top)));
        }
        tracker
            .undocked
            .store(motion.view.paused(), Ordering::Relaxed);
        tracker
            .animating
            .store(motion.animating(), Ordering::Relaxed);
        let mut previous = state.motion.lock().expect("motion state lock");
        let changed = *previous != motion.view;
        if changed {
            motion.view.revision = previous.revision + 1;
            *previous = motion.view;
        }
        (layout, inside, effects, changed)
    };
    if let Some(pos) = effects.position {
        crate::apply_window_pos(app, &state, layout, pos);
    }
    if changed {
        let _ = app.emit("motion://update", motion.view);
    }

    let want_events = inside || motion.view.phase == Phase::Dragging || menu_open;
    if want_events != tracker.accepting.swap(want_events, Ordering::Relaxed) {
        set_click_through(app, !want_events);
    }
    let index = if inside && motion.view.phase == Phase::Docked && !menu_open {
        ring_index_at(
            &layout,
            cursor.1 - effects.position.unwrap_or(window_pos).1 - layout.bar_offset_y,
        )
    } else {
        -1
    };
    let center_y = if index >= 0 {
        layout.ring_center_y(index as usize)
    } else {
        0.0
    };
    let old_index = tracker.hovered.swap(index, Ordering::Relaxed);
    let old_center = tracker
        .hover_center
        .swap(center_y.to_bits(), Ordering::Relaxed);
    let heartbeat = tracker.hover_tick.fetch_add(1, Ordering::Relaxed) % 30 == 0;
    if index != old_index || center_y.to_bits() != old_center || heartbeat {
        let _ = app.emit("hover://update", HoverState { index, center_y });
    }
}

/// Cover both ring and label, but never the empty gaps between providers.
fn ring_index_at(layout: &Layout, y: f64) -> i64 {
    let item_height = layout.ring_diameter + layout.label_gap + layout.label_height;
    let stride = item_height + layout.item_gap;
    let rel = y - layout.pad_y;
    if rel < 0.0 {
        return -1;
    }
    let idx = (rel / stride).floor();
    if idx as usize >= layout.item_count || rel - idx * stride > item_height {
        -1
    } else {
        idx as i64
    }
}

fn set_click_through(app: &AppHandle, ignore: bool) {
    use tauri_nspanel::ManagerExt;
    if let Ok(panel) = app.get_webview_panel("main") {
        panel.set_ignores_mouse_events(ignore);
    }
}
