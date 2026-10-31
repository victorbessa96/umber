# Kill report: 2026-10-31 asset-browser thumbnails IMPLEMENTED

- **Kill**: the R-C1 follow-up "content-keyed thumbnails" — every
  smart-material / smart-mask row in the Assets browser now shows a
  procedural swatch.
- **Lock**: `wave-asset-thumbs` over the 3 touched files.

## What landed

- **`crates/umber-app/src/asset_thumbs.rs`** (new) — the swatch
  renderer, honest previews not renders:
  - `mask_preview` evaluates the preset's REAL
    `SmartMask::compose_scalar` per pixel; sources get procedural
    stand-ins (Constant as-is, noise generators seeded value noise,
    mesh-map generators a canonical pattern per map kind — AO radial,
    curvature bands, ID steps, normals gradient — anchors mid-gray).
  - `material_preview` composites the template's fill/paint layers
    bottom-to-top over neutral gray (opacity + Normal blend; folder
    layers skipped; generator layers sample the map pattern as
    luminance). Other blend modes fall back to Normal — a hint, not a
    render.
  - `ThumbKey` = (kind, name, hash of the preset's JSON): a changed
    preset file changes the key and regenerates the swatch.
  - 5 unit tests: constant mask uniform at the exact gray, noise mask
    varies, empty mask → None, material swatch deterministic + sized +
    varying, key tracks content and kind.
- **`assets_panel.rs`** — `AssetsPanel` gains the texture cache and a
  `thumb()` helper (generate on first sight, `ctx.load_texture`);
  material and mask rows render a 24 px chip before the draggable
  label. Brush rows unchanged (presets have no swatch source).
- **`main.rs`** — `mod asset_thumbs;`.

## Verification

- `cargo test -p umber-app --bin umber asset_thumbs::` — 5/5.
- Gate-1 battery GREEN: layers + fmt + clippy -D warnings + gpu leg +
  umber-app 893 tests 0 failed. CI on the pushed tip = the wave-end
  gate.

## Not in scope

- True material renders (need a mesh + the compositor — the swatch is
  the discoverability layer, named as such in the module doc).
- Layer `mask` references inside material templates are not previewed.
- The browser kittest (separate named follow-up).
