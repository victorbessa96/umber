//! Geometry masks (R-B2), the mesh side: per-part triangle lists and
//! the per-tile coverage plane a mesh exclusion composites through.
//!
//! Design brief: `docs/loop/briefs/2026-11-12-geomask-design.md`
//! ("Mesh names" and "Coverage function"). [`MeshData`] gains no field
//! (its 107 full struct literals would churn); [`MeshParts`] is the side
//! table every importer fills through [`crate::load_with_parts`].
//!
//! umber-mesh sits below umber-core, so this module speaks std types
//! (`BTreeSet<String>`), never umber-core's `GeometryMask`.

use std::collections::BTreeSet;

use crate::MeshData;

/// Longest part name kept, in bytes. Longer names are truncated at a
/// char boundary AT IMPORT, before any mask can store them, so a stored
/// exclusion always matches the imported name (brief, security model).
pub const MAX_PART_NAME_BYTES: usize = 1024;

/// The mesh's named parts (objects / sub-meshes), one name per part and
/// one part index per triangle.
///
/// All-or-nothing like [`MeshData::triangle_materials`]: `triangle_parts`
/// is either exactly `triangle_count()` long or empty. Empty means "no
/// part map": mesh exclusions are inert for that mesh (tile exclusions
/// still apply).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeshParts {
    /// Unique part names, in importer (first-seen) order.
    pub names: Vec<String>,
    /// One index into `names` per triangle, or empty.
    pub triangle_parts: Vec<u16>,
}

impl MeshParts {
    /// `true` when the map covers exactly `mesh`'s triangles.
    pub fn is_complete_for(&self, mesh: &MeshData) -> bool {
        !self.triangle_parts.is_empty() && self.triangle_parts.len() == mesh.triangle_count()
    }

    /// The triangle ids (0-based, into `indices`) of the part named
    /// `name`, ascending. `None` when the map is empty or no part has
    /// that name.
    pub fn triangles_of_part(&self, name: &str) -> Option<Vec<u32>> {
        if self.triangle_parts.is_empty() {
            return None;
        }
        let part = self.names.iter().position(|n| n == name)?;
        Some(
            self.triangle_parts
                .iter()
                .enumerate()
                .filter(|&(_, &p)| usize::from(p) == part)
                .map(|(t, _)| u32::try_from(t).unwrap_or(u32::MAX))
                .collect(),
        )
    }

    /// Triangle count per part, parallel to `names`.
    pub fn triangle_counts(&self) -> Vec<usize> {
        let mut counts = vec![0usize; self.names.len()];
        for &p in &self.triangle_parts {
            if let Some(c) = counts.get_mut(usize::from(p)) {
                *c += 1;
            }
        }
        counts
    }
}

/// Builds a [`MeshParts`] while an importer appends geometry: one
/// [`push`](Self::push) per source object, in emission order.
///
/// Naming rules (brief, "Mesh names"): an empty or missing name becomes
/// `mesh <n>`, `n` counting unnamed parts from 1 in first-seen order
/// (deterministic across reloads of the same file); names longer than
/// [`MAX_PART_NAME_BYTES`] are truncated at a char boundary; same-name
/// objects fold into one part. An object with zero triangles registers
/// no part. Past `u16::MAX + 1` distinct parts, or after
/// [`mark_incomplete`](Self::mark_incomplete), the finished map is empty.
#[derive(Debug, Default)]
pub struct MeshPartsBuilder {
    parts: MeshParts,
    unnamed: usize,
    complete: bool,
}

impl MeshPartsBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self {
            parts: MeshParts::default(),
            unnamed: 0,
            complete: true,
        }
    }

    /// Records `triangles` consecutive triangles belonging to the object
    /// named `name` (`None` or empty = unnamed).
    pub fn push(&mut self, name: Option<&str>, triangles: usize) {
        if triangles == 0 || !self.complete {
            return;
        }
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => truncate_name(n).to_string(),
            None => {
                self.unnamed += 1;
                format!("mesh {}", self.unnamed)
            }
        };
        let slot = match self.parts.names.iter().position(|n| *n == name) {
            Some(i) => i,
            None => {
                self.parts.names.push(name);
                self.parts.names.len() - 1
            }
        };
        let Ok(slot) = u16::try_from(slot) else {
            self.complete = false;
            return;
        };
        self.parts
            .triangle_parts
            .extend(std::iter::repeat_n(slot, triangles));
    }

    /// Some triangle has no part: the finished map is empty.
    pub fn mark_incomplete(&mut self) {
        self.complete = false;
    }

    /// The finished map for `mesh`: complete and exactly
    /// `triangle_count()` long, else empty (names and triangle map both
    /// cleared, the all-or-nothing contract).
    pub fn finish(self, mesh: &MeshData) -> MeshParts {
        if self.complete && self.parts.is_complete_for(mesh) {
            self.parts
        } else {
            MeshParts::default()
        }
    }
}

/// `name` cut to at most [`MAX_PART_NAME_BYTES`] bytes at a char boundary.
pub fn truncate_name(name: &str) -> &str {
    if name.len() <= MAX_PART_NAME_BYTES {
        return name;
    }
    let mut end = MAX_PART_NAME_BYTES;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// The mesh-exclusion coverage plane for one `(layer, tile)` at
/// `width`x`height`, row-major top-down like the paint planes: 255 where
/// the layer composites, 0 where it does not.
///
/// - **`None` = fully open**: no texel of `tile` is covered only by
///   excluded triangles (nothing excluded, no part map, a map that does
///   not fit `mesh`, a zero-sized target, or the excluded parts have no
///   footprint on this tile). The gate then adds no work.
/// - Covered texels: 255 when any INCLUDED triangle covers the texel
///   (overlapping UVs: included wins, hiding paint the user can see on an
///   included mesh is the worse error), 0 when only excluded ones do.
/// - Uncovered (gutter) texels take the value of the nearest covered
///   texel by breadth-first 8-neighbour steps, so island borders do not
///   seam. This is umber-core's `DilationMap` rule (unbounded distance, one
///   block); umber-mesh sits below umber-core, so the walk is local.
/// - The texel set is [`crate::for_each_uv_geometry`]'s, the walk behind
///   the G-buffer and `uv_coverage`: triangle-driven, O(triangles +
///   covered texels), plus O(width x height) for the plane and its
///   dilation. Never a per-texel scan of all triangles.
///
/// Cost (measured 2026-11-12, release build, this host, under sibling
/// cargo load): a 4096x4096 tile with a 131 072-triangle grid covering
/// the whole tile, every other row of cells excluded, builds its plane in
/// 476 ms. Pinned by `geometry_coverage_cost_4096` in
/// `tests/geometry_parts.rs` (`#[ignore]`; run it with
/// `cargo test --release -p umber-mesh --test geometry_parts -- --ignored --nocapture`).
pub fn geometry_coverage(
    mesh: &MeshData,
    parts: &MeshParts,
    excluded: &BTreeSet<String>,
    tile: u16,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    if excluded.is_empty() || !parts.is_complete_for(mesh) || width == 0 || height == 0 {
        return None;
    }
    let excluded_part: Vec<bool> = parts.names.iter().map(|n| excluded.contains(n)).collect();
    if !excluded_part.iter().any(|&e| e) {
        return None;
    }
    let is_excluded = |tri: usize| {
        parts
            .triangle_parts
            .get(tri)
            .and_then(|&p| excluded_part.get(usize::from(p)))
            .copied()
            .unwrap_or(false)
    };

    const UNCOVERED: u8 = 0;
    const EXCLUDED_ONLY: u8 = 1;
    const INCLUDED: u8 = 2;
    let (w, h) = (width as usize, height as usize);
    let mut state = vec![UNCOVERED; w * h];
    let current_excluded = std::cell::Cell::new(false);
    let mut any_excluded = false;
    crate::uv_gbuffer::for_each_uv_geometry(
        mesh,
        tile,
        width,
        height,
        |tri| {
            current_excluded.set(is_excluded(tri));
            true
        },
        |x, y, _, _| {
            if let Some(s) = state.get_mut(y as usize * w + x as usize) {
                if current_excluded.get() {
                    if *s == UNCOVERED {
                        *s = EXCLUDED_ONLY;
                        any_excluded = true;
                    }
                } else {
                    *s = INCLUDED;
                }
            }
        },
    );
    // An excluded-only texel may have been claimed later by an included
    // triangle: re-check on the final state.
    if !any_excluded || !state.contains(&EXCLUDED_ONLY) {
        return None;
    }

    let mut plane: Vec<u8> = state
        .iter()
        .map(|&s| if s == INCLUDED { 255 } else { 0 })
        .collect();
    let mut filled: Vec<bool> = state.iter().map(|&s| s != UNCOVERED).collect();
    // Seed with covered texels that border an uncovered one.
    let mut frontier: Vec<usize> = (0..w * h)
        .filter(|&i| filled[i] && neighbours(i, w, h).any(|j| !filled[j]))
        .collect();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &i in &frontier {
            let value = plane[i];
            for j in neighbours(i, w, h) {
                if !filled[j] {
                    filled[j] = true;
                    plane[j] = value;
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    Some(plane)
}

/// The in-bounds 8-neighbours of texel `i`, in umber-core
/// `uv_dilation`'s order (left, right, up, down, then diagonals) so a
/// tie resolves the same way.
fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % w, i / w);
    let left = x > 0;
    let right = x + 1 < w;
    let up = y > 0;
    let down = y + 1 < h;
    [
        (left, i.wrapping_sub(1)),
        (right, i + 1),
        (up, i.wrapping_sub(w)),
        (down, i + w),
        (left && up, i.wrapping_sub(w + 1)),
        (right && up, i.wrapping_sub(w).wrapping_add(1)),
        (left && down, (i + w).wrapping_sub(1)),
        (right && down, i + w + 1),
    ]
    .into_iter()
    .filter_map(|(ok, j)| ok.then_some(j))
}
