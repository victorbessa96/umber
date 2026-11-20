# Design brief: umber-vector-stroke-cap (K6b, 2026-11-20)

Kill lock: `razul-vector-stroke-cap`. Base: K6a even-odd fill `7372aa53`,
pushed truth `40f41c17`.

## What exists

`Stroke` carries width and color only. `stroke_polys` emits one quad per
segment and one ellipse at every centerline point, producing round caps and
round joins. SVG export always writes `stroke-linecap="round"`, import accepts
only round. The Vector panel has no cap selector.

## Target

K6b adds an additive `StrokeCap` enum with `Round` as default, plus `Butt` and
`Square`. It includes:
- `Stroke { cap }` with serde default and cache identity;
- raster changes limited to open endpoints: Butt adds no endpoint ellipse,
  Square adds a radius-length square cap, and Round keeps current bytes;
- a Vector Properties combo through one undoable `SetVector` edit and both
  i18n catalogs;
- SVG export/import roundtrip of butt/round/square, with other values typed
  `SvgError::Unsupported`;
- endpoint tests for each cap and cache/JSON compatibility.

## Non-goals

No join selectors, dashes, multi-shape, project version bump, or release work.

## Verification

Focused core vector and vector_svg suites, app vector_panel tests, i18n
catalog completeness, full five-crate clippy/test battery, bounded independent
review, then report/queue/STATE/ROADMAP/coordination/vault and safe-push.
