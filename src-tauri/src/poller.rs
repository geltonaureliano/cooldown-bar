//! One owned worker per provider: slow or failed queries never block another.
use crate::config::Config;
use crate::providers::{
    claude::ClaudeProvider, codex::CodexProvider, custom::CustomProvider, Provider,
    ProviderSnapshot,
};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy)]
pub enum PauseReason {
    Sleep = 1,
    Floating = 2,
}

pub struct Poller {
    pauses: AtomicU8,
    pub pause_epoch: AtomicU64,
    running: AtomicBool,
    pub generation: AtomicU64,
    refresh: AtomicU64,
    signal: (Mutex<()>, Condvar),
    workers: Mutex<Vec<JoinHandle<()>>>,
}
impl Poller {
    pub fn new() -> Self {
        Self {
            pauses: AtomicU8::new(0),
            pause_epoch: AtomicU64::new(0),
            running: AtomicBool::new(true),
            generation: AtomicU64::new(0),
            refresh: AtomicU64::new(0),
            signal: (Mutex::new(()), Condvar::new()),
            workers: Mutex::new(Vec::new()),
        }
    }
    pub fn is_paused(&self) -> bool {
        self.pauses.load(Ordering::SeqCst) != 0
    }
    pub fn can_publish(&self, generation: u64, epoch: u64) -> bool {
        !self.is_paused()
            && self.generation.load(Ordering::SeqCst) == generation
            && self.pause_epoch.load(Ordering::SeqCst) == epoch
    }
    pub fn set_paused(&self, reason: PauseReason, paused: bool) {
        let bit = reason as u8;
        let previous = if paused {
            self.pauses.fetch_or(bit, Ordering::SeqCst)
        } else {
            self.pauses.fetch_and(!bit, Ordering::SeqCst)
        };
        let next = if paused {
            previous | bit
        } else {
            previous & !bit
        };
        if previous == next {
            return;
        }
        // An in-flight query cannot reappear after a quick detach + reattach.
        self.pause_epoch.fetch_add(1, Ordering::SeqCst);
        if next == 0 {
            self.request_refresh();
        }
        self.signal.1.notify_all();
    }
    pub fn request_refresh(&self) {
        self.refresh.fetch_add(1, Ordering::SeqCst);
        self.signal.1.notify_all();
    }
    pub fn reconfigure(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.request_refresh();
    }
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        crate::process::shutdown();
        self.signal.1.notify_all();
    }
    pub fn join(&self) {
        if let Ok(mut workers) = self.workers.lock() {
            for worker in workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
    fn wait(&self) {
        if let Ok(guard) = self.signal.0.lock() {
            let _ = self
                .signal
                .1
                .wait_timeout(guard, Duration::from_millis(250));
        }
    }
}
impl Default for Poller {
    fn default() -> Self {
        Self::new()
    }
}

fn provider_for(id: &str, cfg: &Config) -> Option<Box<dyn Provider>> {
    match id {
        "claude" if cfg.show_claude => Some(Box::new(ClaudeProvider::new(
            "Claude Usage",
            cfg.stale_after_seconds,
        ))),
        "codex" if cfg.show_codex => Some(Box::new(CodexProvider::new(
            "Codex Usage",
            cfg.stale_after_seconds,
        ))),
        "custom" => cfg
            .custom_command
            .as_ref()
            .filter(|c| !c.trim().is_empty())
            .map(|c| {
                Box::new(CustomProvider::new(cfg.custom_title.clone(), c)) as Box<dyn Provider>
            }),
        _ => None,
    }
}
fn retry_delay(base: u64, failures: u32) -> Duration {
    Duration::from_secs(
        base.saturating_mul(1u64 << failures.min(5))
            .min(base.max(120)),
    )
}

pub fn spawn(app: AppHandle, poller: Arc<Poller>) {
    for id in ["claude", "codex", "custom"] {
        let app = app.clone();
        let p = poller.clone();
        let worker = std::thread::spawn(move || worker(&app, &p, id));
        poller.workers.lock().expect("workers lock").push(worker);
    }
}
fn worker(app: &AppHandle, poller: &Poller, id: &'static str) {
    let mut generation = u64::MAX;
    let mut refresh = 0;
    let mut provider: Option<Box<dyn Provider>> = None;
    let mut cfg = Config::default();
    let mut due = Instant::now();
    let mut earliest_manual = Instant::now();
    let mut failures: u32 = 0;
    let mut disconnected_for_pause = false;
    while poller.running.load(Ordering::SeqCst) {
        if poller.is_paused() {
            if !disconnected_for_pause {
                if let Some(p) = provider.as_mut() {
                    p.disconnect();
                }
                disconnected_for_pause = true;
            }
            poller.wait();
            continue;
        }
        disconnected_for_pause = false;
        let next_generation = poller.generation.load(Ordering::SeqCst);
        if generation != next_generation {
            let state = app.state::<crate::AppState>();
            let _guard = state.updates.lock().expect("state update lock");
            generation = poller.generation.load(Ordering::SeqCst);
            cfg = state.config.lock().expect("config lock").clone();
            provider = provider_for(id, &cfg);
            failures = 0;
            due = Instant::now();
        }
        let next_refresh = poller.refresh.load(Ordering::SeqCst);
        if refresh != next_refresh && Instant::now() >= earliest_manual {
            refresh = next_refresh;
            due = Instant::now();
        }
        let pause_epoch = poller.pause_epoch.load(Ordering::SeqCst);
        if poller.is_paused() {
            continue;
        }
        if let Some(p) = provider.as_mut() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if Instant::now() >= due {
                    earliest_manual = Instant::now() + Duration::from_secs(2);
                    let next = if p.detect() { Some(p.snapshot()) } else { None };
                    let failed = next
                        .as_ref()
                        .is_some_and(|s| s.error.is_some() || s.primary.is_none());
                    failures = if failed {
                        failures.saturating_add(1)
                    } else {
                        0
                    };
                    due = Instant::now() + retry_delay(cfg.refresh_seconds, failures);
                    Some(next)
                } else {
                    p.poll_event().map(|reading| {
                        if reading.invalidate_previous {
                            due = Instant::now();
                        } else if reading.error.is_some() {
                            failures = failures.saturating_add(1);
                            due = Instant::now() + retry_delay(cfg.refresh_seconds, failures);
                        } else {
                            failures = 0;
                            due =
                                due.min(Instant::now() + Duration::from_secs(cfg.refresh_seconds));
                        }
                        Some(reading)
                    })
                }
            }));
            match result {
                Ok(Some(next)) if poller.running.load(Ordering::SeqCst) && !poller.is_paused() => {
                    crate::publish_snapshot(app, id, generation, pause_epoch, next)
                }
                Ok(_) => {}
                Err(_) => {
                    let next = ProviderSnapshot::errored(
                        id,
                        p.title(),
                        "Usage worker recovered from an unexpected error.",
                    );
                    p.disconnect();
                    failures = failures.saturating_add(1);
                    due = Instant::now() + retry_delay(cfg.refresh_seconds, failures);
                    crate::publish_snapshot(app, id, generation, pause_epoch, Some(next));
                }
            }
        }
        poller.wait();
    }
}
/// Signals workers only; safe on AppKit's main thread and coalesced per provider.
pub fn refresh_now(app: &AppHandle) {
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.poller.request_refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_back_off_without_overflow_or_exceeding_the_cap() {
        assert_eq!(retry_delay(10, 0), Duration::from_secs(10));
        assert_eq!(retry_delay(10, 1), Duration::from_secs(20));
        assert_eq!(retry_delay(10, u32::MAX), Duration::from_secs(120));
        assert_eq!(retry_delay(3600, 1), Duration::from_secs(3600));
    }

    #[test]
    fn pause_reasons_compose_and_invalidate_inflight_results() {
        let poller = Poller::new();
        let generation = poller.generation.load(Ordering::SeqCst);
        let epoch = poller.pause_epoch.load(Ordering::SeqCst);
        assert!(poller.can_publish(generation, epoch));
        poller.set_paused(PauseReason::Floating, true);
        assert!(!poller.can_publish(generation, epoch));
        let floating_epoch = poller.pause_epoch.load(Ordering::SeqCst);
        poller.set_paused(PauseReason::Sleep, true);
        poller.set_paused(PauseReason::Floating, false);
        assert!(poller.is_paused());
        assert!(!poller.can_publish(generation, floating_epoch));
        poller.set_paused(PauseReason::Sleep, false);
        assert!(!poller.is_paused());
        assert!(!poller.can_publish(generation, epoch));
        assert!(poller.can_publish(generation, poller.pause_epoch.load(Ordering::SeqCst)));
    }

    #[test]
    fn repeated_pause_value_does_not_advance_the_epoch() {
        let poller = Poller::new();
        poller.set_paused(PauseReason::Floating, true);
        let epoch = poller.pause_epoch.load(Ordering::SeqCst);
        poller.set_paused(PauseReason::Floating, true);
        assert_eq!(poller.pause_epoch.load(Ordering::SeqCst), epoch);
    }
}
