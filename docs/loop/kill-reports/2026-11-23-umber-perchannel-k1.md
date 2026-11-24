# PerChannel K1 — Scalar-semantic blend restriction

- Date: 2026-11-23.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-23-umber-perchannel-k1.md` (`27c11e66b666`).
- Claim: `96f4a53a5c48`, queue row 582.
- Implementation: `4383e30393b10a84a74dca3dfa5eabad7703334a`.
- Final pre-commit source-diff SHA-256: `cd555462bad82888fb2fbf40039f8f0029294b242e127a65904e667a5b8cffd5`.
- Kill lock: `razul-perchannel-k1`.
- Staged design: `docs/research/perchannel-mode-restriction-2026-10-22.md`, Option 4.

## Built

- Added `channel_is_color_semantic` beside the picker. `baseColor`, `diffuse`, `albedo`, and `emissive` preserve the full color-mode list. The empty string is explicitly the separate layer-compositing context and preserves its established list. Other unknown non-empty names default scalar.
- Added one filter clause hiding exactly Tint, Saturation, and Color on scalar-semantic channels. Value remains everywhere. `normal` bypasses the semantic filter and remains at 31 modes.
- Scalar channels now expose 25 modes; color-semantic channels and the empty layer context keep 28. Passthrough and normal-map gating, enum order, public signatures, saved parameters, and driver evaluation are unchanged.
- Extended the existing picker matrix across normal, the empty context, four color names, and seven scalar/custom names. It pins counts, HSV presence/absence, Value, Multiply, Normal, and normal-map exclusions.

## Review and dispositions

- First review `deleg_14d8487c` interrupted without a verdict; it was not consumed as approval.
- Replacement review `deleg_52c65481`: **SHIP**, zero findings. It independently confirmed normal=31, color semantic and the empty layer context=28, scalar/unknown non-empty=25, Value retained, enum order and saved-document behavior unchanged, and paint/folder counts 28/29 through `document.rs::layer_blend_options`.

## Verification

- RED on the unmodified picker: `cargo test -p umber-app blend_options_filter_passthrough_and_gate_the_normal_trio -- --nocapture` failed at roughness with `left: 28`, `right: 25`; 0 passed, 1 failed. This is the intended new-contract witness.
- GREEN after the predicate/filter: the same test passed in both the lib and binary targets, 1/1 each. Filtered integration targets reported zero selected tests and were not counted as evidence.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-app/src/layers_panel.rs` → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings` → exit 0, zero warnings.
- Full `cargo test -p umber-app` with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **1,260 passed, 0 failed across 11 suite rows**: 321 lib + 892 bin + 2 filter-apply + 1 Gate0 + 3 keyboard-menu + 17 panel-kittest + 11 snapshots + 10 panic-hunt + 2 pool-perf + 1 set-switch + 0 doc-tests.
- The first full run without the override failed seven known env/plugin tests whose compiled fixture root pointed at an old safe-push scratch directory. The app-only rerun used the documented repo-root override and passed all seven plus every other test. This is the `umber-wave-ops` main-tree asset-root pitfall, not a K1 regression.
- A fresh OS-safe `hermes-verify-perchannel-*.sh` probe reran the exact picker test with rustfmt: 1/1 lib + 1/1 bin, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the app suite, not a workspace-green claim.

## Judgment calls

- The empty string is not treated as an unknown material channel. It belongs to the separate layer-compositing picker and keeps its existing full list; K1 does not silently remove three layer-level blend modes.
- Unknown non-empty names default scalar, matching the design's recoverable and honest custom-data default. A future color-named custom channel must extend the pinned color set deliberately.
- Value is retained because approximately-Normal on gray is redundant but meaningful. No evidence supports removing it.
- The change is picker-only. Existing documents that stored Tint/Saturation/Color on a scalar channel continue to load and evaluate identically; the combo only refuses new selections.

## Unverified

- No dedicated pointer-driven material-channel dropdown smoke was added. The pure picker matrix runs in both app targets, and the full app/integration suite passed. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: Bake/Export history `record_context`. Then all-channel material picking, the Assets mesh category, OccluderScope threading kittest, and XI2 hotplug re-query.
- K2 tooltip work, exact Painter per-channel tables, MaterialOutput type threading, and colored-scalar export warnings remain separate designs.
