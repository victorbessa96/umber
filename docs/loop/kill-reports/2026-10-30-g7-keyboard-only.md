# Kill report: 2026-10-30 g7-keyboard-only — focus ring token, Esc closes overlays, menu number keys

**Status: Gate 1 battery running (fmt OK; lib tests 136 umber-gui + 206
umber-app + 3 new kittests GREEN locally) — committing on green.**

- **Kill**: `g7-keyboard-only` (docs/specs/gui-plan-2026-10-15.md G7 row:
  "tab order, focus ring token, Esc closes popups, number keys pick menu
  items"). The tab-order half landed with g0-focus-dispatch (`cfc8158`)
  and the focus-mode work; this kill lands the remaining three clauses.
- **Wave**: the 2026-10-29 wave's open tail (queue.md / STATE.md Next:
  g7-keyboard-only was first in the queue), continued 2026-10-30 under
  Bessa's standing word "Focus only on the umber project, work
  autonomously and with full permissions".
- **Authorship**: this session (razul, dragon) authored all bytes in-
  session; no claw dispatch this kill (small, shell-coupled surface).
- **Kill-lock**: `wave-10-30-code` — undo.rs, document.rs,
  history_panel.rs, main.rs, theme.rs, actions.rs, toolbar.rs,
  first_run.rs, tests/keyboard_menu_kittest.rs, both i18n catalogs, this
  report. Registered 2026-10-30 by this session.

## What landed

1. **Focus ring token** (`crates/umber-gui/src/theme.rs`): egui 0.36
   paints the `active` widget visuals for the widget holding keyboard
   focus, so the ring is pinned as a named token `FOCUS_RING_WIDTH = 2.0`
   on `visuals.widgets.active.bg_stroke` (was a bare `1.0`). Unit test
   `focus_ring_token_is_the_active_widget_ring` pins the token to the
   active visuals.
2. **Esc closes popups** (`actions.rs` + `main.rs` + `first_run.rs`):
   `actions::esc_closes_overlays(ctx, palette_open)` — true when Escape is
   pressed, no text field holds the keyboard (`Focus::current != Text`; a
   text edit keeps its own Esc), and the command palette is closed (it
   consumes Esc itself). `App::close_overlays_on_esc` runs once per frame
   in the menubar path and clears `prefs_open`, `about_open`,
   `shortcuts_open`, `pending_new`, `pending_revert`, `pending_quit`, and
   the Quick Setup window (new `QuickSetup::close`). egui menus/popups
   close on Esc natively. The recovery window is deliberately NOT closed:
   Esc there must not silently discard unsaved work.
3. **Number keys pick menu items** (`toolbar.rs` + `main.rs`):
   `toolbar::menu_digit_pick(ui, &items, &mut actions)` — called at the
   end of the File/Edit/Window menu closures; Digit1..=Digit9 push the
   nth *enabled* collected action and close the menu. The shell's
   `action_item` helper now takes an `items: &mut Vec<ShellAction>` sink
   and pushes only when `availability` is Ok, so number keys skip disabled
   entries exactly like the mouse. View/Help menus have no action rows.

## Tests

- `crates/umber-app/tests/keyboard_menu_kittest.rs` (new, 3 kittests):
  menu Digit2 picks the 2nd item and fires it; Digit9 beyond the item
  count fires nothing; the Esc predicate is true with no focus, false
  with a focused text edit, false with the palette open.
- `theme.rs::focus_ring_token_is_the_active_widget_ring` (unit).
- Full Gate-1 battery (`scripts/wave-battery.sh`: fmt + clippy
  -D warnings + layers + tests) — verdict in the wave table row.

## Process notes (fact, not narrative)

1. Two scripted-patch defects were caught by `cargo check` before any
   test ran: the multiline `action_item(\n ui,` normalization double-
   inserted `&mut items` on the six single-line sites (E0057, fixed by
   regex collapse), and a doc-comment landed on the wrong item when the
   `close_overlays_on_esc` method was inserted before `shell_state` (fixed
   by re-anchoring; `shell_state`'s L6 doc block restored verbatim).
2. kittest idiom notes for the next kill: egui 0.36 panels take
   `&mut Ui` (`CentralPanel::show(ui, …)`, not `show(ctx, …)`);
   `egui_kittest::Harness` lives at the crate root (`kittest::` re-exports
   only `NodeT`/`Queryable`); `Harness::key_press` queues down+up and
   `step()` runs one frame per event, so slice recorded frames after the
   injection point instead of reading `last()`; `Harness::new_ui` closures
   get `&mut Ui`, and a text edit is focused in a test via
   `response.request_focus()`, not a synthetic click.
3. The systemd-run test legs need `export PATH="$HOME/.cargo/bin:$PATH"`
   inside the script — the user unit's PATH lacks `~/.cargo/bin`, and the
   first leg silently no-op'd every cargo invocation (caught because the
   battery script's own cargo-guard exists; the ad-hoc leg did not have
   one — noted for the runbook).
