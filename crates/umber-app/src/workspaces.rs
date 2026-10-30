//! Workspaces (docs/specs/gui-plan-2026-10-15.md kill `g5-workspaces`,
//! B:A2.1-A2.3, P:A4): named dock layouts shown as tabs in the menu row,
//! Paint (the default), Bake, Export and Graph. Ctrl+PgDn / Ctrl+PgUp
//! cycle them.
//!
//! Each workspace starts from its own preset layout ([`preset`]) and then
//! remembers what the user does to it: switching away keeps that
//! workspace's edited layout, and switching back restores it. Window ▸
//! Reset Layout resets only the active workspace, back to its preset.
//! A workspace "pins its mode" through its preset: Bake opens with the
//! Bakes panel in front, Export with Export, Graph with the node graph in
//! the centre.

use std::collections::HashMap;

use egui_dock::{DockState, NodeIndex};

use crate::Panel;

/// A named layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Workspace {
    /// Painting: the boot layout.
    Paint,
    /// Baking: the bake settings beside the viewport.
    Bake,
    /// Exporting: the export dialog beside the viewport.
    Export,
    /// The node graph in the centre.
    Graph,
}

impl Workspace {
    /// Menu-row order (the cycle order).
    pub const ALL: [Workspace; 4] = [
        Workspace::Paint,
        Workspace::Bake,
        Workspace::Export,
        Workspace::Graph,
    ];

    /// The i18n key of the tab label.
    pub fn label_key(self) -> &'static str {
        match self {
            Workspace::Paint => "workspace.paint",
            Workspace::Bake => "workspace.bake",
            Workspace::Export => "workspace.export",
            Workspace::Graph => "workspace.graph",
        }
    }

    /// The next workspace (`forward`) or the previous one, wrapping.
    pub fn cycle(self, forward: bool) -> Workspace {
        let n = Self::ALL.len();
        let i = Self::ALL.iter().position(|w| *w == self).unwrap_or(0);
        Self::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// A workspace's starting layout. Every preset docks all eleven panels
/// (none is lost by switching); they differ in what sits in front.
pub fn preset(workspace: Workspace) -> DockState<Panel> {
    match workspace {
        Workspace::Paint => crate::shell_menu::default_dock(),
        Workspace::Bake => side_layout(
            vec![Panel::Bakes, Panel::TextureSets, Panel::Display],
            vec![
                Panel::LayerStack,
                Panel::Properties,
                Panel::Assets,
                Panel::Export,
                Panel::Graph,
                Panel::History,
            ],
        ),
        Workspace::Export => side_layout(
            vec![Panel::Export, Panel::TextureSets],
            vec![
                Panel::LayerStack,
                Panel::Properties,
                Panel::Assets,
                Panel::Bakes,
                Panel::Display,
                Panel::Graph,
                Panel::History,
            ],
        ),
        Workspace::Graph => {
            let mut dock = DockState::new(vec![Panel::Graph]);
            let tree = dock.main_surface_mut();
            let [_, right] =
                tree.split_right(NodeIndex::root(), 0.6, vec![Panel::Viewport, Panel::UvView]);
            tree.split_below(
                right,
                0.5,
                vec![
                    Panel::Properties,
                    Panel::LayerStack,
                    Panel::TextureSets,
                    Panel::Assets,
                    Panel::Bakes,
                    Panel::Export,
                    Panel::Display,
                    Panel::History,
                ],
            );
            dock
        }
    }
}

/// The viewport in the dominant centre — 3D view left, 2D UV view right,
/// the centre split of the Paint preset — with `front` in a left rail
/// (its first tab active) and `rest` tabbed on the right.
fn side_layout(front: Vec<Panel>, rest: Vec<Panel>) -> DockState<Panel> {
    // Root starts as the 2D view; the centre split below adds the 3D
    // view as its left leaf.
    let mut dock = DockState::new(vec![Panel::UvView]);
    let tree = dock.main_surface_mut();
    tree.split_left(NodeIndex::root(), 0.18, front);
    tree.split_right(NodeIndex::root(), 0.24, rest);
    tree.split_left(NodeIndex::root(), 0.65, vec![Panel::Viewport]);
    dock
}

/// The active workspace and the edited layouts of the inactive ones.
#[derive(Debug)]
pub struct Workspaces {
    active: Workspace,
    stored: HashMap<Workspace, DockState<Panel>>,
}

impl Default for Workspaces {
    fn default() -> Self {
        Workspaces {
            active: Workspace::Paint,
            stored: HashMap::new(),
        }
    }
}

impl Workspaces {
    /// The active workspace.
    pub fn active(&self) -> Workspace {
        self.active
    }

    /// Switches to `to`: `dock` (the active workspace's live layout) is
    /// stored under the current workspace, and `to`'s stored layout (or
    /// its preset, the first time) becomes the live dock. Returns whether
    /// the workspace changed. The caller leaves any focus mode first, so
    /// the stored layout is the real one.
    pub fn switch(&mut self, to: Workspace, dock: &mut DockState<Panel>) -> bool {
        if to == self.active {
            return false;
        }
        let next = self.stored.remove(&to).unwrap_or_else(|| preset(to));
        let previous = std::mem::replace(dock, next);
        self.stored.insert(self.active, previous);
        self.active = to;
        true
    }

    /// The active workspace's preset (Window ▸ Reset Layout).
    pub fn reset_active(&self) -> DockState<Panel> {
        preset(self.active)
    }

    /// The persisted form: the active workspace, plus every workspace's
    /// layout, with `live` (the dock on screen) as the active one's.
    pub fn snapshot(&self, live: &DockState<Panel>) -> SavedLayouts {
        let mut layouts: Vec<(Workspace, DockState<Panel>)> =
            self.stored.iter().map(|(w, d)| (*w, d.clone())).collect();
        layouts.push((self.active, live.clone()));
        layouts.sort_by_key(|(w, _)| Workspace::ALL.iter().position(|x| x == w));
        SavedLayouts {
            version: LAYOUTS_VERSION,
            active: self.active,
            layouts,
        }
    }

    /// Restores a [`Self::snapshot`]: returns the live dock to show and
    /// keeps the rest stored. A HIDDEN panel is a legitimate saved state
    /// (Window ▸ Panels hides panels and re-adds them), so absence is kept.
    /// A layout docking a panel TWICE, or showing no tab at all, is not
    /// one the shell can produce: a hand edit or a bug. It is dropped for
    /// its preset.
    pub fn restore(saved: SavedLayouts) -> (Self, DockState<Panel>) {
        let mut stored = HashMap::new();
        for (workspace, dock) in saved.layouts {
            if is_showable(&dock) {
                stored.insert(workspace, dock);
            }
        }
        let live = stored
            .remove(&saved.active)
            .unwrap_or_else(|| preset(saved.active));
        (
            Workspaces {
                active: saved.active,
                stored,
            },
            live,
        )
    }
}

/// The layouts file's schema version; another version is ignored (the
/// presets load instead).
pub const LAYOUTS_VERSION: u32 = 2;
// v2: the Painter-faithful default — viewport dominant in the centre
// (3D left / 2D right), rails on the edges. v1 files (views tabbed in the
// centre, rails 0.25/0.2) fall back to the presets on load.

/// The persisted workspaces (`<state>/layouts.json`).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SavedLayouts {
    /// [`LAYOUTS_VERSION`].
    pub version: u32,
    /// The workspace that was on screen.
    pub active: Workspace,
    /// Each workspace's layout, in menu order.
    pub layouts: Vec<(Workspace, DockState<Panel>)>,
}

/// Whether `dock` is a layout the shell could have produced: at least
/// one tab, and no panel docked twice. (Unknown panels cannot occur:
/// deserializing an unknown `Panel` variant fails the whole file.)
fn is_showable(dock: &DockState<Panel>) -> bool {
    let mut tabs: Vec<Panel> = dock.iter_all_tabs().map(|(_, t)| t.clone()).collect();
    let n = tabs.len();
    tabs.sort_by_key(|p| crate::shell_menu::ALL_PANELS.iter().position(|q| q == p));
    tabs.dedup();
    n > 0 && n == tabs.len()
}

/// Writes the layouts to `path` as JSON. A failure is the caller's log
/// line: layouts are a convenience, never worth an error dialog.
pub fn save_layouts(path: &std::path::Path, saved: &SavedLayouts) -> std::io::Result<()> {
    // A dock that was never laid out holds `Rect::NOTHING` (±∞ corners)
    // and serde_json writes ±∞/NaN as `null`, which nothing can read
    // back. Rects are re-laid-out on the next frame anyway, so the file
    // keeps them finite (the x/y corner nulls become 0; real
    // `Option::None` nulls, like `focused_surface`, are left alone).
    let mut value = serde_json::to_value(saved).map_err(std::io::Error::other)?;
    de_null_infinities(&mut value);
    let json = serde_json::to_string(&value).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write-then-rename, so a crash mid-write never leaves a torn file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)
}

/// Reads the layouts from `path`. `None` for a missing, unreadable,
/// malformed or other-version file (the presets load instead).
pub fn load_layouts(path: &std::path::Path) -> Option<SavedLayouts> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut value: serde_json::Value = serde_json::from_str(&text).ok()?;
    de_null_infinities(&mut value);
    let saved: SavedLayouts = serde_json::from_value(value).ok()?;
    (saved.version == LAYOUTS_VERSION).then_some(saved)
}

/// Replaces the `x`/`y` `null` corners of every `rect`/`viewport`
/// object with `0.0`: serde_json's spelling of a non-finite f32 (an
/// un-laid-out dock is `Rect::NOTHING`, ±∞). Scoped to those keys —
/// other nulls are real `Option::None`s (`focused_surface`) and must
/// survive. A 0×0 rect is inert (overwritten by the next layout pass).
fn de_null_infinities(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(map) = value {
        let is_corner = map.keys().all(|k| k == "x" || k == "y");
        if is_corner && map.values().any(|v| v.is_null()) {
            for (_, v) in map.iter_mut() {
                if v.is_null() {
                    *v = serde_json::json!(0.0);
                }
            }
            return;
        }
        for (_, item) in map.iter_mut() {
            de_null_infinities(item);
        }
    } else if let serde_json::Value::Array(items) = value {
        for item in items {
            de_null_infinities(item);
        }
    }
}

/// A viewport preset (GUI plan g5-workspaces' F1-F4 row): which of the two
/// views — the 3D [`Panel::Viewport`] and the 2D [`Panel::UvView`] — is
/// docked, and where. Applied to the active workspace's live layout, so the
/// workspaces machinery (switch-away storage, save_layouts) carries it like
/// any other layout edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportPreset {
    /// 3D + 2D both docked.
    Both,
    /// The 3D viewport alone (the 2D view undocked).
    View3d,
    /// The 2D UV view alone (the 3D viewport undocked).
    View2d,
    /// Exchange the two views' places; with only one of them docked, switch
    /// to the other.
    Swap,
}

/// Applies `preset` to `dock` in place.
pub fn apply_viewport_preset(dock: &mut DockState<Panel>, preset: ViewportPreset) {
    use Panel::{UvView, Viewport};

    fn ensure_docked(dock: &mut DockState<Panel>, panel: Panel) {
        if dock.find_tab(&panel).is_some() {
            return;
        }
        if dock.focused_leaf().is_some() {
            dock.push_to_focused_leaf(panel);
        } else {
            dock.push_to_first_leaf(panel);
        }
    }

    fn undock(dock: &mut DockState<Panel>, panel: Panel) {
        if let Some(path) = dock.find_tab(&panel) {
            dock.remove_tab(path);
        }
    }

    fn activate(dock: &mut DockState<Panel>, panel: Panel) {
        if let Some(path) = dock.find_tab(&panel) {
            let _ = dock.set_active_tab(path);
        }
    }

    match preset {
        ViewportPreset::Both => {
            ensure_docked(dock, Viewport);
            ensure_docked(dock, UvView);
        }
        ViewportPreset::View3d => {
            undock(dock, UvView);
            ensure_docked(dock, Viewport);
            activate(dock, Viewport);
        }
        ViewportPreset::View2d => {
            undock(dock, Viewport);
            ensure_docked(dock, UvView);
            activate(dock, UvView);
        }
        ViewportPreset::Swap => match (dock.find_tab(&Viewport), dock.find_tab(&UvView)) {
            (Some(a), Some(b)) => {
                if a.node_path() == b.node_path() {
                    // Same leaf: exchange the two tabs' slots and flip the
                    // active tab to whichever now sits in the other's place.
                    let leaf = dock.leaf_mut(a.node_path()).expect("tab's leaf");
                    leaf.tabs.swap(a.tab.0, b.tab.0);
                    if leaf.active == a.tab {
                        leaf.active = b.tab;
                    } else if leaf.active == b.tab {
                        leaf.active = a.tab;
                    }
                } else {
                    // Different leaves: exchange the tabs across the nodes.
                    let (na, ia) = (a.node_path(), a.tab);
                    let (nb, ib) = (b.node_path(), b.tab);
                    let vp = dock[na].remove_tab(ia).expect("tab present");
                    let uv = dock[nb].remove_tab(ib).expect("tab present");
                    dock[nb].insert_tab(ib, vp);
                    dock[na].insert_tab(ia, uv);
                }
            }
            (Some(_), None) => apply_viewport_preset(dock, ViewportPreset::View2d),
            (None, Some(_)) => apply_viewport_preset(dock, ViewportPreset::View3d),
            (None, None) => apply_viewport_preset(dock, ViewportPreset::Both),
        },
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn layouts_round_trip_through_a_file_with_the_edit_and_the_active_workspace() {
        let mut ws = Workspaces::default();
        let mut dock = preset(Workspace::Paint);
        ws.switch(Workspace::Bake, &mut dock);
        // An edit on the Bake layout: History hidden, then shown again in
        // the focused leaf (moved, still docked once).
        toggle_panel(&mut dock, &Panel::History);
        toggle_panel(&mut dock, &Panel::History);
        let edited = dock.clone();
        let path = std::env::temp_dir().join(format!("umber-layouts-{}.json", std::process::id()));
        save_layouts(&path, &ws.snapshot(&dock)).expect("saves");
        let (back, live) = Workspaces::restore(load_layouts(&path).expect("loads"));
        assert_eq!(back.active(), Workspace::Bake);
        assert_eq!(docked(&live), docked(&edited));
        assert_eq!(
            live.iter_all_tabs()
                .map(|(_, t)| t.clone())
                .collect::<Vec<_>>(),
            edited
                .iter_all_tabs()
                .map(|(_, t)| t.clone())
                .collect::<Vec<_>>(),
            "the exact edited layout"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_never_laid_out_dock_round_trips_because_rects_go_finite() {
        // The battery red: a fresh preset's nodes are Rect::NOTHING (±∞),
        // which serde_json writes as null and cannot read back. The file
        // must keep finite corners (0), whatever the in-memory rects are.
        let mut ws = Workspaces::default();
        let mut dock = preset(Workspace::Paint);
        ws.switch(Workspace::Bake, &mut dock);
        let path =
            std::env::temp_dir().join(format!("umber-layouts-finite-{}.json", std::process::id()));
        save_layouts(&path, &ws.snapshot(&dock)).expect("saves");
        let (back, _) = Workspaces::restore(load_layouts(&path).expect("loads"));
        assert_eq!(back.active(), Workspace::Bake);
        // The corner x/y values are all finite in the file (the repair
        // replaced them); `focused_surface: null` is a real Option::None
        // and must survive.
        let text = std::fs::read_to_string(&path).expect("file");
        assert!(!text.contains("\"x\":null"), "x corners are finite");
        assert!(!text.contains("\"y\":null"), "y corners are finite");
        assert!(text.contains("focused_surface"), "the field is there");
        let _ = std::fs::remove_file(&path);
    }

    /// A file with null corners (a previous version's save) still loads:
    /// built from a REAL save with the corners torn to nulls, so the
    /// shape matches what the writer produces.
    #[test]
    fn a_torn_null_layouts_file_loads_through_the_same_repair() {
        let ws = Workspaces::default();
        let dock = preset(Workspace::Paint);
        let path =
            std::env::temp_dir().join(format!("umber-layouts-null-{}.json", std::process::id()));
        save_layouts(&path, &ws.snapshot(&dock)).expect("saves");
        let text = std::fs::read_to_string(&path).expect("file");
        // Tear ONLY the x/y corners (a previous version's ±∞ save);
        // other numbers (scroll, fraction) must survive as numbers.
        let torn = text
            .replace("\"x\":0.0", "\"x\":null")
            .replace("\"y\":0.0", "\"y\":null");
        std::fs::write(&path, torn).expect("write");
        let saved = load_layouts(&path).expect("the null-repair loads it");
        assert_eq!(saved.active, Workspace::Paint);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_bad_layouts_file_falls_back_to_the_presets() {
        let path =
            std::env::temp_dir().join(format!("umber-layouts-bad-{}.json", std::process::id()));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_layouts(&path).is_none());
        std::fs::write(
            &path,
            r#"{"version": 99, "active": "Paint", "layouts": []}"#,
        )
        .unwrap();
        assert!(load_layouts(&path).is_none(), "another version is ignored");
        // A HIDDEN panel is a real saved state: it survives the reload.
        let mut hidden = preset(Workspace::Paint);
        toggle_panel(&mut hidden, &Panel::History);
        let saved = SavedLayouts {
            version: LAYOUTS_VERSION,
            active: Workspace::Paint,
            layouts: vec![(Workspace::Paint, hidden.clone())],
        };
        let (_, live) = Workspaces::restore(saved);
        assert_eq!(docked(&live), docked(&hidden), "hiding History is kept");
        assert!(!docked(&live).contains(&Panel::History));
        // A panel docked TWICE (a hand edit) falls back to the preset.
        let mut doubled = preset(Workspace::Paint);
        doubled.push_to_first_leaf(Panel::Viewport);
        let saved = SavedLayouts {
            version: LAYOUTS_VERSION,
            active: Workspace::Paint,
            layouts: vec![(Workspace::Paint, doubled)],
        };
        let (_, live) = Workspaces::restore(saved);
        assert_eq!(docked(&live), docked(&preset(Workspace::Paint)));
        let _ = std::fs::remove_file(&path);
    }

    use super::*;
    use crate::shell_menu::{toggle_panel, ALL_PANELS};
    use egui_dock::TabIndex;

    fn docked(dock: &DockState<Panel>) -> Vec<Panel> {
        let mut tabs: Vec<Panel> = dock.iter_all_tabs().map(|(_, t)| t.clone()).collect();
        tabs.sort_by_key(|p| ALL_PANELS.iter().position(|q| q == p));
        tabs
    }

    #[test]
    fn every_preset_docks_every_panel_exactly_once() {
        for w in Workspace::ALL {
            assert_eq!(docked(&preset(w)), ALL_PANELS.to_vec(), "{w:?}");
        }
    }

    #[test]
    fn presets_put_their_panel_in_front() {
        let mut bake = preset(Workspace::Bake);
        let mut export = preset(Workspace::Export);
        let active = |dock: &mut DockState<Panel>, p: Panel| {
            dock.main_surface_mut()
                .iter_mut()
                .filter_map(|node| node.get_leaf_mut())
                .any(|leaf| leaf.tabs.get(leaf.active.0) == Some(&p))
        };
        assert!(active(&mut bake, Panel::Bakes));
        assert!(active(&mut export, Panel::Export));
        let mut graph = preset(Workspace::Graph);
        assert!(active(&mut graph, Panel::Graph));
    }

    #[test]
    fn cycling_wraps_both_ways() {
        assert_eq!(Workspace::Paint.cycle(true), Workspace::Bake);
        assert_eq!(Workspace::Graph.cycle(true), Workspace::Paint);
        assert_eq!(Workspace::Paint.cycle(false), Workspace::Graph);
        let mut w = Workspace::Paint;
        for _ in 0..Workspace::ALL.len() {
            w = w.cycle(true);
        }
        assert_eq!(w, Workspace::Paint);
    }

    #[test]
    fn switching_remembers_each_workspaces_edits() {
        let mut ws = Workspaces::default();
        let mut dock = preset(Workspace::Paint);
        toggle_panel(&mut dock, &Panel::History); // edit Paint
        let painted = docked(&dock);
        assert!(ws.switch(Workspace::Bake, &mut dock));
        assert_eq!(ws.active(), Workspace::Bake);
        assert_eq!(
            docked(&dock),
            ALL_PANELS.to_vec(),
            "Bake starts from its preset"
        );
        toggle_panel(&mut dock, &Panel::Graph); // edit Bake
        let baked = docked(&dock);
        ws.switch(Workspace::Paint, &mut dock);
        assert_eq!(docked(&dock), painted, "Paint's edit survived");
        ws.switch(Workspace::Bake, &mut dock);
        assert_eq!(docked(&dock), baked, "Bake's edit survived");
        assert!(
            !ws.switch(Workspace::Bake, &mut dock),
            "same workspace is a no-op"
        );
        assert_eq!(docked(&ws.reset_active()), ALL_PANELS.to_vec());
    }

    #[test]
    fn labels_exist_in_both_catalogs() {
        let i18n = crate::i18n::I18n::builtin();
        for w in Workspace::ALL {
            for locale in ["en-US", "pt-BR"] {
                assert!(i18n.has_key(locale, w.label_key()), "{locale} {w:?}");
            }
        }
    }

    /// The active tab of the leaf that currently docks `panel`.
    fn active_of(dock: &DockState<Panel>, panel: Panel) -> Option<Panel> {
        let path = dock.find_tab(&panel)?;
        dock.main_surface()[path.node]
            .get_leaf()
            .and_then(|leaf| leaf.tabs.get(leaf.active.0).cloned())
    }

    #[test]
    fn viewport_preset_view3d_undocks_2d_and_activates_3d() {
        let mut dock = preset(Workspace::Paint); // root = [Viewport, UvView]
        apply_viewport_preset(&mut dock, ViewportPreset::View3d);
        assert!(dock.find_tab(&Panel::UvView).is_none(), "2D undocked");
        assert!(dock.find_tab(&Panel::Viewport).is_some());
        assert_eq!(active_of(&dock, Panel::Viewport), Some(Panel::Viewport));
        // idempotent
        apply_viewport_preset(&mut dock, ViewportPreset::View3d);
        assert!(dock.find_tab(&Panel::UvView).is_none());
    }

    #[test]
    fn viewport_preset_view2d_undocks_3d_and_activates_2d() {
        let mut dock = preset(Workspace::Paint);
        apply_viewport_preset(&mut dock, ViewportPreset::View2d);
        assert!(dock.find_tab(&Panel::Viewport).is_none(), "3D undocked");
        assert!(dock.find_tab(&Panel::UvView).is_some());
        assert_eq!(active_of(&dock, Panel::UvView), Some(Panel::UvView));
    }

    #[test]
    fn viewport_preset_both_redocks_a_missing_view() {
        let mut dock = preset(Workspace::Paint);
        apply_viewport_preset(&mut dock, ViewportPreset::View3d);
        apply_viewport_preset(&mut dock, ViewportPreset::Both);
        assert!(dock.find_tab(&Panel::Viewport).is_some());
        assert!(dock.find_tab(&Panel::UvView).is_some());
        assert_eq!(docked(&dock), ALL_PANELS.to_vec(), "no panel lost");
    }

    #[test]
    fn viewport_preset_swap_in_one_leaf_exchanges_tabs_and_flips_active() {
        // A user-merged leaf (both views tabbed together): Swap exchanges
        // the tabs in place. (The Paint preset keeps the views in separate
        // centre leaves — see paint_preset_splits_the_views_side_by_side.)
        let mut dock = DockState::new(vec![Panel::Viewport, Panel::UvView]);
        apply_viewport_preset(&mut dock, ViewportPreset::Swap);
        let vp = dock.find_tab(&Panel::Viewport).expect("3D docked");
        let leaf = dock.main_surface()[vp.node].get_leaf().unwrap();
        assert_eq!(leaf.tabs, vec![Panel::UvView, Panel::Viewport]);
        assert_eq!(leaf.active, TabIndex(1), "active flipped to the other view");
    }

    #[test]
    fn paint_preset_splits_the_views_side_by_side_in_the_centre() {
        // Painter's default viewport mode: 3D left, 2D right as SPLIT
        // leaves (not tabs), with the rails holding only panels.
        let dock = preset(Workspace::Paint);
        let vp = dock.find_tab(&Panel::Viewport).expect("3D docked");
        let uv = dock.find_tab(&Panel::UvView).expect("2D docked");
        assert_ne!(
            vp.node, uv.node,
            "3D and 2D are side-by-side leaves, not tabs of one leaf"
        );
        assert_eq!(
            docked(&dock),
            ALL_PANELS.to_vec(),
            "every panel docked once"
        );
    }

    #[test]
    fn viewport_preset_swap_across_leaves_exchanges_the_tabs() {
        let mut dock = DockState::new(vec![Panel::Viewport]);
        let [left, right] =
            dock.main_surface_mut()
                .split_right(NodeIndex::root(), 0.5, vec![Panel::UvView]);
        apply_viewport_preset(&mut dock, ViewportPreset::Swap);
        let leaf_tabs = |idx: NodeIndex| dock.main_surface()[idx].get_leaf().unwrap().tabs.clone();
        assert_eq!(
            leaf_tabs(left),
            vec![Panel::UvView],
            "2D moved into the root leaf"
        );
        assert_eq!(
            leaf_tabs(right),
            vec![Panel::Viewport],
            "3D moved to the right leaf"
        );
    }

    #[test]
    fn viewport_preset_swap_with_one_view_docked_switches_to_the_other() {
        let mut dock = preset(Workspace::Paint);
        apply_viewport_preset(&mut dock, ViewportPreset::View3d);
        apply_viewport_preset(&mut dock, ViewportPreset::Swap);
        assert!(dock.find_tab(&Panel::Viewport).is_none());
        assert!(dock.find_tab(&Panel::UvView).is_some());
        assert_eq!(active_of(&dock, Panel::UvView), Some(Panel::UvView));
    }

    #[test]
    fn viewport_preset_survives_a_workspace_switch() {
        let mut ws = Workspaces::default();
        let mut dock = preset(Workspace::Paint);
        apply_viewport_preset(&mut dock, ViewportPreset::View3d);
        ws.switch(Workspace::Bake, &mut dock);
        ws.switch(Workspace::Paint, &mut dock);
        assert!(
            dock.find_tab(&Panel::UvView).is_none(),
            "Paint's preset edit survived the round trip"
        );
        assert!(dock.find_tab(&Panel::Viewport).is_some());
    }
}
