# Kill report — rows 278 + 279: the decal projector's module and write path

Date: 2026-11-07, session `local-ee79b5c` (Razul), plan
`docs/specs/projection-fill-plan.md` (`80a224a3`).

## Claws

- Row 278: claude/opus session `137ddcb7-11a3-4729-b49d-210b94e2e461`, $1.66,
  lock `claw-projection-module` (`crates/umber-app/src/projection.rs` NEW +
  `lib.rs`). Brief `docs/loop/briefs/2026-11-07-projection-module-kill-brief.md`.
- Row 279: claude/sonnet session `021094da-af82-45a2-8ce0-518766e42bb2`, $0.42,
  lock `claw-apply-projection` (`crates/umber-paint/src/paint_state.rs`).
  Brief `docs/loop/briefs/2026-11-07-apply-projection-kill-brief.md`.

## What landed (commit `4856b377`)

- Row 278 — `crates/umber-app/src/projection.rs` (new module, 18 pins):
  `RgbaImage` (RGBA8, `len == w*h*4`, bilinear `sample_bilinear` with zero
  border); `load_image_rgba8` — PNG keeps its RGBA unthresholded (refused by
  name past the pixel cap), `.svg`/`.SVG` goes through
  `umber_core::svg::rasterize_svg` + `SvgRaster::to_rgba()` (white RGB +
  coverage alpha — an .svg source projects as WHITE, it is a mask shape),
  with the 16 MiB `MAX_SVG_INPUT_BYTES` metadata-before-read cap; the PNG byte
  budget is `max(16 MiB, 5 bytes/px)`; `ProjectionState` + `FrozenPlacement`
  (`adopt` refits via `fit_to_viewport` and disarms; `arm(camera, rect)`
  freezes; `disarm` keeps the freeze); `rasterize_projection` — target-texel
  driven, NO raycasts: `texel_center_uv` → `world_at_uv` (row 277) →
  frozen `view_proj(aspect)` → perspective divide (w<=0 skipped) → NDC in
  range check → frozen-rect screen px → `inverse_map` → bilinear sample;
  texels off the frozen viewport or outside the image write alpha 0;
  no frozen placement or degenerate transform → `None`; `overlay_quad`
  via `forward_map` of the corners.
- Row 279 — `PaintState::apply_projection(tile, w, h, rgba, opacity) -> bool`
  in `paint_state.rs:4667`: refuses mid-stroke, NaN/zero opacity, mask
  editing, bad dims, tile/channel size mismatch; per-texel gate from
  `gate_plane` (quick × cavity × **the stencil gate** — one gate, two
  consumers); straight-alpha blend over the active channel's plane
  (materialize → read before → blend → `publish_plane`); ONE history entry
  via `record_stroke` + `StrokeCommit` (one undo step); dirty rect unioned
  per changed texel; returns whether anything changed. No `PaintProjection`
  variant added. 4 pins: writes+blends+one-step-undo, stencil-gate blocking,
  opacity scaling, all refusals.

## Dragon gates

rustfmt clean; `check-layers` GREEN (16 crates); `cargo clippy -p umber-app
-p umber-paint --all-targets -- -D warnings` 0; `cargo test -p umber-app
-p umber-paint` — every suite ok, 0 failed (`rows278-279-battery.log`,
BATTERY_278_279_EXIT=0); safe-push ran clippy+test on umber-app, umber-cli,
umber-paint and pushed. Scorecard regenerated in `e419da21` (README tests
2,700+).

## Notes / follow-ups

- Row 278's loader pins cover the metadata-before-read refusal; the claw
  noted the metadata path only bites for a real >16 MiB file (the raster's
  own byte check is the second line).
- Row 279's v1 cut: no seam expansion, no symmetry mirror of a projection
  (named in the doc comment as a follow-up); dirty mark is the union of
  changed texels (not Full).
- `.svg` projection sources are white + coverage alpha (same semantics as
  the stencil); a colored-vector source is a rasterizer feature, not v1.

## Next

Row 280 (viewport wiring: armed S-chords drive the projection transform,
overlay quad, Apply → `rasterize_projection` → `apply_projection` on the
active tile), then row 281 (display panel rows + i18n keys).
