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
//! Typed entry (double-click → text edit) is a v2 follow-up. The math is
//! in free functions so it is testable without an `egui::Context`.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{Align2, CursorIcon, Id, Response, Sense, Ui, Vec2, Widget};

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
        }
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
        let id: Id = response.id.with("scrub-raw");

        let old = self.value.to_f64();
        let mut new = old;

        // Drag-scrub over the persistent unsnapped accumulator.
        if response.drag_started() {
            ui.data_mut(|d| d.insert_temp(id, old));
        }
        if response.dragged() {
            let (shift, command) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
            let raw: f64 = ui.data(|d| d.get_temp(id)).unwrap_or(old);
            let factor = modifier_factor(shift, command);
            let raw = if self.logarithmic && log_capable(self.min, self.max) {
                log_scrub(
                    raw,
                    response.drag_delta().x,
                    rect.width(),
                    self.min,
                    self.max,
                    factor,
                )
            } else {
                scrub(raw, response.drag_delta().x, self.speed, factor)
            };
            ui.data_mut(|d| d.insert_temp(id, raw));
            new = snap_clamp(raw, self.step, self.min, self.max);
        }
        if response.drag_stopped() {
            ui.data_mut(|d| d.remove::<f64>(id));
        }

        // Alt+wheel steps while hovered. A plain wheel must keep scrolling
        // the panel's ScrollArea; otherwise panels full of fields stop
        // scrolling under the cursor and trackpads run the value away. The
        // scroll is consumed only when it steps.
        let alt = ui.input(|i| i.modifiers.alt);
        if alt && response.hovered() && !response.dragged() {
            let steps = wheel_steps(ui.input(|i| i.smooth_scroll_delta.y));
            if steps != 0 {
                new = step_value(new, steps, self.step, self.min, self.max);
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = Vec2::ZERO;
                    i.smooth_scroll_delta = Vec2::ZERO;
                });
            }
        }

        // Copy the bare number (no unit) so it pastes into other fields.
        if response.secondary_clicked() {
            ui.ctx().copy_text(format_value(new, self.decimals, ""));
        }

        if new != old {
            new = clamp(new, N::MIN.to_f64(), N::MAX.to_f64());
            *self.value = N::from_f64(new);
            response.mark_changed();
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
                log_fill_fraction(new, self.min, self.max)
                    .or_else(|| fill_fraction(new, self.min, self.max))
            } else {
                fill_fraction(new, self.min, self.max)
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
    fn format_and_fill() {
        assert_eq!(format_value(12.345, 2, ""), "12.35");
        assert_eq!(format_value(64.0, 0, "px"), "64\u{2009}px");
        assert_eq!(fill_fraction(0.5, 0.0, 2.0), Some(0.25));
        assert_eq!(fill_fraction(5.0, 0.0, f64::INFINITY), None);
        assert_eq!(fill_fraction(5.0, 1.0, 1.0), None);
    }
}
