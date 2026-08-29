//! Screen geometry shim.
//!
//! Tauri exposes monitor size and scale factor but nothing about the menu bar or
//! the notch, and its `set_position` anchors to a different origin than the one
//! we need. Everything geometric therefore goes through this module.
//!
//! # The coordinate flip, stated once
//!
//! AppKit's screen space has its **origin at the bottom-left** and y grows
//! upward. Every UI toolkit convention (and Tauri's own `set_position`) uses a
//! **top-left origin** with y growing downward. `panel_origin_for` below is the
//! only place in this codebase that converts between the two, and it converts in
//! exactly one direction: from a desired top-left position to the bottom-left
//! origin AppKit wants for `setFrameOrigin:`.
//!
//! Concretely, for a node of height `h` whose top edge should sit `top_offset`
//! points below the top of the screen:
//!
//! ```text
//! appkit_y = frame.min_y + frame.height - top_offset - h
//! ```
//!
//! With `top_offset == 0` the node's top edge lands on `frame.max_y`, the true
//! top of the display — above the menu bar, which is what the design wants.

use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::sel;
use objc2_app_kit::{NSPanel, NSScreen};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};

/// A snapshot of the main display, in points (not pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenGeometry {
    /// `(x, y, w, h)` of `NSScreen::frame`, in AppKit bottom-left coordinates.
    pub frame: (f64, f64, f64, f64),
    /// `frame.max_y - visible_frame.max_y`. Includes the notch region on
    /// machines that have one, because AppKit grows the menu bar to cover it.
    pub menu_bar_height: f64,
    /// `safeAreaInsets.top`. Non-zero only on notched displays.
    pub notch_height: f64,
    pub scale_factor: f64,
}

impl ScreenGeometry {
    /// Fallback used when AppKit cannot hand us a main screen (headless CI, or a
    /// call from a non-main thread). Deliberately conservative.
    fn fallback() -> Self {
        Self {
            frame: (0.0, 0.0, 1440.0, 900.0),
            menu_bar_height: 24.0,
            notch_height: 0.0,
            scale_factor: 2.0,
        }
    }

    pub fn max_x(&self) -> f64 {
        self.frame.0 + self.frame.2
    }

    pub fn min_y(&self) -> f64 {
        self.frame.1
    }

    pub fn height(&self) -> f64 {
        self.frame.3
    }
}

/// Read the geometry of the screen that carries the menu bar.
///
/// **Not** `NSScreen::mainScreen`. AppKit defines "main" as *the screen holding
/// the window with keyboard focus*, which for this app is whichever screen the
/// user last clicked on — it changes under us and has nothing to do with where
/// the menu bar is. On a two-display setup that made the bar jump to the
/// external monitor whenever focus moved there.
///
/// `NSScreen::screens[0]` is the display whose origin is `(0,0)`: the primary,
/// the one with the menu bar. That is the stable anchor the design wants.
pub fn main_screen_geometry() -> ScreenGeometry {
    let Some(mtm) = MainThreadMarker::new() else {
        return ScreenGeometry::fallback();
    };
    let screen = NSScreen::screens(mtm)
        .firstObject()
        .or_else(|| NSScreen::mainScreen(mtm));
    let Some(screen) = screen else {
        return ScreenGeometry::fallback();
    };

    let frame = screen.frame();
    let visible = screen.visibleFrame();
    let insets = screen.safeAreaInsets();

    // `visibleFrame` is inset by the menu bar at the top and, when the Dock is on
    // the bottom, by the Dock. We only care about the top delta.
    let menu_bar_height =
        (frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height);

    ScreenGeometry {
        frame: (
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        ),
        menu_bar_height: menu_bar_height.max(0.0),
        notch_height: insets.top.max(0.0),
        scale_factor: screen.backingScaleFactor(),
    }
}

/// `NSScreen::auxiliaryTopRightArea` — the usable strip to the right of the
/// notch, when there is one.
///
/// AppKit returns a zero rect rather than nil on machines without a notch, so we
/// map that to `None` instead of handing back a meaningless rectangle.
#[allow(dead_code)] // part of the documented geometry API; see README
pub fn auxiliary_top_right_area() -> Option<(f64, f64, f64, f64)> {
    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let r = screen.auxiliaryTopRightArea();
    if r.size.width <= 0.0 || r.size.height <= 0.0 {
        return None;
    }
    Some((r.origin.x, r.origin.y, r.size.width, r.size.height))
}

/// Move and resize the panel directly through AppKit.
///
/// We bypass `WebviewWindow::set_position` on purpose: Tauri anchors y to the
/// visible frame, so asking it for `y = 0` lands the window *below* the menu bar
/// instead of on top of it. Setting the frame here keeps the conversion in one
/// place (see the module comment) and makes `topOffset: 0` mean what it says.
pub fn set_panel_frame(panel: &NSPanel, x: f64, y: f64, width: f64, height: f64) {
    let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
    let current = panel.frame();
    // Moving must not trigger a resize/layout/display pass on every drag tick.
    if current.size.width != width || current.size.height != height {
        panel.setFrame_display(frame, false);
    }
    // `setFrame:display:` runs the result through `constrainFrameRect:toScreen:`,
    // which refuses to let a window cover the menu bar / notch and quietly slides
    // it down. `setFrameOrigin:` is not constrained, so re-assert the origin
    // after the size has been applied.
    panel.setFrameOrigin(NSPoint::new(x, y));

    #[cfg(debug_assertions)]
    {
        let got = panel.frame();
        // AppKit rounds to device pixels; only report a real disagreement.
        if (got.origin.x - x).abs() > 1.5 || (got.origin.y - y).abs() > 1.5 {
            eprintln!(
                "[cooldown-bar] frame asked=({x:.0},{y:.0} {width:.0}x{height:.0}) got=({:.0},{:.0} {:.0}x{:.0})",
                got.origin.x, got.origin.y, got.size.width, got.size.height
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Letting the panel cover the menu bar
// ---------------------------------------------------------------------------

/// Replacement for `-[NSWindow constrainFrameRect:toScreen:]` that returns the
/// requested rectangle untouched.
extern "C" fn unconstrained_frame_rect(
    _this: *mut AnyObject,
    _cmd: Sel,
    rect: NSRect,
    _screen: *mut AnyObject,
) -> NSRect {
    rect
}

/// Objective-C type encoding for
/// `NSRect (*)(id self, SEL _cmd, NSRect rect, id screen)` on 64-bit.
const CONSTRAIN_TYPES: &[u8] =
    b"{CGRect={CGPoint=dd}{CGSize=dd}}@:{CGRect={CGPoint=dd}{CGSize=dd}}@\0";

/// Stop AppKit from sliding the panel out from under the menu bar.
///
/// `setFrame:display:` runs its argument through `constrainFrameRect:toScreen:`,
/// whose stock implementation clamps a window's top edge to `visibleFrame` —
/// i.e. just below the menu bar. With `topOffset: 0` the bar is supposed to sit
/// at the *true* top of the screen, so the request came back 33pt lower than
/// asked for, every time.
///
/// Clearing `NSWindowStyleMask::Titled` is enough for a plain `NSPanel`, but not
/// for the window wry hands us, so the method is overridden outright. Installed
/// once on the `NotchPanel` subclass, which is ours alone — no other window in
/// the process is affected.
pub fn allow_covering_menu_bar(panel: &NSPanel) -> bool {
    let cls: *const AnyClass = panel.class();
    unsafe {
        let imp: objc2::runtime::Imp = std::mem::transmute(
            unconstrained_frame_rect
                as extern "C" fn(*mut AnyObject, Sel, NSRect, *mut AnyObject) -> NSRect,
        );
        let added: Bool = objc2::ffi::class_addMethod(
            cls as *mut AnyClass,
            sel!(constrainFrameRect:toScreen:),
            imp,
            CONSTRAIN_TYPES.as_ptr().cast(),
        );
        added.as_bool()
    }
}
