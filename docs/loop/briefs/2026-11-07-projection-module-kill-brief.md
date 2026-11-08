# Kill brief — row 278: the projection module (R-A2 decal projector)

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ the plan first:
`docs/specs/projection-fill-plan.md` (rows 277-281; YOUR row is 278).

## Your file surface (touch NOTHING else)

- NEW `crates/umber-app/src/projection.rs`
- `crates/umber-app/src/lib.rs` — add ONE line `pub mod projection;` in the
  module list (alphabetical, between `psd_import` and `rich_tooltip`)

Do NOT run cargo, clippy or tests (no cargo in claw sessions; the dragon runs
the battery). Do NOT commit, push, stash or touch git. Leave the tree with
only your two files modified. No new dependencies (Cargo.toml untouched).

## What to build (the plan's row 278, verbatim contract)

In `crates/umber-app/src/projection.rs`:

1. `pub struct RgbaImage { pub width: u32, pub height: u32, pub rgba: Vec<u8> }`
   with the private-fields-or-constructor invariant `rgba.len() == width*height*4`
   (mirror `StencilImage` in `crates/umber-app/src/stencil.rs`), plus
   `pub fn sample_bilinear(&self, img_pt: [f32; 2]) -> [u8; 4]` — bilinear sample
   at image-pixel coordinates, ZERO border (outside -> alpha 0).

2. `pub fn load_image_rgba8(path: &Path, max_pixels: u32) -> Result<RgbaImage, ProjectionLoadError>`
   mirroring `load_image_rgba` (`crates/umber-app/src/stencil.rs:418`):
   - non-svg -> the PNG path (`load_png_rgba` in the same file) but KEEP the
     RGBA (do not threshold to coverage); refuse `width*height > max_pixels`
     with a named error;
   - `.svg`/`.SVG` (case-insensitive) -> `std::fs::metadata` FIRST, refuse
     `meta.len() > umber_core::svg::MAX_SVG_INPUT_BYTES` (16 MiB) with
     `SvgError::InputTooLarge` BEFORE `std::fs::read`; then
     `umber_core::svg::rasterize_svg(&bytes, cap)` where `cap` derives from
     `max_pixels` (the raster cap, e.g. `max_pixels.min(DEFAULT_SVG_RASTER_PIXELS)`
     — read `crates/umber-core/src/svg.rs` for the exact API and constants);
     the RGBA comes from `SvgRaster::to_rgba()` (white RGB + coverage alpha —
     document that an .svg source projects as WHITE, it is a mask shape).
   - error enum `ProjectionLoadError` mirroring `StencilLoadError`'s shape
     (Io / Svg(SvgError) / Png / TooLarge{..} / InputTooLarge via Svg).

3. `pub struct FrozenPlacement { pub camera: umber_gpu::camera::OrbitCamera, pub rect: egui::Rect }`

4. `pub struct ProjectionState { pub image: Option<RgbaImage>, pub transform: umber_core::stencil::StencilTransform, pub opacity: f32, pub armed: bool, pub frozen: Option<FrozenPlacement>, pub path: Option<std::path::PathBuf>, pub load_error: Option<String> }`
   + `Default` (opacity 1.0? NO — mirror the stencil's `DEFAULT_OVERLAY_OPACITY`
   idea but for paint: default `0.5`... check `display_panel.rs:289` for the
   stencil's default and pick the analogous paint default, document it) +
   `pub fn adopt(&mut self, path, result, view: [f32; 2])` mirroring
   `StencilState::adopt` (`stencil.rs:262`, uses `fit_to_viewport`) +
   `pub fn arm(&mut self, camera: OrbitCamera, rect: egui::Rect)` (sets
   `frozen` + `armed = true`) + `pub fn disarm(&mut self)`.

5. `pub fn rasterize_projection(state: &ProjectionState, mesh: &umber_mesh::MeshData, tile: u16, w: u32, h: u32) -> Option<Vec<u8>>`
   — the plan's §3, target-texel driven, NO raycasts:
   - `None` unless `state.image` and `state.frozen` are both `Some` and the
     transform is valid (`scale.is_finite() && scale > 0.0`);
   - `let (ox, oy) = umber_mesh::uv_gbuffer::tile_offset(tile);`
   - aspect from the frozen rect: `rect.width() / rect.height()`;
     `let vp = frozen.camera.view_proj(aspect);`
   - for y in 0..h, x in 0..w:
     `let uv = umber_mesh::texel_center_uv(ox, oy, x, y, w, h);`
     `let Some(world) = umber_mesh::world_at_uv(mesh, uv) else { write alpha 0; continue };`
     `let ndc4 = vp * glam::Vec4::new(world[0], world[1], world[2], 1.0);`
     skip (alpha 0) if `ndc4.w <= 0.0` (behind the camera);
     `let ndc = ndc4.truncate() / ndc4.w;` skip if outside `[-1,1]` in x or y;
     `let screen = [rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(), rect.top() + (1.0 - ndc.y) * 0.5 * rect.height()];`
     `let img_pt = umber_core::stencil::inverse_map(&state.transform, [img.width(), img.height()], screen);`
     `let rgba = image.sample_bilinear(img_pt);` write the 4 bytes at `out[(y*w + x)*4 ..]`.
   - returns `Some(out)` of len `w*h*4`.
   - NOTE: `texel_center_uv` is re-exported from `umber_mesh` (check
     `crates/umber-mesh/src/lib.rs:50` for the exact export list and use the
     crate-root paths).

6. `pub fn overlay_quad(transform: &StencilTransform, image_size: [u32; 2]) -> [egui::Pos2; 4]`
   — `umber_core::stencil::forward_map` of the image's four corners, exactly
   like `Viewport::screen_quad` (`crates/umber-app/src/viewport.rs:2067-2073`).

## Pins (in-module `#[cfg(test)] mod tests`)

- loader: an over-16-MiB file is refused from metadata with `InputTooLarge`
  and the file is never read (write a sparse/large temp file in the test);
  an over-cap PNG is refused by name; a small valid PNG round-trips RGBA;
  a small valid svg loads as white+alpha.
- raster: build a flat quad `MeshData` (see `uv_gbuffer.rs` tests' `quad()`)
  in the z=0 plane and an `OrbitCamera` looking straight down -z at it
  (`OrbitCamera::framing(...)` — see `crates/umber-gpu/src/camera.rs:271` and
  the viewport tests at `viewport.rs:2549` for constructors). Identity
  transform centered on the viewport rect with scale = rect.w/img.w: the
  center texel samples the source center color; a source with a red left
  half / blue right half yields red texels on the left half of the tile;
  rotating the transform by 90 degrees swaps which texels see red;
  a texel whose world point projects outside the image gets alpha 0
  (zero border); `w <= 0` (camera behind the plane) writes alpha 0;
  no frozen placement -> None; degenerate transform (scale 0) -> None.
- `overlay_quad` maps the corners through `forward_map` (assert one corner).

## Doc comments

Module doc: row 278 of `docs/specs/projection-fill-plan.md`; the v1 cut line
(no tiling, no live projector, no depth/backface culling — a texel behind
another surface still writes, both faces write); the raster is target-texel
driven and never raycasts (`world_at_uv` landed as row 277).

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report the file list you touched.
