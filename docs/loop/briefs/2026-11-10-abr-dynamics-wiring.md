# Kill brief: ABR dynamics → engine wiring (wave 3.3)

- Lock: `razul-abr-dynamics` over:
  - crates/umber-brush/src/abr_import.rs
  - crates/umber-brush/src/dab_adapter.rs (only if Jitter/ControlCurve APIs need a read-only helper; prefer not to touch)
  - crates/umber-brush/src/lib.rs (re-exports only if needed)
  - crates/umber-cli/src/import_asset.rs (only if an outcome count changes)
  - crates/umber-app/src/brush_panel/abr_import.rs (GUI path parity for the same mapping, `texture: None` precedent: keep the GUI's existing size semantics, only add the dynamics mapping)
- Context: wave 3.3 of docs/loop/waves/2026-11-09-wave-plan.md. `AbrBrush::dynamics` (`szVr`: abr.rs:308-320 — `control_index`, `steps`, `jitter` fraction, `minimum` fraction) is parsed but dropped by the mapping (abr_import.rs:291 `dynamics: None` in the preset literal; module doc abr_import.rs:50 names this as the open wiring).

## Target mapping (engine side already exists)

Engine params (crates/umber-brush/src/dab_adapter.rs:174-230): `BrushParams { alpha, hardness, pressure_gamma, pressure_curve, wash_mode, falloff, falloff_custom, stamped, stamp_angle, jitter: Jitter, pressure_maps }`; `Jitter` is the size/angle/position/flow quartet (dab_adapter.rs:98-143) with `#[serde(default)]` = no jitter; `PressureMaps { flow, size }` are ControlCurves (preset.rs `alpha_curve`/`radius_curve` fold in via `engine_params()`).

Map in `map_preset` (abr_import.rs), only when `dynamics` is `Some` AND `control_index != 0` (0 = off):

1. `dynamics.jitter` (already a fraction, percent/100) → `params.jitter.size = jitter as f32` clamped to the Jitter field's documented range (read dab_adapter.rs:98-143 for it). Position/angle/flow jitter stay 0.
2. `dynamics.minimum` (fraction, 0 = none) → the size pressure map floor: when `minimum > 0.0`, set `radius_curve` to a two-point linear ControlCurve from `(0.0, minimum)` to `(1.0, 1.0)` (read ControlCurve's API in preset.rs / wherever it lives; if it cannot express a floor, use the closest supported shape and SAY SO in your summary). Do not touch `alpha_curve`.
3. `steps` (fade step count) and `control_index`: the engine has no fade-over-distance param. Do NOT invent one. Leave unmapped; count them: add `dynamics_unmapped: u32` (brushes whose szVr carried steps/control but no engine equivalent) to `AbrImport` and `AbrImportOutcome`, dry-run included.

## Rules

- Byte-compat: a brush with `dynamics: None` or `control_index == 0` maps EXACTLY as today (all existing pins must stay green untouched).
- Serde: no new preset fields unless strictly required (prefer folding into existing `jitter`/`radius_curve`); if a field is added it needs `#[serde(default)]` + skip_serializing_if so old files round-trip.
- GUI path (brush_panel/abr_import.rs): apply the same jitter/minimum mapping so both importers agree; keep its existing size semantics untouched.
- Tests: synthetic ABR with szVr (extend the existing `abr_fixture` Enc builders in import_asset.rs tests and/or abr_import tests): jitter lands on `preset.params.jitter.size`; minimum lands on radius_curve floor; control_index 0 → all defaults; counts increment. Byte-compat pin: a no-dynamics brush serializes identically to HEAD behavior.
- No new deps, rustfmt-clean (edition 2021). Do NOT run cargo. Do NOT commit/push. Edit only locked files (+ test modules inside them).
- Attribution: repo-facing comments at most "exploration results assembled from online sources".

## Dragon verification

rustfmt --check touched files, check-layers, scorecard, clippy -p umber-brush -p umber-cli -p umber-app --all-targets -D warnings, test -p umber-brush -p umber-cli, check -p umber-app --all-targets.
