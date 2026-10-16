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
//!
//! # User overrides (kill `g3-keymap-editor`, `docs/specs/gui-plan-2026-10-15.md`)
//!
//! [`BINDINGS`] stays the DEFAULT table (the keyboard doc and the
//! toolbar's poll-list guard pin it). The user's changes are an
//! [`Overrides`] delta: per action, the full list of its bindings (at most
//! [`MAX_BINDINGS`]: primary + alternate; empty = unbound). An override
//! for action X replaces EVERY default row of X; actions without one keep
//! their defaults. The delta holds only what differs from the defaults
//! ([`Overrides::set`] drops an entry equal to them), so exporting it is
//! exporting "the changed bindings".
//!
//! The effective table ([`table_for`]: defaults minus overridden actions,
//! plus the override rows, stable-sorted most-modifiers-first so a Shift
//! row still precedes the Bare row it would otherwise lose to) is cached
//! per UI thread ([`set_overrides`], like `i18n`'s current catalog). Every
//! consumer funnels through that one cache: [`binding_of`],
//! [`resolve_binding`] (the poll), [`shortcut_label`] (menus, tooltips,
//! status hints), [`shortcut_rows`] (the Help ▸ Keyboard window). Tests
//! run on their own threads, so they see the defaults unless they install
//! overrides themselves.
//!
//! Persistence (`preferences.rs`) is one line,
//! `keymap=<action>:<spec>[,<spec>];...`: the action is the `ShellAction`
//! Debug name (`Undo`, `SelectTool(Erase)`), a spec is
//! [`binding_spec`]'s platform-neutral spelling (`ctrl+shift+z`, `tab`).
//! The editor refuses an assignment that would give two actions the same
//! binding ([`conflict_in`]); a pasted / hand-edited line is checked with
//! [`first_conflict`].

use std::cell::RefCell;

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
pub fn resolve(key: Key, mods: Modifiers) -> Option<ShellAction> {
    resolve_binding(key, mods).map(|(_, action)| action)
}

/// The first binding for `action` in the effective table (overrides
/// applied, see the module docs), if it has one.
pub fn binding_of(action: ShellAction) -> Option<Binding> {
    with_table(|table| {
        table
            .iter()
            .find(|(_, bound)| *bound == action)
            .map(|(binding, _)| *binding)
    })
}

/// `action`'s shortcut as a menu / tooltip label ("Ctrl+Shift+Z" off
/// macOS, "⇧⌘Z" on it), or `None` when the action is unbound.
pub fn shortcut_label(action: ShellAction, is_mac: bool) -> Option<String> {
    binding_of(action).map(|binding| binding_label(binding, is_mac))
}

/// `binding` in this platform's spelling (egui's own formatter).
pub fn binding_label(binding: Binding, is_mac: bool) -> String {
    let names = if is_mac {
        ModifierNames::SYMBOLS
    } else {
        ModifierNames::NAMES
    };
    KeyboardShortcut::new(binding.chord.modifiers(), binding.key).format(&names, is_mac)
}

/// [`binding_label`] with the punctuation keys printed as their caps
/// ("[" not "OpenBracket"): the Help window's and the keymap editor's
/// spelling.
pub fn key_cap_label(binding: Binding, is_mac: bool) -> String {
    binding_label(binding, is_mac)
        .replace("OpenBracket", "[")
        .replace("CloseBracket", "]")
        .replace("Comma", ",")
}

// ---------------------------------------------------------------------
// User overrides (kill g3-keymap-editor; see the module docs).
// ---------------------------------------------------------------------

/// Most bindings per action: a primary and an alternate (the editor's
/// two binding columns).
pub const MAX_BINDINGS: usize = 2;

/// Every shell action, in the keymap editor's row order and the persisted
/// line's order. `every_action_is_listed` pins it exhaustive.
pub const ALL_ACTIONS: &[ShellAction] = &[
    ShellAction::NewProject,
    ShellAction::OpenMesh,
    ShellAction::OpenProject,
    ShellAction::SaveProject,
    ShellAction::SaveProjectAs,
    ShellAction::Undo,
    ShellAction::Redo,
    ShellAction::CommandPalette,
    ShellAction::Preferences,
    ShellAction::ToggleWireframe,
    ShellAction::ToggleGrid,
    ShellAction::CycleViewMode,
    ShellAction::ViewMaterial,
    ShellAction::NextChannel,
    ShellAction::PrevChannel,
    ShellAction::NextMeshMap,
    ShellAction::PrevMeshMap,
    ShellAction::ToggleMaskMode,
    ShellAction::InvertMask,
    ShellAction::SelectTool(ToolKind::Paint),
    ShellAction::SelectTool(ToolKind::Smudge),
    ShellAction::SelectTool(ToolKind::Clone),
    ShellAction::SelectTool(ToolKind::Erase),
    ShellAction::SelectTool(ToolKind::Pick),
    ShellAction::SelectTool(ToolKind::IdMask),
    ShellAction::BrushSmaller,
    ShellAction::BrushLarger,
    ShellAction::ToggleHideUi,
    ShellAction::ToggleMaximize,
    ShellAction::NextWorkspace,
    ShellAction::PrevWorkspace,
];

/// The persisted name of `action`: the variant's Debug spelling
/// (`Undo`, `SelectTool(Erase)`).
pub fn action_name(action: ShellAction) -> String {
    format!("{action:?}")
}

/// The action [`action_name`] printed as `name`.
pub fn action_from_name(name: &str) -> Option<ShellAction> {
    let name = name.trim();
    ALL_ACTIONS
        .iter()
        .copied()
        .find(|action| action_name(*action) == name)
}

/// What a binding does in the keyboard reference's English words (a pub
/// accessor over the reference table; empty for undocumented actions).
pub fn reference_label(action: ShellAction) -> &'static str {
    reference_text(action)
}

/// `binding` in the persisted, platform-neutral spelling: what
/// `KeyboardShortcut::format` prints off macOS, lowercased
/// (`ctrl+shift+z`, `shift+c`, `tab`, `openbracket`).
pub fn binding_spec(binding: Binding) -> String {
    let mods = match binding.chord {
        Chord::Bare => "",
        Chord::Shift => "shift+",
        Chord::Cmd => "ctrl+",
        Chord::CmdShift => "ctrl+shift+",
    };
    format!("{mods}{}", binding.key.name().to_lowercase())
}

/// Parses a [`binding_spec`] (case-insensitive; `cmd` / `command` are
/// accepted for `ctrl`). `None` for an unknown key or a modifier no
/// [`Chord`] expresses (Alt).
pub fn parse_binding_spec(spec: &str) -> Option<Binding> {
    let spec = spec.trim();
    let (mods, key) = spec.rsplit_once('+').unwrap_or(("", spec));
    let key = Key::ALL
        .iter()
        .copied()
        .find(|k| k.name().eq_ignore_ascii_case(key.trim()))?;
    let (mut cmd, mut shift) = (false, false);
    for part in mods.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => cmd = true,
            "shift" => shift = true,
            _ => return None,
        }
    }
    let chord = match (cmd, shift) {
        (false, false) => Chord::Bare,
        (false, true) => Chord::Shift,
        (true, false) => Chord::Cmd,
        (true, true) => Chord::CmdShift,
    };
    Some(bind(key, chord))
}

/// `action`'s rows in the default [`BINDINGS`] table, in table order.
pub fn default_bindings(action: ShellAction) -> Vec<Binding> {
    BINDINGS
        .iter()
        .filter(|(_, bound)| *bound == action)
        .map(|(binding, _)| *binding)
        .collect()
}

/// The user's keymap delta (see the module docs): per overridden action,
/// its full binding list. Kept in [`ALL_ACTIONS`] order and holding only
/// entries that differ from the defaults, so equal deltas compare equal
/// and serialize identically.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    entries: Vec<(ShellAction, Vec<Binding>)>,
}

fn action_rank(action: ShellAction) -> usize {
    ALL_ACTIONS
        .iter()
        .position(|a| *a == action)
        .unwrap_or(usize::MAX)
}

impl Overrides {
    /// No action is overridden.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `action`'s override, if it has one (an empty slice = unbound).
    pub fn get(&self, action: ShellAction) -> Option<&[Binding]> {
        self.entries
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, b)| b.as_slice())
    }

    /// `action`'s bindings with this delta applied (override or defaults).
    pub fn bindings(&self, action: ShellAction) -> Vec<Binding> {
        self.get(action)
            .map_or_else(|| default_bindings(action), <[Binding]>::to_vec)
    }

    /// Gives `action` exactly `bindings` (duplicates dropped, capped at
    /// [`MAX_BINDINGS`]). Equal to the defaults = the override is removed.
    pub fn set(&mut self, action: ShellAction, bindings: Vec<Binding>) {
        let mut list: Vec<Binding> = Vec::with_capacity(MAX_BINDINGS);
        for b in bindings {
            if !list.contains(&b) && list.len() < MAX_BINDINGS {
                list.push(b);
            }
        }
        self.reset(action);
        if list == default_bindings(action) {
            return;
        }
        let rank = action_rank(action);
        let at = self
            .entries
            .iter()
            .position(|(a, _)| action_rank(*a) > rank)
            .unwrap_or(self.entries.len());
        self.entries.insert(at, (action, list));
    }

    /// Back to `action`'s default bindings.
    pub fn reset(&mut self, action: ShellAction) {
        self.entries.retain(|(a, _)| *a != action);
    }

    /// The overridden actions, in [`ALL_ACTIONS`] order.
    pub fn iter(&self) -> impl Iterator<Item = (ShellAction, &[Binding])> {
        self.entries.iter().map(|(a, b)| (*a, b.as_slice()))
    }

    /// The value of the persisted `keymap=` line:
    /// `Action:spec,spec;Action:spec` (`Action:` alone = unbound).
    pub fn serialize(&self) -> String {
        self.entries
            .iter()
            .map(|(action, bindings)| {
                let specs: Vec<String> = bindings.iter().map(|b| binding_spec(*b)).collect();
                format!("{}:{}", action_name(*action), specs.join(","))
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Parses [`Overrides::serialize`]'s text leniently: an entry naming
    /// an unknown action is skipped, an unknown spec is dropped from its
    /// entry (an entry left with no valid spec out of some is skipped, so
    /// a typo never unbinds an action).
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for entry in text.split(';') {
            let Some((name, specs)) = entry.split_once(':') else {
                continue;
            };
            let Some(action) = action_from_name(name) else {
                continue;
            };
            let raw: Vec<&str> = specs
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let bindings: Vec<Binding> =
                raw.iter().copied().filter_map(parse_binding_spec).collect();
            if bindings.is_empty() && !raw.is_empty() {
                continue;
            }
            out.set(action, bindings);
        }
        out
    }
}

/// How many modifiers a chord names: the table's sort key (most first).
fn chord_weight(chord: Chord) -> u8 {
    match chord {
        Chord::Bare => 0,
        Chord::Shift | Chord::Cmd => 1,
        Chord::CmdShift => 2,
    }
}

/// The effective table for `overrides`: the default rows of every
/// non-overridden action plus the override rows, stable-sorted most
/// modifiers first (so a Shift row still wins over a Bare row).
pub fn table_for(overrides: &Overrides) -> Vec<(Binding, ShellAction)> {
    let mut table: Vec<(Binding, ShellAction)> = BINDINGS
        .iter()
        .filter(|(_, action)| overrides.get(*action).is_none())
        .copied()
        .collect();
    for (action, bindings) in overrides.iter() {
        table.extend(bindings.iter().map(|b| (*b, action)));
    }
    table.sort_by_key(|(b, _)| std::cmp::Reverse(chord_weight(b.chord)));
    table
}

thread_local! {
    /// The effective table on this (the UI) thread; `None` = [`BINDINGS`].
    static EFFECTIVE: RefCell<Option<Vec<(Binding, ShellAction)>>> = const { RefCell::new(None) };
}

/// Installs `overrides` as this thread's effective table: every
/// consumer ([`binding_of`], [`resolve_binding`], [`shortcut_label`],
/// [`shortcut_rows`]) reads it from here on.
pub fn set_overrides(overrides: &Overrides) {
    let table = (!overrides.is_empty()).then(|| table_for(overrides));
    EFFECTIVE.with(|t| *t.borrow_mut() = table);
}

/// Runs `f` over the effective table (the one funnel; module docs).
fn with_table<R>(f: impl FnOnce(&[(Binding, ShellAction)]) -> R) -> R {
    EFFECTIVE.with(|t| match t.borrow().as_deref() {
        Some(table) => f(table),
        None => f(BINDINGS),
    })
}

/// The action other than `action` that already owns `binding` in
/// `table`, if any: the editor refuses such an assignment.
pub fn conflict_in(
    table: &[(Binding, ShellAction)],
    action: ShellAction,
    binding: Binding,
) -> Option<ShellAction> {
    table
        .iter()
        .find(|(b, a)| *b == binding && *a != action)
        .map(|(_, a)| *a)
}

/// The first binding two actions share once `overrides` apply, as
/// `(binding, first action, second action)`.
pub fn first_conflict(overrides: &Overrides) -> Option<(Binding, ShellAction, ShellAction)> {
    let table = table_for(overrides);
    table
        .iter()
        .enumerate()
        .find_map(|(i, (b, a))| conflict_in(&table[i + 1..], *a, *b).map(|other| (*b, *a, other)))
}

/// What one key press means while the editor captures a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Assign this binding.
    Assign(Binding),
    /// Esc: stop capturing, change nothing.
    Cancel,
    /// Backspace: clear the slot (or restore the defaults of an
    /// overridden action).
    Clear,
    /// A shape no [`Chord`] expresses (Alt held; macOS Control without
    /// Cmd): keep capturing.
    Ignore,
}

/// The [`Capture`] for `key` pressed with `mods`: the shape → [`Chord`]
/// mapping (the inverse of [`Chord::matches`] for the four chords).
pub fn capture_shape(key: Key, mods: Modifiers) -> Capture {
    match key {
        Key::Escape => return Capture::Cancel,
        Key::Backspace => return Capture::Clear,
        _ => {}
    }
    if mods.alt {
        return Capture::Ignore;
    }
    let chord = if mods.command {
        if mods.shift {
            Chord::CmdShift
        } else {
            Chord::Cmd
        }
    } else if mods.ctrl {
        // macOS Control without Cmd: Bare refuses it and no chord takes it.
        return Capture::Ignore;
    } else if mods.shift {
        Chord::Shift
    } else {
        Chord::Bare
    };
    Capture::Assign(bind(key, chord))
}

/// The keymap presets (the editor's combo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapPreset {
    /// The default table: no overrides.
    Umber,
    /// [`blender_overrides`].
    Blender,
    /// Anything else the user built.
    Custom,
}

/// The Blender-like preset: only the rows [`BINDINGS`] owns that Blender
/// spells differently (viewport navigation is `NavPreset`'s job).
/// Blender's Shift+Z toggles wireframe, Z switches the shading mode
/// (here: lit / unlit), Ctrl+I inverts the mask. Undo / redo / save /
/// open / F3 / Ctrl+Space / `[` `]` / E already match Blender.
pub fn blender_overrides() -> Overrides {
    let mut o = Overrides::default();
    o.set(
        ShellAction::ToggleWireframe,
        vec![bind(Key::Z, Chord::Shift)],
    );
    o.set(ShellAction::CycleViewMode, vec![bind(Key::Z, Chord::Bare)]);
    o.set(ShellAction::InvertMask, vec![bind(Key::I, Chord::Cmd)]);
    o
}

impl KeymapPreset {
    /// The preset `overrides` equal, or [`KeymapPreset::Custom`].
    pub fn of(overrides: &Overrides) -> Self {
        if overrides.is_empty() {
            KeymapPreset::Umber
        } else if *overrides == blender_overrides() {
            KeymapPreset::Blender
        } else {
            KeymapPreset::Custom
        }
    }

    /// The overrides this preset installs (`None` for Custom).
    pub fn overrides(self) -> Option<Overrides> {
        match self {
            KeymapPreset::Umber => Some(Overrides::default()),
            KeymapPreset::Blender => Some(blender_overrides()),
            KeymapPreset::Custom => None,
        }
    }
}

/// Every bound action pressed this frame on a key `polled` does not hold
/// (`toolbar::SHORTCUT_KEYS` pre-filters the main poll to the DEFAULT
/// keys, so a key only an override binds is polled here), through the
/// same [`resolve_binding`] + [`Focus::allows`], consuming what fires.
/// No-op while no override is installed.
pub fn poll_override_keys(ctx: &egui::Context, polled: &[Key]) -> Vec<ShellAction> {
    let extra: Vec<Key> = EFFECTIVE.with(|t| {
        t.borrow().as_deref().map_or_else(Vec::new, |table| {
            table
                .iter()
                .map(|(b, _)| b.key)
                .filter(|k| !polled.contains(k))
                .collect()
        })
    });
    if extra.is_empty() {
        return Vec::new();
    }
    let focus = Focus::current(ctx);
    let mut fired = Vec::new();
    ctx.input_mut(|i| {
        i.events.retain(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            if !extra.contains(key) {
                return true;
            }
            match resolve_binding(*key, *modifiers) {
                Some((binding, action)) if focus.allows(binding) => {
                    fired.push(action);
                    false
                }
                _ => true,
            }
        });
    });
    fired
}

/// What a binding does, in the words of the keyboard reference
/// (`docs/user/keyboard.md`, and the Help ▸ Keyboard shortcuts window,
/// which renders the same rows). English: the catalogs translate the
/// window's chrome, not this table.
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
fn reference_scope(binding: Binding) -> &'static str {
    if binding.key == Key::Tab && binding.chord == Chord::Bare {
        "nothing focused"
    } else if Focus::Text.allows(binding) {
        "everywhere, even while typing"
    } else {
        "not while typing in a field"
    }
}

/// One row of the shortcut table: the key on this platform, what it does,
/// and where it fires. The Help ▸ Keyboard shortcuts window reads the
/// effective table (user overrides applied) through this, so the window
/// and the live keys cannot disagree; [`keyboard_reference`] prints the
/// defaults with the same cells.
pub fn shortcut_rows(is_mac: bool) -> Vec<(String, &'static str, &'static str)> {
    // The effective table: the window shows the keys that fire now, user
    // overrides included (the generated doc stays the defaults).
    with_table(|table| {
        table
            .iter()
            .map(|(binding, action)| {
                let key = key_cap_label(*binding, is_mac);
                let text = match reference_text(*action) {
                    "" => "(undocumented)",
                    text => text,
                };
                (key, text, reference_scope(*binding))
            })
            .collect()
    })
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

/// The first row of the effective table matching `key` + `mods`, with
/// its binding.
pub fn resolve_binding(key: Key, mods: Modifiers) -> Option<(Binding, ShellAction)> {
    with_table(|table| {
        table
            .iter()
            .find(|(binding, _)| binding.key == key && binding.chord.matches(mods))
            .copied()
    })
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
    fn the_shortcuts_window_rows_match_every_binding() {
        let doc = keyboard_reference();
        for mac in [false, true] {
            let rows = shortcut_rows(mac);
            assert_eq!(rows.len(), BINDINGS.len());
            for (key, text, scope) in &rows {
                assert!(!key.is_empty() && !scope.is_empty());
                assert_ne!(*text, "(undocumented)", "{key}");
                // The window shows exactly what the generated doc says.
                assert!(doc.contains(&format!("`{key}`")), "{key} in the doc");
                assert!(doc.contains(text), "{text} in the doc");
            }
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

    // --- User overrides (g3-keymap-editor) ---

    #[test]
    fn every_action_is_listed() {
        // A new ShellAction variant fails to compile here until it is
        // added to this match and to ALL_ACTIONS.
        let guard = |a: ShellAction| match a {
            ShellAction::NewProject
            | ShellAction::OpenMesh
            | ShellAction::OpenProject
            | ShellAction::SaveProject
            | ShellAction::SaveProjectAs
            | ShellAction::Undo
            | ShellAction::Redo
            | ShellAction::ToggleWireframe
            | ShellAction::ToggleGrid
            | ShellAction::CycleViewMode
            | ShellAction::ToggleMaskMode
            | ShellAction::InvertMask
            | ShellAction::SelectTool(_)
            | ShellAction::BrushSmaller
            | ShellAction::BrushLarger
            | ShellAction::ToggleHideUi
            | ShellAction::ToggleMaximize
            | ShellAction::CommandPalette
            | ShellAction::Preferences
            | ShellAction::NextWorkspace
            | ShellAction::PrevWorkspace
            | ShellAction::ViewMaterial
            | ShellAction::NextChannel
            | ShellAction::PrevChannel
            | ShellAction::NextMeshMap
            | ShellAction::PrevMeshMap => {}
        };
        ALL_ACTIONS.iter().copied().for_each(guard);
        assert_eq!(ALL_ACTIONS.len(), 25 + ToolKind::ALL.len());
        for kind in ToolKind::ALL {
            assert!(
                ALL_ACTIONS.contains(&ShellAction::SelectTool(kind)),
                "{kind:?}"
            );
        }
        for (_, action) in BINDINGS {
            assert!(ALL_ACTIONS.contains(action), "{action:?}");
        }
        for (i, a) in ALL_ACTIONS.iter().enumerate() {
            assert!(!ALL_ACTIONS[i + 1..].contains(a), "{a:?} listed twice");
            assert_eq!(action_from_name(&action_name(*a)), Some(*a));
        }
        assert_eq!(
            action_name(ShellAction::SelectTool(ToolKind::Erase)),
            "SelectTool(Erase)"
        );
    }

    #[test]
    fn binding_specs_round_trip_for_every_default_row() {
        for (binding, action) in BINDINGS {
            let spec = binding_spec(*binding);
            assert_eq!(
                parse_binding_spec(&spec),
                Some(*binding),
                "{action:?} {spec}"
            );
        }
        assert_eq!(binding_spec(bind(Key::Z, Chord::CmdShift)), "ctrl+shift+z");
        assert_eq!(binding_spec(bind(Key::Tab, Chord::Bare)), "tab");
        assert_eq!(binding_spec(bind(Key::C, Chord::Shift)), "shift+c");
        assert_eq!(parse_binding_spec("Cmd+S"), Some(bind(Key::S, Chord::Cmd)));
        assert_eq!(
            parse_binding_spec("SHIFT+ctrl+p"),
            Some(bind(Key::P, Chord::CmdShift))
        );
        assert_eq!(parse_binding_spec("alt+z"), None, "no chord expresses Alt");
        assert_eq!(parse_binding_spec("ctrl+nosuchkey"), None);
        assert_eq!(parse_binding_spec(""), None);
    }

    #[test]
    fn overrides_round_trip_through_the_line() {
        let mut o = Overrides::default();
        assert_eq!(o.serialize(), "");
        assert_eq!(Overrides::parse(""), o);
        o.set(
            ShellAction::ToggleGrid,
            vec![bind(Key::G, Chord::Shift), bind(Key::F5, Chord::Bare)],
        );
        o.set(ShellAction::Undo, vec![bind(Key::U, Chord::Cmd)]);
        // Unbound is an override too.
        o.set(ShellAction::InvertMask, Vec::new());
        let line = o.serialize();
        // ALL_ACTIONS order, whatever the insertion order.
        assert_eq!(line, "Undo:ctrl+u;ToggleGrid:shift+g,f5;InvertMask:");
        assert_eq!(Overrides::parse(&line), o);
        assert_eq!(
            Overrides::parse(&line).serialize(),
            line,
            "exact round trip"
        );
        // Setting the defaults back removes the entry: the delta stays minimal.
        o.set(ShellAction::Undo, default_bindings(ShellAction::Undo));
        assert_eq!(o.get(ShellAction::Undo), None);
        // Lenient: unknown actions and junk entries are skipped, a typo'd
        // spec never unbinds an action.
        let junk = Overrides::parse("Bogus:ctrl+q;Undo:ctrl+nope;;garbage;ToggleGrid:shift+g");
        assert_eq!(junk.get(ShellAction::Undo), None);
        assert_eq!(
            junk.get(ShellAction::ToggleGrid),
            Some(&[bind(Key::G, Chord::Shift)][..])
        );
        // More than MAX_BINDINGS: capped; duplicates dropped.
        let mut capped = Overrides::default();
        capped.set(
            ShellAction::ToggleGrid,
            vec![
                bind(Key::F5, Chord::Bare),
                bind(Key::F5, Chord::Bare),
                bind(Key::F6, Chord::Bare),
                bind(Key::F7, Chord::Bare),
            ],
        );
        assert_eq!(
            capped.get(ShellAction::ToggleGrid),
            Some(&[bind(Key::F5, Chord::Bare), bind(Key::F6, Chord::Bare)][..])
        );
    }

    #[test]
    fn an_override_changes_every_consumer() {
        let mut o = Overrides::default();
        o.set(ShellAction::Undo, vec![bind(Key::U, Chord::Cmd)]);
        o.set(
            ShellAction::ToggleWireframe,
            vec![bind(Key::W, Chord::Shift)],
        );
        o.set(ShellAction::ToggleGrid, Vec::new());
        set_overrides(&o);
        assert_eq!(
            binding_of(ShellAction::Undo),
            Some(bind(Key::U, Chord::Cmd))
        );
        assert_eq!(
            shortcut_label(ShellAction::Undo, false).as_deref(),
            Some("Ctrl+U")
        );
        assert_eq!(resolve(Key::U, Modifiers::COMMAND), Some(ShellAction::Undo));
        // The default row is gone, not merely shadowed.
        assert_eq!(resolve(Key::Z, Modifiers::COMMAND), None);
        assert_eq!(
            resolve(Key::W, Modifiers::SHIFT),
            Some(ShellAction::ToggleWireframe)
        );
        assert_eq!(resolve(Key::W, Modifiers::NONE), None);
        assert_eq!(binding_of(ShellAction::ToggleGrid), None);
        assert_eq!(resolve(Key::G, Modifiers::NONE), None);
        // Untouched actions keep their defaults; the Help window follows.
        assert_eq!(
            binding_of(ShellAction::Redo),
            Some(bind(Key::Z, Chord::CmdShift))
        );
        let rows = shortcut_rows(false);
        assert!(rows.iter().any(|(k, t, _)| k == "Ctrl+U" && *t == "Undo"));
        assert!(!rows.iter().any(|(_, t, _)| *t == "Toggle grid"));
        // The effective table keeps the most-modifiers-first order.
        let table = table_for(&o);
        for pair in table.windows(2) {
            assert!(chord_weight(pair[0].0.chord) >= chord_weight(pair[1].0.chord));
        }
        // Back to the defaults.
        set_overrides(&Overrides::default());
        assert_eq!(
            binding_of(ShellAction::Undo),
            Some(bind(Key::Z, Chord::Cmd))
        );
        assert_eq!(
            resolve(Key::G, Modifiers::NONE),
            Some(ShellAction::ToggleGrid)
        );
    }

    #[test]
    fn conflicts_name_the_owning_action() {
        let none = Overrides::default();
        let table = table_for(&none);
        assert_eq!(
            conflict_in(&table, ShellAction::ToggleGrid, bind(Key::Z, Chord::Cmd)),
            Some(ShellAction::Undo)
        );
        // An action never conflicts with itself, and a free key is free.
        assert_eq!(
            conflict_in(&table, ShellAction::Undo, bind(Key::Z, Chord::Cmd)),
            None
        );
        assert_eq!(
            conflict_in(&table, ShellAction::Undo, bind(Key::F9, Chord::Bare)),
            None
        );
        // Overrides count: a key moved away is free, its new owner is not.
        let mut o = Overrides::default();
        o.set(ShellAction::Undo, vec![bind(Key::U, Chord::Cmd)]);
        let table = table_for(&o);
        assert_eq!(
            conflict_in(&table, ShellAction::ToggleGrid, bind(Key::Z, Chord::Cmd)),
            None
        );
        assert_eq!(
            conflict_in(&table, ShellAction::ToggleGrid, bind(Key::U, Chord::Cmd)),
            Some(ShellAction::Undo)
        );
        assert_eq!(first_conflict(&o), None);
        // A hand-made duplicate is caught.
        o.set(ShellAction::ToggleGrid, vec![bind(Key::W, Chord::Bare)]);
        assert_eq!(
            first_conflict(&o),
            Some((
                bind(Key::W, Chord::Bare),
                ShellAction::ToggleWireframe,
                ShellAction::ToggleGrid
            ))
        );
        assert_eq!(first_conflict(&none), None);
    }

    #[test]
    fn capture_maps_the_pressed_shape_to_a_chord() {
        let ctrl = Modifiers::CTRL | Modifiers::COMMAND; // egui off macOS
        assert_eq!(
            capture_shape(Key::S, ctrl),
            Capture::Assign(bind(Key::S, Chord::Cmd))
        );
        assert_eq!(
            capture_shape(Key::S, ctrl | Modifiers::SHIFT),
            Capture::Assign(bind(Key::S, Chord::CmdShift))
        );
        assert_eq!(
            capture_shape(Key::C, Modifiers::SHIFT),
            Capture::Assign(bind(Key::C, Chord::Shift))
        );
        assert_eq!(
            capture_shape(Key::W, Modifiers::NONE),
            Capture::Assign(bind(Key::W, Chord::Bare))
        );
        assert_eq!(capture_shape(Key::Escape, Modifiers::NONE), Capture::Cancel);
        assert_eq!(
            capture_shape(Key::Backspace, Modifiers::NONE),
            Capture::Clear
        );
        // Shapes no chord expresses keep the capture waiting.
        assert_eq!(capture_shape(Key::W, Modifiers::ALT), Capture::Ignore);
        assert_eq!(
            capture_shape(Key::W, Modifiers::CTRL | Modifiers::ALT),
            Capture::Ignore
        );
        assert_eq!(
            capture_shape(Key::W, Modifiers::MAC_CMD | Modifiers::COMMAND),
            Capture::Assign(bind(Key::W, Chord::Cmd))
        );
        assert_eq!(
            capture_shape(Key::W, Modifiers::CTRL),
            Capture::Ignore,
            "macOS Control alone"
        );
        // Every default row's own modifiers capture back to that row, and
        // the captured binding resolves under those modifiers.
        for (binding, action) in BINDINGS {
            let mods = binding.chord.modifiers();
            assert_eq!(
                capture_shape(binding.key, mods),
                Capture::Assign(*binding),
                "{action:?}"
            );
            assert!(binding.chord.matches(mods));
        }
    }

    #[test]
    fn presets_are_conflict_free_and_recognised() {
        let blender = blender_overrides();
        assert_eq!(first_conflict(&blender), None);
        assert_eq!(KeymapPreset::of(&Overrides::default()), KeymapPreset::Umber);
        assert_eq!(KeymapPreset::of(&blender), KeymapPreset::Blender);
        let mut custom = blender.clone();
        custom.set(ShellAction::ToggleGrid, vec![bind(Key::F5, Chord::Bare)]);
        assert_eq!(KeymapPreset::of(&custom), KeymapPreset::Custom);
        assert_eq!(KeymapPreset::Custom.overrides(), None);
        assert_eq!(KeymapPreset::Umber.overrides(), Some(Overrides::default()));
        // Blender's spellings resolve once installed.
        set_overrides(&blender);
        assert_eq!(
            resolve(Key::Z, Modifiers::SHIFT),
            Some(ShellAction::ToggleWireframe)
        );
        assert_eq!(
            resolve(Key::Z, Modifiers::NONE),
            Some(ShellAction::CycleViewMode)
        );
        assert_eq!(resolve(Key::Z, Modifiers::COMMAND), Some(ShellAction::Undo));
        assert_eq!(
            resolve(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT),
            Some(ShellAction::Redo)
        );
        assert_eq!(
            resolve(Key::I, Modifiers::COMMAND),
            Some(ShellAction::InvertMask)
        );
        set_overrides(&Overrides::default());
    }
}
