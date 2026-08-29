//! Converts the Tauri window into a non-activating `NSPanel`.
//!
//! Two independent AppKit mechanisms decide whether a window steals focus:
//!
//! 1. `NSWindowStyleMask::NonactivatingPanel` — tells AppKit not to activate the
//!    *application* when the panel is clicked.
//! 2. `canBecomeKeyWindow` / `canBecomeMainWindow` — decide whether this *window*
//!    takes key/main status away from whoever currently has it.
//!
//! Setting only the style mask is not enough: a panel that still returns `true`
//! from `canBecomeKeyWindow` will pull the key state off the terminal the moment
//! the webview asks for first responder. So we subclass and hard-code both to
//! `false`.

use tauri::{Manager as _, WebviewWindow};
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt as _,
};

tauri_panel! {
    panel!(NotchPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false
        }
    })
}

/// Promote `window` to an `NSPanel` and apply every behaviour the bar depends on.
pub fn install(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel::<NotchPanel>()?;

    // Borderless + non-activating. `StyleMask::empty()` (not `::new()`) so we do
    // not inherit Titled/Closable/Miniaturizable/Resizable.
    //
    // Dropping `Titled` is not cosmetic: AppKit runs a titled window's frame
    // through `constrainFrameRect:toScreen:`, which refuses to let it cover the
    // menu bar and silently slides it down by the menu bar height. With
    // `topOffset: 0` the bar has to sit at the true top of the screen, so the
    // mask must be exactly this.
    panel.set_style_mask(StyleMask::empty().nonactivating_panel().value());

    // Float above ordinary windows, including fullscreen ones.
    //
    // Note this does *not* put the bar above the menu bar: on current macOS the
    // menu bar is composited by the WindowServer above every normal window
    // level — verified empirically up to `NSScreenSaverWindowLevel` (1000),
    // where the clock still drew straight through the bar. That is why the
    // default `topOffset` is the menu bar height rather than 0; see config.rs.
    panel.set_level(PanelLevel::Status.value());

    // - can_join_all_spaces: follow the user across Spaces instead of living on one
    // - stationary: do not slide around during Exposé / Mission Control
    // - full_screen_auxiliary: stay visible on top of a fullscreen app. Without
    //   this flag the bar vanishes the moment an editor goes fullscreen.
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .value(),
    );

    // A panel hides itself when the owning app deactivates unless told otherwise.
    // Since the app is never active (Accessory + non-activating), that default
    // would keep the bar permanently hidden.
    panel.set_hides_on_deactivate(false);
    panel.set_floating_panel(true);

    panel.set_has_shadow(false);
    panel.set_opaque(false);
    panel.set_transparent(true);

    // The webview reports mouse-move events on its own; we only need AppKit to
    // keep delivering them while the app is in the background.
    panel.set_accepts_mouse_moved_events(true);
    panel.set_movable_by_window_background(false);
    panel.set_released_when_closed(false);

    // Must happen before the first frame is set.
    let _unconstrained = crate::screen::allow_covering_menu_bar(panel.as_panel());

    panel.set_ignores_mouse_events(true);
    panel.show();

    // Phase 1 diagnostics: prove both focus mechanisms are off, not just the mask.
    #[cfg(debug_assertions)]
    {
        eprintln!(
            "[notchusage] panel class={:?} can_become_key={} can_become_main={} hides_on_deactivate={} visible={}",
            panel.as_panel().class().name(),
            panel.can_become_key_window(),
            panel.can_become_main_window(),
            panel.hides_on_deactivate(),
            panel.is_visible(),
        );
        eprintln!(
            "[notchusage] style_mask={:?} unconstrained={_unconstrained}",
            panel.as_panel().styleMask()
        );
    }

    Ok(())
}
