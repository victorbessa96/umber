# Design brief — wave-svg-import: SVG vector assets as stencils (row 265)

- **Status:** design only. No code in this wave. The kill after this brief
  is the parser+rasterizer crate; the one after is the loader wiring.
- **The gap:** requirements row 112 holds "SVG (later)"; ROADMAP's
  `wave-svg-import` row is open. Stencils load from PNG only
  (`crates/umber-app/src/stencil.rs`: 8-bit RGB/RGBA, 4096² pixel cap,
  RGB loads fully opaque). SVG is the one vector format artists actually
  hand over (logos, decals, UI glyphs), and rasterizing it at import gives
  the exact same artifact a PNG stencil is.

## Decision: rasterize at import, never store vector state

Umber's stencil is an 8-bit coverage plane (`umber_core::stencil::StencilImage`).
Keeping SVG as live vector state (editable paths on the canvas) is the
path-tool wave's problem, not the stencil's. Decision: an `.svg` import
rasterizes to a coverage plane at import time and becomes byte-identical
in behavior to a PNG stencil of the same pixels. Consequence: no SVG
bytes persist in the project; the stencil sidecar carries the plane.

## Parser scope (clean-room, no new dependency)

No SVG crate exists in the workspace and the project rule is clean-room
original code (same posture as the .spp reader). New module
`umber_core::svg` (core, not a new crate: stencil already lives there
and the layering gate stays untouched):

- XML subset: elements `svg`, `g`, `path`, `rect`, `circle`, `ellipse`,
  `polygon`, `polyline`, `line`; attributes parsed with a small
  hand-rolled tokenizer (entities `&amp; &lt; &gt; &quot; &apos;` + numeric
  refs). No CSS, no `<style>`, no `use`/`defs`/`symbol`, no `text`, no
  `image`, no gradients (flat `fill` only; `fill="none"` skips).
- `path` data: `M L m l H h V v C c S s Q q T t A a Z z` — absolute and
  relative, repeated implicit commands, scientific notation. Arcs (`A`)
  are flattened to line segments at import (endpoint parameterization →
  center parameterization → segment fan, the standard conversion).
- Transforms: `transform` attribute with `matrix`, `translate`, `scale`,
  `rotate` (about a point), `skewX/Y`, composed left-to-right; `viewBox`
  maps to the raster size.
- Fill rule: `fill-rule` nonzero (default) and evenodd, scanline
  rasterizer with supersampling 4x4 for edge quality, output u8 coverage
  = round(alpha * 255).

## Raster size and caps

- Default raster: the SVG's `width`/`height` (px units) clamped to the
  existing `DEFAULT_MAX_STENCIL_PIXELS` (4096²) cap; no `width`/`height`
  → the `viewBox` size; neither → refuse with a named error. A
  `--size N` override on import for the times the vector should outsize
  its nominal box.
- Memory discipline mirrors the PNG path: the cap runs BEFORE any
  rasterization (declared size), and the supersample buffer is the only
  overshoot (16x briefly — 4096² x 16 x f32 is 1 GiB worst case, so the
  cap for SVG rasterization is 2048² unless --size is passed explicitly;
  documented in the module).

## Loader wiring (the follow-up kill, not this one)

- `crates/umber-app/src/stencil.rs`: the load path accepts `.svg` next to
  `.png` — same activation/reset semantics, same cap, `StencilLoadError`
  gains an `Svg` variant.
- CLI: `umber-cli import-asset --kind brush` stays PNG; a separate
  `umber-cli stencil <file.svg> [--size N] <out.png>` headless converter
  (so agents can pre-rasterize without the app) — or the stencil lane
  accepts .svg directly in the app only. Open call, default: app loader
  accepts both, CLI gains the converter.

## Pins the implementation wave must land

- A hand-written SVG (rect + circle + one cubic path) rasterizes to a
  coverage plane with exact expected bytes at known texels (corners
  inside/outside).
- evenodd vs nonzero differ on a self-intersecting star path.
- Arc command equals its manual segment fan within 1/255 coverage.
- Over-cap SVG refuses before allocating (cap check precedes raster).
- viewBox-only SVG maps correctly to the raster size.
- Round trip: rasterized coverage feeds `StencilImage::from_alpha`
  unchanged and the gate path is identical to PNG.
