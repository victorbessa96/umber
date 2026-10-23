//! The stack compositor: blends one tile's layer stack into a single output target.
//!
//! Wave-2 core-loop scope (docs/specs/2026-10-12-strategy-review.md, critical 6):
//! the tile pool ([`crate::tile_pool`]) owns one GPU target per (layer, channel,
//! tile); this module owns the composite over the stack — bottom-to-top,
//! linear blending with opacity and the [`umber_core::BlendMode`] set —
//! into one output [`PaintTarget`]. Separable modes blend per channel;
//! the seven triplet modes (HSV + normal-map families) blend the RGB
//! triplet at once via [`umber_core::BlendMode::blend_rgb`]. The caller
//! binds the right tiles (one pool [`PoolKey`] per layer for the channel
//! being composited); per-channel routing above the tile (which pool keys
//! feed a given composite, including the normal-channel gate for the
//! normal-map modes) is the viewport-texture kill's surface, not this
//! module's.
//!
//! The WGSL below is transliterated FROM the Rust reference —
//! [`umber_core::BlendMode::blend_channel`] plus [`umber_core::BlendMode::blend_rgb`]
//! plus [`umber_core::BlendMode::composite_channel`] (layers.rs) — NOT invented: every
//! match arm, clamp, and divide-by-zero guard has a one-to-one counterpart, in the
//! same order, under the same ids ([`blend_mode_id`]). `crate::shaders` is locked
//! by the concurrent viewport-texture kill, so the shader lives here as an inline
//! `const &str` — the established pattern (`TEXTURE_DISPLAY_SHADER`,
//! `VIEW_MODE_WGSL`, `POSTFX_SHADER` all do this).
//!
//! # Judgment calls (all load-bearing; read before extending)
//!
//! 1. **Iterative per-layer dispatches, not one single-pass texture-array
//!    dispatch.** One dispatch per contributing layer (sampled src + in-place
//!    `read_write` accumulator + uniforms) uses only binding shapes the paint pass
//!    already proves (`paint.rs`); a single-pass version needs `binding_array` of
//!    sampled textures plus a max-layers cap, a portability risk this kill cannot
//!    afford with CI as the only runtime gate. Per-texel math is identical either
//!    way; the array form can replace this loop later without changing the oracle.
//! 2. **Coverage-gated composite, equivalent to the reference on opaque sources.**
//!    The scalar reference has no alpha input, so a naive per-channel fold lets a
//!    transparent `Normal` texel *erase* the stack below (src 0 over dst at full
//!    opacity yields 0) — and an evicted (transparent) tile would nuke the
//!    composite instead of passing through. The kernel therefore gates by
//!    coverage `cov = src_alpha × opacity`: `out = dst + (blend − dst) × cov`,
//!    with `blend` from the reference arms. Replace is the one exception:
//!    the mode's contract bypasses opacity AND source-alpha grading
//!    (`cov = mask × presence` — see `composite_pixel_masked`); every
//!    other mode gates exactly as written. For opaque sources (`src_alpha = 1`)
//!    the op order is bit-identical to `composite_channel`, so the oracle test
//!    pins exact equality there; for transparent sources the layer is a no-op,
//!    which is the only sane reading of "missing tile comes back transparent".
//! 3. **Alpha is straight-alpha over, not a 4th `blend_channel` call.** Running
//!    e.g. `Multiply` on the alpha channel is wrong at partial dst alpha (opaque
//!    src over translucent dst must be opaque; `blend_channel` on alpha keeps it
//!    translucent). Alpha composites as `dst_a + (src_a − dst_a) × cov` — which
//!    agrees with the reference exactly where the reference is well-defined
//!    (opaque stacks, `Normal`).
//! 4. **Folders and `Passthrough` are transparent no-ops.** Folder *membership*
//!    is not modeled in `umber-core` yet (see layers.rs), so a folder carries no
//!    pixels: its entry must pass `None` (see [`GpuStackLayer::from_layer`]), and
//!    coverage gating makes that a no-op under every mode. `BlendMode::
//!    Passthrough` entries are skipped before dispatch for the same reason.
//!    Non-passthrough folder isolation (flatten children, then apply the folder's
//!    own blend/opacity) is deferred to the membership work, recorded here.
//! 5. **In-place accumulator.** The output target is bound `read_write` and
//!    updated per layer — the `PAINT_COMPUTE_SHADER` pattern, one thread per
//!    texel, no cross-invocation hazard, no scratch target, no copies. This needs
//!    `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` for `Rgba8Unorm` read/write, the
//!    same floor as the paint pass (one device story — the app creates one device
//!    for both). `output` must not alias a layer texture (same-texture sampled +
//!    storage bind is a GPU validation hazard); this is a documented precondition,
//!    not a runtime check.
//! 6. **Mode census: 32 variants, 31 pixel ops.** The enum carries `Normal`,
//!    `Passthrough`, 24 separable modes, and 7 triplet modes (the HSV and
//!    normal-map families); `Passthrough` is structural (never reaches the
//!    shader). Triplet modes share the SAME per-layer dispatch shape as the
//!    separable ones — the kernel branches to the triplet path inside one
//!    dispatch, so opacity/coverage gating and alpha handling are identical
//!    across families. Unknown WGSL ids fall back to `Normal` —
//!    unreachable via [`blend_mode_id`], defense in depth.
//! 7. **Bounds.** Entries SHOULD match the output dimensions. A smaller entry's
//!    out-of-range texels read as transparent black (defined zero-fill on
//!    `textureLoad`, never an error); the CPU oracle is strict instead
//!    ([`CompositeError::InvalidPixelData`]) because an oracle must fail loud.
//!    Empty stacks, all-invisible stacks, and all-missing stacks clear the output
//!    to transparent and succeed.
//! 8. **GPU-vs-CPU comparison is ±1 LSB, not byte-exact.** UNORM round-trip
//!    rounding and FMA contraction are implementation-defined; the arithmetic
//!    pins exact values on the CPU side instead (every byte asserted there is
//!    derived by hand — see the tests), and the GPU side asserts near-equality
//!    with the reason in the comment.
//! 9. **Layer masks gate coverage, on both paths.** An entry's `mask` plane
//!    (`width × height` grayscale bytes, `255` = open) multiplies the
//!    coverage: `cov = src_alpha × opacity × mask`. The CPU oracle reads the
//!    bytes directly; the GPU path uploads them per dispatch into an `R8Unorm`
//!    `mask_tex` (binding 3) plus a `mask_enabled` uniform — the paint pass's
//!    quick-mask shape (fallback 1×1 texture + per-dab flag), except the flag
//!    is per LAYER here, not per dab. Absent mask ≡ all-`255`: `None` takes
//!    the same code path with `m = 1.0` (CPU) or `mask_enabled = 0` (GPU),
//!    so unmasked stacks are byte-identical to the pre-mask composite.
//!    Wrong-length mask bytes fail loud on both paths
//!    ([`CompositeError::InvalidMaskData`]), like pixel data. A mask on a
//!    skipped entry (invisible, pixel-less, `Passthrough`) is never read.
//! 10. **Premultiplied sources un-premultiply before the blend, never
//!     inside the blend arms.** When an entry's `premultiplied` flag is
//!     set, its decoded texel `p` becomes `s = p / a` for `a > 0` (`a ==
//!     0` gives `cov == 0`, a no-op), clamped to `0..=1` — which is
//!     exactly "clamp `p` to `a`" (`min(p, a) / a == min(p / a, 1)` for
//!     `a > 0`); then [`composite_pixel_masked`] runs unchanged. On WGSL
//!     the flag rides `CompositeParams.flags` bit 0 (the old `pad0`), so
//!     the 32-byte assert holds. Both `from_layer` constructors default
//!     it to `false`, so every existing caller and oracle test is
//!     byte-identical with the flag off.
//! 11. **The bottom-copy identity rule.** When no entry has contributed
//!     yet and the first contributing entry is premultiplied, `Normal`,
//!     full-opacity (`clamped_opacity == 1.0`) and unmasked, its bytes
//!     SEED the accumulator verbatim instead of compositing. On valid
//!     premultiplied data (`p <= a`) the seed equals the general
//!     composite exactly (judgment 10's identity); on anything else it is
//!     still verbatim — which keeps migrated Base bytes (v1's
//!     mixed-convention planes) intact instead of clamping them through
//!     the blend. "Contributed" means the entry passed the skip filters
//!     (visible, pixel-bearing, non-`Passthrough`): even a fully
//!     transparent bottom entry counts, so the seed fires at most once
//!     per call and only at the true bottom. On the CPU the seed copies
//!     `byte / 255` per texel, which [`f32_to_u8`] round-trips exactly;
//!     on the GPU it is a `copy_texture_to_texture` of the entry's tile
//!     overlap — exact by construction, not ±1 LSB (the design doc's
//!     seed-dispatch alternative would pay a UNORM round-trip; the copy
//!     needs no new shader code, and both textures are `Rgba8Unorm`
//!     `PaintTarget`s carrying `COPY_SRC | COPY_DST`).
//! 12. **Region compositing reuses the one composite shader for both
//!     scopes.** [`StackCompositor::composite_stack_region`] clears and
//!     recomposites only `rect`, leaving every texel outside untouched
//!     (the full path's whole-output clear is what makes
//!     [`StackCompositor::composite_stack`] unusable per event). The
//!     dispatch extent rides `CompositeParams.width`/`height` with the
//!     rect origin in `origin_x`/`origin_y`, so the kernel's bounds check
//!     and its mask sample (`texel − origin`) read identically on both
//!     paths: full-tile dispatches carry origin `(0, 0)` with the output
//!     dims, which is exactly the old `gid >= dims` guard and the old
//!     texel mask sample. A masked entry uploads only the rect's mask
//!     bytes into a rect-sized `R8Unorm` texture through the existing
//!     mask-upload helper; the rect clear is a transparent
//!     `PaintTarget::write_region` (the retouch path's proven shape — a
//!     render-pass scissor clear would depend on load-op/scissor
//!     interaction this harness cannot prove). Out-of-bounds rect edges
//!     clamp to the tile and a fully-outside rect is a no-op (empty bytes
//!     from the CPU oracle): dirty rects are computed in-bounds, so the
//!     clamp is a safety net, never a silent fixup of live data.

use std::borrow::Cow;
use std::mem::size_of;
use std::num::NonZeroU64;

use umber_core::{BlendMode, Layer};
use wgpu::util::DeviceExt as _;

use crate::paint::{PaintError, PaintTarget};

/// WGSL entry point for the stack-composite dispatch.
pub const COMPOSITE_ENTRY: &str = "cs_composite";

/// Blend-mode shader ids. `BLEND_*` must equal [`blend_mode_id`] — the
/// `blend_mode_ids_are_pinned` test pins the Rust side; keep this list in
/// the same order when adding modes.
pub const COMPOSITE_WGSL: &str = r#"// ---- stack compositor (crate::compositor — transliterated from umber_core::BlendMode) ----
struct CompositeParams {
    mode: u32,
    opacity: f32,
    width: u32,
    height: u32,
    mask_enabled: u32,
    flags: u32,
    origin_x: u32,
    origin_y: u32,
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var acc_tex: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(2) var<uniform> params: CompositeParams;
@group(0) @binding(3) var mask_tex: texture_2d<f32>;

const BLEND_NORMAL: u32 = 0u;
const BLEND_MULTIPLY: u32 = 1u;
const BLEND_SCREEN: u32 = 2u;
const BLEND_OVERLAY: u32 = 3u;
const BLEND_DARKEN: u32 = 4u;
const BLEND_LIGHTEN: u32 = 5u;
const BLEND_ADD: u32 = 6u;
const BLEND_SUBTRACT: u32 = 7u;
const BLEND_DIFFERENCE: u32 = 8u;
const BLEND_SOFT_LIGHT: u32 = 9u;
const BLEND_HARD_LIGHT: u32 = 10u;
const BLEND_COLOR_DODGE: u32 = 11u;
const BLEND_COLOR_BURN: u32 = 12u;
const BLEND_LINEAR_BURN: u32 = 13u;
const BLEND_DIVIDE: u32 = 14u;
const BLEND_INVERSE_DIVIDE: u32 = 15u;
const BLEND_INVERSE_SUBTRACT: u32 = 16u;
const BLEND_EXCLUSION: u32 = 17u;
const BLEND_ADD_SUB: u32 = 18u;
const BLEND_VIVID_LIGHT: u32 = 19u;
const BLEND_LINEAR_LIGHT: u32 = 20u;
const BLEND_PIN_LIGHT: u32 = 21u;
const BLEND_DISABLE: u32 = 22u;
const BLEND_REPLACE: u32 = 23u;
const BLEND_TINT: u32 = 24u;
const BLEND_SATURATION: u32 = 25u;
const BLEND_COLOR: u32 = 26u;
const BLEND_VALUE: u32 = 27u;
const BLEND_NORMAL_COMBINE: u32 = 28u;
const BLEND_NORMAL_DETAIL: u32 = 29u;
const BLEND_NORMAL_INVERSE_DETAIL: u32 = 30u;

fn clamp01(v: f32) -> f32 {
    // WGSL has NO isNan/isinf builtin (removed from the spec; naga 30's
    // front accepts neither spelling — the Gate-0 journey test caught the
    // isNan case on the shader's FIRST real compile, and the wave-1
    // battery caught the INF case cross-platform: f32 clamp(INF, 0, 1)
    // diverges across implementations, so a plain clamp LEAKS +inf as 1.0
    // (fully open) on some stacks. The portable fail-closed idiom for ALL
    // non-finite values: NaN compares unequal to itself; ±inf fails the
    // finite-range test (a finite v always satisfies both v >= 0.0 OR
    // v <= 1.0 being COMPARABLE — the exact test is that inf compares
    // 'wrongly': +inf > 1.0 survives clamp, so the reject arm is
    // !(v <= 1.0) && !(v >= 0.0) is empty for finite... explicit form:
    // NaN: v != v. +inf: v > 1.0 AND v*0.0 != 0.0 (finite >1 clamps, inf
    // doesn't multiply down). Simplest portable: reject when the value
    // doesn't survive the finite probe v - v == 0 (inf - inf = NaN, fails).
    if (v != v || v - v != 0.0) {
        return 0.0;
    }
    return clamp(v, 0.0, 1.0);
}

fn clamp01v(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(clamp01(c.x), clamp01(c.y), clamp01(c.z));
}

// Guarded dodge/burn shared by the plain arms and Vivid Light — mirrors
// BlendMode::color_dodge/color_burn (the guards, not just the math).
fn color_dodge(s: f32, d: f32) -> f32 {
    if (s >= 1.0) {
        return 1.0;
    }
    return min(d / (1.0 - s), 1.0);
}

fn color_burn(s: f32, d: f32) -> f32 {
    if (s <= 0.0) {
        return 0.0;
    }
    return 1.0 - min((1.0 - d) / s, 1.0);
}

// Mirrors BlendMode::blend_channel arm-for-arm (NaN -> 0, inf saturates,
// dodge/burn/divide guards intact). Triplet modes (24-30) never reach
// here — the kernel routes them to blend_triplet. Unknown modes fall back
// to Normal.
fn blend_channel(mode: u32, src: f32, dst: f32) -> f32 {
    let s = clamp01(src);
    let d = clamp01(dst);
    var v = s;
    if (mode == BLEND_NORMAL) {
        v = s;
    } else if (mode == BLEND_MULTIPLY) {
        v = s * d;
    } else if (mode == BLEND_SCREEN) {
        v = s + d - s * d;
    } else if (mode == BLEND_OVERLAY) {
        if (d <= 0.5) {
            v = 2.0 * s * d;
        } else {
            v = 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
        }
    } else if (mode == BLEND_DARKEN) {
        v = min(s, d);
    } else if (mode == BLEND_LIGHTEN) {
        v = max(s, d);
    } else if (mode == BLEND_ADD) {
        v = min(s + d, 1.0);
    } else if (mode == BLEND_SUBTRACT) {
        v = max(d - s, 0.0);
    } else if (mode == BLEND_DIFFERENCE) {
        v = abs(s - d);
    } else if (mode == BLEND_SOFT_LIGHT) {
        v = ((1.0 - 2.0 * s) * d + 2.0 * s) * d;
    } else if (mode == BLEND_HARD_LIGHT) {
        if (s <= 0.5) {
            v = 2.0 * s * d;
        } else {
            v = 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
        }
    } else if (mode == BLEND_COLOR_DODGE) {
        v = color_dodge(s, d);
    } else if (mode == BLEND_COLOR_BURN) {
        v = color_burn(s, d);
    } else if (mode == BLEND_LINEAR_BURN) {
        v = max(s + d - 1.0, 0.0);
    } else if (mode == BLEND_DIVIDE) {
        if (s <= 0.0) {
            v = 1.0;
        } else {
            v = min(d / s, 1.0);
        }
    } else if (mode == BLEND_INVERSE_DIVIDE) {
        if (d <= 0.0) {
            v = 1.0;
        } else {
            v = min(s / d, 1.0);
        }
    } else if (mode == BLEND_INVERSE_SUBTRACT) {
        v = max(s - d, 0.0);
    } else if (mode == BLEND_EXCLUSION) {
        v = s + d - 2.0 * s * d;
    } else if (mode == BLEND_ADD_SUB) {
        v = clamp(d + s - 0.5, 0.0, 1.0);
    } else if (mode == BLEND_VIVID_LIGHT) {
        if (s <= 0.5) {
            v = color_burn(2.0 * s, d);
        } else {
            v = color_dodge(2.0 * s - 1.0, d);
        }
    } else if (mode == BLEND_LINEAR_LIGHT) {
        v = clamp(d + 2.0 * s - 1.0, 0.0, 1.0);
    } else if (mode == BLEND_PIN_LIGHT) {
        if (s <= 0.5) {
            v = min(d, 2.0 * s);
        } else {
            v = max(d, 2.0 * (s - 0.5));
        }
    } else if (mode == BLEND_DISABLE) {
        v = d;
    } else if (mode == BLEND_REPLACE) {
        v = s;
    }
    return clamp01(v);
}

// Mirrors BlendMode::rgb_to_hsv: hue degrees in 0..<360, achromatic inputs
// yield hue 0. The red-max arm needs no modulo (its quotient sits in
// -1..1, and the negative fixup below folds it into 300..<360).
fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let dd = mx - mn;
    var h: f32 = 0.0;
    var s: f32 = 0.0;
    if (mx > 0.0) {
        s = dd / mx;
    }
    if (dd > 0.0) {
        if (mx == c.r) {
            h = 60.0 * ((c.g - c.b) / dd);
        } else if (mx == c.g) {
            h = 60.0 * ((c.b - c.r) / dd + 2.0);
        } else {
            h = 60.0 * ((c.r - c.g) / dd + 4.0);
        }
        if (h < 0.0) {
            h += 360.0;
        }
    }
    return vec3<f32>(h, s, mx);
}

// Mirrors BlendMode::hsv_to_rgb. The t-mod-2 is written without % (WGSL %
// on floats is a portability smell next to naga 30) — same shape as Rust.
fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let cc = c.z * c.y;
    let t = c.x / 60.0;
    let tm = t - floor(t / 2.0) * 2.0;
    let xx = cc * (1.0 - abs(tm - 1.0));
    let mm = c.z - cc;
    var rgb = vec3<f32>(0.0, 0.0, 0.0);
    if (c.x < 60.0) {
        rgb = vec3<f32>(cc, xx, 0.0);
    } else if (c.x < 120.0) {
        rgb = vec3<f32>(xx, cc, 0.0);
    } else if (c.x < 180.0) {
        rgb = vec3<f32>(0.0, cc, xx);
    } else if (c.x < 240.0) {
        rgb = vec3<f32>(0.0, xx, cc);
    } else if (c.x < 300.0) {
        rgb = vec3<f32>(xx, 0.0, cc);
    } else {
        rgb = vec3<f32>(cc, 0.0, xx);
    }
    return rgb + vec3<f32>(mm, mm, mm);
}

// Normalizes r, falling back to the encoded base when r is degenerate —
// mirrors BlendMode::normalize_or. Inputs here are bounded (decoded from
// clamped lanes), so inf/NaN are unreachable, but the guard is kept so the
// mirror stays line-for-line: NaN fails every comparison (see clamp01),
// and the upper bound additionally catches +inf.
fn normal_encode_or_base(r: vec3<f32>, base_enc: vec3<f32>) -> vec3<f32> {
    let l2 = dot(r, r);
    if (!(l2 > 0.000000000001) || !(l2 <= 1000000.0)) {
        return base_enc;
    }
    return r / sqrt(l2) * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
}

// Mirrors BlendMode::whiteout (Painter's Normal Map Combine).
fn whiteout(s: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let n1 = d * 2.0 - vec3<f32>(1.0, 1.0, 1.0);
    let n2 = s * 2.0 - vec3<f32>(1.0, 1.0, 1.0);
    let r = vec3<f32>(n1.x + n2.x, n1.y + n2.y, n1.z * n2.z);
    return normal_encode_or_base(r, d);
}

// Mirrors BlendMode::rnm (Reoriented Normal Mapping, Detail family).
fn rnm(base: vec3<f32>, det: vec3<f32>) -> vec3<f32> {
    let b = base * 2.0 - vec3<f32>(1.0, 1.0, 1.0);
    let dd = det * 2.0 - vec3<f32>(1.0, 1.0, 1.0);
    let t = b + vec3<f32>(0.0, 0.0, 1.0);
    let u = vec3<f32>(-dd.x, -dd.y, dd.z);
    let r = t * dot(t, u) - u * t.z;
    return normal_encode_or_base(r, base);
}

// Mirrors BlendMode::blend_rgb for the seven triplet modes (inputs already
// clamped by the caller). Separable modes never reach here.
fn blend_triplet(mode: u32, s: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    if (mode == BLEND_TINT) {
        let hs = rgb_to_hsv(s);
        if (hs.y <= 0.0) {
            return d;
        }
        let hd = rgb_to_hsv(d);
        return clamp01v(hsv_to_rgb(vec3<f32>(hs.x, hd.y, hd.z)));
    } else if (mode == BLEND_SATURATION) {
        let hs = rgb_to_hsv(s);
        let hd = rgb_to_hsv(d);
        return clamp01v(hsv_to_rgb(vec3<f32>(hd.x, hs.y, hd.z)));
    } else if (mode == BLEND_COLOR) {
        let hs = rgb_to_hsv(s);
        if (hs.y <= 0.0) {
            return d;
        }
        let hd = rgb_to_hsv(d);
        return clamp01v(hsv_to_rgb(vec3<f32>(hs.x, hs.y, hd.z)));
    } else if (mode == BLEND_VALUE) {
        let hs = rgb_to_hsv(s);
        let hd = rgb_to_hsv(d);
        return clamp01v(hsv_to_rgb(vec3<f32>(hd.x, hd.y, hs.z)));
    } else if (mode == BLEND_NORMAL_COMBINE) {
        return clamp01v(whiteout(s, d));
    } else if (mode == BLEND_NORMAL_DETAIL) {
        return clamp01v(rnm(d, s));
    } else if (mode == BLEND_NORMAL_INVERSE_DETAIL) {
        return clamp01v(rnm(s, d));
    }
    return d;
}

@compute @workgroup_size(8, 8)
fn cs_composite(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Region scope (judgment 12): width/height are the DISPATCH extent
    // and origin_x/origin_y its origin — the output dims with origin
    // (0, 0) on the full path, the rect with its origin on the region
    // path. Threads outside the extent return; texels outside a region
    // rect are never stored to.
    let tx = params.origin_x + gid.x;
    let ty = params.origin_y + gid.y;
    if (tx >= params.origin_x + params.width || ty >= params.origin_y + params.height) {
        return;
    }
    let org = vec2<i32>(i32(params.origin_x), i32(params.origin_y));
    let xy = vec2<i32>(i32(tx), i32(ty));
    let s = textureLoad(src_tex, xy, 0);
    let d = textureLoad(acc_tex, xy);
    var o = params.opacity;
    // Same WGSL fact as clamp01: no isFinite builtin. The finite check is
    // abs(o) != inf, i.e. !(o is NaN or ±inf) — NaN fails every comparison,
    // and inf fails the <= bound. One line, spec-portable.
    if (!(o <= 1000000.0 && o >= -1000000.0)) {
        o = 0.0;
    }
    o = clamp(o, 0.0, 1.0);
    var sa = clamp01(s.w);
    // The premultiplied-source gate (judgment 10): un-premultiply before
    // the blend — s = p / a clamped to 1 ("clamp p to a"); a == 0 zeroes
    // the channels, which coverage then ignores (cov == 0, a no-op).
    var sr = vec3<f32>(s.x, s.y, s.z);
    if ((params.flags & 1u) != 0u) {
        if (sa > 0.0) {
            sr = min(sr / sa, vec3<f32>(1.0, 1.0, 1.0));
        } else {
            sr = vec3<f32>(0.0, 0.0, 0.0);
        }
    }
    // The layer-mask gate (module-docs judgment 9): an enabled entry's
    // coverage is multiplied by its mask texel (`255` = open). Disabled
    // entries never sample the mask (the bound fallback is 1x1, so an
    // out-of-range load there would be meaningless — but it is skipped).
    // Region dispatches bind a rect-sized mask holding the rect's bytes,
    // so the sample is at texel − origin; full-tile dispatches carry
    // origin (0, 0), which reads exactly as the old texel sample.
    var m = 1.0;
    if (params.mask_enabled == 1u) {
        m = clamp01(textureLoad(mask_tex, xy - org, 0).r);
    }
    // Replace bypasses the layer opacity AND the source-alpha grading
    // (the mode's contract; the CPU oracle and umber-core's primitive
    // mirror this): a present texel (any alpha > 0) overwrites in full;
    // a fully-transparent texel stays a no-op — the premultiplied arm
    // above zeroes its channels and relies on cov == 0. The mask still
    // clips.
    if (params.mode == BLEND_REPLACE) {
        o = 1.0;
        if (sa > 0.0) {
            sa = 1.0;
        } else {
            sa = 0.0;
        }
    }
    let cov = sa * o * m;
    let dx = clamp01(d.x);
    let dy = clamp01(d.y);
    let dz = clamp01(d.z);
    let dw = clamp01(d.w);
    // Triplet modes (HSV + normal-map families) blend the RGB pixel at
    // once via blend_triplet; separable modes fold blend_channel per lane.
    // Both paths share this dispatch, the coverage gating, and the alpha
    // handling below — one golden per family pins each path (see tests).
    var b3 = vec3<f32>(
        blend_channel(params.mode, sr.x, d.x),
        blend_channel(params.mode, sr.y, d.y),
        blend_channel(params.mode, sr.z, d.z),
    );
    if (params.mode == BLEND_TINT || params.mode == BLEND_SATURATION || params.mode == BLEND_COLOR || params.mode == BLEND_VALUE || params.mode == BLEND_NORMAL_COMBINE || params.mode == BLEND_NORMAL_DETAIL || params.mode == BLEND_NORMAL_INVERSE_DETAIL) {
        b3 = blend_triplet(params.mode, clamp01v(sr), clamp01v(vec3<f32>(d.x, d.y, d.z)));
    }
    let r = dx + (b3.x - dx) * cov;
    let g = dy + (b3.y - dy) * cov;
    let b = dz + (b3.z - dz) * cov;
    // Straight-alpha over (Porter-Duff): the lerp form diverges at partial
    // src alpha — half-alpha red over opaque white must remain opaque.
    let a = cov + dw * (1.0 - cov);
    textureStore(acc_tex, xy, vec4<f32>(r, g, b, a));
}
// ---- end stack compositor ----"#;

/// Maps a [`BlendMode`] to its [`COMPOSITE_WGSL`] id.
///
/// Returns `None` for [`BlendMode::Passthrough`], which is structural (folder
/// children blend directly below) rather than a pixel op — the composite loop
/// skips such entries before dispatch, mirroring how `composite_channel`
/// returns `None` there.
pub fn blend_mode_id(mode: BlendMode) -> Option<u32> {
    match mode {
        BlendMode::Normal => Some(0),
        BlendMode::Multiply => Some(1),
        BlendMode::Screen => Some(2),
        BlendMode::Overlay => Some(3),
        BlendMode::Darken => Some(4),
        BlendMode::Lighten => Some(5),
        BlendMode::Add => Some(6),
        BlendMode::Subtract => Some(7),
        BlendMode::Difference => Some(8),
        BlendMode::SoftLight => Some(9),
        BlendMode::HardLight => Some(10),
        BlendMode::ColorDodge => Some(11),
        BlendMode::ColorBurn => Some(12),
        BlendMode::LinearBurn => Some(13),
        BlendMode::Divide => Some(14),
        BlendMode::InverseDivide => Some(15),
        BlendMode::InverseSubtract => Some(16),
        BlendMode::Exclusion => Some(17),
        BlendMode::AddSub => Some(18),
        BlendMode::VividLight => Some(19),
        BlendMode::LinearLight => Some(20),
        BlendMode::PinLight => Some(21),
        BlendMode::Disable => Some(22),
        BlendMode::Replace => Some(23),
        BlendMode::Tint => Some(24),
        BlendMode::Saturation => Some(25),
        BlendMode::Color => Some(26),
        BlendMode::Value => Some(27),
        BlendMode::NormalCombine => Some(28),
        BlendMode::NormalDetail => Some(29),
        BlendMode::NormalInverseDetail => Some(30),
        BlendMode::Passthrough => None,
    }
}

/// One stack entry for the GPU composite: layer metadata plus its tile pixels.
///
/// Bottom-to-top order (index 0 is the bottom of the stack, matching
/// `umber_core::LayerStack`). Entries with `visible: false`, `texture: None`
/// (folders, instances, evicted tiles — anything with no resident pixels), or
/// a `Passthrough` blend mode contribute nothing and cost no dispatch.
pub struct GpuStackLayer<'a> {
    /// How this entry's texels combine with the accumulator below.
    pub blend_mode: BlendMode,
    /// Layer opacity (`0` = no effect); clamped, non-finite treated as `0`.
    pub opacity: f32,
    /// Invisible entries are skipped.
    pub visible: bool,
    /// This entry's tile texture, or `None` when it has no pixels.
    pub texture: Option<&'a PaintTarget>,
    /// This entry's layer-mask plane (`width × height` grayscale bytes at
    /// the OUTPUT dimensions, `255` = open), or `None` for an unmasked
    /// layer. `Some` on an entry with an enabled [`umber_core::LayerMask`];
    /// a disabled mask must pass `None` (it composites as if absent — see
    /// the module docs), never an all-white plane.
    pub mask: Option<&'a [u8]>,
    /// Premultiplied-alpha source: the entry's texels un-premultiply
    /// before the blend (module-docs judgment 10). Every v2 pool plane is
    /// premultiplied; `false` is the legacy straight-alpha composite.
    pub premultiplied: bool,
}

impl<'a> GpuStackLayer<'a> {
    /// Builds an entry from a [`Layer`] plus its already-looked-up pool tile.
    ///
    /// Folders and instances carry no pixels of their own — pass `None` for
    /// them (the composite treats that as transparent; see the module docs).
    /// The mask defaults to `None` (unmasked); set `.mask` afterwards when
    /// the layer carries an enabled [`umber_core::LayerMask`] with pixels.
    /// The source defaults to straight alpha (`premultiplied: false`); set
    /// the flag afterwards for pool planes.
    pub fn from_layer(layer: &Layer, texture: Option<&'a PaintTarget>) -> Self {
        Self {
            blend_mode: layer.blend_mode,
            opacity: layer.opacity,
            visible: layer.visible,
            texture,
            mask: None,
            premultiplied: false,
        }
    }
}

/// One stack entry for the CPU reference composite — same metadata as
/// [`GpuStackLayer`], but pixels as tightly-packed RGBA8 bytes
/// (`width × height × 4`, row-major) instead of a GPU texture.
pub struct CpuStackLayer<'a> {
    /// How this entry's texels combine with the accumulator below.
    pub blend_mode: BlendMode,
    /// Layer opacity (`0` = no effect); clamped, non-finite treated as `0`.
    pub opacity: f32,
    /// Invisible entries are skipped.
    pub visible: bool,
    /// This entry's texels, or `None` when it has no pixels.
    pub rgba: Option<&'a [u8]>,
    /// This entry's layer-mask plane (`width × height` grayscale bytes,
    /// `255` = open), or `None` for an unmasked layer — the CPU half of
    /// [`GpuStackLayer::mask`]'s contract.
    pub mask: Option<&'a [u8]>,
    /// Premultiplied-alpha source — the CPU half of
    /// [`GpuStackLayer::premultiplied`]'s contract.
    pub premultiplied: bool,
}

impl<'a> CpuStackLayer<'a> {
    /// Builds an entry from a [`Layer`] plus its already-looked-up pixel bytes.
    ///
    /// Folders and instances carry no pixels of their own — pass `None` for
    /// them (the composite treats that as transparent; see the module docs).
    /// The mask defaults to `None` (unmasked); set `.mask` afterwards when
    /// the layer carries an enabled [`umber_core::LayerMask`] with pixels.
    /// The source defaults to straight alpha (`premultiplied: false`); set
    /// the flag afterwards for pool planes.
    pub fn from_layer(layer: &Layer, rgba: Option<&'a [u8]>) -> Self {
        Self {
            blend_mode: layer.blend_mode,
            opacity: layer.opacity,
            visible: layer.visible,
            rgba,
            mask: None,
            premultiplied: false,
        }
    }
}

/// Errors from building or running the stack composite.
#[derive(Debug, thiserror::Error)]
pub enum CompositeError {
    /// `device` lacks `Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`,
    /// which the in-place `read_write` accumulator binding requires on
    /// `Rgba8Unorm` — the same floor as the paint pass (one device story).
    #[error(
        "device is missing wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, \
         required for read_write storage-texture access on Rgba8Unorm"
    )]
    MissingDeviceFeature,
    /// A [`CpuStackLayer`] carried a byte slice whose length is not
    /// `width × height × 4`. The oracle fails loud rather than compositing
    /// misaligned rows.
    #[error("layer pixel data has {found} bytes, expected {expected} (width * height * 4)")]
    InvalidPixelData {
        /// `width × height × 4`.
        expected: usize,
        /// The slice's actual length.
        found: usize,
    },
    /// A stack entry carried a layer-mask plane whose length is not
    /// `width × height`. Same fail-loud contract as
    /// [`CompositeError::InvalidPixelData`]: a misaligned mask must never
    /// silently gate the wrong texels.
    #[error("layer mask data has {found} bytes, expected {expected} (width * height)")]
    InvalidMaskData {
        /// `width × height`.
        expected: usize,
        /// The slice's actual length.
        found: usize,
    },
    /// [`StackCompositor::composite_stack_region`]'s transparent rect clear
    /// failed. The rect is clamped in-bounds before the upload, so this is
    /// unreachable short of device loss — kept typed so the region path
    /// never panics.
    #[error("region clear upload failed: {0}")]
    RegionClear(#[from] PaintError),
}

/// Clamps a linear-space channel to `0.0..=1.0`, mapping `NaN` to `0.0`.
///
/// Private mirror of `umber_core::BlendMode`'s own clamp (which is private
/// there): ALL non-finite input (NaN AND ±inf — the wave-1 battery caught
/// the inf leak cross-platform: `clamp(inf, 0, 1)` is 1.0 on the CPU and
/// diverges in WGSL stacks) becomes 0.0 so one bad texel cannot poison
/// the composite — fail-closed for every non-finite value.
fn clamp01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Clamps opacity to `0.0..=1.0`, treating non-finite input as `0.0` —
/// the same rule as `BlendMode::composite_channel` (which mirrors
/// `LayerStack::set_opacity`'s refusal to store non-finite input).
fn clamped_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Decodes one `Rgba8Unorm` byte to its linear float value.
pub fn u8_to_f32(b: u8) -> f32 {
    f32::from(b) / 255.0
}

/// Encodes one linear float channel to its `Rgba8Unorm` byte (round half
/// away from zero, then saturate — so out-of-range and non-finite inputs
/// still produce a byte instead of panicking or wrapping).
pub fn f32_to_u8(v: f32) -> u8 {
    let q = (v * 255.0).round();
    if q.is_nan() || q <= 0.0 {
        0
    } else if q >= 255.0 {
        255
    } else {
        q as u8
    }
}

/// Blends one RGBA texel: `src` over `dst` under `mode` at `opacity`.
///
/// Returns `None` exactly for [`BlendMode::Passthrough`] (structural, no
/// pixel op) — the same contract as `composite_channel`. Separable modes
/// follow the reference arms gated by coverage `src_alpha × opacity`
/// (module-docs judgment 2); triplet modes (HSV + normal-map families)
/// route through [`BlendMode::blend_rgb`] first and share the same gating
/// after that. Alpha follows straight-alpha over (judgment 3) under every
/// mode. For opaque sources the RGB result is bit-identical to a
/// per-channel `composite_channel` fold for separable modes (and to a
/// `blend_rgb` + opacity fold for triplet modes) — pinned by test, not
/// just claimed.
///
/// An unmasked texel: delegates to [`composite_pixel_masked`] with a fully
/// open mask, so unmasked stacks are bit-identical to the pre-mask
/// composite.)
pub fn composite_pixel(
    src: [f32; 4],
    dst: [f32; 4],
    mode: BlendMode,
    opacity: f32,
) -> Option<[f32; 4]> {
    composite_pixel_masked(src, dst, mode, opacity, 1.0)
}

/// [`composite_pixel`] with the layer-mask gate: `mask` (`0` = fully
/// gated, `1` = fully open — [`umber_core::mask_coverage`] maps the mask
/// plane's bytes here) multiplies the coverage, so `cov = src_alpha ×
/// opacity × mask` (module-docs judgment 9).
///
/// Non-finite `mask` clamps to `0.0` (gated shut) via the shared clamp —
/// a `NaN` mask must block paint, never leak it (fail-closed, the same
/// rule the quick mask applies off its plane).
///
/// The blend dispatch carries BOTH wave-1 kills: triplet modes (HSV +
/// normal-map families) route through [`BlendMode::blend_rgb`] first,
/// separable modes through the per-channel arms — then the mask gates
/// the shared coverage.
pub fn composite_pixel_masked(
    src: [f32; 4],
    dst: [f32; 4],
    mode: BlendMode,
    opacity: f32,
    mask: f32,
) -> Option<[f32; 4]> {
    if matches!(mode, BlendMode::Passthrough) {
        return None;
    }
    let [blend_r, blend_g, blend_b] = if mode.is_triplet() {
        mode.blend_rgb([src[0], src[1], src[2]], [dst[0], dst[1], dst[2]])?
    } else {
        [
            mode.blend_channel(src[0], dst[0])?,
            mode.blend_channel(src[1], dst[1])?,
            mode.blend_channel(src[2], dst[2])?,
        ]
    };
    let src_a = clamp01(src[3]);
    let base_r = clamp01(dst[0]);
    let base_g = clamp01(dst[1]);
    let base_b = clamp01(dst[2]);
    let base_a = clamp01(dst[3]);
    // Replace bypasses the layer opacity AND the source-alpha grading
    // (the variant doc's contract, mirrored by the WGSL gate and
    // umber-core's primitive): a present texel (any alpha > 0)
    // overwrites in full; a fully-transparent texel stays a no-op —
    // callers rely on cov == 0 there. The mask still clips (it is
    // coverage, not opacity).
    let (sa_gate, o_gate) = if matches!(mode, BlendMode::Replace) {
        (if src_a > 0.0 { 1.0 } else { 0.0 }, 1.0)
    } else {
        (src_a, clamped_opacity(opacity))
    };
    let cov = sa_gate * o_gate * clamp01(mask);
    // Alpha is straight-alpha-over (Porter-Duff), NOT a lerp: out_a must be
    // cov + base_a·(1−cov). The lerp (base_a + (src_a−base_a)·cov) coincides
    // with this when src is opaque (why the opaque-pins test passed) but
    // diverges at partial src alpha — half-alpha over opaque white must stay
    // opaque (1.0), and the lerp gave 0.75. CI's failing pin proved it.
    Some([
        base_r + (blend_r - base_r) * cov,
        base_g + (blend_g - base_g) * cov,
        base_b + (blend_b - base_b) * cov,
        cov + base_a * (1.0 - cov),
    ])
}

/// `CompositeParams.flags` bit: the sampled source is premultiplied alpha
/// (module-docs judgment 10). Higher bits are reserved.
const FLAG_PREMULTIPLIED: u32 = 1;

/// A texel rect for [`StackCompositor::composite_stack_region`] and
/// [`composite_stack_region_cpu`]: origin `(x, y)` plus `width × height`
/// extent, all in texels of the output tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeRect {
    /// Left edge, in texels.
    pub x: u32,
    /// Top edge, in texels.
    pub y: u32,
    /// Rect width, in texels.
    pub width: u32,
    /// Rect height, in texels.
    pub height: u32,
}

impl CompositeRect {
    /// Clamps the rect to an `output_width × output_height` tile
    /// (module-docs judgment 12): out-of-bounds edges clamp, and a rect
    /// fully outside (or empty) is `None` — a no-op, not an error.
    fn clamp(self, output_width: u32, output_height: u32) -> Option<Self> {
        let x = self.x.min(output_width);
        let y = self.y.min(output_height);
        let width = self.width.min(output_width.saturating_sub(x));
        let height = self.height.min(output_height.saturating_sub(y));
        if width == 0 || height == 0 {
            None
        } else {
            Some(Self {
                x,
                y,
                width,
                height,
            })
        }
    }
}

/// Fail-loud byte validation shared by [`composite_stack_cpu`] and
/// [`composite_stack_region_cpu`]: every entry's pixel and mask planes are
/// checked up front, even on entries the fold would skip (a malformed
/// oracle input must fail loud).
fn validate_stack_bytes(
    layers: &[CpuStackLayer<'_>],
    width: u32,
    height: u32,
) -> Result<(), CompositeError> {
    let len = width as usize * height as usize * 4;
    let mask_len = width as usize * height as usize;
    for layer in layers {
        if let Some(rgba) = layer.rgba {
            if rgba.len() != len {
                return Err(CompositeError::InvalidPixelData {
                    expected: len,
                    found: rgba.len(),
                });
            }
        }
        if let Some(mask) = layer.mask {
            if mask.len() != mask_len {
                return Err(CompositeError::InvalidMaskData {
                    expected: mask_len,
                    found: mask.len(),
                });
            }
        }
    }
    Ok(())
}

/// Un-premultiplies one decoded premultiplied texel (module-docs judgment
/// 10): `s = p / a` for `a > 0`, clamped to `0..=1` ("clamp `p` to `a`").
/// An `a == 0` texel zeroes its channels; the caller still folds it
/// through [`composite_pixel_masked`], whose `cov == 0` makes it a no-op.
fn unpremultiply_texel(src: [f32; 4]) -> [f32; 4] {
    let a = clamp01(src[3]);
    if a <= 0.0 {
        [0.0, 0.0, 0.0, src[3]]
    } else {
        [
            (src[0] / a).min(1.0),
            (src[1] / a).min(1.0),
            (src[2] / a).min(1.0),
            src[3],
        ]
    }
}

/// The bottom-copy identity rule (module-docs judgment 11): the first
/// contributing entry seeds the accumulator verbatim when it is
/// premultiplied, `Normal`, full-opacity and unmasked. Shared by the CPU
/// oracle and both GPU entry points so the rule fires identically on all
/// three paths.
fn is_bottom_copy(
    premultiplied: bool,
    blend_mode: BlendMode,
    opacity: f32,
    masked: bool,
    contributed: bool,
) -> bool {
    !contributed
        && premultiplied
        && blend_mode == BlendMode::Normal
        && clamped_opacity(opacity) == 1.0
        && !masked
}

/// Copies `rect`'s bytes out of a full-tile `tile_width`-wide mask plane
/// into a fresh rect-sized plane (module-docs judgment 12): the region
/// path uploads only `|R|` mask bytes. The caller validates the plane
/// length first ([`CompositeError::InvalidMaskData`]); `rect` arrives
/// clamped to the tile, so every row slice is in-bounds.
fn mask_rect_bytes(mask: &[u8], tile_width: u32, rect: CompositeRect) -> Vec<u8> {
    let stride = tile_width as usize;
    let mut out = Vec::with_capacity(rect.width as usize * rect.height as usize);
    for dy in 0..rect.height as usize {
        let row = (rect.y as usize + dy) * stride + rect.x as usize;
        out.extend_from_slice(&mask[row..row + rect.width as usize]);
    }
    out
}

/// The CPU reference composite: folds `layers` bottom-to-top over a
/// transparent accumulator and returns tightly-packed RGBA8 bytes.
///
/// This is the test oracle for [`StackCompositor::composite_stack`] — the
/// GPU path must reproduce these bytes to ±1 LSB (see module-docs judgment
/// 8). Skips invisible entries, pixel-less entries, and `Passthrough`
/// entries; anything else with a wrong-length slice is
/// [`CompositeError::InvalidPixelData`], even when invisible (a malformed
/// oracle input must fail loud).
pub fn composite_stack_cpu(
    layers: &[CpuStackLayer<'_>],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CompositeError> {
    validate_stack_bytes(layers, width, height)?;
    let len = width as usize * height as usize * 4;
    let mut acc = vec![0.0f32; len];
    // Whether any earlier entry folded: the bottom-copy seed fires only at
    // the true bottom (module-docs judgment 11).
    let mut contributed = false;
    for layer in layers {
        if !layer.visible {
            continue;
        }
        if matches!(layer.blend_mode, BlendMode::Passthrough) {
            continue;
        }
        let rgba = match layer.rgba {
            Some(rgba) => rgba,
            None => continue,
        };
        // A mask on a skipped entry is never read (see the module docs):
        // the `mask` lookup sits inside the contributing path only.
        let mask = layer.mask;
        let seed = is_bottom_copy(
            layer.premultiplied,
            layer.blend_mode,
            layer.opacity,
            mask.is_some(),
            contributed,
        );
        contributed = true;
        if seed {
            // Verbatim seed: `acc = byte / 255` per texel, which
            // `f32_to_u8` quantizes back to the same byte.
            for (i, px) in rgba.chunks_exact(4).enumerate() {
                let base = i * 4;
                acc[base] = u8_to_f32(px[0]);
                acc[base + 1] = u8_to_f32(px[1]);
                acc[base + 2] = u8_to_f32(px[2]);
                acc[base + 3] = u8_to_f32(px[3]);
            }
            continue;
        }
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let mut src = [
                u8_to_f32(px[0]),
                u8_to_f32(px[1]),
                u8_to_f32(px[2]),
                u8_to_f32(px[3]),
            ];
            if layer.premultiplied {
                src = unpremultiply_texel(src);
            }
            let m = mask.map_or(1.0, |plane| u8_to_f32(plane[i]));
            let base = i * 4;
            let dst = [acc[base], acc[base + 1], acc[base + 2], acc[base + 3]];
            if let Some(out) = composite_pixel_masked(src, dst, layer.blend_mode, layer.opacity, m)
            {
                acc[base..base + 4].copy_from_slice(&out);
            }
        }
    }
    Ok(acc.iter().map(|v| f32_to_u8(*v)).collect())
}

/// The CPU reference for [`StackCompositor::composite_stack_region`]:
/// folds `layers` bottom-to-top over a transparent accumulator exactly
/// like [`composite_stack_cpu`], but only for `rect`'s texels, and returns
/// only `rect`'s bytes (`rect.width × rect.height × 4`, row-major).
///
/// The per-texel fold is the same code shape as [`composite_stack_cpu`]
/// (same decode, same un-premultiply, same seed rule, one quantization at
/// the end), so the region bytes equal the full composite's same rect
/// EXACTLY — pinned by test, not just claimed. The rect clamps to the
/// tile (`CompositeRect`'s clamp rule, documented on the type); a
/// fully-outside rect returns empty bytes.
pub fn composite_stack_region_cpu(
    layers: &[CpuStackLayer<'_>],
    width: u32,
    height: u32,
    rect: CompositeRect,
) -> Result<Vec<u8>, CompositeError> {
    validate_stack_bytes(layers, width, height)?;
    let clamped = match rect.clamp(width, height) {
        Some(clamped) => clamped,
        None => return Ok(Vec::new()),
    };
    let stride = width as usize;
    let rw = clamped.width as usize;
    let rh = clamped.height as usize;
    let mut acc = vec![0.0f32; rw * rh * 4];
    let mut contributed = false;
    for layer in layers {
        if !layer.visible {
            continue;
        }
        if matches!(layer.blend_mode, BlendMode::Passthrough) {
            continue;
        }
        let rgba = match layer.rgba {
            Some(rgba) => rgba,
            None => continue,
        };
        let mask = layer.mask;
        let seed = is_bottom_copy(
            layer.premultiplied,
            layer.blend_mode,
            layer.opacity,
            mask.is_some(),
            contributed,
        );
        contributed = true;
        for dy in 0..rh {
            for dx in 0..rw {
                let si = ((clamped.y as usize + dy) * stride + clamped.x as usize + dx) * 4;
                let di = (dy * rw + dx) * 4;
                if seed {
                    acc[di] = u8_to_f32(rgba[si]);
                    acc[di + 1] = u8_to_f32(rgba[si + 1]);
                    acc[di + 2] = u8_to_f32(rgba[si + 2]);
                    acc[di + 3] = u8_to_f32(rgba[si + 3]);
                    continue;
                }
                let mut src = [
                    u8_to_f32(rgba[si]),
                    u8_to_f32(rgba[si + 1]),
                    u8_to_f32(rgba[si + 2]),
                    u8_to_f32(rgba[si + 3]),
                ];
                if layer.premultiplied {
                    src = unpremultiply_texel(src);
                }
                // The mask plane stays full-tile on the oracle input; the
                // texel index is the byte index over 4.
                let m = mask.map_or(1.0, |plane| u8_to_f32(plane[si / 4]));
                let dst = [acc[di], acc[di + 1], acc[di + 2], acc[di + 3]];
                if let Some(out) =
                    composite_pixel_masked(src, dst, layer.blend_mode, layer.opacity, m)
                {
                    acc[di..di + 4].copy_from_slice(&out);
                }
            }
        }
    }
    Ok(acc.iter().map(|v| f32_to_u8(*v)).collect())
}

/// Per-dispatch uniforms for [`COMPOSITE_WGSL`]: 32 bytes, 4-byte aligned
/// throughout, so the `#[repr(C)]` layout matches the WGSL struct with no
/// padding surprises (`mask_enabled` + `flags` + `origin_x` + `origin_y`
/// keep the 16-byte WGSL alignment after the first four words).
///
/// Stage-1a packing (per-layer-pool design §4.3, module-docs judgments
/// 10–12): `flags` bit 0 is the premultiplied-source flag;
/// `origin_x`/`origin_y` are the dispatch origin (`0` on the full path);
/// `width`/`height` are the dispatch extent (the output dims on the full
/// path, the rect extent on the region path).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeParams {
    mode: u32,
    opacity: f32,
    width: u32,
    height: u32,
    mask_enabled: u32,
    flags: u32,
    origin_x: u32,
    origin_y: u32,
}

const _: () = assert!(
    size_of::<CompositeParams>() == 32,
    "CompositeParams must stay two 16-byte uniform chunks"
);

/// The GPU stack compositor: one in-place compute dispatch per contributing
/// layer, folding the stack bottom-to-top into `output`.
///
/// Build once per device with [`StackCompositor::new`], then call
/// [`StackCompositor::composite_stack`] per dirty tile. All dispatches for
/// one call are recorded on the caller's encoder in stack order — wgpu
/// orders successive compute passes on the same texture, so each layer
/// always reads the accumulator the previous layer wrote.
pub struct StackCompositor {
    device: wgpu::Device,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// The 1x1 mask bound for an entry with no layer mask (binding 3 must
    /// always hold a view; unmasked dispatches set `mask_enabled = 0` so
    /// they never sample it — the paint pass's fallback pattern).
    _fallback_mask: wgpu::Texture,
    fallback_mask_view: wgpu::TextureView,
}

impl StackCompositor {
    /// Builds the composite pipeline against `device`.
    ///
    /// # Errors
    ///
    /// Returns [`CompositeError::MissingDeviceFeature`] if `device` wasn't
    /// created with `Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` —
    /// checked up front so this fails with a typed error instead of a wgpu
    /// validation panic partway through building the bind-group layout.
    pub fn new(device: wgpu::Device) -> Result<Self, CompositeError> {
        if !device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            return Err(CompositeError::MissingDeviceFeature);
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_stack_composite_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(COMPOSITE_WGSL)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_stack_composite_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadWrite,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<CompositeParams>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // The unmasked fallback (see the field docs): a 1x1 `R8Unorm`
        // texture, the same format the per-dispatch uploads use. Its texel
        // value is never read (`mask_enabled = 0` skips the sample).
        let fallback_mask = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_stack_composite_mask_fallback"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let fallback_mask_view = fallback_mask.create_view(&wgpu::TextureViewDescriptor::default());

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("umber_stack_composite_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("umber_stack_composite_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some(COMPOSITE_ENTRY),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(Self {
            device,
            pipeline,
            bind_group_layout,
            _fallback_mask: fallback_mask,
            fallback_mask_view,
        })
    }

    /// Uploads one layer-mask plane (`width × height` grayscale bytes) into
    /// a fresh `R8Unorm` texture at the output dimensions, returning the
    /// texture and its view (the caller keeps both alive until submit —
    /// see [`Self::composite_stack`]).
    ///
    /// Rows are padded to a 256-byte pitch (the `WriteMask` rule from the
    /// paint pass — `copy_buffer_to_texture` requires it); the staging
    /// buffer is built per dispatch like the uniforms, so masked layers
    /// cost one upload each per composite.
    fn upload_mask(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_stack_composite_mask"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let padding = (256 - (width % 256)) % 256;
        let padded_row = width + padding;
        let mut padded = Vec::with_capacity(padded_row as usize * height as usize);
        for row in bytes.chunks_exact(width as usize) {
            padded.extend_from_slice(row);
            padded.resize(padded.len() + padding as usize, 0);
        }
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_stack_composite_mask_upload"),
                contents: &padded,
                usage: wgpu::BufferUsages::COPY_SRC,
            });
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Records the composite of `layers` (bottom-to-top) into `output`.
    ///
    /// `output` is cleared to transparent first, then every contributing
    /// entry (visible, pixel-bearing, non-`Passthrough`) blends over the
    /// accumulator in place, in order. Non-contributing entries cost no
    /// dispatch. A first contributing entry that is premultiplied,
    /// `Normal`, full-opacity and unmasked seeds the output by verbatim
    /// copy instead of dispatching (module-docs judgment 11). `output`
    /// must not alias any entry texture (see the module docs); entries
    /// SHOULD match `output`'s dimensions (out-of-range reads are
    /// transparent, never an error).
    ///
    /// A contributing entry with `mask: Some` uploads its plane per
    /// dispatch into an `R8Unorm` texture at the output dimensions (a
    /// staging buffer plus `copy_buffer_to_texture` on the caller's
    /// encoder — the `WriteMask` row-padding rule, 256-byte pitch) and
    /// sets `mask_enabled`; wrong-length planes are
    /// [`CompositeError::InvalidMaskData`], even when the entry would
    /// otherwise contribute. Unmasked entries bind the 1x1 fallback with
    /// `mask_enabled = 0`.
    pub fn composite_stack(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        layers: &[GpuStackLayer<'_>],
        output: &PaintTarget,
    ) -> Result<(), CompositeError> {
        output.clear(encoder);
        let (width, height) = output.dimensions();
        if width == 0 || height == 0 {
            return Ok(());
        }
        // Per-dispatch mask textures: created here, referenced by the
        // recorded passes. Dropping them at the end of this call is safe —
        // the encoder holds the GPU-side refs (the paint pass's per-call
        // dab buffer follows the same rule).
        let mut owned_masks: Vec<(wgpu::Texture, wgpu::TextureView)> = Vec::new();
        // Whether any earlier entry folded: the bottom-copy seed fires only
        // at the true bottom (module-docs judgment 11).
        let mut contributed = false;
        for layer in layers {
            if !layer.visible {
                continue;
            }
            let texture = match layer.texture {
                Some(texture) => texture,
                None => continue,
            };
            let mode = match blend_mode_id(layer.blend_mode) {
                Some(mode) => mode,
                None => continue,
            };
            let seed = is_bottom_copy(
                layer.premultiplied,
                layer.blend_mode,
                layer.opacity,
                layer.mask.is_some(),
                contributed,
            );
            contributed = true;
            if seed {
                Self::seed_copy(
                    encoder,
                    texture,
                    output,
                    CompositeRect {
                        x: 0,
                        y: 0,
                        width,
                        height,
                    },
                );
                continue;
            }
            let (mask_view, mask_enabled) = match layer.mask {
                Some(bytes) => {
                    let expected = width as usize * height as usize;
                    if bytes.len() != expected {
                        return Err(CompositeError::InvalidMaskData {
                            expected,
                            found: bytes.len(),
                        });
                    }
                    owned_masks.push(self.upload_mask(encoder, bytes, width, height));
                    let view = &owned_masks.last().expect("just pushed").1;
                    (view, 1u32)
                }
                None => (&self.fallback_mask_view, 0u32),
            };
            let params = CompositeParams {
                mode,
                opacity: layer.opacity,
                width,
                height,
                mask_enabled,
                flags: if layer.premultiplied {
                    FLAG_PREMULTIPLIED
                } else {
                    0
                },
                origin_x: 0,
                origin_y: 0,
            };
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("umber_stack_composite_params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("umber_stack_composite_bind_group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(texture.view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(output.view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(mask_view),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("umber_stack_composite_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
            }
        }
        Ok(())
    }

    /// Records the composite of `layers` (bottom-to-top) into `output`'s
    /// `rect` only — the per-event path (module-docs judgment 12).
    ///
    /// The rect is cleared to transparent first, then every contributing
    /// entry folds over it exactly like [`Self::composite_stack`]: same
    /// skip filters, same bottom-copy seed (which fires per call, so a
    /// region composite seeds its own rect), same premultiplied flag.
    /// Texels outside `rect` are never stored to. A masked entry uploads
    /// only the rect's mask bytes into a rect-sized texture, sampled at
    /// `texel − origin` by the shared kernel. Out-of-bounds rect edges
    /// clamp to the output and a fully-outside rect is a no-op.
    ///
    /// Like [`Self::composite_stack`], `output` must not alias any entry
    /// texture, and entries SHOULD match `output`'s dimensions.
    pub fn composite_stack_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        layers: &[GpuStackLayer<'_>],
        output: &PaintTarget,
        rect: CompositeRect,
    ) -> Result<(), CompositeError> {
        let (width, height) = output.dimensions();
        let clamped = match rect.clamp(width, height) {
            Some(clamped) => clamped,
            None => return Ok(()),
        };
        // Clear ONLY the rect, as transparent bytes through the retouch
        // path's proven `write_region` shape (recorded on the caller's
        // encoder, so it serializes ahead of the dispatches below): the
        // full path's whole-output clear would wipe texels outside it.
        let zeros = vec![0u8; clamped.width as usize * clamped.height as usize * 4];
        output.write_region(
            &self.device,
            encoder,
            (clamped.x, clamped.y),
            (clamped.width, clamped.height),
            &zeros,
        )?;
        let mut owned_masks: Vec<(wgpu::Texture, wgpu::TextureView)> = Vec::new();
        let mut contributed = false;
        for layer in layers {
            if !layer.visible {
                continue;
            }
            let texture = match layer.texture {
                Some(texture) => texture,
                None => continue,
            };
            let mode = match blend_mode_id(layer.blend_mode) {
                Some(mode) => mode,
                None => continue,
            };
            let seed = is_bottom_copy(
                layer.premultiplied,
                layer.blend_mode,
                layer.opacity,
                layer.mask.is_some(),
                contributed,
            );
            contributed = true;
            if seed {
                Self::seed_copy(encoder, texture, output, clamped);
                continue;
            }
            let (mask_view, mask_enabled) = match layer.mask {
                Some(bytes) => {
                    let expected = width as usize * height as usize;
                    if bytes.len() != expected {
                        return Err(CompositeError::InvalidMaskData {
                            expected,
                            found: bytes.len(),
                        });
                    }
                    let rect_bytes = mask_rect_bytes(bytes, width, clamped);
                    owned_masks.push(self.upload_mask(
                        encoder,
                        &rect_bytes,
                        clamped.width,
                        clamped.height,
                    ));
                    let view = &owned_masks.last().expect("just pushed").1;
                    (view, 1u32)
                }
                None => (&self.fallback_mask_view, 0u32),
            };
            let params = CompositeParams {
                mode,
                opacity: layer.opacity,
                width: clamped.width,
                height: clamped.height,
                mask_enabled,
                flags: if layer.premultiplied {
                    FLAG_PREMULTIPLIED
                } else {
                    0
                },
                origin_x: clamped.x,
                origin_y: clamped.y,
            };
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("umber_stack_composite_region_params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("umber_stack_composite_region_bind_group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(texture.view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(output.view()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(mask_view),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("umber_stack_composite_region_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(clamped.width.div_ceil(8), clamped.height.div_ceil(8), 1);
            }
        }
        Ok(())
    }

    /// Verbatim seed copy for the bottom-copy rule (module-docs judgment
    /// 11): copies `rect`'s overlap between the entry's tile (`src`) and
    /// the output (`dst`) at the same origin. Exact by construction — no
    /// UNORM round-trip. An undersized source seeds its overlap and leaves
    /// the rest as cleared (judgment 7's zero-fill: both callers clear
    /// first). Never panics: the overlap clamps to both textures, and an
    /// empty overlap is a no-op (`copy_texture_to_texture` panics on an
    /// overrun, so the clamp is load-bearing, not advisory).
    fn seed_copy(
        encoder: &mut wgpu::CommandEncoder,
        src: &PaintTarget,
        dst: &PaintTarget,
        rect: CompositeRect,
    ) {
        let (sw, sh) = src.dimensions();
        let (dw, dh) = dst.dimensions();
        let sx = rect.x.min(sw);
        let sy = rect.y.min(sh);
        let dx = rect.x.min(dw);
        let dy = rect.y.min(dh);
        let w = rect
            .width
            .min(sw.saturating_sub(sx))
            .min(dw.saturating_sub(dx));
        let h = rect
            .height
            .min(sh.saturating_sub(sy))
            .min(dh.saturating_sub(dy));
        if w == 0 || h == 0 {
            return;
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: src.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x: sx, y: sy, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: dst.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x: dx, y: dy, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::{LayerKind, LayerStack};

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const RED: [u8; 4] = [255, 0, 0, 255];
    const CYAN: [u8; 4] = [0, 255, 255, 255];
    const GRAY128: [u8; 4] = [128, 128, 128, 255];
    const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

    /// One bottom + one top texel through the CPU oracle at 1×1.
    fn two_layer(top: [u8; 4], mode: BlendMode, opacity: f32, bottom: [u8; 4]) -> Vec<u8> {
        let top_bytes = top.to_vec();
        let bottom_bytes = bottom.to_vec();
        let layers = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(bottom_bytes.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: mode,
                opacity,
                visible: true,
                rgba: Some(top_bytes.as_slice()),
                mask: None,
                premultiplied: false,
            },
        ];
        composite_stack_cpu(&layers, 1, 1).expect("1x1 oracle input is well-formed")
    }

    /// One bottom + one masked top texel through the CPU oracle at 1×1.
    fn two_layer_masked(
        top: [u8; 4],
        mode: BlendMode,
        opacity: f32,
        bottom: [u8; 4],
        mask: Option<&[u8]>,
    ) -> Vec<u8> {
        let top_bytes = top.to_vec();
        let bottom_bytes = bottom.to_vec();
        let layers = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(bottom_bytes.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: mode,
                opacity,
                visible: true,
                rgba: Some(top_bytes.as_slice()),
                mask,
                premultiplied: false,
            },
        ];
        composite_stack_cpu(&layers, 1, 1).expect("1x1 oracle input is well-formed")
    }

    #[test]
    fn blend_mode_ids_are_pinned() {
        // WGSL `BLEND_*` consts must equal these, in this order; a mode
        // added to the enum without a shader arm breaks here first.
        let cases = [
            (BlendMode::Normal, 0),
            (BlendMode::Multiply, 1),
            (BlendMode::Screen, 2),
            (BlendMode::Overlay, 3),
            (BlendMode::Darken, 4),
            (BlendMode::Lighten, 5),
            (BlendMode::Add, 6),
            (BlendMode::Subtract, 7),
            (BlendMode::Difference, 8),
            (BlendMode::SoftLight, 9),
            (BlendMode::HardLight, 10),
            (BlendMode::ColorDodge, 11),
            (BlendMode::ColorBurn, 12),
            (BlendMode::LinearBurn, 13),
            (BlendMode::Divide, 14),
            (BlendMode::InverseDivide, 15),
            (BlendMode::InverseSubtract, 16),
            (BlendMode::Exclusion, 17),
            (BlendMode::AddSub, 18),
            (BlendMode::VividLight, 19),
            (BlendMode::LinearLight, 20),
            (BlendMode::PinLight, 21),
            (BlendMode::Disable, 22),
            (BlendMode::Replace, 23),
            (BlendMode::Tint, 24),
            (BlendMode::Saturation, 25),
            (BlendMode::Color, 26),
            (BlendMode::Value, 27),
            (BlendMode::NormalCombine, 28),
            (BlendMode::NormalDetail, 29),
            (BlendMode::NormalInverseDetail, 30),
        ];
        for (mode, id) in cases {
            assert_eq!(blend_mode_id(mode), Some(id), "{mode:?}");
        }
        // Passthrough is structural, never a shader id.
        assert_eq!(blend_mode_id(BlendMode::Passthrough), None);
    }

    #[test]
    fn all_fourteen_modes_pin_exact_bytes() {
        // Endpoint-driven spots where the f32 math is exact (verified by
        // hand against blend_channel; the byte round-trips are exact for
        // all 256 values — see the roundtrip check in the kill notes).
        let cases = [
            (RED, BlendMode::Normal, WHITE, RED),
            (WHITE, BlendMode::Multiply, RED, RED),
            (BLACK, BlendMode::Multiply, WHITE, BLACK),
            (BLACK, BlendMode::Screen, RED, RED),
            (WHITE, BlendMode::Screen, RED, WHITE),
            (RED, BlendMode::Overlay, WHITE, WHITE),
            (RED, BlendMode::Overlay, BLACK, BLACK),
            (RED, BlendMode::Darken, WHITE, RED),
            (RED, BlendMode::Lighten, WHITE, WHITE),
            (RED, BlendMode::Add, RED, RED),
            (WHITE, BlendMode::Add, WHITE, WHITE),
            (BLACK, BlendMode::Subtract, WHITE, WHITE),
            (WHITE, BlendMode::Subtract, RED, BLACK),
            (RED, BlendMode::Difference, RED, BLACK),
            (WHITE, BlendMode::Difference, BLACK, WHITE),
            (WHITE, BlendMode::SoftLight, WHITE, WHITE),
            (BLACK, BlendMode::SoftLight, BLACK, BLACK),
            (WHITE, BlendMode::HardLight, RED, WHITE),
            (BLACK, BlendMode::HardLight, RED, BLACK),
            (WHITE, BlendMode::ColorDodge, RED, WHITE),
            (BLACK, BlendMode::ColorDodge, RED, RED),
            (BLACK, BlendMode::ColorBurn, WHITE, BLACK),
            (WHITE, BlendMode::ColorBurn, WHITE, WHITE),
            (BLACK, BlendMode::LinearBurn, WHITE, BLACK),
            (WHITE, BlendMode::LinearBurn, WHITE, WHITE),
        ];
        for (top, mode, bottom, want) in cases {
            assert_eq!(two_layer(top, mode, 1.0, bottom), want.to_vec(), "{mode:?}");
        }
    }

    #[test]
    fn all_modes_pin_exact_dyadic_pixels() {
        // s = 0.5 over d = 0.25, opaque, full opacity: every value below
        // is an exact binary fraction, so f32 == the hand arithmetic.
        // (Mid-gray blend is a no-op for Vivid/Linear/Pin/AddSub, which is
        // why four rows share 0.25 here — the dedicated layers.rs tests pin
        // their off-center behavior.)
        let cases = [
            (BlendMode::Normal, 0.5),
            (BlendMode::Multiply, 0.125),
            (BlendMode::Screen, 0.625),
            (BlendMode::Overlay, 0.25),
            (BlendMode::Darken, 0.25),
            (BlendMode::Lighten, 0.5),
            (BlendMode::Add, 0.75),
            (BlendMode::Subtract, 0.0),
            (BlendMode::Difference, 0.25),
            (BlendMode::SoftLight, 0.25),
            (BlendMode::HardLight, 0.25),
            (BlendMode::ColorDodge, 0.5),
            (BlendMode::ColorBurn, 0.0),
            (BlendMode::LinearBurn, 0.0),
            (BlendMode::Divide, 0.5),
            (BlendMode::InverseDivide, 1.0),
            (BlendMode::InverseSubtract, 0.25),
            (BlendMode::Exclusion, 0.5),
            (BlendMode::AddSub, 0.25),
            (BlendMode::VividLight, 0.25),
            (BlendMode::LinearLight, 0.25),
            (BlendMode::PinLight, 0.25),
            (BlendMode::Disable, 0.25),
            (BlendMode::Replace, 0.5),
        ];
        for (mode, want) in cases {
            assert_eq!(
                composite_pixel([0.5, 0.5, 0.5, 1.0], [0.25, 0.25, 0.25, 1.0], mode, 1.0),
                Some([want, want, want, 1.0]),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn new_separable_modes_pin_exact_bytes() {
        // Endpoint-driven spots for the ten wave-blends separable modes:
        // every byte below is hand-derived from blend_channel (0/1 inputs
        // round-trip exactly; the 0.5 AddSub row pins the 127.5→128
        // half-away rounding, same as the opacity-ramp test).
        let cases = [
            (RED, BlendMode::Divide, WHITE, WHITE),
            (BLACK, BlendMode::Divide, RED, WHITE),
            (RED, BlendMode::InverseDivide, WHITE, RED),
            (WHITE, BlendMode::InverseSubtract, RED, CYAN),
            (RED, BlendMode::Exclusion, RED, BLACK),
            (WHITE, BlendMode::Exclusion, BLACK, WHITE),
            (BLACK, BlendMode::AddSub, WHITE, GRAY128),
            // VividLight(RED over WHITE): the R channel s=1 dodges to
            // white, G/B s=0 burns to black -> [255, 0, 0] = RED. (The
            // claw's first pin expected WHITE — a hand-derivation miss
            // caught by the engine at the dragon gate; this comment IS
            // the derivation.)
            (RED, BlendMode::VividLight, WHITE, RED),
            (BLACK, BlendMode::VividLight, WHITE, BLACK),
            (RED, BlendMode::LinearLight, BLACK, RED),
            (BLACK, BlendMode::LinearLight, WHITE, BLACK),
            // PinLight(RED over WHITE): R s=1 -> max(1, 1.0)=255; G/B
            // s=0 -> min(1, 0)=0 -> RED (same derivation-miss class as
            // VividLight above — engine correct, pin corrected).
            (RED, BlendMode::PinLight, WHITE, RED),
            (BLACK, BlendMode::PinLight, RED, BLACK),
            (RED, BlendMode::Disable, WHITE, WHITE),
            (RED, BlendMode::Replace, WHITE, RED),
        ];
        for (top, mode, bottom, want) in cases {
            assert_eq!(two_layer(top, mode, 1.0, bottom), want.to_vec(), "{mode:?}");
        }
    }

    #[test]
    fn replace_bypasses_opacity_and_alpha_grading() {
        // The variant doc's contract: Replace ignores the layer opacity
        // AND the source-alpha grading — a present texel overwrites in
        // full; only a fully-transparent texel is a no-op. The mask
        // still clips (it is coverage, not opacity).
        let blue = [0.0, 0.0, 1.0, 1.0];
        // Opacity 0 does not mute a Replace layer.
        assert_eq!(
            composite_pixel([1.0, 0.0, 0.0, 1.0], blue, BlendMode::Replace, 0.0),
            Some([1.0, 0.0, 0.0, 1.0]),
            "opacity 0 must not mute a Replace layer"
        );
        // Partial source alpha overwrites in full (the grading bypass).
        assert_eq!(
            composite_pixel([1.0, 0.0, 0.0, 0.25], blue, BlendMode::Replace, 0.5),
            Some([1.0, 0.0, 0.0, 1.0]),
            "cov = presence × mask — alpha 0.25 still overwrites wholly"
        );
        // A fully-transparent texel is still a no-op.
        assert_eq!(
            composite_pixel([1.0, 0.0, 0.0, 0.0], blue, BlendMode::Replace, 1.0),
            Some([0.0, 0.0, 1.0, 1.0]),
            "alpha 0 never overwrites"
        );
        // The mask still clips a Replace layer.
        assert_eq!(
            composite_pixel_masked([1.0, 0.0, 0.0, 1.0], blue, BlendMode::Replace, 0.0, 0.5),
            Some([0.5, 0.0, 0.5, 1.0]),
            "the mask gate survives the bypass"
        );
    }

    #[test]
    fn triplet_modes_pin_exact_rgb() {
        // One exact RGB anchor per triplet mode through composite_pixel
        // (opaque, full opacity): primaries round-trip exactly, and the
        // opacity rows pin the shared coverage gating on the triplet path.
        let cases = [
            (
                [1.0, 0.0, 0.0, 1.0],
                BlendMode::Tint,
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
            ),
            (
                [1.0, 0.0, 0.0, 1.0],
                BlendMode::Saturation,
                [0.5, 0.5, 0.5, 1.0],
                [0.5, 0.0, 0.0, 1.0],
            ),
            (
                [0.0, 1.0, 0.0, 1.0],
                BlendMode::Color,
                [0.0, 0.0, 0.5, 1.0],
                [0.0, 0.5, 0.0, 1.0],
            ),
            (
                [0.0, 0.5, 0.0, 1.0],
                BlendMode::Value,
                [1.0, 0.0, 0.0, 1.0],
                [0.5, 0.0, 0.0, 1.0],
            ),
            (
                [0.5, 0.5, 1.0, 1.0],
                BlendMode::NormalCombine,
                [0.5, 0.5, 1.0, 1.0],
                [0.5, 0.5, 1.0, 1.0],
            ),
            (
                [0.5, 0.5, 1.0, 1.0],
                BlendMode::NormalDetail,
                [0.5, 0.5, 1.0, 1.0],
                [0.5, 0.5, 1.0, 1.0],
            ),
            (
                [0.5, 0.5, 1.0, 1.0],
                BlendMode::NormalInverseDetail,
                [0.5, 0.5, 1.0, 1.0],
                [0.5, 0.5, 1.0, 1.0],
            ),
        ];
        for (src, mode, dst, want) in cases {
            assert_eq!(composite_pixel(src, dst, mode, 1.0), Some(want), "{mode:?}");
        }
        // Opacity gating is mode-agnostic: Tint at o=0 keeps the base, at
        // o=0.5 mixes exactly halfway (blend red, base blue).
        assert_eq!(
            composite_pixel(
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                BlendMode::Tint,
                0.0
            ),
            Some([0.0, 0.0, 1.0, 1.0])
        );
        assert_eq!(
            composite_pixel(
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                BlendMode::Tint,
                0.5
            ),
            Some([0.5, 0.0, 0.5, 1.0])
        );
    }

    #[test]
    fn triplet_oracle_matches_blend_rgb_fold() {
        // composite_pixel's RGB MUST equal a blend_rgb + opacity fold for
        // opaque sources under triplet modes: same calls, same op order —
        // the triplet twin of the separable naive-fold test below. Alpha
        // follows straight-alpha over, pinned to its own formula.
        let modes = [
            BlendMode::Tint,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Value,
            BlendMode::NormalCombine,
            BlendMode::NormalDetail,
            BlendMode::NormalInverseDetail,
        ];
        for mode in modes {
            for s in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for d in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    for o in [0.0, 0.5, 1.0] {
                        let src = [s, s, s, 1.0];
                        let dst = [d, d, d, 0.5];
                        let got = composite_pixel(src, dst, mode, o).expect("non-passthrough");
                        let b = mode.blend_rgb([s, s, s], [d, d, d]).expect("triplet op");
                        assert_eq!(
                            [got[0], got[1], got[2]],
                            [d + (b[0] - d) * o, d + (b[1] - d) * o, d + (b[2] - d) * o],
                            "{mode:?} s={s} d={d} o={o}"
                        );
                        // Alpha: da + (1 - da) * o, independent of mode.
                        assert_eq!(got[3], 0.5 + (1.0 - 0.5) * o, "{mode:?} alpha o={o}");
                    }
                }
            }
        }
    }

    #[test]
    fn opaque_pixels_match_the_naive_reference_fold() {
        // composite_pixel's RGB MUST equal a per-channel composite_channel
        // fold for opaque sources: same calls, same op order (see the
        // module docs). Alpha follows straight-alpha over instead — opaque
        // src over anything is opaque — so it is pinned to its own formula.
        let modes = [
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
        for mode in modes {
            for s in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for d in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    for o in [0.0, 0.5, 1.0] {
                        let src = [s, s, s, 1.0];
                        let dst = [d, d, d, 0.5];
                        let got = composite_pixel(src, dst, mode, o).expect("non-passthrough");
                        let naive = mode.composite_channel(s, d, o).expect("non-passthrough");
                        assert_eq!(
                            [got[0], got[1], got[2]],
                            [naive, naive, naive],
                            "{mode:?} s={s} d={d} o={o}"
                        );
                        // Alpha: da + (1 - da) * o, independent of
                        // mode — EXCEPT Replace, whose opacity bypass
                        // replaces the pixel wholesale (alpha = the
                        // source's sa; the blend-replace-opacity
                        // kill's contract, pinned by
                        // replace_bypasses_opacity_and_alpha_grading).
                        let expected_a = if matches!(mode, BlendMode::Replace) {
                            src[3]
                        } else {
                            0.5 + (1.0 - 0.5) * o
                        };
                        assert_eq!(got[3], expected_a, "{mode:?} alpha o={o}");
                    }
                }
            }
        }
    }

    #[test]
    fn opacity_endpoints_and_clamping_are_exact() {
        // o = 1 reproduces the source, o = 0 the base — exactly.
        assert_eq!(
            composite_pixel(
                [1.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                BlendMode::Normal,
                1.0
            ),
            Some([1.0, 0.0, 0.0, 1.0])
        );
        assert_eq!(
            composite_pixel(
                [1.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                BlendMode::Normal,
                0.0
            ),
            Some([1.0, 1.0, 1.0, 1.0])
        );
        // Out-of-range opacity clamps; non-finite opacity is base-unchanged.
        for o in [5.0, 1.5] {
            assert_eq!(
                composite_pixel(
                    [1.0, 0.0, 0.0, 1.0],
                    [0.25, 0.5, 0.75, 1.0],
                    BlendMode::Multiply,
                    o
                ),
                composite_pixel(
                    [1.0, 0.0, 0.0, 1.0],
                    [0.25, 0.5, 0.75, 1.0],
                    BlendMode::Multiply,
                    1.0
                ),
                "o={o} must clamp to 1"
            );
        }
        for o in [-2.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                composite_pixel(
                    [1.0, 0.0, 0.0, 1.0],
                    [0.25, 0.5, 0.75, 1.0],
                    BlendMode::Multiply,
                    o
                ),
                Some([0.25, 0.5, 0.75, 1.0]),
                "o={o} must leave the base unchanged"
            );
        }
    }

    #[test]
    fn opacity_ramp_pins_exact_bytes() {
        // Red over white, Normal: o = 0 -> white, o = 0.5 -> pink
        // (0.5 * 255 = 127.5 rounds half-away to 128), o = 1 -> red.
        assert_eq!(
            two_layer(RED, BlendMode::Normal, 0.0, WHITE),
            WHITE.to_vec()
        );
        assert_eq!(
            two_layer(RED, BlendMode::Normal, 0.5, WHITE),
            [255, 128, 128, 255].to_vec()
        );
        assert_eq!(two_layer(RED, BlendMode::Normal, 1.0, WHITE), RED.to_vec());
    }

    #[test]
    fn transparent_and_non_finite_sources_pass_through() {
        let dst = [0.25, 0.5, 0.75, 1.0];
        // Zero-alpha source: coverage is 0 under every mode, base unchanged.
        for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
            assert_eq!(
                composite_pixel([0.9, 0.1, 0.4, 0.0], dst, mode, 1.0),
                Some(dst),
                "{mode:?}"
            );
        }
        // All-NaN source: alpha clamps to 0, so coverage is 0 too.
        assert_eq!(
            composite_pixel(
                [f32::NAN, f32::NAN, f32::NAN, f32::NAN],
                dst,
                BlendMode::Normal,
                1.0
            ),
            Some(dst)
        );
        // Half-alpha red over opaque white: the straight-alpha-over pin.
        assert_eq!(
            composite_pixel(
                [1.0, 0.0, 0.0, 0.5],
                [1.0, 1.0, 1.0, 1.0],
                BlendMode::Normal,
                1.0
            ),
            Some([1.0, 0.5, 0.5, 1.0])
        );
    }

    #[test]
    fn passthrough_mode_returns_none() {
        assert_eq!(
            composite_pixel(
                [1.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                BlendMode::Passthrough,
                1.0
            ),
            None
        );
    }

    #[test]
    fn passthrough_folders_and_hidden_layers_are_transparent() {
        // A stack with a pixel-less passthrough folder entry and a hidden
        // entry composites exactly as the bare two-layer stack — the v1
        // folder contract (membership deferred, folders carry no pixels).
        let red = RED.to_vec();
        let white = WHITE.to_vec();
        let bare = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(white.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(red.as_slice()),
                mask: None,
                premultiplied: false,
            },
        ];
        let with_folder = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(white.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Passthrough,
                opacity: 1.0,
                visible: true,
                rgba: None,
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Multiply,
                opacity: 1.0,
                visible: false,
                rgba: Some(white.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(red.as_slice()),
                mask: None,
                premultiplied: false,
            },
        ];
        let bare_out = composite_stack_cpu(&bare, 1, 1).expect("well-formed");
        let folder_out = composite_stack_cpu(&with_folder, 1, 1).expect("well-formed");
        assert_eq!(bare_out, RED.to_vec());
        assert_eq!(folder_out, bare_out);
    }

    #[test]
    fn empty_and_fully_skipped_stacks_are_transparent() {
        let empty: [CpuStackLayer<'_>; 0] = [];
        assert_eq!(
            composite_stack_cpu(&empty, 1, 1).expect("empty stack succeeds"),
            TRANSPARENT.to_vec()
        );
        // A hidden layer's bytes are never read — even a solid fill
        // composites to transparent when nothing is visible.
        let red = RED.to_vec();
        let hidden = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: false,
            rgba: Some(red.as_slice()),
            mask: None,
            premultiplied: false,
        }];
        assert_eq!(
            composite_stack_cpu(&hidden, 1, 1).expect("hidden stack succeeds"),
            TRANSPARENT.to_vec()
        );
        // Same for a visible layer with no pixels (folder / evicted tile).
        let pixel_less = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: None,
            mask: None,
            premultiplied: false,
        }];
        assert_eq!(
            composite_stack_cpu(&pixel_less, 1, 1).expect("pixel-less stack succeeds"),
            TRANSPARENT.to_vec()
        );
    }

    #[test]
    fn masked_layer_gates_coverage_with_exact_bytes() {
        // Red over white, Normal, full opacity, 1x1 — the mask is the only
        // variable, so the gate math is pinned per texel by hand.
        let open = [255u8; 1];
        let shut = [0u8; 1];
        let half = [128u8; 1];
        // Absent mask: the red top wins outright.
        assert_eq!(
            two_layer_masked(RED, BlendMode::Normal, 1.0, WHITE, None),
            RED.to_vec()
        );
        // All-white mask: identical to absent (the no-op-gate contract).
        assert_eq!(
            two_layer_masked(RED, BlendMode::Normal, 1.0, WHITE, Some(&open)),
            RED.to_vec()
        );
        // All-black mask: the top contributes nothing — byte-identical to
        // the bottom, the Substance-parity gate closed.
        assert_eq!(
            two_layer_masked(RED, BlendMode::Normal, 1.0, WHITE, Some(&shut)),
            WHITE.to_vec()
        );
        // Half mask: cov = 128/255; r = 1 + (1-1)*cov = 1 -> 255;
        // g = b = 1 + (0-1)*128/255 = 127/255 -> 127 (exact: 255-128);
        // a = cov + 1*(1-cov) = 1 -> 255.
        assert_eq!(
            two_layer_masked(RED, BlendMode::Normal, 1.0, WHITE, Some(&half)),
            [255, 127, 127, 255].to_vec()
        );
        // The gate multiplies, not replaces: the same half mask under
        // Multiply gives the same bytes here (blend red*white = red, so
        // the coverages coincide — while a FULL mask under Multiply over
        // white is still red, proving the blend arm ran under the gate).
        assert_eq!(
            two_layer_masked(RED, BlendMode::Multiply, 1.0, WHITE, Some(&half)),
            [255, 127, 127, 255].to_vec()
        );
        assert_eq!(
            two_layer_masked(RED, BlendMode::Multiply, 1.0, WHITE, Some(&open)),
            RED.to_vec()
        );
    }

    #[test]
    fn composite_pixel_masked_pins_partial_alpha_math() {
        // Half-alpha red over opaque white, mask 0.5: cov = 0.5*1*0.5 =
        // 0.25 (exact binary fractions throughout); rgb = white +
        // (red-white)*0.25 = [1, 0.75, 0.75]; a = 0.25 + 1*0.75 = 1.
        assert_eq!(
            composite_pixel_masked(
                [1.0, 0.0, 0.0, 0.5],
                [1.0, 1.0, 1.0, 1.0],
                BlendMode::Normal,
                1.0,
                0.5
            ),
            Some([1.0, 0.75, 0.75, 1.0])
        );
        // Mask 1.0 is exactly the unmasked pixel op (the no-op-gate
        // contract at the texel level, for every mode).
        for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
            assert_eq!(
                composite_pixel_masked([0.9, 0.1, 0.4, 0.8], [0.2, 0.6, 0.3, 1.0], mode, 0.7, 1.0),
                composite_pixel([0.9, 0.1, 0.4, 0.8], [0.2, 0.6, 0.3, 1.0], mode, 0.7),
                "{mode:?}"
            );
        }
        // Non-finite masks gate shut (fail-closed): NaN and +inf leave the
        // base untouched, never leak the source.
        for m in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                composite_pixel_masked(
                    [1.0, 0.0, 0.0, 1.0],
                    [0.25, 0.5, 0.75, 1.0],
                    BlendMode::Normal,
                    1.0,
                    m
                ),
                Some([0.25, 0.5, 0.75, 1.0]),
                "mask={m} must gate shut"
            );
        }
    }

    #[test]
    fn wrong_length_mask_data_fails_loud() {
        let red = RED.to_vec();
        let white = WHITE.to_vec();
        // A 1x1 stack with a 2-byte mask: the oracle must refuse, not gate
        // the wrong texel.
        let bad = [255u8; 2];
        let layers = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(white.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(red.as_slice()),
                mask: Some(bad.as_slice()),
                premultiplied: false,
            },
        ];
        assert!(matches!(
            composite_stack_cpu(&layers, 1, 1),
            Err(CompositeError::InvalidMaskData {
                expected: 1,
                found: 2
            })
        ));
        // Can-fail: an empty plane is wrong-length too (a never-painted
        // mask must arrive as None, not as `Some(&[])`).
        let empty: [u8; 0] = [];
        let layers = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(red.as_slice()),
            mask: Some(empty.as_slice()),
            premultiplied: false,
        }];
        assert!(matches!(
            composite_stack_cpu(&layers, 1, 1),
            Err(CompositeError::InvalidMaskData {
                expected: 1,
                found: 0
            })
        ));
    }

    #[test]
    fn wrong_length_pixel_data_fails_loud() {
        let short = [255u8; 3];
        let layers = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(short.as_slice()),
            mask: None,
            premultiplied: false,
        }];
        assert!(matches!(
            composite_stack_cpu(&layers, 1, 1),
            Err(CompositeError::InvalidPixelData {
                expected: 4,
                found: 3
            })
        ));
    }

    #[test]
    fn premultiplied_identity_matches_straight_composite_of_unpremultiplied_texel() {
        // §8 T1: one premultiplied entry, texel (100,100,100,128), Normal,
        // 1.0, over transparent — with an all-255 mask plane, so the entry
        // is NOT "unmasked" and the bottom-copy rule does NOT fire. The
        // mask's m = 1.0 leaves coverage unchanged, so this exercises the
        // general un-premultiply path: s = 100/128 = 0.78125 (an exact
        // binary fraction), cov = 128/255, out = 0.78125·cov = 100/255 →
        // 100; alpha cov → 128.
        let px = [100u8, 100, 100, 128];
        let open = [255u8; 1];
        let layers = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(px.as_slice()),
            mask: Some(open.as_slice()),
            premultiplied: true,
        }];
        assert_eq!(
            composite_stack_cpu(&layers, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![100, 100, 100, 128]
        );
        // Sentinel: the SAME bytes through the straight path
        // double-multiply the soft edge: (100/255)·(128/255) = 0.196847 →
        // 50, where the flat target shows 100.
        let straight = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(px.as_slice()),
            mask: Some(open.as_slice()),
            premultiplied: false,
        }];
        assert_eq!(
            composite_stack_cpu(&straight, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![50, 50, 50, 128]
        );
    }

    #[test]
    fn bottom_copy_seeds_verbatim() {
        // §8 T2: a legacy straight texel (200,200,200,100) as the bottom
        // premultiplied Normal/1.0/unmasked entry seeds VERBATIM — the
        // migration guarantee for Base's mixed-convention bytes.
        let legacy = [200u8, 200, 200, 100];
        let layers = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(legacy.as_slice()),
            mask: None,
            premultiplied: true,
        }];
        assert_eq!(
            composite_stack_cpu(&layers, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![200, 200, 200, 100]
        );
        // Sentinel: the T1 mask trick forces the general path for the same
        // bytes — s = 200/100 = 2.0 clamps to 1.0, out = 100/255 → 100 —
        // which is what the seed must NOT publish for migrated Base bytes.
        let open = [255u8; 1];
        let general = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(legacy.as_slice()),
            mask: Some(open.as_slice()),
            premultiplied: true,
        }];
        assert_eq!(
            composite_stack_cpu(&general, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![100, 100, 100, 100]
        );
    }

    #[test]
    fn bottom_copy_yields_to_any_prior_contributor() {
        // The seed fires only at the true bottom. Three suppressions, each
        // with invalid data (p > a) so the seed and the general path
        // disagree and the wrong one fails:
        // - a fully transparent bottom entry already contributed, so the
        //   premultiplied top composites generally: s = 200/100 = 2.0 →
        //   1.0, out = 100/255 → 100 (not the verbatim 200).
        let clear = [0u8, 0, 0, 0];
        let legacy = [200u8, 200, 200, 100];
        let layers = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(clear.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(legacy.as_slice()),
                mask: None,
                premultiplied: true,
            },
        ];
        assert_eq!(
            composite_stack_cpu(&layers, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![100, 100, 100, 100]
        );
        // - a non-Normal bottom premultiplied entry takes the general path:
        //   Multiply of s = 100/128 = 0.78125 against transparent black is
        //   0, with alpha cov = 128/255 → 128.
        let px = [100u8, 100, 100, 128];
        let multiply = [CpuStackLayer {
            blend_mode: BlendMode::Multiply,
            opacity: 1.0,
            visible: true,
            rgba: Some(px.as_slice()),
            mask: None,
            premultiplied: true,
        }];
        assert_eq!(
            composite_stack_cpu(&multiply, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![0, 0, 0, 128]
        );
        // - a half-opacity bottom premultiplied entry takes the general
        //   path: cov = (128/255)·0.5, out = 0.78125·cov = 0.196078 → 50,
        //   alpha cov → 64 (exact binary fractions throughout).
        let half = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 0.5,
            visible: true,
            rgba: Some(px.as_slice()),
            mask: None,
            premultiplied: true,
        }];
        assert_eq!(
            composite_stack_cpu(&half, 1, 1).expect("1x1 oracle input is well-formed"),
            vec![50, 50, 50, 64]
        );
    }

    #[test]
    fn from_layer_defaults_to_straight_alpha() {
        // The stage-1a byte-identity anchor: `from_layer` builds
        // straight-alpha entries, so every existing caller composites
        // exactly as before the flag existed.
        let mut stack = LayerStack::new();
        let id = stack.add_layer("top", LayerKind::Paint);
        let layer = stack.layer(id).expect("just added");
        let gpu = GpuStackLayer::from_layer(layer, None);
        assert!(!gpu.premultiplied);
        let red = RED.to_vec();
        let cpu = CpuStackLayer::from_layer(layer, Some(red.as_slice()));
        assert!(!cpu.premultiplied);
    }

    #[test]
    fn region_composite_equals_full_composite_subrect() {
        // §8 T3: a 4×4 two-layer stack with per-texel variation (a uniform
        // fill would hide an offset bug); region (1,1)–(3,3) equals the
        // full composite's same rect EXACTLY — both paths share the
        // per-texel fold, so this is exact equality, not ±1 LSB.
        let (w, h) = (4u32, 4u32);
        let mut bottom = Vec::with_capacity(64);
        let mut top = Vec::with_capacity(64);
        let mut mask = Vec::with_capacity(16);
        for y in 0..h {
            for x in 0..w {
                bottom.extend_from_slice(&[x as u8 * 60, y as u8 * 60, 128, 255]);
                top.extend_from_slice(&[200, 50, 50, 128]);
                mask.push(if (x + y) % 2 == 0 { 255 } else { 128 });
            }
        }
        let layers = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(bottom.as_slice()),
                mask: None,
                premultiplied: false,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(top.as_slice()),
                mask: Some(mask.as_slice()),
                premultiplied: false,
            },
        ];
        let full = composite_stack_cpu(&layers, w, h).expect("well-formed");
        let rect = CompositeRect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };
        let region = composite_stack_region_cpu(&layers, w, h, rect).expect("well-formed");
        assert_eq!(region.len(), 2 * 2 * 4);
        let mut want = Vec::with_capacity(16);
        for dy in 0..2 {
            let row = ((1 + dy) * w + 1) as usize * 4;
            want.extend_from_slice(&full[row..row + 2 * 4]);
        }
        assert_eq!(region, want);
        // A fully-outside rect is an empty no-op, not an error.
        let outside = CompositeRect {
            x: 9,
            y: 9,
            width: 2,
            height: 2,
        };
        assert_eq!(
            composite_stack_region_cpu(&layers, w, h, outside).expect("well-formed"),
            Vec::<u8>::new()
        );
        // An overhanging rect clamps to the tile: (3,3)+(4,4) covers only
        // the bottom-right texel, byte-equal to the full composite's.
        let edge = CompositeRect {
            x: 3,
            y: 3,
            width: 4,
            height: 4,
        };
        assert_eq!(
            composite_stack_region_cpu(&layers, w, h, edge).expect("well-formed"),
            full[60..64].to_vec()
        );
    }

    #[test]
    fn region_bottom_copy_seeds_rect_verbatim() {
        // The seed fires per composite call: a region composite whose first
        // contributor is premultiplied/Normal/1.0/unmasked seeds the rect
        // verbatim — legacy bytes survive even region-scoped.
        let (w, h) = (4u32, 4u32);
        let mut plane = Vec::with_capacity(64);
        for i in 0..16 {
            plane.extend_from_slice(&[200u8, 100 + i as u8, 50, 100]);
        }
        let layers = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(plane.as_slice()),
            mask: None,
            premultiplied: true,
        }];
        let rect = CompositeRect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        };
        let region = composite_stack_region_cpu(&layers, w, h, rect).expect("well-formed");
        // Rect texels (1,1),(2,1),(1,2),(2,2) are plane texels 5,6,9,10.
        let mut want = Vec::with_capacity(16);
        for ti in [5usize, 6, 9, 10] {
            want.extend_from_slice(&plane[ti * 4..ti * 4 + 4]);
        }
        assert_eq!(region, want);
    }

    #[test]
    fn from_layer_carries_blend_opacity_and_visibility() {
        let mut stack = LayerStack::new();
        let id = stack.add_layer("top", LayerKind::Paint);
        stack.set_blend_mode(id, BlendMode::Multiply);
        stack.set_opacity(id, 0.25);
        stack.set_visible(id, false);
        let layer = stack.layer(id).expect("just added");
        let red = RED.to_vec();
        let gpu = GpuStackLayer::from_layer(layer, None);
        assert_eq!(gpu.blend_mode, BlendMode::Multiply);
        assert_eq!(gpu.opacity, 0.25);
        assert!(!gpu.visible);
        assert!(gpu.texture.is_none());
        assert_eq!(gpu.mask, None, "from_layer builds unmasked entries");
        let cpu = CpuStackLayer::from_layer(layer, Some(red.as_slice()));
        assert_eq!(cpu.blend_mode, BlendMode::Multiply);
        assert_eq!(cpu.opacity, 0.25);
        assert!(!cpu.visible);
        assert_eq!(cpu.rgba, Some(red.as_slice()));
        assert_eq!(cpu.mask, None, "from_layer builds unmasked entries");
    }

    #[test]
    fn f32_u8_conversions_saturate() {
        assert_eq!(u8_to_f32(0), 0.0);
        assert_eq!(u8_to_f32(255), 1.0);
        assert_eq!(f32_to_u8(0.0), 0);
        assert_eq!(f32_to_u8(1.0), 255);
        assert_eq!(f32_to_u8(0.5), 128);
        assert_eq!(f32_to_u8(2.0), 255);
        assert_eq!(f32_to_u8(-1.0), 0);
        assert_eq!(f32_to_u8(f32::NAN), 0);
        assert_eq!(f32_to_u8(f32::INFINITY), 255);
    }

    #[cfg(feature = "gpu")]
    mod gpu {
        use super::super::*;
        use super::*;
        use crate::paint::PaintError;

        /// Requests a device with `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`
        /// enabled. Mirrors `tile_pool`'s gpu test helper: this suite runs
        /// with a required real adapter, not an optional one.
        fn request_device() -> (wgpu::Device, wgpu::Queue) {
            let instance = wgpu::Instance::default();
            let adapter = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            )
            .expect("a wgpu adapter must be available for this test");
            assert!(
                adapter
                    .features()
                    .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES),
                "adapter {:?} lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES",
                adapter.get_info().name
            );
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                ..Default::default()
            }))
            .expect("device request must succeed")
        }

        /// Shared device/queue/compositor/size for the GPU tests — bundled
        /// as one struct because clippy rejects >7-arg helpers.
        struct GpuFixture<'a> {
            device: &'a wgpu::Device,
            queue: &'a wgpu::Queue,
            compositor: &'a StackCompositor,
            size: u32,
        }

        impl GpuFixture<'_> {
            /// Uploads `size`×`size` RGBA8 `bytes` into a fresh target.
            fn upload(&self, bytes: &[u8]) -> Result<PaintTarget, PaintError> {
                let target = PaintTarget::new(self.device, self.size, self.size);
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("umber_compositor_test_upload"),
                        });
                target.write_region(
                    self.device,
                    &mut encoder,
                    (0, 0),
                    (self.size, self.size),
                    bytes,
                )?;
                self.queue.submit(Some(encoder.finish()));
                Ok(target)
            }

            /// Runs the GPU composite over `entries` into a fresh target and
            /// reads it back.
            fn composite(&self, entries: &[GpuStackLayer<'_>]) -> Vec<u8> {
                let output = PaintTarget::new(self.device, self.size, self.size);
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("umber_compositor_test_composite"),
                        });
                self.compositor
                    .composite_stack(&mut encoder, entries, &output)
                    .expect("test stacks are well-formed");
                self.queue.submit(Some(encoder.finish()));
                output
                    .read_back_rgba8(self.device, self.queue)
                    .expect("readback should succeed")
            }

            /// Runs the REGION composite over `entries` into `output`'s
            /// `rect` and reads the whole target back: bytes outside the
            /// rect must equal whatever `output` held before the call.
            fn composite_region(
                &self,
                entries: &[GpuStackLayer<'_>],
                output: &PaintTarget,
                rect: CompositeRect,
            ) -> Vec<u8> {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("umber_compositor_test_composite_region"),
                        });
                self.compositor
                    .composite_stack_region(&mut encoder, entries, output, rect)
                    .expect("test stacks are well-formed");
                self.queue.submit(Some(encoder.finish()));
                output
                    .read_back_rgba8(self.device, self.queue)
                    .expect("readback should succeed")
            }

            /// Builds matching GPU/CPU entries from one (bottom, top) pair
            /// and asserts the GPU reproduces the oracle to ±1 LSB.
            fn compare(&self, bottom: &[u8], top: &[u8], mode: BlendMode, opacity: f32) {
                let bottom_tex = self.upload(bottom).expect("upload");
                let top_tex = self.upload(top).expect("upload");
                let gpu_entries = [
                    GpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        texture: Some(&bottom_tex),
                        mask: None,
                        premultiplied: false,
                    },
                    GpuStackLayer {
                        blend_mode: mode,
                        opacity,
                        visible: true,
                        texture: Some(&top_tex),
                        mask: None,
                        premultiplied: false,
                    },
                ];
                let got = self.composite(&gpu_entries);
                let cpu_entries = [
                    CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(bottom),
                        mask: None,
                        premultiplied: false,
                    },
                    CpuStackLayer {
                        blend_mode: mode,
                        opacity,
                        visible: true,
                        rgba: Some(top),
                        mask: None,
                        premultiplied: false,
                    },
                ];
                let want = composite_stack_cpu(&cpu_entries, self.size, self.size)
                    .expect("oracle input well-formed");
                assert_bytes_near_lenient(&got, &want);
            }

            /// [`Self::compare`] with a layer mask on the top entry: the
            /// GPU's per-dispatch `R8Unorm` upload must reproduce the
            /// oracle's byte gate to ±1 LSB.
            fn compare_masked(
                &self,
                bottom: &[u8],
                top: &[u8],
                mode: BlendMode,
                opacity: f32,
                mask: &[u8],
            ) {
                let bottom_tex = self.upload(bottom).expect("upload");
                let top_tex = self.upload(top).expect("upload");
                let gpu_entries = [
                    GpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        texture: Some(&bottom_tex),
                        mask: None,
                        premultiplied: false,
                    },
                    GpuStackLayer {
                        blend_mode: mode,
                        opacity,
                        visible: true,
                        texture: Some(&top_tex),
                        mask: Some(mask),
                        premultiplied: false,
                    },
                ];
                let got = self.composite(&gpu_entries);
                let cpu_entries = [
                    CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(bottom),
                        mask: None,
                        premultiplied: false,
                    },
                    CpuStackLayer {
                        blend_mode: mode,
                        opacity,
                        visible: true,
                        rgba: Some(top),
                        mask: Some(mask),
                        premultiplied: false,
                    },
                ];
                let want = composite_stack_cpu(&cpu_entries, self.size, self.size)
                    .expect("oracle input well-formed");
                assert_bytes_near_lenient(&got, &want);
            }

            /// A two-tone grayscale mask plane (`size`×`size` bytes): shut
            /// on the left half, open on the right — so a dispatch-offset
            /// bug that shifts the mask against the pixels fails instead
            /// of hiding.
            fn two_tone_mask(&self, shut: u8, open: u8) -> Vec<u8> {
                let size = self.size as usize;
                let mut out = Vec::with_capacity(size * size);
                for _ in 0..size {
                    for x in 0..size {
                        out.push(if x < size / 2 { shut } else { open });
                    }
                }
                out
            }
        }

        /// Builds a `GpuFixture` on a required-feature device.
        fn fixture<'a>(
            device: &'a wgpu::Device,
            queue: &'a wgpu::Queue,
            compositor: &'a StackCompositor,
        ) -> GpuFixture<'a> {
            GpuFixture {
                device,
                queue,
                compositor,
                size: 16,
            }
        }

        #[test]
        fn gpu_matches_cpu_for_core_two_layer_stacks() {
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            // Two-tone fills (split at x = 8) so a dispatch-offset bug that
            // shifts or drops half the tile fails instead of hiding.
            let white = two_tone(size, WHITE, WHITE);
            let red = two_tone(size, RED, WHITE);
            let black = two_tone(size, BLACK, BLACK);
            for (top, mode, bottom) in [
                (red.clone(), BlendMode::Normal, white.clone()),
                (white.clone(), BlendMode::Multiply, red.clone()),
                (black.clone(), BlendMode::Screen, red.clone()),
                (red.clone(), BlendMode::Overlay, white.clone()),
            ] {
                fix.compare(&bottom, &top, mode, 1.0);
            }
        }

        #[test]
        fn gpu_matches_cpu_across_an_opacity_ramp() {
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let white = two_tone(size, WHITE, WHITE);
            let red = two_tone(size, RED, RED);
            for opacity in [0.0, 0.25, 0.5, 0.75, 1.0] {
                fix.compare(&white, &red, BlendMode::Normal, opacity);
            }
        }

        #[test]
        fn gpu_matches_cpu_for_new_separable_family() {
            // One golden for the wave-blends separable family: Vivid Light
            // exercises the dodge/burn-helper path plus the mid-gray no-op
            // branch on GPU — the other nine new scalar arms share this
            // exact dispatch shape (branch inside one kernel), so one
            // golden pins the shape and the CPU tests pin each arm's math.)
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let white = two_tone(size, WHITE, WHITE);
            let red = two_tone(size, RED, WHITE);
            let black = two_tone(size, BLACK, BLACK);
            fix.compare(&white, &red, BlendMode::VividLight, 1.0);
            fix.compare(&white, &black, BlendMode::VividLight, 1.0);
            fix.compare(&white, &red, BlendMode::LinearLight, 0.5);
        }

        #[test]
        fn gpu_matches_cpu_for_hsv_family() {
            // One golden for the HSV family: Tint routes the kernel's
            // triplet path (rgb_to_hsv/hsv_to_rgb) with the achromatic
            // guard live on the two-tone split (white half has no hue, so
            // that half must come back as the base).
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let blue = two_tone(size, [0, 0, 255, 255], [0, 0, 255, 255]);
            let red_white = two_tone(size, RED, WHITE);
            fix.compare(&blue, &red_white, BlendMode::Tint, 1.0);
            fix.compare(&blue, &red_white, BlendMode::Color, 0.5);
        }

        #[test]
        fn gpu_matches_cpu_for_normal_map_family() {
            // One golden for the normal-map family: Detail routes the RNM
            // path on encoded-normal texels (flat vs a tilted normal — the
            // tilt encoding is just bytes to the upload; the oracle decodes
            // the same bytes, so any unpack/pack skew fails the compare).
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            const FLAT: [u8; 4] = [128, 128, 255, 255];
            const TILT: [u8; 4] = [218, 128, 218, 255];
            let flat = two_tone(size, FLAT, FLAT);
            let tilt = two_tone(size, TILT, FLAT);
            fix.compare(&flat, &tilt, BlendMode::NormalDetail, 1.0);
            fix.compare(&flat, &tilt, BlendMode::NormalCombine, 1.0);
        }

        #[test]
        fn gpu_matches_cpu_for_masked_stacks() {
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let white = two_tone(size, WHITE, WHITE);
            let red = two_tone(size, RED, RED);
            // Shut-left / open-right: the left half must composite to the
            // bottom, the right half to the top — a mask/pixel misalignment
            // fails on both halves at once.
            let split = fix.two_tone_mask(0, 255);
            fix.compare_masked(&white, &red, BlendMode::Normal, 1.0, &split);
            fix.compare_masked(&white, &red, BlendMode::Multiply, 1.0, &split);
            // Uniform half mask over the whole tile (the CPU golden pins
            // [255, 127, 127, 255] here; the GPU asserts ±1 LSB of it).
            let half = vec![128u8; size as usize * size as usize];
            fix.compare_masked(&white, &red, BlendMode::Normal, 1.0, &half);
        }

        #[test]
        fn gpu_premultiplied_identity_matches_oracle() {
            // The CPU T1 shape at tile scale: a uniform (100,100,100,128)
            // premultiplied plane with an all-open mask (which defeats the
            // bottom-copy seed on both paths, so the un-premultiply runs).
            // ±1 LSB: UNORM round-trip and FMA contraction are
            // implementation-defined (judgment 8); the CPU side pins the
            // exact bytes separately.
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let plane = two_tone(size, [100, 100, 100, 128], [100, 100, 100, 128]);
            let tex = fix.upload(&plane).expect("upload");
            let mask = vec![255u8; size as usize * size as usize];
            let gpu_entries = [GpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                texture: Some(&tex),
                mask: Some(mask.as_slice()),
                premultiplied: true,
            }];
            let got = fix.composite(&gpu_entries);
            let cpu_entries = [CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(plane.as_slice()),
                mask: Some(mask.as_slice()),
                premultiplied: true,
            }];
            let want =
                composite_stack_cpu(&cpu_entries, size, size).expect("oracle input well-formed");
            assert_eq!(&want[..4], &[100, 100, 100, 128][..]);
            assert_bytes_near_lenient(&got, &want);
        }

        #[test]
        fn gpu_bottom_copy_seeds_verbatim() {
            // The CPU T2 shape at tile scale: the seed is a texture copy,
            // so the GPU side is byte-EXACT here — still asserted ±1 LSB
            // like every GPU twin (judgment 8), with the exact bytes
            // pinned on the oracle.
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let plane = two_tone(size, [200, 200, 200, 100], [200, 200, 200, 100]);
            let tex = fix.upload(&plane).expect("upload");
            let gpu_entries = [GpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                texture: Some(&tex),
                mask: None,
                premultiplied: true,
            }];
            let got = fix.composite(&gpu_entries);
            let cpu_entries = [CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(plane.as_slice()),
                mask: None,
                premultiplied: true,
            }];
            let want =
                composite_stack_cpu(&cpu_entries, size, size).expect("oracle input well-formed");
            assert_eq!(&want[..4], &[200, 200, 200, 100][..]);
            assert_bytes_near_lenient(&got, &want);
        }

        #[test]
        fn gpu_region_matches_oracle_and_leaves_outside_untouched() {
            // §8 T3 on the GPU: the output starts as a sentinel fill, the
            // region (4,4,8,8) composites a position-varying two-layer
            // stack (any dispatch-origin or mask-offset bug fails instead
            // of hiding), and the readback must equal the CPU region
            // oracle (±1 LSB) inside the rect and the sentinel EXACTLY
            // outside it.
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let n = size as usize * size as usize;
            let mut bottom = Vec::with_capacity(n * 4);
            let mut top = Vec::with_capacity(n * 4);
            let mut mask = Vec::with_capacity(n);
            for y in 0..size {
                for x in 0..size {
                    bottom.extend_from_slice(&[(x * 16) as u8, (y * 16) as u8, 128, 255]);
                    top.extend_from_slice(&[200, 50, 50, 128]);
                    mask.push(if (x + y) % 2 == 0 { 255 } else { 0 });
                }
            }
            let bottom_tex = fix.upload(&bottom).expect("upload");
            let top_tex = fix.upload(&top).expect("upload");
            let gpu_entries = [
                GpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    texture: Some(&bottom_tex),
                    mask: None,
                    premultiplied: false,
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    texture: Some(&top_tex),
                    mask: Some(mask.as_slice()),
                    premultiplied: false,
                },
            ];
            let rect = CompositeRect {
                x: 4,
                y: 4,
                width: 8,
                height: 8,
            };
            let sentinel = [255u8, 0, 255, 255];
            let mut sentinel_plane = Vec::with_capacity(n * 4);
            for _ in 0..n {
                sentinel_plane.extend_from_slice(&sentinel);
            }
            let output = fix.upload(&sentinel_plane).expect("upload");
            let got = fix.composite_region(&gpu_entries, &output, rect);
            let cpu_entries = [
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(bottom.as_slice()),
                    mask: None,
                    premultiplied: false,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(top.as_slice()),
                    mask: Some(mask.as_slice()),
                    premultiplied: false,
                },
            ];
            let want =
                composite_stack_region_cpu(&cpu_entries, size, size, rect).expect("well-formed");
            assert_eq!(want.len(), 8 * 8 * 4);
            let mut got_inside = Vec::with_capacity(8 * 8 * 4);
            for y in 4..12 {
                for x in 4..12 {
                    let base = (y * size + x) as usize * 4;
                    got_inside.extend_from_slice(&got[base..base + 4]);
                }
            }
            assert_bytes_near_lenient(&got_inside, &want);
            for y in 0..size {
                for x in 0..size {
                    if (4..12).contains(&x) && (4..12).contains(&y) {
                        continue;
                    }
                    let base = (y * size + x) as usize * 4;
                    assert_eq!(
                        &got[base..base + 4],
                        &sentinel[..],
                        "texel ({x}, {y}) outside the rect must stay untouched"
                    );
                }
            }
        }

        #[test]
        fn gpu_passthrough_folder_matches_cpu() {
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let size = fix.size;
            let white = two_tone(size, WHITE, WHITE);
            let red = two_tone(size, RED, WHITE);
            let bottom_tex = fix.upload(&white).expect("upload");
            let top_tex = fix.upload(&red).expect("upload");
            // A pixel-less passthrough folder plus a hidden layer sit
            // between bottom and top: both must be transparent no-ops.
            let gpu_entries = [
                GpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    texture: Some(&bottom_tex),
                    mask: None,
                    premultiplied: false,
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Passthrough,
                    opacity: 1.0,
                    visible: true,
                    texture: None,
                    mask: None,
                    premultiplied: false,
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Multiply,
                    opacity: 1.0,
                    visible: false,
                    texture: Some(&bottom_tex),
                    mask: None,
                    premultiplied: false,
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    texture: Some(&top_tex),
                    mask: None,
                    premultiplied: false,
                },
            ];
            let got = fix.composite(&gpu_entries);
            let cpu_entries = [
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(white.as_slice()),
                    mask: None,
                    premultiplied: false,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Passthrough,
                    opacity: 1.0,
                    visible: true,
                    rgba: None,
                    mask: None,
                    premultiplied: false,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Multiply,
                    opacity: 1.0,
                    visible: false,
                    rgba: Some(white.as_slice()),
                    mask: None,
                    premultiplied: false,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(red.as_slice()),
                    mask: None,
                    premultiplied: false,
                },
            ];
            let want =
                composite_stack_cpu(&cpu_entries, size, size).expect("oracle input well-formed");
            assert_bytes_near_lenient(&got, &want);
        }

        #[test]
        fn gpu_empty_stack_clears_to_transparent() {
            let (device, queue) = request_device();
            let compositor =
                StackCompositor::new(device.clone()).expect("device has the required feature");
            let fix = fixture(&device, &queue, &compositor);
            let got = fix.composite(&[]);
            assert!(
                got.iter().all(|&b| b == 0),
                "an empty stack must composite to fully transparent"
            );
        }
    }

    /// Two-tone `size`×`size` RGBA8 fill (split at `x = size / 2`) for the
    /// GPU tests — shared here so both `gpu` tests use the same builder.
    #[cfg(feature = "gpu")]
    fn two_tone(size: u32, left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::with_capacity(size as usize * size as usize * 4);
        for _ in 0..size {
            for x in 0..size {
                let px = if x < size / 2 { left } else { right };
                out.extend_from_slice(&px);
            }
        }
        out
    }

    /// Byte comparison with ±1 LSB tolerance: UNORM round-trip rounding and
    /// FMA contraction are implementation-defined, so the GPU side may sit
    /// 1 LSB off the CPU oracle (which pins exact values separately).
    #[cfg(feature = "gpu")]
    fn assert_bytes_near_lenient(got: &[u8], want: &[u8]) {
        assert_eq!(got.len(), want.len(), "composite length mismatch");
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            let diff = (*g as i16 - *w as i16).abs();
            assert!(
                diff <= 1,
                "byte {i} differs by {diff} LSB (got {g}, want {w}): beyond rounding tolerance"
            );
        }
    }
}
