# Kill report: abr-patt preset write (wave 3.1b)

- Authorship: opencode claw (dispatch proc_8ee734491835) + dragon compile fix; lock `razul-abr-patt-preset` over preset.rs, lib.rs, abr_import.rs, preset_library.rs, import_asset.rs, brush_panel.rs, brush_panel/abr_import.rs
- Implements: docs/loop/briefs/2026-11-10-abr-patt-preset-write.md
- Verification: dragon battery 2026-11-10 — rustfmt clean, check-layers GREEN, scorecard current, `cargo clippy -p umber-brush -p umber-cli --all-targets -D warnings` clean, `cargo test -p umber-brush -p umber-cli` 386 passed / 0 failed, `cargo check -p umber-app --all-targets` clean

## Built

- `crates/umber-brush/src/preset.rs`: `PatternTexture { file, scale, depth, invert, blend_mode, each_tip }` (all serde defaults) + `BrushPreset.texture: Option<PatternTexture>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; `validate()` requires a bare non-empty file name and finite scale/depth in (0, 100] when present
- `crates/umber-cli/src/import_asset.rs`: import plans and writes the `<slug>.pattern.png` sidecar (same `write_png(..., Transfer::Linear)` shape as the stamp), sets `preset.texture` from `AbrImportTexture` + `patterns[pattern_index]`, adds the sidecar to the targets/exists check, and reports `patterns_written` in `AbrImportOutcome` (dry-run included); new synthetic fixture `bytes_with_pattern()` pins a two-brush one-pattern file
- Struct-literal compile fixes in the four other locked files (`texture: None`), each with a comment naming the path that leaves the block empty (GUI importer drops `abr.patterns`, so its block stays empty by design)

## Judgment calls

- Lock extended mid-kill to adopt the claw's four compile-necessity files (abr_import.rs, preset_library.rs, both brush_panel files) — the new field breaks every `BrushPreset` literal; per the kill-locks adoption rule the claw's bytes are committed with this provenance note
- Dragon fix after claw exit: missing `Enc::into_vec()` in the new fixture helper (claw could not run cargo); one method added, no behavior change
- GUI path (ag-psd based) intentionally leaves `texture: None` — the two-importer consolidation is open question Q2 in the research doc, not this kill

## Unverified

- No engine consumer reads the block yet (that is 3.1c, gated on its own brief); the block is persisted data only
- No real ABR with a populated `patt` section exists in fixtures; sidecar content is pinned against the synthetic pattern

## Deferred

- 3.1c engine consumer (pattern sampling in the dab path) — own brief, not dispatchable yet
- 3.2 v8+ descriptor-only files — needs the stamp-less mapping decision (partner question)
