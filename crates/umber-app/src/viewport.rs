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

use egui::{Color32, PointerButton, Response, Sense, Ui};
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
        let extras = ViewportExtras { home, stencil };
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
        let ViewportExtras { home, mut stencil } = extras;

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
        // while the chord owns the pointer.
        let stencil_owns = self.stencil_input(
            ui,
            &response,
            rect,
            stencil.as_deref_mut(),
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
        }
        if !painting {
            // A stencil chord owns the pointer: no navigation under it.
            if !stencil_owns {
                self.handle_input(ui, &response);
            }
        } else if let (Some(mesh), Some(paint)) = (self.mesh_data.as_ref(), paint.as_mut()) {
            if let Some(pos) = response.interact_pointer_pos() {
                let picked = self.pick(rect, mesh, pos);
                let hit = picked.map(|(uv, _)| egui::Pos2::new(uv[0], uv[1]));
                let facing_weight = picked.map_or(1.0, |(_, w)| w);
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
                let pressure = crate::stylus_input::resolve_pressure(stylus_pressure, touch_force)
                    * facing_weight;
                if !paint.is_stroking() {
                    paint.begin_screen_point_pressured(hit, pressure);
                } else {
                    paint.extend_screen_point_pressured(hit, pressure);
                }
                // Mark ownership only once a stroke actually began (a drag
                // that opened on a miss starts nothing until it hits).
                self.stroke_from_viewport = paint.is_stroking();
                if hit.is_some() {
                    self.last_stroke_end = Some(pos);
                }
            }
        }

        ui.painter().rect_filled(rect, 0.0, EMPTY_VIEWPORT_COLOR);
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
    fn stencil_input(
        &mut self,
        ui: &Ui,
        response: &Response,
        rect: egui::Rect,
        stencil: Option<&mut StencilState>,
        mut paint: Option<&mut PaintState>,
    ) -> bool {
        let Some(stencil) = stencil else {
            self.stencil_drag = None;
            return false;
        };
        // N toggles the stencil (the design: ignored = paint passes
        // through, the overlay hides). Only with an image, never while
        // a text field has the keyboard, and the gate follows the
        // flip immediately through the unified re-stage + hand-off.
        if stencil.image.is_some()
            && ui.input(|i| i.key_pressed(egui::Key::N))
            && !ui.ctx().memory(|m| m.focused().is_some())
        {
            stencil.active = !stencil.active;
            restage_stencil(stencil, paint.as_deref_mut());
        }
        let mods = ui.input(|i| i.modifiers);
        let armed = self.mesh.is_some()
            && stencil.active
            && stencil.image.is_some()
            && !mods.alt
            && !(mods.ctrl || mods.command)
            && ui.input(|i| i.key_down(egui::Key::S));
        let local = |p: egui::Pos2| [p.x - rect.min.x, p.y - rect.min.y];
        if self.stencil_drag.is_none() && armed {
            let started = [
                PointerButton::Primary,
                PointerButton::Secondary,
                PointerButton::Middle,
            ]
            .into_iter()
            .find(|b| response.drag_started_by(*b));
            let chord = started.and_then(StencilChord::for_button);
            // Map from the PRESS: drag_started fires past the threshold.
            let origin = ui
                .input(|i| i.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos());
            if let (Some(chord), Some(origin)) = (chord, origin) {
                self.stencil_drag = Some(StencilDrag {
                    chord,
                    start: stencil.transform,
                    origin: local(origin),
                });
            }
        }
        let Some(drag) = self.stencil_drag else {
            return armed && !response.dragged();
        };
        if response.dragged_by(drag.chord.button()) {
            let dims = stencil.image.as_ref().map(|i| [i.width(), i.height()]);
            if let (Some(pos), Some(dims)) = (response.interact_pointer_pos(), dims) {
                stencil.transform = stencil_drag(
                    &drag.start,
                    drag.chord.with_snap(mods.shift),
                    drag.origin,
                    local(pos),
                    dims,
                    [rect.width(), rect.height()],
                );
            }
        } else {
            // Released: the one re-rasterize this gesture costs.
            self.stencil_drag = None;
            restage_stencil(stencil, paint);
        }
        true
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
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            brush_cursor::paint(&painter, drag.origin, shape);
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
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        brush_cursor::paint(&painter, pos, shape);
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
        let aspect = rect.width() / rect.height();
        let ndc_x = (pos.x - rect.left()) / rect.width() * 2.0 - 1.0;
        let ndc_y = 1.0 - (pos.y - rect.top()) / rect.height() * 2.0;
        let view_proj = self.camera.view_proj(aspect);
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
        let origin = match self.camera.projection {
            umber_gpu::camera::Projection::Perspective => self.camera.eye(),
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

    fn handle_input(&mut self, ui: &Ui, response: &Response) {
        // Navigation goes through the preset (`crate::navigation`): the
        // first held button whose (button, modifiers) chord the preset
        // binds drives the camera this frame.
        let mods = ui.input(|i| i.modifiers);
        let op = [
            PointerButton::Primary,
            PointerButton::Middle,
            PointerButton::Secondary,
        ]
        .into_iter()
        .filter(|b| response.dragged_by(*b))
        .find_map(|b| self.nav.camera_op(b, mods));
        if let Some(op) = op {
            let delta = response.drag_delta();
            match op {
                crate::navigation::CameraOp::Orbit => self
                    .camera
                    .orbit(delta.x * ORBIT_SENSITIVITY, -delta.y * ORBIT_SENSITIVITY),
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
                self.camera.zoom(scroll.signum() * ZOOM_SENSITIVITY);
            }
            // Projection toggle (P, mnemonic: Projection): free because the
            // shell's SHORTCUT_KEYS (toolbar.rs: W/G/V/X/I/E/Z/S/O) and the
            // graph canvas (Delete/Backspace/Escape) bind nothing else;
            // hover-gated and modifier-free so it never fires while typing
            // in a panel. No i18n string by design (the .ftl catalogs are
            // owned by another surface) — same as the drag/scroll inputs.
            if ui.input(|i| i.key_pressed(egui::Key::P)) {
                self.camera.toggle_projection();
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
        let result = crate::stencil::load_png_rgba(&path);
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
}
