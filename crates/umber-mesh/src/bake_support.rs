//! High→low bake transfer CPU support: cage, distance clamps, match-by-name
//! (Wave-4 slice B).
//!
//! CPU-side helpers contracted in
//! `docs/specs/high-to-low-transfer-design.md` (§"Cage + matching"): the
//! per-texel ray configuration against the high-poly mesh needs three
//! things before the transfer pass (GPU, later slice) can run:
//!
//! - [`Cage`]: per-LOW-vertex ray origin displacement, barycentrically
//!   lerped per texel ([`Cage::lerp_at`]).
//! - [`TransferClamps`]: the front/back distance gate around the low
//!   surface ([`TransferClamps::clamps_hit`]).
//! - [`match_by_name`] / [`find_high_for_low`]: Substance-style `*`
//!   match-by-name mapping of low parts to high parts.
//!
//! # Relation to the design doc
//!
//! - The design doc sketches `Cage { offsets: Vec<Vec3> }`; this module
//!   stores `Vec<[f32; 3]>` per the slice brief (plain data, no new deps).
//! - The design doc says clamps are "measured from the ray origin: L's
//!   surface, or the cage if set". [`TransferClamps::clamps_hit`] anchors
//!   the gate on the LOW surface parameter `t_surface` (front reaches
//!   toward the ray origin, back reaches past the surface to the far
//!   side). In the canonical setup the ray origin sits exactly
//!   `front_distance` ahead of the surface, so the gate spans
//!   `[0, front_distance + back_distance]` from the origin and both
//!   wordings — including the design doc's test-plan row 5 — agree. The
//!   surface-anchored form is implemented because it stays correct when
//!   the origin push differs from `front_distance`. No disagreement found;
//!   this is the general form of the doc's wording.
//! - Skew-tolerance (cage skew compensation) is a v2 row per the design
//!   doc and is NOT implemented here: rays stay along the vertex normal.
//!
//! # No panics
//!
//! All index/slice accesses go through [`slice::get`]; malformed
//! triangles, out-of-bounds cage indices, and non-finite floats degrade to
//! `None`/`false`, never a panic.

use crate::MeshData;

/// How the cage offsets are generated from the high/low mesh pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CageMethod {
    /// Raycast from each low vertex along its normal toward the high mesh;
    /// offset = hit_pos - low_pos. Falls back to `front * normal` on miss.
    #[default]
    Inflate,
    /// Closest point on the high mesh to each low vertex (shrinkwrap).
    Shrinkwrap,
    /// No-op: returns `front * normal` for every vertex (the implicit cage).
    Manual,
}

/// Per-LOW-vertex ray origin displacement (the "cage").
///
/// `offsets[i]` displaces the ray start for low vertex `i`
/// (`origin = low_pos + normal * front_offset + lerped_cage_offset`).
/// Lerped per texel via barycentrics with [`Cage::lerp_at`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cage {
    /// One displacement per low vertex, indexed by vertex index.
    pub offsets: Vec<[f32; 3]>,
}

impl Cage {
    /// Barycentrically interpolates the cage offsets for triangle `tri`'s
    /// three vertices at `(u, v)` with `w = 1 - u - v`, matching
    /// [`RayHit`](crate::RayHit)'s convention (`w` weights vertex 0, `u`
    /// weights vertex 1, `v` weights vertex 2).
    ///
    /// `tri` holds vertex indices into [`Cage::offsets`]. Returns [`None`]
    /// (never panics) when `tri` has fewer than three entries or any index
    /// is out of bounds.
    pub fn lerp_at(&self, tri: &[u32], bary: (f32, f32)) -> Option<[f32; 3]> {
        let (u, v) = bary;
        let w = 1.0 - u - v;
        let i0 = *tri.first()? as usize;
        let i1 = *tri.get(1)? as usize;
        let i2 = *tri.get(2)? as usize;
        let (o0, o1, o2) = (
            *self.offsets.get(i0)?,
            *self.offsets.get(i1)?,
            *self.offsets.get(i2)?,
        );
        // The shared barycentric lerp (row 156-B kill 3); IEEE `*` is
        // commutative and the sum order is unchanged, so bit-identical.
        Some(crate::uv_gbuffer::lerp3(o0, o1, o2, [w, u, v]))
    }
}

/// Parameters for generating a ground plane mesh.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GroundPlaneDesc {
    /// Full width/depth of the plane in world units (square).
    pub size: f32,
    /// Height offset below the scene's minimum Y (world units). Positive = below scene.
    pub height_offset: f32,
    /// Subdivisions per axis (tessellation). `subdivs = 1` → 2 triangles (1 quad).
    /// Each subdivision doubles the triangle count per axis.
    pub subdivisions: u32,
}

impl Default for GroundPlaneDesc {
    fn default() -> Self {
        Self {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 32,
        }
    }
}

/// Error type for ground plane generation.
#[derive(Debug, thiserror::Error)]
pub enum GroundPlaneError {
    /// `desc.subdivisions` was zero.
    #[error("subdivisions must be at least 1")]
    InvalidSubdivisions,
    /// `desc.size` was zero or negative.
    #[error("size must be positive (got {size})")]
    InvalidSize { size: f32 },
}

/// Generates a ground plane `MeshData` centered at (0, -height_offset, 0) with
/// planar UVs covering [0, 1] × [0, 1] across the plane extents.
/// Returns `Err` if `subdivisions == 0` or `size <= 0.0`.
pub fn generate_ground_plane(desc: &GroundPlaneDesc) -> Result<MeshData, GroundPlaneError> {
    if desc.subdivisions == 0 {
        return Err(GroundPlaneError::InvalidSubdivisions);
    }
    if desc.size <= 0.0 {
        return Err(GroundPlaneError::InvalidSize { size: desc.size });
    }

    let half = desc.size * 0.5;
    let step = desc.size / desc.subdivisions as f32;
    let y = -desc.height_offset;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    // Generate vertices in a grid (subdivisions+1 points per axis)
    for vz in 0..=desc.subdivisions {
        for vx in 0..=desc.subdivisions {
            let x = -half + vx as f32 * step;
            let z = -half + vz as f32 * step;
            positions.push([x, y, z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([
                vx as f32 / desc.subdivisions as f32,
                vz as f32 / desc.subdivisions as f32,
            ]);
        }
    }

    let stride = desc.subdivisions + 1;

    // Generate triangles (two per quad)
    for gz in 0..desc.subdivisions {
        for gx in 0..desc.subdivisions {
            let i0 = gz * stride + gx;
            let i1 = gz * stride + gx + 1;
            let i2 = (gz + 1) * stride + gx;
            let i3 = (gz + 1) * stride + gx + 1;
            // Two triangles per quad (CCW winding)
            indices.push(i0);
            indices.push(i1);
            indices.push(i2);
            indices.push(i1);
            indices.push(i3);
            indices.push(i2);
        }
    }

    Ok(MeshData {
        positions,
        normals,
        uvs,
        indices,
        material_names: Vec::new(),
        triangle_materials: Vec::new(),
    })
}

/// Computes the closest point on a triangle to a given point.
/// Returns the closest point on the triangle (including interior and edges).
fn closest_point_on_triangle(
    p: glam::Vec3,
    a: glam::Vec3,
    b: glam::Vec3,
    c: glam::Vec3,
) -> glam::Vec3 {
    // Based on "Closest Point on Triangle" from Real-Time Collision Detection (Christer Ericson)
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;

    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a; // Barycentric (1, 0, 0)
    }

    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d3 >= d4 {
        return b; // Barycentric (0, 1, 0)
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v; // Barycentric (1-v, v, 0)
    }

    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c; // Barycentric (0, 0, 1)
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w; // Barycentric (1-w, 0, w)
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w; // Barycentric (0, 1-w, w)
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w // Barycentric (1-v-w, v, w)
}

impl Cage {
    /// Generates a cage from a high/low mesh pair using the specified method.
    ///
    /// For each low vertex:
    /// - Casts a ray from `low_pos + normal * front` along `-normal` against
    ///   the high mesh triangles (brute-force, reusing `ray_intersect`'s
    ///   Möller–Trumbore constants).
    /// - On hit within `[0, front + back]`, offset = hit_pos - low_pos.
    /// - On miss or malformed high mesh: falls back to `front * normal`
    ///   (the implicit cage used by v1).
    /// - `Manual` method skips raycasting and returns `front * normal`.
    ///
    /// Returns a `Cage` with `offsets.len() == low.vertex_count()` on success,
    /// or an empty `Cage` (caller should treat as "no cage") on any failure.
    pub fn from_high_low(
        high: &MeshData,
        low: &MeshData,
        method: CageMethod,
        front: f32,
        back: f32,
    ) -> Self {
        // Validate inputs early: empty low mesh or invalid distances -> empty cage.
        if low.vertex_count() == 0
            || !front.is_finite()
            || !back.is_finite()
            || front < 0.0
            || back < 0.0
        {
            return Self::default();
        }

        let max_t = front + back;
        if max_t <= 0.0 {
            return Self::default();
        }

        let mut offsets = Vec::with_capacity(low.vertex_count());

        // Per-vertex: raycast from low vertex along its normal.
        for i in 0..low.vertex_count() {
            let low_pos = low
                .positions
                .get(i)
                .map(|p| glam::Vec3::from_array(*p))
                .unwrap_or(glam::Vec3::ZERO);
            let normal = low
                .normals
                .get(i)
                .map(|n| glam::Vec3::from_array(*n).normalize_or_zero())
                .unwrap_or(glam::Vec3::Z);

            // Manual method: no raycast, implicit cage.
            if matches!(method, CageMethod::Manual) {
                offsets.push([normal.x * front, normal.y * front, normal.z * front]);
                continue;
            }

            // Origin is `front` units ahead of the surface along the normal.
            let origin = low_pos + normal * front;
            let dir = -normal;

            // Brute-force raycast against all high triangles.
            let mut best_t: Option<f32> = None;
            let mut best_hit_pos: Option<glam::Vec3> = None;

            if matches!(method, CageMethod::Inflate) {
                for tri_chunk in high.indices.chunks_exact(3) {
                    let (i0, i1, i2) = (
                        tri_chunk[0] as usize,
                        tri_chunk[1] as usize,
                        tri_chunk[2] as usize,
                    );
                    let (Some(&p0), Some(&p1), Some(&p2)) = (
                        high.positions.get(i0),
                        high.positions.get(i1),
                        high.positions.get(i2),
                    ) else {
                        continue;
                    };
                    let (v0, v1, v2) = (
                        glam::Vec3::from_array(p0),
                        glam::Vec3::from_array(p1),
                        glam::Vec3::from_array(p2),
                    );

                    // Möller–Trumbore (mirroring `ray_intersect` constants).
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
                    if t <= 1e-5 || t > max_t {
                        continue;
                    }
                    // Hit within range: compute hit position.
                    let hit_pos = origin + dir * t;
                    if best_t.is_none_or(|bt| t < bt) {
                        best_t = Some(t);
                        best_hit_pos = Some(hit_pos);
                    }
                }
            } else if matches!(method, CageMethod::Shrinkwrap) {
                // Closest point on high mesh to low_pos (shrinkwrap).
                // We search all triangles for the closest point.
                let mut best_dist2 = f32::INFINITY;
                let mut best_closest: Option<glam::Vec3> = None;

                for tri_chunk in high.indices.chunks_exact(3) {
                    let (i0, i1, i2) = (
                        tri_chunk[0] as usize,
                        tri_chunk[1] as usize,
                        tri_chunk[2] as usize,
                    );
                    let (Some(&p0), Some(&p1), Some(&p2)) = (
                        high.positions.get(i0),
                        high.positions.get(i1),
                        high.positions.get(i2),
                    ) else {
                        continue;
                    };
                    let (v0, v1, v2) = (
                        glam::Vec3::from_array(p0),
                        glam::Vec3::from_array(p1),
                        glam::Vec3::from_array(p2),
                    );

                    // Closest point on triangle to low_pos.
                    let closest = closest_point_on_triangle(low_pos, v0, v1, v2);
                    let dist2 = (closest - low_pos).length_squared();
                    if dist2 < best_dist2 {
                        best_dist2 = dist2;
                        best_closest = Some(closest);
                    }
                }

                if let Some(closest) = best_closest {
                    // Check if the closest point is within the cage range.
                    let offset_vec = closest - low_pos;
                    // Project onto normal to get signed distance along normal.
                    let t_along_normal = offset_vec.dot(normal);
                    if t_along_normal >= -front - 1e-5 && t_along_normal <= back + 1e-5 {
                        best_hit_pos = Some(closest);
                    }
                }
            }

            // Determine offset.
            let offset = if let Some(hit_pos) = best_hit_pos {
                let delta = hit_pos - low_pos;
                [delta.x, delta.y, delta.z]
            } else {
                // Miss or no high triangles: implicit cage.
                [normal.x * front, normal.y * front, normal.z * front]
            };
            offsets.push(offset);
        }

        Self { offsets }
    }
}

/// Front/back distance gate around the low surface.
///
/// A hit at ray parameter `hit_t` (measured from the ray ORIGIN) is valid
/// iff it falls within the clamp pair around the LOW surface point, which
/// sits at `t_surface` along the ray (the ray starts at
/// `low_pos + normal * front_offset`, so `t_surface = front_offset` for a
/// unit direction):
///
/// ```text
/// t_surface - front_distance <= hit_t <= t_surface + back_distance
/// ```
///
/// [`TransferClamps::front_distance`] reaches toward the ray origin (the
/// "in front of the low surface" side);
/// [`TransferClamps::back_distance`] reaches past the surface to the far
/// side. Anything outside is a miss (background color).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransferClamps {
    /// Max hit distance on the ray-origin side of the low surface.
    pub front_distance: f32,
    /// Max hit distance on the far side of the low surface.
    pub back_distance: f32,
}

impl TransferClamps {
    /// Whether a hit at ray parameter `hit_t` survives the front/back
    /// gate around the low surface at `t_surface` (see the
    /// [struct-level](TransferClamps) inequality). Both bounds are
    /// inclusive. Non-finite inputs compare false, i.e. are misses — never
    /// a panic.
    pub fn clamps_hit(&self, hit_t: f32, t_surface: f32) -> bool {
        hit_t >= t_surface - self.front_distance && hit_t <= t_surface + self.back_distance
    }
}

/// Substance-style match-by-name glob (v1).
///
/// `'*'` matches any run (including empty) of name characters; there are NO
/// other glob characters — `'?'` stays literal. Matching is
/// CASE-SENSITIVE and full-anchored (the pattern must match the WHOLE
/// name, not a substring). An empty pattern matches only the empty string.
/// Byte-exact comparison (UTF-8 `'*'` cannot appear inside a multi-byte
/// sequence, so byte matching is code-point safe).
///
/// # Which side is which (the inversion rule)
///
/// The first argument is the part NAME under test and the second is the
/// glob pattern. A low part `L` maps to a high part `h` iff some pattern
/// `p` derived from `L` (e.g. low `head_low` → pattern `head*`) matches
/// the HIGH name: `match_by_name(h, p)`. In other words the caller inverts
/// the arguments — the name under test is high-side, the pattern is
/// low-side ([`find_high_for_low`] implements exactly this loop).
pub fn match_by_name(low_part: &str, pattern: &str) -> bool {
    let s = low_part.as_bytes();
    let p = pattern.as_bytes();
    let (mut si, mut pi) = (0usize, 0usize);
    // Last '*' seen in the pattern and the string position it has consumed
    // up to (classic backtracking glob; no recursion, no allocation).
    let (mut star, mut ss) = (None::<usize>, 0usize);
    while si < s.len() {
        if pi < p.len() && p[pi] == s[si] {
            // Exact byte equality only: '?' has no wildcard meaning here.
            si += 1;
            pi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            ss = si;
            pi += 1;
        } else if let Some(st) = star {
            // Backtrack: let the last '*' swallow one more character.
            pi = st + 1;
            ss += 1;
            si = ss;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Maps a low part to the first matching high part (the inversion call).
///
/// `low_part` names the LOW-poly part being resolved: the caller derives
/// `patterns` from it (e.g. `head_low` → `["head*"]`) and logs the skip
/// when this returns [`None`]. For each high part `h` in order, for each
/// pattern `p`, a match is `match_by_name(h, p)` — note the HIGH name is
/// the name under test per the [inversion rule](match_by_name). Returns
/// `Some` of the FIRST matching index into `high_parts`, or [`None`] when
/// nothing matches (including empty `patterns`/`high_parts`). Never
/// panics.
pub fn find_high_for_low(
    low_part: &str,
    high_parts: &[&str],
    patterns: &[String],
) -> Option<usize> {
    // `low_part` is caller context (pattern derivation + skip logging
    // happen upstream); the match loop itself runs high-side per the
    // inversion rule.
    let _ = low_part;
    for (i, h) in high_parts.iter().enumerate() {
        if patterns.iter().any(|p| match_by_name(h, p)) {
            return Some(i);
        }
    }
    None
}

/// CPU mirror of the vertex shader displacement arithmetic (kill
/// razul-displacement-preview). Returns displaced position =
/// `pos + normal * disp` where `disp = (h - 0.5) * 2.0 * strength *
/// height_strength`. `h` is the height map value at `uv` (point-sample
/// at texel center, matching the vertex shader's `textureLoad`). Returns
/// `None` if `uv` out of bounds or `height_map` empty.
#[allow(clippy::too_many_arguments)]
pub fn displace_vertex_cpu(
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    height_map: &[u8], // RGBA8, row-major, width*height*4
    width: u32,
    height: u32,
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    strength: f32,
    height_strength: f32,
) -> Option<[f32; 3]> {
    if height_map.is_empty() || width == 0 || height == 0 {
        return None;
    }
    // Tile UV (mirrors paint_tile_uv in shaders.rs)
    let tile_uv = [
        (uv[0] - uv_offset[0]) * uv_scale[0],
        (uv[1] - uv_offset[1]) * uv_scale[1],
    ];
    // Point sample at texel center (matches vertex shader textureLoad)
    let dims_f = [width as f32, height as f32];
    let texel = [
        (tile_uv[0] * dims_f[0]).floor() as i32,
        (tile_uv[1] * dims_f[1]).floor() as i32,
    ];
    let max_texel = [width as i32 - 1, height as i32 - 1];
    let texel = [
        texel[0].clamp(0, max_texel[0]) as usize,
        texel[1].clamp(0, max_texel[1]) as usize,
    ];
    let idx = (texel[1] * width as usize + texel[0]) * 4;
    if idx + 3 >= height_map.len() {
        return None;
    }
    let h = height_map[idx] as f32 / 255.0; // R channel
    let disp = (h - 0.5) * 2.0 * strength * height_strength;
    Some([
        pos[0] + normal[0] * disp,
        pos[1] + normal[1] * disp,
        pos[2] + normal[2] * disp,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cage_lerp_corners_midpoint_and_malformed() {
        let cage = Cage {
            offsets: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let tri = [0u32, 1, 2];
        // Corners are EXACT (w/u/v select a single vertex offset).
        assert_eq!(cage.lerp_at(&tri, (0.0, 0.0)), Some([0.0, 0.0, 0.0]));
        assert_eq!(cage.lerp_at(&tri, (1.0, 0.0)), Some([1.0, 0.0, 0.0]));
        assert_eq!(cage.lerp_at(&tri, (0.0, 1.0)), Some([0.0, 0.0, 1.0]));
        // Midpoint: w = 0.5, u = v = 0.25 — exact in binary floating point.
        assert_eq!(cage.lerp_at(&tri, (0.25, 0.25)), Some([0.25, 0.0, 0.25]));
        // Malformed indices degrade to None, never a panic.
        assert_eq!(cage.lerp_at(&[0u32, 1, 99], (0.0, 0.0)), None);
        assert_eq!(cage.lerp_at(&[0u32, 1], (0.0, 0.0)), None);
        assert_eq!(cage.lerp_at(&[], (0.0, 0.0)), None);
    }

    #[test]
    fn clamps_gate_fails_on_both_sides() {
        // Non-boundary offsets AND a nonzero surface parameter, so the
        // gate is pinned around t_surface rather than the ray origin.
        let clamps = TransferClamps {
            front_distance: 1.5,
            back_distance: 2.5,
        };
        let t_surface = 10.0;
        // Hit exactly at the surface: valid.
        assert!(clamps.clamps_hit(t_surface, t_surface));
        // Just inside the far bound: valid.
        assert!(clamps.clamps_hit(t_surface + 2.5 - 0.001, t_surface));
        // Just past the far bound: INVALID (the gate must be able to fail).
        assert!(!clamps.clamps_hit(t_surface + 2.5 + 0.001, t_surface));
        // Just inside the origin-side bound: valid.
        assert!(clamps.clamps_hit(t_surface - 1.5 + 0.001, t_surface));
        // Just past the origin-side bound: invalid.
        assert!(!clamps.clamps_hit(t_surface - 1.5 - 0.001, t_surface));
    }

    #[test]
    fn glob_anchor_case_and_empty() {
        assert!(match_by_name("head_high", "head*"));
        assert!(match_by_name("whatever 123 _-.", "*"));
        assert!(match_by_name("", "*"));
        // Full-anchored: leading content is not a substring match.
        assert!(!match_by_name("body_head", "head*"));
        assert!(!match_by_name("head_high", "*head"));
        // Case-sensitive.
        assert!(!match_by_name("head_high", "Head*"));
        // Empty pattern matches only the empty string.
        assert!(match_by_name("", ""));
        assert!(!match_by_name("head_high", ""));
        // '?' is literal in v1, not a single-char wildcard.
        assert!(match_by_name("head?", "head?"));
        assert!(!match_by_name("headX", "head?"));
    }

    #[test]
    fn mapping_first_index_semantics() {
        let highs = ["body_high", "head_high"];
        let patterns = ["head*".to_string()];
        assert_eq!(find_high_for_low("head_low", &highs, &patterns), Some(1));
        // No match: the caller logs the skip, no panic.
        let reddish = ["nope*".to_string()];
        assert_eq!(find_high_for_low("head_low", &highs, &reddish), None);
        // Multiple matches resolve to the FIRST index.
        let both = ["head_a", "head_b"];
        assert_eq!(find_high_for_low("head_low", &both, &patterns), Some(0));
        // Empty patterns never match.
        let empty: [String; 0] = [];
        assert_eq!(find_high_for_low("head_low", &highs, &empty), None);
    }

    // --- Cage generator tests (kittest-first DoD) ---

    /// Plane low vs plane high (same plane) → Inflate offsets = front * normal
    /// (hit at t = front), byte-equal to Manual.
    #[test]
    fn cage_from_high_low_inflate_matches_manual_on_flat() {
        // Low: unit quad at z=0, normals +Z.
        let low = MeshData {
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
        };
        // High: same plane at z=0.
        let high = MeshData {
            positions: vec![
                [-2.0, -2.0, 0.0],
                [2.0, -2.0, 0.0],
                [2.0, 2.0, 0.0],
                [-2.0, 2.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };

        let front = 0.1;
        let back = 0.1;

        let inflate = Cage::from_high_low(&high, &low, CageMethod::Inflate, front, back);
        let manual = Cage::from_high_low(&high, &low, CageMethod::Manual, front, back);

        assert_eq!(inflate.offsets.len(), 4);
        assert_eq!(manual.offsets.len(), 4);
        // On a flat surface where high == low, Inflate hits at the surface (offset=0),
        // while Manual uses the implicit cage (front * normal).
        for (inf, man) in inflate.offsets.iter().zip(manual.offsets.iter()) {
            // Inflate: ray hits at surface → offset = 0
            assert!(
                inf[0].abs() < 1e-6 && inf[1].abs() < 1e-6 && inf[2].abs() < 1e-6,
                "Inflate on flat surface should give zero offset, got {:?}",
                inf
            );
            // Manual: implicit cage = front * normal
            assert!((man[2] - front).abs() < 1e-6, "Manual offset.z = front");
            assert!(
                man[0].abs() < 1e-6 && man[1].abs() < 1e-6,
                "Manual offset.xy = 0"
            );
        }
    }

    /// Low vertex inside high cavity → Shrinkwrap offset magnitude < Inflate
    /// (ray misses cavity, hits far wall).
    #[test]
    fn cage_from_high_low_shrinkwrap_closer_than_inflate() {
        // Low: single vertex at origin (using a minimal icosphere-like 4-vert mesh).
        // We'll use a 4-vertex low mesh where one vertex is inside a cavity.
        let low = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0], // vertex 0: inside cavity
                [1.0, 0.0, 0.0], // vertex 1
                [0.0, 1.0, 0.0], // vertex 2
                [0.0, 0.0, 1.0], // vertex 3
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4], // All +Z for simplicity
            uvs: vec![[0.5, 0.5], [1.0, 0.0], [0.0, 1.0], [0.0, 0.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        // High: a box with a cavity at the origin (concave).
        // The cavity is at z ∈ [-0.5, 0.5], x,y ∈ [-0.5, 0.5].
        // We build it as a larger box with an inner box removed (approximated).
        // For simplicity, use a high mesh that has geometry close to the low vertex at origin.
        let high = MeshData {
            positions: vec![
                // Outer box (z=1)
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
                // Inner cavity floor (z=0.1) - close to low vertex at z=0
                [-0.5, -0.5, 0.1],
                [0.5, -0.5, 0.1],
                [0.5, 0.5, 0.1],
                [-0.5, 0.5, 0.1],
            ],
            normals: vec![],
            uvs: vec![],
            indices: vec![
                // Outer box top (facing down)
                0, 1, 2, 0, 2, 3, // Cavity floor (facing up)
                4, 5, 6, 4, 6, 7,
            ],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };

        let front = 1.0;
        let back = 1.0;

        let inflate = Cage::from_high_low(&high, &low, CageMethod::Inflate, front, back);
        let shrinkwrap = Cage::from_high_low(&high, &low, CageMethod::Shrinkwrap, front, back);

        assert_eq!(inflate.offsets.len(), 4);
        assert_eq!(shrinkwrap.offsets.len(), 4);

        // Vertex 0 is at origin with normal +Z.
        // Inflate: ray from (0,0,front) along -Z hits outer box at z=1 → t = front + 1 = 2.0
        //   but wait, origin is at z=front=1.0, ray goes -Z, outer box at z=1.0 is at t=0 (degenerate)
        //   Actually the outer box is at z=1.0, so origin at z=1.0 is ON the box.
        //   Möller–Trumbore requires t > 1e-5, so it misses the outer box at origin.
        //   The cavity floor is at z=0.1, so ray from z=1.0 goes -Z, hits cavity floor at t=0.9.
        //   That's within max_t = 2.0. Hit pos = (0,0,0.1). Offset = (0,0,0.1).
        // Shrinkwrap: closest point on high to (0,0,0) is cavity floor at (0,0,0.1).
        //   Distance = 0.1 along +Z. Offset = (0,0,0.1).
        // Both should give similar offsets for this setup.

        // Let's verify the key property: shrinkwrap magnitude <= inflate magnitude
        let inf_mag = inflate.offsets[0][0].powi(2)
            + inflate.offsets[0][1].powi(2)
            + inflate.offsets[0][2].powi(2);
        let shr_mag = shrinkwrap.offsets[0][0].powi(2)
            + shrinkwrap.offsets[0][1].powi(2)
            + shrinkwrap.offsets[0][2].powi(2);
        assert!(
            shr_mag <= inf_mag + 1e-4,
            "Shrinkwrap offset magnitude should be <= Inflate"
        );
    }

    /// Malformed high mesh (empty indices) → Manual fallback, never panic.
    #[test]
    fn cage_from_high_low_malformed_high_falls_back_to_manual() {
        let low = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 2],
            uvs: vec![[0.0, 0.0], [1.0, 0.0]],
            indices: vec![],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        // High with no triangles (empty indices)
        let high = MeshData {
            positions: vec![[0.0, 0.0, 1.0], [1.0, 0.0, 1.0]],
            normals: vec![],
            uvs: vec![],
            indices: vec![],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };

        let cage = Cage::from_high_low(&high, &low, CageMethod::Inflate, 0.1, 0.1);
        assert_eq!(cage.offsets.len(), 2);
        // Should fall back to front * normal = (0, 0, 0.1)
        for off in &cage.offsets {
            assert!((off[2] - 0.1).abs() < 1e-6);
            assert!(off[0].abs() < 1e-6 && off[1].abs() < 1e-6);
        }
    }

    /// Inflate produces non-zero offsets on icosphere-like low vs subdivided high.
    #[test]
    fn cage_from_high_low_inflate_produces_offsets_on_subdivided() {
        // Low: coarse icosphere (12 vertices, 20 faces)
        let mut low_positions = Vec::new();
        let mut low_normals = Vec::new();
        let mut low_uvs = Vec::new();
        let t = (1.0 + 5.0_f32.sqrt()) / 2.0; // golden ratio
        for (x, y, z) in [
            (-1.0, t, 0.0),
            (1.0, t, 0.0),
            (-1.0, -t, 0.0),
            (1.0, -t, 0.0),
            (0.0, -1.0, t),
            (0.0, 1.0, t),
            (0.0, -1.0, -t),
            (0.0, 1.0, -t),
            (t, 0.0, -1.0),
            (t, 0.0, 1.0),
            (-t, 0.0, -1.0),
            (-t, 0.0, 1.0),
        ] {
            let v = glam::Vec3::new(x, y, z).normalize();
            low_positions.push([v.x, v.y, v.z]);
            low_normals.push([v.x, v.y, v.z]);
            low_uvs.push([0.5, 0.5]);
        }
        let low = MeshData {
            positions: low_positions,
            normals: low_normals,
            uvs: low_uvs,
            indices: vec![
                0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10,
                7, 6, 7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6,
                2, 10, 8, 6, 7, 9, 8, 1,
            ],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };

        // High: same icosphere but scaled up (subdivided approximation - just scale)
        let high_positions: Vec<[f32; 3]> = low
            .positions
            .iter()
            .map(|p| [p[0] * 1.1, p[1] * 1.1, p[2] * 1.1])
            .collect();
        let high = MeshData {
            positions: high_positions,
            normals: vec![],
            uvs: vec![],
            indices: low.indices.clone(),
            material_names: vec![],
            triangle_materials: Vec::new(),
        };

        let cage = Cage::from_high_low(&high, &low, CageMethod::Inflate, 0.2, 0.2);
        assert_eq!(cage.offsets.len(), 12);
        // All offsets should be non-zero (roughly 0.1 along normal)
        for off in &cage.offsets {
            let mag = (off[0].powi(2) + off[1].powi(2) + off[2].powi(2)).sqrt();
            assert!(mag > 0.05, "Expected non-zero offset, got {:?}", off);
        }
    }

    /// Manual method produces exactly front * normal.
    #[test]
    fn cage_from_high_low_manual_is_exact_implicit() {
        let low = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        let high = MeshData::default(); // Empty high

        let front = 0.3;
        let back = 0.2;
        let cage = Cage::from_high_low(&high, &low, CageMethod::Manual, front, back);

        assert_eq!(cage.offsets.len(), 3);
        assert_eq!(cage.offsets[0], [0.0, 0.0, front]);
        assert_eq!(cage.offsets[1], [0.0, front, 0.0]);
        assert_eq!(cage.offsets[2], [front, 0.0, 0.0]);
    }

    // --- Ground plane generator tests (kittest-first DoD) ---

    /// `subdivisions=2` produces 9 vertices, 8 triangles; UVs at corners are
    /// exactly `(0,0)`, `(1,0)`, `(0,1)`, `(1,1)`; interior vertices bilinear.
    #[test]
    fn ground_plane_uvs_cover_unit_square() {
        let desc = GroundPlaneDesc {
            size: 4.0,
            height_offset: 0.5,
            subdivisions: 2,
        };
        let mesh = generate_ground_plane(&desc).expect("ground plane generation");

        // (subdivs+1)^2 vertices = 9
        assert_eq!(mesh.positions.len(), 9);
        // 2 * subdivs^2 triangles = 8
        assert_eq!(mesh.triangle_count(), 8);

        // Corner UVs
        // Vertex order: z=0 (vz=0): x=0,1,2 (vx=0,1,2), then z=1, then z=2
        // vx=0,vz=0 -> idx 0 -> UV (0,0)
        assert_eq!(mesh.uvs[0], [0.0, 0.0]);
        // vx=2,vz=0 -> idx 2 -> UV (1,0)
        assert_eq!(mesh.uvs[2], [1.0, 0.0]);
        // vx=0,vz=2 -> idx 6 -> UV (0,1)
        assert_eq!(mesh.uvs[6], [0.0, 1.0]);
        // vx=2,vz=2 -> idx 8 -> UV (1,1)
        assert_eq!(mesh.uvs[8], [1.0, 1.0]);

        // Interior vertex (vx=1,vz=1) -> idx 4 -> UV (0.5, 0.5)
        assert_eq!(mesh.uvs[4], [0.5, 0.5]);
    }

    /// `height_offset=0.5` → all vertex Y == `-0.5`; `size=4.0` → X/Z span `[-2, 2]`.
    #[test]
    fn ground_plane_height_offset_is_below_scene() {
        let desc = GroundPlaneDesc {
            size: 4.0,
            height_offset: 0.5,
            subdivisions: 2,
        };
        let mesh = generate_ground_plane(&desc).expect("ground plane generation");

        // All vertices at Y = -0.5
        for pos in &mesh.positions {
            assert!(
                (pos[1] + 0.5).abs() < 1e-6,
                "Y should be -0.5, got {}",
                pos[1]
            );
        }

        // X/Z span [-2, 2] (half of size)
        let xs: Vec<f32> = mesh.positions.iter().map(|p| p[0]).collect();
        let zs: Vec<f32> = mesh.positions.iter().map(|p| p[2]).collect();
        assert!((xs.iter().copied().fold(f32::INFINITY, f32::min) + 2.0).abs() < 1e-6);
        assert!((xs.iter().copied().fold(f32::NEG_INFINITY, f32::max) - 2.0).abs() < 1e-6);
        assert!((zs.iter().copied().fold(f32::INFINITY, f32::min) + 2.0).abs() < 1e-6);
        assert!((zs.iter().copied().fold(f32::NEG_INFINITY, f32::max) - 2.0).abs() < 1e-6);
    }

    /// `subdivisions=0` returns `Err`.
    #[test]
    fn ground_plane_subdiv_zero_errors() {
        let desc = GroundPlaneDesc {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 0,
        };
        let err = generate_ground_plane(&desc).unwrap_err();
        assert!(matches!(err, GroundPlaneError::InvalidSubdivisions));
    }

    /// `size=0.0` returns `Err`.
    #[test]
    fn ground_plane_size_zero_errors() {
        let desc = GroundPlaneDesc {
            size: 0.0,
            height_offset: 0.1,
            subdivisions: 32,
        };
        let err = generate_ground_plane(&desc).unwrap_err();
        assert!(matches!(err, GroundPlaneError::InvalidSize { size: 0.0 }));
    }
}
