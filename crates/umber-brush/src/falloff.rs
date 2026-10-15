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
//! - Blender's custom-curve option is deferred (needs a curve editor in
//!   the brush panel).

use serde::{Deserialize, Serialize};
use umber_gpu::paint as gpu;

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
}

impl FalloffCurve {
    /// Every curve, in id order.
    pub const ALL: [FalloffCurve; 9] = [
        FalloffCurve::Smooth,
        FalloffCurve::Smoother,
        FalloffCurve::Sphere,
        FalloffCurve::Root,
        FalloffCurve::Sharp,
        FalloffCurve::Sharper,
        FalloffCurve::Linear,
        FalloffCurve::InverseSquare,
        FalloffCurve::Constant,
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
        }
    }

    /// The curve for a GPU id, or `None` for an unknown id.
    pub fn from_u32(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_u32() == id)
    }

    /// CPU reference of the GPU `radial_falloff`: strength at normalized
    /// distance `t` (0 = centre, 1 = rim) with a flat core of `hardness`.
    /// Mirrors the WGSL operation for operation.
    pub fn eval(self, t: f32, hardness: f32) -> f32 {
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
        }
    }
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
        assert_eq!(ids, (0..9).collect::<Vec<u32>>());
        for c in FalloffCurve::ALL {
            assert_eq!(FalloffCurve::from_u32(c.as_u32()), Some(c));
        }
        assert_eq!(FalloffCurve::from_u32(9), None);
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
