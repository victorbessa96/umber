//! Polygon fill (§2 line 24's P1 row: "Polygon fill
//! (triangle/polygon/mesh/UV-chunk pixel-mask fills)").
//!
//! The four pixel-mask fills, as one boolean-mask family consumed by the
//! SAME [`FillParams`](crate::fill::FillParams) path: [`FillMask`] gates
//! [`evaluate_fill_tile`](crate::fill::evaluate_fill_tile)'s coverage, so
//! the source (solid/gradient/noise/graph) and the projection
//! (UV/tri-planar/planar/spherical/cylindrical) keep working unchanged —
//! the mask decides WHERE the fill paints, never WHAT.
//!
//! * `Polygon` — the 2D UV lasso: even-odd containment of the texel's
//!   global UV in the click-path loop.
//! * `Triangle` — the single picked face (the [`RayHit`](crate::raycast::RayHit)
//!   triangle, or the UV view's [`uv_triangle_at`] pick).
//! * `Mesh` — the whole connected sub-mesh: the position-welded component
//!   of the picked face, resolved to triangle IDs at pick time.
//! * `UvChunk` — the UV island: the UV-welded component of the picked
//!   face, resolved to triangle IDs at pick time.
//!
//! # Rasterizer choice: scanline even-odd, not barycentric edge functions
//!
//! The fill's triangle walk ([`rasterize_uv_geometry`](crate::fill::rasterize_uv_geometry))
//! uses barycentric edge functions because its inputs ARE triangles
//! (convex, one winding). A lasso loop is an arbitrary concave,
//! possibly self-intersecting polygon — edge functions do not apply, and
//! triangulating it first (ear clipping) adds an O(n²), degeneracy-prone
//! step for zero benefit: we sample discrete texel centers, never
//! interpolate attributes across the polygon.
//!
//! So the lasso rasterizes as a bounded scanline: rows clipped to the
//! loop's UV bbox, one even-odd ray cast per texel center
//! ([`point_in_polygon`]). Even-odd (not nonzero winding) because a
//! hand-drawn lasso self-intersects, and parity gives it a deterministic
//! reading: doubly-covered regions read as OUTSIDE. No active-edge table:
//! the bbox-clipped per-texel ray cast IS the scanline math with the sweep
//! state factored out, sharing the single boundary-inclusive predicate
//! with the point tests so in/out/edge/vertex agree by construction.
//!
//! # Edges and anti-aliasing
//!
//! Boundary texels (center on an edge or vertex) count as INSIDE — the
//! fill includes its lasso edge, matching the coverage convention where
//! footprint edges are covered. Coverage stays boolean: anti-aliased
//! edges are deferred (the composite path multiplies f32 coverage and
//! dithers downstream, so a future supersampled edge plugs into
//! [`FillMask::covers`] without changing any gate).
//!
//! # Conventions (the crate's no-panic rule)
//!
//! Every mesh-index access goes through [`slice::get`]; malformed
//! triangles, missing attributes and non-finite coordinates degrade to
//! "outside the mask", never a panic. A degenerate loop (< 3 points)
//! contains nothing. A malformed pick seed floods to the empty set (the
//! fill paints nothing).
//!
//! # Module placement
//!
//! Same dependency profile as its neighbors [`crate::fill`] and
//! [`crate::raycast`]: pure functions of [`MeshData`](crate::MeshData)
//! plus plain parameters — no GPU, no tiles, no egui. The lasso-tool
//! state, the panel rows and the symmetry/quick-mask gates live in
//! `umber-app`'s `fill_layer` module, which calls into here.

use std::collections::HashMap;
use std::mem::size_of;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::MeshData;
use crate::raycast::{ray_intersect, RayHit};

/// The lasso click-path cap (v1): further clicks are ignored, never an
/// error — a 256-gon already covers any hand-drawn lasso, and the bound
/// keeps the per-texel ray cast proportional to intent, not accidents.
pub const LASSO_MAX_POINTS: usize = 256;

/// Boundary tolerance, UV units: a texel center within this of a lasso
/// edge counts as ON the edge (hence inside).
const EDGE_EPS: f32 = 1e-6;

/// The polygon fill's pixel mask: which texels a fill layer may paint.
///
/// Topology modes resolve to triangle-ID sets AT PICK TIME (the
/// constructors flood the mesh once), so evaluation stays a membership
/// test — tile-size-independent and live-re-evaluable like every other
/// [`FillParams`](crate::fill::FillParams) field. ID vecs are sorted and
/// deduplicated at construction (binary-search membership in
/// [`FillMask::covers`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FillMask {
    /// The 2D UV lasso: texels whose global UV is even-odd-inside
    /// `loop_uv` (boundary inclusive).
    Polygon {
        /// The click-path loop, global UVs (tile offsets included — a
        /// lasso drawn in one tile's UV view carries that tile's range).
        loop_uv: Vec<[f32; 2]>,
    },
    /// The single picked face: only texels rasterized from `triangle`
    /// (index into `mesh.indices / 3`, the [`RayHit`](crate::raycast::RayHit)
    /// convention) are painted.
    Triangle {
        /// The picked triangle index.
        triangle: usize,
    },
    /// The whole connected sub-mesh (mesh-ID based): the position-welded
    /// component of the picked face — coincident split vertices merge,
    /// so exporter-split islands still flood as one.
    Mesh {
        /// The flooded triangle IDs, ascending.
        triangles: Vec<usize>,
    },
    /// The UV island (UV-chunk): the UV-welded component of the picked
    /// face — same position but different UVs stays a separate chunk.
    UvChunk {
        /// The flooded triangle IDs, ascending.
        triangles: Vec<usize>,
    },
}

impl FillMask {
    /// Builds the lasso mask from a click-path loop (global UVs).
    pub fn polygon(loop_uv: Vec<[f32; 2]>) -> Self {
        FillMask::Polygon { loop_uv }
    }

    /// Builds the single-triangle mask from a pick.
    pub fn triangle(triangle: usize) -> Self {
        FillMask::Triangle { triangle }
    }

    /// Builds the whole-mesh mask: floods `mesh`'s position-welded
    /// connectivity from `seed` (see [`flood_connected_triangles`]). A
    /// malformed seed floods to the empty set — the fill paints nothing,
    /// never panics.
    pub fn mesh_island(mesh: &MeshData, seed: usize) -> Self {
        FillMask::Mesh {
            triangles: flood_connected_triangles(mesh, seed),
        }
    }

    /// Builds the UV-chunk mask: floods `mesh`'s UV-welded connectivity
    /// from `seed` (see [`flood_uv_island`]). Same empty-set degradation
    /// as [`FillMask::mesh_island`].
    pub fn uv_chunk(mesh: &MeshData, seed: usize) -> Self {
        FillMask::UvChunk {
            triangles: flood_uv_island(mesh, seed),
        }
    }

    /// Approximate heap held by this mask (the loop points + the ID vec;
    /// everything else is inline). Feeds
    /// [`FillParams::memory_bytes`](crate::fill::FillParams::memory_bytes).
    pub fn memory_bytes(&self) -> usize {
        size_of::<FillMask>()
            + match self {
                FillMask::Polygon { loop_uv } => loop_uv.len() * size_of::<[f32; 2]>(),
                FillMask::Triangle { .. } => 0,
                FillMask::Mesh { triangles } | FillMask::UvChunk { triangles } => {
                    triangles.len() * size_of::<usize>()
                }
            }
    }

    /// Whether the texel with global UV `uv`, rasterized from triangle
    /// `triangle`, is inside the mask: the per-texel gate the fill
    /// evaluator calls. Pure — no allocation on any arm (the polygon arm
    /// is one ray cast over the loop; the topology arms are a comparison
    /// or a binary search over the pick-time ID set).
    pub fn covers(&self, uv: [f32; 2], triangle: usize) -> bool {
        match self {
            FillMask::Polygon { loop_uv } => point_in_polygon(uv, loop_uv),
            FillMask::Triangle { triangle: t } => triangle == *t,
            FillMask::Mesh { triangles } | FillMask::UvChunk { triangles } => {
                triangles.binary_search(&triangle).is_ok()
            }
        }
    }
}

/// Even-odd containment of `point` in the `loop_uv` polygon (global UVs):
/// a +u ray from the point toggles inside-ness at every loop edge it
/// crosses strictly right of the point. Boundary (on an edge or a vertex)
/// counts as INSIDE.
///
/// Degenerate loops (< 3 points) contain nothing; non-finite points or
/// edges degrade to "outside" (NaN comparisons never toggle), never NaN
/// propagation.
pub fn point_in_polygon(point: [f32; 2], loop_uv: &[[f32; 2]]) -> bool {
    if loop_uv.len() < 3 || !point[0].is_finite() || !point[1].is_finite() {
        return false;
    }
    // Boundary first: the inclusive rule lives in exactly one place.
    for w in loop_uv.windows(2) {
        if on_segment(point, w[0], w[1]) {
            return true;
        }
    }
    if let (Some(&first), Some(&last)) = (loop_uv.first(), loop_uv.last()) {
        if on_segment(point, last, first) {
            return true;
        }
    }
    let mut inside = false;
    for w in loop_uv.windows(2) {
        if crosses_right(point, w[0], w[1]) {
            inside = !inside;
        }
    }
    if let (Some(&first), Some(&last)) = (loop_uv.first(), loop_uv.last()) {
        if crosses_right(point, last, first) {
            inside = !inside;
        }
    }
    inside
}

/// Whether the closed segment `a`–`b` contains `p` (within [`EDGE_EPS`]).
/// Non-finite inputs never contain; a degenerate (zero-length) edge
/// contains only its own neighborhood.
fn on_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> bool {
    if ![p[0], p[1], a[0], a[1], b[0], b[1]]
        .iter()
        .all(|v| v.is_finite())
    {
        return false;
    }
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    if len2 <= 0.0 {
        let d2 = ap[0] * ap[0] + ap[1] * ap[1];
        return d2 <= EDGE_EPS * EDGE_EPS;
    }
    // Perpendicular distance via the cross product, scaled by |ab|.
    let cross = ab[0] * ap[1] - ab[1] * ap[0];
    if cross.abs() > EDGE_EPS * (len2.sqrt() + EDGE_EPS) {
        return false;
    }
    let dot = ap[0] * ab[0] + ap[1] * ab[1];
    dot >= -EDGE_EPS && dot <= len2 + EDGE_EPS
}

/// Whether the +u ray from `p` crosses the edge `a`–`b` strictly right of
/// `p` (the even-odd toggle). The straddle test implies `a[1] != b[1]`,
/// so the divide is exact-safe; NaN endpoints never straddle.
fn crosses_right(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> bool {
    if (a[1] > p[1]) == (b[1] > p[1]) {
        return false;
    }
    let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
    p[0] < x
}

/// Rasterizes a UV lasso loop into `tile`'s `width`x`height` texel grid:
/// `true` per texel whose center is [`point_in_polygon`]-inside the loop.
/// Row-major top-down (`y = 0` is the top row, the paint-target layout),
/// global-UV texel centers on the bake convention
/// (`u = tu + (x + 0.5) / w`, `v = tv + 1 - (y + 0.5) / h`).
///
/// Rows are clipped to the loop's UV bbox (the bounded scanline: cost is
/// proportional to the lasso's area, not the tile's), and degenerate
/// loops / zero-sized targets yield all-`false` / empty vecs.
pub fn rasterize_polygon_mask(
    loop_uv: &[[f32; 2]],
    tile: u16,
    width: u32,
    height: u32,
) -> Vec<bool> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut out = vec![false; w * h];
    if loop_uv.len() < 3 {
        return out;
    }
    let (mut min_u, mut max_u) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut min_v, mut max_v) = (f32::INFINITY, f32::NEG_INFINITY);
    for uv in loop_uv {
        if !uv[0].is_finite() || !uv[1].is_finite() {
            continue;
        }
        min_u = min_u.min(uv[0]);
        max_u = max_u.max(uv[0]);
        min_v = min_v.min(uv[1]);
        max_v = max_v.max(uv[1]);
    }
    if !(min_u <= max_u && min_v <= max_v) {
        return out;
    }
    let index = tile.saturating_sub(crate::FIRST_TILE);
    let (tu, tv) = (f32::from(index % 10), f32::from(index / 10));
    let x0 = (((min_u - tu) * width as f32 - 0.5).floor() as i64).max(0) as usize;
    let x1 = ((((max_u - tu) * width as f32 - 0.5).floor() as i64).min(width as i64 - 1)).max(0)
        as usize;
    let y0 = ((((tv + 1.0 - max_v) * height as f32 - 0.5).floor() as i64).max(0)) as usize;
    let y1 = (((((tv + 1.0 - min_v) * height as f32 - 0.5).floor() as i64)
        .min(height as i64 - 1))
    .max(0)) as usize;
    if x0 > x1 || y0 > y1 {
        return out;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let uv = [
                tu + (x as f32 + 0.5) / width as f32,
                tv + 1.0 - (y as f32 + 0.5) / height as f32,
            ];
            out[y * w + x] = point_in_polygon(uv, loop_uv);
        }
    }
    out
}

/// A position-weld key for one vertex: the position bits when present, a
/// per-index unique sentinel otherwise (a vertex with no position merges
/// with nothing — missing data isolates, never leaks across the mesh).
fn pos_key(mesh: &MeshData, idx: u32) -> (u8, [u32; 3]) {
    match mesh.positions.get(idx as usize) {
        Some(p) if p.iter().all(|v| v.is_finite()) => {
            (0, [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
        }
        _ => (1, [idx, u32::MAX, u32::MAX]),
    }
}

/// A UV-weld key for one vertex: position bits + UV bits when both are
/// present, a per-index unique sentinel otherwise (a vertex with no UV
/// is its own island — same position but split UVs stays split, which is
/// exactly the island definition).
fn uv_key(mesh: &MeshData, idx: u32) -> (u8, [u32; 5]) {
    match (
        mesh.positions.get(idx as usize),
        mesh.uvs.get(idx as usize),
    ) {
        (Some(p), Some(uv))
            if p.iter().all(|v| v.is_finite()) && uv.iter().all(|v| v.is_finite()) =>
        {
            (
                0,
                [
                    p[0].to_bits(),
                    p[1].to_bits(),
                    p[2].to_bits(),
                    uv[0].to_bits(),
                    uv[1].to_bits(),
                ],
            )
        }
        _ => (1, [idx, u32::MAX, u32::MAX, u32::MAX, u32::MAX]),
    }
}

/// Floods one connectivity over the triangle list from `seed`: triangles
/// sharing any weld `key` are neighbors. Returns the component's triangle
/// indices, ascending and deduplicated. A `seed` past the end, or one
/// whose triangle is malformed, floods to the empty set.
fn flood_by_key<K: Eq + std::hash::Hash + Copy>(
    mesh: &MeshData,
    seed: usize,
    key: impl Fn(&MeshData, u32) -> K,
) -> Vec<usize> {
    let tri_count = mesh.indices.len() / 3;
    if seed >= tri_count {
        return Vec::new();
    }
    // Weld key -> triangles carrying it (malformed triangles contribute
    // no keys, so they are unreachable islands of one — and the seed
    // check below degrades them to the empty set).
    let mut owners: HashMap<K, Vec<usize>> = HashMap::new();
    let mut tri_keys: Vec<[Option<K>; 3]> = Vec::with_capacity(tri_count);
    for (k, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (Some(&i0), Some(&i1), Some(&i2)) = (tri.first(), tri.get(1), tri.get(2)) else {
            tri_keys.push([None, None, None]);
            continue;
        };
        let keys = [Some(key(mesh, i0)), Some(key(mesh, i1)), Some(key(mesh, i2))];
        for kk in keys.iter().flatten() {
            owners.entry(*kk).or_default().push(k);
        }
        tri_keys.push(keys);
    }
    let Some(seed_keys) = tri_keys.get(seed) else {
        return Vec::new();
    };
    if seed_keys.iter().all(|k| k.is_none()) {
        return Vec::new();
    }
    let mut visited = vec![false; tri_count];
    let mut stack = vec![seed];
    visited[seed] = true;
    while let Some(k) = stack.pop() {
        let Some(keys) = tri_keys.get(k) else {
            continue;
        };
        for kk in keys.iter().flatten() {
            if let Some(peers) = owners.get(kk) {
                for &peer in peers {
                    if !visited[peer] {
                        visited[peer] = true;
                        stack.push(peer);
                    }
                }
            }
        }
    }
    visited
        .into_iter()
        .enumerate()
        .filter_map(|(k, v)| v.then_some(k))
        .collect()
}

/// The mesh-mode flood: the position-welded component of `seed`'s
/// triangle — faces sharing a (bit-identical) position are one sub-mesh,
/// so exporter-split coincident vertices still flood together, while
/// genuinely disjoint shells stay separate. Ascending IDs; empty on a
/// malformed seed.
pub fn flood_connected_triangles(mesh: &MeshData, seed: usize) -> Vec<usize> {
    flood_by_key(mesh, seed, pos_key)
}

/// The UV-chunk flood: the UV-welded component of `seed`'s triangle —
/// faces sharing a (position, UV) pair are one island, so a seam (same
/// position, different UVs) stops the flood. Ascending IDs; empty on a
/// malformed seed.
pub fn flood_uv_island(mesh: &MeshData, seed: usize) -> Vec<usize> {
    flood_by_key(mesh, seed, uv_key)
}

/// The UV-view pick: the triangle whose UV footprint contains `uv`, or
/// `None` on a miss. Same edge-function math and tolerance as the fill's
/// [`rasterize_uv_geometry`](crate::fill::rasterize_uv_geometry), and the
/// same overlap rule — the LAST triangle in index order wins — so the
/// pick agrees with what the fill would rasterize there. Malformed,
/// UV-degenerate and non-finite triangles are skipped, never panics.
pub fn uv_triangle_at(mesh: &MeshData, uv: [f32; 2]) -> Option<usize> {
    if !uv[0].is_finite() || !uv[1].is_finite() {
        return None;
    }
    let mut found = None;
    for (k, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (Some(&i0), Some(&i1), Some(&i2)) = (tri.first(), tri.get(1), tri.get(2)) else {
            continue;
        };
        let (Some(&uv0), Some(&uv1), Some(&uv2)) = (
            mesh.uvs.get(i0 as usize),
            mesh.uvs.get(i1 as usize),
            mesh.uvs.get(i2 as usize),
        ) else {
            continue;
        };
        if ![uv0, uv1, uv2]
            .iter()
            .all(|t| t[0].is_finite() && t[1].is_finite())
        {
            continue;
        }
        let area = (uv1[0] - uv0[0]) * (uv2[1] - uv0[1]) - (uv2[0] - uv0[0]) * (uv1[1] - uv0[1]);
        if !area.is_finite() || area.abs() < 1e-12 {
            continue;
        }
        let e0 = (uv[0] - uv1[0]) * (uv2[1] - uv1[1]) - (uv[1] - uv1[1]) * (uv2[0] - uv1[0]);
        let e1 = (uv[0] - uv2[0]) * (uv0[1] - uv2[1]) - (uv[1] - uv2[1]) * (uv0[0] - uv2[0]);
        let e2 = (uv[0] - uv0[0]) * (uv1[1] - uv0[1]) - (uv[1] - uv0[1]) * (uv1[0] - uv0[0]);
        let (b0, b1, b2) = (e0 / area, e1 / area, e2 / area);
        let inside = if area > 0.0 {
            b0 >= -1e-6 && b1 >= -1e-6 && b2 >= -1e-6
        } else {
            b0 <= 1e-6 && b1 <= 1e-6 && b2 <= 1e-6
        };
        if inside {
            found = Some(k);
        }
    }
    found
}

/// The lasso→mesh-intersection path for 3D-viewport picks: casts `ray`
/// (`origin`, `dir` in the viewport's world units — the caller builds it
/// from the NDC unprojection) against the mesh, returning the nearest
/// hit. Thin wrapper over [`ray_intersect`](crate::raycast::ray_intersect)
/// so the polygon tool depends on one documented entry point, not on the
/// Möller–Trumbore primitive directly.
pub fn pick_from_ray(mesh: &MeshData, origin: [f32; 3], dir: [f32; 3]) -> Option<RayHit> {
    ray_intersect(mesh, Vec3::from_array(origin), Vec3::from_array(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit square loop, CCW: (0,0) → (1,0) → (1,1) → (0,1).
    fn square() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    /// Test (a): even-odd containment — inside, outside, on-edge,
    /// on-vertex.
    #[test]
    fn square_contains_inside_and_boundary_but_not_outside() {
        let sq = square();
        assert!(point_in_polygon([0.5, 0.5], &sq), "center inside");
        assert!(point_in_polygon([0.01, 0.99], &sq), "near corner inside");
        for outside in [
            [1.5, 0.5],
            [-0.1, 0.5],
            [0.5, 1.01],
            [0.5, -0.5],
            [2.0, 2.0],
            [-1.0, -1.0],
        ] {
            assert!(!point_in_polygon(outside, &sq), "{outside:?} outside");
        }
        // Boundary is inclusive: edges…
        for edge in [
            [1.0, 0.5],
            [0.0, 0.5],
            [0.5, 0.0],
            [0.5, 1.0],
            [0.25, 0.0],
        ] {
            assert!(point_in_polygon(edge, &sq), "{edge:?} on edge");
        }
        // …and vertices.
        for vertex in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            assert!(point_in_polygon(vertex, &sq), "{vertex:?} on vertex");
        }
    }

    /// Concave loops read correctly: the notch is outside, the bars are
    /// inside (barycentric edge functions cannot express this — the
    /// rasterizer-choice test).
    #[test]
    fn concave_notch_reads_outside() {
        // L-shape: full square minus the top-right quadrant.
        let ell = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 0.5],
            [0.5, 0.5],
            [0.5, 1.0],
            [0.0, 1.0],
        ];
        assert!(point_in_polygon([0.25, 0.75], &ell), "left bar inside");
        assert!(point_in_polygon([0.25, 0.25], &ell), "bottom bar inside");
        assert!(!point_in_polygon([0.75, 0.75], &ell), "notch outside");
        assert!(point_in_polygon([0.5, 0.5], &ell), "notch corner on edge");
    }

    /// Self-intersecting lassos read even-odd: the bowtie's wings are
    /// singly-covered (inside), the far corners outside, and the crossing
    /// itself is boundary (inside).
    #[test]
    fn self_intersecting_bowtie_reads_even_odd() {
        let bow = vec![[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]];
        assert!(point_in_polygon([0.8, 0.2], &bow), "lower wing inside");
        assert!(!point_in_polygon([0.2, 0.8], &bow), "upper wing outside");
        assert!(point_in_polygon([0.5, 0.5], &bow), "crossing is boundary");
        assert!(!point_in_polygon([0.1, 0.5], &bow), "off-loop outside");
    }

    /// Degenerate inputs contain nothing, never panic.
    #[test]
    fn degenerate_loops_contain_nothing() {
        let sq = square();
        for empty in [&[][..], &[ [0.0, 0.0] ][..], &sq[..2]] {
            assert!(!point_in_polygon([0.5, 0.5], empty));
        }
        assert!(!point_in_polygon([f32::NAN, 0.5], &sq));
        assert!(!point_in_polygon([0.5, f32::INFINITY], &sq));
        assert!(!point_in_polygon([0.5, 0.5], &[[0.0, 0.0], [f32::NAN, 0.0], [1.0, 1.0]]));
        // A zero-length edge is its own neighborhood, not a hole.
        assert!(point_in_polygon(
            [0.5, 0.5],
            &[[0.5, 0.5], [0.5, 0.5], [1.0, 1.0]]
        ));
    }

    /// Two quads sharing positions but split UVs: one position-island,
    /// two UV islands. Quad A = verts 0–3, tris (0,1,2),(0,2,3); quad B =
    /// verts 4–7, tris (4,5,6),(4,6,7), coincident positions, offset UVs.
    fn split_island_mesh() -> MeshData {
        let pos = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        MeshData {
            positions: vec![
                pos[0], pos[1], pos[2], pos[3], pos[0], pos[1], pos[2], pos[3],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 8],
            uvs: vec![
                [0.0, 0.0],
                [0.5, 0.0],
                [0.5, 0.5],
                [0.0, 0.5],
                [0.5, 0.5],
                [1.0, 0.5],
                [1.0, 1.0],
                [0.5, 1.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
            material_names: vec![],
        }
    }

    /// Test (b): the UV-island flood stops at the seam — same positions,
    /// different UVs are separate chunks.
    #[test]
    fn uv_island_flood_stops_at_the_seam() {
        let mesh = split_island_mesh();
        assert_eq!(flood_uv_island(&mesh, 0), vec![0, 1]);
        assert_eq!(flood_uv_island(&mesh, 1), vec![0, 1]);
        assert_eq!(flood_uv_island(&mesh, 2), vec![2, 3]);
        assert_eq!(flood_uv_island(&mesh, 3), vec![2, 3]);
        assert_eq!(flood_uv_island(&mesh, 99), Vec::<usize>::new());
    }

    /// Test (c): the mesh-ID flood merges coincident split vertices —
    /// one connected sub-mesh across the UV seam.
    #[test]
    fn mesh_flood_merges_coincident_split_vertices() {
        let mesh = split_island_mesh();
        assert_eq!(flood_connected_triangles(&mesh, 0), vec![0, 1, 2, 3]);
        assert_eq!(flood_connected_triangles(&mesh, 2), vec![0, 1, 2, 3]);
        assert_eq!(flood_connected_triangles(&mesh, 99), Vec::<usize>::new());
        // A genuinely disjoint shell stays separate.
        let mut two = mesh.clone();
        let base = two.positions.len() as u32;
        two.positions.extend_from_slice(&[
            [9.0, 9.0, 0.0],
            [10.0, 9.0, 0.0],
            [9.5, 10.0, 0.0],
        ]);
        two.normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 3]);
        two.uvs
            .extend_from_slice(&[[0.1, 0.1], [0.2, 0.1], [0.15, 0.2]]);
        two.indices.extend_from_slice(&[base, base + 1, base + 2]);
        assert_eq!(flood_connected_triangles(&two, 0), vec![0, 1, 2, 3]);
        assert_eq!(flood_connected_triangles(&two, 4), vec![4]);
    }

    /// The mask constructors resolve floods at pick time, sorted and
    /// deduplicated, and `covers` is the membership test.
    #[test]
    fn mask_constructors_resolve_and_covers_tests_membership() {
        let mesh = split_island_mesh();
        let uv_chunk = FillMask::uv_chunk(&mesh, 0);
        assert_eq!(
            uv_chunk,
            FillMask::UvChunk {
                triangles: vec![0, 1]
            }
        );
        assert!(uv_chunk.covers([0.25, 0.25], 0));
        assert!(uv_chunk.covers([0.25, 0.25], 1));
        assert!(!uv_chunk.covers([0.25, 0.25], 2));
        // The UV is irrelevant to topology modes (only the triangle ID).
        assert!(uv_chunk.covers([9.0, 9.0], 0));

        let island = FillMask::mesh_island(&mesh, 2);
        assert_eq!(
            island,
            FillMask::Mesh {
                triangles: vec![0, 1, 2, 3]
            }
        );
        assert!(island.covers([0.0, 0.0], 3));

        let tri = FillMask::triangle(2);
        assert!(!tri.covers([0.0, 0.0], 1));
        assert!(tri.covers([0.0, 0.0], 2));

        let poly = FillMask::polygon(square());
        assert!(poly.covers([0.5, 0.5], 999));
        assert!(!poly.covers([2.0, 2.0], 0));

        // Malformed seeds degrade to empty sets (nothing covered).
        let empty = FillMask::mesh_island(&mesh, 99);
        assert!(!empty.covers([0.25, 0.25], 0));
        assert_eq!(FillMask::uv_chunk(&mesh, 99), FillMask::UvChunk { triangles: vec![] });
    }

    /// The UV-view pick agrees with the rasterizer's overlap rule
    /// (last-in-index-order wins) and misses cleanly.
    #[test]
    fn uv_pick_returns_the_topmost_triangle_or_none() {
        let mesh = split_island_mesh();
        // Quad A's own UV window: tri 0 covers the lower-left half, tri 1
        // the upper-right; the diagonal belongs to both → last (1) wins.
        assert_eq!(uv_triangle_at(&mesh, [0.4, 0.1]), Some(0));
        assert_eq!(uv_triangle_at(&mesh, [0.1, 0.4]), Some(1));
        assert_eq!(uv_triangle_at(&mesh, [0.25, 0.25]), Some(1));
        assert_eq!(uv_triangle_at(&mesh, [0.75, 0.75]), Some(3));
        assert_eq!(uv_triangle_at(&mesh, [2.0, 2.0]), None);
        assert_eq!(uv_triangle_at(&mesh, [f32::NAN, 0.5]), None);
        assert_eq!(uv_triangle_at(&MeshData::default(), [0.5, 0.5]), None);
    }

    /// The 3D-viewport pick path: a ray down the quad's center hits, a
    /// parallel ray misses.
    #[test]
    fn ray_pick_hits_the_quad_and_misses_beside_it() {
        let mesh = split_island_mesh();
        let hit = pick_from_ray(&mesh, [0.5, 0.5, 5.0], [0.0, 0.0, -1.0]).expect("center hit");
        assert!(hit.triangle <= 1, "quad A's pair: {hit:?}");
        assert!(pick_from_ray(&mesh, [0.5, 0.5, 5.0], [0.0, 1.0, 0.0]).is_none());
        assert!(pick_from_ray(&MeshData::default(), [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).is_none());
    }

    /// The lasso rasterizer agrees with the point predicate texel by
    /// texel, clipped to the loop bbox (untouched rows stay false).
    #[test]
    fn lasso_raster_matches_the_point_predicate() {
        // Left-half loop: columns 0..4 covered, 4..8 clear (no texel
        // center sits on u = 0.5, so the boundary rule is unexercised
        // here — the predicate tests own it).
        let left = vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]];
        let mask = rasterize_polygon_mask(&left, 1001, 8, 8);
        assert_eq!(mask.len(), 64);
        for y in 0..8 {
            for x in 0..8 {
                let uv = [(x as f32 + 0.5) / 8.0, 1.0 - (y as f32 + 0.5) / 8.0];
                assert_eq!(
                    mask[y * 8 + x],
                    point_in_polygon(uv, &left),
                    "texel ({x},{y})"
                );
                assert_eq!(mask[y * 8 + x], x < 4, "texel ({x},{y})");
            }
        }
        // Degenerate + zero-size degrade without panic.
        assert!(rasterize_polygon_mask(&[], 1001, 8, 8).iter().all(|m| !m));
        assert!(rasterize_polygon_mask(&square(), 1001, 0, 8).is_empty());
        // A loop fully outside the tile covers nothing.
        let far = vec![[5.0, 5.0], [6.0, 5.0], [6.0, 6.0], [5.0, 6.0]];
        assert!(rasterize_polygon_mask(&far, 1001, 8, 8).iter().all(|m| !m));
    }
}
