# Kill report — g0-integration (GUI plan G0, wave-close wiring)

Date: 2026-10-15. Author: dragon. Lock: `g0-integration` over
`crates/umber-app/src/{main, toolbar, actions}.rs` and this report.
Plan: `docs/specs/gui-plan-2026-10-15.md` — the follow-ups the G0 kills
themselves named (notices app wiring; theme boot wiring; disabled-reason
feedback).

## Built

- **The toast stack is in the app.** `notices: umber_gui::Notices` on
  `UmberApp`, drawn once per frame after the dock. Six user-facing failure
  paths that reached only the log now also post a STICKY error toast
  (kept until dismissed — a failed save must not vanish):
  mesh load, mesh upload, environment load, PNG export (write +
  readback), project save, project open. The underlying error text
  follows the translated headline. The logs stay.
- **`theme::apply` at boot.** First frame, once: the Umber style lands on
  BOTH `Theme::Dark` and `Theme::Light`, so an OS appearance flip mid-
  session can no longer swap egui's stock light palette over the chrome
  (the g0-style kill's deferred wiring). Chrome panels keep their own
  density via their local `umber_style` calls. `theme::apply` is
  re-exported from the umber-gui crate root.
- **A shortcut on a disabled action explains itself.** `dispatch` now
  runs `actions::availability` first and posts the translated reason
  ("Nothing to undo", "Load a mesh first") as an Info toast instead of
  doing nothing. Menu rows and toolbar buttons were already greyed from
  the same check; only keys reach this arm.
  - The menu block's hand-rolled `ShellState` literal is replaced by
    `shell_state()`, now shared by the menu and dispatch.
- i18n: six `notice.*` keys, en-US + pt-BR.

## Judgment calls

1. **Errors are sticky; reasons auto-hide.** A failure needs a decision
   (dismiss or retry), so it stays. A disabled-action reason is
   education; it auto-hides like every other toast.
2. **`style_installed` flag rather than applying every frame.** egui
   rebuilds the style on theme events; re-applying per frame would fight
   user density choices made later (G3 preferences). The flag is the
   once-only contract.
3. **No toast for warnings that already have a UI.** Bakes and the
   import path report their failures in their panels; only failures that
   had NO surface went to the log. Adding more is a one-line change per
   site as panels migrate.

## Unverified

- The toast's visual stack position over the status bar: manual on
  hardware (the widget's kittest covers the card itself).
- `theme::apply` vs the dock's `Style::from_egui(ui.style())` interplay
  across an OS flip mid-session: the snapshot suite cannot simulate the
  OS event.
