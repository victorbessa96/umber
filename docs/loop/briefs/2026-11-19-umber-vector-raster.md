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
   fill. Fill needs at least three anchors and nonzero enclosed area. Transparent
   fill contributes nothing.
3. **Stroke works on open and closed paths.** Every cubic segment shares the
   same flattened centerline as the fill. K2's documented v1 stroke joins and
   caps are ROUND. Width uses the shorter raster edge: `stroke.width *
   min(width,height)` device px; non-finite or non-positive width contributes
   nothing. Segment quads plus round join/cap polygons are unioned through the
   same scanline core.
4. **Stroke composites source-over fill per texel.** Color alpha multiplies the
   4x4 geometric coverage before source-over. `VectorRaster.rgba` carries the
   resulting straight RGB with alpha 255 wherever coverage is nonzero;
   `VectorRaster.coverage` carries total source-over alpha. This is deliberate:
   the existing `fold_coverage_to_premultiplied` then performs the final color
   and alpha quantization exactly once, after symmetry and quick-mask gating.
5. **Vectors are eval-owned baseColor planes.** A small `VectorLayerState`
   beside the fill state in `fill_layer.rs` caches per `(tile, size, symmetry)`.
   Shape edits, undo/redo/journal replay, tile-size changes, and symmetry changes
   invalidate it. Refresh reuses the fill helpers verbatim: `apply_symmetry` →
   `apply_quick_mask` → `fold_coverage_to_premultiplied` → `publish_plane`.
   No mesh is required.
6. **Open/new behavior follows Fill.** `AppState.vector_states` resets with the
   document. On project open, saved planes seed CLEAN; a planeless vector seeds
   DIRTY and evaluates without waiting for a mesh. Vectors have no legacy
   pre-v2/Base dormancy case. `Passthrough` composites as `Normal`, matching Fill.
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
- Non-finite/degenerate geometry contributes no coverage and never panics.
  Determinism is required at identical inputs, including non-square rasters.

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

- `closed_shape_matches_svg_coverage_exactly`: an opaque closed cubic shape's
  coverage equals the equivalent SVG raster byte-for-byte.
- `open_shape_stroke_has_round_caps_and_no_fill`: exact center/exterior/cap
  coverage pins and zero fill without a stroke.
- `stroke_composites_source_over_fill`: exact RGBA/coverage values for an
  alpha-red fill under an alpha-blue stroke, including zero-alpha behavior.
- `invalid_and_empty_geometry_is_transparent`: empty, one-anchor, NaN,
  zero-size, and degenerate inputs never panic and produce transparent output.
- `raster_is_deterministic_and_non_square`: repeated inputs are byte-identical;
  non-square UV mapping and shorter-edge stroke width are pinned.
- `dimension_and_structure_budgets_are_typed_errors`: 8K-inclusive cap,
  over-cap dimensions, and over-limit anchors return their exact variants.

App tests in `fill_layer.rs`:

- `vector_state_evaluates_caches_and_invalidates`: first evaluation is exact,
  an unchanged call reuses the cache, and shape/symmetry/size changes re-evaluate.
- `vector_refresh_publishes_plane_without_mesh`: a real `PaintState` refresh
  publishes `(Layer(id), tile, baseColor)` with exact premultiplied texels and
  no mesh; quick-mask gating is pinned on the same plane.
- `vector_states_open_clean_or_planeless_dirty`: open seeding does not re-run
  saved planes, while planeless vectors queue exactly once.

Battery: rustfmt on touched `.rs`; `check-layers.py`; `scorecard.py --check`;
clippy `-p umber-core -p umber-paint -p umber-app --all-targets -- -D warnings`;
tests for those three crates under `flock /tmp/umber-cargo.lock`,
`CARGO_BUILD_JOBS=3`, `CARGO_PROFILE_TEST_DEBUG=0`, `nice -n 19`, and
`UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`. A pre-existing failure must be
reproduced at clean HEAD before it can be named environmental. The known
`umber-cli` `library_list_step_replies_rows_over_the_real_dirs` environment
failure is outside this kill's touched crates.

## Exact lock surface

- `crates/umber-core/src/vector.rs`
- `crates/umber-core/src/svg.rs`
- `crates/umber-core/src/layers.rs` (K1 comment sync only)
- `crates/umber-app/src/fill_layer.rs`
- `crates/umber-app/src/main.rs`
- `crates/umber-paint/src/paint_state.rs`
- `crates/umber-paint/src/snapshot.rs` (K1 comment sync only)
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
