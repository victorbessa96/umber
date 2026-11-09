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
//!
//! Per-texel consumers never loop [`uv_to_barycentric`] over a tile (that
//! is O(texels x triangles): R-A2 v2d measured 34 min for one 4096² decal
//! Apply on an 8 192-triangle mesh). They take the one triangle-driven
//! walk instead: [`UvGBuffer::build`] (buffered hits) or
//! [`for_each_uv_geometry`] (streamed world point + raw normal).

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

/// The world-space position of `uv` on the mesh — the inverse direction of
/// [`uv_at`](crate::raycast::uv_at) (hit → UV): the covering triangle's
/// barycentric interpolation of its vertex positions, through
/// [`uv_to_barycentric`] + [`lerp3`] so the interpolation order stays
/// bit-identical with every other CPU consumer. `None` on a UV miss, a
/// non-finite `uv`, an out-of-range index or an empty mesh.
///
/// Row 277 (projection-fill plan `docs/specs/projection-fill-plan.md`): the
/// decal projector's per-texel raster maps texel → UV → world with this and
/// then projects through the frozen camera — it never raycasts.
///
/// A thin wrapper over [`world_and_face_normal_at_uv`] (one barycentric
/// scan implementation); the world point is the same expression, so the
/// result is unchanged.
pub fn world_at_uv(mesh: &MeshData, uv: [f32; 2]) -> Option<[f32; 3]> {
    world_and_face_normal_at_uv(mesh, uv).map(|(world, _)| world)
}

/// The degenerate-normal threshold on the geometric cross product's
/// LENGTH (world units²): at or below it the triangle has no usable
/// facing (R-A2 v2c design brief
/// `docs/loop/briefs/2026-11-09-projection-backface-design.md` §2).
pub const FACE_NORMAL_EPS: f32 = 1e-12;

/// [`world_at_uv`] plus the covering triangle's unit GEOMETRIC normal,
/// from ONE barycentric scan (R-A2 v2c, the decal projector's backface
/// cull). `None` exactly where [`world_at_uv`] is `None` (a UV miss, a
/// non-finite `uv`, an out-of-range index, an empty mesh): nothing to
/// write.
///
/// The world point is `lerp3(p0, p1, p2, bary)`, the same expression and
/// order as every other CPU consumer. The normal is
/// `normalize((p1 - p0) x (p2 - p0))` over the positions IN INDEX ORDER:
/// per-TRIANGLE, matching the mesh pipeline's `cull_mode: Back` +
/// CCW-front convention (the same expression the renderer's tangent-frame
/// builder uses), so a CPU facing test agrees with what the viewport
/// culls. It is NOT the per-vertex `MeshData::normals` (optional and
/// smooth-shaded, which would disagree with the viewport's silhouette).
///
/// The normal is an inner `Option` so a degenerate triangle (cross
/// product length `<= FACE_NORMAL_EPS`, or non-finite) still yields its
/// world point: the caller keeps the texel and simply has no facing to
/// test — malformed geometry never silently kills paint.
pub fn world_and_face_normal_at_uv(
    mesh: &MeshData,
    uv: [f32; 2],
) -> Option<([f32; 3], Option<[f32; 3]>)> {
    let hit = uv_to_barycentric(mesh, uv)?;
    let base = hit.triangle.checked_mul(3)?;
    let idx = mesh.indices.get(base..base.checked_add(3)?)?;
    let p0 = *mesh.positions.get(idx[0] as usize)?;
    let p1 = *mesh.positions.get(idx[1] as usize)?;
    let p2 = *mesh.positions.get(idx[2] as usize)?;
    Some((lerp3(p0, p1, p2, hit.bary), face_normal(p0, p1, p2)))
}

/// `normalize((p1 - p0) x (p2 - p0))`, or `None` when the cross product's
/// length is `<= FACE_NORMAL_EPS` or not finite.
fn face_normal(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> Option<[f32; 3]> {
    let n = raw_face_normal(p0, p1, p2);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !len.is_finite() || len <= FACE_NORMAL_EPS {
        return None;
    }
    Some([n[0] / len, n[1] / len, n[2] / len])
}

/// The UNNORMALIZED geometric normal `(p1 - p0) x (p2 - p0)` over the
/// positions in index order: the one cross-product expression behind
/// [`face_normal`], [`for_each_uv_geometry`] and (through it)
/// `fill::rasterize_uv_geometry`, so every consumer sees the same bits.
#[inline]
fn raw_face_normal(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> [f32; 3] {
    let a = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let b = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
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

/// [`lerp3`] for two-component values (UVs).
#[inline]
pub fn lerp2(a: [f32; 2], b: [f32; 2], c: [f32; 2], w: [f32; 3]) -> [f32; 2] {
    [
        a[0] * w[0] + b[0] * w[1] + c[0] * w[2],
        a[1] * w[0] + b[1] * w[1] + c[1] * w[2],
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

/// The STREAMING per-texel geometry walk (R-A2 v2d, design brief
/// `docs/loop/briefs/2026-11-09-projection-one-pass-design.md`): every
/// covered texel `(x, y)` of `tile` at `width`x`height`, with the covering
/// triangle's world point and RAW geometric normal, in ONE pass over the
/// triangles — O(triangles + covered texels), never O(texels x triangles)
/// like a per-texel [`uv_to_barycentric`] loop, and no buffer (a G-buffer
/// at 4096² would cost 400+ MiB).
///
/// - **The containment is the crate's single primitive**: this IS
///   `for_each_covered_texel_filtered` (the walk behind [`UvGBuffer::build`]
///   and `fill::uv_coverage`) plus per-triangle geometry, so its texel set
///   and barycentrics never drift from the G-buffer's.
/// - **Winner rule**: triangles are visited in INDEX ORDER and a texel two
///   footprints cover is yielded once per footprint, so a consumer that
///   writes every yield into a buffer ends with the LAST triangle in index
///   order — exactly [`uv_to_barycentric`]'s winner. A consumer that has
///   nothing to write for a yield must therefore OVERWRITE (e.g. with
///   transparent), never skip, or an earlier triangle's value survives.
/// - **`filter(triangle)`** runs once per UV-valid triangle (malformed,
///   non-finite-UV and UV-degenerate triangles cover nothing and are never
///   offered), in index order, immediately BEFORE that triangle's texels;
///   `false` skips the triangle entirely (it is not visited). So the last
///   index `filter` accepted names the triangle every following yield
///   belongs to, until the next accepted call.
/// - **`world`** is `lerp3(p0, p1, p2, bary)`, the expression and order of
///   [`world_at_uv`], so the two agree byte for byte.
/// - **`raw_normal`** is the UNNORMALIZED `(p1 - p0) x (p2 - p0)` over the
///   positions in index order. It is raw because the consumers normalize
///   under DIFFERENT rules and each keeps its own: the decal projector
///   needs a usable facing (`len > FACE_NORMAL_EPS` and finite, else "no
///   facing"), the fill normalizes when `len > 0` and else yields
///   `[0, 0, 0]`.
/// - **Malformed positions are still visited** (only `filter` skips): a
///   triangle with non-finite positions yields its (non-finite) world point
///   and normal; one whose POSITION indices are out of range (its UV
///   indices being valid) yields `[NaN; 3]` for both. That keeps the
///   projector's winner semantics — today's malformed winner blanks the
///   texel, it does not let an earlier triangle show through. Never panics.
/// - Zero-sized targets (`width == 0` or `height == 0`) visit nothing.
pub fn for_each_uv_geometry(
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
    filter: impl FnMut(usize) -> bool,
    mut f: impl FnMut(u32, u32, [f32; 3], [f32; 3]),
) {
    // The current triangle's positions + raw normal, computed once per
    // triangle (its texels arrive contiguously, in index order).
    let mut current: Option<(usize, [[f32; 3]; 3], [f32; 3])> = None;
    for_each_covered_texel_filtered(mesh, tile, width, height, filter, |_, x, y, hit| {
        let (p, raw) = match current {
            Some((tri, p, raw)) if tri == hit.triangle => (p, raw),
            _ => {
                let p = triangle_positions(mesh, hit.triangle).unwrap_or([[f32::NAN; 3]; 3]);
                let raw = raw_face_normal(p[0], p[1], p[2]);
                current = Some((hit.triangle, p, raw));
                (p, raw)
            }
        };
        let world = lerp3(p[0], p[1], p[2], hit.bary);
        // `x < width` and `y < height` (the texel window is clamped), so
        // both casts are lossless.
        f(x as u32, y as u32, world, raw);
    });
}

/// Triangle `tri`'s three positions in index order, `None` when an index
/// (or a position index) is out of range.
fn triangle_positions(mesh: &MeshData, tri: usize) -> Option<[[f32; 3]; 3]> {
    let base = tri.checked_mul(3)?;
    let idx = mesh.indices.get(base..base.checked_add(3)?)?;
    Some([
        *mesh.positions.get(idx[0] as usize)?,
        *mesh.positions.get(idx[1] as usize)?,
        *mesh.positions.get(idx[2] as usize)?,
    ])
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

    #[test]
    fn world_at_uv_reconstructs_the_flat_quad() {
        let mesh = quad();
        for (uv, want) in [
            ([0.7, 0.2], [0.7, 0.2, 0.0]),
            ([0.25, 0.75], [0.25, 0.75, 0.0]),
            ([0.0, 0.0], [0.0, 0.0, 0.0]),
            ([1.0, 1.0], [1.0, 1.0, 0.0]),
        ] {
            let p = world_at_uv(&mesh, uv).expect("inside the quad");
            assert!(
                (p[0] - want[0]).abs() < 1e-6
                    && (p[1] - want[1]).abs() < 1e-6
                    && (p[2] - want[2]).abs() < 1e-6,
                "uv {uv:?} -> {p:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn world_at_uv_interpolates_z_and_agrees_with_the_barycentric_hit() {
        // Same UV quad, but the positions rise in z: world_at_uv must carry
        // the interpolated z, and must pick the SAME triangle the point
        // lookup picks (last in index order wins on the diagonal).
        let mut mesh = quad();
        mesh.positions = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 4.0],
            [0.0, 2.0, 4.0],
        ];
        // (0.25, 0.25) is inside tri 0 (uvs (0,0),(1,0),(1,1)): bary
        // (0.75, 0.0, 0.25) -> 0.75*p0 + 0.25*p2 = (0.5, 0.5, 1.0).
        let p = world_at_uv(&mesh, [0.25, 0.25]).expect("inside tri 0");
        assert!(
            (p[0] - 0.5).abs() < 1e-6 && (p[1] - 0.5).abs() < 1e-6 && (p[2] - 1.0).abs() < 1e-6,
            "{p:?}"
        );
        let hit = uv_to_barycentric(&mesh, [0.5, 0.5]).expect("on the diagonal");
        assert_eq!(hit.triangle, 1, "the diagonal belongs to the last tri");
        let p = world_at_uv(&mesh, [0.5, 0.5]).expect("on the diagonal");
        // tri 1 = (0, 2, 3), uvs (0,0),(1,1),(0,1); (0.5,0.5) -> bary (0.5, 0.5, 0).
        let want = lerp3(
            mesh.positions[0],
            mesh.positions[2],
            mesh.positions[3],
            hit.bary,
        );
        assert!(
            (p[0] - want[0]).abs() < 1e-6
                && (p[1] - want[1]).abs() < 1e-6
                && (p[2] - want[2]).abs() < 1e-6,
            "{p:?} vs {want:?}"
        );
    }

    #[test]
    fn world_at_uv_misses_exactly_where_the_point_lookup_misses() {
        let mut mesh = quad();
        assert_eq!(world_at_uv(&mesh, [1.5, 0.5]), None);
        assert_eq!(world_at_uv(&mesh, [f32::NAN, 0.5]), None);
        assert_eq!(world_at_uv(&mesh, [0.5, f32::INFINITY]), None);
        assert_eq!(world_at_uv(&MeshData::default(), [0.5, 0.5]), None);
        // A UV-degenerate triangle covers nothing, so its UVs miss too.
        mesh.uvs = vec![[0.5, 0.5], [0.5, 0.5], [0.5, 0.5], [0.5, 0.5]];
        assert_eq!(world_at_uv(&mesh, [0.5, 0.5]), None);
        // An out-of-range index in the winning triangle misses instead of
        // panicking.
        mesh.uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        mesh.indices = vec![0, 1, 99];
        assert_eq!(world_at_uv(&mesh, [0.7, 0.2]), None);
    }

    // --- world_and_face_normal_at_uv (R-A2 v2c) ---

    #[test]
    fn face_normal_world_point_is_byte_identical_to_world_at_uv() {
        // The z-rising quad so the interpolation is non-trivial.
        let mut mesh = quad();
        mesh.positions = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 4.0],
            [0.0, 2.0, 4.0],
        ];
        for uv in [
            [0.7, 0.2],
            [0.25, 0.75],
            [0.5, 0.5],
            [0.13, 0.91],
            [1.0, 1.0],
        ] {
            let (world, _) = world_and_face_normal_at_uv(&mesh, uv).expect("inside the quad");
            let want = world_at_uv(&mesh, uv).expect("inside the quad");
            assert_eq!(world.map(f32::to_bits), want.map(f32::to_bits), "uv {uv:?}");
        }
    }

    #[test]
    fn face_normal_is_unit_and_along_plus_z_for_the_ccw_quad() {
        // Tri 0 = (0,0,0),(1,0,0),(1,1,0): (1,0,0) x (1,1,0) = (0,0,1),
        // already unit; tri 1 = (0,0,0),(1,1,0),(0,1,0): (1,1,0) x
        // (0,1,0) = (0,0,1).
        let mesh = quad();
        for uv in [[0.7, 0.2], [0.2, 0.7]] {
            let (_, n) = world_and_face_normal_at_uv(&mesh, uv).expect("inside");
            assert_eq!(n, Some([0.0, 0.0, 1.0]), "uv {uv:?}");
        }
        // A non-unit cross product is normalized: scale x by 3 and z-tilt
        // tri 0 to (0,0,0),(3,0,0),(3,1,4): (3,0,0) x (3,1,4) =
        // (0, -12, 3), length sqrt(153) = 12.369317.
        let mut tilted = quad();
        tilted.positions = vec![
            [0.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [3.0, 1.0, 4.0],
            [0.0, 1.0, 0.0],
        ];
        let (_, n) = world_and_face_normal_at_uv(&tilted, [0.7, 0.2]).expect("inside");
        let n = n.expect("non-degenerate");
        let len = 153.0_f32.sqrt();
        assert_eq!(n[0], 0.0);
        assert!((n[1] - (-12.0 / len)).abs() < 1e-6, "{n:?}");
        assert!((n[2] - 3.0 / len).abs() < 1e-6, "{n:?}");
        let unit = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((unit - 1.0).abs() < 1e-6, "length {unit}");
    }

    #[test]
    fn reversing_the_winding_flips_the_face_normal() {
        // The UV lookup is winding-agnostic (same triangle covers), but the
        // normal follows the index order: CW in index order -> -z.
        let mut cw = quad();
        cw.indices = vec![0, 2, 1, 0, 3, 2];
        for uv in [[0.7, 0.2], [0.2, 0.7]] {
            let (world, n) = world_and_face_normal_at_uv(&cw, uv).expect("inside");
            assert_eq!(n, Some([0.0, 0.0, -1.0]), "uv {uv:?}");
            assert_eq!(Some(world), world_at_uv(&cw, uv));
        }
    }

    #[test]
    fn a_degenerate_triangle_keeps_its_world_point_but_has_no_normal() {
        // UVs stay a proper quad (the lookup hits), but tri 0's positions
        // are collinear on the x axis: zero cross product.
        let mut mesh = quad();
        mesh.positions[2] = [2.0, 0.0, 0.0];
        let (world, n) = world_and_face_normal_at_uv(&mesh, [0.7, 0.2]).expect("uv hit");
        assert_eq!(n, None, "collinear triangle has no facing");
        assert_eq!(Some(world), world_at_uv(&mesh, [0.7, 0.2]));
        // Non-finite positions: no normal either, never a NaN normal.
        mesh.positions[2] = [f32::INFINITY, 1.0, 0.0];
        let (_, n) = world_and_face_normal_at_uv(&mesh, [0.7, 0.2]).expect("uv hit");
        assert_eq!(n, None);
    }

    #[test]
    fn a_uv_miss_yields_nothing_from_the_normal_lookup() {
        let mesh = quad();
        assert_eq!(world_and_face_normal_at_uv(&mesh, [1.5, 0.5]), None);
        assert_eq!(world_and_face_normal_at_uv(&mesh, [f32::NAN, 0.5]), None);
        assert_eq!(
            world_and_face_normal_at_uv(&MeshData::default(), [0.5, 0.5]),
            None
        );
        let mut broken = quad();
        broken.indices = vec![0, 1, 99];
        assert_eq!(world_and_face_normal_at_uv(&broken, [0.7, 0.2]), None);
    }

    // --- for_each_uv_geometry (R-A2 v2d) ---

    /// Per texel (row-major): the last `(world, raw)` yielded there.
    type LastYield = Vec<Option<([f32; 3], [f32; 3])>>;

    /// The last yield per texel, the shape a buffer-writing consumer ends
    /// with, plus the total yield count.
    fn last_yields(mesh: &MeshData, w: u32, h: u32) -> (LastYield, usize) {
        let mut last = vec![None; (w * h) as usize];
        let mut yields = 0;
        for_each_uv_geometry(
            mesh,
            1001,
            w,
            h,
            |_| true,
            |x, y, world, raw| {
                last[(y * w + x) as usize] = Some((world, raw));
                yields += 1;
            },
        );
        (last, yields)
    }

    /// The overlap fixture: the z-rising quad plus a third triangle on top
    /// (vertex 4 high in z, so every triangle's raw normal differs).
    fn overlapping() -> MeshData {
        let mut mesh = quad();
        mesh.positions = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 4.0],
            [0.0, 2.0, 4.0],
            [0.5, 0.5, 7.0],
        ];
        mesh.uvs.push([0.25, 0.25]);
        mesh.indices.extend_from_slice(&[4, 1, 2]);
        mesh
    }

    #[test]
    fn the_walk_has_the_gbuffer_texel_set_and_the_point_lookup_winner() {
        let mesh = overlapping();
        let (w, h) = (16, 16);
        let g = UvGBuffer::build(&mesh, 1001, w, h);
        let (last, yields) = last_yields(&mesh, w, h);
        let covered: Vec<bool> = last.iter().map(Option::is_some).collect();
        assert_eq!(covered, g.covered());
        // The overlap is real: some texel was yielded more than once.
        assert!(yields > covered.iter().filter(|&&c| c).count(), "{yields}");
        for y in 0..h {
            for x in 0..w {
                let uv = texel_center_uv(0.0, 0.0, x, y, w, h);
                let Some((world, raw)) = last[(y * w + x) as usize] else {
                    assert_eq!(uv_to_barycentric(&mesh, uv), None, "texel {x},{y}");
                    continue;
                };
                // The world point is world_at_uv's, byte for byte.
                let want = world_at_uv(&mesh, uv).expect("covered");
                assert_eq!(world.map(f32::to_bits), want.map(f32::to_bits), "{x},{y}");
                // The raw normal is the point lookup's WINNER's cross product.
                let hit = uv_to_barycentric(&mesh, uv).expect("covered");
                let idx = &mesh.indices[hit.triangle * 3..hit.triangle * 3 + 3];
                let [p0, p1, p2] = [0, 1, 2].map(|k| mesh.positions[idx[k] as usize]);
                let manual = raw_face_normal(p0, p1, p2);
                assert_eq!(raw.map(f32::to_bits), manual.map(f32::to_bits), "{x},{y}");
            }
        }
        // Texel (11, 9) (uv (0.71875, 0.40625)) is shared by tri 0 and the
        // top tri 2; tri 2 wins: (2,0,0)-(0.5,0.5,7) x (2,2,4)-(0.5,0.5,7)
        // = (1.5,-0.5,-7) x (1.5,1.5,-3) = (1.5+10.5, -10.5+4.5, 2.25+0.75)
        // = (12, -6, 3).
        assert_eq!(
            last[9 * 16 + 11].map(|(_, raw)| raw),
            Some([12.0, -6.0, 3.0])
        );
    }

    #[test]
    fn the_raw_normal_is_the_unnormalized_cross_product_in_index_order() {
        // Tri 0 = (0,0,0),(3,0,0),(3,1,4): (3,0,0) x (3,1,4) = (0, -12, 3),
        // NOT normalized (length sqrt(153)).
        let mut tilted = quad();
        tilted.positions = vec![
            [0.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [3.0, 1.0, 4.0],
            [0.0, 1.0, 0.0],
        ];
        let (last, _) = last_yields(&tilted, 8, 8);
        // Texel (7, 7): uv (0.9375, 0.0625), tri 0's alone.
        assert_eq!(last[7 * 8 + 7].map(|(_, raw)| raw), Some([0.0, -12.0, 3.0]));
        // Reversed winding flips it.
        tilted.indices = vec![0, 2, 1, 0, 3, 2];
        let (last, _) = last_yields(&tilted, 8, 8);
        assert_eq!(last[7 * 8 + 7].map(|(_, raw)| raw), Some([0.0, 12.0, -3.0]));
    }

    #[test]
    fn malformed_positions_are_visited_and_only_the_filter_skips() {
        // Tri 2 = (4, 1, 2) on top: vertex 4 has a valid UV.
        let mut mesh = quad();
        mesh.uvs.push([0.25, 0.25]);
        mesh.indices.extend_from_slice(&[4, 1, 2]);
        // (a) A NaN position: visited unfiltered, its world point NaN.
        mesh.positions.push([f32::NAN, 0.0, 0.0]);
        let (last, _) = last_yields(&mesh, 16, 16);
        let (world, raw) = last[9 * 16 + 11].expect("tri 2 is visited");
        assert!(world[0].is_nan() && raw.iter().any(|c| c.is_nan()));
        // (b) A MISSING position (index past `positions`): visited, NaN.
        mesh.positions.pop();
        let (last, _) = last_yields(&mesh, 16, 16);
        let (world, raw) = last[9 * 16 + 11].expect("tri 2 is visited");
        assert!(world.iter().all(|c| c.is_nan()) && raw.iter().all(|c| c.is_nan()));
        // (c) The filter skips it: tri 0 owns the texel again, and the
        // filter saw every UV-valid triangle once, in index order.
        let mut offered = Vec::new();
        let mut owner = Vec::new();
        let current = std::cell::Cell::new(usize::MAX);
        for_each_uv_geometry(
            &mesh,
            1001,
            16,
            16,
            |tri| {
                offered.push(tri);
                current.set(tri);
                tri != 2
            },
            |x, y, _, _| {
                if (x, y) == (11, 9) {
                    owner.push(current.get());
                }
            },
        );
        assert_eq!(offered, vec![0, 1, 2]);
        assert_eq!(owner, vec![0], "only tri 0 yields the texel");
    }

    #[test]
    fn a_zero_sized_target_or_an_empty_mesh_visits_nothing() {
        let mut yields = 0;
        for (mesh, w, h) in [(quad(), 0, 8), (quad(), 8, 0), (MeshData::default(), 8, 8)] {
            for_each_uv_geometry(&mesh, 1001, w, h, |_| true, |_, _, _, _| yields += 1);
        }
        assert_eq!(yields, 0);
    }

    /// A NEEDLE: three nearly-collinear UVs along the diagonal. Its area
    /// clears `UV_DET_EPS` (it is a VALID footprint), but the triangle is
    /// so thin that the barycentric solve is ill-conditioned.
    ///
    /// The UVs are the exact ones the dragon's fuzz found diverging
    /// (area -1.49e-8).
    fn needle() -> MeshData {
        MeshData {
            positions: vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0]],
            uvs: vec![
                [0.1614995, 0.16149949],
                [0.35829717, 0.3582972],
                [0.57094914, 0.57094914],
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        }
    }

    /// The walk and the POINT LOOKUP differ on a needle, and the point
    /// lookup is the one that is wrong (R-A2 v2d).
    ///
    /// `UvTri::bary` tests only the LOWER bound of each barycentric
    /// weight (`b_i >= -UV_BARY_EPS`) and never the upper bound. On a
    /// needle the edge-function formula stops being a barycentric
    /// DECOMPOSITION: the weights it returns do not reconstruct the point
    /// they came from. Texel (15, 0) — uv (0.96875, 0.96875), far
    /// outside the needle's footprint — gets weights `[0.0, 0.0, 2.0]`,
    /// which reconstruct to `2 * uv2 = (1.1419, 1.1419)`, NOT the texel
    /// centre. Every component clears the lower-bound test, so the point
    /// lookup (`uv_to_barycentric`, and everything built on it) CLAIMS
    /// that texel.
    ///
    /// The walk tests the same `bary` but only inside the triangle's
    /// bounding-box window, so it does not claim it. This pin states the
    /// difference EXACTLY, so a future change to either side has to face
    /// it:
    ///
    /// - the walk's texel set is a SUBSET of the point lookup's here (it
    ///   never claims a texel the lookup misses);
    /// - the texels the lookup claims and the walk does not are ones the
    ///   lookup's weights cannot reconstruct — they are not barycentric
    ///   decompositions at all;
    /// - on a WELL-CONDITIONED mesh the two agree exactly.
    ///
    /// The GPU bake shader quotes the same lower-bound-only formula
    /// (`crates/umber-gpu/src/bake_shaders.rs`), so the GPU has the same
    /// blind spot: fixing it is a separate kill (one formula, both sides).
    #[test]
    fn a_needle_is_claimed_outside_its_footprint_by_the_point_lookup_but_not_by_the_walk() {
        let mesh = needle();
        let t = UvTri::of(&mesh, 0).expect("the needle is a valid footprint");
        // It IS a valid (non-degenerate) footprint: the area clears the
        // epsilon, so nothing filters it out.
        assert!(t.area.abs() >= UV_DET_EPS, "area {}", t.area);

        let (w, h) = (16u32, 16u32);
        let (last, _) = last_yields(&mesh, w, h);
        let mut claimed_only_by_the_lookup = 0usize;
        for y in 0..h {
            for x in 0..w {
                let uv = texel_center_uv(0.0, 0.0, x, y, w, h);
                let hit = uv_to_barycentric(&mesh, uv);
                let walked = last[(y * w + x) as usize].is_some();
                match hit {
                    None => assert!(!walked, "the walk claims texel {x},{y} the lookup misses"),
                    Some(hit) => {
                        if !walked {
                            claimed_only_by_the_lookup += 1;
                            // The weights must NOT reconstruct the point: a
                            // barycentric decomposition always does.
                            let q = [
                                hit.bary[0] * t.uv[0][0]
                                    + hit.bary[1] * t.uv[1][0]
                                    + hit.bary[2] * t.uv[2][0],
                                hit.bary[0] * t.uv[0][1]
                                    + hit.bary[1] * t.uv[1][1]
                                    + hit.bary[2] * t.uv[2][1],
                            ];
                            let err = ((q[0] - uv[0]).powi(2) + (q[1] - uv[1]).powi(2)).sqrt();
                            assert!(
                                err > 1e-3,
                                "texel {x},{y}: the claimed weights reconstruct {q:?}, \
                                 not the texel centre {uv:?} — the ill-conditioned case \
                                 did not happen"
                            );
                        }
                    }
                }
            }
        }
        // The difference is REAL (the fixture exercises it) and the walk
        // never claims a texel the lookup misses.
        assert!(
            claimed_only_by_the_lookup > 0,
            "the needle fixture no longer exercises the difference"
        );
        // And on a well-conditioned mesh the two agree exactly.
        let quad_mesh = quad();
        let (last, _) = last_yields(&quad_mesh, 16, 16);
        for y in 0..16u32 {
            for x in 0..16u32 {
                let uv = texel_center_uv(0.0, 0.0, x, y, 16, 16);
                assert_eq!(
                    last[(y * 16 + x) as usize].is_some(),
                    uv_to_barycentric(&quad_mesh, uv).is_some(),
                    "texel {x},{y}"
                );
            }
        }
    }
}
