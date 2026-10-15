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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// The viewport (3D + 2D) in the centre, `front` in a left column (its
/// first tab active), `rest` tabbed on the right.
fn side_layout(front: Vec<Panel>, rest: Vec<Panel>) -> DockState<Panel> {
    let mut dock = DockState::new(vec![Panel::Viewport, Panel::UvView]);
    let tree = dock.main_surface_mut();
    tree.split_left(NodeIndex::root(), 0.28, front);
    tree.split_right(NodeIndex::root(), 0.22, rest);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_menu::{toggle_panel, ALL_PANELS};

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
}
