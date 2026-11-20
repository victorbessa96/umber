# Kill report: umber-vector-subpaths (K7b, 2026-11-20)

- Kill lock: `razul-vector-subpaths`.
- Brief: `e76cd23e` (`docs(vector): K7b subpath import brief`).
- Implementation: `3d087302` (`feat(vector): K7b stroke-only subpath import`).
- Prior pushed truth: K7a `07010149`.

## Delivered

`VectorImportBuilder` now collects moveto-delimited subpaths in document order,
including a segment after closepath. `import_svg_many` splits stroke-only
compound paths into one vector shape per subpath, preserving the same stroke,
cap, join, and dash state with transparent fill.

Filled compound subpaths remain a typed refusal because splitting them would
change nonzero/evenodd hole semantics. The 256-shape and 65,536-total-anchor
budgets include subpaths, not only top-level path elements. Existing
`import_svg` remains the strict single-shape API. Panel import creates every
split layer in one undo group through K7a.

## Review beam

`opencode/beam-free` read-only review returned SHIP with no HIGH/MEDIUM
findings. It verified subpath collection, closepath splitting, ordering,
fill-refusal, stroke/dash sharing, budgets, failure atomicity, undo grouping,
and strict single-path compatibility. A direct `Z` followed by segment case
was added after review and passed.

Log: `vector-subpaths-review-beam.log` under Hermes scratch.

## Verification

- Focused SVG suite: 23 passed / 0 failed.
- Vector panel tests: 13 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-subpaths-final-battery.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2190 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: ordered stroke-only subpath splitting, closepath handling, compound
fill refusal, subpath-aware budgets, grouped multi-layer undo, and full
battery.
External verification still owed: windowed stroke-only subpath import smoke
and release-ci.sh before the next public tag.
