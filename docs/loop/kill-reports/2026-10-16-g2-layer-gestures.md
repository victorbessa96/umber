# Kill report: g2-layer-gestures (GUI plan G2)

Date: 2026-10-16. Author: claw (wgui2 batch, KILL B). Lock:
`wgui2-layer-gestures` over `crates/umber-app/src/layers_panel.rs`.
Plan: `docs/specs/gui-plan-2026-10-15.md` row `g2-layer-gestures`
(P:B1, B:B7, B:E10). Status: UNCOMPILED and uncommitted. No cargo ran
(brief rule); the dragon gates it on the build host.

## Built (`layers_panel.rs`, new child module `layers_panel::gestures`)

Everything is a seam. The rows (`document::layers_ui`) and the shell
(`main.rs`) are outside the lock, so the module returns gestures and
outcomes, and the out-of-lock callers dispatch them.

- **Pure mappings:** `click_mode(mods) -> ClickMode` (Replace / Toggle /
  Range / AddRange), `mask_click(mods) -> MaskClick` (Alt = Solo, Shift =
  ToggleEnabled, plain = Edit; Alt wins), `layer_key_for(key, mods) ->
  LayerKey` (Ctrl/Cmd+D, Ctrl/Cmd+G, Delete), and
  `delete_needs_confirm(DeleteSource)` (the ✕ button confirms, the key
  does not).
- **`poll_layer_keys(ctx)`:** consumes the first panel chord by removing
  its event, so later `key_pressed` and the shell poll never see it.
  It yields nothing while a text edit has focus, so a rename never
  deletes the layer.
- **`LayerSelection`:** a set plus an anchor and a primary. It covers
  replace, toggle, anchored range (the anchor stays put, file-manager
  style), add-range, and `retain_existing` after deletes/undo.
- **`MaskSolo`:** an Alt+click toggle per layer, pruned when its layer
  disappears.
- **Reorder:** `drop_slot(y, spans)` picks the gap,
  `drop_to_reorder(len, from, slot)` turns a display-order gap into a
  stack `(from, to)` (no-op drops return `None`), and `DropTracker`
  draws the drop bar and yields `LayerGesture::Drop` on release.
  `drag_handle` carries a `LayerDrag` payload via egui's DragAndDrop.
- **Group:** `contiguous_moves` gathers the selected stack indices into
  one block that ends at the top selected index, keeping relative order.
  A `Folder { passthrough: false }` is then added directly above. Every
  step is an undoable command.
- **`GestureState::apply(doc, LayerGesture) -> GestureOutcome`:**
  - Select, mask Solo/Toggle/Edit.
  - Duplicate: every selected layer through the C2-8 `duplicate_layer`,
    highest index first. It returns `(source, copy)` pairs and selects
    the copies.
  - Group.
  - Delete: the key removes now; the button sets `pending_delete`.
  - Drop.

  `resolve_pending(doc, ConfirmChoice)` and `confirm_ui(ctx, doc)` follow
  the File ▸ New confirm-window shape.
- **Row widgets:** `row_click_gesture(&Response, id)` and `mask_thumb(ui,
  id, tex, enabled, soloed)`. The mask thumb is a 28 px slot: an outline,
  a selection stroke when soloed, and a red strike when disabled.

Tests (15, `layers_panel::gestures::tests`):
`click_modes_follow_the_modifiers`,
`mask_clicks_map_alt_solo_shift_toggle_plain_edit`,
`layer_keys_map_only_their_chords`,
`confirm_gate_confirms_the_button_not_the_key`,
`selection_replace_toggle_range_add_range`,
`selection_and_solo_prune_vanished_layers`,
`drop_slot_counts_rows_above_the_pointer`,
`drop_to_reorder_converts_display_gaps_to_stack_moves`,
`contiguous_moves_gather_below_the_top_selected`,
`group_gathers_the_selection_under_a_new_folder_and_undoes`,
`duplicate_copies_each_selected_layer_and_selects_the_copies`,
`delete_key_removes_now_and_the_button_waits_for_confirm`,
`mask_gestures_solo_toggle_and_edit`,
`drop_and_select_gestures_drive_the_stack`,
`poll_consumes_the_chord_and_yields_to_text_fields`.

## Judgment calls

- **Alt/Shift belong to the MASK thumbnail; Shift on a ROW is the range
  select.** This follows the plan row's wording (the mask-thumbnail
  gestures), so the two never collide. "Toggle" flips the mask's
  `enabled` flag (`LayerCommand::SetMaskEnabled`), as in Painter. The
  rejected reading was "Shift+click toggles layer visibility", which
  would take Shift away from multi-select.
- **Group without membership.** `umber_core::layers` does not model
  folder membership yet. "Group" therefore produces the stack shape a
  folder will own once membership exists: the selection made contiguous,
  with a new folder directly above it. Follow-up: when membership lands,
  `group_selected` adds the children to the folder.
- **No new catalog keys.** The module reuses keys that exist in both
  catalogs (checked): `history.op-remove` (the confirm title and button),
  `history.op-reorder` (drag-handle tooltip), `shell.new-confirm-cancel`,
  `layers.kind-folder` (the folder name, the C2-8 add-folder rule), and
  `layers.mask-edit` (mask-thumb tooltip). The confirm body is the layer
  names, which are user data. The tooltips do not name the Alt/Shift
  gestures. A dedicated `layers.mask-thumb-tip` key is the follow-up.
- **Delete key = Delete only, not Backspace.** Backspace stays with text
  and the graph canvas. Multi-delete is one `Remove` command per layer,
  so undo restores one layer per step. Coalescing them belongs to the
  C2-7 one-timeline work.
- **Mask solo is state only.** No viewport mask-solo render exists. The
  only solo today is `ViewMode::Solo(SoloChannel)` in
  `umber-gpu/src/view_mode.rs`, a paint-channel solo. The seam is
  `MaskSolo::layer()`, and the render belongs to the dragon (below).
- **`#![allow(dead_code)]` on the module.** The bin target sees no
  caller until the wiring lands. This is `allow`, never `expect`, per
  the pub-on-lib rule.

## Wiring the dragon owns (outside this lock)

1. **`document::layers_ui`:**
   - Take `&mut GestureState`.
   - Replace each row's `selectable_label(...).clicked()` with
     `gestures::row_click_gesture` (highlight with
     `selection.contains(id)`).
   - Add `gestures::drag_handle` and `gestures::mask_thumb` on rows
     whose layer has a mask. The mask texture is the open half of
     g2-layer-thumbnails.
   - Push each row's `ui.horizontal(..).response.rect` into a
     `DropTracker` and call `finish(ui)` after the loop.
   - Route ✕ through `LayerGesture::Delete { source: RowButton }`.
   - Return the gestures (or the merged `GestureOutcome`) on
     `LayersUiOutcome`.
2. **`main.rs`:**
   - Own one `GestureState`.
   - While the Layers dock tab is hovered or focused:
     `poll_layer_keys(ctx)` → `state.key_gesture(k)` → `apply`. Poll
     BEFORE the shell's `actions` poll. No `actions.rs` registration is
     needed, and none of Ctrl+D, Ctrl+G or Delete is bound there today
     (checked). Registering them in `actions.rs` for the keymap editor
     is an option, not a requirement.
   - Call `confirm_ui(ctx, doc)` each frame.
   - Apply each `GestureOutcome`:
     - `select` → `set_active_target(Layer(id))`;
     - `duplicated` → `copy_owner_planes(Layer(s), Layer(c))` per pair;
     - `mask_edit` → `set_active_target` + `set_mask_edit_layer(Some(id))`
       (the mask section's order);
     - `removed` → prune thumbnails and planes, as the ✕ path does today.
3. **Viewport:** render `mask_solo.layer()`'s mask plane as grayscale
   while it is `Some`. This needs a render path in viewport/renderer,
   which are under the overlays sibling lock.

## Unverified

- Nothing compiled. Every egui call was resolved against the pinned
  egui 0.36.2 sources: `Response::dnd_set_drag_payload`,
  `DragAndDrop::{payload, clear_payload}`, `Context::text_edit_focused`,
  `pointer_hover_pos`, `Painter::{hline, image, line_segment,
  rect_stroke}`, `Label::sense`, `Window::id`, `Rangef::{new, contains}`,
  `Rect::x_range`, the `Modifiers` consts and BitOr, and
  `InputState::events`.
- rustfmt `--check --edition 2021` is clean on the file.
- `drop_slot`, `drop_to_reorder` and `contiguous_moves` were ported to
  Python. The pinned cases pass there, plus an exhaustive check for n ≤ 6:
  every drop lands the row in the gap it was dropped on, and every gather
  is contiguous, keeps order, ends at the top, and leaves the unselected
  layers' order unchanged. This is ad-hoc verified, not suite-verified.
- The drawing helpers (`mask_thumb`, `drag_handle`,
  `DropTracker::finish`, `confirm_ui`) have no test. The plan wants a
  kittest per gesture through AccessKit roles, which needs the
  `layers_ui` wiring first.

## Deferred

- Kittests per gesture (after the wiring).
- Viewport mask-solo render.
- The mask-thumbnail texture.
- Folder membership for Group.
- One-entry undo for multi-delete and multi-duplicate.
- A gesture-naming tooltip key (`layers.mask-thumb-tip`) in both
  catalogs.

## Process note

- The tree also had a sibling's dirty `main.rs` and `display_panel.rs`
  (the tab-overflow and overlays locks). This kill did not touch them.
- The only file written besides this report is `layers_panel.rs`.
