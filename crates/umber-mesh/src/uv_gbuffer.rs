//! The UV G-buffer: the ONE texel/UV -> (triangle, barycentrics) lookup.
//!
//! Queue row 156-B (design brief `docs/loop/briefs/2026-11-05-uv-gbuffer-design.md`).
//! Before this module the 2D point-in-triangle solve lived in four copies
//! (CPU: `fill::rasterize_uv_geometry`, `polygon_fill::uv_triangle_at`;
//! WGSL: the position and ID bake shaders) with two different degenerate
//! epsilons. The CPU copies now delegate here; the GPU side quotes
//! [`UV_DET_EPS`] / [`UV_BARY_EPS`] so both run the same formula.
//!
//! Contract (shared by every consumer):
//! - Triangles are `mesh.indices.chunks_exact(3)`; the triangle index is
//!   the chunk position. Malformed (out-of-range UV index), non-finite and
//!   UV-degenerate (`|area| < UV_DET_EPS`) triangles cover nothing.
//! - Barycentrics are `b_i = -e_i / area` (the edge-function correction
//!   documented at the call sites: `E_ab(v_opp) = -area` in both
//!   windings), so the inside test is one winding-agnostic branch:
//!   every `b_i >= -UV_BARY_EPS` (edge-inclusive).
//! - Overlap: the LAST triangle in index order wins.
//! - Never panics; NaN / infinite inputs miss.

use crate::MeshData;

/// Degenerate-footprint threshold on the signed UV-space area (UV² units).
/// Unifies the CPU's old `1e-12` with the bake shaders' `1e-10`; the one
/// intentional behavior change of row 156-B (pinned by
/// `det_eps_boundary_is_1e_10`). A footprint this small is under ~1.4e-5
/// UV units across, below one texel at every supported resolution.
pub const UV_DET_EPS: f32 = 1e-10;

/// Edge-inclusive inside tolerance in barycentric space (scale-invariant).
pub const UV_BARY_EPS: f32 = 1e-6;

/// One lookup result: the covering triangle and its barycentric weights
/// (`bary[0] + bary[1] + bary[2] == 1` up to f32 rounding).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvHit {
    /// Triangle index (`mesh.indices / 3` position).
    pub triangle: usize,
    /// Weights for the triangle's vertices 0, 1, 2.
    pub bary: [f32; 3],
}

/// A triangle's UV footprint, prepared once so per-texel loops pay only
/// the edge functions. Built by [`UvTri::of`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvTri {
    /// The three vertex UVs.
    pub uv: [[f32; 2]; 3],
    /// Signed UV-space area (cross product; never below `UV_DET_EPS`).
    pub area: f32,
}

impl UvTri {
    /// Prepares triangle `tri` of `mesh`, or `None` when it is malformed,
    /// non-finite or UV-degenerate.
    pub fn of(mesh: &MeshData, tri: usize) -> Option<UvTri> {
        let base = tri.checked_mul(3)?;
        let idx = mesh.indices.get(base..base.checked_add(3)?)?;
        let uv0 = *mesh.uvs.get(idx[0] as usize)?;
        let uv1 = *mesh.uvs.get(idx[1] as usize)?;
        let uv2 = *mesh.uvs.get(idx[2] as usize)?;
        Self::from_uvs([uv0, uv1, uv2])
    }

    /// Prepares a footprint from raw UVs (same validity rules as [`UvTri::of`]).
    pub fn from_uvs(uv: [[f32; 2]; 3]) -> Option<UvTri> {
        if !uv.iter().all(|t| t[0].is_finite() && t[1].is_finite()) {
            return None;
        }
        let [uv0, uv1, uv2] = uv;
        let area = (uv1[0] - uv0[0]) * (uv2[1] - uv0[1]) - (uv2[0] - uv0[0]) * (uv1[1] - uv0[1]);
        if !area.is_finite() || area.abs() < UV_DET_EPS {
            return None;
        }
        Some(UvTri { uv, area })
    }

    /// The barycentric weights of `p` when it lies inside the footprint
    /// (edge-inclusive), else `None`.
    #[inline]
    pub fn bary(&self, p: [f32; 2]) -> Option<[f32; 3]> {
        let [uv0, uv1, uv2] = self.uv;
        let e0 = (p[0] - uv1[0]) * (uv2[1] - uv1[1]) - (p[1] - uv1[1]) * (uv2[0] - uv1[0]);
        let e1 = (p[0] - uv2[0]) * (uv0[1] - uv2[1]) - (p[1] - uv2[1]) * (uv0[0] - uv2[0]);
        let e2 = (p[0] - uv0[0]) * (uv1[1] - uv0[1]) - (p[1] - uv0[1]) * (uv1[0] - uv0[0]);
        let b = [-e0 / self.area, -e1 / self.area, -e2 / self.area];
        (b[0] >= -UV_BARY_EPS && b[1] >= -UV_BARY_EPS && b[2] >= -UV_BARY_EPS).then_some(b)
    }

    /// The footprint's UV bounding box `([min_u, min_v], [max_u, max_v])`.
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        let [a, b, c] = self.uv;
        (
            [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1])],
            [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1])],
        )
    }
}

/// The primitive: the triangle whose UV footprint contains `uv` (last in
/// index order wins) with its barycentrics, or `None` on a miss, a
/// non-finite `uv`, or an empty mesh. O(triangles); per-texel callers use
/// [`UvGBuffer::build`], which culls by bounding box.
pub fn uv_to_barycentric(mesh: &MeshData, uv: [f32; 2]) -> Option<UvHit> {
    if !uv[0].is_finite() || !uv[1].is_finite() {
        return None;
    }
    let mut found = None;
    for triangle in 0..mesh.indices.len() / 3 {
        let Some(t) = UvTri::of(mesh, triangle) else {
            continue;
        };
        if let Some(bary) = t.bary(uv) {
            found = Some(UvHit { triangle, bary });
        }
    }
    found
}

/// Barycentric interpolation of three values (`a*b0 + b*b1 + c*b2`), the
/// order every CPU consumer uses so results stay bit-identical.
#[inline]
pub fn lerp3(a: [f32; 3], b: [f32; 3], c: [f32; 3], w: [f32; 3]) -> [f32; 3] {
    [
        a[0] * w[0] + b[0] * w[1] + c[0] * w[2],
        a[1] * w[0] + b[1] * w[1] + c[1] * w[2],
        a[2] * w[0] + b[2] * w[1] + c[2] * w[2],
    ]
}

/// The UDIM tile's integer UV offset `(tu, tv)` (tile 1001 = (0, 0),
/// ten tiles per row). Tiles below 1001 clamp to 1001.
pub fn tile_offset(tile: u16) -> (f32, f32) {
    let index = tile.saturating_sub(crate::FIRST_TILE);
    (f32::from(index % 10), f32::from(index / 10))
}

/// Texel `(x, y)`'s center UV inside `tile` at `width`x`height`:
/// `u = tu + (x+0.5)/w`, `v = tv + 1 - (y+0.5)/h` (rows top-down; the
/// inverse of the paint path's `local_texel`).
#[inline]
pub fn texel_center_uv(tu: f32, tv: f32, x: u32, y: u32, width: u32, height: u32) -> [f32; 2] {
    [
        tu + (x as f32 + 0.5) / width as f32,
        tv + 1.0 - (y as f32 + 0.5) / height as f32,
    ]
}

/// The inclusive tile-local texel window `(x0, x1, y0, y1)` a footprint's
/// bounding box can touch, expanded by one texel and clamped; `None` when
/// it misses the tile. Shared by every per-texel rasterizer so culling
/// never disagrees with coverage.
pub fn texel_window(
    tri: &UvTri,
    tu: f32,
    tv: f32,
    width: u32,
    height: u32,
) -> Option<(usize, usize, usize, usize)> {
    let ([min_u, min_v], [max_u, max_v]) = tri.bounds();
    let x0 = (((min_u - tu) * width as f32 - 1.0).floor() as i64).max(0) as usize;
    let x1 =
        ((((max_u - tu) * width as f32 + 1.0).ceil() as i64).min(width as i64 - 1)).max(0) as usize;
    let y0 = ((((tv + 1.0 - max_v) * height as f32 - 1.0).floor() as i64).max(0)) as usize;
    let y1 = (((((tv + 1.0 - min_v) * height as f32 + 1.0).ceil() as i64).min(height as i64 - 1))
        .max(0)) as usize;
    (x0 <= x1 && y0 <= y1).then_some((x0, x1, y0, y1))
}

/// The CPU UV G-buffer: one [`UvHit`] per texel of a tile (row-major,
/// top-down; `None` = uncovered). The authority the GPU G-buffer mirrors.
/// Never build this on the paint hot path (16.7 M texels at 4K): it is
/// for bakes, export padding, fills and tests.
#[derive(Debug, Clone, PartialEq)]
pub struct UvGBuffer {
    pub tile: u16,
    pub width: u32,
    pub height: u32,
    pub hits: Vec<Option<UvHit>>,
}

impl UvGBuffer {
    /// Rasterizes `mesh`'s UV layout into `tile`'s texel grid. Zero-sized
    /// targets yield an empty buffer.
    pub fn build(mesh: &MeshData, tile: u16, width: u32, height: u32) -> UvGBuffer {
        let mut hits = Vec::new();
        if width > 0 && height > 0 {
            hits = vec![None; width as usize * height as usize];
            for_each_covered_texel(mesh, tile, width, height, |_, x, y, hit| {
                hits[y * width as usize + x] = Some(hit);
            });
        }
        UvGBuffer {
            tile,
            width,
            height,
            hits,
        }
    }

    /// The hit at texel `(x, y)`, `None` when uncovered or out of range.
    pub fn at(&self, x: u32, y: u32) -> Option<UvHit> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.hits
            .get(y as usize * self.width as usize + x as usize)
            .copied()
            .flatten()
    }

    /// The coverage mask (`true` = some triangle covers the texel), the
    /// shape export padding's dilation consumes.
    pub fn covered(&self) -> Vec<bool> {
        self.hits.iter().map(Option::is_some).collect()
    }
}

/// Visits every covered texel of `tile` in triangle-index order, calling
/// `f(&UvTri, x, y, hit)`. A texel two triangles cover is visited twice;
/// writing the result into a buffer therefore gives the last-in-index-
/// order winner. `filter` lets callers skip triangles (e.g. ones with
/// non-finite positions) before any texel work.
pub(crate) fn for_each_covered_texel_filtered(
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
    mut filter: impl FnMut(usize) -> bool,
    mut f: impl FnMut(&UvTri, usize, usize, UvHit),
) {
    if width == 0 || height == 0 {
        return;
    }
    let (tu, tv) = tile_offset(tile);
    for triangle in 0..mesh.indices.len() / 3 {
        let Some(t) = UvTri::of(mesh, triangle) else {
            continue;
        };
        if !filter(triangle) {
            continue;
        }
        let Some((x0, x1, y0, y1)) = texel_window(&t, tu, tv, width, height) else {
            continue;
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let uv = texel_center_uv(tu, tv, x as u32, y as u32, width, height);
                if let Some(bary) = t.bary(uv) {
                    f(&t, x, y, UvHit { triangle, bary });
                }
            }
        }
    }
}

fn for_each_covered_texel(
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
    f: impl FnMut(&UvTri, usize, usize, UvHit),
) {
    for_each_covered_texel_filtered(mesh, tile, width, height, |_| true, f);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit quad in UV (and in XY), two triangles sharing the diagonal.
    fn quad() -> MeshData {
        MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Default::default()
        }
    }

    #[test]
    fn hit_weights_sum_to_one_and_reconstruct_the_point() {
        let mesh = quad();
        let hit = uv_to_barycentric(&mesh, [0.7, 0.2]).expect("inside tri 0");
        assert_eq!(hit.triangle, 0);
        let s: f32 = hit.bary.iter().sum();
        assert!((s - 1.0).abs() < 1e-6, "sum {s}");
        let p = lerp3(
            mesh.positions[0],
            mesh.positions[1],
            mesh.positions[2],
            hit.bary,
        );
        assert!(
            (p[0] - 0.7).abs() < 1e-6 && (p[1] - 0.2).abs() < 1e-6,
            "{p:?}"
        );
    }

    #[test]
    fn diagonal_goes_to_the_last_triangle_and_misses_are_none() {
        let mesh = quad();
        assert_eq!(
            uv_to_barycentric(&mesh, [0.5, 0.5]).map(|h| h.triangle),
            Some(1)
        );
        assert_eq!(uv_to_barycentric(&mesh, [1.5, 0.5]), None);
        assert_eq!(uv_to_barycentric(&mesh, [f32::NAN, 0.5]), None);
        assert_eq!(uv_to_barycentric(&mesh, [0.5, f32::INFINITY]), None);
        assert_eq!(uv_to_barycentric(&MeshData::default(), [0.5, 0.5]), None);
    }

    #[test]
    fn winding_does_not_matter() {
        let mut cw = quad();
        cw.indices = vec![0, 2, 1, 0, 3, 2];
        assert_eq!(
            uv_to_barycentric(&cw, [0.7, 0.2]).map(|h| h.triangle),
            Some(0)
        );
        assert_eq!(
            uv_to_barycentric(&cw, [0.2, 0.7]).map(|h| h.triangle),
            Some(1)
        );
    }

    #[test]
    fn malformed_and_nonfinite_triangles_are_skipped() {
        let mut mesh = quad();
        mesh.indices.extend_from_slice(&[0, 1, 99]);
        mesh.indices.push(7); // trailing partial triangle
        assert_eq!(UvTri::of(&mesh, 2), None);
        assert_eq!(UvTri::of(&mesh, usize::MAX), None);
        mesh.uvs[3] = [f32::NAN, 1.0];
        assert_eq!(
            uv_to_barycentric(&mesh, [0.2, 0.7]),
            None,
            "tri 1 has a NaN uv"
        );
        assert_eq!(
            uv_to_barycentric(&mesh, [0.7, 0.2]).map(|h| h.triangle),
            Some(0)
        );
    }

    /// The one intentional behavior change: footprints with
    /// `1e-12 <= |area| < 1e-10` used to cover on the CPU (old eps 1e-12)
    /// but not on the GPU bakes (1e-10); now neither covers them.
    #[test]
    fn det_eps_boundary_is_1e_10() {
        assert_eq!(UV_DET_EPS, 1e-10);
        // area = s*s for a right triangle with legs s.
        let tiny = 5e-6_f32; // area 2.5e-11
        assert!(UvTri::from_uvs([[0.0, 0.0], [tiny, 0.0], [0.0, tiny]]).is_none());
        let ok = 2e-5_f32; // area 4e-10
        assert!(UvTri::from_uvs([[0.0, 0.0], [ok, 0.0], [0.0, ok]]).is_some());
    }

    #[test]
    fn gbuffer_covers_the_quad_and_respects_udim_tiles() {
        let mesh = quad();
        let g = UvGBuffer::build(&mesh, 1001, 8, 8);
        assert_eq!(g.hits.len(), 64);
        assert!(g.covered().iter().all(|&c| c), "unit quad covers tile 1001");
        // Top-left texel (u small, v near 1) lies in tri 1 (upper-left half).
        assert_eq!(g.at(0, 0).map(|h| h.triangle), Some(1));
        // Bottom-right texel lies in tri 0.
        assert_eq!(g.at(7, 7).map(|h| h.triangle), Some(0));
        assert_eq!(g.at(8, 0), None);
        let other = UvGBuffer::build(&mesh, 1002, 8, 8);
        assert!(other.covered().iter().all(|&c| !c), "nothing in tile 1002");
        assert!(UvGBuffer::build(&mesh, 1001, 0, 8).hits.is_empty());
    }

    #[test]
    fn gbuffer_agrees_with_the_point_lookup_everywhere() {
        let mut mesh = quad();
        // An overlapping third triangle so the overlap rule is exercised.
        mesh.uvs.push([0.25, 0.25]);
        mesh.positions.push([0.25, 0.25, 0.0]);
        mesh.indices.extend_from_slice(&[4, 1, 2]);
        let (w, h) = (16, 16);
        let g = UvGBuffer::build(&mesh, 1001, w, h);
        for y in 0..h {
            for x in 0..w {
                let uv = texel_center_uv(0.0, 0.0, x, y, w, h);
                assert_eq!(g.at(x, y), uv_to_barycentric(&mesh, uv), "texel {x},{y}");
            }
        }
    }

    #[test]
    fn tile_offset_matches_udim_layout() {
        assert_eq!(tile_offset(1001), (0.0, 0.0));
        assert_eq!(tile_offset(1012), (1.0, 1.0));
        assert_eq!(tile_offset(1100), (9.0, 9.0));
        assert_eq!(tile_offset(0), (0.0, 0.0));
    }
}
