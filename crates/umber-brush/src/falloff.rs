//! Brush falloff curves (ROADMAP wave 8b `wave-brush-falloff-curves`).
//!
//! Design source: Blender's brush falloff presets
//! (`BKE_brush_curve_strength`). A dab's per-texel coverage is
//! `alpha * curve(t, hardness)`, where `t` is the normalized distance from
//! the dab centre (0 = centre, 1 = edge).
//!
//! `hardness` keeps its pre-curve meaning: it is the flat core. Every
//! curve is 1.0 for `t <= hardness` and is evaluated over the remaining
//! ring through the remapped edge parameter
//! `e = clamp((t - hardness) / max(1 - hardness, 1e-4), 0, 1)` (0 at the
//! core edge, 1 at the dab edge). The Blender formulas take `u = 1 - e`
//! (1 at the core edge, 0 at the rim):
//!
//! | curve           | strength                  |
//! |-----------------|---------------------------|
//! | `Smooth`        | `1 - e²(3 - 2e)` (today's smoothstep, unchanged) |
//! | `Smoother`      | `u³(u(6u - 15) + 10)`     |
//! | `Sphere`        | `sqrt(2u - u²)`           |
//! | `Root`          | `sqrt(u)`                 |
//! | `Sharp`         | `u²`                      |
//! | `Sharper`       | `u⁴` (Blender `POW4`)     |
//! | `Linear`        | `u`                       |
//! | `InverseSquare` | `u(2 - u)`                |
//! | `Constant`      | `1`                       |
//!
//! Judgment calls:
//! - `Smooth` is the default and is written with the exact pre-curve
//!   expression (not the algebraically equal `3u² - 2u³`), so existing
//!   brushes stay byte-identical on CPU and GPU.
//! - `Sharper` is included (it is in Blender's preset list) although the
//!   brief's enum list omitted it; it costs one switch arm.
//! - `Constant` ignores hardness: it is a flat disc out to the radius.
//! - The GPU copy lives in `umber_gpu::shaders::PAINT_COMPUTE_SHADER`'s
//!   `radial_falloff`, switched on the id carried in
//!   [`umber_gpu::paint::Dab`]'s former padding slot; ids are the
//!   `umber_gpu::paint::FALLOFF_*` constants. An unknown id falls back to
//!   `Smooth` on the GPU.
//! - `Custom` (docs/specs/falloff-custom-design.md) is the user-drawn
//!   curve; see the next section.
//!
//! ## `Custom`: the user-drawn curve
//!
//! The curve is NOT in the enum (the enum stays `Copy` and id-only): it
//! lives in [`crate::BrushParams::falloff_custom`], a 4-point
//! [`ControlCurve`]. The GPU never evaluates it: the brush side samples it
//! into a 256-tap LUT ([`custom_lut`]) that the paint pass reads at
//! binding 5 (`umber_gpu::paint::PaintCompositor::set_falloff_lut`).
//! Judgment calls:
//! - **The editor's x axis is `u = 1 - e`** (0 at the rim, 1 at the core
//!   edge): the axis every formula above is written in, and Blender's
//!   custom-curve orientation. Tap `i` holds `curve(1 - i / 255)`, so the
//!   default identity curve paints like `Linear` at the taps and a rising
//!   curve reads "strong in the middle". The shader still indexes by `e`
//!   (the design's index, unchanged); the flip lives only in
//!   [`custom_tap`].
//! - **The read is a truncated nearest tap**, not a lerp between taps:
//!   `tap = min(floor(e * 255), 255)`, the WGSL `u32(e * 255.0)`.
//!   [`FalloffCurve::eval_with`] does the same f32 ops, so CPU and GPU
//!   read the same tap (the design's "exact parity" rule).
//! - **The core is flat at the curve's value at `u = 1`**, not always
//!   1.0: `t <= hardness` gives `e = 0`, tap 0. With the identity curve
//!   that is 1.0, like every analytic curve.
//! - Taps are clamped to `0..=1` (a hand-written `.umberbrush` can carry
//!   `y = 2`: [`ControlCurve`] only demands finite points).
//! - [`FalloffCurve::eval`] (two arguments, no curve in reach) evaluates
//!   `Custom` with the default identity curve; callers holding params use
//!   [`FalloffCurve::eval_with`].
//! - Serde back-compat, verified against the derive: the lowercase enum
//!   REJECTS an unknown variant, so a binary older than `Custom` fails to
//!   load a preset whose falloff is `"custom"` (the preset errors, it does
//!   not fall back to Smooth). `BrushParams` does not deny unknown
//!   fields, so the new `falloff_custom` key alone is harmless to old
//!   binaries.

use serde::{Deserialize, Serialize};
use umber_gpu::paint as gpu;

use crate::preset::ControlCurve;

/// Taps in the [`FalloffCurve::Custom`] LUT (the GPU texture's width).
pub const CUSTOM_LUT_LEN: usize = gpu::FALLOFF_LUT_LEN;

/// The radial profile of a dab (see the module docs for the formulas).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FalloffCurve {
    /// Smoothstep from the hardness core to the rim (the historical curve).
    #[default]
    Smooth,
    /// Quintic smootherstep: flatter shoulders than `Smooth`.
    Smoother,
    /// Quarter-circle profile: full strength until near the rim.
    Sphere,
    /// Square root: bright body, sharp rim drop.
    Root,
    /// Quadratic: concentrated centre.
    Sharp,
    /// Quartic: very concentrated centre.
    Sharper,
    /// Linear ramp from core to rim.
    Linear,
    /// `u(2 - u)`: Blender's "Inverse Square" preset.
    InverseSquare,
    /// Flat disc: full strength out to the radius.
    Constant,
    /// The user-drawn curve ([`crate::BrushParams::falloff_custom`]),
    /// sampled into a LUT (see the module docs).
    Custom,
}

impl FalloffCurve {
    /// Every curve, in id order.
    pub const ALL: [FalloffCurve; 10] = [
        FalloffCurve::Smooth,
        FalloffCurve::Smoother,
        FalloffCurve::Sphere,
        FalloffCurve::Root,
        FalloffCurve::Sharp,
        FalloffCurve::Sharper,
        FalloffCurve::Linear,
        FalloffCurve::InverseSquare,
        FalloffCurve::Constant,
        FalloffCurve::Custom,
    ];

    /// The stable id the GPU `radial_falloff` switch reads. Never renumber:
    /// the ids are the `umber_gpu::paint::FALLOFF_*` constants.
    pub fn as_u32(self) -> u32 {
        match self {
            FalloffCurve::Smooth => gpu::FALLOFF_SMOOTH,
            FalloffCurve::Smoother => gpu::FALLOFF_SMOOTHER,
            FalloffCurve::Sphere => gpu::FALLOFF_SPHERE,
            FalloffCurve::Root => gpu::FALLOFF_ROOT,
            FalloffCurve::Sharp => gpu::FALLOFF_SHARP,
            FalloffCurve::Sharper => gpu::FALLOFF_SHARPER,
            FalloffCurve::Linear => gpu::FALLOFF_LINEAR,
            FalloffCurve::InverseSquare => gpu::FALLOFF_INVERSE_SQUARE,
            FalloffCurve::Constant => gpu::FALLOFF_CONSTANT,
            FalloffCurve::Custom => gpu::FALLOFF_CUSTOM,
        }
    }

    /// The curve for a GPU id, or `None` for an unknown id.
    pub fn from_u32(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_u32() == id)
    }

    /// CPU reference of the GPU `radial_falloff`: strength at normalized
    /// distance `t` (0 = centre, 1 = rim) with a flat core of `hardness`.
    /// Mirrors the WGSL operation for operation. `Custom` reads the
    /// default identity curve here; see [`Self::eval_with`].
    pub fn eval(self, t: f32, hardness: f32) -> f32 {
        self.eval_with(t, hardness, &ControlCurve::identity())
    }

    /// [`Self::eval`] with the `Custom` curve supplied (the analytic
    /// curves ignore `custom`). `Custom` returns the LUT tap the GPU
    /// reads: [`custom_tap`] at `min(floor(e * 255), 255)`.
    pub fn eval_with(self, t: f32, hardness: f32, custom: &ControlCurve) -> f32 {
        let denom = (1.0 - hardness).max(1e-4);
        let e = ((t - hardness) / denom).clamp(0.0, 1.0);
        let u = 1.0 - e;
        match self {
            FalloffCurve::Smooth => 1.0 - e * e * (3.0 - 2.0 * e),
            FalloffCurve::Smoother => u * u * u * (u * (u * 6.0 - 15.0) + 10.0),
            FalloffCurve::Sphere => (2.0 * u - u * u).max(0.0).sqrt(),
            FalloffCurve::Root => u.sqrt(),
            FalloffCurve::Sharp => u * u,
            FalloffCurve::Sharper => u * u * u * u,
            FalloffCurve::Linear => u,
            FalloffCurve::InverseSquare => u * (2.0 - u),
            FalloffCurve::Constant => 1.0,
            FalloffCurve::Custom => custom_tap(custom, custom_index(e)),
        }
    }
}

/// The LUT tap the shader reads at edge parameter `e` (in `0..=1`):
/// `min(u32(e * 255), 255)`. `as` truncates toward zero like WGSL's
/// `u32()` (and saturates a NaN to 0).
fn custom_index(e: f32) -> usize {
    ((e * 255.0) as usize).min(CUSTOM_LUT_LEN - 1)
}

/// Tap `i` of `curve`'s LUT: the curve at `u = 1 - i / 255` (the editor's
/// x axis, see the module docs), clamped to `0..=1`; non-finite is 0.
pub fn custom_tap(curve: &ControlCurve, i: usize) -> f32 {
    let x = 1.0 - i as f32 / (CUSTOM_LUT_LEN - 1) as f32;
    let y = curve.evaluate(x);
    if y.is_finite() {
        y.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The 256 taps a `Custom` brush uploads
/// (`umber_gpu::paint::PaintCompositor::set_falloff_lut`).
pub fn custom_lut(curve: &ControlCurve) -> [f32; CUSTOM_LUT_LEN] {
    std::array::from_fn(|i| custom_tap(curve, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pre-curve `radial_falloff`, verbatim (WGSL → Rust).
    fn legacy(t: f32, hardness: f32) -> f32 {
        let denom = (1.0 - hardness).max(1e-4);
        let edge_t = ((t - hardness) / denom).clamp(0.0, 1.0);
        1.0 - edge_t * edge_t * (3.0 - 2.0 * edge_t)
    }

    #[test]
    fn smooth_is_bit_identical_to_the_legacy_formula() {
        for ti in 0..=40 {
            for hi in 0..=20 {
                let t = ti as f32 / 40.0;
                let h = hi as f32 / 20.0;
                assert_eq!(
                    FalloffCurve::Smooth.eval(t, h).to_bits(),
                    legacy(t, h).to_bits(),
                    "t={t} h={h}"
                );
            }
        }
        assert_eq!(FalloffCurve::default(), FalloffCurve::Smooth);
    }

    #[test]
    fn every_curve_is_one_at_centre_and_core_edge() {
        for c in FalloffCurve::ALL {
            assert_eq!(c.eval(0.0, 0.0), 1.0, "{c:?} centre, h=0");
            assert_eq!(c.eval(0.0, 0.25), 1.0, "{c:?} centre, h=0.25");
            // t == hardness: e = 0, u = 1 → every formula is exactly 1.
            assert_eq!(c.eval(0.25, 0.25), 1.0, "{c:?} core edge");
        }
    }

    #[test]
    fn every_curve_but_constant_is_zero_at_the_rim() {
        for c in FalloffCurve::ALL {
            let want = if c == FalloffCurve::Constant {
                1.0
            } else {
                0.0
            };
            assert_eq!(c.eval(1.0, 0.0), want, "{c:?} rim, h=0");
            assert_eq!(c.eval(1.0, 0.25), want, "{c:?} rim, h=0.25");
        }
    }

    /// e = u = 0.5, reached two ways: (t 0.5, h 0) and the hardness remap
    /// (t 0.75, h 0.5: e = 0.25 / 0.5). Every value follows from the
    /// table in the module docs at u = 0.5; all but the sqrt arms are
    /// exact binary fractions, and sqrt is correctly rounded.
    #[test]
    fn midpoint_values_pin_each_formula() {
        let want = [
            (FalloffCurve::Smooth, 0.5),
            (FalloffCurve::Smoother, 0.5),
            (FalloffCurve::Sphere, 0.75f32.sqrt()),
            (FalloffCurve::Root, 0.5f32.sqrt()),
            (FalloffCurve::Sharp, 0.25),
            (FalloffCurve::Sharper, 0.0625),
            (FalloffCurve::Linear, 0.5),
            (FalloffCurve::InverseSquare, 0.75),
            (FalloffCurve::Constant, 1.0),
        ];
        for (c, v) in want {
            assert_eq!(c.eval(0.5, 0.0), v, "{c:?} t=0.5 h=0");
            assert_eq!(c.eval(0.75, 0.5), v, "{c:?} t=0.75 h=0.5");
        }
    }

    /// u = 0.75 (t 0.25, h 0) separates the curves that coincide at the
    /// midpoint (Smooth/Smoother/Linear all give 0.5 there).
    #[test]
    fn quarter_values_separate_midpoint_twins() {
        // Smooth: e = 0.25 → 1 - 0.0625 * 2.5 = 0.84375.
        assert_eq!(FalloffCurve::Smooth.eval(0.25, 0.0), 0.84375);
        // Smoother: 0.421875 * (0.75 * (4.5 - 15) + 10) = 0.421875 * 2.125.
        // = 27/64 * 17/8 = 459/512, exact in f32.
        assert_eq!(FalloffCurve::Smoother.eval(0.25, 0.0), 459.0 / 512.0);
        assert_eq!(FalloffCurve::Linear.eval(0.25, 0.0), 0.75);
        assert_eq!(FalloffCurve::Sharp.eval(0.25, 0.0), 0.5625);
    }

    #[test]
    fn ids_are_stable_and_round_trip() {
        let ids: Vec<u32> = FalloffCurve::ALL.iter().map(|c| c.as_u32()).collect();
        // 9 is the GPU stamp id, which has no brush-side variant.
        let mut want: Vec<u32> = (0..9).collect();
        want.push(10);
        assert_eq!(ids, want);
        for c in FalloffCurve::ALL {
            assert_eq!(FalloffCurve::from_u32(c.as_u32()), Some(c));
        }
        assert_eq!(FalloffCurve::from_u32(9), None);
        assert_eq!(FalloffCurve::from_u32(10), Some(FalloffCurve::Custom));
        assert_eq!(FalloffCurve::from_u32(11), None);
        // Custom is LAST: ALL order is the combo order, appended only.
        assert_eq!(FalloffCurve::ALL[9], FalloffCurve::Custom);
        assert_eq!(
            serde_json::to_string(&FalloffCurve::Custom).expect("ser"),
            "\"custom\""
        );
    }

    /// A curve that tells the readings apart: y falls then rises, so a
    /// flipped axis, a rounded index or a lerp between taps all land on
    /// different values.
    fn zigzag() -> ControlCurve {
        ControlCurve::try_new(&[(0.0, 0.0), (0.25, 0.8), (0.6, 0.3), (1.0, 1.0)]).expect("valid")
    }

    /// `eval_with(Custom)` is the LUT's truncated tap. t 0.38 (h 0):
    /// e * 255 = 96.9 → tap 96 (rounding would pick 97), and tap 96 is
    /// the curve at u = 1 - 96/255 = 0.6235 (≈ 0.3412 on the zigzag; the
    /// unflipped axis would read x = 0.3765 ≈ 0.6193, tap 97 ≈ 0.3343).
    /// The hardness remap reaches the same tap: t 0.69, h 0.5 →
    /// e = 0.19 / 0.5 = 0.38.
    #[test]
    fn custom_eval_is_the_luts_truncated_tap() {
        let c = zigzag();
        let tap96 = c.evaluate(1.0 - 96.0 / 255.0);
        assert_eq!(FalloffCurve::Custom.eval_with(0.38, 0.0, &c), tap96);
        assert_eq!(FalloffCurve::Custom.eval_with(0.69, 0.5, &c), tap96);
        assert!((tap96 - 0.3412).abs() < 1e-3, "{tap96}");
        let lut = custom_lut(&c);
        assert_eq!(lut[96], tap96);
        // The parity loop: at every tap's own e (h 0), eval reads the
        // tap the shader would.
        for i in 0..CUSTOM_LUT_LEN {
            let t = i as f32 / 255.0;
            let want = lut[((t * 255.0) as usize).min(255)];
            assert_eq!(FalloffCurve::Custom.eval_with(t, 0.0, &c), want, "t={t}");
        }
        // Core and rim: tap 0 = curve(u 1), tap 255 = curve(u 0).
        assert_eq!(FalloffCurve::Custom.eval_with(0.1, 0.5, &c), 1.0);
        assert_eq!(FalloffCurve::Custom.eval_with(1.0, 0.0, &c), 0.0);
    }

    /// The default identity curve: Linear at every tap (u = 1 - i/255).
    #[test]
    fn custom_identity_is_linear_at_the_taps_and_eval_uses_it() {
        let lut = custom_lut(&ControlCurve::identity());
        assert_eq!(lut[0], 1.0);
        assert_eq!(lut[255], 0.0);
        for (i, tap) in lut.iter().enumerate() {
            let linear = 1.0 - i as f32 / 255.0;
            assert!((tap - linear).abs() < 1e-6, "tap {i}: {tap} vs {linear}");
        }
        assert_eq!(
            FalloffCurve::Custom.eval(0.38, 0.0),
            FalloffCurve::Custom.eval_with(0.38, 0.0, &ControlCurve::identity())
        );
    }

    /// The analytic curves ignore the custom curve (it is inert unless
    /// the enum says Custom).
    #[test]
    fn analytic_curves_ignore_the_custom_curve() {
        let c = zigzag();
        for curve in FalloffCurve::ALL {
            if curve == FalloffCurve::Custom {
                continue;
            }
            for t in [0.1, 0.38, 0.75] {
                assert_eq!(
                    curve.eval_with(t, 0.2, &c).to_bits(),
                    curve.eval(t, 0.2).to_bits(),
                    "{curve:?} t={t}"
                );
            }
        }
    }

    /// Taps clamp into 0..=1: a hand-written curve may carry y = 2 or -1.
    #[test]
    fn custom_taps_clamp_out_of_range_curves() {
        let wild = ControlCurve::try_new(&[(0.0, -1.0), (0.3, 0.5), (0.6, 0.5), (1.0, 2.0)])
            .expect("valid");
        let lut = custom_lut(&wild);
        assert_eq!(lut[0], 1.0, "curve(1) = 2 clamps to 1");
        assert_eq!(lut[255], 0.0, "curve(0) = -1 clamps to 0");
        assert!(lut.iter().all(|t| (0.0..=1.0).contains(t)));
    }

    #[test]
    fn serde_uses_lowercase_names() {
        assert_eq!(
            serde_json::to_string(&FalloffCurve::InverseSquare).expect("serialize"),
            "\"inversesquare\""
        );
        assert_eq!(
            serde_json::from_str::<FalloffCurve>("\"sphere\"").expect("parse"),
            FalloffCurve::Sphere
        );
        assert!(serde_json::from_str::<FalloffCurve>("\"Sphere\"").is_err());
    }
}
