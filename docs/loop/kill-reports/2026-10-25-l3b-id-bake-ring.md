# Kill report — L3b: the multi-entry ID bake cache (the ring)

Executed 2026-10-25 03:3x by local-7de7d3ed, on Bessa's third
"continue" (the session's third wind).

## The judgment revised

The one-entry cache's own doc named its accepted cost: "a mesh
spanning N tiles pays one re-bake per tile switch". The L3b
follow-up (the 10-19 kill report's Deferred + the prewarm doc's
named lane) was the multi-entry cache. The revision's reasoning:
the re-click case the one-entry judgment optimized for is
untouched, but the alternating A↔B pair re-baked EVERY click —
the thrash, not the switch, is the real cost. A three-entry FIFO
ring kills the thrash and holds one spare at a bounded resident
cost (~192 MB worst case at the 4K default, named in the struct's
doc).

## What landed (paint_state.rs)

- **The field**: `Option<CachedIdBake>` → `Vec<CachedIdBake>`
  with `ID_BAKE_CACHE_CAP = 3` (the ring's bounded worst case).
- **The hit path**: the linear `position` find (the ring is
  tiny), `swap_remove` for the take (the same take-and-restore
  dance around `select_id_mask`'s mutable borrow), and the
  restore pushes to the tail (the MRU slot).
- **The insert**: `push_id_bake` — FIFO eviction of the head at
  cap, push to tail — shared by the click path's
  replace-after-success and the prewarm's insert.
- **The prewarm's doc**: the whole-mesh-only gate STAYS (the
  multi-tile prewarm would guess the click tile — half-fixing,
  smelling wrong); the ring kills the thrash at click time
  instead.
- **The pin**: `idmask_multi_tile_alternating_clicks_both_hit_the_ring`
  — A→B→A on a two-tile mesh: the counter proves two bakes for
  three clicks (the one-entry cache paid three). Both prewarm
  pins pass unchanged (single-entry behavior preserved).

## Verification

`cargo test -p umber-paint`: 180 green (179 + the pin; the
GPU-gated device tests ran — 24.5s). The battery (umber-paint +
umber-app) gates the commit.

## Named follow-ups

- A mesh-edit invalidation hook stays unneeded (the content key
  misses naturally — the one-entry report's own reasoning,
  unchanged).
- The cap's tuning (3 today) rides live use; a fourth-tile
  pattern would name itself.
