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
//!   pair (click previous to revert), and a hex field. Ctrl/Cmd-drag on the
//!   hue strip snaps to 30°.
//!
//! The umber-gui layer rule (egui only) holds: no colour-management crate.
//! The sRGB transfer functions here are the IEC 61966-2-1 piecewise curve,
//! the same one egui's `Rgba` ↔ `Color32` conversion uses.

use egui::{Color32, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

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
    if c <= 0.003_130_8 {
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

fn color32(srgb: [f32; 3]) -> Color32 {
    let [r, g, b] = srgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

/// The picker widget over a [`ColorState`] (see the module docs).
pub struct ColorPicker<'a> {
    state: &'a mut ColorState,
    size: f32,
}

impl<'a> ColorPicker<'a> {
    /// A picker editing `state`, SV square `size` points wide.
    pub fn new(state: &'a mut ColorState) -> Self {
        Self { state, size: 160.0 }
    }

    /// The SV square's side, in points.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size.max(32.0);
        self
    }

    /// Draws the picker; the response is `changed()` when the colour moved.
    pub fn show(self, ui: &mut Ui) -> Response {
        let Self { state, size } = self;
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
        if *state != before {
            response.mark_changed();
        }
        response
    }
}

fn sv_square(ui: &mut Ui, state: &mut ColorState, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
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
        ui.painter()
            .rect_filled(previous, 2.0, color32(state.previous().map(linear_to_srgb)));
        if response.clicked() {
            state.revert();
        }
        response
    })
    .inner
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
}
