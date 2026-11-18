# Kill report — 2026-11-18 D4 IdMulti full persistence

Kill: `razul-d4-idmulti-serial`. Brief:
`docs/loop/briefs/2026-11-18-d4-idmulti-serialization-design.md` (commit
`76f1c549`). Closes the D4 MED findings of
`docs/loop/reviews/2026-11-17-d2-d5-cross-review.md` (lossy IdMulti
serialization; zero-writing mesh-fed stub) and next-steps B3.

## What was wrong (verified live, worse than the review noted)

- `settings_snapshot()` packed sources into one `ParamValue::Stops` under
  `id_multi_sources` ([source, weight, blend] in RGBA), dropping `mask_path`
  and the manual `color`.
- `param_to_carry()` mapped only `Float`/`Int`/`Bool` and `block_to_carry()`
  `filter_map`ed the rest away — so the `Stops` value never reached
  `project.json` at all. Only the blend-mode `Int` persisted; reopening a
  project silently reset the source list to the default Triangle source.
- `id_multi_bake_shader()` carried a `cs_main_from_position` entry that wrote
  zeros and was never dispatched (`id.rs` dispatches `cs_main` only).

## What landed

- `BakeParamCarry` gained an additive `Text(String)` variant (Copy dropped;
  the enum is referenced only by `project.rs` and `bakes_panel.rs`). Old
  files carry only `Float`/`Int`/`Bool` and load unchanged.
- IdMulti sources now serialize as flat scalar keys in the per-baker param
  map: `id_multi_count` (Int) plus per-source `id_multi_src_{i}_source` (Int),
  `_weight` (Float), `_mask` (Text, key absent when `None`), `_color_r/_g/_b`
  (Int 0..=255). `id_multi_blend_mode` unchanged. App-side the mask is
  `ParamValue::Asset`, mirrored to `Text` at the carry boundary.
- `apply_settings()` reads the flat keys with mercy (missing fields default
  to Triangle / 1.0 / no mask / white); the legacy `Stops` key is still read
  for pre-fix files (source + weight only, as before).
- The never-dispatched `cs_main_from_position` stub was deleted from
  `id_multi_bake_shader()`; the doc comment now names `cs_main` as the single
  entry point.

## Tests (kittest-first, all in `bakes_panel.rs`)

- `id_multi_sources_round_trip_through_snapshot_and_apply` — two sources
  (Manual [255,0,0] weight 0.4 with mask path + IdMap), blend Max:
  snapshot carries only flat scalar keys (mask as Asset), fresh panel
  `apply_settings` reproduces the sources field-for-field and the snapshot
  is equal.
- `id_multi_sources_round_trip_through_the_project_file` — the save/reopen
  regression: `sync_carry()` → mask is `Text` in the carry →
  `save_to_dir`/`load_from_dir` → carry equal → `load_sync_carry` →
  `apply_settings` reproduces sources + blend and the snapshot is equal.
- `id_multi_legacy_stops_encoding_still_loads` — a hand-built legacy `Stops`
  block still decodes (source + weight, default mask/color).

## Battery

(see queue row for the verdict line; run under `flock /tmp/umber-cargo.lock`,
`CARGO_BUILD_JOBS=3`, `nice 19`, `UMBER_ASSETS_DIR=<repo root>` — the main-tree
CWD pitfall)

- `rustfmt --edition 2021 --check` on the three touched .rs files: clean.
- `scripts/check-layers.py`: GREEN (16 crates). `scripts/scorecard.py
  --check`: current.
- `cargo clippy -p umber-core -p umber-app -p umber-gpu --all-targets --
  -D warnings`: GREEN (0 errors/warnings, `Finished dev profile`).
- `cargo test -p umber-core -p umber-app`: GREEN — 13 suites, 1439 passed /
  0 failed total, including the three new `id_multi_*` tests
  (`id_multi_sources_round_trip_through_snapshot_and_apply`,
  `id_multi_sources_round_trip_through_the_project_file`,
  `id_multi_legacy_stops_encoding_still_loads`).

## Notes

- No project-format version bump (R-D1 additive pattern); no new
  `BakeSyncCarry` fields. Files written by this build carry a `Text` variant
  an older app cannot parse — forward-only, same class as the R-D1 carry.
- The WGSL edit is a deleted never-dispatched entry point; GPU-feature shader
  tests run on lavapipe in CI (no local adapter on minihost).
- D2/D3/D5 cross-review MEDs remain open as follow-up kills.
