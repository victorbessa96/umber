# Design brief: umber-vector-fill-rule (K6a, 2026-11-20)

Kill lock: `razul-vector-fill-rule`. Base: Fill cache-lifecycle `8747c681`,
pushed truth `4abfb021`, native smoke record `24acff92`.

## What exists

`VectorShape` has one closed/open path, a fill, and an optional round stroke.
K2 rasterizes closed paths only under nonzero. The shared scanline in
`crates/umber-core/src/svg.rs` already supports `FillRule::{NonZero, EvenOdd}`
internally, but `rasterize_polys` hardcodes nonzero. SVG export always writes
`fill-rule="nonzero"`, and import refuses evenodd. The Vector panel has no
fill-rule row.

## Target

K6a adds an additive `fill_rule` to `VectorShape` with `NonZero` as the default,
so old projects and old code paths keep the same pixels. It includes:
- a crate-private `rasterize_polys_rule` wrapper, preserving the existing
  `rasterize_polys` nonzero API;
- `VectorShape::same_cache_key` including fill rule;
- a Vector Properties combo (`Nonzero` / `Even-odd`) through one undoable
  `SetVector` edit and both i18n catalogs;
- SVG export/import roundtrip of both rules, with other values still refused
  as typed `SvgError::Unsupported`;
- a self-intersecting star test proving even-odd changes only the expected
  winding region, plus roundtrip and cache-identity tests.

## Non-goals

No cap/join/dash selectors, multi-shape layers, project version bump, or
release work. Those remain later K6 stages. No changes to Fill or Material
formats.

## Verification

Focused core vector and vector_svg suites, app vector_panel tests, i18n
catalog completeness, full five-crate clippy/test battery, bounded independent
review, then report/queue/STATE/ROADMAP/coordination/vault and safe-push.
