//! The Properties context rail (GUI plan g2-properties-context): a
//! vertical icon rail switching Properties between Brush / Layer / Fill /
//! Mask / Texture set, a breadcrumb header ("Set ▸ Layer 3 ▸ Mask"), and
//! a Ctrl+F property search that greys non-matches and jumps to the
//! first hit (Enter cycles, Shift+Enter cycles back, Escape clears).
//!
//! The split follows `tab_overflow.rs`: the decisions are pure methods on
//! [`ContextRail`] (pinned by the tests below), and [`ContextRail::ui`]
//! only draws them and reports what happened in a [`ContextRailOutcome`],
//! so the shell dispatches after its panel closure.
//!
//! - The CALLER decides which contexts apply to the current target
//!   ([`ContextRail::set_applicable`]); a kind that does not apply is
//!   absent from the rail, never drawn disabled (the plan row's "tabs
//!   that don't apply are hidden").
//! - The caller registers the property LABELS of the active context
//!   ([`ContextRail::set_labels`]) in row order, then draws each row
//!   through [`PropertyRows::row`] by its index; the rail greys the row
//!   when the search does not match it and scrolls the current hit into
//!   view with `Response::scroll_to_me` (egui 0.36 carries no
//!   `scroll_to_id`).
//! - The widget holds no user-facing strings: tooltips, the breadcrumb
//!   segments and the search placeholder all come in through
//!   [`ContextRailCtx`], already localized.

use egui::{
    Align, Id, Key, Label, Modifiers, RichText, ScrollArea, TextEdit, Ui, WidgetInfo, WidgetType,
};

use crate::icons::{icon_button, IconAtlas, IconFamily};
use crate::theme;

/// Opacity a non-matching property row (and the breadcrumb, while a
/// search is active) is drawn at.
pub const DIM_ALPHA: f32 = 0.35;

/// What the breadcrumb puts between its segments.
pub const BREADCRUMB_SEP: &str = " ▸ ";

/// Width of the search field, points.
pub const SEARCH_WIDTH: f32 = 160.0;

/// One Properties context. The declaration order is the rail's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextKind {
    /// The active brush.
    Brush,
    /// The selected layer.
    Layer,
    /// The selected fill layer's fill.
    Fill,
    /// The selected layer's mask.
    Mask,
    /// The active texture set.
    TextureSet,
}

impl ContextKind {
    /// Every kind, in rail order.
    pub const ALL: [ContextKind; 5] = [
        ContextKind::Brush,
        ContextKind::Layer,
        ContextKind::Fill,
        ContextKind::Mask,
        ContextKind::TextureSet,
    ];

    /// Number of kinds (the length of [`ContextRailCtx::tooltips`]).
    pub const COUNT: usize = Self::ALL.len();

    /// This kind's position in [`Self::ALL`] (and in the tooltip array).
    pub fn index(self) -> usize {
        match self {
            ContextKind::Brush => 0,
            ContextKind::Layer => 1,
            ContextKind::Fill => 2,
            ContextKind::Mask => 3,
            ContextKind::TextureSet => 4,
        }
    }

    /// The lucide icon id. Brush, Fill and Mask reuse the tool family's
    /// glyphs; `layers` and `layout-grid` are NOT in the baked atlas yet
    /// (see [`Self::in_atlas`]) and draw the atlas's placeholder box
    /// until a raster pass adds them.
    pub fn icon(self) -> &'static str {
        match self {
            ContextKind::Brush => "paintbrush",
            ContextKind::Layer => "layers",
            ContextKind::Fill => "paint-bucket",
            ContextKind::Mask => "square-dashed",
            ContextKind::TextureSet => "layout-grid",
        }
    }

    /// Whether [`Self::icon`] is one of the atlas's declared ids
    /// ([`crate::icons::TOOL_ICONS`] / [`crate::icons::PANEL_ICONS`]).
    pub fn in_atlas(self) -> bool {
        let id = self.icon();
        crate::icons::TOOL_ICONS.contains(&id) || crate::icons::PANEL_ICONS.contains(&id)
    }
}

/// `kinds` deduplicated and put in rail order.
pub fn ordered_kinds(kinds: &[ContextKind]) -> Vec<ContextKind> {
    ContextKind::ALL
        .into_iter()
        .filter(|k| kinds.contains(k))
        .collect()
}

/// "Set ▸ Layer 3 ▸ Mask": the segments joined by [`BREADCRUMB_SEP`].
pub fn breadcrumb_text<S: AsRef<str>>(segments: &[S]) -> String {
    let parts: Vec<&str> = segments.iter().map(|s| s.as_ref()).collect();
    parts.join(BREADCRUMB_SEP)
}

/// Whether a search query is active: anything but whitespace.
pub fn query_active(query: &str) -> bool {
    !query.trim().is_empty()
}

/// Case-insensitive substring match of the (trimmed) `query` in `label`.
/// An inactive query matches everything.
pub fn label_matches(query: &str, label: &str) -> bool {
    let query = query.trim();
    query.is_empty() || label.to_lowercase().contains(&query.to_lowercase())
}

/// The indices of `labels` that match `query`, in row order. Empty when
/// the query is inactive: no search, no hits.
pub fn match_indices<S: AsRef<str>>(query: &str, labels: &[S]) -> Vec<usize> {
    if !query_active(query) {
        return Vec::new();
    }
    labels
        .iter()
        .enumerate()
        .filter(|(_, l)| label_matches(query, l.as_ref()))
        .map(|(i, _)| i)
        .collect()
}

/// The localized text and the atlas [`ContextRail::ui`] draws with.
pub struct ContextRailCtx<'a> {
    /// The icon atlas (the Panel family is used).
    pub atlas: &'a IconAtlas,
    /// One tooltip per kind, indexed by [`ContextKind::index`].
    pub tooltips: [&'a str; ContextKind::COUNT],
    /// The breadcrumb segments, outermost first.
    pub breadcrumb: &'a [String],
    /// The search field's placeholder.
    pub search_hint: &'a str,
}

/// What one [`ContextRail::ui`] frame did, for the shell to dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextRailOutcome {
    /// The rail switched to this kind (a click on another applicable
    /// kind). The rail's own `active` is already updated.
    pub switched: Option<ContextKind>,
    /// Ctrl+F was consumed this frame and the search field focused.
    pub search_focus: bool,
    /// The query text changed (typing, or Escape clearing it).
    pub query_changed: bool,
    /// The row the search jumped to this frame (first hit or a cycle).
    pub jumped_to: Option<usize>,
    /// Escape cleared the search this frame.
    pub cleared: bool,
}

/// The rail's state: the active context, which contexts apply, the
/// registered property labels and the search.
#[derive(Debug, Clone)]
pub struct ContextRail {
    id_salt: Id,
    active: ContextKind,
    applicable: Vec<ContextKind>,
    labels: Vec<String>,
    query: String,
    /// Position of the current hit inside the match list.
    hit: usize,
    /// The current hit still has to be scrolled into view.
    scroll_pending: bool,
    /// Ctrl+F asked for the search field's focus; consumed once.
    focus_pending: bool,
}

impl ContextRail {
    /// A rail with every kind applicable and `active` selected.
    /// `id_salt` keeps the search field and scroll area ids stable.
    pub fn new(id_salt: impl core::hash::Hash + core::fmt::Debug, active: ContextKind) -> Self {
        ContextRail {
            id_salt: Id::new(id_salt),
            active,
            applicable: ContextKind::ALL.to_vec(),
            labels: Vec::new(),
            query: String::new(),
            hit: 0,
            scroll_pending: false,
            focus_pending: false,
        }
    }

    /// The active context.
    pub fn active(&self) -> ContextKind {
        self.active
    }

    /// The kinds on the rail, in rail order.
    pub fn applicable(&self) -> &[ContextKind] {
        &self.applicable
    }

    /// Whether `kind` is on the rail.
    pub fn is_applicable(&self, kind: ContextKind) -> bool {
        self.applicable.contains(&kind)
    }

    /// Set which kinds apply to the current target (any order, duplicates
    /// ignored). When the active kind no longer applies, the rail falls
    /// back to the first applicable one and returns it; an empty set
    /// keeps `active` (there is nothing to switch to).
    pub fn set_applicable(&mut self, kinds: &[ContextKind]) -> Option<ContextKind> {
        self.applicable = ordered_kinds(kinds);
        match self.applicable.first() {
            Some(&first) if !self.is_applicable(self.active) => {
                self.active = first;
                Some(first)
            }
            _ => None,
        }
    }

    /// Switch to `kind`. True when it changed; a hidden kind or the
    /// already-active one is a no-op (radio, no toggle-off).
    pub fn select(&mut self, kind: ContextKind) -> bool {
        if kind == self.active || !self.is_applicable(kind) {
            return false;
        }
        self.active = kind;
        true
    }

    /// Register the active context's property labels in row order.
    /// Re-registering the same labels every frame keeps the current hit;
    /// a different list (a context switch) restarts at its first hit.
    pub fn set_labels<I, S>(&mut self, labels: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        if labels != self.labels {
            self.labels = labels;
            self.restart_search();
        }
    }

    /// The registered labels.
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// The search text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replace the search text; a change restarts at the first hit.
    pub fn set_query(&mut self, query: &str) {
        if query != self.query {
            self.query = query.to_owned();
            self.restart_search();
        }
    }

    /// Escape: drop the search (no greying, no hit).
    pub fn clear_search(&mut self) {
        self.query.clear();
        self.hit = 0;
        self.scroll_pending = false;
    }

    /// Whether a search is active.
    pub fn search_active(&self) -> bool {
        query_active(&self.query)
    }

    /// The matching row indices, in row order.
    pub fn matches(&self) -> Vec<usize> {
        match_indices(&self.query, &self.labels)
    }

    /// The row of the current hit, if the search has any.
    pub fn current_hit(&self) -> Option<usize> {
        let matches = self.matches();
        if matches.is_empty() {
            return None;
        }
        Some(matches[self.hit % matches.len()])
    }

    /// Enter: advance to the next hit (wrapping) and scroll to it.
    pub fn next_hit(&mut self) -> Option<usize> {
        self.cycle(true)
    }

    /// Shift+Enter: back to the previous hit (wrapping).
    pub fn prev_hit(&mut self) -> Option<usize> {
        self.cycle(false)
    }

    /// Whether row `index` is greyed: a search is active and it does not
    /// match. An unregistered index is never greyed.
    pub fn row_dimmed(&self, index: usize) -> bool {
        self.search_active()
            && self
                .labels
                .get(index)
                .is_some_and(|l| !label_matches(&self.query, l))
    }

    /// Ask for the search field's focus (what Ctrl+F does).
    pub fn request_search_focus(&mut self) {
        self.focus_pending = true;
    }

    /// Take the pending focus request: true exactly once per request.
    pub fn take_search_focus(&mut self) -> bool {
        std::mem::take(&mut self.focus_pending)
    }

    /// Take the row still owed a scroll-into-view: the current hit, once.
    pub fn take_scroll_target(&mut self) -> Option<usize> {
        if !std::mem::take(&mut self.scroll_pending) {
            return None;
        }
        self.current_hit()
    }

    fn restart_search(&mut self) {
        self.hit = 0;
        self.scroll_pending = self.search_active();
    }

    fn cycle(&mut self, forward: bool) -> Option<usize> {
        let n = self.matches().len();
        if n == 0 {
            return None;
        }
        let at = self.hit % n;
        self.hit = if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        };
        self.scroll_pending = true;
        self.current_hit()
    }

    /// Draw the rail, the header (breadcrumb + search) and, beside the
    /// rail in a scroll area, the caller's `body`, which draws its
    /// property rows through the [`PropertyRows`] it is handed.
    ///
    /// Keys: Ctrl+F (consumed here, so call this only while Properties
    /// is shown) focuses the search; while it is focused, Enter / Shift+Enter
    /// cycle the hits and Escape clears it.
    pub fn ui<R>(
        &mut self,
        ui: &mut Ui,
        ctx: &ContextRailCtx<'_>,
        body: impl FnOnce(&mut Ui, &PropertyRows<'_>) -> R,
    ) -> (ContextRailOutcome, R) {
        let mut outcome = ContextRailOutcome::default();
        if ui
            .ctx()
            .input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F))
        {
            self.request_search_focus();
            outcome.search_focus = true;
        }

        let inner = ui.horizontal_top(|ui| {
            outcome.switched = self.rail_ui(ui, ctx);
            ui.separator();
            ui.vertical(|ui| {
                self.header_ui(ui, ctx, &mut outcome);
                outcome.jumped_to = self.take_scroll_target();
                let rows = PropertyRows {
                    rail: &*self,
                    scroll_to: outcome.jumped_to,
                };
                ScrollArea::vertical()
                    .id_salt(self.id_salt.with("body"))
                    .auto_shrink([false, false])
                    .show(ui, |ui| body(ui, &rows))
                    .inner
            })
            .inner
        });
        (outcome, inner.inner)
    }

    /// The vertical icon column. Returns the kind a click switched to.
    fn rail_ui(&mut self, ui: &mut Ui, ctx: &ContextRailCtx<'_>) -> Option<ContextKind> {
        let size = IconFamily::Panel.logical_px();
        let kinds = self.applicable.clone();
        let mut switched = None;
        ui.vertical(|ui| {
            for kind in kinds {
                let active = kind == self.active;
                let tooltip = ctx.tooltips[kind.index()];
                let r =
                    icon_button(ui, ctx.atlas, kind.icon(), size, active).on_hover_text(tooltip);
                r.widget_info(|| {
                    WidgetInfo::selected(WidgetType::SelectableLabel, true, active, tooltip)
                });
                if r.clicked() && self.select(kind) {
                    switched = Some(kind);
                }
            }
        });
        switched
    }

    /// The breadcrumb row and the search field.
    fn header_ui(&mut self, ui: &mut Ui, ctx: &ContextRailCtx<'_>, out: &mut ContextRailOutcome) {
        let searching = self.search_active();
        ui.scope(|ui| {
            if searching {
                ui.multiply_opacity(DIM_ALPHA);
            }
            let crumb = RichText::new(breadcrumb_text(ctx.breadcrumb)).color(theme::muted());
            ui.add(Label::new(crumb).truncate());
        });

        let edit = TextEdit::singleline(&mut self.query)
            .id(self.id_salt.with("search"))
            .hint_text(ctx.search_hint.to_owned())
            .desired_width(SEARCH_WIDTH)
            .show(ui);
        let response = edit.response.response;
        if response.changed() {
            self.restart_search();
            out.query_changed = true;
        }
        if response.lost_focus() {
            let (enter, escape, shift) = ui.input(|i| {
                (
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Escape),
                    i.modifiers.shift,
                )
            });
            if escape {
                if !self.query.is_empty() {
                    out.query_changed = true;
                }
                self.clear_search();
                out.cleared = true;
            } else if enter {
                // A single-line TextEdit surrenders focus on Enter; take
                // it back so the next Enter cycles again.
                if shift {
                    self.prev_hit();
                } else {
                    self.next_hit();
                }
                response.request_focus();
            }
        }
        if self.take_search_focus() {
            response.request_focus();
        }
    }
}

/// The row painter [`ContextRail::ui`] hands its body: greys rows the
/// search does not match and scrolls the current hit into view.
pub struct PropertyRows<'s> {
    rail: &'s ContextRail,
    scroll_to: Option<usize>,
}

impl PropertyRows<'_> {
    /// Draw row `index` (its position in [`ContextRail::set_labels`])
    /// with `add`: at [`DIM_ALPHA`] when the search does not match it,
    /// scrolled to the centre when it is the hit jumped to this frame.
    pub fn row<R>(&self, ui: &mut Ui, index: usize, add: impl FnOnce(&mut Ui) -> R) -> R {
        let dimmed = self.rail.row_dimmed(index);
        let inner = ui.scope(|ui| {
            if dimmed {
                ui.multiply_opacity(DIM_ALPHA);
            }
            add(ui)
        });
        if self.scroll_to == Some(index) {
            inner.response.scroll_to_me(Some(Align::Center));
        }
        inner.inner
    }

    /// Whether row `index` is greyed this frame.
    pub fn is_dimmed(&self, index: usize) -> bool {
        self.rail.row_dimmed(index)
    }

    /// The registered label of row `index`.
    pub fn label(&self, index: usize) -> Option<&str> {
        self.rail.labels.get(index).map(String::as_str)
    }

    /// The row the search scrolls to this frame, if any.
    pub fn scroll_target(&self) -> Option<usize> {
        self.scroll_to
    }
}

#[cfg(test)]
mod tests {
    use super::ContextKind::*;
    use super::*;
    use egui::{Context, Event, RawInput};

    fn brush_rail() -> ContextRail {
        let mut rail = ContextRail::new("test-rail", Brush);
        rail.set_labels(["Size", "Opacity", "Flow", "Spacing", "Opacidade da ponta"]);
        rail
    }

    #[test]
    fn kinds_index_in_rail_order_and_name_their_icons() {
        for (i, kind) in ContextKind::ALL.into_iter().enumerate() {
            assert_eq!(kind.index(), i, "{kind:?}");
            assert!(!format!("{kind:?}").contains('('), "kinds carry no payload");
        }
        // Brush / Fill / Mask reuse declared atlas ids; Layer and
        // TextureSet name lucide ids the atlas does not carry yet. When a
        // raster pass adds them this pin flips on purpose.
        for kind in [Brush, Fill, Mask] {
            assert!(kind.in_atlas(), "{kind:?} → {}", kind.icon());
        }
        for kind in [Layer, TextureSet] {
            assert!(!kind.in_atlas(), "{kind:?} → {}", kind.icon());
        }
    }

    #[test]
    fn hidden_kinds_are_absent_from_the_rail() {
        let mut rail = ContextRail::new("r", Brush);
        assert_eq!(rail.applicable(), &ContextKind::ALL);
        // Any order, duplicates: the rail keeps rail order, once each.
        assert_eq!(rail.set_applicable(&[Mask, Brush, Mask, Layer]), None);
        assert_eq!(rail.applicable(), &[Brush, Layer, Mask]);
        assert!(!rail.is_applicable(Fill));
        assert!(!rail.is_applicable(TextureSet));
        // A hidden kind cannot be selected.
        assert!(!rail.select(Fill));
        assert_eq!(rail.active(), Brush);
    }

    #[test]
    fn losing_the_active_kind_falls_back_to_the_first_applicable() {
        let mut rail = ContextRail::new("r", Fill);
        // A paint layer: no Fill tab.
        assert_eq!(rail.set_applicable(&[Mask, Layer, Brush]), Some(Brush));
        assert_eq!(rail.active(), Brush);
        // Still applicable: no switch.
        assert_eq!(rail.set_applicable(&[Layer, Brush]), None);
        // Nothing applies: keep what we had, there is nothing to show.
        assert_eq!(rail.set_applicable(&[]), None);
        assert_eq!(rail.active(), Brush);
        assert!(rail.applicable().is_empty());
    }

    #[test]
    fn select_switches_once_and_reselect_is_a_noop() {
        let mut rail = ContextRail::new("r", Brush);
        assert!(rail.select(Layer));
        assert_eq!(rail.active(), Layer);
        assert!(!rail.select(Layer), "radio, no toggle-off");
        assert_eq!(rail.active(), Layer);
    }

    #[test]
    fn search_is_a_case_insensitive_substring() {
        assert!(label_matches("OPAC", "Opacity"));
        assert!(label_matches("  pac ", "Opacity"), "the query is trimmed");
        assert!(label_matches("éCR", "Écran"), "non-ASCII case folds too");
        assert!(!label_matches("flow", "Opacity"));
        assert!(label_matches("", "anything"), "no query matches all");
        assert_eq!(
            match_indices("opa", &["Size", "Opacity", "Opacidade"]),
            vec![1, 2]
        );
        assert!(
            match_indices("   ", &["Size"]).is_empty(),
            "no search, no hits"
        );
    }

    #[test]
    fn non_matches_grey_out_only_while_searching() {
        let mut rail = brush_rail();
        assert!(
            (0..5).all(|i| !rail.row_dimmed(i)),
            "no search: nothing grey"
        );
        rail.set_query("opac");
        let dimmed: Vec<bool> = (0..5).map(|i| rail.row_dimmed(i)).collect();
        assert_eq!(dimmed, [true, false, true, true, false]);
        assert!(!rail.row_dimmed(99), "an unregistered row is never grey");
        rail.set_query("zzz");
        assert!((0..5).all(|i| rail.row_dimmed(i)), "no match: all grey");
        assert_eq!(rail.current_hit(), None);
    }

    #[test]
    fn the_first_hit_is_the_first_match_in_row_order_scrolled_once() {
        let mut rail = brush_rail();
        rail.set_query("opac");
        assert_eq!(rail.matches(), vec![1, 4]);
        assert_eq!(rail.current_hit(), Some(1));
        assert_eq!(rail.take_scroll_target(), Some(1), "the jump");
        assert_eq!(rail.take_scroll_target(), None, "only once");
        // No search: never a scroll.
        rail.clear_search();
        assert_eq!(rail.take_scroll_target(), None);
    }

    #[test]
    fn enter_cycles_matches_and_wraps_both_ways() {
        let mut rail = brush_rail();
        rail.set_query("o"); // Opacity(1), Flow(2), Opacidade da ponta(4)
        assert_eq!(rail.matches(), vec![1, 2, 4]);
        assert_eq!(rail.take_scroll_target(), Some(1));
        assert_eq!(rail.next_hit(), Some(2));
        assert_eq!(rail.take_scroll_target(), Some(2), "a cycle scrolls too");
        assert_eq!(rail.next_hit(), Some(4));
        assert_eq!(rail.next_hit(), Some(1), "wraps to the first");
        assert_eq!(rail.prev_hit(), Some(4), "Shift+Enter wraps back");
        assert_eq!(rail.prev_hit(), Some(2));
        rail.set_query("zzz");
        assert_eq!(rail.next_hit(), None, "nothing to cycle");
    }

    #[test]
    fn editing_or_relabelling_restarts_at_the_first_hit() {
        let mut rail = brush_rail();
        rail.set_query("o");
        rail.next_hit();
        assert_eq!(rail.current_hit(), Some(2));
        // The shell re-registers the same labels every frame: hit kept.
        rail.set_labels(["Size", "Opacity", "Flow", "Spacing", "Opacidade da ponta"]);
        assert_eq!(rail.current_hit(), Some(2));
        // Typing restarts.
        rail.set_query("op");
        assert_eq!(rail.current_hit(), Some(1));
        rail.next_hit();
        // A context switch registers other labels: restart + jump.
        rail.take_scroll_target();
        rail.set_labels(["Blend mode", "Layer opacity"]);
        assert_eq!(rail.current_hit(), Some(1));
        assert_eq!(rail.take_scroll_target(), Some(1));
    }

    #[test]
    fn the_breadcrumb_joins_its_segments() {
        let crumbs = vec!["Set".to_owned(), "Layer 3".to_owned(), "Mask".to_owned()];
        assert_eq!(breadcrumb_text(crumbs.as_slice()), "Set ▸ Layer 3 ▸ Mask");
        assert_eq!(breadcrumb_text(&["Set"]), "Set");
        assert_eq!(breadcrumb_text::<&str>(&[]), "");
    }

    #[test]
    fn a_focus_request_is_consumed_exactly_once() {
        let mut rail = brush_rail();
        assert!(!rail.take_search_focus());
        rail.request_search_focus();
        rail.request_search_focus(); // one pending request, not two
        assert!(rail.take_search_focus());
        assert!(!rail.take_search_focus());
    }

    // ---- headless frames ---------------------------------------------

    const TIPS: [&str; ContextKind::COUNT] = ["Brush", "Layer", "Fill", "Mask", "Texture set"];

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Run one frame of the rail with `events`; returns the outcome
    /// (OR-ed over passes, in case egui asks for a discard pass).
    fn frame(egui_ctx: &Context, rail: &mut ContextRail, events: Vec<Event>) -> ContextRailOutcome {
        let atlas = IconAtlas::empty();
        let crumbs = vec!["Set".to_owned(), "Layer 3".to_owned()];
        let ctx = ContextRailCtx {
            atlas: &atlas,
            tooltips: TIPS,
            breadcrumb: &crumbs,
            search_hint: "Search",
        };
        let input = RawInput {
            events,
            ..RawInput::default()
        };
        let mut seen = ContextRailOutcome::default();
        let mut output = egui_ctx.run_ui(input, |ui| {
            let (out, ()) = rail.ui(ui, &ctx, |ui, rows| {
                for i in 0..rows.rail.labels.len() {
                    let label = rows.label(i).unwrap_or_default().to_owned();
                    rows.row(ui, i, |ui| ui.label(label));
                }
            });
            seen.switched = seen.switched.or(out.switched);
            seen.search_focus |= out.search_focus;
            seen.query_changed |= out.query_changed;
            seen.jumped_to = seen.jumped_to.or(out.jumped_to);
            seen.cleared |= out.cleared;
        });
        // Disarm epaint's TexturesDelta drop bomb (the font atlas upload).
        output.textures_delta.clear();
        seen
    }

    #[test]
    fn ctrl_f_focuses_the_search_once_per_press() {
        let egui_ctx = Context::default();
        let mut rail = brush_rail();
        let ctrl = Modifiers::CTRL | Modifiers::COMMAND;
        let out = frame(
            &egui_ctx,
            &mut rail,
            vec![Event::ModifiersChanged(ctrl), key(Key::F, ctrl)],
        );
        assert!(out.search_focus, "Ctrl+F is consumed");
        let search = rail.id_salt.with("search");
        assert!(
            egui_ctx.memory(|m| m.has_focus(search)),
            "the field holds focus"
        );
        assert!(!rail.take_search_focus(), "the request was spent drawing");
        // The next frame has no Ctrl+F: no second focus event.
        let out = frame(&egui_ctx, &mut rail, Vec::new());
        assert!(!out.search_focus);
        // A plain F is typing, not the shortcut.
        let out = frame(
            &egui_ctx,
            &mut rail,
            vec![
                Event::ModifiersChanged(Modifiers::NONE),
                key(Key::F, Modifiers::NONE),
            ],
        );
        assert!(!out.search_focus);
    }

    #[test]
    fn typing_enter_and_escape_drive_the_search() {
        let egui_ctx = Context::default();
        let mut rail = brush_rail();
        let ctrl = Modifiers::CTRL | Modifiers::COMMAND;
        frame(
            &egui_ctx,
            &mut rail,
            vec![Event::ModifiersChanged(ctrl), key(Key::F, ctrl)],
        );
        // Typing into the focused field: the first hit is jumped to.
        let out = frame(
            &egui_ctx,
            &mut rail,
            vec![
                Event::ModifiersChanged(Modifiers::NONE),
                Event::Text("o".to_owned()),
            ],
        );
        assert_eq!(rail.query(), "o");
        assert!(out.query_changed);
        assert_eq!(out.jumped_to, Some(1));
        // Enter cycles and the field keeps focus for the next Enter.
        let out = frame(&egui_ctx, &mut rail, vec![key(Key::Enter, Modifiers::NONE)]);
        assert_eq!(out.jumped_to, Some(2));
        let search = rail.id_salt.with("search");
        assert!(egui_ctx.memory(|m| m.has_focus(search)));
        let out = frame(&egui_ctx, &mut rail, vec![key(Key::Enter, Modifiers::NONE)]);
        assert_eq!(out.jumped_to, Some(4));
        // Escape clears.
        let out = frame(
            &egui_ctx,
            &mut rail,
            vec![key(Key::Escape, Modifiers::NONE)],
        );
        assert!(out.cleared && out.query_changed);
        assert_eq!(rail.query(), "");
        assert!(!rail.search_active());
    }
}
