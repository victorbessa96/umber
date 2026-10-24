# Kill report — R-C1 v1: the Assets panel's library browser (the census's bigger P0)

Executed 2026-10-24 15:2x–15:4x by local-7de7d3ed, from
`docs/loop/briefs/2026-10-24-rc1-assets-panel.md` (brief-first;
the derivation's finding: the census's "absent" pieces are the
engines' own pub surfaces — the kill is the DISCOVERABILITY
layer over them).

## What landed

### The module (crates/umber-app/src/assets_panel.rs)

- **`AssetsPanel`** — the session state (the search field; the
  lists cache in egui memory per frame-group, the smart shelf's
  own precedent).
- **`library_ui(ui, state, brush, selected_layer) ->
  LibraryOutcome`** — the search field (substring,
  case-insensitive) + the three categories:
  - **Brushes**: the brush panel's OWN loaded library (never
    re-scanned — the shelf is the source of truth), each row
    `name + ▸ apply` with the source path in the hover.
  - **Smart materials**: `document::list_smart_materials
    (smart_dirs())` — the hardened loader (sorted, dedup'd,
    first-dir-wins, bad files logged never fatal) — cached in
    egui memory, each row `name + ＋ apply`.
  - **Masks**: `document::list_smart_masks(smart_dirs())` — the
    same rules; a row applies only when a layer is selected.
- **`LibraryOutcome`** — `{ brush: Option<String>, material:
  Option<SmartMaterial>, mask: Option<(u64, SmartMask)> }` — the
  shell owns the applies (the panel touches no session).

### The seam (brush_panel.rs)

**`BrushPanel::select_preset_by_name(name) -> bool`** — the shelf
combo's own pick path minus the combo: find by name, set active,
arm `preset_changed` (the session syncs on take), clear dirty,
prefill Save-As. `false` = a stale row (logged by the caller).

### The extraction (main.rs)

**`compose_mask_flow(paint, doc, bakes, layer, mask)`** — the
C2-10 smart-mask compose flow (the per-tile compose with the
anchor reads + the bake PNGs linearized) extracted VERBATIM from
the Layers panel's arm into a free fn — the Layers arm's 84-line
block became one call, and the Assets arm calls the same. THE
DEDUP: one compose path, two doors.

### The wiring (main.rs's Panel::Assets arm)

The placeholder label is dead. The arm runs the library browser,
then applies each outcome through the EXISTING engines:
- brush → the seam (a stale name logs).
- material → `apply_smart_material` + `bind_smart_generators`
  (the newest bakes) + select the top + sync the gesture
  selection — the shelf's own three-step, verbatim.
- mask → `LayerCommand::attach_mask(target, name)` (the journal
  binds the mask to the layer — the Layers panel's own order:
  attach THEN compose) + `compose_mask_flow`.
The import sections stay below, unchanged.

### The i18n

The new `assets` group (search/brushes/materials/empty) in BOTH
catalogs — the lock is lifted, the completeness test enforces
parity. The dead placeholder key stays in the catalog (orphan
keys pass the code→catalog direction).

## Tests

The battery's umber-app leg: the kittests + the i18n completeness
(the new keys' parity). The list fns' contracts were already
pinned. The named follow-up: a kittest that the browser renders +
filters + applies (the brief's test row — the next pass).

## The derivation's own misses (named)

1. The brush-panel patch MANGLED `active_preset`'s signature (a
   bad old_string pair) — restored in the next patch before any
   compile, then the seam added cleanly.
2. `entry.path` → `source_path` (the LibraryEntry's real field).
3. The bare `document::` paths needed `crate::`.
4. The egui temp read wraps `Option<Option<Vec>>` — the flatten
   named in the fix.

## The named follow-ups (the brief's list, unchanged)

Drag-to-layer-stack (the LayerDrag precedent), the content-keyed
thumbnails, saved searches, custom library locations, the
filter/mesh/env categories (no loaders exist), the import-dialog
integration, the browser kittest.

## Non-goals held

No new loaders, no new formats, no filesystem writes, no changes
to the import sections.
