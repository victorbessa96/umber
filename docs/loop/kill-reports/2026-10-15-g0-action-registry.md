# Kill report — g0-action-registry (GUI plan G0)

Date: 2026-10-15. Author: dragon. Lock: `g0-action-registry` over
`crates/umber-app/src/actions.rs` (new), `toolbar.rs`, `main.rs` (module
registration + the menu-bar block only), `lib.rs`, `assets/i18n/en-US.ftl`,
`assets/i18n/pt-BR.ftl`, this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` G0 row `g0-action-registry`; design
rule 2 ("one action table"). Sources: photocraft `shortcut_dispatch.rs` +
`enable_rules.rs` (PC:#3, PC:#12), Blender's every-menu-item-shows-its-key
(B:D20).

## Built

- `actions.rs` (new): the registry.
  - `BINDINGS`: `(Binding { key, chord }, ShellAction)` rows, most
    modifiers first.
  - `Chord::{Bare, Cmd, CmdShift}` carries the toolbar's exact modifier
    semantics:
    - a bare letter needs no Ctrl/Cmd; Shift and Alt are ignored;
    - Alt+chord is reserved;
    - macOS Control is not a bare press.
  - `resolve` walks the table and `binding_of` looks up an action's
    binding.
  - `shortcut_label` formats through egui's `KeyboardShortcut::format`
    (Ctrl+Shift+Z off macOS, ⇧⌘Z on it).
  - `ShellState` + `availability(action) -> Result<(), reason key>` is the
    one enabled-check.
  - 6 unit tests:
    - the table sorts most-modifiers-first;
    - no duplicate bindings;
    - every row resolves to itself, so no shadowing;
    - platform labels;
    - availability reasons;
    - every reason key is in the catalog.
- `toolbar.rs`: `resolve_shortcut` delegates to `actions::resolve`. The old
  `match` table is gone. A new test pins `SHORTCUT_KEYS` to exactly the
  bound keys. All 9 pre-existing toolbar tests run unchanged: the same 10
  bindings with the same semantics.
- `main.rs`: the menu bar reads the registry. File (New / Open Mesh / Open
  Project / Save Project) and a new Edit menu (Undo / Redo) print each
  row's live shortcut via `Button::shortcut_text`. Rows grey out from
  `availability`, and a disabled row's hover text names the reason. Open
  Project / Save Project now route through `dispatch` like every other
  action; they used to call the methods directly.
- i18n: `menu.edit` and four `action.unavailable-*` reasons, in en-US and
  pt-BR.

## Judgment calls

1. **The registry is keyed by the existing `ShellAction`**, not a new
   string-id enum. The 12 actions are already the dispatch vocabulary.
   String ids (needed by the keymap editor's persisted overrides, G3)
   become a `ShellAction::id()` there, not a second enum now.
2. **`SHORTCUT_KEYS` stays a const list**, pinned to the table by a test
   rather than derived at runtime. egui's per-frame poll wants a fixed
   key slice, and the test makes drift a red.

## Unverified

- The menu's `shortcut_text` rendering has no snapshot yet. The G0
  kittest-harness kill adds the shell snapshot.

## Deferred

- Focus-aware dispatch and the "X is not available: reason" status line:
  `g0-focus-dispatch`, next.
