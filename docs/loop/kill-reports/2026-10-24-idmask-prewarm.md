# Kill report — ID-mask bake-on-tool-entry (W-C4 L3)

Executed 2026-10-24 05:4x–06:0x by local-7de7d3ed (the third wake),
from `docs/loop/briefs/2026-10-24-l3-idmask-prewarm.md` (written from
the derivation first; every anchor held live — the arm block at
main.rs:537, `bake_and_select_id` at paint_state.rs:2473, the P1
cache report's seams).

## What landed

- **`PaintState::prewarm_id_bake_cache(mesh)`**: the whole-mesh
  (single-tile) branch's full-frame bake — 16M texels, ~64 MB at the
  4K default, the entry-latency hitch — runs at TOOL ENTRY instead of
  the first click. Same key (`id_bake_key(mesh, Triangle)`), same
  flavor, same `tile_size(FIRST_TILE)` dims: the entry the click's
  hit test then finds. The `#[cfg(test)]` counter increments in the
  same cfg-guarded BLOCK form the E0658 lesson pinned.
- **The arm's call** (main.rs, beside `set_idmask_armed`): the
  disjoint-borrow pattern the block itself already uses at :585;
  `let _ =` — opportunistic everywhere.
- **Multi-tile meshes NO-OP, by design**: the cache holds ONE entry
  and the click tile is unknown at entry; warming a guessed tile
  would half-fix the hitch. The per-tile bakes are already cheap (one
  tile's triangles). The multi-entry cache = **L3b**, the named
  follow-up.
- **Mid-stroke NO-OP**: the guard mirrors the click's refusal
  discipline (the tool switch is between strokes anyway).
- **`bake_and_select_id` UNTOUCHED**: its hit test, take/restore, and
  replace-after-success stay exactly as the P1 kill wrote them — the
  pre-warm only pre-fills.

## The derivation's own miss (named honestly)

The first test draft asserted two different uvs select the same id —
wrong by the bake's own semantics: the quad's two triangles are two
regions under `IdFlavor::Triangle` (the P1 test itself pins exactly
that). The corrected pin: the same uv re-clicked selects the same id
(the hit's restore semantics). The pre-warm code was never wrong;
the test's assumption was.

## Tests (2, both green through the real adapter)

- `idmask_prewarm_fills_the_cache_and_the_first_click_hits`: prewarm
  bakes once (counter 1), the first click HITS (counter stays 1), the
  repeat click re-selects the same region (the entry restored).
- `idmask_prewarm_noops_multi_tile_and_mid_stroke`: the two-tile
  strip bakes nothing; the mid-stroke pre-warm bakes nothing.

## Evidence ladder

- The two pins green (0.23s).
- The battery's umber-paint + umber-app legs (the app's clippy covers
  the arm's call).
- The plan's L3 row closed in the same commit.

## Non-goals held

- No multi-entry cache (L3b, named in this report).
- No flavor parameterization (the click hardcodes Triangle; a future
  flavor picker moves both fns together — the L2 lesson).
- No UI change (the arm block gains one call).
