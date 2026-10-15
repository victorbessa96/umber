# Kill report: g5-focus-modes (Hide UI, Maximize Area)

Date: 2026-10-15. Author: dragon. Lock: `g5-focus-modes` over
`crates/umber-app/src/{actions,toolbar,rich_tooltip,palette,main,shell_menu}.rs`,
both ftl catalogs and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g5-focus-modes` (P:A4, B:A1.4,
B:A1.8).

## Built

- **Two registry actions:** `ToggleHideUi` (Tab) and `ToggleMaximize`
  (Ctrl+Space). They work through every surface the registry feeds: the
  shortcut poll, the Window menu rows with their printed shortcuts, the
  command palette, and the rich tooltips. Each has a title and a
  description key in both locales.
- **`shell_menu::FocusState`:** entering a mode swaps the live dock for
  a one-tab dock and keeps the real layout aside. Leaving the mode puts
  that exact layout back (sizes, tab order). Switching straight from one
  mode to the other restores first, so the saved layout is always the
  user's real one.
  - Hide UI keeps the viewport. Maximize keeps the focused area's active
    tab, or the viewport when nothing is focused.
- **Hide UI drops the toolbar and the status bar.** The menu bar stays,
  and a corner **Show UI (Tab)** button gives a visible way back (the
  plan row's requirement).
- **Tab only means Hide UI with nothing focused.** `raw_input_hook`
  removes a bare Tab before egui's focus logic sees it, but only when no
  widget has focus. `Focus::allows` also refuses bare Tab for a focused
  widget. Tab between fields and in text edits is unchanged.
- **Window ▸ Panels and Reset Layout leave a focus mode first.** Their
  edit then lands on the real layout, not on the one-tab stand-in.

## Tests

- `focus_modes_swap_and_restore_the_exact_layout`: a user edit before
  entering survives Hide UI → Maximize → back. Restore in Normal is a
  no-op.
- `maximize_targets_the_viewport_without_focus`.
- The focus table test now pins that a focused widget keeps bare Tab
  and that Ctrl+Space passes a text field. The binding census is now
  10 bare + 7 chords. Every key is in both catalogs via `has_key`.
- Gate 1 (umber-app): fmt, clippy, 450 tests.

## Judgment calls

- **Swap, not hide.** egui_dock has no "hide a node" call. Keeping the
  real `DockState` aside and drawing a one-tab stand-in gives an exact
  restore with no layout arithmetic.
- **The menu bar stays in Hide UI.** Painter hides it too, but the menu
  plus the corner button make two visible ways back, which the plan
  asks for. Hiding the menu as well is a one-line follow-up if wanted.
- **Ctrl+Space, not Blender's Ctrl+Space-and-Alt.** One chord, and
  Space alone stays free for the plan's held-Space pan (g7-polish-batch).

## Unverified

- The `raw_input_hook` path (real Tab events) and the corner button draw
  have no kittest. The hook is 15 lines over `memory.focused()`.

## Process note

`crates/umber-gui/src/widgets/axis_gizmo.rs` appeared untracked during
this kill. It is the sibling session's live g1-axis-gizmo work, and this
kill leaves it alone.
