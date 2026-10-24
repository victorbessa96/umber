# Umber audit — lens B: the UI/UX census (2026-10-24)

Author: dragon, solo (the opencode lane hit its free-tier limit;
the lens ran dragon-side). Every claim cites file:line or the
plan doc's own row, verified against the live tree this date.

## The headline

**The GUI is far more complete than the ROADMAP ever showed: 36 of
the gui-plan's 43 kills are ✓-landed**, with named tails on three
more. The old ROADMAP's "Wave 6b" section never carried per-kill
status; the plan doc does, and its rows verify. The UI surface is
mature: a registry-backed shortcut system with focus-aware
dispatch, a command palette, a keymap editor, preferences, rich
tooltips, workspaces (partial), focus modes, a home screen, the
overlays popover, the contextual toolbar, the UV-view parity, the
stencil overlay UI, the AccessKit sweep, the density/scale rows,
and the polish batch — all landed with kittests.

## The G0-G7 scorecard (from docs/specs/gui-plan-2026-10-15.md's
own rows, cross-checked against the tree)

| Wave | Kills | Landed | Open remainder |
|---|---|---|---|
| G0 foundations | 5 | 4 ✓ + 1 ◐ | the kittest harness' panel_snapshot migration + fonts gate (ef4d5fd's tail) |
| G1 first five minutes | 6 | 6 ✓ | tails: the OS-bitmap cursor path, the flow/angle chords, zoom-to-cursor + F-frame, UV-view axes |
| G2 panels | 8 | 8 ✓ | tails: mask thumbnails, per-entry history thumbs, per-set visibility |
| G3 find anything | 4 | 4 ✓ | — |
| G4 flagship widgets | 5 | 3 ✓ + 1 ◐ | **g4-color-picker (OPEN — egui's stock picker is still in use: no color_picker/colorbutton symbol in brush_panel.rs or layers_panel.rs)**; **g4-curve-widget (OPEN — the pressure-curve plot is not yet promoted)**; g4-property-context-menu's Assign Shortcut + per-field defaults tail |
| G5 shell | 6 | 5 ✓ + 1 ◐ | g5-workspaces' F1-F4 viewport presets + layout persistence |
| G6 viewport depth | 5 | 5 ✓ | — |
| G7 a11y + polish | 4 | 3 ✓ | **g7-keyboard-only (OPEN — the tab order/focus-ring/Esc/number-menu journeys)** |

## The panels (the live surface)

Modules: bakes_panel.rs, brush_panel/ (+brush_panel.rs),
display_panel.rs, empty_state.rs, graph_panel/ (+graph_panel.rs +
history.rs: the procedural history panel k1, 2a84fb8, 651 lines),
history_panel.rs, layers_panel.rs, postfx_panel.rs + main.rs's own
sections (the texture-sets panel at main.rs:2499+, the menu bar,
the status bar, the toolbar/strip at toolbar.rs, the UV view at
uv_view.rs, the viewport at viewport.rs, preferences.rs,
first_run.rs, workspaces.rs, recent.rs, thumbnails.rs,
context_toolbar.rs, navigation.rs, path_tool.rs, stencil.rs).

## The i18n verdict

- The catalogs: `assets/i18n/en-US.ftl` (953 lines) +
  `pt-BR.ftl` (972) — the Fluent ATTRIBUTE scheme (grouped
  messages: `menu =\n .file = …`), so raw message-group counts
  (~45) understate the key inventory (hundreds of attributes).
- The completeness test (crates/umber-app/src/i18n.rs) enforces
  key parity — a key the code calls must exist in en-US (the
  source of truth); pt-BR falls back per missing key.
- **THE .ftl LOCK IS LIFTED**: the freeze-3 recovery landed, and
  post-recovery commits add keys again — 2a84fb8 (the procedural
  history panel, 10-23) added 12 keys to each catalog. The L5(b)
  OccluderScope panel row is UNGATED on this side.

## The shortcuts inventory

The registry: crates/umber-app/src/actions.rs (1,437 lines) — the
`BINDINGS` table (`(Chord, ShellAction)` rows, most-modifiers-
first, the ordering pinned by a guard test), `resolve`,
`shortcut_label` (menu/tooltip formatting), `availability` (the
one enabled-check), `Focus::{None, Widget, Text}` focus-aware
dispatch (`Focus::allows`), Bare letters / Cmd / CmdShift chord
classes, Alt reserved. ~97 Chord references in the table's rows.
The keymap editor (g3, 5e0d89e) writes user overrides; the
palette (g3, 979a523) feeds off the same registry.

## The desktop smokes

`docs/loop/desktop-checks.md`: **every Result line is still empty
(0 of 4 checks filled)** — the desktop session never ran them.
The checks (quit guard, kill-9 recovery, the set-switch smoke,
the Windows real-pen leg) remain the unfired end-to-end
instrument; the L1 headless test closed the set-switch check's
NAMED GAP, but the smoke itself stays owed.

## The named gaps (ordered by user pain)

1. **The color picker (g4)** — the flagship widget still egui's
   stock: no SV-square mesh, no dynamic tracks, no hue-survival.
   A Painter user's single most-touched widget.
2. **Keyboard-only (g7)** — tab order, focus rings, Esc, number
   keys in menus: the a11y journeys unfired.
3. **The curve widget (g4)** — the pressure/falloff/levels curves
   deserve the promoted editor with presets + copy/paste.
4. **The desktop smokes unfired** — the end-to-end instrument
   (quit guard, recovery) has never run in a desktop session.
5. **The workspaces tail** — F1-F4 viewport presets + layout
   persistence.
6. **The kittest harness' migration tail** — the fonts gate +
   the panel_snapshot migration to the shared support module.

## The surprises

1. **ActionId → ShellAction**: the G0 plan row named an
   `ActionId` enum; the landed registry kept the existing
   `ShellAction` type and built `BINDINGS` + `Chord` + `Focus`
   around it (actions.rs:1-40's own docs). The plan's working
   name drifted; the function is complete.
2. **The GUI waves landed QUIETLY**: the ROADMAP's "Next waves"
   section never showed G-wave status; a reader of the ROADMAP
   alone would not know 36/43 GUI kills are done. This refresh
   fixes that.
3. **The home screen + first-run + tab overflow all landed in
   the same commit (d9fa931)** — a large wave-kill whose pieces
   each carry their own kittest.

LENS-B-DONE
