# Design brief: umber-fill-cache-lifecycle (2026-11-20)

Kill lock: `razul-fill-cache-lifecycle`. Base: K5b import plus review hardening
(`0d1b6ad7`, `6ffec3bc`, pushed truth `5e3cb8e0`). This is the named follow-up
to K2's vector lifecycle work, applied to the older Fill path rather than left
as a second-class cache.

## What exists

`FillLayerState` caches per tile by one `size` and a single `last_symmetry`.
`refresh_fill_layer` ignores `height`, so non-square channels can evaluate and
fold incompatible grids. `open_project_at` resets vector state but not Fill
state. Fill has no `retain_live` or `observe_frame`, and open seeding treats any
plane for a layer as sufficient even when the layer's other active tiles are
missing. Budget retry has a prospective-bytes threshold but no explicit
refusal watermark test, and refresh clones RGBA and coverage before masking.

K2's vector path already has the stronger contract: full cache identity,
retained-live pruning, frame observation, open completeness, cache-bounded
retry, and single-pass mask/premultiply. Fill should share those semantics
without changing `evaluate_fill_tile`, project files, or the composite format.

## Target and decisions

1. Cache `CachedTile` records width and height. `evaluate` and `coverage_of`
   take both dimensions and recompute on params, symmetry, tile, or dimension
   changes. An `evaluation_count` accessor makes cache hits and retries
   testable. Non-square tiles use `gate_and_fold`, the same single-pass routine
   already proven byte-identical to mask-then-fold.
2. `FillLayerStates` gains `retain_live` and `observe_frame`, matching Vector:
   deleted layers leave the map and dirty queue; symmetry or set-size changes
   queue every live Fill once, with dedup on repeated frames.
3. `fill_planes_complete` checks every active tile on the layer's declared
   channel. `ensure_seeded` uses completeness only on first sight; existing
   states always adopt edits. `open_project_at` resets Fill state, and partial
   or planeless restores seed dirty instead of clean.
4. `mark_dirty` clears cached tiles and budget watermark. Budget refusal
   captures the current `plane_bytes()` watermark; `retry_budget_freed` retries
   only below it and reuses cached tiles without re-rasterizing.
5. `refresh_fill_layer` uses cached grids, folds through `gate_and_fold`, and
   never allocates a plane for an empty or planeless layer. Existing planes
   are explicitly erased when a refresh produces an empty raster. Budget
   refusal leaves dirty state and badge semantics intact.
6. `retain_live` runs in the frame's Fill block before refresh and clears
   deleted-layer badges/dormancy. `open_project_at` clears Fill state beside
   vector state, so document B cannot inherit document A's cache.

## Tests

- Non-square cache identity and single-pass output equality with the previous
  mask-then-fold path.
- Cache hit no re-evaluation; params, symmetry, width, and height each miss.
- Open clean/complete vs dirty/partial/planeless, with existing-state edits
  never suppressed by a restored plane.
- New/open reset clears state and dirty queue.
- Deleted-layer pruning, frame observation, retry below watermark with cached
  tile reuse, and no empty-layer plane allocation.
- Existing Fill, Vector, app, i18n, and full five-crate clippy/test battery.

## Non-goals

No changes to the mesh evaluator, vector raster, project format, Material cache,
release/version work, or public PR. No new UI strings unless a defect requires
one. No claw-driven cargo or commits; independent review stays read-only.
