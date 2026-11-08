# Kill brief — row 284: `umber-cli import-asset --kind brush` accepts `.abr` (fan-out)

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ first:
- `docs/loop/briefs/2026-11-07-wave-abr-design.md` (scope decisions: one .abr
  = a library, one `.umberbrush` + sidecar per brush, CLI fan-out reusing the
  validated-import lane);
- `crates/umber-brush/src/abr_import.rs` (row 283, LANDED `c9ee71e5`):
  `import_abr(bytes) -> AbrImport { entries: Vec<AbrImportEntry { preset,
  stamp: Option<(w, h, rgba)> , brush_index }>, orphan_samples,
  brushes_without_sample, duplicate_stamps_skipped }`;
- `crates/umber-brush/src/abr.rs` (row 282): `read_abr` for the parse-level
  validation of an .abr;
- `crates/umber-cli/src/import_asset.rs` — the K5 lane: `AssetKind` (+ its
  `listed_extension`, :72), `validate` (:177: extension gate then loader),
  `import_asset` (:247: resolve dir, TargetExists refusal, dry-run shape,
  `fs::copy`), `ImportOutcome` (:166), `ImportError`;
- `crates/umber-cli/src/main.rs` — `import_asset_cmd` (:85) + the usage line
  (:64);
- `crates/umber-cli/src/agent.rs` — the `import_asset` agent step (:2156,
  serializes the outcome to JSON fields);
- `crates/umber-export/src/png.rs` — `write_png(path, w, h, rgba,
  Transfer::Linear)` (:63) and `read_png_rgba8` (:171) for tests;
- `crates/umber-brush/src/preset.rs` — `BrushPreset::to_json` (:302) /
  `from_json` (:325) / `validate()`; the `<stem>.stamp.png` sidecar
  convention (:815).

umber-cli already deps on `umber-brush`, `umber-export`, `blake3`,
`serde_json` — use them; NO new dependencies.

## Your file surface (touch NOTHING else)

- `crates/umber-cli/src/import_asset.rs`
- `crates/umber-cli/src/main.rs`
- `crates/umber-cli/src/agent.rs`

Do NOT run cargo/clippy/tests. Do NOT commit or touch git.

## What to build

1. **import_asset.rs**
   - `AssetKind::Brush` validation accepts extension `abr` (case-insensitive)
     IN ADDITION to `umberbrush`: `.abr` validates by
     `umber_brush::abr::read_abr(&bytes)` succeeding (a parse-level check; the
     fan-out does the real work). Keep the `.umberbrush` JSON pipeline exactly
     as-is. (`listed_extension` is used by the extension gate — instead of
     changing it, branch inside `validate` on the actual extension for Brush.)
   - NEW `pub struct AbrImportOutcome` (serde Serialize): `{ kind, source,
     targets: Vec<PathBuf>, dry_run, presets, stamps_written,
     duplicate_stamps_skipped, orphan_samples, brushes_without_sample }`.
   - NEW `pub fn import_abr_asset(file, library, dry_run) -> Result<AbrImportOutcome, ImportError>`:
     read bytes → `import_abr` → for each entry compute a slug from
     `preset.name` (lowercase, non-alphanumeric runs → `-`, trim dashes, cap 48
     chars, empty → `abr-brush-{index}`; de-duplicate within the run by
     appending `-2`, `-3`, ...);
     - resolve the library dir exactly like `import_asset` (explicit
       `--library` or `default_library_dir(Brush)` = the user preset dir);
     - compute ALL target paths first (`<slug>.umberbrush`, plus
       `<slug>.stamp.png` for stamped entries); if ANY target exists →
       `ImportError::TargetExists` for the whole import, NOTHING written
       (same refusal semantics as the single-file lane);
     - dry-run returns the plan, writes nothing;
     - otherwise write `<slug>.umberbrush` = `preset.to_json()?` and, for
       stamped entries, `<slug>.stamp.png` via
       `umber_export::png::write_png(&path, w, h, &rgba, Transfer::Linear)`
       (stamp is a data plane, same as the stencil converter);
     - outcome counts: presets = entries written, stamps_written,
       duplicate_stamps_skipped / orphan_samples / brushes_without_sample
       copied from the `AbrImport`;
     - every written preset must load through `BrushPreset::from_json` +
       `validate()` (pin it in tests).
   - `import_asset` itself: when kind == Brush and the file is `.abr`,
     DELEGATE to `import_abr_asset` — but its return type is the single-file
     `ImportOutcome`. Keep both: `import_asset` keeps its signature for the
     other kinds; for Brush+.abr the cmd/agent call `import_abr_asset`
     directly. Do NOT change `ImportOutcome`.
2. **main.rs** — `import_asset_cmd`: for `--kind brush` with an `.abr` file,
   call `import_abr_asset` and print ONE JSON line (serde_json) with the
   outcome fields; update the usage strings (:64 and the USAGE const) to
   mention `.abr` under brush. Exit codes like the other refusals.
3. **agent.rs** — the `import_asset` step (:2156): when kind == Brush and
   `s.file` ends with `.abr`, call `import_abr_asset` and return its fields as
   JSON (`targets` as a list, plus the counts); otherwise unchanged.

## Pins (tests in import_asset.rs; build a synthetic v6 .abr with the same
## byte layout as umber-brush's abr tests — one samp shared by two brushes +
## one computed brush)

- fan-out: 2 `.umberbrush` + 1 `.stamp.png` land in the tmp library (shared
  stamp deduped within the file), each preset loads via from_json+validate,
  the stamp PNG reads back via `read_png_rgba8` with the fixture's w/h and
  alpha byte-for-byte;
- TargetExists: pre-create one target → whole import refused, no other file
  written;
- dry-run: outcome lists targets, dir stays empty;
- outcome counts: presets/stamps/duplicates/orphans match the fixture;
- `.abr` with garbage bytes → typed `ImportError` (parse refusal), not a copy;
- `.umberbrush` path unchanged: all existing import-asset pins stay green;
- agent step on an .abr returns the multi fields (extend the existing step
  test shape);
- slug rules pinned: "My Brush!!" → `my-brush.umberbrush`, empty name →
  `abr-brush-0`, collision within the run → `-2` suffix.

## Doc comments

Name row 284; module doc of import_asset.rs gains the `.abr` bullet under
brush. Non-goals named: cross-library stamp dedup (blake3 vs existing
sidecars — follow-up), patterns, v8+, dynamics wiring, .abr writing.

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report what you changed and where.
