# Kill report: g4-property-context-menu (Reset / Copy / Paste on right-click)

Date: 2026-10-15. Author: dragon. Lock: `g4-property-context-menu` over
`crates/umber-gui/src/widgets/value_field.rs`, `crates/umber-gui/Cargo.toml`
(+ the one-line Cargo.lock delta) and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g4-property-context-menu` (B:D10).

## Built

- **`ValueField::context_menu(MenuLabels)`.** Right-click opens the
  property menu:
  - **Reset to Default**, only with a `default_value`, disabled when
    already at it;
  - **Copy** (the bare number);
  - **Paste**, disabled when there is nothing usable to paste.

  umber-gui has no i18n, so the caller passes the translated labels.
  Without labels the old behavior stays: right-click copies.
- **The shared field clipboard.** Every copy path (the menu, hover
  Ctrl+C, the legacy right-click) also writes the value into egui temp
  data under one id. Menu Paste reads that through `menu_paste`, which
  evaluates the text through `eval_expr` and clamps it to the hard
  range. egui offers no clipboard READ outside its paste event, so text
  copied from another app arrives through hover Ctrl+V (the grammar
  kill's path) and the menu Paste works field to field. The module
  docs say so.
- `crates/umber-gui/Cargo.toml` gains `egui_kittest` (dev-only, no
  features). It is already in the workspace lock through umber-app, so
  the lock changes by one line: umber-gui's dependency list. There is
  no new package and no version change. The battery on the build host
  produced the lock, and it is committed as produced.

## Tests (2 new, 15 in the module)

- `menu_paste_evaluates_and_clamps`: nothing copied, a plain value,
  clamp past max, an expression with the unit, junk.
- `right_click_menu_resets_copies_and_pastes` (egui_kittest, menu rows
  found by AccessKit label): Reset writes the default; Copy then Paste
  restores the copied value after the field moved; the neighbouring
  field is untouched.
- **Mutation check:** making Reset write nothing fails this test.
- Gate 1 on the build host: umber-gui 72 + umber-app 462 tests, clippy
  and fmt clean on both.

## Judgment calls

- **Assign Shortcut is not in the menu.** The plan says "when bindable".
  Values have no actions to bind, and the action registry offers no
  user overrides until the keymap editor (g3-keymap-editor) lands.
- **No app call sites opt in yet.** 47 `ValueField::new` sites would each
  need translated labels, through an app-side helper over `tr()`. That
  sweep is its own mechanical kill, so it is not hidden inside this one.
- **The first test draft clicked a guessed pixel offset and failed.**
  The menu laid out at the window corner, not under the pointer. The
  test now finds the rows by label through egui_kittest, the way the
  harness kill set up.

## Deferred

- The app-side `prop_menu_labels()` helper and the 47-site opt-in sweep.
- Assign Shortcut, with the keymap editor.
