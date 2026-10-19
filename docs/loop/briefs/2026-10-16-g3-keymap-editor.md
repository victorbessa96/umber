# Kill brief: g3-keymap-editor (GUI plan G3)

You are implementing one kill of Umber's GUI plan, in the Rust workspace
at ~/Projects/umber (current HEAD: ee1a078). Work ONLY on the files your
lock holds. Other sessions are editing other files in the same tree —
do not touch anything outside your lock surface, do not run cargo
(the build host owns cargo; the dragon runs the battery), do not commit.

## What to build

The Keymap editor page inside the existing Preferences window
(`crates/umber-app/src/preferences.rs`): a searchable 3-column grid
(action label, primary binding, alternate binding), click-to-capture,
conflict text, presets, and an export of the modified delta.

The binding table lives in `crates/umber-app/src/actions.rs`:
- `pub const BINDINGS: &[(Binding, ShellAction)]` — the default table
  (27 bind() rows; `binding_of(action)` finds the first).
- `pub struct Binding { pub key: Key, pub chord: Chord }`;
  `Chord` (actions.rs:58) has variants incl. `Cmd`, `CmdShift`, `Bare`, `Shift`.
- `pub fn resolve_binding(key, mods) -> Option<(Binding, ShellAction)>`
  is what the key poll calls; `shortcut_label(action, is_mac)` renders a
  label for menus/tooltips.
- `ShellAction` is defined in `crates/umber-app/src/toolbar.rs:90`.

## Requirements (from the plan row, docs/specs/gui-plan-2026-10-15.md g3-keymap-editor)

1. **User overrides**: a `keymap_overrides` entry in the persisted
   Preferences (`preferences.rs`: the `key=value` file, `Preferences::parse`
   is lenient — unknown keys ignored, bad values fall back). Store as
   `keymap=<action-name>:<binding-spec>;<action-name>:<binding-spec>;...`
   (one line; keep `parse`/`serialize` round-trip exact; an empty or
   absent line = no overrides). A binding-spec is what
   `KeyboardShortcut::format` prints minus the platform names, e.g.
   `ctrl+shift+z`, `tab`, `b`. Action-name = the ShellAction variant
   name (Debug format without the enum prefix).
2. **Resolution**: `binding_of` consults overrides first: an override
   for action X replaces X's default row in every consumer (menu labels,
   status hints, the keyboard reference window, the poll). Implement as
   a lookup the whole file funnels through — do NOT fork `resolve_binding`
   and `shortcut_label` into separate override-aware copies.
3. **The grid**: in the Preferences window, a "Keymap" section: a search
   box filtering rows by the action's label or key name; 3 columns per
   row — the label (use `actions::reference_text` where it is not
   private; if private, add a small pub accessor), the primary binding
   button, the alt binding button. Click a binding button → capture mode
   (the button shows a localized "press keys…" string): the next
   key-press with a non-bare-modifier shape assigns; Esc cancels; the
   physical key is what's captured (match `Chord` on modifiers + key);
   Backspace clears the binding (back to default when it is an override).
4. **Conflicts**: if the captured shape collides with another action's
   binding (defaults + overrides), show "conflicts with <other action>"
   under the row and DO NOT assign (this is the plan's requirement;
   the poll resolves most-modifiers-first, but the editor must not let
   the user create an ambiguous table).
5. **Presets**: a combo: "Umber (default)" = clear all overrides;
   "Blender-like" = a small named set of swaps (Ctrl+Z stays, but
   paint-tool letters follow Blender where they differ from the current
   table: E→erase is already Blender's; add G for grab/move? NO — do not
   invent actions; preset 2 = the same table with Blender's navigation
   (this already exists as `NavPreset` — the keymap preset only swaps
   what the BINDINGS table owns). If a swap would need an action that
   does not exist, skip it and note it in the report.
6. **Export**: a "Copy changed bindings" button → the same `keymap=`
   line format to the clipboard (egui `ctx.copy_text`), plus the reverse
   (a "Paste bindings" that parses a `keymap=` line into overrides).
7. **Right-click any registered property button** → the g4
   property-context-menu already exists (`prop_menu.rs`); do NOT wire
   keymap assignment into it in this kill.
8. **i18n**: every new user-facing string through `crate::i18n::tr()`
   with a new key, and BOTH catalogs updated: `crates/umber-app/assets`
   is WRONG — the catalogs are at `~/Projects/umber/assets/i18n/en-US.ftl`
   and `pt-BR.ftl` (workspace root `assets/i18n/`, embedded via
   `include_str!("../../../assets/i18n/en-US.ftl")` from i18n.rs:62).
   Fluent syntax; one backslash is literal.
9. **Tests** (pure, no GPU): override parse/serialize round-trip;
   override resolution changes `binding_of`'s answer; conflict detection;
   the capture shape→Chord mapping; the search filter fn. Follow the
   existing test style in preferences.rs/actions.rs (plain #[test] in
   the same file's `mod tests`).
10. **Docs**: module doc comment at the top of your changes explaining
    the override model and citing `docs/specs/gui-plan-2026-10-15.md`
    kill g3-keymap-editor. Write the kill report at
    `docs/loop/kill-reports/2026-10-16-g3-keymap-editor.md` (see any
    existing report for the shape: Built / Judgment calls / Unverified /
    Deferred / Process note).

## Rules

- Rust 2021, the workspace clippy runs `-D warnings`: no warnings
  (e.g. use `map_or_else` where the arg allocates).
- egui 0.36: `RawInput` has NO `modifiers` field — held modifiers come
  from `Event::ModifiersChanged`. `ui.ctx().copy_text(String)` exists.
- Do not add dependencies.
- Do not edit `docs/ROADMAP.md` (the dragon ticks it).
- When done, write the kill report and reply with: files changed,
  the ftl keys added, test names added, and any judgment calls.
