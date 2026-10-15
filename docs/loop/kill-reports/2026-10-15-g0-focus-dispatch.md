# Kill report: g0-focus-dispatch (focus-aware shortcut dispatch)

Date: 2026-10-15. Author: dragon. Lock: `g0-focus-dispatch` over
`crates/umber-app/src/{actions,toolbar}.rs` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g0-focus-dispatch` (PC:#3,
PC:#10). It follows g0-action-registry (3ff306e).

## Built

- `actions.rs`:
  - `Focus { None, Widget, Text }`. `Focus::current(ctx)` reads it from
    `egui_wants_keyboard_input()` / `text_edit_focused()`; the mapping
    itself is `Focus::from_flags`, so it is testable without a context.
  - `Focus::allows(binding)`: None and Widget pass every binding. Text
    passes only Ctrl/Cmd chords and the F-keys, and keeps Ctrl+Z /
    Ctrl+Shift+Z for the field.
  - `is_function_key` (F1..F35, by key name) and `resolve_binding`
    (the row with its binding; `resolve` now goes through it).
- `toolbar.rs` `poll_shortcuts`:
  - Walks this frame's key events in order, resolves each, checks the
    focus, and REMOVES every dispatched press from `InputState.events`,
    so a panel drawn later in the frame cannot act on it again.
  - Turns off egui's built-in keyboard UI zoom (`zoom_with_keyboard =
    false`) the first time it runs.

## Tests (5 new)

- `focus_maps_from_the_flags`: the four flag combinations.
- `text_focus_keeps_letters_and_undo_but_passes_other_chords`: over the
  whole table, exactly Save and Open pass a text field; None/Widget pass
  everything; a bare F-key passes, a bare letter does not.
- `function_keys_are_exactly_f1_to_f35`, including `Key::F` (the name
  "F" is not a function key) and `Num1`.
- `poll_fires_and_consumes_with_nothing_focused` (headless egui frame):
  W + E fire in order, the W press is gone for later panel code, and the
  zoom option is off afterward.
- `typing_in_a_text_field_does_not_switch_tools` (headless, a real
  focused `TextEdit`): E and W fire nothing and stay readable for the
  field; Ctrl+Z fires nothing; Ctrl+S still saves.

## Judgment calls

- **Ctrl+Z stays in a text field.** The plan row says the field keeps
  "everything but ⌘-shortcuts and F-keys". Undo is the exception: egui's
  `TextEdit` binds Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y for its own text undo,
  and letting the document undo fire there would throw away paint while
  the user meant to fix a typo. Blender and Painter behave the same way.
- **Focus is last frame's focus.** egui's memory reports the widget that
  held focus when this frame's events were typed, which is the right
  owner for them. A field focused by a click in this same frame starts
  owning keys next frame.
- **Disabled-action reason**: the plan's "X is not available: reason"
  already ships. `UmberApp::dispatch` posts the registry reason as a
  notice (the g0 integration, 1aaa966). This kill adds no second path;
  `main.rs` is under the live cavity-wiring lock and needed no edit.
- **`resolve` / `resolve_shortcut` stay** as the table's plain query for
  the existing tests and docs, with `allow(dead_code)` outside tests.

## Unverified

- A real IME composition (`Event::Ime`) passes through untouched by
  construction (only `Event::Key` is matched). No IME test exists.

## Deferred

- The Preferences-backed UI scale replacing keyboard zoom
  (g7-density-scale).
