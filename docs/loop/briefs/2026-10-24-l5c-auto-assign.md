# Brief — W-C4 L5c: paint tiles auto-map by material (the §4/§5 slice)

Written 2026-10-24 06:2x by local-7de7d3ed (the fourth wake's
derivation) so the kill executes from this file. Source: the spec's
§4 paint-tiles half (`docs/specs/per-set-masked-bakes.md`) + the
closure tail's named remaining item (c). The lane census first: L4
GATED (winink.rs absent — S1's bytes never landed); L5(a) LANDED
(`0955ac8`, the occluder split — the closure tail's "named remaining
work (a)" is STALE, grep-verified: `OccluderScope` lives in
umber-bake/src/lib.rs:53); L5(b) partially landed (the enum exists;
the panel row + i18n keys remain, double-gated on the .ftl catalog
lock state — verify the lock before that slice); L5(c) = THIS KILL,
ungated; L6 (the unified timeline) has history.rs in-tree = open
after this; L7 G1-gated.

## The contract (the spec's §4 words)

"A multi-material mesh's sets map to the tiles their UVs occupy
(`triangles_for_tile` ∩ material triangles), so the composite's
per-set rows line up without manual assignment."

The composite builds per-set rows from the paint session's tile→set
map at EVERY snapshot (export, save, composite) — so the mapping's
home is MESH LOAD (`PaintState::set_mesh`), not the bake trigger: one
assignment at load, manual reassignment stays available.

## The kill's shape

1. **NEW** `PaintState::auto_assign_tiles_by_material(mesh)` (beside
   `set_mesh`):
   - For each present tile (`umber_mesh::present_tiles(mesh)`):
     `triangles_for_tile(mesh, tile)` → the triangles' materials
     (`mesh.triangle_materials[tri]`) → the MAJORITY material (most
     triangles; ties → the LOWEST material index, deterministic) →
     `assign_tile_to_set(tile, &mesh.material_names[material])`.
   - `triangle_materials` EMPTY (unmapped mesh — USD, legacy files):
     NO-OP (the session set keeps every tile; today's behavior
     byte-identical — the v1 no-regression contract the ray half
     pinned too).
   - A tile whose triangles carry NO material index (empty map is
     all-or-nothing per the importers — a partial map cannot occur:
     the all-or-nothing attach is the landed §2 contract) — no such
     tile exists under the landed importers; the majority count
     simply skips index-less triangles defensively (they count for
     no material).
   - The register-first semantics arrive free through
     `assign_tile_to_set` (it registers the set before moving).
2. **The call**: `set_mesh`'s tail — after the seam graph + the tile
   census (the assignment uses the same mesh; set_mesh is the one
   mesh-load site).
3. `assign_tile_to_set` UNTOUCHED — its move/register/mask-sync-flag
   semantics are the tool this kill drives.

## The tests (paint_state's test module)

- The two-material two-tile mesh (Body's triangles all in 1001,
  Hilt's all in 1002, `triangle_materials` mapped): set_mesh → tile
  1001 ∈ Body's set, 1002 ∈ Hilt's set
  (`tiles_of_set("Body") == [1001]`, `tiles_of_set("Hilt") ==
  [1002]`).
- The SHARED tile (both materials have triangles in 1001 only):
  majority wins (3 Body triangles vs 1 Hilt → 1001 ∈ Body); a tie
  (1 vs 1) → the lowest index (Body).
- The UNMAPPED mesh (empty `triangle_materials`): set_mesh → no new
  sets (`set_names()` unchanged), every tile stays in the session
  set.

## Verification ladder

fmt; the battery's umber-paint leg (the pins) + umber-app (set_mesh's
caller compiles); the spec's closure tail gets the (c) row closed in
the same commit; the plan's L5 row gets the census-truth amendment
(the (a) staleness named, the lane's live state recorded).

## Non-goals

- No OccluderScope panel row (L5(b) — the lock state unverified).
- No unified timeline (L6 — the next lane after this).
- No importer changes (§2 is landed; the mapping consumes what
  exists).
- No bake-path changes (the assignment is load-time; the bake
  fan-out reads the sets it already reads).
