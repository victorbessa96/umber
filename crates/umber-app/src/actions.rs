//! The action registry (GUI plan G0, `docs/specs/gui-plan-2026-10-15.md`
//! kill `g0-action-registry`): ONE table of shell actions and their key
//! bindings that the shortcut poll, the menus and the toolbar all read.
//!
//! # Why one table
//!
//! A shortcut printed next to a menu item must be the one that fires
//! (photocraft `shortcut_dispatch.rs`, Blender's every-menu-item-shows-its-
//! key, `docs/research/gui-research-*-2026-10-15.md` PC:#3 / B:D20). Before
//! this module the key table lived as a `match` inside
//! `toolbar::resolve_shortcut` and the menus printed no shortcuts at all. Now:
//!
//! - [`BINDINGS`] is the table: `(Binding, ShellAction)` rows, most
//!   modifiers first, so a more specific chord (Ctrl+Shift+Z) is matched
//!   before the plainer one (Ctrl+Z). The guard test pins the ordering.
//! - [`resolve`] walks it; [`shortcut_label`] formats a row for a menu or
//!   a tooltip through egui's own `KeyboardShortcut::format` (Ctrl on
//!   Windows/Linux, ⌘ on macOS).
//! - [`availability`] is the one enabled-check: `Err(reason key)` when an
//!   action cannot run right now. The toolbar greys from it today; the
//!   focus-aware dispatch kill (`g0-focus-dispatch`) will print the reason
//!   when a shortcut hits a disabled action.
//!
//! # Modifier semantics (unchanged from the toolbar's table)
//!
//! - [`Chord::Bare`]: the letter keys. They need no Ctrl/Cmd (so Ctrl+X /
//!   Ctrl+V never flip the mask or the view). Shift and Alt are ignored, as
//!   before.
//! - [`Chord::Cmd`] / [`Chord::CmdShift`]: Ctrl (Cmd on macOS) with Shift
//!   exactly as written. Alt+chord is reserved: it never matches.
//!
//! Every pre-existing toolbar test passes unchanged through this table.

use egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};

use crate::toolbar::{ShellAction, ToolKind};

/// The modifier shape a binding needs (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chord {
    /// No Ctrl/Cmd; Shift and Alt ignored.
    Bare,
    /// Ctrl/Cmd, no Shift, no Alt.
    Cmd,
    /// Ctrl/Cmd + Shift, no Alt.
    CmdShift,
}

impl Chord {
    /// Whether `mods` satisfy this chord.
    pub fn matches(self, mods: Modifiers) -> bool {
        match self {
            // On macOS `command` is Cmd and `ctrl` is the separate Control
            // key; a held Control is still not a bare press.
            Chord::Bare => !mods.command && !mods.ctrl,
            Chord::Cmd => mods.command && !mods.alt && !mods.shift,
            Chord::CmdShift => mods.command && !mods.alt && mods.shift,
        }
    }

    /// The egui modifiers a menu label prints for this chord.
    fn modifiers(self) -> Modifiers {
        match self {
            Chord::Bare => Modifiers::NONE,
            Chord::Cmd => Modifiers::COMMAND,
            Chord::CmdShift => Modifiers {
                command: true,
                shift: true,
                ..Modifiers::NONE
            },
        }
    }
}

/// One key binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// The key.
    pub key: Key,
    /// The modifier shape.
    pub chord: Chord,
}

const fn bind(key: Key, chord: Chord) -> Binding {
    Binding { key, chord }
}

/// The shell's key table: most modifiers first (see the module docs).
/// Twelve bindings: four chords + six letter keys + the `[` / `]` brush
/// size steps. Ctrl+N stays unbound in v1
/// (the toolbar module's judgment call), so `NewProject` has no row.
pub const BINDINGS: &[(Binding, ShellAction)] = &[
    (bind(Key::Z, Chord::CmdShift), ShellAction::Redo),
    (bind(Key::Z, Chord::Cmd), ShellAction::Undo),
    (bind(Key::S, Chord::Cmd), ShellAction::SaveProject),
    (bind(Key::O, Chord::Cmd), ShellAction::OpenProject),
    (bind(Key::W, Chord::Bare), ShellAction::ToggleWireframe),
    (bind(Key::G, Chord::Bare), ShellAction::ToggleGrid),
    (bind(Key::V, Chord::Bare), ShellAction::CycleViewMode),
    (bind(Key::X, Chord::Bare), ShellAction::ToggleMaskMode),
    (bind(Key::I, Chord::Bare), ShellAction::InvertMask),
    (
        bind(Key::E, Chord::Bare),
        ShellAction::SelectTool(ToolKind::Erase),
    ),
    (
        bind(Key::OpenBracket, Chord::Bare),
        ShellAction::BrushSmaller,
    ),
    (
        bind(Key::CloseBracket, Chord::Bare),
        ShellAction::BrushLarger,
    ),
];

/// The action `key` + `mods` fire, if any: the first matching row.
pub fn resolve(key: Key, mods: Modifiers) -> Option<ShellAction> {
    BINDINGS
        .iter()
        .find(|(binding, _)| binding.key == key && binding.chord.matches(mods))
        .map(|(_, action)| *action)
}

/// The first binding for `action`, if it has one.
pub fn binding_of(action: ShellAction) -> Option<Binding> {
    BINDINGS
        .iter()
        .find(|(_, bound)| *bound == action)
        .map(|(binding, _)| *binding)
}

/// `action`'s shortcut as a menu / tooltip label ("Ctrl+Shift+Z" off
/// macOS, "⇧⌘Z" on it), or `None` when the action is unbound.
pub fn shortcut_label(action: ShellAction, is_mac: bool) -> Option<String> {
    let binding = binding_of(action)?;
    let names = if is_mac {
        ModifierNames::SYMBOLS
    } else {
        ModifierNames::NAMES
    };
    Some(KeyboardShortcut::new(binding.chord.modifiers(), binding.key).format(&names, is_mac))
}

/// What [`availability`] needs to know about the app. A snapshot taken
/// before drawing, like `toolbar::ToolbarView`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShellState {
    /// The document has an edit to undo.
    pub can_undo: bool,
    /// The document has an edit to redo.
    pub can_redo: bool,
    /// A paint session exists (a mesh is loaded).
    pub paint_available: bool,
    /// The quick mask is being edited.
    pub mask_editing: bool,
}

/// Whether `action` can run in `state`: `Err(i18n key)` names why not.
/// Actions with no precondition are always `Ok`.
pub fn availability(action: ShellAction, state: ShellState) -> Result<(), &'static str> {
    match action {
        ShellAction::Undo if !state.can_undo => Err("action.unavailable-nothing-to-undo"),
        ShellAction::Redo if !state.can_redo => Err("action.unavailable-nothing-to-redo"),
        ShellAction::ToggleMaskMode if !state.paint_available => Err("action.unavailable-no-mesh"),
        ShellAction::InvertMask if !state.mask_editing => {
            Err("action.unavailable-not-editing-mask")
        }
        ShellAction::BrushSmaller | ShellAction::BrushLarger if !state.paint_available => {
            Err("action.unavailable-no-mesh")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many modifiers a chord names: the table's sort key.
    fn weight(chord: Chord) -> u8 {
        match chord {
            Chord::Bare => 0,
            Chord::Cmd => 1,
            Chord::CmdShift => 2,
        }
    }

    #[test]
    fn table_is_sorted_most_modifiers_first() {
        for pair in BINDINGS.windows(2) {
            assert!(
                weight(pair[0].0.chord) >= weight(pair[1].0.chord),
                "{:?} before {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn no_two_rows_share_a_binding() {
        for (i, (a, _)) in BINDINGS.iter().enumerate() {
            for (b, _) in &BINDINGS[i + 1..] {
                assert_ne!(a, b, "duplicate binding");
            }
        }
    }

    #[test]
    fn every_row_resolves_to_itself() {
        // Each row fires its own action under its own modifiers, i.e. no
        // earlier row shadows it.
        for (binding, action) in BINDINGS {
            let mods = binding.chord.modifiers();
            // Modifiers::COMMAND sets `command` only; egui on Linux /
            // Windows sets `ctrl` too, which the chords accept.
            assert_eq!(resolve(binding.key, mods), Some(*action), "{binding:?}");
        }
    }

    #[test]
    fn labels_print_the_platform_names() {
        assert_eq!(
            shortcut_label(ShellAction::Redo, false).as_deref(),
            Some("Ctrl+Shift+Z")
        );
        assert_eq!(
            shortcut_label(ShellAction::Undo, false).as_deref(),
            Some("Ctrl+Z")
        );
        assert_eq!(
            shortcut_label(ShellAction::ToggleWireframe, false).as_deref(),
            Some("W")
        );
        let mac = shortcut_label(ShellAction::Redo, true).expect("bound");
        assert!(mac.contains('⌘') && mac.contains('⇧'), "{mac}");
        assert_eq!(shortcut_label(ShellAction::NewProject, false), None);
        assert_eq!(shortcut_label(ShellAction::OpenMesh, false), None);
    }

    #[test]
    fn availability_names_the_reason() {
        let none = ShellState::default();
        assert_eq!(
            availability(ShellAction::Undo, none),
            Err("action.unavailable-nothing-to-undo")
        );
        assert_eq!(
            availability(ShellAction::Redo, none),
            Err("action.unavailable-nothing-to-redo")
        );
        assert_eq!(
            availability(ShellAction::ToggleMaskMode, none),
            Err("action.unavailable-no-mesh")
        );
        assert_eq!(
            availability(ShellAction::InvertMask, none),
            Err("action.unavailable-not-editing-mask")
        );
        for always in [
            ShellAction::NewProject,
            ShellAction::OpenMesh,
            ShellAction::OpenProject,
            ShellAction::SaveProject,
            ShellAction::ToggleWireframe,
            ShellAction::ToggleGrid,
            ShellAction::CycleViewMode,
            ShellAction::SelectTool(ToolKind::Paint),
        ] {
            assert_eq!(availability(always, none), Ok(()), "{always:?}");
        }
        let all = ShellState {
            can_undo: true,
            can_redo: true,
            paint_available: true,
            mask_editing: true,
        };
        for action in [
            ShellAction::Undo,
            ShellAction::Redo,
            ShellAction::ToggleMaskMode,
            ShellAction::InvertMask,
        ] {
            assert_eq!(availability(action, all), Ok(()), "{action:?}");
        }
    }

    #[test]
    fn every_reason_key_is_in_the_catalog() {
        let catalog = include_str!("../../../assets/i18n/en-US.ftl");
        for key in [
            "unavailable-nothing-to-undo",
            "unavailable-nothing-to-redo",
            "unavailable-no-mesh",
            "unavailable-not-editing-mask",
        ] {
            assert!(catalog.contains(&format!(".{key} =")), "{key}");
        }
    }
}
