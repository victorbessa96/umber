# Kill report: subset-mesh (per-set masked bakes, step-1.5)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `subset-mesh`: `crates/umber-mesh/src/udim.rs`,
  `crates/umber-mesh/src/lib.rs` (re-export).
- Implements the missing PRIMITIVE of
  `docs/specs/per-set-masked-bakes.md` step 2:
  `filter_mesh_for_triangles(mesh, tris) -> MeshData` — the general
  triangle-subset builder that `submesh_for_material` (once
  `triangle_materials` lands, host-gated per the doc's own ruling)
  calls directly.

## Built

- `umber_mesh::filter_mesh_for_triangles`: index-subset, not
  re-indexed — the FULL vertex arrays ride along, `indices` holds the
  selected triangles' original vertex ids. This is the exact contract
  the AO bake's per-tile filter (ao.rs `filter_mesh_for_tile`)
  already proven in production: the bake cores resolve every
  attribute through `indices`, so compacting vertices adds remap risk
  for zero benefit.
  - Out-of-range triangle ids are skipped, not panicked (the tile
    filter's own `get()` guard shape).
  - An empty list yields a mesh with no indices — callers treat as
    empty, never a silent blank map.
- Re-exported from `umber_mesh` beside `triangles_for_tile`.
- Tests (3): the two-triangle fixture's subset keeps all 4 vertices
  and only triangle 1's original ids `[3, 1, 2]`; empty list = empty
  indices with vertex arrays intact; a hostile triangle list (id 99)
  skips the bad entry and keeps the valid one.

## Judgment calls

- **Additive, not the struct change**: the design doc's own "Why not
  now" ruling holds — `triangle_materials` (148 break-sites, host
  compiler-guided) stays host-gated. This kill lands the PRIMITIVE so
  the host-up hour starts on step 2's importer work with the mesh
  half already tested.
- **The AO per-tile filter stays duplicated for now**: ao.rs's
  `filter_mesh_for_tile` carries the UV-rebase logic this primitive
  deliberately does NOT (a material submesh keeps UVs absolute; a
  tile submesh rebases by the tile offset). Refactoring the AO filter
  to call this primitive is step 3's cleanup when the host can
  re-verify the byte-identical regression pin.

## Unverified

- Battery (host down since 18:25). Static: rustfmt PASS; the fixture's
  real index shape read before pinning expectations (the first draft
  asserted `[3, 4, 5]`; the fixture's shared vertices make it
  `[3, 1, 2]` — caught by reading, not by a red battery).

## Deferred

- `submesh_for_material` + the 148-site struct change (host-gated,
  the doc's step 1).
- The AO filter refactor onto this primitive (step 3's, with its
  byte-identical pin re-verified).
