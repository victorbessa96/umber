# Kill report: 2026-10-31 import dialog in the Assets tab IMPLEMENTED

- **Kill**: the R-C1 follow-up "import dialog" — the mesh-map, PSD and
  .spp import sections move out of the Assets tab body into one
  Import… window.
- **Lock**: `wave-import-dialog` over the 4 touched files.

## What landed

- **`assets_panel.rs`** — `AssetsPanel::import_open` (the window's
  open flag, session state like the search).
- **`main.rs`** — the Assets arm renders an "Import…" button and an
  `egui::Window` (non-collapsible, `.open(&mut import_open)`) holding
  the three sections unchanged: `mesh_import.show(...)`,
  `psd_import::show_section(...)`, `spp_import::show_section(...)`.
  The inline sections are removed from the tab body.
- **i18n** — `assets.import` / `assets.import-title` (en-US + pt-BR).

## Verification

- Gate-1 battery GREEN: layers + fmt + clippy -D warnings + gpu leg +
  umber-app 898 tests 0 failed. CI on the pushed tip = the wave-end
  gate (prior wave's CI 41225793518 SUCCESS on e84409d).

## Not in scope

- Drag-and-drop of files onto the window (the sections' own pickers
  remain the entry).
- A unified progress/result area across the three importers.
