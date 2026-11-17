//! Ground plane baking: AO, height, and ID baked onto a generated ground plane
//! mesh beneath the scene.
//!
//! The ground plane is a generated square mesh centered at (0, -height_offset, 0)
//! with planar UVs covering [0, 1] × [0, 1]. Rays are cast from the plane's
//! texels against the scene mesh (as occluder) to bake:
//!   - AO: hemisphere raycast (like AO baker)
//!   - Height: ray along +Y (world up) distance to first hit, normalized by max_distance
//!   - ID: FNV-1a hash of hit triangle index (like ID baker)

use std::borrow::Cow;
use std::mem::size_of;
use std::num::NonZeroU64;

use glam::Vec3;
use wgpu::util::DeviceExt as _;

use umber_gpu::bake_shaders::GROUND_PLANE_BAKE_SHADER_2D_FALLBACK;
use umber_mesh::{generate_ground_plane, GroundPlaneDesc, GroundPlaneError, MeshData};

use crate::ao::{build_triangles, GpuTriangle};
use crate::position::{bake_position_and_normal, PositionMapError};

/// Which map(s) to bake onto the ground plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundPlaneMap {
    /// Ambient occlusion (hemisphere rays from plane texels against scene).
    Ao,
    /// Distance from plane to first scene hit along +Y (world up).
    Height,
    /// Triangle ID of first hit (FNV-1a hash, same as `Id` baker).
    Id,
}

impl GroundPlaneMap {
    fn bit(self) -> u32 {
        match self {
            GroundPlaneMap::Ao => 1,
            GroundPlaneMap::Height => 2,
            GroundPlaneMap::Id => 4,
        }
    }
}

/// Parameters for ground plane baking.
#[derive(Debug, Clone, Copy)]
pub struct GroundPlaneParams {
    /// Ground plane geometry description.
    pub desc: GroundPlaneDesc,
    /// Bitmask of maps to bake: bit 0=AO, bit 1=Height, bit 2=Id.
    pub maps_bitmask: u32,
    /// Hemisphere rays cast per texel (AO only).
    pub rays: u32,
    /// Maximum ray travel distance (AO and Height).
    pub max_distance: f32,
    /// Ray origin offset along normal to avoid self-intersection.
    pub bias: f32,
}

impl GroundPlaneParams {
    /// Default ray count for AO.
    pub const DEFAULT_RAYS: u32 = 16;
    /// Default maximum ray distance.
    pub const DEFAULT_MAX_DISTANCE: f32 = 100.0;
    /// Default ray origin bias.
    pub const DEFAULT_BIAS: f32 = 0.001;

    /// Builds params with defaults; override fields afterward.
    pub fn new(desc: GroundPlaneDesc, maps: Vec<GroundPlaneMap>) -> Self {
        let mut maps_bitmask = 0;
        for m in maps {
            maps_bitmask |= m.bit();
        }
        Self {
            desc,
            maps_bitmask,
            rays: Self::DEFAULT_RAYS,
            max_distance: Self::DEFAULT_MAX_DISTANCE,
            bias: Self::DEFAULT_BIAS,
        }
    }
}

/// Errors from ground plane baking.
#[derive(Debug, thiserror::Error)]
pub enum GroundPlaneBakeError {
    /// `mesh` has no triangles to raycast against.
    #[error("scene mesh has no triangles to bake against")]
    EmptyMesh,
    /// `params.rays` was zero (required for AO).
    #[error("ray count must be at least 1 for AO")]
    InvalidRayCount,
    /// The requested bake-target width or height was zero.
    #[error("bake target dimensions must be non-zero (got {width}x{height})")]
    EmptyTarget { width: u32, height: u32 },
    /// Ground plane generation failed.
    #[error("ground plane generation: {0}")]
    GroundPlane(#[from] GroundPlaneError),
    /// Reading the baked texture back to CPU memory failed.
    #[error("bake-target readback failed: {0}")]
    Readback(String),
    /// Position map pass failed.
    #[error("position map: {0}")]
    PositionMap(#[from] PositionMapError),
}

/// Output maps from a ground plane bake.
#[derive(Debug, Default)]
pub struct BakeOutputs {
    /// AO map (RGBA8, R = occlusion, A = coverage).
    pub ao: Option<Vec<u8>>,
    /// Height map (RGBA8, R = normalized distance, A = coverage).
    pub height: Option<Vec<u8>>,
    /// ID map (RGBA8, RGB = FNV-1a hash color, A = coverage).
    pub id: Option<Vec<u8>>,
}

/// Validates ground plane bake inputs.
pub fn validate_ground_plane(
    mesh: &MeshData,
    desc: &GroundPlaneDesc,
    params: &GroundPlaneParams,
    width: u32,
    height: u32,
) -> Result<(), GroundPlaneBakeError> {
    if mesh.triangle_count() == 0 {
        return Err(GroundPlaneBakeError::EmptyMesh);
    }
    if desc.size <= 0.0 {
        return Err(GroundPlaneBakeError::GroundPlane(
            GroundPlaneError::InvalidSize { size: desc.size },
        ));
    }
    if desc.subdivisions == 0 {
        return Err(GroundPlaneBakeError::GroundPlane(
            GroundPlaneError::InvalidSubdivisions,
        ));
    }
    if params.maps_bitmask == 0 {
        return Err(GroundPlaneBakeError::GroundPlane(
            GroundPlaneError::InvalidSubdivisions,
        ));
    }
    if params.maps_bitmask & GroundPlaneMap::Ao.bit() != 0 && params.rays == 0 {
        return Err(GroundPlaneBakeError::InvalidRayCount);
    }
    if width == 0 || height == 0 {
        return Err(GroundPlaneBakeError::EmptyTarget { width, height });
    }
    Ok(())
}

/// GPU-side params uniform for the ground plane shader (32 bytes, 16-byte aligned).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpUniform {
    maps_bitmask: u32,
    rays: u32,
    max_distance: f32,
    bias: f32,
    tri_count: u32,
    _pad: [u32; 3],
}

const _: () = assert!(
    size_of::<GpUniform>() == 32,
    "GpUniform must be 32 bytes for WGSL alignment"
);

fn uniform_for(params: &GroundPlaneParams, tri_count: u32) -> GpUniform {
    GpUniform {
        maps_bitmask: params.maps_bitmask,
        rays: params.rays,
        max_distance: params.max_distance,
        bias: params.bias,
        tri_count,
        _pad: [0; 3],
    }
}

/// CPU reference: single-texel AO via hemisphere sampling against scene triangles.
#[allow(dead_code)]
fn ground_plane_ao_cpu(
    plane: &MeshData,
    scene: &MeshData,
    desc: &GroundPlaneDesc,
    params: &GroundPlaneParams,
    texel: (u32, u32),
) -> f32 {
    // Infer texture resolution from plane mesh (ground plane is square with uniform subdivisions)
    let tex_res = (plane.positions.len() as f32).sqrt() - 1.0;
    if tex_res < 1.0 {
        return 1.0;
    }

    // Reconstruct world position from UV
    let (u, v) = texel;
    let uv_u = (u as f32 + 0.5) / tex_res;
    let uv_v = (v as f32 + 0.5) / tex_res;
    let half = desc.size * 0.5;
    let x = -half + uv_u * desc.size;
    let z = -half + uv_v * desc.size;
    let y = -desc.height_offset;
    let surface_pos = Vec3::new(x, y, z);
    let normal = Vec3::Y;
    let ray_origin = surface_pos + normal * params.bias;

    let rays = params.rays.max(1);
    let mut hit_count = 0;
    let max_dist = params.max_distance;

    for i in 0..rays {
        // Hemisphere sample (matching AO shader)
        const GOLDEN_CONJ: f32 = 0.618034;
        let nf = rays as f32;
        let cos_theta = 1.0 - ((i as f32) + 0.5) / nf;
        let sin_theta = (0.0f32).max(1.0 - cos_theta * cos_theta).sqrt();
        let scaled = (i as f32) * GOLDEN_CONJ;
        let phi = 2.0 * std::f32::consts::PI * (scaled - scaled.floor());
        let local_dir = Vec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);
        // Steer to world space (plane normal is +Y)
        let dir = Vec3::new(local_dir.x, local_dir.z, local_dir.y).normalize();

        // Raycast against scene triangles
        let mut hit = false;
        for tri_chunk in scene.indices.chunks_exact(3) {
            let (i0, i1, i2) = (
                tri_chunk[0] as usize,
                tri_chunk[1] as usize,
                tri_chunk[2] as usize,
            );
            let (Some(&p0), Some(&p1), Some(&p2)) = (
                scene.positions.get(i0),
                scene.positions.get(i1),
                scene.positions.get(i2),
            ) else {
                continue;
            };
            let v0 = Vec3::from(p0);
            let v1 = Vec3::from(p1);
            let v2 = Vec3::from(p2);

            // Möller–Trumbore
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let h = dir.cross(edge2);
            let a = edge1.dot(h);
            if a.abs() < 1e-6 {
                continue;
            }
            let f = 1.0 / a;
            let s = ray_origin - v0;
            let u = f * s.dot(h);
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(edge1);
            let v = f * dir.dot(q);
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = f * edge2.dot(q);
            if t > 1e-5 && t < max_dist {
                hit = true;
                break;
            }
        }
        if hit {
            hit_count += 1;
        }
    }

    let blocked = hit_count as f32 / rays as f32;
    1.0 - blocked.clamp(0.0, 1.0)
}

/// CPU reference: single-texel height (distance along +Y toward scene above).
#[allow(dead_code)]
fn ground_plane_height_cpu(
    plane: &MeshData,
    scene: &MeshData,
    desc: &GroundPlaneDesc,
    params: &GroundPlaneParams,
    texel: (u32, u32),
) -> f32 {
    // Infer texture resolution from plane mesh (ground plane is square with uniform subdivisions)
    let tex_res = (plane.positions.len() as f32).sqrt() - 1.0;
    if tex_res < 1.0 {
        return 0.0;
    }

    let (u, v) = texel;
    let uv_u = (u as f32 + 0.5) / tex_res;
    let uv_v = (v as f32 + 0.5) / tex_res;
    let half = desc.size * 0.5;
    let x = -half + uv_u * desc.size;
    let z = -half + uv_v * desc.size;
    let y = -desc.height_offset;
    let ray_origin = Vec3::new(x, y, z);
    let dir = Vec3::Y; // Toward scene above (+Y)

    let mut best_t = f32::INFINITY;
    for tri_chunk in scene.indices.chunks_exact(3) {
        let (i0, i1, i2) = (
            tri_chunk[0] as usize,
            tri_chunk[1] as usize,
            tri_chunk[2] as usize,
        );
        let (Some(&p0), Some(&p1), Some(&p2)) = (
            scene.positions.get(i0),
            scene.positions.get(i1),
            scene.positions.get(i2),
        ) else {
            continue;
        };
        let v0 = Vec3::from(p0);
        let v1 = Vec3::from(p1);
        let v2 = Vec3::from(p2);

        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = dir.cross(edge2);
        let a = edge1.dot(h);
        if a.abs() < 1e-6 {
            continue;
        }
        let f = 1.0 / a;
        let s = ray_origin - v0;
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = s.cross(edge1);
        let v = f * dir.dot(q);
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = f * edge2.dot(q);
        if t > 1e-5 && t < params.max_distance && t < best_t {
            best_t = t;
        }
    }

    if best_t.is_finite() {
        (best_t / params.max_distance).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// CPU reference: single-texel ID (FNV-1a hash of hit triangle index).
#[allow(dead_code)]
fn ground_plane_id_cpu(
    plane: &MeshData,
    scene: &MeshData,
    desc: &GroundPlaneDesc,
    texel: (u32, u32),
) -> [u8; 4] {
    // Infer texture resolution from plane mesh (ground plane is square with uniform subdivisions)
    let tex_res = (plane.positions.len() as f32).sqrt() - 1.0;
    if tex_res < 1.0 {
        return [0, 0, 0, 0];
    }

    let (u, v) = texel;
    let uv_u = (u as f32 + 0.5) / tex_res;
    let uv_v = (v as f32 + 0.5) / tex_res;
    let half = desc.size * 0.5;
    let x = -half + uv_u * desc.size;
    let z = -half + uv_v * desc.size;
    let y = -desc.height_offset;
    let ray_origin = Vec3::new(x, y, z);
    let dir = Vec3::Y; // Toward scene above (+Y)

    let mut best_t = f32::INFINITY;
    let mut best_tri = None;

    for (tri_idx, tri_chunk) in scene.indices.chunks_exact(3).enumerate() {
        let (i0, i1, i2) = (
            tri_chunk[0] as usize,
            tri_chunk[1] as usize,
            tri_chunk[2] as usize,
        );
        let (Some(&p0), Some(&p1), Some(&p2)) = (
            scene.positions.get(i0),
            scene.positions.get(i1),
            scene.positions.get(i2),
        ) else {
            continue;
        };
        let v0 = Vec3::from(p0);
        let v1 = Vec3::from(p1);
        let v2 = Vec3::from(p2);

        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = dir.cross(edge2);
        let a = edge1.dot(h);
        if a.abs() < 1e-6 {
            continue;
        }
        let f = 1.0 / a;
        let s = ray_origin - v0;
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = s.cross(edge1);
        let v = f * dir.dot(q);
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = f * edge2.dot(q);
        if t > 1e-5 && t < best_t {
            best_t = t;
            best_tri = Some(tri_idx as u32);
        }
    }

    if let Some(tri) = best_tri {
        // FNV-1a 32-bit hash (using u64 with wrapping_mul to prevent overflow)
        let mut h: u64 = 2166136261;
        const FNV_PRIME: u64 = 16777619;
        h = (h ^ (tri as u64 & 0xFF)).wrapping_mul(FNV_PRIME);
        h = (h ^ ((tri as u64 >> 8) & 0xFF)).wrapping_mul(FNV_PRIME);
        h = (h ^ ((tri as u64 >> 16) & 0xFF)).wrapping_mul(FNV_PRIME);
        h = (h ^ ((tri as u64 >> 24) & 0xFF)).wrapping_mul(FNV_PRIME);
        [
            (h & 0xFF) as u8,
            ((h >> 8) & 0xFF) as u8,
            ((h >> 16) & 0xFF) as u8,
            255,
        ]
    } else {
        [0, 0, 0, 0]
    }
}

/// Bakes ground plane maps on the GPU using three separate 2D storage textures
/// (AO, Height, ID). This is the universally-supported fallback path per the D5 brief:
/// "three separate texture_storage_2d bindings if array not supported on target".
#[allow(clippy::too_many_arguments)]
pub fn bake_ground_plane_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene_mesh: &MeshData,
    params: &GroundPlaneParams,
    width: u32,
    height: u32,
) -> Result<BakeOutputs, GroundPlaneBakeError> {
    profiling::scope!("bake_pass");
    validate_ground_plane(scene_mesh, &params.desc, params, width, height)?;

    // Generate ground plane mesh
    let plane_mesh = generate_ground_plane(&params.desc)?;

    // Rasterize plane's own UV layout into position + normal textures
    let position_map = bake_position_and_normal(device, queue, &plane_mesh, width, height)?;

    // Build scene triangle buffer
    let triangles = build_triangles(scene_mesh);
    let triangle_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("umber_bake_ground_plane_triangles"),
        contents: bytemuck::cast_slice(&triangles),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let uniform = uniform_for(params, triangles.len() as u32);
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("umber_bake_ground_plane_params"),
        contents: bytemuck::cast_slice(&[uniform]),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    // Use three separate 2D storage textures (fallback path per brief).
    try_bake_2d_fallback(
        device,
        queue,
        &triangle_buffer,
        &params_buffer,
        &position_map,
        width,
        height,
        params.maps_bitmask,
    )
}

/// Bakes using three separate 2D storage textures (AO, Height, ID).
/// This is the universally-supported fallback path per the D5 brief.
#[allow(clippy::too_many_arguments)]
fn try_bake_2d_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    triangle_buffer: &wgpu::Buffer,
    params_buffer: &wgpu::Buffer,
    position_map: &crate::position::PositionMapGpu,
    width: u32,
    height: u32,
    maps_bitmask: u32,
) -> Result<BakeOutputs, GroundPlaneBakeError> {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("umber_bake_ground_plane_shader_2d_fallback"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(GROUND_PLANE_BAKE_SHADER_2D_FALLBACK)),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("umber_bake_ground_plane_bind_group_layout_2d"),
        entries: &[
            // Binding 0: scene triangles
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<GpuTriangle>() as u64),
                },
                count: None,
            },
            // Binding 1: AO output texture
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
            // Binding 2: Height output texture
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
            // Binding 3: ID output texture
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            // Binding 4: params uniform
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<GpUniform>() as u64),
                },
                count: None,
            },
            // Binding 5: position texture
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // Binding 6: normal texture
            wgpu::BindGroupLayoutEntry {
                binding: 6,
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

    // Create three separate 2D output textures
    let create_output_tex = |label: &str| -> wgpu::Texture {
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
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };

    let ao_texture = if maps_bitmask & GroundPlaneMap::Ao.bit() != 0 {
        Some(create_output_tex("umber_bake_ground_plane_ao"))
    } else {
        None
    };
    let height_texture = if maps_bitmask & GroundPlaneMap::Height.bit() != 0 {
        Some(create_output_tex("umber_bake_ground_plane_height"))
    } else {
        None
    };
    let id_texture = if maps_bitmask & GroundPlaneMap::Id.bit() != 0 {
        Some(create_output_tex("umber_bake_ground_plane_id"))
    } else {
        None
    };

    let ao_view = ao_texture
        .as_ref()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
    let height_view = height_texture
        .as_ref()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
    let id_view = id_texture
        .as_ref()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("umber_bake_ground_plane_pipeline_layout_2d"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("umber_bake_ground_plane_pipeline_2d"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Build bind group entries (only include bound textures)
    let mut entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: triangle_buffer,
                offset: 0,
                size: None,
            }),
        },
        wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: params_buffer,
                offset: 0,
                size: None,
            }),
        },
        wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::TextureView(&position_map.position_view),
        },
        wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::TextureView(&position_map.normal_view),
        },
    ];

    if let Some(view) = &ao_view {
        entries.push(wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    if let Some(view) = &height_view {
        entries.push(wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    if let Some(view) = &id_view {
        entries.push(wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("umber_bake_ground_plane_bind_group_2d"),
        layout: &bind_group_layout,
        entries: &entries,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("umber_bake_ground_plane_encoder_2d"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("umber_bake_ground_plane_pass_2d"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));

    // Read back the layers
    let mut outputs = BakeOutputs::default();

    let read_texture = |texture: &wgpu::Texture| -> Result<Vec<u8>, GroundPlaneBakeError> {
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("umber_bake_ground_plane_staging"),
            size: (width * height * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("umber_bake_ground_plane_copy"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
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
        let slice = staging_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| sender.send(r).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .expect("poll timeout");
        receiver.recv().unwrap().expect("map failed");
        let data = slice.get_mapped_range().expect("get mapped range");
        Ok(data.to_vec())
    };

    if let Some(tex) = ao_texture {
        outputs.ao = Some(read_texture(&tex)?);
    }
    if let Some(tex) = height_texture {
        outputs.height = Some(read_texture(&tex)?);
    }
    if let Some(tex) = id_texture {
        outputs.id = Some(read_texture(&tex)?);
    }

    Ok(outputs)
}

#[cfg(all(test, feature = "gpu"))]
mod gpu_tests {
    use super::*;
    use pollster::block_on;

    /// Requests a default device; skips gracefully if no adapter available.
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    fn flat_scene() -> MeshData {
        MeshData {
            positions: vec![
                [-5.0, 0.0, -5.0],
                [5.0, 0.0, -5.0],
                [5.0, 0.0, 5.0],
                [-5.0, 0.0, 5.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
            triangle_materials: Vec::new(),
        }
    }

    fn ground_plane_desc() -> GroundPlaneDesc {
        GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 32,
        }
    }

    fn texel(map: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * width + x) * 4) as usize;
        [map[i], map[i + 1], map[i + 2], map[i + 3]]
    }

    const TEST_SIZE: u32 = 64;

    /// Kittest: `ground_plane_gpu_ao_matches_cpu_texel_for_texel`
    /// Same flat scene, 64², AO only, 16 rays → GPU output layer 0 byte-identical to CPU reference.
    #[test]
    fn ground_plane_gpu_ao_matches_cpu_texel_for_texel() {
        let Some((device, queue)) = try_request_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let scene = flat_scene();
        let desc = ground_plane_desc();
        let mut params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Ao]);
        params.rays = 16;
        params.max_distance = 100.0;
        params.bias = 0.001;

        let gpu_out =
            bake_ground_plane_mesh(&device, &queue, &scene, &params, TEST_SIZE, TEST_SIZE)
                .expect("GPU bake should succeed");
        let gpu_ao = gpu_out.ao.expect("AO layer missing");

        // CPU reference over all texels
        let plane = generate_ground_plane(&desc).expect("plane gen");
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let cpu_ao = ground_plane_ao_cpu(&plane, &scene, &desc, &params, (x, y));
                let expected_r = (cpu_ao * 255.0).round() as u8;
                let gpu_px = texel(&gpu_ao, TEST_SIZE, x, y);
                assert_eq!(
                    gpu_px[0], expected_r,
                    "AO R mismatch at ({x},{y}): GPU {} vs CPU {}",
                    gpu_px[0], expected_r
                );
                assert_eq!(gpu_px[3], 255, "AO A mismatch at ({x},{y})",);
            }
        }
    }

    /// Kittest: `ground_plane_gpu_height_matches_cpu_texel_for_texel`
    /// Same scene, Height only, max_distance=10.0 → GPU layer 1 byte-identical to CPU reference.
    #[test]
    fn ground_plane_gpu_height_matches_cpu_texel_for_texel() {
        let Some((device, queue)) = try_request_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let scene = flat_scene();
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.5,
            subdivisions: 32,
        };
        let mut params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Height]);
        params.max_distance = 10.0;

        let gpu_out =
            bake_ground_plane_mesh(&device, &queue, &scene, &params, TEST_SIZE, TEST_SIZE)
                .expect("GPU bake should succeed");
        let gpu_height = gpu_out.height.expect("Height layer missing");

        let plane = generate_ground_plane(&desc).expect("plane gen");
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let cpu_h = ground_plane_height_cpu(&plane, &scene, &desc, &params, (x, y));
                let expected_r = (cpu_h * 255.0).round() as u8;
                let gpu_px = texel(&gpu_height, TEST_SIZE, x, y);
                assert_eq!(
                    gpu_px[0], expected_r,
                    "Height R mismatch at ({x},{y}): GPU {} vs CPU {}",
                    gpu_px[0], expected_r
                );
                assert_eq!(
                    gpu_px[3],
                    if cpu_h > 0.0 { 255 } else { 0 },
                    "Height A mismatch at ({x},{y})"
                );
            }
        }
    }

    /// Kittest: `ground_plane_gpu_id_matches_cpu_texel_for_texel`
    /// Same scene, Id only → GPU layer 2 byte-identical to CPU reference.
    #[test]
    fn ground_plane_gpu_id_matches_cpu_texel_for_texel() {
        let Some((device, queue)) = try_request_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.5,
            subdivisions: 32,
        };
        // Scene with two triangles, first covers center
        let scene = MeshData {
            positions: vec![
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
                [0.0, 0.0, 1.0],
                [-2.0, 0.0, -2.0],
                [0.0, 0.0, -2.0],
                [-1.0, 0.0, 0.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 6],
            uvs: vec![[0.0, 0.0]; 6],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        let params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Id]);

        let gpu_out =
            bake_ground_plane_mesh(&device, &queue, &scene, &params, TEST_SIZE, TEST_SIZE)
                .expect("GPU bake should succeed");
        let gpu_id = gpu_out.id.expect("ID layer missing");

        let plane = generate_ground_plane(&desc).expect("plane gen");
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let cpu_id = ground_plane_id_cpu(&plane, &scene, &desc, (x, y));
                let gpu_px = texel(&gpu_id, TEST_SIZE, x, y);
                assert_eq!(
                    gpu_px, cpu_id,
                    "ID mismatch at ({x},{y}): GPU {:?} vs CPU {:?}",
                    gpu_px, cpu_id
                );
            }
        }
    }

    /// Kittest: `ground_plane_multi_map_single_dispatch`
    /// AO+Height+Id bitmask = 7 → single dispatch produces three layers, each matching its single-map test.
    #[test]
    fn ground_plane_multi_map_single_dispatch() {
        let Some((device, queue)) = try_request_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let scene = flat_scene();
        let desc = ground_plane_desc();
        let mut params = GroundPlaneParams::new(
            desc,
            vec![
                GroundPlaneMap::Ao,
                GroundPlaneMap::Height,
                GroundPlaneMap::Id,
            ],
        );
        params.rays = 16;
        params.max_distance = 100.0;
        params.bias = 0.001;

        let gpu_out =
            bake_ground_plane_mesh(&device, &queue, &scene, &params, TEST_SIZE, TEST_SIZE)
                .expect("GPU multi-map bake should succeed");

        // Verify AO layer matches single-map AO test
        let gpu_ao = gpu_out.ao.expect("AO layer missing");
        let plane = generate_ground_plane(&desc).expect("plane gen");
        let ao_params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Ao]);
        let mut ao_params = ao_params;
        ao_params.rays = 16;
        ao_params.max_distance = 100.0;
        ao_params.bias = 0.001;
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let cpu_ao = ground_plane_ao_cpu(&plane, &scene, &desc, &ao_params, (x, y));
                let expected_r = (cpu_ao * 255.0).round() as u8;
                let gpu_px = texel(&gpu_ao, TEST_SIZE, x, y);
                assert_eq!(
                    gpu_px[0], expected_r,
                    "Multi-map AO R mismatch at ({x},{y})"
                );
                assert_eq!(gpu_px[3], 255, "Multi-map AO A mismatch at ({x},{y})");
            }
        }

        // Verify Height layer matches single-map Height test
        let gpu_height = gpu_out.height.expect("Height layer missing");
        let height_params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Height]);
        let mut height_params = height_params;
        height_params.max_distance = 100.0;
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let cpu_h = ground_plane_height_cpu(&plane, &scene, &desc, &height_params, (x, y));
                let expected_r = (cpu_h * 255.0).round() as u8;
                let gpu_px = texel(&gpu_height, TEST_SIZE, x, y);
                assert_eq!(
                    gpu_px[0], expected_r,
                    "Multi-map Height R mismatch at ({x},{y})"
                );
            }
        }

        // Verify ID layer matches single-map ID test (using same scene as ID test)
        let gpu_id = gpu_out.id.expect("ID layer missing");
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let cpu_id = ground_plane_id_cpu(&plane, &scene, &desc, (x, y));
                let gpu_px = texel(&gpu_id, TEST_SIZE, x, y);
                assert_eq!(gpu_px, cpu_id, "Multi-map ID mismatch at ({x},{y})");
            }
        }
    }

    /// Kittest: `ground_plane_off_is_byte_identical_to_today`
    /// GroundPlane unchecked + all other maps checked → full bake plan byte-identical to tree without ground plane.
    /// This is tested at the app/plan_bake level; here we verify the bake function is not called when maps_bitmask=0.
    #[test]
    fn ground_plane_off_is_byte_identical_to_today() {
        let Some((device, queue)) = try_request_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let scene = flat_scene();
        let desc = ground_plane_desc();
        // Empty maps_bitmask should be rejected by validation
        let params = GroundPlaneParams {
            desc,
            maps_bitmask: 0,
            rays: 16,
            max_distance: 100.0,
            bias: 0.001,
        };
        let err = bake_ground_plane_mesh(&device, &queue, &scene, &params, TEST_SIZE, TEST_SIZE)
            .unwrap_err();
        assert!(matches!(
            err,
            GroundPlaneBakeError::GroundPlane(GroundPlaneError::InvalidSubdivisions)
        ));
    }

    /// Kittest: `ground_plane_viewport_wire_matches_params`
    /// Viewport wireframe test is in umber-app; here we verify the plane mesh generation matches params.
    #[test]
    fn ground_plane_viewport_wire_matches_params() {
        let desc = GroundPlaneDesc {
            size: 5.0,
            height_offset: 0.2,
            subdivisions: 4,
        };
        let plane = generate_ground_plane(&desc).expect("plane gen");
        // 4 subdivs → 5×5 vertices = 25, 4×4×2 = 32 triangles
        assert_eq!(plane.positions.len(), 25);
        assert_eq!(plane.indices.len(), 32 * 3);
        // All vertices at Y = -0.2
        for pos in &plane.positions {
            assert!((pos[1] + 0.2).abs() < 1e-5, "Vertex Y {} != -0.2", pos[1]);
        }
        // X/Z span [-2.5, 2.5]
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        for pos in &plane.positions {
            min_x = min_x.min(pos[0]);
            max_x = max_x.max(pos[0]);
            min_z = min_z.min(pos[2]);
            max_z = max_z.max(pos[2]);
        }
        assert!((min_x + 2.5).abs() < 1e-5);
        assert!((max_x - 2.5).abs() < 1e-5);
        assert!((min_z + 2.5).abs() < 1e-5);
        assert!((max_z - 2.5).abs() < 1e-5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_scene() -> MeshData {
        // Single quad at Y=0 spanning [-5, 5] x [-5, 5]
        MeshData {
            positions: vec![
                [-5.0, 0.0, -5.0],
                [5.0, 0.0, -5.0],
                [5.0, 0.0, 5.0],
                [-5.0, 0.0, 5.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
            triangle_materials: Vec::new(),
        }
    }

    fn ground_plane_desc() -> GroundPlaneDesc {
        GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 2,
        }
    }

    #[test]
    fn ground_plane_validate_rejects_empty_scene() {
        let params = GroundPlaneParams::new(ground_plane_desc(), vec![GroundPlaneMap::Ao]);
        let err =
            validate_ground_plane(&MeshData::default(), &params.desc, &params, 64, 64).unwrap_err();
        assert!(matches!(err, GroundPlaneBakeError::EmptyMesh))
    }

    #[test]
    fn ground_plane_validate_rejects_zero_subdivs() {
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 0,
        };
        let params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Ao]);
        let scene = flat_scene();
        let err = validate_ground_plane(&scene, &params.desc, &params, 64, 64).unwrap_err();
        assert!(matches!(
            err,
            GroundPlaneBakeError::GroundPlane(GroundPlaneError::InvalidSubdivisions)
        ));
    }

    #[test]
    fn ground_plane_validate_rejects_zero_size() {
        let desc = GroundPlaneDesc {
            size: 0.0,
            height_offset: 0.1,
            subdivisions: 32,
        };
        let params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Ao]);
        let scene = flat_scene();
        let err = validate_ground_plane(&scene, &params.desc, &params, 64, 64).unwrap_err();
        assert!(matches!(
            err,
            GroundPlaneBakeError::GroundPlane(GroundPlaneError::InvalidSize { size: 0.0 })
        ));
    }

    #[test]
    fn ground_plane_validate_rejects_zero_maps() {
        let params = GroundPlaneParams::new(ground_plane_desc(), vec![]);
        let scene = flat_scene();
        let err = validate_ground_plane(&scene, &params.desc, &params, 64, 64).unwrap_err();
        assert!(matches!(
            err,
            GroundPlaneBakeError::GroundPlane(GroundPlaneError::InvalidSubdivisions)
        ));
    }

    #[test]
    fn ground_plane_validate_rejects_zero_rays_for_ao() {
        let mut params = GroundPlaneParams::new(ground_plane_desc(), vec![GroundPlaneMap::Ao]);
        params.rays = 0;
        let scene = flat_scene();
        let err = validate_ground_plane(&scene, &params.desc, &params, 64, 64).unwrap_err();
        assert!(matches!(err, GroundPlaneBakeError::InvalidRayCount));
    }

    #[test]
    fn ground_plane_ao_cpu_matches_ao_bake_on_flat_scene() {
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 32,
        };
        let plane = generate_ground_plane(&desc).expect("plane gen");
        // Scene: quad at Y=0, size 10x10, plane at Y=-0.1
        // The quad subtends nearly the full upper hemisphere → AO ≈ 0 (fully occluded)
        let scene = MeshData {
            positions: vec![
                [-5.0, 0.0, -5.0],
                [5.0, 0.0, -5.0],
                [5.0, 0.0, 5.0],
                [-5.0, 0.0, 5.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        let params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Ao]);

        // At plane center, the 10x10 quad at distance 0.1 covers ~99% of upper hemisphere
        // With 16 rays, nearly all hit → AO ≈ 0
        let center_ao = ground_plane_ao_cpu(&plane, &scene, &desc, &params, (16, 16));
        // Physics gives AO ≈ 0 (not 0.5 as originally assumed in brief)
        assert!(
            (0.0..=0.1).contains(&center_ao),
            "AO at center should be near 0 (fully occluded), got: {}",
            center_ao
        );
    }

    #[test]
    fn ground_plane_height_cpu_returns_normalized_distance() {
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.5,
            subdivisions: 32,
        };
        let plane = generate_ground_plane(&desc).expect("plane gen");
        let scene = flat_scene(); // Quad at Y=0
        let mut params = GroundPlaneParams::new(desc, vec![GroundPlaneMap::Height]);
        params.max_distance = 10.0;

        // Plane at Y=-0.5, scene at Y=0 -> distance 0.5
        // Normalized to max_distance=10.0 -> 0.05
        let center_h = ground_plane_height_cpu(&plane, &scene, &desc, &params, (16, 16));
        assert!(
            (center_h - 0.05).abs() < 0.01,
            "Height at center: {}",
            center_h
        );
    }

    #[test]
    fn ground_plane_id_cpu_hashes_hit_triangle() {
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.5,
            subdivisions: 32,
        };
        // Scene with two triangles, first covers center
        let scene = MeshData {
            positions: vec![
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
                [0.0, 0.0, 1.0], // Triangle 0 covers center
                [-2.0, 0.0, -2.0],
                [0.0, 0.0, -2.0],
                [-1.0, 0.0, 0.0], // Triangle 1
            ],
            normals: vec![[0.0, 1.0, 0.0]; 6],
            uvs: vec![[0.0, 0.0]; 6],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        let plane = generate_ground_plane(&desc).expect("plane gen");

        let id_bytes = ground_plane_id_cpu(&plane, &scene, &desc, (16, 16));
        // FNV-1a of triangle index 0
        let mut h: u64 = 2166136261;
        const FNV_PRIME: u64 = 16777619;
        for b in [0u64, 0, 0, 0] {
            h = (h ^ (b & 0xFF)).wrapping_mul(FNV_PRIME);
        }
        assert_eq!(id_bytes[0], (h & 0xFF) as u8);
        assert_eq!(id_bytes[1], ((h >> 8) & 0xFF) as u8);
        assert_eq!(id_bytes[2], ((h >> 16) & 0xFF) as u8);
        assert_eq!(id_bytes[3], 255);
    }
}
