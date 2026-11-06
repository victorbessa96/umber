//! The icon rail a folded dock column becomes (UI/UX U7, photocraft's
//! rail pattern): a narrow vertical strip of panel icons with an expand
//! button at the top. Click an icon to open that panel as a flyout over
//! the canvas; click it again (or the expand button) to close the flyout
//! or unfold the column.
//!
//! The split follows `context_rail.rs`: [`IconRail`] only draws and
//! reports what was clicked in an [`IconRailOutcome`]; the shell owns the
//! dock and does the work after the frame. The widget holds no
//! user-facing strings: every tooltip arrives already localized.

use egui::{Align, Layout, Ui, Vec2};

use crate::icons::{icon_button, IconAtlas};
use crate::theme;

/// Icon size on the rail, points (the panel family's raster size).
pub const RAIL_ICON: f32 = 18.0;

/// The rail's width: one icon button plus its margins.
pub const RAIL_WIDTH: f32 = RAIL_ICON + 2.0 * crate::icons::ICON_BUTTON_PAD + 8.0;

/// One icon on the rail.
#[derive(Debug, Clone)]
pub struct RailItem<'a> {
    /// The atlas glyph name (a placeholder box when the atlas lacks it).
    pub icon: &'a str,
    /// The localized panel title, shown as the tooltip.
    pub tooltip: &'a str,
}

/// What the user did on the rail this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IconRailOutcome {
    /// The expand button: unfold the column.
    pub expand: bool,
    /// An icon click, by item index: open (or close) that flyout.
    pub picked: Option<usize>,
}

/// A folded column's rail.
pub struct IconRail<'a> {
    items: &'a [RailItem<'a>],
    /// The item whose flyout is open, drawn active.
    open: Option<usize>,
    /// The expand button's glyph and localized tooltip.
    expand_icon: &'a str,
    expand_tooltip: &'a str,
}

impl<'a> IconRail<'a> {
    /// A rail of `items` under an expand button drawing `expand_icon`
    /// with `expand_tooltip` (already localized).
    pub fn new(items: &'a [RailItem<'a>], expand_icon: &'a str, expand_tooltip: &'a str) -> Self {
        Self {
            items,
            open: None,
            expand_icon,
            expand_tooltip,
        }
    }

    /// Marks the item whose flyout is showing.
    pub fn open(mut self, open: Option<usize>) -> Self {
        self.open = open;
        self
    }

    /// Draws the rail top-down in `ui` and reports the clicks.
    pub fn ui(self, ui: &mut Ui, atlas: &IconAtlas) -> IconRailOutcome {
        let mut out = IconRailOutcome::default();
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(2.0);
            ui.add_space(4.0);
            if icon_button(ui, atlas, self.expand_icon, RAIL_ICON, false)
                .on_hover_text(self.expand_tooltip)
                .clicked()
            {
                out.expand = true;
            }
            ui.add_space(2.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(RAIL_ICON, 1.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, theme::faint());
            ui.add_space(2.0);
            for (i, item) in self.items.iter().enumerate() {
                if icon_button(ui, atlas, item.icon, RAIL_ICON, self.open == Some(i))
                    .on_hover_text(item.tooltip)
                    .clicked()
                {
                    out.picked = Some(i);
                }
            }
        });
        out
    }
}

/// The flyout toggle rule: a click on the open item closes it, a click on
/// another item opens that one.
pub fn toggle_flyout(open: Option<usize>, picked: usize) -> Option<usize> {
    if open == Some(picked) {
        None
    } else {
        Some(picked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    #[test]
    fn toggle_opens_switches_and_closes() {
        assert_eq!(toggle_flyout(None, 2), Some(2));
        assert_eq!(toggle_flyout(Some(2), 1), Some(1));
        assert_eq!(toggle_flyout(Some(1), 1), None);
    }

    #[test]
    fn clicks_report_expand_and_the_picked_item() {
        let atlas = IconAtlas::empty();
        let items = [
            RailItem {
                icon: "layers",
                tooltip: "Layers",
            },
            RailItem {
                icon: "eye",
                tooltip: "History",
            },
        ];
        let mut last = IconRailOutcome::default();
        let mut harness = Harness::new_ui(|ui| {
            let o = IconRail::new(&items, "panel-left", "Expand")
                .open(Some(0))
                .ui(ui, &atlas);
            if o != IconRailOutcome::default() {
                last = o;
            }
        });
        harness.run();
        harness.get_by_label("eye").click();
        harness.run();
        drop(harness);
        assert_eq!(last.picked, Some(1));
        assert!(!last.expand);

        let mut last = IconRailOutcome::default();
        let mut harness = Harness::new_ui(|ui| {
            let o = IconRail::new(&items, "panel-left", "Expand").ui(ui, &atlas);
            if o != IconRailOutcome::default() {
                last = o;
            }
        });
        harness.run();
        harness.get_by_label("panel-left").click();
        harness.run();
        drop(harness);
        assert!(last.expand);
        assert_eq!(last.picked, None);
    }

    #[test]
    fn rail_fits_its_declared_width() {
        let atlas = IconAtlas::empty();
        let items = [RailItem {
            icon: "layers",
            tooltip: "Layers",
        }];
        let mut used = 0.0;
        let mut harness = Harness::new_ui(|ui| {
            ui.set_width(RAIL_WIDTH);
            IconRail::new(&items, "panel-left", "Expand").ui(ui, &atlas);
            used = ui.min_rect().width();
        });
        harness.run();
        drop(harness);
        assert!(used <= RAIL_WIDTH + 0.5, "{used} > {RAIL_WIDTH}");
    }
}
