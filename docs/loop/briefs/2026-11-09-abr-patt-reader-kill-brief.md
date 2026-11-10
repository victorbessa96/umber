# Kill brief — 2026-11-09 — wave 3.1a: `abr-patt-reader` (umber-brush pattern reader + mapping)

Implements the 3.1a section of
`docs/research/2026-11-09-abr-patt-analysis.md` (the fable-5.2 analysis,
landed as row 322). Kill lock: `razul-abr-patt-reader`. Authorship: claw
writes (opencode lane), dragon reviews + batteries + measures.

## Surface (exactly these files)

- `crates/umber-brush/src/abr_patt.rs` — NEW: the pattern record decoder and `AbrPattern`.
- `crates/umber-brush/src/lib.rs` — `pub(crate) mod abr_patt;` next to `lib.rs:32`.
- `crates/umber-brush/src/abr.rs` — the `patt` arm in `walk`, `AbrFile::patterns` + `skipped_patterns`, `AbrBrush::texture`, `match_pattern`, texture keys in `parse_brush`, `pub(crate)` on `Reader` / `decode_packbits_row`, module docs.
- `crates/umber-brush/src/abr_import.rs` — required even for a reader-only change: its test helpers build `AbrBrush`/`AbrFile` as full struct literals (`:192-205`, `:224-228`). Also the mapping: `AbrImportEntry::texture`, `AbrImport::patterns`.
- `crates/umber-brush/tests/abr_real_patt.rs` — NEW.
- `crates/umber-brush/tests/fixtures/FIXTURES.md` and one new fixture ONLY if the partner approved Q3 (default: NO new binary fixture; use the vendored `abr_real_v6_2_full.abr` with its empty `patt`, plus synthetic records).

No other crate needs to change to keep compiling: `AbrImportEntry`/`AbrImport`
are only built inside `abr_import.rs`, and the CLI reads them by field.
CLI behavior is unchanged after this kill.

## Design points (state them in the module docs)

- **Pattern decoding is SOFT.** A `patt` section has been accepted by skipping since row 268: no pattern problem may turn a file that imports today into a refusal. An undecodable record is skipped by its declared length and counted in `AbrFile::skipped_patterns`; a record running past the section ends pattern parsing for that section and counts once. Deliberately unlike `desc`, which is strict.
- **Full mode only.** `read_samples` keeps skipping `patt` by length (`abr.rs:337-339`).
- **Caps, checked BEFORE allocation:** per-side cap (default 4096), patterns-per-file cap (default 64), name length ≤ record length, decoded bytes charged against the same per-file budget as stamps (`MAX_TOTAL_ALPHA_BYTES`, `abr.rs:131`). Mirror the hostile shapes the GUI pre-flight lists (`brush_panel/abr_import.rs:155-220`).
- **No panics.** The reference's `color_mode.unwrap()` and slice indexing must not be ported as such.
- **Texture gate:** `AbrBrush::texture` is `Some` only when `useTexture` is true and `Txtr` is a descriptor. Percent fields via `parse_percent` (`abr_desc.rs:288`); blend mode via `as_text` (`abr_desc.rs:276-281`).
- **Duplicate-stamp rule unchanged** (`abr_import.rs:111-115`).
- Format knowledge comes from public documentation/analysis of the ABR format (clean-room); the repo-facing docs say "exploration results assembled from online sources" at most (attribution ruling).

## Pins

Unit pins on synthetic records (from the research plan §4, items 1-16):
grayscale/RGB/indexed raw mapping, RLE row, 16-bit downconvert, channel
sub-rect, unwritten slots, UTF-16 name, padding, soft failures (mode 4,
version 2, pixel-array v2, depth mismatch/1, compression 2, indexed+RLE)
each yielding `patterns.len()==0, skipped_patterns==1, Ok` with following
`desc` brushes intact; record-past-section-end; cap+1 rect skipped before
allocating; `read_samples` unaffected by garbage `patt`; `parse_brush`
texture gate both ways; `match_pattern` by id; mapping dedup/orphan/missing
counts.

Real-file pins in `tests/abr_real_patt.rs`: vendored
`abr_real_v6_2_full.abr` (empty `patt`): `patterns.is_empty()`,
`skipped_patterns == 0`, brush still "Soft Round".

## Rules

- No cargo at all (dragon batteries). No new deps. rustfmt-clean. No commit/push.
- Byte-compat: files without `patt` import identically; duplicate-stamp drop unchanged.
- Return: files changed, tests added by name, judgment calls, anything unverifiable without compiling.
