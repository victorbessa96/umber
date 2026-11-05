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
//! The design doc's v1 scope is "Material ID ... + Vertex ID", hedging on
//! whether `MeshData` carries material assignments. It does not — not usefully:
//! `MeshData::material_names` is the material *palette* (`Vec<String>`), with
//! no per-triangle assignment array anywhere in the mesh. So the honest v1
//! lineup is:
//!
//! - [`IdFlavor::Triangle`] (shipped): the per-texel winning triangle index,
//!   hashed. Deterministic masking by region, exactly what the design doc
//!   falls back to ("v1 hashes the TRIANGLE INDEX ... useful for masking by
//!   region even without materials").
//! - [`IdFlavor::Material`] (gated): exists in the enum so call sites and the
//!   UI can name the flavor, but [`bake_id_mesh`] rejects it with
//!   [`IdBakeError::MaterialAssignmentUnsupported`] — an honest runtime gate,
//!   not a silent wrong-color bake. It lands with the high-to-low transfer
//!   pass (item 4), which binds the source mesh's parts.
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

use std::borrow::Cow;

use umber_gpu::bake_shaders::ID_BAKE_SHADER;
use umber_mesh::MeshData;

use crate::ao::{AoBakeError, BakeTarget};
use crate::gbuffer::{bake_uv_gbuffer, UvGBufferError};
use crate::position::{build_pos_triangles, PositionMapError, MAX_TRIS_PER_BAKE};

/// Which identity [`bake_id_mesh`] hashes into each texel's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdFlavor {
    /// The per-texel winning triangle index, FNV-1a hashed — the v1 "mesh /
    /// region ID" (see this module's doc header for why this, not materials,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Rust mirror of `ID_BAKE_SHADER::hash_triangle`: FNV-1a over the
    /// triangle index's 4 little-endian bytes. The GPU test asserts the
    /// baked bytes equal this mirror's output — the hash computed twice,
    /// once in Rust once in WGSL, compared byte-for-byte (the design doc's
    /// prescribed test shape).
    fn fnv1a_triangle(tri: u32) -> [u8; 4] {
        let mut h: u32 = 2166136261;
        for b in tri.to_le_bytes() {
            h = (h ^ u32::from(b)).wrapping_mul(16777619);
        }
        [
            (h & 0xFF) as u8,
            ((h >> 8) & 0xFF) as u8,
            ((h >> 16) & 0xFF) as u8,
            255,
        ]
    }

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
        // FNV-1a(0u32 LE) — pins the mirror itself so a shared Rust/WGSL
        // bug can't hide behind the mirror agreeing with the shader.
        assert_eq!(fnv1a_triangle(0), [0x15, 0xF5, 0x95, 255]);
        assert_eq!(fnv1a_triangle(1), [0x04, 0xB6, 0x69, 255]);
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

    #[cfg(feature = "gpu")]
    mod gpu {
        use super::super::*;

        /// Requests a plain default device — this pass needs no special
        /// feature (write-only `Rgba8Unorm` storage is core WebGPU, and the
        /// `Rgba32Float` position input is sampled read-only, not
        /// read-written — see `bake_shaders::ID_BAKE_SHADER`'s doc comment).
        /// Skips gracefully (mirroring `umber_gpu::paint`'s test convention)
        /// if no adapter is available in this environment.
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

        /// Two triangles with clearly separated UV halves: triangle 0 owns
        /// `u in [0, 0.5]` (UVs `(0,0)`, `(0.5,0)`, `(0,1)`), triangle 1
        /// owns `u in [0.5, 1]` (UVs `(0.5,0)`, `(1,0)`, `(1,1)`). World
        /// positions are on the `z = 0` / `z = 1` planes respectively (any
        /// distinct spans do — only the UV footprints matter to this pass).
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

        /// Reads texel `(x, y)` of a `width`-wide RGBA8 `Vec<u8>` (as
        /// returned by [`bake_id_mesh`]).
        fn texel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
            let i = ((y * width + x) * 4) as usize;
            [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
        }

        const SIZE: u32 = 32;

        /// Texels in triangle 0's UV region hash EXACTLY to the Rust-side
        /// FNV-1a of triangle index 0; texels in triangle 1's region to the
        /// FNV-1a of 1 — the hash computed twice (Rust mirror vs WGSL),
        /// asserted byte-for-byte.
        ///
        /// Column `x = 3` sits at `u = 0.109` (inside triangle 0's
        /// `2u + v <= 1` footprint for rows `y >= 7`); column `x = 28` sits
        /// at `u = 0.891` (inside triangle 1's footprint for the same rows).
        /// Neither triangle's footprint reaches the other's column, so
        /// last-writer-wins never comes into play.
        #[test]
        fn split_halves_regions_hash_to_their_triangle_index() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let params = IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: SIZE,
                height: SIZE,
            };
            let bytes = bake_id_mesh(&device, &queue, &split_halves_mesh(), &params)
                .expect("bake should succeed");
            assert_eq!(bytes.len(), (SIZE * SIZE * 4) as usize);

            let expect_tri0 = super::fnv1a_triangle(0);
            let expect_tri1 = super::fnv1a_triangle(1);
            assert_ne!(
                expect_tri0, expect_tri1,
                "test premise: the two region colors must differ"
            );

            for y in [8, 16, 24] {
                assert_eq!(
                    texel(&bytes, SIZE, 3, y),
                    expect_tri0,
                    "texel (3, {y}) is inside triangle 0's UV footprint"
                );
                assert_eq!(
                    texel(&bytes, SIZE, 28, y),
                    expect_tri1,
                    "texel (28, {y}) is inside triangle 1's UV footprint"
                );
            }
        }

        /// Texels no triangle's UV footprint covers read fully zero —
        /// background `(0, 0, 0, 0)`, not a hash. Texel (3, 3) sits at
        /// `(u, v) ~= (0.109, 0.891)`: above triangle 0's `2u + v <= 1`
        /// hypotenuse and left of triangle 1's `u >= 0.5` half.
        #[test]
        fn uncovered_texels_read_fully_zero() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let params = IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: SIZE,
                height: SIZE,
            };
            let bytes = bake_id_mesh(&device, &queue, &split_halves_mesh(), &params)
                .expect("bake should succeed");

            assert_eq!(
                texel(&bytes, SIZE, 3, 3),
                [0, 0, 0, 0],
                "texel outside every UV footprint must read fully zero"
            );
            // And a covered texel is never zero-alpha, even in passing:
            // every texel is either background or full-coverage.
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let px = texel(&bytes, SIZE, x, y);
                    assert!(
                        px[3] == 0 || px[3] == 255,
                        "texel ({x}, {y}) alpha must be 0 or 255, got {}",
                        px[3]
                    );
                    if px[3] == 0 {
                        assert_eq!(px, [0, 0, 0, 0], "uncovered texel ({x}, {y})");
                    }
                }
            }
        }

        /// Baking the same mesh twice yields byte-identical outputs — no
        /// RNG anywhere in the position + ID path.
        #[test]
        fn double_bake_is_byte_identical() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let params = IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: SIZE,
                height: SIZE,
            };
            let first = bake_id_mesh(&device, &queue, &split_halves_mesh(), &params)
                .expect("first bake should succeed");
            let second = bake_id_mesh(&device, &queue, &split_halves_mesh(), &params)
                .expect("second bake should succeed");
            assert_eq!(first, second);
        }

        #[test]
        fn empty_mesh_is_rejected_without_touching_the_gpu_pipeline() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let params = IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: 8,
                height: 8,
            };
            let err = bake_id_mesh(&device, &queue, &MeshData::default(), &params).unwrap_err();
            assert!(matches!(err, IdBakeError::EmptyMesh));
        }

        #[test]
        fn mesh_over_the_triangle_budget_is_rejected() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mut mesh = split_halves_mesh();
            mesh.positions = vec![[0.0, 0.0, 0.0]; 3];
            mesh.uvs = vec![[0.0, 0.0]; 3];
            mesh.indices = [0, 1, 2].repeat(MAX_TRIS_PER_BAKE as usize + 1);
            let params = IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: 8,
                height: 8,
            };
            let err = bake_id_mesh(&device, &queue, &mesh, &params).unwrap_err();
            assert!(matches!(err, IdBakeError::TooManyTriangles { .. }));
        }

        #[test]
        fn material_flavor_is_rejected_on_the_gpu_path_too() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let params = IdBakeParams {
                flavor: IdFlavor::Material,
                width: 8,
                height: 8,
            };
            let err = bake_id_mesh(&device, &queue, &split_halves_mesh(), &params).unwrap_err();
            assert!(matches!(err, IdBakeError::MaterialAssignmentUnsupported));
        }

        /// Row 156-B parity pin (kill 4 groundwork): at EVERY texel the GPU
        /// ID bake's coverage and winning triangle equal the CPU authority
        /// `umber_mesh::UvGBuffer`. Fixture: a unit quad (shared diagonal,
        /// last-in-index-order wins) plus an overlapping third triangle, so
        /// both the edge-inclusive rule and the overlap rule are exercised.
        /// Two sizes, one non-square, catch a texel-center convention drift.
        #[test]
        fn gpu_id_bake_matches_cpu_uv_gbuffer_texel_for_texel() {
            let Some((device, queue)) = try_request_device() else {
                return;
            };
            let mesh = MeshData {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.25, 0.25, 0.5],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 5],
                uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.25, 0.25]],
                indices: vec![0, 1, 2, 0, 2, 3, 4, 1, 2],
                material_names: vec!["m".into()],
                triangle_materials: Vec::new(),
            };
            for (w, h) in [(32u32, 32u32), (24, 40)] {
                let params = IdBakeParams {
                    flavor: IdFlavor::Triangle,
                    width: w,
                    height: h,
                };
                let bytes = bake_id_mesh(&device, &queue, &mesh, &params).expect("bake");
                let cpu = umber_mesh::UvGBuffer::build(&mesh, umber_mesh::FIRST_TILE, w, h);
                let mut mismatches = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        let gpu = texel(&bytes, w, x, y);
                        let want = cpu
                            .at(x, y)
                            .map(|hit| super::fnv1a_triangle(hit.triangle as u32))
                            .unwrap_or([0, 0, 0, 0]);
                        if gpu != want {
                            mismatches.push((x, y, gpu, want));
                        }
                    }
                }
                assert!(
                    mismatches.is_empty(),
                    "{w}x{h}: {} texels differ GPU vs CPU, first: {:?}",
                    mismatches.len(),
                    mismatches.first()
                );
            }
        }
    }
}
