//! `ramp` — the colour ramp editor (docs/specs/gui-plan-2026-10-15.md
//! kill `g4-gradient-ramp`, Blender's ColorRamp: B:D19).
//!
//! Two layers, like [`super::value_field`]:
//!
//! - [`Ramp`] is the model: stops (position + linear RGBA), an active-stop
//!   index, and the interpolation. [`Ramp::evaluate`] is the one place a
//!   colour is read off the ramp, so the strip the widget draws and the
//!   filter/generator that consumes the ramp cannot disagree. Pure, no
//!   egui context: every editing rule is a unit test.
//! - [`RampEdit`] draws it: the gradient strip, one handle per stop. Drag
//!   a handle to move its stop, click to make it active, Ctrl/Cmd+click on
//!   the strip to add a stop there (taking the colour the ramp already has
//!   at that point, so adding a stop never changes the gradient). The
//!   active-stop index is the accessibility fallback for tiny targets: the
//!   panel edits the active stop through ordinary fields
//!   ([`Ramp::active_mut`]), so nothing requires hitting a 6 px handle.
//!
//! # Rules
//!
//! - Positions live in `[0, 1]`; NaN maps to 0 (the value-field rule:
//!   NaN never reaches engine state).
//! - A ramp always has at least one stop. Removing the last one is
//!   refused.
//! - Stops are kept sorted by position. Equal positions keep their
//!   insertion order (a stable sort), and the active index follows the
//!   active stop through every reorder.
//! - Outside the first/last stop the ramp holds that stop's colour
//!   (Blender's behaviour).
//! - Colours interpolate in LINEAR light ([`egui::Rgba`] is linear), the
//!   same space the paint engine composites in. sRGB interpolation would
//!   darken the midpoint of a red→green ramp.

use egui::epaint::Mesh;
use egui::{
    Color32, Pos2, Rect, Rgba, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::theme;

/// How the ramp blends between two neighbouring stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// Straight linear blend.
    #[default]
    Linear,
    /// Smoothstep blend (`3t² − 2t³`): flat at both stops.
    Ease,
    /// No blend: the left stop's colour up to the next stop.
    Constant,
}

impl Interpolation {
    /// The blend factor for a local parameter `t` in `[0, 1]`.
    pub fn weight(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Interpolation::Linear => t,
            Interpolation::Ease => t * t * (3.0 - 2.0 * t),
            Interpolation::Constant => 0.0,
        }
    }
}

/// One ramp stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    /// Position along the ramp, `[0, 1]`.
    pub position: f32,
    /// Linear RGBA.
    pub color: Rgba,
}

/// Clamps a stop position into `[0, 1]`; NaN maps to 0.
pub fn clamp_position(position: f32) -> f32 {
    if position.is_nan() {
        0.0
    } else {
        position.clamp(0.0, 1.0)
    }
}

/// The colour ramp model (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct Ramp {
    stops: Vec<Stop>,
    active: usize,
    interpolation: Interpolation,
}

impl Default for Ramp {
    /// Black at 0 to white at 1, linear: Blender's default ramp.
    fn default() -> Self {
        Self::new(vec![
            Stop {
                position: 0.0,
                color: Rgba::BLACK,
            },
            Stop {
                position: 1.0,
                color: Rgba::WHITE,
            },
        ])
    }
}

impl Ramp {
    /// A ramp over `stops` (positions clamped, then sorted). An empty list
    /// becomes one black stop at 0: a ramp is never empty.
    pub fn new(stops: Vec<Stop>) -> Self {
        let mut stops: Vec<Stop> = stops
            .into_iter()
            .map(|s| Stop {
                position: clamp_position(s.position),
                color: s.color,
            })
            .collect();
        if stops.is_empty() {
            stops.push(Stop {
                position: 0.0,
                color: Rgba::BLACK,
            });
        }
        stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        Ramp {
            stops,
            active: 0,
            interpolation: Interpolation::Linear,
        }
    }

    /// The stops, sorted by position.
    pub fn stops(&self) -> &[Stop] {
        &self.stops
    }

    /// The active stop's index.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Makes stop `index` active (clamped to the stop count).
    pub fn set_active(&mut self, index: usize) {
        self.active = index.min(self.stops.len() - 1);
    }

    /// The active stop, for the panel's Position/Color fields.
    pub fn active_stop(&self) -> Stop {
        self.stops[self.active]
    }

    /// Edits the active stop. A position change re-sorts the stops and
    /// the active index follows the stop.
    pub fn active_mut(&mut self, edit: impl FnOnce(&mut Stop)) {
        edit(&mut self.stops[self.active]);
        self.stops[self.active].position = clamp_position(self.stops[self.active].position);
        self.resort();
    }

    /// The interpolation mode.
    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }

    /// Sets the interpolation mode.
    pub fn set_interpolation(&mut self, interpolation: Interpolation) {
        self.interpolation = interpolation;
    }

    /// The colour at `t` (clamped into `[0, 1]`; NaN reads as 0).
    pub fn evaluate(&self, t: f32) -> Rgba {
        let t = clamp_position(t);
        let first = self.stops[0];
        if t <= first.position {
            return first.color;
        }
        for pair in self.stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t < b.position {
                let span = b.position - a.position;
                let local = if span > 0.0 {
                    (t - a.position) / span
                } else {
                    0.0
                };
                let w = self.interpolation.weight(local);
                return a.color * (1.0 - w) + b.color * w;
            }
        }
        self.stops[self.stops.len() - 1].color
    }

    /// Adds a stop at `position` carrying the ramp's current colour there
    /// (so the gradient does not change), makes it active, and returns its
    /// index.
    pub fn add_stop(&mut self, position: f32) -> usize {
        let position = clamp_position(position);
        let color = self.evaluate(position);
        // After every stop at the same position: a new stop lands to the
        // right of an existing one it coincides with.
        let index = self.stops.partition_point(|s| s.position <= position);
        self.stops.insert(index, Stop { position, color });
        self.active = index;
        index
    }

    /// Removes the active stop. Refused (returns `false`) when it is the
    /// only stop. The previous stop becomes active.
    pub fn remove_active(&mut self) -> bool {
        if self.stops.len() <= 1 {
            return false;
        }
        self.stops.remove(self.active);
        self.active = self.active.saturating_sub(1).min(self.stops.len() - 1);
        true
    }

    /// Moves stop `index` to `position` (clamped). Returns the stop's index
    /// after the re-sort; it stays active if it was.
    pub fn move_stop(&mut self, index: usize, position: f32) -> usize {
        let Some(stop) = self.stops.get_mut(index) else {
            return index;
        };
        stop.position = clamp_position(position);
        let was_active = self.active;
        self.active = index;
        self.resort();
        let moved = self.active;
        if was_active != index {
            // The tracked stop was `index`; restore the old active one by
            // identity is not possible for equal stops, so re-derive it:
            // only `index` moved, so every other stop kept its relative
            // order and the old active shifts by at most one slot.
            self.active = shift_index(was_active, index, moved);
        }
        moved
    }

    /// Mirrors the ramp: position `p` becomes `1 − p`, order reversed.
    pub fn flip(&mut self) {
        for stop in &mut self.stops {
            stop.position = 1.0 - stop.position;
        }
        self.stops.reverse();
        self.active = self.stops.len() - 1 - self.active;
    }

    /// Spaces the stops evenly over `[0, 1]` in their current order. A
    /// single stop goes to 0.
    pub fn distribute(&mut self) {
        let n = self.stops.len();
        if n == 1 {
            self.stops[0].position = 0.0;
            return;
        }
        let last = (n - 1) as f32;
        for (i, stop) in self.stops.iter_mut().enumerate() {
            stop.position = i as f32 / last;
        }
    }

    /// Stable re-sort by position, keeping `active` on the same stop.
    fn resort(&mut self) {
        let mut order: Vec<usize> = (0..self.stops.len()).collect();
        order.sort_by(|&a, &b| self.stops[a].position.total_cmp(&self.stops[b].position));
        self.stops = order.iter().map(|&i| self.stops[i]).collect();
        self.active = order.iter().position(|&i| i == self.active).unwrap_or(0);
    }
}

/// Where a non-moved index lands after the stop at `from` moved to `to`
/// (everything between shifts one slot toward `from`).
fn shift_index(other: usize, from: usize, to: usize) -> usize {
    if from < to && other > from && other <= to {
        other - 1
    } else if to < from && other >= to && other < from {
        other + 1
    } else {
        other
    }
}

/// The handle hit-test: the stop whose handle centre is nearest to `x`
/// within `radius` points, or `None`. Ties take the active stop, then the
/// later stop (it draws on top).
pub fn pick_stop(ramp: &Ramp, rect_left: f32, width: f32, x: f32, radius: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, stop) in ramp.stops().iter().enumerate() {
        let d = (rect_left + stop.position * width - x).abs();
        if d > radius {
            continue;
        }
        let better = match best {
            None => true,
            Some((b, bd)) => d < bd || (d == bd && (i == ramp.active() || b != ramp.active())),
        };
        if better {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// The strip position (`[0, 1]`) under screen `x`.
pub fn position_at(rect_left: f32, width: f32, x: f32) -> f32 {
    if width <= 0.0 {
        0.0
    } else {
        clamp_position((x - rect_left) / width)
    }
}

/// Segments the drawn strip uses (vertex columns minus one).
const STRIP_SEGMENTS: usize = 64;
/// Handle hit radius, points.
const HANDLE_RADIUS: f32 = 6.0;

/// The ramp editor widget (see the module docs).
pub struct RampEdit<'a> {
    ramp: &'a mut Ramp,
    width: Option<f32>,
    height: f32,
}

impl<'a> RampEdit<'a> {
    /// An editor over `ramp`, full available width, 20 pt strip.
    pub fn new(ramp: &'a mut Ramp) -> Self {
        RampEdit {
            ramp,
            width: None,
            height: 20.0,
        }
    }

    /// Fixed width, points.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Draws the strip + handles and applies this frame's input. The
    /// response is `changed()` when a stop moved, was added, or the active
    /// stop changed.
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let width = self.width.unwrap_or_else(|| ui.available_width()).max(16.0);
        let handle_band = 10.0;
        let size = Vec2::new(width, self.height + handle_band);
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        // g7-accesskit-sweep: the gradient ramp strip (Slider role; the
        // drag edits the stops).
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "gradient"));
        let strip = Rect::from_min_size(rect.min, Vec2::new(width, self.height));
        let ramp = self.ramp;

        let drag_id = response.id.with("ramp-drag");
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let picked = pick_stop(ramp, strip.left(), width, pos.x, HANDLE_RADIUS);
                ui.ctx().data_mut(|d| d.insert_temp(drag_id, picked));
                if let Some(i) = picked {
                    if ramp.active() != i {
                        ramp.set_active(i);
                        response.mark_changed();
                    }
                }
            }
        }
        if response.dragged() {
            let dragging: Option<usize> = ui.ctx().data(|d| d.get_temp(drag_id)).flatten();
            if let (Some(i), Some(pos)) = (dragging, response.interact_pointer_pos()) {
                let to = ramp.move_stop(i, position_at(strip.left(), width, pos.x));
                ui.ctx().data_mut(|d| d.insert_temp(drag_id, Some(to)));
                response.mark_changed();
            }
        }
        if response.drag_stopped() {
            ui.ctx().data_mut(|d| d.remove::<Option<usize>>(drag_id));
        }
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if ui.input(|i| i.modifiers.command) {
                    ramp.add_stop(position_at(strip.left(), width, pos.x));
                    response.mark_changed();
                } else if let Some(i) = pick_stop(ramp, strip.left(), width, pos.x, HANDLE_RADIUS) {
                    ramp.set_active(i);
                    response.mark_changed();
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.add(Shape::mesh(strip_mesh(ramp, strip)));
            painter.rect_stroke(
                strip,
                0.0,
                Stroke::new(1.0, theme::faint()),
                StrokeKind::Inside,
            );
            for (i, stop) in ramp.stops().iter().enumerate() {
                let x = strip.left() + stop.position * width;
                let tip = Pos2::new(x, strip.bottom());
                let handle = vec![
                    tip,
                    Pos2::new(x + 5.0, strip.bottom() + handle_band - 1.0),
                    Pos2::new(x - 5.0, strip.bottom() + handle_band - 1.0),
                ];
                let outline = if i == ramp.active() {
                    Stroke::new(1.5, theme::accent())
                } else {
                    Stroke::new(1.0, theme::text())
                };
                painter.add(Shape::convex_polygon(
                    handle,
                    Color32::from(stop.color),
                    outline,
                ));
            }
        }
        response
    }
}

/// The gradient strip as a vertex grid: one column per segment edge,
/// coloured by [`Ramp::evaluate`]. Constant interpolation gets its hard
/// edges from extra columns at each stop.
fn strip_mesh(ramp: &Ramp, strip: Rect) -> Mesh {
    let mut ts: Vec<f32> = (0..=STRIP_SEGMENTS)
        .map(|i| i as f32 / STRIP_SEGMENTS as f32)
        .collect();
    if ramp.interpolation() == Interpolation::Constant {
        for stop in ramp.stops() {
            ts.push((stop.position - 1e-4).max(0.0));
            ts.push(stop.position);
        }
        ts.sort_by(f32::total_cmp);
    }
    let mut mesh = Mesh::default();
    for (col, &t) in ts.iter().enumerate() {
        let x = strip.left() + t * strip.width();
        let color = Color32::from(ramp.evaluate(t));
        mesh.colored_vertex(Pos2::new(x, strip.top()), color);
        mesh.colored_vertex(Pos2::new(x, strip.bottom()), color);
        if col > 0 {
            let base = (col as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(r: f32, g: f32, b: f32) -> Rgba {
        Rgba::from_rgb(r, g, b)
    }

    fn approx(a: Rgba, b: Rgba) -> bool {
        (0..4).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    fn red_green() -> Ramp {
        Ramp::new(vec![
            Stop {
                position: 0.0,
                color: rgba(1.0, 0.0, 0.0),
            },
            Stop {
                position: 1.0,
                color: rgba(0.0, 1.0, 0.0),
            },
        ])
    }

    #[test]
    fn weights_per_interpolation() {
        assert_eq!(Interpolation::Linear.weight(0.25), 0.25);
        // Smoothstep at 0.25: 3/16 − 2/64 = 0.15625.
        assert_eq!(Interpolation::Ease.weight(0.25), 0.15625);
        assert_eq!(Interpolation::Ease.weight(0.5), 0.5);
        assert_eq!(Interpolation::Constant.weight(0.99), 0.0);
        // Out-of-range t clamps.
        assert_eq!(Interpolation::Linear.weight(2.0), 1.0);
    }

    #[test]
    fn evaluate_blends_in_linear_light_per_mode() {
        let mut r = red_green();
        // Linear midpoint: half red + half green in LINEAR light.
        assert!(approx(r.evaluate(0.5), rgba(0.5, 0.5, 0.0)));
        assert!(approx(r.evaluate(0.25), rgba(0.75, 0.25, 0.0)));
        r.set_interpolation(Interpolation::Ease);
        assert!(approx(r.evaluate(0.25), rgba(0.84375, 0.15625, 0.0)));
        r.set_interpolation(Interpolation::Constant);
        assert!(approx(r.evaluate(0.99), rgba(1.0, 0.0, 0.0)));
        // At the last stop's position the last colour holds.
        assert!(approx(r.evaluate(1.0), rgba(0.0, 1.0, 0.0)));
    }

    #[test]
    fn outside_the_stops_holds_the_end_colours_and_nan_reads_as_zero() {
        let r = Ramp::new(vec![
            Stop {
                position: 0.25,
                color: rgba(1.0, 0.0, 0.0),
            },
            Stop {
                position: 0.75,
                color: rgba(0.0, 0.0, 1.0),
            },
        ]);
        assert!(approx(r.evaluate(0.0), rgba(1.0, 0.0, 0.0)));
        assert!(approx(r.evaluate(0.9), rgba(0.0, 0.0, 1.0)));
        assert!(approx(r.evaluate(f32::NAN), rgba(1.0, 0.0, 0.0)));
        assert!(approx(r.evaluate(0.5), rgba(0.5, 0.0, 0.5)));
    }

    #[test]
    fn new_sorts_clamps_and_never_is_empty() {
        let r = Ramp::new(vec![
            Stop {
                position: 2.0,
                color: Rgba::WHITE,
            },
            Stop {
                position: f32::NAN,
                color: Rgba::BLACK,
            },
        ]);
        let positions: Vec<f32> = r.stops().iter().map(|s| s.position).collect();
        assert_eq!(positions, vec![0.0, 1.0]);
        assert_eq!(r.stops()[0].color, Rgba::BLACK);
        let empty = Ramp::new(Vec::new());
        assert_eq!(empty.stops().len(), 1);
    }

    #[test]
    fn add_stop_keeps_the_gradient_and_becomes_active() {
        let mut r = red_green();
        let before: Vec<Rgba> = (0..=8).map(|i| r.evaluate(i as f32 / 8.0)).collect();
        let i = r.add_stop(0.25);
        assert_eq!(i, 1);
        assert_eq!(r.active(), 1);
        assert!(approx(r.stops()[1].color, rgba(0.75, 0.25, 0.0)));
        let after: Vec<Rgba> = (0..=8).map(|i| r.evaluate(i as f32 / 8.0)).collect();
        for (a, b) in before.iter().zip(&after) {
            assert!(approx(*a, *b), "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn remove_active_refuses_the_last_stop() {
        let mut r = red_green();
        r.set_active(1);
        assert!(r.remove_active());
        assert_eq!(r.stops().len(), 1);
        assert_eq!(r.active(), 0);
        assert!(!r.remove_active());
        assert_eq!(r.stops().len(), 1);
    }

    #[test]
    fn moving_past_a_neighbour_reorders_and_tracks_active() {
        let mut r = red_green();
        r.add_stop(0.5); // [0, 0.5, 1], active = 1 (the mid stop)
        r.set_active(2); // the green end
                         // Drag the first (red) stop past the middle one.
        let to = r.move_stop(0, 0.75);
        assert_eq!(to, 1);
        let positions: Vec<f32> = r.stops().iter().map(|s| s.position).collect();
        assert_eq!(positions, vec![0.5, 0.75, 1.0]);
        assert!(approx(r.stops()[1].color, rgba(1.0, 0.0, 0.0)));
        // The active stop is still the green end.
        assert!(approx(r.active_stop().color, rgba(0.0, 1.0, 0.0)));
        // Dragging the active stop keeps it active at its new slot.
        r.set_active(2);
        let to = r.move_stop(2, 0.0);
        assert_eq!((to, r.active()), (0, 0));
        assert!(approx(r.active_stop().color, rgba(0.0, 1.0, 0.0)));
    }

    #[test]
    fn active_mut_resorts_with_the_stop() {
        let mut r = red_green();
        r.add_stop(0.5);
        r.set_active(0);
        // Red moves past the middle stop: the active index follows it.
        r.active_mut(|s| s.position = 0.75);
        assert_eq!(r.active(), 1);
        assert_eq!(r.active_stop().position, 0.75);
        assert!(approx(r.active_stop().color, rgba(1.0, 0.0, 0.0)));
        // Out-of-range edits clamp; an equal position keeps the stable
        // order (red stays ahead of green at 1.0).
        r.active_mut(|s| s.position = 1.5);
        assert_eq!(r.active(), 1);
        assert_eq!(r.active_stop().position, 1.0);
    }

    #[test]
    fn flip_mirrors_and_distribute_spaces_evenly() {
        let mut r = red_green();
        r.add_stop(0.25);
        r.set_active(0);
        r.flip();
        let positions: Vec<f32> = r.stops().iter().map(|s| s.position).collect();
        assert_eq!(positions, vec![0.0, 0.75, 1.0]);
        assert!(approx(r.stops()[0].color, rgba(0.0, 1.0, 0.0)));
        assert_eq!(r.active(), 2, "active follows the red stop");
        // Flipping changes what the ramp reads: green now at 0.
        assert!(approx(r.evaluate(0.0), rgba(0.0, 1.0, 0.0)));
        r.distribute();
        let positions: Vec<f32> = r.stops().iter().map(|s| s.position).collect();
        assert_eq!(positions, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn pick_prefers_the_nearest_then_the_active_stop() {
        let mut r = red_green();
        r.add_stop(0.5);
        // 100 pt strip at x = 10: handles at 10, 60, 110.
        assert_eq!(pick_stop(&r, 10.0, 100.0, 63.0, 6.0), Some(1));
        assert_eq!(pick_stop(&r, 10.0, 100.0, 80.0, 6.0), None);
        // Two stops at the same spot: the active one wins.
        r.move_stop(1, 1.0);
        r.set_active(1);
        assert_eq!(pick_stop(&r, 10.0, 100.0, 110.0, 6.0), Some(1));
        r.set_active(2);
        assert_eq!(pick_stop(&r, 10.0, 100.0, 110.0, 6.0), Some(2));
        assert_eq!(position_at(10.0, 100.0, 35.0), 0.25);
        assert_eq!(position_at(10.0, 100.0, -50.0), 0.0);
        assert_eq!(position_at(10.0, 0.0, 35.0), 0.0);
    }

    #[test]
    fn strip_mesh_colours_come_from_evaluate() {
        let r = red_green();
        let strip = Rect::from_min_size(Pos2::ZERO, Vec2::new(64.0, 10.0));
        let mesh = strip_mesh(&r, strip);
        assert_eq!(mesh.vertices.len(), (STRIP_SEGMENTS + 1) * 2);
        assert_eq!(mesh.indices.len(), STRIP_SEGMENTS * 6);
        let mid = &mesh.vertices[STRIP_SEGMENTS]; // column 32, top
        assert_eq!(mid.pos.x, 32.0);
        assert_eq!(mid.color, Color32::from(r.evaluate(0.5)));
        // Constant mode adds the hard-edge columns at each stop.
        let mut c = red_green();
        c.set_interpolation(Interpolation::Constant);
        assert_eq!(
            strip_mesh(&c, strip).vertices.len(),
            (STRIP_SEGMENTS + 1 + 4) * 2
        );
    }

    #[test]
    fn headless_ctrl_click_adds_a_stop() {
        let ctx = egui::Context::default();
        let mut ramp = red_green();
        let mut frame = |mut events: Vec<egui::Event>, mods: egui::Modifiers| {
            // egui reads held modifiers from this event (RawInput has no
            // modifiers field in 0.36).
            events.insert(0, egui::Event::ModifiersChanged(mods));
            let input = egui::RawInput {
                events,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                ..Default::default()
            };
            let mut changed = false;
            let mut rect = Rect::NOTHING;
            let mut out = ctx.run_ui(input, |ui| {
                let response = RampEdit::new(&mut ramp).width(200.0).show(ui);
                changed = response.changed();
                rect = response.rect;
            });
            out.textures_delta.clear();
            (changed, rect)
        };
        // Lay out once to learn where the strip sits.
        let (_, rect) = frame(Vec::new(), egui::Modifiers::NONE);
        let origin = rect.min;
        let at = Pos2::new(origin.x + 50.0, origin.y + 10.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(
            vec![egui::Event::PointerMoved(at), press(true)],
            egui::Modifiers::COMMAND,
        );
        let (changed, _) = frame(vec![press(false)], egui::Modifiers::COMMAND);
        assert!(changed);
        assert_eq!(ramp.stops().len(), 3);
        assert_eq!(ramp.active(), 1);
        assert!((ramp.active_stop().position - 0.25).abs() < 0.01);
    }

    #[test]
    fn the_ramp_strip_reports_a_slider_role_to_accesskit() {
        use egui_kittest::kittest::Queryable;
        let mut ramp = Ramp::default();
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            RampEdit::new(&mut ramp).show(ui);
        });
        harness.run();
        let _ = harness.get_by_role_and_label(egui::accesskit::Role::Slider, "gradient");
    }
}
