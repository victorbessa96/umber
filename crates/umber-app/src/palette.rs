//! The command palette (GUI plan G3 `g3-command-palette`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: photocraft #2 (a
//! subsequence scorer: +8 contiguous, +5 word start, a length penalty,
//! +1000 for an exact name; matches the translated AND the English label;
//! recents on an empty query; request focus only when missing, for IME),
//! Blender A8.1-A8.3 (F3 search shows each result's menu path and
//! shortcut; most recent first)).
//!
//! Ctrl+Shift+P (or F3) opens an overlay over the action registry. Type to
//! filter, use the arrows to choose, Enter runs the action, Esc closes.
//! Each row shows the label, its menu path (so the palette teaches where
//! the command lives) and the live shortcut from the same table that
//! fires it ([`crate::actions::shortcut_label`]).
//!
//! Pure core: [`CATALOG`] (every palette-visible action with its i18n
//! keys), [`score`], [`search`] and [`Recents`]. The overlay
//! ([`Palette::show`]) only draws and collects keys.

use crate::toolbar::{ShellAction, ToolKind};

/// One palette entry: an action, its label key and its menu-path key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub action: ShellAction,
    /// The label (i18n key).
    pub label: &'static str,
    /// Where it lives in the menus (i18n key), shown dimmed.
    pub path: &'static str,
}

const fn e(action: ShellAction, label: &'static str, path: &'static str) -> Entry {
    Entry {
        action,
        label,
        path,
    }
}

/// Every action the palette offers. Labels reuse the menu / shell keys,
/// so the palette and the menus never name an action two ways.
pub const CATALOG: &[Entry] = &[
    e(ShellAction::NewProject, "shell.new", "menu.file"),
    e(ShellAction::OpenMesh, "menu.open-mesh", "menu.file"),
    e(ShellAction::OpenProject, "menu.open-project", "menu.file"),
    e(ShellAction::SaveProject, "menu.save-project", "menu.file"),
    e(ShellAction::SaveProjectAs, "button.save-as", "menu.file"),
    e(ShellAction::Undo, "shell.undo", "menu.edit"),
    e(ShellAction::Redo, "shell.redo", "menu.edit"),
    e(ShellAction::Preferences, "prefs.title", "menu.edit"),
    e(ShellAction::ToggleWireframe, "shell.wireframe", "menu.view"),
    e(ShellAction::ToggleGrid, "shell.grid", "menu.view"),
    e(
        ShellAction::CycleViewMode,
        "shell.tip-view-cycle",
        "menu.view",
    ),
    e(
        ShellAction::ToggleMaskMode,
        "shell.mask",
        "palette.path-mask",
    ),
    e(
        ShellAction::InvertMask,
        "shell.invert-mask",
        "palette.path-mask",
    ),
    e(
        ShellAction::SelectTool(ToolKind::Paint),
        "brush.tool-paint",
        "palette.path-tools",
    ),
    e(
        ShellAction::SelectTool(ToolKind::Smudge),
        "brush.tool-smudge",
        "palette.path-tools",
    ),
    e(
        ShellAction::SelectTool(ToolKind::Clone),
        "brush.tool-clone",
        "palette.path-tools",
    ),
    e(
        ShellAction::SelectTool(ToolKind::Erase),
        "brush.tool-eraser",
        "palette.path-tools",
    ),
    e(
        ShellAction::SelectTool(ToolKind::Pick),
        "brush.tool-pick",
        "palette.path-tools",
    ),
    e(
        ShellAction::SelectTool(ToolKind::IdMask),
        "brush.tool-id-mask",
        "palette.path-tools",
    ),
    e(
        ShellAction::BrushSmaller,
        "palette.brush-smaller",
        "palette.path-brush",
    ),
    e(
        ShellAction::BrushLarger,
        "palette.brush-larger",
        "palette.path-brush",
    ),
    e(ShellAction::ToggleHideUi, "shell.hide-ui", "menu.window"),
    e(ShellAction::ToggleMaximize, "shell.maximize", "menu.window"),
    e(ShellAction::NextWorkspace, "workspace.next", "menu.window"),
    e(ShellAction::PrevWorkspace, "workspace.prev", "menu.window"),
    e(ShellAction::ViewMaterial, "viewkeys.material", "menu.view"),
    e(
        ShellAction::NextChannel,
        "viewkeys.next-channel",
        "menu.view",
    ),
    e(
        ShellAction::PrevChannel,
        "viewkeys.prev-channel",
        "menu.view",
    ),
    e(ShellAction::NextMeshMap, "viewkeys.next-map", "menu.view"),
    e(ShellAction::PrevMeshMap, "viewkeys.prev-map", "menu.view"),
];

/// Most results shown at once.
pub const MAX_HITS: usize = 12;
/// Most recents remembered.
pub const MAX_RECENTS: usize = 8;

/// The fuzzy score of `query` against `text`: `None` when `query` is not a
/// case-insensitive subsequence of `text`. Higher is better.
///
/// - `+1000` when the whole text equals the query (exact name).
/// - Per matched char: `+1`; `+8` more when it directly follows the
///   previous matched char (a contiguous run); `+5` more when it starts a
///   word (the first char, or after a space / `-` / `(` / `/`).
/// - Minus one point per 4 unmatched chars of `text` (shorter wins ties).
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    if t.iter().collect::<String>() == q.iter().collect::<String>() {
        return Some(1000 + q.len() as i32);
    }
    let mut total = 0i32;
    let mut qi = 0;
    let mut prev: Option<usize> = None;
    for (ti, &c) in t.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            total += 1;
            if prev == Some(ti.wrapping_sub(1)) && ti > 0 {
                total += 8;
            }
            let word_start = ti == 0 || matches!(t[ti - 1], ' ' | '-' | '(' | '/' | '…');
            if word_start {
                total += 5;
            }
            prev = Some(ti);
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    let unmatched = (t.len() - q.len()) as i32;
    Some(total - unmatched / 4)
}

/// One scored hit: the catalog index and its score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub index: usize,
    pub score: i32,
}

/// Ranks the catalog against `query`. Each entry scores the best of its
/// translated label, its English label, and "path label" (so "view grid"
/// finds Grid under View). Ties keep catalog order. An empty query
/// returns the recents (newest first), then the rest of the catalog.
/// `text(i)` returns `(translated label, english label, translated path)`
/// for catalog index `i`.
pub fn search(
    query: &str,
    recents: &Recents,
    text: impl Fn(usize) -> (String, String, String),
) -> Vec<Hit> {
    if query.trim().is_empty() {
        let mut out: Vec<Hit> = recents
            .items
            .iter()
            .filter_map(|a| CATALOG.iter().position(|e| e.action == *a))
            .map(|index| Hit { index, score: 0 })
            .collect();
        for index in 0..CATALOG.len() {
            if !out.iter().any(|h| h.index == index) {
                out.push(Hit { index, score: 0 });
            }
        }
        out.truncate(MAX_HITS);
        return out;
    }
    let mut hits: Vec<Hit> = (0..CATALOG.len())
        .filter_map(|index| {
            let (label, english, path) = text(index);
            let combined = format!("{path} {label}");
            [label.as_str(), english.as_str(), combined.as_str()]
                .into_iter()
                .filter_map(|t| score(query, t))
                .max()
                .map(|score| Hit { index, score })
        })
        .collect();
    // Stable sort: equal scores keep catalog order.
    hits.sort_by_key(|h| std::cmp::Reverse(h.score));
    hits.truncate(MAX_HITS);
    hits
}

/// The recently run actions, newest first, deduplicated, capped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recents {
    items: Vec<ShellAction>,
}

impl Recents {
    /// Records `action` as the most recent.
    pub fn note(&mut self, action: ShellAction) {
        self.items.retain(|a| *a != action);
        self.items.insert(0, action);
        self.items.truncate(MAX_RECENTS);
    }

    /// The recents, newest first.
    #[cfg_attr(not(test), allow(dead_code))] // read by tests; the overlay uses `search`
    pub fn items(&self) -> &[ShellAction] {
        &self.items
    }
}

/// The overlay's state.
#[derive(Debug, Default)]
pub struct Palette {
    open: bool,
    query: String,
    selected: usize,
    recents: Recents,
}

impl Palette {
    /// Opens the overlay with an empty query.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    /// Whether the overlay is on screen.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Replaces the query (tests drive the filter without a focused text
    /// field; a live user's typing resets the selection the same way
    /// through `edit.changed()`).
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.selected = 0;
    }

    /// The recent picks, newest first (what an empty query lists first).
    pub fn recents(&self) -> &[ShellAction] {
        self.recents.items()
    }

    /// Draws the overlay and returns the action chosen this frame (the
    /// caller dispatches it). `label(key)` translates; `english(key)`
    /// returns the en-US text; `shortcut(action)` the live binding label.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        label: impl Fn(&str) -> String,
        english: impl Fn(&str) -> String,
        shortcut: impl Fn(ShellAction) -> Option<String>,
        placeholder: &str,
    ) -> Option<ShellAction> {
        if !self.open {
            return None;
        }
        let hits = search(&self.query, &self.recents, |i| {
            let e = CATALOG[i];
            (label(e.label), english(e.label), label(e.path))
        });
        if self.selected >= hits.len() {
            self.selected = hits.len().saturating_sub(1);
        }
        // Keys first, consumed so nothing behind the overlay reacts.
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if esc {
            self.open = false;
            return None;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        if down && self.selected + 1 < hits.len() {
            self.selected += 1;
        }
        let mut chosen = None;
        if enter {
            chosen = hits.get(self.selected).map(|h| CATALOG[h.index].action);
        }
        let screen = ctx.content_rect();
        let mut close = false;
        // Scrim: a click outside the card closes.
        egui::Area::new(egui::Id::new("umber-palette-scrim"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let r = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(90));
                if r.clicked() {
                    close = true;
                }
            });
        egui::Area::new(egui::Id::new("umber-palette"))
            .order(egui::Order::Tooltip)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(screen.center().x, screen.top() + 110.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(460.0);
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(placeholder)
                            .desired_width(f32::INFINITY),
                    );
                    // Only when missing: requesting every frame breaks IME.
                    if !edit.has_focus() {
                        edit.request_focus();
                    }
                    if edit.changed() {
                        self.selected = 0;
                    }
                    ui.separator();
                    for (row, hit) in hits.iter().enumerate() {
                        let e = CATALOG[hit.index];
                        let text = format!("{}    {}", label(e.label), label(e.path));
                        let mut button = egui::Button::selectable(row == self.selected, text);
                        if let Some(s) = shortcut(e.action) {
                            button = button.shortcut_text(s);
                        }
                        if ui.add(button).clicked() {
                            chosen = Some(e.action);
                        }
                    }
                });
            });
        if close {
            self.open = false;
        }
        if let Some(action) = chosen {
            self.recents.note(action);
            self.open = false;
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_required_and_order_matters() {
        assert!(score("grd", "Grid").is_some());
        assert_eq!(score("dgr", "Grid"), None, "out of order");
        assert_eq!(score("xyz", "Grid"), None);
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn exact_contiguous_and_word_starts_rank_in_that_order() {
        let exact = score("grid", "Grid").unwrap();
        let prefix = score("grid", "Grid overlay").unwrap();
        let scattered = score("grid", "Generate random id").unwrap();
        assert!(
            exact > prefix && prefix > scattered,
            "{exact} {prefix} {scattered}"
        );
        // Word starts beat mid-word matches: "op" -> "Open Project".
        let words = score("op", "Open Project").unwrap();
        let mid = score("op", "Loop").unwrap();
        assert!(words > mid, "{words} {mid}");
        // Case-insensitive.
        assert_eq!(score("GRID", "grid"), score("grid", "GRID"));
    }

    fn texts(i: usize) -> (String, String, String) {
        // English stand-ins: the label key's last segment, the path's.
        let e = CATALOG[i];
        let tail = |k: &str| k.rsplit(['.', '-']).next().unwrap_or(k).to_string();
        (tail(e.label), tail(e.label), tail(e.path))
    }

    #[test]
    fn search_ranks_and_matches_the_path_too() {
        let hits = search("grid", &Recents::default(), texts);
        assert_eq!(CATALOG[hits[0].index].action, ShellAction::ToggleGrid);
        // "view" + label: the path prefix matches entries under View.
        let hits = search("view grid", &Recents::default(), texts);
        assert_eq!(CATALOG[hits[0].index].action, ShellAction::ToggleGrid);
        assert!(hits.len() <= MAX_HITS);
    }

    #[test]
    fn empty_query_lists_recents_first_then_the_catalog() {
        let mut r = Recents::default();
        r.note(ShellAction::ToggleGrid);
        r.note(ShellAction::Undo);
        let hits = search("", &r, texts);
        assert_eq!(
            CATALOG[hits[0].index].action,
            ShellAction::Undo,
            "newest first"
        );
        assert_eq!(CATALOG[hits[1].index].action, ShellAction::ToggleGrid);
        let mut seen: Vec<usize> = hits.iter().map(|h| h.index).collect();
        seen.dedup();
        assert_eq!(seen.len(), hits.len(), "no duplicates");
    }

    #[test]
    fn recents_dedup_cap_and_order() {
        let mut r = Recents::default();
        for _ in 0..3 {
            r.note(ShellAction::Undo);
        }
        assert_eq!(r.items(), &[ShellAction::Undo]);
        for kind in [
            ToolKind::Paint,
            ToolKind::Smudge,
            ToolKind::Clone,
            ToolKind::Erase,
        ] {
            r.note(ShellAction::SelectTool(kind));
        }
        r.note(ShellAction::Redo);
        r.note(ShellAction::ToggleGrid);
        r.note(ShellAction::ToggleWireframe);
        r.note(ShellAction::CycleViewMode);
        assert_eq!(r.items().len(), MAX_RECENTS);
        assert_eq!(r.items()[0], ShellAction::CycleViewMode);
    }

    #[test]
    fn every_catalog_key_is_in_both_catalogs_and_every_action_is_listed_once() {
        // Through the real Fluent bundles (`I18n::has_key`), never a
        // hand-rolled .ftl split: a Windows checkout's CRLF line endings
        // broke the string-split version on CI (979a523).
        let i18n = crate::i18n::I18n::builtin();
        for e in CATALOG {
            for key in [e.label, e.path] {
                for locale in ["en-US", "pt-BR"] {
                    assert!(i18n.has_key(locale, key), "{locale} missing {key}");
                }
            }
        }
        for (i, a) in CATALOG.iter().enumerate() {
            assert!(
                !CATALOG[i + 1..].iter().any(|b| b.action == a.action),
                "{:?} twice",
                a.action
            );
        }
    }
}
