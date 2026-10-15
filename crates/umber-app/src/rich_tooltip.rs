//! Rich tooltips (GUI plan G3 `g3-rich-tooltips`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: Blender E4 ("tooltips
//! explain the control and show its hotkey; a preference turns them
//! off"), Blender E5 (an optional developer line with the command id),
//! photocraft #12 (a disabled control says WHY in its hover)).
//!
//! A shell action's tooltip is built from the registry, never from a
//! string with a key baked into it:
//!
//! - a **title** (the action's label);
//! - a **description** (one sentence of what it does);
//! - the **live shortcut**, formatted from `actions::BINDINGS`, so the
//!   key printed is the key that fires, and it follows a future keymap
//!   edit;
//! - when the action is unavailable, the **reason** (the same i18n key
//!   `actions::availability` returns, which the toast also uses).
//!
//! Before this kill the toolbar's tooltips carried their shortcuts inside
//! the translated strings ("Open a .umber project (Ctrl+O)"). Every locale
//! had to repeat them, and nothing kept them in step with the table.
//! [`tooltip_parts`] is the pure assembly; [`RichTooltip::action_tip`]
//! attaches it to a response.

use crate::actions::{self, ShellState};
use crate::toolbar::ShellAction;

/// The pieces of one action tooltip (all already translated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipParts {
    pub title: String,
    pub description: Option<String>,
    pub shortcut: Option<String>,
    /// `Some` when the action cannot run now: why.
    pub unavailable: Option<String>,
}

/// The i18n keys of an action's title and description.
pub fn tooltip_keys(action: ShellAction) -> (&'static str, Option<&'static str>) {
    use crate::toolbar::ToolKind as T;
    match action {
        ShellAction::NewProject => ("shell.new", Some("shell.tip-new")),
        ShellAction::OpenMesh => ("menu.open-mesh", Some("shell.tip-open-mesh")),
        ShellAction::OpenProject => ("menu.open-project", Some("tip.open-project")),
        ShellAction::SaveProject => ("menu.save-project", Some("tip.save-project")),
        ShellAction::SaveProjectAs => ("button.save-as", Some("tip.save-project")),
        ShellAction::Undo => ("shell.undo", Some("tip.undo")),
        ShellAction::Redo => ("shell.redo", Some("tip.redo")),
        ShellAction::ToggleWireframe => ("shell.wireframe", Some("tip.wireframe")),
        ShellAction::ToggleGrid => ("shell.grid", Some("tip.grid")),
        ShellAction::CycleViewMode => ("tip.view-cycle-title", Some("tip.view-cycle")),
        ShellAction::ToggleMaskMode => ("shell.mask", Some("tip.mask")),
        ShellAction::InvertMask => ("shell.invert-mask", Some("tip.invert-mask")),
        ShellAction::SelectTool(T::Paint) => ("brush.tool-paint", None),
        ShellAction::SelectTool(T::Smudge) => ("brush.tool-smudge", None),
        ShellAction::SelectTool(T::Clone) => ("brush.tool-clone", None),
        ShellAction::SelectTool(T::Erase) => ("brush.tool-eraser", None),
        ShellAction::SelectTool(T::Pick) => ("brush.tool-pick", None),
        ShellAction::SelectTool(T::IdMask) => ("brush.tool-id-mask", None),
        ShellAction::BrushSmaller => ("palette.brush-smaller", None),
        ShellAction::BrushLarger => ("palette.brush-larger", None),
        ShellAction::CommandPalette => ("tip.palette-title", Some("tip.palette")),
        ShellAction::Preferences => ("prefs.title", None),
        ShellAction::ToggleHideUi => ("shell.hide-ui", Some("tip.hide-ui")),
        ShellAction::ToggleMaximize => ("shell.maximize", Some("tip.maximize")),
        ShellAction::NextWorkspace => ("workspace.next", None),
        ShellAction::PrevWorkspace => ("workspace.prev", None),
        ShellAction::ViewMaterial => ("viewkeys.material", None),
        ShellAction::NextChannel => ("viewkeys.next-channel", None),
        ShellAction::PrevChannel => ("viewkeys.prev-channel", None),
        ShellAction::NextMeshMap => ("viewkeys.next-map", None),
        ShellAction::PrevMeshMap => ("viewkeys.prev-map", None),
    }
}

/// Assembles `action`'s tooltip. `tr` translates; `state` decides the
/// unavailable reason; `is_mac` picks the shortcut's modifier names.
pub fn tooltip_parts(
    action: ShellAction,
    state: ShellState,
    is_mac: bool,
    tr: impl Fn(&str) -> String,
) -> TooltipParts {
    let (title, desc) = tooltip_keys(action);
    TooltipParts {
        title: tr(title),
        description: desc.map(&tr),
        shortcut: actions::shortcut_label(action, is_mac),
        unavailable: actions::availability(action, state).err().map(&tr),
    }
}

/// Draws `parts` into a tooltip `ui`: the title strong with the shortcut
/// right-aligned and muted, the description wrapped below, then the
/// unavailable reason in the warning colour.
pub fn draw(ui: &mut egui::Ui, parts: &TooltipParts) {
    ui.horizontal(|ui| {
        ui.strong(&parts.title);
        if let Some(s) = &parts.shortcut {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(s).monospace().color(umber_gui::muted()));
            });
        }
    });
    if let Some(d) = &parts.description {
        ui.label(d);
    }
    if let Some(r) = &parts.unavailable {
        ui.label(egui::RichText::new(r).color(umber_gui::theme::palette::WARN));
    }
}

/// `Response::action_tip`: the rich tooltip for a shell action, shown on
/// hover whether the control is enabled or not (a disabled one says why).
pub trait RichTooltip {
    fn action_tip(self, action: ShellAction, state: ShellState) -> Self;
}

impl RichTooltip for egui::Response {
    fn action_tip(self, action: ShellAction, state: ShellState) -> Self {
        let is_mac = self.ctx.os() == egui::os::OperatingSystem::Mac;
        let parts = tooltip_parts(action, state, is_mac, crate::i18n::tr);
        let parts2 = parts.clone();
        self.on_hover_ui(move |ui| draw(ui, &parts))
            .on_disabled_hover_ui(move |ui| draw(ui, &parts2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo(k: &str) -> String {
        k.to_string()
    }

    #[test]
    fn shortcut_comes_from_the_registry_not_the_string() {
        let p = tooltip_parts(ShellAction::OpenProject, ShellState::default(), false, echo);
        assert_eq!(p.title, "menu.open-project");
        assert_eq!(p.shortcut.as_deref(), Some("Ctrl+O"));
        let mac = tooltip_parts(ShellAction::OpenProject, ShellState::default(), true, echo);
        assert!(mac.shortcut.unwrap().contains('⌘'));
        // An unbound action shows no shortcut line.
        let p = tooltip_parts(ShellAction::OpenMesh, ShellState::default(), false, echo);
        assert_eq!(p.shortcut, None);
    }

    #[test]
    fn unavailable_actions_name_the_registry_reason() {
        let none = ShellState::default();
        let p = tooltip_parts(ShellAction::Undo, none, false, echo);
        assert_eq!(
            p.unavailable.as_deref(),
            Some("action.unavailable-nothing-to-undo")
        );
        let ready = ShellState {
            can_undo: true,
            ..none
        };
        assert_eq!(
            tooltip_parts(ShellAction::Undo, ready, false, echo).unavailable,
            None
        );
    }

    #[test]
    fn every_tooltip_key_exists_in_both_catalogs() {
        use crate::toolbar::ToolKind as T;
        let all = [
            ShellAction::NewProject,
            ShellAction::OpenMesh,
            ShellAction::OpenProject,
            ShellAction::SaveProject,
            ShellAction::SaveProjectAs,
            ShellAction::Undo,
            ShellAction::Redo,
            ShellAction::ToggleWireframe,
            ShellAction::ToggleGrid,
            ShellAction::CycleViewMode,
            ShellAction::ToggleMaskMode,
            ShellAction::InvertMask,
            ShellAction::SelectTool(T::Paint),
            ShellAction::BrushSmaller,
            ShellAction::BrushLarger,
            ShellAction::CommandPalette,
            ShellAction::Preferences,
            ShellAction::ToggleHideUi,
            ShellAction::ToggleMaximize,
            ShellAction::NextWorkspace,
            ShellAction::PrevWorkspace,
            ShellAction::ViewMaterial,
            ShellAction::NextChannel,
            ShellAction::PrevChannel,
            ShellAction::NextMeshMap,
            ShellAction::PrevMeshMap,
        ];
        // The real Fluent bundles, not a hand-split of the .ftl text
        // (CRLF on a Windows checkout broke that, 979a523).
        let i18n = crate::i18n::I18n::builtin();
        for a in all {
            let (t, d) = tooltip_keys(a);
            for key in std::iter::once(t).chain(d) {
                assert!(i18n.has_key("en-US", key), "en-US missing {key}");
                assert!(i18n.has_key("pt-BR", key), "pt-BR missing {key}");
            }
        }
    }

    #[test]
    fn no_new_description_hard_codes_a_shortcut() {
        // The tip.* strings are this kill's: none may carry a key in
        // parentheses (the registry prints it).
        // Normalised: a Windows checkout embeds CRLF.
        let en = include_str!("../../../assets/i18n/en-US.ftl").replace("\r\n", "\n");
        let block = en.split("\ntip =\n").nth(1).expect("tip block");
        for line in block
            .lines()
            .take_while(|l| l.is_empty() || l.starts_with(' '))
        {
            assert!(!line.contains("(Ctrl") && !line.contains("Ctrl+"), "{line}");
        }
    }
}
