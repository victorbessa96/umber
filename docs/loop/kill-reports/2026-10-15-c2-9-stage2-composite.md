# Kill report: C2-9 stage 2, each tile composites its own set's rows

Date: 2026-10-15. Author: dragon. Lock: `c2-9-stage2-composite` over
`crates/umber-paint/src/{paint_state,snapshot}.rs`,
`crates/umber-app/tests/pool_perf.rs` and this report. Design:
`docs/specs/per-set-stacks-design.md`, stage 2 (hazard 2: one stack's
rows composited for every tile).

## Built

- **`CompositeSnapshot.set_layers: BTreeMap<String, Vec<SnapshotLayer>>`.**
  A tile whose set has an entry composites THOSE rows; any other tile uses
  `layers`. Empty is the default, and every single-stack caller is
  unchanged: 17 struct literals gained `set_layers: Default::default()`,
  with no behavior change.
- `rows_for(set)` is read by `resolve_composite_entries` (the flush and
  the partial flatten) and by `read_anchor_prefix`, through
  `set_of_tile(tile)`.
- **`all_rows()`, the union**, is now what the retire census
  (`removed`) and revival (`revive_check`) read. Switching sets therefore
  never retires an inactive set's planes; dropping a set's rows does.
- `full_owners`: any change to `set_layers` recomposites every owner of
  the union plus Base. This is the order arm's conservative rule.

## Test (GPU, ran)

`each_tile_composites_its_own_sets_rows`: tiles 1001/1002 are assigned
to sets A/B, and layers 20 and 21 are published on BOTH tiles. With
per-set rows, 1001 composites 200 (layer 20 only) and 1002 composites 60
(layer 21 only). The union census retires layer 21 exactly when set B's
rows go, and the identical snapshot retires nothing.

**Mutation check:** compositing the base rows for every tile fails it.

Fixture note: the first run read 0. New sets default to the 4K size, so
the 64² uploads were dropped as torn (the publish contract). The test
now `resize_set`s both sets and asserts the size, so a wrong-size
fixture fails loudly at setup.

Gate 1: umber-paint 163, umber-app 519. fmt and clippy clean.

## Next

Stage 3: the per-set `Document` (a stack + history per set,
`activate_set`, every set saved and opened) and the shell's snapshot
builder filling `set_layers` from every set's stack.
