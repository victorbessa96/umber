//! Straight-line constraint + angle snap (§2 line 23's P1 row).
//!
//! A stroke-space filter in the [`StrokeConditioner`](crate::StrokeConditioner)
//! chain, the same shape as [`LazyMouse`](crate::LazyMouse): per-stroke
//! anchor state plus a pure position map. The hook sits AFTER the one-euro
//! filter and lazy mouse and BEFORE spacing, so the snapped positions are
//! what the dab planner sees — spacing and the dab adapter downstream are
//! untouched (the constraint is purely upstream stamp conditioning).
//!
//! * [`LineConstraintMode::Off`]: the identity — bit-identical passthrough,
//!   not even the anchor is touched.
//! * [`LineConstraintMode::Line`]: the Shift-like straight line from the
//!   stroke's first stamp (the anchor) to the cursor. The endpoint projects
//!   onto its own anchor→cursor line, i.e. the identity; the mode's value is
//!   the fixed anchor (the live preview line's origin) and the guarantee
//!   that every spacing stamp is exactly collinear with it.
//! * [`LineConstraintMode::AngleSnap`]: the same line, but its direction is
//!   first quantized to the nearest multiple of `snap_deg` (full-circle
//!   quantization, so dragging back past the anchor snaps to the ray the
//!   cursor is actually on) and the cursor is projected onto that ray.
//!
//! The anchor is the first stamp pushed after [`LineConstraint::reset`]
//! (which [`StrokeConditioner`](crate::StrokeConditioner) calls per stroke,
//! so the constraint holds until stroke end and never leaks across
//! strokes). Deterministic: the same mode + event sequence always yields
//! the same positions.

use glam::Vec2;

/// The default angle-snap step in degrees: the classic 15°
/// (Photoshop/Krita convention).
pub const ANGLE_SNAP_DEFAULT_DEG: f32 = 15.0;

/// The snap-step range the panel offers, in degrees.
pub const ANGLE_SNAP_MIN_DEG: f32 = 1.0;
/// The snap-step range the panel offers, in degrees.
pub const ANGLE_SNAP_MAX_DEG: f32 = 90.0;

/// How a stroke's stamps are constrained to straight lines.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineConstraintMode {
    /// No constraint: the filter is the identity (bit-identical passthrough).
    #[default]
    Off,
    /// Shift-like straight line: anchor (first stamp) → cursor.
    Line,
    /// Anchored line with the direction quantized to multiples of
    /// `snap_deg` degrees before projection.
    AngleSnap {
        /// Quantization step in degrees. Non-finite or `<= 0.0` values
        /// disable snapping ([`Line`](LineConstraintMode::Line) behavior —
        /// see [`sanitize_snap_deg`]); the panel clamps to
        /// `ANGLE_SNAP_MIN_DEG..=ANGLE_SNAP_MAX_DEG`.
        snap_deg: f32,
    },
}

impl LineConstraintMode {
    /// Whether any constraint is on (off = the caller skips the filter
    /// entirely: the zero-cost, byte-identical path).
    #[must_use]
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// The stateful stroke filter: the mode plus the per-stroke anchor.
///
/// The same shape as [`LazyMouse`](crate::LazyMouse): construct (or
/// [`set_mode`](Self::set_mode)), push each smoothed cursor through
/// [`apply`](Self::apply), [`reset`](Self::reset) between strokes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineConstraint {
    mode: LineConstraintMode,
    /// The stroke's first stamp (`None` until the first [`apply`](Self::apply)
    /// after construction or [`reset`](Self::reset)).
    anchor: Option<[f32; 2]>,
}

impl LineConstraint {
    /// Build a filter in `mode`, unanchored.
    #[must_use]
    pub fn new(mode: LineConstraintMode) -> Self {
        Self { mode, anchor: None }
    }

    /// Switch modes. The anchor is kept: a mid-stroke switch re-projects
    /// the rest of the stroke from the same anchor (the paint session only
    /// syncs between strokes, so this only fires through the direct API).
    pub fn set_mode(&mut self, mode: LineConstraintMode) {
        self.mode = mode;
    }

    /// The current mode.
    #[must_use]
    pub fn mode(&self) -> LineConstraintMode {
        self.mode
    }

    /// Drop the anchor: the next [`apply`](Self::apply) re-anchors.
    pub fn reset(&mut self) {
        self.anchor = None;
    }

    /// The stroke anchor, if the stroke has started.
    #[must_use]
    pub fn anchor(&self) -> Option<[f32; 2]> {
        self.anchor
    }

    /// Constrain `target` (the smoothed cursor): the first call anchors and
    /// passes through; later calls project per [`constrain_point`]. `Off`
    /// returns `target` untouched (bit-identical, without even anchoring).
    /// Non-finite targets hold the anchor rather than propagating NaN
    /// downstream (unanchored: the target itself, as there is nothing
    /// finite to hold).
    #[must_use]
    pub fn apply(&mut self, target: [f32; 2]) -> [f32; 2] {
        if matches!(self.mode, LineConstraintMode::Off) {
            return target;
        }
        if !Vec2::new(target[0], target[1]).is_finite() {
            return self.anchor.unwrap_or(target);
        }
        let Some(anchor) = self.anchor else {
            self.anchor = Some(target);
            return target;
        };
        constrain_point(anchor, target, self.mode)
    }
}

impl Default for LineConstraint {
    fn default() -> Self {
        Self::new(LineConstraintMode::Off)
    }
}

/// Project `target` onto the anchored line for `mode`: the pure,
/// stateless half of [`LineConstraint::apply`] (the anchor is a parameter,
/// so this is unit-testable without driving the filter).
///
/// * `Off` / `Line`: the identity — a cursor projects onto its own
///   anchor→cursor line.
/// * `AngleSnap`: the anchor→cursor direction is quantized to the nearest
///   multiple of the sanitized snap step over the full circle (a degenerate
///   step — non-finite or `<= 0.0` — passes through like [`Line`](LineConstraintMode::Line),
///   then the cursor is projected onto that ray (`anchor + (v·d̂) d̂`, clamped to
///   `≥ 0` so stamps never jump behind the anchor). Zero-length vectors
///   return the anchor (no direction exists).
#[must_use]
pub fn constrain_point(anchor: [f32; 2], target: [f32; 2], mode: LineConstraintMode) -> [f32; 2] {
    match mode {
        LineConstraintMode::Off | LineConstraintMode::Line => target,
        LineConstraintMode::AngleSnap { snap_deg } => {
            let dx = target[0] - anchor[0];
            let dy = target[1] - anchor[1];
            if dx == 0.0 && dy == 0.0 {
                return anchor;
            }
            let step_deg = sanitize_snap_deg(snap_deg);
            if step_deg <= 0.0 {
                return target;
            }
            let step = step_deg.to_radians();
            let snapped = (dy.atan2(dx) / step).round() * step;
            let (s, c) = snapped.sin_cos();
            let along = (dx * c + dy * s).max(0.0);
            [anchor[0] + c * along, anchor[1] + s * along]
        }
    }
}

/// A usable snap step in degrees: finite and strictly positive inputs pass
/// through, everything else becomes `0.0` — which disables snapping (see
/// [`constrain_point`): the [`LazyMouse`](crate::LazyMouse) sanitize
/// convention (invalid input disables the effect rather than inventing
/// one). [`ANGLE_SNAP_DEFAULT_DEG`] is only the panel's initial slider
/// value, not a fallback.
#[must_use]
pub fn sanitize_snap_deg(snap_deg: f32) -> f32 {
    if snap_deg.is_finite() && snap_deg > 0.0 {
        snap_deg
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-3;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() <= EPS && (a[1] - b[1]).abs() <= EPS
    }

    #[test]
    fn off_mode_passes_through_byte_identical_without_anchoring() {
        let mut c = LineConstraint::new(LineConstraintMode::Off);
        let p = [13.25, -7.5];
        assert_eq!(c.apply(p), p, "off must be bitwise identity");
        assert_eq!(c.apply(p), p);
        assert_eq!(c.anchor(), None, "off must not even seed the anchor");
    }

    #[test]
    fn line_mode_seeds_anchor_then_passes_through() {
        let mut c = LineConstraint::new(LineConstraintMode::Line);
        assert_eq!(c.apply([2.0, 3.0]), [2.0, 3.0]);
        assert_eq!(c.anchor(), Some([2.0, 3.0]));
        // The endpoint projects onto its own anchor→cursor line: identity.
        assert_eq!(c.apply([9.0, -4.0]), [9.0, -4.0]);
    }

    #[test]
    fn forty_five_degree_drag_with_fifteen_snap_lands_on_forty_five() {
        // 45° is a multiple of 15°, so the quantized ray is the drag
        // direction itself and the projection is the cursor.
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 15.0 });
        assert_eq!(c.apply([0.0, 0.0]), [0.0, 0.0]);
        let out = c.apply([10.0, 10.0]);
        assert!(
            close(out, [10.0, 10.0]),
            "a 45° drag with a 15° snap must hold 45°, got {out:?}"
        );
    }

    #[test]
    fn off_multiple_snaps_to_the_nearest_allowed_ray() {
        // A 40° drag with a 15° snap: 40 / 15 = 2.67 → 3 → the 45° ray.
        // Expected (f64 math, independent of the f32 filter): project the
        // 40° cursor onto the 45° ray.
        let drag = 40.0f64.to_radians();
        let ray = 45.0f64.to_radians();
        let along = 10.0 * (drag - ray).cos();
        let expected = [(along * ray.cos()) as f32, (along * ray.sin()) as f32];
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 15.0 });
        c.apply([0.0, 0.0]);
        let target = [(10.0 * drag.cos()) as f32, (10.0 * drag.sin()) as f32];
        let out = c.apply(target);
        assert!(
            close(out, expected),
            "a 40° drag with a 15° snap must land on 45°, got {out:?}, want {expected:?}"
        );
        // And it must actually differ from the raw cursor (the snap moved it).
        assert!(
            !close(out, target),
            "the snap must move an off-multiple drag"
        );
    }

    #[test]
    fn anchor_resets_on_reset() {
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 15.0 });
        c.apply([1.0, 1.0]);
        assert_eq!(c.anchor(), Some([1.0, 1.0]));
        c.reset();
        assert_eq!(c.anchor(), None, "stroke end must drop the anchor");
        assert_eq!(c.apply([5.0, 6.0]), [5.0, 6.0], "next stroke re-anchors");
        assert_eq!(c.anchor(), Some([5.0, 6.0]));
    }

    #[test]
    fn zero_length_vector_returns_the_anchor() {
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 15.0 });
        c.apply([4.0, 2.0]);
        assert_eq!(c.apply([4.0, 2.0]), [4.0, 2.0]);
    }

    #[test]
    fn degenerate_snap_step_disables_snapping() {
        assert_eq!(sanitize_snap_deg(0.0), 0.0);
        assert_eq!(sanitize_snap_deg(-5.0), 0.0);
        assert_eq!(sanitize_snap_deg(f32::NAN), 0.0);
        assert_eq!(sanitize_snap_deg(f32::INFINITY), 0.0);
        assert_eq!(sanitize_snap_deg(30.0), 30.0);
        // Through the filter: a degenerate step passes through like Line.
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 0.0 });
        c.apply([0.0, 0.0]);
        assert_eq!(c.apply([3.0, 4.0]), [3.0, 4.0]);
    }

    #[test]
    fn non_finite_target_holds_the_anchor() {
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 15.0 });
        c.apply([3.0, 7.0]);
        assert_eq!(c.apply([f32::NAN, 0.0]), [3.0, 7.0]);
        assert_eq!(
            c.anchor(),
            Some([3.0, 7.0]),
            "NaN must not poison the anchor"
        );
    }

    #[test]
    fn dragging_back_past_the_anchor_snaps_to_the_facing_ray() {
        // Full-circle quantization: straight back (−x) with a 90° snap
        // holds −x (180° is a 90° multiple), it does not collapse to +x.
        let mut c = LineConstraint::new(LineConstraintMode::AngleSnap { snap_deg: 90.0 });
        c.apply([0.0, 0.0]);
        let out = c.apply([-8.0, 0.5]);
        assert!(
            close(out, [-8.0, 0.0]),
            "must project onto the 180° ray, got {out:?}"
        );
    }
}
