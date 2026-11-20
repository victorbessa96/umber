# Design brief: umber-vector-dash (K6d, 2026-11-20)

Kill lock: `razul-vector-dash`. Base: K6c stroke joins `68cbf388`, pushed
truth `0df1505e`.

## What exists

`Stroke` carries width, sRGBA color, cap, and join. Solid strokes rasterize
through the existing K6c quad/join path. SVG import currently refuses dash
attributes, and the Vector panel has no dash controls.

## Target

K6d adds bounded, additive SVG-style dash patterns while keeping `Stroke`
copyable and old projects compatible:
- `StrokeDash` with at most 8 UV lengths plus finite offset; empty means solid.
- Odd-length patterns repeat to preserve SVG on/off alternation.
- Deterministic segmentation along the exact flattened centerline with UV to
  device scaling, bounded dash steps/segments, and typed invalid-pattern and
  complexity errors.
- Butt, Round, and Square caps at actual dash breaks; joins only where the
  dash is continuous across a real turn; closed paths support dash breaks at
  the closure without inventing endpoint caps.
- Cache identity and JSON compatibility; old stroke JSON defaults to solid.
- SVG `stroke-dasharray` / `stroke-dashoffset` export and bounded import,
  including unitless/px scaling, odd patterns, `none`, and typed invalid or
  over-cap refusals.
- Vector Properties dash-pattern text field with persistent edit state,
  offset field, en/pt labels, and searchable rows.

## Non-goals

No arbitrary-length dash arrays, multi-shape input, project version bump, or
release work.

## Verification

Focused core vector and vector_svg suites, app vector_panel tests, i18n
catalog completeness, full six-crate clippy/test battery, bounded independent
review, then report/queue/STATE/ROADMAP/coordination/vault and safe-push.
