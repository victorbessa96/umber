# Kill brief: cross-library stamp dedup (wave 3.4)

- Lock: `razul-abr-stamp-dedup` over:
  - crates/umber-cli/src/import_asset.rs
  - crates/umber-brush/src/abr_import.rs (only if the in-file dedup rule needs to share code)
- Context: wave 3.4 of docs/loop/waves/2026-11-09-wave-plan.md — "cross-library stamp dedup (blake3)... saves disk, not correctness... touches the asset store, a wider surface — take it LAST." blake3 is already a workspace dep (Cargo.toml:62). In-file dedup exists: a byte-equal stamp drops the LATER entry and counts `duplicate_stamps_skipped` (abr_import.rs:459-460, judgment call documented at abr_import.rs:57-60).
- Read first: crates/umber-cli/src/import_asset.rs (plan/targets/write loop, AbrImportOutcome), crates/umber-export/src/png.rs (`read_png_rgba8` at :171, `write_png`/`encode_png`), the preset sidecar convention in crates/umber-brush/src/preset.rs (:926-1030).

## Target

Extend the existing within-file dedup to the target library directory, same drop-the-later rule:

1. In the plan phase, after building `plan`, scan the target `dir` for existing `*.stamp.png`, decode each with `umber_export::png::read_png_rgba8`, and blake3-hash the RGBA bytes into a `HashMap<[u8;32], ()>` (skip undecodable files silently — they are not ours to judge).
2. For each planned item with a stamp: hash its raw RGBA; if the hash is already present (in-file earlier entry OR an existing library file), drop the item (do not write preset or stamp), count it in a new `cross_library_duplicates_skipped: u32` on `AbrImportOutcome` (dry-run included). In-file duplicates keep counting in `duplicate_stamps_skipped` as today.
3. Targets/exists check uses the post-dedup plan. The outcome summary prints both counts.
4. The preset still keys its stamp on `stamped: true` + the `<stem>.stamp.png` convention; a dropped duplicate emits nothing, so no schema change. (Sharing one stamp file between two presets would need a new preset field — explicitly OUT of scope.)

## Rules

- Edit only the locked files. No cargo. No commit/push. rustfmt-clean (edition 2021). blake3 via the workspace dep.
- Byte-compat and safety: a file that imports today imports identically; dedup only removes REDUNDANT writes. Never delete or overwrite an existing file.
- Tests: import into a dir that already holds a stamp with identical RGBA (write it via write_png in the test) -> second import skips it, count 1, no new files; different bytes -> both written; dry-run counts without writing.
- Attribution: repo-facing comments at most "exploration results assembled from online sources".

## Dragon verification

rustfmt --check, check-layers, scorecard, clippy -p umber-brush -p umber-cli --all-targets -D warnings, test -p umber-brush -p umber-cli, check -p umber-app --all-targets.
