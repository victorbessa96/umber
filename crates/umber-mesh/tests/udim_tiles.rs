//! UDIM tile addressing: property + golden tests (queue row: UDIM
//! multi-tile paint correctness).
//!
//! Pins the EXISTING policy of `umber_mesh::udim` / `uv_gbuffer` — these
//! tests match the code, they do not change it:
//!
//! - [`tile_of_uv`] = `1001 + clamp(floor(u), 0..=9) + 10 * clamp(floor(v),
//!   0..=9)`: always in `1001..=1100`. UV exactly `1.0` floors to 1, so it
//!   lands in the NEXT tile (1002), never back in 1001. NaN in either
//!   component maps to [`FIRST_TILE`]; ±infinity and huge magnitudes clamp
//!   to the grid edges (no tile 9999 is ever produced from a UV).
//! - [`uv_gbuffer::tile_offset`] is the raw inverse `(tile-1001) % 10,
//!   (tile-1001) / 10` — it saturates below 1001 but EXTRAPOLATES above
//!   1100 (tile 9999 → `(8, 899)`), so a gbuffer built for an out-of-grid
//!   tile id still addresses the matching global-UV cell.
//! - Per-tile rasterization (`UvGBuffer`, `uv_coverage`, `texel_window`)
//!   clips at each tile's own UV cell: a triangle straddling a seam
//!   contributes to both tiles but only inside each tile's texels — no
//!   cross-tile bleed.

use umber_mesh::uv_gbuffer::{
    texel_center_uv, texel_window, tile_offset, uv_to_barycentric, UvGBuffer, UvTri,
};
use umber_mesh::{
    filter_mesh_for_triangles, present_tiles, tile_of_triangle, tile_of_uv, triangles_for_tile,
    uv_coverage, MeshData, FIRST_TILE,
};

/// 2x1 UV strip spanning `[0,2]x[0,1]` (position == UV): the left quad's
/// triangles start at u=0 (tile 1001), the right quad's at u=1 (tile
/// 1002). Shared vertices at u=1, so no seam edges — tile routing is the
/// only thing under test. Same shape as umber-paint's `two_tile_strip`.
fn two_tile_strip() -> MeshData {
    let uvs = vec![
        [0.0, 0.0], // 0
        [1.0, 0.0], // 1
        [2.0, 0.0], // 2
        [0.0, 1.0], // 3
        [1.0, 1.0], // 4
        [2.0, 1.0], // 5
    ];
    MeshData {
        positions: uvs.iter().map(|uv| [uv[0], uv[1], 0.0]).collect(),
        normals: vec![[0.0, 0.0, 1.0]; 6],
        uvs,
        indices: vec![0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4],
        material_names: vec![],
        triangle_materials: Vec::new(),
    }
}

/// The strip plus one triangle straddling the u=1 seam
/// (`(0.9,0.5), (1.1,0.5), (1.0,0.9)` — first vertex in tile 1001).
fn strip_plus_seam_straddler() -> MeshData {
    let mut mesh = two_tile_strip();
    mesh.uvs
        .extend_from_slice(&[[0.9, 0.5], [1.1, 0.5], [1.0, 0.9]]);
    mesh.positions
        .extend_from_slice(&[[0.9, 0.5, 0.0], [1.1, 0.5, 0.0], [1.0, 0.9, 0.0]]);
    mesh.normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 3]);
    mesh.indices.extend_from_slice(&[6, 7, 8]);
    mesh
}

/// The expected tile under the documented clamp policy, computed
/// INDEPENDENTLY of `tile_of_uv`'s `as i64` saturation path: floor in
/// f64 (which holds 1e30 and ±inf exactly enough to clamp) then clamp
/// each axis to `0..=9`. NaN in either component is the documented
/// default-tile case.
fn expected_tile(u: f32, v: f32) -> u16 {
    if u.is_nan() || v.is_nan() {
        return FIRST_TILE;
    }
    let cu = (u.floor() as f64).clamp(0.0, 9.0) as i64;
    let cv = (v.floor() as f64).clamp(0.0, 9.0) as i64;
    (FIRST_TILE as i64 + cu + 10 * cv) as u16
}

/// Hostile UV components: NaN, ±inf, ±1e30, negatives, the exact
/// boundaries 0.0 / 1.0 / 10.0, and just-below-1.0.
const HOSTILE: [f32; 16] = [
    f32::NAN,
    f32::NEG_INFINITY,
    -1e30,
    -100.5,
    -1.0,
    -0.0,
    0.0,
    0.5,
    0.999_999_9,
    1.0,
    1.5,
    9.999_999,
    10.0,
    50.5,
    1e30,
    f32::INFINITY,
];

#[test]
fn tile_index_matches_the_udim_formula_over_a_hostile_uv_grid() {
    for &u in &HOSTILE {
        for &v in &HOSTILE {
            let got = tile_of_uv([u, v]);
            assert_eq!(
                got,
                expected_tile(u, v),
                "tile_of_uv([{u}, {v}]) disagrees with the clamped formula"
            );
            assert!(
                (FIRST_TILE..=1100).contains(&got),
                "tile {got} for uv [{u}, {v}] leaves the 10x10 grid"
            );
        }
    }
}

#[test]
fn uv_exactly_one_goes_to_the_next_tile_not_the_current() {
    // floor(1.0) = 1: the boundary lands in the NEXT tile.
    assert_eq!(tile_of_uv([1.0, 0.5]), 1002, "u == 1.0 -> 1001 + 1");
    assert_eq!(tile_of_uv([0.5, 1.0]), 1011, "v == 1.0 -> 1001 + 10");
    assert_eq!(
        tile_of_uv([1.0, 1.0]),
        1012,
        "the (1,1) corner -> 1001+1+10"
    );
    // Just below 1.0 stays in 1001 / 1010.
    let just_below_one = f32::from_bits(1.0_f32.to_bits() - 1);
    assert!(just_below_one < 1.0);
    assert_eq!(tile_of_uv([just_below_one, 0.5]), 1001);
    assert_eq!(tile_of_uv([0.5, just_below_one]), 1001);
    // The exact grid edges and just-past-them clamp onto the far edge.
    assert_eq!(
        tile_of_uv([10.0, 0.0]),
        1010,
        "floor(10) clamps to column 9"
    );
    assert_eq!(tile_of_uv([9.999, 9.999]), 1100);
    assert_eq!(
        tile_of_uv([-1.0, 0.0]),
        1001,
        "floor(-1) clamps up to column 0"
    );
    assert_eq!(tile_of_uv([-0.0, -0.0]), 1001);
}

#[test]
fn nan_defaults_to_first_tile_and_infinities_clamp_to_grid_corners() {
    // The documented policy: NaN carries no tile information -> 1001;
    // ±inf (and any |uv| past the grid) clamps per axis, never wraps.
    assert_eq!(tile_of_uv([f32::NAN, f32::NAN]), FIRST_TILE);
    assert_eq!(tile_of_uv([f32::NAN, 5.0]), FIRST_TILE);
    assert_eq!(tile_of_uv([5.0, f32::NAN]), FIRST_TILE);
    assert_eq!(tile_of_uv([f32::INFINITY, 0.5]), 1010);
    assert_eq!(tile_of_uv([0.5, f32::INFINITY]), 1091);
    assert_eq!(tile_of_uv([f32::INFINITY, f32::INFINITY]), 1100);
    assert_eq!(tile_of_uv([f32::NEG_INFINITY, f32::NEG_INFINITY]), 1001);
    assert_eq!(
        tile_of_uv([1e30, 1e30]),
        1100,
        "huge finite clamps, no tile 9999"
    );
    assert_eq!(tile_of_uv([-1e30, 0.0]), 1001);
    // ...and the point lookup rejects non-finite UVs outright (miss, not a
    // default hit): the gbuffer contract's "NaN / infinite inputs miss".
    let mesh = two_tile_strip();
    assert_eq!(uv_to_barycentric(&mesh, [f32::NAN, 0.5]), None);
    assert_eq!(uv_to_barycentric(&mesh, [0.5, f32::INFINITY]), None);
    assert_eq!(uv_to_barycentric(&mesh, [1e30, 0.5]), None);
}

#[test]
fn tile_offset_round_trips_every_grid_tile_and_its_texel_centers() {
    for tile in FIRST_TILE..=1100 {
        let index = tile - FIRST_TILE;
        let (tu, tv) = tile_offset(tile);
        assert_eq!((tu, tv), (f32::from(index % 10), f32::from(index / 10)));
        // Inverse: recomposing the tile from its offset is the identity.
        assert_eq!(FIRST_TILE + (tu as u16) + 10 * (tv as u16), tile);
        // Every texel center of the tile maps back to the tile itself via
        // tile_of_uv (centers are at (x+0.5)/w, never on a cell edge).
        for &(x, y) in &[(0, 0), (7, 9), (15, 15)] {
            let uv = texel_center_uv(tu, tv, x, y, 16, 16);
            assert_eq!(tile_of_uv(uv), tile, "texel ({x},{y}) of tile {tile}");
        }
    }
}

#[test]
fn tile_offset_extrapolates_beyond_the_grid_without_panicking() {
    // Below the grid saturates to (0, 0); above 1100 the raw formula
    // extrapolates (tile 9999 -> index 8998 -> (8, 899)) — pinned as-is.
    assert_eq!(tile_offset(0), (0.0, 0.0));
    assert_eq!(tile_offset(1000), (0.0, 0.0));
    assert_eq!(tile_offset(9999), (8.0, 899.0));
    assert_eq!(tile_offset(65535), (4.0, 6453.0));
    // UVs inside tile 9999's global cell (u in [8,9), v in [899,900)) do
    // NOT produce tile 9999 from tile_of_uv — the clamp policy pins them
    // to the grid's far row: 1001 + 8 + 10*9 = 1099.
    assert_eq!(tile_of_uv([8.5, 899.5]), 1099);
    assert_eq!(tile_of_uv([8999.5, 0.5]), 1010, "huge u clamps to column 9");
}

#[test]
fn per_tile_gbuffer_clips_at_each_tiles_own_seam() {
    let mesh = two_tile_strip();
    assert_eq!(tile_of_triangle(&mesh), vec![1001, 1001, 1002, 1002]);
    assert_eq!(present_tiles(&mesh), vec![1001, 1002]);

    let (w, h) = (16, 16);
    // Coverage goldens: each tile's own quad fills it exactly; the
    // neighbor's quad touches only the clamped edge window and its texel
    // centers fall outside the footprint, so it covers nothing.
    let cov_1001 = uv_coverage(&mesh, 1001, w, h);
    let cov_1002 = uv_coverage(&mesh, 1002, w, h);
    assert!(cov_1001.iter().all(|&c| c), "left quad fills tile 1001");
    assert!(cov_1002.iter().all(|&c| c), "right quad fills tile 1002");
    assert!(uv_coverage(&mesh, 1003, w, h).iter().all(|&c| !c));
    assert!(uv_coverage(&mesh, 1011, w, h).iter().all(|&c| !c));

    // The per-tile gbuffer agrees with the global point lookup at every
    // texel of every tile — and every hit's texel center lies inside the
    // gbuffer's own tile (no cross-tile bleed).
    for tile in [1001, 1002, 1003] {
        let g = UvGBuffer::build(&mesh, tile, w, h);
        let (tu, tv) = tile_offset(tile);
        for y in 0..h {
            for x in 0..w {
                let uv = texel_center_uv(tu, tv, x, y, w, h);
                assert_eq!(
                    g.at(x, y),
                    uv_to_barycentric(&mesh, uv),
                    "tile {tile} texel ({x},{y})"
                );
                if let Some(hit) = g.at(x, y) {
                    assert_eq!(tile_of_uv(uv), tile, "hit at ({x},{y}) bleeds across tiles");
                    let _ = hit;
                }
            }
        }
    }
}

#[test]
fn seam_straddling_triangle_splits_per_tile_with_no_bleed() {
    let mesh = strip_plus_seam_straddler();
    // First-vertex assignment: the straddler (first UV (0.9, 0.5)) lands
    // in tile 1001 as a whole triangle.
    assert_eq!(tile_of_triangle(&mesh), vec![1001, 1001, 1002, 1002, 1001]);

    let straddler = UvTri::of(&mesh, 4).expect("straddler is well-formed");
    let (w, h) = (16, 16);
    // The shared windowing clamps at the tile edge: in 1001 the box stops
    // at the last column (x1 = 15, the clamp of ceil(1.1*16+1) = 19); in
    // 1002 it starts at column 0 (the clamp of floor((0.9-1)*16-1) = -3).
    let win_1001 = texel_window(&straddler, 0.0, 0.0, w, h).expect("overlaps 1001");
    let win_1002 = texel_window(&straddler, 1.0, 0.0, w, h).expect("overlaps 1002");
    assert_eq!(win_1001, (13, 15, 0, 9), "clipped at 1001's right edge");
    assert_eq!(win_1002, (0, 3, 0, 9), "clipped at 1002's left edge");

    // Per-tile gbuffer: the straddler covers texels on its own side of the
    // seam only, and the union across tiles matches the global lookup.
    for (tile, tu) in [(1001u16, 0.0f32), (1002, 1.0)] {
        let g = UvGBuffer::build(&mesh, tile, w, h);
        for y in 0..h {
            for x in 0..w {
                let uv = texel_center_uv(tu, 0.0, x, y, w, h);
                assert_eq!(
                    g.at(x, y),
                    uv_to_barycentric(&mesh, uv),
                    "tile {tile} ({x},{y})"
                );
                if let Some(hit) = g.at(x, y) {
                    if hit.triangle == 4 {
                        if tile == 1001 {
                            assert!(uv[0] < 1.0, "straddler hit past the seam in 1001 at {uv:?}");
                        } else {
                            assert!(
                                uv[0] >= 1.0,
                                "straddler hit before the seam in 1002 at {uv:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    // The bake-side per-tile filter agrees: tile 1002's submesh covers
    // nothing in tile 1001 and vice versa.
    let tris_1002 = triangles_for_tile(&mesh, 1002);
    assert_eq!(tris_1002, vec![2, 3]);
    let sub_1002 = filter_mesh_for_triangles(&mesh, &tris_1002);
    assert!(uv_coverage(&sub_1002, 1001, w, h).iter().all(|&c| !c));
    assert!(uv_coverage(&sub_1002, 1002, w, h).iter().all(|&c| c));
    let sub_1001 = filter_mesh_for_triangles(&mesh, &triangles_for_tile(&mesh, 1001));
    assert!(uv_coverage(&sub_1001, 1003, w, h).iter().all(|&c| !c));
    // Tile-1001-assigned triangles reach into tile 1002's UV cell ONLY
    // through the straddler's actual footprint (clipped to 1002's texels)
    // — first-vertex assignment never moves the quads across the seam.
    let sub_straddler = filter_mesh_for_triangles(&mesh, &[4]);
    assert_eq!(
        uv_coverage(&sub_1001, 1002, w, h),
        uv_coverage(&sub_straddler, 1002, w, h),
        "cross-seam coverage of the 1001 submesh is exactly the straddler"
    );
}

/// Tile-local texel of `uv` in `tile` at `texels_per_uv` — mirrors
/// `umber_paint::paint_state::local_texel` (same `tile_offset`
/// subtraction + V flip) so the stroke golden exercises the real routing
/// (`tile_of_uv`) against the same local-texel math the paint path uses.
fn local_texel(uv: [f32; 2], tile: u16, texels_per_uv: f32) -> [f32; 2] {
    let (tu, tv) = tile_offset(tile);
    [
        (uv[0] - tu) * texels_per_uv,
        (1.0 - (uv[1] - tv)) * texels_per_uv,
    ]
}

/// The dab footprint box `[floor(pos-r), ceil(pos+r)]` clamped to the
/// tile image (half-open), `None` when it misses — mirrors
/// `umber_brush::retouch::footprint`, the box the paint shader clamps to
/// the target, i.e. the per-tile clip a seam dab goes through.
fn dab_box(pos: [f32; 2], radius: f32, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let min_x = ((pos[0] - radius).floor() as i64).max(0) as u32;
    let min_y = ((pos[1] - radius).floor() as i64).max(0) as u32;
    let max_x = (((pos[0] + radius).ceil() as i64).min(i64::from(width) - 1)).max(0) as u32;
    let max_y = (((pos[1] + radius).ceil() as i64).min(i64::from(height) - 1)).max(0) as u32;
    ((max_x >= min_x) && (max_y >= min_y)).then_some((min_x, min_y, max_x + 1, max_y + 1))
}

#[test]
fn stroke_across_the_seam_stamps_only_the_routed_tile() {
    // One stroke crossing u=1 at v=0.5 on a 16x16-per-tile target: five
    // dab events, radius 0.05 UV (= 0.8 texels). Routing is the real
    // `tile_of_uv`; the local-texel/footprint math mirrors the paint path
    // (see the helpers' docs).
    let mesh = two_tile_strip();
    let (w, h) = (16u32, 16u32);
    let tpv = 16.0_f32;
    let radius_uv = 0.05;
    let events = [0.875, 0.9375, 1.0, 1.0625, 1.125];

    // The boundary event u == 1.0 routes to tile 1002 — pinned at the
    // routing level (this is exactly what PaintState::route_tile calls).
    let routes: Vec<u16> = events.iter().map(|&u| tile_of_uv([u, 0.5])).collect();
    assert_eq!(routes, vec![1001, 1001, 1002, 1002, 1002]);

    // Stamp each event into its routed tile's own pixel buffer only.
    let mut tiles: std::collections::BTreeMap<u16, Vec<bool>> = std::collections::BTreeMap::new();
    for &u in &events {
        let tile = tile_of_uv([u, 0.5]);
        let buf = tiles.entry(tile).or_insert(vec![false; (w * h) as usize]);
        let pos = local_texel([u, 0.5], tile, tpv);
        let Some((x0, y0, x1, y1)) = dab_box(pos, radius_uv * tpv, w, h) else {
            panic!("dab at u={u} on tile {tile} must land inside the tile");
        };
        for y in y0..y1 {
            for x in x0..x1 {
                buf[(y * w + x) as usize] = true;
            }
        }
    }

    let delta = |tile: u16| -> Vec<(u32, u32)> {
        tiles
            .get(&tile)
            .map(|b| {
                b.iter()
                    .enumerate()
                    .filter(|(_, &c)| c)
                    .map(|(i, _)| (i as u32 % w, i as u32 / w))
                    .collect()
            })
            .unwrap_or_default()
    };

    // Golden per-tile pixel deltas (rows 7..=9 around v=0.5 -> y=8):
    // tile 1001 gets columns 13..=15 (events u=0.875 -> x=14 box 13..=15,
    // u=0.9375 -> x=15 box 14..=15); tile 1002 gets columns 0..=3 (u=1.0
    // -> x=0 box 0..=1, u=1.0625 -> x=1 box 0..=2, u=1.125 -> x=2 box
    // 1..=3). The u == 1.0 seam event writes NOTHING into tile 1001.
    let want_1001: Vec<(u32, u32)> = (7..=9)
        .flat_map(|y| (13..=15).map(move |x| (x, y)))
        .collect();
    let want_1002: Vec<(u32, u32)> = (7..=9).flat_map(|y| (0..=3).map(move |x| (x, y))).collect();
    assert_eq!(delta(1001), want_1001, "tile 1001 delta");
    assert_eq!(delta(1002), want_1002, "tile 1002 delta");
    assert!(delta(1003).is_empty(), "no third tile may be touched");
    assert_eq!(tiles.len(), 2, "exactly the two routed tiles exist");

    // And the strip's geometry agrees: the stroke's texels all fall under
    // the mesh's own UV footprint per tile (the paint mask gates on this).
    let cov_1001 = uv_coverage(&mesh, 1001, w, h);
    let cov_1002 = uv_coverage(&mesh, 1002, w, h);
    for (x, y) in delta(1001) {
        assert!(
            cov_1001[(y * w + x) as usize],
            "1001 texel ({x},{y}) uncovered by mesh"
        );
    }
    for (x, y) in delta(1002) {
        assert!(
            cov_1002[(y * w + x) as usize],
            "1002 texel ({x},{y}) uncovered by mesh"
        );
    }
}

#[test]
fn hostile_tiles_and_uvs_never_panic_the_gbuffer() {
    // A mesh with a triangle inside tile 9999's global cell
    // (u in [8,9), v in [899,900)) plus NaN and 1e30 UVs: every rasterizer
    // must survive and agree with the point lookup.
    let mesh = MeshData {
        positions: vec![[0.0; 3]; 6],
        normals: vec![[0.0; 3]; 6],
        uvs: vec![
            [8.5, 899.2],
            [8.9, 899.2],
            [8.7, 899.8],
            [f32::NAN, 0.5],
            [1e30, 0.5],
            [0.5, -1e30],
        ],
        indices: vec![0, 1, 2, 3, 4, 5],
        material_names: vec![],
        triangle_materials: Vec::new(),
    };
    // tile_of_triangle never panics and never leaves the grid; the
    // malformed triangles default to 1001.
    let tiles = tile_of_triangle(&mesh);
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[0], 1099, "v ~ 899 clamps to grid row 9: 1001+8+90");
    assert_eq!(tiles[1], FIRST_TILE, "NaN/1e30 UVs default to 1001");
    assert!(tiles.iter().all(|t| (FIRST_TILE..=1100).contains(t)));

    // Tile 9999's gbuffer covers exactly its own cell and matches the
    // global point lookup at every texel; the clamped-grid tile 1099 and
    // the saturating tile 0 do not see the v~899 triangle.
    let (w, h) = (8, 8);
    let g_9999 = UvGBuffer::build(&mesh, 9999, w, h);
    let (tu, tv) = tile_offset(9999);
    assert_eq!((tu, tv), (8.0, 899.0));
    for y in 0..h {
        for x in 0..w {
            let uv = texel_center_uv(tu, tv, x, y, w, h);
            assert_eq!(
                g_9999.at(x, y),
                uv_to_barycentric(&mesh, uv),
                "tile 9999 texel ({x},{y})"
            );
        }
    }
    assert!(
        g_9999.covered().iter().any(|&c| c),
        "the v~899 triangle covers tile 9999's gbuffer"
    );
    assert!(UvGBuffer::build(&mesh, 1099, w, h)
        .covered()
        .iter()
        .all(|&c| !c));
    assert!(UvGBuffer::build(&mesh, 0, w, h)
        .covered()
        .iter()
        .all(|&c| !c));
    assert!(UvGBuffer::build(&mesh, 65535, w, h)
        .covered()
        .iter()
        .all(|&c| !c));
    // Zero-sized targets stay empty, never panic.
    assert!(UvGBuffer::build(&mesh, 9999, 0, h).hits.is_empty());
    assert!(uv_coverage(&mesh, 9999, 0, h).is_empty());
}
