# Kill report: C2-29 the Bakes panel bakes all eight mesh-map kinds

Date: 2026-10-15. Author: dragon. Lock: `c2-29-all-bake-maps` over
`crates/umber-app/src/{bakes_panel,bake_sources}.rs` and this report.
Found while writing the user guide: the panel offered AO, curvature,
thickness and position (4 of `MeshMapKind`'s 9), so a painter in the app
could not bake the normal map their export needs.

## Built

- `BakeSelection` gains **Normal** (tangent space, OpenGL Y), **WorldNormal**,
  **Id** (the FNV-hashed triangle flavor) and **BentNormal** (AO's bent
  output at the panel's ray count). `ALL` has 8 entries.
  - `DEFAULT_ON` = the original four + Normal. World normal, ID and bent
    normal are opt-in: niche, and bent normal costs a second AO bake.
  - Height is the ninth `MeshMapKind`. It is a high→low transfer output
    and needs a high-poly source the panel has no picker for yet.
- `bake_extra_map` uses the same `umber_bake` entry points and encodings
  as `umber-cli bake-all`. `encode_unit_vectors_rgba8` is the CLI's
  world-normal encoding: [−1, 1] → [0, 255], uncovered texels clear.
- **Vector maps are written LINEAR.** The panel wrote every PNG through
  the sRGB curve; for a normal map that bends the directions. The CLI
  already wrote them linear, and `BakeSelection::is_vector` makes the
  panel match.
- The four `bake_*: bool` fields became one `BTreeSet<BakeSelection>`,
  and the checkboxes iterate `ALL`. `set_map_enabled`, `is_map_enabled`,
  `any_selected`, `bake_all_sets` and auto-rebake behave as before over
  the larger set.
- The log's "jump to setting" sends bent normal to Rays, like AO.
- Margin and dilation apply to every new map. Multi-tile meshes bake the
  new maps whole-mesh for 1001 and name them as skipped for other tiles
  (the existing v1 rule for non-AO maps).

## Tests

- New: the CLI-matching unit-vector encoding; the linear-vs-sRGB split
  over all 8; and **a GPU bake of the four new maps on a flat quad**
  (ran): each covers the quad, the tangent normal at the centre points
  up, the world normal is +Z (B = 255), and an AO request to the extra
  baker is refused.
- Four existing tests pinned the old 4-map default and were updated to
  `DEFAULT_ON` (the default now also bakes the normal), with the tile
  plans and the skipped lists gaining Normal. No test was weakened.
- Gate 1: umber-app 488. fmt and clippy clean.

## Strings

"Normal" reuses `viewmode.normal`. "World-space normal", "ID" and "Bent
normal" are untranslated until the catalogs (under the sibling's lock)
take `bakes.map-*` keys.

## Not done

- High→low transfer (height, cage) in the panel: it needs a high-poly
  mesh picker. `umber-cli`'s batch `bake_transfer` step does it today.
- Per-tile baking of the new maps on multi-tile meshes (v1 has a
  per-tile entry point for AO only).
- The export still uses its own flat normal unless a baked normal is
  imported. Routing the panel's baked normal into the export's normal
  slot is the follow-up.
