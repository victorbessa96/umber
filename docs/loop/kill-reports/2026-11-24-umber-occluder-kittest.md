# OccluderScope combo → request → resolution kittest

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-occluder-kittest.md` (`f9bdd6d47444`).
- Claim: `c06726576fef`, queue row 590.
- Implementation: `69f02313381cbff393d90b3cb3b6b9bf40ee3a9b`.
- Final pre-commit source-diff SHA-256: `9da5a50a68e11c729a00987a8e3c0f318731dd9958e1e1d2aa07367b51c985d9`.
- Kill lock: `razul-occluder-kittest`.

## Built

- Added one full `egui_kittest` using the real `BakesPanel::show` with a `GpuContext` and the two-material ground-plus-floating-slab mesh.
- The combo is opened by its accessible value (`Whole mesh` / `Baked set only`) and drives the actual `occluder_own_set` state. `get_by_label` does not match this combo's value, so `get_by_value` is used to open it.
- `bake_current` runs through the real worker and poll path twice. OwnSet completes with request snapshot `(true, 512, 16)` and Ground AO fully lit; WholeMesh completes with `(false, 512, 16)` and Ground AO darkened under the slab.
- `completed_jobs == 2` and `last_job_failed == false` are asserted. Actual Ground AO PNGs are decoded for the lit/dark assertions; the Slab set is disabled and the scratch output directory is removed.
- Added a test-only `last_request_scope` hook in `BakesPanel`, captured immediately before `start_job`. No production `BakeRequest`, `run_bake`, call site, settings carry, or i18n changed.
- A missing GPU adapter is a hard test failure, not a graceful skip.

## Review and dispositions

- Independent review `deleg_8cba1f3b`: **SHIP**, zero findings. It independently confirmed the hook is entirely `#[cfg(test)]`, the real combo is driven by accessible value, both bakes use the real worker/poll flow, completions advance 1→2, and the Ground AO lit/dark assertions hold.

## Verification

- RED: `occluder_scope_combo_drives_request_and_resolution` failed with `last_request_scope == None` after the real OwnSet bake and AO assertion passed.
- GREEN: exact kittest 1/1 after capturing the real `BakeRequest` values before `start_job`.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings` → exit 0.
- Full `cargo test -p umber-app` with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **1,277 passed, 0 failed across 11 suite rows**: 323 lib + 907 bin + 2 filter-apply + 1 Gate0 + 3 keyboard-menu + 17 panel-kittest + 11 snapshots + 10 panic-hunt + 2 pool-perf + 1 set-switch + 0 doc-tests.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-app/src/bakes_panel.rs` → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-occluder-kittest-*.sh` probe reran the exact kittest: **1/1**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the app suite, not a workspace-green claim.

## Judgment calls

- The test-only snapshot is observability for the real request construction, not a second bake path. Output assertions use actual PNGs, so the proof is not limited to the combo's boolean state.
- The supported 512 preset is used; an earlier invalid 64 size was rejected by the existing preset contract and corrected before RED evidence was accepted.
- The Slab set is disabled to keep only Ground's AO result relevant, and the scratch output directory is removed after assertions.

## Unverified

- No dedicated interactive application-window bake-button smoke was run. The real headless panel, GPU worker, poll path, and decoded AO outputs carry the evidence. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: XI2 hotplug re-query.
- `OccluderScope::Sets(BTreeSet)` remains deferred with its set-list picker.
