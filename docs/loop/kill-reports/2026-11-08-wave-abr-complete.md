# Kill report — wave-abr complete: .abr → .umberbrush end-to-end (rows 282-285)

Date: 2026-11-08, session `local-ee79b5c` (Razul). Builds on row 268
(`ca70ea80`, samp reader, claw-authored + dragon-corrected) and row 271
(`d8ae420c`, real-file verification, dragon). Design:
`docs/loop/briefs/2026-11-07-wave-abr-design.md` +
`docs/loop/briefs/2026-11-08-wave-abr-desc-design.md`. Format authority
(vendored, clean-room pattern-port only): `docs/claw-artifacts/ag-psd/`.

## The rows

| Row | Commit | Surface | What landed |
| --- | --- | --- | --- |
| 282 | `a68f97f4` | NEW `crates/umber-brush/src/abr_desc.rs`, `abr.rs`, `lib.rs` | claw claude/opus f2644ba1 $3.04. Clean-room minimal descriptor reader (bounds-checked BE cursor, version-16 wrapper, full OSType dispatch incl. `obj ` references/ObAr/Pth, UNITS_MAP, caps MAX_DESC_ITEMS/LIST/TEXT/RAW/DEPTH/REFERENCE checked before allocation). `abr.rs` gains the `desc` 8BIM arm (`Brsh` list walk), `read_abr -> AbrFile { samples, brushes, skipped_shape_classes }`, `AbrBrush` (tool class, name, diameter_px, angle_deg, roundness, spacing_on/spacing, flips, hardness, sampled_data_id, dynamics), `match_sample` by id, `phry` parse-and-ignore; `read_samples` signature unchanged; computedBrush+sampledBrush parsed, other shape classes skipped+counted. Named `AbrError` variants for every refusal. 36 pins. |
| 283 | `c9ee71e5` | NEW `crates/umber-brush/src/abr_import.rs`, `lib.rs` | claw claude/sonnet 80cae1c4 $0.35. `import_abr(bytes) -> AbrImport { entries: Vec<AbrImportEntry { preset, stamp: Option<(w,h,RGBA8)> }>, orphan_samples, brushes_without_sample, duplicate_stamps_skipped }`. Mapping per the design table: diameter→`lazy_mouse.radius_px = d/2` clamp [1,4096] (NaN/0→1.0), hardness clamp (non-finite→1.0), spacing on→`dabs_per_radius = 1/(2s)` clamp [1,100] (off/0→100), angle→`StampAngle::Fixed`, sampled→`stamped`+white-RGB/gray-alpha stamp, computed/unmatched→radial+counted, exact-byte stamp dedup, empty-name fallback, every preset `validate()`s + JSON round-trips. 10 pins. |
| 284 | `4cc2bdfe` | `crates/umber-cli/src/{import_asset,main,agent}.rs` | claw claude/opus b307cce7 $1.93. `import-asset --kind brush` accepts `.abr`: `import_abr_asset` fans out to `<slug>.umberbrush` (`to_json`) + `<slug>.stamp.png` (`umber_export::png::write_png`, `Transfer::Linear`, top-first so the app's `read_png_rgba8`→`set_stamp` path loads it like every existing sidecar). All targets computed first; ANY existing → whole-import `TargetExists`, nothing written. Dry-run = plan only. Slug sanitized, `-2` on collision. One JSON outcome line + agent-step fields. `validate()` accepts `.abr` via `read_abr` parse-check; the `.umberbrush` lane is untouched. 8 pins. |
| 285 | `577bc07c` | NEW `crates/umber-brush/tests/abr_real_desc.rs` + 4 fixture files + FIXTURES.md | dragon. Vendored the UNTRIMMED upstream `simple/src.abr` (662 B) and `sample-and-pattern/src.abr` trimmed to `samp`+`desc` (10,816 B) with both upstream `data.json` oracles. 3 pins: the full file parses exactly upstream's "Soft Round" computed brush (size 30, spacing 0.25, hardness 0); the samp+desc file parses "Bobbys Brush 743" (sampled, size 112, spacing 0.02, hardness default 1.0) with `match_sample` linking sample `a8f34db0-...` (112x105); the samp plane is byte-identical to the row-271 samp-only trim. |

## Live verification (real files through the real CLI)

`cargo build -p umber-cli`, then on the vendored fixtures:
`abr_real_v6_2_samp_desc.abr` → dry-run plan + import wrote
`bobbys-brush-743.umberbrush` (valid preset JSON) + `bobbys-brush-743.stamp.png`
(112x105 RGBA8 PNG). `abr_real_v6_2_full.abr` → `soft-round.umberbrush`:
name "Soft Round", stamped=false, hardness 0.0, radius_px 15.0 (30/2),
dabs_per_radius 2.0 (1/(2·0.25)), stamp_angle fixed 0.

## Gates

Each row: fmt clean, layers GREEN 16, scorecard current, clippy
`--all-targets -D warnings` 0 on the touched crate, full crate suite green
(umber-brush 222/0 at row 283 → 218 lib + real-file suites at 285; umber-cli
149/0), safe-push gates on every push. Live CLI run above on top.

## Judgment calls

- Row order of the stamp: written top-first via `write_png` (the app's
  `read_png_rgba8` flips to GPU order for `set_stamp`), matching every
  existing sidecar — the claw's test compares against the row-reversed
  fixture, which is that contract.
- Hardness absent from a real descriptor → the reference's default (1.0),
  pinned against upstream data.json ("Bobbys Brush 743" has no hardness key).
- `dabs_per_radius = 1/(2·spacing)`: spacing% is a fraction of the diameter
  between dabs; radius = diameter/2, so dabs per radius = 1/(2·s).

## Deferred (named)

Cross-library stamp dedup by blake3 vs existing sidecars; `patt` patterns;
v8+ descriptor-only files; dynamics→engine wiring; dual brush / texture /
scattering / roundness / flips (anamorphic dabs out of scope); `.abr`
writing (never).
