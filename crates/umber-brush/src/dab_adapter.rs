//! Translation from brush-space stamps to paint-space dabs.
//!
//! The bridge between `umber_brush::spacing::DabPlan` stamps (pointer
//! geometry with pressure already interpolated into
//! `alpha_multiplier`) and `umber_gpu::paint::Dab` (premultiplied paint
//! stamps for the compute splat pass). Pure geometry+color math — no
//! GPU objects, fully testable on any platform.
//!
//! Coordinate contract: stamps carry positions in the same space the
//! caller planned them in. For Wave-2 direct painting that space is the
//! paint-target texel grid (the app maps UV → texel before spacing);
//! the adapter neither knows nor scales by texture size.

use crate::{DabPlan, FalloffCurve};
use serde::{Deserialize, Serialize};
use umber_gpu::paint::Dab;

/// How one stroke's overlapping stamps accumulate opacity (§2 line 16).
///
/// - [`WashMode::BuildUp`]: every stamp composites alpha-over on the
///   layer, so overlapping stamps accumulate (`1-(1-a)^n`). This is the
///   historical behavior — and what the GPU pass does by construction,
///   since overlapping dabs are split across `splat_dabs` dispatches
///   that see prior writes.
/// - [`WashMode::Wash`]: Krita wash mode via MyPaint's `opaque_linearize`:
///   the stroke's dabs are renormalized so their overlap accumulates to
///   the stroke opacity instead of past it. Per-stamp flow still varies
///   by pressure (each dab's linearized alpha is multiplied by its
///   shaped pressure multiplier).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WashMode {
    /// Overlapping stamps accumulate (status quo).
    #[default]
    BuildUp,
    /// Overlapping stamps normalize to the stroke opacity.
    Wash,
}

/// How a stamped dab is rotated (C2-11). Read only when the session has a
/// bitmap stamp ([`BrushParams::stamped`]); a radial dab is
/// rotation-symmetric.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum StampAngle {
    /// Every dab at `degrees`.
    Fixed {
        /// The fixed rotation, degrees counter-clockwise.
        degrees: f32,
    },
    /// Each dab turned to the stroke's direction of travel, plus
    /// `offset_degrees` (a flat brush dragged sideways).
    FollowStroke {
        /// Added to the travel direction, degrees.
        offset_degrees: f32,
    },
    /// Each dab at a deterministic pseudo-random angle (hashed from its
    /// position, so a replayed stroke paints identically).
    Random,
    /// Unrotated: the default, and the meaning of every pre-stamp preset.
    #[default]
    None,
}

impl StampAngle {
    /// The dab's angle in radians, given its position and the direction
    /// the stroke travelled to reach it (`None` for a stroke's first dab).
    pub fn radians(self, pos: [f32; 2], travel: Option<[f32; 2]>) -> f32 {
        match self {
            StampAngle::None => 0.0,
            StampAngle::Fixed { degrees } => finite_or_zero(degrees).to_radians(),
            StampAngle::FollowStroke { offset_degrees } => {
                let base = travel
                    .filter(|d| d[0] != 0.0 || d[1] != 0.0)
                    .map_or(0.0, |d| d[1].atan2(d[0]));
                base + finite_or_zero(offset_degrees).to_radians()
            }
            StampAngle::Random => {
                // FNV-1a over the position's bits: deterministic, so the
                // same stroke always rotates the same way.
                let mut h: u32 = 0x811c_9dc5;
                for b in pos[0]
                    .to_bits()
                    .to_le_bytes()
                    .into_iter()
                    .chain(pos[1].to_bits().to_le_bytes())
                {
                    h ^= u32::from(b);
                    h = h.wrapping_mul(0x0100_0193);
                }
                (h as f32 / u32::MAX as f32) * std::f32::consts::TAU
            }
        }
    }
}

/// Per-dab randomness (C2-11's jitter quartet): each dab's size, angle,
/// position and flow vary by up to these amounts. All zero (the
/// default) is no jitter, so every pre-jitter preset paints exactly as
/// before. Deterministic: the variation hashes the dab's planned
/// position, so a replayed stroke jitters identically (the same rule as
/// [`StampAngle::Random`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Jitter {
    /// Radius varies by up to ± this fraction (0..=1).
    #[serde(default)]
    pub size: f32,
    /// Stamp angle varies by up to ± this many degrees (0..=180).
    #[serde(default)]
    pub angle_degrees: f32,
    /// The dab centre moves up to this many RADII in a random direction
    /// (0..=2).
    #[serde(default)]
    pub position: f32,
    /// Flow (the dab alpha) is reduced by up to this fraction (0..=1):
    /// jitter only ever thins a dab, never pushes it past its alpha.
    #[serde(default)]
    pub flow: f32,
}

impl Jitter {
    /// Whether every amount is zero (the fast path: no hashing).
    pub fn is_none(&self) -> bool {
        self.size == 0.0 && self.angle_degrees == 0.0 && self.position == 0.0 && self.flow == 0.0
    }

    /// The clamped amounts (out-of-range and non-finite input is pulled
    /// into each field's documented range).
    fn clamped(&self) -> Self {
        let c = |v: f32, hi: f32| finite_or_zero(v).clamp(0.0, hi);
        Self {
            size: c(self.size, 1.0),
            angle_degrees: c(self.angle_degrees, 180.0),
            position: c(self.position, 2.0),
            flow: c(self.flow, 1.0),
        }
    }
}

/// Four independent uniform samples in `[-1, 1]` for the dab at `pos`
/// (FNV-1a over the position bits, salted per sample, then avalanched).
fn jitter_samples(pos: [f32; 2]) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (salt, slot) in out.iter_mut().enumerate() {
        let mut h: u32 = 0x811c_9dc5 ^ (salt as u32).wrapping_mul(0x9e37_79b9);
        for b in pos[0]
            .to_bits()
            .to_le_bytes()
            .into_iter()
            .chain(pos[1].to_bits().to_le_bytes())
        {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        *slot = (h as f32 / u32::MAX as f32) * 2.0 - 1.0;
    }
    out
}

fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Static brush parameters for one stroke segment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrushParams {
    /// Premultiplied RGBA stroke color, 0..=1 per channel.
    pub color: [f32; 4],
    /// Base per-dab opacity, 0..=1 (multiplied by each stamp's
    /// `alpha_multiplier`, which carries the pressure curve). In
    /// [`WashMode::Wash`] this is reinterpreted as the STROKE opacity
    /// ceiling: per-dab alphas are solved from it via
    /// [`opaque_linearize`].
    pub alpha: f32,
    /// Stamp hardness 0..=1 (1 = hard edge, 0 = softest falloff).
    pub hardness: f32,
    /// Nonlinear response applied to the pressure multiplier
    /// (1.0 = linear; >1 favors light pressure falloff).
    pub pressure_gamma: f32,
    /// How the stroke's overlapping stamps accumulate. Defaults to
    /// [`WashMode::BuildUp`] (the `#[serde(default)]` keeps pre-mode
    /// `.umberbrush` files loading with historical behavior).
    #[serde(default)]
    pub wash_mode: WashMode,
    /// Radial profile of each dab beyond the `hardness` core (see
    /// [`crate::falloff`]). Defaults to [`FalloffCurve::Smooth`], the
    /// historical curve; the `#[serde(default)]` keeps pre-curve
    /// `.umberbrush` files loading unchanged.
    #[serde(default)]
    pub falloff: FalloffCurve,
    /// Whether dabs use the session's bitmap stamp (C2-11) instead of the
    /// radial falloff. The stamp's pixels live in the paint session
    /// (`PaintState::set_stamp`), so the params stay `Copy`.
    /// `#[serde(default)]`: pre-stamp presets load radial.
    #[serde(default)]
    pub stamped: bool,
    /// How stamped dabs rotate. `#[serde(default)]` = unrotated.
    #[serde(default)]
    pub stamp_angle: StampAngle,
    /// Per-dab jitter (size / angle / position / flow). `#[serde(default)]`
    /// = none.
    #[serde(default)]
    pub jitter: Jitter,
}

impl Default for BrushParams {
    fn default() -> Self {
        Self {
            color: [0.85, 0.2, 0.1, 1.0],
            alpha: 0.8,
            hardness: 0.5,
            pressure_gamma: 1.0,
            wash_mode: WashMode::BuildUp,
            falloff: FalloffCurve::Smooth,
            stamped: false,
            stamp_angle: StampAngle::None,
            jitter: Jitter::default(),
        }
    }
}

/// One dab from one stamp: the radial or the stamped shape, per `params`.
/// Stamped dabs carry their angle; `prev` is the previous stamp's position
/// in the same segment (the travel direction for
/// [`StampAngle::FollowStroke`]).
fn dab_for(
    stamp: &DabPlan,
    prev: Option<[f32; 2]>,
    radius_px: f32,
    params: &BrushParams,
    alpha: f32,
) -> Dab {
    let travel = prev.map(|p| [stamp.pos[0] - p[0], stamp.pos[1] - p[1]]);
    let mut angle = if params.stamped {
        params.stamp_angle.radians(stamp.pos, travel)
    } else {
        0.0
    };
    let (mut pos, mut radius, mut alpha) = (stamp.pos, radius_px, alpha);
    if !params.jitter.is_none() {
        let j = params.jitter.clamped();
        // Hash the PLANNED position, before any offset: one dab's jitter
        // never depends on another's.
        let [s_size, s_angle, s_dir, s_flow] = jitter_samples(stamp.pos);
        radius = (radius * (1.0 + j.size * s_size)).max(0.0);
        angle += (j.angle_degrees * s_angle).to_radians();
        let theta = (s_dir * 0.5 + 0.5) * std::f32::consts::TAU;
        // The distance reuses the size sample's magnitude (a fifth salt
        // would buy no visible difference).
        let dist = j.position * radius_px * s_size.abs();
        pos = [pos[0] + dist * theta.cos(), pos[1] + dist * theta.sin()];
        alpha *= 1.0 - j.flow * (s_flow * 0.5 + 0.5);
    }
    let dab = Dab::new(
        pos,
        radius,
        params.color,
        alpha.clamp(0.0, 1.0),
        params.hardness,
    );
    if params.stamped {
        dab.with_falloff(umber_gpu::paint::FALLOFF_STAMP)
            .with_angle(angle)
    } else {
        dab.with_falloff(params.falloff.as_u32())
    }
}

/// MyPaint's `opaque_linearize`: the per-dab base alpha such that
/// `overlap` fully-overlapping dabs accumulate to exactly
/// `stroke_opacity` under alpha-over compositing.
///
/// Solves `1 - (1-a)^n = O` for `a`: `a = 1 - (1-O)^(1/n)`. `overlap`
/// is the mean covering count — the spacing density (`dabs_per_radius`:
/// dabs emitted per radius of travel, so each texel is covered by about
/// that many dabs as the brush passes). Center texels see up to ~2x
/// (the disc's diameter spans two radii) and edge texels fewer; the
/// density is the mean-field estimate, the same one MyPaint uses.
///
/// Degenerate densities (`<= 0`, non-finite) and sparse densities
/// (`< 1`, dabs that never overlap) both collapse to `n = 1`, i.e. each
/// dab paints at the stroke opacity — correct in both cases, since with
/// no overlap there is nothing to normalize.
pub fn opaque_linearize(stroke_opacity: f32, overlap: f32) -> f32 {
    let o = stroke_opacity.clamp(0.0, 1.0);
    let n = if overlap.is_finite() {
        overlap.max(1.0)
    } else {
        1.0
    };
    if o <= 0.0 || o >= 1.0 {
        return o;
    }
    1.0 - (1.0 - o).powf(1.0 / n)
}

/// The multiplicative form of [`opaque_linearize`] for callers that
/// already hold build-up dabs: `wash_scale * (O * pressure)` equals the
/// linearized `a * pressure` exactly (both factors `<= 1`, so no clamp
/// ordering hazard). At `O == 0` the ratio tends to `1/n` (the linear
/// limit of `1-(1-O)^(1/n)`); every dab alpha is `0` there anyway.
pub fn wash_scale(stroke_opacity: f32, overlap: f32) -> f32 {
    let o = stroke_opacity.clamp(0.0, 1.0);
    if o <= 0.0 {
        let n = if overlap.is_finite() {
            overlap.max(1.0)
        } else {
            1.0
        };
        return 1.0 / n;
    }
    opaque_linearize(o, overlap) / o
}

/// Converts brush-space stamps into paint-space dabs.
#[derive(Debug)]
pub struct DabAdapter;

impl DabAdapter {
    /// Maps one segment's `DabPlan` stamps to paint `Dab`s.
    ///
    /// `radius_px` is the brush radius the stamps were spaced with
    /// (`SpacingAccumulator::plan_dabs` takes it per segment; the
    /// adapter carries it onto each dab unchanged). Each stamp's
    /// `alpha_multiplier` (pressure-interpolated) is shaped by
    /// `pressure_gamma`, multiplied by the base `alpha`, and clamped.
    ///
    /// This is the build-up path and ignores `params.wash_mode`: callers
    /// that branch on the mode keep calling this for
    /// [`WashMode::BuildUp`] (byte-identical to the pre-mode behavior)
    /// and call [`Self::stamps_to_dabs_wash`] for [`WashMode::Wash`].
    pub fn stamps_to_dabs(stamps: &[DabPlan], radius_px: f32, params: &BrushParams) -> Vec<Dab> {
        let mut out = Vec::with_capacity(stamps.len());
        let mut prev = None;
        for stamp in stamps {
            let shaped = stamp.alpha_multiplier.powf(params.pressure_gamma);
            let alpha = (params.alpha * shaped).clamp(0.0, 1.0);
            out.push(dab_for(stamp, prev, radius_px, params, alpha));
            prev = Some(stamp.pos);
        }
        out
    }

    /// Wash-mode mapping: like [`Self::stamps_to_dabs`], but the base
    /// alpha is [`opaque_linearize`]d from `params.alpha` (now the stroke
    /// opacity ceiling) and `dabs_per_radius` (the spacing density that
    /// produced `stamps` — the mean overlap count) before the per-stamp
    /// pressure shaping multiplies in. Overlapping stamps then accumulate
    /// to the stroke opacity instead of past it; per-stamp flow still
    /// varies by pressure.
    ///
    /// Equivalent to mapping with [`Self::stamps_to_dabs`] and scaling
    /// every dab alpha by [`wash_scale`] (proven equal by
    /// `wash_scale_matches_opaque_linearize`), which is what the paint
    /// session's post-retouch-hook renormalization does so the retouch
    /// tools keep build-up dabs.
    pub fn stamps_to_dabs_wash(
        stamps: &[DabPlan],
        radius_px: f32,
        params: &BrushParams,
        dabs_per_radius: f32,
    ) -> Vec<Dab> {
        let base = opaque_linearize(params.alpha, dabs_per_radius);
        let mut out = Vec::with_capacity(stamps.len());
        let mut prev = None;
        for stamp in stamps {
            let shaped = stamp.alpha_multiplier.powf(params.pressure_gamma);
            let alpha = (base * shaped).clamp(0.0, 1.0);
            out.push(dab_for(stamp, prev, radius_px, params, alpha));
            prev = Some(stamp.pos);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plans(points: &[[f32; 2]]) -> Vec<DabPlan> {
        points
            .iter()
            .map(|p| DabPlan {
                pos: *p,
                alpha_multiplier: 1.0,
            })
            .collect()
    }

    #[test]
    fn stamped_params_emit_stamp_dabs_with_the_angle_mode() {
        use umber_gpu::paint::FALLOFF_STAMP;
        let stamps = plans(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        // Radial by default: no stamp id, no angle.
        let radial = DabAdapter::stamps_to_dabs(&stamps, 4.0, &BrushParams::default());
        assert!(radial
            .iter()
            .all(|d| d.falloff() != FALLOFF_STAMP && d.angle() == 0.0));
        // Fixed 90 degrees.
        let fixed = BrushParams {
            stamped: true,
            stamp_angle: StampAngle::Fixed { degrees: 90.0 },
            ..BrushParams::default()
        };
        for d in DabAdapter::stamps_to_dabs(&stamps, 4.0, &fixed) {
            assert_eq!(d.falloff(), FALLOFF_STAMP);
            assert!((d.angle() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        }
        // Follow the stroke: dab 2 travelled +x (0 rad), dab 3 travelled +y.
        let follow = BrushParams {
            stamped: true,
            stamp_angle: StampAngle::FollowStroke {
                offset_degrees: 0.0,
            },
            ..BrushParams::default()
        };
        let dabs = DabAdapter::stamps_to_dabs_wash(&stamps, 4.0, &follow, 2.0);
        assert_eq!(dabs[0].angle(), 0.0, "the first dab has no travel yet");
        assert!(dabs[1].angle().abs() < 1e-6);
        assert!((dabs[2].angle() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn no_jitter_is_unchanged_and_jitter_stays_in_range() {
        let stamps = plans(&[[5.0, 5.0], [9.0, 5.0], [13.0, 5.0], [17.0, 5.0]]);
        let base = BrushParams::default();
        let plain = DabAdapter::stamps_to_dabs(&stamps, 8.0, &base);
        for (d, s) in plain.iter().zip(&stamps) {
            assert_eq!(d.pos, s.pos, "zero jitter: the old dab");
            assert_eq!(d.radius, 8.0);
        }
        let jittered = BrushParams {
            jitter: Jitter {
                size: 0.5,
                angle_degrees: 30.0,
                position: 1.0,
                flow: 0.5,
            },
            stamped: true,
            ..base
        };
        let a = DabAdapter::stamps_to_dabs(&stamps, 8.0, &jittered);
        let b = DabAdapter::stamps_to_dabs(&stamps, 8.0, &jittered);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(
                (x.pos, x.radius, x.alpha, x.angle()),
                (y.pos, y.radius, y.alpha, y.angle()),
                "deterministic"
            );
        }
        let mut varied = 0;
        for ((d, s), p) in a.iter().zip(&stamps).zip(&plain) {
            assert!(
                (4.0..=12.0).contains(&d.radius),
                "size within ±50%: {}",
                d.radius
            );
            let off = ((d.pos[0] - s.pos[0]).powi(2) + (d.pos[1] - s.pos[1]).powi(2)).sqrt();
            assert!(off <= 8.0 + 1e-4, "position within one radius: {off}");
            assert!(d.alpha <= p.alpha + 1e-6, "flow jitter only thins");
            assert!(d.alpha >= p.alpha * 0.5 - 1e-6, "by at most half");
            assert!(
                d.angle().abs() <= 30f32.to_radians() + 1e-5,
                "angle within ±30°"
            );
            if off > 0.01 || (d.radius - 8.0).abs() > 0.01 {
                varied += 1;
            }
        }
        assert!(varied >= 3, "jitter actually varies the dabs ({varied}/4)");
        let wild = Jitter {
            size: 9.0,
            angle_degrees: f32::NAN,
            position: -3.0,
            flow: 2.0,
        }
        .clamped();
        assert_eq!(
            (wild.size, wild.angle_degrees, wild.position, wild.flow),
            (1.0, 0.0, 0.0, 1.0)
        );
    }

    #[test]
    fn random_angles_are_deterministic_and_spread() {
        let a = StampAngle::Random.radians([3.0, 7.0], None);
        assert_eq!(
            a,
            StampAngle::Random.radians([3.0, 7.0], None),
            "same dab, same angle"
        );
        assert!((0.0..=std::f32::consts::TAU).contains(&a));
        let spread: std::collections::BTreeSet<u32> = (0..64)
            .map(|i| (StampAngle::Random.radians([i as f32, 0.0], None) * 4.0) as u32)
            .collect();
        assert!(spread.len() >= 6, "angles spread over the turn: {spread:?}");
        assert_eq!(
            StampAngle::Fixed { degrees: f32::NAN }.radians([0.0; 2], None),
            0.0
        );
    }

    #[test]
    fn pre_stamp_params_json_loads_radial_and_unrotated() {
        let json = r#"{"color":[1,0,0,1],"alpha":0.5,"hardness":0.5,"pressure_gamma":1.0}"#;
        let params: BrushParams = serde_json::from_str(json).expect("old shape loads");
        assert!(!params.stamped);
        assert_eq!(params.stamp_angle, StampAngle::None);
        let turned = BrushParams {
            stamped: true,
            stamp_angle: StampAngle::FollowStroke {
                offset_degrees: 15.0,
            },
            ..BrushParams::default()
        };
        let back: BrushParams =
            serde_json::from_str(&serde_json::to_string(&turned).unwrap()).unwrap();
        assert_eq!(back, turned, "round-trips");
    }

    fn stamp(pos: [f32; 2], alpha_multiplier: f32) -> DabPlan {
        DabPlan {
            pos,
            alpha_multiplier,
        }
    }

    #[test]
    fn empty_stamps_yield_no_dabs() {
        assert!(DabAdapter::stamps_to_dabs(&[], 8.0, &BrushParams::default()).is_empty());
    }

    #[test]
    fn multiplier_carries_pressure_into_alpha() {
        let dabs = DabAdapter::stamps_to_dabs(
            &[stamp([10.0, 12.0], 0.5)],
            8.0,
            &BrushParams {
                alpha: 0.8,
                ..BrushParams::default()
            },
        );
        assert_eq!(dabs.len(), 1);
        // alpha = base 0.8 * multiplier 0.5, linear gamma
        assert!((dabs[0].alpha - 0.4).abs() < 1e-6);
        assert_eq!(dabs[0].pos, [10.0, 12.0]);
        assert!((dabs[0].radius - 8.0).abs() < 1e-6);
    }

    #[test]
    fn gamma_shapes_light_pressure() {
        let params = BrushParams {
            pressure_gamma: 2.0,
            ..BrushParams::default()
        };
        // multiplier 0.5, gamma 2 -> 0.25; base 0.8 -> 0.2
        let dabs = DabAdapter::stamps_to_dabs(&[stamp([0.0, 0.0], 0.5)], 8.0, &params);
        assert!((dabs[0].alpha - 0.2).abs() < 1e-6);
    }

    #[test]
    fn alpha_clamps_at_one() {
        let dabs = DabAdapter::stamps_to_dabs(
            &[stamp([0.0, 0.0], 1.0)],
            8.0,
            &BrushParams {
                alpha: 1.0,
                ..BrushParams::default()
            },
        );
        assert!((dabs[0].alpha - 1.0).abs() < 1e-6);
    }

    #[test]
    fn color_and_hardness_pass_through() {
        let params = BrushParams {
            color: [0.1, 0.2, 0.3, 1.0],
            hardness: 0.9,
            ..BrushParams::default()
        };
        let dabs = DabAdapter::stamps_to_dabs(&[stamp([1.0, 1.0], 1.0)], 4.0, &params);
        assert_eq!(dabs[0].color, [0.1, 0.2, 0.3, 1.0]);
        assert!((dabs[0].hardness - 0.9).abs() < 1e-6);
    }

    #[test]
    fn falloff_passes_through_both_paths() {
        let params = BrushParams {
            falloff: FalloffCurve::Sphere,
            ..BrushParams::default()
        };
        let stamps = [stamp([1.0, 1.0], 1.0)];
        let build_up = DabAdapter::stamps_to_dabs(&stamps, 4.0, &params);
        let wash = DabAdapter::stamps_to_dabs_wash(&stamps, 4.0, &params, 4.0);
        assert_eq!(build_up[0].falloff(), umber_gpu::paint::FALLOFF_SPHERE);
        assert_eq!(wash[0].falloff(), umber_gpu::paint::FALLOFF_SPHERE);
        // Default params stay on the historical curve (id 0).
        let plain = DabAdapter::stamps_to_dabs(&stamps, 4.0, &BrushParams::default());
        assert_eq!(plain[0].falloff(), umber_gpu::paint::FALLOFF_SMOOTH);
    }

    #[test]
    fn brush_params_without_falloff_load_as_smooth() {
        let json = r#"{"color":[1.0,0.0,0.0,1.0],"alpha":0.5,"hardness":0.3,"pressure_gamma":1.0}"#;
        let params: BrushParams = serde_json::from_str(json).expect("pre-curve params load");
        assert_eq!(params.falloff, FalloffCurve::Smooth);
        assert_eq!(params.wash_mode, WashMode::BuildUp);
        let sharp = BrushParams {
            falloff: FalloffCurve::Sharp,
            ..params
        };
        let text = serde_json::to_string(&sharp).expect("serialize");
        assert!(text.contains(r#""falloff":"sharp""#), "{text}");
        let back: BrushParams = serde_json::from_str(&text).expect("round trip");
        assert_eq!(back, sharp);
    }

    #[test]
    fn integration_spacing_to_dabs() {
        // End-to-end: a spacing-planned segment adapts into dabs with
        // monotonically interpolated pressure between the endpoints.
        let mut acc = crate::SpacingAccumulator::new(5.0).expect("valid density");
        let stamps = acc.plan_dabs([0.0, 0.0], [40.0, 0.0], 8.0, [1.0, 0.0]);
        assert!(!stamps.is_empty());
        let dabs = DabAdapter::stamps_to_dabs(&stamps, 8.0, &BrushParams::default());
        assert_eq!(dabs.len(), stamps.len());
        // First dab carries near-start pressure, last near-end: alphas descend.
        assert!(dabs[0].alpha > dabs[dabs.len() - 1].alpha);
        // Positions stay inside the segment.
        for dab in &dabs {
            assert!(dab.pos[0] >= 0.0 && dab.pos[0] <= 40.0);
            assert_eq!(dab.pos[1], 0.0);
        }
    }

    /// Alpha-over fold of `n` identical dabs of per-dab alpha `a` onto a
    /// transparent texel: `1 - (1-a)^n`. The CPU mirror of what the GPU
    /// pass computes per texel for fully-overlapping stamps.
    fn accumulate(a: f32, n: usize) -> f32 {
        1.0 - (1.0 - a).powi(n as i32)
    }

    #[test]
    fn wash_mode_defaults_to_build_up() {
        assert_eq!(BrushParams::default().wash_mode, WashMode::BuildUp);
    }

    #[test]
    fn build_up_overlapping_dabs_accumulate() {
        // Two fully-overlapping stamps at stroke opacity 0.5: build-up
        // dabs each carry 0.5 and fold to 1-(1-0.5)^2 = 0.75, NOT 0.5.
        // Pins the pre-mode behavior for the BuildUp arm.
        let params = BrushParams {
            alpha: 0.5,
            wash_mode: WashMode::BuildUp,
            ..BrushParams::default()
        };
        let stamps = [stamp([0.0, 0.0], 1.0), stamp([0.0, 0.0], 1.0)];
        let dabs = DabAdapter::stamps_to_dabs(&stamps, 8.0, &params);
        assert_eq!(dabs.len(), 2);
        assert!((dabs[0].alpha - 0.5).abs() < 1e-6);
        assert!((dabs[1].alpha - 0.5).abs() < 1e-6);
        let folded = accumulate(dabs[0].alpha, 2);
        assert!(
            (folded - 0.75).abs() < 1e-6,
            "build-up must accumulate, got {folded}"
        );
    }

    #[test]
    fn opaque_linearize_hits_stroke_opacity() {
        // The defining identity: n overlapping dabs of the linearized
        // alpha fold to exactly the stroke opacity.
        for opacity in [0.25, 0.5, 0.8] {
            for overlap in [2.0, 4.0, 12.0] {
                let a = opaque_linearize(opacity, overlap);
                assert!(
                    (0.0..=opacity).contains(&a),
                    "linearized {a} must not exceed the stroke opacity {opacity}"
                );
                let folded = accumulate(a, overlap as usize);
                assert!(
                    (folded - opacity).abs() < 1e-5,
                    "n={overlap} dabs must fold to {opacity}, got {folded}"
                );
            }
        }
        // Endpoints are fixed points.
        assert_eq!(opaque_linearize(0.0, 4.0), 0.0);
        assert_eq!(opaque_linearize(1.0, 4.0), 1.0);
        // Degenerate densities collapse to n = 1 (each dab at full
        // stroke opacity): nothing to normalize without overlap
        // (epsilon: `powf(x, 1.0)` need not be bitwise `x`).
        for bad_overlap in [0.0, -3.0, f32::NAN, 0.5] {
            let got = opaque_linearize(0.8, bad_overlap);
            assert!(
                (got - 0.8).abs() < 1e-6,
                "overlap {bad_overlap} must yield the stroke opacity, got {got}"
            );
        }
    }

    #[test]
    fn wash_dabs_sum_to_stroke_opacity_not_build_up() {
        // Four fully-overlapping stamps, uniform full pressure, stroke
        // opacity 0.5, overlap 4: wash dabs fold to 0.5 while build-up
        // dabs fold to 1-(1-0.5)^4 = 0.9375.
        let params = BrushParams {
            alpha: 0.5,
            wash_mode: WashMode::Wash,
            ..BrushParams::default()
        };
        let stamps = [
            stamp([0.0, 0.0], 1.0),
            stamp([0.0, 0.0], 1.0),
            stamp([0.0, 0.0], 1.0),
            stamp([0.0, 0.0], 1.0),
        ];
        let wash = DabAdapter::stamps_to_dabs_wash(&stamps, 8.0, &params, 4.0);
        assert_eq!(wash.len(), 4);
        let folded_wash = accumulate(wash[0].alpha, 4);
        assert!(
            (folded_wash - 0.5).abs() < 1e-5,
            "wash must normalize to the stroke opacity, got {folded_wash}"
        );
        let build_up = DabAdapter::stamps_to_dabs(&stamps, 8.0, &params);
        let folded_build_up = accumulate(build_up[0].alpha, 4);
        assert!(
            (folded_build_up - 0.9375).abs() < 1e-6,
            "build-up must accumulate past the stroke opacity, got {folded_build_up}"
        );
    }

    #[test]
    fn wash_scale_matches_opaque_linearize() {
        // The session's post-hoc scale path equals the direct wash
        // mapping: scale * (O * p) == linearized * p.
        for opacity in [0.1, 0.5, 0.9] {
            let scale = wash_scale(opacity, 4.0);
            let direct = opaque_linearize(opacity, 4.0);
            assert!(
                ((scale * opacity) - direct).abs() < 1e-6,
                "scale path diverged at O={opacity}: {} vs {direct}",
                scale * opacity
            );
        }
        // Zero opacity: the 1/n linear limit (dabs are 0 there anyway).
        assert!((wash_scale(0.0, 4.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn wash_keeps_pressure_variation_per_stamp() {
        // Per-stamp flow still varies by pressure in wash mode: the dab
        // alpha ratio equals the shaped-pressure ratio.
        let params = BrushParams {
            alpha: 0.6,
            pressure_gamma: 2.0,
            wash_mode: WashMode::Wash,
            ..BrushParams::default()
        };
        let stamps = [stamp([0.0, 0.0], 0.25), stamp([1.0, 0.0], 1.0)];
        let dabs = DabAdapter::stamps_to_dabs_wash(&stamps, 8.0, &params, 4.0);
        assert!(
            dabs[0].alpha < dabs[1].alpha,
            "light pressure must stay fainter"
        );
        // Shaped multipliers: 0.25^2 = 0.0625 vs 1.0^2 = 1.0.
        let want_ratio = 0.25f32.powf(2.0);
        let got_ratio = dabs[0].alpha / dabs[1].alpha;
        assert!(
            (got_ratio - want_ratio).abs() < 1e-6,
            "wash must preserve the pressure ratio, got {got_ratio}"
        );
        // And the hard dab still sits under the stroke opacity ceiling.
        assert!(dabs[1].alpha < params.alpha);
    }
}
