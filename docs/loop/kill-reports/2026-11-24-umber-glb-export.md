# GLB mesh export round-trip

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-glb-export.md` (`d3817e4c6f25`).
- Claim: `54d59e22cad5`, queue row 602.
- Implementation: `f949199c07fd7a092dd4c6a235609319ac46d4fd`.
- Final pre-commit full-delta SHA-256, including the new `glb.rs`: `bfce4d8b212a14392bd1355e61dc7e356b7ce86db077e99ad82807d6991f9e35`.
- Kill lock: `razul-glb-export`.

## Built

- Added `crates/umber-export/src/glb.rs` with `to_glb_document` and `write_glb`, using the borrowed `UsdMeshView` geometry shape.
- Deterministic GLB 2.0 container: 12-byte header, JSON chunk padded to 4-byte alignment, BIN chunk with little-endian f32 positions/normals/UVs and u32 triangle indices.
- Accessors use `componentType` 5126 for POSITION/NORMAL/TEXCOORD and 5125 for indices; bufferViews cover each attribute with correct byte offsets and lengths. POSITION min/max are finite and computed after sanitization.
- Ragged meshes, empty triangles, non-multiple indices, and out-of-range indices are typed `ExportError::Write` failures. Non-finite metadata sanitizes to `0.0`.
- Reused OBJ's bounded 240-byte name rule without allocating the name during the size preflight. Output is byte-identical across repeated calls.
- A preflight upper-bound size estimate refuses over-cap documents before allocation, and the finished document is checked again before any directory/file is written.
- Exported `pub mod glb` from `lib.rs`. The hostile-output round-trip battery now enumerates `usda`, `obj`, and `glb`, re-importing GLB through both `load` and `load_gltf`.

## Review and dispositions

- R1 `deleg_3c93bef6`: **REQUEST_CHANGES**, two valid findings. Exact GLB layout was not fully asserted, and writer-level failure atomicity was not pinned for the geometry-validation matrix. The test now asserts offsets `0,48,96,128`, lengths `48,48,32,24`, all accessor component types, POSITION min/max, and accessor counts. Over-cap and huge-name failures now assert no partial file with pre-existing directories, and empty/nonmultiple/out-of-range/short-normal/UV writer cases do likewise.
- R2 `deleg_7db0834b`: **SHIP**, zero findings. It independently confirmed all accessor component types, POSITION min/max, all bufferView offsets/lengths and accessor counts are pinned; invalid meshes, over-cap input, and huge names use pre-existing output directories and assert no output file; and the existing-directory write-failure case reaches `std::fs::write` itself.

## Verification

- RED: `deterministic_binary_layout_and_real_roundtrip`, `ragged_and_over_cap_are_typed_failures`, and `non_finite_metadata_sanitizes_without_non_finite_tokens` failed against the unimplemented surface.
- The over-cap fixture initially used 8,000,000 indices and correctly failed as non-multiple-of-three. It was corrected to 8,000,001, which is both triangle-aligned and over the 256 MiB cap.
- Strict `cargo clippy -p umber-export --all-targets -- -D warnings` → exit 0.
- Full `cargo test -p umber-export` with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **98 passed, 0 failed** in lib plus **7 passed, 0 failed** in `export_roundtrip`, with zero doc-test failures.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-export/src/lib.rs crates/umber-export/src/obj.rs crates/umber-export/src/glb.rs crates/umber-export/tests/export_roundtrip.rs` → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-glb-export-*.sh` probe reran the full export suite after the R1 fixes: **98 + 7 passed, 0 failed**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the export suite, not a workspace-green claim.

## Judgment calls

- The slice is geometry-only. Material/MTL and texture references are explicitly not claimed.
- `UsdMeshView` is reused rather than introducing a second borrowed geometry type, keeping the export crate mesh-free and the attribute contract consistent with OBJ/USDA.
- The writer validates and size-checks before creating the output directory, so validation and cap failures leave no partial document.

## Unverified

- No material/texture round-trip was attempted. No full workspace test or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: material/MTL or texture references for OBJ/GLB, or the generated parity table.
- EyeDepth, COW undo, theme matrix, and packaging deltas remain separately scoped follow-ups.
