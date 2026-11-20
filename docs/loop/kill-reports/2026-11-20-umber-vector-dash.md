# Kill report: umber-vector-dash (K6d, 2026-11-20)

- Kill lock: `razul-vector-dash`.
- Brief: `7da1997f` (`docs(vector): K6d dash pattern brief`).
- Implementation: `58f9e160` (`feat(vector): K6d dash patterns`).
- Prior pushed truth: K6c `0df1505e`.

## Delivered

`StrokeDash` adds bounded SVG-style dash patterns with at most 8 UV lengths
and a finite offset. Empty patterns are solid, odd patterns repeat for SVG
on/off alternation, and old projects default to solid. `Stroke` remains Copy
and cache identity includes the complete dash state.

Dashed strokes segment the exact flattened centerline with UV-to-device
scaling, bounded dash steps/segments, and typed invalid-pattern and complexity
errors. Butt, Round, and Square caps appear at actual dash breaks. Joins only
appear where the dash is continuous across a real turn. Closed paths support
dash breaks at the closure without inventing endpoint caps. Solid strokes
keep the K6c path unchanged.

SVG exports and imports `stroke-dasharray` and `stroke-dashoffset`, including
unitless/px scaling, odd patterns, `none`, and typed invalid or over-cap
refusals. Root dash inheritance remains refused. Vector Properties adds a
persistent dash-pattern editor, offset field, en/pt labels, and searchable
rows.

Zero-length entries preserve on/off parity. Contiguous active intervals are
merged across zero off-lengths so they cannot create false dash caps.

## Review beam

`opencode/beam-free` read-only review:
- R1 SHIP, no HIGH/MEDIUM findings; noted zero-length entries needed direct
  coverage.
- Zero-gap inspection found and fixed a real false-cap edge case by merging
  contiguous active intervals and adding exact zero-on/zero-off tests.
- R2 SHIP, no HIGH/MEDIUM findings; its suggested direct cap-count assertion
  for merged intervals was added and passed.

Logs: `vector-dash-review-beam.log` and `vector-dash-review-beam-r2.log`
under Hermes scratch.

## Verification

- Focused core vector suite: 25 passed / 0 failed.
- Focused SVG suite: 20 passed / 0 failed.
- Vector panel tests: 11 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-dash-final-battery.log`.
- Full six-crate clippy --all-targets -D warnings: 0 errors.
- Full battery: 21 suites, 2185 passed / 0 failed / 0 ignored.
- `cargo fmt --all -- --check`, check-layers.py, scorecard.py --check,
  `git diff --check`: all clean.
- No GPU/golden claims, release tag, version bump, .sbsar execution,
  telemetry, or public PR.

## Landed vs external verification

Landed: dash model, bounded raster segmentation, cap/join behavior, closure
continuity, SVG support, panel controls, compatibility/cache tests, and full
battery.
External verification still owed: windowed dash editing and mixed dash/cap/
join smoke, plus release-ci.sh before the next public tag.
