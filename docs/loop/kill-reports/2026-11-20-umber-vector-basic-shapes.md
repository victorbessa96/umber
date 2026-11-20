# Kill report: umber-vector-basic-shapes (K7e, 2026-11-20)

- Kill lock: `razul-vector-basic-shapes`.
- Brief: `8c448f8d` (`docs(vector): K7e basic shape import brief`).
- Implementation: `64dad2b1` (`feat(vector): K7e straight-line basic shape import`).
- Prior pushed truth: K7d `eec047d0`.

## Delivered

The shared leaf dispatcher now imports exact straight-line basic shapes through
the existing anchor sink:
- `<rect>` emits four corner anchors and an implicit closing segment. Zero or
  negative width/height emits no layer. Negative or zero corner radii follow
  the raster builder's square-rect behavior.
- `<polygon>` emits ordered anchors and closes implicitly; `<polyline>` emits
  the same geometry without closing.
- `<line>` validates x1/y1/x2/y2 with defaults and emits one open two-anchor
  segment, so vector stroke paint works normally.

Rounded rects, circles, and ellipses remain typed refusals pending
arc-to-cubic conversion. Every shape reuses scoped paint, effective CTM,
document order, 256-shape and 65,536-total-anchor budgets, strict one-shape
compatibility, and K7a's one-history-group multi-layer installation. Shape-
specific style/effect/miterlimit errors name the actual element.

## Review beam

`opencode/beam-free` read-only review returned SHIP with no HIGH/MEDIUM
findings and no test-honesty defects. It checked exact geometry and closure,
radius/default behavior, transforms and paint, budgets, failure atomicity,
compatibility, and the grouped undo assertions.

Log: `vector-basic-shapes-review-beam.log` under Hermes scratch.

## Verification

- Focused SVG suite: 26 passed / 0 failed.
- Vector panel tests: 16 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-basic-shapes-final-battery-r1.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2196 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- Fresh ad-hoc verification, not a separate suite claim: 8/8 checks passed for
  committed-byte identity, exact geometry, 257-rect budget refusal, and
  grouped undo. The temporary probe deleted itself.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: exact rect/polygon/polyline/line geometry, closure semantics,
zero-geometry omission, rounded/curved/unit refusals, basic-shape budgets,
single-shape compatibility, grouped undo/redo, and full battery.
External verification still owed: windowed basic-shape import smoke and
release-ci.sh before the next public tag.
