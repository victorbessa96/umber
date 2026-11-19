# Design brief: umber-vector-raster (K2 of the path-tool vector wave, 2026-11-19)

Kill lock: `razul-vector-raster` (code phase; this brief commits docs-first under
`razul-vector-raster-brief`). K1 landed as `46d4bff5`. ROADMAP row 245's open
half is now live vector rendering; K3 canvas editing, K4 panel widgets, and K5
SVG io remain separate kills.

## What exists

- K1's `umber_core::vector::{Anchor, Stroke, VectorShape, VectorParams}` holds
  one cubic-bezier path in UV coordinates, sRGBA fill, and an optional stroke.
  `LayerKind::Vector` and `Layer.vector` are serde-additive, with no format bump.
- K1 deliberately composites vectors as transparent. `SnapshotLayerKind::Vector`
  already rides the same per-layer row path as Fill, including instances,
  visibility, opacity, layer masks, and geometry masks.
- `umber_core::svg` already owns the clean-room device-space cubic flattener,
  nonzero scanline fill, half-open edge rules, 4x4 supersampling, and exact
  coverage quantization. Its internals are private to that module today.
- `umber-app/src/fill_layer.rs` already owns the eval-cache → symmetry →
  quick-mask → premultiplied `publish_plane` pipeline. Paint sidecars persist
  every layer-owned plane generically, so no vector-specific persistence format
  is needed.

## Target and decisions

1. **One raster engine, not two.** K2 exposes only crate-private SVG helpers
   needed by `vector.rs`: device-space poly building and nonzero polygon
   rasterization. Vector geometry maps UV `(x,y)` to raster `(x*width,y*height)`
   before flattening, exactly like an SVG viewBox stretched over the same raster.
   Existing SVG output stays byte-identical.
2. **Closed paths fill under the nonzero rule.** An open path contributes no
   fill. Fill needs at least two anchors and nonzero enclosed area (two cubic
   segments can close a loop). Transparent fill contributes nothing.
3. **Stroke works on open and closed paths.** Every cubic segment shares the
   same flattened centerline as the fill. K2's documented v1 stroke joins and
   caps are ROUND. Width uses the shorter raster edge: `stroke.width *
   min(width,height)` device px; non-finite or non-positive width contributes
   nothing. Segment quads plus round join/cap polygons are unioned through the
   same scanline core.
4. **Stroke composites source-over fill per texel in sRGB-encoded space.** Color
   alpha multiplies the 4x4 geometric coverage before source-over.
   `VectorRaster.rgba` carries the resulting straight RGB with alpha 255 wherever
   coverage is nonzero; `VectorRaster.coverage` carries total source-over alpha.
   This is deliberate: the existing `fold_coverage_to_premultiplied` then performs
   the final color and alpha quantization exactly once, after symmetry and
   quick-mask gating. Linear-light blending is a separate wave.
5. **Vectors are eval-owned baseColor planes.** `VectorLayerState` caches each
   tile by `(generation, size, symmetry)`. Shape edits, undo/redo/journal replay,
   and explicit open invalidation bump the generation; frame observation queues
   every vector on tile-size or symmetry changes. Refresh reuses the fill helpers
   verbatim: `apply_symmetry` → `apply_quick_mask` →
   `gate_and_fold` → `publish_plane`. No mesh is required. `gate_and_fold` is
   byte-identical to `apply_quick_mask` → `fold_coverage_to_premultiplied` and
   borrows the cached tile, eliminating two full-tile clones. Deleted layers are
   pruned; CPU cache is capped per layer and document-wide with LRU eviction.
6. **Open/new behavior follows Fill.** `AppState.vector_states` resets with the
   document and is explicitly cleared on project open. Complete saved-plane sets
   seed CLEAN on first sight, including parked sets; a partial restore queues the
   missing work. Later edits always use normal dirty
   adoption. A planeless vector seeds DIRTY and evaluates without a mesh. Empty
   vectors erase an already-owned plane but never materialize a zero plane.
   Vectors have no legacy pre-v2/Base dormancy case. `Passthrough` composites as
   `Normal`, matching Fill. Budget refusal records a retry watermark and
   republishes cached tiles only after plane bytes fall; raster refusal stores a
   typed error, shows `layers.vector-refused`, and erases any stale plane the
   layer already owns without allocating a planeless fallback. Refusal reporting
   aggregates across the refresh, never letting a later valid tile hide an error.
7. **No schema or migration change.** K1's additive `Layer.vector` remains the
   only document metadata. K2 changes no project version and adds no vector-
   specific sidecar. Existing plane save/load and clean-skip logic is the
   persistence contract.

## API and budgets

`pub fn rasterize(shape: &VectorShape, width: u32, height: u32) ->
Result<VectorRaster, VectorError>`.

- `VectorRaster`: private width/height/rgba/coverage fields with accessors and
  `into_rgba_coverage` for the app refresh.
- Zero-sized requests return an empty raster. Oversized requests return
  `VectorError::TooLarge`; `MAX_VECTOR_RASTER_PIXELS = 8192²`, matching the
  app's 8K ceiling.
- `MAX_VECTOR_ANCHORS = 65_536` and `MAX_VECTOR_POINTS = 262_144` bound work
  before raster allocation. Exceeding either returns `VectorError::TooComplex`.
  Centerline and stroke points are charged BEFORE each cubic, quad, or ellipse
  is emitted, so the cap is also the construction-time memory bound. Cubics
  append into the shared centerline without a per-segment temporary `Vec`.
- `MAX_VECTOR_EDGE_ROW_VISITS = 100_000_000` bounds the conservative
  `edges × height × 4` scanline cost and returns `VectorError::TooExpensive`
  before either coverage plane is allocated. Empty and zero-sized requests
  short-circuit before flattening; refused rasters allocate no fallback plane.
- The CPU cache ceiling is 512 MiB per layer plus the just-used tile and 1 GiB
  document-wide plus the active layer; older entries/layers are evicted by
  deterministic LRU rules and deleted layers are pruned. At the 8K raster cap
  the core's successful-raster peak is bounded at 640 MiB (two u8 coverage
  planes plus straight RGBA and f32 coverage). Single-pass `gate_and_fold`
  removes the app-side RGBA+coverage clones, leaving one output plane.
- Non-finite/degenerate geometry contributes no coverage and never panics.
  Cache-key comparison treats identical NaN bits as stable (ordinary float
  semantics otherwise), so invalid inputs cannot re-edit or re-rasterize every
  frame. Determinism is required at identical inputs, including non-square
  rasters.

## Concurrency model

Everything runs on the existing UI/eval thread, synchronously, like Fill. The
cache has no locks and spawns no worker. `publish_plane` performs the established
GPU queueing; the ordinary end-of-frame composite sees the new plane. No file
locks, network calls, or new dependencies.

## Security and robustness model

Project files can contain arbitrary vectors. Validate dimensions and structural
budgets before allocating, bound flattened points, reject no whole document on
one malformed anchor, and skip only the non-contributing geometry. No unsafe
code, no unwrap/expect/panic in `umber-core` production, no new file parser, no
external content, and no change to `.spp`/`.sbsar` walls.

## Verification

Core unit tests in `vector.rs`:

- `centerline_matches_individually_flattened_cubics` and
  `cubic_into_appends_without_duplicate_start`: the append-only shared
  flattener preserves every point and its pre-emission count exactly.
- `closed_shape_matches_svg_coverage_exactly` and
  `closed_shape_with_curved_closure_matches_svg`: opaque closed cubic coverage
  equals the equivalent SVG byte-for-byte, including a genuine curved closure.
- `open_shape_stroke_has_round_caps`: exact center/exterior/cap coverage,
  including cap/quad overlap; `zero_alpha_stroke_is_identical_to_no_stroke`
  pins transparent stroke behavior.
- `stroke_composites_source_over_fill`: exact RGBA/coverage values for an
  alpha-red fill under an alpha-blue stroke.
- `invalid_and_empty_geometry_is_transparent` and
  `empty_inputs_short_circuit_before_geometry`: empty, one-anchor, NaN control
  points, zero-size, and degenerate inputs never panic, paint accidental lines,
  or allocate needlessly.
- `cache_identity_is_stable_for_nan_and_signed_zero`,
  `cache_identity_is_stable_for_nan_offsets_and_signed_zero`, and
  `identical_nan_geometry_and_symmetry_do_not_invalidate`: malformed inputs
  cannot cause perpetual adoption, cache misses, or frame-wide requeues.
- `raster_is_deterministic_and_non_square`: repeated inputs are byte-identical;
  non-square UV mapping and shorter-edge stroke width are pinned.
- `dimension_budget_is_a_typed_error`, `structure_budgets_are_typed_errors`,
  `stroke_geometry_budget_is_typed_error`, and
  `edge_row_budget_is_a_typed_error`: exact typed dimension, point, stroke
  construction, and scanline-cost refusal.

App tests in `fill_layer.rs`:

- `vector_state_evaluates_caches_and_invalidates`: first evaluation is exact,
  an unchanged call reuses the cache, and shape/symmetry/size changes re-evaluate.
- `vector_refresh_publishes_plane_without_mesh`: a real `PaintState` refresh
  publishes `(Layer(id), tile, baseColor)` with exact premultiplied texels and
  no mesh; quick-mask gating is pinned on the same plane.
- `vector_states_open_clean_or_planeless_dirty`: open seeding does not re-run
  saved planes, while planeless vectors queue exactly once.
- `vector_state_refreshes_every_tile_and_symmetry` and
  `open_seed_recomputes_every_planeless_tile`: multi-tile refreshes cannot serve
  stale bytes after an edit, symmetry change, or document open.
- `frame_observation_requeues_on_symmetry_and_size_changes`,
  `budget_retry_waits_for_bytes_to_fall_and_reuses_cache`,
  `vector_cache_is_bounded_and_pruned_with_live_layers`, and
  `vector_refusal_is_recorded_without_fallback_allocation_and_cleared`: frame
  invalidation, retry, memory, and refusal contracts.
- `non_square_mirror_y_maps_rows_exactly`,
  `empty_tile_symmetry_is_an_untouched_passthrough`, and
  `texel_rect_conjugation_commutes_with_the_uv_map`: rect symmetry uses
  independent X/Y scales and empty grids never panic.
- `gate_and_fold_matches_mask_then_fold_exactly`: the borrowed single-pass
  optimization is byte-identical to the original mask→fold pipeline.
- `ensure_seeded_honors_saved_planes_only_on_first_sight`: parked-set and open
  planes stay authoritative on first sight without suppressing later edits.
- The extended `vector_refresh_publishes_plane_without_mesh` also pins cache-hit
  dirty draining, valid→invalid refusal erasure and recovery, owned-plane
  erasure on empty vectors, and no allocation for planeless empty layers.
- `partial_restore_is_not_clean`: one restored tile cannot suppress a missing
  tile; clean first sight requires every active BaseColor plane.
- `mixed_size_refusal_is_aggregated_in_both_orders`: fail-then-succeed and
  succeed-then-fail both keep the refusal visible without dirty churn.

Battery: rustfmt on touched `.rs`; `check-layers.py`; `scorecard.py --check`;
clippy `-p umber-core -p umber-brush -p umber-paint -p umber-app --all-targets -- -D warnings`;
tests for those four crates under `flock /tmp/umber-cargo.lock`,
`CARGO_BUILD_JOBS=3`, `CARGO_PROFILE_TEST_DEBUG=0`, `nice -n 19`, and
`UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`. A pre-existing failure must be
reproduced at clean HEAD before it can be named environmental. The known
`umber-cli` `library_list_step_replies_rows_over_the_real_dirs` environment
failure is outside this kill's touched crates.

## Exact lock surface

- `crates/umber-core/src/vector.rs`
- `crates/umber-core/src/svg.rs`
- `crates/umber-core/src/layers.rs` (K1 comment sync only)
- `crates/umber-brush/src/symmetry.rs` (rect-tile conjugation)
- `crates/umber-app/src/fill_layer.rs`
- `crates/umber-app/src/main.rs`
- `crates/umber-paint/src/paint_state.rs`
- `crates/umber-paint/src/snapshot.rs` (K1 comment sync only)
- `assets/i18n/en-US.ftl`
- `assets/i18n/pt-BR.ftl`
- `docs/ROADMAP.md`
- `docs/loop/briefs/2026-11-19-umber-vector-raster.md`
- `docs/loop/kill-reports/2026-11-19-umber-vector-raster.md`
- `docs/loop/queue.md`

## Non-goals

No K3 canvas gestures or anchor-edit journal op, K4 vector property widgets,
K5 SVG conversion/export, fill-rule selector, stroke cap/join selector, dash
patterns, gradients, multi-shape layers, per-channel vector targets, or project
version bump. Those boundaries are named here so K2 does not grow into the
whole path-tool wave.
