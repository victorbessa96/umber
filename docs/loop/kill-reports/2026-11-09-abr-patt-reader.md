# Kill report: abr-patt-reader (wave 3.1a)

- Authorship: opencode claw (dispatch proc_272cea0db724, authorized lane per Bessa 2026-11-09) + dragon fixes; lock `razul-abr-patt-reader` over 6 umber-brush files
- Implements: docs/loop/briefs/2026-11-09-abr-patt-reader-kill-brief.md, condensed from docs/research/2026-11-09-abr-patt-analysis.md
- Verification: dragon battery 2026-11-09 — rustfmt clean, check-layers GREEN, scorecard current, `cargo clippy -p umber-brush --all-targets -D warnings` clean, `cargo test -p umber-brush` 254 passed / 0 failed (245 lib + 9 integration), `cargo check -p umber-cli -p umber-app --all-targets` clean

## Built

- `crates/umber-brush/src/abr_patt.rs` (NEW, ~1100 lines): `AbrPattern`, `decode_pattern`, `parse_with_budget`, the section-loop walker, `match_pattern`; soft decoding throughout (unknown version/mode/channel → `Ok` with the record skipped and counted in `skipped_patterns`, never a file refusal); caps before allocation (per-side 4096, max 64 patterns/file, decoded bytes charged to the shared per-file alpha budget); no panics, no new deps
- `crates/umber-brush/src/abr.rs`: `patt` arm in the section walk, `AbrFile::patterns` / `AbrFile::skipped_patterns`, `AbrBrush::texture: Option<AbrTexture>` (gated on the brush's `useTexture` flag + the `Txtr` descriptor), `match_pattern` next to `match_sample`
- `crates/umber-brush/src/abr_import.rs`: pattern mapping into `AbrImportEntry::texture` / `AbrImport::patterns`, per-brush texture scale preserved on the new field
- `crates/umber-brush/src/lib.rs`: public re-exports
- `crates/umber-brush/tests/abr_real_patt.rs` (NEW): real-file pin on the vendored `abr_real_v6_2_full.abr` (empty `patt` section → `patterns` empty, `skipped_patterns == 0`, brush still "Soft Round") plus the synthetic-record unit pins (16)
- `crates/umber-brush/tests/fixtures/FIXTURES.md`: no new binary fixture (Q3 default: no)

## Judgment calls

- Dragon fix after claw exit (claw could not run cargo): import of `match_sample`, the per-brush texture scale moved onto `AbrTexture.scale`, and a temporary-lifetime bind in a synthetic builder — all compile errors only, no behavior change
- Dragon fix: the claw's test helper `decode()` passed the length-prefixed record to `decode_pattern`, which expects the body; the helper now strips the 4-byte prefix. 8 synthetic pins failed on exactly this; all pass after
- Format knowledge is clean-room from public documentation/analysis; repo-facing comments carry at most "exploration results assembled from online sources"

## Unverified

- No real-world file with a populated `patt` section exists in the fixture set, so the decoder's channel-sub-rect and palette paths are pinned only against synthetic records built to the analyzed layout. First real file with patterns will exercise them live
- Byte-compat of existing imports is proven by the untouched v6/v7/v9/v10 pins (all green), not by a new differential fuzz run

## Deferred

- 3.1b (preset schema block + CLI `--write` for pattern presets) is a separate kill, queued next
- Painting with a pattern texture (runtime sampling of `AbrPattern` in the dab path) is wave 3 tail, not this kill
