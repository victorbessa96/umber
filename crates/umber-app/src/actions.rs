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
//!
//! # Focus-aware dispatch (kill `g0-focus-dispatch`)
//!
//! [`Focus`] says who owns the keyboard this frame and [`Focus::allows`]
//! decides whether a binding may fire through it (photocraft PC:#3):
//!
//! - [`Focus::None`] / [`Focus::Widget`]: every binding fires. A focused
//!   button or slider owns arrows, Space, Enter and Tab, none of which the
//!   table binds.
//! - [`Focus::Text`]: a text field keeps its keys. Only Ctrl/Cmd chords and
//!   the F-keys get through, so typing "wedge" into a layer name never
//!   flips the wireframe or picks the eraser. Ctrl+Z / Ctrl+Shift+Z stay
//!   with the field too: every text field undoes its own typing (egui's
//!   `TextEdit` binds both), and a document undo there would throw away
//!   paint while the user meant to fix a typo.
//!
//! `toolbar::poll_shortcuts` consumes every key event it dispatches, so a
//! panel drawn later in the frame never sees the same press twice.

use egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};

use crate::toolbar::{ShellAction, ToolKind};

/// The modifier shape a binding needs (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chord {
    /// No Ctrl/Cmd; Shift and Alt ignored.
    Bare,
    /// Shift, no Ctrl/Cmd, no Alt (Shift+C / Shift+B, the display-mode
    /// keys). Sorted before [`Chord::Bare`], which ignores Shift.
    Shift,
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
            Chord::Shift => mods.shift && !mods.command && !mods.ctrl && !mods.alt,
            Chord::Cmd => mods.command && !mods.alt && !mods.shift,
            Chord::CmdShift => mods.command && !mods.alt && mods.shift,
        }
    }

    /// The egui modifiers a menu label prints for this chord.
    fn modifiers(self) -> Modifiers {
        match self {
            Chord::Bare => Modifiers::NONE,
            Chord::Shift => Modifiers::SHIFT,
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
/// The bindings: nine chords (incl. Ctrl+Shift+P the palette,
/// Ctrl+, Preferences, Ctrl+Space maximize, Ctrl+PgUp/PgDn workspaces) +
/// six letter keys + the `[` /
/// `]` brush size steps + F3 + Tab (Hide UI). Ctrl+N stays unbound in v1
/// (the toolbar module's judgment call), so `NewProject` has no row.
pub const BINDINGS: &[(Binding, ShellAction)] = &[
    (bind(Key::Z, Chord::CmdShift), ShellAction::Redo),
    (bind(Key::P, Chord::CmdShift), ShellAction::CommandPalette),
    (bind(Key::S, Chord::CmdShift), ShellAction::SaveProjectAs),
    (bind(Key::Z, Chord::Cmd), ShellAction::Undo),
    (bind(Key::S, Chord::Cmd), ShellAction::SaveProject),
    (bind(Key::O, Chord::Cmd), ShellAction::OpenProject),
    (bind(Key::Comma, Chord::Cmd), ShellAction::Preferences),
    (bind(Key::Space, Chord::Cmd), ShellAction::ToggleMaximize),
    (bind(Key::PageDown, Chord::Cmd), ShellAction::NextWorkspace),
    (bind(Key::PageUp, Chord::Cmd), ShellAction::PrevWorkspace),
    // The display-mode Shift rows (GUI plan g6-display-mode-keys) sit
    // above every bare row: Chord::Bare ignores Shift, so a bare C row
    // first would swallow Shift+C.
    (bind(Key::C, Chord::Shift), ShellAction::PrevChannel),
    (bind(Key::B, Chord::Shift), ShellAction::PrevMeshMap),
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
    (bind(Key::F3, Chord::Bare), ShellAction::CommandPalette),
    (bind(Key::Tab, Chord::Bare), ShellAction::ToggleHideUi),
    (bind(Key::M, Chord::Bare), ShellAction::ViewMaterial),
    (bind(Key::C, Chord::Bare), ShellAction::NextChannel),
    (bind(Key::B, Chord::Bare), ShellAction::NextMeshMap),
];

/// The action `key` + `mods` fire, if any: the first matching row.
#[cfg_attr(not(test), allow(dead_code))] // the poll uses `resolve_binding`
pub fn resolve(key: Key, mods: Modifiers) -> Option<ShellAction> {
    resolve_binding(key, mods).map(|(_, action)| action)
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

/// What a binding does, in the words of the keyboard reference
/// (`docs/user/keyboard.md`). English on purpose: the reference is a
/// repository document, and the UI's own labels are the catalogs' job.
#[cfg_attr(not(test), allow(dead_code))] // generator + its test only
fn reference_text(action: ShellAction) -> &'static str {
    match action {
        ShellAction::Undo => "Undo",
        ShellAction::Redo => "Redo",
        ShellAction::SaveProject => "Save project (asks for a folder the first time)",
        ShellAction::SaveProjectAs => "Save project as (always asks)",
        ShellAction::OpenProject => "Open project",
        ShellAction::CommandPalette => "Command palette",
        ShellAction::Preferences => "Preferences",
        ShellAction::ToggleMaximize => "Maximize the hovered area",
        ShellAction::NextWorkspace => "Next workspace",
        ShellAction::PrevWorkspace => "Previous workspace",
        ShellAction::ToggleWireframe => "Toggle wireframe",
        ShellAction::ToggleGrid => "Toggle grid",
        ShellAction::CycleViewMode => "Cycle lit / unlit view",
        ShellAction::ToggleMaskMode => "Toggle mask painting",
        ShellAction::InvertMask => "Invert the mask",
        ShellAction::SelectTool(ToolKind::Erase) => "Eraser tool",
        ShellAction::BrushSmaller => "Brush smaller",
        ShellAction::BrushLarger => "Brush larger",
        ShellAction::ToggleHideUi => "Hide / show the UI",
        ShellAction::ViewMaterial => "Material view",
        ShellAction::NextChannel => "Next channel view",
        ShellAction::PrevChannel => "Previous channel view",
        ShellAction::NextMeshMap => "Next mesh-map view",
        ShellAction::PrevMeshMap => "Previous mesh-map view",
        _ => "",
    }
}

/// Where a binding fires from (the [`Focus::allows`] rule, in words).
#[cfg_attr(not(test), allow(dead_code))]
fn reference_scope(binding: Binding) -> &'static str {
    if binding.key == Key::Tab && binding.chord == Chord::Bare {
        "nothing focused"
    } else if Focus::Text.allows(binding) {
        "everywhere, even while typing"
    } else {
        "not while typing in a field"
    }
}

/// The keyboard reference (`docs/user/keyboard.md`), generated from
/// [`BINDINGS`] so the document cannot drift from the keys that fire:
/// `keyboard_reference_is_current` regenerates it and compares. Both
/// platform spellings are printed (egui's own formatter, the one the
/// menus use).
#[cfg_attr(not(test), allow(dead_code))]
pub fn keyboard_reference() -> String {
    let mut out = String::from(
        "# Keyboard reference\n\n\
         Generated from `crates/umber-app/src/actions.rs` (`BINDINGS`): do not edit by\n\
         hand. Regenerate with\n\
         `UMBER_REGEN_KEYBOARD_DOC=1 cargo test -p umber-app --bin umber keyboard_reference`.\n\n\
         Letter keys need no Ctrl/Cmd, so Ctrl+X and Ctrl+V are never these actions. While a\n\
         text field has focus it keeps its own keys: only Ctrl/Cmd chords and F-keys reach the\n\
         shell, and Ctrl+Z undoes the typing, not the document.\n\n\
         | Windows / Linux | macOS | Action | Fires |\n\
         |---|---|---|---|\n",
    );
    for (binding, action) in BINDINGS {
        let label = |mac: bool| {
            let names = if mac {
                ModifierNames::SYMBOLS
            } else {
                ModifierNames::NAMES
            };
            let text =
                KeyboardShortcut::new(binding.chord.modifiers(), binding.key).format(&names, mac);
            // egui spells punctuation by name off macOS ("OpenBracket");
            // a reader wants the key cap.
            text.replace("OpenBracket", "[")
                .replace("CloseBracket", "]")
                .replace("Comma", ",")
        };
        let text = reference_text(*action);
        let text = if text.is_empty() {
            "(undocumented)"
        } else {
            text
        };
        // `|` would end a markdown cell; none of today's keys print one.
        let cell = |s: String| s.replace('|', "\\|");
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            cell(label(false)),
            cell(label(true)),
            text,
            reference_scope(*binding)
        ));
    }
    out
}

/// Who owns the keyboard this frame (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Nothing is focused: the shell owns the keyboard.
    None,
    /// A non-text widget (a button, a slider) has keyboard focus.
    Widget,
    /// A text field is being edited.
    Text,
}

impl Focus {
    /// The focus state from egui's memory (last frame's focus, which is
    /// what this frame's key events were typed into).
    pub fn current(ctx: &egui::Context) -> Self {
        Self::from_flags(ctx.egui_wants_keyboard_input(), ctx.text_edit_focused())
    }

    /// [`Focus::current`]'s mapping, separated so it is testable without a
    /// context: a focused text edit is [`Focus::Text`] whatever else holds.
    pub fn from_flags(widget_focused: bool, text_focused: bool) -> Self {
        if text_focused {
            Focus::Text
        } else if widget_focused {
            Focus::Widget
        } else {
            Focus::None
        }
    }

    /// Whether `binding` may fire while this focus holds the keyboard.
    pub fn allows(self, binding: Binding) -> bool {
        // Bare Tab is egui's focus traversal whenever anything is focused:
        // it only means Hide UI with nothing focused (and `main.rs` takes
        // it out of the input before egui sees it, see `shell_menu`).
        if binding.key == Key::Tab && binding.chord == Chord::Bare && self != Focus::None {
            return false;
        }
        match self {
            Focus::None | Focus::Widget => true,
            Focus::Text => {
                // Only Ctrl/Cmd chords are commands inside a text field:
                // Shift+C is a capital C being typed.
                let command_chord = matches!(binding.chord, Chord::Cmd | Chord::CmdShift);
                is_function_key(binding.key) || (command_chord && binding.key != Key::Z)
            }
        }
    }
}

/// F1..F35: the keys a text field never types, so they pass through it.
pub fn is_function_key(key: Key) -> bool {
    let name = key.name();
    name.len() > 1 && name.starts_with('F') && name[1..].bytes().all(|b| b.is_ascii_digit())
}

/// The first row matching `key` + `mods`, with its binding.
pub fn resolve_binding(key: Key, mods: Modifiers) -> Option<(Binding, ShellAction)> {
    BINDINGS
        .iter()
        .find(|(binding, _)| binding.key == key && binding.chord.matches(mods))
        .copied()
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

    #[test]
    fn every_binding_has_reference_text() {
        for (binding, action) in BINDINGS {
            assert!(
                !reference_text(*action).is_empty(),
                "{binding:?} -> {action:?} needs a line in the keyboard reference"
            );
        }
    }

    #[test]
    fn keyboard_reference_is_current() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user/keyboard.md");
        let fresh = keyboard_reference();
        if std::env::var_os("UMBER_REGEN_KEYBOARD_DOC").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            committed == fresh,
            "docs/user/keyboard.md is stale; regenerate with UMBER_REGEN_KEYBOARD_DOC=1"
        );
        // One row per binding, and both spellings of the redo chord.
        assert_eq!(
            fresh.lines().filter(|l| l.starts_with("| `")).count(),
            BINDINGS.len()
        );
        assert!(fresh.contains("`Ctrl+Shift+Z`"), "{fresh}");
        assert!(fresh.contains("not while typing in a field"));
        assert!(fresh.contains("everywhere, even while typing"));
    }

    use super::*;

    /// How many modifiers a chord names: the table's sort key.
    fn weight(chord: Chord) -> u8 {
        match chord {
            Chord::Bare => 0,
            Chord::Shift => 1,
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
    fn focus_maps_from_the_flags() {
        assert_eq!(Focus::from_flags(false, false), Focus::None);
        assert_eq!(Focus::from_flags(true, false), Focus::Widget);
        assert_eq!(Focus::from_flags(true, true), Focus::Text);
        // A text edit is text focus even if the widget flag lags.
        assert_eq!(Focus::from_flags(false, true), Focus::Text);
    }

    #[test]
    fn text_focus_keeps_letters_and_undo_but_passes_other_chords() {
        for (binding, action) in BINDINGS {
            assert!(Focus::None.allows(*binding), "{action:?}");
            // A focused widget keeps bare Tab (focus traversal) only.
            let hide_ui = *action == ShellAction::ToggleHideUi;
            assert_eq!(Focus::Widget.allows(*binding), !hide_ui, "{action:?}");
            let through_text = Focus::Text.allows(*binding);
            // Ctrl+S / Ctrl+O, and the palette (Ctrl+Shift+P and F3) pass a
            // text field: global commands, never text input. Everything
            // else (letters, Ctrl+Z undo) stays with the field.
            let expected = matches!(
                action,
                ShellAction::SaveProject
                    | ShellAction::SaveProjectAs
                    | ShellAction::OpenProject
                    | ShellAction::CommandPalette
                    | ShellAction::Preferences
                    | ShellAction::ToggleMaximize
                    | ShellAction::NextWorkspace
                    | ShellAction::PrevWorkspace
            );
            assert_eq!(through_text, expected, "{action:?} through a text field");
        }
        // The F-keys pass a text field even bare (F3 opens the palette).
        assert!(Focus::Text.allows(bind(Key::F2, Chord::Bare)));
        assert!(!Focus::Text.allows(bind(Key::A, Chord::Bare)));
    }

    #[test]
    fn shift_rows_resolve_and_stay_out_of_text_fields() {
        let shift = Modifiers::SHIFT;
        assert_eq!(resolve(Key::C, shift), Some(ShellAction::PrevChannel));
        assert_eq!(resolve(Key::B, shift), Some(ShellAction::PrevMeshMap));
        assert_eq!(
            resolve(Key::C, Modifiers::NONE),
            Some(ShellAction::NextChannel)
        );
        // Shift+Alt is not the Shift chord: it falls through to the bare
        // row, which ignores Shift and Alt by contract (module docs).
        assert_eq!(
            resolve(Key::C, Modifiers::SHIFT | Modifiers::ALT),
            Some(ShellAction::NextChannel)
        );
        // Ctrl+Shift+C is neither (no Cmd row for C).
        assert_eq!(resolve(Key::C, Modifiers::SHIFT | Modifiers::COMMAND), None);
        // Typing a capital C into a text field is text, not a command.
        let shift_c = binding_of(ShellAction::PrevChannel).expect("bound");
        assert!(!Focus::Text.allows(shift_c));
        assert!(Focus::None.allows(shift_c));
    }

    #[test]
    fn function_keys_are_exactly_f1_to_f35() {
        assert!(is_function_key(Key::F1));
        assert!(is_function_key(Key::F12));
        assert!(is_function_key(Key::F35));
        for key in [Key::F, Key::A, Key::Escape, Key::OpenBracket, Key::Num1] {
            assert!(!is_function_key(key), "{key:?}");
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
