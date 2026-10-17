# Kill report: snapshot-tests (audit no-test-file finding #3)

- Authorship: dragon (solo, host-down pass; the kill was surfaced by
  the Claude-CLI consultation, 2026-10-17 03:3x).
- Kill-lock `snapshot-tests`: `crates/umber-paint/src/snapshot.rs`.
- Closes one of the full-project audit's five named no-test files
  (audit-2026-10-17, "What's missing" #3): the per-frame composite
  snapshot builder — the single source both front ends (the shell's
  frame, the CLI agent's post-step flush) build their composite rows
  from — had zero tests of its own; only downstream flush consumers
  exercised it.

## Built

7 contract tests, `#[cfg(test)] mod tests` in snapshot.rs:

- `base_is_first_and_carries_base_visible` — Base is row 0 of an
  empty stack; `base_visible` wires to both the row and the snapshot.
- `paint_and_fill_rows_carry_their_own_settings` — bottom-to-top order
  (Base, then the stack's order), the right `SnapshotLayerKind` per
  `LayerKind`, defaults (visible, opacity 1.0, mask off).
- `folders_contribute_no_row` — a folder's INSIDE paint renders; the
  folder itself has no row (compositor judgment 4).
- `a_material_without_state_yields_no_channels_the_agent_path` — the
  CLI agent session's documented truth: empty `MaterialLayerStates`
  produces a Material row with empty channels, not a crash, not a
  fabricated default.
- `an_instance_renders_its_source_and_a_dangling_one_is_skipped` —
  stage-5 shape: the instance row carries the SOURCE's owner with the
  instance's own layer_id; a dangling source contributes nothing.
- `displayed_rides_through_untouched` — the caller's `(tile, channel)`
  list is the snapshot's, byte for byte.
- `resolve_instance_source_walks_chains_and_stops` — a two-hop chain
  resolves to the root source; a dangling id returns None (the walk
  is bounded by the stack's length — the loop-guard contract).

## Judgment calls

- **Contract tests over channel-expansion tests**: the material
  channel expansion (opacity product) needs the FakeTileIo refresh
  drive harness, which is material_layer.rs's own test surface (its
  `opacity_product_golden_publishes_src_for_composite` already pins
  the product at :1706). Duplicating that harness here would double
  maintenance and pin nothing new — snapshot.rs's OWN contracts are
  the row shapes, the order, the skips, and the passthroughs.
- **The agent path IS a contract**: an empty-states material row
  yielding empty channels is documented behavior (the module docs say
  so); now it is pinned, so a future "helpful default channels"
  change fails loudly instead of silently changing what the agent
  session composites.
- **Fixture APIs verified before writing**: `add_layer` returns the
  id and constructs `Instance { source_id }` / `Folder { passthrough
  }` freely (no attach-time validation on add — the dangling instance
  is legal construction and the skip is the tested behavior).

## Unverified

- Battery (host down since ~18:25, ~9h). Static: rustfmt PASS; every
  fixture API read from source before use (add_layer at layers.rs:977,
  payload shapes at :37/:54, SnapshotLayerKind at paint_state.rs:820,
  `displayed` at :910). Ad-hoc verified, not suite-verified.

## Deferred

- The builder.rs test kill (spp) — blocked by the sppv2-r1 lock until
  the wave commit.
- Claude's other two code findings, dispositioned honestly:
  `FalloffCurve::Custom` is NOT a simple variant add — the enum is
  Copy + Eq + a GPU-side stable-id the WGSL `radial_falloff` switch
  dispatches on; a Custom payload needs a curve-texture/uniform
  design + host verification. Parked as a W-GUI-3 candidate with the
  blocker named. The CurveEdit consumers (falloff Custom row, Levels
  editor) share that dependency.
