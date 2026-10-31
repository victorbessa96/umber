# Design kill: 2026-10-31 quick-mask persistence — DECIDED: persist, 8-bit sidecar; undo stays a named follow-up

- **Kill**: `quick-mask-persistence-design` (docs only — the argued
  decision + the implementation brief; the code is the next wave).
- **Lock**: this report + `docs/ROADMAP.md` + `STATE.md`.
- **The question** (ROADMAP row 148 / lens A gap 1): the quick mask is
  session-only ("temporary BY DEFINITION", quick_mask.rs:59-63;
  document.rs's header says the same), and the ftl string admits "not
  saved and not undoable". Substance Painter persists the quick mask.
  Design question, not a mechanical row.

## The argument

1. **The mask is user work product, not tool state.** It is painted
   strokes with soft edges — the same class of artifact as a layer's
   fill, which the project file already persists. The requirements'
   own principle (§8: save → load → export identical bytes) covers it.
   "Temporary by definition" was a wave-2 scope note, not a ratified
   spec; the census flagged the ftl admission precisely because SPP
   persists it. Parity is the bar the repo sets for itself.
2. **Persist the 8-bit form, not the f32.** The GPU mask texture is
   already `round(v * 255)` and the shader's gate differs from the f32
   CPU gate by at most 1/510 at soft edges (module docs). An f32 plane
   is 256 MB at 8K — absurd in a project dir. The 8-bit plane makes the
   persisted gate EXACTLY the GPU gate, and save → load → save is
   byte-identical on the quantized form (the format's own round-trip
   precedent, project.rs:949). Dequantize v/255 on load.
3. **Sidecar layout, mirroring `layers/`.** Project dir gains
   `quick_mask/<set>.bin` (per touched UDIM tile: u16 tile id + u32 w +
   u32 h + w*h bytes, row-major, appended in tile order) and a
   `quick_mask` key in `project.json`'s settings: plane dims, the
   `active`/`inverted`/`editing` flags, and the tile list with each
   tile's byte range. `#[serde(default)]` on the file schema means
   pre-decision projects load with no mask, unchanged — the same rule
   as `display`/`postfx`/`symmetry`.
4. **Undo: NO, and it is a named follow-up, not part of this kill.**
   The undo journal is `LayerCommand`-typed and lives in the app's
   `Document`; the mask lives in `PaintState` (umber-paint). Mask
   strokes as undo entries need a new journal op type plus PaintState
   wiring — a distinct wave, not a rider. SPP's mask-stroke undo is a
   nicety; the parity gap that cost real work is persistence across
   save/load. The existing rule stands meanwhile: mask strokes, invert
   and clear are never undo entries; undoing a gated paint stroke
   leaves the mask alone (quick_mask.rs:59-63).
5. **Resize rule unchanged**: resizing the paint targets resets the
   mask (it resets their contents); the sidecar is dropped on save
   after a reset. Empty mask → no sidecar written (a fresh mask and a
   cleared one are the same thing, all 0.0).

## Implementation brief (the next wave claims this)

- **core** (`crates/umber-core/src/project.rs`): a `ProjectQuickMask`
  serde mirror (the `ProjectSymmetry` pattern, :206-245): dims,
  flags, tile list. New `ProjectModel` field — the exhaustive
  destructure at `save_to_dir` (:589) compile-forces the save path to
  handle it, by design. `save_to_dir` writes `quick_mask/<set>.bin`
  via the existing `write_atomic` and prunes stale mask files (the
  `prune_stale_layer_files` pattern); `load_from_dir` (:651) reads it
  back. `#[serde(default)]` on `ProjectFile.settings.quick_mask`.
- **brush** (`crates/umber-brush/src/quick_mask.rs`): `QuickMask` gains
  `to_quantized() -> BTreeMap<u16, Vec<u8>>` (round(v*255)) and
  `from_quantized(dims, flags, planes)` (v/255), plus the module-doc
  rewrite of lines 59-63 ("persisted as the 8-bit sidecar; still not
  undoable"). Unit tests: quantize/dequantize round-trip, gate
  equality GPU-vs-persisted within 1/510.
- **paint** (`crates/umber-paint/src/paint_state.rs`): per-set
  snapshot/restore of the QuickMask (`quick_mask_for_tile` :2157 is
  the read seam); the app passes the model field in/out.
- **app** (`crates/umber-app/src/main.rs`): save at :3157 builds the
  model including the mask field; load at :3316 restores it into
  PaintState. Update the ftl string ("not saved" → saved) in both
  catalogs.
- **tests**: extend the save/load round-trip precedent
  (:949 `save_load_save_round_trip_is_byte_identical`) with a masked
  project; new: save → load → the gate decisions identical; old-shape
  project (no `quick_mask` key) loads with an empty inactive mask;
  empty mask writes no sidecar.
- **docs**: this report + the ROADMAP row + the module docs; the
  `document.rs` header line ("stays out of … the project file") is
  rewritten by the implementing wave.

## Not in scope

- Undoable mask strokes (the named follow-up above).
- Per-layer quick masks (SPP has per-layer masks; umber's is per
  texture set — a separate design question, not this one).
- Mask export/import as a standalone asset (the Assets panel's Masks
  category is the future home).
