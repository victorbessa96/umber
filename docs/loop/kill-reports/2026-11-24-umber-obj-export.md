# OBJ mesh export round-trip

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-obj-export.md` (`b8e12d0b20d9`).
- Claim: `8383a09b447e`, queue row 598.
- Implementation: `8c9be5793f4fe63b8db0b4e8f8900f4bffc1ffa1`.
- Final pre-commit full-delta SHA-256, including the new `obj.rs`: `9cabec5cf7a0ea71679d217c175ef26a34073f72459e929697d5dde7bc23d222`.
- Kill lock: `razul-obj-export`.

## Built

- Added `crates/umber-export/src/obj.rs` with `to_obj_document` and `write_obj`, using the borrowed `UsdMeshView` geometry shape.
- Deterministic `v`, `vt`, `vn`, and three-corner `f v/vt/vn` records. Parallel attributes use the same one-based index at each corner, matching `tobj`'s `single_index=true` import contract.
- Ragged meshes, empty triangles, non-multiple indices, and out-of-range indices are typed `ExportError::Write` failures. Non-finite metadata sanitizes to `0.0`.
- Object/file names are bounded ASCII identifiers, preventing path escape and malformed object declarations. Output is LF-only and byte-identical across repeated calls.
- A preflight upper-bound size estimate refuses over-cap documents before allocation, and the finished document is checked again before any directory/file is written.
- Exported `pub mod obj` from `lib.rs`. The hostile-output round-trip battery now enumerates `usda` and `obj`, re-importing OBJ through both `load` and `load_obj`.

## Review and dispositions

- R1 `deleg_5b3ce5ba`: **REQUEST_CHANGES**, two valid findings. Names were unbounded and the size estimate allocated the sanitized name; writer-level and ragged-attribute coverage was incomplete. `MAX_OBJ_NAME_BYTES=240` now bounds names, `obj_name_len` computes the estimate without allocating, and name validation runs before sanitization. Added short-normal/UV, over-cap writer no-dir, huge-name no-dir, write-failure, and nonfinite real re-import coverage.
- R2 `deleg_48f05a49`: **REQUEST_CHANGES**, one valid finding. The write-failure test stopped at `create_dir_all` instead of `std::fs::write`. The test now pre-creates `out_dir/Sword.obj` as a directory, so directory creation succeeds and the actual file write fails with `ExportError::Write`.
- R3 `deleg_6453e892`: **SHIP**, zero findings. It independently confirmed the R2 write-path test reaches `std::fs::write`, the R1 name-allocation preflight is allocation-free and bounded before sanitization, and the ragged/over-cap/write-failure/nonfinite coverage is complete.

## Verification

- RED: `deterministic_document_and_real_roundtrip`, `ragged_and_over_cap_are_typed_failures`, and `non_finite_metadata_sanitizes_without_non_finite_tokens` all failed against the unimplemented surface.
- First implementation caught a real syntax bug: corners were emitted as separate face lines. They are now grouped into proper three-corner OBJ faces; exact tests pass.
- Strict `cargo clippy -p umber-export --all-targets -- -D warnings` → exit 0 after the test helper's `QuadFixture` alias.
- Full `cargo test -p umber-export` with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **94 passed, 0 failed** in lib plus **7 passed, 0 failed** in `export_roundtrip`, with zero doc-test failures.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-export/src/lib.rs crates/umber-export/src/obj.rs crates/umber-export/tests/export_roundtrip.rs` → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-obj-export-*.sh` probe reran the full export suite after the R1 fixes: **94 + 7 passed, 0 failed**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the export suite, not a workspace-green claim.

## Judgment calls

- The slice is geometry-only. GLB, material/MTL, and texture references are explicitly not claimed.
- `UsdMeshView` is reused rather than introducing a second borrowed geometry type, keeping the export crate mesh-free and the attribute contract consistent with USDA.
- The writer validates and size-checks before creating the output directory, so validation and cap failures leave no partial document.

## Unverified

- No GLB writer or material/texture round-trip was attempted. No full workspace test or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: row-156 candidate scoping beyond OBJ, then docs reconciliation.
- GLB export, material/texture references, EyeDepth, COW undo, generated parity table, theme matrix, and packaging deltas remain separately scoped follow-ups.
