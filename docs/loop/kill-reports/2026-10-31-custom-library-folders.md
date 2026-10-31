# Kill report: 2026-10-31 custom library folders IMPLEMENTED

- **Kill**: the R-C1 follow-up "custom library locations" — the user
  can add their own folders to the smart-material / smart-mask library
  search path, persisted across sessions.
- **Lock**: `wave-asset-extradrs` over the touched files.

## What landed

- **`document.rs`** — `SMART_EXTRA_DIRS_FILE` (`smart_dirs` in the
  state dir beside prefs), `extra_dirs_path`, `load_extra_dirs`
  (trims, drops blanks/duplicates/non-directories), `save_extra_dirs`,
  and `smart_dirs()` now appends them after the built-ins via the
  testable core `smart_dirs_with` (built-ins first, so they win the
  listers' same-name dedup). 2 new tests: file round trip with
  filtering + dedup, and the built-ins-first ordering.
- **`assets_panel.rs`** — an "Extra library folders" collapsing
  section: a path field + ＋ (validates `is_dir`, dedups), one row per
  extra folder with a ✕. Changes persist immediately and bump
  `scan_epoch`, which keys the material/mask list caches so both
  rescan. i18n key `assets.extra-dirs` (en-US + pt-BR).

## Verification

- `cargo test -p umber-app --bin umber` (filtered new tests) — 3/3.
- Gate-1 battery GREEN: layers + fmt + clippy -D warnings + gpu leg +
  umber-app 898 tests 0 failed. CI on the pushed tip = the wave-end
  gate (prior wave's CI 41219701214 SUCCESS on f188ac3).

## Not in scope

- Native folder-picker dialog (the text field is the headless-safe
  lane; a rfd dialog is the named nicety).
- Per-category extra dirs (one shared list covers both smart
  libraries — they read the same dirs).
- The browser kittest (still blocked on the dual-crate split; design
  named in the saved-search report).
