# Kill brief — row 280: the viewport wiring for the decal projector

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ first:
`docs/specs/projection-fill-plan.md` (YOUR row is 280) and the landed module
`crates/umber-app/src/projection.rs` (row 278 — its API is the contract:
`ProjectionState { image: Option<RgbaImage>, transform: StencilTransform,
opacity: f32, armed: bool, frozen: Option<FrozenPlacement>, path, load_error }`,
`arm(camera, rect)`, `disarm()`, `rasterize_projection(&state, mesh, tile, w, h)
-> Option<Vec<u8>>`, `overlay_quad(&transform, [w, h]) -> [egui::Pos2; 4]`,
`RgbaImage::width()/height()`).

## Your file surface (touch NOTHING else)

- `crates/umber-app/src/viewport.rs` ONLY.

Do NOT run cargo/clippy/tests. Do NOT commit or touch git. No new deps.
Leave the tree with only viewport.rs modified.

## What to build

1. `ViewportExtras` (around `:495`, field `pub stencil: Option<&'a mut
   StencilState>`): add `pub projection: Option<&'a mut crate::projection::ProjectionState>`.
   Thread it through `ui_with_stencil` / the internal ui fns exactly the way
   `stencil` is threaded (search every `stencil:` use site inside viewport.rs
   and mirror them). Callers that pass `stencil: None` pass `projection: None`
   too — the row-281 lane (main.rs) will wire a real state later; keep the
   `Option` shape so nothing else changes.

2. Chords: the S-chord dispatch is around `:1345-1395` (`stencil_drag`,
   `StencilChord::for_button`, `stencil_drag(...)`). Add a parallel
   `projection_drag: Option<StencilDrag>` field on `Viewport` (the drag struct
   is chord-shaped and reusable). When the handed-in projection is `Some` AND
   `projection.armed`, the S+button drag edits **the projection's transform**
   (same `StencilChord` set, same `stencil_drag(...)` mapping fn, same
   Shift-snap); when not armed, the stencil keeps the chords exactly as today.
   Both drags never run at once.

3. Overlay: mirror `paint_stencil_overlay` (`:1393`) with a
   `paint_projection_overlay(ui, rect, projection)` — draw the quad from
   `overlay_quad(&projection.transform, [img.w, img.h])` as a translucent
   accent-filled polygon with a 1px border (NO texture upload in v1; the
   stencil's texture path stays stencil-only). Draw it only when armed and an
   image is loaded. Opacity of the fill: `projection.opacity * 0.25`.

4. The Apply action — a pub method on `Viewport`:
   ```rust
   pub fn apply_projection(
       &mut self,
       rect: egui::Rect,
       mesh: &umber_mesh::MeshData,
       paint: &mut crate::paint_state::PaintState,
       projection: &crate::projection::ProjectionState,
   ) -> bool
   ```
   - `None`-out (return false) unless `projection.image` and `projection.frozen` exist;
   - tile = `paint.active_tile()`; dims = `paint.quick_mask_for_tile(tile).dimensions()`
     (mirror the target-dims logic in `crates/umber-app/src/stencil.rs`
     `panel_section`);
   - `let Some(rgba) = crate::projection::rasterize_projection(projection, mesh, tile, w, h) else { return false };`;
   - `paint.apply_projection(tile, w, h, &rgba, projection.opacity)` and return it.
   - The frozen placement inside `projection` already carries the camera+rect;
     `rect` here is the LIVE viewport rect — do NOT use it for the raster, only
     for nothing. (The raster uses the frozen one.)

5. Pins (append to viewport.rs's tests module, mirroring the pwys row-272
   tests around `:2662` which build a viewport + latched camera):
   - armed projection chord moves `projection.transform` and leaves
     `stencil.transform` untouched; unarmed moves the stencil's;
   - `apply_projection` writes through the FROZEN camera: arm with camera A,
     then move the live camera (or call the method with a rect from a turned
     camera) — the written texels match the frozen placement's raster
     (compare against `rasterize_projection` output directly);
   - no image / not armed-frozen -> false, no paint call.

## Doc comments

Name row 280 of the plan; the v1 cut line (no tiling, no live projector — the
frozen placement never re-projects after arm; no depth/backface culling —
`rasterize_projection` writes every texel whose projection lands in the
image).

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report what you changed.
