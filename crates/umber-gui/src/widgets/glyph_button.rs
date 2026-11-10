//! Glyph-only small buttons with a mandatory tooltip (round-3 audit F8).
//!
//! Every glyph button in the app goes through [`glyph_button`]: the
//! tooltip is a required argument and doubles as the button's AccessKit
//! label, so a tooltip-less glyph button cannot drift in again.

use egui::{Response, Ui, WidgetInfo, WidgetType};

/// A small glyph button whose tooltip is mandatory and is also the
/// button's accessible name (`WidgetInfo::labeled`, same pattern as
/// `tool_row`'s row widget).
pub fn glyph_button(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    let r = ui.small_button(glyph).on_hover_text(tooltip);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tooltip));
    r
}
