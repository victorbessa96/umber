# Kill report: 2026-10-31 quick-mask persistence IMPLEMENTED

- **Kill**: `quick-mask-persistence-impl` — the design kill's brief, landed.
- **Design**: `docs/loop/kill-reports/2026-10-31-quick-mask-persistence-design.md`
  (amended: the sidecar landed under `paint/quick_mask/`, not top-level).
- **Locks**: the touched files, registered around the dcommit.

## What landed

- **`umber-brush/src/quick_mask.rs`** — `QuickMaskCarry` + `QuickMaskTileCarry`
  (serde), `MaskPaint` now serde, and `QuickMask::{is_persist_worthy, carry,
  from_carry}`. The persisted planes are the 8-bit quantization
  `round(v*255)` — byte-identical to `gpu_bytes`, so the persisted gate IS
  the GPU gate and save→load→save is byte-identical (test-pinned). Module
  doc rewritten: persisted in the sidecar, still not undoable. 3 new unit
  tests (worthiness, round trip + GPU-byte equality + byte-stable re-save,
  torn/mismatched blob refusal). 13/13 quick_mask tests green.
- **`umber-paint/src/persistence.rs`** — the `paint/quick_mask/` sidecar:
  `<set-hash>.json` (envelope `{set, mask: QuickMaskCarry}`) + `<set-hash>.bin`
  (concatenated quantized planes), hash-named stems (blake3 of the set name —
  never a path component), own manifest `manifest.txt`
  (`# Umber quick-mask commit v1` + `<file> <blake3>`), same
  stage → manifest.pending (commit point) → apply → sweep → prune protocol
  as the paint tiles, AssetStore blobs, `recover_quick_mask_sidecar` on both
  paths. The paint commit's 5-field codec is untouched. Pub API:
  `save_quick_masks` / `load_quick_masks` + codec fns. 3 new tests: codec
  round trip + garbage rejection; PaintState round trip (gate survives within
  1/510, manifest byte-identical on re-save, empty project → 0, no-worthy-mask
  → only the empty manifest); tampered .bin is hash-skipped, never trusted.
- **`umber-paint/src/paint_state.rs`** — `quick_masks()` census +
  `restore_quick_mask(set, mask)` (registers the set, clears GPU-sync marks).
- **`umber-app/src/main.rs`** — save: `save_quick_masks` right after
  `save_paint_tiles` (second commit point; a crash between keeps old masks);
  open: `load_quick_masks` right after `load_paint_tiles_reported`
  (pre-kill projects have no dir → restore 0).
- **i18n** — the quick-mask hint (en-US + pt-BR) now says the mask is saved
  with the project but not undoable. `document.rs`'s header updated.

## Not in scope (unchanged follow-ups)

- Undoable mask strokes (needs a new journal op type; the journal is
  `LayerCommand`-typed, the mask lives in `PaintState`).
- Per-layer quick masks; mask export/import as a standalone asset.

## Verification

- `cargo test -p umber-brush --lib quick_mask`: 13 passed.
- `cargo test -p umber-paint --lib persistence::tests::quick_mask`: 3 passed
  (real wgpu device via lavapipe).
- `cargo check -p umber-app --all-targets` rc=0; `cargo fmt --check` clean;
  clippy on brush/paint/app clean.
- Gate-1 battery (full umber-app suite + fmt + clippy -D warnings + gpu leg)
  run at the wave tip; verdict recorded in COORDINATION.
