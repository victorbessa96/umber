//! `tool_row` — the icon-button strip with the animated active underline
//! (docs/specs/gui-audit-and-roadmap.md §3 Phase 0a, §2.4 "the underline
//! slide").
//!
//! Radio semantics: exactly one tool is active. Clicking the active tool
//! is a no-op, not a toggle-off, because a painting app always has a
//! current tool. When the active tool is not in this row (it belongs to
//! another strip), no underline is drawn.
//!
//! The underline's x slides to the active slot over [`theme::MOTION_SECS`]
//! via `animate_value_with_time`.

use egui::{Id, Pos2, Rect, Response, Ui, Vec2};

use crate::icons::{icon_button, IconAtlas, IconFamily, ICON_BUTTON_PAD};
use crate::theme;

/// Underline thickness, points.
pub const UNDERLINE_HEIGHT: f32 = 2.0;

/// One tool in the row.
#[derive(Debug, Clone, Copy)]
pub struct Tool<'a, T> {
    /// The tool id the caller's state holds.
    pub id: T,
    /// Lucide icon name (see [`crate::icons::TOOL_ICONS`]).
    pub icon: &'a str,
    /// Hover text (already localized by the caller).
    pub tooltip: &'a str,
}

/// The slot index of `active` in `tools`, if it is in this row.
pub fn active_index<T: PartialEq>(tools: &[Tool<'_, T>], active: &T) -> Option<usize> {
    tools.iter().position(|t| t.id == *active)
}

/// Whether slot `index` is the active one.
pub fn is_active(index: usize, active: Option<usize>) -> bool {
    active == Some(index)
}

/// Resolve a click: the new active tool and whether it changed.
/// Re-clicking the active tool keeps it (radio, no toggle-off).
pub fn resolve_click<T: PartialEq + Copy>(active: T, clicked: Option<T>) -> (T, bool) {
    match clicked {
        Some(c) if c != active => (c, true),
        _ => (active, false),
    }
}

/// The left x of slot `index` in a row starting at `left`.
pub fn slot_x(left: f32, index: usize, slot_w: f32, gap: f32) -> f32 {
    left + index as f32 * (slot_w + gap)
}

/// The icon-button strip.
pub struct ToolRow<'a, T> {
    id_salt: Id,
    tools: &'a [Tool<'a, T>],
    family: IconFamily,
}

impl<'a, T: PartialEq + Copy> ToolRow<'a, T> {
    /// A row of `tools`. `id_salt` keeps the underline animation stable.
    pub fn new(id_salt: impl std::hash::Hash, tools: &'a [Tool<'a, T>]) -> Self {
        ToolRow {
            id_salt: Id::new(id_salt),
            tools,
            family: IconFamily::Tool,
        }
    }

    /// Icon family (default: [`IconFamily::Tool`], 20px).
    pub fn family(mut self, family: IconFamily) -> Self {
        self.family = family;
        self
    }

    /// Draw the row. Updates `active` on click. The returned response
    /// is the union of the buttons, marked changed when `active` changed.
    pub fn show(self, ui: &mut Ui, atlas: &IconAtlas, active: &mut T) -> Response {
        let size = self.family.logical_px();
        let slot_w = size + 2.0 * ICON_BUTTON_PAD;
        let gap = ui.spacing().item_spacing.x;
        let current = active_index(self.tools, active);

        let mut clicked = None;
        let inner = ui.horizontal(|ui| {
            let mut row: Option<Response> = None;
            for (i, tool) in self.tools.iter().enumerate() {
                let r = icon_button(ui, atlas, tool.icon, size, is_active(i, current))
                    .on_hover_text(tool.tooltip);
                if r.clicked() {
                    clicked = Some(tool.id);
                }
                row = Some(match row {
                    Some(acc) => acc.union(r),
                    None => r,
                });
            }
            row
        });
        let mut response = inner.inner.unwrap_or(inner.response);

        let (next, changed) = resolve_click(*active, clicked);
        *active = next;
        if changed {
            response.mark_changed();
        }

        // The underline slides to the (post-click) active slot.
        if let Some(index) = active_index(self.tools, active) {
            let row_rect = response.rect;
            let target = slot_x(row_rect.left(), index, slot_w, gap);
            let x = ui.ctx().animate_value_with_time(
                self.id_salt.with("underline"),
                target,
                theme::MOTION_SECS,
            );
            let line = Rect::from_min_size(
                Pos2::new(x, row_rect.bottom() - UNDERLINE_HEIGHT),
                Vec2::new(slot_w, UNDERLINE_HEIGHT),
            );
            ui.painter().rect_filled(line, 1.0, theme::accent());
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum T {
        Paint,
        Erase,
        Smudge,
        Fill,
    }

    fn row() -> [Tool<'static, T>; 3] {
        let tool = |id, icon| Tool {
            id,
            icon,
            tooltip: icon,
        };
        [
            tool(T::Paint, "paintbrush"),
            tool(T::Erase, "eraser"),
            tool(T::Smudge, "fingerprint"),
        ]
    }

    #[test]
    fn active_index_and_flag() {
        let tools = row();
        assert_eq!(active_index(&tools, &T::Erase), Some(1));
        assert_eq!(active_index(&tools, &T::Fill), None);
        assert!(is_active(1, Some(1)));
        assert!(!is_active(0, Some(1)));
        assert!(!is_active(0, None));
    }

    #[test]
    fn click_selects_and_reclick_is_noop() {
        assert_eq!(resolve_click(T::Paint, Some(T::Erase)), (T::Erase, true));
        assert_eq!(resolve_click(T::Erase, Some(T::Erase)), (T::Erase, false));
        assert_eq!(resolve_click(T::Paint, None), (T::Paint, false));
        // Selecting into this row from another row's tool.
        assert_eq!(resolve_click(T::Fill, Some(T::Smudge)), (T::Smudge, true));
    }

    #[test]
    fn exactly_one_active_after_any_click() {
        let tools = row();
        let mut active = T::Paint;
        for click in [T::Smudge, T::Smudge, T::Erase, T::Paint] {
            active = resolve_click(active, Some(click)).0;
            let idx = active_index(&tools, &active);
            let count = (0..tools.len()).filter(|&i| is_active(i, idx)).count();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn underline_slot_positions() {
        assert_eq!(slot_x(10.0, 0, 28.0, 4.0), 10.0);
        assert_eq!(slot_x(10.0, 2, 28.0, 4.0), 74.0);
    }
}
