# Kill report: bakes-panel-perset (per-set masked bakes, step 3 — the panel consumer)

- Authorship: dragon (the 11:48/12:31 loop session). Takes the chain
  the sibling's subset-mesh + occluder-scope kills and my
  `triangle_materials` (`a23bcac`) unblocked.
- Kill-lock `bakes-panel-perset`: `crates/umber-app/src/bakes_panel.rs`
  (+ the sibling's orphaned `umber-bake/src/ao.rs` tile-map subset,
  whose author session went idle mid-edit with a non-compiling
  `.filter_map` on a Vec — completed and fixed here, same shape as the
  landed `filter_mesh_for_triangles`).
- Implements `docs/specs/per-set-masked-bakes.md` §3: `bake_jobs` bakes
  each enabled set from its OWN submesh — the last whole-mesh artifact
  in a per-set pipeline is gone.

## Built

- `BakesPanel::bake_jobs`: resolves each enabled set's name to its
  `material_names` slot (the map's index space; the checklist's sort
  is display-only) and precomputes the set's RASTER submesh via
  `submesh_for_material`. `None` when the mesh carries no
  `triangle_materials` — today's byte-for-byte whole-mesh behavior
  (the v1 no-regression contract) or the set name matches no
  material.
- `BakeRequest.jobs` carries the per-set raster submesh (4-tuple).
- `run_bake`:
  - **Raster maps** (curvature, position, normal, world-normal, ID):
    bake from the set's submesh; the margin's seam graph follows the
    mesh the map was rasterized from (the existing margin contract),
    cached per submesh identity (`set_seams`, keyed by pointer —
    `MeshData` is not `Hash`), whole-mesh jobs keep the shared
    `whole_seams` cache.
  - **Ray maps** (AO, thickness, bent normal, transfer): whole mesh —
    occluders cross materials, the design doc §3's ruling. The baker
    `with_occluder(mesh)` signature split stays the named host-gated
    follow-up (with `occluding_triangles` already landed).
- `umber-bake/src/ao.rs` `filter_mesh_for_tile`: subsets the tile's
  triangles' material slots in lockstep with `indices` — the tile
  window's per-set AO stays honest (`triangles_for_tile(...).iter()`,
  the Vec-not-iterator fix).

## Tests

- Existing panel plumbing tests green (the fan-out, path resolution,
  and worker tests all ride the 4-tuple).
- Battery GREEN (8 crates, centralhost 13:1x): 778 app / 127 gui /
  112 core / 48 spp / 159 brush / 170 paint / 129 mesh / 92 bake,
  clippy -D warnings clean, layers gate green.
- No GPU-visible behavior change for single-material meshes: the
  raster submesh is `None`, every arm takes the whole-mesh path.

## Process notes (two real lessons this kill)

1. **Battery scope**: the `triangle_materials` commit's battery named
   only `umber-mesh`, so the downstream `MeshData` literals in
   umber-bake/umber-app/umber-cli never compiled before the push —
   CI caught umber-bake's `ao.rs` E0063. This kill's battery is the
   full 8-crate set, and a workspace-wide `cargo check --workspace`
   sweep precedes any future field-add across crate boundaries.
   (Downstream literals fixed: ao.rs's was the sibling's; bench.rs
   + 6 bakes_panel.rs test literals gained `Vec::new()`.)
2. **Concurrent half-kill adoption**: the sibling's ao.rs edit sat
   uncommitted with its author session idle; per the standing
   division of labor (the wave owner's session was NOT resumable at
   census time) the edit was completed under my lock rather than
   reverted.

## Deferred (named follow-ups)

- `with_occluder(mesh)` baker split (ray maps' per-set occluder scope;
  `occluding_triangles` is landed and waiting).
- A multi-material FBX/gltf REAL fixture for the end-to-end per-set
  bake — none exists in the corpus (Painter files are .spp).
- USD GeomSubset import.
