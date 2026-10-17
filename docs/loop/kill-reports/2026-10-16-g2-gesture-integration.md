# Kill report: wgui2-gesture-integration (g2-layer-gestures wiring)

- Authorship: claw (dispatched by the dragon), brief
  `docs/loop/briefs/2026-10-16-wgui2-gesture-integration.md`.
- Kill-lock `wgui2-gesture-integration`: `crates/umber-app/src/document.rs`,
  `crates/umber-app/src/main.rs`, `crates/umber-app/src/layers_panel.rs`.
- Implements GUI plan row `g2-layer-gestures` (docs/specs/gui-plan-2026-10-15.md)
  on top of the landed seams in `layers_panel::gestures`.
- Status: UNCOMPILED. No cargo run (brief rule); rustfmt `--check` clean on
  all three files. Battery owed by the dragon on the build host.

## Built

- `document.rs`
  - `LayersUiOutcome.gestures: Vec<LayerGesture>`: the rows' gestures this
    frame, in draw order.
  - `layers_ui` takes a new `gestures: &GestureState` param (read-only:
    the multi-selection highlight and the solo outline).
  - Each row: a `drag_handle` first, the content thumbnail, a
    `mask_thumb` slot when the layer has a mask (`enabled` from
    `LayerMask::enabled`, `soloed` from `mask_solo`), and the name click
    through `row_click_gesture` (it replaces the three old
    `outcome.select` name-click branches; the hover hints stay).
  - Each row's rect goes into a `DropTracker`; `finish` runs after the
    rows inside the ScrollArea and pushes the `Drop` gesture.
  - ✕ now pushes `Delete { source: RowButton }` (the confirm gate) instead
    of running `LayerCommand::remove` itself; `remove_id` is gone.
  - Kept unchanged: ▲/▼, ⧉ duplicate (legacy `outcome.duplicated`), add
    buttons, shelf, Base row (`outcome.select`).
  - The existing kittest call site gets `&GestureState::default()`.
- `main.rs`
  - `AppState.layer_gestures: GestureState` (the selection + `MaskSolo` +
    `pending_delete` already live inside it); `reset_for_new` resets it.
  - `PanelViewer.layer_gestures: &mut GestureState`, wired at the viewer
    literal.
  - LayerStack tab: seeds the selection from the paint target when the
    selection is empty; polls `poll_layer_keys` only while
    `ui.rect_contains_pointer(ui.max_rect())` (before the rows draw, so
    the chord is consumed first); appends `key_gesture(key)` to the rows'
    gestures; drains them all through `GestureState::apply`.
  - Outcome → session: free fns `layer_session_ops(&GestureOutcome) ->
    Vec<LayerSessionOp>` (pure) + `apply_layer_session_ops(paint, ops)`.
    Order: `CopyPlanes` per duplicated pair → `EndMaskEditIfRemoved` →
    `SetTarget(mask_edit.or(select))` → `EditMask` (the [m5] order:
    `set_active_target` clears a foreign mask edit, so the target lands
    first).
  - `sync_gesture_selection`: selections made OUTSIDE the gestures (add
    buttons, shelf, ⧉, Base row) follow into the multi-selection (a layer
    selects alone, Base clears it).
  - The ✕ confirm: `layer_gestures.confirm_ui(ctx, doc)` drawn after the
    dock beside the toasts/Preferences; its removals run through the same
    session mapping.
- `layers_panel.rs`: only the `#![allow(dead_code)]` comment re-worded
  (it now names the integration). The allow stays: `LayerSelection::len`,
  and some pure helpers are still reached only from tests. No pub
  additions.

## Judgment calls

- Dispatch design: the rows RETURN raw gestures through
  `LayersUiOutcome.gestures`, and the shell applies them through its one
  `GestureState`. The alternative (`&mut GestureState` into
  `layers_ui`, applied inside the rows) was rejected: the key poll and
  the confirm window both live in the shell, so applying in the shell
  keeps one apply site for all three sources, and `layers_ui` stays
  free of paint-session effects like its existing outcome contract.
- Key-poll scope: pointer over the LayerStack tab body. egui_dock calls
  `ui()` only for the visible tab, so the hover check is "visible AND
  hovered". `tab_active_seen` was rejected: it tracks scroll state per
  leaf, and the Layers tab being active in its leaf does not mean the
  user is working in it (the viewport's or graph's Delete would be
  stolen).
- Selection seeding from `active` when the selection is empty: without
  it, Delete / Ctrl+D act on nothing until the user clicks a row, while
  the row already highlights.
- Removed layers: planes are NOT dropped. Delete is undoable and
  `copy_owner_planes`/the sidecar prune by live set on save
  (`prune_layer_masks(&live)`); dropping planes on delete would make undo
  restore an empty layer. Thumbnails already prune per frame
  (`thumbnails.prune(&live)` in the tab). Only a mask edit on a removed
  layer is ended.
- `solo_changed`: no render path reads `MaskSolo` (the gestures module
  docs say so); the UV view's overlay keys on the QUICK mask's
  generation, not layer masks, so nothing refreshes there. The shell
  requests a repaint for the row outline; that is the whole consumer.
- The name highlight is `selection.contains(id) || active == Layer(id)`:
  multi-select shows every selected row, and an auto-resolved target
  still shows before the seed lands.

## Unverified

- Everything compiles only by construction: no cargo ran. Names checked
  by grep against the tree: `copy_owner_planes`, `set_active_target`,
  `set_mask_edit_layer`, `mask_edit_layer`, `Ui::rect_contains_pointer`
  (egui 0.36.2 `ui.rs:1004`), `Harness::state_mut` (egui_kittest 0.36.2).
- `apply_layer_session_ops` needs a GPU `PaintState`: no headless test.
- Drag-reorder end to end (handle → payload → drop bar → `Drop`) has no
  kittest; the pure `drop_slot`/`drop_to_reorder` stay pinned in
  `layers_panel::gestures::tests`.
- The mask thumbnail slot draws without a texture (`tex: None`): no layer
  mask thumbnail source exists (`thumbnails.rs` reads only baseColor).

## Tests added

- `main.rs` `layer_gesture_shell_tests`:
  `an_empty_outcome_touches_nothing`,
  `a_duplicate_copies_planes_before_selecting_the_copy`,
  `a_mask_edit_targets_the_layer_first_then_its_mask`,
  `a_removal_ends_its_mask_edit_and_retargets`,
  `outside_selections_follow_into_the_gesture_selection`.
- `main.rs` `reset_for_new_restores_the_boot_fresh_shape`: extended to
  dirty + assert `layer_gestures` reset.
- `document.rs` `layer_rows_return_gestures_instead_of_applying_them`
  (kittest): a name click yields `Select{Replace}`; the ✕ yields a
  `RowButton` delete and removes nothing, no history entry.
- Bin target: run with `cargo test -p umber-app --bin umber --
  layer_gesture_shell_tests` (main.rs-private module); the document.rs
  test is on the lib or bin target per where `document` is compiled.

## Deferred

- Mask thumbnail texture (a layer-mask read path in `thumbnails.rs`).
- Viewport render of `MaskSolo` (no render path exists).
- ⧉ duplicate stays on the legacy single-layer path; Ctrl/Cmd+D is the
  multi-selection duplicate. Folding ⧉ into `LayerGesture::Duplicate`
  would need the row to set the selection first.
- No Delete-key hint in the UI (no catalog key; catalogs were not
  touched).

## Process note

- The shared tree carried a sibling's dirty `main.rs`/`layers_panel.rs`
  hunks (tab-overflow, first-run, the gestures module itself) under this
  lock's files; only this kill's hunks were added. The dragon's
  per-kill gate must copy these three files as a unit with the
  g2-layer-gestures module they depend on.
