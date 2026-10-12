//! The stack compositor: blends one tile's layer stack into a single output target.
//!
//! Wave-2 core-loop scope (docs/specs/2026-10-12-strategy-review.md, critical 6):
//! the tile pool ([`crate::tile_pool`]) owns one GPU target per (layer, channel,
//! tile); this module owns the composite over the stack — bottom-to-top, per-channel
//! linear blending with opacity and the [`umber_core::BlendMode`] set — into one
//! output [`PaintTarget`]. The caller binds the right tiles (one pool [`PoolKey`]
//! per layer for the channel being composited); per-channel routing above the tile
//! (which pool keys feed a given composite) is the viewport-texture kill's surface,
//! not this module's.
//!
//! The WGSL below is transliterated FROM the Rust reference —
//! [`umber_core::BlendMode::blend_channel`] plus
//! [`umber_core::BlendMode::composite_channel`] (layers.rs) — NOT invented: every
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
//!    with `blend` from the reference arms. For opaque sources (`src_alpha = 1`)
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
//! 6. **Mode census: 15 variants, 14 pixel ops.** The enum carries `Normal`,
//!    `Passthrough`, and 13 other separable modes; `Passthrough` is structural
//!    (never reaches the shader). Unknown WGSL ids fall back to `Normal` —
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

use std::borrow::Cow;
use std::mem::size_of;
use std::num::NonZeroU64;

use umber_core::{BlendMode, Layer};
use wgpu::util::DeviceExt as _;

use crate::paint::PaintTarget;

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
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var acc_tex: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(2) var<uniform> params: CompositeParams;

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

fn clamp01(v: f32) -> f32 {
    // WGSL has NO isNan/isnan builtin (removed from the spec; naga 30's
    // front accepts neither spelling — the Gate-0 journey test caught this
    // on the shader's FIRST real compile, since the unit tests exercise
    // only the CPU oracle). The portable idiom: NaN is the only f32 that
    // compares unequal to itself.
    if (v != v) {
        return 0.0;
    }
    return clamp(v, 0.0, 1.0);
}

// Mirrors BlendMode::blend_channel arm-for-arm (NaN -> 0, inf saturates,
// dodge/burn guards intact). Unknown modes fall back to Normal.
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
        if (s >= 1.0) {
            v = 1.0;
        } else {
            v = min(d / (1.0 - s), 1.0);
        }
    } else if (mode == BLEND_COLOR_BURN) {
        if (s <= 0.0) {
            v = 0.0;
        } else {
            v = 1.0 - min((1.0 - d) / s, 1.0);
        }
    } else if (mode == BLEND_LINEAR_BURN) {
        v = max(s + d - 1.0, 0.0);
    }
    return clamp01(v);
}

@compute @workgroup_size(8, 8)
fn cs_composite(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let xy = vec2<i32>(i32(gid.x), i32(gid.y));
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
    let sa = clamp01(s.w);
    let cov = sa * o;
    let dx = clamp01(d.x);
    let dy = clamp01(d.y);
    let dz = clamp01(d.z);
    let dw = clamp01(d.w);
    let r = dx + (blend_channel(params.mode, s.x, d.x) - dx) * cov;
    let g = dy + (blend_channel(params.mode, s.y, d.y) - dy) * cov;
    let b = dz + (blend_channel(params.mode, s.z, d.z) - dz) * cov;
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
}

impl<'a> GpuStackLayer<'a> {
    /// Builds an entry from a [`Layer`] plus its already-looked-up pool tile.
    ///
    /// Folders and instances carry no pixels of their own — pass `None` for
    /// them (the composite treats that as transparent; see the module docs).
    pub fn from_layer(layer: &Layer, texture: Option<&'a PaintTarget>) -> Self {
        Self {
            blend_mode: layer.blend_mode,
            opacity: layer.opacity,
            visible: layer.visible,
            texture,
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
}

impl<'a> CpuStackLayer<'a> {
    /// Builds an entry from a [`Layer`] plus its already-looked-up pixel bytes.
    ///
    /// Folders and instances carry no pixels of their own — pass `None` for
    /// them (the composite treats that as transparent; see the module docs).
    pub fn from_layer(layer: &Layer, rgba: Option<&'a [u8]>) -> Self {
        Self {
            blend_mode: layer.blend_mode,
            opacity: layer.opacity,
            visible: layer.visible,
            rgba,
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
}

/// Clamps a linear-space channel to `0.0..=1.0`, mapping `NaN` to `0.0`.
///
/// Private mirror of `umber_core::BlendMode`'s own clamp (which is private
/// there): `±inf` saturates, `NaN` becomes black so one bad texel cannot
/// poison the composite.
fn clamp01(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
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
/// pixel op) — the same contract as `composite_channel`. Otherwise the RGB
/// channels follow the reference arms gated by coverage `src_alpha ×
/// opacity` (module-docs judgment 2), and alpha follows straight-alpha over
/// (judgment 3). For opaque sources the RGB result is bit-identical to a
/// per-channel `composite_channel` fold — pinned by test, not just claimed.
pub fn composite_pixel(
    src: [f32; 4],
    dst: [f32; 4],
    mode: BlendMode,
    opacity: f32,
) -> Option<[f32; 4]> {
    let blend_r = mode.blend_channel(src[0], dst[0])?;
    let blend_g = mode.blend_channel(src[1], dst[1])?;
    let blend_b = mode.blend_channel(src[2], dst[2])?;
    let src_a = clamp01(src[3]);
    let base_r = clamp01(dst[0]);
    let base_g = clamp01(dst[1]);
    let base_b = clamp01(dst[2]);
    let base_a = clamp01(dst[3]);
    let cov = src_a * clamped_opacity(opacity);
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
    let len = width as usize * height as usize * 4;
    for layer in layers {
        if let Some(rgba) = layer.rgba {
            if rgba.len() != len {
                return Err(CompositeError::InvalidPixelData {
                    expected: len,
                    found: rgba.len(),
                });
            }
        }
    }
    let mut acc = vec![0.0f32; len];
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
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let src = [
                u8_to_f32(px[0]),
                u8_to_f32(px[1]),
                u8_to_f32(px[2]),
                u8_to_f32(px[3]),
            ];
            let base = i * 4;
            let dst = [acc[base], acc[base + 1], acc[base + 2], acc[base + 3]];
            if let Some(out) = composite_pixel(src, dst, layer.blend_mode, layer.opacity) {
                acc[base..base + 4].copy_from_slice(&out);
            }
        }
    }
    Ok(acc.iter().map(|v| f32_to_u8(*v)).collect())
}

/// Per-dispatch uniforms for [`COMPOSITE_WGSL`]: 16 bytes, 4-byte aligned
/// throughout, so the `#[repr(C)]` layout matches the WGSL struct with no
/// padding surprises.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeParams {
    mode: u32,
    opacity: f32,
    width: u32,
    height: u32,
}

const _: () = assert!(
    size_of::<CompositeParams>() == 16,
    "CompositeParams must stay one 16-byte uniform chunk"
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
            ],
        });

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
        })
    }

    /// Records the composite of `layers` (bottom-to-top) into `output`.
    ///
    /// `output` is cleared to transparent first, then every contributing
    /// entry (visible, pixel-bearing, non-`Passthrough`) blends over the
    /// accumulator in place, in order. Non-contributing entries cost no
    /// dispatch. `output` must not alias any entry texture (see the module
    /// docs); entries SHOULD match `output`'s dimensions (out-of-range
    /// reads are transparent, never an error).
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
            let params = CompositeParams {
                mode,
                opacity: layer.opacity,
                width,
                height,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::{LayerKind, LayerStack};

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const RED: [u8; 4] = [255, 0, 0, 255];
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
            },
            CpuStackLayer {
                blend_mode: mode,
                opacity,
                visible: true,
                rgba: Some(top_bytes.as_slice()),
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
                        // Alpha: da + (1 - da) * o, independent of mode.
                        assert_eq!(got[3], 0.5 + (1.0 - 0.5) * o, "{mode:?} alpha o={o}");
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
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(red.as_slice()),
            },
        ];
        let with_folder = [
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(white.as_slice()),
            },
            CpuStackLayer {
                blend_mode: BlendMode::Passthrough,
                opacity: 1.0,
                visible: true,
                rgba: None,
            },
            CpuStackLayer {
                blend_mode: BlendMode::Multiply,
                opacity: 1.0,
                visible: false,
                rgba: Some(white.as_slice()),
            },
            CpuStackLayer {
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                rgba: Some(red.as_slice()),
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
        }];
        assert_eq!(
            composite_stack_cpu(&pixel_less, 1, 1).expect("pixel-less stack succeeds"),
            TRANSPARENT.to_vec()
        );
    }

    #[test]
    fn wrong_length_pixel_data_fails_loud() {
        let short = [255u8; 3];
        let layers = [CpuStackLayer {
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            rgba: Some(short.as_slice()),
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
        let cpu = CpuStackLayer::from_layer(layer, Some(red.as_slice()));
        assert_eq!(cpu.blend_mode, BlendMode::Multiply);
        assert_eq!(cpu.opacity, 0.25);
        assert!(!cpu.visible);
        assert_eq!(cpu.rgba, Some(red.as_slice()));
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
                    },
                    GpuStackLayer {
                        blend_mode: mode,
                        opacity,
                        visible: true,
                        texture: Some(&top_tex),
                    },
                ];
                let got = self.composite(&gpu_entries);
                let cpu_entries = [
                    CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(bottom),
                    },
                    CpuStackLayer {
                        blend_mode: mode,
                        opacity,
                        visible: true,
                        rgba: Some(top),
                    },
                ];
                let want = composite_stack_cpu(&cpu_entries, self.size, self.size)
                    .expect("oracle input well-formed");
                assert_bytes_near_lenient(&got, &want);
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
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Passthrough,
                    opacity: 1.0,
                    visible: true,
                    texture: None,
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Multiply,
                    opacity: 1.0,
                    visible: false,
                    texture: Some(&bottom_tex),
                },
                GpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    texture: Some(&top_tex),
                },
            ];
            let got = fix.composite(&gpu_entries);
            let cpu_entries = [
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(white.as_slice()),
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Passthrough,
                    opacity: 1.0,
                    visible: true,
                    rgba: None,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Multiply,
                    opacity: 1.0,
                    visible: false,
                    rgba: Some(white.as_slice()),
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(red.as_slice()),
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
