# Kill report: export UV-padding dilation (queue row 151) — 2026-11-02

Texturecraft absorption suggestion row 151, landed end to end on the
2026-11-02 no-cargo envelope (rustfmt/docs/git only; no cargo, no tests
run — the pinned tests ride the next battery-capable wave push).

## What landed

- `3450913` — umber-export driver + umber-app wiring:
  - `pub type DilateHook<'a> = &'a dyn Fn(&mut [u8], u32, u32, u16)` and
    `pub const DEFAULT_PADDING_TEXELS: u32 = 32`
    (crates/umber-export/src/driver.rs). The crate stays mesh-free AND
    core-free per scripts/check-layers.py (umber-export is L1; same-layer
    edges banned) — the caller builds the `DilationMap` and closes over it.
  - New entry points `run_preset_dilated`, `run_preset_dilated_with_surface`,
    `run_preset_tiled_dilated`, `run_preset_tiled_dilated_with_surface`.
    The old `run_preset*` fns delegate with `None` — byte-identical output
    (pinned by `no_hook_and_none_hook_write_identical_bytes`).
  - `write_outputs` takes `tile: u16` + `dilation: Option<DilateHook>` and
    applies the hook to every packed output map after packing, before
    encoding: `hook(&mut packed, size, size, tile)`.
  - Pinned driver tests: hook fires once per output with (len, w, h, tile);
    tiled run passes each tile's own number; hook mutation lands in the
    written PNG (linear data map read back exactly); no-hook/None byte
    identity. ALL UNRUN per envelope.
  - umber-app export_dialog.rs: `dilation_map_for_tile(mesh, tile, size)`
    builds one `umber_core::DilationMap` per exported tile from
    `umber_mesh::rasterize_uv_geometry` (covered[] = texels with a
    TexelGeom), padding capped at DEFAULT_PADDING_TEXELS; `run()` caches
    them in a `RefCell<HashMap<u16, DilationMap>>` and calls
    `run_preset_tiled_dilated(.., Some(pad_hook))`. App test
    `dilation_map_for_tile_covers_the_uv_footprint` (hand-built triangle
    mesh, 4x4: covered texel is its own source, uncovered texel padded).
    UNRUN per envelope.
  - presets.rs: pack_texel's missing-MAP transparent-black rule unchanged
    by design — dilation pads uncovered TEXELS of maps that exist; it
    cannot invent a map the set never carried (doc updated).
  - uv_dilation.rs module doc updated: the consumer is now the driver's
    `DilateHook` (wired 2026-11-02).
- `3a07fb7` — umber-cli `export_mesh`: same per-tile DilationMap cache from
  the whole mesh's UV layout (rasterize_uv_geometry rasterizes into each
  tile's own window), passed through `run_preset_tiled_dilated`.

## Attribution + collision note

The driver half (DilateHook type, dilated entry points, write_outputs
plumbing, 2 hook tests, presets.rs docs) was authored by sibling
local-4fe45fb / subagent 20261101_155725_f9d173, whose working-tree bytes
were uncommitted when this session adopted the lane. Two sessions had
claimed export-dilation lock surfaces between 18:33 and 19:4x
(export-padding-dilation.lock, then export-dilation-pad.lock); the
dragon re-derived the full diff, REVERTED its own colliding umber-core
dependency on umber-export (check-layers.py: L1->L1 edge goes red — the
reason the hook design exists), reviewed the sibling's hook API, and
completed the caller side (app + CLI), the tests, the queue row, this
report. Sibling session 20261102_181308_6f2321 committed the coordination
note + design briefs as `acca0d2` (its lock covered only
.dragon/ + docs/loop/briefs/). The tree was clean of the sibling's
uncommitted bytes at commit time except driver.rs/presets.rs, which this
commit lands verbatim-reviewed.

## Verification (envelope: no cargo, no tests)

- rustfmt --edition 2021 --check CLEAN x5 (driver.rs, presets.rs,
  uv_dilation.rs, export_dialog.rs, main.rs).
- No new dependencies anywhere: Cargo.toml/Cargo.lock untouched (the
  umber-core dep attempt was reverted); check-layers.py constraint
  respected (umber-export stays core-free).
- Static cross-checks: `write_outputs` has exactly 2 call sites, both
  updated; `umber_core::DilationMap` root re-export exists
  (uv_dilation.rs:53 area); `rasterize_uv_geometry` is exported at the
  umber-mesh root; `read_png_rgba8` is pub; ExportContext.mesh is
  Option<&MeshData>; umber-app and umber-cli already depend on
  umber-core + umber-mesh + umber-export.
- 5 pinned tests written, UNRUN until the next battery-capable session
  (Bessa's no-cargo envelope 2026-11-01 ~11:1x; reaffirmed 2026-11-02
  "only coding and similar activities, no cargo or test").

## Named follow-ups

- Battery (check + clippy + the 5 new tests + full suites) at the next
  cargo-capable wave push; CI on the pushed tip is the verdict gate.
- Desktop check n/a (no UI change).
- Rows 152-157 (mcp registration, --control+bridge, CommandSpec
  unification design landed as acca0d2's brief, rmcp/cap-std, engine +
  packaging, standing re-sweep) remain the texturecraft queue.
