# Kill report: umber-vector-stroke-cap (K6b, 2026-11-20)

Authorship: dragon implementation and verification, with bounded read-only reviews by `opencode/beam-free`. Kill lock: `razul-vector-stroke-cap`.

Base: K6a even-odd fill `7372aa53`, pushed truth `40f41c17`. Brief: `fc022678`. Code landing: `1839de83`.

Implements `docs/loop/briefs/2026-11-20-umber-vector-stroke-cap.md`: additive stroke cap options with SVG roundtrip.

## Built

- `StrokeCap` has `Butt`, `Round`, and `Square`, with Round as serde default and cache identity. Old projects load as Round.
- `join_at` limits non-round-cap ellipses to actual turns, so collinear flattening points cannot create accidental caps. Round behavior is unchanged.
- Square caps extend by radius at open endpoints. Closed paths force Round join behavior, including the closure corner.
- Vector Properties has a cap combo through one undoable `SetVector` edit and both i18n catalogs.
- SVG export/import roundtrips all three caps; invalid caps are typed Unsupported when a stroke is present.
- Endpoint tests cover Butt/Square and closed invariance, cache/JSON compatibility, and cap roundtrips. The closure test also asserts corner coverage directly.

## Verification

- Focused core vector suite: **21 passed / 0 failed**.
- Focused vector_svg suite: **18 passed / 0 failed**.
- Focused app vector_panel suite: **10 passed / 0 failed**.
- Full five-crate battery: **clippy 0 errors; 21 suites, 2,178 passed / 0 failed**. Receipt: `~/.hermes/cache/scratch/vector-stroke-cap-final-battery-r3.log`.
- Round 1 review: FIX-FIRST for closure-corner cap variance. Fixed and re-reviewed: **SHIP**. Receipts: `~/.hermes/cache/scratch/vector-stroke-cap-review-beam.log` and `vector-stroke-cap-review-r2-beam.log`.

## Deferred

- Join selectors remain K6c; dashes, multi-shape, and release work remain out of scope for this kill.
