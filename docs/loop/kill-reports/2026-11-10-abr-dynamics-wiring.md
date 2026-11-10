# Kill report: abr dynamics → engine wiring (wave 3.3)

- Authorship: opencode claw (dispatch proc_7b734980d58d) + dragon clippy fix; lock `razul-abr-dynamics` over abr_import.rs, dab_adapter.rs, lib.rs, import_asset.rs, brush_panel/abr_import.rs
- Implements: docs/loop/briefs/2026-11-10-abr-dynamics-wiring.md
- Verification: dragon battery 2026-11-10 — rustfmt clean, check-layers GREEN, scorecard current, `cargo clippy -p umber-brush -p umber-cli -p umber-app --all-targets -D warnings` clean, `cargo test -p umber-brush -p umber-cli` 391 passed / 0 failed, `cargo check -p umber-app --all-targets` clean

## Built

- `crates/umber-brush/src/abr_import.rs`: `map_preset` now maps `AbrDynamics` when `control_index != 0` — `jitter` (fraction) → `params.jitter.size` (clamped to the Jitter range), `minimum` → a 4-point linear `radius_curve` floor from `(0, minimum)` to `(1, 1)`; `steps`/`control_index` have no engine counterpart and are counted in a new `AbrImport::dynamics_unmapped`; module doc synced to name the landed behavior; byte-compat pinned: `dynamics: None` or `control_index == 0` serializes exactly as before
- `crates/umber-cli/src/import_asset.rs`: `dynamics_unmapped` surfaced in `AbrImportOutcome` (dry-run included); synthetic szVr fixtures pin jitter, the minimum floor, the off case, and the counts
- `crates/umber-app/src/brush_panel/abr_import.rs`: same dynamics mapping on the GUI importer (its size semantics untouched), so both importers agree

## Judgment calls

- Dragon fix after claw exit: clippy's `match_same_arms`-class lint on the `ControlCurve::try_new` match → `unwrap_or_else`; no behavior change
- No fade param invented: the engine has no fade-over-distance, so steps/control are counted, not approximated
- Minimum floor uses a 4-point linear curve (the ControlCurve API's expressible shape), not a true constant floor

## Unverified

- Real ABR files with populated `szVr` are not in the fixture set; mapping is pinned against synthetic records
- Whether ABR's `minimum` semantics match a pressure-curve floor exactly is inferred from public format descriptions, not a behavioral test

## Deferred

- 3.2 v8+ descriptor-only acceptance — gated on Bessa's answers to Q-a/Q-b (queue row 333)
- 3.4 cross-library stamp dedup (blake3) — last, widest surface
