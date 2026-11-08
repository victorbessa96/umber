# Design — R-A2 v2a: projection tiling (repeat over the frozen viewport)

Date: 2026-11-08. Follow-up named in
`docs/loop/kill-reports/2026-11-08-projection-fill-v1.md` ("v2 follow-ups:
tiling/repeat, then the live projector"). Row 287.

## What exists (v1, landed rows 277-281, `c573dc87`)

`crates/umber-app/src/projection.rs`:
- `ProjectionState { image, transform: StencilTransform, opacity, armed,
  frozen: Option<FrozenPlacement>, path, load_error }` (session state, not
  serialized).
- `rasterize_projection(state, mesh, tile, w, h)`: per texel,
  `texel_center_uv` -> `world_at_uv` -> frozen `view_proj` -> ndc ->
  viewport-local px -> `inverse_map(transform, size, screen)` ->
  `image.sample_bilinear(img_pt)` (zero-border: outside the image -> alpha 0).
  Texels off-mesh / behind-eye / off-NDC / off-frozen-rect skip.
- `overlay_quad(transform, image_size)` — one quad, viewport-local px.
- Panel: `display_panel::projection_rows(ProjectionRow)` + main.rs
  `projection_section`; keys `overlays.projection-*` in both .ftl catalogs.

## Target

A **Tile** toggle on the projection section. When on, the decal repeats
infinitely over the frozen viewport: the per-texel image point wraps modulo
the image size instead of falling off the edge. When off, v1 behavior is
byte-identical (regression pin).

- `ProjectionState` gains `pub tiling: bool` (default `false`;
  `adopt()` does NOT reset it — tiling is a placement mode, not image data;
  pin that a load keeps the flag).
- `RgbaImage::sample_bilinear_wrapped(&self, img_pt) -> [u8;4]`: wrap the tap
  indices with `rem_euclid` into `[0, w)` x `[0, h)` so the image tiles
  seamlessly (edge taps wrap to the opposite edge — NOT clamped, NOT zero).
  NaN/inf img_pt -> `[0;4]` (same guard as v1).
- `rasterize_projection`: after `inverse_map`, if `state.tiling` wrap
  `img_pt` via `rem_euclid(w, h)` and call the wrapped sampler; else the v1
  path. All upstream skips (off-mesh, behind eye, off-NDC, off-frozen-rect)
  are unchanged — tiling changes only what happens INSIDE the viewport.
- Overlay: unchanged, single quad (cut: the overlay previews one tile; the
  applied result tiles everywhere in view. Named, not hidden).
- `apply_projection` (paint) unchanged — it consumes the rasterized buffer.
- Panel: `ProjectionRow` gains `pub tiling: &'a mut bool` (a real mutable
  borrow like `opacity`, so the checkbox writes through); section gains a
  "Tile" checkbox enabled with an image; response unchanged. main.rs
  `projection_section` passes `&mut state.tiling`.
- i18n: `overlays.projection-tile` + `overlays.projection-tile-hint` in
  en-US.ftl and pt-BR.ftl (pt-BR translation by the claw, matching register).

## Pins (in projection.rs tests + one panel pin)

1. tiling=false: rasterize output byte-identical with the flag present and
   false vs a hand-computed v1 path (pin against the existing v1 pins' shape).
2. tiling=true wraps: a transform placing the image left of a known texel's
   screen point gives `img_pt.x in [-w, 0)`; the wrapped sample equals
   `sample_bilinear` at `x + w` (pin the exact rgba).
3. seam: a texel mapping to `img_pt.x == w - epsilon` and one at `x ==
   -epsilon` sample adjacent texels (wrap, not clamp, not zero).
4. `sample_bilinear_wrapped` on NaN / negative-huge / inf -> `[0;4]`; on
   in-range pt -> equals `sample_bilinear`.
5. `adopt()` keeps `tiling` across loads; `arm`/`disarm` untouched.
6. rasterize with tiling: texel outside the frozen rect still alpha 0
   (tiling does not leak outside the viewport).
7. panel pin: the Tile checkbox flips the borrowed bool (kittest, following
   the existing projection section pins).

## Non-goals (v2a cut)

- No finite repeat COUNT (infinite wrap only; the frozen rect bounds it).
- No overlay change (single quad). No mirror/alternate tiling. No UV-space
  tiling (the projector is screen-space; UDIM repeat is a different feature).
- No live camera-following projector (that is v2b, pwys option B).
- No persistence (session state stays un-serialized, as v1).

## Verification

Gates per umber-wave-ops: rustfmt clean, check-layers GREEN,
scorecard.py --check, clippy `-p umber-app --all-targets -D warnings` 0,
cargo test -p umber-app green (existing 1082/0 baseline + new pins), i18n
catalog completeness test green. Then dcommit path-limited + safe-push +
release-ci watch.
