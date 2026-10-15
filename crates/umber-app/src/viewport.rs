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

use egui::{Color32, PointerButton, Response, Sense, Ui};
use umber_gpu::postfx::PostChain;
use umber_gpu::renderer::PaintParams;
use umber_gpu::{
    DisplayLut, EnvIrradiance, GpuContext, MeshBuffers, OrbitCamera, PostFxViewport, SoloChannel,
    ViewMode, ViewportRoute,
};
use umber_mesh::tile_of_uv;

use crate::i18n::tr;

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

/// An eyedropper sample (C1 worklist #10): the sRGBA bytes the viewport
/// read from the paint target at the clicked UV, one per Pick click.
/// Carried to the shell so it can set the brush color (and later: the
/// alpha, masks, anything color-keyed) from what the artist actually
/// sees on the mesh.
pub struct PickSample(pub [u8; 4]);

/// What a strip-only tool's click resolved to (C1 worklist #10, extended
/// by #11): the viewport never acts on these itself — the shell matches
/// on the variant and routes through the paint session.
pub enum ViewportClick {
    /// The eyedropper's sRGBA sample at the picked UV.
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
}

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
    }

    /// Shows or hides the ground grid (View menu `Show Grid`, G).
    /// Synced from the app shell each frame before `ui`.
    pub fn set_show_grid(&mut self, on: bool) {
        self.show_grid = on;
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
        mut paint: Option<&mut crate::paint_state::PaintState>,
        stylus_pressure: Option<f32>,
        display_lut: Option<&DisplayLut>,
        postfx: &PostChain,
    ) -> Option<ViewportClick> {
        let rect = ui.available_rect_before_wrap();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.gizmo_clicked = false;

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
        if self.mesh.is_some() && self.show_gizmo && response.clicked_by(PointerButton::Primary) {
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
        if self.pick_armed && !self.gizmo_clicked && response.clicked_by(PointerButton::Primary) {
            if let (Some(mesh), Some(paint)) = (self.mesh_data.as_ref(), paint.as_mut()) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(uv) = self.pick_uv(rect, mesh, pos) {
                        // Read the texel from the tile the UV routes to: the
                        // paint target holds LINEAR RGBA8, the shell wants
                        // sRGB bytes — convert per-texel here (the identity
                        // for the v1 no-LUT path; a display-transform-aware
                        // sample is the follow-up when the picker meets the
                        // display chain).
                        let tile = tile_of_uv(uv);
                        if let Some(image) = paint.read_tile(tile) {
                            let x = ((uv[0].clamp(0.0, 1.0)) * (image.width as f32 - 1.0)).round()
                                as u32;
                            let y = ((uv[1].clamp(0.0, 1.0)) * (image.height as f32 - 1.0)).round()
                                as u32;
                            let i = (y * image.width + x) as usize * 4;
                            if i + 3 < image.data.len() {
                                let srgba = [
                                    linear_to_srgb_byte(image.data[i]),
                                    linear_to_srgb_byte(image.data[i + 1]),
                                    linear_to_srgb_byte(image.data[i + 2]),
                                    image.data[i + 3],
                                ];
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
        if self.idmask_armed && !self.gizmo_clicked && response.clicked_by(PointerButton::Primary) {
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
        let resize_chord = ui
            .input(|i| (i.modifiers.ctrl || i.modifiers.command) && !i.modifiers.alt)
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

        let painting = !self.pick_armed
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
            self.handle_input(ui, &response);
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
        // The empty state (Phase 0b): the drop-zone hero until a mesh
        // loads, lit while a file drag hovers the window.
        if self.mesh.is_none() {
            let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
            crate::empty_state::paint_hero(ui, rect, hovering);
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
            if self.show_wireframe {
                let wire = gpu.wire_callback(&*mesh, uniform, umber_gpu::WireColor::default());
                ui.painter().add(umber_gpu::wire_paint_shape(rect, wire));
            }
            // Cage overlay (§6:59): after the wireframe so its edges win,
            // skipped entirely when off (byte-identical to cage-off).
            if self.show_cage {
                let cage = gpu.wire_callback(&*mesh, uniform, umber_gpu::WireColor::cage());
                ui.painter().add(umber_gpu::wire_paint_shape(rect, cage));
            }
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
        // No strip-tool click this frame: the signature is Option so the
        // shell reads one click per primary press, `None` every other frame.
        None
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
        let texels_per_uv = paint.texels_per_uv_for(tile_of_uv(uv));
        let probe = |dir: f32| {
            let at = pos + egui::vec2(dir * brush_cursor::PROBE_PX, 0.0);
            let uv_b = self.pick_uv(rect, mesh, at)?;
            brush_cursor::texels_spanned(uv, uv_b, texels_per_uv)
        };
        brush_cursor::combine_spans(probe(1.0), probe(-1.0))
            .map(|t| brush_cursor::screen_radius(paint.brush_radius_texels(), t))
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
}
