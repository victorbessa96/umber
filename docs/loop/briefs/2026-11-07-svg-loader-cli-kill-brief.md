# Kill brief — svg-stencil-cli: `umber-cli stencil` headless .svg → .png converter (row 269, leaf B)

## Seat and surface

- Authoring claw for ONE kill: `claw-svg-stencil-cli`. Dragon runs the gates.
- Files you may touch: `crates/umber-cli/src/main.rs` (dispatch + usage strings only) and NEW `crates/umber-cli/src/stencil_convert.rs`. Nothing else — no Cargo.toml (umber-cli already depends on umber-core AND umber-export), no app/core files, no docs/queue/STATE/CHANGELOG (dragon-owned).
- Kill-lock `claw-svg-stencil-cli` is registered by the orchestrator over exactly those two files. Do NOT call `scripts/kill-locks.sh`.
- No commit, push, stash, checkout or reset. No cargo (the orchestrator gates); bash may be denied in this untrusted workspace — expected, named here.

## Read first

- `docs/loop/briefs/2026-11-07-svg-loader-wiring-design.md` (the wave design; leaf B section).
- `crates/umber-core/src/svg.rs` (row 266, landed): `rasterize_svg(bytes, max_pixels)`, `rasterize_svg_default(bytes)`, `SvgRaster{width(), height(), coverage(), to_rgba()}`, `SvgError`, `DEFAULT_SVG_RASTER_PIXELS` (2048²).
- `crates/umber-cli/src/main.rs`: the dispatch table (:42, `Some("import-asset") => import_asset_cmd(&args[1..])`), the usage line (:49) and the per-command usage detail (:62), and `import_asset_cmd` (:82) for the argument-parsing shape to copy.
- `crates/umber-export/src/png.rs`: `write_png(path, width, height, rgba, transfer)` (:63) and `encode_png(width, height, rgba, transfer)` (:91) — note the `transfer` parameter (pick the enum variant the export side uses for color data; grep `Transfer::` in umber-export for the available variants and the convention). `read_png_rgba8(path)` (:171) decodes back for the test.

## What to build

- New module `crates/umber-cli/src/stencil_convert.rs` with the conversion:
  `umber-cli stencil <in.svg> [--size N] <out.png>`
  - read the input bytes → `rasterize_svg(&bytes, size)` where `size` = the `--size N` value when passed, else `DEFAULT_SVG_RASTER_PIXELS`.
  - `--size N` is clamped by a HARD CEILING of 8192² (the rasterizer's 4x4 supersampling memory grows quadratically — 8192² is the outer bound; beyond it, refuse with a named error, never allocate).
  - write the PNG: `umber_export::png::write_png(out, raster.width(), raster.height(), &raster.to_rgba(), transfer)` with `transfer = umber_export::png::Transfer::Linear` — the stencil plane is data (white RGB, alpha = coverage), not an sRGB image, so the bytes go out as-is (`Transfer::Linear` is the `#[default]` variant; `Transfer::Srgb` would apply a transfer curve to the RGB channels).
  - stdout: one line of JSON `{"width":W,"height":H,"out":"path"}`.
  - exit codes: 0 success; 2 refusal with the named error on stderr (the `import-asset` contract: `import-asset: refused: {e}` — match that shape).
- Wire it in `main.rs`: add `Some("stencil") => stencil_cmd(&args[1..]),` to the dispatch table, add `stencil` to the usage list (:49) and a usage detail line (:62): `stencil <file.svg> [--size N] <out.png>`.
- `stencil_cmd` lives in main.rs (like `import_asset_cmd`) and delegates to `stencil_convert::convert(...)`.

## Tests (in `stencil_convert.rs`'s own `#[cfg(test)]` module, plus a main.rs usage test if the pattern exists)

- Convert a written .svg fixture to PNG in a tempdir; decode it back with `umber_export::png::read_png_rgba8`; build `umber_core::stencil::StencilImage::from_alpha`; assert the coverage equals the direct `rasterize_svg` plane thresholded at 128.
- `--size` beyond the 8192² ceiling refuses (exit-2 path, named error).
- Missing arguments print the usage line and exit 2.

## Style and gates

- rustfmt (edition 2021) clean, clippy `-D warnings` clean by construction: no unwrap/expect/panic outside tests, doc comments adjacent to items.
- The CLI's usage text and the module docs must describe the LANDED behavior (a `.svg` in, a `.png` out, the size ceiling).

## Handoff

Return the landing notes INLINE (no report files): what changed per file, the exact CLI invocation shape, the `Transfer` variant chosen and why, the tests added, and what you could not verify without cargo.
