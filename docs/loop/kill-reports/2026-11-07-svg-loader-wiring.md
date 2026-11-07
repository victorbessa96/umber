# Kill report — 2026-11-07 svg loader wiring: .svg stencils in the app + `umber-cli stencil` (row 269)

- **Authorship:** two claws — leaf A `claw-svg-loader-app` (Claude Code sonnet, session `e4185b18-6911-4739-a621-f9b02f3b2c5c`, $0.26) and leaf B `claw-svg-stencil-cli` (opencode, session `ses_e48286661ffej550nbMvdwUkAR`). Razul (dragon) reviewed, extended, gated, committed.
- **Kill-locks:** `claw-svg-loader-app` (umber-app stencil.rs + display_panel.rs + viewport.rs + both .ftl catalogs), `claw-svg-stencil-cli` (umber-cli main.rs + new stencil_convert.rs), `razul-svg-input-cap` (umber-core svg.rs, dragon-owned review pass).
- **Implements:** `docs/loop/briefs/2026-11-07-svg-loader-wiring-design.md` + the two leaf briefs; consumes the row 266 core API (`umber_core::svg`), including the sibling's cross-review fix `107f90d1` (hostile-input budgets).

## Built

### Leaf A — the app stencil loader accepts .svg

- `crates/umber-app/src/stencil.rs`: new `pub fn load_image_rgba(path)` — an `.svg`/`.SVG` extension (case-insensitive) is rasterized by `umber_core::svg::rasterize_svg_default` (2048² cap) and thresholded through `StencilImage::from_alpha` exactly like a PNG (alpha ≥ 128); every other extension takes the unchanged `load_png_rgba` path. `StencilLoadError` gains `Svg(#[from] umber_core::svg::SvgError)`; `reason_key()` + `localized()` map it (the raster-cap case reuses `overlays.stencil-too-large` with width/height/max, the rest use the new `overlays.stencil-unsupported-svg` with the error's detail). Three new tests: end-to-end load equals the thresholded direct raster (plus the uppercase extension), over-cap refusal, malformed refusal + reason key.
- `crates/umber-app/src/viewport.rs`: the second load site now calls `load_image_rgba`.
- `crates/umber-app/src/display_panel.rs`: the picker filter is `PNG / SVG` (`&["png", "svg"]`).
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl`: new `.stencil-unsupported-svg` (with `{ $detail }`) in both catalogs; `.stencil-unsupported` reworded so it no longer mislabels an SVG refusal.

### Leaf B — `umber-cli stencil <in.svg> [--size N] <out.png>`

- New `crates/umber-cli/src/stencil_convert.rs`: argument parsing (`--size` anywhere), the 8192² `--size` ceiling refused by name BEFORE the file is touched, the conversion (rasterize → `umber_export::png::write_png` with `Transfer::Linear` — the plane is data, white RGB, alpha = coverage), a one-line JSON summary `{width,height,out}`, and the refusal shape `stencil: refused: {e}` with exit 2 (usage errors print the usage line, exit 2). `convert(args, &mut out, &mut err) -> i32` is the testable body — no subprocess needed.
- `crates/umber-cli/src/main.rs`: `mod stencil_convert;`, the `Some("stencil")` dispatch arm, the usage list and the usage detail line.
- Five tests: convert → decode → rebuild the stencil and compare against the direct raster (accounting for `umber_export::png::read_png_rgba8`'s documented GPU row flip), over-ceiling refusal writes no PNG, `--size` raises the raster cap, missing/extra args and unknown flags exit 2, `--size` parsing bounds.

### Dragon review pass (the cross-review's F2, plus two refinements)

- **F2 (input byte budget, the cross-review's named loader-wave task):** `umber_core::svg` gains `MAX_SVG_INPUT_BYTES` (16 MiB) and `SvgError::InputTooLarge { bytes, max_bytes }`; `rasterize_svg` refuses an over-cap document before parsing it (new pin `input_byte_budget_refuses_an_oversized_document`). Both consumers refuse the FILE from its metadata BEFORE the read — the app loader and the CLI converter — so a huge SVG never enters memory.
- **Over-cap SVG now reports as a size refusal:** `SvgError::TooLarge` maps to the existing `overlays.stencil-too-large` key (with width/height/max), not to "unsupported".
- **Constant rename:** the CLI's ceiling is `MAX_SVG_RASTER_PIXELS` (it is a raster cap, not the app's `DEFAULT_MAX_STENCIL_PIXELS` PNG cap).

## Judgment calls

- `--size N` is the rasterizer's PIXEL CAP (it bounds `width * height`), not a target size; the output dimensions come from the SVG. Documented in the module docs and the usage line.
- The CLI refuses with exit 2 through `std::process::exit` (main returns `anyhow::Result`, which exits 1) — the brief's exit contract wins over the surrounding convention.
- `Transfer::Linear` for the PNG: the stencil plane is data, not an sRGB image.
- The app's picker uses ONE filter ("PNG / SVG"), and the extension test is case-insensitive.
- The byte budget lives in the core module (16 MiB) with the enforcement at both file-read sites; the core check is the last line of defence for callers that pass bytes directly.
- The CLI's PNG is written top-down (PNG file order), which is what the APP's decoder (`umber_graph::decode_png_rgba8`, no flip) expects; the CLI's own test reads back through `umber_export::png::read_png_rgba8`, which DOES flip to GPU row order (its documented contract), so the test compares with the flip. Orientation in the app is correct.

## Unverified at authoring time

- Neither claw could run cargo (bash gated; the workspace is untrusted for claude): the leaves' compile, clippy and test claims were unverified until the dragon battery below. The battery closed them: the leaves' code compiles clean under `-D warnings` and every pin they wrote is green.

## Deferred

- Core (row 266, unchanged): CSS/`<style>`, `use`/`defs`/`symbol`/`text`/`image`/gradients, strokes/opacity, `display`/`visibility`, nested `svg`, `preserveAspectRatio`, percentage lengths.
- The app picker still offers no separate SVG filter, and the CLI has no integration test that drives the binary as a subprocess (the body is tested directly).
- The `--size` ceiling (8192²) is a policy constant in the CLI crate; a future shared policy could move it next to the core caps.

## Gates (dragon, 2026-11-07)

- rustfmt --edition 2021 --check on all six touched .rs files: CLEAN (one rustfmt pass over the leaves' files + the review edits).
- `python3 scripts/check-layers.py`: GREEN (16 crates). `scorecard.py --check`: current.
- clippy `-p umber-core -p umber-app -p umber-cli --all-targets -- -D warnings` under /tmp/umber-cargo.lock: 0 errors, exit 0 (~/.hermes/cache/scratch/svg268-clippy.log).
- `cargo test -p umber-core -p umber-app -p umber-cli` under the flock: **1,407 passed / 0 failed** across 21 suites — umber-core 215 (18 svg pins, including the new input-byte budget), umber-app lib 256 + bin 756 + the integration suites, umber-cli 99 (5 `stencil_convert` pins) (~/.hermes/cache/scratch/svg268-test.log).

## Process note

- Two leaves ran in parallel on disjoint surfaces (claude/sonnet + opencode); each brief named every wall it would hit (no cargo, no locks script, no commit). The dragon owned the review pass, the byte budget, the gates and the landing.
- Row 268 was taken by the sibling lane's wave-abr design brief (`f8d6d84f`) and then by its abr-core kill; this wave is row 269.

## Live smoke (the packaged binary, not a mock)

Run with the debug binary from a clean env and a different CWD (`env -i PATH=/usr/bin:/bin HOME=/tmp`,
cwd = scratch), against a hand-written fixture with a rect, a circle, a transformed path and an arc:

- `umber-cli stencil fixture.svg out.png` → **exit 0**, stdout `{"width":120,"height":90,"out":"…/out.png"}`, the PNG written.
- The PNG decoded (PIL): 120×90 RGBA, RGB white everywhere sampled, alpha 255 inside the rect / circle / transformed triangle / arc region, 0 outside, and **179 partial-coverage pixels** on the antialiased edges — the 4x4 supersampled plane, not a hard mask.
- Missing argument → **exit 2** with the usage line. Malformed SVG → **exit 2**, `stencil: refused: SVG parse error at byte 28: unterminated tag <rect`.
- **The F2 byte budget, live:** a 24 MB SVG → **exit 2**, `stencil: refused: SVG input is 24000034 bytes, over the 16777216-byte cap`, and **no PNG was written** (refused before the read).
- `--size 99999999` → **exit 2**, `stencil: refused: --size 99999999 exceeds the stencil ceiling of 67108864 pixels (8192x8192)`.
