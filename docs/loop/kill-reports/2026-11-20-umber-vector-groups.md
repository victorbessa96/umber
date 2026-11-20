# Kill report: umber-vector-groups (K7c, 2026-11-20)

- Kill lock: `razul-vector-groups`.
- Brief: `793fed3d` (`docs(vector): K7c group inheritance brief`).
- Implementation: `f04ed236` (`feat(vector): K7c root and group paint inheritance`).
- Prior pushed truth: K7b `e0d8aca9`.

## Delivered

Root and nested `<g>` frames now inherit `fill`, `fill-opacity`, `fill-rule`,
`stroke`, `stroke-opacity`, `stroke-width`, `stroke-linecap`,
`stroke-linejoin`, `stroke-dasharray`, and `stroke-dashoffset`. Child values
override by property; empty and `inherit` values preserve the parent. Scoped
frames preserve document order and isolate sibling groups.

Paths consume their effective frame through the existing `parse_vector_paint`,
preserving geometry, paint, dash, subpath behavior, the 256-shape and
65,536-total-anchor budgets, strict single-shape compatibility, and K7a's
one-history-group multi-layer installation. Unknown elements retain the
skipped-subtree policy.

Root transforms remain supported; group transforms, CSS `style`, group/root/
path opacity, visibility, clip/mask/filter/markers/vector-effect/paint-order
remain typed refusals. Non-default `stroke-miterlimit` is now refused at every
level; explicit default `4` and `inherit` are accepted because the rasterizer
uses the fixed SVG-default 4× limit.

## Review beam

`opencode/beam-free` read-only review returned SHIP with no HIGH/MEDIUM
findings and no test-honesty defects. It checked scoped inheritance,
empty/inherit behavior, sibling isolation, typed refusals, miterlimit handling,
budgets, failure atomicity, and the preservation of prior refusal coverage.

Log: `vector-groups-review-beam.log` under Hermes scratch.

## Verification

- Focused SVG suite: 25 passed / 0 failed.
- Vector panel tests: 14 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-groups-final-battery-r1.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2193 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: root/nested-group paint inheritance, override and sibling scoping,
unknown-subtree skipping, non-default miterlimit refusal, grouped budgets,
failure-atomic panel import, grouped undo/redo, and full battery.
External verification still owed: windowed grouped-SVG import smoke and
release-ci.sh before the next public tag.
