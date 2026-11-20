# Kill report: umber-vector-multipath (K7a, 2026-11-20)

- Kill lock: `razul-vector-multipath`.
- Brief: `925c525b` (`docs(vector): K7a multi-path import brief`).
- Implementation: `29a02f9a` (`feat(vector): K7a multi-path SVG import`).
- Prior pushed truth: K6d `417b62bc`.

## Delivered

`import_svg_many` returns one `VectorParams` per top-level path in document
order, preserving each path's fill, stroke, cap, join, and dash state. The
existing `import_svg` API remains a strict single-path compatibility wrapper.
The one-shape-per-layer raster model is unchanged.

Imports are bounded at 256 paths and 65,536 anchors across the whole document;
excess returns typed `SvgError::TooComplex`. Exact acceptance at both limits
is pinned alongside over-limit refusals. Groups, non-path shapes, path
transforms/style, unsupported effects, and multiple subpaths inside one `d`
remain refused.

The panel parses before mutation and installs every returned shape as its own
vector layer in one undoable history group. Undo removes the whole import and
redo restores it. The shell selects the last imported layer and shows singular
or plural localized notices.

## Review beam

`opencode/beam-free` read-only review returned SHIP with no HIGH/MEDIUM
findings and no new defects. Its test-honesty note about exact boundary
acceptance was addressed by pinning 256 paths and exactly 65,536 total anchors.

Log: `vector-multipath-review-beam.log` under Hermes scratch.

## Verification

- Focused SVG suite: 22 passed / 0 failed.
- Vector panel tests: 12 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-multipath-final-battery.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2188 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: ordered multi-path parser, strict single-path compatibility, total
shape/anchor budgets, grouped multi-layer undo/redo, selection, localized
notices, and full battery.
External verification still owed: windowed multi-path import smoke and
release-ci.sh before the next public tag.
