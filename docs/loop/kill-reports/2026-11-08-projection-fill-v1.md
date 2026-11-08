# Kill report — R-A2 projection fill v1 complete (rows 277-281)

Date: 2026-11-08, session `local-ee79b5c` (Razul). Plan:
`docs/specs/projection-fill-plan.md` (`80a224a3`). Research:
`docs/research/2026-11-07-projection-fill-{brief,research}.md` (row 276).

## The wave

| Row | Commit | Surface | What landed |
| --- | --- | --- | --- |
| 277 | `92d0d329` | `umber-mesh/src/uv_gbuffer.rs` + `lib.rs` | `world_at_uv(mesh, uv)` — UV→world via `uv_to_barycentric` + `lerp3`; 3 pins; 128/0 |
| 278 | `4856b377` | NEW `umber-app/src/projection.rs` + `lib.rs` | `RgbaImage` + bilinear zero-border sample; `load_image_rgba8` (PNG unthresholded; `.svg` white+coverage; 16 MiB metadata-before-read; pixel caps); `ProjectionState`/`FrozenPlacement` (adopt/arm/disarm); `rasterize_projection` (target-texel: `texel_center_uv` → `world_at_uv` → frozen `view_proj` → NDC → frozen rect → `inverse_map` → bilinear; behind-camera/off-viewport/outside-image → alpha 0; no frozen/degenerate → None); `overlay_quad`; 18 pins |
| 279 | `4856b377` | `umber-paint/src/paint_state.rs:4667` | `apply_projection(tile,w,h,rgba,opacity) -> bool` — mid-stroke/NaN/bad-dims/tile-mismatch refusals; per-texel gate from `gate_plane` (quick × cavity × stencil — one gate, two consumers); straight-alpha blend over the active channel; ONE undo entry via `record_stroke`; 4 pins |
| 280 | `23331a32` | `umber-app/src/viewport.rs` | `ViewportExtras.projection`; armed S-chords drive the projection transform (parallel `projection_drag`); translucent overlay quad (no texture); `Viewport::apply_projection(rect, mesh, paint, projection)`; 4 pins; 1078/0 |
| 281 | `c573dc87` | `main.rs`, `display_panel.rs`, `viewport.rs` (accessor), both `.ftl` | bin re-export; `AppState.projection`; extras wiring with captured view rect; `projection_section` (Load→adopt→arm, Arm re-freezes, Opacity, Apply→apply_projection); `Viewport::camera()`; `projection_rows` panel + 7 `overlays.projection-*` keys in en-US + pt-BR; 1082/0 |

## Claws

278 claude/opus `137ddcb7` $1.66 · 279 claude/sonnet `021094da` $0.42 ·
280 claude/opus `f6fff0c5` $2.87 · 281 claude/opus `b3484104` $3.13.
Dragon review passes: rows 278/279 none needed beyond fmt; row 280 two stale
doc lines fixed; row 281 two clippy fixes (`needless_option_as_deref`,
`unused_mut`) + the sanctioned `camera()` accessor + dead-code allow removed.

## Gates (each row)

fmt clean · `check-layers` GREEN 16 · clippy `--all-targets -D warnings` 0 ·
`cargo test` 0 failed (umber-mesh 128/0; umber-app 1078/0 then 1082/0;
umber-paint green in the 278/279 battery) · safe-push gates on every push.

## The v1 cut line (held)

Static decal projector: NO tiling/repeat · NO live camera-following projector
(pwys option B, the named follow-up) · NO depth/backface culling (both faces
write; a texel behind another surface still writes) · NO bake family · NO
decal channel · NO seam/symmetry expansion of the projection · `.svg` sources
project as white + coverage alpha (mask semantics). `PaintProjection` gained
no variant — the paint crate never sees the camera.

## End-to-end flow (usable now)

Display panel → Projection: Load (PNG/SVG) → adopt + Arm freezes the live
camera + viewport rect → S+LMB/S+RMB/S+MMB place rotate/scale/move (Shift
snaps rotation) → overlay quad shows the placement → Apply rasterizes through
the frozen camera and writes into the active layer, stencil-gated, one undo
step.

## Next

The named v2 follow-ups (tiling/repeat first, then the live projector), or the
sibling lanes' open work (the `.abr` descriptor / samples→`.umberbrush`
mapping waves). Truth-check landed: ROADMAP rows 168 + 258 mark R-A2 LANDED.
