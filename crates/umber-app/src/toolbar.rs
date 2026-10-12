//! The top toolbar (GUI roadmap Phase 0b, docs/specs/gui-audit-and-roadmap.md
//! Part 3): file ops | undo/redo | the tool strip | view toggles | quick
//! mask, built from `umber_gui` parts (the theme tokens + `ToolRow`).
//!
//! # One action path
//!
//! Every toolbar button and every shell shortcut resolves to a
//! [`ShellAction`]; `UmberApp::dispatch` (main.rs) is the only code that
//! acts on one. The keyboard and the toolbar therefore cannot drift: a
//! button does exactly what its key does. The pieces that need no egui
//! context (the key table [`resolve_shortcut`], the view-state machine
//! [`ViewToggles::apply`], the tool mapping [`ToolKind::of`]) are pure and
//! tested headless; [`show`] only draws and collects clicks.
//!
//! # The shortcut table
//!
//! | Key | Action |
//! |---|---|
//! | W / G | wireframe / ground-grid overlay |
//! | V | view mode: lit ↔ unlit (from any other mode, back to lit) |
//! | X | quick-mask mode on/off |
//! | I | invert the quick mask (only while editing it) |
//! | E | eraser |
//! | Ctrl+Z / Ctrl+Shift+Z | undo / redo |
//! | Ctrl+S / Ctrl+O | save project / open project |
//!
//! That is ten bindings; the roadmap line says "the 9 shortcut actions".
//! The brief behind this module enumerated all ten (the six pre-existing
//! letter keys + four chords), so all ten ship.
//!
//! # Judgment calls
//!
//! - **Letter keys need a bare press.** W/G/V/X/I/E fire only without
//!   Ctrl/Cmd held, so Ctrl+X / Ctrl+V / Ctrl+E (cut, paste, and the
//!   Ctrl-held painting gesture) never flip the mask, the view, or the
//!   tool. Before this module they fired whatever the modifiers. Shift and
//!   Alt are still ignored for the letter keys, as before.
//! - **Ctrl+Shift+Z is checked before Ctrl+Z**, so redo never also undoes.
//!   Ctrl+Y is not bound: the brief named Ctrl+Shift+Z only.
//! - **Undo/redo target the document journal** (`Document::undo/redo`, the
//!   layer stack the History panel lists). Retouch strokes keep their own
//!   per-stroke stack in v1 (History panel buttons). The two stacks share
//!   no timeline, so routing Ctrl+Z between them would guess at ordering.
//!   A unified timeline is the follow-up, not a toolbar concern.
//! - **Ctrl+O opens a project, not a mesh.** That matches the DCC habit
//!   where Ctrl+O is the document. The mesh comes in through the toolbar's
//!   Open Mesh button, the File menu, or a drop on the empty viewport.
//! - **Ctrl+S is Save Project as it exists today**: `save_project` always
//!   shows the save dialog (Save As semantics). A silent re-save to
//!   `project_dir` would need its own overwrite-safety story.
//! - **New is shown but disabled.** A real New has to clear the paint
//!   targets, the fill caches, the mesh and the graph as well as the
//!   document. A Document-only reset would leave the old painting on
//!   screen under an empty layer stack. It is deferred, with the reason in
//!   its tooltip, and has no `ShellAction` until it can be honored.
//! - **The tool strip shows placeholder glyphs.** `ToolRow` draws from an
//!   [`IconAtlas`], and the lucide rasterizing step (`umber_gui::icons`'
//!   `icon-raster` follow-up) has not landed, so the only atlas is
//!   `IconAtlas::empty()`. Each slot draws a faint box with a localized
//!   tooltip. The active tool's name is printed beside the strip, so the
//!   strip stays readable until the glyphs arrive. File/history/view
//!   controls are text buttons for the same reason.
//! - **The tool strip is the four brush tools** (`BrushTool`: paint,
//!   smudge, clone, eraser), keyed by the fieldless [`ToolKind`].
//!   `BrushTool` itself can't key `ToolRow`: its `PartialEq` compares the
//!   retouch params, so an edited smudge would never match its slot.
//!   Fill, path and polygon are panel-armed modes, not brush tools, and
//!   stay in their panels.
//! - **Chrome-scoped style.** The bar applies `umber_style` to its own
//!   `Ui` only. Applying it app-wide is the Phase 0a boot wiring, which
//!   would restyle the eleven unmigrated panels ahead of the Phase 1 wave
//!   ("no panel gets migrated twice").

use egui::{Key, Modifiers, RichText, Ui};
use umber_brush::BrushTool;
use umber_gpu::ViewMode;
use umber_gui::theme;
use umber_gui::{IconAtlas, Tool, ToolRow};

use crate::i18n::tr;

/// Everything the toolbar and the shell shortcuts can ask the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellAction {
    /// File > Open Mesh (dialog).
    OpenMesh,
    /// Open a `.umber` project (dialog). Ctrl+O.
    OpenProject,
    /// Save the project (dialog). Ctrl+S.
    SaveProject,
    /// Undo one document edit. Ctrl+Z.
    Undo,
    /// Redo one document edit. Ctrl+Shift+Z.
    Redo,
    /// Wireframe overlay on/off. W.
    ToggleWireframe,
    /// Ground grid on/off. G.
    ToggleGrid,
    /// Lit ↔ unlit (any other mode returns to lit). V.
    CycleViewMode,
    /// Quick-mask mode on/off. X.
    ToggleMaskMode,
    /// Invert the quick mask (no-op outside mask mode). I.
    InvertMask,
    /// Select a brush tool. E selects the eraser.
    SelectTool(ToolKind),
}

/// The brush tool without its params: the tool strip's slot id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// The GPU stamp.
    Paint,
    /// Smudge (retouch).
    Smudge,
    /// Clone (retouch).
    Clone,
    /// Eraser (retouch).
    Erase,
}

impl ToolKind {
    /// The strip order.
    pub const ALL: [ToolKind; 4] = [
        ToolKind::Paint,
        ToolKind::Smudge,
        ToolKind::Clone,
        ToolKind::Erase,
    ];

    /// The kind of a (param-carrying) brush tool.
    pub fn of(tool: &BrushTool) -> Self {
        match tool {
            BrushTool::Paint => ToolKind::Paint,
            BrushTool::Smudge(_) => ToolKind::Smudge,
            BrushTool::Clone(_) => ToolKind::Clone,
            BrushTool::Erase(_) => ToolKind::Erase,
        }
    }

    /// The lucide id of the strip glyph (a member of
    /// `umber_gui::icons::TOOL_ICONS`).
    pub fn icon(self) -> &'static str {
        match self {
            ToolKind::Paint => "paintbrush",
            ToolKind::Smudge => "fingerprint",
            ToolKind::Clone => "stamp",
            ToolKind::Erase => "eraser",
        }
    }

    /// The localized tool name (the Brush panel's own keys).
    pub fn label(self) -> String {
        match self {
            ToolKind::Paint => tr("brush.tool-paint"),
            ToolKind::Smudge => tr("brush.tool-smudge"),
            ToolKind::Clone => tr("brush.tool-clone"),
            ToolKind::Erase => tr("brush.tool-eraser"),
        }
    }
}

/// The keys the shell polls each frame (the letter keys + the chord keys).
pub const SHORTCUT_KEYS: [Key; 9] = [
    Key::W,
    Key::G,
    Key::V,
    Key::X,
    Key::I,
    Key::E,
    Key::Z,
    Key::S,
    Key::O,
];

/// The shell's key table (see the module docs). `None` for any key or
/// modifier combination that isn't bound.
pub fn resolve_shortcut(key: Key, mods: Modifiers) -> Option<ShellAction> {
    if mods.command {
        // Chords. Alt+chord is reserved (unbound) rather than aliased.
        if mods.alt {
            return None;
        }
        return match key {
            Key::Z if mods.shift => Some(ShellAction::Redo),
            Key::Z => Some(ShellAction::Undo),
            Key::S if !mods.shift => Some(ShellAction::SaveProject),
            Key::O if !mods.shift => Some(ShellAction::OpenProject),
            _ => None,
        };
    }
    // On macOS `command` is Cmd and `ctrl` is the separate Control key;
    // a held Control is still not a bare press.
    if mods.ctrl {
        return None;
    }
    match key {
        Key::W => Some(ShellAction::ToggleWireframe),
        Key::G => Some(ShellAction::ToggleGrid),
        Key::V => Some(ShellAction::CycleViewMode),
        Key::X => Some(ShellAction::ToggleMaskMode),
        Key::I => Some(ShellAction::InvertMask),
        Key::E => Some(ShellAction::SelectTool(ToolKind::Erase)),
        _ => None,
    }
}

/// Every bound action pressed this frame, in [`SHORTCUT_KEYS`] order.
/// Empty while a text edit has focus, so typing a preset name never flips
/// the viewport.
pub fn poll_shortcuts(ctx: &egui::Context) -> Vec<ShellAction> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    ctx.input(|i| {
        SHORTCUT_KEYS
            .iter()
            .filter(|key| i.key_pressed(**key))
            .filter_map(|key| resolve_shortcut(*key, i.modifiers))
            .collect()
    })
}

/// The view state the toolbar toggles and the viewport reads each frame
/// (wireframe, grid, shading mode). Off/off/Lit by default: today's render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewToggles {
    /// Wireframe overlay (View menu, W).
    pub wireframe: bool,
    /// Ground grid (View menu, G).
    pub grid: bool,
    /// Shading mode (View menu rows, V).
    pub view_mode: ViewMode,
}

impl ViewToggles {
    /// Applies a view action. Returns whether `action` was one; any other
    /// action leaves the state unchanged and returns `false`.
    pub fn apply(&mut self, action: ShellAction) -> bool {
        match action {
            ShellAction::ToggleWireframe => self.wireframe = !self.wireframe,
            ShellAction::ToggleGrid => self.grid = !self.grid,
            ShellAction::CycleViewMode => self.view_mode = self.view_mode.cycle_v(),
            _ => return false,
        }
        true
    }
}

/// A read-only snapshot of what the toolbar shows, taken before it draws
/// so drawing never borrows the app.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolbarView {
    /// The active brush tool.
    pub tool: ToolKind,
    /// Whether the document has an edit to undo.
    pub can_undo: bool,
    /// Whether the document has an edit to redo.
    pub can_redo: bool,
    /// The view toggles' current state.
    pub view: ViewToggles,
    /// Whether a paint session exists (the mask controls need one).
    pub paint_available: bool,
    /// Whether the quick mask is being edited (enables Invert).
    pub mask_editing: bool,
}

/// Draws the toolbar into `ui` and returns the actions clicked this frame
/// (dispatched by the caller after the panel closes).
pub fn show(ui: &mut Ui, view: &ToolbarView, atlas: &IconAtlas) -> Vec<ShellAction> {
    ui.set_style(theme::umber_style(theme::Density::Comfortable));
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        // File ops.
        ui.add_enabled(false, egui::Button::new(tr("shell.new")))
            .on_disabled_hover_text(tr("shell.new-deferred"));
        if ui
            .button(tr("menu.open-mesh"))
            .on_hover_text(tr("shell.tip-open-mesh"))
            .clicked()
        {
            actions.push(ShellAction::OpenMesh);
        }
        if ui
            .button(tr("shell.open"))
            .on_hover_text(tr("shell.tip-open-project"))
            .clicked()
        {
            actions.push(ShellAction::OpenProject);
        }
        if ui
            .button(tr("button.save"))
            .on_hover_text(tr("shell.tip-save-project"))
            .clicked()
        {
            actions.push(ShellAction::SaveProject);
        }
        ui.separator();

        // History.
        if ui
            .add_enabled(view.can_undo, egui::Button::new(tr("shell.undo")))
            .on_hover_text(tr("shell.tip-undo"))
            .clicked()
        {
            actions.push(ShellAction::Undo);
        }
        if ui
            .add_enabled(view.can_redo, egui::Button::new(tr("shell.redo")))
            .on_hover_text(tr("shell.tip-redo"))
            .clicked()
        {
            actions.push(ShellAction::Redo);
        }
        ui.separator();

        // The tool strip (placeholder glyphs until icon-raster lands; see
        // the module docs) + the active tool's name.
        let tips: Vec<String> = ToolKind::ALL.iter().map(|k| k.label()).collect();
        let tools: Vec<Tool<'_, ToolKind>> = ToolKind::ALL
            .iter()
            .zip(&tips)
            .map(|(kind, tip)| Tool {
                id: *kind,
                icon: kind.icon(),
                tooltip: tip.as_str(),
            })
            .collect();
        let mut active = view.tool;
        ToolRow::new("umber-toolbar-tools", &tools).show(ui, atlas, &mut active);
        if active != view.tool {
            actions.push(ShellAction::SelectTool(active));
        }
        ui.label(RichText::new(view.tool.label()).color(theme::muted()));
        ui.separator();

        // View toggles (the View menu's checkboxes, W / G / V).
        if ui
            .selectable_label(view.view.wireframe, tr("shell.wireframe"))
            .on_hover_text(tr("shell.tip-wireframe"))
            .clicked()
        {
            actions.push(ShellAction::ToggleWireframe);
        }
        if ui
            .selectable_label(view.view.grid, tr("shell.grid"))
            .on_hover_text(tr("shell.tip-grid"))
            .clicked()
        {
            actions.push(ShellAction::ToggleGrid);
        }
        let lit = view.view.view_mode == ViewMode::Lit;
        let mode = if lit {
            tr("shell.view-lit")
        } else {
            tr("shell.view-other")
        };
        if ui
            .button(mode)
            .on_hover_text(tr("shell.tip-view-cycle"))
            .clicked()
        {
            actions.push(ShellAction::CycleViewMode);
        }
        ui.separator();

        // Quick mask (X / I).
        let mask = ui
            .add_enabled(
                view.paint_available,
                egui::Button::selectable(view.mask_editing, tr("shell.mask")),
            )
            .on_hover_text(tr("shell.tip-mask"));
        if mask.clicked() {
            actions.push(ShellAction::ToggleMaskMode);
        }
        if ui
            .add_enabled(view.mask_editing, egui::Button::new(tr("shell.invert-mask")))
            .on_hover_text(tr("shell.tip-invert-mask"))
            .clicked()
        {
            actions.push(ShellAction::InvertMask);
        }
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_brush::retouch::{CloneParams, EraseParams, SmudgeParams};

    fn bare() -> Modifiers {
        Modifiers::default()
    }

    /// Ctrl on Windows/Linux: egui sets both `ctrl` and `command`.
    fn cmd() -> Modifiers {
        Modifiers {
            command: true,
            ctrl: true,
            ..bare()
        }
    }

    fn cmd_shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..cmd()
        }
    }

    #[test]
    fn letter_keys_map_to_their_actions() {
        let table = [
            (Key::W, ShellAction::ToggleWireframe),
            (Key::G, ShellAction::ToggleGrid),
            (Key::V, ShellAction::CycleViewMode),
            (Key::X, ShellAction::ToggleMaskMode),
            (Key::I, ShellAction::InvertMask),
            (Key::E, ShellAction::SelectTool(ToolKind::Erase)),
        ];
        for (key, action) in table {
            assert_eq!(resolve_shortcut(key, bare()), Some(action), "{key:?}");
            // Shift is ignored for the letter keys (pre-existing behavior).
            let shift = Modifiers {
                shift: true,
                ..bare()
            };
            assert_eq!(resolve_shortcut(key, shift), Some(action), "{key:?}");
        }
    }

    #[test]
    fn letter_keys_do_not_fire_under_ctrl_or_cmd() {
        for key in [Key::W, Key::G, Key::V, Key::X, Key::I, Key::E] {
            assert_eq!(resolve_shortcut(key, cmd()), None, "{key:?}");
            // macOS: Control held without Cmd.
            let ctrl_only = Modifiers {
                ctrl: true,
                ..bare()
            };
            assert_eq!(resolve_shortcut(key, ctrl_only), None, "{key:?}");
        }
    }

    #[test]
    fn chords_map_and_redo_wins_over_undo() {
        assert_eq!(resolve_shortcut(Key::Z, cmd()), Some(ShellAction::Undo));
        assert_eq!(resolve_shortcut(Key::Z, cmd_shift()), Some(ShellAction::Redo));
        assert_eq!(
            resolve_shortcut(Key::S, cmd()),
            Some(ShellAction::SaveProject)
        );
        assert_eq!(
            resolve_shortcut(Key::O, cmd()),
            Some(ShellAction::OpenProject)
        );
        // Unbound shifted / alt variants.
        assert_eq!(resolve_shortcut(Key::S, cmd_shift()), None);
        assert_eq!(resolve_shortcut(Key::O, cmd_shift()), None);
        let cmd_alt = Modifiers {
            alt: true,
            ..cmd()
        };
        assert_eq!(resolve_shortcut(Key::Z, cmd_alt), None);
        // The chord keys alone do nothing.
        for key in [Key::Z, Key::S, Key::O] {
            assert_eq!(resolve_shortcut(key, bare()), None, "{key:?}");
        }
    }

    #[test]
    fn every_polled_key_is_bound_to_something() {
        // A key in SHORTCUT_KEYS with no binding would be polled for
        // nothing; each one resolves bare or under Cmd.
        for key in SHORTCUT_KEYS {
            let bound =
                resolve_shortcut(key, bare()).is_some() || resolve_shortcut(key, cmd()).is_some();
            assert!(bound, "{key:?} is polled but unbound");
        }
        // Ten bindings: six bare + four chords (Z, Shift+Z, S, O).
        let bare = SHORTCUT_KEYS
            .iter()
            .filter(|k| resolve_shortcut(**k, bare()).is_some())
            .count();
        let chords = SHORTCUT_KEYS
            .iter()
            .filter(|k| resolve_shortcut(**k, cmd()).is_some())
            .count()
            + SHORTCUT_KEYS
                .iter()
                .filter(|k| resolve_shortcut(**k, cmd_shift()).is_some())
                .count();
        assert_eq!((bare, chords), (6, 4));
    }

    #[test]
    fn view_toggles_apply_the_view_actions_only() {
        let mut v = ViewToggles::default();
        assert!(v.apply(ShellAction::ToggleWireframe));
        assert!(v.apply(ShellAction::ToggleGrid));
        assert_eq!(
            v,
            ViewToggles {
                wireframe: true,
                grid: true,
                view_mode: ViewMode::Lit,
            }
        );
        assert!(v.apply(ShellAction::ToggleWireframe));
        assert!(!v.wireframe);

        // V: Lit → Unlit → Lit.
        assert!(v.apply(ShellAction::CycleViewMode));
        assert_eq!(v.view_mode, ViewMode::Unlit);
        assert!(v.apply(ShellAction::CycleViewMode));
        assert_eq!(v.view_mode, ViewMode::Lit);

        // Non-view actions are refused and change nothing.
        let before = v;
        for action in [
            ShellAction::Undo,
            ShellAction::Redo,
            ShellAction::OpenMesh,
            ShellAction::OpenProject,
            ShellAction::SaveProject,
            ShellAction::ToggleMaskMode,
            ShellAction::InvertMask,
            ShellAction::SelectTool(ToolKind::Smudge),
        ] {
            assert!(!v.apply(action), "{action:?}");
        }
        assert_eq!(v, before);
    }

    #[test]
    fn tool_kind_ignores_params_and_icons_are_in_the_tool_family() {
        let edited_smudge = BrushTool::Smudge(SmudgeParams {
            strength: 0.123,
            ..SmudgeParams::default()
        });
        assert_eq!(ToolKind::of(&edited_smudge), ToolKind::Smudge);
        assert_eq!(ToolKind::of(&BrushTool::Paint), ToolKind::Paint);
        assert_eq!(
            ToolKind::of(&BrushTool::Clone(CloneParams::default())),
            ToolKind::Clone
        );
        assert_eq!(
            ToolKind::of(&BrushTool::Erase(EraseParams::default())),
            ToolKind::Erase
        );
        for kind in ToolKind::ALL {
            assert!(
                umber_gui::icons::TOOL_ICONS.contains(&kind.icon()),
                "{kind:?}"
            );
        }
    }
}
