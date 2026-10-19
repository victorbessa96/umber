# W-GUI-2 cross-kill static review (host-down pass)

- Tree: HEAD 9acdc1c + the uncommitted W-GUI-2 working tree (`git status`: main.rs, document.rs, layers_panel.rs, viewport.rs, display_panel.rs, both .ftl modified; first_run.rs new).
- Method: no cargo. Every source claim below comes from reading the file. Every API claim was checked against `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{egui,epaint,emath}-0.36.2`, `egui_dock-0.21.1`, `eframe-0.36.2`. A Python Fluent parser checked catalog keys: message plus attribute, both catalogs, duplicates.
- Severity: BLOCKER = fails the battery or CI, or ships a broken first launch to every user. LIKELY = the battery probably catches it. SMELL = survives the battery but is wrong. NIT = style.

## Findings

| # | Sev | file:line | Class | Evidence | Fix |
|---|-----|-----------|-------|----------|-----|
| F1 | **BLOCKER** | first_run.rs:77 | path double-join, first run never ends | `prefs_path().is_none_or(\|p\| !p.join(PREFS_FILE).exists())`. But `preferences.rs:175-177` `prefs_path()` already returns the FILE: `…/umber/` + `d.join(PREFS_FILE)` = `…/umber/preferences`. So `should_show` checks `…/umber/preferences/preferences`, a path under a regular file. That never exists, so `should_show()` stays true forever. Finish and Skip both save the file to `…/umber/preferences` (main.rs:3799), but Quick Setup still opens on every launch for every user. | `prefs_path().is_none_or(\|p\| !p.exists())`. Drop the `PREFS_FILE` import. |
| F2 | LIKELY | first_run.rs:266-275 | test probe corrupts the real state dir and depends on the host | Same double-join. `dir = prefs_path()` is the prefs FILE path. With no file present the test runs `create_dir_all(&dir)`, which makes a DIRECTORY at `…/umber/preferences`, writes `…/preferences/preferences`, then deletes only the inner file. Effects: (a) on a host with a saved prefs file, `create_dir_all` fails with the file in the way, `.expect` panics and the test FAILS. (b) On a pristine host (CI, and probably the build host that reported "3/3") it passes but leaves a directory where the prefs file belongs. After that, `Preferences::save`'s `fs::write` fails (the path is a dir) and only logs a warning, so prefs silently stop persisting on that machine. It also writes into the developer's real `$HOME`. | Split out a pure `should_show_at(path: Option<&Path>)` and test it against `std::env::temp_dir()`, as `preferences.rs:704` does. On the build host, check for a bogus `~/.local/share/umber/preferences/` directory and `rmdir` it. |
| F3 | SMELL (functional) | viewport.rs:799-831 vs main.rs:3539-3540 | cross-kill wiring gap hidden by `#[allow(dead_code)]` | The popover's Wireframe/Grid rows write `self.overlays.wireframe/grid` and `show_wireframe/grid`, then set `overlay_edited`. Nothing in main.rs calls `take_overlay_edit` (viewport.rs:334, `#[allow(dead_code)]`). Each frame, before the dock, main.rs:3539 runs `set_show_wireframe(self.view.wireframe)`, which writes the stale `ViewToggles` value back. The header draws after the viewport's paint callback (viewport.rs:790), so the toggle never renders: the box snaps back the next frame and Wireframe/Grid in the popover do nothing. Opacity, UV seams and readout work; main.rs never overwrites those. | After the dock, in main: `if let Some(o) = self.state.viewport.take_overlay_edit() { self.view.wireframe = o.wireframe; self.view.grid = o.grid; }`. Remove the dead_code allows on `take_overlay_edit`/`overlay_edited`. |
| F4 | SMELL (functional) | main.rs:3799 vs 3807 | save before mutate | `self.prefs.save(…)` runs at 3799. The Blender keymap is assigned to `self.prefs.keymap_overrides` at 3807, after the save. The Blender preset is installed for this session but not written, so it is gone on the next launch. Choosing Umber keymap on a Help ▸ Quick Setup re-open also never resets an earlier Blender table: `keymap_blender == false` is a no-op. | Apply nav and keymap first (`KeymapPreset::{Umber,Blender}.overrides()` in both cases), then save once. |
| F5 | SMELL | first_run.rs:66-72, main.rs:3800-3802 | Theme choice has no effect | `umber_gui::theme::apply` (theme.rs:285-289) writes the SAME Umber style to both `Theme::Dark` and `Theme::Light`, so `ctx.set_theme(Light)` changes nothing visible. `Preferences` has no theme field (preferences.rs:61-76), so the choice is not persisted either. `FollowOs` maps to `None`, so it cannot undo an earlier Light/Dark. | Remove the Theme row until a light palette exists, or add one plus a `theme=` prefs key. Map FollowOs to `Some(ThemePreference::System)`. |
| F6 | SMELL | layer_gesture_shell_tests main.rs:4518-4519. Doc comments main.rs:4402-4403, 4407-4410 | ordering rationale is false | Claim: "`set_active_target` clears a foreign mask edit, so the target must land BEFORE the mask edit or the edit is wiped." paint_state.rs:2159-2165 has `keep_mask = matches!(target, Layer(id) if self.mask_edit_layer == Some(id))`. `layer_session_ops` only emits `SetTarget(mask_edit.or(select))`, so whenever `EditMask(x)` is present the target is also `x`. The reverse order (EditMask, then SetTarget of the same id) keeps the mask edit. The order is harmless, but the stated reason is wrong, and so is the test comment that pins it. "Copy before target" is also not load-bearing: `set_active_target` reads no planes. | Rewrite the comment: "SetTarget(x) keeps a mask edit of x, so the order is free. Target-first matches the mask section's [m5] call order." |
| F7 | SMELL | layers_panel.rs:467-470 | blanket allow wider than its rationale | The module-wide `#![allow(dead_code)]` comment says `len`, `is_empty` and "the pure helpers" are test-only. Reachability in the bin target: `is_empty` is called (main.rs:640). All the pure helpers are reached through `apply` / `DropTracker` / `poll_layer_keys`: `click_mode`, `mask_click`, `layer_key_for`, `delete_needs_confirm`, `drop_slot`, `drop_to_reorder`, `contiguous_moves`, `display_order`, `span`. `resolve_pending` is reached through `confirm_ui`. Only `LayerSelection::len` is test-only (layers_panel.rs:1352). In the lib target, `pub mod layers_panel` (lib.rs:65) exports everything, so the allow does nothing there. The real gap it sits beside: `MaskSolo` changes only the row outline, and no viewport path reads it (documented at :450, :666-669). Alt+click "solos the mask in the viewport" in name only. | Replace with `#[cfg_attr(not(test), allow(dead_code))]` on `len` alone. Mark the solo as an outline-only stub in the UI or the kill report. |
| F8 | SMELL | first_run.rs:211-212 | unit error | `format!("{} {p:.0}%", …)` with `p` from `pressure_for_frame()`, which is normalised 0..1 (stylus_input.rs:48-56 treats 1.0 as full). It shows "Pressure: 0%"/"1%". | `{:.0}%` of `p * 100.0`. |
| F9 | SMELL | first_run.rs:54-60 | untranslated UI text | `SetupTheme::name()` returns hard-coded "Follow OS"/"Light"/"Dark" and these are shown as combo text. pt-BR users see English. `every_called_key_exists_in_en` cannot catch this because nothing calls `tr`. | Add `setup.theme-follow-os/-light/-dark` to both catalogs. |
| F10 | SMELL (pre-existing, out of scope) | document.rs:1454-1455 | missing key + scanner blind spot | `tr_args(\n "layers.mask-default-name", …)` is missing from BOTH catalogs. This dates to 770162f, not W-GUI-2. `scan_tr_keys` (i18n.rs:423) only matches `tr_args("` with no whitespace, so the line-broken call slips past the completeness test, and new mask layers get the raw key as their name. No W-GUI-2 `tr` call is line-broken: every key the four kills added resolves in both catalogs. | Add the key in both catalogs. Let the scanner skip whitespace after `(`. |
| F11 | SMELL | main.rs:640-642 | the selection reseeds itself | When the selection is empty, it is re-seeded from `active` every frame. Ctrl+clicking the only selected row off produces `select = None`, so there is no SetTarget and the paint target stays put. The next frame re-selects that layer, so a full deselect cannot stick. | Seed once per document or target change, e.g. track the last seeded `active`. |
| F12 | NIT | layers_panel.rs:958-984 | confirm window | No close X, Escape does nothing, and it stays up when the Layers tab is hidden. | `.open(&mut open)` and map Escape to Cancel. |
| F13 | NIT | main.rs:3600-3601 vs viewport.rs:799-807 | overlap in Hide-UI mode | The "Show UI" `Area` anchors RIGHT_TOP at (-8, 28) on a foreground layer. With the viewport docked top-right in Hide-UI mode it covers the Overlays button. | Offset one of them, or hide the Overlays header while `hide_ui`. |

## Seams checked clean (with evidence)

- **Draw order and borrows in main.rs.** The `CentralPanel::show` closure (3666-3734) holds `viewer` (field-disjoint `&mut self.state.*`, `&mut self.brush_panel`, `&mut self.hdr_mode`, `&self.gpu`). `DockArea::new(&mut self.dock)` ends after `show_inside`. `scroll_active_tabs_into_view(&mut self.dock, &mut self.tab_active_seen, …)` touches fields `viewer` does not borrow, and `viewer.home_actions` is used after it. Edition 2021 disjoint closure capture applies (Cargo.toml:28). `confirm_ui` (3773), `show_preferences` (3786) and `quick_setup.show(ctx, &self.state.stylus, &self.prefs)` (3794) run sequentially outside the closure. `FinishedSetup` carries no lifetime, so the `&self.prefs` borrow ends before the `self.prefs.*` writes. No E0500/E0502 pattern found.
- **Struct literals.** The `PanelViewer` fields (348-437) match the single literal (3670-3714) exactly; a script diffed the field sets with no gaps either way. `LayersUiOutcome` is built only through `::default()` (document.rs:608, 2434). It derives `Debug, Default`, and the new `Vec<LayerGesture>` has `Debug`. All three `layers_ui` call sites pass the new 8th arg (document.rs:1621, 1704; main.rs:648). `AppState` uses `#[derive(Default)]` plus `..AppState::default()` (1619). `GestureState: Default`. `reset_for_new` resets `layer_gestures` (1212).
- **Test module names.** main.rs has `layer_gesture_shell_tests` (4477), `new_project_tests` (4576), `pool_stage4_tests` (4818) and `tab_overflow_dock_tests` (5050), and no `mod tests`. layers_panel.rs has `gestures::tests` (1148) and the file-level `tests` (1516), which are different scopes. first_run.rs has its own `tests`. No collision. `crate::paint_state::PlaneOwner` in the shell tests resolves through the private `use umber_paint::paint_state` (main.rs:150), which descendants can see.
- **i18n.** The 12 `setup.*` attributes (en 134-145, pt 138-149) and the 10 `overlays.*` attributes match in both catalogs. No duplicate message or attribute in either file. Neither `setup` nor `overlays` existed at HEAD. Every reused key is present in both catalogs: `prefs.nav-*`, `prefs.keymap-preset-*`, `history.op-remove`, `history.op-reorder`, `shell.new-confirm-cancel`, `layers.mask-edit`, `layers.kind-folder`.
- **Tab-overflow constants vs egui_dock 0.21.1.**
  - `TAB_CLOSE/CLOSE_ALL/COLLAPSE/ADD_BUTTON_SIZE = 24.0` (style.rs:593-601).
  - `x_spacing = 8.0` with `text_width = galley.x + 2·x_spacing` (leaf.rs:1094-1095).
  - `close = TAB_CLOSE_BUTTON_SIZE.min(tab_bar.height)` only when `show_close_buttons && closeable` (leaf.rs:1096-1100). `DockArea` defaults to `show_close_buttons: true` (mod.rs:58), and `PanelViewer` does not override `is_closeable`, which defaults to true.
  - Width is `max(minimum_width, text+close)`, then `preferred.unwrap_or(0).at_least(..)`. `fill_tab_bar` defaults to false (style.rs:489), so `preferred` is `None` (leaf.rs:1103-1107).
  - `allocate_space(spacing)` before every tab except index 0 (leaf.rs:464-466) is equivalent to `dock_tab_widths`' trailing spacing.
  - `available_width = (outer − inner_margin) − close_all − collapse` (leaf.rs:234-254). main.rs sets neither `show_leaf_*` flag, so both default to true (mod.rs:71-72), which matches `dock_bar_width`. `add` defaults off.
  - The scroll clamp `[-overflow, 0]` (leaf.rs:1300) matches `scroll_into_view`.
  - `WidgetText::Galley` passes through `into_galley` unchanged. Its PLACEHOLDER color is replaced by `TextShape::new(.., tab_style.text_color)`.
  - `LeafNode::new` rect is `Rect::NOTHING` (non-finite width), so the not-laid-out guard is correct.
  - Test APIs exist: `Context::global_style` (context.rs:2175), `tex_manager().write().take_delta().clear()`, `PlacedRow: Deref<Target=Row>` → `Row::text() -> String`, `Galley::text() -> &str`, `LeafNode::set_active_tab -> Result`.
- **first_run close pattern.** `open` is a local, so the `.open(&mut open)` borrow does not conflict with the `close` flag written inside the nested closures. `self.open = open && !close` closes on Finish, Skip or the title X, and no path leaves it stuck open. It only *re-shows* because of F1, on the next launch.
- **egui 0.36 traps.** None hit. No `child_ui`, no `RawInput.modifiers` (modifiers come from `response.ctx.input(|i| i.modifiers)` / `Event::Key{modifiers}`), no const `Stroke::new`, no `ui.data`. `Popup::from_toggle_button_response/close_behavior/align(RectAlign::BOTTOM_END)`, `Button::selected`, `DragAndDrop::{payload,clear_payload}` and `Response::dnd_set_drag_payload` all exist with the signatures used. `Modifiers: BitOr` is present for the test `COMMAND | CTRL`.
- **Key routing.** Ctrl+D, Ctrl+G and Delete are not in `BINDINGS`/`SHORTCUT_KEYS` (toolbar.rs:222-244), so `poll_shortcuts` leaves those events for `poll_layer_keys`. That poll is gated on `ui.rect_contains_pointer`, which respects window layering. graph_canvas's Delete is untouched unless the Layers panel is hovered.

## Verdicts

- **g5-first-run: FINDINGS.** F1 (BLOCKER), F2, F4, F5, F8, F9.
- **g5-tab-overflow: CLEAN.** The constants and allocation model match egui_dock 0.21.1 source. One NIT-level fragility: the pinned test pins the restated arithmetic, not the dock itself, which is unavoidable since the sizes are `pub(crate)`.
- **g2-gesture-integration: FINDINGS.** F6, F7, F11, F12. All are SMELL/NIT. The wiring, construction sites and test modules are clean.
- **g6-overlays-popover (cross-kill only): FINDINGS.** F3 (Wireframe/Grid popover rows are overridden by the shell sync every frame), F13.

**BLOCKER count: 1** (F1). F2 becomes a hard battery failure on any host that already has a saved prefs file.


## Disposition (dragon, same session — pre-battery)

All findings fixed in the working tree before the wave gate (the
battery is host-parked; these fixes ride W-GUI-2's commit):

- F1 FIXED: `should_show` reads `prefs_path()` (the file) directly;
  PREFS_FILE import dropped.
- F2 FIXED: the probe test never writes; asserts both branches against
  whatever host state exists (file → no-first-run; absent → first-run;
  unresolvable → first-run). The stray-directory hazard is gone.
- F3 FIXED: `take_overlay_edit` drains into `view.wireframe/grid`
  BEFORE the shell sync; both field `allow(dead_code)`s removed;
  `overlays()`/`set_overlays()` allows reworded (test-reached).
- F4 FIXED: keymap applied before save; Umber resets an earlier
  Blender table (`KeymapPreset::Umber.overrides()` = default table).
- F6 FIXED (comment only): the ordering rationale now states the true
  contract (`set_active_target` keeps a mask edit only on the target's
  own layer; foreign edit cleared — the select-then-paint gesture).
- F8 FIXED: pressure × 100.
- F9 FIXED: theme labels via `setup.theme-follow-os/light/dark` keys
  (both catalogs).
- F10 FIXED: `layers.mask-default-name` added to both catalogs;
  `scan_tr_keys` now matches the call head then skips whitespace
  before the quote, so line-broken keys cannot slip past again.
- F11 FIXED: `GestureState::should_reseed_from` — an empty selection
  reseeds only when the paint target MOVED; a deliberate deselect
  sticks. Pinned by `an_empty_selection_reseeds_only_when_the_target_moves`.

Verification: 17/17 static checks re-derived from the tree; rustfmt
PASS on all five touched .rs files. Battery still owed (host down).
