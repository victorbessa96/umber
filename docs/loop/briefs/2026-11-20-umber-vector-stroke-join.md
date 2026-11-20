# Design brief: umber-vector-stroke-join (K6c, 2026-11-20)

Kill lock: `razul-vector-stroke-join`. Base: K6b stroke cap options `1839de83`,
pushed truth `e28f54e8`.

## What exists

`Stroke` carries width, color, and cap. `stroke_polys` emits one quad per
segment and round ellipses at every point for Round joins, or only at actual
turns for other caps. SVG export always writes `stroke-linejoin="round"`, and
import accepts only round. The Vector panel has no join selector.

## Target

K6c adds an additive `StrokeJoin` enum with `Round` as default, plus `Miter`
and `Bevel`. It includes:
- `Stroke { join }` with serde default and cache identity;
- join polygons at actual turns: Bevel triangles, Miter quadrilaterals with a
  bounded miter-limit fallback to bevel;
- closure handling for closed paths so the first/last join is not lost;
- a Vector Properties combo through one undoable `SetVector` edit and both
  i18n catalogs;
- SVG export/import roundtrip of all three joins, with invalid values typed
  `SvgError::Unsupported`;
- direct join-geometry tests and cache/JSON compatibility.

## Non-goals

No dashes, multi-shape, project version bump, or release work.

## Verification

Focused core vector and vector_svg suites, app vector_panel tests, i18n
catalog completeness, full five-crate clippy/test battery, bounded independent
review, then report/queue/STATE/ROADMAP/coordination/vault and safe-push.
