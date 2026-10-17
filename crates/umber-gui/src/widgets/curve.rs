//! `curve` — the general curve editor (docs/specs/gui-plan-2026-10-15.md
//! kill `g4-curve-widget`, B:D18, B:C6.1). Promoted from `umber-app`'s
//! brush-panel pressure-curve plot into the widget kit, so pressure,
//! falloff Custom and Levels share one editor.
//!
//! Two layers, like [`super::ramp`]:
//!
//! - [`Curve`] is the model: control points in `[0, 1]²`, x strictly
//!   increasing (at least [`MIN_GAP`] apart), at least two points.
//!   [`Curve::evaluate`] is clamped piecewise-linear, the same
//!   interpolation as `umber_brush::preset::ControlCurve::evaluate`, so a
//!   curve edited here and one baked into a preset read the same values.
//!   Every edit (move, insert, delete, paste) keeps the invariant: the
//!   model is never invalid. Pure, no egui context.
//! - [`CurveEdit`] draws it: the grid, the polyline, one handle per point.
//!   Drag a handle to move it (clamped between its neighbours). Click on
//!   empty plot space to add a point there; right-click a handle to delete
//!   it (never an endpoint). With [`CurveEdit::fixed_count`] the editor
//!   only moves points, for models with a frozen point count (the brush
//!   preset's four-point curve). Hover + Ctrl/Cmd+C copies the curve as
//!   text and Ctrl/Cmd+V pastes one ([`Curve::to_text`] / [`Curve::parse`]).
//!
//! The umber-gui layer rule (scripts/check-layers.py: egui only) keeps this
//! free of umber-brush: a consumer converts its own curve type to and from
//! [`Curve::points`].

use egui::{
    Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::theme;

/// The smallest x distance between neighbouring points.
pub const MIN_GAP: f32 = 0.01;
/// Handle hit radius, points.
const HANDLE_RADIUS: f32 = 6.0;
/// Grid divisions per axis.
const GRID_DIVS: usize = 4;

/// Why an edit or a parse was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveError {
    /// Fewer than two points.
    TooFewPoints,
    /// A coordinate was NaN or infinite, or outside `[0, 1]`.
    OutOfRange,
    /// x values not strictly increasing by at least [`MIN_GAP`].
    NotMonotonic,
    /// The text was not `x,y;x,y;...`.
    Malformed,
    /// The edit would remove an endpoint or go below two points.
    Protected,
}

/// The curve model (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    points: Vec<(f32, f32)>,
}

impl Default for Curve {
    /// The identity line `(0,0)–(1,1)`.
    fn default() -> Self {
        Curve {
            points: vec![(0.0, 0.0), (1.0, 1.0)],
        }
    }
}

impl Curve {
    /// A curve over `points`, validated (see [`CurveError`]).
    pub fn new(points: Vec<(f32, f32)>) -> Result<Self, CurveError> {
        if points.len() < 2 {
            return Err(CurveError::TooFewPoints);
        }
        let in_unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        if points.iter().any(|&(x, y)| !in_unit(x) || !in_unit(y)) {
            return Err(CurveError::OutOfRange);
        }
        if points.windows(2).any(|w| w[1].0 - w[0].0 < MIN_GAP) {
            return Err(CurveError::NotMonotonic);
        }
        Ok(Curve { points })
    }

    /// A curve with `count` points (at least 2) sampling `shape` at evenly
    /// spaced x from 0 to 1, y clamped to `[0, 1]`. Used for the presets.
    pub fn sampled(count: usize, shape: impl Fn(f32) -> f32) -> Self {
        let count = count.max(2);
        let last = (count - 1) as f32;
        let points = (0..count)
            .map(|i| {
                let x = i as f32 / last;
                let y = shape(x);
                (
                    x,
                    if y.is_finite() {
                        y.clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                )
            })
            .collect();
        Curve { points }
    }

    /// The points, x ascending.
    pub fn points(&self) -> &[(f32, f32)] {
        &self.points
    }

    /// Clamped piecewise-linear interpolation: below the first point the
    /// first y, above the last the last y. NaN reads as 0.
    pub fn evaluate(&self, x: f32) -> f32 {
        let x = if x.is_nan() { 0.0 } else { x };
        let p = &self.points;
        if x <= p[0].0 {
            return p[0].1;
        }
        for w in p.windows(2) {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            if x <= x1 {
                return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
            }
        }
        p[p.len() - 1].1
    }

    /// The legal x window for point `index`: between its neighbours with
    /// [`MIN_GAP`] each side, inside `[0, 1]`.
    pub fn x_window(&self, index: usize) -> (f32, f32) {
        let lo = if index == 0 {
            0.0
        } else {
            self.points[index - 1].0 + MIN_GAP
        };
        let hi = if index + 1 >= self.points.len() {
            1.0
        } else {
            self.points[index + 1].0 - MIN_GAP
        };
        (lo, hi)
    }

    /// Moves point `index` to `(x, y)`, clamped: y into `[0, 1]`, x into
    /// [`Self::x_window`]. A non-finite x keeps the old x, a non-finite y
    /// the old y. Returns whether anything changed.
    pub fn move_point(&mut self, index: usize, x: f32, y: f32) -> bool {
        let Some(&old) = self.points.get(index) else {
            return false;
        };
        let (lo, hi) = self.x_window(index);
        let nx = if x.is_finite() && lo <= hi {
            x.clamp(lo, hi)
        } else {
            old.0
        };
        let ny = if y.is_finite() {
            y.clamp(0.0, 1.0)
        } else {
            old.1
        };
        self.points[index] = (nx, ny);
        (nx, ny) != old
    }

    /// Inserts a point at `(x, y)` and returns its index. Refused
    /// ([`CurveError::NotMonotonic`]) when x is within [`MIN_GAP`] of an
    /// existing point or outside the first/last point's x.
    pub fn insert(&mut self, x: f32, y: f32) -> Result<usize, CurveError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(CurveError::OutOfRange);
        }
        let index = self.points.partition_point(|p| p.0 < x);
        if index == 0 || index == self.points.len() {
            return Err(CurveError::NotMonotonic);
        }
        let (left, right) = (self.points[index - 1].0, self.points[index].0);
        if x - left < MIN_GAP || right - x < MIN_GAP {
            return Err(CurveError::NotMonotonic);
        }
        self.points.insert(index, (x, y.clamp(0.0, 1.0)));
        Ok(index)
    }

    /// Removes interior point `index`. Endpoints are protected, and so is
    /// any point when only two remain.
    pub fn remove(&mut self, index: usize) -> Result<(), CurveError> {
        if index == 0 || index + 1 >= self.points.len() || self.points.len() <= 2 {
            return Err(CurveError::Protected);
        }
        self.points.remove(index);
        Ok(())
    }

    /// The clipboard form: `x,y;x,y;...` with four decimals.
    pub fn to_text(&self) -> String {
        self.points
            .iter()
            .map(|(x, y)| format!("{x:.4},{y:.4}"))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Parses [`Self::to_text`]'s form (whitespace allowed), validated
    /// like [`Self::new`].
    pub fn parse(text: &str) -> Result<Self, CurveError> {
        let mut points = Vec::new();
        for pair in text.trim().split(';').filter(|p| !p.trim().is_empty()) {
            let (x, y) = pair.split_once(',').ok_or(CurveError::Malformed)?;
            let x: f32 = x.trim().parse().map_err(|_| CurveError::Malformed)?;
            let y: f32 = y.trim().parse().map_err(|_| CurveError::Malformed)?;
            points.push((x, y));
        }
        Curve::new(points)
    }

    /// Replaces the points with `other`'s when the point count is allowed
    /// (`fixed` = must match). Returns whether the curve changed.
    pub fn assign(&mut self, other: Curve, fixed: bool) -> bool {
        if (fixed && other.points.len() != self.points.len()) || *self == other {
            return false;
        }
        *self = other;
        true
    }
}

/// The presets: Linear, Smooth (smoothstep), Ease (ease-out cubic), each
/// sampled at `count` points so a fixed-count curve keeps its count.
pub fn preset_linear(count: usize) -> Curve {
    Curve::sampled(count, |t| t)
}

/// Smoothstep `3t² − 2t³`.
pub fn preset_smooth(count: usize) -> Curve {
    Curve::sampled(count, |t| t * t * (3.0 - 2.0 * t))
}

/// Ease-out cubic `1 − (1 − t)³`.
pub fn preset_ease(count: usize) -> Curve {
    Curve::sampled(count, |t| 1.0 - (1.0 - t).powi(3))
}

/// Plot-space mapping: curve `(x, y)` in `[0, 1]²` to a screen point in
/// `rect` (y up), and back.
pub fn to_screen(rect: Rect, p: (f32, f32)) -> Pos2 {
    Pos2::new(
        rect.left() + p.0 * rect.width(),
        rect.bottom() - p.1 * rect.height(),
    )
}

/// The inverse of [`to_screen`], unclamped.
pub fn from_screen(rect: Rect, pos: Pos2) -> (f32, f32) {
    let w = rect.width().max(f32::EPSILON);
    let h = rect.height().max(f32::EPSILON);
    ((pos.x - rect.left()) / w, (rect.bottom() - pos.y) / h)
}

/// The handle under `pos` within `radius` points, nearest first.
pub fn pick_point(curve: &Curve, rect: Rect, pos: Pos2, radius: f32) -> Option<usize> {
    curve
        .points()
        .iter()
        .enumerate()
        .map(|(i, &p)| (i, to_screen(rect, p).distance(pos)))
        .filter(|&(_, d)| d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// The curve editor widget (see the module docs).
pub struct CurveEdit<'a> {
    curve: &'a mut Curve,
    size: f32,
    fixed_count: bool,
}

impl<'a> CurveEdit<'a> {
    /// An editor over `curve`, a 160 pt square plot.
    pub fn new(curve: &'a mut Curve) -> Self {
        CurveEdit {
            curve,
            size: 160.0,
            fixed_count: false,
        }
    }

    /// The plot edge, points.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Moves only: no insert, no delete, and a pasted curve must carry
    /// the same point count.
    pub fn fixed_count(mut self, fixed: bool) -> Self {
        self.fixed_count = fixed;
        self
    }

    /// Draws the plot and applies this frame's input; `changed()` when the
    /// curve changed.
    pub fn show(self, ui: &mut Ui) -> egui::Response {
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::splat(self.size), Sense::click_and_drag());
        // g7-accesskit-sweep: the curve editor canvas (an editor, not a
        // static plot — Slider role carries the keyboard-adjacency).
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "curve"));
        let curve = self.curve;
        let drag_id = response.id.with("curve-drag");

        if response.drag_started() {
            let picked = response
                .interact_pointer_pos()
                .and_then(|pos| pick_point(curve, rect, pos, HANDLE_RADIUS));
            ui.ctx().data_mut(|d| d.insert_temp(drag_id, picked));
        }
        if response.dragged() {
            let dragging: Option<usize> = ui.ctx().data(|d| d.get_temp(drag_id)).flatten();
            if let (Some(i), Some(pos)) = (dragging, response.interact_pointer_pos()) {
                let (x, y) = from_screen(rect, pos);
                if curve.move_point(i, x, y) {
                    response.mark_changed();
                }
            }
        }
        if response.drag_stopped() {
            ui.ctx().data_mut(|d| d.remove::<Option<usize>>(drag_id));
        }
        if !self.fixed_count {
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if pick_point(curve, rect, pos, HANDLE_RADIUS).is_none() {
                        let (x, y) = from_screen(rect, pos);
                        if curve.insert(x, y).is_ok() {
                            response.mark_changed();
                        }
                    }
                }
            }
            if response.secondary_clicked() {
                let hit = response
                    .interact_pointer_pos()
                    .and_then(|pos| pick_point(curve, rect, pos, HANDLE_RADIUS));
                if hit.is_some_and(|i| curve.remove(i).is_ok()) {
                    response.mark_changed();
                }
            }
        }

        // Hover clipboard, consumed so a later panel does not see it.
        if response.hovered() && !ui.ctx().text_edit_focused() {
            let (copy, paste) = ui.input_mut(|i| {
                let (mut copy, mut paste) = (false, None);
                i.events.retain(|e| match e {
                    egui::Event::Copy => {
                        copy = true;
                        false
                    }
                    egui::Event::Paste(text) => {
                        paste = Some(text.clone());
                        false
                    }
                    _ => true,
                });
                (copy, paste)
            });
            if copy {
                ui.ctx().copy_text(curve.to_text());
            }
            if let Some(Ok(pasted)) = paste.map(|t| Curve::parse(&t)) {
                if curve.assign(pasted, self.fixed_count) {
                    response.mark_changed();
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, theme::raised());
            let grid = Stroke::new(1.0, theme::faint());
            for i in 1..GRID_DIVS {
                let t = i as f32 / GRID_DIVS as f32;
                let x = rect.left() + t * rect.width();
                let y = rect.top() + t * rect.height();
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    grid,
                );
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    grid,
                );
            }
            painter.rect_stroke(rect, 2.0, grid, StrokeKind::Inside);
            let line: Vec<Pos2> = curve.points().iter().map(|&p| to_screen(rect, p)).collect();
            painter.add(Shape::line(line, Stroke::new(1.5, theme::accent())));
            let hover = response.hover_pos();
            for &p in curve.points() {
                let centre = to_screen(rect, p);
                let hot = hover.is_some_and(|h| h.distance(centre) <= HANDLE_RADIUS);
                let fill = if hot { theme::accent() } else { Color32::WHITE };
                painter.circle(centre, 3.5, fill, Stroke::new(1.0, theme::app_bg()));
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Curve {
        Curve::new(vec![(0.0, 0.0), (0.5, 0.8), (1.0, 1.0)]).unwrap()
    }

    #[test]
    fn new_validates_every_invariant() {
        assert_eq!(Curve::new(vec![(0.0, 0.0)]), Err(CurveError::TooFewPoints));
        assert_eq!(
            Curve::new(vec![(0.0, 0.0), (1.0, 1.5)]),
            Err(CurveError::OutOfRange)
        );
        assert_eq!(
            Curve::new(vec![(0.0, f32::NAN), (1.0, 1.0)]),
            Err(CurveError::OutOfRange)
        );
        assert_eq!(
            Curve::new(vec![(0.0, 0.0), (0.005, 0.5), (1.0, 1.0)]),
            Err(CurveError::NotMonotonic)
        );
        assert_eq!(
            Curve::new(vec![(0.5, 0.0), (0.2, 1.0)]),
            Err(CurveError::NotMonotonic)
        );
        assert!(Curve::new(vec![(0.2, 0.1), (0.9, 0.7)]).is_ok());
    }

    #[test]
    fn evaluate_is_clamped_piecewise_linear() {
        let c = three();
        assert_eq!(c.evaluate(0.25), 0.4);
        assert_eq!(c.evaluate(0.75), 0.9);
        assert_eq!(c.evaluate(0.5), 0.8);
        let inset = Curve::new(vec![(0.2, 0.1), (0.8, 0.7)]).unwrap();
        assert_eq!(inset.evaluate(0.0), 0.1);
        assert_eq!(inset.evaluate(1.0), 0.7);
        assert_eq!(inset.evaluate(f32::NAN), 0.1);
    }

    #[test]
    fn move_clamps_into_the_window_and_reports_change() {
        let mut c = three();
        // The middle point cannot pass its neighbours.
        assert!(c.move_point(1, 2.0, 0.5));
        assert_eq!(c.points()[1], (1.0 - MIN_GAP, 0.5));
        assert!(c.move_point(1, -1.0, -1.0));
        assert_eq!(c.points()[1], (MIN_GAP, 0.0));
        // Endpoints stay inside [0, 1] and before their neighbour.
        assert!(c.move_point(0, -0.5, 0.3));
        assert_eq!(c.points()[0], (0.0, 0.3));
        // Non-finite input keeps the old coordinate; no change = false.
        assert!(!c.move_point(0, f32::NAN, f32::INFINITY));
        assert!(!c.move_point(9, 0.5, 0.5));
        assert!(Curve::new(c.points().to_vec()).is_ok(), "still valid");
    }

    #[test]
    fn insert_and_remove_keep_the_invariant() {
        let mut c = three();
        assert_eq!(c.insert(0.25, 0.3), Ok(1));
        assert_eq!(c.points().len(), 4);
        assert_eq!(c.points()[1], (0.25, 0.3));
        // Too close to a neighbour, or past an end, is refused.
        assert_eq!(c.insert(0.255, 0.3), Err(CurveError::NotMonotonic));
        assert_eq!(c.insert(1.5, 0.3), Err(CurveError::NotMonotonic));
        let inset = &mut Curve::new(vec![(0.2, 0.0), (0.9, 1.0)]).unwrap();
        assert_eq!(inset.insert(0.1, 0.5), Err(CurveError::NotMonotonic));
        // Endpoints are protected; interior points go.
        assert_eq!(c.remove(0), Err(CurveError::Protected));
        assert_eq!(c.remove(3), Err(CurveError::Protected));
        assert_eq!(c.remove(1), Ok(()));
        assert_eq!(c, three());
        let mut two = Curve::default();
        assert_eq!(two.remove(1), Err(CurveError::Protected));
    }

    #[test]
    fn text_round_trips_and_rejects_bad_input() {
        let c = three();
        assert_eq!(c.to_text(), "0.0000,0.0000;0.5000,0.8000;1.0000,1.0000");
        assert_eq!(Curve::parse(&c.to_text()), Ok(c.clone()));
        assert_eq!(Curve::parse(" 0,0 ; 1,1 ;"), Ok(Curve::default()));
        assert_eq!(Curve::parse("0,0;banana"), Err(CurveError::Malformed));
        assert_eq!(Curve::parse("0,0;1"), Err(CurveError::Malformed));
        assert_eq!(Curve::parse("0,0;2,1"), Err(CurveError::OutOfRange));
        assert_eq!(Curve::parse(""), Err(CurveError::TooFewPoints));
    }

    #[test]
    fn assign_respects_a_fixed_count() {
        let mut c = three();
        assert!(!c.assign(Curve::default(), true), "count mismatch");
        assert_eq!(c, three());
        assert!(!c.assign(three(), false), "no change");
        assert!(c.assign(Curve::default(), false));
        assert_eq!(c, Curve::default());
    }

    #[test]
    fn presets_hit_their_shapes_at_the_samples() {
        let s = preset_smooth(4);
        assert_eq!(s.points().len(), 4);
        // Smoothstep(1/3) = 7/27.
        assert!((s.points()[1].1 - 7.0 / 27.0).abs() < 1e-6);
        let e = preset_ease(4);
        // Ease-out cubic(1/3) = 1 - (2/3)^3 = 19/27.
        assert!((e.points()[1].1 - 19.0 / 27.0).abs() < 1e-6);
        assert_eq!(preset_linear(2), Curve::default());
        assert!(Curve::new(preset_ease(7).points().to_vec()).is_ok());
        assert_eq!(Curve::sampled(1, |t| t).points().len(), 2);
    }

    #[test]
    fn screen_mapping_round_trips_and_picks_nearest() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(100.0));
        assert_eq!(to_screen(rect, (0.0, 0.0)), Pos2::new(10.0, 120.0));
        assert_eq!(to_screen(rect, (1.0, 1.0)), Pos2::new(110.0, 20.0));
        assert_eq!(from_screen(rect, Pos2::new(60.0, 70.0)), (0.5, 0.5));
        let c = three();
        // (0.5, 0.8) sits at (60, 40).
        assert_eq!(pick_point(&c, rect, Pos2::new(62.0, 41.0), 6.0), Some(1));
        assert_eq!(pick_point(&c, rect, Pos2::new(80.0, 80.0), 6.0), None);
    }

    /// One headless frame over a curve editor at a known spot.
    fn frame(
        ctx: &egui::Context,
        curve: &mut Curve,
        fixed: bool,
        events: Vec<egui::Event>,
    ) -> (bool, Rect) {
        let input = egui::RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0))),
            ..Default::default()
        };
        let (mut changed, mut rect) = (false, Rect::NOTHING);
        let mut out = ctx.run_ui(input, |ui| {
            let r = CurveEdit::new(curve)
                .size(100.0)
                .fixed_count(fixed)
                .show(ui);
            changed = r.changed();
            rect = r.rect;
        });
        out.textures_delta.clear();
        (changed, rect)
    }

    fn button(at: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn headless_click_adds_and_right_click_removes() {
        let ctx = egui::Context::default();
        let mut c = Curve::default();
        let (_, rect) = frame(&ctx, &mut c, false, Vec::new());
        let at = to_screen(rect, (0.25, 0.75));
        let primary = egui::PointerButton::Primary;
        frame(
            &ctx,
            &mut c,
            false,
            vec![egui::Event::PointerMoved(at), button(at, primary, true)],
        );
        let (changed, _) = frame(&ctx, &mut c, false, vec![button(at, primary, false)]);
        assert!(changed);
        assert_eq!(c.points().len(), 3);
        assert!((c.points()[1].0 - 0.25).abs() < 0.02 && (c.points()[1].1 - 0.75).abs() < 0.02);
        // The same click in fixed-count mode adds nothing.
        let mut fixed = Curve::default();
        frame(&ctx, &mut fixed, true, vec![button(at, primary, true)]);
        frame(&ctx, &mut fixed, true, vec![button(at, primary, false)]);
        assert_eq!(fixed.points().len(), 2);
        // Right-click on the new handle removes it.
        let secondary = egui::PointerButton::Secondary;
        let handle = to_screen(rect, c.points()[1]);
        frame(
            &ctx,
            &mut c,
            false,
            vec![
                egui::Event::PointerMoved(handle),
                button(handle, secondary, true),
            ],
        );
        frame(&ctx, &mut c, false, vec![button(handle, secondary, false)]);
        assert_eq!(c, Curve::default());
    }

    #[test]
    fn headless_paste_replaces_the_curve() {
        let ctx = egui::Context::default();
        let mut c = Curve::default();
        let (_, rect) = frame(&ctx, &mut c, false, Vec::new());
        frame(
            &ctx,
            &mut c,
            false,
            vec![egui::Event::PointerMoved(rect.center())],
        );
        let (changed, _) = frame(
            &ctx,
            &mut c,
            false,
            vec![egui::Event::Paste(three().to_text())],
        );
        assert!(changed);
        assert_eq!(c, three());
        // A junk paste changes nothing.
        frame(&ctx, &mut c, false, vec![egui::Event::Paste("junk".into())]);
        assert_eq!(c, three());
    }

    #[test]
    fn the_curve_canvas_reports_a_slider_role_to_accesskit() {
        use egui_kittest::kittest::Queryable;
        let mut curve = Curve::default();
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            CurveEdit::new(&mut curve).size(120.0).show(ui);
        });
        harness.run();
        let _ = harness.get_by_role_and_label(egui::accesskit::Role::Slider, "curve");
    }
}
