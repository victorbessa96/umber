# Brief — W-C4 L3: ID-mask bake-on-tool-entry (the pre-warm)

Written 2026-10-24 05:4x by local-7de7d3ed (the third wake's
derivation) so the kill executes from this file. Source: the W-C4
plan's L3 section + the P1 tile-cache report
(`docs/loop/kill-reports/2026-10-19-idmask-tile-cache.md` — the
anchored cache seams). Every anchor re-derived live before this
write; all held.

## The lane (the plan's words, the derivation's numbers)

The IdMask arm computes `idmask_armed` and calls
`set_idmask_armed` at tool entry (main.rs:537-541). The first click
then pays the bake: for a whole-mesh (single-tile) mesh — today's
every mesh — that is the FULL-FRAME compute bake plus readback
(`bake_and_select_id`'s own doc: 16M texels, ~64 MB at the 4K
default) — the entry-latency hitch. The lane: PRE-WARM the cache at
tool entry so the first click hits.

## The kill's shape

1. **NEW** `prewarm_id_bake_cache(&mut self, mesh: &MeshData)` on
   PaintState (beside `bake_and_select_id`, paint_state.rs:2473):
   - `if self.stroking { return }` — a no-op, never an error (the
     pre-warm is opportunistic; the tool switch is between strokes
     anyway, but the guard mirrors the click's refusal discipline).
   - **The whole-mesh gate ONLY**: `present_tiles(mesh) ==
     [FIRST_TILE]` — the exact branch whose bake is the hitch. The
     pre-warm bakes `mesh` whole at `tile_size(FIRST_TILE)` with
     `IdFlavor::Triangle` (the flavor the click hardcodes — same fn,
     same key: `id_bake_key(mesh, IdFlavor::Triangle)`), inserts the
     entry, and increments the `#[cfg(test)]` counter (the same
     cfg-guarded BLOCK form the E0658 lesson pinned).
   - **Multi-tile meshes NO-OP** — honestly, by design: the cache
     holds ONE entry (`CachedIdBake { key, tile, width, height, bytes
     }`), so a multi-tile pre-warm cannot warm every tile; warming
     only the first present tile would half-fix the hitch and add a
     wrong-entry smell (an entry the click's tile misses on anyway).
     The per-tile bakes are already cheap (one tile's triangles);
     the hitch is the whole-mesh bake. The multi-entry cache = L3b,
     the named follow-up.
   - Errors: the bake's error returns WITHOUT touching the cache
     (the same replace-only-after-success discipline — the pre-warm
     inherits it by inserting only on bake success); the caller
     ignores the Result (`let _ =`).
2. **The arm's call** (main.rs, after `set_idmask_armed`):
   `if idmask_armed { if let (Some(p), Some(mesh)) = (self.paint.as_deref_mut(), self.mesh.as_ref()) { let _ = p.prewarm_id_bake_cache(mesh); } }`
   — the disjoint-borrow pattern is the block's own precedent
   (main.rs:585's `(self.paint.as_mut(), self.mesh.as_ref())`).
3. **`bake_and_select_id` UNTOUCHED** — its cache hit test,
   take/restore, and replace-after-success stay exactly as the P1
   kill wrote them; the pre-warm only PRE-FILLS the entry the hit
   test then finds.

## The tests (the P1 kill's own observability: `id_bakes_run()`)

- **The click-hit pin** (GPU): a single-tile mesh → prewarm →
  `id_bakes_run()` == 1 → `bake_and_select_id(uv)` succeeds →
  `id_bakes_run()` STILL 1 (the hit — no second bake) and a second
  click's selection equals the first's id (byte equality).
- **The multi-tile no-op pin** (CPU): a two-tile mesh → prewarm →
  counter unchanged (nothing baked; the entry untouched).
- The prewarm-while-stroking no-op: set stroking → prewarm →
  counter unchanged.

## Verification ladder

fmt; the battery's umber-paint + umber-app legs; the plan's L3 row
closed in the same commit; the report names L3b (the multi-entry
cache for multi-tile pre-warm) as the follow-up.

## Non-goals

- No multi-entry cache (L3b, named).
- No flavor parameterization (the click hardcodes Triangle today;
  when a flavor picker exists, BOTH fns move together — the L2
  lesson).
- No UI change (the arm block gains one call; nothing visible).
