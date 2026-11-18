# Design brief — D4 IdMulti full persistence (cross-review MED, next-steps B3)

Date: 2026-11-18. Kill: `razul-d4-idmulti-serial`. Supersedes the TODO'd
lossy encoding in `bakes_panel.rs` (2026-11-16 D4 kill `35097df5`).

## Problem (verified against the live tree, not memory)

- `settings_snapshot()` encodes IdMulti sources as one `ParamValue::Stops`
  under `id_multi_sources`, packing `[source_idx, weight, blend_mode]` into
  the RGBA color and dropping `mask_path` and the manual `color`.
- Worse, `param_to_carry()` maps only `Float`/`Int`/`Bool` and returns `None`
  for everything else, and `block_to_carry()` `filter_map`s that away — so the
  `Stops` value **never reaches `project.json` at all**. Only the blend-mode
  `Int` survives today. Reopening a project silently resets sources to the
  default single Triangle source.
- `crates/umber-gpu/src/bake_shaders.rs` `id_multi_bake_shader()` carries a
  `cs_main_from_position` entry that writes zeros and is never dispatched
  (`id.rs:256` and `id.rs:701` both dispatch `cs_main` only). The cross-review
  MED: a stub that returns black if ever dispatched.

## Target contract

Flat, scalar-only keys inside the existing per-baker param map, so every
value survives the `BakeParamCarry` mirror. Additive: no project-format
version bump, no new `BakeSyncCarry` fields, old files load unchanged.

### Carry level (`umber-core/src/project.rs`)

- Add one variant to `BakeParamCarry`: `Text(String)` (drops the `Copy`
  derive; the enum is referenced only by `project.rs` and `bakes_panel.rs`).
  Additive serde variant; old files contain only `Float`/`Int`/`Bool` and
  deserialize unchanged. Files are read forward-only (same class as the
  R-D1 additive carry).

### Key layout (per-baker params map, `bake_param` consts)

- `id_multi_count`: `Int(n)` — number of sources (0..=8).
- Per source `i`: `id_multi_src_{i}_source` `Int` (0=Triangle, 1=Material,
  2=IdMap, 3=Manual — same discriminants as today), `id_multi_src_{i}_weight`
  `Float`, `id_multi_src_{i}_mask` `Text(path)` (key absent when `None`),
  `id_multi_src_{i}_color_r/_g/_b` `Int` 0..=255.
- `id_multi_blend_mode`: `Int` unchanged (0=Additive, 1=Max, 2=Replace).
- The legacy `id_multi_sources` `Stops` key is never written again, but is
  still READ for pre-fix files (decoded as today: source+weight only).

### App boundary (`bakes_panel.rs`)

- `settings_snapshot()` emits the flat keys using `ParamValue::Int` /
  `Float` / `Asset(path)` for the mask (Asset maps to `Text` at the carry
  boundary). Color as three `Int` keys.
- `apply_settings()`: if `id_multi_count` is present, decode the flat keys
  with mercy (missing per-source fields fall back to
  Triangle / 1.0 / no mask / white); else fall back to the legacy `Stops`
  decode; empty result resets to the default single Triangle source, as
  today.
- `param_to_carry()`: add `Asset(s) => Some(Text(s.clone()))`; drop nothing
  else new. `param_from_carry()`: `Text(s) => Asset(s)`. `block_from_carry()`
  switches `param_from_carry(*v)` to `param_from_carry(v.clone())` (enum no
  longer `Copy`).

### Shader stub (`umber-gpu/src/bake_shaders.rs`)

- Delete the never-dispatched `cs_main_from_position` entry from
  `id_multi_bake_shader()` and fix its doc comment to say the mesh-fed path
  does not exist for ID multi (the UV G-buffer is the only route). No Rust
  dispatch references it (`id.rs` uses `cs_main` only).

## Verification (kittest-first)

1. `id_multi_sources_round_trip_through_snapshot_and_apply`: two sources
   (Manual `[255,0,0]` weight 0.4 with `mask_path` Some, plus IdMap weight
   1.0), blend Max → `settings_snapshot` → fresh panel `apply_settings` →
   sources equal field-for-field and `settings_snapshot` blocks equal.
2. `id_multi_sources_round_trip_through_project_file`: same panel →
   `sync_carry()` → `ProjectModel::with_bake_sync` → `save_to_dir` /
   `load_from_dir` → `load_sync_carry` → `apply_settings` → sources and
   snapshot equal (this is the save/reopen regression the next-steps list
   demands).
3. `id_multi_legacy_stops_encoding_still_loads`: hand-built block carrying
   the old `Stops` encoding → `apply_settings` → sources decoded with
   `mask_path: None`, default color.
4. Battery: `rustfmt --check` on touched files, `scripts/check-layers.py`,
   `scripts/scorecard.py --check`, `cargo clippy -p umber-core -p umber-app
   -p umber-gpu --all-targets -- -D warnings`, `cargo test -p umber-core -p
   umber-app` under `flock /tmp/umber-cargo.lock` with `UMBER_ASSETS_DIR`
   pointed at the repo root (main-tree CWD pitfall), `CARGO_BUILD_JOBS=3`,
   `nice 19`.

## Non-goals

- No `ParamValue` enum change (wire-shared with plugins; the flat-key
  encoding works inside the existing scalar arms + `Asset`).
- No change to bake-time behavior: `mask_path` resolution to a
  `TextureView` stays where it is.
- D2/D3/D5 cross-review items are separate kills, not this one.
