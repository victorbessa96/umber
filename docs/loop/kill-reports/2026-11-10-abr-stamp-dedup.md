# Kill report: cross-library stamp dedup (wave 3.4)

- Authorship: opencode claw (dispatch proc_f7f03d6bbd0a); lock `razul-abr-stamp-dedup` over import_asset.rs (+abr_import.rs); dragon battery green first pass
- Implements: docs/loop/briefs/2026-11-10-abr-stamp-dedup.md
- Verification: rustfmt clean, check-layers GREEN, scorecard current, clippy -p umber-cli -p umber-brush --all-targets -D warnings clean, cargo test both crates 0 failed, cargo check -p umber-app --all-targets clean

## Built

- Plan phase in `crates/umber-cli/src/import_asset.rs` now scans the target dir's existing `*.stamp.png`, decodes each with `umber_export::png::read_png_rgba8`, and blake3-hashes the RGBA bytes (workspace dep, Cargo.toml:62)
- A planned stamp whose hash already exists (earlier in-file entry or an existing library file) drops the whole item: no preset, no stamp write, counted in new `cross_library_duplicates_skipped` on `AbrImportOutcome` (dry-run included); in-file duplicates keep their existing count
- Targets/exists check runs on the post-dedup plan; summary prints both counts; tests pin identical-stamp skip, different-bytes both-write, and dry-run counting

## Judgment calls

- Same drop-the-later rule as the in-file dedup — consistent semantics, saves disk only; no preset schema change (sharing one stamp file between presets would need a new field, deliberately out of scope)
- Undecodable existing PNGs are skipped silently, never judged or deleted

## Unverified

- Hash is over decoded RGBA, so a foreign-encoded PNG with identical pixels still dedups correctly; tested only against encoder-written fixtures

## Deferred

- Shared-stamp preset field (one file, many presets) — only if disk data says it matters
- Wave 3 is now closed; next per wave plan: wave 4 (4.1 smart-material params schema, 4.2 community templates, 4.3 procedural history)
