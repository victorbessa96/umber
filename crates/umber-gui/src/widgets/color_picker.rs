//! `color_picker` — the colour picker (docs/specs/gui-plan-2026-10-15.md
//! kill `g4-color-picker`, P:D1, B:D14-D17, PC:#7; audit C2-12). Replaces
//! egui's stock `color_edit_button_rgb` at the brush and fill sites.
//!
//! Two layers, like [`super::ramp`] and [`super::curve`]:
//!
//! - [`ColorState`] is the model: the colour as LINEAR RGB (what the paint
//!   engine consumes) plus a separately kept HSV, so the hue survives greys
//!   (dragging saturation to 0 and back does not snap the hue to red) and
//!   brightness 0 keeps hue and saturation. HSV is in the sRGB-encoded
//!   space, so the SV square reads perceptually, the way painters expect.
//!   Hex is always sRGB (`#rrggbb`). [`ColorState::previous`] is the colour
//!   the picker opened with; [`ColorState::revert`] restores it. Pure, no
//!   egui context.
//! - [`ColorPicker`] draws it: an SV square and a hue strip as `egui::Mesh`
//!   vertex grids (no texture upload per drag), the current/previous swatch
//!   pair (click previous to revert), a hex field, and HSV + RGB slider rows
//!   under it whose tracks are tinted across their own channel range (also
//!   meshes, no texture). Ctrl/Cmd-drag on the hue strip snaps to 30°.
//! - [`color_button_with`] is the popup form that also remembers recent
//!   colours in a [`RecentColors`] list — pushed once when the popup CLOSES,
//!   never per drag frame — and shows its [`SwatchGrid`] so one click
//!   restores a recent colour. umber-gui has no i18n: the caller passes
//!   [`PickerLabels`]. [`color_button`] stays the stock drop-in (English
//!   glyph labels, no recents).
//!
//! The umber-gui layer rule (egui only) holds: no colour-management crate.
//! The sRGB transfer functions here are the IEC 61966-2-1 piecewise curve,
//! the same one egui's `Rgba` ↔ `Color32` conversion uses.

use egui::{
    Align2, Color32, FontId, Id, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2, WidgetInfo, WidgetType,
};

use super::swatch_grid::{RecentColors, SwatchGrid};
use crate::theme;

/// sRGB-encoded → linear, IEC 61966-2-1.
pub fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear → sRGB-encoded, IEC 61966-2-1.
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    // Boundary-exact top: the curve's endpoint is EXACTLY 1.0, but the
    // f32 rational loses a half-ULP there (f32(1.055) - f32(0.055) =
    // 0.99999994), which first surfaced as the kept-HSV pin on a
    // full-bright construction (the battery's first run of these
    // bytes, 2026-10-22: `setting_the_three_components_to_a_grey_
    // keeps_the_hue` saw v 0.99999994). The bottom endpoint is already
    // exact (12.92 * 0 = 0); nothing below the top changes.
    if c >= 1.0 {
        1.0
    } else if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// HSV (each in `0..=1`, hue as a turn fraction) → RGB in the same
/// encoding.
pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [f32; 3] {
    let h = (h - h.floor()) * 6.0;
    let i = h.floor() as i32 % 6;
    let f = h - h.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// RGB → HSV. Greys report hue 0 and saturation 0; [`ColorState`] keeps
/// its own hue/saturation across them.
pub fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max > 0.0 { d / max } else { 0.0 };
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, s, max]
}

/// The picker's model (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorState {
    /// Hue, saturation, value in the sRGB-encoded space, each `0..=1`.
    hsv: [f32; 3],
    /// The colour the picker opened with (linear RGB).
    previous: [f32; 3],
}

impl ColorState {
    /// Opens on `linear` (linear RGB, clamped to `0..=1`).
    pub fn new(linear: [f32; 3]) -> Self {
        let linear = linear.map(|c| {
            if c.is_finite() {
                c.clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        Self {
            hsv: rgb_to_hsv(linear.map(linear_to_srgb)),
            previous: linear,
        }
    }

    /// The current colour, linear RGB.
    pub fn linear(&self) -> [f32; 3] {
        hsv_to_rgb(self.hsv).map(srgb_to_linear)
    }

    /// The current colour, sRGB-encoded `0..=1`.
    pub fn srgb(&self) -> [f32; 3] {
        hsv_to_rgb(self.hsv)
    }

    /// `[h, s, v]`, each `0..=1`.
    pub fn hsv(&self) -> [f32; 3] {
        self.hsv
    }

    /// The colour the picker opened with (linear RGB).
    pub fn previous(&self) -> [f32; 3] {
        self.previous
    }

    /// Sets hue (wrapped into `0..1`), keeping saturation and value.
    pub fn set_hue(&mut self, h: f32) {
        if h.is_finite() {
            self.hsv[0] = h - h.floor();
        }
    }

    /// Sets saturation and value (clamped), keeping the hue: the SV-square
    /// drag. A grey (s = 0) or black (v = 0) keeps the hue for the way back.
    pub fn set_sv(&mut self, s: f32, v: f32) {
        if s.is_finite() {
            self.hsv[1] = s.clamp(0.0, 1.0);
        }
        if v.is_finite() {
            self.hsv[2] = v.clamp(0.0, 1.0);
        }
    }

    /// Sets the colour from linear RGB. Hue (and saturation, for black) are
    /// KEPT when the new colour does not define them, so typing a grey into
    /// a hex field does not lose the hue the user was working in.
    pub fn set_linear(&mut self, linear: [f32; 3]) {
        let linear = linear.map(|c| {
            if c.is_finite() {
                c.clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        let [h, s, v] = rgb_to_hsv(linear.map(linear_to_srgb));
        if v <= 0.0 {
            self.hsv[2] = 0.0;
        } else if s <= 0.0 {
            self.hsv[1] = 0.0;
            self.hsv[2] = v;
        } else {
            self.hsv = [h, s, v];
        }
    }

    /// Sets one sRGB-encoded component (`i`: 0 = R, 1 = G, 2 = B; each
    /// `0..=1`, clamped), keeping the other two. Goes through
    /// [`Self::set_linear`], so a grey (s = 0) keeps the hue exactly as a
    /// typed hex grey does. An out-of-range `i` or a non-finite `v` changes
    /// nothing.
    pub fn set_srgb_component(&mut self, i: usize, v: f32) {
        if i >= 3 || !v.is_finite() {
            return;
        }
        let mut srgb = self.srgb();
        srgb[i] = v.clamp(0.0, 1.0);
        self.set_linear(srgb.map(srgb_to_linear));
    }

    /// Back to the colour the picker opened with.
    pub fn revert(&mut self) {
        let previous = self.previous;
        self.set_linear(previous);
    }

    /// `#rrggbb`, sRGB-encoded, lowercase.
    pub fn hex(&self) -> String {
        let [r, g, b] = self.srgb().map(|c| (c * 255.0).round() as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// Sets the colour from sRGB hex: `#rrggbb`, `rrggbb`, `#rgb` or `rgb`
    /// (case-insensitive, surrounding whitespace ignored). `false` and no
    /// change on anything else.
    pub fn set_hex(&mut self, text: &str) -> bool {
        let Some([r, g, b]) = parse_hex(text) else {
            return false;
        };
        self.set_linear([r, g, b].map(|c| srgb_to_linear(c as f32 / 255.0)));
        true
    }
}

/// Parses sRGB hex into bytes (see [`ColorState::set_hex`]).
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let t = text.trim();
    let t = t.strip_prefix('#').unwrap_or(t);
    if !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nib = |i: usize| u8::from_str_radix(&t[i..i + 1], 16).ok();
    match t.len() {
        3 => Some([nib(0)? * 17, nib(1)? * 17, nib(2)? * 17]),
        6 => Some([
            u8::from_str_radix(&t[0..2], 16).ok()?,
            u8::from_str_radix(&t[2..4], 16).ok()?,
            u8::from_str_radix(&t[4..6], 16).ok()?,
        ]),
        _ => None,
    }
}

/// Snaps a hue to the nearest 30° (1/12 turn): the Ctrl-drag rule.
pub fn snap_hue(h: f32) -> f32 {
    let snapped = (h * 12.0).round() / 12.0;
    snapped - snapped.floor()
}

/// The hue a hue-strip x position picks (`0..=1` across the strip),
/// snapped when `snap`.
pub fn hue_at(x_frac: f32, snap: bool) -> f32 {
    let h = x_frac.clamp(0.0, 1.0);
    if snap {
        snap_hue(h)
    } else {
        // 1.0 is the same hue as 0.0; keep it in `0..1`.
        if h >= 1.0 {
            0.0
        } else {
            h
        }
    }
}

/// Grid resolution of the SV square's vertex mesh (per side).
const SV_CELLS: usize = 16;
/// Hue strip segments (six would be exact for linear HSV; more keeps the
/// gamma-space vertex interpolation close).
const HUE_CELLS: usize = 36;
/// Channel-row label column, points. Callers pass short glyphs (R/G/B,
/// H/S/V); the stock default labels are exactly that.
const CHANNEL_LABEL_WIDTH: f32 = 16.0;
/// Channel-row numeric readout column, points.
const CHANNEL_VALUE_WIDTH: f32 = 34.0;
/// Channel-row height, points.
const CHANNEL_ROW_HEIGHT: f32 = 15.0;
/// Channel track height, points.
const CHANNEL_TRACK_HEIGHT: f32 = 9.0;
/// Gradient segments per channel track: one per ~3pt at the stock width,
/// smooth enough that the tint reads as continuous.
const CHANNEL_CELLS: usize = 32;
/// Two linear colours closer than this are "the same" for the recents
/// push: the picker's HSV round-trip error is ~1e-7, so this only absorbs
/// float noise, never a deliberate edit.
const SAME_COLOUR_EPS: f32 = 1e-6;

fn color32(srgb: [f32; 3]) -> Color32 {
    let [r, g, b] = srgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

/// Row labels for the picker's slider rows. umber-gui has no `tr()`; the
/// caller (umber-app) translates them. [`Default`] is the English glyph
/// set the stock [`color_button`] uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerLabels {
    /// R, G, B row labels: sRGB-encoded `0..255`, matching the hex field.
    pub rgb: [String; 3],
    /// H, S, V row labels: the kept HSV (`0..=1` internally).
    pub hsv: [String; 3],
}

impl Default for PickerLabels {
    fn default() -> Self {
        Self {
            rgb: ["R".to_owned(), "G".to_owned(), "B".to_owned()],
            hsv: ["H".to_owned(), "S".to_owned(), "V".to_owned()],
        }
    }
}

/// The picker widget over a [`ColorState`] (see the module docs).
pub struct ColorPicker<'a> {
    state: &'a mut ColorState,
    size: f32,
    labels: Option<&'a PickerLabels>,
}

impl<'a> ColorPicker<'a> {
    /// A picker editing `state`, SV square `size` points wide.
    pub fn new(state: &'a mut ColorState) -> Self {
        Self {
            state,
            size: 160.0,
            labels: None,
        }
    }

    /// The SV square's side, in points.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size.max(32.0);
        self
    }

    /// Row labels for the RGB/HSV slider rows (default: English glyphs).
    pub fn labels(mut self, labels: &'a PickerLabels) -> Self {
        self.labels = Some(labels);
        self
    }

    /// Draws the picker; the response is `changed()` when the colour moved.
    pub fn show(self, ui: &mut Ui) -> Response {
        let Self {
            state,
            size,
            labels,
        } = self;
        let before = *state;
        let mut response = sv_square(ui, state, size);
        response |= hue_strip(ui, state, size);
        response |= swatches(ui, state);
        ui.horizontal(|ui| {
            let id = ui.id().with("color-picker-hex");
            let mut text: String = ui
                .ctx()
                .data(|d| d.get_temp(id))
                .unwrap_or_else(|| state.hex());
            let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(72.0));
            if edit.changed() {
                ui.ctx().data_mut(|d| d.insert_temp(id, text.clone()));
            }
            if edit.lost_focus() {
                ui.ctx().data_mut(|d| d.remove::<String>(id));
                state.set_hex(&text);
            }
        });
        let default_labels;
        let labels = match labels {
            Some(labels) => labels,
            None => {
                default_labels = PickerLabels::default();
                &default_labels
            }
        };
        channel_rows(ui, state, labels, size);
        if *state != before {
            response.mark_changed();
        }
        response
    }
}

/// The colour a channel row's track shows at fraction `t`: the current
/// sRGB colour with component `i` swept across `0..=1`.
fn rgb_channel_tint(srgb: [f32; 3], i: usize, t: f32) -> Color32 {
    let mut c = srgb;
    if let Some(slot) = c.get_mut(i) {
        *slot = t.clamp(0.0, 1.0);
    }
    color32(c)
}

/// The colour a HSV row's track shows at fraction `t`: the kept HSV with
/// component `i` swept across `0..=1`, so the row reads at the current
/// saturation/value the way the SV square does.
fn hsv_channel_tint(hsv: [f32; 3], i: usize, t: f32) -> Color32 {
    let mut c = hsv;
    if let Some(slot) = c.get_mut(i) {
        *slot = t.clamp(0.0, 1.0);
    }
    color32(hsv_to_rgb(c))
}

/// The HSV + RGB slider rows under the hex field (g4 spec). RGB rows edit
/// sRGB-encoded `0..255` (the hex field's space); HSV rows edit the kept
/// HSV, so hue survives greys through [`ColorState::set_hue`] /
/// [`ColorState::set_sv`].
fn channel_rows(ui: &mut Ui, state: &mut ColorState, labels: &PickerLabels, width: f32) {
    let width = width.max(CHANNEL_LABEL_WIDTH + CHANNEL_VALUE_WIDTH + 24.0);
    ui.add_space(2.0);
    let srgb = state.srgb();
    for (i, label) in labels.rgb.iter().enumerate() {
        let byte = (srgb[i] * 255.0).round() as i32;
        let readout = byte.to_string();
        if let Some(t) = channel_row(
            ui,
            width,
            label,
            srgb[i],
            &readout,
            ui.id().with(("color-picker-rgb", i)),
            |t| rgb_channel_tint(srgb, i, t),
        ) {
            state.set_srgb_component(i, t);
        }
    }
    // Re-read: the RGB rows may have changed the HSV this same frame.
    let hsv = state.hsv();
    for (i, label) in labels.hsv.iter().enumerate() {
        let readout = if i == 0 {
            // 0..360, numbers only: umber-gui has no i18n, so no unit glyphs.
            let degrees = hsv[0] * 360.0;
            format!("{degrees:.0}")
        } else {
            let percent = hsv[i] * 100.0;
            format!("{percent:.0}")
        };
        if let Some(t) = channel_row(
            ui,
            width,
            label,
            hsv[i],
            &readout,
            ui.id().with(("color-picker-hsv", i)),
            |t| hsv_channel_tint(hsv, i, t),
        ) {
            match i {
                0 => state.set_hue(t),
                1 => state.set_sv(t, hsv[2]),
                _ => state.set_sv(hsv[1], t),
            }
        }
    }
}

/// One tinted channel slider row: label, gradient track (a [`Mesh`] grid —
/// the SV square's precedent, no texture), the handle at `t`, and a numeric
/// readout. Returns the new fraction when clicked or dragged.
fn channel_row(
    ui: &mut Ui,
    width: f32,
    label: &str,
    t: f32,
    readout: &str,
    id: Id,
    tint: impl Fn(f32) -> Color32,
) -> Option<f32> {
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, CHANNEL_ROW_HEIGHT), Sense::hover());
    let track = Rect::from_min_max(
        Pos2::new(
            row.left() + CHANNEL_LABEL_WIDTH,
            row.center().y - CHANNEL_TRACK_HEIGHT / 2.0,
        ),
        Pos2::new(
            row.right() - CHANNEL_VALUE_WIDTH,
            row.center().y + CHANNEL_TRACK_HEIGHT / 2.0,
        ),
    );
    let response = ui.interact(
        Rect::from_min_max(
            Pos2::new(track.left(), row.top()),
            Pos2::new(track.right(), row.bottom()),
        ),
        id,
        Sense::click_and_drag(),
    );
    // g7-accesskit-sweep: the picker's channel slider rows.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, label));
    let new_t = response
        .interact_pointer_pos()
        .filter(|_| response.dragged() || response.clicked())
        .map(|pos| ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0));
    if ui.is_rect_visible(row) {
        let mut mesh = Mesh::default();
        for i in 0..=CHANNEL_CELLS {
            let s = i as f32 / CHANNEL_CELLS as f32;
            let x = track.left() + track.width() * s;
            let c = tint(s);
            mesh.colored_vertex(Pos2::new(x, track.top()), c);
            mesh.colored_vertex(Pos2::new(x, track.bottom()), c);
        }
        for i in 0..CHANNEL_CELLS as u32 {
            let b = i * 2;
            mesh.add_triangle(b, b + 1, b + 3);
            mesh.add_triangle(b, b + 3, b + 2);
        }
        let painter = ui.painter();
        painter.add(Shape::mesh(mesh));
        painter.rect_stroke(
            track,
            2.0,
            Stroke::new(1.0, theme::faint()),
            StrokeKind::Inside,
        );
        let x = track.left() + track.width() * t.clamp(0.0, 1.0);
        painter.rect_stroke(
            Rect::from_center_size(
                Pos2::new(x, track.center().y),
                Vec2::new(3.0, track.height() + 2.0),
            ),
            1.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Outside,
        );
        painter.text(
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            theme::muted(),
        );
        painter.text(
            Pos2::new(row.right(), row.center().y),
            Align2::RIGHT_CENTER,
            readout,
            FontId::monospace(10.0),
            theme::muted(),
        );
    }
    new_t
}

fn sv_square(ui: &mut Ui, state: &mut ColorState, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    // g7-accesskit-sweep: the picker's saturation/value square.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "saturation / value"));
    if let Some(pos) = response.interact_pointer_pos() {
        if response.dragged() || response.clicked() {
            let s = (pos.x - rect.left()) / rect.width();
            let v = 1.0 - (pos.y - rect.top()) / rect.height();
            state.set_sv(s, v);
        }
    }
    if ui.is_rect_visible(rect) {
        let h = state.hsv()[0];
        let mut mesh = Mesh::default();
        let n = SV_CELLS;
        for row in 0..=n {
            for col in 0..=n {
                let (s, v) = (col as f32 / n as f32, 1.0 - row as f32 / n as f32);
                let pos = Pos2::new(
                    rect.left() + rect.width() * col as f32 / n as f32,
                    rect.top() + rect.height() * row as f32 / n as f32,
                );
                mesh.colored_vertex(pos, color32(hsv_to_rgb([h, s, v])));
            }
        }
        let w = (n + 1) as u32;
        for row in 0..n as u32 {
            for col in 0..n as u32 {
                let i = row * w + col;
                mesh.add_triangle(i, i + 1, i + w + 1);
                mesh.add_triangle(i, i + w + 1, i + w);
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        let [_, s, v] = state.hsv();
        let marker = Pos2::new(
            rect.left() + s * rect.width(),
            rect.top() + (1.0 - v) * rect.height(),
        );
        let ring = if v > 0.5 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        ui.painter()
            .circle_stroke(marker, 5.0, Stroke::new(1.5, ring));
    }
    response
}

fn hue_strip(ui: &mut Ui, state: &mut ColorState, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::click_and_drag());
    // g7-accesskit-sweep: the picker's hue strip.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, "hue"));
    if let Some(pos) = response.interact_pointer_pos() {
        if response.dragged() || response.clicked() {
            let snap = ui.input(|i| i.modifiers.command);
            state.set_hue(hue_at((pos.x - rect.left()) / rect.width(), snap));
        }
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = Mesh::default();
        for i in 0..=HUE_CELLS {
            let t = i as f32 / HUE_CELLS as f32;
            let c = color32(hsv_to_rgb([t, 1.0, 1.0]));
            let x = rect.left() + rect.width() * t;
            mesh.colored_vertex(Pos2::new(x, rect.top()), c);
            mesh.colored_vertex(Pos2::new(x, rect.bottom()), c);
        }
        for i in 0..HUE_CELLS as u32 {
            let b = i * 2;
            mesh.add_triangle(b, b + 1, b + 3);
            mesh.add_triangle(b, b + 3, b + 2);
        }
        ui.painter().add(Shape::mesh(mesh));
        let x = rect.left() + state.hsv()[0] * rect.width();
        ui.painter().rect_stroke(
            Rect::from_center_size(
                Pos2::new(x, rect.center().y),
                Vec2::new(4.0, rect.height() + 2.0),
            ),
            1.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Outside,
        );
    }
    response
}

fn swatches(ui: &mut Ui, state: &mut ColorState) -> Response {
    ui.horizontal(|ui| {
        let (current, _) = ui.allocate_exact_size(Vec2::new(36.0, 18.0), Sense::hover());
        ui.painter()
            .rect_filled(current, 2.0, color32(state.srgb()));
        let (previous, response) = ui.allocate_exact_size(Vec2::new(36.0, 18.0), Sense::click());
        // g7-accesskit-sweep: the "previous colour" restore swatch.
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "previous colour"));
        ui.painter()
            .rect_filled(previous, 2.0, color32(state.previous().map(linear_to_srgb)));
        if response.clicked() {
            state.revert();
        }
        response
    })
    .inner
}

/// The swatch-button form every colour field uses (C2-12): a swatch of
/// `linear`; clicking it opens a [`ColorPicker`] popup below. The popup's
/// [`ColorState`] lives in egui memory under the button's id while it is
/// open, so the hue survives greys across frames and the "previous"
/// swatch is the colour the popup OPENED with. Writes `linear` and
/// reports `changed()` when the colour moved. The drop-in for
/// `Ui::color_edit_button_rgb` (same linear-RGB contract).
pub fn color_button(ui: &mut Ui, linear: &mut [f32; 3]) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(40.0, 18.0), Sense::click());
    // g7-accesskit-sweep: the inline colour swatch (opens the picker).
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "colour"));
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, 3.0, color32(linear.map(linear_to_srgb)));
        ui.painter().rect_stroke(
            rect,
            3.0,
            ui.visuals().widgets.inactive.bg_stroke,
            StrokeKind::Inside,
        );
    }
    let popup_id = response.id.with("color-popup");
    let state_id = response.id.with("color-state");
    let mut open: bool = ui.ctx().data(|d| d.get_temp(popup_id)).unwrap_or(false);
    if response.clicked() {
        open = !open;
        if open {
            ui.ctx()
                .data_mut(|d| d.insert_temp(state_id, ColorState::new(*linear)));
        }
    }
    if open {
        let mut state: ColorState = ui
            .ctx()
            .data(|d| d.get_temp(state_id))
            .unwrap_or_else(|| ColorState::new(*linear));
        let area = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + Vec2::new(0.0, 4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .show(ui, |ui| ColorPicker::new(&mut state).size(150.0).show(ui))
            });
        let picked = area.inner.inner;
        if picked.changed() {
            *linear = state.linear();
            response.mark_changed();
        }
        ui.ctx().data_mut(|d| d.insert_temp(state_id, state));
        // A click outside both the swatch and the popup closes it.
        let clicked_elsewhere = ui.input(|i| i.pointer.any_click())
            && !response.clicked()
            && !area
                .response
                .rect
                .contains(ui.input(|i| i.pointer.interact_pos().unwrap_or_default()));
        if clicked_elsewhere || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(popup_id, open));
    response
}

fn colours_differ(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter()
        .zip(b)
        .any(|(x, y)| (x - y).abs() > SAME_COLOUR_EPS)
}

/// [`color_button`] plus recent colours and caller-supplied row labels: the
/// app-facing form (C2-12's open half). The popup is the same [`ColorPicker`]
/// over the same in-memory [`ColorState`], with a [`SwatchGrid`] of
/// `recents` below it; clicking a swatch sets the colour.
///
/// A colour enters `recents` exactly once, when the popup CLOSES, and only
/// if the final colour differs from the one the popup opened with
/// ([`ColorState::previous`]) — never per drag frame, and never for an
/// untouched colour. umber-gui has no i18n, so `labels` comes from the
/// caller; [`color_button`] stays the stock drop-in with English glyphs and
/// no recents.
pub fn color_button_with(
    ui: &mut Ui,
    linear: &mut [f32; 3],
    labels: &PickerLabels,
    recents: &mut RecentColors,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(40.0, 18.0), Sense::click());
    // g7-accesskit-sweep: the inline colour swatch (opens the picker).
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, "colour"));
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, 3.0, color32(linear.map(linear_to_srgb)));
        ui.painter().rect_stroke(
            rect,
            3.0,
            ui.visuals().widgets.inactive.bg_stroke,
            StrokeKind::Inside,
        );
    }
    let popup_id = response.id.with("color-popup");
    let state_id = response.id.with("color-state");
    let mut open: bool = ui.ctx().data(|d| d.get_temp(popup_id)).unwrap_or(false);
    let was_open = open;
    if response.clicked() {
        open = !open;
        if open {
            ui.ctx()
                .data_mut(|d| d.insert_temp(state_id, ColorState::new(*linear)));
        }
    }
    let state: Option<ColorState> = ui.ctx().data(|d| d.get_temp(state_id));
    if open {
        let mut state = state.unwrap_or_else(|| ColorState::new(*linear));
        let area = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + Vec2::new(0.0, 4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let mut picked = ColorPicker::new(&mut state)
                        .size(150.0)
                        .labels(labels)
                        .show(ui);
                    if !recents.colors().is_empty() {
                        // A gap, not `ui.separator()`: an Area's first-frame
                        // available width is the 600pt default area size, and
                        // a separator would stretch the popup to it.
                        ui.add_space(4.0);
                        if let Some(color) = SwatchGrid::new(recents.colors()).columns(8).show(ui) {
                            *linear = [color.r(), color.g(), color.b()]
                                .map(|c| srgb_to_linear(c as f32 / 255.0));
                            state.set_linear(*linear);
                            picked.mark_changed();
                        }
                    }
                    picked
                })
            });
        let picked = area.inner.inner;
        if picked.changed() {
            *linear = state.linear();
            response.mark_changed();
        }
        ui.ctx().data_mut(|d| d.insert_temp(state_id, state));
        // A click outside both the swatch and the popup closes it.
        let clicked_elsewhere = ui.input(|i| i.pointer.any_click())
            && !response.clicked()
            && !area
                .response
                .rect
                .contains(ui.input(|i| i.pointer.interact_pos().unwrap_or_default()));
        if clicked_elsewhere || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    if was_open && !open {
        if let Some(state) = state {
            if colours_differ(*linear, state.previous()) {
                recents.push(color32(state.srgb()));
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(popup_id, open));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= eps)
    }

    #[test]
    fn srgb_transfer_round_trips_and_hits_the_anchors() {
        for i in 0..=255 {
            let c = i as f32 / 255.0;
            assert!((linear_to_srgb(srgb_to_linear(c)) - c).abs() < 1e-5, "{i}");
        }
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        // sRGB 0.5 is ~21.4 % linear: the curve, not a plain gamma 2.2.
        assert!((srgb_to_linear(0.5) - 0.214_04).abs() < 1e-4);
    }

    #[test]
    fn hsv_round_trips_through_rgb() {
        for &rgb in &[
            [1.0, 0.0, 0.0],
            [0.2, 0.6, 0.9],
            [0.9, 0.9, 0.1],
            [0.3, 0.1, 0.5],
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 0.0],
        ] {
            assert!(close(hsv_to_rgb(rgb_to_hsv(rgb)), rgb, 1e-5), "{rgb:?}");
        }
        assert!(close(
            rgb_to_hsv([0.0, 1.0, 0.0]),
            [1.0 / 3.0, 1.0, 1.0],
            1e-6
        ));
    }

    #[test]
    fn the_field_round_trips_the_colour() {
        let linear = [0.05, 0.4, 0.8];
        let state = ColorState::new(linear);
        assert!(close(state.linear(), linear, 1e-5));
        assert_eq!(state.previous(), linear);
    }

    #[test]
    fn hue_survives_greys_and_black() {
        let mut state = ColorState::new([0.0, 0.0, 1.0]); // blue
        let hue = state.hsv()[0];
        state.set_sv(0.0, 0.7); // drag to grey
        assert_eq!(state.hsv()[0], hue, "grey keeps the hue");
        state.set_sv(1.0, 0.7); // and back
        assert!(state.linear()[2] > state.linear()[0], "still blue, not red");
        state.set_sv(1.0, 0.0); // to black
        state.set_sv(1.0, 1.0);
        assert!(
            close(state.linear(), [0.0, 0.0, 1.0], 1e-5),
            "black kept hue + saturation"
        );
        // A grey typed as hex keeps the hue too.
        assert!(state.set_hex("#808080"));
        assert_eq!(state.hsv()[0], hue);
        assert_eq!(state.hsv()[1], 0.0);
        state.set_sv(1.0, 1.0);
        assert!(close(state.linear(), [0.0, 0.0, 1.0], 1e-5));
    }

    #[test]
    fn hex_is_srgb_and_parses_the_short_forms() {
        let mut state = ColorState::new([srgb_to_linear(1.0), srgb_to_linear(0.5), 0.0]);
        assert_eq!(state.hex(), "#ff8000");
        assert!(state.set_hex("  #00FF80 "));
        assert_eq!(state.hex(), "#00ff80");
        assert!(state.set_hex("f0a"));
        assert_eq!(state.hex(), "#ff00aa");
        for bad in [
            "", "#", "#12345", "#1234567", "gggggg", "#ff 000", "+ff0000",
        ] {
            let before = state;
            assert!(!state.set_hex(bad), "{bad:?}");
            assert_eq!(state, before, "a bad hex changes nothing");
        }
        // A non-ASCII string never panics on a byte boundary.
        assert!(parse_hex("#ffé").is_none());
    }

    #[test]
    fn revert_restores_the_opening_colour() {
        let mut state = ColorState::new([0.2, 0.3, 0.4]);
        state.set_hue(0.9);
        state.set_sv(0.1, 0.1);
        state.revert();
        assert!(close(state.linear(), [0.2, 0.3, 0.4], 1e-5));
    }

    #[test]
    fn ctrl_drag_snaps_hue_to_30_degrees() {
        assert!((snap_hue(0.04) - 0.0).abs() < 1e-6);
        assert!((snap_hue(0.07) - 1.0 / 12.0).abs() < 1e-6);
        assert!((snap_hue(0.99) - 0.0).abs() < 1e-6, "wraps to red");
        assert!((hue_at(0.2, false) - 0.2).abs() < 1e-6);
        assert!((hue_at(0.2, true) - 2.0 / 12.0).abs() < 1e-6);
        assert_eq!(hue_at(1.0, false), 0.0);
        assert_eq!(hue_at(-3.0, false), 0.0);
    }

    #[test]
    fn non_finite_input_never_poisons_the_state() {
        let mut state = ColorState::new([f32::NAN, f32::INFINITY, -1.0]);
        assert!(state.linear().iter().all(|c| c.is_finite()));
        state.set_hue(f32::NAN);
        state.set_sv(f32::NAN, f32::INFINITY);
        state.set_linear([f32::NAN; 3]);
        assert!(state.linear().iter().all(|c| c.is_finite()));
    }

    #[test]
    fn the_color_button_opens_a_picker_and_writes_the_colour_back() {
        use egui_kittest::kittest::Queryable;
        struct S {
            color: [f32; 3],
            changed: bool,
            swatch: Rect,
        }
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(400.0, 400.0))
            .build_ui_state(
                |ui, s: &mut S| {
                    let r = color_button(ui, &mut s.color);
                    s.changed |= r.changed();
                    s.swatch = r.rect;
                },
                S {
                    color: [1.0, 0.0, 0.0],
                    changed: false,
                    swatch: Rect::NOTHING,
                },
            );
        harness.run();
        let swatch = harness.state().swatch;
        let click = |h: &mut egui_kittest::Harness<'_, S>, pos: egui::Pos2| {
            h.event(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.run();
            }
        };
        click(&mut harness, swatch.center());
        // The popup is open: its hex field shows the opening colour.
        assert!(
            // A TextEdit exposes its value on more than one AccessKit node.
            harness.query_all_by_value("#ff0000").next().is_some(),
            "the picker opened on red"
        );
        // A click in the popup's SV square, low s / low v, writes back.
        let sv = egui::Pos2::new(swatch.left() + 20.0, swatch.bottom() + 4.0 + 8.0 + 130.0);
        click(&mut harness, sv);
        let s = harness.state();
        assert!(s.changed, "the button reports the change");
        assert!(
            s.color[0] < 0.5,
            "the colour moved off full red: {:?}",
            s.color
        );
    }

    #[test]
    fn dragging_the_sv_square_changes_the_colour() {
        // kittest: a click in the square writes s/v through the widget.
        // The square's rect is recorded from the frame (a guessed pixel
        // is how the property-menu test first went wrong).
        struct S {
            color: ColorState,
            changed: bool,
            square: Rect,
        }
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, state: &mut S| {
                let top_left = ui.cursor().min;
                let response = ColorPicker::new(&mut state.color).size(100.0).show(ui);
                state.changed |= response.changed();
                state.square = Rect::from_min_size(top_left, Vec2::splat(100.0));
            },
            S {
                color: ColorState::new([1.0, 0.0, 0.0]),
                changed: false,
                square: Rect::NOTHING,
            },
        );
        harness.run();
        let square = harness.state().square;
        assert!(square.is_positive(), "the square was laid out");
        let target = square.left_top() + Vec2::new(10.0, 90.0);
        harness.event(egui::Event::PointerMoved(target));
        harness.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        harness.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        let state = harness.state();
        assert!(state.changed, "the widget reports the change");
        let [h, s, v] = state.color.hsv();
        assert!(
            (s - 0.1).abs() < 0.05 && (v - 0.1).abs() < 0.05,
            "s/v follow the click: {:?}",
            state.color.hsv()
        );
        assert_eq!(h, 0.0, "an SV click leaves the hue alone");
    }

    #[test]
    fn the_inline_swatch_reports_a_button_role_to_accesskit() {
        use egui_kittest::kittest::Queryable;
        struct S {
            color: [f32; 3],
        }
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, s: &mut S| {
                color_button(ui, &mut s.color);
            },
            S {
                color: [1.0, 0.0, 0.0],
            },
        );
        harness.run();
        let _ = harness.get_by_role_and_label(egui::accesskit::Role::Button, "colour");
    }

    fn click_at<S>(harness: &mut egui_kittest::Harness<'_, S>, pos: Pos2) {
        harness.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
    }

    #[test]
    fn an_srgb_component_round_trips_128() {
        // 128 / 255 is the encoded value; `set_srgb_component` stores the
        // linear form and `srgb()` comes back to the same byte.
        let encoded = 128.0 / 255.0;
        let mut state = ColorState::new([0.0, 0.0, 0.0]);
        state.set_srgb_component(0, encoded);
        assert!(
            (state.srgb()[0] - encoded).abs() < 1e-5,
            "round-trip: {}",
            state.srgb()[0]
        );
        assert_eq!((state.srgb()[0] * 255.0).round() as u8, 128);
        assert_eq!(state.hex(), "#800000");
        // The stored linear value is the IEC curve's, not byte / 255.
        let linear = srgb_to_linear(encoded);
        assert!((state.linear()[0] - linear).abs() < 1e-5);
        assert!(
            (linear - encoded).abs() > 0.2,
            "sRGB is not a byte/255 linear map"
        );
        assert_eq!(state.srgb()[1], 0.0, "the other components are untouched");
        assert_eq!(state.srgb()[2], 0.0);
    }

    #[test]
    fn setting_the_three_components_to_a_grey_keeps_the_hue() {
        let mut state = ColorState::new([1.0, 0.0, 0.0]); // red
        assert_eq!(state.hsv(), [0.0, 1.0, 1.0]);
        // Red's own channel last: the G and B edits stay red-hued, and the
        // final edit makes the grey, where `set_linear` keeps the hue.
        state.set_srgb_component(1, 0.5);
        state.set_srgb_component(2, 0.5);
        state.set_srgb_component(0, 0.5);
        assert!(close(state.srgb(), [0.5, 0.5, 0.5], 1e-5));
        assert_eq!(state.hsv()[0], 0.0, "the grey kept the hue");
        assert_eq!(state.hsv()[1], 0.0, "a grey has no saturation");
        // Saturation back restores the red the hue remembered.
        state.set_sv(1.0, 1.0);
        assert!(
            close(state.linear(), [1.0, 0.0, 0.0], 1e-5),
            "S back restores red: {:?}",
            state.linear()
        );
    }

    #[test]
    fn the_channel_tracks_are_tinted_across_their_range() {
        // Pins the mesh vertex tints at 0 %, 50 % and 100 %.
        // sRGB (0.2, 0.4, 0.6): 0.2*255 = 51, 0.4*255 = 102,
        // 0.6*255 = 153; 0.5*255 = 127.5 rounds to 128.
        let srgb = [0.2, 0.4, 0.6];
        let r = [0.0, 0.5, 1.0].map(|t| rgb_channel_tint(srgb, 0, t));
        assert_eq!(
            r,
            [
                Color32::from_rgb(0, 102, 153),
                Color32::from_rgb(128, 102, 153),
                Color32::from_rgb(255, 102, 153),
            ]
        );
        // The hue row at full saturation/value: red, cyan, red (1 is the
        // same hue as 0).
        let hue = [0.0, 0.5, 1.0].map(|t| hsv_channel_tint([0.0, 1.0, 1.0], 0, t));
        assert_eq!(
            hue,
            [
                Color32::from_rgb(255, 0, 0),
                Color32::from_rgb(0, 255, 255),
                Color32::from_rgb(255, 0, 0),
            ]
        );
    }

    #[test]
    fn dragging_a_channel_slider_changes_the_colour() {
        use egui_kittest::kittest::Queryable;
        struct S {
            color: ColorState,
        }
        let labels = PickerLabels::default();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(400.0, 400.0))
            .build_ui_state(
                |ui, s: &mut S| {
                    ColorPicker::new(&mut s.color)
                        .size(160.0)
                        .labels(&labels)
                        .show(ui);
                },
                S {
                    color: ColorState::new([srgb_to_linear(0.5); 3]),
                },
            );
        harness.run();
        let track = harness
            .get_by_role_and_label(egui::accesskit::Role::Slider, "G")
            .rect();
        assert!(track.is_positive(), "the G row was laid out");
        let start = Pos2::new(track.left() + track.width() * 0.25, track.center().y);
        let end = Pos2::new(track.left() + track.width() * 0.75, track.center().y);
        harness.hover_at(start);
        harness.drag_at(start);
        harness.run();
        harness.event(egui::Event::PointerMoved(end));
        harness.run();
        harness.drop_at(end);
        harness.run();
        let srgb = harness.state().color.srgb();
        assert!(
            (srgb[1] - 0.75).abs() < 0.02,
            "G follows the drag: {srgb:?}"
        );
        assert!(
            (srgb[0] - 0.5).abs() < 0.02 && (srgb[2] - 0.5).abs() < 0.02,
            "R/B stay put: {srgb:?}"
        );
    }

    #[test]
    fn a_channel_slider_reports_a_slider_role_to_accesskit() {
        use egui_kittest::kittest::Queryable;
        let labels = PickerLabels::default();
        let mut color = ColorState::new([1.0, 0.0, 0.0]);
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            ColorPicker::new(&mut color)
                .size(160.0)
                .labels(&labels)
                .show(ui);
        });
        harness.run();
        for label in ["R", "G", "B", "H", "S", "V"] {
            let _ = harness.get_by_role_and_label(egui::accesskit::Role::Slider, label);
        }
    }

    #[test]
    fn recents_are_pushed_once_when_the_popup_closes() {
        use egui_kittest::kittest::Queryable;
        struct S {
            color: [f32; 3],
            recents: RecentColors,
            changed: bool,
            swatch: Rect,
        }
        let labels = PickerLabels::default();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(600.0, 900.0))
            .build_ui_state(
                |ui, s: &mut S| {
                    let r = color_button_with(ui, &mut s.color, &labels, &mut s.recents);
                    s.changed |= r.changed();
                    s.swatch = r.rect;
                },
                S {
                    color: [1.0, 0.0, 0.0],
                    recents: RecentColors::default(),
                    changed: false,
                    swatch: Rect::NOTHING,
                },
            );
        harness.run();
        let swatch = harness.state().swatch;
        click_at(&mut harness, swatch.center());
        let square = harness
            .get_by_role_and_label(egui::accesskit::Role::Slider, "saturation / value")
            .rect();
        assert!(square.is_positive(), "the popup's SV square was laid out");
        // Three drag frames inside the square: the colour moves, recents
        // must not.
        let path = [
            square.left_top() + Vec2::new(20.0, 20.0),
            square.left_top() + Vec2::new(30.0, 40.0),
            square.left_top() + Vec2::new(40.0, 60.0),
            square.left_top() + Vec2::new(50.0, 80.0),
        ];
        harness.hover_at(path[0]);
        harness.drag_at(path[0]);
        harness.run();
        for pos in &path[1..] {
            harness.event(egui::Event::PointerMoved(*pos));
            harness.run();
        }
        harness.drop_at(path[3]);
        harness.run();
        assert!(
            harness.state().recents.colors().is_empty(),
            "no recents push while the popup is open"
        );
        assert!(harness.state().changed, "the drag changed the colour");
        click_at(&mut harness, swatch.center());
        let s = harness.state();
        assert_eq!(s.recents.colors().len(), 1, "one entry on close");
        let recorded = s.recents.colors()[0];
        assert_ne!(
            recorded,
            Color32::from_rgb(255, 0, 0),
            "not the opening red"
        );
        let expected = s.color.map(linear_to_srgb);
        assert!(
            (recorded.r() as f32 - expected[0] * 255.0).abs() <= 1.5
                && (recorded.g() as f32 - expected[1] * 255.0).abs() <= 1.5
                && (recorded.b() as f32 - expected[2] * 255.0).abs() <= 1.5,
            "the final colour was recorded: {recorded:?} vs {expected:?}"
        );
    }

    #[test]
    fn closing_without_a_change_records_nothing() {
        struct S {
            color: [f32; 3],
            recents: RecentColors,
            swatch: Rect,
        }
        let labels = PickerLabels::default();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(600.0, 900.0))
            .build_ui_state(
                |ui, s: &mut S| {
                    let r = color_button_with(ui, &mut s.color, &labels, &mut s.recents);
                    s.swatch = r.rect;
                },
                S {
                    color: [0.2, 0.4, 0.6],
                    recents: RecentColors::default(),
                    swatch: Rect::NOTHING,
                },
            );
        harness.run();
        let swatch = harness.state().swatch;
        click_at(&mut harness, swatch.center());
        click_at(&mut harness, swatch.center());
        assert!(
            harness.state().recents.colors().is_empty(),
            "an untouched popup records nothing"
        );
    }

    #[test]
    fn clicking_a_recent_swatch_sets_the_colour() {
        use egui_kittest::kittest::Queryable;
        struct S {
            color: [f32; 3],
            recents: RecentColors,
            changed: bool,
            swatch: Rect,
        }
        let labels = PickerLabels::default();
        let mut recents = RecentColors::default();
        recents.push(Color32::from_rgb(0, 255, 0));
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(600.0, 900.0))
            .build_ui_state(
                |ui, s: &mut S| {
                    let r = color_button_with(ui, &mut s.color, &labels, &mut s.recents);
                    s.changed |= r.changed();
                    s.swatch = r.rect;
                },
                S {
                    color: [1.0, 0.0, 0.0],
                    recents,
                    changed: false,
                    swatch: Rect::NOTHING,
                },
            );
        harness.run();
        let swatch = harness.state().swatch;
        click_at(&mut harness, swatch.center());
        let grid = harness.get_by_label("colour palette").rect();
        assert!(grid.is_positive(), "the recents grid is in the popup");
        // The first cell: 16pt swatch, centred 8pt into the grid's min.
        click_at(&mut harness, grid.min + Vec2::new(8.0, 8.0));
        let s = harness.state();
        assert!(s.changed, "the recent pick reports a change");
        assert!(
            close(s.color, [0.0, 1.0, 0.0], 1e-3),
            "the recent green was applied: {:?}",
            s.color
        );
    }
}
