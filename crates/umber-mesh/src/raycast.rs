//! CPU ray-mesh intersection for viewport painting and picking.
//!
//! Möller–Trumbore over the triangle list — no acceleration structure
//! (the Wave-3 bake BVH will share this primitive). Sufficient for
//! per-pointer picking at interactive rates up to ~100k tris; a BVH
//! arrives with the bake engine's mesh-map path.

use crate::MeshData;
use glam::Vec3;

/// A ray hit: the intersected triangle index, distance along the ray,
/// and the barycentric hit point (for attribute interpolation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Triangle index (position in the index list / 3).
    pub triangle: usize,
    /// Ray parameter (`origin + dir * t`), always finite and positive.
    pub t: f32,
    /// Barycentric coordinates at the hit (u, v; w = 1-u-v).
    pub bary: (f32, f32),
}

/// Interpolates a per-vertex attribute at a hit via barycentrics.
///
/// `tri` is the raw 3-index slice for the hit's triangle.
pub fn interpolate_attr<T: Copy>(
    tri: &[u32],
    attrs: &[T],
    hit: RayHit,
    lerp: impl Fn(T, T, T, f32, f32) -> T,
) -> Option<T> {
    let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
    let (a, b, c) = (attrs.get(i0)?, attrs.get(i1)?, attrs.get(i2)?);
    Some(lerp(*a, *b, *c, hit.bary.0, hit.bary.1))
}

/// Casts `ray` against the mesh, returning the nearest hit.
///
/// `dir` need not be normalized; `t` is in the ray's own units.
pub fn ray_intersect(mesh: &MeshData, origin: Vec3, dir: Vec3) -> Option<RayHit> {
    let mut best: Option<RayHit> = None;
    for (tri_idx, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (Some(&p0), Some(&p1), Some(&p2)) = (
            mesh.positions.get(i0),
            mesh.positions.get(i1),
            mesh.positions.get(i2),
        ) else {
            continue;
        };
        let (v0, v1, v2) = (
            Vec3::from_array(p0),
            Vec3::from_array(p1),
            Vec3::from_array(p2),
        );
        // Möller–Trumbore.
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = dir.cross(edge2);
        let a = edge1.dot(h);
        // Parallel (or degenerate) triangle.
        if a.abs() < 1e-8 {
            continue;
        }
        let f = 1.0 / a;
        let s = origin - v0;
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
        if t <= 1e-5 {
            continue; // Behind origin or epsilon-touching.
        }
        if best.is_none_or(|b| t < b.t) {
            best = Some(RayHit {
                triangle: tri_idx,
                t,
                bary: (u, v),
            });
        }
    }
    best
}

/// The hit triangle's geometric (face) normal, normalized, from the
/// winding `p0 -> p1 -> p2` (counter-clockwise = front, the renderer's
/// convention). `None` for a degenerate triangle or an index past the
/// mesh — never a NaN.
pub fn face_normal(mesh: &MeshData, hit: RayHit) -> Option<Vec3> {
    let tri = mesh.indices.chunks_exact(3).nth(hit.triangle)?;
    let p = |k: usize| {
        mesh.positions
            .get(tri[k] as usize)
            .map(|v| Vec3::from_array(*v))
    };
    let (p0, p1, p2) = (p(0)?, p(1)?, p(2)?);
    let n = (p1 - p0).cross(p2 - p0);
    let len = n.length();
    (len > 1e-12 && len.is_finite()).then(|| n / len)
}

/// Front-face falloff (absorbed 2026-10-14 from Blender's brush "Normal
/// Falloff"): the dab-strength weight for a surface seen at an angle.
///
/// `facing` is `|cos θ|` between the view ray and the face normal (1 =
/// head-on, 0 = edge-on). Faces within `start_deg` of head-on paint at
/// full strength; past it the weight falls linearly in ANGLE to 0 at 90°,
/// so grazing faces stop taking stretched, smeared dabs. `start_deg` is
/// clamped to `0..=90`; `90` disables the falloff (weight 1 everywhere);
/// a non-finite `facing` gives `1.0` (never silently kill a stroke).
pub fn front_facing_weight(facing: f32, start_deg: f32) -> f32 {
    if !facing.is_finite() {
        return 1.0;
    }
    let start = if start_deg.is_finite() {
        start_deg.clamp(0.0, 90.0)
    } else {
        90.0
    };
    if start >= 90.0 {
        return 1.0;
    }
    let angle = facing.abs().clamp(0.0, 1.0).acos().to_degrees();
    if angle <= start {
        1.0
    } else {
        ((90.0 - angle) / (90.0 - start)).clamp(0.0, 1.0)
    }
}

/// The UV at a hit, barycentrically interpolated.
pub fn uv_at(mesh: &MeshData, hit: RayHit) -> Option<[f32; 2]> {
    let tri = mesh.indices.chunks_exact(3).nth(hit.triangle)?;
    interpolate_attr(tri, &mesh.uvs, hit, |a, b, c, u, v| {
        let w = 1.0 - u - v;
        [
            a[0] * w + b[0] * u + c[0] * v,
            a[1] * w + b[1] * u + c[1] * v,
        ]
    })
}

/// Samples a 3D curve on the mesh surface into path UVs (the path tool's
/// "3D curve" source): one `(origin, dir)` ray per curve sample, each
/// resolving through [`ray_intersect`] + [`uv_at`].
///
/// Rays that miss (a drag across a silhouette, a gap in the mesh) or hit
/// a triangle with no UVs contribute NO point rather than aborting the
/// walk: a partial curve still replays, and the caller (`PaintPath`) caps
/// the length anyway. `dir` need not be normalized (the
/// [`ray_intersect`] contract).
pub fn walk_surface_path(mesh: &MeshData, rays: &[([f32; 3], [f32; 3])]) -> Vec<[f32; 2]> {
    rays.iter()
        .filter_map(|(origin, dir)| {
            let hit = ray_intersect(mesh, Vec3::from_array(*origin), Vec3::from_array(*dir))?;
            uv_at(mesh, hit)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_facing_weight_is_one_head_on_and_zero_edge_on() {
        // Head-on and within the start angle: full strength.
        assert_eq!(front_facing_weight(1.0, 60.0), 1.0);
        // Inside the start angle (50 deg < 60). Not AT 60: cos -> acos does
        // not round-trip exactly in f32 (60 comes back 60.000004), so the
        // boundary itself is not a stable pin.
        assert_eq!(front_facing_weight(50f32.to_radians().cos(), 60.0), 1.0);
        // Edge-on: nothing.
        assert_eq!(front_facing_weight(0.0, 60.0), 0.0);
        // Halfway through the falloff band (75 deg with start 60): 0.5.
        let w = front_facing_weight(75f32.to_radians().cos(), 60.0);
        assert!((w - 0.5).abs() < 1e-4, "{w}");
        // The sign of cos is ignored (back-facing hit through an open mesh).
        assert_eq!(
            front_facing_weight(-0.5, 30.0),
            front_facing_weight(0.5, 30.0)
        );
        // 90 = disabled; garbage never kills a stroke.
        assert_eq!(front_facing_weight(0.0, 90.0), 1.0);
        assert_eq!(front_facing_weight(f32::NAN, 60.0), 1.0);
        assert_eq!(front_facing_weight(0.0, f32::NAN), 1.0);
        // start 0: linear in angle over the whole quarter turn.
        let w = front_facing_weight(45f32.to_radians().cos(), 0.0);
        assert!((w - 0.5).abs() < 1e-4, "{w}");
    }

    #[test]
    fn face_normal_follows_winding_and_refuses_degenerates() {
        let mesh = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [2.0, 0.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            // tri 0: CCW in xy -> +z; tri 1: collinear (degenerate).
            indices: vec![0, 1, 2, 0, 1, 3],
            material_names: vec![],
        };
        let hit = |triangle| RayHit {
            triangle,
            t: 1.0,
            bary: (0.3, 0.3),
        };
        assert_eq!(face_normal(&mesh, hit(0)), Some(Vec3::Z));
        assert_eq!(face_normal(&mesh, hit(1)), None);
        assert_eq!(face_normal(&mesh, hit(7)), None);
    }

    fn quad() -> MeshData {
        // Two-triangle quad on the z=0 plane, unit UVs.
        MeshData {
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
        }
    }

    #[test]
    fn center_hit_returns_center_uv() {
        let mesh = quad();
        let hit = ray_intersect(&mesh, Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0))
            .expect("quad must be hit from +z");
        let uv = uv_at(&mesh, hit).expect("uv present");
        assert!((uv[0] - 0.5).abs() < 1e-5 && (uv[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn corner_hits_map_to_corner_uvs() {
        let mesh = quad();
        // Aim at vertex 1's position (1,-1): its UV is (1,0).
        let hit = ray_intersect(&mesh, Vec3::new(1.0, -1.0, 5.0), Vec3::new(0.0, 0.0, -1.0))
            .expect("corner hit");
        let uv = uv_at(&mesh, hit).expect("uv");
        assert!((uv[0] - 1.0).abs() < 1e-5 && (uv[1] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn miss_returns_none() {
        let mesh = quad();
        assert!(ray_intersect(&mesh, Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 1.0, 0.0)).is_none());
    }

    #[test]
    fn nearest_hit_wins_with_occluder() {
        let mut mesh = quad();
        // A second quad closer to the ray origin (z = 2).
        let base = mesh.positions.len() as u32;
        for p in [
            [-1.0, -1.0, 2.0],
            [1.0, -1.0, 2.0],
            [1.0, 1.0, 2.0],
            [-1.0, 1.0, 2.0],
        ] {
            mesh.positions.push(p);
            mesh.normals.push([0.0, 0.0, 1.0]);
            mesh.uvs.push([0.5, 0.5]);
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        let hit =
            ray_intersect(&mesh, Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0)).expect("hit");
        assert_eq!(hit.triangle, 2, "the z=2 quad (second pair) must win");
    }

    #[test]
    fn surface_walk_keeps_hits_and_skips_misses() {
        // A 3-sample curve over the quad: center hits, a parallel ray
        // misses, a corner hits. The walk keeps the two UVs in order and
        // drops the miss without aborting.
        let mesh = quad();
        let rays = [
            ([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
            ([0.0, 0.0, 5.0], [0.0, 1.0, 0.0]),
            ([1.0, -1.0, 5.0], [0.0, 0.0, -1.0]),
        ];
        let uvs = walk_surface_path(&mesh, &rays);
        assert_eq!(uvs.len(), 2, "the miss contributes no point");
        assert!((uvs[0][0] - 0.5).abs() < 1e-5 && (uvs[0][1] - 0.5).abs() < 1e-5);
        assert!((uvs[1][0] - 1.0).abs() < 1e-5 && (uvs[1][1] - 0.0).abs() < 1e-5);
        assert!(walk_surface_path(&mesh, &[]).is_empty());
        assert!(walk_surface_path(&MeshData::default(), &rays).is_empty());
    }
}
