//! Paint paths (§2 line 26's P1 row: "Path tools (paint/erase/smudge
//! along 3D curve; per-vertex size+opacity; filled path)").
//!
//! A [`PaintPath`] is an ordered point list replayed as ONE stroke through
//! the EXISTING pipeline: [`sample_events`](PaintPath::sample_events)
//! converts the points into the [`StrokeEvent`](crate::StrokeEvent) stream,
//! and [`replay`](PaintPath::replay) feeds that stream through a
//! [`StrokeConditioner`](crate::StrokeConditioner) (filter → lazy mouse →
//! line constraint → spacing). No new composite math anywhere: paint dabs
//! keep flowing into [`DabAdapter`](crate::DabAdapter)'s splat mapping,
//! erase/smudge dabs into the [`retouch`](crate::retouch) CPU ops.
//!
//! # The two point sources (deliverable 1)
//!
//! * The 3D curve sampled on the mesh surface: the raycast walk
//!   ([`umber_mesh::raycast`](https://docs.rs/umber_mesh) —
//!   `walk_surface_path` there casts one ray per curve sample and keeps
//!   the hits' UVs). This crate stays mesh-free, so the walk lives in
//!   `umber-mesh` and its output feeds [`PaintPath::from_uvs`]; the UV
//!   space here is the same global-UV space the conditioner consumes
//!   (tile offsets included, exactly like
//!   [`FillMask::polygon`](https://docs.rs/umber_mesh)'s loop).
//! * Hand-drawn in UV space: the 2D UV view's path editor pushes points
//!   directly ([`PaintPath::push_point`]).
//!
//! # Per-vertex size+opacity (deliverable 1)
//!
//! Each point carries its own `size` (a radius scale factor, `1.0` = the
//! session default) and `opacity` (a flow multiplier). The interpolation
//! between points drives the replay WITHOUT new plumbing:
//!
//! * `opacity` folds into the event pressure
//!   (`pressure * opacity`, saturated): both are flow multipliers into the
//!   adapter's `alpha`, so spacing's linear pressure interpolation and the
//!   adapter's `pressure_gamma` shaping vary opacity along the stroke for
//!   free — including through wash-mode renormalization downstream.
//! * `size` selects the per-segment spacing radius: the segment ending at
//!   point `i + 1` is spaced (and adapted) with
//!   `base_radius * size[i + 1]`. Spacing already takes its radius per
//!   call, so a varying radius mid-stroke is existing behavior.
//!
//! # Op modes (deliverable 2)
//!
//! [`PathOp`] is paint / erase / smudge. It maps onto [`BrushTool`] 1:1
//! ([`PathOp::to_brush_tool`]) so the app routes the replayed dabs to the
//! right consumer with the same branch it already has (paint → GPU splat,
//! erase/smudge → the per-stroke CPU mirror). Clone is deliberately NOT a
//! path mode: a clone dab samples `cursor + offset` from a live source
//! anchor, which is meaningless on a replayed (non-interactive) stroke.
//!
//! # Filled path (deliverable 3)
//!
//! The closed-path case reuses the polygon fill's even-odd rasterizer
//! (`umber_mesh::polygon_fill`): [`PaintPath::loop_uv`] hands the loop to
//! `FillMask::polygon` / `rasterize_polygon_mask` at the call site. This
//! crate only owns the loop extraction and the closed-≥3-points gate, so
//! the boundary-inclusive parity rule lives in exactly one place.
//!
//! # Conventions
//!
//! * [`PATH_MAX_POINTS`] caps the list at 256, mirroring
//!   `umber_mesh::LASSO_MAX_POINTS`: further pushes are refused (`false`),
//!   never an error.
//! * [`PATH_EVENT_STEP_NS`] (8 ms) matches the brush tests' and the paint
//!   session's synthetic event clock: the conditioner needs strictly
//!   advancing timestamps, and sharing the regime keeps replayed strokes
//!   in the same filter regime as live ones.
//! * Degenerate inputs degrade, never panic: non-finite UVs are refused at
//!   push time; non-finite sizes sanitize to `1.0`; an empty or
//!   single-point path replays to zero dabs (the seeding event emits
//!   nothing — the [`StrokeConditioner`] contract).

use serde::{Deserialize, Serialize};

use crate::{
    retouch::{BrushTool, EraseParams, SmudgeParams},
    DabPlan, StrokeConditioner, StrokeEvent,
};

/// The path-point cap (v1): further pushes are refused, never an error —
/// mirrors `umber_mesh::LASSO_MAX_POINTS` so a path and a lasso loop cost
/// the same order of per-texel work when filled.
pub const PATH_MAX_POINTS: usize = 256;

/// Synthetic event-clock step between replayed points (nominal
/// nanoseconds): the same 8 ms regime as the brush tests' and the paint
/// session's clock, so the filter sees the same regime live and replayed.
pub const PATH_EVENT_STEP_NS: u64 = 8_000_000;

/// Radius-scale clamp: a point smaller than this would starve spacing
/// (near-zero step → the `MAX_DABS_PER_CALL` cap), larger is a wash.
pub const PATH_MIN_SIZE: f32 = 0.05;
/// Radius-scale clamp, upper end.
pub const PATH_MAX_SIZE: f32 = 8.0;

/// One path vertex: where, and how strongly / how large.
///
/// `uv` is global UV (tile offsets included — the same space the stroke
/// conditioner consumes and `FillMask::polygon` loops live in).
/// `pressure` is the raw stylus pressure `0..=1`; `size` the radius scale
/// (`1.0` = session default); `opacity` the flow multiplier `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathPoint {
    /// Global-UV position.
    pub uv: [f32; 2],
    /// Raw stylus pressure, `0..=1` (saturated at use).
    pub pressure: f32,
    /// Radius scale factor, `1.0` = the session default (sanitized at use).
    pub size: f32,
    /// Flow multiplier, `0..=1` (saturated at use).
    pub opacity: f32,
}

impl PathPoint {
    /// Builds a point (stored raw; saturation/sanitization happens at
    /// replay so the editor keeps what the user set).
    pub fn new(uv: [f32; 2], pressure: f32, size: f32, opacity: f32) -> Self {
        Self {
            uv,
            pressure,
            size,
            opacity,
        }
    }

    /// The event pressure this point replays as: `pressure * opacity`
    /// saturated to `0..=1` (non-finite → `0.0`). Both factors are flow
    /// multipliers into the adapter's dab alpha, so folding them here
    /// reuses spacing's interpolation with no new math.
    pub fn effective_pressure(&self) -> f32 {
        saturate01(self.pressure * self.opacity)
    }

    /// The radius scale this point replays as (see [`sanitize_size`]).
    pub fn sanitized_size(&self) -> f32 {
        sanitize_size(self.size)
    }
}

/// Clamps a radius scale to `PATH_MIN_SIZE..=PATH_MAX_SIZE`; non-finite
/// values become `1.0` (the session default — a corrupt point replays as
/// an ordinary dab, never a spacing starvation or a wash).
pub fn sanitize_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(PATH_MIN_SIZE, PATH_MAX_SIZE)
    } else {
        1.0
    }
}

/// Finite values clamp to `0..=1`; non-finite values become `0.0`.
fn saturate01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The replayed stroke: an ordered [`PathPoint`] list plus the closed-loop
/// flag (which appends the sealing segment on replay and gates the fill).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaintPath {
    points: Vec<PathPoint>,
    closed: bool,
}

impl PaintPath {
    /// An empty, open path.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a path from global UVs (the raycast walk's output, or a
    /// restored capture): every point replays at full pressure, default
    /// size and full opacity. Non-finite UVs are skipped; past
    /// [`PATH_MAX_POINTS`] the rest are dropped (never an error).
    pub fn from_uvs(uvs: &[[f32; 2]]) -> Self {
        let mut path = Self::new();
        for uv in uvs {
            if !path.push_point(*uv, 1.0, 1.0, 1.0) && path.len() >= PATH_MAX_POINTS {
                break;
            }
        }
        path
    }

    /// Appends a point: refused (`false`) while keeping the old list when
    /// the cap is reached or either coordinate is non-finite.
    pub fn push_point(&mut self, uv: [f32; 2], pressure: f32, size: f32, opacity: f32) -> bool {
        if self.points.len() >= PATH_MAX_POINTS {
            return false;
        }
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return false;
        }
        self.points
            .push(PathPoint::new(uv, pressure, size, opacity));
        true
    }

    /// Inserts a point at `index` (clamped to the end): same refusal rules
    /// as [`Self::push_point`].
    pub fn insert_point(
        &mut self,
        index: usize,
        uv: [f32; 2],
        pressure: f32,
        size: f32,
        opacity: f32,
    ) -> bool {
        if self.points.len() >= PATH_MAX_POINTS {
            return false;
        }
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return false;
        }
        let index = index.min(self.points.len());
        self.points
            .insert(index, PathPoint::new(uv, pressure, size, opacity));
        true
    }

    /// Removes the point at `index`; `false` when out of bounds.
    pub fn remove_point(&mut self, index: usize) -> bool {
        if index >= self.points.len() {
            return false;
        }
        self.points.remove(index);
        true
    }

    /// Moves the point at `index` to `uv`; `false` on out-of-bounds or
    /// non-finite coordinates (the point keeps its old position).
    pub fn move_point(&mut self, index: usize, uv: [f32; 2]) -> bool {
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return false;
        }
        match self.points.get_mut(index) {
            Some(p) => {
                p.uv = uv;
                true
            }
            None => false,
        }
    }

    /// Restyles the point at `index` (pressure/size/opacity stored raw —
    /// saturation applies at replay); `false` when out of bounds.
    pub fn restyle_point(&mut self, index: usize, pressure: f32, size: f32, opacity: f32) -> bool {
        match self.points.get_mut(index) {
            Some(p) => {
                p.pressure = pressure;
                p.size = size;
                p.opacity = opacity;
                true
            }
            None => false,
        }
    }

    /// Drops every point (the closed flag is kept — it belongs to the
    /// editing session, like the lasso tool's mode).
    pub fn clear_points(&mut self) {
        self.points.clear();
    }

    /// The points in replay order.
    pub fn points(&self) -> &[PathPoint] {
        &self.points
    }

    /// How many points are captured.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether any point is captured.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Whether the path seals last→first on replay (and is fill-eligible).
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Arms or disarms the sealing segment.
    pub fn set_closed(&mut self, closed: bool) {
        self.closed = closed;
    }

    /// Whether the path can replay as a stroke (≥ 2 points — a single
    /// point only seeds the conditioner and emits nothing).
    pub fn can_replay(&self) -> bool {
        self.points.len() >= 2
    }

    /// Whether the path can fill (a closed loop of ≥ 3 points — the
    /// polygon rasterizer's degenerate-loop rule).
    pub fn can_fill(&self) -> bool {
        self.closed && self.points.len() >= 3
    }

    /// The closed-loop UVs for `FillMask::polygon` / the even-odd
    /// rasterizer (an empty vec unless [`Self::can_fill`]).
    pub fn loop_uv(&self) -> Vec<[f32; 2]> {
        if !self.can_fill() {
            return Vec::new();
        }
        self.points.iter().map(|p| p.uv).collect()
    }

    /// The mean replay style `(effective pressure, sanitized size)` over
    /// the points — what the filled-path walk stamps its interior scan
    /// rows with. Empty paths report the neutral `(1.0, 1.0)`.
    pub fn mean_style(&self) -> (f32, f32) {
        if self.points.is_empty() {
            return (1.0, 1.0);
        }
        let n = self.points.len() as f32;
        let pressure = self
            .points
            .iter()
            .map(PathPoint::effective_pressure)
            .sum::<f32>()
            / n;
        let size = self
            .points
            .iter()
            .map(PathPoint::sanitized_size)
            .sum::<f32>()
            / n;
        (pressure, size)
    }

    /// Converts the path into the stroke event stream: one event per
    /// point (position = the point's UV, pressure = its folded effective
    /// pressure), timestamps strictly advancing from
    /// [`PATH_EVENT_STEP_NS`]. A closed path appends its first point's
    /// event so the sealing segment replays through the same spacing.
    pub fn sample_events(&self) -> Vec<StrokeEvent> {
        let mut events = Vec::with_capacity(self.points.len() + 1);
        for p in &self.points {
            events.push(event_for(p, (events.len() as u64 + 1) * PATH_EVENT_STEP_NS));
        }
        if self.closed {
            if let Some(first) = self.points.first() {
                events.push(event_for(
                    first,
                    (events.len() as u64 + 1) * PATH_EVENT_STEP_NS,
                ));
            }
        }
        events
    }

    /// Replays the path through `conditioner` (reset first, so a replay
    /// never inherits a live stroke's residual): each event is spaced with
    /// `base_radius * <target point size>`, and every emitted [`DabPlan`]
    /// is returned with the radius that spaced it. Deterministic: the same
    /// path through equally constructed conditioners yields identical
    /// [`PathDab`]s.
    pub fn replay(&self, conditioner: &mut StrokeConditioner, base_radius: f32) -> Vec<PathDab> {
        conditioner.reset();
        let mut sequence: Vec<PathPoint> = self.points.clone();
        if self.closed {
            // The sealing event (absent on an empty path — `first` is
            // None there and the replay degrades to zero dabs).
            if let Some(first) = self.points.first() {
                sequence.push(*first);
            }
        }
        let mut out = Vec::new();
        for (i, point) in sequence.iter().enumerate() {
            let event = event_for(point, (i as u64 + 1) * PATH_EVENT_STEP_NS);
            let radius = base_radius * point.sanitized_size();
            for plan in conditioner.push(&event, radius) {
                out.push(PathDab {
                    pos: plan.pos,
                    alpha_multiplier: plan.alpha_multiplier,
                    radius,
                });
            }
        }
        out
    }
}

/// One replayed dab: the spacing plan plus the per-dab radius that the
/// point-size interpolation selected for its segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathDab {
    /// Dab center, in the path's UV space.
    pub pos: [f32; 2],
    /// Pressure-interpolated flow multiplier (carries the folded
    /// per-point opacity — see [`PathPoint::effective_pressure`]).
    pub alpha_multiplier: f32,
    /// `base_radius * <segment target size>` (the radius spacing used).
    pub radius: f32,
}

impl From<PathDab> for DabPlan {
    fn from(dab: PathDab) -> DabPlan {
        DabPlan {
            pos: dab.pos,
            alpha_multiplier: dab.alpha_multiplier,
        }
    }
}

/// The replay op mode: paint / erase / smudge along the path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum PathOp {
    /// The stamp (the GPU splat pass).
    #[default]
    Paint,
    /// The CPU eraser op, with these channel flags.
    Erase(EraseParams),
    /// The CPU smudge op, with these params.
    Smudge(SmudgeParams),
}

impl PathOp {
    /// Whether this mode's dabs run through the CPU retouch path instead
    /// of the GPU splat (mirrors [`BrushTool::is_retouch`], 1:1).
    pub fn is_retouch(&self) -> bool {
        self.to_brush_tool().is_retouch()
    }

    /// The brush tool the replayed dabs run as: the app routes the replay
    /// through the consumer it already uses for that tool — no new branch.
    pub fn to_brush_tool(&self) -> BrushTool {
        match *self {
            Self::Paint => BrushTool::Paint,
            Self::Erase(params) => BrushTool::Erase(params),
            Self::Smudge(params) => BrushTool::Smudge(params),
        }
    }
}

/// One point's stroke event (position in path-UV space, folded pressure).
fn event_for(point: &PathPoint, time_ns: u64) -> StrokeEvent {
    StrokeEvent {
        pos: point.uv,
        pressure: point.effective_pressure(),
        tilt: None,
        time_ns,
        in_proximity: true,
        contact: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OneEuroParams;

    const EPS: f32 = 1e-5;

    fn conditioner() -> StrokeConditioner {
        StrokeConditioner::new(OneEuroParams::default(), OneEuroParams::default(), 0.0, 2.0)
            .expect("valid params")
    }

    /// A straight 200-px path with per-point style ramps.
    fn styled_path() -> PaintPath {
        let mut path = PaintPath::new();
        // Geometry steps of 40 px (5 points); pressure full, size ramps
        // 0.5 → 2.0, opacity ramps 0.2 → 1.0.
        for i in 0..5 {
            let t = i as f32 / 4.0;
            assert!(path.push_point([40.0 * i as f32, 0.0], 1.0, 0.5 + 1.5 * t, 0.2 + 0.8 * t,));
        }
        path
    }

    #[test]
    fn same_path_through_equal_conditioners_replays_identical() {
        // Deliverable 5: replay determinism — the whole PathDab vec
        // compares bitwise-equal (same binary ops, no clock reads).
        let path = styled_path();
        let a = path.replay(&mut conditioner(), 10.0);
        let b = path.replay(&mut conditioner(), 10.0);
        assert!(!a.is_empty(), "a 160 px path must emit dabs");
        assert_eq!(a, b, "same path must replay to identical dabs");
        // And replaying again on the SAME conditioner (reset inside)
        // matches too — no residual leaks across replays.
        let mut c = conditioner();
        let first = path.replay(&mut c, 10.0);
        let second = path.replay(&mut c, 10.0);
        assert_eq!(first, second);
        assert_eq!(first, a);
    }

    #[test]
    fn degenerate_paths_emit_nothing() {
        assert!(PaintPath::new().replay(&mut conditioner(), 10.0).is_empty());
        let mut single = PaintPath::new();
        assert!(single.push_point([5.0, 5.0], 1.0, 1.0, 1.0));
        assert!(
            single.replay(&mut conditioner(), 10.0).is_empty(),
            "one point only seeds the conditioner"
        );
        assert!(!single.can_replay());
        assert!(!single.can_fill());
        assert!(single.loop_uv().is_empty());
    }

    #[test]
    fn per_point_size_selects_the_segment_radius() {
        // Deliverable 5: size interpolation — a single 200-px segment
        // ending at a size-2.0 point spaces every dab at 2× the base.
        let mut path = PaintPath::new();
        assert!(path.push_point([0.0, 0.0], 1.0, 0.5, 1.0));
        assert!(path.push_point([200.0, 0.0], 1.0, 2.0, 1.0));
        let dabs = path.replay(&mut conditioner(), 10.0);
        assert!(!dabs.is_empty());
        for dab in &dabs {
            assert!(
                (dab.radius - 20.0).abs() <= EPS,
                "every dab of the segment carries base * target size, got {}",
                dab.radius
            );
        }
        // And a multi-segment path carries each segment's own radius: the
        // last segment's dabs hold the last point's size.
        let dabs = styled_path().replay(&mut conditioner(), 8.0);
        let last_size = 2.0;
        assert!(
            dabs.iter()
                .any(|d| (d.radius - 8.0 * last_size).abs() <= EPS),
            "the last segment must space at the last point's size"
        );
        for dab in &dabs {
            let size = dab.radius / 8.0;
            assert!(
                (0.5 - EPS..=2.0 + EPS).contains(&size),
                "every radius comes from the size ramp, got {size}"
            );
        }
    }

    #[test]
    fn per_point_opacity_varies_along_the_stroke() {
        // Deliverable 5: opacity interpolation — uniform dim vs. uniform
        // full paths: every dim dab sits strictly under every full dab
        // (the fold is linear, so the filter regime is shared); a ramped
        // path's tail exceeds its head.
        let mut dim = PaintPath::new();
        let mut full = PaintPath::new();
        for i in 0..4 {
            assert!(dim.push_point([50.0 * i as f32, 0.0], 1.0, 1.0, 0.25));
            assert!(full.push_point([50.0 * i as f32, 0.0], 1.0, 1.0, 1.0));
        }
        let dim_dabs = dim.replay(&mut conditioner(), 10.0);
        let full_dabs = full.replay(&mut conditioner(), 10.0);
        assert_eq!(dim_dabs.len(), full_dabs.len());
        assert!(!dim_dabs.is_empty());
        let (dim_max, full_min) = (
            dim_dabs
                .iter()
                .map(|d| d.alpha_multiplier)
                .fold(0.0f32, f32::max),
            full_dabs
                .iter()
                .map(|d| d.alpha_multiplier)
                .fold(1.0f32, f32::min),
        );
        assert!(
            dim_max < full_min,
            "dim ({dim_max}) must sit under full ({full_min})"
        );
        // The ramp: tail exceeds head.
        let ramp = styled_path().replay(&mut conditioner(), 10.0);
        assert!(
            ramp.last().expect("dabs").alpha_multiplier
                > ramp.first().expect("dabs").alpha_multiplier,
            "opacity must vary along the stroke"
        );
        // The fold itself is exact at the event layer.
        let events = styled_path().sample_events();
        assert!((events[0].pressure - 0.2).abs() <= EPS);
        assert!((events[4].pressure - 1.0).abs() <= EPS);
    }

    #[test]
    fn closed_path_appends_the_sealing_event() {
        let mut open = styled_path();
        assert_eq!(open.sample_events().len(), 5);
        assert!(!open.can_fill(), "open paths never fill");
        open.set_closed(true);
        assert!(open.is_closed());
        let events = open.sample_events();
        assert_eq!(events.len(), 6, "closed appends the seal");
        assert_eq!(events[5].pos, events[0].pos);
        assert_eq!(events[5].pressure, events[0].pressure);
        // Timestamps strictly advance (the conditioner regime).
        for w in events.windows(2) {
            assert!(w[1].time_ns > w[0].time_ns);
        }
        assert!(open.can_fill(), "closed ≥ 3 points fills");
        assert_eq!(open.loop_uv().len(), 5);
        // Two closed points replay (the seal is a real segment) but never
        // fill (the rasterizer's degenerate-loop rule).
        let mut tiny = PaintPath::new();
        assert!(tiny.push_point([0.0, 0.0], 1.0, 1.0, 1.0));
        assert!(tiny.push_point([40.0, 0.0], 1.0, 1.0, 1.0));
        tiny.set_closed(true);
        assert!(!tiny.can_fill());
        assert!(tiny.loop_uv().is_empty());
    }

    #[test]
    fn point_cap_and_degenerate_pushes_refuse() {
        let mut path = PaintPath::new();
        for i in 0..PATH_MAX_POINTS {
            assert!(path.push_point([i as f32, 0.0], 1.0, 1.0, 1.0));
        }
        assert_eq!(path.len(), PATH_MAX_POINTS);
        assert!(!path.push_point([999.0, 0.0], 1.0, 1.0, 1.0));
        assert!(!path.insert_point(0, [999.0, 0.0], 1.0, 1.0, 1.0));
        assert_eq!(path.len(), PATH_MAX_POINTS);
        assert!(!path.push_point([f32::NAN, 0.0], 1.0, 1.0, 1.0));
        assert!(!path.move_point(0, [0.0, f32::INFINITY]));
        assert_eq!(path.points()[0].uv, [0.0, 0.0]);
        assert!(!path.move_point(999, [1.0, 1.0]));
        assert!(!path.remove_point(999));
        assert!(!path.restyle_point(999, 1.0, 1.0, 1.0));
    }

    #[test]
    fn three_op_modes_route_to_the_right_consumer() {
        // Deliverable 5: paint → the splat (not retouch); erase/smudge →
        // the CPU mirror — via the SAME predicate the session branches on.
        assert!(!PathOp::Paint.is_retouch());
        assert_eq!(PathOp::Paint.to_brush_tool(), BrushTool::Paint);
        let erase = EraseParams::default();
        assert!(PathOp::Erase(erase).is_retouch());
        assert_eq!(
            PathOp::Erase(erase).to_brush_tool(),
            BrushTool::Erase(erase)
        );
        let smudge = SmudgeParams::default();
        assert!(PathOp::Smudge(smudge).is_retouch());
        assert_eq!(
            PathOp::Smudge(smudge).to_brush_tool(),
            BrushTool::Smudge(smudge)
        );
        // The mapping preserves the params (no silent defaulting).
        let custom = EraseParams {
            channels: crate::retouch::EraseChannels {
                r: true,
                g: false,
                b: false,
                a: false,
            },
        };
        assert_eq!(
            PathOp::Erase(custom).to_brush_tool(),
            BrushTool::Erase(custom)
        );
    }

    #[test]
    fn sanitize_size_clamps_and_neutralizes() {
        assert!((sanitize_size(1.0) - 1.0).abs() <= EPS);
        assert!((sanitize_size(0.0) - PATH_MIN_SIZE).abs() <= EPS);
        assert!((sanitize_size(-3.0) - PATH_MIN_SIZE).abs() <= EPS);
        assert!((sanitize_size(100.0) - PATH_MAX_SIZE).abs() <= EPS);
        assert!((sanitize_size(f32::NAN) - 1.0).abs() <= EPS);
        assert!((sanitize_size(f32::INFINITY) - 1.0).abs() <= EPS);
        // effective_pressure folds and saturates (non-finite → 0).
        let p = PathPoint::new([0.0, 0.0], 0.8, 1.0, 0.5);
        assert!((p.effective_pressure() - 0.4).abs() <= EPS);
        let bad = PathPoint::new([0.0, 0.0], f32::NAN, 1.0, 1.0);
        assert_eq!(bad.effective_pressure(), 0.0);
    }

    #[test]
    fn from_uvs_skips_non_finite_and_caps() {
        let path = PaintPath::from_uvs(&[[0.0, 0.0], [f32::NAN, 0.0], [1.0, 1.0]]);
        assert_eq!(path.len(), 2);
        assert_eq!(path.points()[1].uv, [1.0, 1.0]);
        let many = vec![[0.5, 0.5]; PATH_MAX_POINTS + 10];
        assert_eq!(PaintPath::from_uvs(&many).len(), PATH_MAX_POINTS);
    }

    #[test]
    fn mean_style_is_neutral_when_empty() {
        assert_eq!(PaintPath::new().mean_style(), (1.0, 1.0));
        let mut path = PaintPath::new();
        assert!(path.push_point([0.0, 0.0], 1.0, 2.0, 0.5));
        assert!(path.push_point([1.0, 0.0], 1.0, 4.0, 1.0));
        // Pressures 0.5, 1.0 → mean 0.75; sizes 2.0, 4.0 → mean 3.0.
        let (pressure, size) = path.mean_style();
        assert!((pressure - 0.75).abs() <= EPS);
        assert!((size - 3.0).abs() <= EPS);
    }
}
