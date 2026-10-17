# Kill report: triangle-materials (per-set masked bakes, step 1)

- Authorship: dragon (the 04:41/11:48 coordination session, taking the
  W-GUI-4 queue's item 3 prerequisite while the wave-owner session
  holds the graph-curves-editor lock).
- Kill-lock `triangle-materials`:
  `crates/umber-mesh/src/lib.rs`,
  `crates/umber-mesh/src/fbx.rs`,
  `crates/umber-mesh/src/gltf.rs`.
- Implements `docs/specs/per-set-masked-bakes.md` §1 + §2: the
  triangle→material mapping `MeshData.triangle_materials` and the
  `submesh_for_material` consumer, so the landed
  `filter_mesh_for_triangles` (subset-mesh) and `occluding_triangles`
  (occluder-scope) primitives finally have their input.

## Built

- `MeshData.triangle_materials: Vec<u16>` — one index into
  `material_names` per triangle. **Empty means "one material"**:
  every pre-field mesh and test literal stays valid via
  `..Default::default()` / `Vec::new()`, and consumers (the bakes
  panel) treat empty as the whole-mesh fallback — the v1
  no-regression contract from the design doc.
- `MeshData::triangles_of_material(material) -> Option<Vec<u32>>` —
  `None` on an unmapped mesh (distinct from an empty selection on a
  mapped one).
- `MeshData::submesh_for_material(material) -> MeshData` — the
  material's triangles through the PROVEN
  `filter_mesh_for_triangles` primitive (full vertex arrays ride
  along, `indices` holds original vertex ids). Out-of-range material
  or unmapped mesh → empty submesh, never a silent whole-mesh bake.
- `filter_mesh_for_triangles` itself now carries the SUBSET's
  material map (nested submeshing stays honest).
- Importers:
  - **FBX** (`fbx.rs`): a face→slot map built from ufbx's
    `material_parts` (`List<MeshPart>` face_indices — the API the
    design doc's `face_material` claim resolves to in 0.11.5), one
    slot per emitted triangle (`corners.len() / 3` — a quad yields
    two triangles under one material).
  - **glTF** (`gltf.rs`): the primitive's material slot repeated per
    its triangles (`(indices.len() - before) / 3` — the count is
    corners/3, not corners; the first draft over-counted 3× and the
    fixture caught it).
  - **OBJ** (`lib.rs` `load_obj`): tobj's `mesh.material_id` per
    model (the `Model` struct has no such field — the design doc's
    "mesh.material_id" was right), slot repeated per model triangle
    count.
  - **USD** (`usd.rs`, `usdc.rs`): `Vec::new()` with the doc comment
    naming GeomSubset as the follow-up — the design doc's own
    "USD: later, empty is honest".
- **All-or-nothing at file scope (the judgment call)**: the map is
  either empty or EXACTLY `triangle_count()` long. A file mixing
  named-material geometry with unnamed geometry (multi-mesh FBX,
  multi-primitive glTF, OBJ without an .mtl on one model) keeps the
  whole-mesh fallback for the entire file rather than a ragged
  half-map that would mis-attribute the unmapped half to material 0.
  FBX: all faces must carry a slot; glTF: every triangle-emitting
  primitive must name a material; OBJ: every model must map.

## Tests (97 lib, +2 net-new; 129 with integration, host battery GREEN)

- `triangle_materials_empty_means_whole_mesh` — the §1 contract:
  unmapped mesh → `triangles_of_material` is `None`, submesh is
  empty, vertex arrays ride along.
- `submesh_for_material_keeps_the_sets_triangles` — two materials /
  two triangles: each set's submesh keeps exactly its triangle with
  original ids; out-of-range material → empty; the map is exactly
  `triangle_count()` long.
- Both glTF fixtures (`loads_embedded_gltf_triangle`,
  `loads_glb_triangle`) now assert `triangle_materials == [0]` and
  `len == triangle_count()`.
- The FBX cube fixture (Blender 2.7 export, no materials) exercises
  the all-or-nothing fallback path: map stays empty.
- 22 pre-field MeshData literals across bvh/fill/overlay/
  polygon_fill/raycast/seam/udim/usd/usdc gained
  `triangle_materials: Vec::new()` — mechanical, verified by compile
  sweep to zero E0063.

## Verification

- `cargo fmt` + `cargo clippy --all-targets` clean.
- Local: 97 lib + 32 integration pass.
- Host battery (`scripts/wave-battery.sh umber-mesh`, centralhost,
  2026-10-17 ~12:2x): **GREEN — 129 tests, BATTERY_EXIT=0**, layers
  gate GREEN, fmt applied, clippy -D warnings clean.

## Unverified / deferred

- The bakes-panel consumer (`bake_jobs` per-set split) is the NEXT
  kill — it needs this field live in the app layer and is
  bakes_panel.rs work, outside this lock.
- No multi-material REAL fixture exists yet (the corpus's Painter
  files are .spp, not importable meshes) — the FBX map path is
  compile+API verified against ufbx 0.11.5's MeshPart face_indices
  and the all-or-nothing path is fixture-verified (the cube); a
  multi-material FBX fixture is the named follow-up when one is
  found or built.
- USD GeomSubset: named follow-up per the design doc.

## Process note

- The glTF triangles-count bug (corners vs triangles, 3×) was caught
  by the existing fixture asserting `len == triangle_count()` before
  any host run — the assert-first shape held. Probes were added and
  removed within the same kill; the final tree carries zero probe
  residue (`grep -c eprintln == 0`).
