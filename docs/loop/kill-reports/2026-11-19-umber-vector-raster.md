# Kill report: umber-vector-raster (K2, 2026-11-19)

Authorship: dragon implementation and verification, with independent read-only
reviews by Pi and a fresh-context delegate. Kill lock: `razul-vector-raster`.
The lock held `crates/umber-core/src/{vector,svg,layers}.rs`,
`crates/umber-brush/src/symmetry.rs`, `crates/umber-app/src/{fill_layer,main}.rs`,
`crates/umber-paint/src/{paint_state,snapshot}.rs`, both i18n catalogs,
`README.md`, `docs/ROADMAP.md`, the K2 brief, this report, `docs/loop/queue.md`,
`STATE.md`, and `.dragon/COORDINATION-next-wave.md`.

Implements `docs/loop/briefs/2026-11-19-umber-vector-raster.md`: Target and
decisions 1–7, API and budgets, concurrency, robustness, and verification.
K1 is `46d4bff5`; the K2 brief is `60322097`; the K2 code landing is
`0b5cb72d`. K3 canvas editing, K4 panel widgets, and K5 SVG conversion/export
remain separate kills.

## Built

- `umber-core/src/vector.rs`: public `rasterize` + `VectorRaster` and typed
  `VectorError`. Closed paths use nonzero fill; open/closed paths use round-cap
  and round-join strokes; stroke composites source-over fill in sRGB-encoded
  space. Straight RGB plus total coverage enters the existing premultiply path.
  Empty inputs short-circuit; width/height accessors and deterministic output
  are pinned. Budgets cover 8K pixels, anchors, flattened points, construction-
  time stroke polygons, and 100M conservative edge-row visits.
- `umber-core/src/svg.rs`: crate-private device-poly helpers and exact ellipse
  cost sharing. K2 uses the existing cubic flattener and scanline core; the
  public SVG API is unchanged and empty polygon lists allocate nothing.
- `umber-brush/src/symmetry.rs`: `to_texel_space_xy` and
  `texel_transforms_rect` apply the full rect affine conjugation, including
  `sx/sy` and `sy/sx` off-diagonal rotation terms. Square behavior delegates to
  the same math unchanged.
- `umber-app/src/fill_layer.rs`: `VectorLayerState` caches per tile by
  generation, dimensions, and symmetry. Refresh is evaluate → symmetry →
  single-pass mask/premultiply → `publish_plane`, with no mesh input. Frame
  observation queues on symmetry/size changes; project-open and planeless-open
  invalidation cannot serve stale tiles. Cache is capped per layer and globally,
  budget retry reuses cached bytes, empty grids never panic, empty and refused
  rasters erase only already-owned planes, refusal aggregates across tiles,
  and only complete restored plane sets seed clean.
- `umber-app/src/main.rs`: document reset and `open_project_at` clear vector
  state; open seeding distinguishes restored planes from planeless vectors;
  the frame loop adopts by reference, prunes deleted layers, observes symmetry
  and size, refreshes dirty vectors, and shows localized refusal/budget badges.
- `umber-paint`: vector rows use eval-owned baseColor planes and normalize
  `Passthrough` to `Normal`, matching Fill.
- i18n: `layers.vector-refused` in en-US and pt-BR.
- Tests: 18 focused core vector tests, 15 brush symmetry tests, and 24 app
  fill-layer tests covering cache identity, every-tile refresh, open and parked
  seeding, frame observation, budget retry, cache bounds/pruning, typed refusal,
  empty symmetry, owned-plane erasure, single-pass byte identity, and real plane
  publication. The GPU test prints an explicit SKIP when
  no adapter exists and fails when `UMBER_REQUIRE_GPU=1` without one.
- `README.md`: scorecard regenerated after the new tests (`3,000+` → `3,100+`).

## Judgment calls

- Reused the clean-room SVG raster core through crate-private helpers rather
  than adding a dependency or maintaining a second geometry engine.
- Source-over color mixing is deliberately sRGB-encoded, matching the app's
  existing byte-space premultiply boundary. Linear-light blending is deferred.
- Stroke polygon points are charged before emission. The separate edge-row cap
  is conservative and returns `TooExpensive` before coverage allocation.
- CPU cache is capped at 512 MiB per layer plus the just-used tile and 1 GiB
  document-wide plus the active layer, with deterministic LRU eviction. At 8K
  the successful core raster's documented peak is 640 MiB. Single-pass
  `gate_and_fold` removes the app-side RGBA+coverage clones; streaming row
  composition is deferred rather than hidden.
- `Passthrough` is structural for folders, not a vector pixel operation, so
  vector leaf layers use `Normal` exactly like Fill.
- Pi claimed cap/quad winding cancellation. The exact overlap pixels were
  executed and are fully covered, so no orientation rewrite landed.
- Fill's older per-state dirty and prospective-budget retry patterns were not
  rewritten in this vector-scoped kill. A Fill cache-lifecycle parity pass is
  the named follow-up; the shared rect-symmetry correction does benefit Fill.

## Verification

- Focused core vector suite: **18 passed / 0 failed**.
- Focused app fill-layer suite: **24 passed / 0 failed**.
- Focused brush symmetry suite: **15 passed / 0 failed**.
- Static: rustfmt clean on every touched Rust file; `check-layers.py` GREEN
  (16 crates); `scorecard.py --check` current; `git diff --check` clean.
- Final four-crate clippy + test battery after partial-restore and multi-tile
  refusal aggregation fixes: **clippy 0 errors; 20 suites, 1,964 passed / 0
  failed; `FINAL_BATTERY_EXIT=0`**. Receipt:
  `~/.hermes/cache/scratch/vector-raster-final-battery-r9.log`.
- Fresh independent re-review: **SHIP** (final two-point Nemotron review after
  the Kimi review's FIX-FIRST findings were fixed). The extra notes were
  non-blocking: `ensure_open` is already called before `mark_dirty`, and
  huge-tile zeroing is a bounded allocation required to erase stale art.

Previous battery receipt (`vector-raster-final-battery-r2.log`): clippy 0
errors, 20 suites, 1,954 passed, 0 failed. It predates the second review fixes
and is not the landing gate.

Earlier pre-hardening battery receipt (`vector-raster-tests-r1.log`): 15 suites,
1,677 passed, 0 failed across core, paint, and app. It is historical context,
not the final gate.

## Unverified

- No canvas editor exists yet; K3 owns interactive path manipulation and its
  journal commands. K4 owns property widgets and K5 owns SVG conversion/export.
- No wall-clock claim is made for an 8K vector. The edge-row ceiling bounds the
  worst-case scanline product; dedicated performance fixtures remain follow-up.
- The real publish test runs only where a suitable wgpu adapter exists. Its
  explicit skip is not counted as GPU execution on adapter-less hosts.

## Deferred

- K3 canvas editing, K4 panel widgets, K5 SVG conversion/export.
- Fill-rule/cap/join selectors, dashes, gradients, multi-shape layers,
  per-channel vector targets, linear-light blending, and project format changes.
- Streaming row composition to reduce the documented 8K peak.
- Fill cache-lifecycle parity for the pre-existing per-state dirty, open-reset,
  and prospective-budget retry patterns.

## Process note

Claude review was unavailable because its interactive login had expired;
OpenCode failed with `ECONNREFUSED`. Pi and the fresh-context delegate produced
real reviews. Their actionable findings drove the hardening above. Pi's fresh
re-review then found the empty-symmetry panic, cache-hit dirty churn, empty-plane
drop, app-side clone cost, parked-set seeding, and cubic allocation issues;
those are fixed and pinned in the current tree. The broad Kimi review stayed
active for over an hour; its findings were harvested, then it was replaced by a
bounded final-delta review of the exact fixed paths. Its last two findings —
partial restores seeding clean and last-tile-wins refusal reporting — are also
fixed and pinned. The first final-battery attempt stopped at two test-only
clippy lints before tests; those were corrected and the battery restarted with
a fresh log.
