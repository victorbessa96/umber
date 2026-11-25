# OBJ material/MTL export

- Date: 2026-11-24.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-24-umber-obj-mtl.md` (`3c83c23ca533`).
- Claim: `d489da63d83c`, queue row 608.
- Implementation: `d874fa546caef0a6c3cfd3d3d0cf5f61b293dd07`.
- Reviewed implementation-delta SHA-256: `f45d05a468f1e0c8bc385426f148712b1a7106e5a202ad2c776a7a395fbe3892`.
- Kill lock: `razul-obj-mtl`.

## Built so far

- Added additive `ExportPreset::obj_mtl` with `#[serde(default)]`, off in every built-in preset and older JSON.
- Updated driver and app preset literals.
- Added app dialog `obj_mtl` state, getter/setter, bilingual `export.obj` checkbox, filtering/preset propagation, and per-set/per-tile OBJ/MTL calls.
- Added a dialog kittest for the third document toggle and a real 8x8 GPU acceptance test that writes and re-imports OBJ/MTL with a derived texture-set name.
- Added CLI `ExportDocFlags { usda, obj_mtl }`, `--obj` parsing, and shared per-tile OBJ/MTL calls through `export_mesh`.
- Added batch and agent `obj_mtl` fields with older-recipe compatibility and shared flag propagation.
- Parent-owned Rust surfaces were formatted. These edits are not yet a completion claim.

## Core completed

The implementation claw stalled after producing only imports, so the dragon took over `obj.rs` directly. It now provides `to_mtl_document`, `to_obj_document_with_material`, `write_obj_with_mtl`, and `obj_file_for_output` with:

- Passthrough `map_Kd`/`map_Bump`/`map_Pr`/`map_Pm`/`map_Ke`/`map_d` bindings.
- Typed packed-channel, absolute/URI/parent/backslash/newline/NUL/UDIM-token refusal.
- Canonical actual-file containment that also rejects symlink escapes.
- Concrete per-tile OBJ/MTL pairs and previous-MTL rollback on write failure.
- Geometry-only API compatibility and document size caps.

## Review and dispositions

- R1 `deleg_76f6bca1`: **REQUEST_CHANGES**, two valid findings.
  - **P1:** per-set exports used the global tile-suffix decision. Fixed by deriving `group_tile_names` from each `group_tiles` and using it for both texture references and OBJ/MTL filenames. `obj_tile_suffix_decision_is_per_group` pins 1001-only, 1002-only, mixed, and duplicate semantics; full app **1,280/0**.
  - **P2:** URI/drive-prefix rejection was host-dependent and incomplete. Fixed by rejecting general `scheme:` and Windows drive prefixes on every host, with `file:base.png`, `C:/base.png`, and `C:base.png` in the unsafe corpus; export **110/0**.
- R2 `deleg_02f5c976`: **SHIP, zero findings** on both repaired findings.
- Fresh self-deleting `hermes-verify-obj-mtl-*.sh` probe: **11 targeted tests passed / 0 failed** (9 OBJ core + per-group regression + real app export), strict export+app clippy clean, and static parity/layers/scorecard/diff gates green. The script was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Verification

- `cargo test -p umber-export` → **110 passed / 0 failed**: 103 unit + 7 integration + 0 doc-tests. Covers the real Blender driver, deterministic OBJ/MTL writes, packed/unsafe refusal, missing/directory failures, and tiled material re-import.
- `cargo test -p umber-cli` → **159 passed / 0 failed** across all suites after correcting the assets root to `/mnt/hdd/data/Projects/umber/assets`.
- `cargo test -p umber-app` → **1,280 passed / 0 failed** after the R1 suffix fix. The saved log confirms `obj_mtl_export_writes_and_reimports_material_pair`, `obj_mtl_toggle_reaches_the_run_preset`, `obj_tile_suffix_decision_is_per_group`, and the updated keyboard journey.
- Strict `cargo clippy -p umber-export -p umber-cli -p umber-app --all-targets -- -D warnings` → clean. Export clippy was rerun after the canonical-symlink guard and is clean.
- Actual `target/debug/umber-cli export ... --preset blender --size 8 --rays 1 --obj` wrote `cube_uv_ascii7400.obj` + `.mtl`; `umber-cli inspect` re-imported 12 triangles with `materials: 1`. Every MTL texture reference exists.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- An initial CLI assets-root failure was environmental and corrected by using the `assets/` directory itself; no code was changed for it. This is targeted battery evidence, not a workspace-green claim.

## Deferred

- Resolve the core writer, run the full battery, and obtain independent SHIP review.
- Requirements/parity, the row-156 scoping record, and live status stay unchanged until then.
- EyeDepth, the theme matrix, and the COW A/B measurement remain separate queued kills.
