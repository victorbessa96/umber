# Bake/Export history `record_context`

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-history-context.md` (`8ab0264ca923`).
- Claim: `30011e23d8ef`, queue row 584.
- Implementation: `364b082ebc773f37dc1defb7e01e14754a3550b4`.
- Final pre-commit source-diff SHA-256: `268ee036c959d20fed47c0871af224f982db7774ce3c61ab9804d30a8e17ab85`.
- Kill lock: `razul-history-context`.
- Requirement: `docs/specs/requirements.md:155`, the Bakes/Export `record_context` follow-up.

## Built

- Added `history_panel::record_job_result`: successful jobs call `record_session_event`; failed jobs do nothing. Unit coverage pins both outcomes and every depth `None`.
- Wired the shell's existing once-only `prev_baking` completion edge to record `HistoryKind::Bake` only when `last_job_failed()` is false. Repeated idle frames add nothing.
- Made `ExportContext.doc` mutable and moved the context by value through `export_current`, `run_requested_export`, and `export_now`, so the real synchronous result can record `HistoryKind::Export` in its success arm. `run` uses `as_deref` for the existing token reader.
- Early no-GPU refusals and failed driver runs record nothing. Export statuses, written/skipped lists, `completed_exports`, `last_export_failed`, UI behavior, and driver output are unchanged.
- Added a real 8x8 export acceptance test: first success adds one row, second adds exactly one more, failed driver adds none, early refusal adds none. The helper test covers success/failure and display-only depth semantics.

## Review and dispositions

- R1 `deleg_2460132d`: **REQUEST_CHANGES**, two valid findings.
  - **P1:** `prev_baking` was set and consumed in the same running frame, so running frames could add rows and landing could add none. Fixed with an explicit `was_baking`/`still_baking` transition through `job_completed(was, still)`. `job_completion_fires_only_on_landing_edge` pins idle, start, running, landing, and the following idle frame. The focused `job_` run passed 2/2 lib + 7/7 bin, including real worker completion/failure tests.
  - **P2:** the real export test skipped absent GPUs and asserted no files. It now requires a suitable adapter, asserts non-empty `last_written`, decodes every written PNG, and requires 8x8 dimensions before cleanup. Focused export acceptance passes 1/1 in bin.
- R2 `deleg_6ca74146`: **REQUEST_CHANGES**, one valid finding. A completed job could be replaced by a new bake before the shell poll, so the frame transition suppressed the old row. Fixed by moving history events into `BakesPanel.job_history`: each `poll_job` landing enqueues its failure flag once, and the shell drains independently of the CURRENT worker. `completed_job_history_survives_a_restart_before_the_shell_drains` proves the old landing survives a restart, then success and failure each drain exactly once. The frame `job_completed` edge now serves only the existing cage update, not history.
- R3 `deleg_71ccde41` interrupted without a verdict; it was not consumed as approval.
- R4 `deleg_bfc2e18a`: **SHIP**, zero findings. It independently confirmed each landing enqueues and drains once even during a successor job, the shell drains independently of the CURRENT worker, failures record nothing, export records only success, and the real test requires GPU and decodes every written file as 8x8 PNG data.

## Verification

- RED: `job_results_record_only_successful_jobs` failed with `left: 0`, `right: 2` after two successful helper calls. The real export test was then implemented and passed after the recording calls were added; no separate pre-fix export RED is claimed.
- GREEN: helper test `1/1` in both lib and bin targets; real export test `1/1` in the bin target. The first `record_only` filter selected only the helper, so the real export test was also run explicitly by name.
- Strict `cargo clippy -p umber-app --all-targets -- -D warnings` → exit 0, zero warnings.
- Full `cargo test -p umber-app` after the R1/R2 fixes with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **1,266 passed, 0 failed across 11 suite rows**: 323 lib + 896 bin + 2 filter-apply + 1 Gate0 + 3 keyboard-menu + 17 panel-kittest + 11 snapshots + 10 panic-hunt + 2 pool-perf + 1 set-switch + 0 doc-tests.
- `~/.cargo/bin/rustfmt --edition 2021 --check` on all touched Rust → exit 0 after formatting the new assertions. The first ad-hoc attempt stopped on those formatting diffs and was not counted as a pass.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-history-*.sh` post-review probe reran `cargo test -p umber-app history -- --nocapture`: 35/35 lib + 52/52 bin + 1/1 history kittest + 2/2 history snapshots, including `completed_job_history_survives_a_restart_before_the_shell_drains`. `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the app suite, not a workspace-green claim.

## Judgment calls

- Recording lives on actual completion, not button clicks. Bake uses the existing once-only completion edge; export records inside the synchronous success arm. No worker or counter was invented.
- `completed_exports` alone is not a history signal because failures increment it. Early refusals and failures produce no row.
- Moving `ExportContext` by value is an ownership change only. The export driver and all user-visible status/output behavior remain the same.
- No new context kind, persistence, or i18n string was added.

## Unverified

- No dedicated interactive History-panel smoke was run. The real export driver, worker completion logic, history model, and full app/integration suites carry the evidence. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: all-channel material picking with a complete gesture/apply e2e kittest. Then the Assets mesh category, OccluderScope threading kittest, and XI2 hotplug re-query.
- Async export and worker cancellation remain separate designs.
