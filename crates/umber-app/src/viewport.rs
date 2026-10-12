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

/// An eyedropper sample (C1 worklist #10): the sRGBA bytes the viewport
/// read from the paint target at the clicked UV, one per Pick click.
/// Carried to the shell so it can set the brush color (and later: the
/// alpha, masks, anything color-keyed) from what the artist actually
/// sees on the mesh.
pub struct PickSample(pub [u8; 4]);

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
}

impl Viewport {
    /// Uploads `mesh` to the GPU and re-frames the camera on its bounds,
    /// keeping the current yaw/pitch so loading a new mesh doesn't spin
    /// the view out from under the user.
    pub fn load_mesh(
        &mut self,
        gpu: &GpuContext,
        mesh: &umber_mesh::MeshData,
    ) -> anyhow::Result<()> {
        let mut buffers = MeshBuffers::upload(gpu, mesh)?;
        // Fresh buffers default to procedural — re-apply our env.
        buffers.set_environment(gpu, self.env.as_ref());
        if let Some((min, max)) = mesh.bounds() {
            self.camera = OrbitCamera::framing(min, max, self.camera.yaw, self.camera.pitch);
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
    ) -> Option<PickSample> {
        let rect = ui.available_rect_before_wrap();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        // The eyedropper (C1 worklist #10): when armed, a PRIMARY CLICK (not
        // a drag — a stray orbit still works) samples the paint target's
        // texel at the picked UV. Returns the sRGBA sample for the shell to
        // feed the brush; painting is disabled while the eyedropper is the
        // active tool (it never strokes).
        if self.pick_armed && response.clicked_by(PointerButton::Primary) {
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
                                return Some(PickSample(srgba));
                            }
                        }
                    }
                }
            }
        }

        let painting = !self.pick_armed
            && ui.input(|i| i.modifiers.ctrl || i.modifiers.command)
            && response.dragged_by(PointerButton::Primary)
            && self.mesh_data.is_some()
            && paint
                .as_ref()
                .is_some_and(|p| p.projection().viewport_paints());
        if !painting {
            self.handle_input(ui, &response);
        } else if let (Some(mesh), Some(paint)) = (self.mesh_data.as_ref(), paint.as_mut()) {
            if let Some(pos) = response.interact_pointer_pos() {
                let hit = self
                    .pick_uv(rect, mesh, pos)
                    .map(|uv| egui::Pos2::new(uv[0], uv[1]));
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
                let pressure = crate::stylus_input::resolve_pressure(stylus_pressure, touch_force);
                if !paint.is_stroking() {
                    paint.begin_screen_point_pressured(hit, pressure);
                } else {
                    paint.extend_screen_point_pressured(hit, pressure);
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
        // No eyedropper sample this frame: the signature is Option so the
        // shell reads one Sample per click, `None` every other frame.
        None
    }

    /// Arms or disarms the eyedropper (the shell syncs this from
    /// ToolKind::Pick each frame; painting is suppressed while armed).
    pub fn set_pick_armed(&mut self, armed: bool) {
        self.pick_armed = armed;
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
        let hit = umber_mesh::ray_intersect(mesh, self.camera.eye(), dir)?;
        umber_mesh::uv_at(mesh, hit)
    }

    fn handle_input(&mut self, ui: &Ui, response: &Response) {
        if response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if ui.input(|i| i.modifiers.shift) {
                self.camera
                    .pan(-delta.x * PAN_SENSITIVITY, delta.y * PAN_SENSITIVITY);
            } else {
                self.camera
                    .orbit(delta.x * ORBIT_SENSITIVITY, -delta.y * ORBIT_SENSITIVITY);
            }
        } else if response.dragged_by(PointerButton::Middle) {
            let delta = response.drag_delta();
            self.camera
                .pan(-delta.x * PAN_SENSITIVITY, delta.y * PAN_SENSITIVITY);
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.zoom(scroll.signum() * ZOOM_SENSITIVITY);
            }
        }
    }
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
/// # No height-channel counterpart (audit worklist #12, checked — not wired)
///
/// `umber_gpu::MeshBuffers` now has `set_height_texture`/
/// `clear_height_texture` (mirroring `set_base_color_texture`/
/// `clear_base_color_texture` above) feeding the Lit path's per-frame
/// normal perturbation. This function does NOT call them: `PaintState`
/// has no per-channel paint targets — `targets: BTreeMap<u16,
/// PaintThread>` is one target PER TILE, used for base color only (see
/// `crate::document`'s own doc: "the paint-channel binding to GPU
/// targets arrive with the tile pool"). There is no "the artist is
/// painting Height now" state anywhere in umber-app to read, live or
/// baked, and inventing one here would be exactly the fabricated-global
/// CLAUDE.md's contract warns against. Wiring this seam is therefore the
/// tile-pool's job, not this kill's — see the kill report
/// (`docs/loop/kill-reports/2026-10-12-height-normal.md`) for what was
/// checked before reaching this conclusion.
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
}
