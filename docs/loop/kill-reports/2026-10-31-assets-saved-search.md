# Kill report: 2026-10-31 assets-browser saved search IMPLEMENTED

- **Kill**: the R-C1 follow-up "saved searches" v1 — the browser's
  search string persists across sessions (named multi-save stays the
  named follow-up).
- **Lock**: `wave-asset-search` over the 3 touched files.

## What landed

- **`preferences.rs`** — `ASSETS_SEARCH_FILE` + `assets_search_path()`
  (the state dir beside `prefs`/`swatches`, same `user_preset_dir`
  parent pattern).
- **`assets_panel.rs`** — `AssetsPanel` gains `restored`/`last_saved`;
  `restore_search` reads the file once per session (newlines
  stripped — the field is single-line), `persist_search` writes only
  on change. `library_ui` takes the path and runs both around the
  rows. 1 unit test: round trip through a real file, no-op write
  keeps mtime, newline stripping.
- **`main.rs`** — the Assets arm passes `preferences::assets_search_path()`.

## Verification

- `cargo test -p umber-app --bin umber assets_panel::` — 1/1.
- Gate-1 battery GREEN: layers + fmt + clippy -D warnings + gpu leg +
  umber-app 894 tests 0 failed. CI on the pushed tip = the wave-end
  gate (thumbs wave's CI 41216677353 SUCCESS on fc552ca preceded it).

## Not in scope

- Named/multiple saved searches (the fuller reading of the follow-up):
  v1 persists the current query only.
- The browser kittest — blocked on the dual-crate split: the kittest
  harness links the LIB target while `assets_panel` lives in the bin
  (its `BrushPanel` dependency is bin-only). The design that unblocks
  it: extract a plain `&[BrushRow]` seam in `library_ui` and move
  assets_panel + asset_thumbs into the lib. Named, not started.
