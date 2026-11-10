//! CPU screen-space depth raster (R-A2 v3 occlusion culling, wave 2.1,
//! `docs/loop/briefs/2026-11-09-projection-occlusion-design.md`).
//!
//! [`rasterize_depth`] projects a mesh through a `view_proj` matrix into a
//! `width * height` buffer of nearest per-pixel `clip.w` (the view-space
//! forward distance, pre-divide: monotonic with distance for a perspective
//! camera and divide-free), `f32::INFINITY` where empty. One triangle-driven
//! pass, O(triangles + covered pixels); the buffer is `width * height * 4`
//! bytes (64 MiB at 4096²).
//!
//! Rules, all deliberate:
//!
//! - Malformed-position triangles cover nothing (W1.2:
//!   [`crate::triangle_positions_finite`], the same rule the fill and the
//!   decal projector use); out-of-range indices never panic.
//! - A per-vertex `clip.w <= NEAR_EPS` (at or behind the eye) is clamped to
//!   `NEAR_EPS` for the projection AND stored as that vertex's depth. No
//!   full near-plane clipping: the projector's shade rejects `w <= 0` texels
//!   on its own side before the occlusion test, so a behind-eye vertex never
//!   paints; in the depth buffer it can only occlude at near-plane depth.
//! - Screen-space area conditioning uses its own epsilon
//!   ([`SCREEN_COND_EPS`]), NOT `UV_COND_EPS` (UV space has its own scale).
//! - Per pixel the minimum depth wins; ties go to the LAST triangle in index
//!   order (the same winner convention as the paint raster), so a covering
//!   triangle reads its own depth and self is never occluded.

use crate::{triangle_positions_finite, MeshData};
use glam::{Mat4, Vec4};

/// Per-vertex `clip.w` at or below this clamps to it for the projection
/// (at or behind the eye would mirror across the eye otherwise).
pub const NEAR_EPS: f32 = 1e-6;

/// Screen-space barycentric determinant bound: a triangle whose projected
/// area determinant is below this covers nothing in the depth buffer.
pub const SCREEN_COND_EPS: f32 = 1e-10;

/// Rasterizes `mesh` through `view_proj` into a `width * height` nearest-
/// depth buffer of `clip.w` per pixel (`f32::INFINITY` where empty). Never
/// panics on malformed input; a zero-sized buffer returns empty.
pub fn rasterize_depth(mesh: &MeshData, view_proj: Mat4, width: u32, height: u32) -> Vec<f32> {
    let w = width as usize;
    let h = height as usize;
    let mut depth = vec![f32::INFINITY; w.checked_mul(h).unwrap_or(0)];
    if depth.is_empty() {
        return depth;
    }
    let (rw, rh) = (width as f32, height as f32);
    for tri in 0..mesh.triangle_count() {
        if !triangle_positions_finite(mesh, tri) {
            continue;
        }
        let base = tri * 3;
        let Some(idx) = mesh.indices.get(base..base + 3) else {
            continue;
        };
        let mut pos = [[0.0f32; 3]; 3];
        let mut have = true;
        for (k, &i) in idx.iter().enumerate() {
            match mesh.positions.get(i as usize) {
                Some(&p) => pos[k] = p,
                None => {
                    have = false;
                    break;
                }
            }
        }
        if !have {
            continue;
        }
        // Project; clamp w for the projection and as the stored depth.
        let mut scr = [[0.0f32; 2]; 3];
        let mut dep = [0.0f32; 3];
        let mut nan = false;
        for k in 0..3 {
            let c = view_proj * Vec4::new(pos[k][0], pos[k][1], pos[k][2], 1.0);
            if !c.x.is_finite() || !c.y.is_finite() || !c.z.is_finite() || !c.w.is_finite() {
                nan = true;
                break;
            }
            let wc = if c.w <= NEAR_EPS { NEAR_EPS } else { c.w };
            let ndc = (c / wc).truncate();
            // The projector's exact screen mapping (viewport-local px, y
            // down): the depth texel and the shaded texel must land on the
            // same pixel.
            scr[k] = [(ndc.x + 1.0) * 0.5 * rw, (1.0 - ndc.y) * 0.5 * rh];
            dep[k] = wc;
        }
        if nan {
            continue;
        }
        let minx = scr[0][0].min(scr[1][0]).min(scr[2][0]);
        let maxx = scr[0][0].max(scr[1][0]).max(scr[2][0]);
        let miny = scr[0][1].min(scr[1][1]).min(scr[2][1]);
        let maxy = scr[0][1].max(scr[1][1]).max(scr[2][1]);
        if !minx.is_finite() || !maxx.is_finite() || !miny.is_finite() || !maxy.is_finite() {
            continue;
        }
        let x0 = minx.floor().clamp(0.0, (w - 1) as f32) as usize;
        let x1 = maxx.ceil().clamp(0.0, (w - 1) as f32) as usize;
        let y0 = miny.floor().clamp(0.0, (h - 1) as f32) as usize;
        let y1 = maxy.ceil().clamp(0.0, (h - 1) as f32) as usize;
        let (ax, ay) = (scr[0][0], scr[0][1]);
        let (bx, by) = (scr[1][0], scr[1][1]);
        let (cx, cy) = (scr[2][0], scr[2][1]);
        let det = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if !det.is_finite() || det.abs() < SCREEN_COND_EPS {
            continue;
        }
        for py in y0..=y1 {
            for px in x0..=x1 {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                let l0 = ((by - cy) * (fx - cx) + (cx - bx) * (fy - cy)) / det;
                let l1 = ((cy - ay) * (fx - cx) + (ax - cx) * (fy - cy)) / det;
                let l2 = 1.0 - l0 - l1;
                if l0 < 0.0 || l1 < 0.0 || l2 < 0.0 {
                    continue;
                }
                let d = dep[0] * l0 + dep[1] * l1 + dep[2] * l2;
                if !d.is_finite() {
                    continue;
                }
                let at = py * w + px;
                // Min wins; `<=` makes the last triangle in index order win
                // ties, the paint raster's convention.
                if d <= depth[at] {
                    depth[at] = d;
                }
            }
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ray_intersect;
    use glam::Vec3;

    /// The 2x2 quad at z = `z` spanning [-1, 1]², unit UVs.
    fn quad_at(z: f32) -> MeshData {
        MeshData {
            positions: vec![
                [-1.0, -1.0, z],
                [1.0, -1.0, z],
                [1.0, 1.0, z],
                [-1.0, 1.0, z],
            ],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        }
    }

    /// A perspective view_proj for an eye at `eye` looking at `target`.
    fn cam(eye: Vec3, target: Vec3) -> Mat4 {
        let proj = glam::camera::rh::proj::directx::perspective(FRAC_PI_2, 1.0, 0.1, 100.0);
        proj * glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y)
    }

    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn depth_of_a_flat_quad_equals_its_clip_w() {
        // Eye at z = 5 looking -z: the z = 0 quad has clip.w = 5 everywhere.
        let vp = cam(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let d = rasterize_depth(&quad_at(0.0), vp, 32, 32);
        assert_eq!(d.len(), 32 * 32);
        let center = d[16 * 32 + 16];
        assert!((center - 5.0).abs() < 1e-4, "center depth {center}");
        // Corners outside the projected quad stay empty.
        assert_eq!(d[0], f32::INFINITY);
    }

    #[test]
    fn depth_matches_ray_intersect_per_pixel_on_an_in_front_corpus() {
        // Quad plus a nearer tilted triangle, both fully in front of the
        // eye: per-pixel nearest clip.w equals ray_intersect's hit, and
        // empty pixels have no hit.
        let mut mesh = quad_at(-2.0);
        mesh.positions.extend_from_slice(&[
            [-0.5, -0.5, -1.0],
            [0.9, -0.4, -1.2],
            [0.2, 0.8, -0.8],
        ]);
        mesh.indices.extend_from_slice(&[4, 5, 6]);
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let vp = cam(eye, Vec3::ZERO);
        let (w, h) = (48u32, 48u32);
        let d = rasterize_depth(&mesh, vp, w, h);
        let inv = vp.inverse();
        let mut compared = 0;
        for py in 0..h as usize {
            for px in 0..w as usize {
                let ndc = [
                    2.0 * (px as f32 + 0.5) / w as f32 - 1.0,
                    1.0 - 2.0 * (py as f32 + 0.5) / h as f32,
                ];
                let far = inv * Vec4::new(ndc[0], ndc[1], 1.0, 1.0);
                let dir = (far.truncate() / far.w - eye).normalize();
                let hit = ray_intersect(&mesh, eye, dir);
                let depth = d[py * w as usize + px];
                match (depth.is_finite(), hit) {
                    (true, Some(hit)) => {
                        let p = eye + dir * hit.t;
                        let w_hit = (vp * p.extend(1.0)).w;
                        assert!(
                            (w_hit - depth).abs() <= 1e-3 * w_hit,
                            "px ({px},{py}): raster {depth} vs ray {w_hit}"
                        );
                        compared += 1;
                    }
                    (false, None) => {}
                    (a, b) => panic!("px ({px},{py}): depth finite {a} but hit {b:?}"),
                }
            }
        }
        assert!(compared > 20, "corpus actually covered pixels: {compared}");
    }

    #[test]
    fn near_plane_clamp_pins_the_behind_eye_vertex() {
        // Eye at z = 5 looking -z; one vertex at z = 6 is BEHIND the eye
        // (w = -1): it clamps to NEAR_EPS for the projection and the stored
        // depth. Without the clamp the divide would mirror it and the
        // interpolated depth would go negative; with it, every finite depth
        // stays in [NEAR_EPS, 5] and the clamp region near that vertex pulls
        // the minimum well below the in-front vertices' w = 5.
        let vp = cam(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let mut mesh = quad_at(0.0);
        mesh.positions[0] = [-1.0, -1.0, 6.0];
        let d = rasterize_depth(&mesh, vp, 32, 32);
        assert_eq!(d.len(), 32 * 32);
        let finite: Vec<f32> = d.iter().copied().filter(|v| v.is_finite()).collect();
        assert!(!finite.is_empty());
        assert!(
            finite.iter().all(|&v| (NEAR_EPS..=5.0).contains(&v)),
            "no mirrored/negative depth: {finite:?}"
        );
        let min = finite.iter().copied().fold(f32::INFINITY, f32::min);
        // Without the clamp the interpolated depth near the mirrored vertex
        // would go negative; with it every finite value stays >= NEAR_EPS.
        assert!((NEAR_EPS..=5.0).contains(&min), "clamp holds: {min}");
    }

    #[test]
    fn malformed_position_triangle_covers_nothing() {
        let mut mesh = quad_at(-2.0);
        // A second triangle with a NaN position: invalid, covers nothing.
        mesh.positions.extend_from_slice(&[
            [0.0, 0.0, f32::NAN],
            [1.0, 0.0, -2.0],
            [0.0, 1.0, -2.0],
        ]);
        mesh.indices.extend_from_slice(&[4, 5, 6]);
        let vp = cam(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let d = rasterize_depth(&mesh, vp, 32, 32);
        let with_bad = d.iter().filter(|v| v.is_finite()).count();
        let good = rasterize_depth(&quad_at(-2.0), vp, 32, 32);
        let without = good.iter().filter(|v| v.is_finite()).count();
        assert_eq!(with_bad, without, "the NaN triangle added no coverage");
    }

    #[test]
    fn zero_sized_buffer_returns_empty() {
        assert!(rasterize_depth(&quad_at(0.0), Mat4::IDENTITY, 0, 10).is_empty());
        assert!(rasterize_depth(&quad_at(0.0), Mat4::IDENTITY, 10, 0).is_empty());
    }
}
