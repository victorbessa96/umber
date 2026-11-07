# Kill brief — svg-core: clean-room SVG subset parser + rasterizer (row 266)

## Seat and surface

- You are the authoring claw (claude) for ONE kill: `claude-svg-core`.
- Files you may touch: `crates/umber-core/src/svg.rs` (NEW file) and the EXISTING
  uncommitted hunk in `crates/umber-core/src/lib.rs` (`pub mod svg;` at line 35 —
  keep it, do not revert it, do not add other mods).
- Kill-lock `claude-svg-core` already holds exactly those two files. Do NOT call
  `scripts/kill-locks.sh`; the orchestrator owns the lock registry.
- Do NOT touch any other file: no Cargo.toml/Cargo.lock (zero new dependencies;
  the module is std + thiserror, which umber-core already has), no
  queue/STATE/CHANGELOG/requirements/ROADMAP, no `.ftl` catalogs, no app/cli code
  (a later wave wires the loader).
- Do NOT commit, push, stash, checkout, or reset. Do NOT run cargo/clippy/tests
  if bash is denied to you; the orchestrator runs the full battery after your exit.

## Read first

1. `AGENTS.md` and `CLAUDE.md` at the repo root (standing rules; auto-loaded).
2. `docs/loop/briefs/2026-11-07-wave-svg-import-design.md` — the DESIGN brief;
   it decides scope. Where this kill brief and the design brief disagree on
   scope, the design brief wins; where they disagree on API names, this brief wins.
3. `crates/umber-core/src/stencil.rs` (`StencilImage`, `StencilError` style,
   `from_alpha` at :138, `ALPHA_CUT` at :57) and the `crates/umber-core/src/lib.rs`
   header — the never-crash policy: non-test code denies
   `clippy::unwrap_used`/`expect_used`/`panic`. No unwrap/expect/panic outside
   `#[cfg(test)]`.

## What to build

Module `umber_core::svg`: clean-room SVG-subset parser + scanline rasterizer
producing an 8-bit coverage plane. Decision (design brief): rasterize at import;
no vector state persists.

Public API (exact names — the follow-up loader wave is written against these):

- `pub const DEFAULT_SVG_RASTER_PIXELS: usize = 2048 * 2048;` — documented:
  2048^2 default cap because the 4x4 f32 supersample buffer is 16x the output
  (1 GiB at 4096^2).
- `pub struct SvgRaster` with private fields `width: u32, height: u32,
  coverage: Vec<u8>` plus `width()`, `height()`, `coverage()`, and
  `to_rgba(&self) -> Vec<u8>` (r=g=b=255, a=coverage).
- `pub enum SvgError` (thiserror, named-field variants with `#[error(...)]`,
  matching `StencilError` style): at least `Parse { offset: usize, message: String }`,
  `NoSize`, `ZeroSize { width: u32, height: u32 }`,
  `TooLarge { width: u32, height: u32, max_pixels: usize }`,
  `Unsupported { what: String }`.
- `pub fn rasterize_svg(bytes: &[u8], max_pixels: usize) -> Result<SvgRaster, SvgError>`
- `pub fn rasterize_svg_default(bytes: &[u8]) -> Result<SvgRaster, SvgError>`
  = `rasterize_svg(bytes, DEFAULT_SVG_RASTER_PIXELS)`

Parser scope (clean-room, no crates):

- XML subset tokenizer: elements `svg g path rect circle ellipse polygon polyline
  line`; attributes via a hand-rolled tokenizer; entities
  `&amp; &lt; &gt; &quot; &apos;` plus numeric `&#NN;` / `&#xHH;`.
  No CSS, no `<style>`, no `use`/`defs`/`symbol`, no `text`, no `image`,
  no gradients. Unknown elements: choose skip-subtree (recommended, forward-compat
  with real-world files carrying metadata) or `Unsupported`; document and test
  the choice.
- Attributes honored: `fill` (flat color or `none`; the plane is coverage so the
  color value itself is ignored — document), `fill-rule` (`nonzero` default,
  `evenodd`), `transform`, `d` (path data), shape attributes for
  `rect(x,y,width,height,rx,ry)`, `circle(cx,cy,r)`, `ellipse(cx,cy,rx,ry)`,
  `polygon`/`polyline(points)`, `line(x1,y1,x2,y2)`. A `g` carries
  fill/fill-rule/transform inherited by children unless overridden. Root `svg`:
  `width`/`height` (unitless or `px`; reject other units as `Unsupported`),
  `viewBox="minx miny w h"`.
- Path data: `M L m l H h V v C c S s Q q T t A a Z z` — absolute/relative,
  implicit command repetition, scientific notation, comma/space separators.
  Arcs `A`: endpoint parameterization converted to center parameterization,
  flattened to a segment fan of line segments.
- Curves (C/S/Q/T) flattened to segments at a documented flatness tolerance.
- Transforms: `matrix(a b c d e f)`, `translate`, `scale`,
  `rotate(angle [cx cy])`, `skewX`, `skewY`, composed left-to-right, inherited
  through `g`.

Raster:

- Output size: root `width`/`height` if present, else viewBox w/h, else
  `NoSize`. A zero dimension → `ZeroSize`. `width * height > max_pixels` →
  `TooLarge` BEFORE any allocation (the size check must textually precede the
  supersample buffer allocation; test 4 pins this).
- `viewBox` maps linearly onto the full raster (stretch: minx,miny maps to
  0,0; viewBox w/h scales to raster w/h). Document that choice (meet/slice not
  implemented).
- Scanline fill per shape under the shape's fill rule; 4x4 supersampling
  (16 subsamples per pixel); coverage byte = round(255 * hits / 16).
  Transforms are applied to geometry first (device-space polygons into the
  rasterizer). `fill="none"` skips the shape; zero-area shapes contribute
  nothing.

Tests (in-module `#[cfg(test)]`; the orchestrator runs `cargo test -p umber-core svg`):

1. Hand-written SVG (one rect + one circle + one cubic path) rasterizes to exact
   expected coverage bytes at known texels (corner texels inside/outside pinned
   exactly).
2. evenodd vs nonzero differ on a self-intersecting star path (pin a texel in
   the overlap region).
3. Arc command vs its manual segment fan agree within 1/255 coverage.
4. Over-cap refuses before allocating: declared 100000x100000 with
   max_pixels = 2048^2 returns `TooLarge`.
5. viewBox-only SVG: raster size == viewBox w/h and a known rect lands at the
   expected texels.
6. Round trip: `SvgRaster::to_rgba()` fed to `StencilImage::from_alpha(w, h, &rgba)`
   yields coverage equal to the raster plane thresholded at `ALPHA_CUT`
   (>= 128 -> 255) — the gate path identical to a PNG stencil.
7. Malformed XML -> `Parse` with offset; unsupported construct -> your documented
   policy; transform composition (translate then rotate about a point) lands a
   rect at the expected texels.

## Style and gates

- rustfmt (edition 2021) clean and clippy `-D warnings` clean by construction:
  no unwrap/expect/panic outside tests, no needless borrows, doc comments
  adjacent to their items (no blank `///` lines), no lazy doc continuations.
  Write the code already formatted; if bash is denied you cannot run rustfmt —
  the orchestrator runs fmt and fixes whitespace-only drift.
- Module doc header: scope, the rasterize-at-import decision, the 2048^2 cap
  rationale, what is NOT parsed (CSS/style/use/defs/symbol/text/image/gradients),
  the unknown-element policy, and the viewBox stretch mapping.

## Handoff

Return the landing notes INLINE in your final result text (do not write report
files; docs are orchestrator-owned): what was built (file line count, public
items), the test list, every choice you made where the brief allowed one,
deliberately-not-done items, and anything unverified because you could not run
cargo.
