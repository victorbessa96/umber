# Design brief: umber-vector-basic-shapes (K7e, 2026-11-20)

Kill lock: `razul-vector-basic-shapes`. Base: K7d affine transforms `65957542`,
pushed truth `eec047d0`.

## What exists

Vector import handles paths, scoped paint, and nested affine transforms. The
raster SVG builder also handles rect, polygon, polyline, and line geometry.
Vector import currently refuses every non-path shape even though straight-line
forms map exactly onto the existing anchor sink.

## Target

K7e imports exact straight-line basic shapes:
- `<rect>` becomes four corner anchors and an implicit closing segment. Zero or
  negative width/height produces no layer. Negative or zero corner radii follow
  the raster builder's square-rect behavior; genuinely rounded rects remain a
  typed refusal until arc-to-cubic conversion exists.
- `<polygon>` parses the shared points list, emits its anchors in order, and
  closes implicitly. `<polyline>` emits the same geometry without closing.
- `<line>` validates x1/y1/x2/y2 with defaults and emits one open two-anchor
  segment, allowing vector stroke paint to work normally.
- Circle and ellipse remain typed refusals rather than flattened or approximated
  curves. Empty basic geometry follows the existing no-layer behavior.
- Every shape uses the same scoped paint, effective CTM, shape/anchor budgets,
  document order, strict single-shape API, and grouped panel undo/redo as paths.

## Concurrency and safety

All changes stay inside the registered lock. No vector schema, rasterizer,
serialization, or project-version changes. Invalid lengths, rounded/curved
geometry, CSS style, effects, and non-default miter limits fail before panel
mutation.

## Verification

Focused tests cover exact rect/polygon/polyline/line anchors and closure,
transforms and paint inheritance, zero geometry, rounded/curved refusals,
basic-shape budgets, strict single-shape compatibility, and grouped panel
undo/redo. Then full six-crate clippy/test battery, bounded independent review,
kill report, queue/STATE/ROADMAP/coordination/vault bookkeeping, safe-push,
and origin verification.

## Non-goals

No arcs, circles, ellipses, rounded rects, CSS style, group opacity, compound
filled subpaths, release work, or public PR.
