# Kill report — export > 30 s ui_timeout over --control (row 246)

- Date: 2026-11-06 ~16:3x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-job-timeout` (control.rs, ROADMAP.md, STATE.md, queue.md,
  COORDINATION-next-wave.md, this report).
- Debt closed: ROADMAP standing debt added 2026-11-06 with charter-v2
  T1-T3 (`5af3c84`): "an export longer than 30 s times out over
  `--control` although it finishes". `export.run` runs synchronously on
  the UI thread (`run_export` -> `export_current`), so
  `forward_ui`'s 30 s `UI_REPLY_TIMEOUT` fired while the export was still
  running; the client got `ui_timeout after 30s` and the job then
  completed anyway (visible via `job.status`).

## The fix (crates/umber-app/src/control.rs only)

- New `JOB_COMMANDS = ["bake.run", "export.run"]` and
  `JOB_REPLY_TIMEOUT = 30 min`, plus `reply_timeout_for(req)`:
  `command.run` of a job id waits the job ceiling; everything else keeps
  `UI_REPLY_TIMEOUT` (30 s).
- `forward_ui` is now a 30 s wrapper over `forward_ui_with(timeout)`;
  the `ui_timeout` wire error names the ceiling actually used.
- The `command.run` dispatch arm passes `reply_timeout_for(&req)`.
- Pin: `control::tests::job_commands_get_the_job_reply_timeout` — job
  ids map to 30 min; `view.toggle_grid`, bare `command.run`,
  `job.status`, `ui.inspect`, and a job id on a non-command.run method
  all map to 30 s.

## Verification (all live, this session, under the repo flock, nice 19)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p umber-app --all-targets -- -D warnings`: 0 errors
  (one fix-forward: the doc comment's `> 30 s` tripped
  `doc_lazy_continuation`; reworded).
- `cargo test -p umber-app --all-targets`: lib 253/0 (incl. the new
  pin), bin 753/0, integration suites (filter_apply 2, gate0 1,
  keyboard_menu 3, panel_kittest 12, panel_snapshot 8, +3 more suites)
  all 0 failed.
- Release CI run 42276402204 on tip `da3ed59`: GREEN (layering gate,
  change classifier, fmt+clippy+test on ubuntu-24.04 and windows).

## Same-session housekeeping

- M1 stale-row sweep: hunt-queue row `b09ae253` (perf-gate instrument
  wave) completed with artifacts — the wave landed as `0981d2f` (P1
  same-job A/B ratio gate) and closed in docs `6ef5b3d`;
  `docs/perf/protocol.md:561` records the landing.
- `scripts/kill-locks.sh` ENOSPC fail-loudly fix (row 231 incident)
  verified already landed in `87d0285` — no change needed.

## Not done / next

- A truly multi-hour export still outlasts 30 min; the contract for
  that remains "`command.run` starts the job, poll `job.status`" (STATE.md
  T1-T3 section). Making `export.run` itself async like `bake.run` is a
  bigger change (export needs `&mut` paint for `flush_for_export`); not
  taken here.
- Next queue: installers gaps from row 240, History-panel split-redo
  wiring (row 242), desktop checks 1/3/4 (Bessa's desktop).
