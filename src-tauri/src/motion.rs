//! Pure drag/dock state machine. No AppKit, I/O, timers or provider calls here.
use crate::{
    config::{Config, Edge},
    layout::Layout,
    screen::ScreenGeometry,
};
use serde::Serialize;

const DRAG_THRESHOLD: f64 = 4.0;
const MAGNET_ENTER: f64 = 64.0;
const MAGNET_LEAVE: f64 = 88.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Docked,
    Dragging,
    Floating,
    Docking,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VisibleBounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionState {
    pub revision: u64,
    pub phase: Phase,
    pub edge: Edge,
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub focus_index: usize,
    pub magnet: f64,
    pub target_edge: Option<Edge>,
    pub velocity_x: f64,
    pub velocity_y: f64,
    pub over: bool,
    pub visible: VisibleBounds,
}
impl MotionState {
    pub fn docked(layout: &Layout) -> Self {
        Self {
            revision: 0,
            phase: Phase::Docked,
            edge: layout.edge,
            anchor_x: layout.bar_width / 2.0,
            anchor_y: layout.bar_height / 2.0,
            focus_index: 0,
            magnet: 0.0,
            target_edge: None,
            velocity_x: 0.0,
            velocity_y: 0.0,
            over: false,
            visible: VisibleBounds::default(),
        }
    }
    pub fn paused(&self) -> bool {
        self.phase != Phase::Docked
    }
}

pub fn orb_diameter(layout: &Layout) -> f64 {
    (layout.bar_width + 6.0).clamp(54.0, 76.0)
}

pub fn visible_bounds(layout: &Layout, geo: &ScreenGeometry, pos: (f64, f64)) -> VisibleBounds {
    VisibleBounds {
        left: (geo.frame.0 - pos.0).max(0.0).min(layout.window_width),
        top: (-pos.1).max(0.0).min(layout.window_height),
        right: (geo.max_x() - pos.0).clamp(0.0, layout.window_width),
        bottom: (geo.height() - pos.1).clamp(0.0, layout.window_height),
    }
}

pub fn dock_position(
    layout: &Layout,
    geo: &ScreenGeometry,
    cfg: &Config,
    edge: Edge,
    top: f64,
) -> (f64, f64) {
    let inset = cfg
        .edge_inset
        .clamp(0.0, (geo.frame.2 - layout.bar_width).max(0.0));
    let rail_x = if edge.is_left() {
        geo.frame.0 + inset
    } else {
        geo.max_x() - inset - layout.bar_width
    };
    (
        rail_x - layout.bar_offset_x(),
        top.clamp(0.0, (geo.height() - layout.bar_height).max(0.0)) - layout.bar_offset_y,
    )
}

fn spring(value: f64, velocity: f64, target: f64, dt: f64) -> (f64, f64) {
    let omega = 26.0;
    let change = value - target;
    let temp = (velocity + omega * change) * dt;
    let decay = (-omega * dt).exp();
    (
        target + (change + temp) * decay,
        (velocity - omega * temp) * decay,
    )
}

#[derive(Default)]
pub struct Effects {
    pub position: Option<(f64, f64)>,
    pub pause: Option<bool>,
    pub new_edge: Option<Edge>,
    pub docked: Option<(Edge, f64)>,
}

pub struct Input {
    pub cursor: (f64, f64),
    pub position: (f64, f64),
    pub pressed: bool,
    pub just_pressed: bool,
    pub inside: bool,
    pub dt: f64,
    pub reduced_motion: bool,
    pub force_edge: Option<Edge>,
}

pub struct Motion {
    pub view: MotionState,
    armed: bool,
    origin_cursor: (f64, f64),
    origin_position: (f64, f64),
    last_cursor: (f64, f64),
    dock_top: f64,
    dock_velocity: (f64, f64),
    elapsed: f64,
}
impl Motion {
    pub fn new(layout: &Layout) -> Self {
        Self {
            view: MotionState::docked(layout),
            armed: false,
            origin_cursor: (0.0, 0.0),
            origin_position: (0.0, 0.0),
            last_cursor: (0.0, 0.0),
            dock_top: 0.0,
            dock_velocity: (0.0, 0.0),
            elapsed: 0.0,
        }
    }
    pub fn animating(&self) -> bool {
        self.armed || matches!(self.view.phase, Phase::Dragging | Phase::Docking)
    }
    pub fn nearest_edge(&self, layout: &Layout, geo: &ScreenGeometry, pos: (f64, f64)) -> Edge {
        if pos.0 + layout.bar_offset_x() + self.view.anchor_x < geo.frame.0 + geo.frame.2 / 2.0 {
            Edge::Left
        } else {
            Edge::Right
        }
    }
    pub fn contains(&self, layout: &Layout, cursor: (f64, f64), pos: (f64, f64)) -> bool {
        let x = cursor.0 - pos.0 - layout.bar_offset_x();
        let y = cursor.1 - pos.1 - layout.bar_offset_y;
        match self.view.phase {
            Phase::Docked => layout.contains_bar_point(x, y),
            Phase::Docking => false, // Do not interrupt the 0.5s attachment half-way.
            _ => {
                (x - self.view.anchor_x).hypot(y - self.view.anchor_y)
                    <= orb_diameter(layout) / 2.0 + 3.0
            }
        }
    }
    fn clamp_floating(&self, layout: &Layout, geo: &ScreenGeometry, pos: (f64, f64)) -> (f64, f64) {
        let r = orb_diameter(layout) / 2.0 + 8.0;
        let offset = (
            layout.bar_offset_x() + self.view.anchor_x,
            layout.bar_offset_y + self.view.anchor_y,
        );
        let left = geo.frame.0 + r;
        let right = (geo.max_x() - r).max(left);
        let top = geo.menu_bar_height + r;
        let bottom = (geo.height() - r - 20.0).max(top);
        (
            (pos.0 + offset.0).clamp(left, right) - offset.0,
            (pos.1 + offset.1).clamp(top, bottom) - offset.1,
        )
    }
    fn begin_dock(
        &mut self,
        edge: Edge,
        input: &Input,
        layout: &Layout,
        geo: &ScreenGeometry,
        effects: &mut Effects,
    ) {
        self.armed = false;
        self.view.phase = Phase::Docking;
        self.view.edge = edge;
        self.view.target_edge = Some(edge);
        self.view.magnet = 1.0;
        self.view.velocity_x = 0.0;
        self.view.velocity_y = 0.0;
        self.dock_top = (input.position.1 + layout.bar_offset_y)
            .clamp(0.0, (geo.height() - layout.bar_height).max(0.0));
        self.dock_velocity = (0.0, 0.0);
        self.elapsed = 0.0;
        effects.new_edge = Some(edge);
        effects.pause = Some(true);
    }
    fn update_magnet(&mut self, layout: &Layout, geo: &ScreenGeometry, pos: (f64, f64)) -> Edge {
        let center = pos.0 + layout.bar_offset_x() + self.view.anchor_x;
        let edge = self.nearest_edge(layout, geo, pos);
        let distance = if edge.is_left() {
            center - geo.frame.0
        } else {
            geo.max_x() - center
        };
        let threshold = if self.view.target_edge == Some(edge) {
            MAGNET_LEAVE
        } else {
            MAGNET_ENTER
        };
        self.view.target_edge = (distance <= threshold).then_some(edge);
        self.view.magnet = if self.view.target_edge.is_some() {
            ((MAGNET_LEAVE - distance) / (MAGNET_LEAVE - orb_diameter(layout) / 2.0))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        edge
    }
    pub fn step(
        &mut self,
        mut input: Input,
        layout: &Layout,
        geo: &ScreenGeometry,
        cfg: &Config,
    ) -> Effects {
        let mut effects = Effects::default();
        let dt = input.dt.clamp(0.001, 0.05);
        if self.view.phase == Phase::Docked {
            self.view.edge = layout.edge;
        }
        if let Some(edge) = input.force_edge {
            if self.view.paused() {
                self.begin_dock(edge, &input, layout, geo, &mut effects);
            }
        }
        if self.view.phase == Phase::Docking {
            self.elapsed += dt;
            let target = dock_position(layout, geo, cfg, self.view.edge, self.dock_top);
            let (x, vx) = spring(input.position.0, self.dock_velocity.0, target.0, dt);
            let (y, vy) = spring(input.position.1, self.dock_velocity.1, target.1, dt);
            self.dock_velocity = (vx, vy);
            let done = if input.reduced_motion {
                self.elapsed >= 0.12
            } else {
                self.elapsed >= 0.52 && (x - target.0).hypot(y - target.1) < 0.3
                    || self.elapsed > 1.0
            };
            effects.position = Some(if done || input.reduced_motion {
                target
            } else {
                (x, y)
            });
            if done {
                self.view.phase = Phase::Docked;
                self.view.magnet = 0.0;
                self.view.target_edge = None;
                effects.pause = Some(false);
                effects.docked = Some((self.view.edge, self.dock_top));
            }
        } else if input.just_pressed && input.inside {
            self.armed = true;
            self.origin_cursor = input.cursor;
            self.last_cursor = input.cursor;
            self.origin_position = input.position;
            if self.view.phase == Phase::Docked {
                self.view.anchor_x = (input.cursor.0 - input.position.0 - layout.bar_offset_x())
                    .clamp(0.0, layout.bar_width);
                self.view.anchor_y = (input.cursor.1 - input.position.1 - layout.bar_offset_y)
                    .clamp(0.0, layout.bar_height);
                self.view.focus_index = (0..layout.item_count)
                    .min_by(|a, b| {
                        (layout.ring_center_y(*a) - layout.bar_offset_y - self.view.anchor_y)
                            .abs()
                            .total_cmp(
                                &(layout.ring_center_y(*b)
                                    - layout.bar_offset_y
                                    - self.view.anchor_y)
                                    .abs(),
                            )
                    })
                    .unwrap_or(0);
            }
        } else if input.pressed && self.armed {
            let dx = input.cursor.0 - self.origin_cursor.0;
            let dy = input.cursor.1 - self.origin_cursor.1;
            if self.view.phase == Phase::Dragging || dx.hypot(dy) >= DRAG_THRESHOLD {
                if self.view.phase != Phase::Dragging {
                    effects.pause = Some(true);
                }
                self.view.phase = Phase::Dragging;
                let next = self.clamp_floating(
                    layout,
                    geo,
                    (self.origin_position.0 + dx, self.origin_position.1 + dy),
                );
                let edge = self.update_magnet(layout, geo, next);
                self.view.velocity_x =
                    ((input.cursor.0 - self.last_cursor.0) / dt / 1800.0).clamp(-1.0, 1.0);
                self.view.velocity_y =
                    ((input.cursor.1 - self.last_cursor.1) / dt / 1800.0).clamp(-1.0, 1.0);
                self.last_cursor = input.cursor;
                // A small magnetic pull; the final attachment is committed only on release.
                effects.position = Some((
                    next.0 + if edge.is_left() { -1.0 } else { 1.0 } * self.view.magnet * 7.0,
                    next.1,
                ));
            }
        } else if !input.pressed {
            self.armed = false;
            self.view.velocity_x = 0.0;
            self.view.velocity_y = 0.0;
            if self.view.phase == Phase::Dragging {
                // Sample the release itself. The last pressed tick may still be
                // on the other side after a fast flick between two polls.
                let next = self.clamp_floating(
                    layout,
                    geo,
                    (
                        self.origin_position.0 + input.cursor.0 - self.origin_cursor.0,
                        self.origin_position.1 + input.cursor.1 - self.origin_cursor.1,
                    ),
                );
                let edge = self.update_magnet(layout, geo, next);
                let next = (
                    next.0 + if edge.is_left() { -1.0 } else { 1.0 } * self.view.magnet * 7.0,
                    next.1,
                );
                if (next.0 - input.position.0).hypot(next.1 - input.position.1) > 0.01 {
                    effects.position = Some(next);
                }
                input.position = next;
                if let Some(edge) = self.view.target_edge {
                    self.begin_dock(edge, &input, layout, geo, &mut effects);
                } else {
                    self.view.phase = Phase::Floating;
                    self.view.magnet = 0.0;
                }
            } else if self.view.phase == Phase::Floating {
                let next = self.clamp_floating(layout, geo, input.position);
                if next != input.position {
                    effects.position = Some(next);
                }
            }
        }
        self.view.over = input.inside;
        self.view.visible = visible_bounds(layout, geo, effects.position.unwrap_or(input.position));
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn geometry() -> ScreenGeometry {
        ScreenGeometry {
            frame: (0.0, 0.0, 1000.0, 800.0),
            menu_bar_height: 24.0,
            notch_height: 0.0,
            scale_factor: 2.0,
        }
    }
    fn input(
        cursor: (f64, f64),
        position: (f64, f64),
        pressed: bool,
        just_pressed: bool,
        inside: bool,
    ) -> Input {
        Input {
            cursor,
            position,
            pressed,
            just_pressed,
            inside,
            dt: 1.0 / 60.0,
            reduced_motion: false,
            force_edge: None,
        }
    }

    #[test]
    fn release_in_the_middle_leaves_a_paused_reachable_orb() {
        let cfg = Config::default();
        let layout = Layout::compute(&cfg, 2);
        let geo = geometry();
        let pos = dock_position(&layout, &geo, &cfg, Edge::Right, 80.0);
        let cursor = (
            pos.0 + layout.bar_offset_x() + layout.bar_width / 2.0,
            pos.1 + layout.ring_center_y(0),
        );
        let mut motion = Motion::new(&layout);
        motion.step(input(cursor, pos, true, true, true), &layout, &geo, &cfg);
        let middle = (500.0, 300.0);
        let dragged = motion.step(input(middle, pos, true, false, true), &layout, &geo, &cfg);
        assert_eq!(motion.view.phase, Phase::Dragging);
        assert_eq!(dragged.pause, Some(true));
        let next = dragged.position.unwrap();
        motion.step(input(middle, next, false, false, true), &layout, &geo, &cfg);
        assert_eq!(motion.view.phase, Phase::Floating);
        assert!(motion.view.paused());
        let center = (
            next.0 + layout.bar_offset_x() + motion.view.anchor_x,
            next.1 + layout.bar_offset_y + motion.view.anchor_y,
        );
        assert!(center.0 > 0.0 && center.0 < geo.max_x());
        assert!(center.1 > geo.menu_bar_height && center.1 < geo.height());
    }

    #[test]
    fn final_release_position_decides_magnetic_edge_after_a_fast_flick() {
        let cfg = Config::default();
        let layout = Layout::compute(&cfg, 1);
        let geo = geometry();
        let pos = dock_position(&layout, &geo, &cfg, Edge::Right, 80.0);
        let cursor = (
            pos.0 + layout.bar_offset_x() + 31.0,
            pos.1 + layout.bar_offset_y + layout.bar_height / 2.0,
        );
        let mut motion = Motion::new(&layout);
        motion.step(input(cursor, pos, true, true, true), &layout, &geo, &cfg);
        let center = (500.0, 300.0);
        let mid = motion
            .step(input(center, pos, true, false, true), &layout, &geo, &cfg)
            .position
            .unwrap();
        assert_eq!(motion.view.target_edge, None);
        let released = motion.step(
            input((15.0, 300.0), mid, false, false, true),
            &layout,
            &geo,
            &cfg,
        );
        assert_eq!(motion.view.phase, Phase::Docking);
        assert_eq!(released.new_edge, Some(Edge::Left));
    }

    #[test]
    fn forced_dock_finishes_once_and_resumes_polling() {
        let cfg = Config::default();
        let layout = Layout::compute(&cfg, 1);
        let geo = geometry();
        let mut motion = Motion::new(&layout);
        motion.view.phase = Phase::Floating;
        let mut pos = (300.0, 150.0);
        let mut first = input((400.0, 250.0), pos, false, false, true);
        first.force_edge = Some(Edge::Left);
        let effect = motion.step(first, &layout, &geo, &cfg);
        pos = effect.position.unwrap_or(pos);
        assert_eq!(effect.new_edge, Some(Edge::Left));
        let mut completed = None;
        for _ in 0..90 {
            let effect = motion.step(
                input((0.0, 0.0), pos, false, false, false),
                &layout,
                &geo,
                &cfg,
            );
            pos = effect.position.unwrap_or(pos);
            if effect.docked.is_some() {
                completed = Some(effect);
                break;
            }
        }
        let completed = completed.expect("spring should settle");
        assert_eq!(motion.view.phase, Phase::Docked);
        assert_eq!(completed.pause, Some(false));
        assert!(completed.docked.is_some());
    }

    #[test]
    fn symmetric_viewport_keeps_the_rail_at_one_local_x() {
        let right = Layout::compute(&Config::default(), 2);
        let left = Layout::compute(
            &Config {
                edge: Edge::Left,
                ..Config::default()
            },
            2,
        );
        assert_eq!(right.window_width, left.window_width);
        assert_eq!(right.bar_offset_x(), left.bar_offset_x());
        assert_eq!(
            visible_bounds(&right, &geometry(), (-right.bar_offset_x(), 0.0)).left,
            right.bar_offset_x()
        );
    }
}
