# Kill brief: abr-patt preset write (wave 3.1b)

- Lock: `razul-abr-patt-preset` over:
  - crates/umber-brush/src/preset.rs
  - crates/umber-brush/src/lib.rs (only if a re-export is needed)
  - crates/umber-cli/src/import_asset.rs
  - crates/umber-cli/tests/ (new or extended import test file)
- Builds on landed 3.1a (72b409cc): `AbrImportEntry::texture: Option<AbrImportTexture>` and `AbrImport::patterns: Vec<AbrPattern>` (each `AbrPattern` already carries `rgba: Vec<u8>`, RGBA8, alpha 255).
- Design source: docs/research/2026-11-09-abr-patt-analysis.md (row 322); recommended target = brush texture sidecar `<slug>.pattern.png` + optional texture block in `.umberbrush`.

## What exists

- `BrushPreset` (crates/umber-brush/src/preset.rs:234) is a serde struct written as the `.umberbrush` envelope; every field is `#[serde(default)]` and optional fields use `skip_serializing_if`, so pre-texture files round-trip byte-identical.
- The CLI ABR import (crates/umber-cli/src/import_asset.rs) plans one `AbrPlanItem { preset_path, json, stamp }` per brush, refuses if any target exists, writes `<slug>.umberbrush` + `<slug>.stamp.png` (via `umber_export::png::write_png(..., Transfer::Linear)`), and reports counts in `AbrImportOutcome`.
- 3.1a added `texture`/`patterns` to the import mapping but the CLI write path ignores them today.

## Target

1. **Preset schema block** (preset.rs): `pub struct PatternTexture { pub file: String, pub scale: f32, pub depth: f32, pub invert: bool, pub blend_mode: String, pub each_tip: bool }` (all serde defaults; `file` = sidecar file name relative to the preset, e.g. `<slug>.pattern.png`). `BrushPreset` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub texture: Option<PatternTexture>`. Extend `validate()`: if `texture` is `Some`, `file` must be a bare file name (no path separators, non-empty), `scale`/`depth` finite and in (0, 100]. Everything else unchanged.
2. **CLI write** (import_asset.rs): when `entry.texture` is `Some(t)`, resolve `imported.patterns[t.pattern_index]`, set `preset.texture = Some(PatternTexture { file: "<slug>.pattern.png", scale: t.scale as f32, ... })`, plan `pattern: Some((dir.join(format!("{slug}.pattern.png")), w, h, rgba))`, write the PNG sidecar with the same `write_png` call shape as the stamp, add the path to `targets` (exists check), and add a `patterns_written: u32` count to `AbrImportOutcome`. Dry-run counts it too.
3. **Tests**: preset round-trip byte-compat without texture (existing pins must stay green); a synthetic import with a pattern produces the sidecar + the block, and the block round-trips through `BrushPreset::from_json`; `TargetExists` fires on an existing `.pattern.png`; validate rejects a path-y `file`.

## Rules

- No engine consumer (that is 3.1c, own brief, not dispatchable yet). No GUI changes. No new deps. rustfmt-clean.
- Do NOT run cargo build/test/clippy; do NOT commit or push. Edit only the locked files. The dragon runs the battery (`-p umber-brush -p umber-cli`) and lands.
- Byte-compat holds: presets without `texture` serialize identically to before.
- Attribution: repo-facing comments say at most "exploration results assembled from online sources".

## Verification the dragon will run

rustfmt --check on touched files, check-layers, scorecard, clippy -p umber-brush -p umber-cli --all-targets -D warnings, cargo test -p umber-brush -p umber-cli, plus cargo check -p umber-app --all-targets (preset field is additive).
