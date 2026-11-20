# Design brief: umber-vector-arcs (K7f, 2026-11-20)

Kill lock: `razul-vector-arcs`. Base: K7e report `b72b9a1c`, with straight-line
basic-shape implementation `64dad2b1` and brief `8c448f8d`.

## What exists

The strict SVG path walker already parses absolute and relative `A/a` commands,
including compact flags, and dispatches them through `PathSink::sink_arc_to`.
`PolyBuilder::arc_to` performs the SVG endpoint-to-center conversion for raster
flattening, but `VectorImportBuilder` refuses arcs. The shared leaf dispatcher
imports square rects, polygons, polylines, and lines; rounded rects, circles,
and ellipses remain typed refusals. Existing import code already supplies scoped
paint, the effective root/group/leaf CTM, document-order layers, per-shape and
65,536-total-anchor budgets, strict one-shape compatibility, subpath splitting,
and one grouped panel undo/redo operation.

## Target

K7f preserves curved geometry as editable cubic anchors rather than polylines:

- Extract the SVG 1.1 implementation-notes endpoint-to-center calculation into
  shared geometry used by the raster builder and vector import. Negative radii
  become absolute values; zero radii become a line; identical endpoints emit no
  segment; insufficient radii grow uniformly; large-arc/sweep select the center
  and signed sweep; x-axis rotation is honored.
- Convert the center-parameterized arc to bounded cubic Bézier segments, split
  at no more than 90° each, using the standard derivative-derived control points
  and exact final endpoints. Charge every generated destination anchor before
  emission and refuse non-finite results without panicking.
- Rounded rects follow SVG rx/ry inheritance, negative-radius auto behavior,
  half-width/half-height clamping, and square behavior when either effective
  radius is zero. Their straight edges and quarter-circle corners use the same
  `VectorImportBuilder` sink, omitting zero-length straight edges.
- Circles and ellipses with positive radii become four quarter-arc cubics in a
  closed shape. Zero or negative radii produce no layer. Closure folds the
  duplicate final anchor while retaining the closing segment's incoming handle.
- All curved forms reuse the established paint, CTM-to-UV mapping, order,
  subpath, budget, strict single-shape, typed-failure, and grouped panel undo
  behavior. No schema, serialization format, or project version changes.

## Concurrency and safety

All writes stay inside the registered lock: `svg.rs`, `vector_svg.rs`, optional
`vector_panel.rs` grouped-import tests, this brief/report, queue/STATE/ROADMAP,
and the coordination note. Parse and geometry construction finish before panel
mutation. Structural budgets remain fail-closed; no new dependencies, public
API/schema migration, release action, public PR, or work outside the kill.

## Verification

Tests independently pin known quarter/half/large-arc geometry and derivatives,
absolute/relative commands, every flag combination, rotation, radius correction,
negative/zero radii, identical endpoints, malformed/non-finite/over-budget
refusal, rounded-rect rx/ry and clamping rules, circle/ellipse closure, CTM and
paint inheritance, exact anchor/shape boundaries, export→import→export
roundtrip, strict single-shape behavior, subpaths, and grouped undo/redo. The
real battery is rustfmt, check-layers, scorecard, crate-scoped clippy with
`-D warnings`, and full core/app test suites under `/tmp/umber-cargo.lock` with
`UMBER_ASSETS_DIR=/home/bessa/Projects/umber`; zero-test output is not a pass.
A bounded independent read-only CLI review must disposition every finding and
record SHIP/block before the report and landing commits.

## Non-goals

No filled compound-subpath model, CSS, effects, unit expansion, raster-only
fallback, arc-preserving SVG export syntax, exact single-cubic representation of
arcs over 90°, UI redesign, version bump, release/tag, or public PR.
