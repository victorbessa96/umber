# Kill report: umber-fill-cache-lifecycle (2026-11-20)

Authorship: dragon implementation and verification, with bounded read-only reviews by OpenCode DeepSeek and a fresh-context delegate, then final review by `opencode/beam-free`. Kill lock: `razul-fill-cache-lifecycle`.

Base: K5b import plus review hardening (`0d1b6ad7`, `6ffec3bc`, pushed truth `5e3cb8e0`). Brief: `35aae84b`. Code landing: `8747c681`.

Implements `docs/loop/briefs/2026-11-20-umber-fill-cache-lifecycle.md`: full-dimension cache identity, open/reset and open-seeding lifecycle, live pruning, frame observation, and prospective-budget retry parity with Vector.

## Built

- `CachedTile` records width and height. `evaluate` and `coverage_of` take both dimensions and recompute on params, symmetry, tile, or dimension changes. `evaluation_count` proves cache hits. Refresh uses `gate_and_fold`, the single-pass mask/premultiply routine already byte-equivalent to mask-then-fold.
- `FillLayerStates` gains `retain_live` and `observe_frame`, with deleted-layer pruning and deduped symmetry/size transitions. Main calls them in the Fill frame block.
- `fill_planes_complete` checks every active tile on the layer's declared channel, rejects empty tile sets and foreign-channel planes. `ensure_seeded` uses completeness only on first sight; existing states always adopt edits. `open_project_at` resets Fill state, and partial/planeless restores seed dirty instead of clean.
- `mark_dirty` clears cached tiles and all budget state. Plane-budget refusal captures current `plane_bytes()` and `plane_hard_cap_bytes()`. Retry only below the byte watermark or above the refusal cap and reuses cached tiles without re-rasterizing.
- `refresh_fill_layer` uses cached grids, folds through `gate_and_fold`, never allocates a plane for an empty or planeless layer. Existing planes are explicitly erased on empty raster. Budget refusal leaves dirty state and badge semantics.
- Observation includes sorted active-tile membership and skips dormant ids. Ambient transitions never wake dormant fills; explicit edits still queue through `ensure`.
- `ensure_open` was removed. `ChannelSizeSnapshot` and the struct doc comments are attached correctly.

## Verification

- Focused Fill suite: **30 passed / 0 failed**, including non-square cache identity, open seeding/dormancy, pruning/observation, retry with cache reuse, non-square refresh, and zero-size channel behavior.
- Actual budget-refusal test drives PlaneBudget, frees bytes, and proves retry publishes the cached grid with `evaluation_count == 1`.
- Full five-crate battery: **clippy 0 errors; 21 suites, 2,174 passed / 0 failed**. Receipt: `~/.hermes/cache/scratch/fill-cache-lifecycle-final-battery-r3.log`.
- Final review: **SHIP** from `opencode/beam-free`, no HIGH/MEDIUM defects. Receipt: `~/.hermes/cache/scratch/fill-cache-lifecycle-review-beam.log`.
- Ad-hoc verifier accepted four focused checks, 1/1 each.

## Deferred

- Native open-project integration testing with real project files and UI smoke, beyond the headless lifecycle tests.
- Fill cache-lifecycle parity is otherwise complete for this kill.
