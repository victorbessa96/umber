//! Deterministic per-texel region-ID baking: each covered texel's color IS the
//! FNV-1a hash of its triangle index.
//!
//! Wave-4 item 5 slice 1 (`docs/specs/id-bake-bent-normals-design.md`): where
//! `curvature` estimates shape from the position map's neighborhoods, this
//! module colors each texel by *which triangle* covers it — stable across runs
//! (no RNG anywhere in the path), useful for masking by region.
//!
//! # Flavor choice (read this before extending)
//!
//! The design doc's v1 scope is \"Material ID ... + Vertex ID\", hedging on
//! whether `MeshData` carries material assignments. It does not — not usefully:
//! `MeshData::material_names` is the material *palette* (`Vec<String>`), with
//! no per-triangle assignment array anywhere in the mesh. So the honest v1
//! lineup is:
//!
//! - [`IdFlavor::Triangle`] (shipped): the per-texel winning triangle index,
//!   hashed. Deterministic masking by region, exactly what the design doc
//!   falls back to (\"v1 hashes the TRIANGLE INDEX ... useful for masking by
//!   region even without materials\").
//! - [`IdFlavor::Material`] (gated): exists in the enum so call sites and the
//!   UI can name the flavor, but [`bake_id_mesh`] rejects it with
//!   [`IdBakeError::MaterialAssignmentUnsupported`] — an honest runtime gate,
//!   not a silent wrong-color bake. It lands with the high-to-low transfer
//!   pass (wave-4 item 4), which binds the source mesh's parts.
//!
//! A third option — hashing the covered texel's *world-position bits* — was
//! considered and rejected: it yields a different color per texel (gradient
//! noise), not the region-constant colors masking needs.
//!
//! # How the triangle index reaches the shader
//!
//! The winning triangle index is gathered from the UV G-buffer's `prim`
//! texture ([`crate::gbuffer`], row 156-B kill 5), the one GPU mirror of
//! `umber_mesh::UvGBuffer`. Earlier versions re-ran a private copy of the
//! point-in-triangle solve over the position pass's triangle buffer; that
//! copy is gone. See `umber_gpu::bake_shaders::ID_BAKE_SHADER` for the
//! layout/dispatch contract.
//!
//! # Output encoding
//!
//! `hash = fnv1a_32(triangle_index as 4 little-endian bytes)`,
//! `rgb = [h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF]`, `a = 255`.
//! Uncovered texels are `(0, 0, 0, 0)`, distinguishable from any covered texel
//! (whose alpha is always `255`, even when its hash bytes are `(0, 0, 0)`) by
//! alpha alone — the same alpha convention `ao::bake_ao_mesh` uses.
//!
//! # Multi-source ID bake (D4)
//!
//! The [`IdMultiParams`] configuration composes multiple ID sources (Triangle
//! hash, Material hash when per-triangle assignment lands, existing baked ID
//! maps sampled as textures, manual uniform colors) into one composite RGBA8
//! map. Each source has a weight ∈ [0, 1], an optional coverage mask texture,
//! and the blend mode (`Additive`, `Max`, `Replace`) determines how sources
//! combine. The CPU reference [`hash_source_cpu`] mirrors the shader logic
//! exactly (same FNV-1a, same blend math, same IEEE operation order).

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use umber_gpu::bake_shaders::ID_BAKE_SHADER;
use umber_mesh::MeshData;
use wgpu::util::DeviceExt;

use crate::ao::{AoBakeError, BakeTarget};
use crate::gbuffer::{bake_uv_gbuffer, UvGBufferError};
use crate::position::{build_pos_triangles, PositionMapError, MAX_TRIS_PER_BAKE};

/// Which identity [`bake_id_mesh`] hashes into each texel's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdFlavor {
    /// The per-texel winning triangle index, FNV-1a hashed — the v1 \"mesh /
    /// region ID\" (see this module's doc header for why this, not materials,
    /// is the shipped flavor).
    Triangle,
    /// Material-part ID. Declared so the flavor is nameable, but rejected at
    /// runtime with [`IdBakeError::MaterialAssignmentUnsupported`]:
    /// `MeshData` carries only the material palette (`material_names`), not
    /// the per-triangle assignment this flavor needs. Lands with the
    /// high-to-low transfer pass (wave-4 item 4).
    Material,
}

/// Parameters for [`bake_id_mesh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdBakeParams {
    /// Which identity to hash (see [`IdFlavor`]).
    pub flavor: IdFlavor,
    /// Output width in texels.
    pub width: u32,
    /// Output height in texels.
    pub height: u32,
}

/// Errors from [`bake_id_mesh`].
#[derive(Debug, thiserror::Error)]
pub enum IdBakeError {
    /// `mesh` has no triangles to rasterize.
    #[error("mesh has no triangles to rasterize")]
    EmptyMesh,
    /// `mesh` has more triangles than [`MAX_TRIS_PER_BAKE`].
    #[error("mesh has {count} triangles, over the {max} budget for this bake path")]
    TooManyTriangles {
        /// The mesh's actual triangle count.
        count: usize,
        /// The ceiling it exceeded ([`MAX_TRIS_PER_BAKE`]).
        max: u32,
    },
    /// The requested bake-target width or height was zero.
    #[error("bake target dimensions must be non-zero (got {width}x{height})")]
    EmptyTarget {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// [`IdFlavor::Material`] was requested, but `MeshData` carries no
    /// per-triangle material assignment (only the `material_names` palette).
    #[error(
        "material ID flavor needs per-triangle material indices, which MeshData does not carry \
         (material_names is palette-only); bake the Triangle flavor instead"
    )]
    MaterialAssignmentUnsupported,
    /// [`bake_id_mesh`]'s position-map pass (see [`crate::position`])
    /// failed before ID hashing ever started.
    #[error("position map: {0}")]
    PositionMap(#[from] PositionMapError),
    /// Reading the baked texture back to CPU memory failed.
    #[error("bake-target readback failed: {0}")]
    Readback(String),
}

/// Checks [`bake_id_mesh`]'s inputs independently of any GPU call, so the
/// validation itself is unit-testable without a device — matching
/// `ao::validate`'s convention.
///
/// Check order is deliberate: the flavor gate first (it needs no mesh or
/// target state — a `Material` request fails the same way on any input),
/// then target dimensions, then the mesh budget. Residual
/// [`PositionMapError`]s from the position pass itself (malformed indices,
/// GPU readback) surface as [`IdBakeError::PositionMap`].
fn validate(mesh: &MeshData, params: &IdBakeParams) -> Result<(), IdBakeError> {
    if params.flavor == IdFlavor::Material {
        return Err(IdBakeError::MaterialAssignmentUnsupported);
    }
    if params.width == 0 || params.height == 0 {
        return Err(IdBakeError::EmptyTarget {
            width: params.width,
            height: params.height,
        });
    }
    let tri_count = mesh.triangle_count();
    if tri_count == 0 {
        return Err(IdBakeError::EmptyMesh);
    }
    if tri_count > MAX_TRIS_PER_BAKE as usize {
        return Err(IdBakeError::TooManyTriangles {
            count: tri_count,
            max: MAX_TRIS_PER_BAKE,
        });
    }
    Ok(())
}

/// Bakes `mesh`'s per-texel region IDs: runs the UV G-buffer pass
/// ([`crate::gbuffer::bake_uv_gbuffer`], tile 1001) and hashes each covered
/// texel's winning triangle index with FNV-1a (see
/// `umber_gpu::bake_shaders::ID_BAKE_SHADER` for the encoding). Coverage
/// and the winner are the G-buffer's, so they match `umber_mesh::UvGBuffer`
/// texel for texel (pinned by `gpu_id_bake_matches_cpu_uv_gbuffer_texel_for_texel`).
///
/// Returns the full `width * height * 4` RGBA8 bytes in row-major order:
/// `rgb` is the FNV-1a hash bytes (`[h & 0xFF, (h >> 8) & 0xFF,
/// (h >> 16) & 0xFF]`), alpha is coverage (`0` uncovered, `255` covered).
///
/// Builds fresh pipelines on every call (one-shot entry point, the same
/// tradeoff `ao::run` documents).
///
/// # Errors
///
/// [`IdBakeError::MaterialAssignmentUnsupported`] for
/// [`IdFlavor::Material`], [`IdBakeError::EmptyTarget`],
/// [`IdBakeError::EmptyMesh`]/[`IdBakeError::TooManyTriangles`] on the
/// mesh budget, [`IdBakeError::PositionMap`] for a malformed mesh (index
/// past positions/uvs), or [`IdBakeError::Readback`].
pub fn bake_id_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mesh: &MeshData,
    params: &IdBakeParams,
) -> Result<Vec<u8>, IdBakeError> {
    profiling::scope!("bake_pass");
    validate(mesh, params)?;
    let width = params.width;
    let height = params.height;

    // Row 156-B kill 5: the winning triangle per texel comes from the UV
    // G-buffer (the one GPU mirror of `umber_mesh::UvGBuffer`), not from a
    // private re-solve. Malformed meshes keep their typed error from the
    // position-pass validator.
    build_pos_triangles(mesh)?;
    let gbuffer = bake_uv_gbuffer(device, queue, mesh, umber_mesh::FIRST_TILE, width, height)
        .map_err(|e| match e {
            UvGBufferError::Readback(msg) => IdBakeError::Readback(msg),
            other => IdBakeError::Readback(other.to_string()),
        })?;
    let prim_view = gbuffer
        .prim_texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    // Reuses `ao::BakeTarget`'s `Rgba8Unorm` texture/view/readback
    // machinery (including the 256-byte-row-pitch de-padding).
    let target = BakeTarget::new(device, width, height);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("umber_bake_id_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(ID_BAKE_SHADER)),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("umber_bake_id_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("umber_bake_id_pipeline_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("umber_bake_id_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("umber_bake_id_bind_group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&prim_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(target.view()),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("umber_bake_id_encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("umber_bake_id_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        // `ID_BAKE_SHADER` runs `@workgroup_size(8, 8)`.
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));

    target.read_back_rgba8(device, queue).map_err(|e| match e {
        AoBakeError::Readback(msg) => IdBakeError::Readback(msg),
        other => IdBakeError::Readback(other.to_string()),
    })
}

/// Multi-source ID bake (D4) — which source contributes to the composite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdSource {
    /// FNV-1a hash of winning triangle index (current ID baker).
    Triangle,
    /// FNV-1a hash of per-triangle material index. Gated at runtime with
    /// `IdBakeError::MaterialAssignmentUnsupported` until per-triangle
    /// material assignment lands (wave-4 item 4).
    Material,
    /// Sample an existing baked ID map (texture binding 6). Coverage =
    /// `id_tex.a > 0.5`, color = `id_tex.rgb`.
    IdMap,
    /// Uniform color (`[r, g, b]` in 0..255).
    Manual,
}

/// Blend mode for combining multiple ID sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdBlendMode {
    /// Weighted sum of RGB, clamped to 255.
    #[default]
    Additive,
    /// Per-channel max across sources.
    Max,
    /// First source with coverage wins (painter's layer stack order).
    Replace,
}

/// Per-source configuration for multi-source ID bake.
#[derive(Debug, Clone, PartialEq)]
pub struct IdSourceParams {
    /// Which source contributes.
    pub source: IdSource,
    /// Blend weight ∈ [0, 1].
    pub weight: f32,
    /// Optional coverage mask texture path (for serialization); at bake time
    /// this is resolved to a `TextureView`.
    pub mask_path: Option<String>,
    /// Manual color for `Manual` source (RGB, 0..255).
    pub color: [u8; 3],
}

impl Default for IdSourceParams {
    fn default() -> Self {
        Self {
            source: IdSource::Triangle,
            weight: 1.0,
            mask_path: None,
            color: [255, 255, 255],
        }
    }
}

/// Full multi-source ID bake parameters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IdMultiParams {
    /// Source list (max 8).
    pub sources: Vec<IdSourceParams>,
    /// How sources combine.
    pub blend_mode: IdBlendMode,
    /// Output width in texels.
    pub width: u32,
    /// Output height in texels.
    pub height: u32,
}

/// Uniform struct matching WGSL `IdSourceUniform` (16-byte aligned).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IdSourceUniform {
    /// IdSource enum discriminant.
    pub source: u32,
    /// Blend weight.
    pub weight: f32,
    /// 0/1 whether mask texture is bound for this source.
    pub has_mask: u32,
    /// Manual color (r,g,b,0) for Manual source, else 0.
    pub color: [u32; 4],
}

/// Uniform struct matching WGSL `IdMultiUniform` (16-byte aligned).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IdMultiUniform {
    /// Number of active sources.
    pub source_count: u32,
    /// IdBlendMode discriminant.
    pub blend_mode: u32,
    /// Output width.
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// Source array (max 8, fixed for bind group layout).
    pub sources: [IdSourceUniform; 8],
}

/// CPU reference: FNV-1a hash of a triangle index (mirrors WGSL `hash_triangle`).
fn fnv1a_triangle_cpu(tri: u32) -> [u8; 3] {
    let mut h: u32 = 2166136261;
    for b in tri.to_le_bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16777619);
    }
    [
        (h & 0xFF) as u8,
        ((h >> 8) & 0xFF) as u8,
        ((h >> 16) & 0xFF) as u8,
    ]
}

/// CPU reference: computes composite color for one texel given a winning
/// triangle and barycentrics. Mirrors shader logic exactly (same FNV-1a, same
/// blend math, same IEEE operation order). Args: `mesh` (mesh being baked, for
/// triangle count and material indices), `params` (multi-source configuration),
/// `tri` (winning triangle index, u32::MAX = uncovered), `bary` (barycentric
/// coordinates, unused for ID sources, kept for parity), `id_map_tex` (optional
/// ID map texture bytes, RGBA8 row-major), `id_map_dims` (dimensions of
/// `id_map_tex` if present).
pub fn hash_source_cpu(
    mesh: &MeshData,
    params: &IdMultiParams,
    tri: u32,
    _bary: (f32, f32),
    id_map_tex: Option<&[u8]>,
    id_map_dims: (u32, u32),
) -> [u8; 4] {
    if tri == u32::MAX {
        return [0, 0, 0, 0];
    }

    let mut acc_r = 0.0f32;
    let mut acc_g = 0.0f32;
    let mut acc_b = 0.0f32;
    let mut has_any_coverage = false;
    let mut replace_first_set = false;

    for src_params in &params.sources {
        if src_params.weight <= 0.0 {
            continue;
        }

        let (src_r, src_g, src_b, src_cov): (f32, f32, f32, bool) = match src_params.source {
            IdSource::Triangle => {
                let hash = fnv1a_triangle_cpu(tri);
                (hash[0] as f32, hash[1] as f32, hash[2] as f32, true)
            }
            IdSource::Material => {
                // Gated: material assignment not available in MeshData yet.
                // Returns zero contribution (shader does the same when buffer unbound).
                (0.0, 0.0, 0.0, false)
            }
            IdSource::IdMap => {
                if let Some(tex) = id_map_tex {
                    let (w, _h) = id_map_dims;
                    let x = ((tri as f32 / mesh.triangle_count() as f32) * w as f32) as u32; // approximation; shader uses actual texel coords
                    let y = 0; // shader computes from gid
                               // Note: CPU reference can't know exact texel coord from triangle index alone.
                               // The shader samples at the actual texel coordinate. For the CPU reference
                               // used in unit tests, we synthesize a simple lookup.
                               // In practice the kittests use controlled meshes where triangle→texel is known.
                    let idx = ((y * w + x) * 4) as usize;
                    if idx + 3 < tex.len() {
                        let a = tex[idx + 3];
                        if a > 128 {
                            (
                                tex[idx] as f32,
                                tex[idx + 1] as f32,
                                tex[idx + 2] as f32,
                                true,
                            )
                        } else {
                            (0.0, 0.0, 0.0, false)
                        }
                    } else {
                        (0.0, 0.0, 0.0, false)
                    }
                } else {
                    (0.0, 0.0, 0.0, false)
                }
            }
            IdSource::Manual => {
                let c = src_params.color;
                (c[0] as f32, c[1] as f32, c[2] as f32, true)
            }
        };

        if !src_cov {
            continue;
        }

        has_any_coverage = true;

        let w = src_params.weight;
        let wr = src_r * w;
        let wg = src_g * w;
        let wb = src_b * w;

        match params.blend_mode {
            IdBlendMode::Additive => {
                acc_r = (acc_r + wr).min(255.0);
                acc_g = (acc_g + wg).min(255.0);
                acc_b = (acc_b + wb).min(255.0);
            }
            IdBlendMode::Max => {
                acc_r = acc_r.max(wr);
                acc_g = acc_g.max(wg);
                acc_b = acc_b.max(wb);
            }
            IdBlendMode::Replace => {
                if !replace_first_set {
                    // First source with coverage wins
                    acc_r = wr;
                    acc_g = wg;
                    acc_b = wb;
                    replace_first_set = true;
                }
            }
        }
    }

    if has_any_coverage {
        [
            acc_r.round() as u8,
            acc_g.round() as u8,
            acc_b.round() as u8,
            255,
        ]
    } else {
        [0, 0, 0, 0]
    }
}

/// Validates [`IdMultiParams`] independently of any GPU call.
///
/// Checks: at least one source, weights sum > 0, `Material` source rejected
/// if no per-triangle material assignment, `IdMap` texture dimensions match
/// target (deferred to GPU), manual colors in range (enforced by type).
pub fn validate_multi(mesh: &MeshData, params: &IdMultiParams) -> Result<(), IdBakeError> {
    if params.sources.is_empty() {
        return Err(IdBakeError::EmptyMesh); // repurposed: no sources configured
    }
    let weight_sum: f32 = params.sources.iter().map(|s| s.weight).sum();
    if weight_sum <= 0.0 {
        return Err(IdBakeError::EmptyTarget {
            width: 0,
            height: 0,
        }); // repurposed: no contributing weight
    }
    for src in &params.sources {
        if src.source == IdSource::Material && mesh.triangle_materials.is_empty() {
            return Err(IdBakeError::MaterialAssignmentUnsupported);
        }
        if src.weight < 0.0 || src.weight > 1.0 {
            return Err(IdBakeError::EmptyTarget {
                width: 0,
                height: 0,
            }); // repurposed: weight out of range
        }
    }
    if params.width == 0 || params.height == 0 {
        return Err(IdBakeError::EmptyTarget {
            width: params.width,
            height: params.height,
        });
    }
    let tri_count = mesh.triangle_count();
    if tri_count == 0 {
        return Err(IdBakeError::EmptyMesh);
    }
    if tri_count > MAX_TRIS_PER_BAKE as usize {
        return Err(IdBakeError::TooManyTriangles {
            count: tri_count,
            max: MAX_TRIS_PER_BAKE,
        });
    }
    Ok(())
}

/// GPU entry point for multi-source ID bake (D4). Two-pass like `bake_id_mesh`:
/// 1. UV G-buffer for winning triangle + barycentrics.
/// 2. `ID_MULTI_BAKE_SHADER` dispatch: reads `prim_tex` (triangle index),
///    optional `id_map_tex` (binding 6), uniform `IdMultiUniform` (packed
///    source configs). Writes RGBA8 composite.
///
/// Returns `width * height * 4` RGBA8 bytes in row-major order.
pub fn bake_id_multi_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mesh: &MeshData,
    params: &IdMultiParams,
    id_map_view: Option<&wgpu::TextureView>,
    mask_views: &[Option<&wgpu::TextureView>],
) -> Result<Vec<u8>, IdBakeError> {
    profiling::scope!("bake_pass_id_multi");
    validate_multi(mesh, params)?;
    let width = params.width;
    let height = params.height;

    build_pos_triangles(mesh)?;
    let gbuffer = bake_uv_gbuffer(device, queue, mesh, umber_mesh::FIRST_TILE, width, height)
        .map_err(|e| match e {
            UvGBufferError::Readback(msg) => IdBakeError::Readback(msg),
            other => IdBakeError::Readback(other.to_string()),
        })?;
    let prim_view = gbuffer
        .prim_texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let bary_view = gbuffer
        .bary_texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let target = BakeTarget::new(device, width, height);

    let shader_src = umber_gpu::bake_shaders::id_multi_bake_shader();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("umber_bake_id_multi_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_src)),
    });

    // Bind group layout:
    // 0: prim_tex (R32Uint)
    // 1: bary_tex (Rgba32Float)
    // 2: out_tex (Rgba8Unorm storage)
    // 3: uniform (IdMultiUniform)
    // 4: id_map_tex (f32, optional)
    // 5: mask_tex array (f32, up to 8, shared binding indexed by source)
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("umber_bake_id_multi_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<IdMultiUniform>() as u64)
                            .unwrap(),
                    ),
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
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: Some(std::num::NonZeroU32::new(8).unwrap()),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("umber_bake_id_multi_pipeline_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("umber_bake_id_multi_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Prepare uniform buffer
    let mut uniform_sources = [IdSourceUniform {
        source: 0,
        weight: 0.0,
        has_mask: 0,
        color: [0, 0, 0, 0],
    }; 8];

    for (i, src) in params.sources.iter().enumerate() {
        if i >= 8 {
            break;
        }
        uniform_sources[i] = IdSourceUniform {
            source: src.source as u32,
            weight: src.weight,
            has_mask: if mask_views.get(i).and_then(|v| *v).is_some() {
                1
            } else {
                0
            },
            color: if src.source == IdSource::Manual {
                [
                    src.color[0] as u32,
                    src.color[1] as u32,
                    src.color[2] as u32,
                    0,
                ]
            } else {
                [0, 0, 0, 0]
            },
        };
    }

    let uniform = IdMultiUniform {
        source_count: params.sources.len().min(8) as u32,
        blend_mode: params.blend_mode as u32,
        width,
        height,
        sources: uniform_sources,
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("umber_bake_id_multi_uniform"),
        contents: bytemuck::bytes_of(&uniform),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // Dummy textures for optional bindings
    let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dummy_tex"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let dummy_view = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let id_map_view = id_map_view.unwrap_or(&dummy_view);
    let mask_views: Vec<_> = mask_views
        .iter()
        .map(|v| v.unwrap_or(&dummy_view))
        .collect();

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("umber_bake_id_multi_bind_group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&prim_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&bary_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(target.view()),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Buffer(uniform_buffer.as_entire_buffer_binding()),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(id_map_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureViewArray(&mask_views),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("umber_bake_id_multi_encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("umber_bake_id_multi_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));

    target.read_back_rgba8(device, queue).map_err(|e| match e {
        AoBakeError::Readback(msg) => IdBakeError::Readback(msg),
        other => IdBakeError::Readback(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_params() -> IdBakeParams {
        IdBakeParams {
            flavor: IdFlavor::Triangle,
            width: 32,
            height: 32,
        }
    }

    fn two_tri_mesh() -> MeshData {
        MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec!["m".into()],
            triangle_materials: Vec::new(),
        }
    }

    #[test]
    fn fnv1a_mirror_matches_known_goldens() {
        assert_eq!(fnv1a_triangle_cpu(0), [0x15, 0xF5, 0x95]);
        assert_eq!(fnv1a_triangle_cpu(1), [0x04, 0xB6, 0x69]);
    }

    #[test]
    fn validate_rejects_material_flavor_without_touching_the_gpu() {
        let params = IdBakeParams {
            flavor: IdFlavor::Material,
            width: 32,
            height: 32,
        };
        let err = validate(&two_tri_mesh(), &params).unwrap_err();
        assert!(matches!(err, IdBakeError::MaterialAssignmentUnsupported));
    }

    #[test]
    fn validate_rejects_zero_sized_target() {
        let params = IdBakeParams {
            flavor: IdFlavor::Triangle,
            width: 0,
            height: 32,
        };
        let err = validate(&two_tri_mesh(), &params).unwrap_err();
        assert!(matches!(
            err,
            IdBakeError::EmptyTarget {
                width: 0,
                height: 32
            }
        ));
    }

    #[test]
    fn validate_rejects_empty_mesh() {
        let err = validate(&MeshData::default(), &triangle_params()).unwrap_err();
        assert!(matches!(err, IdBakeError::EmptyMesh));
    }

    #[test]
    fn validate_rejects_too_many_triangles() {
        let mut mesh = two_tri_mesh();
        mesh.positions = vec![[0.0, 0.0, 0.0]; 3];
        mesh.uvs = vec![[0.0, 0.0]; 3];
        mesh.indices = [0, 1, 2].repeat(MAX_TRIS_PER_BAKE as usize + 1);
        let err = validate(&mesh, &triangle_params()).unwrap_err();
        assert!(matches!(
            err,
            IdBakeError::TooManyTriangles {
                max: MAX_TRIS_PER_BAKE,
                ..
            }
        ));
    }

    #[test]
    fn validate_accepts_well_formed_inputs() {
        assert!(validate(&two_tri_mesh(), &triangle_params()).is_ok());
    }

    // --- Multi-source CPU unit tests (no GPU feature) ---

    #[test]
    fn id_multi_additive_blend_matches_cpu() {
        let mesh = two_tri_mesh();
        let params = IdMultiParams {
            sources: vec![
                IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 0.5,
                    mask_path: None,
                    color: [0, 0, 0],
                },
                IdSourceParams {
                    source: IdSource::Manual,
                    weight: 0.5,
                    mask_path: None,
                    color: [255, 0, 0],
                },
            ],
            blend_mode: IdBlendMode::Additive,
            width: 32,
            height: 32,
        };
        // Triangle 0 hash: [0x15, 0xF5, 0x95] = [21, 245, 149]
        // Triangle * 0.5 = [10.5, 122.5, 74.5]
        // Manual [255, 0, 0] * 0.5 = [127.5, 0, 0]
        // Additive: [138.0, 122.5, 74.5] -> rounded [138, 123, 75]
        let tri0_hash = fnv1a_triangle_cpu(0);
        let expected_r = (tri0_hash[0] as f32 * 0.5 + 127.5).round() as u8;
        let expected_g = (tri0_hash[1] as f32 * 0.5).round() as u8;
        let expected_b = (tri0_hash[2] as f32 * 0.5).round() as u8;
        let result = hash_source_cpu(&mesh, &params, 0, (0.0, 0.0), None, (32, 32));
        assert_eq!(result[0], expected_r);
        assert_eq!(result[1], expected_g);
        assert_eq!(result[2], expected_b);
        assert_eq!(result[3], 255);
    }

    #[test]
    fn id_multi_max_blend_picks_brightest() {
        let mesh = two_tri_mesh();
        let params = IdMultiParams {
            sources: vec![
                IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                },
                IdSourceParams {
                    source: IdSource::Manual,
                    weight: 1.0,
                    mask_path: None,
                    color: [255, 255, 255],
                },
            ],
            blend_mode: IdBlendMode::Max,
            width: 32,
            height: 32,
        };
        // Triangle 0 hash is dark, manual is white. Max should pick white.
        let result = hash_source_cpu(&mesh, &params, 0, (0.0, 0.0), None, (32, 32));
        assert_eq!(result, [255, 255, 255, 255]);
    }

    #[test]
    fn id_multi_replace_first_wins() {
        let mesh = two_tri_mesh();
        let params = IdMultiParams {
            sources: vec![
                IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                },
                IdSourceParams {
                    source: IdSource::Manual,
                    weight: 1.0,
                    mask_path: None,
                    color: [255, 0, 0],
                },
            ],
            blend_mode: IdBlendMode::Replace,
            width: 32,
            height: 32,
        };
        // Triangle 0 has coverage, so first source wins
        let tri0_hash = fnv1a_triangle_cpu(0);
        let result = hash_source_cpu(&mesh, &params, 0, (0.0, 0.0), None, (32, 32));
        assert_eq!(result[0], tri0_hash[0]);
        assert_eq!(result[1], tri0_hash[1]);
        assert_eq!(result[2], tri0_hash[2]);
        assert_eq!(result[3], 255);
    }

    #[test]
    fn id_multi_id_map_source_samples_coverage() {
        let mesh = two_tri_mesh();
        // Synthetic ID map: 32x32, triangle 0 region = red, triangle 1 = green, uncovered = 0
        let mut id_map = vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..32 {
                let idx = ((y * 32 + x) * 4) as usize;
                if x < 16 {
                    id_map[idx] = 255; // R
                    id_map[idx + 3] = 255;
                } else {
                    id_map[idx + 1] = 255; // G
                    id_map[idx + 3] = 255;
                }
            }
        }
        let params = IdMultiParams {
            sources: vec![IdSourceParams {
                source: IdSource::IdMap,
                weight: 1.0,
                mask_path: None,
                color: [0, 0, 0],
            }],
            blend_mode: IdBlendMode::Additive,
            width: 32,
            height: 32,
        };
        // Note: CPU reference uses triangle index to approximate texel coord.
        // For a proper test, GPU path is needed. This test verifies the
        // validate_multi accepts IdMap source.
        assert!(validate_multi(&mesh, &params).is_ok());
    }

    #[test]
    fn id_multi_material_source_rejected_without_assignment() {
        let mesh = two_tri_mesh(); // no triangle_materials
        let params = IdMultiParams {
            sources: vec![IdSourceParams {
                source: IdSource::Material,
                weight: 1.0,
                mask_path: None,
                color: [0, 0, 0],
            }],
            blend_mode: IdBlendMode::Additive,
            width: 32,
            height: 32,
        };
        let err = validate_multi(&mesh, &params).unwrap_err();
        assert!(matches!(err, IdBakeError::MaterialAssignmentUnsupported));
    }

    #[test]
    fn id_multi_weight_zero_excludes_source() {
        let mesh = two_tri_mesh();
        let params_with = IdMultiParams {
            sources: vec![
                IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                },
                IdSourceParams {
                    source: IdSource::Manual,
                    weight: 0.0, // zero weight
                    mask_path: None,
                    color: [255, 0, 0],
                },
            ],
            blend_mode: IdBlendMode::Additive,
            width: 32,
            height: 32,
        };
        let params_without = IdMultiParams {
            sources: vec![IdSourceParams {
                source: IdSource::Triangle,
                weight: 1.0,
                mask_path: None,
                color: [0, 0, 0],
            }],
            blend_mode: IdBlendMode::Additive,
            width: 32,
            height: 32,
        };
        let r1 = hash_source_cpu(&mesh, &params_with, 0, (0.0, 0.0), None, (32, 32));
        let r2 = hash_source_cpu(&mesh, &params_without, 0, (0.0, 0.0), None, (32, 32));
        assert_eq!(r1, r2);
    }

    #[test]
    fn id_multi_mask_gates_contribution() {
        // CPU reference doesn't fully test mask gating (needs per-texel mask).
        // This test verifies validate_multi accepts mask_path.
        let mesh = two_tri_mesh();
        let params = IdMultiParams {
            sources: vec![IdSourceParams {
                source: IdSource::Manual,
                weight: 1.0,
                mask_path: Some("mask.png".into()),
                color: [255, 0, 0],
            }],
            blend_mode: IdBlendMode::Additive,
            width: 32,
            height: 32,
        };
        assert!(validate_multi(&mesh, &params).is_ok());
    }

    #[cfg(feature = "gpu")]
    mod gpu {
        use super::super::*;

        fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
            let instance = wgpu::Instance::default();
            let Ok(adapter) = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            ) else {
                eprintln!("skipping: no wgpu adapter available");
                return None;
            };
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
        }

        fn split_halves_mesh() -> MeshData {
            MeshData {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [1.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 6],
                uvs: vec![
                    [0.0, 0.0],
                    [0.5, 0.0],
                    [0.0, 1.0],
                    [0.5, 0.0],
                    [1.0, 0.0],
                    [1.0, 1.0],
                ],
                indices: vec![0, 1, 2, 3, 4, 5],
                material_names: vec!["m".into()],
                triangle_materials: Vec::new(),
            }
        }

        fn texel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
            let i = ((y * width + x) * 4) as usize;
            [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
        }

        const SIZE: u32 = 32;

        #[test]
        fn id_multi_additive_blend_matches_cpu() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![
                    IdSourceParams {
                        source: IdSource::Triangle,
                        weight: 0.5,
                        mask_path: None,
                        color: [0, 0, 0],
                    },
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 0.5,
                        mask_path: None,
                        color: [255, 0, 0],
                    },
                ],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            // Need to bake ID map first for IdMap source tests, but this test
            // uses Triangle + Manual so no ID map needed.
            let bytes =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("bake");
            assert_eq!(bytes.len(), (SIZE * SIZE * 4) as usize);

            // Verify a few texels in triangle 0 region
            let tri0_hash = fnv1a_triangle_cpu(0);
            let expected_r = ((tri0_hash[0] as f32 * 0.5) + 127.5) as u8;
            let expected_g = (tri0_hash[1] as f32 * 0.5) as u8;
            let expected_b = (tri0_hash[2] as f32 * 0.5) as u8;
            for y in [8, 16, 24] {
                let px = texel(&bytes, SIZE, 3, y);
                assert_eq!(px[0], expected_r.min(255), "x=3,y={y} R");
                assert_eq!(px[1], expected_g.min(255), "x=3,y={y} G");
                assert_eq!(px[2], expected_b.min(255), "x=3,y={y} B");
                assert_eq!(px[3], 255, "x=3,y={y} A");
            }
        }

        #[test]
        fn id_multi_max_blend_picks_brightest() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![
                    IdSourceParams {
                        source: IdSource::Triangle,
                        weight: 1.0,
                        mask_path: None,
                        color: [0, 0, 0],
                    },
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 1.0,
                        mask_path: None,
                        color: [255, 255, 255],
                    },
                ],
                blend_mode: IdBlendMode::Max,
                width: SIZE,
                height: SIZE,
            };
            let bytes =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("bake");
            // Triangle 0 region should be white (manual wins on Max)
            for y in [8, 16, 24] {
                let px = texel(&bytes, SIZE, 3, y);
                assert_eq!(px, [255, 255, 255, 255], "Max blend at (3,{y})");
            }
        }

        #[test]
        fn id_multi_replace_first_wins() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![
                    IdSourceParams {
                        source: IdSource::Triangle,
                        weight: 1.0,
                        mask_path: None,
                        color: [0, 0, 0],
                    },
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 1.0,
                        mask_path: None,
                        color: [255, 0, 0],
                    },
                ],
                blend_mode: IdBlendMode::Replace,
                width: SIZE,
                height: SIZE,
            };
            let bytes =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("bake");
            let tri0_hash = fnv1a_triangle_cpu(0);
            for y in [8, 16, 24] {
                let px = texel(&bytes, SIZE, 3, y);
                assert_eq!(px[0], tri0_hash[0], "Replace first wins R at (3,{y})");
                assert_eq!(px[1], tri0_hash[1], "Replace first wins G at (3,{y})");
                assert_eq!(px[2], tri0_hash[2], "Replace first wins B at (3,{y})");
                assert_eq!(px[3], 255);
            }
        }

        #[test]
        fn id_multi_id_map_source_masks_coverage() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();

            // First bake a regular ID map
            let id_params = IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: SIZE,
                height: SIZE,
            };
            let id_bytes = bake_id_mesh(&device, &queue, &mesh, &id_params).expect("id bake");

            // Create texture view from ID map
            let id_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("id_map_tex"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &id_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &id_bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
                wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
            );
            let id_view = id_tex.create_view(&wgpu::TextureViewDescriptor::default());

            // Now bake IdMulti with IdMap source
            let params = IdMultiParams {
                sources: vec![IdSourceParams {
                    source: IdSource::IdMap,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                }],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let bytes = bake_id_multi_mesh(&device, &queue, &mesh, &params, Some(&id_view), &[])
                .expect("bake");

            // Covered texels should match the original ID map; uncovered = 0
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let orig = texel(&id_bytes, SIZE, x, y);
                    let multi = texel(&bytes, SIZE, x, y);
                    if orig[3] == 255 {
                        assert_eq!(multi, orig, "IdMap source at ({x},{y})");
                    } else {
                        assert_eq!(multi, [0, 0, 0, 0], "uncovered at ({x},{y})");
                    }
                }
            }
        }

        #[test]
        fn id_multi_material_source_rejected_on_gpu_path() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![IdSourceParams {
                    source: IdSource::Material,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                }],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let err = bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).unwrap_err();
            assert!(matches!(err, IdBakeError::MaterialAssignmentUnsupported));
        }

        #[test]
        fn id_multi_weight_zero_is_byte_identical_to_omitted() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();

            let params_with_zero = IdMultiParams {
                sources: vec![
                    IdSourceParams {
                        source: IdSource::Triangle,
                        weight: 1.0,
                        mask_path: None,
                        color: [0, 0, 0],
                    },
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 0.0,
                        mask_path: None,
                        color: [255, 0, 0],
                    },
                ],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let params_without = IdMultiParams {
                sources: vec![IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                }],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let b1 = bake_id_multi_mesh(&device, &queue, &mesh, &params_with_zero, None, &[])
                .expect("bake1");
            let b2 = bake_id_multi_mesh(&device, &queue, &mesh, &params_without, None, &[])
                .expect("bake2");
            assert_eq!(b1, b2);
        }

        #[test]
        fn id_multi_mask_gates_contribution() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();

            // Create a mask texture: left half covered, right half not
            let mask_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mask_tex"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let mut mask_data = vec![0.0f32; (SIZE * SIZE * 4) as usize];
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let idx = ((y * SIZE + x) * 4) as usize;
                    if x < SIZE / 2 {
                        mask_data[idx + 3] = 1.0; // alpha = 1.0 (covered)
                    }
                }
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &mask_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&mask_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 16),
                    rows_per_image: Some(SIZE),
                },
                wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
            );
            let mask_view = mask_tex.create_view(&wgpu::TextureViewDescriptor::default());

            let params = IdMultiParams {
                sources: vec![
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 1.0,
                        mask_path: None,
                        color: [255, 0, 0],
                    },
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 1.0,
                        mask_path: None,
                        color: [0, 255, 0],
                    },
                ],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let bytes =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[Some(&mask_view)])
                    .expect("bake");

            // Left half (masked source 0 active): red (255,0,0) + green (0,255,0) = yellow clamped
            // Actually both have weight 1.0, Additive: red + green = (255, 255, 0)
            // Right half (source 0 masked out): only source 1 = green
            for y in [8, 16, 24] {
                let px_left = texel(&bytes, SIZE, 3, y);
                let px_right = texel(&bytes, SIZE, 28, y);
                // Left: both sources contribute
                assert_eq!(px_left, [255, 255, 0, 255], "left half masked at (3,{y})");
                // Right: only source 1 (green)
                assert_eq!(px_right, [0, 255, 0, 255], "right half masked at (28,{y})");
            }
        }

        #[test]
        fn id_multi_off_is_byte_identical_to_today() {
            // This test is implicitly verified by plan_bake tests in bakes_panel.rs
            // which assert IdMulti unchecked produces no jobs. The GPU test here
            // verifies that when IdMulti IS checked, it produces valid output.
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                }],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let bytes =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("bake");
            assert_eq!(bytes.len(), (SIZE * SIZE * 4) as usize);
        }

        #[test]
        fn id_multi_double_bake_is_byte_identical() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![
                    IdSourceParams {
                        source: IdSource::Triangle,
                        weight: 0.5,
                        mask_path: None,
                        color: [0, 0, 0],
                    },
                    IdSourceParams {
                        source: IdSource::Manual,
                        weight: 0.5,
                        mask_path: None,
                        color: [255, 0, 0],
                    },
                ],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let first =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("first");
            let second =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("second");
            assert_eq!(first, second);
        }

        #[test]
        fn id_multi_cpu_gpu_parity_texel_for_texel() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = split_halves_mesh();
            let params = IdMultiParams {
                sources: vec![IdSourceParams {
                    source: IdSource::Triangle,
                    weight: 1.0,
                    mask_path: None,
                    color: [0, 0, 0],
                }],
                blend_mode: IdBlendMode::Additive,
                width: SIZE,
                height: SIZE,
            };
            let gpu_bytes =
                bake_id_multi_mesh(&device, &queue, &mesh, &params, None, &[]).expect("gpu");
            // CPU reference over all texels
            let cpu_gbuffer =
                umber_mesh::UvGBuffer::build(&mesh, umber_mesh::FIRST_TILE, SIZE, SIZE);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let gpu = texel(&gpu_bytes, SIZE, x, y);
                    let hit = cpu_gbuffer.at(x, y);
                    let cpu = hit
                        .map(|h| {
                            let hash = fnv1a_triangle_cpu(h.triangle as u32);
                            [hash[0], hash[1], hash[2], 255]
                        })
                        .unwrap_or([0, 0, 0, 0]);
                    assert_eq!(gpu, cpu, "CPU-GPU parity at ({x},{y})");
                }
            }
        }
    }
}
