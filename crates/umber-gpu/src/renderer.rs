//! The mesh render pass: pipeline, GPU buffers, and the egui paint-callback
//! glue that lets it draw into egui's shared wgpu render pass.
//!
//! Pipeline descriptor shapes are copied from the vendored egui-wgpu 0.36.2
//! renderer (`docs/claw-artifacts/umber-gpu/egui_wgpu_renderer.rs:354-436`),
//! not written from memory, since wgpu 30 changed several field shapes
//! (`depth_write_enabled`/`depth_compare` became `Option`, pipeline layouts
//! take `immediate_size`, vertex/fragment states take `entry_point: Option`
//! and `compilation_options`).

use std::borrow::Cow;
use std::mem::size_of;
use std::num::NonZeroU64;

use wgpu::util::DeviceExt as _;

use crate::ibl::{EnvFlags, EnvIrradiance, SpecParams};
use crate::shaders::{GRID_SHADER, MESH_SHADER, WIREFRAME_SHADER};
use crate::view_mode::{ViewMode, ViewModeParams};

/// Errors raised while preparing GPU-side mesh data.
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    /// The source mesh had zero vertices.
    #[error("mesh has no vertices")]
    EmptyMesh,
    /// The index buffer length wasn't a multiple of 3, so it can't be a
    /// triangle list.
    #[error("mesh index count {0} is not a multiple of 3 (not a triangle list)")]
    InvalidIndexCount(usize),
    /// Index count exceeds u32::MAX — cannot be expressed in a
    /// [`wgpu::IndexFormat::Uint32`] draw range (review #3).
    #[error("index count {0} exceeds u32::MAX")]
    IndexCountOverflow(usize),
}

/// Interleaved vertex layout uploaded to the GPU: position + normal + uv,
/// 32 bytes.
///
/// The uv slot is the core-loop kill-1 handoff: the shader samples the
/// bound paint tile at the interpolated uv, so the viewport shows paint.
/// Uploaded verbatim from [`umber_mesh::MeshData::uvs`] when the counts
/// match; zero-filled when the source mesh has no (or mismatched) uvs —
/// a uv-less mesh then samples tile-local (0, 0) everywhere, which only
/// matters once a texture is bound (the unbound fallback ignores uv).
/// Replacing the old `_pad` keeps the stride at 32 bytes, so the
/// OpenPBR pipeline (which reads locations 0–1 only) and every buffer
/// size in the tree are untouched.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Object-space normal (recomputed on upload if the source mesh lacks
    /// one — see [`MeshBuffers::upload`]).
    pub normal: [f32; 3],
    /// Paint UV, in the mesh's own uv space (global UDIM uvs for a
    /// multi-tile mesh — the `PaintParams` tile select reduces them).
    pub uv: [f32; 2],
}

const _: () = assert!(
    size_of::<Vertex>() == 32,
    "Vertex must stay position + normal + uv (32 bytes)"
);

/// The mesh pass's base-color + height paint uniform (`MESH_SHADER`
/// binding 11): the tile-select for the bound paint/height textures,
/// their bound flags, and the height perturbation's strength.
/// Byte-identical to the WGSL `PaintParams` struct (see that block's
/// layout contract) — 48 bytes (grown from 32 for the height→normal
/// live kill, audit worklist #12 — `height_strength` plus padding).
///
/// # UDIM scale convention (v1)
///
/// The bound texture holds ONE tile's texels; the shader samples
/// `(uv - uv_offset) * uv_scale`. [`Self::identity`] (offset 0, scale 1)
/// is the single-tile path (uvs already 0..1). [`Self::for_udim`] maps a
/// UDIM tile number to its origin: tile (tu, tv) binds offset (tu, tv),
/// scale (1, 1), i.e. the fractional part for uvs inside the tile.
/// Mesh UVs are used as-is (no V flip): a paint upload's row 0 is v 0 —
/// see `crate::paint::PaintTarget`'s viewport-sampling note. The height
/// texture (binding 12) shares this same tile addressing.
///
/// # v1 boundary
///
/// One bound tile only (the app binds the active tile's `PaintTarget`;
/// the tile-pool atlas + page-table indirection of the architecture
/// doc's virtual texturing is a later slice). No per-channel textures
/// beyond base color and height — the group-2 reservation in
/// `crate::view_mode` still holds for the rest.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PaintParams {
    /// Tile origin subtracted from the global uv before scaling.
    pub uv_offset: [f32; 2],
    /// Scale applied after the offset (1, 1 for UDIM tiles).
    pub uv_scale: [f32; 2],
    /// Bit 0 ([`Self::HAS_TEXTURE`]) = a paint texture is bound; bit 1
    /// ([`Self::HAS_HEIGHT_TEXTURE`]) = a height texture is bound.
    pub flags: u32,
    /// Scale applied to the height channel's finite-difference
    /// gradient before it perturbs the normal (world units of
    /// displacement per unit height — depends on mesh scale, a v1
    /// judgment call). 1.0 = unscaled. Ignored while
    /// [`Self::HAS_HEIGHT_TEXTURE`] is clear.
    pub height_strength: f32,
    /// Padding to the 48-byte uniform multiple.
    pub _pad: [u32; 6],
}

const _: () = assert!(
    size_of::<PaintParams>() == 48,
    "PaintParams must match the WGSL PaintParams layout"
);

impl PaintParams {
    /// Flag bit: a paint texture is bound (the shader samples it).
    pub const HAS_TEXTURE: u32 = 1;
    /// Flag bit: a height texture is bound (the shader perturbs the
    /// normal from it — see `shaders::MESH_SHADER`'s
    /// `height_perturbed_normal`).
    pub const HAS_HEIGHT_TEXTURE: u32 = 2;

    /// UDIM tile origin (1001..=1100, clamped): tile (tu, tv) at
    /// offset (tu, tv). Shared by [`Self::for_udim`] and
    /// [`Self::height_for_udim`].
    fn udim_origin(tile: u16) -> [f32; 2] {
        let clamped = tile.clamp(umber_mesh::FIRST_TILE, 1100);
        let local = clamped - umber_mesh::FIRST_TILE;
        [f32::from(local % 10), f32::from(local / 10)]
    }

    /// No texture bound: the shader returns the `BASE_COLOR` constant
    /// (pixel-identical to the pre-texture pass). The offset/scale are
    /// still the identity so a stale uniform can never skew a re-bind.
    pub fn disabled() -> Self {
        Self {
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            flags: 0,
            height_strength: 1.0,
            _pad: [0; 6],
        }
    }

    /// A texture bound over single-tile (0..1) uvs: sample them directly.
    pub fn identity() -> Self {
        Self {
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            flags: Self::HAS_TEXTURE,
            height_strength: 1.0,
            _pad: [0; 6],
        }
    }

    /// A texture holding UDIM `tile`'s texels (1001..=1100, clamped),
    /// sampled by global mesh uvs: offset = the tile's origin, scale 1.
    pub fn for_udim(tile: u16) -> Self {
        Self {
            uv_offset: Self::udim_origin(tile),
            uv_scale: [1.0, 1.0],
            flags: Self::HAS_TEXTURE,
            height_strength: 1.0,
            _pad: [0; 6],
        }
    }

    /// A height texture bound over single-tile (0..1) uvs, with
    /// `strength` scaling the finite-difference gradient — world
    /// units of displacement per unit height (1.0 = unscaled).
    pub fn height(strength: f32) -> Self {
        Self {
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            flags: Self::HAS_HEIGHT_TEXTURE,
            height_strength: strength,
            _pad: [0; 6],
        }
    }

    /// [`Self::height`] over a UDIM tile's own uvs (mirrors
    /// [`Self::for_udim`]).
    pub fn height_for_udim(tile: u16, strength: f32) -> Self {
        Self {
            uv_offset: Self::udim_origin(tile),
            uv_scale: [1.0, 1.0],
            flags: Self::HAS_HEIGHT_TEXTURE,
            height_strength: strength,
            _pad: [0; 6],
        }
    }
}

/// The specular tier's GPU handles: prefilter views + spec uniform.
/// Bundled so [`GpuContext::mesh_bind_group_full`] stays at 7 args
/// (clippy's too-many-arguments floor).
pub(crate) struct MeshTier<'a> {
    /// The three prefiltered environment-mip views bound at slots 4–6.
    pub prefilter_views: [&'a wgpu::TextureView; 3],
    /// The specular-params uniform bound at slot 7.
    pub spec_buffer: &'a wgpu::Buffer,
}

/// The base-color + height paint binding: tile texture + sampler +
/// tile-select uniform + height texture. Bundled for the same reason
/// as [`MeshTier`] (one param, bindings 9–12).
pub(crate) struct MeshPaint<'a> {
    /// The tile's texture view (a [`crate::paint::PaintTarget::view`], or
    /// the context's 1×1 fallback when unbound).
    pub view: &'a wgpu::TextureView,
    /// The shared paint sampler (filtering, clamp-to-edge — v1 uses one
    /// sampler for every tile; per-texture samplers are deferred).
    pub sampler: &'a wgpu::Sampler,
    /// The tile-select uniform bound at slot 11.
    pub params_buffer: &'a wgpu::Buffer,
    /// The height tile's texture view (bound at slot 12), or the
    /// context's 1×1 fallback when unbound — reuses [`Self::sampler`],
    /// never sampled while `PaintParams::HAS_HEIGHT_TEXTURE` is clear.
    pub height_view: &'a wgpu::TextureView,
}

/// The tail of the mesh pass's group-0 bind group (bindings 4–12): the
/// specular tier, the paint set, and the view-mode selector. One bundle
/// so [`GpuContext::mesh_bind_group_full`] stays at clippy's 7-arg
/// floor — it would otherwise take eight.
pub(crate) struct MeshTail<'a> {
    /// The specular tier (bindings 4–7).
    pub tier: MeshTier<'a>,
    /// The base-color + height paint set (bindings 9–12).
    pub paint: MeshPaint<'a>,
    /// The view-mode selector (binding 8).
    pub view_mode_buffer: &'a wgpu::Buffer,
}

/// Per-frame camera uniform, matching `shaders::MESH_SHADER`'s `Camera`
/// struct layout exactly (16-byte alignment: `light_dir` carries an unused
/// `w` so it packs cleanly after the mat4x4).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    /// Combined projection * view matrix, column-major.
    pub view_proj: [[f32; 4]; 4],
    /// Normalized direction the light travels (surface -> fragment); `w` is
    /// unused padding.
    pub light_dir: [f32; 4],
    /// World-space eye position; `w` unused. The mesh pass ignores it
    /// (no view-dependent term); `OPENPBR_SHADER` reads it for its
    /// specular/Fresnel view vector.
    pub eye: [f32; 4],
}

impl CameraUniform {
    /// Builds the uniform from a view-projection matrix and a light
    /// direction (need not be pre-normalized). Eye defaults to the
    /// +Z axis — use [`Self::with_eye`] for view-dependent shading.
    pub fn new(view_proj: glam::Mat4, light_dir: glam::Vec3) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            eye: [0.0, 0.0, 1.0, 0.0],
        }
    }

    /// Sets the world-space eye position (view-dependent shading).
    #[must_use]
    pub fn with_eye(mut self, eye: glam::Vec3) -> Self {
        self.eye = [eye.x, eye.y, eye.z, 0.0];
        self
    }
}

/// Wireframe overlay color (group 1 binding 0 of
/// `shaders::WIREFRAME_SHADER`): rgb + premultiplied-style alpha the
/// fragment scales by the edge coverage.
///
/// Default is 40% white (the design's default); user-settable through
/// the app, which passes its choice per draw — no pipeline rebuild.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WireColor {
    /// Linear-space rgba.
    pub color: [f32; 4],
}

impl Default for WireColor {
    fn default() -> Self {
        Self {
            color: [0.4, 0.4, 0.4, 0.4],
        }
    }
}

impl WireColor {
    /// The cage overlay color (§6:59): amber at 60% so cage edges read
    /// distinctly over the 40%-white wireframe when both show.
    pub fn cage() -> Self {
        Self {
            color: [0.9, 0.6, 0.1, 0.6],
        }
    }
}

/// Ground-grid uniforms (group 0 binding 0 of `shaders::GRID_SHADER`):
/// the inverse view-projection matrix the fragment unprojects NDC
/// through to find the y=0 plane hit per pixel.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GridUniform {
    /// Inverse of the current `CameraUniform::view_proj`, column-major.
    pub inv_view_proj: [[f32; 4]; 4],
}

impl GridUniform {
    /// Builds the uniform from a view-projection matrix.
    pub fn new(view_proj: glam::Mat4) -> Self {
        Self {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
        }
    }
}

/// The single device context + mesh render pipeline for the viewport.
///
/// Owns no surface or frame-loop state — eframe/egui own the surface and
/// the shared render pass; this only owns the pipeline and the device
/// handles needed to build and update [`MeshBuffers`].
pub struct GpuContext {
    /// The eframe-provided device, reused for every buffer/pipeline this
    /// crate creates.
    pub device: wgpu::Device,
    /// The eframe-provided queue, used to submit buffer writes.
    pub queue: wgpu::Queue,
    /// The eframe-provided adapter (kept for diagnostics via [`crate::Backend`]).
    pub adapter: wgpu::Adapter,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// 1×1 fallback irradiance texture, bound at group 0 binding 1
    /// whenever no environment is loaded. Never sampled while
    /// `env_flags == 0` (the shader's procedural arm doesn't touch it),
    /// but the bind group must always be fully populated.
    fallback_env_view: wgpu::TextureView,
    /// Shared irradiance sampler (group 0 binding 2) — bilinear,
    /// repeat-U/clamp-V (see `crate::ibl::ibl_sampler`).
    env_sampler: wgpu::Sampler,
    /// 1×1 fallback paint tile, bound at group 0 binding 9 whenever no
    /// paint is set. Never sampled while the params flag is 0 (the
    /// shader's constant arm doesn't touch it), but the bind group must
    /// always be fully populated.
    fallback_paint_view: wgpu::TextureView,
    /// Shared paint sampler (group 0 binding 10) — filtering,
    /// clamp-to-edge on both axes. Clamp (not repeat) so a tile-local
    /// uv at exactly 1.0 reads the edge texel instead of wrapping into
    /// the opposite edge.
    paint_sampler: wgpu::Sampler,
    depth_format: Option<wgpu::TextureFormat>,
    /// The eframe surface format every display pipeline targets (kept
    /// for the post-effects composite, built lazily by
    /// `crate::postfx_viewport`).
    color_format: wgpu::TextureFormat,
    /// Textured-quad display pipeline for paint-target presentation
    /// (built once; see `texture_display`).
    texture_display: crate::texture_display::TextureDisplay,
    /// OpenPBR viewport pipeline (binding 0 camera + binding 1 params).
    openpbr_pipeline: wgpu::RenderPipeline,
    /// Bind-group layout shared by OpenPBR draw calls (camera + params).
    openpbr_layout: wgpu::BindGroupLayout,
    /// Barycentric-edge wireframe pipeline (Wave-4 item 7): same
    /// triangles as the mesh, shaded as 1px edges, drawn after the mesh
    /// with `LessEqual` depth + clip-space bias. Group 0 reuses the
    /// mesh's camera bind-group layout object, so the mesh bind group
    /// binds straight through; group 1 is the [`WireColor`] uniform.
    wire_pipeline: wgpu::RenderPipeline,
    /// Group-1 layout for the wireframe pass ([`WireColor`]).
    wire_color_layout: wgpu::BindGroupLayout,
    /// Procedural ground-grid pipeline (Wave-4 item 7): big-triangle
    /// fullscreen pass, inverse-VP unprojection in the fragment, drawn
    /// before the mesh with depth-write off / compare Always.
    grid_pipeline: wgpu::RenderPipeline,
    /// Group-0 layout for the grid pass ([`GridUniform`]).
    grid_layout: wgpu::BindGroupLayout,
    /// Group-1 layout of the two display-LUT consumers (the mesh pass
    /// and the texture display) — see `crate::display_lut`.
    display_lut_layout: wgpu::BindGroupLayout,
    /// The identity display LUT, bound whenever a consumer is handed no
    /// LUT (`None`) — renders the pre-LUT bytes. Same always-populated
    /// pattern as `fallback_env_view`.
    identity_lut: crate::display_lut::DisplayLut,
    /// The HDR surface-color-space probe result (§5 P1 row 85,
    /// `crate::hdr`): always [`crate::hdr::HdrProbe::NotExposed`] in
    /// this version — this context holds no `wgpu::Surface` to query
    /// (see `crate::hdr`'s module boundary note). `pub` so umber-app can
    /// weigh a requested [`crate::hdr::HdrMode`] against it
    /// (`HdrMode::effective`) without this crate needing an i18n-facing
    /// getter.
    pub hdr_probe: crate::hdr::HdrProbe,
}

/// Depth format used by the viewport mesh pipeline when depth is enabled.
///
/// The app requests the matching bit depth from eframe (`DEPTH_FORMAT_BITS`)
/// so the egui renderer's shared render pass carries the attachment; the
/// value is kept here so umber-app never names a wgpu type.
pub fn depth_format() -> Option<wgpu::TextureFormat> {
    Some(wgpu::TextureFormat::Depth32Float)
}

impl GpuContext {
    /// The depth format this context's pipeline depth-tests against.
    ///
    /// Returned as an opaque value so umber-app can pass it to
    /// `GpuContext::new` without naming a wgpu type.
    pub fn depth_format(&self) -> Option<wgpu::TextureFormat> {
        self.depth_format
    }

    /// The texture-display pipeline for presenting paint targets.
    pub fn texture_display(&self) -> &crate::texture_display::TextureDisplay {
        &self.texture_display
    }

    /// The display-LUT consumers' shared group-1 layout
    /// ([`crate::display_lut::DisplayLut::new`] builds against it).
    pub(crate) fn display_lut_layout(&self) -> &wgpu::BindGroupLayout {
        &self.display_lut_layout
    }

    /// The identity display LUT (what a consumer handed `None` binds).
    pub(crate) fn identity_lut(&self) -> &crate::display_lut::DisplayLut {
        &self.identity_lut
    }

    /// The mesh pass's group-0 layout (camera + IBL + spec + view mode):
    /// the post-effects path's scene-linear mesh pipeline shares it, so
    /// [`MeshBuffers`]' bind group binds straight through.
    pub(crate) fn mesh_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// The shared paint sampler (mesh-pass group 0 binding 10).
    pub(crate) fn paint_sampler(&self) -> &wgpu::Sampler {
        &self.paint_sampler
    }

    /// The 1×1 fallback paint tile (mesh-pass group 0 binding 9 when
    /// no paint is set).
    pub(crate) fn fallback_paint_view(&self) -> &wgpu::TextureView {
        &self.fallback_paint_view
    }

    /// The surface format the display pipelines target.
    pub(crate) fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    /// Creates a uniform buffer holding `params`, ready for an OpenPBR
    /// bind group (binding 1 in [`Self::openpbr_draw`]).
    pub fn openpbr_params_buffer(&self, params: &crate::material::OpenPbrParams) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_openpbr_params"),
                contents: params.as_bytes(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
    }

    /// Renders `buffers` with the OpenPBR surface model: returns a paint
    /// callback shape drawing the mesh with `params` through the OpenPBR
    /// pipeline. `camera_uniform_buffer` is the same buffer the plain
    /// mesh pass uses (rebound here at group 0 of the OpenPBR layout).
    ///
    /// The params buffer is written on `prepare` every frame from
    /// `params` so live parameter edits show without re-upload plumbing.
    pub fn openpbr_paint_shape(
        &self,
        rect: epaint::emath::Rect,
        buffers: &MeshBuffers,
        uniform: CameraUniform,
        params: crate::material::OpenPbrParams,
    ) -> epaint::Shape {
        let callback = self.openpbr_callback(buffers, uniform, params);
        egui_wgpu::Callback::new_paint_callback(rect, callback).into()
    }

    /// Builds the OpenPBR callback without the egui shape wrapper —
    /// used by [`Self::openpbr_paint_shape`] and by offscreen tests.
    pub fn openpbr_callback(
        &self,
        buffers: &MeshBuffers,
        uniform: CameraUniform,
        params: crate::material::OpenPbrParams,
    ) -> OpenPbrPaintCallback {
        let params_buffer = self.openpbr_params_buffer(&params);
        // Fresh camera uniform buffer per callback: the OpenPBR layout's
        // binding 0 is byte-compatible with the mesh pass's.
        let camera_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_openpbr_camera"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("umber_openpbr_bind_group"),
            layout: &self.openpbr_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &camera_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &params_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });
        OpenPbrPaintCallback {
            pipeline: self.openpbr_pipeline.clone(),
            bind_group,
            vertex_buffer: buffers.vertex_buffer.clone(),
            index_buffer: buffers.index_buffer.clone(),
            index_count: buffers.index_count,
        }
    }

    /// Builds the wireframe overlay callback (Wave-4 item 7): draws
    /// `buffers`' duplicated-vertex buffer through the wire pipeline
    /// AFTER the mesh draw, in the same render pass, reusing the mesh's
    /// camera bind group at group 0 and a fresh per-frame color uniform
    /// at group 1.
    ///
    /// The app calls this only when the wireframe toggle is on —
    /// skip-draw when off is cheaper than a uniform flag, and renders
    /// byte-identical to the mesh-only path.
    pub fn wire_callback(
        &self,
        buffers: &MeshBuffers,
        uniform: CameraUniform,
        color: WireColor,
    ) -> WireframePaintCallback {
        let color_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_wire_color_buffer"),
                contents: bytemuck::bytes_of(&color),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let color_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("umber_wire_color_bind_group"),
            layout: &self.wire_color_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &color_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });
        WireframePaintCallback {
            pipeline: self.wire_pipeline.clone(),
            bind_group: buffers.bind_group.clone(),
            color_bind_group,
            vertex_buffer: buffers.wire_buffer.clone(),
            vertex_count: buffers.wire_count,
            uniform_buffer: buffers.uniform_buffer.clone(),
            uniform,
        }
    }

    /// Builds the ground-grid callback (Wave-4 item 7): a fullscreen
    /// big-triangle pass unprojecting through `view_proj`'s inverse.
    /// The app adds its shape BEFORE the mesh shape so the mesh
    /// occludes the reference plane; skipped entirely when the grid
    /// toggle is off.
    pub fn grid_callback(&self, view_proj: glam::Mat4) -> GridPaintCallback {
        let uniform = GridUniform::new(view_proj);
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("umber_grid_uniform_buffer"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("umber_grid_bind_group"),
            layout: &self.grid_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });
        GridPaintCallback {
            pipeline: self.grid_pipeline.clone(),
            bind_group,
        }
    }

    /// `adapter`/`device`/`queue` must come from eframe's
    /// `CreationContext::wgpu_render_state` — this never constructs a new
    /// `wgpu::Instance` or requests its own adapter.
    ///
    /// `color_format` should be `RenderState::target_format` so the pipeline
    /// matches the surface egui is already configured for.
    ///
    /// `depth_format` is `None` in the current wiring: the shared render
    /// pass egui builds only gains a depth attachment if eframe threads
    /// `NativeOptions::depth_buffer` through to the wgpu `Renderer`'s
    /// `RendererOptions::depth_stencil_format` — that internal wiring isn't
    /// in the vendored source, so flipping this to `Some(..)` without first
    /// verifying it on real hardware risks a wgpu validation panic on the
    /// first painted frame. Depth correctness is instead provided by the
    /// callback owning its own depth texture (see `MeshPaintCallback`).
    /// See `LANDING_NOTES.md` and review finding #1.
    pub fn new(
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_mesh_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(MESH_SHADER)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_mesh_camera_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<CameraUniform>() as u64),
                    },
                    count: None,
                },
                // IBL set (Wave-4 item 6 — see `shaders::MESH_SHADER`):
                // the convolved irradiance map (always Rgba16Float,
                // filterable), its sampler, and the selector uniform.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<EnvFlags>() as u64),
                    },
                    count: None,
                },
                // Specular IBL tier (wave-5 v1 — see `shaders::MESH_SHADER`):
                // the three cone-scaled prefilter levels (filterable, like
                // the irradiance map) and the roughness/metallic uniform.
                // Appended AFTER the IBL set — additive bindings, the
                // classic layout-sync trap avoided by construction.
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<SpecParams>() as u64),
                    },
                    count: None,
                },
                // View-mode selector (view-modes slice — see
                // `shaders::MESH_SHADER`'s `VIEW_MODE_WGSL` block):
                // which shading the fragment runs (lit / unlit /
                // solo / mesh-map preview). Appended AFTER the
                // specular tier — additive bindings, the classic
                // layout-sync trap avoided by construction.
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<ViewModeParams>() as u64),
                    },
                    count: None,
                },
                // Base-color paint set (core-loop kill 1 — see
                // `shaders::MESH_SHADER`'s paint block): the tile
                // texture (rgba8, filterable), its sampler, and the
                // tile-select uniform. Appended AFTER the view-mode
                // selector — same additive pattern. Group 0 (not a
                // new group) so every existing draw — tests, the
                // post-effects scene pass, the wireframe overlay's
                // layout reuse — binds unchanged with the fallback.
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<PaintParams>() as u64),
                    },
                    count: None,
                },
                // Height channel (audit worklist #12 — see
                // `shaders::MESH_SHADER`'s `height_perturbed_normal`):
                // appended AFTER the paint params — same additive,
                // layout-sync-safe pattern as every earlier block in
                // this group.
                wgpu::BindGroupLayoutEntry {
                    binding: 12,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // Display LUT (crate::display_lut): group 1 of the mesh pass and
        // the texture display — an additive group, so group 0 (and the
        // wireframe pass that reuses its layout) is untouched. The
        // identity table is the fallback for consumers handed no LUT.
        let display_lut_layout = crate::display_lut::display_lut_layout(&device);
        let identity_lut = crate::display_lut::DisplayLut::with_layout(
            &device,
            &queue,
            &display_lut_layout,
            &crate::display_lut::identity_lut_bytes(),
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("umber_mesh_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&display_lut_layout)],
            immediate_size: 0,
        });

        let depth_stencil = depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("umber_mesh_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                entry_point: Some("vs_main"),
                module: &shader,
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                unclipped_depth: false,
                conservative: false,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::default(),
                polygon_mode: wgpu::PolygonMode::default(),
                strip_index_format: None,
            },
            depth_stencil: depth_stencil.clone(),
            multisample: wgpu::MultisampleState {
                alpha_to_coverage_enabled: false,
                count: 1,
                mask: !0,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(crate::shaders::MESH_ENTRY_DIRECT),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let texture_display = crate::texture_display::TextureDisplay::new(
            &device,
            color_format,
            &display_lut_layout,
            identity_lut.clone(),
        );

        // OpenPBR pipeline: camera (binding 0) + material params
        // (binding 1, the 96-byte OpenPbrParams uniform).
        let openpbr_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_openpbr_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<CameraUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(96),
                    },
                    count: None,
                },
            ],
        });
        let openpbr_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_openpbr_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(crate::shaders::OPENPBR_SHADER)),
        });
        let openpbr_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("umber_openpbr_pipeline_layout"),
                bind_group_layouts: &[Some(&openpbr_layout)],
                immediate_size: 0,
            });
        let openpbr_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("umber_openpbr_pipeline"),
            layout: Some(&openpbr_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &openpbr_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                unclipped_depth: false,
                conservative: false,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::default(),
                polygon_mode: wgpu::PolygonMode::default(),
                strip_index_format: None,
            },
            depth_stencil: depth_stencil.clone(),
            multisample: wgpu::MultisampleState {
                alpha_to_coverage_enabled: false,
                count: 1,
                mask: !0,
            },
            fragment: Some(wgpu::FragmentState {
                module: &openpbr_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let (fallback_env_view, env_sampler) = Self::fallback_env_resources(&device);
        let (fallback_paint_view, paint_sampler) = Self::fallback_paint_resources(&device);

        // HDR surface probe (§5 P1 row 85, `crate::hdr`): `None` because
        // this context is never handed a `wgpu::Surface` to query (see
        // the module's boundary note) — eframe owns the real surface.
        let hdr_probe = crate::hdr::probe(None);

        // Wireframe overlay pipeline (Wave-4 item 7): the SAME group-0
        // layout object as the mesh pass (the shader only declares
        // binding 0 — unused layout entries are legal), plus a group-1
        // color uniform. Depth: no writes (an overlay must not poison
        // later draws), LessEqual so the clip-space-biased edges win
        // against the coplanar surface. Alpha blending: the fragment's
        // edge coverage is smooth, not binary.
        let wire_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_wireframe_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(WIREFRAME_SHADER)),
        });
        let wire_color_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_wire_color_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<WireColor>() as u64),
                },
                count: None,
            }],
        });
        let wire_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("umber_wire_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&wire_color_layout)],
            immediate_size: 0,
        });
        let wire_depth_stencil = depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let wire_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("umber_wire_pipeline"),
            layout: Some(&wire_pipeline_layout),
            vertex: wgpu::VertexState {
                entry_point: Some("vs_main"),
                module: &wire_shader,
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<umber_mesh::WireVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                unclipped_depth: false,
                conservative: false,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::default(),
                polygon_mode: wgpu::PolygonMode::default(),
                strip_index_format: None,
            },
            depth_stencil: wire_depth_stencil,
            multisample: wgpu::MultisampleState {
                alpha_to_coverage_enabled: false,
                count: 1,
                mask: !0,
            },
            fragment: Some(wgpu::FragmentState {
                module: &wire_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        // Ground-grid pipeline (Wave-4 item 7): the vertex stage needs
        // no buffers (big triangle from `vertex_index`); depth-write OFF
        // and compare Always — a reference plane drawn first, which the
        // mesh then occludes.
        let grid_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("umber_grid_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(GRID_SHADER)),
        });
        let grid_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("umber_grid_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<GridUniform>() as u64),
                },
                count: None,
            }],
        });
        let grid_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("umber_grid_pipeline_layout"),
            bind_group_layouts: &[Some(&grid_layout)],
            immediate_size: 0,
        });
        let grid_depth_stencil = depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("umber_grid_pipeline"),
            layout: Some(&grid_pipeline_layout),
            vertex: wgpu::VertexState {
                entry_point: Some("vs_main"),
                module: &grid_shader,
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                unclipped_depth: false,
                conservative: false,
                cull_mode: None,
                front_face: wgpu::FrontFace::default(),
                polygon_mode: wgpu::PolygonMode::default(),
                strip_index_format: None,
            },
            depth_stencil: grid_depth_stencil,
            multisample: wgpu::MultisampleState {
                alpha_to_coverage_enabled: false,
                count: 1,
                mask: !0,
            },
            fragment: Some(wgpu::FragmentState {
                module: &grid_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            device,
            queue,
            adapter,
            pipeline,
            bind_group_layout,
            depth_format,
            color_format,
            texture_display,
            openpbr_pipeline,
            openpbr_layout,
            fallback_env_view,
            env_sampler,
            fallback_paint_view,
            paint_sampler,
            wire_pipeline,
            wire_color_layout,
            grid_pipeline,
            grid_layout,
            display_lut_layout,
            identity_lut,
            hdr_probe,
        }
    }

    /// Builds the fallback IBL resources for [`Self::new`]: a 1×1
    /// `Rgba16Float` texture view (never sampled while `env_flags == 0`,
    /// but the bind group must stay fully populated) and the shared
    /// irradiance sampler (bilinear, repeat-U/clamp-V).
    fn fallback_env_resources(device: &wgpu::Device) -> (wgpu::TextureView, wgpu::Sampler) {
        let fallback = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_mesh_fallback_irradiance"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        (
            fallback.create_view(&wgpu::TextureViewDescriptor::default()),
            crate::ibl::ibl_sampler(device),
        )
    }

    /// Builds the fallback paint resources for [`Self::new`]: a 1×1
    /// `Rgba8Unorm` texture view (never sampled while the paint flags
    /// are 0, but the bind group must stay fully populated) and the
    /// shared paint sampler (filtering, clamp-to-edge).
    fn fallback_paint_resources(device: &wgpu::Device) -> (wgpu::TextureView, wgpu::Sampler) {
        let fallback = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("umber_mesh_fallback_paint"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("umber_mesh_paint_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        (
            fallback.create_view(&wgpu::TextureViewDescriptor::default()),
            sampler,
        )
    }

    /// Builds the mesh pass's group-0 bind group: camera + diffuse IBL
    /// set (irradiance view, sampler, flags buffer) + specular tier
    /// (prefilter views at bindings 4–6, spec uniform at 7) + the
    /// view-mode selector (binding 8) + the base-color + height paint
    /// set (tile view, sampler, params, height view at bindings
    /// 9–12). Used by [`MeshBuffers::upload`] (fallback views/samplers,
    /// procedural flags, default spec, Lit mode, unbound paint) and
    /// [`MeshBuffers`]'s rebuild path (`set_environment` /
    /// `set_spec_params` / `set_base_color_texture` / `set_height_texture`:
    /// the map's own views and sampler, or the fallback pair).
    pub(crate) fn mesh_bind_group_full(
        &self,
        uniform_buffer: &wgpu::Buffer,
        flags_buffer: &wgpu::Buffer,
        env_view: &wgpu::TextureView,
        env_sampler: &wgpu::Sampler,
        tail: MeshTail<'_>,
    ) -> wgpu::BindGroup {
        let MeshTail {
            tier,
            paint,
            view_mode_buffer,
        } = tail;
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("umber_mesh_camera_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(env_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(env_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: flags_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(tier.prefilter_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(tier.prefilter_views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(tier.prefilter_views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: tier.spec_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: view_mode_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(paint.view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::Sampler(paint.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: paint.params_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(paint.height_view),
                },
            ],
        })
    }
}

/// The retained half of a paint binding: the tile view plus the
/// tile-select params the bind group was built with, so environment and
/// spec rebuilds keep the paint without the caller re-binding it.
struct BoundPaint {
    /// Cloned tile view — keeps the `PaintTarget`'s texture alive.
    view: wgpu::TextureView,
    /// The params uploaded alongside (kept for introspection/rebuild).
    params: PaintParams,
}

/// The retained half of a height binding (audit worklist #12): the
/// tile view plus the addressing/strength the shader needs, kept
/// separately from [`BoundPaint`] since base color and height bind
/// (and clear) independently — see [`MeshBuffers::write_paint_params`].
struct BoundHeight {
    /// Cloned height-tile view — keeps its texture alive.
    view: wgpu::TextureView,
    /// Tile addressing, shared with base color when both are bound to
    /// the same tile (the common case — see `write_paint_params`'s
    /// doc for what happens when only one side is bound).
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    /// The finite-difference gradient scale (see
    /// [`PaintParams::height_strength`]'s doc).
    strength: f32,
}

/// GPU-resident mesh data for the viewport: vertex/index buffers plus the
/// per-mesh camera uniform buffer, the environment-flag uniform buffer,
/// the specular-tier uniform buffer, the view-mode uniform buffer, the
/// paint tile-select uniform buffer, and the combined bind group
/// (camera + IBL set + specular tier + view mode + paint set).
pub struct MeshBuffers {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    flags_buffer: wgpu::Buffer,
    /// Roughness/metallic uniform (mesh-pass binding 7): v1 stand-ins
    /// (see [`SpecParams`]), written by [`Self::set_spec_params`].
    spec_buffer: wgpu::Buffer,
    /// View-mode selector (mesh-pass binding 8): written per frame by
    /// [`MeshPaintCallback::write_uniform`] from the callback's mode
    /// (see [`Self::paint_callback_with_mode`] — Lit by default).
    view_mode_buffer: wgpu::Buffer,
    /// Tile-select + height-strength uniform (mesh-pass binding 11):
    /// composed by [`Self::write_paint_params`] from [`Self::bound_paint`]
    /// and [`Self::bound_height`] — never written verbatim from a
    /// caller's [`PaintParams`], so binding one channel can't clobber
    /// the other's flag.
    paint_params_buffer: wgpu::Buffer,
    /// Currently bound environment (if any): retained so
    /// [`Self::set_spec_params`] can rebuild the bind group without
    /// dropping the map. Cloned handles — no GPU cost.
    bound_env: Option<EnvIrradiance>,
    /// Currently bound paint tile (if any): the texture view plus the
    /// tile-select params, retained so [`Self::set_environment`] /
    /// [`Self::set_spec_params`] rebuild the bind group without
    /// dropping the paint. The cloned view keeps the tile's texture
    /// alive (wgpu views hold their texture) until replaced or cleared
    /// — see the bind-group lifetime note on
    /// [`Self::set_base_color_texture`].
    bound_paint: Option<BoundPaint>,
    /// Currently bound height tile (if any) — see [`BoundHeight`].
    bound_height: Option<BoundHeight>,
    bind_group: wgpu::BindGroup,
    /// Duplicated-vertex wireframe buffer (Wave-4 item 7): 3 verts per
    /// triangle with barycentric corners, built once at upload from the
    /// index list. Drawn non-indexed through the wire pipeline.
    wire_buffer: wgpu::Buffer,
    /// Number of wire verts (3x the valid-triangle count; may be 0 for
    /// an index-less mesh — the buffer then holds one padding vert and
    /// every wire draw is a 0..0 no-op).
    wire_count: u32,
}

impl MeshBuffers {
    /// Uploads `mesh` to GPU buffers. Recomputes vertex normals when the
    /// source mesh has none (or a mismatched count) rather than failing —
    /// OBJ files without `vn` lines and in-flight glTF/FBX loaders are
    /// expected to hit this path. UVs ride the same rule: uploaded
    /// verbatim when the counts match, zero-filled otherwise (a uv-less
    /// mesh samples tile-local (0, 0) once a texture is bound; unbound
    /// it renders the constant fallback either way).
    pub fn upload(gpu: &GpuContext, mesh: &umber_mesh::MeshData) -> Result<Self, GpuError> {
        let device = &gpu.device;
        if mesh.positions.is_empty() {
            return Err(GpuError::EmptyMesh);
        }
        if !mesh.indices.len().is_multiple_of(3) {
            return Err(GpuError::InvalidIndexCount(mesh.indices.len()));
        }

        let normals: Cow<'_, [[f32; 3]]> = if mesh.normals.len() == mesh.positions.len() {
            Cow::Borrowed(&mesh.normals)
        } else {
            Cow::Owned(compute_vertex_normals(&mesh.positions, &mesh.indices))
        };
        let uvs: Cow<'_, [[f32; 2]]> = if mesh.uvs.len() == mesh.positions.len() {
            Cow::Borrowed(&mesh.uvs)
        } else {
            Cow::Owned(vec![[0.0; 2]; mesh.positions.len()])
        };

        let vertices: Vec<Vertex> = mesh
            .positions
            .iter()
            .zip(normals.iter())
            .zip(uvs.iter())
            .map(|((p, n), uv)| Vertex {
                position: *p,
                normal: *n,
                uv: *uv,
            })
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        // An index-less mesh uploads a one-index padding buffer while
        // `index_count` stays 0: slicing a zero-sized wgpu buffer
        // panics at draw time, so the padding keeps the no-op draw
        // (0..0) valid — same pattern as the wire buffer below.
        let index_upload: &[u32] = if mesh.indices.is_empty() {
            &[0]
        } else {
            &mesh.indices
        };
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_index_buffer"),
            contents: bytemuck::cast_slice(index_upload),
            usage: wgpu::BufferUsages::INDEX,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_camera_uniform_buffer"),
            contents: bytemuck::bytes_of(&CameraUniform::new(
                glam::Mat4::IDENTITY,
                glam::Vec3::NEG_Y,
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // Procedural fallback by default: no map bound, flags == 0.
        // `set_environment` flips this when an environment loads.
        let flags_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_env_flags_buffer"),
            contents: bytemuck::bytes_of(&EnvFlags::procedural()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // Specular-tier uniform at the v1 stand-in defaults (see
        // `SpecParams`); `set_spec_params` overwrites it per mesh.
        let spec_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_spec_params_buffer"),
            contents: bytemuck::bytes_of(&SpecParams::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // View-mode selector at the Lit default (mode 0, channel 0);
        // the per-frame callback overwrites it from its own mode (see
        // `MeshPaintCallback::write_uniform`).
        let view_mode_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_view_mode_buffer"),
            contents: bytemuck::bytes_of(&ViewMode::Lit.uniform()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let tier = MeshTier {
            prefilter_views: [
                &gpu.fallback_env_view,
                &gpu.fallback_env_view,
                &gpu.fallback_env_view,
            ],
            spec_buffer: &spec_buffer,
        };
        // Unbound paint by default: the 1×1 fallback tile + the disabled
        // params (flags == 0); `set_base_color_texture` flips this when
        // the app binds a tile.
        let paint_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_mesh_paint_params_buffer"),
            contents: bytemuck::bytes_of(&PaintParams::disabled()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let paint = MeshPaint {
            view: gpu.fallback_paint_view(),
            sampler: gpu.paint_sampler(),
            params_buffer: &paint_params_buffer,
            height_view: gpu.fallback_paint_view(),
        };
        let bind_group = gpu.mesh_bind_group_full(
            &uniform_buffer,
            &flags_buffer,
            &gpu.fallback_env_view,
            &gpu.env_sampler,
            MeshTail {
                tier,
                paint,
                view_mode_buffer: &view_mode_buffer,
            },
        );

        let index_count = u32::try_from(mesh.indices.len())
            .map_err(|_| GpuError::IndexCountOverflow(mesh.indices.len()))?;

        // Wireframe overlay buffer (Wave-4 item 7): duplicated verts, one
        // triangle's worth per index-triple. A zero-sized wgpu buffer is
        // not portable, so an index-less mesh uploads one padding vert
        // while `wire_count` stays 0 — every wire draw is then 0..0.
        let wire_vertices = umber_mesh::wire_vertices_from_indices(mesh);
        let wire_count = u32::try_from(wire_vertices.len())
            .map_err(|_| GpuError::IndexCountOverflow(mesh.indices.len()))?;
        let padding = [umber_mesh::WireVertex {
            position: [0.0; 3],
            bary: [1.0, 0.0, 0.0],
        }];
        let wire_upload: &[umber_mesh::WireVertex] = if wire_vertices.is_empty() {
            &padding
        } else {
            &wire_vertices
        };
        let wire_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("umber_wire_vertex_buffer"),
            contents: bytemuck::cast_slice(wire_upload),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Ok(Self {
            vertex_buffer,
            index_buffer,
            index_count,
            uniform_buffer,
            flags_buffer,
            spec_buffer,
            view_mode_buffer,
            paint_params_buffer,
            bound_env: None,
            bound_paint: None,
            bound_height: None,
            bind_group,
            wire_buffer,
            wire_count,
        })
    }

    /// Rebuilds the group-0 bind group from the current flags + bound
    /// environment + spec buffer + bound paint + bound height. Shared
    /// by [`Self::set_environment`], [`Self::set_spec_params`],
    /// [`Self::set_base_color_texture`], and [`Self::set_height_texture`]
    /// (disjoint field borrows — no borrow of the whole `self`).
    fn rebuild_bind_group(&mut self, gpu: &GpuContext) {
        let (view, sampler) = match &self.bound_env {
            Some(env) => (env.view(), env.sampler()),
            None => (&gpu.fallback_env_view, &gpu.env_sampler),
        };
        let prefilter = match &self.bound_env {
            Some(env) => [
                env.prefilter_view(0),
                env.prefilter_view(1),
                env.prefilter_view(2),
            ],
            None => [
                &gpu.fallback_env_view,
                &gpu.fallback_env_view,
                &gpu.fallback_env_view,
            ],
        };
        let tier = MeshTier {
            prefilter_views: prefilter,
            spec_buffer: &self.spec_buffer,
        };
        // Bound paint and bound height survive environment/spec
        // rebuilds (and each other): each setter only replaces its
        // own slice.
        let paint_view = match &self.bound_paint {
            Some(bound) => &bound.view,
            None => gpu.fallback_paint_view(),
        };
        let height_view = match &self.bound_height {
            Some(bound) => &bound.view,
            None => gpu.fallback_paint_view(),
        };
        let paint = MeshPaint {
            view: paint_view,
            sampler: gpu.paint_sampler(),
            params_buffer: &self.paint_params_buffer,
            height_view,
        };
        self.bind_group = gpu.mesh_bind_group_full(
            &self.uniform_buffer,
            &self.flags_buffer,
            view,
            sampler,
            MeshTail {
                tier,
                paint,
                view_mode_buffer: &self.view_mode_buffer,
            },
        );
    }

    /// Composes and writes the mesh-pass tile-select uniform (binding
    /// 11) from [`Self::bound_paint`] and [`Self::bound_height`] —
    /// never a caller's [`PaintParams`] verbatim. Each setter/clearer
    /// updates its own `bound_*` field, then calls this, so binding
    /// (or clearing) one channel can never clobber the other's flag
    /// bit — the bug a single shared "last write wins" buffer write
    /// would have (base color is re-bound every viewport frame per
    /// `umber_app::viewport::bind_paint_target`).
    ///
    /// Tile addressing (`uv_offset`/`uv_scale`) prefers the bound
    /// paint's, falling back to the bound height's, then the
    /// identity — the two are expected to agree (the app binds both
    /// from the same active tile); v1 doesn't reconcile a mismatch.
    fn write_paint_params(&mut self, gpu: &GpuContext) {
        let mut flags = 0u32;
        let (uv_offset, uv_scale) = if let Some(bound) = &self.bound_paint {
            flags |= PaintParams::HAS_TEXTURE;
            (bound.params.uv_offset, bound.params.uv_scale)
        } else if let Some(bound) = &self.bound_height {
            (bound.uv_offset, bound.uv_scale)
        } else {
            ([0.0, 0.0], [1.0, 1.0])
        };
        let height_strength = if let Some(bound) = &self.bound_height {
            flags |= PaintParams::HAS_HEIGHT_TEXTURE;
            bound.strength
        } else {
            1.0
        };
        let params = PaintParams {
            uv_offset,
            uv_scale,
            flags,
            height_strength,
            _pad: [0; 6],
        };
        gpu.queue
            .write_buffer(&self.paint_params_buffer, 0, bytemuck::bytes_of(&params));
    }

    /// Swaps the bound environment: `Some(env)` samples the convolved
    /// map (`env_flags = 1`), `None` restores the procedural fallback
    /// (`env_flags = 0`, 1×1 fallback texture bound but never sampled).
    /// The next [`Self::paint_callback`] picks up the new bind group —
    /// the viewport builds one per frame, so this takes effect
    /// immediately with no re-upload.
    pub fn set_environment(&mut self, gpu: &GpuContext, env: Option<&EnvIrradiance>) {
        let flags = match env {
            Some(_) => EnvFlags::from_map(),
            None => EnvFlags::procedural(),
        };
        gpu.queue
            .write_buffer(&self.flags_buffer, 0, bytemuck::bytes_of(&flags));
        self.bound_env = env.cloned();
        self.rebuild_bind_group(gpu);
    }

    /// Sets the specular-tier roughness/metallic (mesh-pass binding 7).
    /// Takes effect on the next [`Self::paint_callback`], like
    /// [`Self::set_environment`]; the bound environment is preserved.
    pub fn set_spec_params(&mut self, gpu: &GpuContext, params: SpecParams) {
        gpu.queue
            .write_buffer(&self.spec_buffer, 0, bytemuck::bytes_of(&params));
        self.rebuild_bind_group(gpu);
    }

    /// Binds `target`'s view as the mesh pass's base-color tile
    /// (bindings 9–11) with `params` selecting the tile region — the
    /// viewport shows paint. Takes effect on the next
    /// [`Self::paint_callback`], like [`Self::set_environment`]; the
    /// bound environment and spec params are preserved.
    ///
    /// # Bind-group lifetime
    ///
    /// The rebuilt bind group clones `target`'s view, and a wgpu view
    /// holds its texture alive — so `target` (a tile-pool entry, a
    /// scratch surface, …) may be dropped or evicted right after this
    /// call without breaking later draws: the bound texels stay valid
    /// until replaced by another `set_base_color_texture` or
    /// [`Self::clear_base_color_texture`]. Re-binding after repainting
    /// the SAME `PaintTarget` is unnecessary (the view aliases the
    /// texture, not a snapshot) — but re-binding after the tile's
    /// texture was recreated (evict + recreate) IS required, since the
    /// new texture is a different GPU object.
    pub fn set_base_color_texture(
        &mut self,
        gpu: &GpuContext,
        target: &crate::paint::PaintTarget,
        params: PaintParams,
    ) {
        self.bound_paint = Some(BoundPaint {
            view: target.view().clone(),
            params,
        });
        self.write_paint_params(gpu);
        self.rebuild_bind_group(gpu);
    }

    /// Unbinds the paint tile: the 1×1 fallback returns and the params
    /// flag goes to 0, so the shader takes the constant arm again —
    /// pixel-identical to a mesh that never bound paint. The next
    /// [`Self::paint_callback`] picks it up. The bound height (if any)
    /// is untouched.
    pub fn clear_base_color_texture(&mut self, gpu: &GpuContext) {
        self.bound_paint = None;
        self.write_paint_params(gpu);
        self.rebuild_bind_group(gpu);
    }

    /// Binds `target`'s view as the mesh pass's height tile (binding
    /// 12), gated by `params`'s [`PaintParams::HAS_HEIGHT_TEXTURE`]
    /// flag and scaled by its `height_strength` — the lit path's
    /// `height_perturbed_normal` (audit worklist #12) picks it up on
    /// the next [`Self::paint_callback`]. Mirrors
    /// [`Self::set_base_color_texture`]'s pattern and bind-group
    /// lifetime contract (the cloned view keeps `target`'s texture
    /// alive); the bound base color is untouched.
    pub fn set_height_texture(
        &mut self,
        gpu: &GpuContext,
        target: &crate::paint::PaintTarget,
        params: PaintParams,
    ) {
        self.bound_height = Some(BoundHeight {
            view: target.view().clone(),
            uv_offset: params.uv_offset,
            uv_scale: params.uv_scale,
            strength: params.height_strength,
        });
        self.write_paint_params(gpu);
        self.rebuild_bind_group(gpu);
    }

    /// Unbinds the height tile: the 1×1 fallback returns and the
    /// height flag goes to 0, so the shader skips
    /// `height_perturbed_normal` again — pixel-identical to a mesh
    /// that never bound one. The bound base color (if any) is
    /// untouched.
    pub fn clear_height_texture(&mut self, gpu: &GpuContext) {
        self.bound_height = None;
        self.write_paint_params(gpu);
        self.rebuild_bind_group(gpu);
    }

    /// The currently bound tile-select params (the last
    /// [`Self::set_base_color_texture`] value, or [`PaintParams::disabled`]).
    /// Base-color-only by design — `umber_app::viewport::bind_paint_target`'s
    /// skip check reads this to mean "is a base-color texture bound",
    /// independent of whether a height texture also is.
    pub fn paint_params(&self) -> PaintParams {
        self.bound_paint
            .as_ref()
            .map_or_else(PaintParams::disabled, |bound| bound.params)
    }

    /// Number of indices in this mesh's index buffer (3x the triangle count).
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Number of wireframe verts (3x the valid-triangle count).
    pub fn wire_count(&self) -> u32 {
        self.wire_count
    }

    /// Builds this frame's paint callback. `uniform` is computed by the
    /// caller from the current camera + viewport aspect ratio. `lut` is
    /// the app's display LUT (the viewer chain, applied as the pass's
    /// last step), or `None` for the context's identity table.
    /// Renders [`ViewMode::Lit`] — use
    /// [`Self::paint_callback_with_mode`] for the other view modes.
    pub fn paint_callback(
        &self,
        gpu: &GpuContext,
        uniform: CameraUniform,
        lut: Option<&crate::display_lut::DisplayLut>,
    ) -> MeshPaintCallback {
        self.paint_callback_with_mode(gpu, uniform, lut, ViewMode::Lit)
    }

    /// [`Self::paint_callback`] with an explicit [`ViewMode`]: the
    /// callback writes the mode's [`ViewModeParams`] into the mesh's
    /// view-mode buffer on `prepare` (like the camera uniform), so the
    /// shader's `view_mode` selector lands before this frame's draw.
    ///
    /// LUT contract: pass `mode.select_lut(lut)` as `lut` — data modes
    /// (data solos, every mesh-map preview) then bind the identity
    /// table, i.e. bypass the display transform per the §9 rule (see
    /// [`crate::view_mode`).
    pub fn paint_callback_with_mode(
        &self,
        gpu: &GpuContext,
        uniform: CameraUniform,
        lut: Option<&crate::display_lut::DisplayLut>,
        mode: ViewMode,
    ) -> MeshPaintCallback {
        MeshPaintCallback {
            pipeline: gpu.pipeline.clone(),
            bind_group: self.bind_group.clone(),
            lut_bind_group: lut.unwrap_or(&gpu.identity_lut).bind_group().clone(),
            vertex_buffer: self.vertex_buffer.clone(),
            index_buffer: self.index_buffer.clone(),
            index_count: self.index_count,
            uniform_buffer: self.uniform_buffer.clone(),
            uniform,
            view_mode_buffer: self.view_mode_buffer.clone(),
            view_mode: mode.uniform(),
        }
    }
}

/// Per-frame egui paint callback for the mesh pass.
///
/// Holds cloned wgpu resource handles (cheap: wgpu resource types are
/// internally `Arc`-backed) rather than reaching back into `GpuContext`/
/// `MeshBuffers`, because [`egui_wgpu::CallbackTrait`] callbacks must be
/// `'static` + `Send + Sync`.
pub struct MeshPaintCallback {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    /// Group 1: the display LUT's bind group (the app's, or the
    /// context's identity fallback).
    lut_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    uniform: CameraUniform,
    /// Mesh-pass binding 8's buffer (shared with the mesh) plus this
    /// frame's mode value, written by [`Self::write_uniform`].
    view_mode_buffer: wgpu::Buffer,
    view_mode: ViewModeParams,
}

impl MeshPaintCallback {
    /// `prepare`'s camera-uniform write, callable without egui's
    /// callback plumbing (the perf harness's offscreen frames). Also
    /// writes the view-mode selector (same per-frame uniform pattern).
    pub(crate) fn write_uniform(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniform));
        queue.write_buffer(
            &self.view_mode_buffer,
            0,
            bytemuck::bytes_of(&self.view_mode),
        );
    }

    /// Records the mesh draw onto any render pass — the state `paint`
    /// applies (pipeline, group 0, group 1 = display LUT, buffers).
    pub(crate) fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &self.lut_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }

    /// [`Self::draw`] through `pipeline` — the post-effects path's
    /// scene-linear mesh pipeline (`crate::postfx_viewport`), whose layout
    /// is group 0 only: the LUT gate is the entry point, so no group 1.
    pub(crate) fn draw_scene_linear(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
    ) {
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

impl egui_wgpu::CallbackTrait for MeshPaintCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        self.write_uniform(queue);
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        self.draw(render_pass);
    }
}

/// Per-frame egui paint callback for the OpenPBR mesh pass.
///
/// Structural mirror of [`MeshPaintCallback`]: cloned handles, params
/// baked into the bind group at shape-build time.
pub struct OpenPbrPaintCallback {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl OpenPbrPaintCallback {
    /// Draws the OpenPBR mesh into `render_pass` — the exact state the
    /// egui `CallbackTrait::paint` applies, reusable by offscreen tests.
    pub(crate) fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

impl egui_wgpu::CallbackTrait for OpenPbrPaintCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // Buffers were initialized at creation (create_buffer_init maps +
        // writes), nothing to stage per frame.
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        self.draw(render_pass);
    }
}

/// Per-frame egui paint callback for the wireframe overlay.
///
/// Structural mirror of [`MeshPaintCallback`]: cloned handles, the mesh's
/// camera bind group rebound at group 0 of the wire layout (the layout
/// object is shared, so this is the identical bind group — no rebuild),
/// the per-frame color at group 1.
pub struct WireframePaintCallback {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    color_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    uniform_buffer: wgpu::Buffer,
    uniform: CameraUniform,
}

impl WireframePaintCallback {
    /// Draws the wireframe into `render_pass` — the exact state the egui
    /// `CallbackTrait::paint` applies, reusable by offscreen tests. Uses
    /// a non-indexed draw over the duplicated-vertex buffer.
    pub(crate) fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &self.color_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

impl egui_wgpu::CallbackTrait for WireframePaintCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // Same camera uniform bytes the mesh pass writes this frame.
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniform));
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        self.draw(render_pass);
    }
}

/// Per-frame egui paint callback for the procedural ground grid.
///
/// Buffer-less fullscreen pass: the only per-frame state is the inverse
/// view-proj uniform, baked at creation (like the OpenPBR params), so
/// `prepare` stages nothing.
pub struct GridPaintCallback {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl GridPaintCallback {
    /// Draws the grid into `render_pass` — the exact state the egui
    /// `CallbackTrait::paint` applies, reusable by offscreen tests.
    pub(crate) fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

impl egui_wgpu::CallbackTrait for GridPaintCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    fn paint(
        &self,
        _info: epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        self.draw(render_pass);
    }
}

/// Wraps `callback` into an `epaint::Shape` ready for `ui.painter().add(..)`.
///
/// Kept in `umber-gpu` (rather than calling `egui_wgpu::Callback` from
/// `umber-app` directly) so the app crate never needs to name an
/// egui-wgpu/wgpu type itself — it only ever holds opaque values handed
/// back by this crate.
pub fn mesh_paint_shape(rect: epaint::emath::Rect, callback: MeshPaintCallback) -> epaint::Shape {
    egui_wgpu::Callback::new_paint_callback(rect, callback).into()
}

/// Wraps the wireframe overlay `callback` into an `epaint::Shape` — the
/// app adds it AFTER the mesh shape, in the same render pass, when the
/// wireframe toggle is on.
pub fn wire_paint_shape(
    rect: epaint::emath::Rect,
    callback: WireframePaintCallback,
) -> epaint::Shape {
    egui_wgpu::Callback::new_paint_callback(rect, callback).into()
}

/// Wraps the ground-grid `callback` into an `epaint::Shape` — the app
/// adds it BEFORE the mesh shape so geometry occludes the plane.
pub fn grid_paint_shape(rect: epaint::emath::Rect, callback: GridPaintCallback) -> epaint::Shape {
    egui_wgpu::Callback::new_paint_callback(rect, callback).into()
}

/// Smooth per-vertex normals for a shared-vertex (single-indexed) triangle
/// mesh, used when the source data has no normals at all (or a mismatched
/// count — some importers emit per-face data).
fn compute_vertex_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut accum = vec![glam::Vec3::ZERO; positions.len()];
    for tri in indices.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (Some(&pa), Some(&pb), Some(&pc)) =
            (positions.get(a), positions.get(b), positions.get(c))
        else {
            continue;
        };
        let (pa, pb, pc) = (
            glam::Vec3::from(pa),
            glam::Vec3::from(pb),
            glam::Vec3::from(pc),
        );
        // Assumes CCW front faces, matching the pipeline's cull_mode::Back
        // (review #7).
        let face_normal = (pb - pa).cross(pc - pa);
        accum[a] += face_normal;
        accum[b] += face_normal;
        accum[c] += face_normal;
    }
    accum
        .into_iter()
        .map(|n| {
            let n = n.normalize_or_zero();
            if n == glam::Vec3::ZERO {
                glam::Vec3::Y
            } else {
                n
            }
            .to_array()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_vertex_normals_single_triangle_points_up() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let indices = [0u32, 1, 2];
        let normals = compute_vertex_normals(&positions, &indices);
        assert_eq!(normals.len(), 3);
        for n in normals {
            let n = glam::Vec3::from(n);
            assert!((n.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn compute_vertex_normals_handles_degenerate_triangle() {
        let positions = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let indices = [0u32, 1, 2];
        let normals = compute_vertex_normals(&positions, &indices);
        assert_eq!(normals.len(), 3);
        for n in normals {
            assert!(glam::Vec3::from(n).is_finite());
        }
    }

    #[test]
    fn vertex_stays_32_bytes_and_paint_params_match_the_wgsl_layout() {
        // The uv replaced the old `_pad` in place: stride is unchanged,
        // so the OpenPBR pipeline (locations 0–1) needs no update.
        assert_eq!(size_of::<Vertex>(), 32);
        // PaintParams grew 32→48 for `height_strength` (worklist #12,
        // binding 12); the WGSL block and the assert at the struct's
        // definition carry the same number.
        assert_eq!(size_of::<PaintParams>(), 48);
    }

    #[test]
    fn paint_params_constructors_pin_the_tile_select() {
        // Disabled: no flag, but still the identity transform — a stale
        // uniform can never skew a re-bind.
        let disabled = PaintParams::disabled();
        assert_eq!(disabled.flags, 0);
        assert_eq!(disabled.uv_offset, [0.0, 0.0]);
        assert_eq!(disabled.uv_scale, [1.0, 1.0]);

        // Single tile: sample uvs directly.
        let identity = PaintParams::identity();
        assert_eq!(identity.flags, PaintParams::HAS_TEXTURE);
        assert_eq!(identity.uv_offset, [0.0, 0.0]);
        assert_eq!(identity.uv_scale, [1.0, 1.0]);

        // UDIM: offset = the tile's origin, scale 1 (the shader samples
        // the fractional part for uvs inside the tile).
        assert_eq!(PaintParams::for_udim(1001).uv_offset, [0.0, 0.0]);
        assert_eq!(PaintParams::for_udim(1002).uv_offset, [1.0, 0.0]);
        assert_eq!(PaintParams::for_udim(1011).uv_offset, [0.0, 1.0]);
        assert_eq!(PaintParams::for_udim(1100).uv_offset, [9.0, 9.0]);
        for tile in [1001, 1002, 1011, 1100] {
            let params = PaintParams::for_udim(tile);
            assert_eq!(params.flags, PaintParams::HAS_TEXTURE);
            assert_eq!(params.uv_scale, [1.0, 1.0]);
        }
        // Out-of-range tiles clamp to the 10x10 grid (the udim crate's
        // own clamp policy, mirrored here so a bad tile id can't
        // address outside the texture).
        assert_eq!(PaintParams::for_udim(999).uv_offset, [0.0, 0.0]);
        assert_eq!(PaintParams::for_udim(9999).uv_offset, [9.0, 9.0]);
    }

    #[cfg(feature = "gpu")]
    mod gpu {
        // NOTE: `wgpu::util::DeviceExt` (for `create_buffer_init` below)
        // arrives via this glob: the parent module imports it and glob
        // imports carry a module's `use` names to child modules — so no
        // explicit import here (clippy would flag it unused).
        use super::super::*;
        use crate::ibl::{EnvFormat, EnvIrradiance};

        fn try_request_device() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
            let instance = wgpu::Instance::default();
            let adapter = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            )
            .ok()?;
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                    .ok()?;
            Some((adapter, device, queue))
        }

        #[test]
        fn mesh_buffers_upload_roundtrip() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping mesh_buffers_upload_roundtrip: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = umber_mesh::MeshData {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![],
                uvs: vec![],
                indices: vec![0, 1, 2],
                material_names: vec![],
                triangle_materials: Vec::new(),
            };
            let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload should succeed");
            assert_eq!(buffers.index_count(), 3);

            let uniform = CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::NEG_Y);
            let _callback = buffers.paint_callback(&gpu, uniform, None);
        }

        #[test]
        fn empty_mesh_is_rejected() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping empty_mesh_is_rejected: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = umber_mesh::MeshData::default();
            assert!(matches!(
                MeshBuffers::upload(&gpu, &mesh),
                Err(GpuError::EmptyMesh)
            ));
        }

        /// Offscreen-renders a triangle through the OpenPBR pipeline and
        /// returns the 4x4 target's bytes.
        fn render_openpbr(gpu: &GpuContext, params: crate::material::OpenPbrParams) -> Vec<u8> {
            let device = &gpu.device;
            let mesh = umber_mesh::MeshData {
                positions: vec![[-1.0, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs: vec![],
                indices: vec![0, 1, 2],
                material_names: vec![],
                triangle_materials: Vec::new(),
            };
            let buffers = MeshBuffers::upload(gpu, &mesh).expect("upload succeeds");
            let uniform = CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::NEG_Y);
            let callback = gpu.openpbr_callback(&buffers, uniform, params);

            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_openpbr_test_target"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_openpbr_test_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("umber_openpbr_test_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                callback.draw(&mut pass);
            }
            gpu.queue.submit(Some(encoder.finish()));

            // Readback: rows must respect COPY_BYTES_PER_ROW_ALIGNMENT
            // (256); 4x4 RGBA rows are 16 bytes, so pad each row to 256
            // and extract the first 16 bytes of every padded row.
            const PADDED_ROW: u32 = 256;
            let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("umber_openpbr_test_readback"),
                size: PADDED_ROW as u64 * 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_openpbr_test_readback_encoder"),
            });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(PADDED_ROW),
                        rows_per_image: Some(4),
                    },
                },
                wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
            );
            gpu.queue.submit(Some(enc.finish()));

            let (sx, rx) = std::sync::mpsc::channel();
            readback_buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sx.send(result);
                });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .expect("poll succeeds");
            rx.recv().expect("map callback ran").expect("map ok");
            let padded = readback_buffer
                .slice(..)
                .get_mapped_range()
                .expect("mapped range available")
                .to_vec();
            readback_buffer.unmap();
            let mut data = Vec::with_capacity(4 * 4 * 4);
            for row in padded.chunks_exact(PADDED_ROW as usize) {
                data.extend_from_slice(&row[..16]);
            }
            data
        }

        #[test]
        fn openpbr_defaults_render_nonzero_and_bounded() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping openpbr_defaults_render_nonzero_and_bounded: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let bytes = render_openpbr(&gpu, crate::material::OpenPbrParams::default());
            // Non-zero: the triangle covers every pixel of the 4x4.
            let nonzero = bytes
                .chunks_exact(4)
                .any(|px| px[0] > 0 || px[1] > 0 || px[2] > 0);
            assert!(nonzero, "OpenPBR default render must be non-zero");
            // Energy sanity note: channels are u8 and in-range by
            // construction; a NaN leak surfaces as saturation, which
            // the nonzero check plus the live-render tests would
            // catch as a pixel-level anomaly. No vacuous range
            // assertion here (clippy: absurd_extreme_comparisons).
        }

        #[test]
        fn openpbr_metalness_changes_response() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping openpbr_metalness_changes_response: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let dielectric = render_openpbr(&gpu, crate::material::OpenPbrParams::default());
            let mut metal = crate::material::OpenPbrParams::default();
            metal.surface[0] = 1.0; // base_metalness = 1
            let metallic = render_openpbr(&gpu, metal);
            assert_ne!(
                dielectric, metallic,
                "metalness=1 must shade differently from metalness=0"
            );
        }

        // --- Wave-4 item 7 (overlays) tests. Same offscreen pattern as
        // the IBL tests below: real pipelines, real bind groups, linear
        // Rgba8Unorm target, padded-row readback. The camera math is
        // derived IN RUST via the shared `crate::world_from_ndc` helper
        // (the mirrored-math pattern) — never hardcoded pixels.

        /// What one overlay-test frame draws: grid first (when
        /// `grid_view_proj` is `Some`), then the mesh (when `buffers` is
        /// `Some`), then the wireframe (when `wire_color` is `Some`) —
        /// the app's frame order.
        struct OverlayFrame<'a> {
            buffers: Option<&'a MeshBuffers>,
            uniform: CameraUniform,
            grid_view_proj: Option<glam::Mat4>,
            wire_color: Option<super::WireColor>,
            w: u32,
            h: u32,
            clear: wgpu::Color,
        }

        /// Renders one [`OverlayFrame`] into a linear target and returns
        /// tightly-packed RGBA8 bytes — in ONE pass with an optional
        /// depth attachment matching the context's depth format.
        fn render_overlays(gpu: &GpuContext, frame: OverlayFrame<'_>) -> Vec<u8> {
            let OverlayFrame {
                buffers,
                uniform,
                grid_view_proj,
                wire_color,
                w,
                h,
                clear,
            } = frame;
            let device = &gpu.device;
            if let Some(buffers) = buffers {
                gpu.queue
                    .write_buffer(&buffers.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
            }
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_overlay_test_target"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let depth_view = gpu.depth_format().map(|format| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("umber_overlay_test_depth"),
                        size: wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            });

            let grid_callback = grid_view_proj.map(|vp| gpu.grid_callback(vp));
            let wire_callback = wire_color.map(|color| {
                let buffers = buffers.expect("wireframe draw needs mesh buffers");
                gpu.wire_callback(buffers, uniform, color)
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_overlay_test_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("umber_overlay_test_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: depth_view.as_ref().map(|depth| {
                        wgpu::RenderPassDepthStencilAttachment {
                            view: depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if let Some(grid) = &grid_callback {
                    grid.draw(&mut pass);
                }
                if let Some(buffers) = buffers {
                    // The exact state `MeshPaintCallback::paint` applies
                    // (group 1 = the identity display LUT).
                    pass.set_pipeline(&gpu.pipeline);
                    pass.set_bind_group(0, &buffers.bind_group, &[]);
                    pass.set_bind_group(1, gpu.identity_lut.bind_group(), &[]);
                    pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
                    pass.set_index_buffer(
                        buffers.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..buffers.index_count, 0, 0..1);
                }
                if let Some(wire) = &wire_callback {
                    wire.draw(&mut pass);
                }
            }
            gpu.queue.submit(Some(encoder.finish()));
            readback_packed(device, &gpu.queue, &target, w, h)
        }

        /// Padded-row readback into tightly-packed RGBA8 (the IBL tests'
        /// pattern, factored so the overlay tests share it).
        fn readback_packed(
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            target: &wgpu::Texture,
            w: u32,
            h: u32,
        ) -> Vec<u8> {
            const PADDED_ROW: u32 = 256;
            assert!(
                w * 4 <= PADDED_ROW,
                "overlay test targets must fit one row per 256-byte stride"
            );
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("umber_overlay_test_readback"),
                size: PADDED_ROW as u64 * h as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_overlay_test_readback_encoder"),
            });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(PADDED_ROW),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(enc.finish()));

            let (sx, rx) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sx.send(result);
                });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .expect("poll succeeds");
            rx.recv().expect("map callback ran").expect("map ok");
            let padded = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped range available")
                .to_vec();
            readback.unmap();
            let mut data = Vec::with_capacity((w * h * 4) as usize);
            for row in padded.chunks_exact(PADDED_ROW as usize) {
                data.extend_from_slice(&row[..(w * 4) as usize]);
            }
            data
        }

        /// Projects a world point to the CONTAINING pixel in a W×H
        /// target (NDC Y-up → row 0 at top, matching `pick_uv`):
        /// float-pixel coords truncated toward the pixel origin and
        /// clamped into range (a point exactly at the far edge lands in
        /// the last pixel, never one past it).
        fn world_to_pixel(view_proj: glam::Mat4, world: glam::Vec3, w: u32, h: u32) -> (u32, u32) {
            let (fx, fy) = world_to_pixel_f(view_proj, world, w, h);
            (
                (fx.floor() as u32).min(w - 1),
                (fy.floor() as u32).min(h - 1),
            )
        }

        /// Float-pixel projection (pixel centers at half-integers).
        fn world_to_pixel_f(
            view_proj: glam::Mat4,
            world: glam::Vec3,
            w: u32,
            h: u32,
        ) -> (f32, f32) {
            let clip = view_proj * world.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            (
                (ndc.x + 1.0) * 0.5 * w as f32,
                (1.0 - ndc.y) * 0.5 * h as f32,
            )
        }

        /// The Rust mirror of the grid shader's fragment math: NDC
        /// near/far unprojected via the shared `world_from_ndc`, ray
        /// intersected with the y=0 plane. Lets the test prove a pixel
        /// really contains a world target before asserting on its bytes.
        fn grid_pixel_world(
            inv_view_proj: glam::Mat4,
            i: u32,
            j: u32,
            w: u32,
            h: u32,
        ) -> glam::Vec3 {
            let ndc_x = (i as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let ndc_y = 1.0 - (j as f32 + 0.5) / h as f32 * 2.0;
            let near =
                crate::world_from_ndc(inv_view_proj, glam::Vec4::new(ndc_x, ndc_y, 0.0, 1.0));
            let far = crate::world_from_ndc(inv_view_proj, glam::Vec4::new(ndc_x, ndc_y, 1.0, 1.0));
            let dir = far - near;
            near + dir * (-near.y / dir.y)
        }

        /// GRID TEST (the design's test 1): camera straight down at
        /// height 10. World (0.5, 0, 0.5) is a cell center (NOT a line);
        /// world (1.0, 0, 0.5) sits ON a minor line. Both pixel positions
        /// are derived from the camera math in Rust (never hardcoded),
        /// and each is forward-checked through `grid_pixel_world` to
        /// prove the world target really falls inside that pixel.
        #[test]
        fn grid_cell_center_vs_line_pixels() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping grid_cell_center_vs_line_pixels: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            // Straight down: eye +Y over the origin. Up is -Z — up=+Y
            // would be parallel to the view direction (degenerate
            // look_at). Same `rh` camera modules `camera.rs` uses.
            let eye = glam::Vec3::new(0.0, 10.0, 0.0);
            let view =
                glam::camera::rh::view::look_at_mat4(eye, glam::Vec3::ZERO, glam::Vec3::NEG_Z);
            let proj = glam::camera::rh::proj::directx::perspective(
                45.0_f32.to_radians(),
                1.0,
                0.1,
                100.0,
            );
            let view_proj = proj * view;
            let inv = view_proj.inverse();

            const W: u32 = 64;
            const H: u32 = 64;
            // World-per-pixel ≈ 2*10*tan(22.5°)/64 ≈ 0.13 — the half-
            // pixel corpului: a target within 0.065 world units of a
            // pixel's ray hit provably lies in that pixel.
            let (ci, cj) = world_to_pixel(view_proj, glam::Vec3::new(0.5, 0.0, 0.5), W, H);
            let (li, lj) = world_to_pixel(view_proj, glam::Vec3::new(1.0, 0.0, 0.5), W, H);
            let center_hit = grid_pixel_world(inv, ci, cj, W, H);
            assert!(
                (center_hit.x - 0.5).abs() < 0.065 && (center_hit.z - 0.5).abs() < 0.065,
                "cell-center pixel ({ci},{cj}) must contain (0.5,0,0.5), hit {center_hit:?}"
            );
            let line_hit = grid_pixel_world(inv, li, lj, W, H);
            assert!(
                (line_hit.x - 1.0).abs() < 0.065 && line_hit.y.abs() < 0.065,
                "line pixel ({li},{lj}) must contain the x=1 line at y=0, hit {line_hit:?}"
            );

            // Grid OFF renders exactly the clear color (byte-identical —
            // the design's test 2): nothing is drawn, so every pixel
            // must equal the clear bytes, not merely resemble them.
            let off = render_overlays(
                &gpu,
                OverlayFrame {
                    buffers: None,
                    uniform: CameraUniform::new(view_proj, glam::Vec3::NEG_Y),
                    grid_view_proj: None,
                    wire_color: None,
                    w: W,
                    h: H,
                    clear: wgpu::Color::BLACK,
                },
            );
            assert_eq!(off.len(), (W * H * 4) as usize);
            for (i, px) in off.chunks_exact(4).enumerate() {
                assert_eq!(
                    px,
                    &[0, 0, 0, 255],
                    "grid-off pixel {i} must equal the clear color exactly"
                );
            }

            let on = render_overlays(
                &gpu,
                OverlayFrame {
                    buffers: None,
                    uniform: CameraUniform::new(view_proj, glam::Vec3::NEG_Y),
                    grid_view_proj: Some(view_proj),
                    wire_color: None,
                    w: W,
                    h: H,
                    clear: wgpu::Color::BLACK,
                },
            );
            let at = |bytes: &[u8], i: u32, j: u32| {
                let base = ((j * W + i) * 4) as usize;
                [
                    bytes[base],
                    bytes[base + 1],
                    bytes[base + 2],
                    bytes[base + 3],
                ]
            };
            // Cell center: the fragment discards below the alpha
            // threshold (line distance 0.5 world units >> fwidth), so
            // the pixel is the clear color EXACTLY.
            assert_eq!(
                at(&on, ci, cj),
                [0, 0, 0, 255],
                "cell-center pixel must stay the clear color with the grid on"
            );
            // On the line: the pixel is brighter — the line pixel sits
            // within half a pixel-world of x=1, so coverage ≥ 0.5 and
            // the 25%-white minor line lands ≥31 LSB over black; the
            // assert pins half that (2× margin for adapter variance).
            let line_px = at(&on, li, lj);
            assert_ne!(
                line_px,
                [0, 0, 0, 255],
                "on-line pixel ({li},{lj}) must differ from the clear color"
            );
            assert!(
                line_px[0].max(line_px[1]).max(line_px[2]) >= 16,
                "on-line pixel ({li},{lj}) must be visibly brighter, got {line_px:?}"
            );
        }

        /// Distance from a float-pixel point to a float-pixel segment.
        fn seg_distance(px: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
            let ab = (b.0 - a.0, b.1 - a.1);
            let len_sq = ab.0 * ab.0 + ab.1 * ab.1;
            let t = if len_sq < 1e-9 {
                0.0
            } else {
                (((px.0 - a.0) * ab.0 + (px.1 - a.1) * ab.1) / len_sq).clamp(0.0, 1.0)
            };
            let closest = (a.0 + ab.0 * t, a.1 + ab.1 * t);
            ((px.0 - closest.0).powi(2) + (px.1 - closest.1).powi(2)).sqrt()
        }

        /// WIREFRAME TEST (the design's test pin): a single quad (two
        /// triangles sharing a diagonal), wireframe on. The diagonal
        /// edge pixel row MUST exist (byte-diff vs wireframe-off at the
        /// diagonal), and mesh shading elsewhere MUST be unchanged —
        /// every differing pixel must lie in the edge neighborhood, whose
        /// count E is derived from the projected edge geometry (the
        /// bound: differing_total ≤ E).
        #[test]
        fn wireframe_diagonal_exists_and_interior_unchanged() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping wireframe_diagonal_exists_and_interior_unchanged: no wgpu adapter available"
                );
                return;
            };
            // Real depth range (the app's configuration): the
            // clip-space bias must resolve inside [near, far] — with an
            // identity view-proj the mesh depth sits exactly at 0 and any
            // negative bias clips away, so the test needs perspective.
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                Some(wgpu::TextureFormat::Depth32Float),
            );
            let mesh = umber_mesh::MeshData {
                positions: vec![
                    [-1.0, -1.0, 0.0],
                    [1.0, -1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [-1.0, 1.0, 0.0],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 4],
                uvs: vec![],
                indices: vec![0, 1, 2, 0, 2, 3],
                material_names: vec![],
                triangle_materials: Vec::new(),
            };
            let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            // Builder→GPU plumbing pin: 2 triangles → 6 wire verts.
            assert_eq!(buffers.wire_count(), 6);

            // Perspective camera framing the quad (the orbit camera's own
            // framing math — same matrices the live viewport uses).
            let (min, max) = mesh.bounds().expect("quad has bounds");
            let cam = crate::OrbitCamera::framing(min, max, 0.6, 0.4);
            const W: u32 = 32;
            const H: u32 = 32;
            let view_proj = cam.view_proj(W as f32 / H as f32);
            let uniform = CameraUniform::new(view_proj, glam::Vec3::NEG_Y);

            let off = render_overlays(
                &gpu,
                OverlayFrame {
                    buffers: Some(&buffers),
                    uniform,
                    grid_view_proj: None,
                    wire_color: None,
                    w: W,
                    h: H,
                    clear: wgpu::Color::BLACK,
                },
            );
            let on = render_overlays(
                &gpu,
                OverlayFrame {
                    buffers: Some(&buffers),
                    uniform,
                    grid_view_proj: None,
                    wire_color: Some(super::WireColor::default()),
                    w: W,
                    h: H,
                    clear: wgpu::Color::BLACK,
                },
            );
            assert_eq!(off.len(), on.len());

            // Projected edge segments in float-pixel space: the 4
            // perimeter edges plus the shared diagonal (corner 0→2).
            let corners = [
                world_to_pixel_f(view_proj, glam::Vec3::new(-1.0, -1.0, 0.0), W, H),
                world_to_pixel_f(view_proj, glam::Vec3::new(1.0, -1.0, 0.0), W, H),
                world_to_pixel_f(view_proj, glam::Vec3::new(1.0, 1.0, 0.0), W, H),
                world_to_pixel_f(view_proj, glam::Vec3::new(-1.0, 1.0, 0.0), W, H),
            ];
            let segments = [
                (corners[0], corners[1]),
                (corners[1], corners[2]),
                (corners[2], corners[3]),
                (corners[3], corners[0]),
                (corners[0], corners[2]),
            ];
            // Edge neighborhood E: every pixel whose center lies within
            // 1.5px of ANY triangle edge. fwidth 1px lines can only touch
            // these pixels — anything differing beyond E is mesh-shading
            // corruption, not wire.
            let mut edge_neighborhood = vec![false; (W * H) as usize];
            for j in 0..H {
                for i in 0..W {
                    let px = (i as f32 + 0.5, j as f32 + 0.5);
                    if segments.iter().any(|&(a, b)| seg_distance(px, a, b) <= 1.5) {
                        edge_neighborhood[(j * W + i) as usize] = true;
                    }
                }
            }
            let bound = edge_neighborhood.iter().filter(|&&b| b).count();

            let mut differing_total = 0usize;
            let mut diag_differ = 0usize;
            for (idx, (a, b)) in off.chunks_exact(4).zip(on.chunks_exact(4)).enumerate() {
                if a == b {
                    continue;
                }
                differing_total += 1;
                let i = (idx as u32) % W;
                let j = (idx as u32) / W;
                assert!(
                    edge_neighborhood[idx],
                    "pixel ({i},{j}) differs outside the edge neighborhood — wireframe corrupted mesh shading"
                );
                // Strictly on-diagonal: within 0.75px of the shared edge.
                let px = (i as f32 + 0.5, j as f32 + 0.5);
                if seg_distance(px, corners[0], corners[2]) <= 0.75 {
                    diag_differ += 1;
                }
            }
            // Existence: the diagonal row renders (a boundary-only
            // implementation would leave diag_differ at 0).
            assert!(
                diag_differ >= 3,
                "the shared diagonal must shade on-diagonal pixels (got {diag_differ})"
            );
            // Unchanged elsewhere: every differing pixel is inside E by
            // construction of the assert above; this pins the total
            // against the geometry-derived estimate so the bound can't
            // silently inflate (E ≈ perimeter+diagonal pixels here).
            assert!(
                differing_total <= bound,
                "differing pixels ({differing_total}) must stay within the edge-neighborhood estimate ({bound})"
            );
            assert!(
                bound < (W * H) as usize / 2,
                "neighborhood estimate ({bound}) must be a small fraction of the frame"
            );
        }

        /// UPLOAD PLUMBING: an index-less mesh uploads fine with an empty
        /// wire draw (padding vert, count 0) — the overlay path must not
        /// make index-less meshes uploadable-today fail.
        #[test]
        fn wire_upload_handles_index_less_mesh() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping wire_upload_handles_index_less_mesh: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = umber_mesh::MeshData {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: vec![],
                uvs: vec![],
                indices: vec![],
                material_names: vec![],
                triangle_materials: Vec::new(),
            };
            let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            assert_eq!(buffers.wire_count(), 0);
            // And the mesh-only draw still works through the overlay
            // helper with both toggles off.
            let uniform = CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::NEG_Y);
            let bytes = render_overlays(
                &gpu,
                OverlayFrame {
                    buffers: Some(&buffers),
                    uniform,
                    grid_view_proj: None,
                    wire_color: None,
                    w: 8,
                    h: 8,
                    clear: wgpu::Color::BLACK,
                },
            );
            assert_eq!(bytes.len(), 8 * 8 * 4);
        }

        // --- Wave-4 item 6 (IBL) mesh-pass tests. `golden::RenderTarget`
        // can't serve here — its `callback_paint` is a documented no-op
        // stub — so these tests draw for real: same pipeline, same bind
        // group, same buffers as `MeshPaintCallback::paint`, into a
        // linear (non-sRGB) Rgba8Unorm target so the expected bytes are
        // closed-form shader math, not tone-mapped guesses.

        /// Full-screen triangle (the big-triangle trick: covers every
        /// pixel) with a constant normal, CCW so backface culling keeps
        /// it (see `compute_vertex_normals`' CCW note).
        fn fullscreen_mesh(normal: [f32; 3]) -> umber_mesh::MeshData {
            umber_mesh::MeshData {
                positions: vec![[-1.0, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]],
                normals: vec![normal; 3],
                uvs: vec![],
                indices: vec![0, 1, 2],
                material_names: vec![],
                triangle_materials: Vec::new(),
            }
        }

        /// Renders `buffers` (with its CURRENT bind group — whatever
        /// `set_environment` last installed) into an 8×8 linear target
        /// and returns tightly-packed RGBA8 bytes.
        fn render_mesh_offscreen(gpu: &GpuContext, buffers: &MeshBuffers) -> Vec<u8> {
            const W: u32 = 8;
            const H: u32 = 8;
            let device = &gpu.device;
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_ibl_test_target"),
                size: wgpu::Extent3d {
                    width: W,
                    height: H,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let uniform = CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::NEG_Y);
            gpu.queue
                .write_buffer(&buffers.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_ibl_test_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("umber_ibl_test_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                // The exact state `MeshPaintCallback::paint` applies
                // (group 1 = the identity display LUT — so the
                // closed-form fallback tests below double as the mesh
                // pass's identity-LUT no-regression proof).
                pass.set_pipeline(&gpu.pipeline);
                pass.set_bind_group(0, &buffers.bind_group, &[]);
                pass.set_bind_group(1, gpu.identity_lut.bind_group(), &[]);
                pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
                pass.set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..buffers.index_count, 0, 0..1);
            }
            gpu.queue.submit(Some(encoder.finish()));

            const PADDED_ROW: u32 = 256;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("umber_ibl_test_readback"),
                size: PADDED_ROW as u64 * H as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_ibl_test_readback_encoder"),
            });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(PADDED_ROW),
                        rows_per_image: Some(H),
                    },
                },
                wgpu::Extent3d {
                    width: W,
                    height: H,
                    depth_or_array_layers: 1,
                },
            );
            gpu.queue.submit(Some(enc.finish()));

            let (sx, rx) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sx.send(result);
                });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .expect("poll succeeds");
            rx.recv().expect("map callback ran").expect("map ok");
            let padded = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped range available")
                .to_vec();
            readback.unmap();
            let mut data = Vec::with_capacity((W * H * 4) as usize);
            for row in padded.chunks_exact(PADDED_ROW as usize) {
                data.extend_from_slice(&row[..(W * 4) as usize]);
            }
            data
        }

        /// Closed-form procedural byte for a constant normal under the
        /// test light (`NEG_Y`, so `to_light = +Y`): the shader's
        /// `BASE_COLOR * (mix(GROUND, SKY, t) + diffuse * SUN * 0.55)`,
        /// rounded half-up. The GPU's own unorm rounding (nearest-even
        /// vs. half-up) can only disagree at an exact .5-in-byte-units
        /// boundary — the margin note below shows the test normal sits
        /// ≥0.03 byte-units clear of every boundary (the ao.rs
        /// `encode_unorm` precedent).
        fn expected_procedural_byte(normal: [f32; 3]) -> [u8; 4] {
            const GROUND: [f32; 3] = [0.30, 0.27, 0.24];
            const SKY: [f32; 3] = [0.45, 0.55, 0.78];
            const BASE: [f32; 3] = [0.72, 0.72, 0.75];
            const SUN: [f32; 3] = [1.0, 0.96, 0.90];
            let t = (normal[1] * 0.5 + 0.5).clamp(0.0, 1.0);
            let env = [
                GROUND[0] + (SKY[0] - GROUND[0]) * t,
                GROUND[1] + (SKY[1] - GROUND[1]) * t,
                GROUND[2] + (SKY[2] - GROUND[2]) * t,
            ];
            // to_light = +Y (light_dir = NEG_Y).
            let diffuse = normal[1].max(0.0);
            let px = [
                BASE[0] * (env[0] + diffuse * SUN[0] * 0.55),
                BASE[1] * (env[1] + diffuse * SUN[1] * 0.55),
                BASE[2] * (env[2] + diffuse * SUN[2] * 0.55),
            ];
            [
                (px[0] * 255.0 + 0.5).floor() as u8,
                (px[1] * 255.0 + 0.5).floor() as u8,
                (px[2] * 255.0 + 0.5).floor() as u8,
                255,
            ]
        }

        /// FALLBACK REGRESSION (the design's test 3): with `env_flags =
        /// 0` the mesh pass renders the procedural path byte-identical
        /// to the closed-form expectation above — the shader change must
        /// not reshape the fallback by a single LSB beyond unorm
        /// rounding. Normal +Z, light +Y: diffuse = 0, t = 0.5, expected
        /// (69, 75, 98, 255); margins to the nearest .5 boundary are
        /// 0.35/0.22/0.04 byte-units — f32 arithmetic noise is ~1e-5, so
        /// a ≤1-LSB tolerance only admits genuine rounding, never drift.
        #[test]
        fn fallback_procedural_matches_closed_form() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping fallback_procedural_matches_closed_form: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = fullscreen_mesh([0.0, 0.0, 1.0]);
            let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let bytes = render_mesh_offscreen(&gpu, &buffers);
            let want = expected_procedural_byte([0.0, 0.0, 1.0]);
            assert_eq!(bytes.len(), 8 * 8 * 4);
            for (i, px) in bytes.chunks_exact(4).enumerate() {
                for c in 0..4 {
                    let diff = px[c].abs_diff(want[c]);
                    assert!(
                        diff <= 1,
                        "pixel {i} ch{c}: got {px:?}, want {want:?} (≤1 LSB unorm rounding)"
                    );
                }
            }
        }

        /// DISPLAY LUT, MESH PASS (can-fail): the app's real path —
        /// `paint_callback(.., Some(lut))` → `MeshPaintCallback::draw`
        /// setting group 1 — with an inverting table (entry i = 255 − i)
        /// renders 255 − the closed-form procedural byte. A mesh pass
        /// that ignored its LUT (or a callback that bound the identity
        /// fallback instead) renders the un-inverted (69, 75, 98).
        #[test]
        fn mesh_pass_reads_the_bound_display_lut() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping mesh_pass_reads_the_bound_display_lut: no wgpu adapter");
                return;
            };
            const W: u32 = 8;
            const H: u32 = 8;
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = fullscreen_mesh([0.0, 0.0, 1.0]);
            let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let mut inverted = [0u8; crate::display_lut::DISPLAY_LUT_BYTES];
            for (i, entry) in inverted.chunks_exact_mut(4).enumerate() {
                let b = 255 - i as u8;
                entry.copy_from_slice(&[b, b, b, 255]);
            }
            let lut = crate::display_lut::DisplayLut::new(&gpu, &inverted);
            let uniform = CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::NEG_Y);
            let callback = buffers.paint_callback(&gpu, uniform, Some(&lut));
            callback.write_uniform(&gpu.queue);

            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_mesh_lut_test_target"),
                size: wgpu::Extent3d {
                    width: W,
                    height: H,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("umber_mesh_lut_test_encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("umber_mesh_lut_test_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                callback.draw(&mut pass);
            }
            gpu.queue.submit(Some(encoder.finish()));
            let bytes = readback_packed(&gpu.device, &gpu.queue, &target, W, H);

            let plain = expected_procedural_byte([0.0, 0.0, 1.0]);
            let want = [255 - plain[0], 255 - plain[1], 255 - plain[2], 255];
            assert_eq!(bytes.len(), (W * H * 4) as usize);
            for (i, px) in bytes.chunks_exact(4).enumerate() {
                for c in 0..4 {
                    let diff = px[c].abs_diff(want[c]);
                    assert!(
                        diff <= 1,
                        "pixel {i} ch{c}: got {px:?}, want {want:?} (inverted LUT)"
                    );
                }
            }
        }

        /// The flag-0 path must be *texture-independent*: the same scene
        /// with a bright-white map bound but flags forced to 0 renders
        /// byte-IDENTICAL (not within tolerance — identical) to the
        /// fallback bind group. An `if` on a uniform is uniform control
        /// flow, so the unsampled texture cannot leak into the output;
        /// any difference is a real wiring bug.
        #[test]
        fn fallback_ignores_bound_map() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping fallback_ignores_bound_map: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = fullscreen_mesh([0.0, 1.0, 0.0]);
            let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let plain = render_mesh_offscreen(&gpu, &buffers);

            // Bright uniform map, but flags forced procedural: build the
            // bind group by hand (the public `set_environment` couples
            // map+flag, which is exactly what this test bypasses).
            let white = vec![4.0f32; 16 * 8 * 4];
            let env = EnvIrradiance::from_equirect(
                &gpu.device,
                &gpu.queue,
                bytemuck::cast_slice(&white),
                16,
                8,
                EnvFormat::Rgba32Float,
            )
            .expect("convolve succeeds");
            let flags = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("umber_ibl_test_flags0"),
                    contents: bytemuck::bytes_of(&crate::ibl::EnvFlags::procedural()),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let mut masked = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            // The specular tier binds through the same full constructor
            // (flag-gated like the irradiance map — flags=0 keeps it
            // unsampled too).
            let spec = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("umber_ibl_test_spec"),
                    contents: bytemuck::bytes_of(&crate::ibl::SpecParams::default()),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let tier = MeshTier {
                prefilter_views: [
                    env.prefilter_view(0),
                    env.prefilter_view(1),
                    env.prefilter_view(2),
                ],
                spec_buffer: &spec,
            };
            // Hand-built group binds the unbound-paint fallback (the
            // flag-0 proof below is about the IBL set — paint stays off).
            let paint = MeshPaint {
                view: gpu.fallback_paint_view(),
                sampler: gpu.paint_sampler(),
                params_buffer: &masked.paint_params_buffer,
                height_view: gpu.fallback_paint_view(),
            };
            masked.bind_group = gpu.mesh_bind_group_full(
                &masked.uniform_buffer,
                &flags,
                env.view(),
                env.sampler(),
                MeshTail {
                    tier,
                    paint,
                    view_mode_buffer: &masked.view_mode_buffer,
                },
            );
            let masked_bytes = render_mesh_offscreen(&gpu, &masked);

            assert_eq!(
                plain, masked_bytes,
                "flags=0 must ignore the bound map byte-for-byte"
            );
        }

        /// THE CAN-FAIL END-TO-END (the design's test 4): the same scene
        /// with the map enabled vs. procedural must DIFFER — an IBL that
        /// silently no-ops fails this. Synthetic two-tone equirect (red
        /// +Y cap, green -Y) so the delta is unmistakable at a +Y-facing
        /// normal; threshold is >50% of pixels differing by >8 LSB in
        /// any channel — robust across adapters (convolution + unorm
        /// noise is ~1 LSB, the expected delta is ~100).
        #[test]
        fn env_enabled_differs_from_procedural() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping env_enabled_differs_from_procedural: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = fullscreen_mesh([0.0, 1.0, 0.0]);
            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let plain = render_mesh_offscreen(&gpu, &buffers);

            // Two-tone equirect in GPU row order (row 0 = -Y = green,
            // top rows = +Y = red).
            const W: u32 = 64;
            const H: u32 = 32;
            let mut pixels = vec![0.0f32; (W * H * 4) as usize];
            for y in 0..H {
                let top = y >= H / 2;
                for x in 0..W {
                    let base = ((y * W + x) * 4) as usize;
                    if top {
                        pixels[base..base + 4].copy_from_slice(&[3.0, 0.25, 0.2, 1.0]);
                    } else {
                        pixels[base..base + 4].copy_from_slice(&[0.2, 1.5, 0.25, 1.0]);
                    }
                }
            }
            let env = EnvIrradiance::from_equirect(
                &gpu.device,
                &gpu.queue,
                bytemuck::cast_slice(&pixels),
                W,
                H,
                EnvFormat::Rgba32Float,
            )
            .expect("convolve succeeds");
            buffers.set_environment(&gpu, Some(&env));
            let mapped = render_mesh_offscreen(&gpu, &buffers);

            assert_eq!(plain.len(), mapped.len());
            let mut differ = 0usize;
            let (mut sum_r, mut sum_b) = (0u64, 0u64);
            for (a, b) in plain.chunks_exact(4).zip(mapped.chunks_exact(4)) {
                if a[0].abs_diff(b[0]) > 8 || a[1].abs_diff(b[1]) > 8 || a[2].abs_diff(b[2]) > 8 {
                    differ += 1;
                }
                sum_r += b[0] as u64;
                sum_b += b[2] as u64;
            }
            let total = plain.len() / 4;
            assert!(
                differ > total / 2,
                "env render must differ from procedural on most pixels (got {differ}/{total})"
            );
            // The map actually drove the output: red cap above means
            // the +Y-facing render skews red over blue.
            assert!(
                sum_r > sum_b,
                "mapped render should skew red (R sum {sum_r} vs B sum {sum_b})"
            );

            // And unsetting restores the fallback exactly.
            buffers.set_environment(&gpu, None);
            let restored = render_mesh_offscreen(&gpu, &buffers);
            assert_eq!(
                plain, restored,
                "unsetting the env must restore fallback bytes"
            );
        }

        /// Bright-half-space fixture for the specular tests: uniform
        /// dim gray with radiance 0.3 over the whole +X half-space
        /// (`d[0] > 0`), 0.05 elsewhere. A half-space (not a narrow
        /// cap): the mirror direction must land DEEPLY inside
        /// brightness so no texel-quantization wobble can flip it dim,
        /// while the diffuse irradiance stays mid-range (this pass has
        /// no tonemapper — a brighter fixture would saturate and hide
        /// the specular delta).
        fn half_space_image(width: u32, height: u32) -> Vec<f32> {
            let mut pixels = vec![0.0f32; (width * height * 4) as usize];
            for y in 0..height {
                for x in 0..width {
                    let u = (x as f32 + 0.5) / width as f32;
                    let v = (y as f32 + 0.5) / height as f32;
                    let d = crate::ibl::equirect_normal_cpu(u, v);
                    let base = ((y * width + x) * 4) as usize;
                    if d[0] > 0.0 {
                        pixels[base..base + 4].copy_from_slice(&[0.3, 0.3, 0.3, 1.0]);
                    } else {
                        pixels[base..base + 4].copy_from_slice(&[0.05, 0.05, 0.05, 1.0]);
                    }
                }
            }
            pixels
        }

        /// SPECULAR ADDS ENERGY (wave-5 v1 test c): a mirror-ish
        /// surface (rough 0, metal 1) whose reflection vector hits the
        /// bright half-space renders BRIGHTER than the same scene with
        /// specular-minimizing params (rough 1, metal 0).
        ///
        /// Setup: normal N=(√½, 0, √½) is sun-free (dot with +Y is 0)
        /// and, with the v1 uniform view (+Z eye), reflects to exactly
        /// +X — mid half-space — so the rough-0 prefilter returns the
        /// 0.3 radiance. The diffuse component is IDENTICAL across
        /// both renders (same env, same mesh — the spec params never
        /// touch the diffuse path), so the delta is pure specular.
        ///
        /// # The margin (derived, not fudged)
        ///
        /// Mirror: 0.3 × (0.72 × 0.985 + 0.015) ≈ 0.217 → measured
        /// +54 LSB (mirror=207, baseline=153). Baseline: dim prefilter
        /// (≈0.175) × (0.04 × 0.452 − 0.002) ≈ 0.003 → sub-LSB by
        /// construction, i.e. the baseline IS the diffuse-only render
        /// within rounding. Neither render saturates (mirror total
        /// ≈ 0.81). The ≥20 LSB bound is ~2.7× headroom under the
        /// measured 54 LSB structural delta.
        #[test]
        fn specular_adds_energy_mirror() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!("skipping specular_adds_energy_mirror: no wgpu adapter available");
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let s = std::f32::consts::FRAC_1_SQRT_2;
            let mesh = fullscreen_mesh([s, 0.0, s]);
            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");

            const W: u32 = 64;
            const H: u32 = 32;
            let pixels = half_space_image(W, H);
            let env = EnvIrradiance::from_equirect(
                &gpu.device,
                &gpu.queue,
                bytemuck::cast_slice(&pixels),
                W,
                H,
                EnvFormat::Rgba32Float,
            )
            .expect("convolve succeeds");
            buffers.set_environment(&gpu, Some(&env));

            buffers.set_spec_params(&gpu, SpecParams::new(0.0, 1.0));
            let mirror = render_mesh_offscreen(&gpu, &buffers);
            buffers.set_spec_params(&gpu, SpecParams::new(1.0, 0.0));
            let baseline = render_mesh_offscreen(&gpu, &buffers);

            // Every pixel shares the normal and the uniform view, so
            // all 64 agree — read the center one.
            let px = (4 * 8 + 4) * 4;
            let (m, b) = (&mirror[px..px + 4], &baseline[px..px + 4]);
            let delta = m[0] as i16 - b[0] as i16;
            assert!(
                delta >= 20,
                "mirror must beat diffuse-only by ≥20 LSB in red: mirror={m:?} baseline={b:?}"
            );
        }

        /// DIFFUSE REGRESSION (wave-5 v1 test d): the specular tier is
        /// purely additive under the map flag, so with `env_flags = 0`
        /// the diffuse term must be UNCHANGED at every rough/metal
        /// setting — each renders the closed-form procedural golden
        /// within the same ≤1-LSB unorm bound the pre-specular
        /// fallback test pins. (The env-flag-1 arm's diffuse path is
        /// textually untouched — the same `env` value flows into the
        /// same `color` expression, specular is a separate `+` — and
        /// the pre-existing IBL tests above pass without modification,
        /// which is the env arm's regression proof.)
        #[test]
        fn fallback_diffuse_unchanged_across_spec_params() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping fallback_diffuse_unchanged_across_spec_params: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = fullscreen_mesh([0.0, 0.0, 1.0]);
            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let want = expected_procedural_byte([0.0, 0.0, 1.0]);
            for (rough, metal) in [(0.0, 1.0), (1.0, 0.0), (0.0, 0.0), (1.0, 1.0), (0.5, 0.0)] {
                buffers.set_spec_params(&gpu, SpecParams::new(rough, metal));
                let bytes = render_mesh_offscreen(&gpu, &buffers);
                assert_eq!(bytes.len(), 8 * 8 * 4);
                for (i, px) in bytes.chunks_exact(4).enumerate() {
                    for c in 0..4 {
                        let diff = px[c].abs_diff(want[c]);
                        assert!(
                            diff <= 1,
                            "rough={rough} metal={metal} pixel {i} ch{c}: got {px:?}, want {want:?}"
                        );
                    }
                }
            }
        }

        // --- Core-loop kill 1 (the viewport shows paint) tests. Same
        // offscreen pattern as above, plus a real `PaintTarget` as the
        // bound tile: the paint engine's own surface, not a hand-made
        // texture — so the test proves the paint → viewport edge, not
        // just sampling in the abstract.

        /// Full-screen triangle with per-vertex uvs (the big-triangle
        /// trick + degenerate uvs): every fragment interpolates to `uv`,
        /// so with all three verts at a texel center the whole frame
        /// samples ONE texel — filtering- and orientation-independent.
        /// CCW like `fullscreen_mesh`, so backface culling keeps it.
        fn fullscreen_textured_mesh(uv: [f32; 2]) -> umber_mesh::MeshData {
            umber_mesh::MeshData {
                positions: vec![[-1.0, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]; 3],
                uvs: vec![uv; 3],
                indices: vec![0, 1, 2],
                material_names: vec![],
                triangle_materials: Vec::new(),
            }
        }

        /// A 2×2 paint tile through the real `PaintTarget` write path:
        /// row 0 (v 0) red/green, row 1 blue/white. All four corners use
        /// 0/255 channels only, so the sampled + identity-LUT bytes are
        /// bit-exact, never rounding-adjacent.
        fn paint_2x2(gpu: &GpuContext) -> crate::paint::PaintTarget {
            let target = crate::paint::PaintTarget::new(&gpu.device, 2, 2);
            let rgba: [u8; 16] = [
                255, 0, 0, 255, 0, 255, 0, 255, //
                0, 0, 255, 255, 255, 255, 255, 255,
            ];
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("umber_paint_test_upload_encoder"),
                });
            target
                .write_region(&gpu.device, &mut encoder, (0, 0), (2, 2), &rgba)
                .expect("2x2 region fits the 2x2 target");
            gpu.queue.submit(Some(encoder.finish()));
            target
        }

        /// [`render_mesh_mode`] at the family's usual test camera
        /// (identity projection, `NEG_Y` light).
        fn render_mesh_mode(
            gpu: &GpuContext,
            buffers: &MeshBuffers,
            mode: ViewMode,
            w: u32,
            h: u32,
        ) -> Vec<u8> {
            render_mesh_mode_with_camera(
                gpu,
                buffers,
                mode,
                w,
                h,
                CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::NEG_Y),
            )
        }

        /// Renders `buffers` in `mode` into a W×H linear target through
        /// the real mesh pipeline (the `MeshPaintCallback::draw` state:
        /// group 0 = the mesh's CURRENT bind group, group 1 = identity
        /// LUT) and returns tightly-packed RGBA8 bytes. Takes an
        /// explicit camera uniform — the height-perturbation tests need
        /// a light with x/y components (`NEG_Y` only reads `n.y`, which
        /// would hide a pure-x or pure-y perturbation; see
        /// `height_stroke_perturbs_the_lit_normal`).
        fn render_mesh_mode_with_camera(
            gpu: &GpuContext,
            buffers: &MeshBuffers,
            mode: ViewMode,
            w: u32,
            h: u32,
            uniform: CameraUniform,
        ) -> Vec<u8> {
            let device = &gpu.device;
            gpu.queue
                .write_buffer(&buffers.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
            gpu.queue.write_buffer(
                &buffers.view_mode_buffer,
                0,
                bytemuck::bytes_of(&mode.uniform()),
            );
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("umber_paint_viewport_test_target"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("umber_paint_viewport_test_encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("umber_paint_viewport_test_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&gpu.pipeline);
                pass.set_bind_group(0, &buffers.bind_group, &[]);
                pass.set_bind_group(1, gpu.identity_lut.bind_group(), &[]);
                pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
                pass.set_index_buffer(buffers.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..buffers.index_count, 0, 0..1);
            }
            gpu.queue.submit(Some(encoder.finish()));
            readback_packed(device, &gpu.queue, &target, w, h)
        }

        /// THE CAN-FAIL END-TO-END: the quad mesh with the 2×2 paint
        /// tile bound renders the painted texel colors in solo
        /// base-color — every pixel within 1 LSB of the written texel.
        /// A viewport that ignored the bound texture (constant gray)
        /// misses by ~70+ LSB; a broken tile select lands on the wrong
        /// texel (255 LSB). The ≤1 LSB bound admits only unorm rounding.
        #[test]
        fn paint_texture_sample_appears_in_solo_base_color() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping paint_texture_sample_appears_in_solo_base_color: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let target = paint_2x2(&gpu);
            let mode = ViewMode::Solo(crate::SoloChannel::BaseColor);
            // (texel-center uv, written texel bytes). Upload row 0 is v
            // 0 (the no-V-flip convention — see `PaintParams`), so
            // (0.25, 0.25) is red, (0.75, 0.75) white.
            for (uv, want) in [
                ([0.25, 0.25], [255u8, 0, 0, 255]),
                ([0.75, 0.25], [0u8, 255, 0, 255]),
                ([0.25, 0.75], [0u8, 0, 255, 255]),
                ([0.75, 0.75], [255u8, 255, 255, 255]),
            ] {
                let mesh = fullscreen_textured_mesh(uv);
                let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
                buffers.set_base_color_texture(&gpu, &target, PaintParams::identity());
                let bytes = render_mesh_mode(&gpu, &buffers, mode, 4, 4);
                assert_eq!(bytes.len(), 4 * 4 * 4);
                for (i, px) in bytes.chunks_exact(4).enumerate() {
                    for c in 0..4 {
                        let diff = px[c].abs_diff(want[c]);
                        assert!(
                            diff <= 1,
                            "uv {uv:?} pixel {i} ch{c}: got {px:?}, want {want:?}"
                        );
                    }
                }
            }
        }

        /// The unbound path in the color views: solo base-color AND
        /// unlit render the `BASE_COLOR` constant through the identity
        /// LUT — 0.72·255 = 183.6 → 184, 0.75·255 = 191.25 → 191.
        /// Pixel-pins the fallback the textured test above compares
        /// against (and the empty-mesh path the goldens pin).
        #[test]
        fn unbound_paint_renders_the_constant_in_color_views() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping unbound_paint_renders_the_constant_in_color_views: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let want = [184u8, 184, 191, 255];
            for mode in [
                ViewMode::Solo(crate::SoloChannel::BaseColor),
                ViewMode::Unlit,
            ] {
                let mesh = fullscreen_textured_mesh([0.25, 0.25]);
                let buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
                let bytes = render_mesh_mode(&gpu, &buffers, mode, 4, 4);
                assert_eq!(bytes.len(), 4 * 4 * 4);
                for (i, px) in bytes.chunks_exact(4).enumerate() {
                    for c in 0..4 {
                        let diff = px[c].abs_diff(want[c]);
                        assert!(
                            diff <= 1,
                            "{mode:?} pixel {i} ch{c}: got {px:?}, want {want:?}"
                        );
                    }
                }
            }
        }

        /// Paint feeds the lighting math, not just the solo proxy: the
        /// red texel bound in Lit renders near (96, 0, 0) — red × the
        /// +Z procedural env (t = 0.5, diffuse 0) — instead of the
        /// constant's (69, 75, 98). Both halves pinned: the textured
        /// bytes AND the unbound bytes they must differ from.
        #[test]
        fn lit_mode_multiplies_the_bound_texture_by_light() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping lit_mode_multiplies_the_bound_texture_by_light: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let target = paint_2x2(&gpu);
            let mesh = fullscreen_textured_mesh([0.25, 0.25]);
            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let plain = render_mesh_mode(&gpu, &buffers, ViewMode::Lit, 4, 4);
            let closed = expected_procedural_byte([0.0, 0.0, 1.0]);
            assert_eq!(closed, [69, 75, 98, 255]);
            for (i, px) in plain.chunks_exact(4).enumerate() {
                for c in 0..4 {
                    assert!(
                        px[c].abs_diff(closed[c]) <= 1,
                        "unbound lit pixel {i} ch{c}: got {px:?}, want {closed:?}"
                    );
                }
            }

            buffers.set_base_color_texture(&gpu, &target, PaintParams::identity());
            let bytes = render_mesh_mode(&gpu, &buffers, ViewMode::Lit, 4, 4);
            assert_eq!(bytes.len(), 4 * 4 * 4);
            for (i, px) in bytes.chunks_exact(4).enumerate() {
                // Red × env (0.375, 0.41, 0.51): 0.375·255 = 95.6 → 96.
                assert!(
                    px[0].abs_diff(96) <= 2,
                    "bound lit pixel {i}: red got {px:?}, want ≈96"
                );
                assert_eq!(
                    (px[1], px[2]),
                    (0, 0),
                    "bound lit pixel {i}: no green/blue may survive the red albedo: {px:?}"
                );
            }
        }

        /// Clearing restores the fallback byte-for-byte: bind → clear
        /// renders IDENTICAL bytes to a mesh that never bound paint (the
        /// `fallback_ignores_bound_map` pattern for the paint set — the
        /// flag flip is uniform control flow, so no sampling residue).
        #[test]
        fn clear_base_color_texture_restores_the_fallback() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping clear_base_color_texture_restores_the_fallback: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let target = paint_2x2(&gpu);
            let mode = ViewMode::Solo(crate::SoloChannel::BaseColor);
            let mesh = fullscreen_textured_mesh([0.75, 0.75]);
            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let plain = render_mesh_mode(&gpu, &buffers, mode, 4, 4);

            buffers.set_base_color_texture(&gpu, &target, PaintParams::identity());
            let textured = render_mesh_mode(&gpu, &buffers, mode, 4, 4);
            assert_ne!(
                plain, textured,
                "binding white over the constant must change the frame"
            );

            buffers.clear_base_color_texture(&gpu);
            let restored = render_mesh_mode(&gpu, &buffers, mode, 4, 4);
            assert_eq!(
                plain, restored,
                "clearing the paint must restore fallback bytes exactly"
            );
            assert_eq!(buffers.paint_params().flags, 0);
            assert_eq!(
                buffers.paint_params().uv_scale,
                [1.0, 1.0],
                "cleared params keep the identity transform"
            );
        }

        // --- Height→normal live (audit worklist #12) tests. ---

        /// A quad spanning NDC `[-1, 1]²`, uv = `pos.xy * 0.5 + 0.5` (u
        /// tracks world x, v tracks world y) — unlike
        /// `fullscreen_textured_mesh`'s degenerate uvs (one texel for
        /// the whole frame, by design), this mesh needs a REAL uv
        /// gradient across the screen for the height perturbation's
        /// finite differences and `dpdx`/`dpdy` to be nonzero. Same CCW
        /// winding convention as `fullscreen_textured_mesh` (first
        /// triangle's cross product is +Z, matching the vertex normal,
        /// so backface culling keeps both).
        fn textured_quad_mesh() -> umber_mesh::MeshData {
            umber_mesh::MeshData {
                positions: vec![
                    [-1.0, -1.0, 0.0],
                    [1.0, -1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [-1.0, 1.0, 0.0],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 4],
                uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                indices: vec![0, 1, 2, 0, 2, 3],
                material_names: vec![],
                triangle_materials: Vec::new(),
            }
        }

        /// A 4×4 height tile (R channel only; G/B/A unused) through the
        /// real `PaintTarget` write path: a linear ramp 0/85/170/255
        /// along ONE axis, constant along the other, so the per-texel
        /// finite difference is exactly 85/255 and the orthogonal one
        /// is exactly 0. `horizontal` ramps by column (a "+u" ramp — u
        /// tracks world x on `textured_quad_mesh`); `!horizontal` ramps
        /// by row (a "+v" ramp).
        fn height_ramp_target(gpu: &GpuContext, horizontal: bool) -> crate::paint::PaintTarget {
            let target = crate::paint::PaintTarget::new(&gpu.device, 4, 4);
            let steps: [u8; 4] = [0, 85, 170, 255];
            let mut rgba = [0u8; 4 * 4 * 4];
            for (i, texel) in rgba.chunks_exact_mut(4).enumerate() {
                let (row, col) = (i / 4, i % 4);
                texel[0] = if horizontal { steps[col] } else { steps[row] };
                texel[3] = 255;
            }
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("umber_height_ramp_test_upload_encoder"),
                });
            target
                .write_region(&gpu.device, &mut encoder, (0, 0), (4, 4), &rgba)
                .expect("4x4 region fits the 4x4 target");
            gpu.queue.submit(Some(encoder.finish()));
            target
        }

        /// Mirrors `shaders::MESH_SHADER`'s `height_perturbed_normal`
        /// (the Mikkelsen screen-space surface-gradient formula) so the
        /// golden test below pins bytes derived from the SAME
        /// arithmetic the GPU runs — not hand-expanded decimals (the
        /// `expected_procedural_byte` family's style, extended).
        /// `f32::signum` (never 0) stands in for WGSL's `sign` (which
        /// can return 0): the test's `det` is always nonzero, so the
        /// two never diverge here.
        fn mikkelsen_perturbed_normal(
            n: glam::Vec3,
            dpos_dx: glam::Vec3,
            dpos_dy: glam::Vec3,
            dhdx: f32,
            dhdy: f32,
        ) -> glam::Vec3 {
            let r1 = dpos_dy.cross(n);
            let r2 = n.cross(dpos_dx);
            let det = dpos_dx.dot(r1);
            let surf_grad = det.signum() * (dhdx * r1 + dhdy * r2);
            (det.abs() * n - surf_grad).normalize()
        }

        /// Closed-form expected byte for a height-perturbed normal
        /// under a light with x, y, AND z components — unlike the
        /// family's usual `NEG_Y` (`to_light = +Y`, which reads only
        /// `n.y` and would hide a pure-x, "+u ramp" perturbation
        /// entirely). `to_light = normalize(1, 1, 1)`.
        ///
        /// `dhdx`/`dhdy` are the already-strength-scaled screen-space
        /// height derivatives (see `height_stroke_perturbs_the_lit_normal`'s
        /// doc for how they're derived from the bound ramp); `(0.0,
        /// 0.0)` is the unperturbed/flat case.
        fn expected_height_perturbed_byte(dhdx: f32, dhdy: f32) -> [u8; 4] {
            const GROUND: [f32; 3] = [0.30, 0.27, 0.24];
            const SKY: [f32; 3] = [0.45, 0.55, 0.78];
            const BASE: [f32; 3] = [0.72, 0.72, 0.75];
            const SUN: [f32; 3] = [1.0, 0.96, 0.90];
            // Identity view_proj: world_pos IS the vertex position, so
            // its screen-space derivative is the per-pixel NDC step
            // (2/W, -2/H — the y flip is wgpu's NDC-to-framebuffer
            // convention) at W=H=4.
            let n0 = glam::Vec3::new(0.0, 0.0, 1.0);
            let dpos_dx = glam::Vec3::new(0.5, 0.0, 0.0);
            let dpos_dy = glam::Vec3::new(0.0, -0.5, 0.0);
            let n = mikkelsen_perturbed_normal(n0, dpos_dx, dpos_dy, dhdx, dhdy);
            let to_light = glam::Vec3::new(1.0, 1.0, 1.0).normalize();
            let diffuse = n.dot(to_light).max(0.0);
            let t = (n.y * 0.5 + 0.5).clamp(0.0, 1.0);
            let env = [
                GROUND[0] + (SKY[0] - GROUND[0]) * t,
                GROUND[1] + (SKY[1] - GROUND[1]) * t,
                GROUND[2] + (SKY[2] - GROUND[2]) * t,
            ];
            let px = [
                BASE[0] * (env[0] + diffuse * SUN[0] * 0.55),
                BASE[1] * (env[1] + diffuse * SUN[1] * 0.55),
                BASE[2] * (env[2] + diffuse * SUN[2] * 0.55),
            ];
            [
                (px[0] * 255.0 + 0.5).floor() as u8,
                (px[1] * 255.0 + 0.5).floor() as u8,
                (px[2] * 255.0 + 0.5).floor() as u8,
                255,
            ]
        }

        /// THE CAN-FAIL END-TO-END for the height→normal live kill
        /// (audit worklist #12, gate: "height stroke → lit viewport
        /// normal response, golden-image pinned"): a known height ramp
        /// bound on `textured_quad_mesh` perturbs the Lit normal, and
        /// the response differs from the flat baseline AND between a
        /// +u-only and a +v-only ramp — a viewport that ignored the
        /// bound height, or perturbed along the wrong axis, collapses
        /// these to the same bytes.
        ///
        /// # Derivation (pixel: row 1, col 1 of the 4×4 readback)
        ///
        /// At W=H=4, pixel (col=1, row=1)'s uv is (0.375, 0.625) —
        /// which lands at the CENTER of height-texture texel (col=1,
        /// row=2) (the height texture is also 4×4, so every readback
        /// pixel's uv lands exactly on its matching texel's center —
        /// no filtering ambiguity). `duv/dscreen` is (0.25, 0) in x,
        /// (0, -0.25) in y (constant across this planar quad).
        ///
        /// A "+u" ramp (`height_ramp_target(horizontal: true)`) is
        /// column-only: at texel (1, 2), h0 = 85/255, h(u+1) = 170/255,
        /// h(v+1) = 85/255 (row-independent, same as h0) → dhdv = 0.
        /// At strength 0.75: dhdu = (170-85)/255 * 4 * 0.75 = 1.0
        /// exactly, so dhdx = dhdu*0.25 = 0.25, dhdy = -dhdv*0.25 = 0.
        ///
        /// A "+v" ramp is row-only: at texel (1, 2), h0 = 170/255,
        /// h(u+1) = 170/255 (column-independent, same as h0) → dhdu =
        /// 0; h(v+1) = 255/255, so dhdv = (255-170)/255*4*0.75 = 1.0,
        /// dhdy = -0.25, dhdx = 0.
        #[test]
        fn height_stroke_perturbs_the_lit_normal() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping height_stroke_perturbs_the_lit_normal: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = textured_quad_mesh();
            let camera =
                CameraUniform::new(glam::Mat4::IDENTITY, glam::Vec3::new(-1.0, -1.0, -1.0));
            // Pixel (row=1, col=1) → byte offset (1*4 + 1) * 4 = 20.
            const OFFSET: usize = 20;

            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let flat = render_mesh_mode_with_camera(&gpu, &buffers, ViewMode::Lit, 4, 4, camera);
            let flat_expected = expected_height_perturbed_byte(0.0, 0.0);
            let flat_px = &flat[OFFSET..OFFSET + 4];
            for c in 0..4 {
                assert!(
                    flat_px[c].abs_diff(flat_expected[c]) <= 1,
                    "flat pixel ch{c}: got {flat_px:?}, want {flat_expected:?}"
                );
            }

            let strength = 0.75;
            let u_ramp = height_ramp_target(&gpu, true);
            buffers.set_height_texture(&gpu, &u_ramp, PaintParams::height(strength));
            let u_bytes = render_mesh_mode_with_camera(&gpu, &buffers, ViewMode::Lit, 4, 4, camera);
            let u_expected = expected_height_perturbed_byte(0.25, 0.0);
            let u_px = &u_bytes[OFFSET..OFFSET + 4];
            for c in 0..4 {
                assert!(
                    u_px[c].abs_diff(u_expected[c]) <= 1,
                    "+u ramp pixel ch{c}: got {u_px:?}, want {u_expected:?}"
                );
            }

            let v_ramp = height_ramp_target(&gpu, false);
            buffers.set_height_texture(&gpu, &v_ramp, PaintParams::height(strength));
            let v_bytes = render_mesh_mode_with_camera(&gpu, &buffers, ViewMode::Lit, 4, 4, camera);
            let v_expected = expected_height_perturbed_byte(0.0, -0.25);
            let v_px = &v_bytes[OFFSET..OFFSET + 4];
            for c in 0..4 {
                assert!(
                    v_px[c].abs_diff(v_expected[c]) <= 1,
                    "+v ramp pixel ch{c}: got {v_px:?}, want {v_expected:?}"
                );
            }

            assert_ne!(
                flat_px, u_px,
                "a bound +u height ramp must change the flat baseline"
            );
            assert_ne!(
                flat_px, v_px,
                "a bound +v height ramp must change the flat baseline"
            );
            assert_ne!(u_px, v_px, "a +u ramp must differ from a +v ramp");
        }

        /// FALLBACK REGRESSION for the height kill: binding then
        /// clearing a height texture renders byte-identical to a mesh
        /// that never touched the height API — mirrors
        /// `clear_base_color_texture_restores_the_fallback`'s pattern
        /// for `PaintParams::HAS_HEIGHT_TEXTURE`. Uses the family's
        /// usual `NEG_Y`-light helpers (no need for the custom camera
        /// above — this test only needs byte equality, not a nonzero
        /// response).
        #[test]
        fn height_texture_flag_off_matches_unbound_baseline() {
            let Some((adapter, device, queue)) = try_request_device() else {
                eprintln!(
                    "skipping height_texture_flag_off_matches_unbound_baseline: no wgpu adapter available"
                );
                return;
            };
            let gpu = GpuContext::new(
                adapter,
                device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
                None,
            );
            let mesh = fullscreen_textured_mesh([0.25, 0.25]);
            let never_touched = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let baseline = render_mesh_mode(&gpu, &never_touched, ViewMode::Lit, 4, 4);

            let mut buffers = MeshBuffers::upload(&gpu, &mesh).expect("upload succeeds");
            let height = height_ramp_target(&gpu, true);
            buffers.set_height_texture(&gpu, &height, PaintParams::height(1.0));
            buffers.clear_height_texture(&gpu);
            let cleared = render_mesh_mode(&gpu, &buffers, ViewMode::Lit, 4, 4);

            assert_eq!(
                baseline, cleared,
                "clearing the height texture must restore fallback bytes exactly"
            );
        }
    }
}
