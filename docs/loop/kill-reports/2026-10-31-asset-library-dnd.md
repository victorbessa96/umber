# Kill report: 2026-10-31 asset-library drag-to-layers IMPLEMENTED

- **Kill**: `asset-library-dnd-impl` — materials/masks in the Assets
  browser are now draggable onto the Layers panel.
- **Lock**: `wave-asset-dnd` over the 5 touched files.

## What landed

- **`layers_panel.rs` (gestures)** — new payload types
  `AssetDropRef::{Material,Mask}(name)` + `AssetDrag(AssetDropRef)`
  (Eq-safe, egui drag payload), re-exported at the module root together
  with `drop_to_reorder`. New gesture `LayerGesture::AssetDrop { asset,
  slot }`: the `DropTracker` accepts `AssetDrag` payloads on the same
  per-row drop targets, converts the drop slot through the existing
  display-gap mapping (`drop_to_reorder`), and hands
  `(ref, slot, layer_under_drop)` up as `GestureOutcome::asset_drop`
  WITHOUT touching the document (the panel cannot create material layers
  — generators bind bakes). 1 new test: hand-up carries ref + resolved
  slot + the row under the drop; slot past the end clamps to the last row.
- **`assets_panel.rs`** — material and mask rows now call
  `.draggable_text(...)` instead of plain labels, carrying an
  `AssetDrag` payload; brush rows stay click-only (a brush is not a
  layer). New i18n hint `assets.drag-to-layers` (en-US + pt-BR) states
  the two drop semantics.
- **`main.rs` (shell)** — the `asset_drop` outcome resolves the name
  against the same library scan the browser rendered, then:
  - **Material** → the ＋ button's exact flow, extracted into
    `PanelViewer::apply_library_material` (apply → bind generators to the
    newest bakes → select the top layer as the paint target), now shared
    by the button and the drop.
  - **Mask** → `bind_smart_mask(self.doc, &mask, layer_under_drop)` on
    the layer under the drop slot (or the selected layer when the drop
    missed every row).

## Verification

- `cargo test -p umber-app --bin umber layers_panel::` — 23 passed
  (22 prior + 1 new asset-drop test).
- `cargo fmt --check` clean; `cargo clippy -p umber-app --all-targets
  -- -D warnings` clean; Gate-1 battery run on umber-app (see battery
  log); CI on the pushed tip via the single wave-end gate.

## Not in scope

- Dragging a material onto the VIEWPORT (no drop target there yet) —
  the panel drop is the landed lane.
- Dropping between the panel and a folder row groups differently;
  the slot mapping treats folder headers as rows, same as reorder.
