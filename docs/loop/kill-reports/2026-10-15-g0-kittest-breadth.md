# Kill report (IN FLIGHT, uncommitted): G0 kittest breadth — command palette + History

Date: 2026-10-15. Author: dragon. Locks: `g0-kittest-breadth` +
`g0-kittest-palette` + `g0-kittest-lib` (files below), plus the
uncommitted `g5-workspace-persist` tree it sits on.

## State

Blocked on the build host outage (192.168.15.64 down since ~18:43). The
edits are uncommitted and UNRUN — no cargo anywhere until the host
returns. Two earlier speculative tests in this kill were rewritten after
reading the sources (see "corrections"); whatever the first battery says
is the truth, not this report.

## Built (unverified)

- `tests/panel_kittest.rs`:
  - `the_command_palette_picks_and_notes_a_recent`: the overlay with a
    "wireframe" query; Enter picks the selected row, closes the overlay
    and notes the recents.
  - `the_history_buttons_undo_and_redo_layer_edits`: the panel's Undo and
    Redo buttons drive a two-add document (1 then 2 layers).
- `support::press_enter` (key_press + step).
- `Palette::is_open` / `set_query` test seams (documented as such).
- `lib.rs` exposes `actions`, `document`, `history_panel`, `palette`,
  `toolbar` for the behavior kittests (the shell still uses `crate::`).

## Corrections made while writing (before any run)

- The first draft assumed palette = a color swatch grid. It is the
  COMMAND palette overlay. Rewritten around `Palette::show`.
- The first draft assumed `history_panel::show`; the entry point is
  `history_ui(ui, doc, paint)`.
- `press_enter` first used a nonexistent 3-arg `key_press`; the harness
  API is `key_press(&self, key)`.
- The user guide gained a "The menus" section (committed, `da05190`),
  whose workspace-persistence claim rides on the unverified
  `g5-workspace-persist` tree.

## When the host returns

1. Re-run the workspace-persist battery first (it holds the plan file).
2. Then this kill's battery; mutation-check the palette test (Enter
   ignored) and the history test (buttons reading, not running).
3. Commit both, then the wave gate on a clean clone, push as W-C2g.
