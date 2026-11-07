# Design brief — svg loader wiring: .svg stencils in app + headless CLI converter (row 269)

- **Status:** design only. Depends on row 266 (`claude-svg-core`, in flight)
  landing with its pinned API: `umber_core::svg::{rasterize_svg(bytes, max_pixels),
  rasterize_svg_default(bytes), SvgRaster{width,height,coverage,to_rgba},
  SvgError, DEFAULT_SVG_RASTER_PIXELS (2048^2)}`.
- **Decision carried from row 265:** rasterize at import; no SVG bytes persist
  in the project; an imported .svg is behaviorally a PNG stencil of the same
  pixels.

## Kill A — app stencil loader accepts .svg (umber-app)

Surface: `crates/umber-app/src/stencil.rs` (+ the file-picker filter wherever
the stencil picker sets its extension filter — grep `png` in
`crates/umber-app/src/display_panel.rs` and the picker call site) + both
`.ftl` catalogs if a new reason key is added.

- New `load_image_rgba(path: &Path) -> Result<StencilImage, StencilLoadError>`:
  extension `.svg`/`.SVG` → `std::fs::read` → `rasterize_svg_default(&bytes)`
  (2048^2 cap, documented) → `StencilImage::from_alpha(w, h, &raster.to_rgba())`
  — the pin-6 gate path, identical to PNG (ALPHA_CUT 128 threshold). All other
  extensions → the existing `load_png_rgba` path, unchanged.
- `StencilLoadError` gains `Svg(#[from] umber_core::svg::SvgError)`; `reason_key()`
  maps it to `overlays.stencil-unsupported` (reuse, no new catalog key) unless a
  distinct key is wanted — default reuse; `localized()` arm matches.
- Same activation/reset semantics: wherever the panel calls `load_png_rgba`,
  switch to `load_image_rgba`; the picker filter lists `*.png *.svg`.
- Tests (in stencil.rs's test module): a written .svg loads end-to-end to the
  expected thresholded coverage; an over-cap .svg (declared 100000x100000)
  refuses with `Svg(TooLarge)`; malformed svg refuses with `Svg(Parse)`;
  the png path is byte-unchanged (existing pins keep passing).

## Kill B — headless converter `umber-cli stencil` (umber-cli)

Surface: `crates/umber-cli/src/main.rs` (dispatch table :42, usage :49/:62) +
NEW `crates/umber-cli/src/stencil_convert.rs` + its tests. No Cargo.toml change:
umber-cli already depends on umber-core and umber-export.

- `umber-cli stencil <in.svg> [--size N] <out.png>`:
  `rasterize_svg(&bytes, size)` where size = N if passed else
  `DEFAULT_SVG_RASTER_PIXELS`; `--size` is clamped to a hard ceiling of
  8192^2 with a named refusal beyond it (the 16x f32 supersample buffer is
  1 GiB at 4096^2 and grows quadratically — the ceiling is the guard).
  Output PNG via `umber_export::png::write_png(out, w, h, &raster.to_rgba())`;
  stdout prints a one-line JSON summary `{width, height, out}`; exit 0 ok,
  exit 2 named refusal (SvgError Display), matching import-asset's contract.
- Tests: convert a fixture .svg to PNG in a tempdir, decode back with
  `umber_graph::decode_png_rgba8`, build `StencilImage::from_alpha`, and assert
  the coverage equals the direct `rasterize_svg` plane thresholded at 128;
  over-ceiling `--size` refuses; missing args print usage exit 2.

## Wave shape

Two parallel leaves with disjoint locks next wave:
`claw-svg-loader-app` (umber-app files + ftl) and `claw-svg-stencil-cli`
(umber-cli files). Dragon gates + battery + push both, or one commit per leaf
via safe-push.
