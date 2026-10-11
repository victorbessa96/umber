//! `param_group` — the labeled, collapsible parameter section
//! (Blender-style group header; docs/specs/gui-audit-and-roadmap.md §3
//! Phase 0a).
//!
//! The header is a full-width raised strip with a chevron, the title,
//! and a muted child count on the right. The body animates its height
//! through egui's `CollapsingState`, which follows `style.animation_time`
//! (the house 120ms, set by [`crate::theme::umber_style`]).
//!
//! Counting: the body adds parameters through [`ParamGroupUi::row`], and
//! each row increments the count. A collapsed body does not run, so the
//! header shows the count from the last open frame, kept in egui temp
//! memory. A group never opened shows no count.

use egui::collapsing_header::CollapsingState;
use egui::{Align2, Id, Pos2, Sense, Shape, Stroke, TextStyle, Ui, Vec2};

use crate::theme;

/// The header strip's height above the interact height.
const HEADER_EXTRA: f32 = 2.0;

/// The body handle: wraps the body `Ui` and counts rows.
pub struct ParamGroupUi<'u> {
    /// The body ui, for anything that is not a counted row.
    pub ui: &'u mut Ui,
    count: usize,
}

impl ParamGroupUi<'_> {
    /// One labeled parameter row: the muted label, then `add` (the
    /// value widget). Counts toward the header's child count.
    pub fn row<R>(&mut self, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        self.count += 1;
        self.ui
            .horizontal(|ui| {
                ui.label(egui::RichText::new(label).color(theme::muted()));
                add(ui)
            })
            .inner
    }

    /// Rows added so far this frame.
    pub fn count(&self) -> usize {
        self.count
    }
}

/// The header's count text: empty when unknown.
pub fn count_label(count: Option<usize>) -> String {
    count.map(|n| n.to_string()).unwrap_or_default()
}

/// The chevron's rotation in radians for an openness in 0..=1:
/// 0 (pointing right) when closed, a quarter turn (pointing down) when open.
pub fn chevron_angle(openness: f32) -> f32 {
    openness.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2
}

/// A collapsible parameter section.
pub struct ParamGroup<'a> {
    id: Id,
    title: &'a str,
    default_open: bool,
}

impl<'a> ParamGroup<'a> {
    /// A group titled `title` (already localized). `id_salt` must be
    /// unique within the panel.
    pub fn new(id_salt: impl core::hash::Hash + core::fmt::Debug, title: &'a str) -> Self {
        ParamGroup {
            id: Id::new(id_salt),
            title,
            default_open: true,
        }
    }

    /// Whether the group starts open (default: open).
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Draw the header and, if open, the body. Returns the body's value
    /// when it ran.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_body: impl FnOnce(&mut ParamGroupUi<'_>) -> R,
    ) -> Option<R> {
        let id = ui.make_persistent_id(self.id);
        let count_id = id.with("count");
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, self.default_open);

        // Header strip.
        let height = ui.spacing().interact_size.y + HEADER_EXTRA;
        let (rect, header) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        if header.clicked() {
            state.toggle(ui);
        }
        let openness = state.openness(ui.ctx());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let bg = if header.hovered() {
                theme::hover()
            } else {
                theme::raised()
            };
            painter.rect_filled(rect, 3.0, bg);

            // Chevron: a small triangle rotated by openness.
            let c = Pos2::new(rect.left() + 10.0, rect.center().y);
            let r = 3.5;
            let (s, co) = chevron_angle(openness).sin_cos();
            let rot = |v: Vec2| c + Vec2::new(v.x * co - v.y * s, v.x * s + v.y * co);
            let tri = vec![
                rot(Vec2::new(r, 0.0)),
                rot(Vec2::new(-r * 0.6, -r)),
                rot(Vec2::new(-r * 0.6, r)),
            ];
            painter.add(Shape::convex_polygon(tri, theme::muted(), Stroke::NONE));

            let font = ui.style().text_styles[&TextStyle::Body].clone();
            painter.text(
                Pos2::new(rect.left() + 20.0, rect.center().y),
                Align2::LEFT_CENTER,
                self.title,
                font,
                theme::text(),
            );
            let count: Option<usize> = ui.data(|d| d.get_temp(count_id));
            let small = ui.style().text_styles[&TextStyle::Small].clone();
            painter.text(
                Pos2::new(rect.right() - 8.0, rect.center().y),
                Align2::RIGHT_CENTER,
                count_label(count),
                small,
                theme::faint(),
            );
        }
        state.store(ui.ctx());

        state
            .show_body_unindented(ui, |ui| {
                ui.indent(id.with("body"), |ui| {
                    let mut group = ParamGroupUi {
                        ui: &mut *ui,
                        count: 0,
                    };
                    let out = add_body(&mut group);
                    let n = group.count;
                    ui.data_mut(|d| d.insert_temp(count_id, n));
                    out
                })
                .inner
            })
            .map(|r| r.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_label_text() {
        assert_eq!(count_label(None), "");
        assert_eq!(count_label(Some(7)), "7");
    }

    #[test]
    fn chevron_quarter_turn() {
        assert_eq!(chevron_angle(0.0), 0.0);
        assert_eq!(chevron_angle(1.0), std::f32::consts::FRAC_PI_2);
        assert_eq!(chevron_angle(5.0), std::f32::consts::FRAC_PI_2);
    }
}
