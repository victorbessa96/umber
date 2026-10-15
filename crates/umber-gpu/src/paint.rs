//! The paint-target compute pass: an RGBA8 storage texture plus a compute
//! pipeline that splats dab batches into it with premultiplied alpha-over
//! compositing.
//!
//! Wave 2 scope (docs/specs/architecture.md "Paint data flow"): one tile is
//! one [`PaintTarget`] texture; the tile-pool allocator that packs many
//! tiles into a shared atlas is a later slice. The dab batch this pass
//! consumes is the GPU-side mirror of `umber_brush::spacing::DabPlan` —
//! `DabPlan` carries stroke-space position + pressure; by the time a batch
//! reaches here it has been resolved into paint-space [`Dab`]s (position,
//! radius, premultiplied color, alpha, hardness) ready to rasterize.
//!
//! **Overlap contract:** a single [`PaintCompositor::splat_dabs`] call is
//! one compute dispatch, one workgroup per dab, with no atomics on the
//! shared storage texture. Dabs within one call must not have overlapping
//! bounding circles, or the two workgroups race on the shared texels.
//! Overlapping dabs (e.g. consecutive dabs along a stroke) must be split
//! across separate `splat_dabs` calls — wgpu's resource hazard tracking
//! orders successive compute passes on the same texture, so a second call
//! always sees the first call's writes. See `shaders::PAINT_COMPUTE_SHADER`
//! for the shader-side half of this contract.
//!
//! **Viewport sampling (core-loop kill 1):** a tile's [`PaintTarget::view`]
//! binds straight into the mesh pass
//! ([`crate::renderer::MeshBuffers::set_base_color_texture`]) as the
//! base-color tile — no copy, no conversion. The target's `Rgba8Unorm`
//! bytes ARE the sampled texels (row 0 is v 0 — mesh UVs are used
//! as-is, no V flip; see [`crate::renderer::PaintParams`]' tile-select
//! convention). Repainting the same target needs no re-bind (the view
//! aliases the texture); recreating it (evict + recreate) does — the
//! bind group holds the old texture alive (cloned view) until replaced.

use std::borrow::Cow;
use std::mem::size_of;
use std::num::NonZeroU64;

use wgpu::util::DeviceExt as _;

use crate::shaders::PAINT_COMPUTE_SHADER;

/// A single stamp to composite into a [`PaintTarget`].
///
/// `color` is premultiplied (`color.rgb` already scaled by `color.a`);
/// `alpha` is a separate, independent flow/opacity multiplier applied on
/// top (so a stroke can fade in/out without re-baking it into `color`).
/// `hardness` is the radial falloff knob: `1.0` is a hard-edged disc, `0.0`
/// falls off linearly-ish from the center.
///
/// Field *declaration* order here is `pos, radius, alpha, color, hardness`
/// rather than the conceptual `pos, radius, color, alpha, hardness` order,
/// plus a trailing `_pad` — purely to make the `#[repr(C)]` byte layout
/// match `shaders::PAINT_COMPUTE_SHADER`'s WGSL `Dab` struct, where
/// `vec4<f32>` forces 16-byte alignment that Rust's `[f32; 4]` (4-byte
/// aligned) wouldn't otherwise produce. [`Dab::new`] takes the conceptual
/// order; the padding field being private means a struct-literal can't be
/// built any other way.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Dab {
    /// Dab center, in target texel space.
    pub pos: [f32; 2],
    /// Dab radius, in texels.
    pub radius: f32,
    /// Flow/opacity multiplier applied on top of `color`'s own alpha.
    pub alpha: f32,
    /// Premultiplied RGBA.
    pub color: [f32; 4],
    /// Radial falloff: `1.0` hard edge, `0.0` soft falloff from the center.
    pub hardness: f32,
    /// Quick-mask gate: `1.0` multiplies the dab's per-texel coverage by
    /// the target's mask texture, `0.0` ignores it. Set through
    /// [`Dab::with_mask_gate`] (it occupies what was the first padding
    /// slot, so the layout is unchanged).
    mask_gate: f32,
    /// Falloff-curve id (one of the `FALLOFF_*` constants) read by the
    /// shader's `radial_falloff` switch. Set through [`Dab::with_falloff`]
    /// (it occupies what was the second padding slot, byte 40, so the
    /// layout is unchanged). `0` = [`FALLOFF_SMOOTH`], the historical curve.
    falloff: u32,
    /// Stamp rotation in radians (C2-11), read only by [`FALLOFF_STAMP`]
    /// dabs. It occupies the last padding slot (byte 44), so the layout is
    /// unchanged and a default dab's bytes are byte-identical to before.
    angle: f32,
}

/// Falloff-curve ids carried in [`Dab`] and switched on by
/// `shaders::PAINT_COMPUTE_SHADER`'s `radial_falloff`. The brush-side enum
/// (`umber_brush::falloff::FalloffCurve`) maps onto these; never renumber.
/// Smoothstep from the hardness core to the rim (the default).
pub const FALLOFF_SMOOTH: u32 = 0;
/// Quintic smootherstep.
pub const FALLOFF_SMOOTHER: u32 = 1;
/// Quarter-circle `sqrt(2u - u²)`.
pub const FALLOFF_SPHERE: u32 = 2;
/// `sqrt(u)`.
pub const FALLOFF_ROOT: u32 = 3;
/// `u²`.
pub const FALLOFF_SHARP: u32 = 4;
/// `u⁴`.
pub const FALLOFF_SHARPER: u32 = 5;
/// `u`.
pub const FALLOFF_LINEAR: u32 = 6;
/// `u(2 - u)`.
pub const FALLOFF_INVERSE_SQUARE: u32 = 7;
/// Flat disc out to the radius.
pub const FALLOFF_CONSTANT: u32 = 8;
/// Bitmap stamp (C2-11): the dab's coverage is the compositor's stamp
/// texture ([`PaintCompositor::set_stamp`]), sampled over the dab's
/// square and rotated by the dab's angle ([`Dab::with_angle`]).
/// `hardness` is ignored.
pub const FALLOFF_STAMP: u32 = 9;

const _: () = assert!(size_of::<Dab>() == 48, "Dab must match the WGSL Dab layout");

impl Dab {
    /// Builds a dab from its conceptual fields (padding is internal). The
    /// dab is not mask-gated.
    pub fn new(pos: [f32; 2], radius: f32, color: [f32; 4], alpha: f32, hardness: f32) -> Self {
        Self {
            pos,
            radius,
            alpha,
            color,
            hardness,
            mask_gate: 0.0,
            falloff: FALLOFF_SMOOTH,
            angle: 0.0,
        }
    }

    /// This dab rotated by `radians` (read by [`FALLOFF_STAMP`] dabs only;
    /// a radial dab is rotation-symmetric). Non-finite input is `0.0`.
    #[must_use]
    pub fn with_angle(mut self, radians: f32) -> Self {
        self.angle = if radians.is_finite() { radians } else { 0.0 };
        self
    }

    /// The stamp rotation this dab carries, in radians.
    pub fn angle(&self) -> f32 {
        self.angle
    }

    /// This dab with falloff-curve id `id` (a `FALLOFF_*` constant). An
    /// unknown id renders as [`FALLOFF_SMOOTH`] in the shader.
    #[must_use]
    pub fn with_falloff(mut self, id: u32) -> Self {
        self.falloff = id;
        self
    }

    /// The falloff-curve id this dab carries.
    pub fn falloff(&self) -> u32 {
        self.falloff
    }

    /// This dab with the quick-mask gate set (`true`) or cleared. A gated
    /// dab's coverage is multiplied by the target's mask texture per
    /// texel; the caller uploads the mask
    /// ([`PaintTarget::write_mask_region`]) before staging gated dabs.
    #[must_use]
    pub fn with_mask_gate(mut self, gated: bool) -> Self {
        self.mask_gate = if gated { 1.0 } else { 0.0 };
        self
    }

    /// Whether the quick-mask gate is set.
    pub fn mask_gated(&self) -> bool {
        self.mask_gate > 0.5
    }
}

/// Errors from staging or splatting a dab batch.
#[derive(Debug, thiserror::Error)]
pub enum PaintError {
    /// `splat_dabs`/`DabBuffer::stage` was called with no dabs.
    #[error("dab batch is empty")]
    EmptyBatch,
    /// The batch exceeds the compositor's/buffer's configured capacity.
    #[error("dab batch of {requested} exceeds capacity {capacity}")]
    CapacityOverflow {
        /// Dabs the caller tried to stage.
        requested: usize,
        /// The configured maximum.
        capacity: usize,
    },
    /// `device` lacks `Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`,
    /// which this pass's `read_write` storage-texture binding requires on
    /// `Rgba8Unorm` (that access mode is only free on the core r32
    /// formats; everything else is a non-portable adapter capability
    /// gated behind this feature). See `LANDING_NOTES_PAINT.md`.
    #[error(
        "device is missing wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, \
         required for read_write storage-texture access on Rgba8Unorm"
    )]
    MissingDeviceFeature,
    /// `PaintThread::publish` tried to send on its command channel after the
    /// receiving end was dropped. `PaintThread` owns both ends for its whole
    /// lifetime, so in practice this is unreachable — it exists because
    /// `mpsc::Sender::send` returns a `Result` that must be handled.
    #[error("paint-thread command channel is closed")]
    ChannelClosed,
    /// [`PaintTarget::read_back_rgba8`](PaintTarget::read_back_rgba8) failed:
    /// device poll, buffer-map callback loss, or the map itself.
    #[error("paint-target readback failed: {0}")]
    Readback(String),
    /// [`PaintTarget::write_region`] got a rect outside the target, an
    /// empty rect, or a byte count that is not `width * height * 4`.
    #[error("region write {width}x{height} at ({x}, {y}) does not fit the target or its bytes")]
    InvalidRegion {
        /// Region origin x, texels.
        x: u32,
        /// Region origin y, texels.
        y: u32,
        /// Region width, texels.
        width: u32,
        /// Region height, texels.
        height: u32,
    },
}

/// CPU-side staging for a dab batch, uploaded to a GPU storage buffer just
/// before a dispatch.
///
/// `capacity` bounds how many dabs one batch may hold — in practice this
/// should never exceed `wgpu::Limits::max_compute_workgroups_per_dimension`
/// (one workgroup per dab), which is exactly what
/// [`PaintCompositor::splat_dabs`] enforces when it stages internally.
pub struct DabBuffer {
    capacity: usize,
    staged: Vec<Dab>,
}

impl DabBuffer {
    /// Builds an empty buffer that rejects batches larger than `capacity`.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            staged: Vec::new(),
        }
    }

    /// The configured maximum batch size.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Replaces the staged batch with `dabs`.
    ///
    /// # Errors
    ///
    /// Returns [`PaintError::EmptyBatch`] if `dabs` is empty, or
    /// [`PaintError::CapacityOverflow`] if it exceeds `capacity`. On error
    /// the previously staged batch is left untouched.
    pub fn stage(&mut self, dabs: &[Dab]) -> Result<(), PaintError> {
        if dabs.is_empty() {
            return Err(PaintError::EmptyBatch);
        }
        if dabs.len() > self.capacity {
            return Err(PaintError::CapacityOverflow {
                requested: dabs.len(),
                capacity: self.capacity,
            });
        }
        self.staged.clear();
        self.staged.extend_from_slice(dabs);
        Ok(())
    }

    /// The currently staged batch.
    pub fn staged(&self) -> &[Dab] {
        &self.staged
    }

    /// Uploads the staged batch to a fresh GPU storage buffer.
    ///
    /// Uses `create_buffer_init` (not `queue.write_buffer`) deliberately:
    /// it writes the bytes synchronously at buffer creation, so there is no
    /// deferred-write ordering hazard if a caller stages and uploads two
    /// different batches onto the same encoder before submitting.
    ///
    /// # Errors
    ///
    /// Returns [`PaintError::EmptyBatch`] if nothing has been staged yet,
    /// rather than handing wgpu a zero-size buffer.
    pub fn upload(&self, device: &wgpu::Device) -> Result<wgpu::Buffer, PaintError> {
        if self.staged.is_empty() {
            return Err(PaintError::EmptyBatch);
        }
        Ok(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_paint_dab_buffer"),
                contents: bytemuck::cast_slice(&self.staged),
                usage: wgpu::BufferUsages::STORAGE,
            }),
        )
    }
}

/// An RGBA8 (linear, `Rgba8Unorm`) GPU paint surface.
///
/// Wave 2 scope: one tile is one `PaintTarget`; the tile-pool atlas that
/// packs many tiles into shared physical pages is a later slice (see
/// docs/specs/architecture.md "Virtual texturing").
pub struct PaintTarget {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    dims_buffer: wgpu::Buffer,
    /// The quick-mask texture (R8, same size as the target), created on
    /// the first [`Self::write_mask_region`]: a session that never masks
    /// never pays for it.
    mask: Option<(wgpu::Texture, wgpu::TextureView)>,
    width: u32,
    height: u32,
}

impl PaintTarget {
    /// Creates a new `width`x`height` paint target.
    ///
    /// Newly created wgpu textures are zero-initialized per the WebGPU
    /// spec, so a fresh target already starts fully transparent; call
    /// [`PaintTarget::clear`] to reset an existing target instead.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_paint_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // COPY_DST: the retouch tools' region write-back
            // (`write_region`) copies CPU-edited texels back in.
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dims_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_paint_target_dims"),
            contents: bytemuck::cast_slice(&[width, height]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        Self {
            texture,
            texture_view,
            dims_buffer,
            mask: None,
            width,
            height,
        }
    }

    /// Clears the whole target to transparent black.
    pub fn clear(&self, encoder: &mut wgpu::CommandEncoder) {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("umber_paint_target_clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        drop(pass);
    }

    /// Target dimensions in texels.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Backing-store size in bytes (RGBA8: 4 bytes/texel). The perf HUD's
    /// paint-target line reads this; it is the v1 proxy for the §12 tile
    /// budget until the tile-pool allocator lands its own accounting.
    pub fn byte_len(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// The backing GPU texture — for readback (e.g. in golden-image tests)
    /// or future tile-pool wiring.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The texture's default view — for display callbacks sampling the
    /// paint surface, and for the mesh pass's base-color tile binding
    /// ([`crate::renderer::MeshBuffers::set_base_color_texture`]).
    pub fn view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    /// Reads the full target back as tightly-packed RGBA8 bytes
    /// (width×height×4, row-major), blocking until the copy completes.
    ///
    /// Rows are padded to 256 bytes for the GPU copy and de-padded on
    /// extraction, so callers get exactly `width * height * 4` bytes —
    /// the format `umber_export::png::write_png` consumes.
    pub fn read_back_rgba8(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<u8>, PaintError> {
        let (width, height) = (self.width, self.height);
        let unpadded_row = width * 4;
        let padding = (256 - (unpadded_row % 256)) % 256;
        let padded_row = unpadded_row + padding;
        let size = padded_row as u64 * height as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("umber_paint_readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("umber_paint_readback_encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| PaintError::Readback(format!("device poll: {e}")))?;
        receiver
            .recv()
            .map_err(|_| PaintError::Readback("map callback channel closed".into()))?
            .map_err(|e| PaintError::Readback(format!("buffer map: {e}")))?;

        let data = slice
            .get_mapped_range()
            .map_err(|e| PaintError::Readback(format!("mapped range: {e}")))?;
        let bytes: &[u8] = &data;
        let mut out = Vec::with_capacity(unpadded_row as usize * height as usize);
        for row in 0..height as usize {
            let start = row * padded_row as usize;
            out.extend_from_slice(&bytes[start..start + unpadded_row as usize]);
        }
        drop(data);
        buffer.unmap();
        Ok(out)
    }

    /// Records a copy of tightly-packed RGBA8 `rgba` (`width * height * 4`
    /// bytes, row-major) into the `width`x`height` region at `(x, y)` —
    /// the inverse of [`Self::read_back_rgba8`] for one rect, used by the
    /// retouch tools' write-back.
    ///
    /// Recorded on `encoder` (a `create_buffer_init` staging buffer +
    /// `copy_buffer_to_texture`), NOT via `queue.write_texture`: a queue
    /// write lands at submit time, ahead of every dispatch already on the
    /// encoder — the same ordering hazard [`PaintCompositor`] documents for
    /// `queue.write_buffer`. Recorded, it serializes with the splat passes
    /// in command order.
    ///
    /// # Errors
    ///
    /// Returns [`PaintError::InvalidRegion`] for an empty or out-of-bounds
    /// rect, or a byte count that doesn't match it.
    pub fn write_region(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (x, y): (u32, u32),
        (width, height): (u32, u32),
        rgba: &[u8],
    ) -> Result<(), PaintError> {
        let fits = width > 0
            && height > 0
            && x.checked_add(width).is_some_and(|x1| x1 <= self.width)
            && y.checked_add(height).is_some_and(|y1| y1 <= self.height)
            && rgba.len() == width as usize * height as usize * 4;
        if !fits {
            return Err(PaintError::InvalidRegion {
                x,
                y,
                width,
                height,
            });
        }
        let unpadded_row = width * 4;
        let padding = (256 - (unpadded_row % 256)) % 256;
        let padded_row = unpadded_row + padding;
        let mut padded = Vec::with_capacity(padded_row as usize * height as usize);
        for row in rgba.chunks_exact(unpadded_row as usize) {
            padded.extend_from_slice(row);
            padded.resize(padded.len() + padding as usize, 0);
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_paint_region_upload"),
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
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Records a copy of tightly-packed 8-bit quick-mask `values`
    /// (`width * height` bytes, row-major, `255` = paint allowed) into the
    /// `width`x`height` region of the mask texture at `(x, y)`, creating
    /// the (zero = blocked) texture on first use. Recorded on `encoder`
    /// like [`Self::write_region`], so it serializes with the splat
    /// passes in command order.
    ///
    /// # Errors
    ///
    /// Returns [`PaintError::InvalidRegion`] for an empty or out-of-bounds
    /// rect, or a byte count that doesn't match it.
    pub fn write_mask_region(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (x, y): (u32, u32),
        (width, height): (u32, u32),
        values: &[u8],
    ) -> Result<(), PaintError> {
        let fits = width > 0
            && height > 0
            && x.checked_add(width).is_some_and(|x1| x1 <= self.width)
            && y.checked_add(height).is_some_and(|y1| y1 <= self.height)
            && values.len() == width as usize * height as usize;
        if !fits {
            return Err(PaintError::InvalidRegion {
                x,
                y,
                width,
                height,
            });
        }
        let (target_width, target_height) = (self.width, self.height);
        let (mask_texture, _) = self.mask.get_or_insert_with(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_paint_quick_mask"),
                size: wgpu::Extent3d {
                    width: target_width,
                    height: target_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        });
        let padding = (256 - (width % 256)) % 256;
        let padded_row = width + padding;
        let mut padded = Vec::with_capacity(padded_row as usize * height as usize);
        for row in values.chunks_exact(width as usize) {
            padded.extend_from_slice(row);
            padded.resize(padded.len() + padding as usize, 0);
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_paint_mask_upload"),
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
                texture: mask_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Whether the quick-mask texture exists (a mask was uploaded since
    /// the target was created).
    pub fn has_mask(&self) -> bool {
        self.mask.is_some()
    }
}

/// Owns the dab-splat compute pipeline and bind-group layout shared by
/// every [`PaintTarget`] it composites into.
///
/// A fresh dab storage buffer and bind group are built on every
/// [`PaintCompositor::splat_dabs`] call rather than cached per-target: the
/// dab buffer's *contents* change every call (and `wgpu::BindGroup`s bind
/// to a buffer's identity, not a snapshot of its contents), so a cached
/// bind group would either need to be rebuilt anyway or reuse a persistent
/// buffer via `queue.write_buffer` — whose write lands at submit time, not
/// encoder-record time, breaking the ordering two `splat_dabs` calls on one
/// encoder depend on. `splat_dabs` takes `&self` accordingly: there's no
/// cached, mutated state to protect.
pub struct PaintCompositor {
    device: wgpu::Device,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// The 1x1 mask bound for a target with no quick-mask texture yet
    /// (binding 3 must always hold a view; only gated dabs read it, and
    /// the caller uploads a mask before staging those).
    _fallback_mask: wgpu::Texture,
    fallback_mask_view: wgpu::TextureView,
    /// The bitmap stamp [`FALLOFF_STAMP`] dabs sample (binding 4): a 1x1
    /// white texture until [`Self::set_stamp`] uploads one, so a stamp
    /// dab with no stamp paints the full square.
    stamp: (wgpu::Texture, wgpu::TextureView),
    /// Whether the white fallback's single texel still needs its upload
    /// (recorded on the next splat's encoder: construction has no queue).
    stamp_needs_white: std::sync::atomic::AtomicBool,
    max_dabs_per_batch: usize,
}

impl PaintCompositor {
    /// Builds the compute pipeline against `device`.
    ///
    /// `device` is cloned (wgpu resource handles are internally
    /// `Arc`-backed, so this is cheap) and kept so `splat_dabs` can create
    /// the per-call dab buffer and bind group without an extra parameter.
    ///
    /// # Errors
    ///
    /// Returns [`PaintError::MissingDeviceFeature`] if `device` wasn't
    /// created with `Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` —
    /// checked up front so this fails with a typed error instead of a wgpu
    /// validation panic partway through building the bind-group layout.
    pub fn new(device: wgpu::Device) -> Result<Self, PaintError> {
        if !device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            return Err(PaintError::MissingDeviceFeature);
        }

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_paint_compute_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(PAINT_COMPUTE_SHADER)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_paint_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<Dab>() as u64),
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
                        min_binding_size: NonZeroU64::new(size_of::<[u32; 2]>() as u64),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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

        let fallback_mask = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_paint_quick_mask_fallback"),
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
            label: Some("umber_paint_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("umber_paint_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let max_dabs_per_batch = device.limits().max_compute_workgroups_per_dimension as usize;
        let stamp = Self::stamp_texture(&device, None, 1, 1, &[255]);

        Ok(Self {
            device,
            pipeline,
            bind_group_layout,
            _fallback_mask: fallback_mask,
            fallback_mask_view,
            stamp,
            stamp_needs_white: std::sync::atomic::AtomicBool::new(true),
            max_dabs_per_batch,
        })
    }

    /// Uploads `values` (`width * height` R8 coverage, row-major, top row
    /// first) as the stamp every [`FALLOFF_STAMP`] dab samples from now
    /// on. Uploaded through `queue` at once, so call it between strokes,
    /// not mid-batch: a splat recorded before the call but submitted after
    /// it would sample the new stamp.
    ///
    /// # Errors
    ///
    /// [`PaintError::InvalidRegion`] for a zero dimension, a byte count
    /// that does not match, or a side beyond the device's 2D texture limit.
    pub fn set_stamp(
        &mut self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        values: &[u8],
    ) -> Result<(), PaintError> {
        let max = self.device.limits().max_texture_dimension_2d;
        if width == 0
            || height == 0
            || width > max
            || height > max
            || values.len() != width as usize * height as usize
        {
            return Err(PaintError::InvalidRegion {
                x: 0,
                y: 0,
                width,
                height,
            });
        }
        self.stamp = Self::stamp_texture(&self.device, Some(queue), width, height, values);
        self.stamp_needs_white
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Back to the white fallback (stamp dabs paint the full square).
    pub fn clear_stamp(&mut self) {
        self.stamp = Self::stamp_texture(&self.device, None, 1, 1, &[255]);
        self.stamp_needs_white
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// The current stamp's dimensions (`(1, 1)` = the white fallback).
    pub fn stamp_size(&self) -> (u32, u32) {
        let size = self.stamp.0.size();
        (size.width, size.height)
    }

    /// An R8 stamp texture holding `values`. With a `queue` the bytes go
    /// through `write_texture`; without one (construction and
    /// `clear_stamp`, the 1x1 white fallback) the texture is created empty
    /// and `splat_dabs` records its single white texel on the next
    /// encoder (`stamp_needs_white`).
    fn stamp_texture(
        device: &wgpu::Device,
        queue: Option<&wgpu::Queue>,
        width: u32,
        height: u32,
        values: &[u8],
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let desc = wgpu::TextureDescriptor {
            label: Some("umber_paint_stamp"),
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
        };
        let texture = match queue {
            Some(queue) => {
                let texture = device.create_texture(&desc);
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    values,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width),
                        rows_per_image: Some(height),
                    },
                    desc.size,
                );
                texture
            }
            None => device.create_texture(&desc),
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// The maximum dabs accepted in one [`PaintCompositor::splat_dabs`]
    /// call — one compute workgroup per dab, bounded by
    /// `wgpu::Limits::max_compute_workgroups_per_dimension`.
    pub fn max_dabs_per_batch(&self) -> usize {
        self.max_dabs_per_batch
    }

    /// Records one compute dispatch that splats `dabs` into `target`: one
    /// workgroup per dab, premultiplied alpha-over compositing.
    ///
    /// See the module-level docs for the overlap contract: dabs within one
    /// call must not spatially overlap.
    ///
    /// # Errors
    ///
    /// Returns [`PaintError::EmptyBatch`] if `dabs` is empty, or
    /// [`PaintError::CapacityOverflow`] if `dabs.len()` exceeds
    /// [`PaintCompositor::max_dabs_per_batch`].
    pub fn splat_dabs(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &PaintTarget,
        dabs: &[Dab],
    ) -> Result<(), PaintError> {
        let mut staging = DabBuffer::new(self.max_dabs_per_batch);
        staging.stage(dabs)?;
        let dab_buffer = staging.upload(&self.device)?;
        if self
            .stamp_needs_white
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            // The fallback's one white texel, recorded ahead of the pass
            // (256-byte row pitch, the copy minimum).
            let mut row = vec![0u8; 256];
            row[0] = 255;
            let white = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("umber_paint_stamp_white"),
                    contents: &row,
                    usage: wgpu::BufferUsages::COPY_SRC,
                });
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &white,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.stamp.0,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("umber_paint_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &dab_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&target.texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &target.dims_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        target
                            .mask
                            .as_ref()
                            .map_or(&self.fallback_mask_view, |(_, view)| view),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.stamp.1),
                },
            ],
        });

        let dab_count = dabs.len() as u32;
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("umber_paint_splat_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(dab_count, 1, 1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dab_buffer_rejects_empty_batch() {
        let mut buf = DabBuffer::new(16);
        assert!(matches!(buf.stage(&[]), Err(PaintError::EmptyBatch)));
    }

    #[test]
    fn dab_buffer_rejects_overflow() {
        let mut buf = DabBuffer::new(1);
        let dabs = [
            Dab::new([0.0, 0.0], 1.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0),
            Dab::new([1.0, 1.0], 1.0, [0.0, 1.0, 0.0, 1.0], 1.0, 1.0),
        ];
        assert!(matches!(
            buf.stage(&dabs),
            Err(PaintError::CapacityOverflow {
                requested: 2,
                capacity: 1
            })
        ));
    }

    #[test]
    fn dab_buffer_stage_replaces_previous_batch() {
        let mut buf = DabBuffer::new(4);
        let first = [Dab::new([0.0, 0.0], 1.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0)];
        buf.stage(&first).expect("first batch fits");
        assert_eq!(buf.staged().len(), 1);

        let second = [
            Dab::new([0.0, 0.0], 1.0, [0.0, 1.0, 0.0, 1.0], 1.0, 1.0),
            Dab::new([1.0, 1.0], 1.0, [0.0, 0.0, 1.0, 1.0], 1.0, 1.0),
        ];
        buf.stage(&second).expect("second batch fits");
        assert_eq!(buf.staged().len(), 2);
        assert_eq!(buf.staged()[0].color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn dab_layout_matches_wgsl_struct_size() {
        assert_eq!(size_of::<Dab>(), 48);
    }

    /// The per-layer-pool share (stage 1b, [M4]): every plane and cache
    /// thread holds one [`PaintCompositor`] behind an `Arc`, so this pins
    /// `Send + Sync` — the bound that makes the share legal. The fields
    /// are all wgpu handle types (`Device` is asserted `Send + Sync` in
    /// the wgpu 30.0.1 source; the rest are the same Arc-backed family).
    #[test]
    fn paint_compositor_is_send_sync_for_pool_sharing() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PaintCompositor>();
    }

    #[test]
    fn mask_gate_defaults_off_and_round_trips() {
        let dab = Dab::new([1.0, 2.0], 3.0, [1.0, 0.0, 0.0, 1.0], 0.5, 0.25);
        assert!(!dab.mask_gated());
        let gated = dab.with_mask_gate(true);
        assert!(gated.mask_gated());
        // The gate rides in the old first padding slot (byte 36): the
        // conceptual fields are untouched.
        assert_eq!(bytemuck::bytes_of(&gated)[36..40], 1.0f32.to_ne_bytes());
        assert_eq!(gated.pos, dab.pos);
        assert_eq!(gated.hardness, dab.hardness);
        assert!(!gated.with_mask_gate(false).mask_gated());
    }

    #[test]
    fn falloff_defaults_smooth_and_rides_in_byte_40() {
        let dab = Dab::new([1.0, 2.0], 3.0, [1.0, 0.0, 0.0, 1.0], 0.5, 0.25);
        assert_eq!(dab.falloff(), FALLOFF_SMOOTH);
        // A default dab's former padding bytes stay zero: the pre-curve
        // upload is byte-identical (the angle slot included).
        assert_eq!(bytemuck::bytes_of(&dab)[40..48], [0u8; 8]);
        let turned = dab.with_angle(1.25);
        assert_eq!(bytemuck::bytes_of(&turned)[44..48], 1.25f32.to_ne_bytes());
        assert_eq!(dab.with_angle(f32::NAN).angle(), 0.0);
        let sharp = dab.with_falloff(FALLOFF_SHARP).with_mask_gate(true);
        assert_eq!(sharp.falloff(), FALLOFF_SHARP);
        assert_eq!(
            bytemuck::bytes_of(&sharp)[40..44],
            FALLOFF_SHARP.to_ne_bytes()
        );
        assert_eq!(bytemuck::bytes_of(&sharp)[36..40], 1.0f32.to_ne_bytes());
        assert_eq!(sharp.hardness, dab.hardness);
    }

    /// The WGSL `Dab` must declare the id as `u32` at the same slot.
    #[test]
    fn shader_dab_struct_carries_the_falloff_slot() {
        assert!(
            PAINT_COMPUTE_SHADER.contains("mask_gate: f32,\n    falloff: u32,\n    angle: f32,")
        );
        assert!(PAINT_COMPUTE_SHADER.contains(&format!("FALLOFF_STAMP: u32 = {FALLOFF_STAMP}u")));
        for id in 0..=FALLOFF_CONSTANT {
            assert!(
                PAINT_COMPUTE_SHADER.contains(&format!("case {id}u")),
                "shader lacks a case for falloff id {id}"
            );
        }
    }

    #[cfg(feature = "gpu")]
    mod gpu {
        use super::super::*;

        /// Requests a device with `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`
        /// enabled.
        ///
        /// `Rgba8Unorm` reports `STORAGE_READ_WRITE` in
        /// `Adapter::get_texture_format_features` on this sandbox's
        /// adapter, but wgpu only grants non-portable per-format
        /// capabilities (read-write storage access on anything other than
        /// the r32 formats) when this feature is requested on the device —
        /// without it, `PaintCompositor::new`'s bind-group-layout creation
        /// hits a validation error. See `LANDING_NOTES_PAINT.md` for the
        /// production-wiring consequence (eframe's device needs this too).
        fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
            let instance = wgpu::Instance::default();
            let Ok(adapter) = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            ) else {
                eprintln!("skipping: no wgpu adapter available");
                return None;
            };
            if !adapter
                .features()
                .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
            {
                eprintln!(
                    "skipping: adapter {:?} lacks wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES",
                    adapter.get_info().name
                );
                return None;
            }
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                ..Default::default()
            }))
            .ok()
        }

        fn read_back_rgba8(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            target: &PaintTarget,
        ) -> Vec<u8> {
            let (width, height) = target.dimensions();
            let unpadded_row = width * 4;
            let padding = (256 - (unpadded_row % 256)) % 256;
            let padded_row = unpadded_row + padding;
            let size = padded_row as u64 * height as u64;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("umber_paint_test_readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_test_readback_encoder"),
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: target.texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));

            let slice = buffer.slice(..);
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .expect("poll should succeed in tests");
            receiver
                .recv()
                .expect("map_async callback should fire")
                .expect("buffer map should succeed");

            let data = slice.get_mapped_range().expect("mapped range");
            let bytes: &[u8] = &data;
            let mut out = Vec::with_capacity(unpadded_row as usize * height as usize);
            for row in 0..height as usize {
                let start = row * padded_row as usize;
                out.extend_from_slice(&bytes[start..start + unpadded_row as usize]);
            }
            drop(data);
            buffer.unmap();
            out
        }

        fn pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
            let i = (y * width + x) as usize * 4;
            [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
        }

        /// Splats one dab with `compositor` into a fresh 64² target and
        /// reads it back.
        fn splat_one(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            compositor: &PaintCompositor,
            dab: Dab,
        ) -> Vec<u8> {
            let target = PaintTarget::new(device, 64, 64);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_stamp_test"),
            });
            compositor
                .splat_dabs(&mut encoder, &target, &[dab])
                .expect("splats");
            queue.submit(Some(encoder.finish()));
            read_back_rgba8(device, queue, &target)
        }

        #[test]
        fn a_stamp_dab_without_a_stamp_paints_its_full_square() {
            // C2-11: the 1x1 white fallback = full coverage over the dab's
            // square, corners included (a radial dab leaves them empty).
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let compositor = PaintCompositor::new(device.clone()).expect("compositor");
            assert_eq!(compositor.stamp_size(), (1, 1));
            let dab = Dab::new([32.0, 32.0], 16.0, [1.0, 0.0, 0.0, 1.0], 1.0, 0.0)
                .with_falloff(FALLOFF_STAMP);
            let out = splat_one(&device, &queue, &compositor, dab);
            // Inside the square's corner (17, 17): outside the r=16 disc.
            assert_eq!(pixel(&out, 64, 17, 17)[3], 255, "the corner is painted");
            assert_eq!(pixel(&out, 64, 32, 32)[3], 255);
            assert_eq!(
                pixel(&out, 64, 14, 32)[3],
                0,
                "outside the square stays clear"
            );
            // The same dab radial leaves that corner clear.
            let radial = Dab::new([32.0, 32.0], 16.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0);
            let out = splat_one(&device, &queue, &compositor, radial);
            assert_eq!(pixel(&out, 64, 17, 17)[3], 0);
        }

        #[test]
        fn a_stamp_shapes_the_dab_and_the_angle_rotates_it() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mut compositor = PaintCompositor::new(device.clone()).expect("compositor");
            // An 8x8 stamp: left half opaque, right half empty.
            let stamp: Vec<u8> = (0..64).map(|i| if i % 8 < 4 { 255 } else { 0 }).collect();
            compositor
                .set_stamp(&queue, 8, 8, &stamp)
                .expect("stamp uploads");
            assert_eq!(compositor.stamp_size(), (8, 8));
            let dab = Dab::new([32.0, 32.0], 16.0, [0.0, 1.0, 0.0, 1.0], 1.0, 0.0)
                .with_falloff(FALLOFF_STAMP);
            let out = splat_one(&device, &queue, &compositor, dab);
            assert_eq!(pixel(&out, 64, 22, 32)[3], 255, "left half painted");
            assert_eq!(pixel(&out, 64, 42, 32)[3], 0, "right half empty");

            // Rotated a quarter turn, the opaque half moves to the top or
            // bottom: the left and right probes now read the same, and a
            // vertical pair differs.
            let turned = dab.with_angle(std::f32::consts::FRAC_PI_2);
            let out = splat_one(&device, &queue, &compositor, turned);
            let (left, right) = (pixel(&out, 64, 22, 32)[3], pixel(&out, 64, 42, 32)[3]);
            let (top, bottom) = (pixel(&out, 64, 32, 22)[3], pixel(&out, 64, 32, 42)[3]);
            assert_eq!(left, right, "rotated: left/right are no longer split");
            assert_ne!(top, bottom, "rotated: the split is vertical now");
            assert!(top == 255 || bottom == 255);

            // Bad uploads are refused and leave the stamp in place.
            assert!(compositor.set_stamp(&queue, 0, 8, &[]).is_err());
            assert!(compositor.set_stamp(&queue, 8, 8, &stamp[..10]).is_err());
            assert_eq!(compositor.stamp_size(), (8, 8));
            compositor.clear_stamp();
            assert_eq!(compositor.stamp_size(), (1, 1));
            // The fallback after a clear is white again (the deferred
            // upload re-arms).
            let out = splat_one(&device, &queue, &compositor, dab);
            assert_eq!(pixel(&out, 64, 42, 32)[3], 255, "cleared: full square");
        }

        #[test]
        fn splat_one_dab_paints_center_leaves_edges_untouched() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let compositor =
                PaintCompositor::new(device.clone()).expect("device has the required feature");
            let target = PaintTarget::new(&device, 64, 64);

            let red = Dab::new([32.0, 32.0], 16.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_test_encoder"),
            });
            compositor
                .splat_dabs(&mut encoder, &target, &[red])
                .expect("single dab batch should splat");
            queue.submit(Some(encoder.finish()));

            let bytes = read_back_rgba8(&device, &queue, &target);
            let center = pixel(&bytes, 64, 32, 32);
            assert!(center[0] > 200, "center should be strongly red: {center:?}");
            assert!(
                center[1] < 20 && center[2] < 20,
                "center should have no g/b: {center:?}"
            );
            assert!(center[3] > 200, "center should be opaque: {center:?}");

            // Just outside the dab's radius (16 + 2 margin) along +x.
            let edge = pixel(&bytes, 64, 50, 32);
            assert_eq!(
                edge,
                [0, 0, 0, 0],
                "outside the dab radius should be untouched: {edge:?}"
            );

            let corner = pixel(&bytes, 64, 0, 0);
            assert_eq!(
                corner,
                [0, 0, 0, 0],
                "corner should be untouched: {corner:?}"
            );
        }

        /// The quick-mask gate, per texel: a hard dab straddling a mask
        /// edge (left half blocked, right half allowed) paints only the
        /// allowed half; the same dab ungated paints both halves.
        #[test]
        fn mask_gated_dab_paints_only_where_the_mask_allows() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let compositor =
                PaintCompositor::new(device.clone()).expect("device has the required feature");
            let mut target = PaintTarget::new(&device, 64, 64);
            let mask: Vec<u8> = (0..64 * 64)
                .map(|i| if i % 64 < 32 { 0 } else { 255 })
                .collect();
            let red = Dab::new([32.0, 32.0], 16.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_test_encoder"),
            });
            target
                .write_mask_region(&device, &mut encoder, (0, 0), (64, 64), &mask)
                .expect("full-size mask fits");
            assert!(target.has_mask());
            compositor
                .splat_dabs(&mut encoder, &target, &[red.with_mask_gate(true)])
                .expect("gated dab splats");
            queue.submit(Some(encoder.finish()));
            let bytes = read_back_rgba8(&device, &queue, &target);
            assert_eq!(pixel(&bytes, 64, 24, 32), [0, 0, 0, 0], "blocked half");
            assert!(pixel(&bytes, 64, 40, 32)[0] > 200, "allowed half paints");

            // Ungated: the mask is bound but ignored.
            let plain = PaintTarget::new(&device, 64, 64);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_test_encoder"),
            });
            compositor
                .splat_dabs(&mut encoder, &plain, &[red])
                .expect("ungated dab splats");
            queue.submit(Some(encoder.finish()));
            let bytes = read_back_rgba8(&device, &queue, &plain);
            assert!(pixel(&bytes, 64, 24, 32)[0] > 200);
            assert!(pixel(&bytes, 64, 40, 32)[0] > 200);
        }

        /// A non-default falloff reaches the shader: the same soft dab
        /// (hardness 0, r 16) with `FALLOFF_SHARP` paints a visibly
        /// dimmer centre texel than `FALLOFF_SMOOTH`, and each texel's
        /// alpha matches the CPU formula (byte-rounded, ±1 for the unorm
        /// conversion — implementation-defined rounding).
        #[test]
        fn sharp_falloff_dims_the_centre_texel_versus_smooth() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let compositor =
                PaintCompositor::new(device.clone()).expect("device has the required feature");
            let paint = |id: u32| {
                let target = PaintTarget::new(&device, 64, 64);
                let dab =
                    Dab::new([32.0, 32.0], 16.0, [1.0, 0.0, 0.0, 1.0], 1.0, 0.0).with_falloff(id);
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("umber_paint_test_falloff_encoder"),
                });
                compositor
                    .splat_dabs(&mut encoder, &target, &[dab])
                    .expect("falloff dab splats");
                queue.submit(Some(encoder.finish()));
                read_back_rgba8(&device, &queue, &target)
            };
            let smooth = paint(FALLOFF_SMOOTH);
            let sharp = paint(FALLOFF_SHARP);
            // Texel (32,32)'s centre is (32.5,32.5): t = sqrt(0.5)/16.
            let t = 0.5f32.sqrt() / 16.0;
            let want_smooth = 1.0 - t * t * (3.0 - 2.0 * t); // ~0.9943
            let want_sharp = (1.0 - t) * (1.0 - t); // ~0.9135
            let byte = |v: f32| (v * 255.0).round() as i32;
            let got_smooth = pixel(&smooth, 64, 32, 32)[3] as i32;
            let got_sharp = pixel(&sharp, 64, 32, 32)[3] as i32;
            assert!(
                (got_smooth - byte(want_smooth)).abs() <= 1,
                "smooth centre {got_smooth} vs {}",
                byte(want_smooth)
            );
            assert!(
                (got_sharp - byte(want_sharp)).abs() <= 1,
                "sharp centre {got_sharp} vs {}",
                byte(want_sharp)
            );
            assert!(
                got_smooth - got_sharp >= 15,
                "sharp must dim the centre: smooth {got_smooth}, sharp {got_sharp}"
            );
        }

        #[test]
        fn splat_two_dabs_composites_overlap_alpha_over() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let compositor =
                PaintCompositor::new(device.clone()).expect("device has the required feature");
            let target = PaintTarget::new(&device, 64, 64);

            // Two half-alpha dabs, far enough apart that neither call's
            // single dab overlaps itself, but their footprints overlap each
            // other — hence two separate splat_dabs calls (see the module
            // contract) rather than one batch of two.
            let red = Dab::new([28.0, 32.0], 16.0, [1.0, 0.0, 0.0, 1.0], 0.5, 1.0);
            let blue = Dab::new([36.0, 32.0], 16.0, [0.0, 0.0, 1.0, 1.0], 0.5, 1.0);

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_test_encoder_overlap"),
            });
            compositor
                .splat_dabs(&mut encoder, &target, &[red])
                .expect("red batch should splat");
            compositor
                .splat_dabs(&mut encoder, &target, &[blue])
                .expect("blue batch should splat");
            queue.submit(Some(encoder.finish()));

            let bytes = read_back_rgba8(&device, &queue, &target);

            // Red-only region: inside red's radius (center 28, r=16 ->
            // [12, 44]) but outside blue's (center 36, r=16 -> [20, 52]).
            let red_only = pixel(&bytes, 64, 14, 32);
            assert!(
                red_only[0] > 100,
                "red-only region should be red-ish: {red_only:?}"
            );
            assert!(
                red_only[2] < 20,
                "red-only region should have no blue: {red_only:?}"
            );

            // Blue-only region: inside blue's radius but outside red's.
            let blue_only = pixel(&bytes, 64, 50, 32);
            assert!(
                blue_only[2] > 100,
                "blue-only region should be blue-ish: {blue_only:?}"
            );
            assert!(
                blue_only[0] < 20,
                "blue-only region should have no red: {blue_only:?}"
            );

            // Overlap region at the midpoint: blue was drawn second (over
            // red) at half alpha, so it should carry both some red
            // (showing through) and the dominant blue contribution.
            let overlap = pixel(&bytes, 64, 32, 32);
            assert!(
                overlap[2] > overlap[0],
                "blue drawn over red should dominate: {overlap:?}"
            );
            assert!(
                overlap[0] > 20,
                "red should still show through underneath: {overlap:?}"
            );
            // Two 0.5-alpha overs compose to 1 - (1 - 0.5)^2 = 0.75 coverage
            // (~191/255), not full opacity.
            assert!(
                (170..210).contains(&overlap[3]),
                "two half-alpha overs should land near 0.75 coverage: {overlap:?}"
            );
        }

        /// Exercises, in one batch, everything the two tests above don't:
        /// `workgroup_id.x` beyond 0 (three dabs → the shader indexes
        /// `dabs[1]`/`dabs[2]`, proving the 48-byte stride is right, not
        /// just that one `dabs[0]` read works), bounding-box clamping
        /// against both the min and max edges of the target, and a
        /// non-hard (`hardness = 0.0`) falloff. Finishes by clearing the
        /// target and confirming it goes back to fully transparent.
        #[test]
        fn splat_three_dabs_one_batch_clamps_edges_and_falls_off_then_clears() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let compositor =
                PaintCompositor::new(device.clone()).expect("device has the required feature");
            let target = PaintTarget::new(&device, 64, 64);

            // dab0: hard disc clipped against the min (0,0) corner.
            let corner_min = Dab::new([2.0, 2.0], 6.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0);
            // dab1: soft (hardness 0) falloff, fully interior — no clamping.
            let soft = Dab::new([32.0, 32.0], 12.0, [0.0, 1.0, 0.0, 1.0], 1.0, 0.0);
            // dab2: hard disc clipped against the max (63,63) corner.
            let corner_max = Dab::new([62.0, 62.0], 6.0, [0.0, 0.0, 1.0, 1.0], 1.0, 1.0);
            // None of the three bounding circles overlap each other.
            let dabs = [corner_min, soft, corner_max];

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_test_encoder_three"),
            });
            compositor
                .splat_dabs(&mut encoder, &target, &dabs)
                .expect("three-dab batch should splat");
            queue.submit(Some(encoder.finish()));

            let bytes = read_back_rgba8(&device, &queue, &target);

            // dab0 clamped against the min corner: (0,0) is still inside
            // its radius (distance ~2.1 from center (2,2)) even though the
            // dab's unclamped bounding box would start at x,y = -4.
            let min_corner = pixel(&bytes, 64, 0, 0);
            assert!(
                min_corner[0] > 200 && min_corner[1] < 20 && min_corner[2] < 20,
                "min-corner dab should paint (0,0) red: {min_corner:?}"
            );

            // dab2 clamped against the max corner: (63,63) is inside its
            // radius even though the unclamped bounding box would end at
            // x,y = 68 (target is only 64 wide/tall, max index 63).
            let max_corner = pixel(&bytes, 64, 63, 63);
            assert!(
                max_corner[2] > 200 && max_corner[0] < 20 && max_corner[1] < 20,
                "max-corner dab should paint (63,63) blue: {max_corner:?}"
            );

            // dab1's soft falloff: alpha strictly decreases from the
            // center outward toward the radius-12 edge.
            let soft_center = pixel(&bytes, 64, 32, 32);
            let soft_mid = pixel(&bytes, 64, 38, 32);
            let soft_edge = pixel(&bytes, 64, 42, 32);
            assert!(
                soft_center[3] > 240,
                "soft dab center should be nearly opaque: {soft_center:?}"
            );
            assert!(
                soft_center[3] > soft_mid[3] && soft_mid[3] > soft_edge[3],
                "soft dab alpha should strictly decrease outward: center={soft_center:?} mid={soft_mid:?} edge={soft_edge:?}"
            );

            // Points inside none of the three dabs' radii stay untouched.
            let untouched_a = pixel(&bytes, 64, 15, 50);
            let untouched_b = pixel(&bytes, 64, 63, 0);
            assert_eq!(
                untouched_a,
                [0, 0, 0, 0],
                "point outside all three dabs should be untouched: {untouched_a:?}"
            );
            assert_eq!(
                untouched_b,
                [0, 0, 0, 0],
                "point outside all three dabs should be untouched: {untouched_b:?}"
            );

            // clear() resets the whole target back to transparent black.
            let mut clear_encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("umber_paint_test_clear_encoder"),
                });
            target.clear(&mut clear_encoder);
            queue.submit(Some(clear_encoder.finish()));

            let cleared = read_back_rgba8(&device, &queue, &target);
            assert!(
                cleared.iter().all(|&b| b == 0),
                "clear() should reset every byte to 0"
            );
        }
    }
}
