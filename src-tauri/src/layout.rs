//! Layout maths, computed once in Rust so the window size and the CSS agree.
//!
//! If React computed the bar height independently the window would clip the last
//! ring whenever the two drifted apart.

use serde::Serialize;

use crate::config::{Config, Edge};

/// Clear gap between the bubble tip and the straight inner edge of the rail.
pub const GUTTER: f64 = 9.0;
/// Room reserved beside the node for the hover bubble.
pub const BUBBLE_WIDTH: f64 = 200.0;
pub const BUBBLE_TAIL: f64 = 25.0;
pub const BUBBLE_TAIL_HEIGHT: f64 = 46.0;
pub const BUBBLE_RADIUS: f64 = 16.0;
pub const BUBBLE_MARGIN: f64 = 10.0;
/// Transparent slack **above and below** the bar inside the window.
///
/// The bubble is centred on whichever ring is hovered. Without slack above the
/// bar, a bubble anchored to the first ring would want to start at a negative
/// offset and had to be clamped to the window's top edge — which is what made it
/// look stuck, with only the tail moving between rings. The window simply
/// extends past the bar on both sides instead; the surplus is transparent and
/// click-through.
const BUBBLE_SLACK: f64 = 130.0;
/// Label line under each ring.
const LABEL_HEIGHT: f64 = 16.0;
const LABEL_GAP: f64 = 8.0;
const MIN_PAD_Y: f64 = 15.0;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Width of the black bar itself.
    pub bar_width: f64,
    /// Bar width plus the fillet gutter — the visible node.
    pub node_width: f64,
    pub bar_height: f64,
    /// Full webview window, including the transparent bubble area.
    pub window_width: f64,
    pub window_height: f64,
    /// Distance from the top of the window down to the top of the bar.
    pub bar_offset_y: f64,
    pub concave_radius: f64,
    pub ring_diameter: f64,
    pub ring_line_width: f64,
    pub item_gap: f64,
    pub pad_y: f64,
    pub label_height: f64,
    pub label_gap: f64,
    pub bubble_width: f64,
    pub bubble_tail_width: f64,
    pub bubble_tail_height: f64,
    pub bubble_radius: f64,
    pub item_count: usize,
    /// Which side the bar clings to. The frontend mirrors the fillets and opens
    /// the bubble on the opposite side when this is `left`.
    pub edge: Edge,
}

impl Layout {
    pub fn compute(cfg: &Config, item_count: usize) -> Self {
        let n = item_count.max(1) as f64;
        let item_height = cfg.ring_diameter + LABEL_GAP + LABEL_HEIGHT;
        // Place the end rings just inside the shoulders of the curved caps.
        let pad_y = (cfg.concave_radius * 2.0 - 4.0).max(MIN_PAD_Y);
        let bar_height = pad_y * 2.0 + n * item_height + (n - 1.0) * cfg.item_gap;
        let node_width = cfg.bar_width + GUTTER;

        Self {
            bar_width: cfg.bar_width,
            node_width,
            bar_height,
            // A stationary local rail centre avoids a webview jump when edges flip.
            window_width: cfg.bar_width
                + 2.0 * (GUTTER + BUBBLE_WIDTH + BUBBLE_TAIL + BUBBLE_MARGIN),
            window_height: bar_height + BUBBLE_SLACK * 2.0,
            bar_offset_y: BUBBLE_SLACK,
            concave_radius: cfg.concave_radius,
            ring_diameter: cfg.ring_diameter,
            ring_line_width: cfg.ring_line_width,
            item_gap: cfg.item_gap,
            pad_y,
            label_height: LABEL_HEIGHT,
            label_gap: LABEL_GAP,
            bubble_width: BUBBLE_WIDTH,
            bubble_tail_width: BUBBLE_TAIL,
            bubble_tail_height: BUBBLE_TAIL_HEIGHT,
            bubble_radius: BUBBLE_RADIUS,
            item_count,
            edge: cfg.edge,
        }
    }

    /// Centre of ring `index`, measured from the top of the **window**.
    ///
    /// Window-relative rather than bar-relative because this is what positions
    /// the bubble, which lives in the window's transparent area.
    pub fn ring_center_y(&self, index: usize) -> f64 {
        let item_height = self.ring_diameter + self.label_gap + self.label_height;
        self.bar_offset_y
            + self.pad_y
            + index as f64 * (item_height + self.item_gap)
            + self.ring_diameter / 2.0
    }

    /// Horizontal offset of the bar inside the window, in CSS pixels.
    ///
    /// Symmetric transparent space keeps the local rail centre stationary
    /// while its silhouette and tooltip switch sides during attachment.
    pub fn bar_offset_x(&self) -> f64 {
        (self.window_width - self.bar_width) / 2.0
    }

    /// Hit-test the painted silhouette, leaving the hollow caps click-through.
    /// `x` and `y` are relative to the solid rail's bounding rectangle.
    pub fn contains_bar_point(&self, x: f64, y: f64) -> bool {
        if !(0.0..=self.bar_width).contains(&x) || !(0.0..=self.bar_height).contains(&y) {
            return false;
        }
        let ry = self.concave_radius.min(self.bar_height / 4.0).max(0.0);
        if ry == 0.0 {
            return true;
        }
        let depth = y.min(self.bar_height - y);
        let half = self.bar_width / 2.0;
        let inner = if depth < ry {
            half + half * (1.0 - (depth / ry).powi(2)).max(0.0).sqrt()
        } else if depth < 2.0 * ry {
            half - half * (1.0 - ((depth - 2.0 * ry) / ry).powi(2)).max(0.0).sqrt()
        } else {
            0.0
        };
        let mirrored_x = if self.edge.is_left() {
            self.bar_width - x
        } else {
            x
        };
        mirrored_x >= inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curved_caps_only_capture_the_painted_part() {
        let layout = Layout::compute(&Config::default(), 3);
        let mid = layout.bar_width / 2.0;
        assert!(!layout.contains_bar_point(mid, 0.0));
        assert!(!layout.contains_bar_point(mid / 2.0, layout.concave_radius));
        assert!(layout.contains_bar_point(mid * 1.5, layout.concave_radius));
        assert!(layout.contains_bar_point(0.0, 2.0 * layout.concave_radius));
        assert!(!layout.contains_bar_point(mid, layout.bar_height));
        assert!(!layout.contains_bar_point(-1.0, layout.bar_height / 2.0));
    }

    #[test]
    fn left_and_right_hit_regions_are_mirrored() {
        let cfg = Config::default();
        let right = Layout::compute(&cfg, 2);
        let left = Layout::compute(
            &Config {
                edge: Edge::Left,
                ..cfg
            },
            2,
        );
        for x in [0.0, 8.0, 23.0, 31.0, 48.0, 62.0] {
            for y in [0.0, 8.0, 31.0, 48.0, 62.0, 120.0, 260.0] {
                assert_eq!(
                    right.contains_bar_point(x, y),
                    left.contains_bar_point(right.bar_width - x, y)
                );
            }
        }
    }

    #[test]
    fn all_rings_and_labels_fit_between_the_curved_ends() {
        for count in 1..=3 {
            let layout = Layout::compute(&Config::default(), count);
            let mid = layout.bar_width / 2.0;
            for index in 0..count {
                let center = layout.ring_center_y(index) - layout.bar_offset_y;
                let top = center - layout.ring_diameter / 2.0;
                let bottom =
                    center + layout.ring_diameter / 2.0 + layout.label_gap + layout.label_height;
                assert!(layout.contains_bar_point(mid, top));
                assert!(layout.contains_bar_point(mid, bottom));
                assert!(layout.contains_bar_point(mid - layout.ring_diameter / 2.0, center));
                assert!(layout.contains_bar_point(mid + layout.ring_diameter / 2.0, center));
            }
        }
    }
}
