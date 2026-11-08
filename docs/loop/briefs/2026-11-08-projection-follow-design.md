# Design — R-A2 v2b: follow-camera projection (live projector at Apply)

Date: 2026-11-08. Follow-up named in the projection-fill v1 kill report
("then the live camera-following projector (pwys option B)"). Row 288.
Builds on v1 (rows 277-281) + v2a tiling (row 287, `788f95ea`).

## What exists

`projection.rs`: `ProjectionState { image, transform, opacity, armed,
tiling, frozen: Option<FrozenPlacement>, ... }`.
`rasterize_projection(state, mesh, tile, w, h)` uses `state.frozen`
(camera + rect captured at `arm()`). `Viewport::apply_projection` calls it.
The overlay quad is screen-space (camera-independent) and redrawn per frame.

## Target

A **Follow camera** toggle. When on, `arm()` still arms the chords but does
NOT freeze; at Apply, the raster runs through the LIVE camera and the live
view rect (paint-what-you-see now), not the frozen placement. When off, v1
behavior (frozen placement) byte-identical.

- `ProjectionState` gains `pub follow_camera: bool` (default false; adopt/arm/
  disarm/tiling interactions untouched — pin).
- `arm()` behavior depends on the flag: follow on -> `frozen = None`,
  `armed = true` (chords drive the transform); follow off -> current freeze.
  Re-arming re-freezes when follow is off. (Pin both.)
- Raster entry point: extract the placement as a parameter.
  `rasterize_projection(state, mesh, tile, w, h)` keeps its signature and
  uses `frozen` when present; NEW `rasterize_projection_live(state, mesh,
  tile, w, h, camera: &OrbitCamera, rect: egui::Rect)` shares the per-texel
  core via a private `rasterize_with(state, mesh, tile, w, h, camera, rect)`.
  `rasterize_projection` = `frozen` required, else None (v1 contract pinned).
- `Viewport::apply_projection(rect, mesh, paint, projection)` gains the live
  camera + view rect (it already has them at the call site — the viewport
  owns the camera; pass `&self.camera` and the captured view rect). If
  `projection.follow_camera` && armed -> `rasterize_projection_live` with the
  live camera/rect; else the frozen path. Refusal semantics unchanged (no
  image / not armed -> refuse; with follow on, frozen may be None — that is
  fine, the live camera is the placement).
- Panel: `ProjectionRow` gains `pub follow_camera: &'a mut bool`; a "Follow
  camera" checkbox next to Arm (enabled with image); main.rs passes it
  through. i18n: `overlays.projection-follow` + `-hint` in both catalogs.
- Interaction with tiling (row 287): orthogonal, both compose (tiling wraps
  the image point; follow picks the camera). Pin one composed case.

## Pins

1. follow off: `arm()` freezes (frozen.is_some()), rasterize uses frozen —
   byte-identical to v1 output (reuse the v1 pin shape).
2. follow on: `arm()` leaves frozen None, armed true; `rasterize_projection`
   (frozen entry) returns None (no frozen placement) — pinned, so callers
   must use the live entry.
3. live raster equals frozen raster when the live camera == frozen camera
   (same view_proj) — pin equality of the buffers.
4. live raster differs when the camera moved (rotate the live camera, assert
   the buffer changed and matches a hand-computed re-projection through the
   new view_proj).
5. composed tiling+follow: wraps as v2a under the live camera.
6. adopt keeps follow_camera; panel pin for the checkbox.

## Non-goals (v2b cut)

- No per-stroke continuous re-projection (the decal re-rasterizes at Apply,
  not every frame mid-stroke). No depth/backface culling. No persistence.
- No overlay change (screen-space quad is camera-independent already).

## Verification

Same gates as row 287: fmt, layers GREEN, scorecard, clippy
`-p umber-app --all-targets -D warnings` 0, umber-app suite green
(1090/0 + new pins), i18n completeness. Then dcommit + safe-push +
release-ci.
