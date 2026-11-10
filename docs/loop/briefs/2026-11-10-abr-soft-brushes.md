# Kill brief: ABR per-brush soft failure (wave 3.2)

- Lock: `razul-abr-soft-brushes` over:
  - crates/umber-brush/src/abr.rs
  - crates/umber-brush/src/abr_import.rs
  - crates/umber-cli/src/import_asset.rs (outcome count only)
- Decision basis: docs/research/2026-11-10-abr-v8-descriptor-analysis.md (landed, row 333). Its headline: v8/11 are unattested in the public record, so the reader KEEPS refusing them (reword the "v8 is not a thing" comment at abr.rs:482-484 to "unattested", not a format fact). The real defect: one brush descriptor that fails to parse aborts the WHOLE file.
- Partner questions Q-a/Q-b resolved by default under the standing autonomy directive: tip-less brushes already import via the radial fallback when their descriptor parses (keep); ABR size semantics stay as-is per importer (no visible default change; divergence documented, not unified).

## Target

1. `parse_desc` (abr.rs:570-589): a `parse_brush` error for ONE brush becomes skip-and-count, not file abort. Add `pub skipped_brushes: u32` to `AbrFile` (next to `skipped_shape_classes`), saturating_add on error. Keep `Ok(())` overall. Only structural file-level failures (bad descriptor root, bad section framing) still abort.
2. Reword abr.rs:482-484: "v8 is not a thing" -> v8/11+ are unattested in the public record; the whitelist stays 6/7/9/10. Also the module doc line 59 ("v8+ files ... rejected as unsupported") -> "rejected as unattested versions".
3. Surface the count: `AbrImport` gains `skipped_brushes: u32` (cloned from the file), `AbrImportOutcome` gains the same field, dry-run included; CLI prints it in its summary line next to the other counts.
4. Tests: synthetic ABR with one good brush + one brush descriptor missing `Brsh` -> file imports, good brush present, `skipped_brushes == 1`; all-bad file -> imports with zero brushes (or errors only if zero brushes is already an error — check and keep current behavior); existing pins untouched. Byte-compat: files that parse today parse identically.

## Rules

- Edit only the locked files. No cargo. No commit/push. rustfmt-clean (edition 2021). No new deps.
- Never refuse a file that imports today; this kill only makes MORE files import.
- Attribution: repo-facing comments at most "exploration results assembled from online sources".

## Dragon verification

rustfmt --check, check-layers, scorecard, clippy -p umber-brush -p umber-cli --all-targets -D warnings, test -p umber-brush -p umber-cli, check -p umber-app --all-targets.
