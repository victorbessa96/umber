# Kill brief — svg-loader-app: the app stencil loader accepts .svg (row 269, leaf A)

## Seat and surface

- Authoring claw for ONE kill: `claw-svg-loader-app`. Dragon runs the gates.
- Files you may touch: `crates/umber-app/src/stencil.rs`, `crates/umber-app/src/display_panel.rs`, `crates/umber-app/src/viewport.rs`, `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`. Nothing else — no Cargo.toml (umber-app already depends on umber-core), no core/cli files, no docs/queue/STATE/CHANGELOG (dragon-owned).
- Kill-lock `claw-svg-loader-app` is registered by the orchestrator over exactly those five files. Do NOT call `scripts/kill-locks.sh`.
- No commit, push, stash, checkout or reset. No cargo (the orchestrator gates); the workspace is untrusted for claude, so bash may be denied — expected, named here.

## Read first

- `docs/loop/briefs/2026-11-07-svg-loader-wiring-design.md` (the wave design) and `docs/loop/briefs/2026-11-07-wave-svg-import-design.md` (row 265, the decision).
- `crates/umber-core/src/svg.rs` (the module this kill consumes; landed in row 266) — public API: `rasterize_svg(bytes, max_pixels)`, `rasterize_svg_default(bytes)`, `SvgRaster{width(), height(), coverage(), to_rgba()}`, `SvgError`, `DEFAULT_SVG_RASTER_PIXELS` (2048²).
- `crates/umber-app/src/stencil.rs`: `DEFAULT_MAX_STENCIL_PIXELS` (:77), `StencilLoadError` (:86) with `reason_key()` (:108) + `localized()`, `stencil_from_png_bytes` (:368), `load_png_rgba` (:382), the panel load site (:425), the test module's `load` helper (:502).
- `crates/umber-app/src/display_panel.rs:333` — the stencil file picker: `rfd::FileDialog::new().add_filter("PNG", &["png"])`.
- `crates/umber-app/src/viewport.rs:2069` — the second load site (`crate::stencil::load_png_rgba(&path)`).
- `assets/i18n/en-US.ftl` / `assets/i18n/pt-BR.ftl`, section `overlays`: `.stencil-unsupported` currently reads "not a supported PNG (8-bit RGB or RGBA)." — a new key is needed for an SVG refusal; BOTH catalogs must carry it (catalog parity is a CI gate).

## What to build

- New `pub fn load_image_rgba(path: &Path) -> Result<StencilImage, StencilLoadError>` in `crates/umber-app/src/stencil.rs`:
  - extension `.svg`/`.SVG` → `std::fs::read(path)` → `umber_core::svg::rasterize_svg_default(&bytes)` (2048² cap) → `StencilImage::from_alpha(w, h, &raster.to_rgba())` — the row-266 pin-6 gate path, identical to PNG (ALPHA_CUT 128 threshold).
  - every other extension → the existing `load_png_rgba` path, unchanged.
- `StencilLoadError` gains `Svg(#[from] umber_core::svg::SvgError)`; `reason_key()` maps it to a NEW key `overlays.stencil-unsupported-svg`; `localized()` gets the matching arm (same shape as the other arms: `i18n::tr_args` with the named args it needs — the SVG error carries a message, so the detail may be the error's Display; keep the ftl value a sentence with a `{ $reason }`-style placeholder only if the ftl defines it).
- Switch BOTH load sites to `load_image_rgba` (stencil.rs:425 and viewport.rs:2069) so the app accepts .svg anywhere a stencil is loaded.
- The picker filter lists SVG next to PNG: `.add_filter("PNG / SVG", &["png", "svg"])` (or two filters) in display_panel.rs:333.
- i18n: add the new key to BOTH catalogs; update `.stencil-unsupported` text if it now covers non-PNG non-SVG cases only — do not leave a message that mislabels an SVG refusal.

## Tests (in `crates/umber-app/src/stencil.rs`'s test module)

- A written .svg (a rect + a circle) loads end-to-end from a temp file to the expected thresholded coverage (compare against `umber_core::svg::rasterize_svg_default` on the same bytes).
- An over-cap .svg (declared 100000x100000) refuses with `StencilLoadError::Svg(SvgError::TooLarge { .. })`.
- A malformed .svg refuses with `StencilLoadError::Svg(SvgError::Parse { .. })`.
- The PNG path is unchanged: the existing pins still pass (do not weaken them).

## Style and gates

- rustfmt (edition 2021) clean, clippy `-D warnings` clean by construction: no unwrap/expect/panic outside tests (umber-app denies them in non-test code), doc comments adjacent to items, no needless borrows.
- New public items get doc comments; the module docs of stencil.rs get a line naming the .svg acceptance (the doc-comment-sync rule: the module doc must describe the LANDED behavior).

## Handoff

Return the landing notes INLINE (no report files): what changed per file, the new i18n keys in both catalogs, the tests added, the choices you made where the brief allowed one, and what you could not verify without cargo.
