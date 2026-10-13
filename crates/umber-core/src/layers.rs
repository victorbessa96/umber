//! The layer stack: paint/fill/folder layers with blend modes, opacity, and
//! masks (requirements.md §2).
//!
//! [`LayerStack`] is a flat, ordered `Vec<Layer>`. Folder *membership* (which
//! layers nest inside a given folder) is not modeled yet — see
//! LANDING_NOTES.md for why that is an open question left to the Wave 2
//! compositor rather than guessed at here.

use serde::{Deserialize, Serialize};

use std::mem::size_of;

use umber_mesh::FillParams;

use crate::undo::Command;

/// What a layer contains and how its children (if any) relate to the stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayerKind {
    /// A layer painted into directly with brush strokes.
    Paint,
    /// A layer filled procedurally via a projection (UV/tri-planar/etc).
    /// Stays a plain tag: the evaluatable parameters live on
    /// [`Layer::fill`] (`Some` exactly for fill layers), so the
    /// journal/project matches over `LayerKind` never churn when the fill
    /// model grows.
    Fill,
    /// A group of layers.
    ///
    /// `passthrough: true` means child layers blend directly with the
    /// layers below the folder, as if the folder were not there. `false`
    /// means the folder's contents are flattened to a single result first,
    /// and that result blends with layers below using the folder layer's
    /// own `blend_mode`/`opacity`. See [`BlendMode::Passthrough`] for how
    /// this flag relates to the blend-mode enum.
    Folder {
        /// See the variant's own doc comment above.
        passthrough: bool,
    },
    /// An instance: renders its source layer's composited pixels.
    ///
    /// Source-edits propagate: the composite eval walks instances
    /// transitively (A instances B instances C → eval order C,B,A — see
    /// [`LayerStack::instance_eval_order`]). The instance graph is a DAG:
    /// a would-be cycle is rejected at attach time with
    /// [`InstanceError::Cycle`].
    ///
    /// The `Layer`'s own `opacity`/`blend_mode`/`mask` act as per-instance
    /// OVERLAYS on the link (not edits to the source pixels): an instance
    /// can restyle its source without touching it. Instances carry no
    /// pixels of their own, so painting one directly is rejected with
    /// [`InstanceError::PaintOnInstance`].
    Instance {
        /// The instanced layer's id (same texture set).
        source_id: u64,
    },
}

/// Everything that can go wrong with layer instancing (§3:41).
///
/// JUDGMENT CALL — cycle logic lives here as a small DFS rather than
/// reusing `umber-graph`'s `topo.rs`: `umber-core` is the dependency-minimal
/// document model (it does not depend on `umber-graph`), and pulling the
/// graph crate in for one DFS would invert the layering. The algorithm is
/// the same shape (Kahn sweep for ordering, DFS for the would-be-cycle
/// probe) and is documented as a structural mirror.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum InstanceError {
    /// No layer with this id exists in the stack.
    #[error("unknown layer {0}")]
    UnknownLayer(u64),
    /// The requested source layer does not exist.
    #[error("instance {instance} references unknown source id {source_id}")]
    UnknownSource {
        /// The instance layer's id.
        instance: u64,
        /// The missing source id.
        source_id: u64,
    },
    /// The layer is not an instance (retarget/overlay ops need one).
    #[error("layer {0} is not an instance")]
    NotAnInstance(u64),
    /// Attaching this source would close a cycle (participants listed,
    /// starting at the instance being retargeted).
    #[error("instance cycle detected among layers {0:?}")]
    Cycle(Vec<u64>),
    /// Instances carry no pixels: paint the source instead.
    #[error("layer {instance} is an instance of {source_id}: paint the source, not the instance")]
    PaintOnInstance {
        /// The instance a paint was attempted on.
        instance: u64,
        /// The source to paint instead.
        source_id: u64,
    },
}

/// The core-12 blend modes plus the separable extras the stack already
/// carried (requirements.md §2: "P0 core 12, full set 4").
///
/// Blending is computed in linear space, per channel (architecture.md), via
/// [`BlendMode::blend_channel`]. A layer's `blend_mode` is meaningful for
/// `Paint`/`Fill` layers and for a `Folder` layer's own composite step;
/// `Passthrough` only makes sense on a `Folder` layer and is the
/// blend-mode-enum spelling of what [`LayerKind::Folder`]'s `passthrough`
/// flag already controls. The two are kept as separate fields (flag vs.
/// enum variant) because the task's authoritative source for "does this
/// folder pass through" is the flag; the compositor (not built in this
/// task) is expected to treat a folder's `blend_mode: Passthrough` as
/// redundant with `passthrough: true` and should not need to special-case
/// a mismatch, but resolving that precisely is left open for Wave 2's
/// compositor work.
///
/// # Blend-set census (2026-10-12: full 32)
///
/// The 32 modes of Painter's blending-modes doc page (verified 2026-10-12
/// against the live page, which also confirms every mode runs in linear
/// gamma space internally), in the doc's order: Normal, Passthrough,
/// Disable, Replace, Multiply, Divide, Inverse Divide, Darken, Lighten,
/// Linear Dodge (Add), Subtract, Inverse Subtract, Difference, Exclusion,
/// Signed Addition (AddSub), Overlay, Screen, Linear Burn, Color Burn,
/// Color Dodge, Soft Light, Hard Light, Vivid Light, Linear Light,
/// Pin Light, Tint, Saturation, Color, Value, Normal Map Combine,
/// Normal Map Detail, Normal Map Inverse Detail.
///
/// Before this task 15 were built (14 separable + Passthrough). This task
/// adds the remaining 17: 10 separable (Divide, InverseDivide,
/// InverseSubtract, Exclusion, AddSub, VividLight, LinearLight, PinLight,
/// Disable, Replace) and 7 triplet (Tint, Saturation, Color, Value,
/// NormalCombine, NormalDetail, NormalInverseDetail) — see
/// [`BlendMode::blend_rgb`] for why the triplets cannot live in the
/// per-channel scalar primitive.
///
/// Two corrections to early-wave guesses, verified against the doc page:
/// - Combine is Whiteout and Detail is Reoriented Normal Mapping ("more
///   precise than Combine"); Inverse Detail is RNM with the roles
///   exchanged (the bottom fitted to the top). An early brief had
///   Combine=RNM guesswork — the doc page rules it out.
/// - Reflect/Glow are not in Painter's list, so they stay out. The
///   separable remainder is Divide/Inverse-Divide/Inverse-Subtract/
///   Exclusion/AddSub/Vivid/Linear/Pin/Disable/Replace.
/// - Painter's Dissolve is noise-based (needs RNG/seed plumbing per texel)
///   and is the one documented mode with no variant: deliberately excluded,
///   not separable-safe (see the [`BlendMode::NormalDetail`] docs for the
///   other v1 boundaries this census takes on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    /// Top layer fully replaces what's below it, modulated by opacity.
    #[default]
    Normal,
    /// Folder-only: children blend directly with layers below the folder,
    /// as if the folder were not there. See [`LayerKind::Folder`].
    /// Not a pixel op: [`BlendMode::blend_channel`] returns `None`.
    Passthrough,
    /// Result × base — darkens; black stays black, white is a no-op.
    Multiply,
    /// Inverse-multiply — lightens; white stays white, black is a no-op.
    Screen,
    /// Multiply below 0.5 result luminance, Screen above — contrast boost.
    Overlay,
    /// Per-channel minimum of result and base.
    Darken,
    /// Per-channel maximum of result and base.
    Lighten,
    /// Painter's "Linear Dodge (Add)": `min(1, src + dst)` — linear-light
    /// additive blend; clamps at white. Covers both the `linear-dodge`
    /// and the `add` slots of the task's core-12 (they are one mode).
    Add,
    /// Painter's "Subtract": `max(0, dst - src)` — base minus blend;
    /// clamps at black. NOT the same as [`BlendMode::LinearBurn`].
    Subtract,
    /// Absolute difference between result and base.
    Difference,
    /// Low-contrast Overlay variant (Photoshop/Substance-style soft light,
    /// Pegtop approximation — see [`BlendMode::blend_channel`]).
    SoftLight,
    /// Higher-contrast Overlay variant (roles of result/base swapped).
    HardLight,
    /// Brightens the base toward white proportionally to the blend:
    /// `src >= 1` yields white (guarded — a naive `dst / (1 - src)`
    /// divides by zero), else `min(1, dst / (1 - src))`.
    ColorDodge,
    /// Darkens the base toward black proportionally to the blend:
    /// `src <= 0` yields black (guarded), else
    /// `1 - min(1, (1 - dst) / src)`.
    ColorBurn,
    /// Painter's "Linear Burn": `max(0, src + dst - 1)` — darkens every
    /// pair whose sum is under white; clamps at black. Distinct from
    /// [`BlendMode::Subtract`] (`dst - src`): e.g. src=0.6, dst=0.7 gives
    /// 0.3 here, 0.1 under Subtract.
    LinearBurn,
    /// Painter's "Divide": `dst / src` — lightens, often to burned-out
    /// white. `src <= 0` yields white (guarded — a naive division gives
    /// `+inf`, and `0/0` gives `NaN`), else `min(1, dst / src)`.
    Divide,
    /// Painter's "Inverse Divide": Divide with top and bottom exchanged,
    /// i.e. `src / dst`. `dst <= 0` yields white (guarded), else
    /// `min(1, src / dst)`.
    InverseDivide,
    /// Painter's "Inverse Subtract": Subtract with top and bottom
    /// exchanged, i.e. `max(0, src - dst)` — the blend minus the base;
    /// clamps at black.
    InverseSubtract,
    /// Exclusion: `src + dst - 2·src·dst` — Difference's lower-contrast
    /// sibling; identical to Difference at the corners, grayer mid-range.
    Exclusion,
    /// Painter's "Signed Addition (AddSub)": `src + dst - 0.5`, clamped —
    /// mid-gray blend is a no-op, darker blends subtract, lighter add.
    AddSub,
    /// Vivid Light: Color Burn below mid-gray blend, Color Dodge above —
    /// `burn(dst, 2·src)` for `src <= 0.5`, `dodge(dst, 2·src - 1)`
    /// otherwise (exact no-op at `src = 0.5`; the dodge/burn guards carry
    /// over, so black blend burns to black and white dodges to white).
    VividLight,
    /// Linear Light: Linear Dodge above mid-gray, Linear Burn below —
    /// `dst + 2·src - 1`, clamped (exact no-op at `src = 0.5`).
    LinearLight,
    /// Pin Light: Darken below mid-gray blend, Lighten above —
    /// `min(dst, 2·src)` for `src <= 0.5`, `max(dst, 2·src - 1)`
    /// otherwise (exact no-op at `src = 0.5`; drops mid-tones by design).
    PinLight,
    /// Painter's "Disable": discards the layer, showing only the layers
    /// below — a dispatch skip spelled as a pixel op (`dst` unchanged at
    /// any opacity, so [`BlendMode::composite_channel`] is already exact).
    Disable,
    /// Painter's "Replace": overwrites the base with the source. At the
    /// [`BlendMode::blend_channel`] level this is `src` (Normal's math);
    /// what distinguishes Replace in Painter is ignoring the layer's
    /// opacity and the source alpha — an opacity-bypass the compositor
    /// does NOT implement yet (it gates every mode by coverage), so v1
    /// Replace composites exactly like Normal and the bypass is a named
    /// follow-up for the channel-routing work. See
    /// [`BlendMode::NormalDetail`]'s docs for the family's v1 boundaries.
    Replace,
    /// Painter's "Tint" (Hue): hue of the top, saturation and value of
    /// the bottom, via the HSV cylinder in linear working space (see
    /// [`BlendMode::blend_rgb`]). An achromatic top (no hue to take —
    /// black, gray, white) leaves the bottom unchanged, per the doc's
    /// "very dark colors remain unchanged" (v1 reads "very dark" as the
    /// exactly-achromatic guard; near-black chromatic texels still take
    /// hue 0 — a documented approximation).
    Tint,
    /// Painter's "Saturation": saturation of the top, hue and value of
    /// the bottom. A black top desaturates the bottom to gray at the
    /// bottom's value (e.g. red becomes white) — the doc's pinned
    /// behavior, not a guard bug.
    Saturation,
    /// Painter's "Color": hue and saturation of the top, value of the
    /// bottom. Achromatic top leaves the bottom unchanged (same guard as
    /// [`BlendMode::Tint`]).
    Color,
    /// Painter's "Value" (Luminosity slot): hue and saturation of the
    /// bottom, value of the top. Always defined — no guard.
    Value,
    /// Painter's "Normal Map Combine": Whiteout blending
    /// (`n1.xy + n2.xy`, `n1.z·n2.z`, renormalized — Oat 2007, per
    /// Barré-Brisebois & Hill's survey). Flat normals are identities on
    /// both sides. Normal-channel-only: see [`BlendMode::is_normal_map`].
    NormalCombine,
    /// Painter's "Normal Map Detail": Reoriented Normal Mapping — the top
    /// (detail) is reoriented to follow the bottom (base) surface before
    /// combining, preserving both intensities ("more precise than
    /// Combine"). Normal-channel-only: see [`BlendMode::is_normal_map`].
    ///
    /// # v1 boundaries (all load-bearing; read before extending)
    ///
    /// - No DirectX/OpenGL Y-flip is applied: the engine operates on raw
    ///   encoded triplets in whatever Y convention the channel holds. The
    ///   flip belongs to import/export color management, not the blend.
    /// - The normal-channel gate is a routing contract, not a math check:
    ///   [`BlendMode::is_normal_map`] tells the channel-routing layer
    ///   (the viewport-texture kill's surface, per the compositor docs)
    ///   which modes may only be selected for the normal channel. The
    ///   math itself runs on any triplet it is handed.
    /// - Degenerate triplets (a mid-gray texel decodes to the zero vector,
    ///   which is not a direction) fall back to the base unchanged rather
    ///   than producing `NaN`.
    NormalDetail,
    /// Painter's "Normal Map Inverse Detail": RNM with the roles
    /// exchanged — the bottom is fitted to the top's surface, i.e.
    /// `rnm(base = top, detail = bottom)`. Same v1 boundaries as
    /// [`BlendMode::NormalDetail`].
    NormalInverseDetail,
}

impl BlendMode {
    /// Clamps a linear-space channel value to `0.0..=1.0` for blending.
    ///
    /// Fail-closed for ALL non-finite input: `NaN` AND `±inf` map to `0.0`
    /// rather than poisoning the composite (the wave-1 battery caught the
    /// inf leak cross-platform: `clamp(inf, 0, 1)` is 1.0 on the CPU and
    /// diverges in WGSL stacks — a `±inf` texel must composite to
    /// *something* finite; black is the safe choice, matching how
    /// [`LayerStack::set_opacity`] refuses to store non-finite input
    /// instead of letting it through).
    fn clamp01(v: f32) -> f32 {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Blends one linear-space channel: `src` is the top (blend) layer's
    /// value, `dst` the accumulated base below — the same src/dst
    /// convention as the stamp composite (`src * cov + dst * (1 - …)`).
    ///
    /// Returns `None` for [`BlendMode::Passthrough`] (structural: folder
    /// children blend directly below) and for the seven triplet modes
    /// (Tint, Saturation, Color, Value, NormalCombine, NormalDetail,
    /// NormalInverseDetail), which need an RGB triplet and live in [`BlendMode::blend_rgb`] instead — callers
    /// must handle folders before reaching per-pixel math, and must route
    /// triplet modes through `blend_rgb` (single-channel use of those is
    /// meaningless, so `None` is the honest answer, not a panic).
    /// Every other variant returns `Some` finite `0.0..=1.0` value for any
    /// finite *or* non-finite input (inputs pass through [`BlendMode`]'s
    /// private clamp first, and the dodge/burn/divide guards below keep
    /// the `1 - src` / `src` / `dst` divisors off zero).
    ///
    /// Soft-light is the Pegtop approximation
    /// `((1 - 2·src)·dst + 2·src)·dst`: exact no-op at 50% gray
    /// (`src = 0.5` returns `dst`), softer than Overlay at the extremes —
    /// the standard cheap Photoshop/Substance-style form.
    pub fn blend_channel(self, src: f32, dst: f32) -> Option<f32> {
        let (s, d) = (Self::clamp01(src), Self::clamp01(dst));
        let v = match self {
            BlendMode::Normal => s,
            BlendMode::Passthrough => return None,
            BlendMode::Multiply => s * d,
            BlendMode::Screen => s + d - s * d,
            BlendMode::Overlay => {
                if d <= 0.5 {
                    2.0 * s * d
                } else {
                    1.0 - 2.0 * (1.0 - s) * (1.0 - d)
                }
            }
            BlendMode::Darken => s.min(d),
            BlendMode::Lighten => s.max(d),
            BlendMode::Add => (s + d).min(1.0),
            BlendMode::Subtract => (d - s).max(0.0),
            BlendMode::Difference => (s - d).abs(),
            BlendMode::SoftLight => ((1.0 - 2.0 * s) * d + 2.0 * s) * d,
            BlendMode::HardLight => {
                if s <= 0.5 {
                    2.0 * s * d
                } else {
                    1.0 - 2.0 * (1.0 - s) * (1.0 - d)
                }
            }
            BlendMode::ColorDodge => Self::color_dodge(s, d),
            BlendMode::ColorBurn => Self::color_burn(s, d),
            BlendMode::LinearBurn => (s + d - 1.0).max(0.0),
            BlendMode::Divide => {
                if s <= 0.0 {
                    1.0
                } else {
                    (d / s).min(1.0)
                }
            }
            BlendMode::InverseDivide => {
                if d <= 0.0 {
                    1.0
                } else {
                    (s / d).min(1.0)
                }
            }
            BlendMode::InverseSubtract => (s - d).max(0.0),
            BlendMode::Exclusion => s + d - 2.0 * s * d,
            BlendMode::AddSub => (d + s - 0.5).clamp(0.0, 1.0),
            BlendMode::VividLight => {
                if s <= 0.5 {
                    Self::color_burn(2.0 * s, d)
                } else {
                    Self::color_dodge(2.0 * s - 1.0, d)
                }
            }
            BlendMode::LinearLight => (d + 2.0 * s - 1.0).clamp(0.0, 1.0),
            BlendMode::PinLight => {
                if s <= 0.5 {
                    d.min(2.0 * s)
                } else {
                    d.max(2.0 * (s - 0.5))
                }
            }
            BlendMode::Disable => d,
            BlendMode::Replace => s,
            BlendMode::Tint
            | BlendMode::Saturation
            | BlendMode::Color
            | BlendMode::Value
            | BlendMode::NormalCombine
            | BlendMode::NormalDetail
            | BlendMode::NormalInverseDetail => return None,
        };
        Some(Self::clamp01(v))
    }

    /// Blends one channel and mixes the result with the base according to
    /// `opacity` (`0` = base unchanged, `1` = full blend effect) — the
    /// per-pixel half of a layer's `blend_mode` + `opacity` pair.
    ///
    /// Non-finite `opacity` is treated as `0.0` (base unchanged), mirroring
    /// [`LayerStack::set_opacity`]'s refusal to store non-finite input.
    /// Returns `None` exactly when [`BlendMode::blend_channel`] does
    /// (`Passthrough` plus the seven triplet modes, which have no
    /// single-channel meaning — use [`BlendMode::blend_rgb`]).
    pub fn composite_channel(self, src: f32, dst: f32, opacity: f32) -> Option<f32> {
        let blend = self.blend_channel(src, dst)?;
        let base = Self::clamp01(dst);
        let o = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(Self::clamp01(base + (blend - base) * o))
    }

    /// The guarded Color Dodge primitive shared by [`BlendMode::ColorDodge`]
    /// and [`BlendMode::VividLight`]: `src >= 1` dodges to white, else
    /// `min(1, dst / (1 - src))`. Inputs are already clamped by the caller.
    fn color_dodge(s: f32, d: f32) -> f32 {
        if s >= 1.0 {
            1.0
        } else {
            (d / (1.0 - s)).min(1.0)
        }
    }

    /// The guarded Color Burn primitive shared by [`BlendMode::ColorBurn`]
    /// and [`BlendMode::VividLight`]: `src <= 0` burns to black, else
    /// `1 - min(1, (1 - dst) / src)`. Inputs are already clamped.
    fn color_burn(s: f32, d: f32) -> f32 {
        if s <= 0.0 {
            0.0
        } else {
            1.0 - ((1.0 - d) / s).min(1.0)
        }
    }

    /// `true` for the seven non-separable modes (the HSV family plus the
    /// normal-map family), which operate on an RGB triplet rather than one
    /// channel at a time. Separated out so the compositor and the census
    /// test can pin the split without repeating the variant list.
    pub fn is_triplet(self) -> bool {
        matches!(
            self,
            BlendMode::Tint
                | BlendMode::Saturation
                | BlendMode::Color
                | BlendMode::Value
                | BlendMode::NormalCombine
                | BlendMode::NormalDetail
                | BlendMode::NormalInverseDetail
        )
    }

    /// `true` for the three normal-map modes. This is the normal-channel
    /// gate as a routing predicate: the channel-routing layer must only
    /// offer these modes for the normal channel (Painter: "the Normal map
    /// blending modes are only useful for the Normal channel"). The math
    /// in [`BlendMode::blend_rgb`] does not re-check the channel — it runs
    /// on whatever triplet it is handed (see the [`BlendMode::NormalDetail`]
    /// docs for why the gate lives at routing time, not blend time).
    pub fn is_normal_map(self) -> bool {
        matches!(
            self,
            BlendMode::NormalCombine | BlendMode::NormalDetail | BlendMode::NormalInverseDetail
        )
    }

    /// Converts a clamped linear RGB triplet to HSV: hue in degrees
    /// `0.0..<360.0`, saturation and value in `0.0..=1.0`.
    ///
    /// Achromatic inputs (including black) yield hue `0.0` — the red axis —
    /// by convention, since they carry no hue. Callers that need "no hue"
    /// semantics ([`BlendMode::Tint`]/[`BlendMode::Color`]) check the
    /// saturation first and never read this fallback hue.
    fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
        let mx = c[0].max(c[1]).max(c[2]);
        let mn = c[0].min(c[1]).min(c[2]);
        let d = mx - mn;
        let s = if mx <= 0.0 { 0.0 } else { d / mx };
        let mut h = 0.0;
        if d > 0.0 {
            if mx == c[0] {
                h = 60.0 * ((c[1] - c[2]) / d);
            } else if mx == c[1] {
                h = 60.0 * ((c[2] - c[0]) / d + 2.0);
            } else {
                h = 60.0 * ((c[0] - c[1]) / d + 4.0);
            }
            if h < 0.0 {
                h += 360.0;
            }
        }
        [h, s, mx]
    }

    /// Converts an HSV triplet (hue degrees, saturation, value) back to
    /// linear RGB. The `t mod 2` is written without `%` so the WGSL twin
    /// in the compositor stays a line-for-line transliteration.
    fn hsv_to_rgb(c: [f32; 3]) -> [f32; 3] {
        let (h, s, v) = (c[0], c[1], c[2]);
        let chroma = v * s;
        let t = h / 60.0;
        let t_mod_2 = t - (t / 2.0).floor() * 2.0;
        let x = chroma * (1.0 - (t_mod_2 - 1.0).abs());
        let m = v - chroma;
        let (r, g, b) = if h < 60.0 {
            (chroma, x, 0.0)
        } else if h < 120.0 {
            (x, chroma, 0.0)
        } else if h < 180.0 {
            (0.0, chroma, x)
        } else if h < 240.0 {
            (0.0, x, chroma)
        } else if h < 300.0 {
            (x, 0.0, chroma)
        } else {
            // `h >= 300.0`, including the `h == 360.0` wraparound: red.
            (chroma, 0.0, x)
        };
        [r + m, g + m, b + m]
    }

    /// Decodes one tangent-space normal lane-triplet (`n * 0.5 + 0.5`
    /// encoding) to a signed direction. No Y-flip is applied (see the
    /// [`BlendMode::NormalDetail`] docs).
    fn decode_normal(c: [f32; 3]) -> [f32; 3] {
        [2.0 * c[0] - 1.0, 2.0 * c[1] - 1.0, 2.0 * c[2] - 1.0]
    }

    /// Normalizes `n`, falling back to `fallback` when it is degenerate:
    /// a zero-length vector (e.g. a mid-gray texel decodes to `(0, 0, 0)`,
    /// which is not a direction) or a non-finite one normalizes to `NaN`
    /// and would poison the composite, so the base is kept instead.
    fn normalize_or(n: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
        let len_sq = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
        if !len_sq.is_finite() || len_sq <= 1e-12 {
            return fallback;
        }
        let inv = 1.0 / len_sq.sqrt();
        [n[0] * inv, n[1] * inv, n[2] * inv]
    }

    /// Painter's "Normal Map Combine": Whiteout blending (Oat 2007, via
    /// Barré-Brisebois & Hill's "Blending in Detail" survey:
    /// `r = (n1.xy + n2.xy, n1.z·n2.z)`, renormalized). Flat normals are
    /// identities on both sides: detail-flat returns the base, base-flat
    /// returns the detail.
    fn whiteout(base: [f32; 3], detail: [f32; 3]) -> [f32; 3] {
        let n1 = Self::decode_normal(base);
        let n2 = Self::decode_normal(detail);
        let r = [n1[0] + n2[0], n1[1] + n2[1], n1[2] * n2[2]];
        let n = Self::normalize_or(r, n1);
        [0.5 * n[0] + 0.5, 0.5 * n[1] + 0.5, 0.5 * n[2] + 0.5]
    }

    /// Reoriented Normal Mapping (RNM — Barré-Brisebois & Hill, "Blending
    /// in Detail", 2012): rotates the detail normal so it follows the base
    /// surface, via the shortest-arc quaternion reduced to
    /// `r = t·(t·u) − u·t.z` with `t = base + (0,0,1)` and
    /// `u = (−detail.x, −detail.y, detail.z)`, renormalized (the article's
    /// HLSL Appendix form, with its recommended normalize kept — decoded
    /// texels are rarely exactly unit length after quantization). Unity's
    /// contemporary basis-transform variant from the same article is
    /// deliberately NOT used: its basis degenerates away from flat
    /// normals (the article's Figure 8), while the quaternion form does
    /// not. Flat base or flat detail are both identities.
    fn rnm(base: [f32; 3], detail: [f32; 3]) -> [f32; 3] {
        let b = Self::decode_normal(base);
        let d = Self::decode_normal(detail);
        let t = [b[0], b[1], b[2] + 1.0];
        let u = [-d[0], -d[1], d[2]];
        let dot = t[0] * u[0] + t[1] * u[1] + t[2] * u[2];
        let r = [
            t[0] * dot - u[0] * t[2],
            t[1] * dot - u[1] * t[2],
            t[2] * dot - u[2] * t[2],
        ];
        let n = Self::normalize_or(r, b);
        [0.5 * n[0] + 0.5, 0.5 * n[1] + 0.5, 0.5 * n[2] + 0.5]
    }

    /// Blends one linear-space RGB triplet: `src` is the top layer, `dst`
    /// the accumulated base below.
    ///
    /// Separable modes fold the per-channel [`BlendMode::blend_channel`]
    /// primitive lane-by-lane. The seven triplet modes need the whole
    /// pixel: the HSV family converts in the linear working space (no sRGB
    /// round-trip — Painter runs every mode in linear gamma internally,
    /// and converting in the working space keeps the CPU oracle and the
    /// WGSL mirror bit-agreeing), and the normal-map family decodes
    /// tangent-space directions (see [`BlendMode::NormalDetail`]).
    /// Returns `None` exactly for [`BlendMode::Passthrough`].
    pub fn blend_rgb(self, src: [f32; 3], dst: [f32; 3]) -> Option<[f32; 3]> {
        if matches!(self, BlendMode::Passthrough) {
            return None;
        }
        let s = [
            Self::clamp01(src[0]),
            Self::clamp01(src[1]),
            Self::clamp01(src[2]),
        ];
        let d = [
            Self::clamp01(dst[0]),
            Self::clamp01(dst[1]),
            Self::clamp01(dst[2]),
        ];
        let v = match self {
            BlendMode::Tint => {
                let hs = Self::rgb_to_hsv(s);
                if hs[1] <= 0.0 {
                    // Achromatic top carries no hue — the bottom stays.
                    d
                } else {
                    let hd = Self::rgb_to_hsv(d);
                    Self::hsv_to_rgb([hs[0], hd[1], hd[2]])
                }
            }
            BlendMode::Saturation => {
                let hs = Self::rgb_to_hsv(s);
                let hd = Self::rgb_to_hsv(d);
                Self::hsv_to_rgb([hd[0], hs[1], hd[2]])
            }
            BlendMode::Color => {
                let hs = Self::rgb_to_hsv(s);
                if hs[1] <= 0.0 {
                    d
                } else {
                    let hd = Self::rgb_to_hsv(d);
                    Self::hsv_to_rgb([hs[0], hs[1], hd[2]])
                }
            }
            BlendMode::Value => {
                let hs = Self::rgb_to_hsv(s);
                let hd = Self::rgb_to_hsv(d);
                Self::hsv_to_rgb([hd[0], hd[1], hs[2]])
            }
            BlendMode::NormalCombine => Self::whiteout(d, s),
            BlendMode::NormalDetail => Self::rnm(d, s),
            BlendMode::NormalInverseDetail => Self::rnm(s, d),
            // Every remaining variant is a separable per-channel op
            // (Passthrough returned above): fold the scalar primitive.
            _ => [
                self.blend_channel(s[0], d[0])?,
                self.blend_channel(s[1], d[1])?,
                self.blend_channel(s[2], d[2])?,
            ],
        };
        Some([
            Self::clamp01(v[0]),
            Self::clamp01(v[1]),
            Self::clamp01(v[2]),
        ])
    }
}

/// A paint mask attached to a layer.
///
/// The metadata half: name and whether the mask currently affects
/// compositing. The pixel half — one grayscale `u8` plane per painted
/// tile, keyed `(layer id, tile)` at the tile's set size — lives in the
/// app's tile pool (`umber-app`'s `PaintState::layer_masks`, the
/// per-channel pool's neighbor): `umber-core` is the dependency-minimal
/// document model and owns no GPU or tile concepts, so planes cannot live
/// here. An attached mask with no plane yet reads as fully white (a
/// no-op gate) — attaching must never black out the layer. The
/// compositor multiplies the layer's coverage by
/// [`mask_coverage`] per texel while [`LayerMask::gates`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerMask {
    /// Display name, shown in the layer-stack panel next to the layer's own.
    pub name: String,
    /// Whether the mask currently affects compositing.
    pub enabled: bool,
}

impl LayerMask {
    /// Whether this mask gates its layer's compositing right now: present
    /// and enabled. A disabled mask keeps its pixels (paint and save carry
    /// them) but composites exactly as if absent.
    pub fn gates(&self) -> bool {
        self.enabled
    }
}

/// One grayscale mask byte as coverage: `0` = fully gated (the texel
/// contributes nothing), `255` = fully open. The compositor's per-texel
/// gate multiplies the layer's coverage (`src_alpha × opacity`) by this.
pub fn mask_coverage(value: u8) -> f32 {
    f32::from(value) / 255.0
}

/// One entry in a [`LayerStack`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Monotonically-allocated within the owning [`LayerStack`]; stable
    /// across save/load (see project.rs).
    pub id: u64,
    /// Display name, shown in the layer-stack panel.
    pub name: String,
    /// What this layer contains (paint/fill/folder).
    pub kind: LayerKind,
    /// Clamped to `0.0..=1.0` by every mutator on [`LayerStack`].
    pub opacity: f32,
    /// Whether this layer contributes to compositing.
    pub visible: bool,
    /// How this layer's result combines with the layers below it.
    pub blend_mode: BlendMode,
    /// Attached paint mask, if any.
    pub mask: Option<LayerMask>,
    /// The fill source + projection, `Some` exactly for [`LayerKind::Fill`]
    /// layers. `#[serde(default)]` so projects saved before the fill row
    /// landed (no such key) still load — as paint-style layers with no
    /// evaluatable fill.
    #[serde(default)]
    pub fill: Option<FillParams>,
}

impl Layer {
    /// Approximate RAM held by this layer, in bytes: the inline struct
    /// plus heap name bytes plus the fill params' snapshot bytes (if any).
    /// See [`LayerStack::memory_bytes`] for the honesty contract (metadata
    /// only, no GPU tiles yet).
    pub fn memory_bytes(&self) -> usize {
        size_of::<Layer>()
            + self.name.len()
            + self.mask.as_ref().map(|m| m.name.len()).unwrap_or(0)
            + self
                .fill
                .as_ref()
                .map(FillParams::memory_bytes)
                .unwrap_or(0)
    }
}

/// An ordered stack of layers for one texture set.
///
/// Index 0 is the bottom of the stack; the last element is the top.
/// Mutating methods return the *previous* value — an "op handle" callers
/// (notably [`LayerCommand`]) use to build undo without re-querying the
/// stack afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerStack {
    /// Stack order, bottom (index 0) to top.
    pub layers: Vec<Layer>,
    next_id: u64,
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerStack {
    /// An empty stack with id allocation starting at 0.
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            next_id: 0,
        }
    }

    /// Rebuilds a stack from layers loaded from disk plus the id counter
    /// that was in effect when they were saved.
    ///
    /// Takes the saved counter rather than deriving `max(id) + 1` so that
    /// ids stay monotonic across a save/load cycle even if the top layer
    /// was deleted before saving (see project.rs LayerSetOrder).
    pub fn from_parts(layers: Vec<Layer>, saved_next_id: u64) -> Self {
        let min_safe = layers.iter().map(|l| l.id + 1).max().unwrap_or(0);
        Self {
            layers,
            next_id: saved_next_id.max(min_safe),
        }
    }

    /// The id the next [`add_layer`](Self::add_layer) call will allocate.
    pub fn next_layer_id(&self) -> u64 {
        self.next_id
    }

    /// The current stack position of the layer with `id`, if it exists.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Looks up a layer by id.
    pub fn layer(&self, id: u64) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Looks up a layer by id, mutably.
    pub fn layer_mut(&mut self, id: u64) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Pushes a new layer onto the top of the stack, returning its
    /// newly-allocated id.
    pub fn add_layer(&mut self, name: impl Into<String>, kind: LayerKind) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.layers.push(Layer {
            id,
            name: name.into(),
            kind,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            mask: None,
            fill: None,
        });
        id
    }

    /// Re-inserts a layer at the top of the stack under a *specific* id,
    /// without allocating a fresh one.
    ///
    /// Only used by [`LayerCommand::Add`]'s redo path, so that redoing an
    /// add restores the exact same id a later command in the journal (e.g.
    /// a `SetOpacity` on that id) may still refer to. Keeps `next_id`
    /// monotonic even though `id` was not issued by this call.
    fn add_layer_with_id(&mut self, id: u64, name: String, kind: LayerKind) {
        self.layers.push(Layer {
            id,
            name,
            kind,
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            mask: None,
            fill: None,
        });
        self.next_id = self.next_id.max(id + 1);
    }

    /// Removes the layer with `id`, returning its former `(index, Layer)` so
    /// it can be reinserted verbatim (see [`insert_layer`](Self::insert_layer)).
    pub fn remove_layer(&mut self, id: u64) -> Option<(usize, Layer)> {
        let index = self.index_of(id)?;
        Some((index, self.layers.remove(index)))
    }

    /// Re-inserts a previously-removed layer at `index` (clamped to the
    /// current length).
    pub fn insert_layer(&mut self, index: usize, layer: Layer) {
        self.next_id = self.next_id.max(layer.id + 1);
        let index = index.min(self.layers.len());
        self.layers.insert(index, layer);
    }

    /// Moves the layer at `from` to `to` (clamped to the valid range),
    /// returning the index it actually landed at, or `None` if `from` was
    /// out of range.
    pub fn reorder(&mut self, from: usize, to: usize) -> Option<usize> {
        if from >= self.layers.len() {
            return None;
        }
        let layer = self.layers.remove(from);
        let to = to.min(self.layers.len());
        self.layers.insert(to, layer);
        Some(to)
    }

    /// Sets opacity, clamped to `0.0..=1.0`. Non-finite input is ignored
    /// (the layer's opacity is left unchanged). Returns the previous value.
    pub fn set_opacity(&mut self, id: u64, value: f32) -> Option<f32> {
        let layer = self.layer_mut(id)?;
        let old = layer.opacity;
        if value.is_finite() {
            layer.opacity = value.clamp(0.0, 1.0);
        }
        Some(old)
    }

    /// Returns the previous visibility.
    pub fn set_visible(&mut self, id: u64, value: bool) -> Option<bool> {
        let layer = self.layer_mut(id)?;
        let old = layer.visible;
        layer.visible = value;
        Some(old)
    }

    /// Returns the previous blend mode.
    pub fn set_blend_mode(&mut self, id: u64, mode: BlendMode) -> Option<BlendMode> {
        let layer = self.layer_mut(id)?;
        let old = layer.blend_mode;
        layer.blend_mode = mode;
        Some(old)
    }

    /// Sets a layer's fill parameters (source + projection + mask), returning the
    /// previous value (`None` when the layer had no fill params — including
    /// non-fill layers, which accept the write but stay non-fill: the
    /// evaluator only runs `fill` on [`LayerKind::Fill`] layers, so a
    /// stray write is inert data, never a behavior change).
    pub fn set_fill(&mut self, id: u64, params: FillParams) -> Option<Option<FillParams>> {
        let layer = self.layer_mut(id)?;
        Some(layer.fill.replace(params))
    }

    /// Attaches a fresh enabled mask named `name` to the layer, replacing
    /// any existing one. Returns the previous mask (`None` when there was
    /// none) — the undo handle. Unknown ids return `None` and change
    /// nothing.
    ///
    /// The pixels are NOT created here (see [`LayerMask`]: planes live in
    /// the app's mask pool and materialize white on the first mask stroke,
    /// so attaching is a no-op gate until painted).
    pub fn attach_mask(&mut self, id: u64, name: impl Into<String>) -> Option<Option<LayerMask>> {
        let layer = self.layer_mut(id)?;
        Some(layer.mask.replace(LayerMask {
            name: name.into(),
            enabled: true,
        }))
    }

    /// Removes the layer's mask, returning it (`None` when there was none
    /// — including unknown ids, which change nothing).
    ///
    /// The pixels are NOT dropped here either (same split as
    /// [`Self::attach_mask`]): the app prunes the mask pool against live
    /// layers on save, so an undone detach finds its planes intact.
    pub fn detach_mask(&mut self, id: u64) -> Option<LayerMask> {
        self.layer_mut(id)?.mask.take()
    }

    /// Enables or disables the layer's mask, returning the previous flag.
    /// `None` when the layer is unknown or has no mask (nothing changes).
    pub fn set_mask_enabled(&mut self, id: u64, enabled: bool) -> Option<bool> {
        let mask = self.layer_mut(id)?.mask.as_mut()?;
        let old = mask.enabled;
        mask.enabled = enabled;
        Some(old)
    }

    /// Pushes a new instance layer onto the top of the stack, rendering
    /// `source_id`'s composited pixels. The new layer's own
    /// `opacity`/`blend_mode`/`mask` are its per-instance overlays.
    ///
    /// # Errors
    ///
    /// [`InstanceError::UnknownSource`] when `source_id` names no layer.
    /// A fresh instance cannot cycle (its id is new to the graph), so no
    /// [`InstanceError::Cycle`] arises here — cycles are rejected by
    /// [`Self::set_instance_source`].
    pub fn add_instance(
        &mut self,
        name: impl Into<String>,
        source_id: u64,
    ) -> Result<u64, InstanceError> {
        if self.layer(source_id).is_none() {
            return Err(InstanceError::UnknownSource {
                instance: self.next_id,
                source_id,
            });
        }
        let id = self.next_id;
        self.next_id += 1;
        self.layers.push(Layer {
            id,
            name: name.into(),
            kind: LayerKind::Instance { source_id },
            opacity: 1.0,
            visible: true,
            blend_mode: BlendMode::Normal,
            mask: None,
            fill: None,
        });
        Ok(id)
    }

    /// The source of an instance layer, if `id` names one (`None` for
    /// non-instance layers or unknown ids).
    pub fn instance_source(&self, id: u64) -> Option<u64> {
        match self.layer(id) {
            Some(Layer {
                kind: LayerKind::Instance { source_id },
                ..
            }) => Some(*source_id),
            _ => None,
        }
    }

    /// Whether `id` names an instance layer.
    pub fn is_instance(&self, id: u64) -> bool {
        self.instance_source(id).is_some()
    }

    /// Retargets an instance layer onto a new source, rejecting the
    /// would-be cycle at attach time. Returns the previous source.
    ///
    /// # Errors
    ///
    /// [`InstanceError::UnknownLayer`] when `id` names no layer;
    /// [`InstanceError::NotAnInstance`] when it names a non-instance;
    /// [`InstanceError::UnknownSource`] when `source_id` names no layer;
    /// [`InstanceError::Cycle`] when the retarget would close a cycle
    /// (direct `A→B→A` or indirect — the DFS walks the transitive chain).
    pub fn set_instance_source(&mut self, id: u64, source_id: u64) -> Result<u64, InstanceError> {
        let layer = self.layer(id).ok_or(InstanceError::UnknownLayer(id))?;
        let old = match layer {
            Layer {
                kind: LayerKind::Instance { source_id },
                ..
            } => *source_id,
            _ => return Err(InstanceError::NotAnInstance(id)),
        };
        if self.layer(source_id).is_none() {
            return Err(InstanceError::UnknownSource {
                instance: id,
                source_id,
            });
        }
        if let Some(participants) = self.would_cycle(id, source_id) {
            return Err(InstanceError::Cycle(participants));
        }
        if let Some(layer) = self.layer_mut(id) {
            layer.kind = LayerKind::Instance { source_id };
        }
        Ok(old)
    }

    /// Whether pointing `instance_id` at `new_source` would close a cycle:
    /// `Some(participants)` (starting at `instance_id`) when yes, `None`
    /// when safe. A small iterative DFS over the instance edges — the
    /// structural mirror of `umber-graph`'s Kahn check (see
    /// [`InstanceError`] for why it lives here).
    fn would_cycle(&self, instance_id: u64, new_source: u64) -> Option<Vec<u64>> {
        // Follow source links transitively from the candidate source: if
        // the walk reaches the instance being retargeted, the edge closes
        // a cycle. Non-instance layers terminate the walk (they have no
        // outgoing edge).
        let mut chain = vec![instance_id];
        let mut cursor = new_source;
        // Bounded by the layer count: a longer walk must repeat a layer,
        // which is itself a cycle through the new edge.
        for _ in 0..=self.layers.len() {
            if cursor == instance_id {
                chain.push(cursor);
                return Some(chain);
            }
            chain.push(cursor);
            match self.layer(cursor) {
                Some(Layer {
                    kind: LayerKind::Instance { source_id },
                    ..
                }) => cursor = *source_id,
                _ => return None,
            }
        }
        chain.push(cursor);
        Some(chain)
    }

    /// The transitive source chain of `id`, nearest first
    /// (`A→B→C` yields `[B, C]`). Stops at the first non-instance layer
    /// or missing link; empty for non-instances.
    pub fn instance_chain(&self, id: u64) -> Vec<u64> {
        let mut chain = Vec::new();
        let mut cursor = match self.instance_source(id) {
            Some(s) => s,
            None => return chain,
        };
        for _ in 0..=self.layers.len() {
            chain.push(cursor);
            match self.layer(cursor) {
                Some(Layer {
                    kind: LayerKind::Instance { source_id },
                    ..
                }) => cursor = *source_id,
                _ => break,
            }
        }
        chain
    }

    /// Evaluation order for the instance subgraph: sources before their
    /// instances, transitively (A instances B instances C → `C` before
    /// `B` before `A`). Non-instance layers keep stack order; instance
    /// layers sort after their transitive sources (stable otherwise).
    ///
    /// # Errors
    ///
    /// [`InstanceError::Cycle`] if the stored graph already cycles
    /// (attach-time rejection in [`Self::set_instance_source`] should make
    /// this unreachable — it fires for hand-built or legacy files), or
    /// [`InstanceError::UnknownSource`] on a dangling source link.
    pub fn instance_eval_order(&self) -> Result<Vec<u64>, InstanceError> {
        // Validate links first (dangling sources are loud, not skipped).
        for layer in &self.layers {
            if let LayerKind::Instance { source_id } = &layer.kind {
                if self.layer(*source_id).is_none() {
                    return Err(InstanceError::UnknownSource {
                        instance: layer.id,
                        source_id: *source_id,
                    });
                }
            }
        }
        // Kahn sweep over instance edges (source → instance), restricted
        // to instance layers; non-instances emit in stack order first.
        use std::collections::HashMap;
        let instances: Vec<u64> = self
            .layers
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Instance { .. }))
            .map(|l| l.id)
            .collect();
        let in_set: std::collections::HashSet<u64> = instances.iter().copied().collect();
        let mut indegree: HashMap<u64, usize> = instances.iter().map(|&id| (id, 0)).collect();
        let mut outs: HashMap<u64, Vec<u64>> = HashMap::new();
        for &id in &instances {
            let Some(src) = self.instance_source(id) else {
                // Unreachable (validated above) but LOUD, never a panic:
                // a legacy file with an instance kind but no source key
                // fails the sweep, not the app.
                return Err(InstanceError::UnknownSource {
                    instance: id,
                    source_id: id,
                });
            };
            if in_set.contains(&src) {
                // `id` keys `indegree` by construction (the loop's
                // own collect above); the get cannot miss.
                if let Some(d) = indegree.get_mut(&id) {
                    *d += 1;
                }
                outs.entry(src).or_default().push(id);
            }
        }
        let mut queue: Vec<u64> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        queue.sort_unstable();
        let mut order = Vec::with_capacity(instances.len());
        while let Some(&id) = queue.first() {
            queue.remove(0);
            order.push(id);
            if let Some(next) = outs.get(&id) {
                let mut next = next.clone();
                next.sort_unstable();
                for to in next {
                    // Only in-set instances land in `outs` (the entry
                    // above is gated on in_set), so `to` keys `indegree`
                    // by construction; a miss would be a logic bug, and
                    // skipping it keeps the sweep total.
                    let Some(d) = indegree.get_mut(&to) else {
                        continue;
                    };
                    *d -= 1;
                    if *d == 0 {
                        queue.push(to);
                        queue.sort_unstable();
                    }
                }
            }
        }
        if order.len() != instances.len() {
            let mut cyclic: Vec<u64> = indegree
                .iter()
                .filter(|(_, &d)| d > 0)
                .map(|(&id, _)| id)
                .collect();
            cyclic.sort_unstable();
            return Err(InstanceError::Cycle(cyclic));
        }
        // Full stack order: plain layers in stack order, then instances in
        // dependency order (sources — plain or ordered instances — always
        // precede their instances).
        let mut full: Vec<u64> = self
            .layers
            .iter()
            .filter(|l| !matches!(l.kind, LayerKind::Instance { .. }))
            .map(|l| l.id)
            .collect();
        full.extend(order);
        Ok(full)
    }

    /// Rejects direct painting on an instance layer: instances carry no
    /// pixels, so a paint attempt is a typed error naming the source to
    /// paint instead. `Ok(())` for paintable (non-instance) layers.
    ///
    /// # Errors
    ///
    /// [`InstanceError::UnknownLayer`] for unknown ids;
    /// [`InstanceError::PaintOnInstance`] for instances (names the source).
    pub fn check_paintable(&self, id: u64) -> Result<(), InstanceError> {
        let layer = self.layer(id).ok_or(InstanceError::UnknownLayer(id))?;
        match &layer.kind {
            LayerKind::Instance { source_id } => Err(InstanceError::PaintOnInstance {
                instance: id,
                source_id: *source_id,
            }),
            _ => Ok(()),
        }
    }

    /// Approximate RAM held by the whole stack, in bytes: per-layer
    /// [`Layer::memory_bytes`] plus the `Vec` buffer itself.
    ///
    /// An honest approximation, not a budget: layer metadata only (names,
    /// struct fields). GPU-tile bytes are not modeled here — they arrive
    /// with the tile-pool slice, at which point this sum grows a tile term.
    /// The undo soak test samples this after each push to assert the growth
    /// curve's shape (sub-linear) plus an empirical byte ceiling.
    pub fn memory_bytes(&self) -> usize {
        self.layers.iter().map(Layer::memory_bytes).sum::<usize>()
            + self.layers.capacity() * size_of::<Layer>()
    }
}

/// A reversible [`LayerStack`] edit, for use with `UndoStack<LayerCommand>`.
///
/// One enum rather than a family of small command structs, because
/// [`crate::undo::UndoStack`] is monomorphic over a single command type —
/// callers need one type that covers every op to run a single journal over
/// a layer stack.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerCommand {
    /// Adds a new layer to the top of the stack.
    Add {
        /// Name of the layer to create.
        name: String,
        /// Kind of the layer to create.
        kind: LayerKind,
        /// `None` until the first `apply`; afterwards holds the allocated
        /// id so redo can restore the exact same one.
        id: Option<u64>,
    },
    /// Removes an existing layer by id.
    Remove {
        /// Id of the layer to remove.
        id: u64,
        /// Captured `(index, Layer)` after `apply`, for `revert`.
        removed: Option<(usize, Layer)>,
    },
    /// Moves a layer from one stack position to another.
    Reorder {
        /// Source index.
        from: usize,
        /// Requested destination index (clamped on apply).
        to: usize,
        /// The index the layer actually landed at after `apply`.
        applied_to: Option<usize>,
    },
    /// Sets a layer's opacity.
    SetOpacity {
        /// Id of the layer to update.
        id: u64,
        /// New opacity value.
        new: f32,
        /// Previous opacity, captured by `apply` for `revert`.
        old: Option<f32>,
    },
    /// Sets a layer's visibility.
    SetVisible {
        /// Id of the layer to update.
        id: u64,
        /// New visibility value.
        new: bool,
        /// Previous visibility, captured by `apply` for `revert`.
        old: Option<bool>,
    },
    /// Sets a layer's blend mode.
    SetBlendMode {
        /// Id of the layer to update.
        id: u64,
        /// New blend mode.
        new: BlendMode,
        /// Previous blend mode, captured by `apply` for `revert`.
        old: Option<BlendMode>,
    },
    /// Sets a layer's fill parameters (source + projection + mask).
    SetFill {
        /// Id of the layer to update.
        id: u64,
        /// New fill parameters.
        new: FillParams,
        /// Previous fill parameters (`None` when the layer had none),
        /// captured by `apply` for `revert`.
        old: Option<Option<FillParams>>,
    },
    /// Retargets an instance layer onto a new source (§3:41). Cycle-safe:
    /// the apply reuses [`LayerStack::set_instance_source`], so a redo
    /// that would now close a cycle is ignored rather than applied.
    SetInstanceSource {
        /// Id of the instance layer to retarget.
        id: u64,
        /// New source layer id.
        new: u64,
        /// Previous source, captured by `apply` for `revert`.
        old: Option<u64>,
    },
    /// Attaches a fresh enabled mask to a layer (see
    /// [`LayerStack::attach_mask`]).
    AttachMask {
        /// Id of the layer gaining the mask.
        id: u64,
        /// Name of the new mask.
        name: String,
        /// Previous mask, captured by `apply` for `revert`.
        old: Option<Option<LayerMask>>,
    },
    /// Removes a layer's mask (see [`LayerStack::detach_mask`]).
    DetachMask {
        /// Id of the layer losing the mask.
        id: u64,
        /// Removed mask, captured by `apply` for `revert`.
        removed: Option<LayerMask>,
    },
    /// Enables or disables a layer's mask (see
    /// [`LayerStack::set_mask_enabled`]).
    SetMaskEnabled {
        /// Id of the layer whose mask flips.
        id: u64,
        /// New flag value.
        new: bool,
        /// Previous flag, captured by `apply` for `revert`.
        old: Option<bool>,
    },
}

impl LayerCommand {
    /// Builds an [`Add`](LayerCommand::Add) command for a new layer.
    pub fn add(name: impl Into<String>, kind: LayerKind) -> Self {
        LayerCommand::Add {
            name: name.into(),
            kind,
            id: None,
        }
    }

    /// Builds a [`Remove`](LayerCommand::Remove) command for an existing layer.
    pub fn remove(id: u64) -> Self {
        LayerCommand::Remove { id, removed: None }
    }

    /// Builds a [`Reorder`](LayerCommand::Reorder) command.
    pub fn reorder(from: usize, to: usize) -> Self {
        LayerCommand::Reorder {
            from,
            to,
            applied_to: None,
        }
    }

    /// Builds a [`SetOpacity`](LayerCommand::SetOpacity) command.
    pub fn set_opacity(id: u64, value: f32) -> Self {
        LayerCommand::SetOpacity {
            id,
            new: value,
            old: None,
        }
    }

    /// Builds a [`SetVisible`](LayerCommand::SetVisible) command.
    pub fn set_visible(id: u64, value: bool) -> Self {
        LayerCommand::SetVisible {
            id,
            new: value,
            old: None,
        }
    }

    /// Builds a [`SetBlendMode`](LayerCommand::SetBlendMode) command.
    pub fn set_blend_mode(id: u64, mode: BlendMode) -> Self {
        LayerCommand::SetBlendMode {
            id,
            new: mode,
            old: None,
        }
    }

    /// Builds a [`SetFill`](LayerCommand::SetFill) command.
    pub fn set_fill(id: u64, params: FillParams) -> Self {
        LayerCommand::SetFill {
            id,
            new: params,
            old: None,
        }
    }

    /// Builds a [`SetInstanceSource`](LayerCommand::SetInstanceSource) command.
    pub fn set_instance_source(id: u64, source: u64) -> Self {
        LayerCommand::SetInstanceSource {
            id,
            new: source,
            old: None,
        }
    }

    /// Builds an [`AttachMask`](LayerCommand::AttachMask) command.
    pub fn attach_mask(id: u64, name: impl Into<String>) -> Self {
        LayerCommand::AttachMask {
            id,
            name: name.into(),
            old: None,
        }
    }

    /// Builds a [`DetachMask`](LayerCommand::DetachMask) command.
    pub fn detach_mask(id: u64) -> Self {
        LayerCommand::DetachMask { id, removed: None }
    }

    /// Builds a [`SetMaskEnabled`](LayerCommand::SetMaskEnabled) command.
    pub fn set_mask_enabled(id: u64, enabled: bool) -> Self {
        LayerCommand::SetMaskEnabled {
            id,
            new: enabled,
            old: None,
        }
    }
}

impl Command for LayerCommand {
    type Doc = LayerStack;

    fn apply(&mut self, doc: &mut LayerStack) {
        match self {
            LayerCommand::Add { name, kind, id } => match id {
                None => *id = Some(doc.add_layer(name.clone(), kind.clone())),
                Some(existing) => doc.add_layer_with_id(*existing, name.clone(), kind.clone()),
            },
            LayerCommand::Remove { id, removed } => *removed = doc.remove_layer(*id),
            LayerCommand::Reorder {
                from,
                to,
                applied_to,
            } => {
                *applied_to = doc.reorder(*from, *to);
            }
            LayerCommand::SetOpacity { id, new, old } => *old = doc.set_opacity(*id, *new),
            LayerCommand::SetVisible { id, new, old } => *old = doc.set_visible(*id, *new),
            LayerCommand::SetBlendMode { id, new, old } => *old = doc.set_blend_mode(*id, *new),
            LayerCommand::SetFill { id, new, old } => *old = doc.set_fill(*id, new.clone()),
            LayerCommand::SetInstanceSource { id, new, old } => {
                *old = doc.set_instance_source(*id, *new).ok();
            }
            LayerCommand::AttachMask { id, name, old } => {
                *old = doc.attach_mask(*id, name.clone());
            }
            LayerCommand::DetachMask { id, removed } => {
                *removed = doc.detach_mask(*id);
            }
            LayerCommand::SetMaskEnabled { id, new, old } => {
                *old = doc.set_mask_enabled(*id, *new);
            }
        }
    }

    fn revert(&mut self, doc: &mut LayerStack) {
        match self {
            // Deliberately does not take() the id: redo re-applies Add via
            // the Some(existing) arm above, which needs it to restore the
            // same id rather than allocating a new one.
            LayerCommand::Add { id, .. } => {
                if let Some(id) = id {
                    doc.remove_layer(*id);
                }
            }
            LayerCommand::Remove { removed, .. } => {
                if let Some((index, layer)) = removed.take() {
                    doc.insert_layer(index, layer);
                }
            }
            LayerCommand::Reorder {
                from, applied_to, ..
            } => {
                if let Some(to) = applied_to.take() {
                    doc.reorder(to, *from);
                }
            }
            LayerCommand::SetOpacity { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_opacity(*id, value);
                }
            }
            LayerCommand::SetVisible { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_visible(*id, value);
                }
            }
            LayerCommand::SetBlendMode { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_blend_mode(*id, value);
                }
            }
            LayerCommand::SetFill { id, old, .. } => {
                if let Some(previous) = old.take() {
                    match previous {
                        Some(params) => {
                            doc.set_fill(*id, params);
                        }
                        // The layer had no fill params before: undo removes
                        // what the apply set (there is no "clear fill"
                        // mutator — `None`-before means absent-before).
                        None => {
                            if let Some(layer) = doc.layer_mut(*id) {
                                layer.fill = None;
                            }
                        }
                    }
                }
            }
            LayerCommand::SetInstanceSource { id, old, .. } => {
                if let Some(previous) = old.take() {
                    // Undo of a retarget restores the old source; like the
                    // apply, it goes through the cycle guard (a redo/undo
                    // pair that now cycles is ignored, never forced).
                    let _ = doc.set_instance_source(*id, previous);
                }
            }
            LayerCommand::AttachMask { id, old, .. } => {
                if let Some(previous) = old.take() {
                    match previous {
                        Some(mask) => {
                            if let Some(layer) = doc.layer_mut(*id) {
                                layer.mask = Some(mask);
                            }
                        }
                        // No mask before: undo removes what the apply
                        // attached (there is no "clear mask" mutator —
                        // `None`-before means absent-before, the `SetFill`
                        // pattern).
                        None => {
                            doc.detach_mask(*id);
                        }
                    }
                }
            }
            LayerCommand::DetachMask { id, removed, .. } => {
                if let Some(mask) = removed.take() {
                    if let Some(layer) = doc.layer_mut(*id) {
                        layer.mask = Some(mask);
                    }
                }
            }
            LayerCommand::SetMaskEnabled { id, old, .. } => {
                if let Some(value) = old.take() {
                    doc.set_mask_enabled(*id, value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::undo::UndoStack;

    fn ids(stack: &LayerStack) -> Vec<u64> {
        stack.layers.iter().map(|l| l.id).collect()
    }

    #[test]
    fn add_and_remove_round_trip_index_and_id() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("Base", LayerKind::Paint);
        let b = stack.add_layer("Detail", LayerKind::Paint);
        assert_eq!(stack.layers.len(), 2);
        assert_eq!(stack.layers[0].id, a);
        assert_eq!(stack.layers[1].id, b);

        let (index, removed) = stack.remove_layer(a).expect("layer a exists");
        assert_eq!(index, 0);
        assert_eq!(removed.id, a);
        assert_eq!(stack.layers.len(), 1);

        stack.insert_layer(index, removed);
        assert_eq!(stack.layers[0].id, a);
    }

    #[test]
    fn reorder_moves_layer_and_clamps_target() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_layer("B", LayerKind::Paint);
        let c = stack.add_layer("C", LayerKind::Paint);
        assert_eq!(ids(&stack), vec![a, b, c]);

        let landed = stack.reorder(0, 2).unwrap();
        assert_eq!(landed, 2);
        assert_eq!(ids(&stack), vec![b, c, a]);

        let landed = stack.reorder(0, 999).unwrap();
        assert_eq!(landed, 2); // clamped to the end
        assert_eq!(ids(&stack), vec![c, a, b]);

        assert_eq!(stack.reorder(99, 0), None); // out-of-range source
    }

    #[test]
    fn set_opacity_clamps_and_ignores_non_finite() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);

        assert_eq!(stack.set_opacity(a, 0.5), Some(1.0));
        assert_eq!(stack.layer(a).unwrap().opacity, 0.5);

        stack.set_opacity(a, 5.0);
        assert_eq!(stack.layer(a).unwrap().opacity, 1.0);

        stack.set_opacity(a, -5.0);
        assert_eq!(stack.layer(a).unwrap().opacity, 0.0);

        stack.set_opacity(a, 0.75);
        stack.set_opacity(a, f32::NAN);
        assert_eq!(stack.layer(a).unwrap().opacity, 0.75); // NaN ignored, not stored
    }

    #[test]
    fn layer_command_interleaved_undo_redo_matches_direct_mutation() {
        let mut stack = LayerStack::new();
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);

        let snapshot_empty = stack.layers.clone();

        undo.push(LayerCommand::add("Base", LayerKind::Paint), &mut stack);
        let id = stack.layers[0].id;
        let snapshot_after_add = stack.layers.clone();

        undo.push(LayerCommand::set_opacity(id, 0.5), &mut stack);
        let snapshot_after_opacity = stack.layers.clone();
        assert_eq!(stack.layer(id).unwrap().opacity, 0.5);

        undo.push(LayerCommand::set_visible(id, false), &mut stack);
        let snapshot_after_visible = stack.layers.clone();
        assert!(!stack.layer(id).unwrap().visible);

        // Undo back through each intermediate state.
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_opacity);
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_add);
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, snapshot_empty);
        assert!(!undo.undo(&mut stack));

        // Redo forward through each intermediate state, re-using the same id.
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_add);
        assert_eq!(stack.layers[0].id, id);

        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_opacity);

        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layers, snapshot_after_visible);
        assert!(!undo.can_redo());
    }

    #[test]
    fn remove_command_undo_restores_layer_at_original_index() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_layer("B", LayerKind::Paint);
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);

        let before = stack.layers.clone();
        undo.push(LayerCommand::remove(a), &mut stack);
        assert_eq!(ids(&stack), vec![b]);

        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layers, before);
    }

    #[test]
    fn set_fill_round_trips_through_undo_and_json() {
        use umber_mesh::{FillProjection, FillSource};
        let mut stack = LayerStack::new();
        let id = stack.add_layer("Tint", LayerKind::Fill);
        assert_eq!(stack.layer(id).unwrap().fill, None);

        let red = FillParams {
            source: FillSource::Solid {
                color: [1.0, 0.0, 0.0, 1.0],
            },
            projection: FillProjection::Uv,
            mask: None,
        };
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);
        undo.push(LayerCommand::set_fill(id, red.clone()), &mut stack);
        assert_eq!(stack.layer(id).unwrap().fill, Some(red.clone()));

        let planar = FillParams {
            source: red.source.clone(),
            projection: FillProjection::Planar {
                axis: umber_mesh::ProjAxis::X,
            },
            mask: None,
        };
        undo.push(LayerCommand::set_fill(id, planar.clone()), &mut stack);
        assert_eq!(stack.layer(id).unwrap().fill, Some(planar.clone()));

        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().fill, Some(red.clone()));
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().fill, None);
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().fill, Some(red.clone()));

        // JSON round-trip preserves the params …
        let json = serde_json::to_string(stack.layer(id).unwrap()).unwrap();
        let back: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fill, Some(red.clone()));
        // … and pre-fill saves (no `fill` key) still load, as fill-less.
        let legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut legacy = legacy.as_object().unwrap().clone();
        legacy.remove("fill");
        let back: Layer = serde_json::from_value(serde_json::Value::Object(legacy)).unwrap();
        assert_eq!(back.fill, None);
        assert_eq!(back.name, "Tint");
        // … and pre-polygon saves (a `fill` without `mask`) still load,
        // as unmasked fills.
        let mut unmasked = serde_json::to_value(stack.layer(id).unwrap().fill.clone()).unwrap();
        unmasked.as_object_mut().unwrap().remove("mask");
        let back: FillParams = serde_json::from_value(unmasked).unwrap();
        assert_eq!(back.mask, None);
        assert_eq!(back, red);
    }

    fn blend(mode: BlendMode, src: f32, dst: f32) -> f32 {
        mode.blend_channel(src, dst)
            .unwrap_or_else(|| panic!("{mode:?} should be a separable pixel op"))
    }

    #[test]
    fn mask_coverage_pins_endpoints_and_midpoint() {
        // Exact: 0 -> 0.0, 255 -> 1.0, 128 -> 128/255 (the compositor's
        // half-mask golden depends on this exact fraction).
        assert_eq!(mask_coverage(0), 0.0);
        assert_eq!(mask_coverage(255), 1.0);
        assert_eq!(mask_coverage(128), 128.0 / 255.0);
        // Can-fail: integer division (`v / 255`) would pin every partial
        // mask to 0.0 and gate every masked layer shut.
        assert!(
            mask_coverage(1) > 0.0,
            "a 1/255 mask must gate almost-shut, not fully shut"
        );
    }

    #[test]
    fn mask_gates_reports_the_enabled_flag() {
        let on = LayerMask {
            name: "m".to_string(),
            enabled: true,
        };
        let off = LayerMask {
            name: "m".to_string(),
            enabled: false,
        };
        assert!(on.gates());
        assert!(!off.gates());
    }

    #[test]
    fn mask_attach_detach_toggle_round_trip_through_undo_and_json() {
        let mut stack = LayerStack::new();
        let id = stack.add_layer("Base", LayerKind::Paint);
        assert_eq!(stack.layer(id).unwrap().mask, None);

        // Attach replaces and reports the previous mask.
        assert_eq!(stack.attach_mask(id, "m1"), Some(None));
        let first = stack.layer(id).unwrap().mask.clone().unwrap();
        assert_eq!(first.name, "m1");
        assert!(first.enabled);
        assert!(first.gates());
        assert_eq!(
            stack.attach_mask(id, "m2"),
            Some(Some(first.clone())),
            "re-attaching must report the replaced mask"
        );
        assert_eq!(stack.layer(id).unwrap().mask.as_ref().unwrap().name, "m2");

        // Toggle reports the previous flag; unknown ids and mask-less
        // layers change nothing and report None.
        assert_eq!(stack.set_mask_enabled(id, false), Some(true));
        assert!(!stack.layer(id).unwrap().mask.as_ref().unwrap().gates());
        assert_eq!(stack.set_mask_enabled(id, false), Some(false));
        let bare = stack.add_layer("Bare", LayerKind::Paint);
        assert_eq!(stack.set_mask_enabled(bare, false), None);
        assert_eq!(stack.attach_mask(999, "x"), None);
        assert_eq!(stack.detach_mask(999), None);
        assert_eq!(stack.detach_mask(bare), None);

        // Detach returns the mask verbatim.
        let detached = stack.detach_mask(id).expect("mask present");
        assert_eq!(detached.name, "m2");
        assert!(!detached.enabled);
        assert_eq!(stack.layer(id).unwrap().mask, None);

        // Undo journal round-trip across all three commands.
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);
        undo.push(LayerCommand::attach_mask(id, "u"), &mut stack);
        assert_eq!(stack.layer(id).unwrap().mask.as_ref().unwrap().name, "u");
        undo.push(LayerCommand::set_mask_enabled(id, false), &mut stack);
        assert!(!stack.layer(id).unwrap().mask.as_ref().unwrap().enabled);
        undo.push(LayerCommand::detach_mask(id), &mut stack);
        assert_eq!(stack.layer(id).unwrap().mask, None);
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().mask.as_ref().unwrap().name, "u");
        assert!(undo.undo(&mut stack));
        assert!(stack.layer(id).unwrap().mask.as_ref().unwrap().enabled);
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().mask, None);
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.layer(id).unwrap().mask.as_ref().unwrap().name, "u");

        // JSON round-trip preserves the mask; pre-mask saves (no `mask`
        // key) still load as mask-less layers.
        let json = serde_json::to_string(stack.layer(id).unwrap()).unwrap();
        let back: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mask.as_ref().unwrap().name, "u");
        let legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut legacy = legacy.as_object().unwrap().clone();
        legacy.remove("mask");
        let back: Layer = serde_json::from_value(serde_json::Value::Object(legacy)).unwrap();
        assert_eq!(back.mask, None);
    }

    #[test]
    fn color_dodge_pins_known_value_and_guards_white_source() {
        // Known value: base 0.3 dodged by 0.5 -> 0.3 / 0.5 = 0.6.
        let v = blend(BlendMode::ColorDodge, 0.5, 0.3);
        assert!(
            (v - 0.6).abs() < 1e-6,
            "color dodge src=0.5 dst=0.3 should be 0.6, got {v}"
        );
        // Can-fail: a naive `dst / (1 - src)` divides by zero here
        // (0/0 = NaN, which the output clamp maps to 0). White blend
        // must dodge to white regardless of the base.
        assert_eq!(
            blend(BlendMode::ColorDodge, 1.0, 0.0),
            1.0,
            "color dodge by white over black must be white, not NaN-black"
        );
        assert_eq!(
            blend(BlendMode::ColorDodge, 1.0, 0.4),
            1.0,
            "color dodge by white must saturate to white"
        );
    }

    #[test]
    fn color_burn_pins_known_value_and_guards_black_source() {
        // Known value: base 0.7 burned by 0.5 -> 1 - 0.3/0.5 = 0.4.
        let v = blend(BlendMode::ColorBurn, 0.5, 0.7);
        assert!(
            (v - 0.4).abs() < 1e-6,
            "color burn src=0.5 dst=0.7 should be 0.4, got {v}"
        );
        // Can-fail: a naive `1 - (1 - dst) / src` divides by zero here.
        // Black blend must burn to black regardless of the base.
        assert_eq!(
            blend(BlendMode::ColorBurn, 0.0, 1.0),
            0.0,
            "color burn by black over white must be black"
        );
        assert_eq!(
            blend(BlendMode::ColorBurn, 0.0, 0.8),
            0.0,
            "color burn by black must stay black"
        );
    }

    #[test]
    fn linear_burn_clamps_and_differs_from_subtract() {
        // Can-fail: LinearBurn is `src + dst - 1`, NOT Subtract's
        // `dst - src`. src=0.6, dst=0.7 -> 0.3 here, 0.1 under Subtract.
        let v = blend(BlendMode::LinearBurn, 0.6, 0.7);
        assert!(
            (v - 0.3).abs() < 1e-6,
            "linear burn src=0.6 dst=0.7 should be 0.3 (not Subtract's 0.1), got {v}"
        );
        // Can-fail: the sum-minus-one goes negative and must clamp at
        // black rather than leaking a negative channel value.
        assert_eq!(
            blend(BlendMode::LinearBurn, 0.1, 0.2),
            0.0,
            "linear burn 0.1/0.2 sums under 1 and must clamp to black"
        );
        assert_eq!(
            blend(BlendMode::LinearBurn, 1.0, 1.0),
            1.0,
            "linear burn white/white stays white"
        );
    }

    #[test]
    fn add_is_linear_dodge_census_pin() {
        // Painter's vocabulary item 10 is literally "Linear Dodge (Add)":
        // one mode, two names. Pin `Add` to `min(1, src + dst)` so the
        // core-12's `linear-dodge` and `add` slots stay covered by this
        // single variant rather than a duplicate alias variant.
        for (s, d) in [(0.0, 0.0), (0.3, 0.4), (0.6, 0.7), (1.0, 1.0)] {
            assert_eq!(
                blend(BlendMode::Add, s, d),
                (s + d).min(1.0),
                "Add({s}, {d}) must equal linear-dodge min(1, src + dst)"
            );
        }
    }

    fn blend3(mode: BlendMode, src: [f32; 3], dst: [f32; 3]) -> [f32; 3] {
        mode.blend_rgb(src, dst)
            .unwrap_or_else(|| panic!("{mode:?} should be a triplet-capable pixel op"))
    }

    #[test]
    fn divide_pins_known_value_and_guards_zero_src() {
        // Known value: base 0.25 divided by 0.5 -> 0.5 (exact binary).
        assert_eq!(
            blend(BlendMode::Divide, 0.5, 0.25),
            0.5,
            "divide src=0.5 dst=0.25 should be 0.5"
        );
        // Can-fail: a naive `dst / src` divides by zero here (0.7/0 = +inf,
        // 0/0 = NaN). Zero blend must divide to white regardless of base.
        assert_eq!(
            blend(BlendMode::Divide, 0.0, 0.7),
            1.0,
            "divide by black must be white, not inf"
        );
        assert_eq!(
            blend(BlendMode::Divide, 0.0, 0.0),
            1.0,
            "divide 0/0 must be white, not NaN-black"
        );
        // Over-unity quotients clamp at white (the burned-out look).
        assert_eq!(
            blend(BlendMode::Divide, 0.25, 1.0),
            1.0,
            "divide 1.0/0.25 saturates to white"
        );
    }

    #[test]
    fn inverse_divide_pins_value_guards_and_mirrors_divide() {
        // Known value: 0.25 over base 0.5 -> 0.5 (exact binary).
        assert_eq!(
            blend(BlendMode::InverseDivide, 0.25, 0.5),
            0.5,
            "inverse divide src=0.25 dst=0.5 should be 0.5"
        );
        // Zero base guards to white (the divisor is the bottom here).
        assert_eq!(
            blend(BlendMode::InverseDivide, 0.7, 0.0),
            1.0,
            "inverse divide over black must be white"
        );
        // Role exchange with Divide: identical math on swapped inputs, so
        // strictly-positive pairs must agree bit-exactly (same f32 ops).
        for (s, d) in [(0.3, 0.8), (0.9, 0.1), (0.6, 0.6)] {
            assert_eq!(
                blend(BlendMode::InverseDivide, s, d),
                blend(BlendMode::Divide, d, s),
                "InverseDivide({s}, {d}) must equal Divide({d}, {s})"
            );
        }
    }

    #[test]
    fn inverse_subtract_pins_value_clamp_and_mirrors_subtract() {
        // Known value: 0.75 minus base 0.25 -> 0.5 (exact binary).
        assert_eq!(
            blend(BlendMode::InverseSubtract, 0.75, 0.25),
            0.5,
            "inverse subtract src=0.75 dst=0.25 should be 0.5"
        );
        // Can-fail: blend-under-base goes negative and must clamp at black.
        assert_eq!(
            blend(BlendMode::InverseSubtract, 0.25, 0.75),
            0.0,
            "inverse subtract 0.25/0.75 must clamp to black"
        );
        // Role exchange with Subtract: same f32 ops on swapped inputs.
        for (s, d) in [(0.3, 0.8), (0.9, 0.1), (0.0, 1.0)] {
            assert_eq!(
                blend(BlendMode::InverseSubtract, s, d),
                blend(BlendMode::Subtract, d, s),
                "InverseSubtract({s}, {d}) must equal Subtract({d}, {s})"
            );
        }
    }

    #[test]
    fn exclusion_pins_value_and_endpoints() {
        // Known value: 0.5 ^ 0.25 = 0.5 + 0.25 - 2*0.5*0.25 = 0.5 (exact).
        assert_eq!(
            blend(BlendMode::Exclusion, 0.5, 0.25),
            0.5,
            "exclusion src=0.5 dst=0.25 should be 0.5"
        );
        // Can-fail: Exclusion agrees with Difference at the corners (both
        // give 0 for identical inputs, identity for black) — a sign error
        // in the `- 2·s·d` term breaks these while keeping mid-range gray.
        assert_eq!(blend(BlendMode::Exclusion, 1.0, 1.0), 0.0);
        assert_eq!(blend(BlendMode::Exclusion, 1.0, 0.0), 1.0);
        assert_eq!(blend(BlendMode::Exclusion, 0.0, 0.0), 0.0);
        // Lower contrast than Difference mid-range: 0.5/0.5 gives 0.5 here,
        // 0.0 under Difference.
        assert_eq!(blend(BlendMode::Exclusion, 0.5, 0.5), 0.5);
        assert_eq!(blend(BlendMode::Difference, 0.5, 0.5), 0.0);
    }

    #[test]
    fn addsub_gray_is_noop_and_clamps() {
        // Can-fail: AddSub is `dst + src - 0.5`, NOT LinearLight's
        // `dst + 2·src - 1`. Mid-gray blend must leave every base exactly
        // alone (the doc's "grayscale values have no effect").
        for d in [0.0, 0.25, 1.0] {
            assert_eq!(
                blend(BlendMode::AddSub, 0.5, d),
                d,
                "addsub by mid-gray over {d} must be a no-op"
            );
        }
        // Lighter adds (0.2 + 0.7 - 0.5 = 0.4), darker subtracts with a
        // black clamp (0.3 + 0.2 - 0.5 underflows).
        let v = blend(BlendMode::AddSub, 0.7, 0.2);
        assert!(
            (v - 0.4).abs() < 1e-6,
            "addsub src=0.7 dst=0.2 should be 0.4, got {v}"
        );
        let v = blend(BlendMode::AddSub, 0.2, 0.3);
        assert!(
            v.abs() < 1e-6,
            "addsub src=0.2 dst=0.3 must clamp to black, got {v}"
        );
    }

    #[test]
    fn vivid_light_mid_gray_is_noop_and_pins_halves() {
        // Mid-gray blend is an exact no-op (burn(dst, 1) = dodge(dst, 0)).
        for d in [0.0, 0.25, 0.8, 1.0] {
            assert_eq!(
                blend(BlendMode::VividLight, 0.5, d),
                d,
                "vivid light by mid-gray over {d} must be a no-op"
            );
        }
        // Dark half burns: vivid(0.25, 0.8) = burn(0.8, 0.5) = 0.6.
        let v = blend(BlendMode::VividLight, 0.25, 0.8);
        assert!(
            (v - 0.6).abs() < 1e-6,
            "vivid light src=0.25 dst=0.8 should be 0.6, got {v}"
        );
        // Light half dodges: vivid(0.75, 0.5) = dodge(0.5, 0.5) = 1.0.
        assert_eq!(
            blend(BlendMode::VividLight, 0.75, 0.5),
            1.0,
            "vivid light src=0.75 dst=0.5 should dodge to white"
        );
        // The dodge/burn guards carry over: black burns to black, white
        // dodges to white (a naive burn/dodge divides by zero here).
        assert_eq!(blend(BlendMode::VividLight, 0.0, 0.3), 0.0);
        assert_eq!(blend(BlendMode::VividLight, 1.0, 0.3), 1.0);
    }

    #[test]
    fn linear_light_mid_gray_is_noop_and_pins_value_and_clamp() {
        // Mid-gray blend is an exact no-op (`d + 2·0.5 - 1 = d`).
        for d in [0.0, 0.25, 1.0] {
            assert_eq!(
                blend(BlendMode::LinearLight, 0.5, d),
                d,
                "linear light by mid-gray over {d} must be a no-op"
            );
        }
        // Known value: 0.25 + 2·0.75 - 1 = 0.75 (exact binary).
        assert_eq!(
            blend(BlendMode::LinearLight, 0.75, 0.25),
            0.75,
            "linear light src=0.75 dst=0.25 should be 0.75"
        );
        // Can-fail: LinearLight is NOT AddSub (`d + 2·s - 1` vs `d + s`).
        // src=0.75, dst=0.25 gives 0.75 here, 0.5 under AddSub.
        assert_eq!(blend(BlendMode::AddSub, 0.75, 0.25), 0.5);
        // Both clamps: black blend underflows, white blend overflows.
        assert_eq!(blend(BlendMode::LinearLight, 0.0, 0.2), 0.0);
        assert_eq!(blend(BlendMode::LinearLight, 1.0, 0.2), 1.0);
    }

    #[test]
    fn pin_light_mid_gray_is_noop_and_pins_min_max() {
        // Mid-gray blend is an exact no-op (min(d, 1) = max(d, 0) = d).
        for d in [0.0, 0.25, 1.0] {
            assert_eq!(
                blend(BlendMode::PinLight, 0.5, d),
                d,
                "pin light by mid-gray over {d} must be a no-op"
            );
        }
        // Dark half darkens via min: pin(0.25, 0.8) = min(0.8, 0.5) = 0.5.
        assert_eq!(
            blend(BlendMode::PinLight, 0.25, 0.8),
            0.5,
            "pin light src=0.25 dst=0.8 should be 0.5"
        );
        // Light half lightens via max: pin(0.75, 0.2) = max(0.2, 0.5).
        assert_eq!(
            blend(BlendMode::PinLight, 0.75, 0.2),
            0.5,
            "pin light src=0.75 dst=0.2 should be 0.5"
        );
    }

    #[test]
    fn disable_returns_base_and_replace_returns_source() {
        // Disable discards the layer at any opacity (Painter's compute
        // skip spelled as a pixel op).
        assert_eq!(blend(BlendMode::Disable, 0.9, 0.2), 0.2);
        for opacity in [0.0, 0.5, 1.0] {
            assert_eq!(
                BlendMode::Disable
                    .composite_channel(0.9, 0.2, opacity)
                    .expect("separable op"),
                0.2,
                "disable must show the base at opacity {opacity}"
            );
        }
        // Replace overwrites with the source at the primitive level (the
        // opacity-bypass delta is documented on the variant and deferred).
        assert_eq!(blend(BlendMode::Replace, 0.9, 0.2), 0.9);
        assert_eq!(
            BlendMode::Replace
                .composite_channel(0.9, 0.2, 1.0)
                .expect("separable op"),
            0.9,
            "replace at full opacity must be the source"
        );
    }

    #[test]
    fn hsv_round_trip_is_identity_on_wheel_colors() {
        // Can-fail: a swapped sector or a wrong `t mod 2` in either
        // conversion breaks the round trip while leaving primaries intact
        // (primaries sit on sector boundaries). The dusty-blue
        // (0.2, 0.4, 0.8) sits mid-sector and catches those.
        for c in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.2, 0.4, 0.8],
        ] {
            let back = BlendMode::hsv_to_rgb(BlendMode::rgb_to_hsv(c));
            for (got, want) in back.iter().zip(c.iter()) {
                assert!(
                    (got - want).abs() < 1e-6,
                    "hsv round trip of {c:?} drifted to {back:?}"
                );
            }
        }
        // Primaries pin known conversions exactly (sector boundaries).
        assert_eq!(BlendMode::rgb_to_hsv([1.0, 0.0, 0.0]), [0.0, 1.0, 1.0]);
        assert_eq!(BlendMode::rgb_to_hsv([0.0, 1.0, 0.0]), [120.0, 1.0, 1.0]);
        assert_eq!(BlendMode::rgb_to_hsv([0.0, 0.0, 1.0]), [240.0, 1.0, 1.0]);
        assert_eq!(
            BlendMode::rgb_to_hsv([0.5, 0.5, 0.5]),
            [0.0, 0.0, 0.5],
            "gray carries no saturation (hue is the 0.0 convention)"
        );
    }

    #[test]
    fn tint_keeps_src_hue_and_guards_achromatic_source() {
        // Red over blue takes red's hue with blue's sat/val: still red.
        assert_eq!(
            blend3(BlendMode::Tint, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0]
        );
        // Yellow hue (60°) over dark blue (v=0.5): (0.5, 0.5, 0.0) —
        // exact binary fractions, so this pins the sector math, not just
        // a primary passthrough.
        assert_eq!(
            blend3(BlendMode::Tint, [1.0, 1.0, 0.0], [0.0, 0.0, 0.5]),
            [0.5, 0.5, 0.0]
        );
        // Can-fail: black/gray/white carry no hue, so the bottom must come
        // back byte-identical (a naive hue-0 substitution reddens it).
        for src in [[0.0; 3], [0.5; 3], [1.0; 3]] {
            assert_eq!(
                blend3(BlendMode::Tint, src, [0.0, 0.0, 1.0]),
                [0.0, 0.0, 1.0],
                "tint by achromatic {src:?} must leave the bottom unchanged"
            );
        }
    }

    #[test]
    fn saturation_takes_src_saturation_and_grays_on_black() {
        // Red saturation at gray's value: dark red (0.5, 0, 0), exact.
        assert_eq!(
            blend3(BlendMode::Saturation, [1.0, 0.0, 0.0], [0.5; 3]),
            [0.5, 0.0, 0.0]
        );
        // Can-fail: black's zero saturation must actively desaturate — red
        // at value 1 with saturation 0 is WHITE, not red and not black.
        assert_eq!(
            blend3(BlendMode::Saturation, [0.0; 3], [1.0, 0.0, 0.0]),
            [1.0, 1.0, 1.0],
            "black saturation over red must gray to white (v=1)"
        );
    }

    #[test]
    fn color_takes_src_hue_sat_and_guards_achromatic() {
        // Green hue+sat at dark blue's value: (0, 0.5, 0), exact binary.
        assert_eq!(
            blend3(BlendMode::Color, [0.0, 1.0, 0.0], [0.0, 0.0, 0.5]),
            [0.0, 0.5, 0.0]
        );
        // Red over blue is red (hue+sat from red, value 1 from blue).
        assert_eq!(
            blend3(BlendMode::Color, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0]
        );
        // Achromatic top leaves the bottom unchanged (same guard as Tint).
        assert_eq!(
            blend3(BlendMode::Color, [0.5; 3], [0.0, 0.0, 1.0]),
            [0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn value_takes_src_value() {
        // Dark green's value (0.5) at red's hue+sat: (0.5, 0, 0), exact.
        assert_eq!(
            blend3(BlendMode::Value, [0.0, 0.5, 0.0], [1.0, 0.0, 0.0]),
            [0.5, 0.0, 0.0]
        );
        // Black's value is 0 regardless of bottom: always black.
        assert_eq!(
            blend3(BlendMode::Value, [0.0; 3], [1.0, 0.0, 0.0]),
            [0.0; 3]
        );
        // White's value is 1: dark red (0.5, 0, 0) becomes full red.
        assert_eq!(
            blend3(BlendMode::Value, [1.0; 3], [0.5, 0.0, 0.0]),
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn normal_combine_is_whiteout_with_flat_identities() {
        // Flat is the identity on both sides (the Whiteout guarantee).
        let flat = [0.5, 0.5, 1.0];
        assert_eq!(blend3(BlendMode::NormalCombine, flat, flat), flat);
        // A 45° +X tilt over flat returns the tilt (within f32 noise —
        // decode/normalize round-trip, not bit-exact by construction).
        let tilt = [
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
            0.5,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
        ];
        let got = blend3(BlendMode::NormalCombine, tilt, flat);
        for (g, w) in got.iter().zip(tilt.iter()) {
            assert!(
                (g - w).abs() < 1e-6,
                "combine of tilt over flat must return the tilt, got {got:?}"
            );
        }
        // Non-trivial Whiteout anchor, hand-derived in f64: +X tilt over
        // +Y tilt gives r = (t, t, 0.5) with t = √2/2, |r|² = 1.25, so
        // n = (0.63245553, 0.63245553, 0.44721360) and the encoding is
        // (0.816_227_8, 0.816_227_8, 0.723_606_8). Tolerance 1e-5 covers f32
        // normalize noise; a swapped z-term (e.g. UDN's `n1.z`) lands at
        // (0.85355339, …) and fails.
        let tilt_y = [
            0.5,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
        ];
        let got = blend3(BlendMode::NormalCombine, tilt, tilt_y);
        for (g, w) in got.iter().zip([0.816_227_8, 0.816_227_8, 0.723_606_8]) {
            assert!(
                (g - w).abs() < 1e-5,
                "whiteout tilt-over-tilt should be ~(0.8162, 0.8162, 0.7236), got {got:?}"
            );
        }
        // Whiteout is symmetric in its inputs (xy add, z multiply commute).
        assert_eq!(
            blend3(BlendMode::NormalCombine, tilt, tilt_y),
            blend3(BlendMode::NormalCombine, tilt_y, tilt)
        );
        // Degenerate detail (mid-gray decodes to the zero vector) keeps
        // the base instead of producing NaN.
        assert_eq!(blend3(BlendMode::NormalCombine, [0.5; 3], flat), flat);
    }

    #[test]
    fn normal_detail_is_rnm_with_flat_identities_and_pins_double_tilt() {
        // Flat is the identity on both sides (the RNM guarantee: detail on
        // a flat base returns the detail, flat detail returns the base).
        let flat = [0.5, 0.5, 1.0];
        let tilt = [
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
            0.5,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
        ];
        assert_eq!(blend3(BlendMode::NormalDetail, flat, flat), flat);
        for (name, got) in [
            (
                "detail-on-flat",
                blend3(BlendMode::NormalDetail, tilt, flat),
            ),
            ("flat-on-base", blend3(BlendMode::NormalDetail, flat, tilt)),
        ] {
            for (g, w) in got.iter().zip(tilt.iter()) {
                assert!(
                    (g - w).abs() < 1e-6,
                    "{name} must return the tilt, got {got:?}"
                );
            }
        }
        // Discriminating RNM anchor, hand-derived in f64: the same 45° +X
        // tilt as base AND detail composes the two rotations to a 90°
        // grazing normal — r = (1.70710678, 0, ~0), normalized (1, 0, 0),
        // encoded (1.0, 0.5, 0.5). Whiteout on the same inputs gives
        // (0.9714, 0.5, 0.6667), so this test cannot pass under the wrong
        // algorithm (and the combine test above pins the other side).
        let got = blend3(BlendMode::NormalDetail, tilt, tilt);
        for (g, w) in got.iter().zip([1.0, 0.5, 0.5]) {
            assert!(
                (g - w).abs() < 1e-5,
                "rnm double-tilt should be ~(1.0, 0.5, 0.5), got {got:?}"
            );
        }
        // Degenerate detail keeps the base instead of producing NaN.
        assert_eq!(blend3(BlendMode::NormalDetail, [0.5; 3], flat), flat);
    }

    #[test]
    fn normal_inverse_detail_swaps_rnm_roles() {
        // Inverse Detail fits the bottom to the top: bit-exact role swap
        // with Detail (same f32 op sequence on exchanged inputs).
        let flat = [0.5, 0.5, 1.0];
        let tilt_x = [
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
            0.5,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
        ];
        let tilt_y = [
            0.5,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
            0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2,
        ];
        for (s, d) in [(tilt_x, tilt_y), (tilt_x, flat), (flat, tilt_y)] {
            assert_eq!(
                blend3(BlendMode::NormalInverseDetail, s, d),
                blend3(BlendMode::NormalDetail, d, s),
                "InverseDetail({s:?}, {d:?}) must equal Detail({d:?}, {s:?})"
            );
        }
        // Absolute anchor: bottom tilt fitted to a flat top returns the
        // tilt (flat top contributes no reorientation).
        let got = blend3(BlendMode::NormalInverseDetail, flat, tilt_x);
        for (g, w) in got.iter().zip(tilt_x.iter()) {
            assert!(
                (g - w).abs() < 1e-6,
                "inverse detail of tilt under flat must return the tilt, got {got:?}"
            );
        }
    }

    #[test]
    fn blend_census_every_separable_mode_returns_finite_clamped_math() {
        // The static census pin: every separable variant must be a total
        // scalar pixel op — Some finite 0..=1 value for corner and
        // mid-range inputs alike. (The seven triplet modes are pinned by
        // the triplet census below: None here, total RGB ops there.) The
        // `len` assert below is the tripwire: add a variant without
        // extending this list and it fails, forcing the census (and the
        // enum docs above) to be updated with the enum.
        let separable = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Difference,
            BlendMode::SoftLight,
            BlendMode::HardLight,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::LinearBurn,
            BlendMode::Divide,
            BlendMode::InverseDivide,
            BlendMode::InverseSubtract,
            BlendMode::Exclusion,
            BlendMode::AddSub,
            BlendMode::VividLight,
            BlendMode::LinearLight,
            BlendMode::PinLight,
            BlendMode::Disable,
            BlendMode::Replace,
        ];
        assert_eq!(separable.len(), 24, "census list must track the enum");
        for mode in separable {
            for (s, d) in [
                (0.0, 0.0),
                (1.0, 1.0),
                (1.0, 0.0),
                (0.0, 1.0),
                (0.25, 0.75),
                (0.75, 0.25),
            ] {
                let v = mode
                    .blend_channel(s, d)
                    .unwrap_or_else(|| panic!("{mode:?} must return Some"));
                assert!(
                    v.is_finite() && (0.0..=1.0).contains(&v),
                    "{mode:?}({s}, {d}) must be finite 0..=1, got {v}"
                );
            }
        }
        assert_eq!(
            BlendMode::Passthrough.blend_channel(0.5, 0.5),
            None,
            "Passthrough is structural, not a pixel op"
        );
        assert_eq!(
            BlendMode::Passthrough.blend_rgb([0.5; 3], [0.5; 3]),
            None,
            "Passthrough has no triplet math either"
        );
    }

    #[test]
    fn blend_census_every_triplet_mode_returns_finite_clamped_rgb() {
        // The triplet census pin: all seven non-separable modes must be
        // total RGB ops — Some finite 0..=1 triplet for corner and
        // mid-range inputs — while the scalar primitive honestly returns
        // None for them (single-channel use is meaningless). The `len`
        // assert is the tripwire, mirroring the separable census above.
        // `is_triplet`/`is_normal_map` membership is pinned here too, so a
        // variant added to the enum without updating the predicates fails.
        let triplets = [
            BlendMode::Tint,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Value,
            BlendMode::NormalCombine,
            BlendMode::NormalDetail,
            BlendMode::NormalInverseDetail,
        ];
        assert_eq!(triplets.len(), 7, "triplet census must track the enum");
        for mode in triplets {
            assert!(mode.is_triplet(), "{mode:?} must report as triplet");
            assert_eq!(
                mode.is_normal_map(),
                matches!(
                    mode,
                    BlendMode::NormalCombine
                        | BlendMode::NormalDetail
                        | BlendMode::NormalInverseDetail
                ),
                "{mode:?} normal-map membership must be exactly the three normal modes"
            );
            assert_eq!(
                mode.blend_channel(0.5, 0.5),
                None,
                "{mode:?} must not answer the scalar primitive"
            );
            for (s, d) in [
                ([0.0; 3], [0.0; 3]),
                ([1.0; 3], [1.0; 3]),
                ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
                ([0.5; 3], [0.25, 0.5, 0.75]),
                ([0.75, 0.25, 0.5], [0.5; 3]),
            ] {
                let v = mode
                    .blend_rgb(s, d)
                    .unwrap_or_else(|| panic!("{mode:?} must return Some"));
                for lane in v {
                    assert!(
                        lane.is_finite() && (0.0..=1.0).contains(&lane),
                        "{mode:?}({s:?}, {d:?}) must be finite 0..=1, got {v:?}"
                    );
                }
            }
        }
        // And the separable census is exactly the complement (plus
        // Passthrough, which is neither): no variant may hide from both.
        for mode in [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Difference,
            BlendMode::SoftLight,
            BlendMode::HardLight,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::LinearBurn,
            BlendMode::Divide,
            BlendMode::InverseDivide,
            BlendMode::InverseSubtract,
            BlendMode::Exclusion,
            BlendMode::AddSub,
            BlendMode::VividLight,
            BlendMode::LinearLight,
            BlendMode::PinLight,
            BlendMode::Disable,
            BlendMode::Replace,
        ] {
            assert!(!mode.is_triplet(), "{mode:?} must stay separable");
            assert!(!mode.is_normal_map(), "{mode:?} must not claim normal-map");
        }
    }

    #[test]
    fn composite_channel_honors_opacity_and_ignores_non_finite() {
        // Opacity 1 = full blend effect, 0 = base untouched.
        assert_eq!(
            BlendMode::Multiply
                .composite_channel(0.5, 0.5, 1.0)
                .expect("separable op"),
            0.25,
            "full-opacity multiply 0.5/0.5 is 0.25"
        );
        assert_eq!(
            BlendMode::Multiply
                .composite_channel(0.5, 0.5, 0.0)
                .expect("separable op"),
            0.5,
            "zero opacity must leave the base untouched"
        );
        // Half opacity mixes halfway: 0.5 + (0.25 - 0.5) * 0.5 = 0.375.
        let v = BlendMode::Multiply
            .composite_channel(0.5, 0.5, 0.5)
            .expect("separable op");
        assert!(
            (v - 0.375).abs() < 1e-6,
            "half-opacity multiply should be 0.375, got {v}"
        );
        // Non-finite opacity leaves the base unchanged, mirroring
        // `set_opacity`'s refusal to store non-finite input.
        assert_eq!(
            BlendMode::Screen
                .composite_channel(0.9, 0.2, f32::NAN)
                .expect("separable op"),
            0.2,
            "NaN opacity must leave the base untouched"
        );
        assert_eq!(
            BlendMode::Passthrough.composite_channel(0.9, 0.2, 1.0),
            None,
            "Passthrough has no composite math either"
        );
    }

    #[test]
    fn instance_renders_source_and_overlays_are_link_local() {
        // An instance renders its source's pixels: the instance holds no
        // fill of its own, while its opacity/blend/mask overlay the link.
        let mut stack = LayerStack::new();
        let src = stack.add_layer("Base", LayerKind::Paint);
        let inst = stack.add_instance("Base inst", src).expect("source exists");
        assert_eq!(stack.instance_source(inst), Some(src));
        assert!(stack.is_instance(inst));
        assert!(!stack.is_instance(src));
        // Per-instance overlays live on the link, not the pixels.
        stack.set_opacity(inst, 0.5);
        stack.set_blend_mode(inst, BlendMode::Multiply);
        assert_eq!(stack.layer(inst).unwrap().opacity, 0.5);
        assert_eq!(stack.layer(inst).unwrap().blend_mode, BlendMode::Multiply);
        // … and the source is untouched by the overlay.
        assert_eq!(stack.layer(src).unwrap().opacity, 1.0);
        assert_eq!(stack.layer(src).unwrap().blend_mode, BlendMode::Normal);
    }

    #[test]
    fn instance_direct_cycle_is_rejected_at_attach_time() {
        // A→B→A: retargeting B onto A (which instances B) must fail.
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_instance("B inst A", a).expect("source exists");
        assert_eq!(
            stack.set_instance_source(b, b),
            Err(InstanceError::Cycle(vec![b, b])),
            "self-instancing closes a 1-cycle"
        );
        // Build the reverse link attempt: A is paint, so make A an
        // instance of B first is impossible — instead add C instancing B,
        // then retarget B onto C (B→C→B).
        let c = stack.add_instance("C inst B", b).expect("source exists");
        match stack.set_instance_source(b, c) {
            Err(InstanceError::Cycle(participants)) => {
                assert_eq!(participants.first(), Some(&b));
                assert!(participants.contains(&c));
            }
            other => panic!("A→B→A must be rejected, got {other:?}"),
        }
        // The failed retarget changed nothing.
        assert_eq!(stack.instance_source(b), Some(a));
        let _ = a;
    }

    #[test]
    fn instance_four_hop_indirect_cycle_is_rejected() {
        // D→C→B→A chain, then retarget A (as instance) onto D: the DFS
        // must walk all four hops and reject.
        let mut stack = LayerStack::new();
        let a0 = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_instance("B inst A", a0).expect("source exists");
        let c = stack.add_instance("C inst B", b).expect("source exists");
        let d = stack.add_instance("D inst C", c).expect("source exists");
        // Turn the paint root into an instance of D would close
        // A→D→C→B→A — but A is paint, so instead retarget B onto D:
        // B→D→C→B (3 hops through the new edge).
        match stack.set_instance_source(b, d) {
            Err(InstanceError::Cycle(participants)) => {
                assert_eq!(participants.first(), Some(&b));
                for id in [b, c, d] {
                    assert!(participants.contains(&id), "participants: {participants:?}");
                }
            }
            other => panic!("4-hop indirect cycle must be rejected, got {other:?}"),
        }
        assert_eq!(stack.instance_source(b), Some(a0));
    }

    #[test]
    fn instance_eval_order_walks_transitively() {
        // A instances B instances C (paint root): eval order C,B,A —
        // sources before instances, transitively, regardless of stack
        // (insertion) order.
        let mut stack = LayerStack::new();
        let c = stack.add_layer("C", LayerKind::Paint);
        let b = stack.add_instance("B inst C", c).expect("source exists");
        let a = stack.add_instance("A inst B", b).expect("source exists");
        assert_eq!(stack.instance_chain(a), vec![b, c]);
        assert_eq!(stack.instance_chain(b), vec![c]);
        assert!(stack.instance_chain(c).is_empty());
        let order = stack.instance_eval_order().expect("acyclic");
        let pos = |id: u64| order.iter().position(|&x| x == id).unwrap();
        assert!(pos(c) < pos(b), "C before B: {order:?}");
        assert!(pos(b) < pos(a), "B before A: {order:?}");
    }

    #[test]
    fn instance_paint_attempt_is_a_typed_error_naming_the_source() {
        let mut stack = LayerStack::new();
        let src = stack.add_layer("Base", LayerKind::Paint);
        let inst = stack.add_instance("inst", src).expect("source exists");
        assert_eq!(stack.check_paintable(src), Ok(()));
        assert_eq!(
            stack.check_paintable(inst),
            Err(InstanceError::PaintOnInstance {
                instance: inst,
                source_id: src,
            })
        );
        assert_eq!(
            stack.check_paintable(999),
            Err(InstanceError::UnknownLayer(999))
        );
    }

    #[test]
    fn instance_unknown_source_and_not_an_instance_are_typed() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        assert!(matches!(
            stack.add_instance("ghost", 999),
            Err(InstanceError::UnknownSource { .. })
        ));
        assert_eq!(
            stack.set_instance_source(a, a),
            Err(InstanceError::NotAnInstance(a))
        );
        let inst = stack.add_instance("inst", a).expect("source exists");
        assert!(matches!(
            stack.set_instance_source(inst, 999),
            Err(InstanceError::UnknownSource { .. })
        ));
    }

    #[test]
    fn instance_retarget_round_trips_through_undo() {
        let mut stack = LayerStack::new();
        let a = stack.add_layer("A", LayerKind::Paint);
        let b = stack.add_layer("B", LayerKind::Paint);
        let inst = stack.add_instance("inst", a).expect("source exists");
        let mut undo: UndoStack<LayerCommand> = UndoStack::new(100);
        undo.push(LayerCommand::set_instance_source(inst, b), &mut stack);
        assert_eq!(stack.instance_source(inst), Some(b));
        assert!(undo.undo(&mut stack));
        assert_eq!(stack.instance_source(inst), Some(a));
        assert!(undo.redo(&mut stack));
        assert_eq!(stack.instance_source(inst), Some(b));
    }
}
