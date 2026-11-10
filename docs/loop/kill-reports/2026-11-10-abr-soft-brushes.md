# Kill report: ABR per-brush soft failure (wave 3.2)

- Authorship: opencode claw (dispatch proc_c18ee097cf2b); lock `razul-abr-soft-brushes` over abr.rs, abr_import.rs, import_asset.rs; dragon battery green first pass
- Implements: docs/loop/briefs/2026-11-10-abr-soft-brushes.md; analysis docs/research/2026-11-10-abr-v8-descriptor-analysis.md
- Verification: rustfmt clean, check-layers GREEN, scorecard current, clippy -p umber-brush -p umber-cli --all-targets -D warnings clean, cargo test -p umber-brush -p umber-cli 391+ passed / 0 failed, cargo check -p umber-app --all-targets clean

## Built

- `parse_desc` (abr.rs): one brush descriptor that fails to parse now skips and counts (`AbrFile.skipped_brushes`, saturating) instead of aborting the whole file; only structural file-level failures still abort
- Version gate unchanged (6/7/9/10); comment and module doc reworded from "v8 is not a thing" to "unattested in the public record"
- `skipped_brushes` threaded through `AbrImport` and `AbrImportOutcome` (dry-run included) and the CLI summary
- Tests: good brush + Brsh-less brush in one file imports with count 1; byte-compat pins untouched and green

## Judgment calls

- Q-a/Q-b from row 333 resolved by standing-autonomy default: tip-less brushes keep importing via the radial fallback; per-importer size semantics left unchanged (divergence documented, not unified — no visible default change)
- The wave plan's premise (v8+ descriptor-only files are legal) was disproved by the analysis; the kill fixes the real defect class instead

## Unverified

- No real-world file with a malformed brush descriptor in fixtures; the skip path is pinned against synthetic records

## Deferred

- 3.4 cross-library stamp dedup (blake3) — next kill
