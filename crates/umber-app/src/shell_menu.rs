//! The shell menus' pure parts (docs/specs/gui-plan-2026-10-15.md kill
//! `g5-menu-completion`, P:A1-A5, B:A3.2-A3.4): the default dock layout
//! (Window ▸ Reset Layout rebuilds it), the panel list Window ▸ Panels
//! toggles, the About facts, and the File ▸ Revert gate. `main.rs` draws
//! the menus over these; every decision here is testable without egui.

use egui_dock::{DockState, NodeIndex};

use crate::Panel;

/// Every panel, in the order Window ▸ Panels lists them.
pub const ALL_PANELS: [Panel; 11] = [
    Panel::Viewport,
    Panel::UvView,
    Panel::LayerStack,
    Panel::Properties,
    Panel::Assets,
    Panel::History,
    Panel::TextureSets,
    Panel::Bakes,
    Panel::Export,
    Panel::Graph,
    Panel::Display,
];

/// The boot layout: [left column | centre (3D + 2D UV tabs)] with the
/// history docked right. `UmberApp::new` and Window ▸ Reset Layout both
/// build it from here, so a reset is exactly the first-launch layout.
pub fn default_dock() -> DockState<Panel> {
    let mut dock = DockState::new(vec![Panel::Viewport, Panel::UvView]);
    let tree = dock.main_surface_mut();
    let [left, _] = tree.split_left(
        NodeIndex::root(),
        0.25,
        vec![
            Panel::LayerStack,
            Panel::TextureSets,
            Panel::Bakes,
            Panel::Export,
            Panel::Graph,
            Panel::Display,
        ],
    );
    let [_top, _bottom] = tree.split_below(left, 0.4, vec![Panel::Properties, Panel::Assets]);
    tree.split_right(NodeIndex::root(), 0.2, vec![Panel::History]);
    dock
}

/// Shows or hides `panel`: a hidden panel is pushed into the focused
/// leaf (the first leaf when nothing is focused), a shown one is removed
/// from wherever it is docked. Returns whether it is open afterwards.
pub fn toggle_panel(dock: &mut DockState<Panel>, panel: &Panel) -> bool {
    match dock.find_tab(panel) {
        Some(path) => {
            dock.remove_tab(path);
            false
        }
        None => {
            if dock.focused_leaf().is_some() {
                dock.push_to_focused_leaf(panel.clone());
            } else {
                dock.push_to_first_leaf(panel.clone());
            }
            true
        }
    }
}

/// Whether `panel` is docked anywhere.
pub fn is_open(dock: &DockState<Panel>, panel: &Panel) -> bool {
    dock.find_tab(panel).is_some()
}

/// A focus mode (GUI plan g5-focus-modes, P:A4, B:A1.4, B:A1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// The normal docked layout.
    Normal,
    /// Hide UI (Tab): the viewport alone, no toolbar or status bar. The
    /// menu bar and a corner button stay as the visible way back.
    HideUi,
    /// One dock area fills the window (Ctrl+Space); the menu, toolbar and
    /// status bar stay.
    Maximized,
}

/// The focus-mode state: which mode is on, and the layout it replaced.
/// Entering a mode swaps the live dock for a one-tab dock and keeps the
/// real layout aside, so leaving the mode restores it exactly (sizes,
/// tab order, the focused tab), whatever the user did in between.
#[derive(Debug)]
pub struct FocusState {
    mode: FocusMode,
    saved: Option<DockState<Panel>>,
}

impl Default for FocusState {
    fn default() -> Self {
        FocusState {
            mode: FocusMode::Normal,
            saved: None,
        }
    }
}

impl FocusState {
    /// The current mode.
    pub fn mode(&self) -> FocusMode {
        self.mode
    }

    /// Toggles `target`: entering it shows `keep` alone; toggling the mode
    /// that is already on returns to the saved layout. Switching straight
    /// from one mode to the other restores first, so the saved layout is
    /// always the user's real one.
    pub fn toggle(&mut self, target: FocusMode, dock: &mut DockState<Panel>, keep: Panel) {
        if self.mode == target || target == FocusMode::Normal {
            self.restore(dock);
            return;
        }
        self.restore(dock);
        self.saved = Some(std::mem::replace(dock, DockState::new(vec![keep])));
        self.mode = target;
    }

    /// Leaves any focus mode, restoring the saved layout (a no-op in
    /// Normal). The Window menu calls this before it edits the layout.
    pub fn restore(&mut self, dock: &mut DockState<Panel>) {
        if let Some(saved) = self.saved.take() {
            *dock = saved;
        }
        self.mode = FocusMode::Normal;
    }
}

/// The tab Ctrl+Space maximizes: the focused dock area's active tab, or
/// the viewport when nothing is focused.
pub fn maximize_target(dock: &mut DockState<Panel>) -> Panel {
    dock.find_active_focused()
        .map(|(_, tab)| tab.clone())
        .unwrap_or(Panel::Viewport)
}

/// File ▸ Revert needs a saved project with unsaved edits: `Err(reason
/// key)` otherwise, the same shape as `actions::availability`.
pub fn revert_availability(has_project_dir: bool, dirty: bool) -> Result<(), &'static str> {
    if !has_project_dir {
        Err("menu.revert-unsaved")
    } else if !dirty {
        Err("menu.revert-clean")
    } else {
        Ok(())
    }
}

/// The About window's facts: the package version, the commit the binary
/// was built from (`UMBER_GIT_HASH` at build time, when the build sets
/// it), and the GPU the session is running on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutInfo {
    /// `CARGO_PKG_VERSION`.
    pub version: &'static str,
    /// The build's commit, or `None` for a build that did not record it.
    pub commit: Option<&'static str>,
    /// The adapter name.
    pub adapter: String,
    /// The backend (Vulkan, Dx12, Metal, Gl, ...).
    pub backend: String,
}

impl AboutInfo {
    /// The facts for this build on `adapter` / `backend`. The commit comes
    /// from `build.rs` (`GITHUB_SHA`, else `git rev-parse`).
    pub fn new(adapter: String, backend: String) -> Self {
        AboutInfo {
            version: env!("CARGO_PKG_VERSION"),
            commit: option_env!("UMBER_GIT_HASH").filter(|h| !h.is_empty()),
            adapter,
            backend,
        }
    }

    /// One line per fact, for the window and for Copy to Clipboard (a bug
    /// report carries the same text the user sees).
    pub fn lines(&self) -> Vec<String> {
        vec![
            format!("Umber {}", self.version),
            format!("commit: {}", self.commit.unwrap_or("unknown")),
            format!("GPU: {} ({})", self.adapter, self.backend),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docked(dock: &DockState<Panel>) -> Vec<Panel> {
        let mut tabs: Vec<Panel> = dock.iter_all_tabs().map(|(_, t)| t.clone()).collect();
        tabs.sort_by_key(|p| ALL_PANELS.iter().position(|q| q == p));
        tabs
    }

    #[test]
    fn default_dock_holds_every_panel_once() {
        assert_eq!(docked(&default_dock()), ALL_PANELS.to_vec());
    }

    #[test]
    fn toggle_hides_then_shows_and_reset_restores() {
        let mut dock = default_dock();
        assert!(is_open(&dock, &Panel::History));
        assert!(!toggle_panel(&mut dock, &Panel::History));
        assert!(!is_open(&dock, &Panel::History));
        assert_eq!(docked(&dock).len(), ALL_PANELS.len() - 1);
        assert!(toggle_panel(&mut dock, &Panel::History));
        assert!(is_open(&dock, &Panel::History));
        assert_eq!(docked(&dock), ALL_PANELS.to_vec(), "back exactly once");
        // Reset is the boot layout again, whatever was closed.
        toggle_panel(&mut dock, &Panel::Bakes);
        toggle_panel(&mut dock, &Panel::Graph);
        dock = default_dock();
        assert_eq!(docked(&dock), ALL_PANELS.to_vec());
    }

    #[test]
    fn closing_everything_and_reopening_never_loses_a_panel() {
        let mut dock = default_dock();
        for p in &ALL_PANELS {
            toggle_panel(&mut dock, p);
        }
        assert!(docked(&dock).is_empty());
        for p in &ALL_PANELS {
            assert!(toggle_panel(&mut dock, p), "{p:?}");
        }
        assert_eq!(docked(&dock), ALL_PANELS.to_vec());
    }

    #[test]
    fn focus_modes_swap_and_restore_the_exact_layout() {
        let mut dock = default_dock();
        toggle_panel(&mut dock, &Panel::Graph); // a user edit before entering
        let before = docked(&dock);
        let mut focus = FocusState::default();
        focus.toggle(FocusMode::HideUi, &mut dock, Panel::Viewport);
        assert_eq!(focus.mode(), FocusMode::HideUi);
        assert_eq!(docked(&dock), vec![Panel::Viewport]);
        // Switching straight to Maximized restores first, then maximizes.
        focus.toggle(FocusMode::Maximized, &mut dock, Panel::Bakes);
        assert_eq!(focus.mode(), FocusMode::Maximized);
        assert_eq!(docked(&dock), vec![Panel::Bakes]);
        // Toggling the active mode returns to the user's real layout.
        focus.toggle(FocusMode::Maximized, &mut dock, Panel::Bakes);
        assert_eq!(focus.mode(), FocusMode::Normal);
        assert_eq!(docked(&dock), before);
        // Restore in Normal is a no-op.
        focus.restore(&mut dock);
        assert_eq!(docked(&dock), before);
    }

    #[test]
    fn maximize_targets_the_viewport_without_focus() {
        let mut dock = default_dock();
        assert_eq!(maximize_target(&mut dock), Panel::Viewport);
    }

    #[test]
    fn revert_needs_a_saved_dirty_project() {
        assert_eq!(revert_availability(false, true), Err("menu.revert-unsaved"));
        assert_eq!(revert_availability(true, false), Err("menu.revert-clean"));
        assert_eq!(
            revert_availability(false, false),
            Err("menu.revert-unsaved")
        );
        assert_eq!(revert_availability(true, true), Ok(()));
    }

    #[test]
    fn about_lines_carry_version_commit_and_gpu() {
        let info = AboutInfo::new("TestGPU".into(), "Vulkan".into());
        let lines = info.lines();
        assert_eq!(lines[0], format!("Umber {}", env!("CARGO_PKG_VERSION")));
        assert!(lines[1].starts_with("commit: "));
        assert_eq!(lines[2], "GPU: TestGPU (Vulkan)");
        // build.rs records a hex commit whenever git or GITHUB_SHA is
        // there (always true in this repo's builds and in CI).
        if let Some(commit) = info.commit {
            assert!(commit.len() >= 7 && commit.bytes().all(|b| b.is_ascii_hexdigit()));
            assert_eq!(lines[1], format!("commit: {commit}"));
        }
        let none = AboutInfo {
            commit: None,
            ..info
        };
        assert_eq!(none.lines()[1], "commit: unknown");
    }

    #[test]
    fn every_menu_key_exists_in_both_catalogs() {
        let i18n = crate::i18n::I18n::builtin();
        for key in [
            "menu.revert",
            "menu.revert-unsaved",
            "menu.revert-clean",
            "menu.window",
            "menu.panels",
            "menu.reset-layout",
            "menu.about-copy",
            "menu.about-close",
            "shell.hide-ui",
            "shell.maximize",
            "shell.show-ui",
        ] {
            for locale in ["en-US", "pt-BR"] {
                assert!(i18n.has_key(locale, key), "{locale} missing {key}");
            }
        }
    }
}
