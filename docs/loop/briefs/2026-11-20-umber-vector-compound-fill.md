# Design brief: umber-vector-compound-fill (K8a, 2026-11-20)

Kill lock: `razul-vector-compound-fill`.
Base: K7f implementation/report `e6d7a5cd` plus its independent review handoff
`7b8326fc`.

## What exists

`VectorShape` stores one primary contour (`closed + anchors`) with shared fill,
fill rule, and stroke. K7b splits stroke-only subpaths into separate layers and
refuses filled compounds so holes cannot be corrupted. K7f adds editable arc
cubics. `rasterize` flattens only the primary contour; cache identity and SVG
export likewise see only that contour.

Independent design review: `opencode/beam-free` returned DESIGN-REVISE on the
initial scout. This brief incorporates every disposition, including always
combining contours per path element and aggregate per-shape budgets. The
independent coverage oracle passes `COMPOUND_FILL_ORACLE_PASS 8/8`.

## Target

K8a adds additive compound geometry and one-layer-per-SVG-path semantics:
- Add `VectorSubpath { closed, anchors }` and additive
  `VectorShape.subpaths: Vec<VectorSubpath>` with serde defaults and
  skip-if-empty. Existing `closed + anchors` remains the primary contour.
  Old JSON stays byte-compatible and needs no project-version bump.
- Rasterize primary + secondary contours independently. Gather every closed
  centerline into one `rasterize_polys_rule` call so nonzero/evenodd holes work
  across contours. Build stroke geometry per contour with that contour's own
  closure joins/caps and a fresh dash phase.
- `is_empty` is true only when no contour has at least two anchors.
  `same_cache_key` compares every contour with NaN-stable anchor equality.
- Charge total anchors and total centerline points per compound shape, plus
  aggregate edge-row visits. Primary-empty shapes with valid secondary contours
  render normally. Contour count remains bounded by `MAX_VECTOR_SHAPES`.
- SVG export emits every contour in one `<path>` `d` with shared paint/rule/
  stroke. Import always combines contours per path element, filled or stroked.
  Existing K7b-created projects remain valid; only new import grouping changes.
- Retain `MAX_VECTOR_SHAPES` as a contour-count resource budget even when
  contours share a layer. Builder anchor charging tracks primary + secondary
  contours before mutation.
- Existing scoped paint, CTM, document order, strict one-shape compatibility,
  and grouped panel undo/redo remain intact. Canvas editing continues to target
  the primary contour only; secondary geometry is preserved by SetVector.

## Concurrency and safety

All changes stay inside the registered lock: core `vector.rs`, `svg.rs`,
`vector_svg.rs`, `journal.rs`, `project.rs`, `layers.rs`; app `vector_panel.rs`,
`fill_layer.rs`, `vector_edit.rs`; this brief/report, queue/STATE/ROADMAP, and
the coordination note. The additional five files contain exhaustive
`VectorShape` literals or cache/edit tests that must explicitly cover additive
compound behavior. No schema version bump, new dependency, release action,
public PR, or work outside the kill.

## Verification

Tests cover donut holes under both rules and both windings, mixed open/closed
contours, primary-empty secondary rendering, secondary closure joins/caps and
dash reset, aggregate anchor and centerline boundaries, NaN-stable cache
identity, deterministic old/new JSON, compound SVG byte stability, strict
single-shape behavior, grouped undo/redo, and full core/app battery. A bounded
independent review must disposition every finding before landing.

## Non-goals

No secondary-contour canvas editing, compound-specific panel affordances,
boolean path operations, CSS/effects, release work, or public PR.
