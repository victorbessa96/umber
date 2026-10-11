//! The 3D viewport: owns the orbit camera and the GPU-resident mesh, and
//! paints into the dock's `Panel::Viewport` tab each frame.
//!
//! Never touches a `wgpu` type by name — every value it holds or passes to
//! `umber_gpu` is opaque to this crate (received from or handed to
//! `umber_gpu`'s API), per the architecture rule that GPU objects stay
//! behind `umber-gpu`.

use egui::{Color32, PointerButton, Response, Sense, Ui};
use umber_gpu::{
    DisplayLut, EnvIrradiance, GpuContext, MeshBuffers, OrbitCamera, SoloChannel, ViewMode,
};

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
    /// The shading mode (View menu; V cycles lit ↔ unlit). Defaults
    /// to Lit — today's render, byte-identical with the mode uniform
    /// at (0, 0).
    view_mode: ViewMode,
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
    /// Paint mode: Ctrl/Cmd (or pen-tip with `ctrl`) + primary drag casts
    /// the pointer through the camera into the mesh and feeds the hit UV
    /// to `paint`'s stroke path. Plain drag orbits; shift-drag pans.
    ///
    /// `display_lut` is the app's display LUT (the Display panel's
    /// chain), applied as the mesh pass's last step; `None` draws
    /// through the identity table.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        gpu: &GpuContext,
        paint: Option<&mut crate::paint_state::PaintState>,
        display_lut: Option<&DisplayLut>,
    ) {
        let rect = ui.available_rect_before_wrap();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        let painting = ui.input(|i| i.modifiers.ctrl || i.modifiers.command)
            && response.dragged_by(PointerButton::Primary)
            && self.mesh_data.is_some()
            && paint.is_some();
        if !painting {
            self.handle_input(ui, &response);
        } else if let (Some(mesh), Some(paint)) = (self.mesh_data.as_ref(), paint) {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(uv) = self.pick_uv(rect, mesh, pos) {
                    if !paint.is_stroking() {
                        paint.begin_stroke(egui::Pos2::new(uv[0], uv[1]));
                    } else {
                        paint.extend_stroke(egui::Pos2::new(uv[0], uv[1]));
                    }
                }
            }
        }

        ui.painter().rect_filled(rect, 0.0, EMPTY_VIEWPORT_COLOR);

        if let Some(mesh) = &self.mesh {
            profiling::scope!("display_pass");
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
            let callback = {
                // Data modes (data solos, every mesh-map preview) bind
                // the identity table here — the §9 display-transform
                // bypass (see `ViewMode::select_lut`).
                let lut = self.view_mode.select_lut(display_lut);
                mesh.paint_callback_with_mode(gpu, uniform, lut, self.view_mode)
            };
            let shape = umber_gpu::mesh_paint_shape(rect, callback);
            ui.painter().add(shape);
            if self.show_wireframe {
                let wire = gpu.wire_callback(mesh, uniform, umber_gpu::WireColor::default());
                ui.painter().add(umber_gpu::wire_paint_shape(rect, wire));
            }
        }
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
}
