//! The 2D UV view: draws the loaded mesh's UV triangles as a wireframe
//! on the 0..1 UV square (requirements.md §5: "2D UV view alongside 3D
//! view (synchronized)" — Wave 2 scope: the wireframe + fit; painting INTO
//! this view arrives with the paint engine).

use egui::{Color32, Sense, Stroke, Ui};
use umber_brush::symmetry::{RADIAL_CENTER, RADIAL_COUNT_MAX, RADIAL_COUNT_MIN};
use umber_brush::{MirrorAxis, MirrorSettings, QuickMask, SymmetrySettings};
use umber_mesh::MeshData;

use crate::i18n::tr;

/// Wireframe color for UV island edges.
const UV_EDGE_COLOR: Color32 = Color32::from_rgb(140, 190, 255);
/// Background inside the 0..1 UV square.
const UV_BG_COLOR: Color32 = Color32::from_rgb(38, 38, 42);
/// Background outside the UV square (padding area).
const PAD_COLOR: Color32 = Color32::from_rgb(28, 28, 30);

/// The 2D UV view panel state: the cached quick-mask overlay texture.
/// Zoom/pan for this view lands with the paint-engine sync work.
#[derive(Default)]
pub struct UvView {
    /// The quick-mask overlay, rebuilt only when its key changes.
    mask_overlay: Option<MaskOverlay>,
}

/// The uploaded quick-mask overlay and what it was built from.
struct MaskOverlay {
    /// `(mask generation, displayed tile, mask dimensions)`.
    key: (u64, u16, (u32, u32)),
    texture: egui::TextureHandle,
}

impl UvView {
    /// Draws the UV wireframe for `mesh` (or a hint when none is loaded)
    /// and feeds pointer strokes into the paint session. The paint
    /// canvas presents through `display_lut` (the Display panel's chain;
    /// `None` = the identity table).
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        mesh: Option<&MeshData>,
        gpu: &umber_gpu::GpuContext,
        mut paint: Option<&mut crate::paint_state::PaintState>,
        display_lut: Option<&umber_gpu::DisplayLut>,
    ) {
        let rect = ui.available_rect_before_wrap();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        // The UV square occupies the largest centered square that fits.
        let side = rect.width().min(rect.height());
        let origin = egui::Pos2::new(
            rect.left() + (rect.width() - side) / 2.0,
            rect.top() + (rect.height() - side) / 2.0,
        );
        let square = egui::Rect::from_min_size(origin, egui::vec2(side, side));

        // Stroke input: primary-button drag inside the UV square maps to
        // UV space and feeds the paint conditioner. Left-drag orbits in
        // the 3D view; here left-drag paints.
        if let Some(paint) = paint.as_deref_mut() {
            let interact = ui.interact(
                square,
                ui.id().with("uv-paint"),
                egui::Sense::click_and_drag(),
            );
            // The mirror manipulator: a grab strip along the mirror line,
            // registered after the paint surface so it wins the hit test
            // where they overlap. Dragging it moves the line (handed back
            // to the panel via `PaintState::sync_symmetry`) and never
            // paints.
            let mut handle_active = false;
            let mirror = paint.symmetry().mirror;
            if mirror.enabled {
                let strip = mirror_line_rect(square, mirror).expand(MIRROR_GRAB_HALF_WIDTH);
                // Click-and-drag: a bare click on the line must not fall
                // through to the paint surface and stamp a dab.
                let handle = ui.interact(
                    strip,
                    ui.id().with("uv-mirror-handle"),
                    Sense::click_and_drag(),
                );
                handle_active = handle.clicked();
                if handle.hovered() || handle.dragged() {
                    let icon = match mirror.axis {
                        MirrorAxis::X => egui::CursorIcon::ResizeHorizontal,
                        MirrorAxis::Y => egui::CursorIcon::ResizeVertical,
                    };
                    ui.ctx().set_cursor_icon(icon);
                }
                if handle.dragged() {
                    handle_active = true;
                    if let Some(pos) = handle.interact_pointer_pos() {
                        paint.set_mirror_offset(mirror_offset_at(square, mirror.axis, pos));
                    }
                }
            }
            let pointer_down = !handle_active
                && (interact.dragged_by(egui::PointerButton::Primary) || interact.clicked());
            // The clone tool's source affordance: Alt + press sets the
            // clone source and never paints — no stroke starts while Alt
            // is held. Re-set every frame the button is down, so the last
            // position before release wins.
            let alt = ui.input(|i| i.modifiers.alt);
            if alt && paint.clone_tool_active() {
                if paint.is_stroking() {
                    paint.end_stroke();
                }
                if pointer_down {
                    if let Some(uv) = interact
                        .interact_pointer_pos()
                        .and_then(|pos| crate::paint_state::uv_from_pointer(square, pos))
                    {
                        paint.set_clone_source(uv);
                    }
                }
            } else if pointer_down {
                if let Some(pos) = interact.interact_pointer_pos() {
                    if let Some(uv) = crate::paint_state::uv_from_pointer(square, pos) {
                        if !paint.is_stroking() {
                            paint.begin_stroke(uv);
                        } else {
                            paint.extend_stroke(uv);
                        }
                    }
                }
            } else if paint.is_stroking() {
                paint.end_stroke();
            }
        }

        // Read before `paint` is consumed by the canvas callback below.
        let symmetry = paint.as_deref().map(|p| *p.symmetry());
        let mask_texture = paint
            .as_deref()
            .and_then(|p| self.mask_overlay_texture(ui.ctx(), p));
        let mask_mode = paint
            .as_deref()
            .is_some_and(|p| p.quick_mask().is_editing());

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, PAD_COLOR);
        painter.rect_filled(square, 0.0, UV_BG_COLOR);

        // Live paint canvas: the paint target presented through the
        // texture-display callback, clipped to the UV square, through the
        // display LUT. Drawn beneath the wireframe so strokes show
        // through (the egui-drawn UV wireframe is not LUT-transformed).
        if let Some(paint) = paint {
            let callback = gpu.texture_display().callback(
                &gpu.device,
                paint.paint_target(),
                square,
                display_lut,
            );
            let shape = umber_gpu::texture_display::texture_display_shape(square, callback);
            painter.add(shape);
        }

        // The quick-mask overlay over the canvas (red where blocked), under
        // the wireframe like the strokes. Its rows run top-down like the
        // paint target's, so it maps onto the square with no V flip.
        if let Some(texture) = mask_texture {
            let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture, square, full_uv, Color32::WHITE);
        }
        if mask_mode {
            painter.text(
                square.left_top() + egui::vec2(6.0, 6.0),
                egui::Align2::LEFT_TOP,
                tr("uv.mask-mode"),
                egui::FontId::proportional(13.0),
                MASK_OVERLAY_LABEL_COLOR,
            );
        }

        // UV-space -> screen-space: v is flipped (UV origin is bottom-left,
        // screen origin is top-left).
        let to_screen = |uv: [f32; 2]| {
            egui::Pos2::new(
                square.left() + uv[0].clamp(0.0, 1.0) * side,
                square.bottom() - uv[1].clamp(0.0, 1.0) * side,
            )
        };

        match mesh {
            Some(mesh) => {
                // Draw each triangle as a 3-segment line loop. Per-triangle
                // iteration is fine for Wave 2 viewport counts; batching
                // into a single mesh arrives with paint-engine sync.
                let stroke = Stroke::new(1.0, UV_EDGE_COLOR);
                for tri in mesh.indices.chunks_exact(3) {
                    let pts: Option<Vec<egui::Pos2>> = (0..3)
                        .map(|i| mesh.uvs.get(tri[i] as usize).map(|uv| to_screen(*uv)))
                        .collect();
                    if let Some([a, b, c]) = pts
                        .as_deref()
                        .and_then(|p: &[egui::Pos2]| <&[egui::Pos2; 3]>::try_from(p).ok())
                    {
                        let (a, b, c) = (*a, *b, *c);
                        painter.line_segment([a, b], stroke);
                        painter.line_segment([b, c], stroke);
                        painter.line_segment([c, a], stroke);
                    }
                }
            }
            None => {
                painter.text(
                    square.center(),
                    egui::Align2::CENTER_CENTER,
                    "no mesh loaded",
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_gray(120),
                );
            }
        }
        if let Some(symmetry) = symmetry {
            draw_symmetry_guides(&painter, square, &symmetry);
        }
        let _ = response;
    }

    /// The quick-mask overlay texture for the displayed (active) tile, or
    /// `None` while the mask is neither gating nor being edited. Rebuilt
    /// and re-uploaded only when the mask's generation, the tile or the
    /// size changes.
    fn mask_overlay_texture(
        &mut self,
        ctx: &egui::Context,
        paint: &crate::paint_state::PaintState,
    ) -> Option<egui::TextureId> {
        let mask = paint.quick_mask();
        if !mask.shows_overlay() {
            return None;
        }
        let tile = paint.active_tile();
        let key = (mask.generation(), tile, mask.dimensions());
        if self.mask_overlay.as_ref().map(|o| o.key) != Some(key) {
            let (size, rgba) = mask_overlay_rgba(mask, tile);
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            self.mask_overlay = Some(MaskOverlay {
                key,
                texture: ctx.load_texture("uv-quick-mask", image, Default::default()),
            });
        }
        self.mask_overlay.as_ref().map(|o| o.texture.id())
    }
}

/// The quick-mask overlay tint (conventional quick-mask red) and its
/// opacity over a fully blocked texel.
const MASK_OVERLAY_RGB: [u8; 3] = [255, 0, 0];
const MASK_OVERLAY_ALPHA: f32 = 0.5;
/// The overlay's longest side, texels: larger masks are point-sampled
/// down (a 4K plane would otherwise re-upload 64 MB per mask edit).
const MASK_OVERLAY_MAX_SIDE: u32 = 512;
/// The "mask mode" label color.
const MASK_OVERLAY_LABEL_COLOR: Color32 = Color32::from_rgb(255, 110, 110);

/// `tile`'s quick mask as an unmultiplied RGBA overlay: red at
/// `(1 - mask) * MASK_OVERLAY_ALPHA` opacity (blocked tinted, allowed
/// clear), point-sampled to at most [`MASK_OVERLAY_MAX_SIDE`] per side.
/// Returns `([width, height], bytes)`, rows top-down.
fn mask_overlay_rgba(mask: &QuickMask, tile: u16) -> ([usize; 2], Vec<u8>) {
    let (w, h) = mask.dimensions();
    let step = w.max(h).div_ceil(MASK_OVERLAY_MAX_SIDE).max(1);
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let [r, g, b] = MASK_OVERLAY_RGB;
    let view = mask.view(tile);
    let empty = mask.empty_value();
    let mut rgba = Vec::with_capacity(ow as usize * oh as usize * 4);
    for oy in 0..oh {
        for ox in 0..ow {
            let v = view
                .map_or(empty, |view| view.at(ox * step, oy * step))
                .clamp(0.0, 1.0);
            let a = ((1.0 - v) * MASK_OVERLAY_ALPHA * 255.0).round() as u8;
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    ([ow as usize, oh as usize], rgba)
}

/// Half-width (logical pixels) of the mirror manipulator's grab strip.
const MIRROR_GRAB_HALF_WIDTH: f32 = 5.0;
/// The symmetry guide color (mirror line, radial spokes).
const SYMMETRY_GUIDE_COLOR: Color32 = Color32::from_rgb(255, 170, 60);

/// The zero-width rect the mirror line occupies on the UV square (V up:
/// a V-offset of 1 is the square's top edge).
fn mirror_line_rect(square: egui::Rect, mirror: MirrorSettings) -> egui::Rect {
    match mirror.axis {
        MirrorAxis::X => {
            let x = square.left() + mirror.offset * square.width();
            egui::Rect::from_x_y_ranges(x..=x, square.y_range())
        }
        MirrorAxis::Y => {
            let y = square.bottom() - mirror.offset * square.height();
            egui::Rect::from_x_y_ranges(square.x_range(), y..=y)
        }
    }
}

/// The mirror offset (tile-local UV, clamped to the tile) under a pointer
/// at `pos`: its U for a U mirror, its V for a V mirror.
fn mirror_offset_at(square: egui::Rect, axis: MirrorAxis, pos: egui::Pos2) -> f32 {
    let t = match axis {
        MirrorAxis::X => (pos.x - square.left()) / square.width(),
        MirrorAxis::Y => 1.0 - (pos.y - square.top()) / square.height(),
    };
    t.clamp(0.0, 1.0)
}

/// The symmetry guides over the UV square: the mirror line, and the
/// radial copies' spokes from the radial center.
fn draw_symmetry_guides(painter: &egui::Painter, square: egui::Rect, symmetry: &SymmetrySettings) {
    let stroke = Stroke::new(1.5, SYMMETRY_GUIDE_COLOR);
    if symmetry.mirror.enabled {
        let line = mirror_line_rect(square, symmetry.mirror);
        painter.line_segment([line.min, line.max], stroke);
    }
    let radial = &symmetry.radial;
    if radial.enabled {
        let n = radial.count.clamp(RADIAL_COUNT_MIN, RADIAL_COUNT_MAX);
        let center = egui::Pos2::new(
            square.left() + RADIAL_CENTER.0 * square.width(),
            square.bottom() - RADIAL_CENTER.1 * square.height(),
        );
        let reach = square.width().min(square.height()) * 0.5;
        let faint = Stroke::new(1.0, SYMMETRY_GUIDE_COLOR.gamma_multiply(0.5));
        for k in 0..n {
            // Counter-clockwise in UV is counter-clockwise on screen too
            // (the V flip and the screen's downward y cancel).
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            let tip = center + egui::vec2(a.cos(), -a.sin()) * reach;
            painter.line_segment([center, tip], faint);
        }
        painter.circle_stroke(center, 4.0, stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_view_default_holds_only_the_mask_overlay_cache() {
        // The panel's only state is the quick-mask overlay cache (empty
        // until a mask shows); the default exists so AppState can derive
        // Default. If UV view gains more state (zoom/pan), update this
        // test deliberately.
        let view = UvView::default();
        assert!(view.mask_overlay.is_none());
    }

    #[test]
    fn mask_overlay_tints_blocked_texels_and_clears_allowed_ones() {
        let mut mask = QuickMask::new(8, 8);
        mask.stamp(
            1001,
            &umber_brush::retouch::RetouchDab {
                pos: [2.0, 2.0],
                radius: 1.0,
                alpha: 1.0,
                hardness: 1.0,
            },
        );
        let (size, rgba) = mask_overlay_rgba(&mask, 1001);
        assert_eq!(size, [8, 8]);
        let at = |x: usize, y: usize| &rgba[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
        assert_eq!(at(1, 1), [255, 0, 0, 0], "allowed: clear");
        assert_eq!(at(6, 6), [255, 0, 0, 128], "blocked: half-opacity red");
        // Inverted: the tint swaps sides.
        mask.invert();
        let (_, rgba) = mask_overlay_rgba(&mask, 1001);
        assert_eq!(rgba[(8 + 1) * 4 + 3], 128);
        assert_eq!(rgba[(6 * 8 + 6) * 4 + 3], 0);
    }

    #[test]
    fn mask_overlay_downsamples_large_masks() {
        let (size, rgba) = mask_overlay_rgba(&QuickMask::new(4096, 2048), 1001);
        assert_eq!(size, [512, 256]);
        assert_eq!(rgba.len(), 512 * 256 * 4);
        let (size, _) = mask_overlay_rgba(&QuickMask::new(512, 512), 1001);
        assert_eq!(size, [512, 512], "the app's 512² target maps 1:1");
    }
}
