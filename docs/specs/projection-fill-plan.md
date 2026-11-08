# Projection fill (R-A2) — the decal projector: PLAN

Date: 2026-11-07 (row 276's research → this plan). Research:
`docs/research/2026-11-07-projection-fill-{brief,research}.md`. Brief:
`docs/research/2026-11-07-projection-fill-brief.md`.
Supersedes: nothing (the ROADMAP projection row is open). Superseded by: nothing yet.

## 1. The v1 decision (carried from the research, binding)

Umber v1 gets a **decal projector**: the artist loads an image (PNG or `.svg`),
places it over the viewport with translate/scale/rotate + opacity, and the image
is projected onto the mesh through the camera **frozen at the placement moment**
(row 272's snapshot discipline); the paint lands in the **ordinary paint layers**
(history, layers, symmetry, export unchanged) and is gated by the **existing
stencil gate** (one screen-space gate, two consumers).

Cut line (NO in v1): tiling/repeat over the viewport · a live camera-following
projector (that is the pwys option-B follow-up, and it contradicts row 272's
frozen rule) · backface/depth culling, shape crop, filtering (3D-projector
family) · Mari's bake-behavior family · a separate decal channel.

`PaintProjection` gains **no** third variant: the paint crate never sees the
camera; the hit handed to it is still a UV (research §3.5,
`paint_state.rs:456`).

## 2. The seams, counted from source

| Seam | Where | What the plan reuses |
| --- | --- | --- |
| placement transform | `StencilTransform { center, scale, rotation }`, `umber_core::stencil` (`crates/umber-core/src/stencil.rs:98`) | the projection's transform IS a `StencilTransform` (translate = center) |
| image↔screen maps | `forward_map` / `inverse_map` (`stencil.rs:266,278`) | the overlay quad (`forward_map` of the corners, as `viewport.rs:2067-2073`) and the raster (`inverse_map`) |
| per-texel raster | `rasterize_gate` (`stencil.rs:294`) | the shape of the projection raster (target-texel driven, bilinear sample) |
| gate upload seam | `PaintState::set_stencil_gate(Option<(u32,u32,Vec<u8>)>)` (`crates/umber-paint/src/paint_state.rs:5772`), field at `:1366`, `stencil_gates()` at `:5787` | the mask: `apply_projection` writes go through this gate automatically |
| undo group | `begin_stroke` (`paint_state.rs:1930`) / `end_stroke` (`:4521`) | one Apply = one undo step |
| frozen camera | `Viewport::stroke_camera: Option<OrbitCamera>` (`crates/umber-app/src/viewport.rs:459`), `latch_stroke_camera` (`:1861`), `pick_with(rect, mesh, pos, camera)` (`:1873`) | the placement's frozen camera; the raster projects through `camera.view_proj(aspect)` (`umber-gpu/src/camera.rs:166`) |
| texel→UV | `texel_center_uv` (`crates/umber-mesh/src/uv_gbuffer.rs:151`), `tile_offset` (`:142`) | the raster's per-texel UV |
| UV→world | `uv_to_barycentric(mesh, uv) -> Option<UvHit>` (`uv_gbuffer.rs:104`), `lerp3` (`:123`), `ray_intersect`/`uv_at` (`raycast.rs:40,138`) | row 277 adds `world_at_uv`; the raster never raycasts |
| chords | `StencilChord::{Rotate{snap},Scale,Move}` (`viewport.rs:500`), `for_button` (`:513`), drag at `:546`, dispatch at `:1354` | while the projection is ARMED, the same S-chords drive the projection's transform |
| overlay | `paint_stencil_overlay` (`viewport.rs:1393`) | the projection's overlay quad mirrors it |
| panel | `stencil_rows(ui, StencilRow)` (`crates/umber-app/src/display_panel.rs:323`), borrow shape at `:94-101` | a sibling `projection_rows` with a `ProjectionRow` adapter |
| loader | `load_image_rgba` (`crates/umber-app/src/stencil.rs`) — PNG + `.svg` (rows 266/269), `MAX_SVG_INPUT_BYTES` metadata-before-read, `SvgRaster::to_rgba` in core | the projection's source keeps RGBA (the stencil thresholds alpha≥128; the projector must not) |

## 3. The write direction (decided)

**Target-texel driven, never source-pixel raycasts.** For each texel of the
active tile: `texel_center_uv` → `world_at_uv` → `view_proj(aspect) * world`
(perspective divide; `w <= 0` = behind the camera, skipped) → NDC → screen px in
the frozen rect → `inverse_map` → bilinear RGBA sample of the source image
(zero border) → one RGBA byte quad into the tile-sized output buffer. This is
`rasterize_gate`'s direction with color instead of coverage. The rejected
alternative (iterate source pixels, `inverse_map` → screen → `pick_with`
raycast) costs one BVH raycast per covered source pixel and is unbounded in
overdraw; the chosen direction costs one `uv_to_barycentric` + one matrix
multiply per target texel and no raycasts.

Known v1 artifacts, all on the cut line: no depth test (a texel behind another
surface still writes), no backface culling (both faces write), no tiling.

## 4. Budgets

- Source image: the raster pixel cap (2048² default; a `--size`-style override
  may reuse `DEFAULT_SVG_RASTER_PIXELS` discipline), and the input-byte cap
  `MAX_SVG_INPUT_BYTES` (16 MiB) checked from `fs::metadata` BEFORE the read,
  exactly as the stencil loader does. PNGs: same byte cap; dimension cap from
  the same raster cap (a PNG's `w*h*4` must fit).
- Raster cost: active-tile texels × O(1). Memory: ONE tile-sized RGBA buffer
  (the same order as the existing gate buffer); stream per row if a tile ever
  exceeds 4096².
- Writes: applied per texel through the paint conditioning, gated by the
  stencil gate — no separate mask.

## 5. The rows (disjoint file surfaces)

| Row | Surface (files) | Content | Pins |
| --- | --- | --- | --- |
| 277 | `crates/umber-mesh/src/uv_gbuffer.rs` | `world_at_uv(mesh, uv) -> Option<Vec3>`: `uv_to_barycentric` → the hit's tri + bary → `lerp3` over `MeshData` positions | a known triangle's corners map to their positions; outside every tri → None; degenerate tri → None |
| 278 | NEW `crates/umber-app/src/projection.rs` + `crates/umber-app/src/lib.rs` | `RgbaImage { w, h, rgba }`; `load_image_rgba8` (PNG + `.svg` via `umber_core::svg`, 16 MiB byte cap before read, raster cap); `ProjectionState { image, transform: StencilTransform, opacity, armed, frozen: Option<FrozenPlacement { camera: OrbitCamera, rect: egui::Rect }>, path }`; `rasterize_projection(state, mesh, tile, w, h) -> Option<Vec<u8>>` (row 277's `world_at_uv`, `view_proj`, `inverse_map`, bilinear); the overlay quad from `forward_map` corners | loader budgets refuse; a flat quad facing the camera rasterizes to the expected texel colors; rotated transform moves them; behind-camera texels skip; degenerate transform → None |
| 279 | `crates/umber-paint/src/paint_state.rs` | `apply_projection(&mut self, tile: u16, w: u32, h: u32, rgba: &[u8], opacity: f32) -> bool`: refuse while stroking; `rgba.len() == w*h*4`; one undo group (`begin_stroke`/`end_stroke` semantics); each texel written through the dab conditioning (blend mode, active layer, symmetry hooks) with the stencil gate applied per texel; returns whether anything landed | writes land in the active layer; ONE undo step reverts all; the stencil gate blocks the gated texels; opacity scales the written alpha; mid-stroke refusal; bad dims refused; empty source → false |
| 280 | `crates/umber-app/src/viewport.rs` | arm/adopt captures `FrozenPlacement { camera, rect }`; while armed the S-chord dispatch (`:1354`) drives the projection transform (stencil chords keep theirs when not armed); overlay quad drawn like `paint_stencil_overlay`; the Apply action rasterizes (row 278) and calls `apply_projection` on the active tile (the stencil's `staged_for` tile) | armed chords move the projection transform, not the stencil's; unarmed chords still drive the stencil; Apply writes through the frozen camera after the live camera moved |
| 281 | `crates/umber-app/src/display_panel.rs` + `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` | `projection_rows` mirroring `stencil_rows` (Load / Arm / opacity / Apply / path label), `ProjectionRow` adapter in the panel's borrow shape | i18n keys present in both catalogs; the rows compile against the adapter (no `crate::projection` named in the panel) |

Order: **277 → (278 ∥ 279) → 280 → 281**. 278 and 279 are parallel: their
contract is this plan's `apply_projection(tile, w, h, rgba, opacity)` signature
and the tile-sized RGBA buffer, mirroring the gate's `(u32, u32, Vec<u8>)`.
280 needs 278; 281 needs 278's state shape.

## 6. Semantics details (binding for the rows)

- **Freeze**: the frozen placement (camera + viewport rect) is captured at
  adopt/arm. Apply rasterizes through the frozen placement even if the live
  camera has moved — the same rule row 272 gave strokes. Re-arm re-freezes;
  Apply does not re-freeze (re-applying is deterministic and identical).
- **Chords**: while armed, S+LMB rotates (Shift snaps to quarter turns),
  S+RMB scales, S+MMB moves — the `StencilChord` set verbatim, dispatched to the
  projection's transform. When not armed, the stencil keeps them.
- **Mask**: the stencil gate is untouched; `apply_projection` writes through
  it. One gate, two consumers.
- **Undo**: one Apply = one undo step, named in history as a projection.
- **The paint crate never sees the camera**: rows 278/280 own the frozen
  placement and the projection math; row 279 only receives texels.

## 7. Named follow-ups (not v1)

Tiling/repeat over the viewport (Painter's projection repeats) · the live
camera-following projector (pwys option B — reuses the frozen-placement seam by
re-projecting on camera change) · backface/depth culling and the 3D-projector
family · Mari's bake family · a decal channel.
