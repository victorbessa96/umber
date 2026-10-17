//! `value_field` — the DCC numeric workhorse
//! (docs/specs/gui-audit-and-roadmap.md §3 Phase 0a).
//!
//! - Drag horizontally to scrub. Shift = fine (×0.1), Ctrl/Cmd = coarse
//!   (×10). The scrub accumulates an unsnapped value across frames, so
//!   slow drags still cross step boundaries.
//! - Alt + mouse wheel over the field = ±1 step per notch. A plain wheel
//!   still scrolls the panel.
//! - Right-click copies the bare number (field decimals, no unit) to the
//!   clipboard.
//! - The value renders in the tabular font (JetBrains Mono, see
//!   [`crate::fonts::numeric_font`]). The unit renders muted. A finite
//!   range draws an accent fill bar under the value.
//!
//! - [`ValueField::logarithmic`] (2026-10-15): for a range spanning
//!   decades (DoF focus 0.01..=100), the scrub moves through LOG space —
//!   each point of drag multiplies the value by a constant factor — and the
//!   fill bar shows the log fraction, so every decade gets the same drag
//!   distance. A linear scrub there makes the bottom decades unreachable.
//!   Requires a finite range with `min > 0`; otherwise it falls back to
//!   linear (logged nowhere: the field still works).
//!
//! # The field grammar (kill `g4-valuefield-grammar`, B:D1-D9, P:D2, PC:#8)
//!
//! - Click without dragging, or Tab into the field, to TYPE a value: the
//!   field turns into a text edit with everything selected. Enter or
//!   leaving the field (Tab to the next one, a click elsewhere) commits;
//!   Escape cancels. The text is parsed by [`eval_expr`]: numbers,
//!   `+ - * /`, parentheses and unary minus (`2*512`, `-(1/3)`), with an
//!   optional trailing unit that matches the field's. Anything else is
//!   rejected and the old value stays.
//! - Ctrl/Cmd + wheel over the field steps it (one step per notch). Alt +
//!   wheel still works. A plain wheel scrolls the panel.
//! - [`ValueField::soft_range`]: dragging clamps to the soft range, while
//!   typing, pasting and the wheel reach the hard [`ValueField::range`]
//!   (Blender's soft/hard limits). The fill bar shows the soft range.
//! - Hover + Ctrl/Cmd+C copies the value, hover + Ctrl/Cmd+V pastes one
//!   (through [`eval_expr`]). Right-click still copies.
//! - Hover + Backspace resets to [`ValueField::default_value`], when one is
//!   set. A double-click reset is NOT offered: the first click of a
//!   double-click already opens typing.
//! - [`ValueField::context_menu`] (kill `g4-property-context-menu`,
//!   B:D10): right-click opens Reset to Default / Copy / Paste. umber-gui
//!   has no i18n, so the caller passes the labels ([`MenuLabels`]) already
//!   translated. Without labels, right-click keeps copying the value.
//!   Paste reads the field's own clipboard (the last value a field
//!   copied), because egui exposes no clipboard READ outside its paste
//!   event; a value copied from another app arrives by hover + Ctrl+V.
//!
//! The math is in free functions so it is testable without an
//! `egui::Context`.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{Align2, CursorIcon, Id, Response, Sense, Ui, Vec2, Widget, WidgetInfo};

use crate::fonts::numeric_font;
use crate::theme;

/// Shift: fine scrub factor.
pub const FINE_FACTOR: f64 = 0.1;
/// Ctrl/Cmd: coarse scrub factor.
pub const COARSE_FACTOR: f64 = 10.0;

/// Clamp into `[min, max]` (either bound may be infinite). NaN maps to
/// 0 clamped into the range, which keeps NaN out of engine state.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        return 0.0f64.clamp(min, max);
    }
    value.clamp(min, max)
}

/// Snap `value` to the nearest multiple of `step` from `origin`.
/// `step <= 0` disables snapping.
pub fn snap(value: f64, step: f64, origin: f64) -> f64 {
    if step.is_nan() || step <= 0.0 || !value.is_finite() {
        return value;
    }
    origin + ((value - origin) / step).round() * step
}

/// Snap then clamp. The snap origin is the range minimum when finite
/// (else 0), so a 0.5 step on 0.25..=1 lands on 0.25, 0.75. Clamping
/// last guarantees the bound.
pub fn snap_clamp(value: f64, step: f64, min: f64, max: f64) -> f64 {
    let origin = if min.is_finite() { min } else { 0.0 };
    clamp(snap(value, step, origin), min, max)
}

/// The value after `steps` wheel/arrow steps.
pub fn step_value(value: f64, steps: i32, step: f64, min: f64, max: f64) -> f64 {
    snap_clamp(value + steps as f64 * step, step, min, max)
}

/// Wheel delta → whole steps: one step per event direction, regardless of
/// how many points the platform reports per notch.
pub fn wheel_steps(delta_y: f32) -> i32 {
    if delta_y > 0.0 {
        1
    } else if delta_y < 0.0 {
        -1
    } else {
        0
    }
}

/// The scrub-speed multiplier from the held modifiers. Fine wins when
/// both are held.
pub fn modifier_factor(shift: bool, command: bool) -> f64 {
    if shift {
        FINE_FACTOR
    } else if command {
        COARSE_FACTOR
    } else {
        1.0
    }
}

/// Advance the unsnapped scrub accumulator by a drag of `dx` points.
pub fn scrub(raw: f64, dx: f32, speed: f64, factor: f64) -> f64 {
    raw + dx as f64 * speed * factor
}

/// Display string: fixed `decimals`, then the unit after a thin space.
pub fn format_value(value: f64, decimals: usize, unit: &str) -> String {
    if unit.is_empty() {
        format!("{value:.decimals$}")
    } else {
        format!("{value:.decimals$}\u{2009}{unit}")
    }
}

/// Evaluates a typed value: decimal numbers, `+ - * /`, parentheses and
/// unary signs, with whitespace anywhere. A trailing `unit` (the field's
/// own, e.g. `px`) is accepted and ignored. Returns `None` for any other
/// input, for division by zero, and for non-finite results, so a typo
/// never writes NaN or infinity into engine state. Recursion depth is
/// capped, so a pathological `((((…` cannot overflow the stack.
pub fn eval_expr(text: &str, unit: &str) -> Option<f64> {
    let mut body = text.trim();
    if !unit.is_empty() {
        if let Some(stripped) = body.strip_suffix(unit) {
            body = stripped.trim_end();
        }
    }
    let mut parser = ExprParser {
        bytes: body.as_bytes(),
        pos: 0,
        depth: 0,
    };
    let value = parser.expr()?;
    parser.skip_ws();
    (parser.pos == parser.bytes.len() && value.is_finite()).then_some(value)
}

/// Nesting cap for [`eval_expr`].
const MAX_EXPR_DEPTH: u32 = 32;

struct ExprParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: u32,
}

impl ExprParser<'_> {
    fn skip_ws(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    /// expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        while let Some(op @ (b'+' | b'-')) = self.peek() {
            self.pos += 1;
            let rhs = self.term()?;
            value = if op == b'+' { value + rhs } else { value - rhs };
        }
        Some(value)
    }

    /// term := unary (('*' | '/') unary)*
    fn term(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        while let Some(op @ (b'*' | b'/')) = self.peek() {
            self.pos += 1;
            let rhs = self.unary()?;
            value = if op == b'*' {
                value * rhs
            } else if rhs == 0.0 {
                return None;
            } else {
                value / rhs
            };
        }
        Some(value)
    }

    /// unary := ('-' | '+') unary | atom
    fn unary(&mut self) -> Option<f64> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            return None;
        }
        let value = match self.peek()? {
            b'-' => {
                self.pos += 1;
                self.unary().map(|v| -v)
            }
            b'+' => {
                self.pos += 1;
                self.unary()
            }
            _ => self.atom(),
        };
        self.depth -= 1;
        value
    }

    /// atom := number | '(' expr ')'
    fn atom(&mut self) -> Option<f64> {
        if self.peek()? == b'(' {
            self.pos += 1;
            let value = self.expr()?;
            if self.peek()? != b')' {
                return None;
            }
            self.pos += 1;
            return Some(value);
        }
        let start = self.pos;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
        {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()?
            .parse::<f64>()
            .ok()
    }
}

/// The right-click menu's labels, already translated by the caller.
/// `Cow` so a caller can hand over freshly translated `String`s (the app's
/// `tr()` output) without keeping them alive past the builder.
#[derive(Debug, Clone)]
pub struct MenuLabels<'a> {
    /// "Reset to Default".
    pub reset: std::borrow::Cow<'a, str>,
    /// "Copy".
    pub copy: std::borrow::Cow<'a, str>,
    /// "Paste".
    pub paste: std::borrow::Cow<'a, str>,
}

/// The clipboard every field shares within one egui context (temp data
/// under one id): what Copy wrote last, as the bare number.
fn shared_clip_id() -> Id {
    Id::new("umber-gui-value-field-clip")
}

/// What a context-menu Paste of `clip` gives, clamped to the hard range:
/// `None` when nothing was copied yet or the text does not evaluate.
pub fn menu_paste(clip: Option<&str>, unit: &str, min: f64, max: f64) -> Option<f64> {
    clip.and_then(|text| eval_expr(text, unit))
        .map(|v| clamp(v, min, max))
}

/// The drag clamp: the soft range intersected with the hard one (an
/// inverted intersection falls back to the hard range).
pub fn soft_bounds(hard: (f64, f64), soft: Option<(f64, f64)>) -> (f64, f64) {
    match soft {
        Some((lo, hi)) => {
            let (lo, hi) = (lo.max(hard.0), hi.min(hard.1));
            if lo <= hi {
                (lo, hi)
            } else {
                hard
            }
        }
        None => hard,
    }
}

/// Whether a log scrub is possible on `[min, max]`: both finite, `min > 0`,
/// `max > min`.
pub fn log_capable(min: f64, max: f64) -> bool {
    min.is_finite() && max.is_finite() && min > 0.0 && max > min
}

/// The log-mode scrub: `dx` points of drag over a `[min, max]` field
/// `width_pts` wide move `raw` by `dx / width_pts` of the full log span, so
/// dragging the field's own width traverses the whole range. `factor` is
/// the modifier multiplier (fine / coarse). A non-positive `raw` restarts
/// from `min`.
pub fn log_scrub(raw: f64, dx: f32, width_pts: f32, min: f64, max: f64, factor: f64) -> f64 {
    let start = if raw > 0.0 { raw } else { min };
    if !log_capable(min, max) || width_pts.is_nan() || width_pts <= 0.0 {
        return start;
    }
    let span = (max / min).ln();
    (start.ln() + f64::from(dx) / f64::from(width_pts) * span * factor).exp()
}

/// The log-space fill fraction (`ln(v/min) / ln(max/min)`), `None` when the
/// range cannot be log-scaled.
pub fn log_fill_fraction(value: f64, min: f64, max: f64) -> Option<f32> {
    if !log_capable(min, max) {
        return None;
    }
    let v = value.max(min);
    Some(((v / min).ln() / (max / min).ln()).clamp(0.0, 1.0) as f32)
}

/// The 0..1 fill fraction for a finite range, `None` otherwise.
pub fn fill_fraction(value: f64, min: f64, max: f64) -> Option<f32> {
    if !(min.is_finite() && max.is_finite()) || max <= min {
        return None;
    }
    Some(((value - min) / (max - min)).clamp(0.0, 1.0) as f32)
}

/// A numeric field over any egui `Numeric` (f32, f64, integers).
#[must_use = "add it with `ui.add(...)`"]
pub struct ValueField<'a, N: Numeric> {
    value: &'a mut N,
    min: f64,
    max: f64,
    step: f64,
    speed: f64,
    decimals: usize,
    unit: &'a str,
    width: Option<f32>,
    logarithmic: bool,
    soft: Option<(f64, f64)>,
    default: Option<f64>,
    menu: Option<MenuLabels<'a>>,
}

impl<'a, N: Numeric> ValueField<'a, N> {
    /// An unbounded field, clamped only to `N`'s own range on write.
    /// Integers step by 1 with no decimals. Floats step by 0.01 with 2
    /// decimals. Unbounded means no fill bar, and the snap origin is 0.
    /// (Using `N::MIN` as the origin would lose all f64 precision for f32.)
    pub fn new(value: &'a mut N) -> Self {
        let integral = N::INTEGRAL;
        ValueField {
            value,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step: if integral { 1.0 } else { 0.01 },
            speed: if integral { 0.25 } else { 0.005 },
            decimals: if integral { 0 } else { 2 },
            unit: "",
            width: None,
            logarithmic: false,
            soft: None,
            default: None,
            menu: None,
        }
    }

    /// Right-click opens Reset to Default / Copy / Paste with these labels
    /// (see the module docs). Reset is offered only with a
    /// [`Self::default_value`].
    pub fn context_menu(mut self, labels: MenuLabels<'a>) -> Self {
        self.menu = Some(labels);
        self
    }

    /// The drag range (see the module docs). Typing still reaches the
    /// hard [`Self::range`].
    pub fn soft_range(mut self, range: RangeInclusive<N>) -> Self {
        self.soft = Some((range.start().to_f64(), range.end().to_f64()));
        self
    }

    /// The value hover + Backspace resets to.
    pub fn default_value(mut self, value: N) -> Self {
        self.default = Some(value.to_f64());
        self
    }

    /// Log-space scrubbing + fill (see the module docs). Needs a finite
    /// range with `min > 0`; otherwise the field stays linear. Snapping
    /// still applies after the scrub (use a small `step`, or `0.0` to
    /// disable, for a range spanning decades).
    pub fn logarithmic(mut self, logarithmic: bool) -> Self {
        self.logarithmic = logarithmic;
        self
    }

    /// Clamp range.
    pub fn range(mut self, range: RangeInclusive<N>) -> Self {
        self.min = range.start().to_f64();
        self.max = range.end().to_f64();
        self
    }

    /// Snap/wheel step. Integers never go below 1.
    pub fn step(mut self, step: f64) -> Self {
        self.step = if N::INTEGRAL {
            step.max(1.0).round()
        } else {
            step
        };
        self
    }

    /// Value units per dragged point.
    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// Displayed decimals (ignored for integers).
    pub fn decimals(mut self, decimals: usize) -> Self {
        if !N::INTEGRAL {
            self.decimals = decimals;
        }
        self
    }

    /// The unit suffix ("px", "%", "°").
    pub fn unit(mut self, unit: &'a str) -> Self {
        self.unit = unit;
        self
    }

    /// Fixed width in points (default: 2× the interact width).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

impl<N: Numeric> Widget for ValueField<'_, N> {
    fn ui(self, ui: &mut Ui) -> Response {
        let width = self.width.unwrap_or(ui.spacing().interact_size.x * 2.0);
        let size = Vec2::new(width, ui.spacing().interact_size.y);
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        // g7-accesskit-sweep: the field is a number editor (Slider
        // role, the value as the accesskit value; `unit` is its only
        // label source — the app's tr() strings name fields at the
        // panel layer, not here).
        response.widget_info(|| WidgetInfo::slider(true, self.value.to_f64(), self.unit));
        let id: Id = response.id.with("scrub-raw");
        let edit_id: Id = response.id.with("edit-buf");
        let text_id: Id = response.id.with("edit-text");
        let hard = (self.min, self.max);
        let (drag_min, drag_max) = soft_bounds(hard, self.soft);

        let old = self.value.to_f64();
        let mut new = old;

        // Typed entry: a click (no drag) or keyboard focus opens it.
        let mut editing: Option<String> = ui.ctx().data(|d| d.get_temp(edit_id));
        if editing.is_none() && (response.clicked() || response.gained_focus()) {
            editing = Some(format_value(old, self.decimals, ""));
            ui.ctx()
                .data_mut(|d| d.insert_temp(text_id.with("fresh"), true));
        }
        if let Some(mut buffer) = editing {
            // Focus the text edit on its first frame WITHOUT a click: in the
            // opening click's own frame the edit is new, so egui's hit test
            // (last frame's rects) sees the click as elsewhere and would
            // surrender the focus straight away.
            let fresh_id = text_id.with("fresh");
            let fresh: bool = ui.ctx().data(|d| d.get_temp(fresh_id)).unwrap_or(false);
            let focus_now = fresh && !ui.input(|i| i.pointer.any_click());
            if focus_now {
                ui.ctx().memory_mut(|m| m.request_focus(text_id));
            }
            let output = egui::TextEdit::singleline(&mut buffer)
                .id(text_id)
                .font(numeric_font(
                    ui.style().text_styles[&egui::TextStyle::Monospace].size,
                ))
                .horizontal_align(egui::Align::Center)
                .desired_width(rect.width())
                .show(&mut ui.new_child(egui::UiBuilder::new().max_rect(rect)));
            // Select everything on the focusing frame, so typing replaces.
            if focus_now {
                ui.ctx().data_mut(|d| d.remove::<bool>(fresh_id));
                let mut state = output.state.clone();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::select_all(&output.galley)));
                state.store(ui.ctx(), text_id);
            }
            let text_response = output.response.response;
            if text_response.lost_focus() {
                let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if !cancelled {
                    if let Some(typed) = eval_expr(&buffer, self.unit) {
                        new = clamp(typed, hard.0, hard.1);
                    }
                }
                ui.ctx().data_mut(|d| d.remove::<String>(edit_id));
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(edit_id, buffer));
            }
            if new != old {
                write_back(self.value, new, &mut response);
            }
            return response;
        }

        // Drag-scrub over the persistent unsnapped accumulator, clamped to
        // the soft range.
        if response.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(id, old));
        }
        if response.dragged() {
            let (shift, command) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
            let raw: f64 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(old);
            let factor = modifier_factor(shift, command);
            let raw = if self.logarithmic && log_capable(drag_min, drag_max) {
                log_scrub(
                    raw,
                    response.drag_delta().x,
                    rect.width(),
                    drag_min,
                    drag_max,
                    factor,
                )
            } else {
                scrub(raw, response.drag_delta().x, self.speed, factor)
            };
            ui.ctx().data_mut(|d| d.insert_temp(id, raw));
            new = snap_clamp(raw, self.step, drag_min, drag_max);
        }
        if response.drag_stopped() {
            ui.ctx().data_mut(|d| d.remove::<f64>(id));
        }

        let hovered = response.hovered() && !response.dragged();
        // Hover gestures: wheel steps, clipboard, reset. None of them fires
        // while a text field elsewhere owns the keyboard.
        if hovered && !ui.ctx().text_edit_focused() {
            // Ctrl/Cmd + wheel (egui turns it into zoom, so read the raw
            // wheel events) or Alt + wheel (the smoothed scroll) steps.
            // A plain wheel keeps scrolling the panel's ScrollArea.
            let cmd_steps: i32 = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel {
                            delta, modifiers, ..
                        } if modifiers.command => Some(wheel_steps(delta.y)),
                        _ => None,
                    })
                    .sum()
            });
            let alt = ui.input(|i| i.modifiers.alt);
            let alt_steps = if alt {
                wheel_steps(ui.input(|i| i.smooth_scroll_delta.y))
            } else {
                0
            };
            let steps = cmd_steps + alt_steps;
            if steps != 0 {
                new = step_value(new, steps, self.step, hard.0, hard.1);
                ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }

            let (copy, paste, reset) = ui.input_mut(|i| {
                let mut copy = false;
                let mut paste = None;
                let mut reset = false;
                let has_default = self.default.is_some();
                i.events.retain(|e| match e {
                    egui::Event::Copy => {
                        copy = true;
                        false
                    }
                    egui::Event::Paste(text) => {
                        paste = Some(text.clone());
                        false
                    }
                    egui::Event::Key {
                        key: egui::Key::Backspace,
                        pressed: true,
                        ..
                    } if has_default => {
                        reset = true;
                        false
                    }
                    _ => true,
                });
                (copy, paste, reset)
            });
            if copy {
                let text = format_value(new, self.decimals, "");
                ui.ctx()
                    .data_mut(|d| d.insert_temp(shared_clip_id(), text.clone()));
                ui.ctx().copy_text(text);
            }
            if let Some(value) = paste.and_then(|text| eval_expr(&text, self.unit)) {
                new = clamp(value, hard.0, hard.1);
            }
            if let (true, Some(default)) = (reset, self.default) {
                new = clamp(default, hard.0, hard.1);
            }
        }

        match self.menu {
            // Right-click opens the property menu.
            Some(labels) => {
                let decimals = self.decimals;
                let unit = self.unit;
                let default = self.default;
                let current = new;
                let mut picked = None;
                response.context_menu(|ui| {
                    if let Some(d) = default {
                        let at_default = (current - d).abs() < f64::EPSILON;
                        if ui
                            .add_enabled(!at_default, egui::Button::new(labels.reset.as_ref()))
                            .clicked()
                        {
                            picked = Some(clamp(d, hard.0, hard.1));
                            ui.close();
                        }
                    }
                    if ui.button(labels.copy.as_ref()).clicked() {
                        let text = format_value(current, decimals, "");
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(shared_clip_id(), text.clone()));
                        ui.ctx().copy_text(text);
                        ui.close();
                    }
                    let clip: Option<String> = ui.ctx().data(|d| d.get_temp(shared_clip_id()));
                    let pasted = menu_paste(clip.as_deref(), unit, hard.0, hard.1);
                    if ui
                        .add_enabled(pasted.is_some(), egui::Button::new(labels.paste.as_ref()))
                        .clicked()
                    {
                        picked = pasted;
                        ui.close();
                    }
                });
                if let Some(v) = picked {
                    new = v;
                }
            }
            // No menu: right-click copies the bare number (no unit) so it
            // pastes into other fields.
            None => {
                if response.secondary_clicked() {
                    let text = format_value(new, self.decimals, "");
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(shared_clip_id(), text.clone()));
                    ui.ctx().copy_text(text);
                }
            }
        }

        if new != old {
            write_back(self.value, new, &mut response);
            new = self.value.to_f64();
        }
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let bg = if response.hovered() || response.dragged() {
                theme::hover()
            } else {
                theme::raised()
            };
            painter.rect_filled(rect, 3.0, bg);
            let fill = if self.logarithmic {
                log_fill_fraction(new, drag_min, drag_max)
                    .or_else(|| fill_fraction(new, drag_min, drag_max))
            } else {
                fill_fraction(new, drag_min, drag_max)
            };
            if let Some(f) = fill {
                let mut fill = rect;
                fill.set_width(rect.width() * f);
                painter.rect_filled(fill, 3.0, theme::accent().gamma_multiply(0.25));
            }
            let font = numeric_font(ui.style().text_styles[&egui::TextStyle::Monospace].size);
            let number = format_value(new, self.decimals, "");
            let galley_w = painter
                .text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &number,
                    font.clone(),
                    theme::text(),
                )
                .width();
            if !self.unit.is_empty() {
                painter.text(
                    rect.center() + Vec2::new(galley_w / 2.0 + 3.0, 0.0),
                    Align2::LEFT_CENTER,
                    self.unit,
                    font,
                    theme::muted(),
                );
            }
        }
        response
    }
}

/// Writes `new` into the field's value (clamped to `N`'s own range) and
/// marks the response changed.
fn write_back<N: Numeric>(value: &mut N, new: f64, response: &mut Response) {
    let new = clamp(new, N::MIN.to_f64(), N::MAX.to_f64());
    *value = N::from_f64(new);
    response.mark_changed();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_bounds_and_nan() {
        assert_eq!(clamp(5.0, 0.0, 1.0), 1.0);
        assert_eq!(clamp(-5.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp(0.5, 0.0, 1.0), 0.5);
        assert_eq!(clamp(f64::NAN, 0.0, 1.0), 0.0);
        assert_eq!(clamp(f64::NAN, 2.0, 3.0), 2.0);
        assert_eq!(clamp(7.0, f64::NEG_INFINITY, f64::INFINITY), 7.0);
    }

    #[test]
    fn snap_from_range_origin() {
        assert_eq!(snap_clamp(0.6, 0.5, 0.25, 1.0), 0.75);
        assert_eq!(snap_clamp(0.3, 0.5, 0.25, 1.0), 0.25);
        // The clamp wins over the snap at the top.
        assert_eq!(snap_clamp(0.99, 0.5, 0.25, 1.0), 0.75);
        assert_eq!(snap_clamp(1.4, 0.5, 0.25, 1.0), 1.0);
        // step 0 = no snapping.
        assert_eq!(snap_clamp(0.333, 0.0, 0.0, 1.0), 0.333);
        // Unbounded: the snap origin is 0.
        assert_eq!(snap_clamp(1.26, 0.5, f64::NEG_INFINITY, f64::INFINITY), 1.5);
    }

    #[test]
    fn step_math() {
        assert_eq!(step_value(10.0, 1, 1.0, 0.0, 100.0), 11.0);
        assert_eq!(step_value(10.0, -3, 1.0, 0.0, 100.0), 7.0);
        assert_eq!(step_value(99.0, 5, 1.0, 0.0, 100.0), 100.0);
        assert_eq!(step_value(0.0, -1, 1.0, 0.0, 100.0), 0.0);
        let v = step_value(0.1, 1, 0.1, 0.0, 1.0);
        assert!((v - 0.2).abs() < 1e-12);
    }

    #[test]
    fn wheel_is_one_step_per_event() {
        assert_eq!(wheel_steps(120.0), 1);
        assert_eq!(wheel_steps(0.5), 1);
        assert_eq!(wheel_steps(-50.0), -1);
        assert_eq!(wheel_steps(0.0), 0);
    }

    #[test]
    fn slow_scrub_accumulates_across_frames() {
        // 0.005/pt with a 0.1 step: one 1pt frame never crosses a step
        // alone, but 20 frames do.
        let mut raw = 0.0;
        let mut shown = 0.0;
        for _ in 0..20 {
            raw = scrub(raw, 1.0, 0.005, 1.0);
            shown = snap_clamp(raw, 0.1, 0.0, 1.0);
        }
        assert!((shown - 0.1).abs() < 1e-12);
        assert_eq!(snap_clamp(scrub(0.0, 1.0, 0.005, 1.0), 0.1, 0.0, 1.0), 0.0);
    }

    #[test]
    fn modifiers_scale_speed() {
        assert_eq!(modifier_factor(false, false), 1.0);
        assert_eq!(modifier_factor(true, false), FINE_FACTOR);
        assert_eq!(modifier_factor(false, true), COARSE_FACTOR);
        assert_eq!(modifier_factor(true, true), FINE_FACTOR);
    }

    #[test]
    fn log_scrub_spends_equal_drag_per_decade() {
        // 0.01..=100 is four decades over a 200-point field: 50 points of
        // drag = one decade, from anywhere.
        let (min, max, w) = (0.01, 100.0, 200.0);
        let one = log_scrub(0.01, 50.0, w, min, max, 1.0);
        assert!((one - 0.1).abs() < 1e-9, "{one}");
        let two = log_scrub(1.0, 50.0, w, min, max, 1.0);
        assert!((two - 10.0).abs() < 1e-9, "{two}");
        // The full width traverses the full range; negative drag comes back.
        let full = log_scrub(0.01, 200.0, w, min, max, 1.0);
        assert!((full - 100.0).abs() < 1e-9, "{full}");
        let back = log_scrub(10.0, -50.0, w, min, max, 1.0);
        assert!((back - 1.0).abs() < 1e-9, "{back}");
        // Fine modifier = a tenth of a decade.
        let fine = log_scrub(1.0, 50.0, w, min, max, FINE_FACTOR);
        assert!((fine - 10f64.powf(0.1)).abs() < 1e-9, "{fine}");
        // A non-positive accumulator restarts from min; an unusable range
        // (min <= 0, or zero width) is a no-op, never NaN.
        assert!((log_scrub(0.0, 50.0, w, min, max, 1.0) - 0.1).abs() < 1e-9);
        assert_eq!(log_scrub(5.0, 50.0, w, 0.0, 100.0, 1.0), 5.0);
        assert_eq!(log_scrub(5.0, 50.0, 0.0, min, max, 1.0), 5.0);
    }

    #[test]
    fn log_fill_is_the_log_fraction() {
        assert_eq!(log_fill_fraction(0.01, 0.01, 100.0), Some(0.0));
        assert_eq!(log_fill_fraction(100.0, 0.01, 100.0), Some(1.0));
        let mid = log_fill_fraction(1.0, 0.01, 100.0).expect("capable");
        assert!((mid - 0.5).abs() < 1e-6, "{mid}");
        // Below min clamps to empty; an incapable range has no log fill.
        assert_eq!(log_fill_fraction(0.0001, 0.01, 100.0), Some(0.0));
        assert_eq!(log_fill_fraction(1.0, 0.0, 100.0), None);
        assert_eq!(log_fill_fraction(1.0, 0.01, f64::INFINITY), None);
        assert!(!log_capable(-1.0, 1.0) && !log_capable(2.0, 1.0) && log_capable(0.5, 1.0));
    }

    #[test]
    fn expressions_evaluate_and_reject_junk() {
        assert_eq!(eval_expr("2*512", ""), Some(1024.0));
        assert_eq!(eval_expr(" 1 + 2 * 3 ", ""), Some(7.0));
        assert_eq!(eval_expr("(1 + 2) * 3", ""), Some(9.0));
        assert_eq!(eval_expr("-(1/4)", ""), Some(-0.25));
        assert_eq!(eval_expr("--3", ""), Some(3.0));
        assert_eq!(eval_expr("10 - 2 - 3", ""), Some(5.0), "left-assoc");
        assert_eq!(eval_expr("12 / 3 / 2", ""), Some(2.0), "left-assoc");
        assert_eq!(eval_expr(".5", ""), Some(0.5));
        // The field's own unit is accepted; another unit is not.
        assert_eq!(eval_expr("64 px", "px"), Some(64.0));
        assert_eq!(eval_expr("64px", "px"), Some(64.0));
        assert_eq!(eval_expr("64 cm", "px"), None);
        for junk in [
            "", "abc", "1/0", "2*", "(1", "1)", "1..2", "1e999", "nan", "inf", "3 4",
        ] {
            assert_eq!(eval_expr(junk, ""), None, "{junk:?}");
        }
        // Deep nesting is rejected, not a stack overflow.
        let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
        assert_eq!(eval_expr(&deep, ""), None);
        let fine = format!("{}1{}", "(".repeat(8), ")".repeat(8));
        assert_eq!(eval_expr(&fine, ""), Some(1.0));
    }

    #[test]
    fn soft_bounds_intersect_the_hard_range() {
        assert_eq!(soft_bounds((0.0, 100.0), None), (0.0, 100.0));
        assert_eq!(soft_bounds((0.0, 100.0), Some((0.0, 10.0))), (0.0, 10.0));
        // A soft range wider than the hard one is cut down to it.
        assert_eq!(soft_bounds((0.0, 100.0), Some((-5.0, 500.0))), (0.0, 100.0));
        // Disjoint ranges fall back to the hard range.
        assert_eq!(soft_bounds((0.0, 1.0), Some((5.0, 9.0))), (0.0, 1.0));
    }

    /// Headless frames over one field (`f32` at 0..=1000, soft 0..=10).
    fn run(ctx: &egui::Context, value: &mut f32, events: Vec<egui::Event>) -> Response {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(400.0, 200.0),
            )),
            ..Default::default()
        };
        let mut response = None;
        let mut out = ctx.run_ui(input, |ui| {
            response = Some(
                ui.add(
                    ValueField::new(value)
                        .range(0.0..=1000.0)
                        .soft_range(0.0..=10.0)
                        .default_value(5.0)
                        .unit("px"),
                ),
            );
        });
        out.textures_delta.clear();
        response.expect("drawn")
    }

    fn click(at: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn click_types_an_expression_past_the_soft_range() {
        let ctx = egui::Context::default();
        let mut v = 1.0f32;
        let at = run(&ctx, &mut v, Vec::new()).rect.center();
        run(
            &ctx,
            &mut v,
            vec![egui::Event::PointerMoved(at), click(at, true)],
        );
        run(&ctx, &mut v, vec![click(at, false)]);
        // The text edit opens with everything selected: typing replaces.
        run(&ctx, &mut v, Vec::new());
        assert!(ctx.text_edit_focused());
        run(&ctx, &mut v, vec![egui::Event::Text("2*256 px".into())]);
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        run(&ctx, &mut v, vec![key(egui::Key::Enter, true)]);
        run(&ctx, &mut v, vec![key(egui::Key::Enter, false)]);
        // 512 is past the soft max (10): typing reaches the hard range.
        assert_eq!(v, 512.0);
        assert!(!ctx.text_edit_focused());
    }

    #[test]
    fn hover_paste_ctrl_wheel_and_backspace_reset() {
        let ctx = egui::Context::default();
        let mut v = 1.0f32;
        let at = run(&ctx, &mut v, Vec::new()).rect.center();
        run(&ctx, &mut v, vec![egui::Event::PointerMoved(at)]);
        run(&ctx, &mut v, vec![egui::Event::Paste("100/4".into())]);
        assert_eq!(v, 25.0);
        // Junk pastes change nothing.
        run(&ctx, &mut v, vec![egui::Event::Paste("hello".into())]);
        assert_eq!(v, 25.0);
        let wheel = |dy, command| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(0.0, dy),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers {
                command,
                ctrl: command,
                ..Default::default()
            },
        };
        run(&ctx, &mut v, vec![wheel(1.0, true)]);
        assert_eq!(v, 25.01, "one step (0.01) per notch");
        // A plain wheel does not step: it belongs to the scroll area.
        run(&ctx, &mut v, vec![wheel(1.0, false)]);
        assert_eq!(v, 25.01);
        let backspace = egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        run(&ctx, &mut v, vec![backspace]);
        assert_eq!(v, 5.0);
    }

    #[test]
    fn menu_paste_evaluates_and_clamps() {
        assert_eq!(menu_paste(None, "", 0.0, 1.0), None);
        assert_eq!(menu_paste(Some("0.25"), "", 0.0, 1.0), Some(0.25));
        assert_eq!(menu_paste(Some("5"), "", 0.0, 1.0), Some(1.0), "clamped");
        assert_eq!(menu_paste(Some("2*8 px"), "px", 0.0, 100.0), Some(16.0));
        assert_eq!(menu_paste(Some("junk"), "", 0.0, 1.0), None);
    }

    /// The harness state: field A's value, field B's value, and field A's
    /// rect as last drawn (the test right-clicks it).
    struct Fields {
        a: f32,
        b: f32,
        a_rect: egui::Rect,
    }

    /// Field A (0..=10, default 5) with the property menu, beside field B.
    fn menu_harness(a: f32) -> egui_kittest::Harness<'static, Fields> {
        egui_kittest::Harness::new_ui_state(
            |ui, f: &mut Fields| {
                let labels = MenuLabels {
                    reset: "Reset".into(),
                    copy: "Copy".into(),
                    paste: "Paste".into(),
                };
                f.a_rect = ui
                    .add(
                        ValueField::new(&mut f.a)
                            .range(0.0..=10.0)
                            .default_value(5.0)
                            .context_menu(labels.clone()),
                    )
                    .rect;
                ui.add(
                    ValueField::new(&mut f.b)
                        .range(0.0..=10.0)
                        .context_menu(labels),
                );
            },
            Fields {
                a,
                b: 0.0,
                a_rect: egui::Rect::NOTHING,
            },
        )
    }

    /// Right-clicks field A, then clicks menu row `row` (found by label).
    fn pick(harness: &mut egui_kittest::Harness<'_, Fields>, row: &str) {
        use egui_kittest::kittest::Queryable;
        let field = harness.state().a_rect.center();
        for pressed in [true, false] {
            harness.hover_at(field);
            harness.event(egui::Event::PointerButton {
                pos: field,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.step();
        }
        harness.run();
        harness.get_by_label(row).click();
        harness.run();
    }

    #[test]
    fn right_click_menu_resets_copies_and_pastes() {
        let mut harness = menu_harness(2.0);
        harness.run();
        pick(&mut harness, "Reset");
        assert_eq!(harness.state().a, 5.0, "Reset to Default wrote the default");
        // Copy A (5), change A, Paste restores the copied 5.
        pick(&mut harness, "Copy");
        harness.state_mut().a = 9.0;
        harness.run();
        pick(&mut harness, "Paste");
        assert_eq!(harness.state().a, 5.0, "Paste wrote the copied value");
        assert_eq!(harness.state().b, 0.0, "field B untouched");
    }

    #[test]
    fn format_and_fill() {
        assert_eq!(format_value(12.345, 2, ""), "12.35");
        assert_eq!(format_value(64.0, 0, "px"), "64\u{2009}px");
        assert_eq!(fill_fraction(0.5, 0.0, 2.0), Some(0.25));
        assert_eq!(fill_fraction(5.0, 0.0, f64::INFINITY), None);
        assert_eq!(fill_fraction(5.0, 1.0, 1.0), None);
    }

    #[test]
    fn the_field_reports_a_slider_role_with_its_value() {
        use egui_kittest::kittest::Queryable;
        struct S {
            v: f32,
        }
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, s: &mut S| {
                ui.add(ValueField::new(&mut s.v).range(0.0..=10.0));
            },
            S { v: 4.5 },
        );
        harness.run();
        // The a11y contract that is OURS to hold: the field exposes
        // the Slider ROLE (get_by_ panics on miss — the call is the
        // assertion). The numeric VALUE exposure is egui's
        // accesskit mapping, not ours to pin.
        let _ = harness.get_by_role(egui::accesskit::Role::Slider);
    }
}
