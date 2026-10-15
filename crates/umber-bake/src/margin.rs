//! Bake margin types: Blender's "Adjacent Faces" margin (cross-UV-seam fill).
//!
//! Source: Blender manual, Cycles baking, Margin Type
//! (<https://docs.blender.org/manual/en/latest/render/cycles/baking.html>):
//! - **Extend** — "extend border pixels outwards": what [`crate::dilation`]
//!   already does on the GPU (nearest-donor ping-pong).
//! - **Adjacent Faces** — "fill margin using pixels from adjacent faces
//!   across UV seams": this module, pure CPU.
//!
//! # Algorithm
//!
//! Work happens in PIXEL space, using the bake convention shared by every
//! baker in this crate (`POSITION_BAKE_SHADER`): texel `(x, y)` has its
//! centre at UV `u = (x + 0.5) / w`, `v = 1 - (y + 0.5) / h`, so a UV point
//! maps to pixel coordinates `(u * w, (1 - v) * h)` and texel centres sit at
//! `(x + 0.5, y + 0.5)`. `margin_px` is therefore measured in texels.
//!
//! Every [`SeamEdge`] has two sides: its UV segment in `tri_a`'s island and
//! its UV segment in `tri_b`'s island. For each side (source `S`,
//! destination `D`) and each UNCOVERED texel (`a == 0`) with centre `p`:
//!
//! ```text
//! s = dot(p - S0, dS) / |dS|^2          (along-edge parameter, must be in [0, 1])
//! d = dot(p - S0, n_out_S)              (outward distance, must be in (0, margin_px])
//! q = D0 + s * dD + d * (|dD| / |dS|) * n_in_D
//! ```
//!
//! `n_out_S` is the unit perpendicular of the source edge pointing AWAY from
//! the source triangle's UV centroid; `n_in_D` points TOWARD the destination
//! triangle's UV centroid. So a margin texel at outward distance `d` beyond
//! island `S`'s seam samples the neighbouring face's INTERIOR at the
//! corresponding depth — the same 3D surface continuing across the cut.
//! The texel then takes the value of the texel containing `q` (NEAREST
//! sampling, `floor(q)`), read from the ORIGINAL map, if that texel is in
//! bounds and covered (`a != 0`); otherwise this candidate is dropped.
//!
//! # Judgment calls
//!
//! - **Nearest, not bilinear.** Exact byte copies (testable to the byte, no
//!   blending of covered with uncovered neighbours near the destination's
//!   own island edge). Bilinear is a follow-up if seams show stair-stepping.
//! - **Depth scaling `|dD| / |dS|`.** Matches `umber_mesh::seam_mirror`:
//!   the two islands may carry different texel densities along the same 3D
//!   edge, so the perpendicular depth is rescaled by the edge-length ratio
//!   (an isotropic-density approximation). The brief's unscaled `d` is the
//!   equal-length special case.
//! - **Perpendicular strip only.** `s` must be in `[0, 1]`: the wedge beyond
//!   a seam edge's endpoints is not filled here (the corresponding 3D
//!   neighbourhood belongs to a third face whose mapping needs vertex-fan
//!   adjacency). Those texels are left for Extend.
//! - **Closest seam wins.** A texel reachable from several seam sides takes
//!   the candidate with the smallest outward distance `d`; exact ties keep
//!   the first in `seams.edges` order (A side before B side). Deterministic.
//! - **Single pass, no chaining.** Destinations are read from the input
//!   map, never from texels this pass just filled.
//! - **Degenerate sides are skipped**, never divided by: zero-length UV
//!   edges and source/destination triangles whose UV centroid lies on the
//!   edge line (zero UV area — no inside/outside).
//!
//! # Composition with Extend
//!
//! Texels not reached (beyond `margin_px`, outside every seam strip, next to
//! non-seam island borders, or whose partner texel is uncovered) are
//! returned BYTE-IDENTICAL, still `a == 0`. The intended pipeline is
//! `adjacent_faces_margin` first, then [`crate::dilation::dilate_map`]
//! (Extend) over the result: Extend then grows from both the original
//! islands and the seam-filled strips, closing the corners and non-seam
//! borders. Running Extend first would cover the seam strip with
//! border-extended colour and leave this pass nothing to fill.
//!
//! # No panics
//!
//! All indexing into the map and mesh goes through checked arithmetic and
//! `slice::get`; malformed input returns a [`MarginError`].

use umber_mesh::seam::{SeamEdge, SeamGraph};
use umber_mesh::MeshData;

/// Bake margin fill strategy (Blender's Margin Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarginType {
    /// Extend island border pixels outwards ([`crate::dilation`]).
    Extend,
    /// Fill the margin from adjacent faces across UV seams
    /// ([`adjacent_faces_margin`]), then Extend the rest.
    AdjacentFaces,
}

/// Errors from [`adjacent_faces_margin`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MarginError {
    /// Width or height was zero.
    #[error("margin target dimensions must be non-zero (got {width}x{height})")]
    EmptyTarget {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// `map`'s byte length does not match `width * height * 4` (or that
    /// product overflows `usize`, reported as `expected: None`).
    #[error(
        "map length {len} does not match dimensions {width}x{height} (expected {expected:?} bytes)"
    )]
    SizeMismatch {
        /// The input slice's actual length in bytes.
        len: usize,
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// The expected `width * height * 4` byte count, `None` on overflow.
        expected: Option<usize>,
    },
    /// The seam graph was not built from this mesh: its per-triangle flag
    /// count differs from the mesh's triangle count, or a seam edge names
    /// a triangle the mesh does not have (or whose corners are missing).
    #[error(
        "seam graph does not match mesh: {seam_tris} seam-graph triangles vs {mesh_tris} mesh triangles (offending triangle {tri:?})"
    )]
    MeshSeamMismatch {
        /// `seams.tri_touches_seam.len()`.
        seam_tris: usize,
        /// `mesh.indices.len() / 3`.
        mesh_tris: usize,
        /// The first seam-edge triangle that could not be resolved, if the
        /// counts agreed.
        tri: Option<u32>,
    },
}

type V2 = [f32; 2];

fn sub(a: V2, b: V2) -> V2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: V2, b: V2) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// One directed side of a seam edge, in pixel space.
struct Side {
    /// Source edge start and direction.
    s0: V2,
    ds: V2,
    /// Source unit normal pointing away from the source triangle.
    n_out: V2,
    /// Destination edge start and direction.
    d0: V2,
    dd: V2,
    /// Destination unit normal pointing into the destination triangle.
    n_in: V2,
}

/// UV -> pixel space under the bake convention (see module docs).
fn to_px(uv: V2, w: f32, h: f32) -> V2 {
    [uv[0] * w, (1.0 - uv[1]) * h]
}

/// Pixel-space UV centroid of triangle `tri`, or `None` if the triangle or
/// any corner UV is missing.
fn tri_centroid_px(mesh: &MeshData, tri: u32, w: f32, h: f32) -> Option<V2> {
    let base = (tri as usize).checked_mul(3)?;
    let mut acc = [0.0f32; 2];
    for k in 0..3 {
        let idx = *mesh.indices.get(base.checked_add(k)?)? as usize;
        let p = to_px(*mesh.uvs.get(idx)?, w, h);
        acc[0] += p[0];
        acc[1] += p[1];
    }
    Some([acc[0] / 3.0, acc[1] / 3.0])
}

/// Unit perpendicular of `dir` pointing toward `toward` (relative to
/// `origin`); `None` when `toward` lies on the edge line.
fn normal_toward(origin: V2, dir: V2, len: f32, toward: V2) -> Option<V2> {
    let n = [dir[1] / len, -dir[0] / len];
    let side = dot(sub(toward, origin), n);
    if side > 0.0 {
        Some(n)
    } else if side < 0.0 {
        Some([-n[0], -n[1]])
    } else {
        None
    }
}

/// Builds the two directed sides of `edge` (A->B, B->A), skipping
/// degenerate ones. `Err(tri)` names an unresolvable triangle.
fn edge_sides(edge: &SeamEdge, mesh: &MeshData, w: f32, h: f32) -> Result<Vec<Side>, u32> {
    let ca = tri_centroid_px(mesh, edge.tri_a, w, h).ok_or(edge.tri_a)?;
    let cb = tri_centroid_px(mesh, edge.tri_b, w, h).ok_or(edge.tri_b)?;
    let a0 = to_px(edge.uv_a[0], w, h);
    let a1 = to_px(edge.uv_a[1], w, h);
    let b0 = to_px(edge.uv_b[0], w, h);
    let b1 = to_px(edge.uv_b[1], w, h);
    let (da, db) = (sub(a1, a0), sub(b1, b0));
    let (la, lb) = (dot(da, da).sqrt(), dot(db, db).sqrt());
    if !(la > 0.0 && lb > 0.0 && la.is_finite() && lb.is_finite()) {
        return Ok(Vec::new());
    }
    let (Some(n_in_a), Some(n_in_b)) =
        (normal_toward(a0, da, la, ca), normal_toward(b0, db, lb, cb))
    else {
        return Ok(Vec::new());
    };
    Ok(vec![
        Side {
            s0: a0,
            ds: da,
            n_out: [-n_in_a[0], -n_in_a[1]],
            d0: b0,
            dd: db,
            n_in: n_in_b,
        },
        Side {
            s0: b0,
            ds: db,
            n_out: [-n_in_b[0], -n_in_b[1]],
            d0: a0,
            dd: da,
            n_in: n_in_a,
        },
    ])
}

/// Fills uncovered texels (`a == 0`) within `margin_px` of a UV seam with
/// the adjacent face's texels across that seam (Blender "Adjacent Faces").
///
/// `map` is RGBA8, row-major, `width * height * 4` bytes, row 0 at UV
/// `v = 1` (the crate's bake convention). `seams` must come from
/// `umber_mesh::seam::build_seam_graph(mesh)`. Texels this pass does not
/// reach are returned unchanged — run Extend ([`crate::dilation`]) after
/// it (see the module docs). `margin_px == 0` or an empty seam graph
/// returns a copy of the input.
pub fn adjacent_faces_margin(
    map: &[u8],
    width: u32,
    height: u32,
    mesh: &MeshData,
    seams: &SeamGraph,
    margin_px: u32,
) -> Result<Vec<u8>, MarginError> {
    if width == 0 || height == 0 {
        return Err(MarginError::EmptyTarget { width, height });
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if expected != Some(map.len()) {
        return Err(MarginError::SizeMismatch {
            len: map.len(),
            width,
            height,
            expected,
        });
    }
    let mesh_tris = mesh.indices.len() / 3;
    let seam_tris = seams.tri_touches_seam.len();
    if seam_tris != mesh_tris {
        return Err(MarginError::MeshSeamMismatch {
            seam_tris,
            mesh_tris,
            tri: None,
        });
    }

    let (w, h) = (width as f32, height as f32);
    let mut sides = Vec::with_capacity(seams.edges.len() * 2);
    for edge in &seams.edges {
        let built = edge_sides(edge, mesh, w, h).map_err(|tri| MarginError::MeshSeamMismatch {
            seam_tris,
            mesh_tris,
            tri: Some(tri),
        })?;
        sides.extend(built);
    }

    let mut out = map.to_vec();
    if margin_px == 0 || sides.is_empty() {
        return Ok(out);
    }
    let m = margin_px as f32;
    let (wu, hu) = (width as usize, height as usize);
    // Best candidate per texel: (outward distance, rgba).
    let mut best: Vec<Option<(f32, [u8; 4])>> = vec![None; wu * hu];

    for side in &sides {
        let ls_sq = dot(side.ds, side.ds);
        let ratio = (dot(side.dd, side.dd) / ls_sq).sqrt();
        // Bounding box of the source strip (edge swept by n_out * m).
        let s1 = [side.s0[0] + side.ds[0], side.s0[1] + side.ds[1]];
        let off = [side.n_out[0] * m, side.n_out[1] * m];
        let xs = [side.s0[0], s1[0], side.s0[0] + off[0], s1[0] + off[0]];
        let ys = [side.s0[1], s1[1], side.s0[1] + off[1], s1[1] + off[1]];
        let (x_lo, x_hi) = (
            xs.iter().copied().fold(f32::INFINITY, f32::min),
            xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        );
        let (y_lo, y_hi) = (
            ys.iter().copied().fold(f32::INFINITY, f32::min),
            ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        );
        // Texel centres x + 0.5 in [x_lo, x_hi]; clamp to the image (the
        // float->usize casts saturate NaN/negatives to 0).
        let tx0 = (x_lo - 0.5).floor().max(0.0) as usize;
        let ty0 = (y_lo - 0.5).floor().max(0.0) as usize;
        let tx1 = ((x_hi - 0.5).ceil().max(0.0) as usize).min(wu - 1);
        let ty1 = ((y_hi - 0.5).ceil().max(0.0) as usize).min(hu - 1);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let ti = ty * wu + tx;
                let Some(alpha) = map.get(ti * 4 + 3) else {
                    continue;
                };
                if *alpha != 0 {
                    continue;
                }
                let p = [tx as f32 + 0.5, ty as f32 + 0.5];
                let rel = sub(p, side.s0);
                let s = dot(rel, side.ds) / ls_sq;
                let d = dot(rel, side.n_out);
                if !(0.0..=1.0).contains(&s) || !(d > 0.0 && d <= m) {
                    continue;
                }
                if matches!(best.get(ti), Some(Some((bd, _))) if *bd <= d) {
                    continue;
                }
                let depth = d * ratio;
                let q = [
                    side.d0[0] + s * side.dd[0] + depth * side.n_in[0],
                    side.d0[1] + s * side.dd[1] + depth * side.n_in[1],
                ];
                let (qx, qy) = (q[0].floor(), q[1].floor());
                if !(qx >= 0.0 && qy >= 0.0 && qx < w && qy < h) {
                    continue;
                }
                let qi = (qy as usize * wu + qx as usize) * 4;
                let Some(texel) = map.get(qi..qi + 4) else {
                    continue;
                };
                if texel[3] == 0 {
                    continue;
                }
                if let Some(slot) = best.get_mut(ti) {
                    *slot = Some((d, [texel[0], texel[1], texel[2], texel[3]]));
                }
            }
        }
    }

    for (ti, cand) in best.iter().enumerate() {
        if let (Some((_, rgba)), Some(dst)) = (cand, out.get_mut(ti * 4..ti * 4 + 4)) {
            dst.copy_from_slice(rgba);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_mesh::seam::build_seam_graph;

    const W: u32 = 16;
    const H: u32 = 8;

    /// Pixel -> UV under the bake convention (`u = px / W`, `v = 1 - py / H`);
    /// all values are dyadic, so the round trip is exact.
    fn uv(px: f32, py: f32) -> [f32; 2] {
        [px / W as f32, 1.0 - py / H as f32]
    }

    /// 3D quad A(0,0,0) B(1,0,0) C(1,1,0) D(0,1,0), tris (A,B,C) and
    /// (A,C,D) sharing 3D edge A-C, which is a UV cut. In pixel space:
    /// - tri 0 (island A): A=(6,1) C=(6,7) B=(0,7) — seam on x = 6, body left;
    /// - tri 1 (island B): A=(10,1) C=(10,7) D=(16,1) — seam on x = 10, body right.
    fn seamed_quad() -> MeshData {
        MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uvs: vec![
                uv(6.0, 1.0),
                uv(0.0, 7.0),
                uv(6.0, 7.0),
                uv(10.0, 1.0),
                uv(10.0, 7.0),
                uv(16.0, 1.0),
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec![],
        }
    }

    /// Island A = columns 0..6, island B = columns 10..16, rows 1..7, each
    /// texel a distinct colour `[x * 10, y * 10, island, 255]` so a copy
    /// pins exactly which partner texel was sampled. Columns 6..10 and rows
    /// 0 and 7 are uncovered (all zero).
    fn island_map() -> Vec<u8> {
        let mut m = vec![0u8; (W * H * 4) as usize];
        for y in 1..7u32 {
            for x in (0..6u32).chain(10..16) {
                let island = if x < 6 { 1 } else { 2 };
                let i = ((y * W + x) * 4) as usize;
                m[i..i + 4].copy_from_slice(&[(x * 10) as u8, (y * 10) as u8, island, 255]);
            }
        }
        m
    }

    fn texel(m: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * W + x) * 4) as usize;
        [m[i], m[i + 1], m[i + 2], m[i + 3]]
    }

    fn color(x: u32, y: u32, island: u8) -> [u8; 4] {
        [(x * 10) as u8, (y * 10) as u8, island, 255]
    }

    #[test]
    fn fixture_has_one_seam_with_expected_endpoints() {
        let g = build_seam_graph(&seamed_quad());
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].uv_a, [uv(6.0, 1.0), uv(6.0, 7.0)]);
        assert_eq!(g.edges[0].uv_b, [uv(10.0, 1.0), uv(10.0, 7.0)]);
    }

    /// margin 2: column 6 (d = 0.5 from A's seam) samples B at x = 10.5,
    /// column 7 (d = 1.5) at x = 11.5; column 9 (d = 0.5 from B's seam)
    /// samples A at x = 5.5, column 8 (d = 1.5) at x = 4.5. Same row each
    /// time (vertical, equal-length edges => s maps row to row).
    #[test]
    fn seam_margin_takes_partner_interior_texels() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        let input = island_map();
        let out = adjacent_faces_margin(&input, W, H, &mesh, &g, 2).expect("valid input");
        for y in 1..7 {
            assert_eq!(texel(&out, 6, y), color(10, y, 2), "col 6 row {y}");
            assert_eq!(texel(&out, 7, y), color(11, y, 2), "col 7 row {y}");
            assert_eq!(texel(&out, 8, y), color(4, y, 1), "col 8 row {y}");
            assert_eq!(texel(&out, 9, y), color(5, y, 1), "col 9 row {y}");
        }
    }

    /// Rows 0 and 7 lie beyond the seam's endpoints (s < 0 / s > 1) and
    /// row 7 is also outside tri 0's non-seam edge C-B: untouched. Covered
    /// texels are never rewritten.
    #[test]
    fn non_seam_borders_and_covered_texels_are_untouched() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        let input = island_map();
        let out = adjacent_faces_margin(&input, W, H, &mesh, &g, 4).expect("valid input");
        for x in 0..W {
            assert_eq!(texel(&out, x, 0), [0; 4], "row 0 col {x}");
            assert_eq!(texel(&out, x, 7), [0; 4], "row 7 col {x}");
            for y in 1..7 {
                if !(6..10).contains(&x) {
                    assert_eq!(texel(&out, x, y), texel(&input, x, y), "covered ({x},{y})");
                }
            }
        }
    }

    /// margin 1 reaches only d = 0.5 (columns 6 and 9); columns 7 and 8
    /// (d = 1.5 from their nearest seam) stay uncovered.
    #[test]
    fn margin_px_bound_is_respected() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        let out = adjacent_faces_margin(&island_map(), W, H, &mesh, &g, 1).expect("valid input");
        for y in 1..7 {
            assert_eq!(texel(&out, 6, y), color(10, y, 2));
            assert_eq!(texel(&out, 7, y), [0; 4]);
            assert_eq!(texel(&out, 8, y), [0; 4]);
            assert_eq!(texel(&out, 9, y), color(5, y, 1));
        }
    }

    /// Larger margin: closest seam wins. Column 7 is d = 1.5 from A's seam
    /// and d = 2.5 from B's: it takes B's texel (11). Column 8 mirrors.
    #[test]
    fn closest_seam_wins_when_strips_overlap() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        let out = adjacent_faces_margin(&island_map(), W, H, &mesh, &g, 3).expect("valid input");
        for y in 1..7 {
            assert_eq!(texel(&out, 7, y), color(11, y, 2));
            assert_eq!(texel(&out, 8, y), color(4, y, 1));
        }
    }

    /// Partner texel uncovered => the margin texel stays as-is. Clear
    /// island B's column 10; column 6 (samples col 10) stays empty while
    /// column 7 (samples col 11) still fills.
    #[test]
    fn uncovered_partner_leaves_texel_unchanged() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        let mut input = island_map();
        for y in 0..H {
            let i = ((y * W + 10) * 4) as usize;
            input[i..i + 4].copy_from_slice(&[0; 4]);
        }
        let out = adjacent_faces_margin(&input, W, H, &mesh, &g, 2).expect("valid input");
        for y in 1..7 {
            assert_eq!(texel(&out, 6, y), [0; 4]);
            assert_eq!(texel(&out, 7, y), color(11, y, 2));
        }
    }

    #[test]
    fn no_seam_mesh_and_zero_margin_return_input_unchanged() {
        let mut mesh = seamed_quad();
        // Collapse island B onto island A's seam UVs: continuous, no seam.
        mesh.uvs[3] = mesh.uvs[0];
        mesh.uvs[4] = mesh.uvs[2];
        let g = build_seam_graph(&mesh);
        assert!(g.edges.is_empty());
        let input = island_map();
        assert_eq!(
            adjacent_faces_margin(&input, W, H, &mesh, &g, 4),
            Ok(input.clone())
        );

        let seamed = seamed_quad();
        let g = build_seam_graph(&seamed);
        assert_eq!(
            adjacent_faces_margin(&input, W, H, &seamed, &g, 0),
            Ok(input)
        );
    }

    #[test]
    fn zero_dims_error() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        assert_eq!(
            adjacent_faces_margin(&[], 0, 4, &mesh, &g, 2),
            Err(MarginError::EmptyTarget {
                width: 0,
                height: 4
            })
        );
        assert_eq!(
            adjacent_faces_margin(&[], 4, 0, &mesh, &g, 2),
            Err(MarginError::EmptyTarget {
                width: 4,
                height: 0
            })
        );
    }

    #[test]
    fn size_mismatch_error() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        assert_eq!(
            adjacent_faces_margin(&[0u8; 63], 4, 4, &mesh, &g, 2),
            Err(MarginError::SizeMismatch {
                len: 63,
                width: 4,
                height: 4,
                expected: Some(64),
            })
        );
    }

    #[test]
    fn mesh_seam_mismatch_errors() {
        let mesh = seamed_quad();
        let g = build_seam_graph(&mesh);
        let input = island_map();
        // Triangle-count mismatch: graph from a 2-tri mesh, mesh with 1 tri.
        let mut short = mesh.clone();
        short.indices.truncate(3);
        assert_eq!(
            adjacent_faces_margin(&input, W, H, &short, &g, 2),
            Err(MarginError::MeshSeamMismatch {
                seam_tris: 2,
                mesh_tris: 1,
                tri: None,
            })
        );
        // Counts agree but tri 1 references a missing UV.
        let mut broken = mesh.clone();
        broken.indices[5] = 99;
        assert_eq!(
            adjacent_faces_margin(&input, W, H, &broken, &g, 2),
            Err(MarginError::MeshSeamMismatch {
                seam_tris: 2,
                mesh_tris: 2,
                tri: Some(1),
            })
        );
    }

    /// Degenerate destination triangle (zero UV area, centroid on the edge
    /// line): no inside/outside, so the edge is skipped and the map is
    /// returned unchanged rather than filled from an arbitrary side.
    #[test]
    fn zero_area_partner_triangle_is_skipped() {
        let mut mesh = seamed_quad();
        mesh.uvs[5] = uv(10.0, 4.0); // D on the seam line x = 10.
        let g = build_seam_graph(&mesh);
        assert_eq!(g.edges.len(), 1);
        let input = island_map();
        assert_eq!(adjacent_faces_margin(&input, W, H, &mesh, &g, 2), Ok(input));
    }
}
