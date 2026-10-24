# Kill report — paint tiles auto-map by material (W-C4 L5c)

Executed 2026-10-24 06:2x–06:4x by local-7de7d3ed (the fourth wake),
from `docs/loop/briefs/2026-10-24-l5c-auto-assign.md` (brief-first;
the lane census inside it is the live-tree truth, not the docs').

## The lane census (found by this kill's derivation, recorded for the
next session)

The spec's closure tail ("named remaining work, in order: (a) (b)
(c)") is PARTIALLY STALE against the live tree: **(a) the
`with_occluder` split LANDED** as `0955ac8` ("the masked-bakes ray
half", grep-verified: `OccluderScope` lives at
umber-bake/src/lib.rs:53) — the tail predates it. (b) the OccluderScope
PANEL ROW + i18n keys remain (the enum itself landed with (a); the
.ftl catalog lock state unverified — check before that slice). (c)
= THIS KILL. L4 stays GATED (winink.rs absent — S1's bytes never
landed). L6's history.rs IS in-tree (open after this). L7 G1-gated.

## What landed

- **`PaintState::auto_assign_tiles_by_material(mesh)`**: for each
  present tile of a material-mapped mesh, the MAJORITY material's
  set claims the tile through `assign_tile_to_set` (which registers
  the set and steers the tile's future `ensure_tile` sizes and mask
  gating). Ties break to the LOWEST material index — deterministic.
  `triangle_materials` EMPTY (USD, legacy) = an honest no-op: today's
  behavior byte-identical, the v1 no-regression contract.
- **The call: `set_mesh`'s tail** — one pass at mesh load, because
  the composite builds per-set rows at EVERY snapshot (export, save,
  composite), so the mapping must hold continuously, not at bake
  time. Manual reassignment stays available.
- `assign_tile_to_set` UNTOUCHED — its registry/mask-sync semantics
  are the tool this kill drives.

## The two derivation misses (named honestly)

1. The first test draft moved `device`/`queue` into four
   constructions — the crate's own `.clone()`-per-state pattern is
   the rule.
2. The first asserts read `tiles_of_set`, which lists only
   MATERIALIZED tiles (tile 1002 has no target yet, so Hilt's set
   read empty while the registry held the assignment). The true
   store is the `tile_sets` registry, read through `set_of_tile` —
   the composite resolves tiles as they materialize. The asserts
   moved to the registry surface.

## Tests (1, green through the real adapter)

`set_mesh_auto_assigns_tiles_by_material`: the two-material
two-tile mesh (Body owns 1001, Hilt owns 1002); the shared tile's
3-vs-1 majority (Body) and the 1-vs-1 tie (Body, lowest index); the
unmapped mesh assigning nothing (no new sets).

## Evidence ladder

- The pin green (0.25s).
- The battery's umber-paint + umber-app legs — the full suites guard
  set_mesh's every caller (the kittests included).
- The spec's closure tail + the plan's L5 row amended in the same
  commit.

## Non-goals held

- No OccluderScope panel row (the .ftl lock state unverified).
- No bake-path changes, no importer changes, no L6/L7 work.
