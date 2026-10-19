# Kill brief: wgui2-gesture-integration — wire the g2-layer-gestures seams into the rows + shell

You are integrating a LANDED seam module into Umber's UI, in the Rust
workspace at ~/Projects/umber (HEAD 9acdc1c). The seams exist and are
tested in `crates/umber-app/src/layers_panel.rs` (child module
`layers_panel::gestures`, 15 pure tests). Your job: make the ROWS use
them and the SHELL dispatch them. No cargo (the dragon gates on the
build host). No commits. Lock:
`crates/umber-app/src/document.rs`, `crates/umber-app/src/main.rs`,
`crates/umber-app/src/layers_panel.rs` (edits to the gestures module
only if an integration needs a tiny pub addition — name any).

## The seams (read layers_panel.rs:500-1150 first)

- `gestures::{click_mode, mask_click, layer_key_for, delete_needs_confirm,
  poll_layer_keys(ctx) -> Option<LayerKey>}` — pure mappings + the
  key poll (consumes the chord's event; text fields exempt).
- `LayerSelection` — ids + anchor + primary; replace/toggle/range/
  add-range; `retain_existing`.
- `GestureState::apply(doc, LayerGesture) -> GestureOutcome` —
  Select/Mask/Duplicate/Group/Delete/Drop; `resolve_pending`,
  `confirm_ui`.
- `row_click_gesture(&Response, id) -> Option<LayerGesture>`,
  `mask_thumb(ui, id, tex, enabled, soloed) -> Option<LayerGesture>`,
  `drag_handle(ui, id) -> Response` (carries `LayerDrag(id)`),
  `DropTracker` (paints the drop bar, yields Drop gestures).
- `GestureOutcome { select, duplicated, mask_edit, removed, solo_changed }`.

## Integration work

1. **document.rs `layers_ui`**: each layer row gains
   - the `row_click_gesture` (replacing the current plain-name-click
     select path where they overlap — keep the existing behavior for
     anything the gesture does not cover),
   - the `mask_thumb` slot (beside the row; `enabled` from the layer's
     mask state, `soloed` from the panel's MaskSolo — thread the state
     through `MaterialPanelContext` or a new param; check what it
     already carries),
   - the `drag_handle` + `DropTracker` reorder (the rows today have
     up/down buttons or none — the drag becomes the primary reorder,
     keep the buttons if they exist).
   The panel needs a `GestureState` — `layers_ui`'s signature may gain
   a `&mut gestures: GestureState` param (main.rs holds it) OR the
   rows return raw gestures through `LayersUiOutcome` and the shell
   applies them through its own GestureState. Pick the cleaner one,
   justify it, and note that `LayersUiOutcome` may gain a
   `gestures: Vec<LayerGesture>` field (main.rs dispatches them all
   through `GestureState::apply` after the panel closes).
2. **main.rs**:
   - hold `layer_gestures: layers_panel::gestures::GestureState`
     (+ `LayerSelection`/`MaskSolo` inside it if the type wraps them —
     check `GestureState`'s fields),
   - poll `poll_layer_keys` ONLY while the Layers panel's dock tab is
     the visible/active one or the pointer hovers the panel (check how
     the app knows which tab shows — `tab_active_seen` from the
     tab-overflow kill, or the dock's tree; use the simplest honest
     signal) and dispatch the LayerKey through `apply`,
   - after the panel draws: drain the returned gestures through
     `GestureState::apply`, then apply the `GestureOutcome` to the
     paint session: `select` → `PaintState::set_active_target`
     (PlaneOwner::Layer), `duplicated` pairs →
     `PaintState::copy_owner_planes`, `mask_edit` → the mask-edit
     order, `removed` → prune thumbnails + planes if such a path
     exists (grep how deletes clean up today), `solo_changed` → the
     UV view's mask overlay key refresh (it keys on mask generation —
     likely automatic; verify + name),
   - `resolve_pending` + `confirm_ui(ctx, doc)` drawn with the other
     shell windows (the File>New confirm pattern).
3. **Tests** (pure, in the files you touch): the gesture→outcome→
   session mapping fns you add in main.rs (free functions, tested
   headless); document.rs row-level tests where the existing suite
   has them (grep `mod tests` in document.rs and follow its fixtures).

## Rules

- i18n: reuse existing keys (the gestures module already reuses
  history.op-*, layers.*, shell.new-confirm-*). New strings only if a
  row element genuinely has none — both catalogs then.
- egui 0.36 (no Modal; DragAndDrop payload via `ctx.data`/`Ui::put` —
  check how `drag_handle` already does it and follow).
- Rust 2021, clippy -D warnings clean, rustfmt clean.
- Kill report: docs/loop/kill-reports/2026-10-16-g2-gesture-integration.md
  (shape: Built / Judgment calls / Unverified / Deferred / Process note).
- Reply with: files changed, the dispatch design chosen + why, test
  names, anything the dragon still owes.
