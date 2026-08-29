//! AppKit notification observers.
//!
//! Two things must be reacted to rather than polled:
//!
//! * **Screen parameters changed** — a display was connected, the resolution
//!   changed, or the Dock/menu bar auto-hide setting was toggled. Any of those
//!   moves the top-right corner, so the panel has to be repositioned.
//! * **Sleep / wake** — polling while the machine is asleep is pointless, and
//!   resuming immediately on wake gives a fresh reading before the user looks.
//!
//! Observers are registered with block callbacks and intentionally leaked: they
//! must outlive this function and live for the whole process.

use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use objc2_app_kit::{
    NSApplicationDidChangeScreenParametersNotification, NSWorkspace,
    NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use tauri::AppHandle;

use crate::poller::{PauseReason, Poller};

pub fn install(app: AppHandle, poller: Arc<Poller>) {
    install_screen_observer(app.clone());
    install_power_observers(app, poller);
}

fn install_screen_observer(app: AppHandle) {
    let center = NSNotificationCenter::defaultCenter();
    let block = RcBlock::new(move |_n: NonNull<NSNotification>| {
        // Re-read geometry and move the panel. Provider count is unchanged, so
        // reuse whatever the last layout decided.
        crate::reposition(&app);
    });
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidChangeScreenParametersNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(token);
}

fn install_power_observers(app: AppHandle, poller: Arc<Poller>) {
    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();

    let p_sleep = poller.clone();
    let sleep_block = RcBlock::new(move |_n: NonNull<NSNotification>| {
        p_sleep.set_paused(PauseReason::Sleep, true);
    });
    let sleep_token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceWillSleepNotification),
            None,
            None,
            &sleep_block,
        )
    };
    std::mem::forget(sleep_token);

    let wake_block = RcBlock::new(move |_n: NonNull<NSNotification>| {
        poller.set_paused(PauseReason::Sleep, false);
        // Displays can come back in a different arrangement after sleep.
        crate::reposition(&app);
        crate::poller::refresh_now(&app);
    });
    let wake_token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            None,
            &wake_block,
        )
    };
    std::mem::forget(wake_token);
}
