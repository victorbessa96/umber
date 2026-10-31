# Kill report: 2026-10-31 Assets browser kittest IMPLEMENTED

- **Kill**: the R-C1 follow-up "the browser kittest".
- **Lock**: `wave-browser-kittest` over assets_panel.rs.

## What landed

- **`assets_panel.rs` `mod kittest`** (bin-side: the panel is a bin
  module, so the kittest drives `library_ui` through an egui_kittest
  harness directly — no lib move needed; the larger dual-crate
  unification stays named in the saved-search report):
  - `browser_lists_bundled_presets_and_applies_on_plus` — the bundled
    library (assets/smart) lists "Painted wood" / "Worn metal" /
    "Edge wear" rows; the first ＋ (materials render first, name-sorted)
    produces `LibraryOutcome.material == Painted wood`.
  - `search_filters_the_library_rows` — typing "worn" leaves only
    "Worn metal".
- **AccessKit naming** — the drag-only row labels now carry
  `widget_info` names (the g7-accesskit-sweep pattern from
  `drag_handle`): drag labels are otherwise unlabeled in the tree.

## Verification

- `cargo test -p umber-app --bin umber assets_panel::` — 3/3.
- Gate-1 battery GREEN: layers + fmt + clippy -D warnings + gpu leg +
  umber-app 900 tests 0 failed.
- CI note: the import-dialog push (3e92299) went red on the ubuntu
  scorecard leg only — the README test count went stale with the new
  tests; regenerated (`14d8392`, 2,200+ → 2,300+). Windows + classifier
  + layering legs were green.

## Not in scope

- Drag-drop kittest (egui_kittest dnd simulation) — named follow-up.
- Moving assets_panel to the lib for tests/-side kittests — the
  dual-crate unification, named, not started.
