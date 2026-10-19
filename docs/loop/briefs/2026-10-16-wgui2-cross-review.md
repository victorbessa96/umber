# Review brief: W-GUI-2 cross-kill static review (host-down pass)

You are the adversarial reviewer for Umber's W-GUI-2 wave (Rust
workspace ~/Projects/umber, HEAD 9acdc1c). The build host is DOWN —
you have NO cargo. Your review is the static class that precedes the
merged battery: cross-kill interactions, API misuse, borrow
architecture, i18n collisions. READ-ONLY over source files. Your ONLY
writes: the review artifact below. Do not edit any source file. Do
not commit.

## The kills under review (all landed, never compiled together)

1. **g5-first-run** (dragon): `crates/umber-app/src/first_run.rs` (new)
   + its wiring in `main.rs` (search `quick_setup`) + `setup.*` keys
   in both catalogs.
2. **g5-tab-overflow** (claw): tab title elision + scroll-into-view in
   `main.rs` (search `tab_overflow_dock_tests`, `dock_tab_widths`,
   `scroll_active_tabs_into_view`, `tab_title_text`).
3. **g2-gesture-integration** (claw): `LayersUiOutcome.gestures` in
   `document.rs`, shell dispatch in `main.rs` (search
   `layer_gestures`, `layer_session_ops`, `apply_layer_session_ops`,
   `sync_gesture_selection`, `layer_gesture_shell_tests`),
   gestures module in `layers_panel.rs`.
4. **g6-overlays-popover** (claw, separately ad-hoc verified):
   `viewport.rs` + `display_panel.rs` — review only for CROSS-kill
   interactions with the above (shared fields, draw order, i18n).

## Review protocol (cargo-free, evidence-graded)

For each finding: file:line, the exact code, why it breaks, the class
(severity: BLOCKER = would fail the battery or CI; LIKELY = battery
probably catches; SMELL = survives the battery but is wrong;
NIT = style). Verify every API against the pinned registry
(`~/.cargo/registry/src/*/egui-0.36.2/`, `egui_dock-0.21.1/`,
`eframe-*`) — signature, generic bounds, Deref reachability. egui 0.36
traps: no `ui.child_ui` (use `new_child` + `UiBuilder`); no
`RawInput.modifiers` (use `Event::ModifiersChanged`); `Stroke::new`
not const; `ctx.data` not `ui.data`; `ComboBox` selected text is the
accesskit VALUE not the label.

Specific cross-kill seams to check:

- **main.rs draw order**: where each kill's block sits in the frame
  loop relative to the others (quick_setup draw vs confirm_ui draw vs
  scroll_active_tabs_into_view after DockArea::show_inside) — two
  blocks fighting the same borrow of `self` inside a closure is the
  E0500 class that bit the home-card wiring.
- **LayersUiOutcome consumers**: `gestures` is a new field — every
  construction site of `LayersUiOutcome` (grep the workspace) must
  compile with it. Same for any struct literal the kills extended
  (AppState, MaterialPanelContext, PanelViewer).
- **Test module name collisions**: `layer_gesture_shell_tests` vs
  `tab_overflow_dock_tests` vs existing `mod tests` in main.rs —
  duplicate `#[cfg(test)] mod tests` in one file is an E0659-class
  break the wave gate once caught.
- **i18n**: `setup.*` keys exist in BOTH catalogs, no key collides
  with an existing one (grep both .ftl files for each new key), and
  every `tr("...")` call the kills added has a key.
- **gestures module**: `layers_panel.rs`'s `gestures` — the
  `#![allow(dead_code)]` the claw kept: list which items are actually
  test-only-reached and whether the allow is honest or masks a wiring
  gap (an accessor the integration was supposed to call but doesn't).
- **first_run**: the `close` flag pattern (`.open(&mut open)` +
  `self.open = open && !close`) — confirm no path leaves the window
  stuck open or re-showing after Skip; `should_show()` semantics
  against the real `prefs_path()`; the probe test's dir creation.
- **LayerSessionOp ordering**: `layer_session_ops` claims copy →
  end-mask-edit → set-target → start-mask-edit, with the reasoning
  that `set_active_target` clears a foreign mask edit. Read
  `paint_state::PaintState` (crates/umber-paint/src/paint_state.rs)
  and verify that reasoning — if `set_active_target` does NOT clear
  mask edits, the ordering comment is wrong even if the order is
  harmless.
- **tab-overflow width math**: `DOCK_TAB_X_SPACING`, `DOCK_BUTTON_SIZE`
  constants vs egui_dock 0.21.1's real layout (read its
  `src/tab.rs`/`style.rs` in the registry) — the pinned test claims
  to copy its allocation; verify the constants against the source.

## Output

`docs/loop/artifacts/2026-10-16-wgui2-cross-review.md` — findings
table (severity, file:line, class, evidence, suggested fix), then a
verdict line per kill (CLEAN / FINDINGS) and an overall BLOCKER
count. Reply with: the artifact path, the BLOCKER count, and the top
three findings by severity.
