# Kill report: expanded-path preflight + whole-run export rollback

Base: `b13a27a84bf1` (brief commit; `d21fdcef` is the code base)
Lock: `razul-export-transaction`

## Files changed

- `crates/umber-export/src/artifacts.rs` (new)
- `crates/umber-export/src/driver.rs`
- `crates/umber-export/src/lib.rs`
- `crates/umber-export/src/obj.rs`
- `crates/umber-export/src/usd.rs`
- `crates/umber-app/src/export_dialog.rs`
- `crates/umber-cli/src/main.rs`
- `docs/loop/kill-reports/2026-11-27-export-transaction.md` (this report)

No file outside the brief's list was touched. No dependencies, no test-only
production switches, no presets/EXR/PSD changes.

## What landed

### 1. Expanded-path preflight

`driver.rs` now exposes:

- `expanded_output_paths(preset, sources, out_dir) -> Result<Vec<PathBuf>, ExportError>`
- `expanded_output_paths_tiled(preset, tiles, sources, out_dir) -> Result<Vec<PathBuf>, ExportError>`

Both use the same private `output_path` expansion as `write_outputs`, so the
preflight cannot drift from the writers. The tiled form applies
`tiled_template` and each tile's `$udim` exactly as the writer does. Both
include `<textureSet>.mtlx` when `ExportPreset::materialx` is on (and the
tiled form only when `tiles` is non-empty, matching the writer).

`ExportError::DuplicateOutput(PathBuf)` is the typed duplicate error.

### 2. Rollback guard

`artifacts.rs` adds `ExportArtifactGuard`:

- `new(out_dir)` creates a unique `.umber-export-backup-<pid>-<seq>`
  directory inside `out_dir`.
- `backup_all(paths)` rejects duplicate destinations first, then calls
  `backup` for every path. Duplicate comparison is lexical: `.` is dropped,
  `..` pops a preceding normal component, and surplus `..` above a root is
  ignored. It deliberately does not use `canonicalize`, so destinations do
  not need to exist and symlinks are not followed for comparison.
- `backup(path)` is idempotent. An existing regular file is copied into the
  backup directory; an absent path is recorded as absent. Symlinks and
  non-regular destinations are refused with `ExportError::Write`.
- `commit()` marks the run successful, removes the backup directory, and
  leaves the new outputs in place.
- `Drop` without commit restores every original file from its backup and
  removes every newly created destination, in reverse write order, best
  effort.

Backups are file copies on disk, never in-memory image buffers.

### 3. Driver integration

Both `run_preset_dilated_with_surface` and
`run_preset_tiled_dilated_with_surface` now:

1. validate every set,
2. compute every expanded destination (textures + `.mtlx`),
3. create one guard and `backup_all` before the first writer,
4. write the run,
5. `commit` only on full success.

Any `Err` between backup and commit drops the guard and restores the whole
run. Existing public entry points and output bytes are unchanged.

### 4. OBJ/MTL + USDA helpers

- `obj::obj_mtl_output_paths(out_dir, asset, tile) -> Result<(PathBuf, PathBuf), ExportError>`
  uses the same `object_stem` sanitizer as `write_obj_with_mtl`, which now
  calls it.
- `usd::usda_output_path(out_dir, asset) -> Result<PathBuf, ExportError>`
  wraps `document_output_path`, and `write_usda` now calls it. Document
  naming/sanitization is unchanged.

### 5. App integration

`export_dialog.rs` now builds one `GroupPlan` per texture-set group, then,
before the first texture write:

- collects every group's `expanded_output_paths_tiled` paths (textures +
  `.mtlx`),
- calls `to_mtl_document` for every OBJ/MTL tile as a compatibility
  preflight (packed channels / unsafe references fail before textures are
  written),
- adds every OBJ, MTL, and USDA destination,
- creates one outer `ExportArtifactGuard` across ALL groups and calls
  `backup_all`.

The write loop then runs the groups/documents as before. `commit` happens
only after every group/document succeeds, so any error restores the whole
app export rather than one group. Inner driver guards remain; they commit
per group, while the outer guard preserves the originals for the whole run.

### 6. CLI integration

`export_mesh` now performs the same whole-run preflight: expanded texture
paths, the USDA destination, every OBJ/MTL destination, and the
`to_mtl_document` compatibility walk for every tile, all before the texture
run. One outer guard spans the texture + document run and commits only on
success.

## Tests added

Driver (`driver.rs`):

- `duplicate_expanded_output_names_fail_before_the_first_output_exists`
- `lexically_aliased_output_names_are_duplicates_too`
- `expanded_output_paths_match_the_written_names`
- `tiled_expanded_output_paths_match_the_written_names`

Guard (`artifacts.rs`):

- `dropped_guard_restores_an_existing_destination`
- `dropped_guard_removes_a_newly_created_destination`
- `commit_keeps_the_new_bytes_and_removes_the_backup`
- `backup_all_rejects_duplicate_and_lexically_aliased_destinations`
- `backup_is_idempotent_for_the_same_destination`
- `backup_refuses_a_symlink_destination` (`cfg(unix)`)

Path helpers:

- `obj.rs`: `output_path_helper_matches_the_writer_naming`
- `usd.rs`: `output_path_helper_matches_the_writer_naming`

## Design invariants / judgment calls

- Duplicate detection is lexical, not canonical: the guard must work before
  destinations exist, and canonicalization would follow symlinks. On a
  case-insensitive filesystem two differently-cased paths are still treated
  as distinct.
- The backup directory lives inside `out_dir` so it shares the filesystem
  and is removed with the run.
- A guard refuses a symlink/non-regular destination rather than following or
  copying it.
- Restoration is best effort because `Drop` cannot return an error. A
  read-only filesystem can still leave a partial export; the original typed
  error remains the primary failure.
- Empty directories created by a failed run are not removed; the guard owns
  files, not directory lifetimes.
- USDA has no document-compatibility preflight (the brief only required the
  MTL one); a late USDA failure is still rolled back by the outer guard.
- Inner driver guards are retained as allowed by the brief; this costs one
  extra copy of each texture in app/CLI runs but keeps the driver
  independently transactional.

## Orchestrator verification

- `cargo test -p umber-export -- --test-threads=1`: **122 passed, 0 failed**,
  plus **7/7** `export_roundtrip` integration tests.
- `cargo test -p umber-app -p umber-cli -- --test-threads=1`: **931 app bin
  tests**, **123 CLI bin tests**, and every integration target, **0 failed**.
- `cargo clippy -p umber-export -p umber-app -p umber-cli --all-targets --
  -D warnings`: clean.
- Rustfmt, `git diff --check`, scorecard (**3,352**), layers, and parity: clean.
- `scripts/verify-kill.sh` passed on the transaction files. Its two `source`
  WARNs were pre-existing identifiers, not introduced error fields.
- Independent final review: `opencode/beam-free` **SHIP**, zero findings.

## Deliberate non-goals

- No changes to `presets.rs`, `exr.rs`, `psd.rs`, or shared status docs.
- No release, tag, PR, push, or commit.
- No case-insensitive path comparison, no fsync/atomic-rename durability
  protocol, and no directory-tree rollback.
- No USDA document preflight beyond destination guarding.

## Gates run

- `bash scripts/verify-kill.sh` on all seven touched Rust files: PASS
  (balanced, fmt-clean, trap-scanned).
- `python3 scripts/check-layers.py`: `layers: GREEN (16 crates)`.
- `rustfmt --edition 2021` on all touched `.rs` files: clean.
- Cargo battery intentionally not run per the brief; the orchestrator runs
  `fmt`/`clippy`/tests.
