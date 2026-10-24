# Brief — R-C1: the Assets panel's library browser (the census's bigger P0 moat)

Written 2026-10-24 13:5x by local-7de7d3ed, derived live. Source:
the SPP census row C1 + the 2026-10-24 SPP lens + this derivation.

## The census's own words, and what the tree already holds

Census C1: "Assets window: starter + user libraries, category
filters, saved searches, drag-out, import dialog, custom library
locations, thumbnails … R-C1 the Assets panel: libraries of
brushes/smart materials/masks/filters/meshes/env maps, filterable,
drag-to-layer-stack — this is Umber's biggest single UX gap after
the paint loop."

The derivation's findings (all verified live):

- **The Assets TAB exists** (main.rs:202 `Panel::Assets`, the arm
  at :1141) but holds a placeholder label + the three import
  sections (mesh-map/PSD/.spp). The census's "ABSENT as a panel"
  means the LIBRARY BROWSER, not the tab.
- **The loaders exist and are hardened**:
  `document::list_smart_materials(dirs)` (:1004 — sorted, dedup'd
  by file name, first-dir-wins, bad files skipped with a log) and
  `list_smart_masks(dirs)` (:1038, same rules).
- **The library dirs exist**: `smart_dirs()` (:992) — the repo's
  `assets/smart` starter + the exe-dir `smart/` (the installed
  side). The brush shelf: brush_panel.rs:794's `.umberbrush`
  listing + the `.stamp.png` tips + the reload path.
- **The apply paths exist**: the smart-material apply (the
  document.rs:1282-1294 shelf flow → add a material layer);
  `compose_smart_mask` (:1086); the brush preset load (the shelf's
  own click path).

## The v1 slice (this kill) — the DISCOVERABILITY layer

The census's diagnosis: "capability exists, discoverability does
not." The v1 closes exactly that:

1. **The Assets tab gains a LIBRARY section** above the imports:
   a search field (substring, case-insensitive) + three
   collapsible categories — Brushes, Smart materials, Masks —
   each listing its library (the existing loaders), the rows
   filtered by the search.
2. **Apply-on-click**: a brush row loads the preset into the
   brush; a smart-material row adds the material layer (the
   existing shelf flow); a mask row composes onto the SELECTED
   layer's mask (`compose_smart_mask`'s existing path).
3. **The rows show name + source dir** (the starter vs user
   distinction the census names; a tooltip carries the full
   path).
4. **The refresh**: a reload button per category (the brush
   shelf's own reload precedent).

## The named follow-ups (not v1)

- Drag-to-layer-stack (the drag payload + the drop targets — the
  LayerDrag precedent exists).
- Thumbnails (the layer thumbnails' content-keyed pattern).
- Saved searches, custom library locations, the filter/mesh/env
  categories (no loaders exist for those yet).
- The import dialog (the three import sections MOVE into the
  panel's flow in a later pass).

## Tests

- The list fns' contracts are already pinned (the existing suite);
  the NEW surface: a kittest that the Assets panel renders the
  library rows from a fixture dir + the search filters them + a
  click applies (the smart-material path at minimum — the
  material layer appears in the stack).

## Verification ladder

fmt; the battery's umber-app leg (the kittests + i18n
completeness — new keys: the panel's category headers + search
placeholder + empty states, BOTH catalogs); the ROADMAP's R-C1 row
closes to the LANDED archive; the report names the follow-ups.

## Non-goals

- No new loaders, no new formats, no filesystem writes.
- No drag-drop, no thumbnails (named above).
- No changes to the import sections (they stay as-is in v1).
