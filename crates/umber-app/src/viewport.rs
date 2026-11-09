//! The 3D viewport: owns the orbit camera and the GPU-resident mesh, and
//! paints into the dock's `Panel::Viewport` tab each frame.
//!
//! Never touches a `wgpu` type by name — every value it holds or passes to
//! `umber_gpu` is opaque to this crate (received from or handed to
//! `umber_gpu`'s API), per the architecture rule that GPU objects stay
//! behind `umber-gpu`.
//!
//! CORE-LOOP follow-up (the strategy-review disposition's first core-loop
//! kill: viewport texture binding; the GPU side —
//! `MeshBuffers::set_base_color_texture` / `clear_base_color_texture` —
//! landed first and waited uncallable). Each frame with a mesh loaded, the
//! paint session's ACTIVE tile is bound as the mesh pass's base-color
//! texture just before the draw callback is built, so the stroke you paint
//! is the stroke you see lit in 3D. Judgment calls, all v1:
//!
//! * One tile bound at a time — the active tile
//!   (`PaintState::active_tile` + `tile_target`), not a hardcoded 1001.
//!   Single-tile sessions always have the active tile at 1001, so their
//!   behavior is unchanged; multi-tile meshes show the tile under the last
//!   pointer. The tile-pool atlas + page-table indirection (every tile at
//!   once) is the architecture doc's named later slice.
//! * The tile select is `PaintParams::for_udim` (the real constructor —
//!   there is no `for_tile(tile, size)`); no target size enters the bind
//!   because the shader samples in UV space and the texel density lives in
//!   `PaintState`.
//! * Re-bound EVERY frame while paint is present: a `Resize` command
//!   replaces the tile's texture object, so a params-equality skip would
//!   keep sampling the evicted texture. The matching clear is skipped once
//!   already cleared (flags == 0 holds no view — provably identical).
//! * The bind sits before the route match, so the Direct and PostFx arms
//!   both see paint (the PostFx path reuses the mesh's own callback).
//! * No paint session (or a session tile with no target yet) clears back
//!   to the constant arm — pixel-identical to a mesh that never bound
//!   paint, and the mesh-less empty-state hero path is untouched (it binds
//!   nothing at all).
//!
//! Orthographic mode (P while hovered — P is free: the shell's
//! `SHORTCUT_KEYS` in `toolbar.rs` bind W/G/V/X/I/E/Z/S/O and the graph
//! canvas takes Delete/Backspace/Escape): the camera's
//! [`umber_gpu::camera::Projection`]
//! flips, `load_mesh` keeps it across re-framing like yaw/pitch, and
//! picking unprojects the NEAR point as the ray origin (parallel rays have
//! no eye to fan out from).
//!
//! Orthographic frames always take [`ViewportRoute::Direct`], even with an
//! active post chain: the chain's only depth reader is DoF, whose WGSL
//! `linearize_depth` (`postfx_exec.rs`, `POSTFX_SHADER`) inverts the
//! perspective depth encoding (`near * far / (far - d * (far - near))`,
//! mirrored by `postfx_exec::linearize_depth`) — under orthographic
//! projection the depth buffer is linear (`near + d * (far - near)`), so
//! that formula would defocus by the wrong distance. Bloom / vignette /
//! sharpen / tone-map are color-only and need no such gate. Named
//! follow-up: an ortho-aware DoF (linear-depth branch) that re-admits the
//! PostFx route under [`umber_gpu::camera::Projection::Orthographic`].
//!
//! Ortho-safety of the shared unprojection: [`umber_gpu::world_from_ndc`]
//! divides by `w`, and the orthographic matrix keeps `w = 1` (last row
//! `0,0,0,1` — pinned by the camera test), so the perspective divide is a
//! no-op there. The ground grid (`GpuContext::grid_callback`, whose
//! `GRID_SHADER` carries the WGSL copy of `world_from_ndc` and rebuilds
//! its ray from the NDC near/far unprojections) therefore works unchanged:
//! ortho rays come out parallel instead of fanning, and the y=0 plane
//! intersection is the same arithmetic.
//!
//! # The display-chain-aware eyedropper (B-10's named follow-up)
//!
//! The picker reads the paint target's linear RGBA8 texel, converts it
//! per-texel to an sRGB byte (the v1 sample), then runs that sample
//! through the session's viewer chain — the same `apply_display_chain`
//! call the Display panel's preview strip makes on the same
//! `DisplaySettings` — before returning [`PickSample`]. The default
//! settings (Raw / 0 EV / gamma 1) are the bit-exact identity chain, so
//! an untouched project picks byte-for-byte as before. Judgment calls:
//! the chain's input is the display-encoded sample (the brief's stage
//! order — `linear_to_srgb_byte`, then the chain), and the adopted OCIO
//! processor is not threaded here (the preview strip threads it only in
//! `ocio` builds), so an adopted config's pick uses the fixed view stage.
//!
//! # The Overlays button + popover (GUI plan g6-overlays-popover)
//!
//! docs/specs/gui-plan-2026-10-15.md row `g6-overlays-popover` (B:C3.1-
//! C3.4, P:B10): one "Overlays" button at the viewport's top-right opens
//! a popover over [`crate::display_panel::OverlaySettings`] (drawn by
//! [`crate::display_panel::overlay_rows`]): wireframe + opacity, grid,
//! UV seams, face orientation, the channel-name readout.
//!
//! - Wireframe / grid stay owned by the shell (`ViewToggles`, W / G, the
//!   View menu): [`Viewport::set_show_wireframe`] / `set_show_grid` write
//!   into the overlay struct, and a popover edit is reported back through
//!   [`Viewport::take_overlay_edit`] for the shell to store — one source
//!   of truth, the popover a second editor of it.
//! - Wireframe opacity rides the wire pass's per-draw color
//!   ([`wire_color_for`]); the default opacity is the old default color,
//!   so the off-the-shelf frame is unchanged.
//! - UV seams: [`uv_seam_segments`] (pure, cached per mesh load) turns
//!   `umber_mesh::seam::build_seam_graph`'s cuts into 3D segments; they
//!   draw as screen-space lines BEFORE the wireframe callback (so wire
//!   edges win), skipped when both adjacent faces point away
//!   ([`face_side`]). No depth test: a seam behind other front-facing
//!   geometry still shows — a depth-tested seam pass in `umber-gpu` is
//!   the named follow-up.
//! - Face orientation: [`face_side`] is the decision; the flag
//!   round-trips ([`Viewport::face_orientation`]) but the tint needs the
//!   mesh pipeline to render back faces (it culls them), which is
//!   renderer work outside this file — the row is disabled with a reason
//!   until then (`display_panel::FACE_ORIENTATION_RENDERS`).
//! - Channel readout: the soloed channel / mesh map's name
//!   ([`readout_text`], the display-mode keys' own toast text) at the
//!   viewport's top-left while a solo mode is active.
//!
//! # The stencil overlay + chords (wave-stencil step 3, g6-stencil-overlay)
//!
//! docs/specs/stencil-engine-design.md "What the user sees" + "The
//! chords". The shell hands its [`crate::stencil::StencilState`] in
//! through [`ViewportExtras`] ([`Viewport::ui_with_stencil`]); `None`
//! (every [`Viewport::ui_with_home`] call) draws and gates nothing.
//! Judgment calls, all v1:
//!
//! - **The transform's space is VIEWPORT-LOCAL px** (origin at the
//!   viewport rect's top-left): the design's "center (screen px)" and
//!   `fit_to_viewport`'s `[w/2, h/2]` center. The pointer converts with
//!   `pos - rect.min` before any math against `center`. Step 2's
//!   `StencilState` docs say "target texel space" and its Display-panel
//!   Reset fits to the quick-mask plane size; that Reset and the gate's
//!   target-texel rasterize are the named handoffs (the screen -> UV
//!   projection belongs to the paint-hook kill).
//! - **The quad is a textured `egui::epaint::Mesh`** (4 vertices, 2
//!   triangles): the corners come from [`screen_quad`], which is
//!   `umber_core::stencil::forward_map` at the image's four corners — the
//!   gate's own map, never re-derived. The texture follows uv_view.rs's
//!   quick-mask overlay (`mask_overlay_texture`): cached and keyed, built
//!   point-sampled at most [`STENCIL_OVERLAY_MAX_SIDE`] per side, and
//!   uploaded with `TextureOptions::NEAREST`. Nearest, not bilinear: the
//!   gate is binary, so the preview's hard edges are the truth. It is
//!   tinted [`STENCIL_TINT`] (the image keeps coverage only, no RGB),
//!   drawn at `overlay_opacity`, with an outline in the full tint so a
//!   faint stencil's bounds still show.
//! - **Draw order**: after the mesh, the seams, the wireframe and the cage
//!   (the overlays family's slot), before the line preview, the gizmo and
//!   the brush cursor, so the tools' own marks stay on top. It is hidden
//!   while the stencil is inactive (the design: N hides it), without a
//!   mesh, and at opacity 0.
//! - **The chords** (S held, no Ctrl/Alt; Shift is free for the snap):
//!   S+LMB rotates around the center, S+RMB scales, S+MMB moves. A drag
//!   latches its chord and the transform at its PRESS
//!   (`pointer.press_origin()`, graph_canvas.rs's precedent).
//!   [`stencil_drag`] always maps from that snapshot, so frames never
//!   compound. Releasing S mid-drag keeps the gesture, and pressing S
//!   mid-orbit doesn't take it over. While a chord is latched, or S is
//!   armed on a click, the gizmo, eyedropper, ID mask, straight line,
//!   brush resize, painting and navigation all stand down.
//! - **Rotate**: the arc's angle change around the center (`atan2`; on
//!   the y-down screen that is clockwise-positive, the core's own sign).
//!   Shift snaps the RESULT to quarter turns (the straight line's
//!   round-to-step idiom, [`straight_line_end`]). The result wraps into
//!   `(-π, π]`. A pointer within [`STENCIL_MIN_ARM_PX`] of the center
//!   holds the rotation (the angle is undefined there).
//! - **Scale**: the image's on-screen half-width grows by the drag's
//!   horizontal delta (`scale + dx / (image_w / 2)`), so a drag from the
//!   unrotated image's right edge keeps that edge under the pointer.
//!   Vertical motion is ignored; the result is clamped to at least
//!   [`MIN_STENCIL_SCALE`].
//! - **Move**: the center follows the pointer delta, clamped into the
//!   viewport so a stencil can't be flung out of reach.
//! - **The gate re-stages ONCE, on release** (and on a popover toggle,
//!   Reset or load), never per drag frame: step 2's judgment call 17 puts
//!   a 4K rasterize at ~16M samples.
//! - **The Overlays popover hosts step 2's rows**
//!   (`display_panel::stencil_rows`) with THIS viewport's rect as the
//!   fit view, so the popover's Reset and the load-time fit land on
//!   screen. It mirrors `stencil::panel_section`'s adapter rather than
//!   calling it, because that fn fits to the target size (stencil.rs is
//!   lock-held).
//! - **N is not wired here**: W/G (the overlays family) are shell-owned
//!   registry rows (actions.rs `BINDINGS`), so N's toggle is the shell's
//!   handoff (a `ShellAction` row; P-in-viewport is the alternative
//!   precedent).
//!
//! # The decal projector's viewport wiring (projection-fill row 280)
//!
//! `docs/specs/projection-fill-plan.md` row 280, on top of row 278's
//! [`umber_app::projection`] module (the lib's; main.rs re-exports it as
//! `projection::` since row 281). The shell hands its
//! [`ProjectionState`] in through
//! [`ViewportExtras::projection`]; `None` (every [`Viewport::ui_with_home`]
//! call until row 281 wires main.rs) changes nothing. Judgment calls:
//!
//! - **Arming routes the S-chords**: while the handed-in projection is
//!   armed AND holds an image, an S+button drag edits
//!   `projection.transform` (same [`StencilChord`] set, same
//!   [`stencil_drag`] mapping, same Shift snap); the stencil's chords stand
//!   down. Unarmed, the stencil keeps its chords exactly as before. An
//!   armed projection WITHOUT an image routes nothing (it has no dims to
//!   map with), so the stencil keeps the chords then too.
//! - **Never two drags at once**: the projection latches only while no
//!   stencil drag is in flight, and the stencil latches only while the
//!   projection is unarmed and no projection drag is in flight. A drag
//!   already latched runs to its release whatever arms meanwhile.
//! - **The chord frame is read once** ([`ChordFrame`], every egui input
//!   the chords consume) and both input blocks run from it, so the routing
//!   is pinnable without an egui frame or a GPU mesh.
//! - **The projection's release re-stages nothing**: unlike the stencil,
//!   its transform feeds no paint gate (Apply rasterizes on demand).
//! - **The overlay is an untextured quad** (v1): the corners of
//!   [`overlay_quad`] as a translucent polygon in the
//!   theme accent (`visuals.selection.bg_fill`), its fill at the
//!   projection's opacity times [`PROJECTION_FILL_FACTOR`] (0.25), plus a
//!   1 px accent border. No texture upload: the stencil's texture
//!   path stays stencil-only. Drawn only while armed with an image, in the
//!   stencil's slot (after it, so the armed decal reads on top).
//! - **Apply ([`Viewport::apply_projection`]) rasterizes through the
//!   FROZEN placement**, never the live camera or rect: the target dims
//!   are the active tile's quick-mask dims (`stencil::panel_section`'s
//!   target-dims rule), the texels come from
//!   [`rasterize_projection`], and
//!   `PaintState::apply_projection` (row 279) blends them at
//!   `projection.opacity`.
//! - **Follow camera (R-A2 v2b, row 288)**: with
//!   `projection.follow_camera` AND armed, Apply instead runs
//!   [`rasterize_projection_live`] through the viewport's LIVE camera and
//!   the `rect` the shell hands in (its last drawn view rect), once per
//!   Apply click (never per frame or mid-stroke). Follow off is the
//!   frozen path above, unchanged.
//! - **v1 cut line**: (tiling landed in v2a, the live projector in v2b
//!   above); no depth culling (`rasterize_projection` writes every texel
//!   whose projection lands in the image, including one behind another
//!   front-facing surface). Backface culling landed in v2c (row 297):
//!   `projection.cull_backfaces` (default on) skips the texels whose
//!   covering triangle faces away from the projecting camera's eye.
//!
//! # The camera mid-stroke (pwys-stroke-buffer, option A)
//!
//! docs/specs/pwys-stroke-buffer-design.md, "The v1 choice: A"
//! (requirements §1's "the camera can't move mid-stroke" follow-up).
//! Before this, the frame loop skipped ALL camera input during a paint
//! drag, so the live camera could not move while a stroke was open. Now
//! the stroke freezes its RAYCAST instead of the camera:
//!
//! - **The snapshot**: `Viewport::stroke_camera` is a plain-data copy of
//!   the [`OrbitCamera`] (it is `Copy`), latched by the first event that
//!   opens a viewport stroke — the very camera that event resolved
//!   against — and cleared on the frame the paint drag ends (the same
//!   frame `end_stroke` runs). A drag that opens on a miss latches
//!   nothing until its first hit (the stroke starts there).
//! - **The resolve**: every stroke event goes through `stroke_event`,
//!   which picks against the snapshot (`pick_with`), never the live
//!   camera. Hit/miss is the START camera's: a ray that hit at stroke
//!   start still paints after the live camera turns away, and a frozen
//!   miss skips the dab whatever the live view shows. The front-face
//!   weight comes from the same frozen pick. The eyedropper, the ID-mask
//!   select, the brush cursor, the straight line and the orbit pivot keep
//!   picking against the LIVE camera.
//! - **Per-event staging is unchanged**: the paint crate still receives
//!   one resolved UV per event (`begin_/extend_screen_point_pressured`),
//!   so the live preview (UV view, the 3D bind, export reads), the drain
//!   cadence, undo capture and the seam/symmetry/retouch accounting see
//!   exactly the old flow. A stationary camera paints byte-identically.
//! - **The guard is per input source**: pointer navigation
//!   (`handle_pointer_nav`: the preset's drag chords + the wheel) still
//!   stands down while painting — the artist's pointer belongs to the
//!   stroke. The non-pointer camera sources (`handle_camera_keys`: P
//!   projection, F frame) now run mid-stroke too. Judgment call: a
//!   Shift-held key stands down mid-stroke, because a stroke always
//!   holds Ctrl/Cmd and Cmd+Shift+P is the shell's command palette. No
//!   new binding was invented; a turntable or gamepad source would join
//!   `handle_camera_keys`' side of the guard.
//! - **Deferred** (the design's non-goals): screen-space GPU reprojection,
//!   resolve-at-end (option B, the named follow-up), camera animation
//!   authored mid-stroke. Known v1 seam: the live 3D view may now show
//!   the dab landing where the mesh WAS at stroke start, not under the
//!   cursor — the design's paint-what-you-saw-at-stroke-start contract.
//!   `load_mesh` mid-stroke does not reset the snapshot (the stroke ends
//!   with the drag, as before).

use egui::{Color32, PointerButton, Response, Sense, Ui};
// The lib owns the projection module (row 278); main.rs re-exports it
// (row 281), so the path names the lib directly — the same types either way.
use umber_app::projection::{
    overlay_quad, rasterize_projection, rasterize_projection_live, ProjectionState,
};
use umber_color::{apply_display_chain, DisplaySettings};
use umber_core::stencil::{forward_map, StencilImage, StencilTransform};
use umber_gpu::postfx::PostChain;
use umber_gpu::renderer::PaintParams;
use umber_gpu::{
    DisplayLut, EnvIrradiance, GpuContext, MeshBuffers, OrbitCamera, PostFxViewport, SoloChannel,
    ViewMode, ViewportRoute,
};
use umber_mesh::tile_of_uv;

use crate::i18n::tr;
use crate::paint_state::PaintState;
use crate::set_switch::restage_stencil;
use crate::stencil::StencilState;

/// Background shown when there is no mesh loaded (and behind the mesh
/// otherwise, since the shared egui render pass has no clear op of its
/// own — see `GpuContext::new` docs on why there's no depth/clear here).
const EMPTY_VIEWPORT_COLOR: Color32 = Color32::from_rgb(28, 28, 30);

/// Degrees-per-pixel for drag-orbit; chosen so a full-width drag on a
/// typical viewport is roughly one full turn.
const ORBIT_SENSITIVITY: f32 = 0.01;
/// Fraction of the viewport's visible size a full-width pan drag covers.
const PAN_SENSITIVITY: f32 = 0.0015;
/// Zoom step per wheel notch (multiplicative, see `OrbitCamera::zoom`).
const ZOOM_SENSITIVITY: f32 = 0.08;

/// One frame of the RMB-cancel bookkeeping (Painter: RMB cancels the
/// running nav drag): arms on a held RMB beside a dragged non-RMB
/// button, holds while the cancelled button stays down, clears on its
/// release.
fn rmb_cancel_step(
    prev: Option<PointerButton>,
    dragged: Option<PointerButton>,
    rmb_down: bool,
    cancelled_down: bool,
) -> Option<PointerButton> {
    match (rmb_down, dragged) {
        (true, Some(b)) if b != PointerButton::Secondary => Some(b),
        _ if cancelled_down => prev,
        _ => None,
    }
}

/// One [ / ] brush-size step: x1.25 to grow, x0.8 to shrink (the clamp
/// rides `set_brush_size`'s `sanitize_size`).
fn bracket_size_step(current: f32, grow: bool) -> f32 {
    current * if grow { 1.25 } else { 0.8 }
}

/// Pixels of horizontal drag per alpha doubling (Ctrl+LMB flow chord).
const FLOW_PX_PER_DOUBLING: f32 = 200.0;
/// The angle chord's sweep: one full -180..=180 deg turn per this many px
/// of vertical drag.
const ANGLE_PX_PER_FULL_SWEEP: f32 = 400.0;

/// The flow chord's alpha: `alpha0 * 2^(dx / FLOW_PX_PER_DOUBLING)`,
/// clamped to 0.01..=1 (matches the preset editor's alpha field).
fn flow_alpha_step(alpha0: f32, dx_px: f32) -> f32 {
    (alpha0 * 2f32.powf(dx_px / FLOW_PX_PER_DOUBLING)).clamp(0.01, 1.0)
}

/// The angle chord's degrees: linear in the vertical drag, clamped to the
/// preset editor's -180..=180 deg range.
fn angle_step(angle0_deg: f32, dy_px: f32) -> f32 {
    // egui's dy is screen-down-positive; dragging UP (negative dy) sweeps the
    // stamp angle positive, matching the chord's test contract.
    (angle0_deg - dy_px * 360.0 / ANGLE_PX_PER_FULL_SWEEP).clamp(-180.0, 180.0)
}

/// The cursor's position in NDC (x right ∈ [-1,1], y UP ∈ [-1,1]) inside
/// `rect` — the anchor for zoom-toward-cursor (egui's y is down, NDC's
/// is up, hence the flip).
fn cursor_ndc(rect: egui::Rect, pos: egui::Pos2) -> egui::Vec2 {
    egui::vec2(
        (pos.x - rect.center().x) / (rect.width() * 0.5),
        (rect.center().y - pos.y) / (rect.height() * 0.5),
    )
}
/// Dolly per pixel of a zoom DRAG (Painter's Alt+RMB), as a fraction of
/// the distance: 200 px of drag ≈ the wheel's 8 % notch, twenty times over.
const ZOOM_DRAG_SENSITIVITY: f32 = 0.004;

/// An eyedropper sample (C1 worklist #10): the display-chain sRGBA bytes
/// the viewport read from the paint target at the clicked UV, one per Pick
/// click. The texel is linear RGBA8; the sample runs it through
/// [`linear_to_srgb_byte`] and then the session's viewer chain
/// ([`display_chain_srgba`]), so it carries what the artist actually sees
/// on the mesh. Carried to the shell so it can set the brush color (and
/// later: the alpha, masks, anything color-keyed).
pub struct PickSample(pub [u8; 4]);

/// What a strip-only tool's click resolved to (C1 worklist #10, extended
/// by #11): the viewport never acts on these itself — the shell matches
/// on the variant and routes through the paint session.
pub enum ViewportClick {
    /// The eyedropper's display-chain sRGBA sample at the picked UV.
    Pick(PickSample),
    /// The ID-mask select's click UV (mesh space, `0..1` frame): the shell
    /// bakes the region IDs and selects through the paint session.
    IdMask([f32; 2]),
}

/// One LINEAR u8 texel component to its sRGB byte (the eyedropper's
/// per-texel conversion; matches umber-color's private `linear_to_srgb`
/// math — the standard 0.0031308-threshold curve). Alpha passes through.
fn linear_to_srgb_byte(linear: u8) -> u8 {
    let l = linear as f32 / 255.0;
    let s = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (s.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The eyedropper's viewer-chain step: [`linear_to_srgb_byte`]'s RGB
/// channels run through the session's display chain and quantize back to
/// display bytes — the same [`apply_display_chain`] call the Display
/// panel's preview strip makes on `PREVIEW_RAMP`
/// (`display_panel::preview_values`). The input is the sRGB byte already
/// produced from the linear texel (the v1 sample), so the identity chain
/// (`DisplaySettings::default()`: Raw / 0 EV / gamma 1) returns those
/// bytes unchanged; alpha passes through.
fn display_chain_srgba(srgba: [u8; 4], settings: &DisplaySettings) -> [u8; 4] {
    let unit = |b: u8| b as f32 / 255.0;
    let [r, g, b] = apply_display_chain([unit(srgba[0]), unit(srgba[1]), unit(srgba[2])], settings);
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    [byte(r), byte(g), byte(b), srgba[3]]
}

/// An in-flight Ctrl+RMB brush-resize drag (see `Viewport::resize`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct ResizeDrag {
    size: f32,
    hardness: f32,
    origin: egui::Pos2,
}

/// Owns the camera and the currently-loaded mesh's GPU buffers.
#[derive(Default)]
pub struct Viewport {
    camera: OrbitCamera,
    mesh: Option<MeshBuffers>,
    /// CPU-side copy of the loaded mesh for ray picking (paint mode).
    mesh_data: Option<umber_mesh::MeshData>,
    /// The bound environment (`None` = procedural fallback). Kept here
    /// so `load_mesh` (which rebuilds the GPU buffers from scratch)
    /// re-applies it instead of silently dropping back to procedural.
    env: Option<EnvIrradiance>,
    /// Barycentric-edge wireframe overlay (View menu / W). Off by
    /// default: the off path draws nothing extra and renders
    /// byte-identical to the mesh-only frame.
    show_wireframe: bool,
    /// Procedural ground grid (View menu / G). Off by default, same
    /// byte-identical-off contract.
    show_grid: bool,
    /// Cage wireframe overlay (§6:59, synced from the Bakes panel's cage
    /// toggle by the app shell, like wireframe/grid). Off by default with
    /// the same byte-identical-off contract. V1 reuses the wireframe
    /// pass's mesh edges in amber (a true cage buffer — mesh inflated
    /// along normals by the cage distance — is the named follow-up); the
    /// toggle, ordering (after the mesh, with the wireframe), and overlay
    /// plumbing are the shipped surface.
    show_cage: bool,
    /// The shading mode (View menu; V cycles lit ↔ unlit). Defaults
    /// to Lit — today's render, byte-identical with the mode uniform
    /// at (0, 0).
    view_mode: ViewMode,
    /// The post-effects path (§7:84: mesh → HDR target → chain → LUT),
    /// built on the first frame the chain routes there and kept for its
    /// pipelines; its HDR target and intermediates are released on every
    /// direct frame. `None` until then — the default path never builds it.
    postfx: Option<PostFxViewport>,
    /// The eyedropper arm (C1 worklist #10): set by the shell when
    /// ToolKind::Pick is the active tool. While armed, a primary click
    /// samples the paint target at the picked UV instead of orbiting
    /// or painting; the sample returns to the shell as [`PickSample`].
    pick_armed: bool,
    /// The session's viewer chain (the Display panel's
    /// [`DisplaySettings`]), synced from the shell each frame beside the
    /// picker arm. The eyedropper runs each sampled texel through it
    /// ([`display_chain_srgba`]) so the pick matches the displayed mesh;
    /// the default settings are the identity chain, so the v1 sample is
    /// byte-for-byte unchanged until the artist edits the Display panel.
    display_settings: DisplaySettings,
    /// The ID-mask arm (C1 worklist #11): set by the shell when
    /// ToolKind::IdMask is the active tool. While armed, a primary click
    /// resolves to the mesh UV instead of orbiting or painting; the UV
    /// returns to the shell (as [`ViewportClick::IdMask`]), which bakes
    /// the region IDs and selects through the paint session.
    idmask_armed: bool,
    /// Front-face falloff start angle, degrees (absorbed from Blender's
    /// brush Normal Falloff): viewport strokes on faces seen more than this
    /// far from head-on paint weaker, down to nothing edge-on. `None` = off
    /// — an `Option` so the derived `Default` is OFF (a bare `f32` would
    /// default to 0.0 = fully ON and silently weaken every stroke). Strokes
    /// are byte-identical to before until the shell enables it
    /// ([`Self::set_front_face_falloff_deg`]).
    front_face_falloff_deg: Option<f32>,
    /// The navigation scheme (`crate::navigation`): Painter's Alt+mouse by
    /// default, `UMBER_NAV` to choose until the preferences window lands.
    nav: crate::navigation::NavPreset,
    /// Wheel-zoom inversion (Preferences ▸ Navigation).
    wheel_invert: bool,
    /// The nav drag button cancelled by a held RMB (Painter's
    /// right-click-cancels), cleared when that button is released.
    nav_cancel: Option<PointerButton>,
    /// The Ctrl+LMB flow chord's live alpha handed up to the shell this
    /// frame (the preset owns alpha; the viewport owns no brush state).
    pub live_flow_alpha: Option<f32>,
    /// Live stamp angle (deg) from the Ctrl+LMB chord's vertical drag.
    pub live_stamp_angle: Option<f32>,
    /// The in-flight Ctrl+LMB flow drag: the pointer origin and the alpha
    /// at press.
    flow_drag: Option<(egui::Pos2, f32, f32)>,
    /// A Ctrl+RMB brush-resize drag in flight: the brush size and
    /// hardness at its start and the pointer origin (GUI plan G1
    /// g1-brush-resize). The ring stays at the origin while it runs.
    resize: Option<ResizeDrag>,
    /// Whether the axis gizmo draws (View menu; GUI plan G1
    /// `g1-axis-gizmo`). Shown by default.
    show_gizmo: bool,
    /// The in-flight paint stroke was begun by THIS viewport (so the
    /// viewport ends it on release, and never ends a UV-view stroke).
    stroke_from_viewport: bool,
    /// The live stroke's FROZEN camera (pwys-stroke-buffer option A): a
    /// plain-data copy of [`Self::camera`] latched when the stroke begins
    /// (the first event that opens a stroke) and cleared when the paint
    /// drag ends. Every later event of the stroke raycasts against it, so
    /// the live camera may move without disturbing the stroke in flight.
    /// `None` between strokes (and while a drag that opened on a miss has
    /// not hit yet).
    stroke_camera: Option<OrbitCamera>,
    /// A gizmo tip took this frame's click (the straight line, the
    /// eyedropper and the ID-mask must not also act on it).
    gizmo_clicked: bool,
    /// Where the last viewport stroke ended, in SCREEN space (Painter's
    /// straight line is screen-space too, with the same caveat: move the
    /// camera and the anchor no longer sits on the old stroke end).
    last_stroke_end: Option<egui::Pos2>,
    /// The home card's click this frame (GUI plan `g5-home-screen`): set
    /// by [`Self::ui_with_home`] while no mesh is loaded, drained by the
    /// shell through [`Self::take_home_action`].
    home_action: Option<crate::empty_state::HomeCardAction>,
    /// The Overlays popover's state (GUI plan `g6-overlays-popover`):
    /// wireframe/grid mirrored from the shell, the rest owned here.
    overlays: crate::display_panel::OverlaySettings,
    /// A popover edit not yet taken by the shell
    /// ([`Self::take_overlay_edit`]).
    overlay_edited: bool,
    /// The loaded mesh's UV-seam segments, built on the first frame the
    /// seam overlay shows and dropped on every `load_mesh`.
    seam_cache: Option<Vec<SeamSegment>>,
    /// An S-chord stencil drag in flight (latched at its press; see the
    /// module docs' stencil section).
    stencil_drag: Option<StencilDrag>,
    /// An S-chord PROJECTION drag in flight (projection-fill row 280: the
    /// same chord-shaped latch, editing the armed projection's transform;
    /// never `Some` alongside [`Self::stencil_drag`]).
    projection_drag: Option<StencilDrag>,
    /// The stencil overlay's texture and the image it was built from.
    stencil_texture: Option<(StencilTextureKey, egui::TextureHandle)>,
}

/// The shell state the viewport reads beyond the GPU / paint inputs (one
/// struct so [`Viewport::ui_with_stencil`] stays within clippy's argument
/// count).
pub struct ViewportExtras<'a> {
    /// The home card's input (see [`Viewport::ui_with_home`]).
    pub home: Option<crate::empty_state::HomeCardInput<'a>>,
    /// The session's stencil: drawn, chord-edited and hosted in the
    /// Overlays popover. `None` = no stencil at all.
    pub stencil: Option<&'a mut StencilState>,
    /// The session's decal projector (projection-fill row 280): while
    /// armed it takes the S-chords and draws its quad. `None` = no
    /// projector (row 281 wires the shell's state).
    pub projection: Option<&'a mut ProjectionState>,
}

/// Which transform an S-chord drag edits (g6-stencil-overlay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StencilChord {
    /// S+LMB: turn around the center; `snap` (Shift) rounds the result
    /// to quarter turns.
    Rotate {
        /// Shift held this frame.
        snap: bool,
    },
    /// S+RMB: uniform scale from the horizontal delta.
    Scale,
    /// S+MMB: move the center.
    Move,
}

impl StencilChord {
    /// The chord an S-held drag with `button` runs (Rotate unsnapped;
    /// the snap is read live each frame).
    pub fn for_button(button: PointerButton) -> Option<Self> {
        match button {
            PointerButton::Primary => Some(Self::Rotate { snap: false }),
            PointerButton::Secondary => Some(Self::Scale),
            PointerButton::Middle => Some(Self::Move),
            _ => None,
        }
    }

    /// The button that keeps this chord's drag alive.
    fn button(self) -> PointerButton {
        match self {
            Self::Rotate { .. } => PointerButton::Primary,
            Self::Scale => PointerButton::Secondary,
            Self::Move => PointerButton::Middle,
        }
    }

    /// This chord with Rotate's snap set to `snap` (the others ignore it).
    fn with_snap(self, snap: bool) -> Self {
        match self {
            Self::Rotate { .. } => Self::Rotate { snap },
            other => other,
        }
    }
}

/// An S-chord drag in flight: everything it maps from, fixed at the
/// press, so a frame never compounds on the last one.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StencilDrag {
    chord: StencilChord,
    /// The stencil's transform at the press.
    start: StencilTransform,
    /// The press, viewport-local px.
    origin: [f32; 2],
}

/// Identifies the image a stencil texture was built from: the coverage
/// buffer's address and length plus the dims. `StencilState::adopt`
/// decodes the new image while the old one is still held, so a reload
/// always arrives at a fresh address.
type StencilTextureKey = (usize, usize, u32, u32);

impl Viewport {
    /// Uploads `mesh` to the GPU and re-frames the camera on its bounds,
    /// keeping the current yaw/pitch so loading a new mesh doesn't spin
    /// the view out from under the user. The current projection
    /// (perspective/orthographic) is kept the same way: `framing` always
    /// builds Perspective, so without this the toggle would reset on every
    /// load.
    pub fn load_mesh(
        &mut self,
        gpu: &GpuContext,
        mesh: &umber_mesh::MeshData,
    ) -> anyhow::Result<()> {
        let mut buffers = MeshBuffers::upload(gpu, mesh)?;
        // Fresh buffers default to procedural — re-apply our env.
        buffers.set_environment(gpu, self.env.as_ref());
        if let Some((min, max)) = mesh.bounds() {
            let projection = self.camera.projection;
            self.camera = OrbitCamera::framing(min, max, self.camera.yaw, self.camera.pitch);
            self.camera.projection = projection;
        }
        self.mesh_data = Some(mesh.clone());
        self.mesh = Some(buffers);
        self.seam_cache = None;
        Ok(())
    }

    /// Swaps the bound environment: `Some(env)` enables the convolved
    /// map, `None` restores the procedural fallback. Applies to the
    /// loaded mesh immediately (and is remembered for the next
    /// `load_mesh`).
    pub fn set_environment(&mut self, gpu: &GpuContext, env: Option<EnvIrradiance>) {
        self.env = env;
        if let Some(mesh) = self.mesh.as_mut() {
            mesh.set_environment(gpu, self.env.as_ref());
        }
    }

    /// Shows or hides the wireframe overlay (View menu `Show Wireframe`,
    /// W). Synced from the app shell each frame before `ui`.
    pub fn set_show_wireframe(&mut self, on: bool) {
        self.show_wireframe = on;
        self.overlays.wireframe = on;
    }

    /// Shows or hides the ground grid (View menu `Show Grid`, G).
    /// Synced from the app shell each frame before `ui`.
    pub fn set_show_grid(&mut self, on: bool) {
        self.show_grid = on;
        self.overlays.grid = on;
    }

    /// The Overlays popover's current state (test-reached; the shell
    /// consumes the edit through [`Self::take_overlay_edit`]).
    #[allow(dead_code)]
    pub fn overlays(&self) -> crate::display_panel::OverlaySettings {
        self.overlays
    }

    /// Replaces the overlay state wholesale (a project/preference load,
    /// or a test). Wireframe/grid follow it like their own setters.
    /// (Test-reached; the shell consumes edits through
    /// [`Self::take_overlay_edit`].)
    #[allow(dead_code)]
    pub fn set_overlays(&mut self, overlays: crate::display_panel::OverlaySettings) {
        self.overlays = overlays;
        self.overlays.wireframe_opacity = overlays.wireframe_opacity.clamp(0.0, 1.0);
        self.show_wireframe = overlays.wireframe;
        self.show_grid = overlays.grid;
    }

    /// The popover's edit since the last take, for the shell to store
    /// its wireframe/grid back into `ViewToggles` (otherwise the shell's
    /// next-frame sync would undo the popover's toggle). `None` when the
    /// popover changed nothing.
    // `dead_code`: the shell's sync calls this once main.rs wires the
    // popover (named in the g6-overlays-popover kill report).
    #[allow(dead_code)]
    pub fn take_overlay_edit(&mut self) -> Option<crate::display_panel::OverlaySettings> {
        std::mem::take(&mut self.overlay_edited).then_some(self.overlays)
    }

    /// Sets the face-orientation overlay flag (the popover's row; the
    /// tint itself is renderer follow-up — see the module docs).
    // `dead_code`: the shell's sync calls this once main.rs wires the
    // popover (named in the g6-overlays-popover kill report).
    #[allow(dead_code)]
    pub fn set_face_orientation(&mut self, on: bool) {
        self.overlays.face_orientation = on;
    }

    /// Whether the face-orientation overlay is requested.
    // `dead_code`: the shell's sync calls this once main.rs wires the
    // popover (named in the g6-overlays-popover kill report).
    #[allow(dead_code)]
    pub fn face_orientation(&self) -> bool {
        self.overlays.face_orientation
    }

    /// Shows or hides the cage wireframe overlay (Bakes panel `Show
    /// cage`). Synced from the app shell each frame before `ui`, like the
    /// wireframe/grid toggles.
    pub fn set_show_cage(&mut self, on: bool) {
        self.show_cage = on;
    }

    /// Whether the cage overlay shows.
    pub fn show_cage(&self) -> bool {
        self.show_cage
    }

    /// Sets the shading mode (View menu rows / V shortcut; synced from
    /// the app shell each frame before `ui`, like the overlay toggles).
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
    }

    /// The current shading mode.
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Draws the viewport and handles orbit/pan/zoom/paint input.
    ///
    /// Paint mode (§2's P1 row, [`crate::paint_state::PaintProjection`]):
    /// Ctrl/Cmd (or pen-tip with `ctrl`) + primary drag casts the pointer
    /// through the camera into the mesh and feeds the hit UV to `paint`'s
    /// Screen-projection path — the stroke you SEE is the stroke you get
    /// (misses skip the stamp, seam mirrors fan out from the hit). Plain
    /// drag orbits; shift-drag pans. In `Uv` projection the viewport never
    /// paints (Ctrl+drag orbits); UV-space editing owns the stroke there.
    ///
    /// `display_lut` is the app's display LUT (the Display panel's
    /// chain), applied as the mesh pass's last step; `None` draws
    /// through the identity table.
    ///
    /// `postfx` is the post-effects chain: with an active effect (and a
    /// display view mode — [`ViewportRoute::select`]) the mesh renders
    /// scene-linear offscreen, the chain runs, and the LUT is applied by
    /// the composite instead; otherwise the direct path above, unchanged.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        gpu: &GpuContext,
        paint: Option<&mut crate::paint_state::PaintState>,
        stylus_pressure: Option<f32>,
        display_lut: Option<&DisplayLut>,
        postfx: &PostChain,
    ) -> Option<ViewportClick> {
        self.ui_with_home(
            ui,
            gpu,
            paint,
            stylus_pressure,
            display_lut,
            postfx,
            None,
            None,
        )
    }

    /// [`Self::ui`] with the home card's shell state (GUI plan
    /// `g5-home-screen`): while no mesh is loaded the empty state is the
    /// home card ([`crate::empty_state::home_card`]), fed the recent list
    /// and the pending autosave snapshot from `home`. `None` draws the
    /// card with New / Open only. The card's click (or a double-click on
    /// the empty viewport = Open) is kept for [`Self::take_home_action`],
    /// so the strip-tool return value stays one `ViewportClick` per frame.
    /// With `stencil`, the session's stencil reaches the viewport too
    /// (the overlay draws, the S chords and N edit it, and the Overlays
    /// popover hosts its rows).
    #[allow(clippy::too_many_arguments)]
    pub fn ui_with_home(
        &mut self,
        ui: &mut Ui,
        gpu: &GpuContext,
        paint: Option<&mut crate::paint_state::PaintState>,
        stylus_pressure: Option<f32>,
        display_lut: Option<&DisplayLut>,
        postfx: &PostChain,
        home: Option<crate::empty_state::HomeCardInput<'_>>,
        stencil: Option<&mut StencilState>,
    ) -> Option<ViewportClick> {
        let extras = ViewportExtras {
            home,
            stencil,
            projection: None,
        };
        self.ui_with_stencil(ui, gpu, paint, stylus_pressure, display_lut, postfx, extras)
    }

    /// [`Self::ui_with_home`] plus the session's stencil
    /// (`extras.stencil`; see the module docs' stencil section): the
    /// overlay quad draws, the S chords edit its transform (re-staging
    /// the gate on release), and the Overlays popover hosts its rows.
    #[allow(clippy::too_many_arguments)]
    pub fn ui_with_stencil(
        &mut self,
        ui: &mut Ui,
        gpu: &GpuContext,
        mut paint: Option<&mut crate::paint_state::PaintState>,
        stylus_pressure: Option<f32>,
        display_lut: Option<&DisplayLut>,
        postfx: &PostChain,
        extras: ViewportExtras<'_>,
    ) -> Option<ViewportClick> {
        let ViewportExtras {
            home,
            mut stencil,
            mut projection,
        } = extras;

        // THE TILE OBSERVER (the cross-tile-stroke residual seam):
        // umber-paint's `push_event_styled` moves `active_tile` when a
        // stroke crosses into another tile (paint_state.rs:4694-4697),
        // but the staged gate keeps the OLD tile's dims; `gate_plane`
        // skips composing on a dims mismatch, so the new tile's dabs
        // paint stencil-ungated until something re-stages. Watch the
        // gate's dims (`staged_for`, set only on an adopted hand-off)
        // against paint's current tile's quick-mask dims, and re-stage on
        // the first frame they differ. Frame-cheap: the watch is a dims
        // compare; the re-stage only fires on a size-change frame. The
        // tile id is deliberately not part of the compare: the hook's
        // bytes are a pure function of the dims, so a same-size tile
        // change needs no re-hand-off — and comparing the id would call
        // `set_stencil_gate`, whose identical-value no-op returns `false`
        // without refreshing the record, re-staging every frame.
        //
        // The mid-stroke window stays BY DESIGN: one stroke never changes
        // its gate partway (the snapshot contract, `set_stencil_gate`'s
        // own refusal). After the stroke ends, the next frame's observer
        // closes the gap.
        if let (Some(stencil), Some(p)) = (stencil.as_deref_mut(), paint.as_deref_mut()) {
            if stencil.active && stencil.image.is_some() && !p.is_stroking() {
                let tile = p.active_tile();
                let (w, h) = p.quick_mask_for_tile(tile).dimensions();
                let staged_dims = stencil.staged_for.map(|(_, sw, sh)| (sw, sh));
                if staged_dims != Some((w, h)) {
                    restage_stencil(&mut *stencil, Some(&mut *p));
                }
            }
        }

        let rect = ui.available_rect_before_wrap();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.gizmo_clicked = false;

        // The stencil chords (S + LMB / RMB / MMB, the module docs): run
        // before every other click/drag consumer, which all stand down
        // while the chord owns the pointer. An armed projection takes the
        // same chords (row 280); `stencil_owns` is "a chord owns the
        // pointer", whichever target it edits.
        let chords = ChordFrame::read(ui, &response, rect, self.mesh.is_some());
        let stencil_owns = self.chord_input(
            &chords,
            stencil.as_deref_mut(),
            projection.as_deref_mut(),
            paint.as_deref_mut(),
        );

        // The axis gizmo (GUI plan G1): a primary click on a tip snaps the
        // view down that axis (again: the opposite side). It takes the
        // click before painting, the straight line or the strip tools.
        let gizmo_tips = crate::axis_gizmo::project_axes(
            self.camera.yaw,
            self.camera.pitch,
            crate::axis_gizmo::RADIUS,
        );
        let gizmo_centre = crate::axis_gizmo::centre(rect);
        let gizmo_hover = self.show_gizmo.then(|| {
            response
                .hover_pos()
                .and_then(|p| crate::axis_gizmo::hit(&gizmo_tips, p - gizmo_centre))
        });
        if self.mesh.is_some()
            && self.show_gizmo
            && !stencil_owns
            && response.clicked_by(PointerButton::Primary)
        {
            if let Some(axis) = response
                .interact_pointer_pos()
                .and_then(|p| crate::axis_gizmo::hit(&gizmo_tips, p - gizmo_centre))
            {
                let target =
                    crate::axis_gizmo::click_target(axis, self.camera.yaw, self.camera.pitch);
                let (yaw, pitch) = crate::axis_gizmo::snap_angles(target, self.camera.yaw);
                self.camera.yaw = yaw;
                self.camera.pitch = pitch;
                self.gizmo_clicked = true;
            }
        }

        // The eyedropper (C1 worklist #10): when armed, a PRIMARY CLICK (not
        // a drag — a stray orbit still works) samples the paint target's
        // texel at the picked UV. Returns the sRGBA sample for the shell to
        // feed the brush; painting is disabled while the eyedropper is the
        // active tool (it never strokes).
        if self.pick_armed
            && !self.gizmo_clicked
            && !stencil_owns
            && response.clicked_by(PointerButton::Primary)
        {
            if let (Some(mesh), Some(paint)) = (self.mesh_data.as_ref(), paint.as_mut()) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(uv) = self.pick_uv(rect, mesh, pos) {
                        // Read the texel from the tile the UV routes to: the
                        // paint target holds LINEAR RGBA8, the shell wants
                        // sRGB bytes — convert per-texel, then run the
                        // session's display chain over those bytes so the
                        // pick matches the displayed mesh (the Display
                        // panel's preview strip applies the SAME chain to the
                        // same `DisplaySettings`). The default settings are
                        // the identity chain, so the v1 no-LUT sample is
                        // unchanged.
                        let tile = tile_of_uv(uv);
                        if let Some(image) = paint.read_tile(tile) {
                            let x = ((uv[0].clamp(0.0, 1.0)) * (image.width as f32 - 1.0)).round()
                                as u32;
                            let y = ((uv[1].clamp(0.0, 1.0)) * (image.height as f32 - 1.0)).round()
                                as u32;
                            let i = (y * image.width + x) as usize * 4;
                            if i + 3 < image.data.len() {
                                let srgba = display_chain_srgba(
                                    [
                                        linear_to_srgb_byte(image.data[i]),
                                        linear_to_srgb_byte(image.data[i + 1]),
                                        linear_to_srgb_byte(image.data[i + 2]),
                                        image.data[i + 3],
                                    ],
                                    &self.display_settings,
                                );
                                return Some(ViewportClick::Pick(PickSample(srgba)));
                            }
                        }
                    }
                }
            }
        }

        // The ID-mask select (C1 worklist #11): when armed, a PRIMARY
        // CLICK (not a drag — a stray orbit still works) resolves to the
        // mesh UV. The shell bakes the region IDs and selects through the
        // paint session; painting is disabled while armed (it never
        // strokes). A miss falls through to orbit, like the eyedropper.
        if self.idmask_armed
            && !self.gizmo_clicked
            && !stencil_owns
            && response.clicked_by(PointerButton::Primary)
        {
            if let (Some(mesh), Some(_)) = (self.mesh_data.as_ref(), paint.as_ref()) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(uv) = self.pick_uv(rect, mesh, pos) {
                        return Some(ViewportClick::IdMask(uv));
                    }
                }
            }
        }

        // Ctrl+RMB drag resizes the brush (Painter's chord): horizontal =
        // size, vertical = hardness, relative to the drag origin. It is
        // never navigation under any preset (`navigation.rs` binds RMB
        // only with Alt), so it is handled before the camera.
        let resize_chord = !stencil_owns
            && ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && !i.modifiers.alt)
            && response.dragged_by(PointerButton::Secondary);
        match (resize_chord, paint.as_mut()) {
            (true, Some(p)) => {
                let origin = response.interact_pointer_pos().unwrap_or(rect.center());
                let drag = *self.resize.get_or_insert(ResizeDrag {
                    size: p.brush_size(),
                    hardness: p.brush_hardness(),
                    origin,
                });
                let now = response.interact_pointer_pos().unwrap_or(drag.origin);
                let (size, hardness) =
                    crate::brush_cursor::resize_drag((drag.size, drag.hardness), now - drag.origin);
                p.set_brush_size(size);
                p.set_brush_hardness(hardness);
            }
            _ => self.resize = None,
        }

        // Ctrl+LMB drag sets the flow/alpha (Painter's chord): horizontal
        // delta scales alpha exponentially; vertical delta sweeps the stamp
        // angle linearly. Both are handed up to the shell — the preset owns
        // them. Same never-nav rule as the resize chord (LMB binds only
        // with Alt).
        let flow_chord = !stencil_owns
            && ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && !i.modifiers.alt)
            && response.dragged_by(PointerButton::Primary);
        if flow_chord && paint.is_some() {
            if response.drag_started_by(PointerButton::Primary) {
                let origin = ui
                    .input(|i| i.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos())
                    .unwrap_or(rect.center());
                let (alpha0, angle0) = paint
                    .as_ref()
                    .map(|p| {
                        let params = p.brush_params();
                        let angle = match &params.stamp_angle {
                            umber_brush::StampAngle::Fixed { degrees } => *degrees,
                            umber_brush::StampAngle::FollowStroke { offset_degrees } => {
                                *offset_degrees
                            }
                            _ => 0.0,
                        };
                        (params.alpha, angle)
                    })
                    .unwrap_or((1.0, 0.0));
                self.flow_drag = Some((origin, alpha0, angle0));
            }
            if let Some((origin, alpha0, angle0)) = self.flow_drag {
                let now = response.interact_pointer_pos().unwrap_or(rect.center());
                self.live_flow_alpha = Some(flow_alpha_step(alpha0, now.x - origin.x));
                self.live_stamp_angle = Some(angle_step(angle0, now.y - origin.y));
            }
        } else {
            self.flow_drag = None;
        }

        // [ / ] step the brush size (Painter's chord; hover-gated like the
        // P/F keys, so it never fires while typing in a panel). One step is
        // x1.25 / x0.8 of the current multiplier; set_brush_size clamps.
        let bracket = ui.input(|i| {
            if i.key_pressed(egui::Key::CloseBracket) {
                Some(true)
            } else if i.key_pressed(egui::Key::OpenBracket) {
                Some(false)
            } else {
                None
            }
        });
        if let (Some(grow), Some(p)) = (bracket, paint.as_mut()) {
            p.set_brush_size(bracket_size_step(p.brush_size(), grow));
        }

        // Straight line (Painter C1): Shift held previews a dotted line from
        // the last stroke's end to the pointer (drawn in `paint_line_preview`);
        // Shift+click paints it as one stroke, Ctrl+Shift snaps the angle to
        // 5° steps. Clicks only: a Shift+drag stays a pan under the classic
        // preset.
        if let (Some(from), Some(mesh), Some(p)) = (
            self.last_stroke_end,
            self.mesh_data.as_ref(),
            paint.as_mut(),
        ) {
            let mods = ui.input(|i| i.modifiers);
            if mods.shift
                && !mods.alt
                && !self.pick_armed
                && !self.idmask_armed
                && !p.is_stroking()
                && !self.gizmo_clicked
                && !stencil_owns
                && p.projection().viewport_paints()
                && response.clicked_by(PointerButton::Primary)
            {
                if let Some(to) = response.interact_pointer_pos() {
                    let to = straight_line_end(from, to, mods.ctrl || mods.command);
                    self.paint_screen_line(rect, mesh, p, from, to);
                    self.last_stroke_end = Some(to);
                }
            }
        }

        let painting = !stencil_owns
            && !self.pick_armed
            && !self.idmask_armed
            && ui.input(|i| (i.modifiers.ctrl || i.modifiers.command) && !i.modifiers.alt)
            && response.dragged_by(PointerButton::Primary)
            && self.mesh_data.is_some()
            && paint
                .as_ref()
                .is_some_and(|p| p.projection().viewport_paints());
        // A viewport stroke ENDS when the paint drag does (button up, the
        // modifier released, or a tool arm). Before 2026-10-15 nothing here
        // ended it: `is_stroking()` stayed true after release, so every
        // later Ctrl+drag EXTENDED the previous stroke (one session-long
        // stroke, one undo step, and History never got a row — recording
        // happens in `end_stroke`). The UV view always ended its strokes;
        // the viewport now matches it.
        if let Some(p) = paint.as_mut() {
            if ends_viewport_stroke(painting, p.is_stroking(), self.stroke_from_viewport) {
                p.end_stroke();
            }
        }
        if !painting {
            self.stroke_from_viewport = false;
            // The stroke's frozen camera dies with the stroke (see the
            // module docs' "camera mid-stroke" section).
            self.stroke_camera = None;
        }
        // The guard is per INPUT SOURCE (pwys-stroke-buffer option A): the
        // artist's pointer belongs to a live stroke, so pointer-driven
        // navigation (drag chords, wheel) stands down while painting; the
        // non-pointer camera sources (the P / F keys) stay live, because
        // the stroke raycasts against its start-camera snapshot and never
        // reads the live camera. A stencil chord owns the pointer: no
        // navigation of either kind under it.
        if !stencil_owns {
            if !painting {
                self.handle_pointer_nav(ui, &response);
            }
            self.handle_camera_keys(ui, &response, painting);
        }
        if let (true, Some(paint), Some(pos)) =
            (painting, paint.as_mut(), response.interact_pointer_pos())
        {
            // CORE-LOOP CRITICAL 5 — real pressure: the digitizer's
            // per-event force feeds the stroke. The source is the RAW
            // `Event::Touch` stream, NOT `multi_touch()`: the gesture
            // aggregate returns `None` below two active touches
            // (egui's own contract — "returns None for single-touch
            // gestures"), so a single pen contact never carried force
            // through it and painted at a constant 1.0 (the cross-review
            // finding on f867215, receipt 2026-10-12-pressure-kill-
            // cross-review-receipt.md). `Event::Touch` carries
            // `force: Option<f32>` per event, single touches included,
            // in 0..=1; `None`/0.0 maps to 1.0 — a mouse drag (no touch
            // events at all) and a platform that cannot measure force
            // both paint at full pressure, never a ghost-faint stroke.
            // V1 takes the frame's LAST touch event's force (the
            // single-pen case; per-touch-id tracking rides with the
            // stylus-crate backends — Windows Ink, XI2 — the named
            // follow-up for tilt, proximity, eraser).
            //
            // STYLUS (XI2) PREFERENCE: `stylus_pressure` is this
            // frame's Motion pressure from the poller
            // (`stylus_input::StylusPoller::pressure_for_frame` —
            // `Some` only when a stylus Motion drained THIS frame).
            // It wins over Touch; Touch wins over 1.0; the shared
            // `resolve_pressure` folds non-positive/non-finite to
            // 1.0 (the mouse / can't-measure-force rule, unchanged).
            let touch_force = ui.input(|i| {
                let mut force: Option<f32> = None;
                for event in &i.events {
                    if let egui::Event::Touch {
                        phase: egui::TouchPhase::Start | egui::TouchPhase::Move,
                        pos,
                        force: f,
                        ..
                    } = event
                    {
                        // Only the touch stream inside this viewport
                        // feeds the stroke: a second finger landing on
                        // a panel mid-stroke must not override the
                        // pen's force for the frame.
                        if rect.contains(*pos) {
                            force = *f;
                        }
                    }
                }
                force
            });
            // Front-face falloff rides the pressure channel: it scales
            // the dab's strength exactly as a lighter pen would, so it
            // inherits every downstream rule (pressure curve, wash,
            // symmetry) with no new paint-engine surface. Off (the
            // default) the weight is exactly 1.0 — pressure unchanged.
            // The facing weight is applied inside `stroke_event`, from
            // the same frozen-camera pick that resolves the UV.
            let pressure = crate::stylus_input::resolve_pressure(stylus_pressure, touch_force);
            self.stroke_event(rect, paint, pos, pressure);
        }

        if self.mesh.is_none() {
            crate::empty_state::paint_backdrop(ui.painter(), rect);
        } else {
            ui.painter().rect_filled(rect, 0.0, EMPTY_VIEWPORT_COLOR);
        }
        // The empty state (Phase 0b, GUI plan g5-home-screen): the home
        // card until a mesh loads, lit while a file drag hovers the
        // window. Its widgets are added after the viewport's own response,
        // so they take their clicks first; a double-click that reaches the
        // viewport (empty space) is Open.
        if self.mesh.is_none() {
            let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
            let card = crate::empty_state::home_card(ui, rect, hovering, home);
            let action = crate::empty_state::resolve_action(card, response.double_clicked());
            if action.is_some() {
                self.home_action = action;
            }
        }

        let route = match self.mesh {
            Some(_) => ViewportRoute::select(postfx, self.view_mode),
            None => ViewportRoute::Direct,
        };
        // Orthographic frames stay Direct even with an active chain: DoF's
        // depth linearization is the perspective formula (see the module
        // docs), so the PostFx route would defocus by the wrong distance.
        let route = if self.camera.projection == umber_gpu::camera::Projection::Orthographic {
            ViewportRoute::Direct
        } else {
            route
        };
        // Direct frame: drop the HDR target + the chain's intermediates
        // (last frame's callbacks have already run; a no-op once empty).
        if route == ViewportRoute::Direct {
            if let Some(post) = &self.postfx {
                post.release();
            }
        }

        if let Some(mesh) = self.mesh.as_mut() {
            profiling::scope!("display_pass");
            // CORE-LOOP bind (see the module docs): the active paint tile
            // becomes the mesh pass's base-color texture before EITHER
            // route's callback is built, so paint shows in 3D the frame it
            // lands. No session / no target clears to the constant arm.
            bind_paint_target(gpu, mesh, paint.as_deref());
            let aspect = rect.width() / rect.height();
            let view_proj = self.camera.view_proj(aspect);
            // Normalized once here (review #6): the shader re-normalizes
            // per-fragment defensively, but the uniform arrives clean.
            let light_dir = glam::Vec3::new(-0.4, -1.0, -0.3).normalize();
            let uniform = umber_gpu::CameraUniform::new(view_proj, light_dir);
            // Overlay order (Wave-4 item 7): grid BEFORE the mesh so
            // geometry occludes the reference plane, wireframe AFTER so
            // its LessEqual edges win. Either toggle off adds no shape
            // at all — the off frame is byte-identical to mesh-only.
            if self.show_grid {
                let grid = gpu.grid_callback(view_proj);
                ui.painter().add(umber_gpu::grid_paint_shape(rect, grid));
            }
            let shape = match route {
                ViewportRoute::Direct => {
                    // Data modes (data solos, every mesh-map preview) bind
                    // the identity table here — the §9 display-transform
                    // bypass (see `ViewMode::select_lut`).
                    let mode = self.view_mode;
                    let lut = mode.select_lut(display_lut);
                    let callback = mesh.paint_callback_with_mode(gpu, uniform, lut, mode);
                    umber_gpu::mesh_paint_shape(rect, callback)
                }
                ViewportRoute::PostFx => {
                    // Same slot as the mesh shape: after the grid (which
                    // shows through the composite's alpha), before the
                    // overlays (which test against its written depth).
                    let build = || PostFxViewport::new(gpu);
                    let post = self.postfx.get_or_insert_with(build);
                    let callback = post.callback(
                        gpu,
                        &*mesh,
                        uniform,
                        self.view_mode,
                        display_lut,
                        postfx,
                        rect,
                        (self.camera.near, self.camera.far),
                    );
                    umber_gpu::postfx_paint_shape(rect, callback)
                }
            };
            ui.painter().add(shape);
            // UV seams (g6-overlays-popover): after the mesh, BEFORE the
            // wireframe so wire edges draw over them. Off adds no shape.
            if self.overlays.uv_seams {
                if let Some(data) = self.mesh_data.as_ref() {
                    let segments = self
                        .seam_cache
                        .get_or_insert_with(|| uv_seam_segments(data));
                    paint_seams(
                        &ui.painter().with_clip_rect(rect),
                        rect,
                        view_proj,
                        &self.camera,
                        segments,
                    );
                }
            }
            if self.show_wireframe {
                let color = wire_color_for(self.overlays.wireframe_opacity);
                let wire = gpu.wire_callback(&*mesh, uniform, color);
                ui.painter().add(umber_gpu::wire_paint_shape(rect, wire));
            }
            // Cage overlay (§6:59): after the wireframe so its edges win,
            // skipped entirely when off (byte-identical to cage-off).
            if self.show_cage {
                let cage = gpu.wire_callback(&*mesh, uniform, umber_gpu::WireColor::cage());
                ui.painter().add(umber_gpu::wire_paint_shape(rect, cage));
            }
        }
        // The stencil overlay (g6-stencil-overlay): the overlays family's
        // slot — after the wire/cage callbacks, before the tools' marks.
        if let (true, Some(s)) = (self.mesh.is_some(), stencil.as_deref()) {
            self.paint_stencil_overlay(ui, rect, s);
        }
        // The armed projector's quad (row 280): the same slot, after the
        // stencil so the decal being placed reads on top.
        if let (true, Some(p)) = (self.mesh.is_some(), projection.as_deref()) {
            self.paint_projection_overlay(ui, rect, p);
        }
        // The brush cursor (GUI plan G1): the ring at the brush's on-screen
        // radius, over everything above. Drawn last so no overlay hides it.
        self.paint_line_preview(ui, &response, rect);
        if self.mesh.is_some() && self.show_gizmo {
            crate::axis_gizmo::paint(
                &ui.painter().with_clip_rect(rect),
                gizmo_centre,
                &gizmo_tips,
                gizmo_hover.flatten(),
            );
        }
        self.paint_brush_cursor(ui, &response, rect, paint.as_deref());
        self.paint_channel_readout(ui, rect);
        self.overlays_header(ui, rect, stencil, paint);
        // No strip-tool click this frame: the signature is Option so the
        // shell reads one click per primary press, `None` every other frame.
        None
    }

    /// The Overlays button at the viewport's top-right and its popover
    /// (GUI plan `g6-overlays-popover`). Added after the viewport's own
    /// response, so it takes its clicks first (the home card's rule).
    /// With a `stencil`, its rows follow the overlay rows.
    fn overlays_header(
        &mut self,
        ui: &mut Ui,
        rect: egui::Rect,
        stencil: Option<&mut StencilState>,
        paint: Option<&mut PaintState>,
    ) {
        let size = egui::vec2(OVERLAYS_BUTTON_WIDTH, ui.spacing().interact_size.y);
        let at = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - OVERLAYS_MARGIN - size.x,
                rect.top() + OVERLAYS_MARGIN,
            ),
            size,
        );
        // No mesh: the home card owns the viewport, overlays have nothing
        // to draw on. Too small: no room for the header.
        if self.mesh.is_none() || !rect.contains_rect(at) {
            return;
        }
        let button = ui
            .put(
                at,
                egui::Button::new(tr("overlays.button")).selected(self.overlays.any_on()),
            )
            .on_hover_text(tr("overlays.tooltip"));
        let mut edited = false;
        let overlays = &mut self.overlays;
        egui::Popup::from_toggle_button_response(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                edited = crate::display_panel::overlay_rows(ui, overlays);
                if let Some(stencil) = stencil {
                    ui.separator();
                    stencil_popover_rows(ui, stencil, paint, [rect.width(), rect.height()]);
                }
            });
        if edited {
            self.show_wireframe = self.overlays.wireframe;
            self.show_grid = self.overlays.grid;
            self.overlay_edited = true;
        }
    }

    /// The stencil chords' input block (the module docs' stencil
    /// section): latches an S-held drag at its press, maps every frame
    /// through [`stencil_drag`] from that snapshot into
    /// `stencil.transform`, and re-stages the gate once on release.
    /// Returns whether the stencil owns the pointer this frame (a latched
    /// drag, or S armed on a click), which every other consumer obeys.
    ///
    /// `chords_free` is the projector's routing (row 280): `false` while
    /// an armed projection holds the chords, so no NEW stencil drag
    /// latches (one already in flight still runs to its release, and N
    /// still toggles).
    fn stencil_input(
        &mut self,
        frame: &ChordFrame,
        stencil: Option<&mut StencilState>,
        mut paint: Option<&mut PaintState>,
        chords_free: bool,
    ) -> bool {
        let Some(stencil) = stencil else {
            self.stencil_drag = None;
            return false;
        };
        // N toggles the stencil (the design: ignored = paint passes
        // through, the overlay hides). Only with an image, never while
        // a text field has the keyboard, and the gate follows the
        // flip immediately through the unified re-stage + hand-off.
        if stencil.image.is_some() && frame.n_pressed {
            stencil.active = !stencil.active;
            restage_stencil(stencil, paint.as_deref_mut());
        }
        let armed = chords_free && frame.s_held && stencil.active && stencil.image.is_some();
        if self.stencil_drag.is_none() && armed {
            self.stencil_drag = frame.latch(stencil.transform);
        }
        let Some(drag) = self.stencil_drag else {
            return armed && !frame.dragged;
        };
        if frame.held(drag.chord.button()) {
            let dims = stencil.image.as_ref().map(|i| [i.width(), i.height()]);
            if let Some(t) = dims.and_then(|dims| frame.drag_to(&drag, dims)) {
                stencil.transform = t;
            }
        } else {
            // Released: the one re-rasterize this gesture costs.
            self.stencil_drag = None;
            restage_stencil(stencil, paint);
        }
        true
    }

    /// The projector's chord block (row 280; the module docs' projector
    /// section): while `projection` is armed with an image, an S-held
    /// drag latches at its press and maps every frame through
    /// [`stencil_drag`] into `projection.transform`. Latches only while no
    /// stencil drag is in flight. Returns whether it owns the pointer this
    /// frame. The release re-stages nothing (no gate reads this
    /// transform).
    fn projection_input(
        &mut self,
        frame: &ChordFrame,
        projection: Option<&mut ProjectionState>,
    ) -> bool {
        let Some(projection) = projection else {
            self.projection_drag = None;
            return false;
        };
        let dims = projection.image.as_ref().map(|i| [i.width(), i.height()]);
        let armed = frame.s_held && projection.armed && dims.is_some();
        if self.projection_drag.is_none() && self.stencil_drag.is_none() && armed {
            self.projection_drag = frame.latch(projection.transform);
        }
        let Some(drag) = self.projection_drag else {
            return armed && !frame.dragged;
        };
        if frame.held(drag.chord.button()) {
            if let Some(t) = dims.and_then(|dims| frame.drag_to(&drag, dims)) {
                projection.transform = t;
            }
        } else {
            self.projection_drag = None;
        }
        true
    }

    /// Routes this frame's S-chords (row 280): an armed projection with
    /// an image takes them, otherwise the stencil keeps them exactly as
    /// before. Both blocks always run (each clears its own latch when its
    /// target is absent, and a drag in flight runs to its release).
    /// Returns whether either owns the pointer.
    fn chord_input(
        &mut self,
        frame: &ChordFrame,
        stencil: Option<&mut StencilState>,
        projection: Option<&mut ProjectionState>,
        paint: Option<&mut PaintState>,
    ) -> bool {
        let projection_routes = projection
            .as_deref()
            .is_some_and(|p| p.armed && p.image.is_some());
        let projection_owns = self.projection_input(frame, projection);
        let chords_free = !projection_routes && self.projection_drag.is_none();
        let stencil_owns = self.stencil_input(frame, stencil, paint, chords_free);
        projection_owns || stencil_owns
    }

    /// Draws the armed projector's quad (row 280; the module docs'
    /// projector section): the corners of [`overlay_quad`] as a
    /// translucent accent polygon, its fill alpha the opacity times
    /// [`PROJECTION_FILL_FACTOR`], with a 1 px accent border. Untextured in v1. Nothing while unarmed or
    /// imageless.
    fn paint_projection_overlay(&self, ui: &Ui, rect: egui::Rect, projection: &ProjectionState) {
        let Some(image) = projection.image.as_ref() else {
            return;
        };
        if !projection.armed || projection.opacity.is_nan() {
            return;
        }
        let corners = overlay_quad(&projection.transform, [image.width(), image.height()])
            .map(|p| rect.min + p.to_vec2());
        if corners.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return;
        }
        let accent = ui.visuals().selection.bg_fill;
        let fill_alpha = (projection.opacity * PROJECTION_FILL_FACTOR).clamp(0.0, 1.0);
        // A rotated, uniformly scaled rectangle: always convex.
        let quad = egui::Shape::convex_polygon(
            corners.to_vec(),
            accent.gamma_multiply(fill_alpha),
            egui::Stroke::new(PROJECTION_OUTLINE_WIDTH, accent),
        );
        ui.painter().with_clip_rect(rect).add(quad);
    }

    /// The projector's Apply (row 280): rasterizes `projection` into the
    /// active tile through its FROZEN placement
    /// ([`rasterize_projection`]) and blends it via
    /// `PaintState::apply_projection` (row 279) at `projection.opacity`.
    /// The target dims are the active tile's quick-mask dims
    /// (`stencil::panel_section`'s target-dims rule).
    ///
    /// `rect` is the LIVE viewport rect. With follow camera off it is
    /// deliberately unused: the frozen placement carries its own camera
    /// and rect, so the live view may have moved since arm (no depth/
    /// backface culling — see the module docs). With
    /// `projection.follow_camera` AND armed (R-A2 v2b, row 288) the
    /// raster is [`rasterize_projection_live`] through `self.camera` and
    /// `rect`: paint what you see now, and `frozen` may be `None`.
    ///
    /// `false` without an image (no paint call), unarmed-follow or
    /// follow-off without a frozen placement, on a degenerate raster, or
    /// when the paint state refused / changed nothing. The shell's Apply
    /// button (row 281) calls this.
    pub fn apply_projection(
        &mut self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        paint: &mut PaintState,
        projection: &ProjectionState,
    ) -> bool {
        let live = projection.follow_camera && projection.armed;
        if projection.image.is_none() || (!live && projection.frozen.is_none()) {
            return false;
        }
        let tile = paint.active_tile();
        let (w, h) = paint.quick_mask_for_tile(tile).dimensions();
        let raster = if live {
            rasterize_projection_live(projection, mesh, tile, w, h, &self.camera, rect)
        } else {
            rasterize_projection(projection, mesh, tile, w, h)
        };
        let Some(rgba) = raster else {
            return false;
        };
        paint.apply_projection(tile, w, h, &rgba, projection.opacity)
    }

    /// Draws the stencil quad (see the module docs): the cached coverage
    /// texture on [`screen_quad`]'s corners at `overlay_opacity`, plus
    /// its outline. Nothing while inactive, imageless or fully faded.
    fn paint_stencil_overlay(&mut self, ui: &Ui, rect: egui::Rect, stencil: &StencilState) {
        let Some(image) = stencil.image.as_ref() else {
            return;
        };
        let opacity = stencil.overlay_opacity;
        if !stencil.active || opacity.is_nan() || opacity <= 0.0 {
            return;
        }
        let texture = self.stencil_texture(ui.ctx(), image);
        let corners = screen_quad(&stencil.transform, image.width(), image.height())
            .map(|[x, y]| rect.min + egui::vec2(x, y));
        if corners.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return;
        }
        let tint = Color32::WHITE.gamma_multiply(opacity.min(1.0));
        let mut mesh = egui::epaint::Mesh::with_texture(texture);
        for (pos, uv) in corners.iter().zip(STENCIL_QUAD_UVS) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: *pos,
                uv: egui::pos2(uv[0], uv[1]),
                color: tint,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        let painter = ui.painter().with_clip_rect(rect);
        painter.add(egui::Shape::mesh(mesh));
        let [r, g, b] = STENCIL_TINT;
        let outline = egui::Stroke::new(STENCIL_OUTLINE_WIDTH, Color32::from_rgb(r, g, b));
        for (a, b) in corners.iter().zip(corners.iter().cycle().skip(1)) {
            painter.line_segment([*a, *b], outline);
        }
    }

    /// The overlay texture for `image`, rebuilt only when the image
    /// changes (uv_view.rs's `mask_overlay_texture` shape).
    fn stencil_texture(&mut self, ctx: &egui::Context, image: &StencilImage) -> egui::TextureId {
        let coverage = image.coverage();
        let key = (
            coverage.as_ptr() as usize,
            coverage.len(),
            image.width(),
            image.height(),
        );
        if let Some((cached, texture)) = &self.stencil_texture {
            if *cached == key {
                return texture.id();
            }
        }
        let (size, rgba) = stencil_overlay_rgba(image);
        let color = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
        let texture = ctx.load_texture("viewport-stencil", color, egui::TextureOptions::NEAREST);
        let id = texture.id();
        self.stencil_texture = Some((key, texture));
        id
    }

    /// The soloed channel / mesh map's name at the viewport's top-left
    /// ([`readout_text`]): nothing in the Lit / Unlit views or with the
    /// readout off.
    fn paint_channel_readout(&self, ui: &Ui, rect: egui::Rect) {
        if self.mesh.is_none() {
            return;
        }
        let Some(text) = readout_text(self.view_mode, self.overlays.channel_readout, tr) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
        let pos = rect.left_top() + egui::vec2(OVERLAYS_MARGIN, OVERLAYS_MARGIN);
        let backing = egui::Rect::from_min_size(pos, galley.size()).expand(4.0);
        painter.rect_filled(backing, 3.0, Color32::from_black_alpha(160));
        painter.galley(pos, galley, Color32::WHITE);
    }

    /// The brush cursor (`crate::brush_cursor`): when the pointer is over
    /// the mesh with a paint tool, measure the brush's on-screen radius
    /// with a two-ray probe and draw the ring; the OS cursor hides while
    /// the ring shows. Off the mesh, with the eyedropper / ID-mask armed,
    /// or while orbiting (a drag without the paint modifier), nothing.
    /// Paints a screen-space segment as ONE stroke: samples every
    /// `LINE_STEP_PX` along `from -> to`, each through the real pick. Samples
    /// that miss the mesh are skipped, but the stroke keeps going, the same
    /// rule a drag across a silhouette follows.
    fn paint_screen_line(
        &self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        paint: &mut crate::paint_state::PaintState,
        from: egui::Pos2,
        to: egui::Pos2,
    ) {
        let samples = line_samples(from, to, LINE_STEP_PX);
        for at in samples {
            let hit = self
                .pick(rect, mesh, at)
                .map(|(uv, w)| (egui::Pos2::new(uv[0], uv[1]), w));
            let Some((uv, weight)) = hit else {
                continue;
            };
            if paint.is_stroking() {
                paint.extend_screen_point_pressured(Some(uv), weight);
            } else {
                paint.begin_screen_point_pressured(Some(uv), weight);
            }
        }
        if paint.is_stroking() {
            paint.end_stroke();
        }
    }

    /// The dotted straight-line preview while Shift is held between strokes.
    fn paint_line_preview(&self, ui: &Ui, response: &Response, rect: egui::Rect) {
        let Some(from) = self.last_stroke_end else {
            return;
        };
        let mods = ui.input(|i| i.modifiers);
        if !mods.shift || mods.alt || self.pick_armed || self.idmask_armed {
            return;
        }
        let Some(pos) = response.hover_pos() else {
            return;
        };
        let to = straight_line_end(from, pos, mods.ctrl || mods.command);
        let painter = ui.painter().with_clip_rect(rect);
        for (stroke, dash) in [
            (egui::Stroke::new(3.0, Color32::from_black_alpha(150)), 6.0),
            (egui::Stroke::new(1.0, Color32::from_white_alpha(230)), 6.0),
        ] {
            painter.extend(egui::Shape::dashed_line(&[from, to], stroke, dash, 4.0));
        }
    }

    /// Sets the brush cursor at `at`: the OS bitmap when the platform
    /// allows it (photocraft #1 — no render lag behind the pointer),
    /// else `CursorIcon::None` + the painted fallback ring. The shell
    /// clears any cursor image at the start of every pass, so this only
    /// sticks while the pointer is over the paintable surface.
    fn set_brush_cursor(
        ctx: &egui::Context,
        painter: &egui::Painter,
        at: egui::Pos2,
        shape: crate::brush_cursor::CursorShape,
    ) {
        use crate::brush_cursor;
        let ppp = ctx.input(|i| i.pixels_per_point);
        if let Some(img) = brush_cursor::bitmap(shape, ppp) {
            ctx.set_cursor_image(Some(img));
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            brush_cursor::paint(painter, at, shape);
        }
    }

    fn paint_brush_cursor(
        &self,
        ui: &Ui,
        response: &Response,
        rect: egui::Rect,
        paint: Option<&crate::paint_state::PaintState>,
    ) {
        use crate::brush_cursor;
        let (Some(mesh), Some(paint)) = (self.mesh_data.as_ref(), paint) else {
            return;
        };
        if self.pick_armed || self.idmask_armed || !paint.projection().viewport_paints() {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        // A resize drag in flight: the ring stays at the drag origin and
        // reads the live size/hardness, with a numeric HUD beside it.
        if let Some(drag) = self.resize {
            let radius = self.cursor_radius_px(rect, mesh, paint, drag.origin);
            let shape = brush_cursor::shape(radius, paint.brush_hardness());
            Self::set_brush_cursor(ui.ctx(), &painter, drag.origin, shape);
            let hud = format!(
                "{} ×{:.2}   {} {:.2}",
                tr("hint.hud-size"),
                paint.brush_size(),
                tr("hint.hud-hardness"),
                paint.brush_hardness()
            );
            let r = radius.unwrap_or(0.0);
            painter.text(
                drag.origin + egui::vec2(r + 10.0, -r.min(40.0)),
                egui::Align2::LEFT_BOTTOM,
                hud,
                egui::FontId::proportional(13.0),
                Color32::WHITE,
            );
            return;
        }
        // A flow drag in flight: the same contract as the resize chord,
        // the ring stays at the drag origin with a numeric HUD beside it.
        if let Some((origin, alpha0, angle0)) = self.flow_drag {
            let radius = self.cursor_radius_px(rect, mesh, paint, origin);
            let shape = brush_cursor::shape(radius, paint.brush_hardness());
            Self::set_brush_cursor(ui.ctx(), &painter, origin, shape);
            let alpha = self
                .live_flow_alpha
                .unwrap_or_else(|| flow_alpha_step(alpha0, 0.0));
            let angle = self.live_stamp_angle.unwrap_or(angle0);
            let hud = format!(
                "{} {:.0}%   {} {:.0}°",
                tr("hint.hud-flow"),
                alpha * 100.0,
                tr("hint.hud-angle"),
                angle
            );
            let r = radius.unwrap_or(0.0);
            painter.text(
                origin + egui::vec2(r + 10.0, -r.min(40.0)),
                egui::Align2::LEFT_BOTTOM,
                hud,
                egui::FontId::proportional(13.0),
                Color32::WHITE,
            );
            return;
        }
        let Some(pos) = response.hover_pos() else {
            return;
        };
        let mods = ui.input(|i| i.modifiers);
        let navigating = [
            PointerButton::Primary,
            PointerButton::Middle,
            PointerButton::Secondary,
        ]
        .into_iter()
        .any(|b| response.dragged_by(b) && self.nav.camera_op(b, mods).is_some());
        if navigating || mods.alt {
            return; // a navigation chord: the arrow, not a brush
        }
        if self.pick_uv(rect, mesh, pos).is_none() {
            return; // off the mesh
        }
        let radius = self.cursor_radius_px(rect, mesh, paint, pos);
        let shape = brush_cursor::shape(radius, paint.brush_hardness());
        Self::set_brush_cursor(ui.ctx(), &painter, pos, shape);
    }

    /// The brush's on-screen radius at `pos` (`crate::brush_cursor`):
    /// probes 8 px right and left through `pick_uv`, `None` when the
    /// pointer is off the mesh or no probe measures.
    fn cursor_radius_px(
        &self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        paint: &crate::paint_state::PaintState,
        pos: egui::Pos2,
    ) -> Option<f32> {
        use crate::brush_cursor;
        let uv = self.pick_uv(rect, mesh, pos)?;
        // The L2 kill: the ring reads the ACTIVE CHANNEL's density —
        // the same fn the dab conversion (push_event_styled's
        // local_texel) reads, so the ring and the dabs agree even
        // under a channel override that diverges from the set's size.
        let texels_per_uv = paint.texels_per_uv_for_channel(tile_of_uv(uv), paint.active_channel());
        let probe = |dir: f32| {
            let at = pos + egui::vec2(dir * brush_cursor::PROBE_PX, 0.0);
            let uv_b = self.pick_uv(rect, mesh, at)?;
            brush_cursor::texels_spanned(uv, uv_b, texels_per_uv)
        };
        brush_cursor::combine_spans(probe(1.0), probe(-1.0))
            .map(|t| brush_cursor::screen_radius(paint.brush_radius_texels(), t))
    }

    /// The home card's last click, taken (GUI plan `g5-home-screen`): the
    /// shell reads this after [`Self::ui_with_home`] and dispatches it
    /// (New / Open / the recent dir / Recover). `None` when nothing was
    /// clicked since the last take.
    //
    // `dead_code`: the binary calls this once main.rs wires the home card
    // (the integration step of g5-home-screen).
    #[allow(dead_code)]
    pub fn take_home_action(&mut self) -> Option<crate::empty_state::HomeCardAction> {
        self.home_action.take()
    }

    /// Sets the navigation preset.
    pub fn set_nav_preset(&mut self, nav: crate::navigation::NavPreset) {
        self.nav = nav;
    }

    /// Sets wheel-zoom inversion (Preferences ▸ Navigation): wheel up
    /// zooms OUT when on.
    pub fn set_wheel_invert(&mut self, invert: bool) {
        self.wheel_invert = invert;
    }

    /// F-frame: fits the camera on the loaded mesh's bounds
    /// (g1-nav-presets' open item; the engine half is
    /// [`OrbitCamera::frame`]). No mesh, no move.
    pub fn frame_mesh(&mut self) {
        let Some(mesh) = self.mesh_data.as_ref() else {
            return;
        };
        let Some((min, max)) = mesh.bounds() else {
            return;
        };
        let center = (min + max) * 0.5;
        let radius = (max - min).length() * 0.5;
        self.camera.frame(center, radius, 1.15);
    }

    /// Whether the axis gizmo draws (GUI plan G1 `g1-axis-gizmo`): the
    /// View menu's toggle, synced by the shell each frame like the
    /// overlay toggles. The gizmo itself (`crate::axis_gizmo`) is
    /// always computed; this only gates drawing + clicking.
    pub fn show_gizmo(&self) -> bool {
        self.show_gizmo
    }

    /// Shows or hides the axis gizmo.
    pub fn set_show_gizmo(&mut self, on: bool) {
        self.show_gizmo = on;
    }

    /// The navigation preset (the status bar's hints read it).
    pub fn nav_preset(&self) -> crate::navigation::NavPreset {
        self.nav
    }

    /// The LIVE camera, by value (`OrbitCamera` is `Copy`): the shell's
    /// projection Arm freezes it (projection-fill row 281; the brief's
    /// sanctioned "tiny accessor" — no other surface reaches it).
    pub fn camera(&self) -> OrbitCamera {
        self.camera
    }

    /// Sets the front-face falloff start angle in degrees, or `None` to
    /// turn it off. Clamped to `0..=90` (`90` is also off); a non-finite
    /// angle turns it off.
    pub fn set_front_face_falloff_deg(&mut self, deg: Option<f32>) {
        self.front_face_falloff_deg = deg.filter(|d| d.is_finite()).map(|d| d.clamp(0.0, 90.0));
    }

    /// Arms or disarms the eyedropper (the shell syncs this from
    /// ToolKind::Pick each frame; painting is suppressed while armed).
    pub fn set_pick_armed(&mut self, armed: bool) {
        self.pick_armed = armed;
    }

    /// Syncs the session's viewer chain — the same `DisplaySettings` the
    /// Display panel edits and its preview strip transforms. The
    /// eyedropper applies it to the sampled texel; the default settings
    /// are the identity chain, so an untouched project picks exactly as
    /// it did before this chain reached the picker.
    pub fn set_display_settings(&mut self, settings: &DisplaySettings) {
        self.display_settings.clone_from(settings);
    }

    /// Arms or disarms the ID-mask select (the shell syncs this from
    /// ToolKind::IdMask each frame; painting is suppressed while armed).
    pub fn set_idmask_armed(&mut self, armed: bool) {
        self.idmask_armed = armed;
    }

    /// Casts a ray through the pointer position and returns the mesh UV
    /// at the nearest hit (or `None` on miss). NDC Y matches the
    /// directx/wgpu convention (up = +1). The NDC→world unprojection is
    /// the shared [`umber_gpu::world_from_ndc`] helper (deduped with the
    /// ground grid's test math — same function, not a copy).
    fn pick_uv(
        &self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        pos: egui::Pos2,
    ) -> Option<[f32; 2]> {
        self.pick(rect, mesh, pos).map(|(uv, _)| uv)
    }

    /// The world-space hit point under screen `pos` — the pivot an orbit
    /// re-targets to when it starts over the last stroke's end
    /// (g1-nav-presets' orbit-around-last-stroke).
    fn world_at(
        &self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        pos: egui::Pos2,
    ) -> Option<glam::Vec3> {
        let aspect = rect.width() / rect.height();
        let ndc = cursor_ndc(rect, pos);
        let inv = self.camera.view_proj(aspect).inverse();
        let near = umber_gpu::world_from_ndc(inv, glam::Vec4::new(ndc.x, ndc.y, 0.0, 1.0));
        let far = umber_gpu::world_from_ndc(inv, glam::Vec4::new(ndc.x, ndc.y, 1.0, 1.0));
        let dir = far - near;
        let origin = match self.camera.projection {
            umber_gpu::camera::Projection::Perspective => self.camera.eye(),
            umber_gpu::camera::Projection::Orthographic => near,
        };
        let hit = umber_mesh::ray_intersect(mesh, origin, dir)?;
        let tri = mesh.indices.get(hit.triangle * 3..hit.triangle * 3 + 3)?;
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let (p0, p1, p2) = (
            mesh.positions.get(i0)?,
            mesh.positions.get(i1)?,
            mesh.positions.get(i2)?,
        );
        let (v0, v1, v2) = (
            glam::Vec3::from_array(*p0),
            glam::Vec3::from_array(*p1),
            glam::Vec3::from_array(*p2),
        );
        let (b0, b1) = hit.bary;
        Some(v0 * (1.0 - b0 - b1) + v1 * b0 + v2 * b1)
    }

    /// [`Self::pick_uv`] plus the front-face weight at the hit:
    /// `|cos|` between the view ray and the hit face's normal, through
    /// [`umber_mesh::front_facing_weight`] at the current start angle.
    /// A degenerate face (no normal) weighs `1.0` — the falloff only ever
    /// weakens a stroke it can measure.
    fn pick(
        &self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        pos: egui::Pos2,
    ) -> Option<([f32; 2], f32)> {
        self.pick_with(rect, mesh, pos, &self.camera)
    }

    /// The camera a viewport stroke's raycast resolves against: the
    /// stroke's frozen start camera while one is latched, the live camera
    /// otherwise (the stroke's opening event, or a drag still waiting for
    /// its first hit).
    fn stroke_raycast_camera(&self) -> &OrbitCamera {
        self.stroke_camera.as_ref().unwrap_or(&self.camera)
    }

    /// One viewport paint event (pwys-stroke-buffer option A): resolves
    /// `pos` against [`Self::stroke_raycast_camera`] — never the live
    /// camera once the stroke is open — and feeds the hit to the paint
    /// session (`begin_screen_point_pressured` on the opening event,
    /// `extend_screen_point_pressured` after). `pressure` is the resolved
    /// pen pressure; the front-face weight of the SAME pick scales it.
    /// The hit/miss decision is the frozen camera's: a ray that hits under
    /// the start camera paints even if the live camera has turned away,
    /// and a frozen-camera miss skips the dab (the paint crate's
    /// no-orphan-dab rule) whatever the live camera sees. Returns the
    /// resolved UV (`None` on a miss).
    fn stroke_event(
        &mut self,
        rect: egui::Rect,
        paint: &mut PaintState,
        pos: egui::Pos2,
        pressure: f32,
    ) -> Option<[f32; 2]> {
        let camera = *self.stroke_raycast_camera();
        let picked = self
            .mesh_data
            .as_ref()
            .and_then(|mesh| self.pick_with(rect, mesh, pos, &camera));
        let hit = picked.map(|(uv, _)| egui::Pos2::new(uv[0], uv[1]));
        let pressure = pressure * picked.map_or(1.0, |(_, w)| w);
        if !paint.is_stroking() {
            paint.begin_screen_point_pressured(hit, pressure);
        } else {
            paint.extend_screen_point_pressured(hit, pressure);
        }
        // Mark ownership only once a stroke actually began (a drag that
        // opened on a miss starts nothing until it hits).
        self.stroke_from_viewport = paint.is_stroking();
        self.latch_stroke_camera(paint.is_stroking(), camera);
        if hit.is_some() {
            self.last_stroke_end = Some(pos);
        }
        picked.map(|(uv, _)| uv)
    }

    /// The snapshot's lifecycle: the FIRST event that leaves a stroke open
    /// latches `camera` (the very camera that event resolved against, so
    /// the opening dab and the rest of the stroke share one ray set); a
    /// latched snapshot is never overwritten mid-stroke; no open stroke
    /// clears it.
    fn latch_stroke_camera(&mut self, stroking: bool, camera: OrbitCamera) {
        if !stroking {
            self.stroke_camera = None;
        } else if self.stroke_camera.is_none() {
            self.stroke_camera = Some(camera);
        }
    }

    /// [`Self::pick`] against an explicit `camera` — the live one for the
    /// non-stroke picks (eyedropper, ID mask, the brush cursor, the
    /// straight line), a stroke's frozen start camera for
    /// [`Self::stroke_event`].
    fn pick_with(
        &self,
        rect: egui::Rect,
        mesh: &umber_mesh::MeshData,
        pos: egui::Pos2,
        camera: &OrbitCamera,
    ) -> Option<([f32; 2], f32)> {
        let aspect = rect.width() / rect.height();
        let ndc_x = (pos.x - rect.left()) / rect.width() * 2.0 - 1.0;
        let ndc_y = 1.0 - (pos.y - rect.top()) / rect.height() * 2.0;
        let view_proj = camera.view_proj(aspect);
        let inv = view_proj.inverse();
        // Unproject the near (z=0) and far (z=1) NDC points (directx
        // depth convention) to build the ray.
        let near = umber_gpu::world_from_ndc(inv, glam::Vec4::new(ndc_x, ndc_y, 0.0, 1.0));
        let far = umber_gpu::world_from_ndc(inv, glam::Vec4::new(ndc_x, ndc_y, 1.0, 1.0));
        let dir = far - near;
        // Orthographic rays are parallel: the origin is the unprojected
        // NEAR point on the sight ray, not the eye (fanning from the eye
        // would slant every pick). The perspective arm below is exactly
        // today's code.
        let origin = match camera.projection {
            umber_gpu::camera::Projection::Perspective => camera.eye(),
            umber_gpu::camera::Projection::Orthographic => near,
        };
        let hit = umber_mesh::ray_intersect(mesh, origin, dir)?;
        let uv = umber_mesh::uv_at(mesh, hit)?;
        let weight = match (
            self.front_face_falloff_deg,
            umber_mesh::face_normal(mesh, hit),
        ) {
            (Some(start), Some(n)) => {
                umber_mesh::front_facing_weight(n.dot(dir.normalize_or_zero()), start)
            }
            // Off, or a degenerate face it cannot measure: full strength.
            _ => 1.0,
        };
        Some((uv, weight))
    }

    /// The POINTER-driven navigation: the preset's drag chords (orbit /
    /// pan / zoom, RMB-cancel) and the wheel zoom. Skipped while a
    /// viewport stroke is live — the artist's pointer belongs to the
    /// stroke (the caller's guard).
    fn handle_pointer_nav(&mut self, ui: &Ui, response: &Response) {
        // Navigation goes through the preset (`crate::navigation`): the
        // first held button whose (button, modifiers) chord the preset
        // binds drives the camera this frame.
        let mods = ui.input(|i| i.modifiers);
        // g1-nav-presets' open item: RMB cancels the running navigation
        // drag (Painter feel). Sticky until the cancelled button is
        // released, so re-pressing the same button does not resume.
        let buttons = [
            PointerButton::Primary,
            PointerButton::Middle,
            PointerButton::Secondary,
        ];
        let dragged = buttons.into_iter().find(|b| response.dragged_by(*b));
        let rmb_down = ui.input(|i| i.pointer.secondary_down());
        let cancelled_down = self
            .nav_cancel
            .map(|b| ui.input(|i| i.pointer.button_down(b)))
            .unwrap_or(false);
        self.nav_cancel = rmb_cancel_step(self.nav_cancel, dragged, rmb_down, cancelled_down);
        let op = buttons
            .into_iter()
            .filter(|b| response.dragged_by(*b))
            .filter(|b| Some(*b) != self.nav_cancel)
            .find_map(|b| self.nav.camera_op(b, mods));
        if let Some(op) = op {
            let delta = response.drag_delta();
            match op {
                crate::navigation::CameraOp::Orbit => {
                    // g1-nav-presets' last open tail: an orbit that starts
                    // over the last stroke's end pivots around that world
                    // point, not the mesh centre — Painter orbits around
                    // what was just painted.
                    if let Some(b) = dragged {
                        if response.drag_started_by(b) {
                            let pivot = match (self.mesh_data.as_ref(), self.last_stroke_end) {
                                (Some(mesh), Some(end)) => self.world_at(response.rect, mesh, end),
                                _ => None,
                            };
                            if let Some(pivot) = pivot {
                                self.camera.target = pivot;
                            }
                        }
                    }
                    self.camera
                        .orbit(delta.x * ORBIT_SENSITIVITY, -delta.y * ORBIT_SENSITIVITY)
                }
                crate::navigation::CameraOp::Pan => self
                    .camera
                    .pan(-delta.x * PAN_SENSITIVITY, delta.y * PAN_SENSITIVITY),
                // Drag right / down dollies in (Painter's Alt+RMB feel).
                crate::navigation::CameraOp::Zoom => self
                    .camera
                    .zoom((delta.x + delta.y) * ZOOM_DRAG_SENSITIVITY),
            }
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                // g1-nav-presets' open item: wheel zoom TOWARD the cursor —
                // the point under the wheel stays fixed on screen.
                let aspect = response.rect.width() / response.rect.height().max(1.0);
                let ndc = response
                    .hover_pos()
                    .map(|pos| cursor_ndc(response.rect, pos))
                    .unwrap_or(egui::Vec2::ZERO);
                let mut step = scroll.signum() * ZOOM_SENSITIVITY;
                if self.wheel_invert {
                    step = -step;
                }
                self.camera
                    .zoom_toward(glam::Vec2::new(ndc.x, ndc.y), step, aspect);
            }
        }
    }

    /// The NON-pointer camera sources (today: the P projection toggle and
    /// the F frame), live during a stroke as well as between strokes —
    /// the stroke resolves against its frozen start camera, so moving the
    /// live camera here cannot disturb it. Mid-stroke (`stroking`) a
    /// Shift-held press stands down: a viewport stroke always holds the
    /// paint chord's Ctrl/Cmd, so Shift+P there is the shell's command
    /// palette (Cmd+Shift+P, actions.rs `BINDINGS`), never a projection
    /// flip. Between strokes the keys behave exactly as before.
    fn handle_camera_keys(&mut self, ui: &Ui, response: &Response, stroking: bool) {
        if stroking && ui.input(|i| i.modifiers.shift) {
            return;
        }
        if response.hovered() {
            // Projection toggle (P, mnemonic: Projection): free because the
            // shell's SHORTCUT_KEYS (toolbar.rs: W/G/V/X/I/E/Z/S/O) and the
            // graph canvas (Delete/Backspace/Escape) bind nothing else;
            // hover-gated and modifier-free so it never fires while typing
            // in a panel. No i18n string by design (the .ftl catalogs are
            // owned by another surface) — same as the drag/scroll inputs.
            if ui.input(|i| i.key_pressed(egui::Key::P)) {
                self.camera.toggle_projection();
            }
            // F frames the loaded mesh (g1-nav-presets' F-frame; same
            // hover-gated, modifier-free contract as the P toggle).
            if ui.input(|i| i.key_pressed(egui::Key::F)) {
                self.frame_mesh();
            }
        }
    }
}

/// The Overlays button's width and its inset from the viewport corner.
const OVERLAYS_BUTTON_WIDTH: f32 = 84.0;
const OVERLAYS_MARGIN: f32 = 8.0;
/// The seam overlay's line color: cyan, distinct from the 40%-white
/// wireframe and the amber cage.
const SEAM_COLOR: Color32 = Color32::from_rgb(40, 200, 255);
const SEAM_WIDTH: f32 = 1.5;

/// The wire pass's color at `opacity` (`0..=1`, clamped; NaN = the
/// default): the default wire rgb with the alpha replaced, so the
/// default opacity reproduces `WireColor::default()` exactly.
fn wire_color_for(opacity: f32) -> umber_gpu::WireColor {
    let mut color = umber_gpu::WireColor::default();
    if opacity.is_finite() {
        color.color[3] = opacity.clamp(0.0, 1.0);
    }
    color
}

/// The stencil chords' smallest scale (g6's clamp): a smaller quad is a
/// speck no chord can grab back.
const MIN_STENCIL_SCALE: f32 = 0.05;
/// Shift+S+LMB snaps the stencil's rotation to quarter turns.
const STENCIL_SNAP_STEP: f32 = std::f32::consts::FRAC_PI_2;
/// Within this many px of the center the drag angle is undefined (the
/// `atan2` of a ~zero vector): the rotation holds.
const STENCIL_MIN_ARM_PX: f32 = 1.0;
/// The overlay texture's longest side, texels: larger stencils are
/// point-sampled down (uv_view.rs `MASK_OVERLAY_MAX_SIDE`'s cap — a 4096²
/// stencil would otherwise upload 64 MB).
const STENCIL_OVERLAY_MAX_SIDE: u32 = 512;
/// The overlay's tint where the stencil paints through: violet, distinct
/// from the white wireframe, cyan seams, amber cage and the quick mask's
/// red.
const STENCIL_TINT: [u8; 3] = [190, 120, 255];
const STENCIL_OUTLINE_WIDTH: f32 = 1.0;

/// The projector overlay's fill alpha per unit of `projection.opacity`
/// (row 280's brief: `opacity * 0.25`): faint enough that the mesh under
/// the decal being placed stays readable.
pub const PROJECTION_FILL_FACTOR: f32 = 0.25;
/// The projector overlay's border width, px.
const PROJECTION_OUTLINE_WIDTH: f32 = 1.0;

/// The chord buttons, in latch priority order (LMB, RMB, MMB).
const CHORD_BUTTONS: [PointerButton; 3] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Middle,
];

/// One frame of the S-chord input, read once from egui
/// ([`ChordFrame::read`]) so both chord targets — the stencil and the
/// armed projection (row 280) — route from the same snapshot, and the
/// routing pins without an egui frame or a GPU mesh. Positions are
/// viewport-local px (the stencil section's space).
#[derive(Debug, Clone, Copy, PartialEq)]
struct ChordFrame {
    /// S held with no Alt/Ctrl/Cmd and a mesh loaded (the arm condition,
    /// minus the target's own state).
    s_held: bool,
    /// Shift held (Rotate's quarter-turn snap).
    shift: bool,
    /// N pressed this frame with no text field focused (the stencil's
    /// toggle).
    n_pressed: bool,
    /// The first chord button whose drag started this frame.
    started: Option<PointerButton>,
    /// The press origin (`pointer.press_origin()`, else the interact
    /// position).
    origin: Option<[f32; 2]>,
    /// The pointer (`interact_pointer_pos`).
    pos: Option<[f32; 2]>,
    /// Whether each of [`CHORD_BUTTONS`] is dragging the viewport.
    dragged_by: [bool; 3],
    /// The viewport is being dragged by any button.
    dragged: bool,
    /// The viewport's size, px.
    view: [f32; 2],
}

impl ChordFrame {
    /// Reads this frame's chord input off `ui` + the viewport's
    /// `response` (exactly the reads the stencil block made inline).
    fn read(ui: &Ui, response: &Response, rect: egui::Rect, has_mesh: bool) -> Self {
        let mods = ui.input(|i| i.modifiers);
        let local = |p: egui::Pos2| [p.x - rect.min.x, p.y - rect.min.y];
        let s_held = has_mesh
            && !mods.alt
            && !(mods.ctrl || mods.command)
            && ui.input(|i| i.key_down(egui::Key::S));
        let n_pressed = ui.input(|i| i.key_pressed(egui::Key::N))
            && !ui.ctx().memory(|m| m.focused().is_some());
        // Map from the PRESS: drag_started fires past the threshold.
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos())
            .map(local);
        Self {
            s_held,
            shift: mods.shift,
            n_pressed,
            started: CHORD_BUTTONS
                .into_iter()
                .find(|b| response.drag_started_by(*b)),
            origin,
            pos: response.interact_pointer_pos().map(local),
            dragged_by: CHORD_BUTTONS.map(|b| response.dragged_by(b)),
            dragged: response.dragged(),
            view: [rect.width(), rect.height()],
        }
    }

    /// Whether `button` is dragging the viewport this frame.
    fn held(&self, button: PointerButton) -> bool {
        CHORD_BUTTONS
            .iter()
            .position(|b| *b == button)
            .is_some_and(|i| self.dragged_by[i])
    }

    /// The drag this frame's press latches on `start`: a chord button's
    /// drag started and the press origin is known.
    fn latch(&self, start: StencilTransform) -> Option<StencilDrag> {
        let chord = self.started.and_then(StencilChord::for_button)?;
        let origin = self.origin?;
        Some(StencilDrag {
            chord,
            start,
            origin,
        })
    }

    /// `drag`'s transform this frame for an image of `dims` (the snap
    /// read live), or `None` without a pointer position.
    fn drag_to(&self, drag: &StencilDrag, dims: [u32; 2]) -> Option<StencilTransform> {
        let pos = self.pos?;
        Some(stencil_drag(
            &drag.start,
            drag.chord.with_snap(self.shift),
            drag.origin,
            pos,
            dims,
            self.view,
        ))
    }
}
/// Texture coords for [`screen_quad`]'s corner order (image rows run
/// top-down, as egui's v does).
const STENCIL_QUAD_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// The stencil quad's corners in viewport-local px under `transform`:
/// the image px corners `(0,0)`, `(w,0)`, `(w,h)`, `(0,h)` through
/// `umber_core::stencil::forward_map` — the gate's own map, so the quad
/// the eye sees is the region the rasterizer opens. At rotation 0 the
/// order runs top-left, top-right, bottom-right, bottom-left.
pub fn screen_quad(transform: &StencilTransform, image_w: u32, image_h: u32) -> [[f32; 2]; 4] {
    let (w, h) = (image_w as f32, image_h as f32);
    [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
        .map(|corner| forward_map(transform, [image_w, image_h], corner))
}

/// One S-chord drag's transform (pure; the module docs' stencil
/// section): `start` is the transform at the press, `drag_start` the
/// press and `drag_current` the pointer (viewport-local px). Rotate turns
/// by the arc's angle change around the center, snapping the result to
/// quarter turns with `snap`, wrapped into `(-π, π]`. Scale grows the
/// on-screen half-width (`image_dims[0] / 2 * scale`) by the horizontal
/// delta, clamped to [`MIN_STENCIL_SCALE`]. Move shifts the center by the
/// delta, clamped into `view_dims`.
pub fn stencil_drag(
    start: &StencilTransform,
    chord: StencilChord,
    drag_start: [f32; 2],
    drag_current: [f32; 2],
    image_dims: [u32; 2],
    view_dims: [f32; 2],
) -> StencilTransform {
    let delta = [
        drag_current[0] - drag_start[0],
        drag_current[1] - drag_start[1],
    ];
    match chord {
        StencilChord::Rotate { snap } => {
            let arm = |p: [f32; 2]| [p[0] - start.center[0], p[1] - start.center[1]];
            let (from, to) = (arm(drag_start), arm(drag_current));
            let short = |v: [f32; 2]| v[0].hypot(v[1]) < STENCIL_MIN_ARM_PX;
            let mut rotation = start.rotation;
            if !short(from) && !short(to) {
                rotation += to[1].atan2(to[0]) - from[1].atan2(from[0]);
            }
            if snap {
                rotation = (rotation / STENCIL_SNAP_STEP).round() * STENCIL_SNAP_STEP;
            }
            StencilTransform {
                rotation: wrap_angle(rotation),
                ..*start
            }
        }
        StencilChord::Scale => {
            let half_w = image_dims[0].max(1) as f32 * 0.5;
            StencilTransform {
                scale: (start.scale + delta[0] / half_w).max(MIN_STENCIL_SCALE),
                ..*start
            }
        }
        StencilChord::Move => {
            let keep_in = |v: f32, max: f32| {
                if max.is_finite() && max > 0.0 {
                    v.clamp(0.0, max)
                } else {
                    v
                }
            };
            StencilTransform {
                center: [
                    keep_in(start.center[0] + delta[0], view_dims[0]),
                    keep_in(start.center[1] + delta[1], view_dims[1]),
                ],
                ..*start
            }
        }
    }
}

/// `angle` wrapped into `(-π, π]`. In-range angles pass through
/// untouched, so a quarter-turn snap stays bit-exact (a `rem_euclid`
/// round trip would round `-π/2` through `3π/2`).
fn wrap_angle(angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if angle > -PI && angle <= PI {
        return angle;
    }
    let a = angle.rem_euclid(TAU);
    if a > PI {
        a - TAU
    } else {
        a
    }
}

/// `image`'s coverage as the overlay's unmultiplied RGBA: [`STENCIL_TINT`]
/// opaque where it paints through, clear elsewhere, point-sampled to at
/// most [`STENCIL_OVERLAY_MAX_SIDE`] per side. Returns the overlay's
/// `[width, height]` and its bytes, rows top-down (the image's order).
fn stencil_overlay_rgba(image: &StencilImage) -> ([usize; 2], Vec<u8>) {
    let (w, h) = (image.width(), image.height());
    let step = w.max(h).div_ceil(STENCIL_OVERLAY_MAX_SIDE).max(1);
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let [r, g, b] = STENCIL_TINT;
    let coverage = image.coverage();
    let mut rgba = Vec::with_capacity(ow as usize * oh as usize * 4);
    for oy in 0..oh {
        for ox in 0..ow {
            let at = (oy * step) as usize * w as usize + (ox * step) as usize;
            let on = coverage.get(at).is_some_and(|&c| c != 0);
            rgba.extend_from_slice(&if on { [r, g, b, 255] } else { [0; 4] });
        }
    }
    ([ow as usize, oh as usize], rgba)
}

/// Re-stages the stencil gate against the paint session's active tile —
/// BOTH halves: the composed LABEL bytes (`stencil::panel_section`'s own
/// target + quick-mask lines, mirrored because stencil.rs is lock-held)
/// and the HOOK hand-off (Amendment A: the RAW coverage through
/// `set_stencil_gate`, so every viewport path — chord release, popover
/// edit, N — and the panel path keep one gate). `pub(crate)` because the
/// set switch's paint-side event (`texture_sets_panel_ui`, main.rs) is
/// the fourth caller: Amendment A point 4's per-set scope, the gate
/// stages at the ACTIVE tile's quick-mask plane, so a set switch must
/// re-stage at the newly-resolved plane or the new tile paints
/// stencil-ungated.
/// The Overlays popover's stencil rows: step 2's
/// `display_panel::stencil_rows` with `view` (this viewport's size) as
/// the fit for a load and for Reset — `stencil::panel_section`'s adapter
/// with the viewport rect in place of its target size (see the module
/// docs).
fn stencil_popover_rows(
    ui: &mut Ui,
    stencil: &mut StencilState,
    paint: Option<&mut PaintState>,
    view: [f32; 2],
) {
    let staged = stencil
        .staged
        .as_ref()
        .map(|g| (g.width, g.height, g.open_percent()));
    let response = crate::display_panel::stencil_rows(
        ui,
        crate::display_panel::StencilRow {
            active: &mut stencil.active,
            opacity: &mut stencil.overlay_opacity,
            path: stencil.path.as_deref(),
            has_image: stencil.image.is_some(),
            load_error: stencil.load_error.as_deref(),
            staged,
        },
    );
    let mut restage = response.toggled;
    if let Some(path) = response.picked {
        let result = crate::stencil::load_image_rgba(&path);
        stencil.adopt(path, result, view);
        restage = true;
    }
    if response.reset {
        stencil.reset(view[0], view[1]);
        restage = true;
    }
    if restage {
        restage_stencil(stencil, paint);
    }
}

/// Which side of a face the eye sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceSide {
    /// The face's winding normal points at the eye.
    Front,
    /// The face points away (or is edge-on): a back face.
    Back,
}

/// The face-orientation decision: `normal` is the face's CCW winding
/// normal (the mesh pipeline's front-face convention), `to_eye` points
/// from the face toward the eye (perspective) or against the view
/// direction (orthographic). Edge-on (`dot == 0`) and degenerate inputs
/// read as Back — the side that does not render under back-face culling.
pub fn face_side(normal: glam::Vec3, to_eye: glam::Vec3) -> FaceSide {
    if normal.dot(to_eye) > 0.0 {
        FaceSide::Front
    } else {
        FaceSide::Back
    }
}

/// One UV-seam edge for the overlay: the shared 3D edge's endpoints and
/// the two adjacent faces' winding normals (for back-side culling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeamSegment {
    /// The edge's 3D endpoints.
    pub ends: [glam::Vec3; 2],
    /// The winding normals of the two triangles sharing the edge
    /// (`Vec3::ZERO` for a degenerate one).
    pub normals: [glam::Vec3; 2],
}

impl SeamSegment {
    /// Whether the eye at `eye` (or, with `view_dir`, an orthographic
    /// view along it) sees either adjacent face from the front.
    fn visible(&self, eye: glam::Vec3, view_dir: Option<glam::Vec3>) -> bool {
        let mid = (self.ends[0] + self.ends[1]) * 0.5;
        let to_eye = view_dir.map_or(eye - mid, |d| -d);
        self.normals
            .iter()
            .any(|n| face_side(*n, to_eye) == FaceSide::Front)
    }
}

/// A triangle's CCW winding normal (unnormalized is fine for the sign
/// test; `ZERO` when an index is out of range).
fn tri_normal(mesh: &umber_mesh::MeshData, tri: usize) -> glam::Vec3 {
    let corner = |k: usize| {
        mesh.indices
            .get(tri * 3 + k)
            .and_then(|&i| mesh.positions.get(i as usize))
            .map(|&p| glam::Vec3::from(p))
    };
    match (corner(0), corner(1), corner(2)) {
        (Some(a), Some(b), Some(c)) => (b - a).cross(c - a),
        _ => glam::Vec3::ZERO,
    }
}

/// The mesh's UV-seam edges as 3D segments (pure; the seam overlay's
/// source). Reuses `umber_mesh::seam::build_seam_graph` — the seam-aware
/// stamping's graph, so the overlay shows exactly the cuts the paint
/// engine mirrors across — and recovers each cut's endpoints from its
/// first triangle: the graph keeps only the edge midpoint, computed as
/// `(a + b) * 0.5` over the exact positions, so the same arithmetic on
/// the triangle's own corners matches it bit for bit. Open mesh borders
/// (edges with one triangle) are not seams and do not appear.
pub fn uv_seam_segments(mesh: &umber_mesh::MeshData) -> Vec<SeamSegment> {
    let graph = umber_mesh::seam::build_seam_graph(mesh);
    let mut out = Vec::with_capacity(graph.edges.len());
    for edge in &graph.edges {
        let tri = edge.tri_a as usize;
        let corners: Option<Vec<glam::Vec3>> = (0..3)
            .map(|k| {
                mesh.indices
                    .get(tri * 3 + k)
                    .and_then(|&i| mesh.positions.get(i as usize))
                    .map(|&p| glam::Vec3::from(p))
            })
            .collect();
        let Some(c) = corners else { continue };
        let ends = [(0, 1), (1, 2), (2, 0)]
            .into_iter()
            .map(|(a, b)| [c[a], c[b]])
            .find(|[a, b]| (*a + *b) * 0.5 == edge.mid_3d);
        if let Some(ends) = ends {
            out.push(SeamSegment {
                ends,
                normals: [
                    tri_normal(mesh, edge.tri_a as usize),
                    tri_normal(mesh, edge.tri_b as usize),
                ],
            });
        }
    }
    out
}

/// World → screen through `view_proj` into `rect`; `None` behind the
/// eye (`w <= 0`) or outside the depth range.
fn project_to_screen(view_proj: glam::Mat4, rect: egui::Rect, p: glam::Vec3) -> Option<egui::Pos2> {
    let clip = view_proj * p.extend(1.0);
    if clip.w <= f32::EPSILON {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if !(0.0..=1.0).contains(&ndc.z) {
        return None;
    }
    Some(egui::pos2(
        rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
        rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
    ))
}

/// Draws the visible seam segments as screen-space lines.
fn paint_seams(
    painter: &egui::Painter,
    rect: egui::Rect,
    view_proj: glam::Mat4,
    camera: &OrbitCamera,
    segments: &[SeamSegment],
) {
    let eye = camera.eye();
    let view_dir = match camera.projection {
        umber_gpu::camera::Projection::Perspective => None,
        umber_gpu::camera::Projection::Orthographic => Some(camera.target - eye),
    };
    let stroke = egui::Stroke::new(SEAM_WIDTH, SEAM_COLOR);
    for seg in segments.iter().filter(|s| s.visible(eye, view_dir)) {
        if let (Some(a), Some(b)) = (
            project_to_screen(view_proj, rect, seg.ends[0]),
            project_to_screen(view_proj, rect, seg.ends[1]),
        ) {
            painter.line_segment([a, b], stroke);
        }
    }
}

/// The channel-name readout for `mode`: the display-mode keys' toast
/// text (`crate::display_keys::mode_name`) while a channel or mesh map
/// is soloed and the readout is on; `None` in Lit / Unlit (nothing is
/// soloed — the material needs no label) or with the readout off.
fn readout_text(mode: ViewMode, on: bool, tr: impl Fn(&str) -> String) -> Option<String> {
    match mode {
        ViewMode::Solo(_) | ViewMode::MeshMapPreview(_) if on => {
            Some(crate::display_keys::mode_name(mode, tr))
        }
        _ => None,
    }
}

/// Screen distance between straight-line samples, px. Below the smallest
/// brush the cursor draws (`MIN_RING_PX`), so the conditioner's own spacing
/// decides the dab density, not this.
const LINE_STEP_PX: f32 = 2.0;

/// The straight line's end: the pointer, or (with `snap`) the pointer
/// projected onto the nearest 5° direction from `from` (Painter's
/// Ctrl+Shift).
fn straight_line_end(from: egui::Pos2, to: egui::Pos2, snap: bool) -> egui::Pos2 {
    if !snap {
        return to;
    }
    let d = to - from;
    let len = d.length();
    if len <= f32::EPSILON {
        return to;
    }
    let step = 5f32.to_radians();
    let angle = (d.y.atan2(d.x) / step).round() * step;
    // Project onto the snapped ray (keeps the pointer's distance along it).
    let dir = egui::vec2(angle.cos(), angle.sin());
    from + dir * d.dot(dir)
}

/// Sample points from `from` to `to` inclusive, at most `step` px apart.
fn line_samples(from: egui::Pos2, to: egui::Pos2, step: f32) -> Vec<egui::Pos2> {
    let len = (to - from).length();
    let n = ((len / step.max(0.5)).ceil() as usize).max(1);
    (0..=n)
        .map(|i| from + (to - from) * (i as f32 / n as f32))
        .collect()
}

/// Whether this frame ends the viewport's paint stroke: the paint drag
/// stopped (`painting` false) while a stroke THIS viewport began is still
/// open. A stroke the UV view owns is never ended here (both views share
/// one paint session).
fn ends_viewport_stroke(painting: bool, stroking: bool, from_viewport: bool) -> bool {
    !painting && stroking && from_viewport
}

/// Binds the paint session's active tile as `mesh`'s base-color texture
/// (the CORE-LOOP follow-up — see the module docs). `None` (view-only
/// device, or a tile with no target yet) clears to the constant arm, which
/// renders pixel-identical to a mesh that never bound paint.
///
/// UV derivation: [`PaintParams::for_udim`] subtracts the tile origin
/// (`(tile - 1001) % 10`, `(tile - 1001) / 10`) at scale 1 — the exact
/// inverse of [`crate::paint_state`] tile-local mapping (its `local_texel`
/// subtracts the same offsets before the V-flipped texel map), so the
/// shader's `(uv - offset) * scale` lands on the bound tile's own texels.
///
/// # The height-channel bind (audit #12's (c) seam — landed with the tile pool)
///
/// The per-channel tile pool (docs/specs/per-channel-tile-pool-design.md)
/// gave `PaintState` its channel dimension; this function now binds the
/// active tile's HEIGHT-channel target through `set_height_texture` when
/// one has materialized (lazy — an unrouted channel simply leaves the
/// perturbation off, no invented state). The bind is read-only by
/// contract: `channel_target` never materializes a target, so binding
/// cannot create GPU memory as a side effect. When no height target
/// exists the perturbation clears (the flag-off path is the golden-tested
/// byte-identical baseline from the heightnormal kill).
fn bind_paint_target(
    gpu: &GpuContext,
    mesh: &mut MeshBuffers,
    paint: Option<&crate::paint_state::PaintState>,
) {
    let Some(paint) = paint else {
        // Already on the constant arm: nothing is bound and the skip is
        // provably identical (no view held, flags already 0).
        if mesh.paint_params().flags != 0 {
            mesh.clear_base_color_texture(gpu);
        }
        return;
    };
    let tile = paint.active_tile();
    // Unconditional while paint is present: a `Resize` command replaces the
    // tile's texture object, so a params-equality skip would keep sampling
    // the evicted texture. One bind group + one small uniform write a frame.
    match paint.tile_target(tile) {
        Some(target) => mesh.set_base_color_texture(gpu, target, PaintParams::for_udim(tile)),
        None => mesh.clear_base_color_texture(gpu),
    }
    // The height arm (audit #12): the same active tile, its height channel.
    // Mirrors the baseColor pattern — bind when present, clear when not —
    // so a height stroke's texture reaches the Lit path's perturbation the
    // same frame it landed (the compositor-free live view the audit gated).
    match paint.channel_target(tile, crate::paint_state::HEIGHT_CHANNEL) {
        Some(target) => {
            mesh.set_height_texture(gpu, target, PaintParams::height_for_udim(tile, 1.0))
        }
        None => mesh.clear_height_texture(gpu),
    }
}

/// The View menu's view-mode rows (where the wireframe/grid toggles
/// live): Lit / Unlit / Solo (+ channel picker) / Mesh-map preview
/// (+ bake-map picker). Edits `mode` in place; returns whether it
/// changed this frame. Every string goes through [`tr`] (both
/// catalogs carry the `viewmode.*` keys).
pub fn view_mode_menu(ui: &mut Ui, mode: &mut ViewMode) -> bool {
    let before = *mode;
    ui.label(tr("viewmode.label"));
    ui.selectable_value(mode, ViewMode::Lit, tr("viewmode.lit"));
    ui.selectable_value(mode, ViewMode::Unlit, tr("viewmode.unlit"));

    let solo_active = matches!(mode, ViewMode::Solo(_));
    if ui
        .selectable_label(solo_active, tr("viewmode.solo"))
        .clicked()
        && !solo_active
    {
        *mode = ViewMode::Solo(SoloChannel::BaseColor);
    }
    if let ViewMode::Solo(channel) = mode {
        egui::ComboBox::from_label(tr("viewmode.channel"))
            .selected_text(solo_channel_label(*channel))
            .show_ui(ui, |ui| {
                for candidate in SoloChannel::ALL {
                    ui.selectable_value(channel, candidate, solo_channel_label(candidate));
                }
            });
    }

    let preview_active = matches!(mode, ViewMode::MeshMapPreview(_));
    if ui
        .selectable_label(preview_active, tr("viewmode.mesh-preview"))
        .clicked()
        && !preview_active
    {
        *mode = ViewMode::MeshMapPreview(umber_mesh::MeshMapKind::AmbientOcclusion);
    }
    if let ViewMode::MeshMapPreview(kind) = mode {
        egui::ComboBox::from_label(tr("viewmode.map"))
            .selected_text(kind.suffix())
            .show_ui(ui, |ui| {
                for candidate in ViewMode::ALL_PREVIEW_KINDS {
                    ui.selectable_value(kind, candidate, candidate.suffix());
                }
            });
    }

    *mode != before
}

/// The picker label for a solo channel (i18n'd; the shader-side token
/// is [`SoloChannel::as_str`], not this).
fn solo_channel_label(channel: SoloChannel) -> String {
    match channel {
        SoloChannel::BaseColor => tr("viewmode.basecolor"),
        SoloChannel::Emissive => tr("viewmode.emissive"),
        SoloChannel::Roughness => tr("viewmode.roughness"),
        SoloChannel::Metallic => tr("viewmode.metallic"),
        SoloChannel::AmbientOcclusion => tr("viewmode.ao"),
        SoloChannel::Height => tr("viewmode.height"),
        SoloChannel::Normal => tr("viewmode.normal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A viewport staring straight down -z at the origin (yaw = pitch = 0
    /// puts the eye on +z looking at the target), for the picking tests.
    fn head_on_viewport() -> Viewport {
        Viewport {
            camera: OrbitCamera {
                target: glam::Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                ..OrbitCamera::default()
            },
            ..Viewport::default()
        }
    }

    /// Two-triangle quad on the z=0 plane with unit UVs (mirrors
    /// `umber_mesh::raycast`'s fixture: positions == UVs on x/y).
    fn quad() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
            triangle_materials: Vec::new(),
        }
    }

    fn rect200() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 200.0))
    }

    #[test]
    fn world_at_center_hits_quad_center() {
        // The orbit pivot: the ray through the viewport center meets the
        // quad at its world center (0, 0, 0).
        let viewport = head_on_viewport();
        let w = viewport
            .world_at(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("center ray must hit the quad");
        assert!(w.distance(glam::Vec3::ZERO) < 1e-4, "got {w:?}");
    }

    #[test]
    fn pick_uv_center_hits_center_uv() {
        // Screen→UV mapping (§2 P1): the ray through the viewport center
        // meets the quad's center, whose UV is (0.5, 0.5).
        let viewport = head_on_viewport();
        let uv = viewport
            .pick_uv(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("center ray must hit the quad");
        assert!(
            (uv[0] - 0.5).abs() < 1e-4 && (uv[1] - 0.5).abs() < 1e-4,
            "center must map to center UV, got {uv:?}"
        );
    }

    // --- pwys-stroke-buffer option A: the camera may move mid-stroke ---
    //
    // Geometry shared by the pins (the head-on fixture: eye at (0, 0, 5),
    // fov 45°, so tan(22.5°) = 0.414214, rect 200×200, aspect 1). A
    // screen point at NDC x = s, y = 0 casts the ray
    // `forward + k·right` with `k = s·tan(22.5°)`; under yaw `y` it meets
    // the z = 0 quad at `x = 5k / (cos y + k·sin y)` (y = 0 ⇒ `5k`), and
    // the quad's UV is `u = (x + 1) / 2`, `v = 0.5`.

    /// Pixel x = 80 ⇒ NDC s = -0.2.
    const STROKE_PX: egui::Pos2 = egui::Pos2 { x: 80.0, y: 100.0 };
    /// Head-on: x = 5·(-0.2·0.414214) = -0.414214 ⇒ u = 0.292893.
    const START_U: f32 = 0.292_893;
    /// Yaw 0.7: k = -0.082843, den = 0.764842 - 0.082843·0.644218 =
    /// 0.711473, x = -0.414214 / 0.711473 = -0.582191 ⇒ u = 0.208905.
    const ROTATED_U: f32 = 0.208_905;
    const ROTATED_YAW: f32 = 0.7;

    fn painting_viewport() -> Viewport {
        Viewport {
            mesh_data: Some(quad()),
            ..head_on_viewport()
        }
    }

    /// The stroke's own resolve (what `stroke_event` picks with).
    fn stroke_uv(viewport: &Viewport, pos: egui::Pos2) -> Option<[f32; 2]> {
        let mesh = viewport.mesh_data.as_ref()?;
        viewport
            .pick_with(rect200(), mesh, pos, viewport.stroke_raycast_camera())
            .map(|(uv, _)| uv)
    }

    fn assert_uv(uv: [f32; 2], u: f32, v: f32, what: &str) {
        assert_uv_loose(uv, u, v, 1e-4, what);
    }

    /// The same assertion with an explicit tolerance — for the
    /// hand-derived CONTROL values, whose paper arithmetic lands a few
    /// 1e-4 away from the f32 ray math.
    fn assert_uv_loose(uv: [f32; 2], u: f32, v: f32, tol: f32, what: &str) {
        assert!(
            (uv[0] - u).abs() < tol && (uv[1] - v).abs() < tol,
            "{what}: expected ({u}, {v}) +/- {tol}, got {uv:?}"
        );
    }

    #[test]
    fn stroke_resolves_against_the_start_camera_after_a_rotation() {
        // Pin 1, the frozen rule: the stroke opens head-on, the live
        // camera rotates, and the stroke's later events still resolve to
        // the START camera's UV — not the rotated camera's.
        let mut viewport = painting_viewport();
        let start = stroke_uv(&viewport, STROKE_PX).expect("head-on ray hits");
        assert_uv(start, START_U, 0.5, "the opening event (live == start)");
        let start_camera = viewport.camera;
        viewport.latch_stroke_camera(true, start_camera);

        viewport.camera.yaw = ROTATED_YAW;
        // The live camera really moved (else the pin is vacuous): its own
        // pick of the same pixel lands at the rotated UV.
        let live = viewport
            .pick_uv(rect200(), &quad(), STROKE_PX)
            .expect("rotated ray still hits the quad");
        // The control's expected value is HAND-DERIVED (the ray math in
        // f32 lands ~1.5e-4 away from the paper value), so it carries a
        // looser tolerance than the pins. What matters is that the live
        // camera's pick is a DIFFERENT point from the frozen one — that
        // is what makes the pin non-vacuous.
        assert_uv_loose(live, ROTATED_U, 0.5, 1e-3, "the live camera's pick");
        assert!(
            (live[0] - START_U).abs() > 0.05,
            "the live camera really moved: {live:?} vs {START_U}"
        );

        let during = stroke_uv(&viewport, STROKE_PX).expect("frozen ray hits");
        assert_uv(during, START_U, 0.5, "the mid-stroke event");
        let reference = viewport
            .pick_with(rect200(), &quad(), STROKE_PX, &start_camera)
            .map(|(uv, _)| uv)
            .expect("start camera's own pick");
        assert_eq!(during, reference, "the stroke IS the start camera's pick");
    }

    #[test]
    fn the_stroke_snapshot_is_independent_of_the_live_camera() {
        // Pin 2: mutating the live camera mid-stroke never reaches the
        // snapshot, a second latch mid-stroke never overwrites it, and the
        // stroke's end clears it.
        let mut viewport = painting_viewport();
        assert_eq!(viewport.stroke_camera, None, "no stroke, no snapshot");
        let (yaw0, pitch0, distance0) = (
            viewport.camera.yaw,
            viewport.camera.pitch,
            viewport.camera.distance,
        );
        viewport.latch_stroke_camera(true, viewport.camera);

        viewport.camera.orbit(0.9, 0.3);
        viewport.camera.zoom(0.5);
        viewport.camera.toggle_projection();
        assert_ne!(viewport.camera.yaw, yaw0, "the live camera moved");
        // A later event of the same stroke re-latches with the moved camera:
        // the snapshot must keep the start state.
        let moved = viewport.camera;
        viewport.latch_stroke_camera(true, moved);

        let snap = viewport.stroke_camera.expect("latched for the stroke");
        assert_eq!(snap.yaw, yaw0);
        assert_eq!(snap.pitch, pitch0);
        assert_eq!(snap.distance, distance0);
        assert_eq!(snap.projection, umber_gpu::camera::Projection::Perspective);
        assert_eq!(viewport.stroke_raycast_camera(), &snap);

        viewport.latch_stroke_camera(false, moved);
        assert_eq!(viewport.stroke_camera, None, "the stroke's end clears it");
        assert_eq!(
            viewport.stroke_raycast_camera(),
            &moved,
            "between strokes the resolve is the live camera"
        );
    }

    #[test]
    fn a_start_camera_hit_still_paints_after_the_live_camera_turns_away() {
        // Pin 4, miss-skip under a rotated camera: pixel x = 70 (s = -0.3,
        // k = -0.124264) hits head-on at x = -0.621320 ⇒ u = 0.189340.
        // Under yaw 1.0: den = 0.540302 - 0.124264·0.841471 = 0.435738,
        // x = -0.621320 / 0.435738 = -1.425906 — off the ±1 quad, a MISS.
        // The stroke still resolves (hit/miss is the start camera's).
        let pos = egui::pos2(70.0, 100.0);
        let mut viewport = painting_viewport();
        viewport.latch_stroke_camera(true, viewport.camera);
        viewport.camera.yaw = 1.0;
        assert_eq!(
            viewport.pick_uv(rect200(), &quad(), pos),
            None,
            "the live camera must miss here (else the pin is vacuous)"
        );
        let uv = stroke_uv(&viewport, pos).expect("the start camera hits");
        assert_uv(uv, 0.189_340, 0.5, "the frozen hit");

        // The converse: a start-camera miss stays a miss even where the
        // live camera would hit. Pixel x = 30 (s = -0.7, k = -0.289950):
        // head-on x = 5k = -1.449749, off the quad. The live camera then
        // dollies to distance 5/3, where the same pixel meets x = (5/3)k
        // = -0.483250 — a hit the frozen stroke must not take.
        let far = egui::pos2(30.0, 100.0);
        let mut viewport = painting_viewport();
        viewport.latch_stroke_camera(true, viewport.camera);
        viewport.camera.distance = 5.0 / 3.0;
        assert!(
            viewport.pick_uv(rect200(), &quad(), far).is_some(),
            "the zoomed live camera hits here"
        );
        assert_eq!(stroke_uv(&viewport, far), None, "the frozen camera misses");
    }

    /// A paint-capable device: the paint state needs
    /// `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, so the device request
    /// carries it and an adapter without it SKIPS the test (the
    /// perf_soak helper's pattern; the bakes panel's copy asks for a
    /// plain device, which the paint state rejects).
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return None;
        };
        if !adapter
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ..Default::default()
        }))
        .ok()
    }

    #[test]
    fn the_staged_dabs_read_back_mid_stroke_at_the_start_camera_uvs() {
        // Pin 3 (GPU-gated): the preview stays live. A real stroke through
        // `stroke_event` (head-on, x 80 → 82 px), then the camera rotates
        // and the stroke goes on at x = 82: the tile readback WHILE the
        // stroke is still open shows the dabs at the start camera's UVs,
        // and nothing at the rotated camera's UV for the same pixel.
        //
        // Texel math (1024² target, texel = round(uv · 1023)): x = 80 ⇒
        // u 0.292893 ⇒ texel 300; x = 82 (s = -0.18, k = -0.074558) ⇒
        // head-on u = (1 - 0.372792)/2 = 0.313604 ⇒ texel 321. Rotated to
        // yaw 0.7: den = 0.764842 - 0.074558·0.644218 = 0.716810, x =
        // -0.372792 / 0.716810 = -0.520071 ⇒ u 0.239965 ⇒ texel 245 — 55
        // texels left of the painted span, beyond the 12-texel brush
        // (`BRUSH_RADIUS_TEXELS` 24 × size 0.5). v = 0.5 ⇒ row 512.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint = PaintState::with_target_size(device, queue, 1024).expect("paint builds");
        paint.set_brush_color([1.0, 0.0, 0.0, 1.0]);
        paint.set_brush_size(0.5);
        let mut viewport = painting_viewport();
        let rect = rect200();

        let mut first = None;
        for i in 0..=4 {
            let pos = egui::pos2(80.0 + 0.5 * i as f32, 100.0);
            let uv = viewport.stroke_event(rect, &mut paint, pos, 1.0);
            first = first.or(uv);
        }
        assert!(paint.is_stroking(), "the stroke is open");
        assert_uv(first.expect("opening hit"), START_U, 0.5, "the opening UV");
        let start_camera = viewport.camera;
        assert_eq!(viewport.stroke_camera, Some(start_camera), "latched");

        viewport.camera.yaw = ROTATED_YAW;
        let edge = egui::pos2(82.0, 100.0);
        for _ in 0..4 {
            let uv = viewport
                .stroke_event(rect, &mut paint, edge, 1.0)
                .expect("the frozen ray still hits");
            assert_uv(uv, 0.313_604, 0.5, "a post-rotation event");
        }
        assert_eq!(viewport.stroke_camera, Some(start_camera), "kept");

        // Mid-stroke readback (no end_stroke yet): `read_tile` drains the
        // staged dabs first, so the open stroke is visible.
        assert!(paint.is_stroking(), "still mid-stroke at the readback");
        let image = paint.read_tile(1001).expect("tile 1001 reads back");
        assert_eq!((image.width, image.height), (1024, 1024));
        let texel = |x: u32, y: u32| {
            let i = (y * image.width + x) as usize * 4;
            [
                image.data[i],
                image.data[i + 1],
                image.data[i + 2],
                image.data[i + 3],
            ]
        };
        // The untouched reference: a far corner no dab reaches (whatever
        // the Base plane's clear value is, it is this).
        let untouched = texel(1000, 1000);
        // The painted SPAN along the stroke's row (row 512 = v 0.5).
        //
        // The pin is the SPAN, not the last point's exact texel: the dab
        // placement along a stroke is spacing-driven, so the final
        // point's own dab is not necessarily stamped while the stroke is
        // still open (the first battery run showed the span ending at
        // 318, three texels short of the frozen UV's 321 — the dabs are
        // there, the last texel is not). What the pin asserts is the
        // CONTRACT: the dabs live where the FROZEN camera said and never
        // where the rotated camera said.
        let span: Vec<u32> = (0..image.width)
            .filter(|&x| texel(x, 512) != untouched)
            .collect();
        assert!(!span.is_empty(), "the stroke shows mid-stroke");
        let first = *span.first().expect("non-empty span");
        let last = *span.last().expect("non-empty span");
        assert_eq!(
            span.len() as u32,
            last - first + 1,
            "the span is contiguous: {first}..{last}"
        );
        assert!(
            span.contains(&300),
            "the start-camera dab shows: {first}..{last}"
        );
        assert!(
            last >= 309,
            "the post-rotation dabs reach the FROZEN uv's texels: {first}..{last}"
        );
        assert!(
            first > 257,
            "nothing landed at the rotated camera's uv (245 +- the brush radius): {first}..{last}"
        );
        assert_eq!(
            texel(245, 512),
            untouched,
            "nothing lands at the rotated camera's UV"
        );
    }

    #[test]
    fn brush_cursor_probe_measures_the_quad() {
        // The cursor's probe on the head-on quad: an 8 px step
        // spans a measurable, non-seam UV distance, and the resulting ring
        // radius grows when the camera moves closer (the same brush covers
        // more pixels).
        use crate::brush_cursor::{screen_radius, texels_spanned, PROBE_PX};
        let radius_at = |viewport: &Viewport| {
            let a = viewport
                .pick_uv(rect200(), &quad(), egui::pos2(100.0, 100.0))
                .expect("centre hits");
            let b = viewport
                .pick_uv(rect200(), &quad(), egui::pos2(100.0 + PROBE_PX, 100.0))
                .expect("probe hits");
            let t = texels_spanned(a, b, 512.0).expect("measurable span");
            screen_radius(24.0, t)
        };
        let far = radius_at(&head_on_viewport());
        let mut near_vp = head_on_viewport();
        near_vp.camera.distance *= 0.5;
        let near = radius_at(&near_vp);
        assert!(far.is_finite() && far > 0.0, "{far}");
        assert!(
            near > far * 1.5,
            "closer camera, bigger ring: {near} vs {far}"
        );
    }

    #[test]
    fn straight_line_snaps_to_five_degree_rays() {
        let from = egui::pos2(0.0, 0.0);
        // No snap: the pointer itself.
        assert_eq!(
            straight_line_end(from, egui::pos2(10.0, 3.0), false),
            egui::pos2(10.0, 3.0)
        );
        // 2° off horizontal snaps to horizontal (the nearest 5° ray),
        // keeping the distance along it; 3° is nearer 5° and snaps there.
        let to = egui::pos2(100.0, 100.0 * 2f32.to_radians().tan());
        let s = straight_line_end(from, to, true);
        assert!(s.y.abs() < 1e-3 && (s.x - 100.0).abs() < 1e-3, "{s:?}");
        let to = egui::pos2(100.0, 100.0 * 3f32.to_radians().tan());
        let s = straight_line_end(from, to, true);
        assert!((s.y.atan2(s.x).to_degrees() - 5.0).abs() < 1e-3, "{s:?}");
        // 44° snaps to 45°.
        let a = 44f32.to_radians();
        let s = straight_line_end(from, egui::pos2(a.cos() * 50.0, a.sin() * 50.0), true);
        assert!((s.y.atan2(s.x).to_degrees() - 45.0).abs() < 1e-3, "{s:?}");
        // Degenerate (no length): unchanged.
        assert_eq!(straight_line_end(from, from, true), from);
    }

    #[test]
    fn line_samples_cover_both_ends_densely() {
        let pts = line_samples(egui::pos2(0.0, 0.0), egui::pos2(10.0, 0.0), 2.0);
        assert_eq!(pts.first(), Some(&egui::pos2(0.0, 0.0)));
        assert_eq!(pts.last(), Some(&egui::pos2(10.0, 0.0)));
        assert!(pts.windows(2).all(|w| (w[1] - w[0]).length() <= 2.0 + 1e-4));
        // A zero-length line still yields its point (a click paints a dab).
        assert_eq!(
            line_samples(egui::pos2(3.0, 3.0), egui::pos2(3.0, 3.0), 2.0).len(),
            2
        );
    }

    #[test]
    fn eyedropper_picks_the_display_chained_texel() {
        // B-10 follow-up: a known linear texel plus known non-default
        // display settings picks the exact bytes the artist sees. The
        // v1 step converts the linear texel to sRGB first ...
        let texel = [64u8, 128, 200, 173];
        let srgb = [
            linear_to_srgb_byte(texel[0]),
            linear_to_srgb_byte(texel[1]),
            linear_to_srgb_byte(texel[2]),
            texel[3],
        ];
        assert_eq!(srgb, [137, 188, 229, 173], "the v1 linear->sRGB step");
        // ... then the session chain runs on those display-encoded bytes
        // (the Display panel's preview-strip input shape).
        let settings = DisplaySettings {
            view: umber_color::DisplayTransform::Srgb,
            exposure: -0.5,
            gamma: 0.9,
        };
        let picked = display_chain_srgba(srgb, &settings);
        // Mirror `display_panel::preview_values`: `apply_display_chain`
        // on the sample, quantized like `display_panel::to_color32`.
        let expected = apply_display_chain(
            [
                f32::from(srgb[0]) / 255.0,
                f32::from(srgb[1]) / 255.0,
                f32::from(srgb[2]) / 255.0,
            ],
            &settings,
        )
        .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        assert_eq!(
            picked,
            [expected[0], expected[1], expected[2], texel[3]],
            "the pick must be the preview strip's chain on the sRGB sample"
        );
        // Hand-pinned from that arithmetic (linear_to_srgb_byte -> chain ->
        // quantize), so a drift in the input space, stage order or
        // rounding fails even if the two calls stay in lockstep.
        assert_eq!(picked, [158, 185, 204, 173]);
    }

    #[test]
    fn eyedropper_default_settings_keep_the_v1_srgb_bytes() {
        // `DisplaySettings::default()` is Raw / 0 EV / gamma 1 — the
        // bit-exact identity chain (read from umber-color's
        // `apply_display_chain`, not assumed): an untouched project's
        // pick is the v1 linear->sRGB sample, byte for byte.
        let settings = DisplaySettings::default();
        assert!(settings.is_identity());
        assert!(
            Viewport::default().display_settings.is_identity(),
            "a fresh viewport starts on the identity chain"
        );
        let srgba = [137, 188, 229, 173];
        assert_eq!(display_chain_srgba(srgba, &settings), srgba);
    }

    #[test]
    fn a_viewport_stroke_ends_when_the_paint_drag_stops() {
        // The 2026-10-15 bug: release never ended the stroke, so every
        // later Ctrl+drag extended it (one undo step per session).
        assert!(ends_viewport_stroke(false, true, true), "release ends it");
        assert!(!ends_viewport_stroke(true, true, true), "mid-drag keeps it");
        assert!(!ends_viewport_stroke(false, false, true), "nothing open");
        // A UV-view stroke in flight is not the viewport's to end.
        assert!(!ends_viewport_stroke(false, true, false));
    }

    #[test]
    fn pick_uv_corner_misses() {
        // The 2x2 quad at distance 5 under a 45° fov fills the middle of
        // the frame: the rect corner's ray leaves the quad → None, which
        // is what the Screen path skips (no orphan dabs).
        let viewport = head_on_viewport();
        assert_eq!(
            viewport.pick_uv(rect200(), &quad(), egui::pos2(0.0, 0.0)),
            None
        );
        assert_eq!(
            viewport.pick_uv(rect200(), &quad(), egui::pos2(200.0, 200.0)),
            None
        );
    }

    #[test]
    fn front_face_falloff_is_off_by_default_and_weighs_one() {
        // The derived Default must be OFF: a fresh viewport's pick carries
        // weight exactly 1.0 even on the quad's head-on centre, so every
        // existing stroke keeps its pressure byte-for-byte.
        let viewport = head_on_viewport();
        let (uv, w) = viewport
            .pick(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("centre hits");
        assert_eq!(w, 1.0);
        assert_eq!(
            Some(uv),
            viewport.pick_uv(rect200(), &quad(), egui::pos2(100.0, 100.0))
        );
    }

    #[test]
    fn front_face_falloff_weakens_a_grazing_face_and_spares_head_on() {
        // The quad faces +z; the head-on camera looks down -z, so the
        // centre ray is perpendicular (facing = 1): weight 1 at any start.
        let mut viewport = head_on_viewport();
        viewport.set_front_face_falloff_deg(Some(0.0));
        let (_, w) = viewport
            .pick(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("centre hits");
        assert!((w - 1.0).abs() < 1e-4, "head-on must be full strength: {w}");
        // Tilt the camera 60 degrees: the ray now meets the face at 60 deg
        // off its normal. With start 0, the weight is (90-60)/90 = 1/3.
        viewport.camera.pitch = 60f32.to_radians();
        let (_, w) = viewport
            .pick(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("centre still hits");
        assert!(
            (w - 1.0 / 3.0).abs() < 2e-3,
            "60 deg off normal, start 0: {w}"
        );
        // Start 70 covers 60 deg: full strength again.
        viewport.set_front_face_falloff_deg(Some(70.0));
        let (_, w) = viewport
            .pick(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("centre still hits");
        assert_eq!(w, 1.0);
        // Turned back off, and garbage turns it off.
        viewport.set_front_face_falloff_deg(Some(f32::NAN));
        viewport.set_front_face_falloff_deg(Some(0.0));
        viewport.set_front_face_falloff_deg(None);
        let (_, w) = viewport
            .pick(rect200(), &quad(), egui::pos2(100.0, 100.0))
            .expect("centre still hits");
        assert_eq!(w, 1.0);
    }

    #[test]
    fn default_view_mode_is_lit() {
        // Lit is today's render: the default must be the mode whose
        // uniform is (0, 0), so existing frames are untouched.
        assert_eq!(Viewport::default().view_mode(), ViewMode::Lit);
        assert_eq!(ViewMode::default(), ViewMode::Lit);
    }

    #[test]
    fn set_view_mode_round_trips() {
        let mut viewport = Viewport::default();
        viewport.set_view_mode(ViewMode::Unlit);
        assert_eq!(viewport.view_mode(), ViewMode::Unlit);
        viewport.set_view_mode(ViewMode::Solo(SoloChannel::Roughness));
        assert_eq!(viewport.view_mode(), ViewMode::Solo(SoloChannel::Roughness));
    }

    #[test]
    fn cage_overlay_defaults_off_and_toggles() {
        // Byte-identical-off contract: a fresh viewport draws the cage
        // overlay nowhere until the Bakes panel's toggle syncs on.
        let mut viewport = Viewport::default();
        assert!(!viewport.show_cage());
        viewport.set_show_cage(true);
        assert!(viewport.show_cage());
        viewport.set_show_cage(false);
        assert!(!viewport.show_cage());
    }

    #[test]
    fn ortho_pick_at_center_matches_perspective_pick() {
        // Parallel-vs-fan consistency: head-on at the quad, the ortho ray
        // through the rect centre must meet the quad's centre (0.5, 0.5) —
        // the same UV the perspective pick returns. An eye-origin ortho
        // ray would still hit centre here (symmetry), so this also pins
        // the exact UV rather than just hitting; the off-centre geometry
        // is what distinguishes the origins, and the camera tests pin the
        // ortho matrix itself.
        let quad_mesh = quad();
        let center = egui::pos2(100.0, 100.0);
        let perspective_uv = head_on_viewport()
            .pick_uv(rect200(), &quad_mesh, center)
            .expect("perspective center ray must hit the quad");
        let mut ortho_viewport = head_on_viewport();
        ortho_viewport.camera.projection = umber_gpu::camera::Projection::Orthographic;
        let ortho_uv = ortho_viewport
            .pick_uv(rect200(), &quad_mesh, center)
            .expect("ortho center ray must hit the quad");
        for (got, name) in [(perspective_uv, "perspective"), (ortho_uv, "ortho")] {
            assert!(
                (got[0] - 0.5).abs() < 1e-4 && (got[1] - 0.5).abs() < 1e-4,
                "{name} center must map to center UV, got {got:?}"
            );
        }
        assert!(
            (ortho_uv[0] - perspective_uv[0]).abs() < 1e-5
                && (ortho_uv[1] - perspective_uv[1]).abs() < 1e-5,
            "ortho {ortho_uv:?} must match perspective {perspective_uv:?}"
        );
    }

    /// Two quads sharing the x = 0 edge, the right one on its own UV
    /// island (a cut along x = 0), plus one island-internal diagonal per
    /// quad: exactly one seam edge, from (0,-1,0) to (0,1,0).
    fn two_island_strip() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                // Left quad (island A): x in -1..0.
                [-1.0, -1.0, 0.0],
                [0.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
                // Right quad (island B): x in 0..1, its x = 0 corners
                // duplicated (same positions, different UVs).
                [0.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 8],
            uvs: vec![
                [0.0, 0.0],
                [0.4, 0.0],
                [0.4, 1.0],
                [0.0, 1.0],
                [0.6, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.6, 1.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
            material_names: vec![],
            triangle_materials: Vec::new(),
        }
    }

    #[test]
    fn seam_extraction_finds_exactly_the_uv_cut() {
        let segs = uv_seam_segments(&two_island_strip());
        assert_eq!(segs.len(), 1, "{segs:?}");
        let mut ends = segs[0].ends;
        ends.sort_by(|a, b| a.y.total_cmp(&b.y));
        assert_eq!(ends[0], glam::Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(ends[1], glam::Vec3::new(0.0, 1.0, 0.0));
        // Both faces wind CCW toward +z.
        for n in segs[0].normals {
            assert!(n.z > 0.0 && n.x == 0.0 && n.y == 0.0, "{n:?}");
        }
        // A single-island mesh (the unit quad) has no seams — its
        // diagonal is island-internal and its border is open, not cut.
        assert!(uv_seam_segments(&quad()).is_empty());
        // Continuous UVs across the shared edge: the cut disappears.
        let mut welded = two_island_strip();
        welded.uvs[4] = welded.uvs[1];
        welded.uvs[7] = welded.uvs[2];
        assert!(uv_seam_segments(&welded).is_empty());
        // An empty mesh is empty, not a panic.
        let empty = umber_mesh::MeshData {
            positions: vec![],
            normals: vec![],
            uvs: vec![],
            indices: vec![],
            material_names: vec![],
            triangle_materials: Vec::new(),
        };
        assert!(uv_seam_segments(&empty).is_empty());
    }

    #[test]
    fn face_side_decides_front_back_and_edge_on() {
        let n = glam::Vec3::Z;
        assert_eq!(
            face_side(n, glam::Vec3::new(0.0, 0.0, 5.0)),
            FaceSide::Front
        );
        assert_eq!(
            face_side(n, glam::Vec3::new(0.3, 0.2, 1.0)),
            FaceSide::Front
        );
        assert_eq!(
            face_side(n, glam::Vec3::new(0.0, 0.0, -5.0)),
            FaceSide::Back
        );
        // Edge-on and degenerate read as Back (what culling hides).
        assert_eq!(face_side(n, glam::Vec3::X), FaceSide::Back);
        assert_eq!(face_side(glam::Vec3::ZERO, glam::Vec3::Z), FaceSide::Back);
    }

    #[test]
    fn seams_hide_when_both_faces_point_away() {
        let seg = uv_seam_segments(&two_island_strip())[0];
        // Eye on +z: the faces' fronts. Eye on -z: both backs.
        assert!(seg.visible(glam::Vec3::new(0.0, 0.0, 5.0), None));
        assert!(!seg.visible(glam::Vec3::new(0.0, 0.0, -5.0), None));
        // Orthographic: the view direction decides, not the eye offset.
        assert!(seg.visible(glam::Vec3::new(9.0, 0.0, -5.0), Some(-glam::Vec3::Z)));
        assert!(!seg.visible(glam::Vec3::new(0.0, 0.0, 5.0), Some(glam::Vec3::Z)));
        // One face flipped: the seam borders a front face from either
        // side, so it shows both ways (any face, not all faces).
        let mixed = SeamSegment {
            normals: [glam::Vec3::Z, -glam::Vec3::Z],
            ..seg
        };
        assert!(mixed.visible(glam::Vec3::new(0.0, 0.0, 5.0), None));
        assert!(mixed.visible(glam::Vec3::new(0.0, 0.0, -5.0), None));
    }

    #[test]
    fn seam_endpoints_project_into_the_rect() {
        // Head-on at the strip: the cut (x = 0) is the vertical centre
        // line of the 200-px rect; a point behind the eye projects to None.
        let vp = head_on_viewport();
        let view_proj = vp.camera.view_proj(1.0);
        let top = project_to_screen(view_proj, rect200(), glam::Vec3::new(0.0, 1.0, 0.0))
            .expect("in front of the eye");
        let bottom = project_to_screen(view_proj, rect200(), glam::Vec3::new(0.0, -1.0, 0.0))
            .expect("in front of the eye");
        assert!((top.x - 100.0).abs() < 1e-3 && (bottom.x - 100.0).abs() < 1e-3);
        assert!(top.y < 100.0 && bottom.y > 100.0, "{top:?} {bottom:?}");
        assert_eq!(
            project_to_screen(view_proj, rect200(), glam::Vec3::new(0.0, 0.0, 50.0)),
            None
        );
    }

    #[test]
    fn wire_opacity_keeps_the_default_color_at_the_default() {
        use crate::display_panel::OverlaySettings;
        let default = umber_gpu::WireColor::default().color;
        assert_eq!(
            wire_color_for(OverlaySettings::DEFAULT_WIRE_OPACITY).color,
            default
        );
        let half = wire_color_for(0.5).color;
        assert_eq!(&half[..3], &default[..3], "rgb untouched");
        assert_eq!(half[3], 0.5);
        assert_eq!(wire_color_for(7.0).color[3], 1.0);
        assert_eq!(wire_color_for(-1.0).color[3], 0.0);
        assert_eq!(wire_color_for(f32::NAN).color, default);
    }

    #[test]
    fn overlay_state_round_trips_through_the_shell_setters() {
        let mut vp = Viewport::default();
        assert_eq!(
            vp.overlays(),
            crate::display_panel::OverlaySettings::default()
        );
        // The shell's W / G sync lands in the popover's struct.
        vp.set_show_wireframe(true);
        vp.set_show_grid(true);
        assert!(vp.overlays().wireframe && vp.overlays().grid);
        // Nothing was edited in the popover: nothing to take.
        assert_eq!(vp.take_overlay_edit(), None);
        // A popover edit is taken once.
        vp.overlay_edited = true;
        assert_eq!(vp.take_overlay_edit(), Some(vp.overlays()));
        assert_eq!(vp.take_overlay_edit(), None);
        // set_overlays drives the draw flags and clamps the opacity.
        vp.set_overlays(crate::display_panel::OverlaySettings {
            wireframe: false,
            grid: false,
            wireframe_opacity: 3.0,
            ..crate::display_panel::OverlaySettings::default()
        });
        assert!(!vp.show_wireframe && !vp.show_grid);
        assert_eq!(vp.overlays().wireframe_opacity, 1.0);
        vp.set_face_orientation(true);
        assert!(vp.face_orientation());
    }

    #[test]
    fn readout_names_only_soloed_modes() {
        let tr = |k: &str| format!("<{k}>");
        assert_eq!(readout_text(ViewMode::Lit, true, tr), None);
        assert_eq!(readout_text(ViewMode::Unlit, true, tr), None);
        let solo = ViewMode::Solo(SoloChannel::Roughness);
        assert_eq!(
            readout_text(solo, true, tr).as_deref(),
            Some("<viewkeys.toast-channel> <viewmode.roughness>")
        );
        assert_eq!(readout_text(solo, false, tr), None);
        let map = ViewMode::MeshMapPreview(umber_mesh::MeshMapKind::Curvature);
        assert_eq!(
            readout_text(map, true, tr).as_deref(),
            Some("<viewkeys.toast-map> curvature")
        );
    }

    #[test]
    fn ortho_off_centre_pick_uses_the_near_point_origin() {
        // The test that separates the two origins. Default distance 5,
        // fov 45°: ortho half-height at the target plane = 5·tan(22.5°) =
        // 2.0710678, so pixel x = 140 in the 200-px rect (NDC x = 0.4)
        // meets the quad at world x = 0.4 × 2.0710678 = 0.8284271 →
        // u = (0.8284271 + 1) / 2 = 0.9142136 (positions == UVs on x/y).
        // An eye-origin ray with the parallel direction (0, 0, -1) would
        // drop straight onto (0, 0) → u = 0.5 — off by 0.41.
        let mut viewport = head_on_viewport();
        viewport.camera.projection = umber_gpu::camera::Projection::Orthographic;
        let uv = viewport
            .pick_uv(rect200(), &quad(), egui::pos2(140.0, 100.0))
            .expect("the off-centre ortho ray must hit the quad");
        assert!(
            (uv[0] - 0.914_213_6).abs() < 1e-4 && (uv[1] - 0.5).abs() < 1e-4,
            "ortho off-centre pick must land at u = 0.9142136, got {uv:?}"
        );
    }

    /// Both coordinates within `tol`.
    fn near(a: [f32; 2], b: [f32; 2], tol: f32) -> bool {
        (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
    }

    /// The four corners pairwise within `tol`.
    fn quad_near(got: [[f32; 2]; 4], want: [[f32; 2]; 4], tol: f32) -> bool {
        got.iter().zip(want.iter()).all(|(g, w)| near(*g, *w, tol))
    }

    /// The inverse test helper: viewport px -> image px, the core's own
    /// `inverse_map` (the exact inverse of the map `screen_quad` uses).
    fn image_point_of(t: &StencilTransform, dims: [u32; 2], screen: [f32; 2]) -> [f32; 2] {
        umber_core::stencil::inverse_map(t, dims, screen)
    }

    /// A stencil sitting at viewport px (100, 100), unit scale, turned
    /// 0.3 rad: a non-quarter start, so snapping the result and snapping
    /// the delta disagree.
    const DRAG_START: StencilTransform = StencilTransform {
        center: [100.0, 100.0],
        scale: 1.0,
        rotation: 0.3,
    };

    /// The pointer 50 px from DRAG_START's center at `angle` (y-down:
    /// positive angles turn clockwise).
    fn at_angle(angle: f32) -> [f32; 2] {
        [100.0 + 50.0 * angle.cos(), 100.0 + 50.0 * angle.sin()]
    }

    fn rotate(snap: bool, from: [f32; 2], to: [f32; 2]) -> StencilTransform {
        let chord = StencilChord::Rotate { snap };
        stencil_drag(&DRAG_START, chord, from, to, [64, 64], [800.0, 600.0])
    }

    #[test]
    fn screen_quad_at_identity_is_the_image_bounds_centered_at_the_origin() {
        // Default transform: center (0,0), scale 1, rotation 0. A 4x2
        // image's corners sit at ±(w/2, h/2) = ±(2, 1); sin 0 = 0 and
        // cos 0 = 1 exactly, so the pin is exact.
        let quad = screen_quad(&StencilTransform::default(), 4, 2);
        assert_eq!(quad, [[-2.0, -1.0], [2.0, -1.0], [2.0, 1.0], [-2.0, 1.0]]);
    }

    #[test]
    fn screen_quad_quarter_turn_permutes_the_corners_clockwise() {
        // θ = π/2, center (10, 20), 4x2 image. forward: c + R·d with
        // R·(dx, dy) = (cos·dx − sin·dy, sin·dx + cos·dy) = (−dy, dx):
        //   (0,0): d = (−2,−1) -> (1, −2)  -> (11, 18)
        //   (4,0): d = ( 2,−1) -> (1,  2)  -> (11, 22)
        //   (4,2): d = ( 2, 1) -> (−1, 2)  -> ( 9, 22)
        //   (0,2): d = (−2, 1) -> (−1,−2)  -> ( 9, 18)
        // The top-left corner lands top-RIGHT of the turned 2x4 box: a
        // clockwise turn on the y-down screen. f32 cos(π/2) ≈ −4.4e-8,
        // so each coordinate is ~1e-7 off: 1e-5 tolerance.
        let t = StencilTransform {
            center: [10.0, 20.0],
            scale: 1.0,
            rotation: std::f32::consts::FRAC_PI_2,
        };
        let quad = screen_quad(&t, 4, 2);
        let want = [[11.0, 18.0], [11.0, 22.0], [9.0, 22.0], [9.0, 18.0]];
        assert!(quad_near(quad, want, 1e-5), "{quad:?}");
    }

    #[test]
    fn screen_quad_at_scale_two_doubles_the_extent() {
        let t = StencilTransform {
            scale: 2.0,
            ..StencilTransform::default()
        };
        let quad = screen_quad(&t, 4, 2);
        assert_eq!(quad, [[-4.0, -2.0], [4.0, -2.0], [4.0, 2.0], [-4.0, 2.0]]);
    }

    #[test]
    fn frame_mesh_centers_the_camera_on_the_mesh_bounds() {
        let mut vp = head_on_viewport();
        vp.mesh_data = Some(quad()); // bounds (-1,-1,0)..(1,1,0)
        vp.camera.distance = 500.0;
        vp.frame_mesh();
        let c = vp.camera.target;
        assert!(c.x.abs() < 1e-4 && c.y.abs() < 1e-4 && c.z.abs() < 1e-4);
        // Framed: the unit sphere at distance ~1.15/tan(fov/2) fits.
        assert!(vp.camera.distance < 500.0);
        // No mesh: the camera does not move.
        let mut vp2 = head_on_viewport();
        let d0 = vp2.camera.distance;
        vp2.frame_mesh();
        assert_eq!(vp2.camera.distance, d0);
    }

    #[test]
    fn angle_chord_sweeps_linearly_and_clamps() {
        assert!((angle_step(0.0, 0.0) - 0.0).abs() < 1e-6);
        // 100 px up = +90 deg (400 px per full sweep), down = -90.
        assert!((angle_step(0.0, -100.0) - 90.0).abs() < 1e-4);
        assert!((angle_step(0.0, 100.0) + 90.0).abs() < 1e-4);
        // Clamps at the editor's range.
        assert_eq!(angle_step(170.0, -100.0), 180.0);
        assert_eq!(angle_step(-170.0, 100.0), -180.0);
    }

    #[test]
    fn flow_chord_scales_alpha_exponentially_and_clamps() {
        assert!((flow_alpha_step(0.5, 0.0) - 0.5).abs() < 1e-6);
        // +200 px doubles.
        assert!((flow_alpha_step(0.5, 200.0) - 1.0).abs() < 1e-4);
        // -200 px halves; the clamp holds at the floor.
        assert!((flow_alpha_step(0.5, -200.0) - 0.25).abs() < 1e-6);
        assert_eq!(flow_alpha_step(0.01, -10000.0), 0.01);
        assert_eq!(flow_alpha_step(1.0, 10000.0), 1.0);
    }

    #[test]
    fn bracket_steps_grow_and_shrink_the_size() {
        assert!((bracket_size_step(1.0, true) - 1.25).abs() < 1e-6);
        assert!((bracket_size_step(1.0, false) - 0.8).abs() < 1e-6);
        // Growing past the sanitize clamp saturates, not overflows.
        assert!(bracket_size_step(8.0, true) > 8.0); // clamped by the caller
    }

    #[test]
    fn rmb_cancel_arms_on_rmb_beside_drag_holds_until_release() {
        use PointerButton::*;
        // RMB beside an LMB drag cancels the LMB.
        assert_eq!(
            rmb_cancel_step(None, Some(Primary), true, false),
            Some(Primary)
        );
        // RMB dragged alone cancels nothing.
        assert_eq!(rmb_cancel_step(None, Some(Secondary), true, false), None);
        // No RMB, LMB still held: a prior cancel holds.
        assert_eq!(
            rmb_cancel_step(Some(Primary), Some(Primary), false, true),
            Some(Primary)
        );
        // Button released: the cancel clears.
        assert_eq!(
            rmb_cancel_step(Some(Primary), Some(Primary), false, false),
            None
        );
        // RMB beside the RMB drag itself: prior cancel on a still-down
        // LMB holds.
        assert_eq!(
            rmb_cancel_step(Some(Primary), Some(Secondary), true, true),
            Some(Primary)
        );
    }

    #[test]
    fn cursor_ndc_maps_center_to_zero_and_corners_to_unit() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 100.0));
        let c = cursor_ndc(rect, rect.center());
        assert!(c.x.abs() < 1e-6 && c.y.abs() < 1e-6);
        // Top-right corner: NDC (+1, +1) — y flipped from egui's down.
        let tr = cursor_ndc(rect, rect.right_top());
        assert!((tr.x - 1.0).abs() < 1e-6 && (tr.y - 1.0).abs() < 1e-6);
        let bl = cursor_ndc(rect, rect.left_bottom());
        assert!((bl.x + 1.0).abs() < 1e-6 && (bl.y + 1.0).abs() < 1e-6);
    }

    #[test]
    fn screen_quad_corners_invert_to_the_image_corners() {
        // An arbitrary transform: each corner maps back through the
        // core's inverse to its image px corner, and the corners average
        // to the center. Magnitudes ~400 px put the f32 ulp near 3e-5, so
        // 1e-3 holds any real map error out.
        let t = StencilTransform {
            center: [320.0, 240.0],
            scale: 1.5,
            rotation: 0.7,
        };
        let quad = screen_quad(&t, 200, 100);
        let corners = [[0.0, 0.0], [200.0, 0.0], [200.0, 100.0], [0.0, 100.0]];
        for (screen, image) in quad.iter().zip(corners) {
            let back = image_point_of(&t, [200, 100], *screen);
            assert!(near(back, image, 1e-3), "{screen:?} -> {back:?}");
        }
        let mean = [
            quad.iter().map(|p| p[0]).sum::<f32>() / 4.0,
            quad.iter().map(|p| p[1]).sum::<f32>() / 4.0,
        ];
        assert!(near(mean, t.center, 1e-3), "{mean:?}");
    }

    #[test]
    fn rotate_drag_turns_by_the_arc_angle() {
        // From angle 0 (east of the center) to π/2 (south: clockwise on
        // the y-down screen): +π/2. atan2(0, 50) = 0 and atan2(50, 0) =
        // π/2 are exact, so only the one add rounds.
        let t = rotate(false, [150.0, 100.0], [100.0, 150.0]);
        let want = 0.3 + std::f32::consts::FRAC_PI_2;
        assert!((t.rotation - want).abs() < 1e-6, "{}", t.rotation);
        assert_eq!((t.center, t.scale), (DRAG_START.center, DRAG_START.scale));
        // The reverse arc turns the other way.
        let t = rotate(false, [100.0, 150.0], [150.0, 100.0]);
        let want = 0.3 - std::f32::consts::FRAC_PI_2;
        assert!((t.rotation - want).abs() < 1e-6, "{}", t.rotation);
        // A generic arc a1 = 0.4 -> a2 = 1.1 turns by 0.7 (the pointer px
        // round to ~4e-6, i.e. ~1e-7 rad at 50 px).
        let t = rotate(false, at_angle(0.4), at_angle(1.1));
        assert!((t.rotation - 1.0).abs() < 1e-5, "{}", t.rotation);
    }

    #[test]
    fn shift_snaps_the_resulting_angle_to_quarter_turns() {
        // 0.3 + 1.2 = 1.5 -> 1.5 / (π/2) = 0.955 -> 1 quarter: exactly
        // π/2. Snapping the DELTA instead gives 0.3 + π/2 = 1.87.
        let t = rotate(true, at_angle(0.0), at_angle(1.2));
        assert_eq!(t.rotation, std::f32::consts::FRAC_PI_2);
        // 0.3 − 0.2 = 0.1 -> 0 quarters: exactly 0.
        let t = rotate(true, at_angle(0.0), at_angle(-0.2));
        assert_eq!(t.rotation, 0.0);
        // 0.3 − 1.9 = −1.6 -> −1.019 -> −1 quarter: exactly −π/2 (the
        // in-range wrap passes it through untouched).
        let t = rotate(true, at_angle(0.0), at_angle(-1.9));
        assert_eq!(t.rotation, -std::f32::consts::FRAC_PI_2);
        // No snap: the raw 1.5.
        let t = rotate(false, at_angle(0.0), at_angle(1.2));
        assert!((t.rotation - 1.5).abs() < 1e-5, "{}", t.rotation);
    }

    #[test]
    fn rotate_holds_when_the_pointer_sits_on_the_center() {
        // atan2 of a zero arm is undefined (it reads 0): the rotation
        // must hold, not jump to the arc from angle 0.
        let center = DRAG_START.center;
        assert_eq!(rotate(false, [150.0, 120.0], center).rotation, 0.3);
        assert_eq!(rotate(false, center, [150.0, 120.0]).rotation, 0.3);
    }

    #[test]
    fn rotation_wraps_into_minus_pi_to_pi() {
        // 3.0 + 0.5 = 3.5 > π -> 3.5 − 2π = −2.7831853.
        let start = StencilTransform {
            rotation: 3.0,
            ..DRAG_START
        };
        let chord = StencilChord::Rotate { snap: false };
        let t = stencil_drag(
            &start,
            chord,
            at_angle(0.0),
            at_angle(0.5),
            [64, 64],
            [800.0, 600.0],
        );
        assert!((t.rotation - (3.5 - std::f32::consts::TAU)).abs() < 1e-5);
    }

    #[test]
    fn scale_drag_follows_the_horizontal_delta_monotonically() {
        // A 200-px-wide image: half-width 100 image px. scale' = 1 +
        // dx / 100 (exact in binary for these deltas).
        let scale_at = |dx: f32, dy: f32| {
            let t = stencil_drag(
                &StencilTransform {
                    rotation: 0.0,
                    ..DRAG_START
                },
                StencilChord::Scale,
                [200.0, 100.0],
                [200.0 + dx, 100.0 + dy],
                [200, 50],
                [800.0, 600.0],
            );
            assert_eq!((t.center, t.rotation), (DRAG_START.center, 0.0));
            t.scale
        };
        assert_eq!(scale_at(50.0, 0.0), 1.5);
        assert_eq!(scale_at(100.0, 0.0), 2.0);
        assert_eq!(scale_at(-50.0, 0.0), 0.5);
        // Vertical motion is ignored.
        assert_eq!(scale_at(0.0, 80.0), 1.0);
        // Strictly increasing in dx above the clamp.
        let sweep: Vec<f32> = (-8..=8).map(|k| scale_at(k as f32 * 10.0, 0.0)).collect();
        assert!(sweep.windows(2).all(|w| w[1] > w[0]), "{sweep:?}");
        // Dragged from the right edge (x = 100 + 100·1 = 200), the edge
        // stays under the pointer: scale 1.5 puts it at 100 + 150 = 250.
        let t = StencilTransform {
            scale: scale_at(50.0, 0.0),
            rotation: 0.0,
            ..DRAG_START
        };
        assert_eq!(screen_quad(&t, 200, 50)[1][0], 250.0);
    }

    #[test]
    fn scale_drag_clamps_at_the_minimum() {
        let scale_at = |dx: f32| {
            let current = [200.0 + dx, 100.0];
            stencil_drag(
                &DRAG_START,
                StencilChord::Scale,
                [200.0, 100.0],
                current,
                [200, 50],
                [800.0, 600.0],
            )
            .scale
        };
        // 1 − 150/100 = −0.5 and 1 − 10 = −9: both clamp.
        assert_eq!(scale_at(-150.0), MIN_STENCIL_SCALE);
        assert_eq!(scale_at(-1000.0), MIN_STENCIL_SCALE);
    }

    #[test]
    fn move_drag_translates_by_the_delta_inside_the_view() {
        let moved = |to: [f32; 2]| {
            let t = stencil_drag(
                &DRAG_START,
                StencilChord::Move,
                [10.0, 10.0],
                to,
                [64, 64],
                [200.0, 150.0],
            );
            assert_eq!((t.scale, t.rotation), (1.0, 0.3));
            t.center
        };
        // Delta (30, −20): (100, 100) -> (130, 80).
        assert_eq!(moved([40.0, -10.0]), [130.0, 80.0]);
        // Flung past either edge: clamped to the 200x150 view.
        assert_eq!(moved([1000.0, 1000.0]), [200.0, 150.0]);
        assert_eq!(moved([-1000.0, -1000.0]), [0.0, 0.0]);
    }

    #[test]
    fn each_mouse_button_maps_to_its_chord() {
        assert_eq!(
            StencilChord::for_button(PointerButton::Primary),
            Some(StencilChord::Rotate { snap: false })
        );
        assert_eq!(
            StencilChord::for_button(PointerButton::Secondary),
            Some(StencilChord::Scale)
        );
        assert_eq!(
            StencilChord::for_button(PointerButton::Middle),
            Some(StencilChord::Move)
        );
        assert_eq!(StencilChord::for_button(PointerButton::Extra1), None);
        // The latch keeps its drag alive on the same button.
        for b in [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
        ] {
            let chord = StencilChord::for_button(b).expect("bound");
            assert_eq!(chord.button(), b);
        }
        // Shift rides Rotate only.
        let rotate = StencilChord::Rotate { snap: false };
        assert_eq!(rotate.with_snap(true), StencilChord::Rotate { snap: true });
        assert_eq!(StencilChord::Scale.with_snap(true), StencilChord::Scale);
    }

    /// A `w` x `h` stencil whose texel `(x, y)` is covered iff `on(x, y)`.
    fn stencil_image(w: u32, h: u32, on: impl Fn(u32, u32) -> bool) -> StencilImage {
        let rgba: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| [0, 0, 0, if on(x, y) { 255 } else { 0 }])
            .collect();
        StencilImage::from_alpha(w, h, &rgba).expect("valid stencil")
    }

    #[test]
    fn overlay_rgba_tints_the_coverage_and_caps_the_side() {
        // 4x4, the diagonal covered: under the cap, texel for texel.
        let image = stencil_image(4, 4, |x, y| x == y);
        let (size, rgba) = stencil_overlay_rgba(&image);
        assert_eq!(size, [4, 4]);
        let [r, g, b] = STENCIL_TINT;
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let on = i % 4 == i / 4;
            let want = if on { [r, g, b, 255] } else { [0; 4] };
            assert_eq!(px, &want[..], "texel {i}");
        }
        // 1030 wide: step = ceil(1030 / 512) = 3 -> ceil(1030 / 3) = 344
        // texels sampled at x = 0, 3, 6, … — exactly the covered columns
        // (a floor step of 2 would sample x = 2, which is clear).
        let image = stencil_image(1030, 1, |x, _| x % 3 == 0);
        let (size, rgba) = stencil_overlay_rgba(&image);
        assert_eq!(size, [344, 1]);
        assert!(rgba.chunks_exact(4).all(|px| px[3] == 255));
        // 3x600, the even rows covered: step 2 -> 2x300, sampling rows
        // 0, 2, 4, … (all covered). The row stride is the WIDTH: a stride
        // of the height would index past the coverage and read clear.
        let image = stencil_image(3, 600, |_, y| y % 2 == 0);
        let (size, rgba) = stencil_overlay_rgba(&image);
        assert_eq!(size, [2, 300]);
        assert!(rgba.chunks_exact(4).all(|px| px[3] == 255));
    }

    // --- projection-fill row 280: the projector's chords + Apply ---

    use umber_app::projection::RgbaImage;

    /// The chords' starting transform for both targets: centered in the
    /// 200 px view at unit scale.
    const CHORD_START: StencilTransform = StencilTransform {
        center: [100.0, 100.0],
        scale: 1.0,
        rotation: 0.0,
    };

    /// One S-held chord frame: `started` begins a drag this frame,
    /// `held` is the MMB still dragging, the pointer sits at `pos`.
    fn s_frame(started: Option<PointerButton>, pos: [f32; 2], held: bool) -> ChordFrame {
        ChordFrame {
            s_held: true,
            shift: false,
            n_pressed: false,
            started,
            origin: Some([100.0, 100.0]),
            pos: Some(pos),
            dragged_by: [false, false, held],
            dragged: held,
            view: [200.0, 200.0],
        }
    }

    fn active_stencil() -> StencilState {
        StencilState {
            image: Some(stencil_image(4, 4, |_, _| true)),
            transform: CHORD_START,
            active: true,
            ..StencilState::default()
        }
    }

    fn opaque_projection(armed: bool) -> ProjectionState {
        ProjectionState {
            image: Some(RgbaImage::new(2, 2, vec![255; 16]).expect("2x2 RGBA")),
            transform: CHORD_START,
            armed,
            ..ProjectionState::default()
        }
    }

    /// An S+MMB move from (100, 100) to (130, 120): press, drag, release.
    /// Returns the ownership flag of each of the three frames.
    fn run_move(
        viewport: &mut Viewport,
        stencil: &mut StencilState,
        projection: &mut ProjectionState,
    ) -> [bool; 3] {
        let frames = [
            s_frame(Some(PointerButton::Middle), [100.0, 100.0], true),
            s_frame(None, [130.0, 120.0], true),
            s_frame(None, [130.0, 120.0], false),
        ];
        frames.map(|f| viewport.chord_input(&f, Some(&mut *stencil), Some(&mut *projection), None))
    }

    #[test]
    fn armed_projection_takes_the_chords_and_unarmed_leaves_them_to_the_stencil() {
        // Move = start center + the pointer delta (30, 20), inside the
        // 200 px view so the clamp is idle: (130, 120) exactly.
        let moved = [130.0, 120.0];
        let mut viewport = Viewport::default();
        let mut stencil = active_stencil();
        let mut projection = opaque_projection(true);
        let owns = run_move(&mut viewport, &mut stencil, &mut projection);
        assert_eq!(owns, [true, true, true], "the chord owns every frame");
        assert_eq!(
            projection.transform.center, moved,
            "armed: projection moves"
        );
        assert_eq!(stencil.transform, CHORD_START, "armed: stencil untouched");
        assert_eq!(viewport.projection_drag, None, "released");
        assert_eq!(viewport.stencil_drag, None, "the stencil never latched");

        // Unarmed: the same gesture drives the stencil, not the projection.
        let mut viewport = Viewport::default();
        let mut stencil = active_stencil();
        let mut projection = opaque_projection(false);
        let owns = run_move(&mut viewport, &mut stencil, &mut projection);
        assert_eq!(owns, [true, true, true]);
        assert_eq!(stencil.transform.center, moved, "unarmed: stencil moves");
        assert_eq!(
            projection.transform, CHORD_START,
            "unarmed: projection untouched"
        );
        assert_eq!(
            viewport.projection_drag, None,
            "the projection never latched"
        );

        // Armed without an image: nothing to map, the stencil keeps them.
        let mut viewport = Viewport::default();
        let mut stencil = active_stencil();
        let mut projection = ProjectionState {
            image: None,
            ..opaque_projection(true)
        };
        run_move(&mut viewport, &mut stencil, &mut projection);
        assert_eq!(
            stencil.transform.center, moved,
            "imageless arm: stencil moves"
        );
        assert_eq!(projection.transform, CHORD_START);
    }

    #[test]
    fn a_stencil_drag_in_flight_keeps_running_when_the_projection_arms() {
        let mut viewport = Viewport::default();
        let mut stencil = active_stencil();
        let mut projection = opaque_projection(false);
        let press = s_frame(Some(PointerButton::Middle), [100.0, 100.0], true);
        assert!(viewport.chord_input(&press, Some(&mut stencil), Some(&mut projection), None));
        assert!(viewport.stencil_drag.is_some(), "the stencil latched");
        // The projection arms mid-gesture: the stencil's drag runs on,
        // and the projection does not latch alongside it.
        projection.armed = true;
        let drag = s_frame(None, [130.0, 120.0], true);
        assert!(viewport.chord_input(&drag, Some(&mut stencil), Some(&mut projection), None));
        assert_eq!(stencil.transform.center, [130.0, 120.0]);
        assert_eq!(projection.transform, CHORD_START);
        assert_eq!(viewport.projection_drag, None, "never two drags at once");
    }

    #[test]
    fn projection_apply_writes_through_the_frozen_camera() {
        // GPU-gated (the paint state needs a device). Arm head-on, then
        // turn the LIVE camera: Apply's texels are the frozen placement's
        // raster byte for byte, and differ from the turned camera's.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint = PaintState::with_target_size(device, queue, 8).expect("paint builds");
        let mesh = quad();
        let mut viewport = painting_viewport();
        let rect = rect200();
        // Left half red, right half blue, 8x8, scale 25 = the whole
        // 200 px view: every on-quad texel (screen ~52..148 px at the
        // fixture's distance 5) samples the image interior, alpha 255.
        let (red, blue) = ([255u8, 0, 0, 255], [0u8, 0, 255, 255]);
        let mut rgba = Vec::new();
        for _y in 0..8 {
            for x in 0..8 {
                rgba.extend_from_slice(if x < 4 { &red } else { &blue });
            }
        }
        let mut projection = ProjectionState {
            image: Some(RgbaImage::new(8, 8, rgba).expect("8x8 RGBA")),
            transform: StencilTransform {
                center: [100.0, 100.0],
                scale: 25.0,
                rotation: 0.0,
            },
            opacity: 1.0,
            ..ProjectionState::default()
        };
        projection.arm(viewport.camera, rect);

        let tile = paint.active_tile();
        let (w, h) = paint.quick_mask_for_tile(tile).dimensions();
        let frozen = rasterize_projection(&projection, &mesh, tile, w, h).expect("frozen raster");
        assert!(
            frozen.chunks_exact(4).all(|t| t[3] == 255),
            "every texel opaque (the exact-replace precondition)"
        );
        let turned_camera = OrbitCamera {
            yaw: ROTATED_YAW,
            ..viewport.camera
        };
        let mut turned = projection.clone();
        turned.arm(turned_camera, rect);
        let live = rasterize_projection(&turned, &mesh, tile, w, h).expect("turned raster");
        assert_ne!(frozen, live, "the pin can tell the two cameras apart");

        // The live camera moves after arm.
        viewport.camera = turned_camera;
        let owner = paint.active_target();
        let channel = paint.active_channel().to_string();
        let depth = paint.stroke_len();
        assert!(viewport.apply_projection(rect, &mesh, &mut paint, &projection));
        assert_eq!(paint.stroke_len(), depth + 1, "one history entry");
        let after = paint
            .read_plane(owner, tile, &channel)
            .expect("plane reads");
        // Opaque source at opacity 1: alpha 1 replaces each texel exactly.
        assert_eq!(after.data, frozen, "Apply wrote the FROZEN raster");
    }

    #[test]
    fn projection_apply_with_follow_camera_writes_through_the_live_camera() {
        // GPU-gated. R-A2 v2b: follow on, arm freezes nothing, then the
        // LIVE camera turns: Apply writes the turned camera's raster.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint = PaintState::with_target_size(device, queue, 8).expect("paint builds");
        let mesh = quad();
        let mut viewport = painting_viewport();
        let rect = rect200();
        let (red, blue) = ([255u8, 0, 0, 255], [0u8, 0, 255, 255]);
        let mut rgba = Vec::new();
        for _y in 0..8 {
            for x in 0..8 {
                rgba.extend_from_slice(if x < 4 { &red } else { &blue });
            }
        }
        let mut projection = ProjectionState {
            image: Some(RgbaImage::new(8, 8, rgba).expect("8x8 RGBA")),
            transform: StencilTransform {
                center: [100.0, 100.0],
                scale: 25.0,
                rotation: 0.0,
            },
            opacity: 1.0,
            follow_camera: true,
            ..ProjectionState::default()
        };
        // Unarmed follow: refused, nothing frozen to fall back on.
        assert!(!viewport.apply_projection(rect, &mesh, &mut paint, &projection));
        let armed_camera = viewport.camera;
        projection.arm(armed_camera, rect);
        assert!(projection.armed && projection.frozen.is_none());

        let tile = paint.active_tile();
        let (w, h) = paint.quick_mask_for_tile(tile).dimensions();
        let turned_camera = OrbitCamera {
            yaw: ROTATED_YAW,
            ..armed_camera
        };
        let raster = |cam: &OrbitCamera| {
            rasterize_projection_live(&projection, &mesh, tile, w, h, cam, rect)
        };
        let at_arm = raster(&armed_camera).expect("arm-camera raster");
        let turned = raster(&turned_camera).expect("turned raster");
        assert_ne!(at_arm, turned, "the pin can tell the two cameras apart");
        assert!(
            turned.chunks_exact(4).all(|t| t[3] == 255),
            "every texel opaque (the exact-replace precondition)"
        );

        viewport.camera = turned_camera;
        let owner = paint.active_target();
        let channel = paint.active_channel().to_string();
        let depth = paint.stroke_len();
        assert!(viewport.apply_projection(rect, &mesh, &mut paint, &projection));
        assert_eq!(paint.stroke_len(), depth + 1, "one history entry");
        let after = paint
            .read_plane(owner, tile, &channel)
            .expect("plane reads");
        assert_eq!(after.data, turned, "Apply wrote the LIVE camera's raster");
    }

    #[test]
    fn projection_apply_without_image_or_freeze_is_refused_unpainted() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint = PaintState::with_target_size(device, queue, 8).expect("paint builds");
        let mesh = quad();
        let mut viewport = painting_viewport();
        let rect = rect200();
        let tile = paint.active_tile();
        let owner = paint.active_target();
        let channel = paint.active_channel().to_string();
        let before = paint
            .read_plane(owner, tile, &channel)
            .expect("plane reads");
        let depth = paint.stroke_len();

        // Never armed: an image, no frozen placement.
        let unfrozen = opaque_projection(false);
        assert!(unfrozen.frozen.is_none());
        assert!(!viewport.apply_projection(rect, &mesh, &mut paint, &unfrozen));

        // Frozen, but no image.
        let mut imageless = ProjectionState::default();
        imageless.arm(viewport.camera, rect);
        assert!(!viewport.apply_projection(rect, &mesh, &mut paint, &imageless));

        assert_eq!(paint.stroke_len(), depth, "no history entry");
        assert!(paint.take_stroke_commits().is_empty(), "no commit queued");
        let after = paint
            .read_plane(owner, tile, &channel)
            .expect("plane reads");
        assert_eq!(after.data, before.data, "no texel written");
    }
}
