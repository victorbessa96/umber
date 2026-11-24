# Design brief: Bake/Export history record_context (2026-11-24)

Planned kill lock: `razul-history-context`.
Base: PerChannel K1 status tip `dbbd14da0bed9d51e6134ef109633b0c42fdfa17`.
Requirement: `docs/specs/requirements.md:155`, the Bakes/Export `record_context` follow-up.

## What exists

`HistoryKind::Bake` and `HistoryKind::Export` are context-only finished-job rows with all journal depths `None`. Production currently calls `record_session_event` only for FilterApply and QuickMask. Bake/Export appear in tests and document helpers, not in the real job flow.

`BakesPanel::poll_job` drains a worker exactly once. The shell's `prev_baking` flag then identifies a just-completed job and `last_job_failed()` separates success from failure. This is the correct once-only completion edge; no new worker or counter is needed.

`ExportDialog::export_now` is synchronous and receives an `ExportContext` containing `Option<&Document>`. Its `run` returns a real outcome, with success and error arms. `completed_exports` increments on both outcomes, while `last_export_failed` separates them. `export_current` can also refuse before any run, so history must be recorded inside `export_now`, not inferred from its counter or from a button click.

## Target

- Add a tiny `record_job_result(doc, kind, failed)` helper in `history_panel`: when `failed` is false it records one context row through `record_session_event`; when true it does nothing. Unit-test both outcomes and the display-only depth shape.
- In the shell's existing `prev_baking` completion branch, call the helper with `HistoryKind::Bake` and `last_job_failed()`. Successful jobs get exactly one row; failed jobs get none; repeated idle frames add nothing.
- Pass a mutable document through `ExportContext` and move the context by value through `export_current`, `run_requested_export`, and `export_now`, so `export_now` can record on the real result. `run` keeps borrowing it. Update the two production constructors and current test call sites without changing `show`'s existing by-value shape.
- In `export_now`, record `HistoryKind::Export` only in the success arm. The error arm records nothing, and early `export_current` refusals still produce no row.
- Preserve the export driver, filenames, statuses, `completed_exports`, `last_export_failed`, UI behavior, and all other history semantics. No schema or i18n changes.

Add acceptance coverage for a real successful export, a failed driver run, and an early refusal. Reuse the existing small full-UV quad and GPU test convention at 8x8 output, so the success assertion runs the actual export driver and writes real files rather than fabricating an outcome.

## Concurrency and safety

Locked surface: `crates/umber-app/src/history_panel.rs`, `export_dialog.rs`, `commands.rs`, `main.rs`, this brief/report, `docs/specs/requirements.md`, `CHANGELOG.md`, `docs/loop/queue.md`, `STATE.md`, `docs/ROADMAP.md`, `docs/loop/next-steps-2026-11-23.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, and `.dragon/COORDINATION-next-wave.md`.

No worker/driver rewrite, new dependency, schema, persistence, release, public-PR, or second-kill scope. Any foreign dirty file, live sibling lock, origin movement, disk I/O error, or unexpected behavior delta stops the lane for re-census.

## Verification

- RED first: acceptance tests must show a successful bake/export currently has no context row. A no-op helper stub and the by-value context migration may land only to make the tests compile; do not add the actual recording calls until the RED run is captured.
- Assert success records exactly one display-only row, failed runs and early refusals record zero, and a second successful run adds exactly one more.
- Assert the shell's success/failure helper semantics and all depths `None`.
- Run `~/.cargo/bin/rustfmt --edition 2021 --check` on touched Rust, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Run strict `cargo clippy -p umber-app --all-targets -- -D warnings` and full `cargo test -p umber-app` separately under `/tmp/umber-cargo.lock` with the slow-build environment and `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`. Record every suite's passed/failed count; 0-test success is not a pass.
- Obtain an independent read-only review of once-only completion, error/refusal behavior, context ownership/lifetimes, and the real success test's honesty.
- Record the kill and update the current remaining-work order, commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.

## Non-goals

No async-export rewrite, worker cancellation, job-status changes, new context kinds, persistence, i18n strings, UI restructuring, release action, or second kill.
