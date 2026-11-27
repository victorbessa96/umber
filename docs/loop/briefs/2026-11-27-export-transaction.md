# Kill brief: expanded-path preflight + whole-run export rollback

- Base: `d21fdcefb2f009341182e7052abf4e9aae482f2e`.
- Lock: `razul-export-transaction` is already registered. Do not add locks, commit, push, or edit outside this brief's file list.
- Trigger: late formats review finding 4. Duplicate expanded output paths can silently overwrite; packed MTL inputs are rejected only after textures are written; later document failures leave a partial export.

## Allowed files

- `crates/umber-export/src/driver.rs`
- `crates/umber-export/src/artifacts.rs` (new)
- `crates/umber-export/src/lib.rs`
- `crates/umber-export/src/obj.rs`
- `crates/umber-export/src/usd.rs`
- `crates/umber-app/src/export_dialog.rs`
- `crates/umber-cli/src/main.rs`
- `docs/loop/kill-reports/2026-11-27-export-transaction.md` (handoff)

Do NOT touch presets.rs, exr.rs, psd.rs, shared status docs, or any other file. Another lane owns EXR/PSD routing. Extend current code; never revert unrelated bytes.

## Required behavior

1. **Expanded-path preflight.** Add a public driver helper that computes every texture output path for the same single/tiled inputs as the writers, plus MaterialX when enabled. Detect duplicate expanded destinations before any output is written. Add `ExportError::DuplicateOutput(PathBuf)` or equivalent typed error. Compare paths in a way that catches lexical aliases (`a/../b` vs `b`) without requiring destinations to exist.

2. **Rollback guard.** Add `artifacts.rs` with an `ExportArtifactGuard`:
   - unique backup directory inside `out_dir`;
   - `backup_all` first rejects duplicate destinations, then backs up every destination before any writer runs;
   - `backup(path)` is idempotent and rejects symlinks/non-regular destinations;
   - existing destinations are copied to backup; absent destinations are recorded as absent;
   - on `commit`, backups are removed and output remains;
   - on Drop without commit, every original file is restored and every newly created output is removed, best effort;
   - no unbounded in-memory image backups: copy to the backup directory.

3. **Driver integration.** Single and tiled preset runs validate all sets, preflight all expanded paths, then guard every writer. MaterialX is included. On any `Err`, Drop restores the complete run. On success, commit the guard. Keep existing output bytes and public call signatures unchanged.

4. **OBJ/MTL + USDA helpers.** Add a public `obj_mtl_output_paths(out_dir, asset, tile)` helper and use it in `write_obj_with_mtl`. Add a public `usda_output_path(out_dir, asset)` wrapper and use it in `write_usda`. Do not change document naming semantics.

5. **App integration.** Before the group write loop in `export_dialog.rs`:
   - compute all grouped driver/document paths and call one outer guard across ALL groups;
   - call `to_mtl_document` for every OBJ/MTL tile as a compatibility preflight BEFORE any texture is written;
   - include OBJ, MTL, and USDA paths in the outer guard;
   - commit only after every group/document succeeds. Any error must restore the whole app export, not just one group.
   Inner driver guards may remain; the outer guard must preserve originals across the whole app run.

6. **CLI integration.** Apply the same MTL preflight and outer guard around its texture + document run. Do not leave CLI on a weaker failure path than the app.

## Required tests (orchestrator runs them, not you)

- Driver: duplicate expanded output names fail before the first output exists.
- Driver: lexical alias duplicates fail too.
- Guard: an existing destination is restored after a simulated failed write; a newly created destination is removed; commit preserves the new bytes.
- OBJ/USDA path helpers preserve current naming.
- Existing suites must remain green.

Add tests in the existing module test blocks. Do not add dependencies or test-only production environment switches.

## Constraints and handoff

- Do NOT run cargo/rustc/clippy/tests; the orchestrator runs the real battery. Standalone rustfmt on touched files is allowed.
- Verify every API against current source before using it. No invented symbols.
- Write `docs/loop/kill-reports/2026-11-27-export-transaction.md` with files changed, design invariants, tests added, exact seams you could not execute, and any deliberate non-goals. If a required piece is incomplete, name it instead of claiming completion.
- No release, tag, PR, or push. Return a concise summary after the report exists.
