//! The GPU UV G-buffer (queue row 156-B kill 4): per texel of one UDIM
//! tile, the covering triangle and its barycentrics, computed on the GPU
//! by `umber_gpu::bake_shaders::UV_GBUFFER_SHADER`.
//!
//! The CPU `umber_mesh::UvGBuffer` is the authority; this pass is its
//! accelerator. The per-triangle signed UV area is computed on the CPU by
//! `umber_mesh::UvTri::from_uvs` and uploaded, so both sides divide by
//! identical bits and apply identical validity rules (malformed, non-finite
//! and degenerate triangles upload as all-zero records and claim nothing).
//! Parity is pinned texel-for-texel by the `gpu` tests below.

use std::borrow::Cow;
use std::mem::size_of;
use std::num::NonZeroU64;

use bytemuck::Zeroable as _;
use wgpu::util::DeviceExt as _;

use umber_gpu::bake_shaders::UV_GBUFFER_SHADER;
use umber_mesh::uv_gbuffer::tile_offset;
use umber_mesh::{MeshData, UvGBuffer, UvHit, UvTri};

use crate::position::{readback_rows_per_band, MAX_TRIS_PER_BAKE};

/// The `prim` value of an uncovered texel.
pub const NO_PRIM: u32 = u32::MAX;

/// Errors from [`bake_uv_gbuffer`].
#[derive(Debug, thiserror::Error)]
pub enum UvGBufferError {
    /// `mesh` has no triangles.
    #[error("mesh has no triangles")]
    EmptyMesh,
    /// More triangles than the brute-force budget.
    #[error("mesh has {count} triangles, over the {max} budget for this bake path")]
    TooManyTriangles {
        /// The mesh's triangle count.
        count: usize,
        /// [`MAX_TRIS_PER_BAKE`].
        max: u32,
    },
    /// Zero width or height.
    #[error("G-buffer dimensions must be non-zero (got {width}x{height})")]
    EmptyTarget {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// GPU readback failed.
    #[error("G-buffer readback failed: {0}")]
    Readback(String),
}

/// One triangle record, byte-identical to the WGSL `GbTri`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuGbTri {
    uv01: [f32; 4],
    /// `[uv2.x, uv2.y, signed area (0 = skip), 0]`.
    uv2: [f32; 4],
}

const _: () = assert!(
    size_of::<GpuGbTri>() == 32,
    "GpuGbTri must match WGSL GbTri"
);

/// The params uniform, byte-identical to the WGSL `GbParams`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GbUniform {
    dims: [u32; 2],
    tri_count: u32,
    _pad: u32,
    tile_off: [f32; 2],
    _pad2: [f32; 2],
}

const _: () = assert!(
    size_of::<GbUniform>() == 32,
    "GbUniform must match WGSL GbParams"
);

fn validate(mesh: &MeshData, width: u32, height: u32) -> Result<(), UvGBufferError> {
    let count = mesh.triangle_count();
    if count == 0 {
        return Err(UvGBufferError::EmptyMesh);
    }
    if count > MAX_TRIS_PER_BAKE as usize {
        return Err(UvGBufferError::TooManyTriangles {
            count,
            max: MAX_TRIS_PER_BAKE,
        });
    }
    if width == 0 || height == 0 {
        return Err(UvGBufferError::EmptyTarget { width, height });
    }
    Ok(())
}

/// The triangle upload: one record per triangle in index order; invalid
/// triangles (per `UvTri::of`) become all-zero records, so indices stay
/// aligned with the CPU G-buffer's triangle numbering.
fn build_triangles(mesh: &MeshData) -> Vec<GpuGbTri> {
    (0..mesh.triangle_count())
        .map(|t| match UvTri::of(mesh, t) {
            Some(tri) => {
                let [a, b, c] = tri.uv;
                GpuGbTri {
                    uv01: [a[0], a[1], b[0], b[1]],
                    uv2: [c[0], c[1], tri.area, 0.0],
                }
            }
            None => GpuGbTri::zeroed(),
        })
        .collect()
}

/// The GPU-resident G-buffer for one tile.
pub struct UvGBufferGpu {
    /// `Rgba32Float`: `(b0, b1, b2, coverage)`.
    pub bary_texture: wgpu::Texture,
    /// `R32Uint`: triangle index or [`NO_PRIM`].
    pub prim_texture: wgpu::Texture,
    /// The tile this buffer covers.
    pub tile: u16,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
}

/// Runs the G-buffer pass for `tile` at `width`x`height`, leaving both
/// textures on the GPU (`STORAGE_BINDING | TEXTURE_BINDING | COPY_SRC`).
///
/// # Errors
///
/// [`UvGBufferError::EmptyMesh`], [`UvGBufferError::TooManyTriangles`],
/// [`UvGBufferError::EmptyTarget`]; never panics on malformed meshes.
pub fn bake_uv_gbuffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
) -> Result<UvGBufferGpu, UvGBufferError> {
    profiling::scope!("bake_pass");
    validate(mesh, width, height)?;
    let triangles = build_triangles(mesh);
    let triangle_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("umber_bake_gbuffer_triangles"),
        contents: bytemuck::cast_slice(&triangles),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let (tu, tv) = tile_offset(tile);
    let uniform = GbUniform {
        dims: [width, height],
        tri_count: triangles.len() as u32,
        _pad: 0,
        tile_off: [tu, tv],
        _pad2: [0.0; 2],
    };
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("umber_bake_gbuffer_params"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let make = |label: &str, format: wgpu::TextureFormat| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let bary_texture = make("umber_bake_gbuffer_bary", wgpu::TextureFormat::Rgba32Float);
    let prim_texture = make("umber_bake_gbuffer_prim", wgpu::TextureFormat::R32Uint);
    let bary_view = bary_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let prim_view = prim_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("umber_bake_gbuffer_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(UV_GBUFFER_SHADER)),
    });
    let storage_tex = |binding, format| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("umber_bake_gbuffer_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<GpuGbTri>() as u64),
                },
                count: None,
            },
            storage_tex(1, wgpu::TextureFormat::Rgba32Float),
            storage_tex(2, wgpu::TextureFormat::R32Uint),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<GbUniform>() as u64),
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("umber_bake_gbuffer_pipeline_layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("umber_bake_gbuffer_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("umber_bake_gbuffer_bind_group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: triangle_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&bary_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&prim_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("umber_bake_gbuffer_encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("umber_bake_gbuffer_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));
    Ok(UvGBufferGpu {
        bary_texture,
        prim_texture,
        tile,
        width,
        height,
    })
}

/// Reads a texture back as tightly packed bytes (`bytes_per_texel` per
/// texel, row-major), banded by the device's `max_buffer_size`.
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    bytes_per_texel: u32,
) -> Result<Vec<u8>, UvGBufferError> {
    let unpadded = width * bytes_per_texel;
    let padded = unpadded.div_ceil(256) * 256;
    let band = readback_rows_per_band(padded, height, device.limits().max_buffer_size);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("umber_bake_gbuffer_readback"),
        size: u64::from(padded) * u64::from(band),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut out = Vec::with_capacity(unpadded as usize * height as usize);
    let mut y0 = 0;
    while y0 < height {
        let rows = band.min(height - y0);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("umber_bake_gbuffer_readback_encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(rows),
                },
            },
            wgpu::Extent3d {
                width,
                height: rows,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..u64::from(padded) * u64::from(rows));
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| UvGBufferError::Readback(format!("device poll: {e}")))?;
        rx.recv()
            .map_err(|_| UvGBufferError::Readback("map callback channel closed".into()))?
            .map_err(|e| UvGBufferError::Readback(format!("buffer map: {e}")))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| UvGBufferError::Readback(format!("mapped range: {e}")))?;
        for r in 0..rows as usize {
            let s = r * padded as usize;
            out.extend_from_slice(&data[s..s + unpadded as usize]);
        }
        drop(data);
        buffer.unmap();
        y0 += rows;
    }
    Ok(out)
}

impl UvGBufferGpu {
    /// Reads both textures back into the CPU `UvGBuffer` shape, so callers
    /// and tests compare GPU output against `UvGBuffer::build` directly.
    ///
    /// # Errors
    ///
    /// [`UvGBufferError::Readback`] on any GPU map failure.
    pub fn read_back(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<UvGBuffer, UvGBufferError> {
        let (w, h) = (self.width, self.height);
        let bary = read_back(device, queue, &self.bary_texture, w, h, 16)?;
        let prim = read_back(device, queue, &self.prim_texture, w, h, 4)?;
        let bary: &[f32] = bytemuck::cast_slice(&bary);
        let prim: &[u32] = bytemuck::cast_slice(&prim);
        let hits = prim
            .iter()
            .enumerate()
            .map(|(i, &t)| {
                (t != NO_PRIM).then(|| UvHit {
                    triangle: t as usize,
                    bary: [bary[i * 4], bary[i * 4 + 1], bary[i * 4 + 2]],
                })
            })
            .collect();
        Ok(UvGBuffer {
            tile: self.tile,
            width: w,
            height: h,
            hits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> MeshData {
        MeshData {
            positions: vec![[0.0; 3]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        }
    }

    #[test]
    fn uniform_and_triangle_layouts_match_wgsl() {
        assert_eq!(size_of::<GpuGbTri>(), 32);
        assert_eq!(size_of::<GbUniform>(), 32);
    }

    #[test]
    fn invalid_triangles_upload_as_zero_records_keeping_index_alignment() {
        let mut mesh = quad();
        mesh.indices.extend_from_slice(&[0, 1, 99]); // malformed uv index
        mesh.uvs.push([f32::NAN, 0.0]);
        mesh.indices.extend_from_slice(&[0, 1, 4]); // non-finite
        let tris = build_triangles(&mesh);
        assert_eq!(tris.len(), 4);
        assert_ne!(tris[0].uv2[2], 0.0);
        assert_eq!(tris[2], GpuGbTri::zeroed());
        assert_eq!(tris[3], GpuGbTri::zeroed());
        assert_eq!(
            tris[0].uv2[2],
            UvTri::of(&mesh, 0).expect("valid").area,
            "area uploaded bit-for-bit from the CPU authority"
        );
    }

    #[test]
    fn validate_rejects_empty_inputs() {
        assert!(matches!(
            validate(&MeshData::default(), 8, 8),
            Err(UvGBufferError::EmptyMesh)
        ));
        assert!(matches!(
            validate(&quad(), 0, 8),
            Err(UvGBufferError::EmptyTarget { .. })
        ));
        assert!(validate(&quad(), 8, 8).is_ok());
    }

    #[cfg(feature = "gpu")]
    mod gpu {
        use super::super::*;
        use super::quad;

        fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
            let instance = wgpu::Instance::default();
            let Ok(adapter) = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            ) else {
                eprintln!("skipping: no wgpu adapter available");
                return None;
            };
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
        }

        /// Triangle ids + coverage must match EXACTLY; barycentrics within
        /// `max_abs` absolute (Vulkan allows 2.5 ULP f32 division and may
        /// contract to FMA, and a weight near 0 can flip sign, so a ULP
        /// metric is meaningless there). `max_abs` = `UV_BARY_EPS` keeps the
        /// bound at the inside-test's own tolerance scale.
        fn assert_parity(mesh: &MeshData, tile: u16, w: u32, h: u32, max_abs: f32) {
            let Some((device, queue)) = device() else {
                return;
            };
            let gpu = bake_uv_gbuffer(&device, &queue, mesh, tile, w, h)
                .expect("bake")
                .read_back(&device, &queue)
                .expect("readback");
            let cpu = UvGBuffer::build(mesh, tile, w, h);
            let mut prim_bad = 0;
            let mut worst = 0.0f32;
            for (i, (g, c)) in gpu.hits.iter().zip(&cpu.hits).enumerate() {
                match (g, c) {
                    (Some(g), Some(c)) if g.triangle == c.triangle => {
                        for k in 0..3 {
                            worst = worst.max((g.bary[k] - c.bary[k]).abs());
                        }
                    }
                    (None, None) => {}
                    _ => {
                        prim_bad += 1;
                        if prim_bad == 1 {
                            eprintln!("first prim mismatch at {i}: gpu {g:?} cpu {c:?}");
                        }
                    }
                }
            }
            assert_eq!(
                prim_bad, 0,
                "tile {tile} {w}x{h}: coverage/triangle mismatches"
            );
            assert!(
                worst <= max_abs,
                "tile {tile} {w}x{h}: bary differs by {worst:e} (> {max_abs:e})"
            );
            eprintln!("parity tile {tile} {w}x{h}: prim exact, bary worst abs {worst:e}");
        }

        fn overlapped_quad() -> MeshData {
            let mut m = quad();
            m.uvs.push([0.25, 0.25]);
            m.positions.push([0.0; 3]);
            m.indices.extend_from_slice(&[4, 1, 2]);
            m
        }

        #[test]
        fn gpu_gbuffer_matches_cpu_single_tile() {
            assert_parity(&overlapped_quad(), 1001, 32, 32, umber_mesh::UV_BARY_EPS);
            assert_parity(&overlapped_quad(), 1001, 24, 40, umber_mesh::UV_BARY_EPS);
        }

        /// A 2-tile UDIM mesh: a quad in 1001 and a skewed triangle in 1012
        /// (offset (1, 1)); each tile sees only its own footprint.
        #[test]
        fn gpu_gbuffer_matches_cpu_udim_tiles() {
            let mut m = quad();
            m.uvs
                .extend_from_slice(&[[1.1, 1.05], [1.9, 1.2], [1.3, 1.95]]);
            m.positions.extend_from_slice(&[[0.0; 3]; 3]);
            m.indices.extend_from_slice(&[4, 5, 6]);
            assert_parity(&m, 1001, 16, 16, umber_mesh::UV_BARY_EPS);
            assert_parity(&m, 1012, 16, 16, umber_mesh::UV_BARY_EPS);
            assert_parity(&m, 1002, 8, 8, umber_mesh::UV_BARY_EPS);
        }

        #[test]
        fn malformed_triangles_claim_nothing_on_the_gpu() {
            let mut m = quad();
            m.indices.extend_from_slice(&[0, 1, 99]);
            assert_parity(&m, 1001, 16, 16, umber_mesh::UV_BARY_EPS);
        }
    }
}
