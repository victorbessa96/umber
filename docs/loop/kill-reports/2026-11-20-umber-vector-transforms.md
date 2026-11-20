# Kill report: umber-vector-transforms (K7d, 2026-11-20)

- Kill lock: `razul-vector-transforms`.
- Brief: `13772368` (`docs(vector): K7d affine transform brief`).
- Implementation: `65957542` (`feat(vector): K7d group and path affine transforms`).
- Prior pushed truth: K7c `97f33a9e`.

## Delivered

Scoped import frames now carry inherited paint plus an effective CTM. Nested
`<g>` and leaf `<path>` transforms compose through the shared affine parser as
`parent_ctm.compose(local)`, in document order. Root viewBox/root transform is
still applied exactly once, and sibling transforms cannot leak.

Geometry anchors and cubic handles use the effective CTM. Stroke width, dash
lengths, and dash offset use the same CTM's established maximum-scale UV
conversion. Existing paint inheritance, path/subpath order, 256-shape and
65,536-total-anchor budgets, strict single-shape compatibility, and K7a's
one-history-group multi-layer installation remain unchanged.

CSS `style`, group opacity, visibility, effects, filled compound subpaths, and
non-default miter limits retain typed refusal behavior. Unknown elements still
skip their subtrees.

## Review beam

`opencode/beam-free` read-only review returned SHIP with no HIGH/MEDIUM
findings and no test-honesty defects. It checked composition order, root-once
behavior, parent-frame isolation, effective-CTM paint/dash scaling, malformed
transform failure, budgets, atomicity, and the arithmetic-derived assertions.

Log: `vector-transforms-review-beam.log` under Hermes scratch.

## Verification

- Focused SVG suite: 25 passed / 0 failed.
- Vector panel tests: 15 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-transforms-final-battery-r1.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2194 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: nested group/path affine composition, root-once behavior, sibling
isolation, exact anchor/handle mapping, effective-CTM stroke/dash scaling,
transform failure atomicity, grouped undo/redo, and full battery.
External verification still owed: windowed transformed-group import smoke and
release-ci.sh before the next public tag.
