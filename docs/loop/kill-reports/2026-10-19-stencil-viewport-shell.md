# stencil-viewport-shell — the viewport stencil path wired to the shell

2026-10-19, session local-3d2151f3. Lock: stencil-viewport-shell
(viewport.rs, main.rs, display_panel.rs, both catalogs, this report).

## The gap this closes

The wave landed the viewport's full step-3 surface (the overlay
quad draw, the S chords with their latched drags, the Overlays
popover rows, the pure `stencil_drag` + its tests) — but the SHELL
never handed the session's stencil in: `ui_with_home` hard-coded
`stencil: None` at main.rs's only viewport call, so the overlay
never drew, the chords never fired, and N existed nowhere in
umber-app.

## Built

- `ui_with_home` gains the stencil param (`ui()` passes None; the
  shell passes `Some(&mut *self.stencil)` — the PanelViewer's own
  field), forwarding it through `ViewportExtras` into
  `ui_with_stencil`.
- The N key, in `stencil_input`: toggles `active` with an image
  present, suppressed while any text field holds the keyboard
  (`ctx.memory` focused — main.rs:3393's precedent), and the gate
  follows the flip immediately.
- `restage_stencil` becomes the UNIFIED re-stage: the composed LABEL
  bytes AND the hook hand-off (Amendment A raw coverage through
  `set_stencil_gate`) — so chord release, popover edits, and N keep
  one gate with the panel path. The mutable-paint chain threads
  through `overlays_header` and `stencil_popover_rows` (FnOnce
  closures, disjoint captures).
- The N hint flips live in both catalogs ("N toggles the stencil —
  paint passes through while it is ignored." / "N alterna o
  estêncil — a tinta passa livre enquanto ele é ignorado.") and the
  hint row draws enabled.

## Gates run

- Host battery (`wave-battery.sh umber-app`, centralhost):
  BATTERY_EXIT=0 after three fix-forward rounds — E0061 (ui()'s
  7-arg call + the PanelViewer field shape, not the App's state),
  E0596 (the mut-binding chain for `as_deref_mut` reborrows),
  then `needless_option_as_deref` + `unused_mut` cleanups (the
  same lint class the wave's viewport.rs:1033 fix established).
  Final legs: layers 16 GREEN, fmt PASS (applied), clippy umber-app
  PASS (`-D warnings`), test umber-app 821 passed / 0 failed.
- rustfmt CLEAN standalone at every round.

## What remains in the stencil lane

- The active-tile-change re-stage (per-set scope — the
  `activate_set` sites; other-sized tiles paint stencil-ungated
  until then, Amendment A's documented safe degradation).
- The docs tick (design step 4: gui-plan g6 row, ROADMAP, queue)
  once the lane closes.
