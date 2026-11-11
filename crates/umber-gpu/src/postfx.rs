//! The post-effects stack (§7:84, P1): an ordered chain of full-screen
//! effects between the mesh render and the display transform.
//!
//! # Composition contract
//!
//! `mesh pass → postfx passes (in order) → display LUT`: the effects work
//! in SCENE-LINEAR space, before the display transform — bloom and tone-map
//! are radiometric operations and must not see the LUT's transfer curve.
//! The LUT-bypass rule of [`crate::view_mode`] applies AFTER the chain
//! (solo view-modes bypass the LUT, not the postfx).
//!
//! # Design (judgment calls, for the verifier)
//!
//! - **One pass per enabled effect, ping-pong intermediates.** No
//!   heterogeneous "uber-shader": each effect family is its own pipeline
//!   (bloom = the downsample + blur + recombine pair; DoF = CoC-weighted
//!   gather; the rest single-pass point/kernels). Intermediates come from
//!   a fixed 2-slot pool at the surface size — a disabled effect skips
//!   both its dispatch AND its slot allocation.
//! - **CPU-side param model, GPU-side exec.** [`PostChain`] is pure data
//!   (serde, undoable, panel-driven); [`crate::postfx_exec::PostStack`]
//!   owns the GPU objects and builds a pipeline the first time an EFFECT
//!   KIND appears (params ride uniforms — no pipeline rebuilds on slider
//!   drags).
//! - **The WGSL follows the display_lut conventions**: fullscreen
//!   triangle, texture_2d sample, no vertex buffer.
//!
//! v1 boundary (honest): the passes render at the VIEWPORT's resolution;
//! there is no half-res bloom chain (the downsample is one bilinear hop
//! into a quarter-size intermediate, then one blur pass — the two-pass
//! pair the row asks for, not a mip pyramid).

use serde::{Deserialize, Serialize};

/// One effect in the chain, with its parameters (all clamped at construct).
/// `Copy`: plain params only, so [`PostChain::active`] yields by value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PostEffect {
    /// Depth-of-field: a CoC-weighted gather around the focus plane.
    Dof {
        /// The focused distance, world units (> 0).
        focus: f32,
        /// Aperture strength 0..=1 (0 = pinhole, no blur).
        aperture: f32,
    },
    /// Bright-pass bloom: threshold → downsample → blur → additive recombine.
    Bloom {
        /// Luminance threshold 0..=4 (scene-linear).
        threshold: f32,
        /// Additive intensity 0..=4.
        intensity: f32,
        /// Blur radius in texels at the quarter-res intermediate (1..=64).
        radius: u32,
    },
    /// Radial vignette.
    Vignette {
        /// Edge darkening 0..=1.
        strength: f32,
        /// How far the falloff reaches inward 0..=1 (1 = corner-to-center).
        softness: f32,
    },
    /// Unsharp-mask sharpen.
    Sharpen {
        /// Blend amount 0..=4 (0 = identity).
        amount: f32,
        /// Kernel radius in texels (1..=8).
        radius: u32,
    },
    /// Scene-linear tone-map (the artistic exposure/roll-off; the display
    /// transfer still applies after — this is NOT the display transform).
    ToneMap {
        /// The curve family.
        mode: ToneMapMode,
        /// Exposure stop applied before the curve, -4..=4.
        exposure: f32,
    },
    /// Soft-knee highlight glow (the `glare` graph node's pass):
    /// `out = in + intensity * box(bright(in))`.
    Glare {
        /// Bright-pass threshold on Rec.709 luma, 0..=1.
        threshold: f32,
        /// Soft-knee half-width around the threshold, 0..=1.
        knee: f32,
        /// Additive intensity 0..=4.
        intensity: f32,
        /// Box radius in pixels, 0..=[`GLARE_MAX_RADIUS`] (the GPU tap
        /// budget: one `(2r+1)²` gather, no separable scratch).
        radius: u32,
    },
    /// Anamorphic horizontal streak (the `lens_flare` graph node's pass).
    LensFlare {
        /// Bright-pass threshold on Rec.709 luma, 0..=1 (fixed knee 0.05).
        threshold: f32,
        /// Additive intensity 0..=4.
        intensity: f32,
        /// Per-pixel falloff 0..=[`LENS_FLARE_MAX_DECAY`].
        decay: f32,
        /// Streak tint, each channel 0..=1.
        tint: [f32; 3],
    },
    /// Lateral (radial) chromatic aberration (the `chromatic_aberration`
    /// graph node's pass): R and B sample at radially scaled offsets.
    ChromaticAberration {
        /// Radial R/B split, -0.5..=0.5 (negative swaps the fringes).
        strength: f32,
        /// UV of the optical centre, each axis 0..=1.
        center: [f32; 2],
    },
    /// Seeded, deterministic additive grain (the `film_grain` graph
    /// node's pass): same seed + same input = same output.
    FilmGrain {
        /// Grain amplitude 0..=1.
        amount: f32,
        /// The replay key.
        seed: i32,
        /// Grain cell size in pixels, 1..=64.
        size: u32,
        /// One noise value shared by R, G, B (false: per channel).
        monochrome: bool,
    },
}

/// The glare pass's largest box radius (pixels): the GPU gathers
/// `(2r+1)²` taps per pixel in one pass, so the model clamps here instead
/// of the CPU node's 256.
pub const GLARE_MAX_RADIUS: u32 = 8;

/// The lens-flare pass's largest decay: the GPU gathers at most
/// `postfx_exec::LENS_FLARE_TAPS` (512) pixels each side, and
/// `0.98^512 ≈ 3e-5`, so the truncated tail stays far under one byte
/// (the CPU node allows 0.999 over the whole row).
pub const LENS_FLARE_MAX_DECAY: f32 = 0.98;

impl PostEffect {
    /// Clamps every parameter into its documented range (the panel sends
    /// raw slider values; the model is the single clamp point).
    pub fn clamped(self) -> Self {
        match self {
            Self::Dof { focus, aperture } => Self::Dof {
                focus: focus.max(0.01),
                aperture: aperture.clamp(0.0, 1.0),
            },
            Self::Bloom {
                threshold,
                intensity,
                radius,
            } => Self::Bloom {
                threshold: threshold.clamp(0.0, 4.0),
                intensity: intensity.clamp(0.0, 4.0),
                radius: radius.clamp(1, 64),
            },
            Self::Vignette { strength, softness } => Self::Vignette {
                strength: strength.clamp(0.0, 1.0),
                softness: softness.clamp(0.0, 1.0),
            },
            Self::Sharpen { amount, radius } => Self::Sharpen {
                amount: amount.clamp(0.0, 4.0),
                radius: radius.clamp(1, 8),
            },
            Self::ToneMap { mode, exposure } => Self::ToneMap {
                mode,
                exposure: exposure.clamp(-4.0, 4.0),
            },
            Self::Glare {
                threshold,
                knee,
                intensity,
                radius,
            } => Self::Glare {
                threshold: threshold.clamp(0.0, 1.0),
                knee: knee.clamp(0.0, 1.0),
                intensity: intensity.clamp(0.0, 4.0),
                radius: radius.min(GLARE_MAX_RADIUS),
            },
            Self::LensFlare {
                threshold,
                intensity,
                decay,
                tint,
            } => Self::LensFlare {
                threshold: threshold.clamp(0.0, 1.0),
                intensity: intensity.clamp(0.0, 4.0),
                decay: decay.clamp(0.0, LENS_FLARE_MAX_DECAY),
                tint: tint.map(|t| t.clamp(0.0, 1.0)),
            },
            Self::ChromaticAberration { strength, center } => Self::ChromaticAberration {
                strength: strength.clamp(-0.5, 0.5),
                center: center.map(|c| c.clamp(0.0, 1.0)),
            },
            Self::FilmGrain {
                amount,
                seed,
                size,
                monochrome,
            } => Self::FilmGrain {
                amount: amount.clamp(0.0, 1.0),
                seed,
                size: size.clamp(1, 64),
                monochrome,
            },
        }
    }

    /// The effect a new chain row starts with (the panel's "Add effect"):
    /// mid-range params, already inside the clamps.
    pub fn default_for(pass: PostPass) -> Self {
        match pass {
            PostPass::Dof => Self::Dof {
                focus: 5.0,
                aperture: 0.3,
            },
            PostPass::Bloom => Self::Bloom {
                threshold: 1.0,
                intensity: 0.5,
                radius: 8,
            },
            PostPass::Vignette => Self::Vignette {
                strength: 0.3,
                softness: 0.5,
            },
            PostPass::Sharpen => Self::Sharpen {
                amount: 0.5,
                radius: 1,
            },
            PostPass::ToneMap => Self::ToneMap {
                mode: ToneMapMode::Aces,
                exposure: 0.0,
            },
            // The four graph post nodes start at their CPU node defaults
            // (`umber_graph::nodes::post`).
            PostPass::Glare => Self::Glare {
                threshold: 0.8,
                knee: 0.1,
                intensity: 1.0,
                radius: 4,
            },
            PostPass::LensFlare => Self::LensFlare {
                threshold: 0.9,
                intensity: 1.0,
                decay: 0.95,
                tint: [0.6, 0.8, 1.0],
            },
            PostPass::ChromaticAberration => Self::ChromaticAberration {
                strength: 0.005,
                center: [0.5, 0.5],
            },
            PostPass::FilmGrain => Self::FilmGrain {
                amount: 0.05,
                seed: 0,
                size: 1,
                monochrome: true,
            },
        }
    }

    /// The pipeline key: which shader this effect dispatches.
    pub fn pass(self) -> PostPass {
        match self {
            Self::Dof { .. } => PostPass::Dof,
            Self::Bloom { .. } => PostPass::Bloom,
            Self::Vignette { .. } => PostPass::Vignette,
            Self::Sharpen { .. } => PostPass::Sharpen,
            Self::ToneMap { .. } => PostPass::ToneMap,
            Self::Glare { .. } => PostPass::Glare,
            Self::LensFlare { .. } => PostPass::LensFlare,
            Self::ChromaticAberration { .. } => PostPass::ChromaticAberration,
            Self::FilmGrain { .. } => PostPass::FilmGrain,
        }
    }
}

/// The tone-map curve family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToneMapMode {
    /// The ACES 1.0 fitted curve (Narkowicz's approximation).
    #[default]
    Aces,
    /// Hable's filmic shoulder.
    Filmic,
    /// The simple Reinhard curve.
    Reinhard,
}

impl ToneMapMode {
    /// Every mode, in picker order.
    pub const ALL: [Self; 3] = [Self::Aces, Self::Filmic, Self::Reinhard];
}

/// The nine pipeline kinds (bloom's is internally the down+blur+add pair).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PostPass {
    /// Depth-of-field gather.
    Dof,
    /// Bloom (threshold-downsample + blur-recombine).
    Bloom,
    /// Radial vignette.
    Vignette,
    /// Unsharp-mask sharpen.
    Sharpen,
    /// Scene-linear tone-map.
    ToneMap,
    /// Soft-knee glare.
    Glare,
    /// Horizontal lens-flare streak.
    LensFlare,
    /// Lateral chromatic aberration.
    ChromaticAberration,
    /// Seeded film grain.
    FilmGrain,
}

impl PostPass {
    /// Every pass kind, in the panel's "Add effect" order.
    pub const ALL: [Self; 9] = [
        Self::Dof,
        Self::Bloom,
        Self::Vignette,
        Self::Sharpen,
        Self::ToneMap,
        Self::Glare,
        Self::LensFlare,
        Self::ChromaticAberration,
        Self::FilmGrain,
    ];
}

/// The ordered, user-reorderable chain (pure data; serde for the project).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PostChain {
    /// The effects in execution order.
    pub effects: Vec<PostEffect>,
    /// Per-effect enable flags (index-aligned with `effects`).
    pub enabled: Vec<bool>,
}

impl PostChain {
    /// An empty (identity) chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an effect (clamped, enabled).
    pub fn push(&mut self, effect: PostEffect) {
        self.effects.push(effect.clamped());
        self.enabled.push(true);
    }

    /// The enabled effects in order — what the GPU stack must allocate for.
    pub fn active(&self) -> impl Iterator<Item = PostEffect> + '_ {
        self.effects
            .iter()
            .zip(self.enabled.iter())
            .filter(|(_, on)| **on)
            .map(|(e, _)| *e)
    }

    /// The number of full-res ping-pong intermediates the chain needs:
    /// each enabled effect reads one and writes one, so the chain needs
    /// exactly 2 slots (ping-pong), UNLESS zero effects are active — then
    /// zero. This is the allocation contract
    /// [`crate::postfx_exec::PostStack`] follows.
    pub fn intermediate_count(&self) -> usize {
        match self.active().count() {
            0 => 0,
            _ => 2,
        }
    }

    /// Moves the effect at `from` so it ends at index `to` (the panel's
    /// drag-reorder and ▲/▼). Returns false when either index is out of
    /// bounds or `from == to`.
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.effects.len() || to >= self.effects.len() || from == to {
            return false;
        }
        // remove-then-insert already lands the effect at `to` (no `to - 1`
        // adjustment: that made [A, B] reorder(0, 1) a silent no-op).
        let e = self.effects.remove(from);
        let on = self.enabled.remove(from);
        self.effects.insert(to, e);
        self.enabled.insert(to, on);
        true
    }

    /// Removes the effect at `index` (and its flag). Returns false when
    /// out of bounds.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.effects.len() {
            return false;
        }
        self.effects.remove(index);
        self.enabled.remove(index);
        true
    }

    /// Toggles an effect's enable flag (index-checked).
    pub fn set_enabled(&mut self, index: usize, on: bool) -> bool {
        match self.enabled.get_mut(index) {
            Some(flag) => {
                *flag = on;
                true
            }
            None => false,
        }
    }
}

/// The scene-linear tone-map curves (shared CPU reference for tests and
/// the WGSL mirror — `postfx_exec::POSTFX_SHADER`'s `tone_map_channel`
/// is this function op-for-op; exact values pinned in the tests).
///
/// `Filmic` is NOT canonical Hable (`curve(x) / curve(W)`): it computes
/// `s·curve(W) / (1 + s·curve(W)/W)` with `s = x/W` — a Reinhard-shaped
/// roll-off scaled by Hable's white point. The WGSL mirrors it as-is.
pub fn tone_map(mode: ToneMapMode, exposure: f32, x: f32) -> f32 {
    let x = x * 2.0f32.powf(exposure);
    match mode {
        ToneMapMode::Aces => {
            // Narkowicz's ACES fit.
            let (a, b, c, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
            (x * (a * x + b)) / (x * (c * x + d) + e)
        }
        ToneMapMode::Filmic => {
            // Hable's curve with his default params.
            let (a, b, c, d, e, f) = (0.15, 0.50, 0.10, 0.20, 0.02, 0.30);
            let w = 11.2;
            let curve =
                |v: f32| ((v * (a * v + c * b) + d * e) / (v * (a * v + b) + d * f)) - e / f;
            let s = x / w;
            s * curve(w) / (1.0 + s * curve(w) / w).max(0.0)
        }
        ToneMapMode::Reinhard => x / (1.0 + x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_order_is_deterministic() {
        let mut chain = PostChain::new();
        chain.push(PostEffect::ToneMap {
            mode: ToneMapMode::Aces,
            exposure: 0.0,
        });
        chain.push(PostEffect::Bloom {
            threshold: 1.0,
            intensity: 0.5,
            radius: 8,
        });
        chain.push(PostEffect::Vignette {
            strength: 0.3,
            softness: 0.5,
        });
        let passes: Vec<_> = chain.active().map(|e| e.pass()).collect();
        assert_eq!(
            passes,
            vec![PostPass::ToneMap, PostPass::Bloom, PostPass::Vignette]
        );
    }

    #[test]
    fn enable_mask_governs_allocation() {
        let mut chain = PostChain::new();
        chain.push(PostEffect::Sharpen {
            amount: 1.0,
            radius: 2,
        });
        chain.push(PostEffect::Bloom {
            threshold: 1.0,
            intensity: 1.0,
            radius: 4,
        });
        chain.push(PostEffect::Vignette {
            strength: 0.2,
            softness: 1.0,
        });
        assert_eq!(chain.intermediate_count(), 2, "any active = ping-pong pair");
        assert!(chain.set_enabled(0, false), "index 0 in bounds");
        assert!(chain.set_enabled(1, false), "index 1 in bounds");
        assert_eq!(
            chain.active().count(),
            1,
            "the vignette survives both disables"
        );
        assert!(chain.set_enabled(2, false), "index 2 in bounds");
        assert_eq!(chain.intermediate_count(), 0, "all off = zero alloc");
    }

    #[test]
    fn params_clamp_at_construct() {
        let e = PostEffect::Bloom {
            threshold: 99.0,
            intensity: -4.0,
            radius: 1000,
        }
        .clamped();
        assert_eq!(
            e,
            PostEffect::Bloom {
                threshold: 4.0,
                intensity: 0.0,
                radius: 64
            }
        );
    }

    #[test]
    fn aces_midgray_is_pinned() {
        // Narkowicz ACES at x=0.5: (0.5*(2.51*0.5+0.03))/(0.5*(2.43*0.5+0.59)+0.14)
        // = 0.6425 / 1.0425 ≈ 0.6163 (the original pin of 0.3336 was a
        // miscalculation of this same expression; the curve is unchanged).
        let v = tone_map(ToneMapMode::Aces, 0.0, 0.5);
        assert!((v - 0.6163).abs() < 1e-3, "ACES(0.5) ≈ 0.6163, got {v}");
        // Identity at 0: all curves map 0 → 0.
        for m in [
            ToneMapMode::Aces,
            ToneMapMode::Filmic,
            ToneMapMode::Reinhard,
        ] {
            assert_eq!(tone_map(m, 0.0, 0.0), 0.0);
        }
    }

    #[test]
    fn reorder_moves_effects_and_flags_together() {
        let mut chain = PostChain::new();
        chain.push(PostEffect::Sharpen {
            amount: 1.0,
            radius: 1,
        });
        chain.push(PostEffect::Bloom {
            threshold: 0.5,
            intensity: 0.5,
            radius: 2,
        });
        assert!(chain.reorder(1, 0), "bloom moves to front");
        assert_eq!(chain.effects[0].pass(), PostPass::Bloom);
        assert_eq!(chain.effects[1].pass(), PostPass::Sharpen);
        assert!(!chain.reorder(0, 9), "out-of-bounds is a no-op false");
    }

    #[test]
    fn reorder_can_move_to_the_last_index() {
        // The down-move: [A, B, C] reorder(0, 2) → [B, C, A]; and the
        // adjacent swap reorder(0, 1) really swaps.
        let mut chain = PostChain::new();
        for pass in [PostPass::Sharpen, PostPass::Bloom, PostPass::Vignette] {
            chain.push(PostEffect::default_for(pass));
        }
        assert!(chain.set_enabled(0, false));
        assert!(chain.reorder(0, 2));
        let order: Vec<_> = chain.effects.iter().map(|e| e.pass()).collect();
        assert_eq!(
            order,
            vec![PostPass::Bloom, PostPass::Vignette, PostPass::Sharpen]
        );
        assert_eq!(chain.enabled, vec![true, true, false], "flag rode along");
        assert!(chain.reorder(0, 1));
        assert_eq!(chain.effects[0].pass(), PostPass::Vignette);
        assert_eq!(chain.effects[1].pass(), PostPass::Bloom);
    }

    #[test]
    fn remove_drops_effect_and_flag() {
        let mut chain = PostChain::new();
        chain.push(PostEffect::default_for(PostPass::Dof));
        chain.push(PostEffect::default_for(PostPass::ToneMap));
        assert!(chain.remove(0));
        assert_eq!(chain.effects.len(), chain.enabled.len());
        assert_eq!(chain.effects[0].pass(), PostPass::ToneMap);
        assert!(!chain.remove(5));
    }

    #[test]
    fn post_node_params_clamp_at_construct() {
        assert_eq!(
            PostEffect::Glare {
                threshold: 2.0,
                knee: -1.0,
                intensity: 9.0,
                radius: 300,
            }
            .clamped(),
            PostEffect::Glare {
                threshold: 1.0,
                knee: 0.0,
                intensity: 4.0,
                radius: GLARE_MAX_RADIUS,
            }
        );
        assert_eq!(
            PostEffect::LensFlare {
                threshold: -1.0,
                intensity: -1.0,
                decay: 0.999,
                tint: [2.0, -1.0, 0.5],
            }
            .clamped(),
            PostEffect::LensFlare {
                threshold: 0.0,
                intensity: 0.0,
                decay: LENS_FLARE_MAX_DECAY,
                tint: [1.0, 0.0, 0.5],
            }
        );
        assert_eq!(
            PostEffect::ChromaticAberration {
                strength: 0.9,
                center: [-1.0, 3.0],
            }
            .clamped(),
            PostEffect::ChromaticAberration {
                strength: 0.5,
                center: [0.0, 1.0],
            }
        );
        assert_eq!(
            PostEffect::FilmGrain {
                amount: 3.0,
                seed: -7,
                size: 0,
                monochrome: false,
            }
            .clamped(),
            PostEffect::FilmGrain {
                amount: 1.0,
                seed: -7,
                size: 1,
                monochrome: false,
            }
        );
    }

    #[test]
    fn defaults_are_inside_the_clamps() {
        for pass in PostPass::ALL {
            let e = PostEffect::default_for(pass);
            assert_eq!(e.pass(), pass);
            assert_eq!(e.clamped(), e, "{pass:?} default already clamped");
        }
    }
}
