# Kill report — `export.failed` in job.status (row 232's tail, queue row 250)

- Date: 2026-11-06 ~21:0x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-export-failed-flag` (export_dialog.rs, commands.rs, STATE.md, this
  report).
- What was open: row 232 (87d0285) landed the bake side of
  `job.status`'s `failed` (`BakesPanel::last_job_failed`) and made a failed
  export clear `last_written`/`last_skipped`, but noted: "Wiring
  last_job_failed into job.status needs commands.rs … WAITS" — the export
  object had no `failed` field, so a control client could not tell a failed
  export from a good one. commands.rs is no longer sibling-locked, so the
  tail is closed now.

## The change

- `ExportDialog` gains `last_export_failed: bool` (default false); the Ok
  arm of `export_now` clears it, the Err arm sets it (next to the
  `last_written.clear()` from 87d0285). New accessor
  `pub fn last_export_failed()` mirroring `BakesPanel::last_job_failed`.
- `commands.rs::job_status_json`: the `export` object gains
  `"failed": export.last_export_failed()`. Signature unchanged, so main.rs
  (sibling-locked) needed no edit.
- Pins: `a_failed_export_reports_no_files_not_the_previous_runs` now also
  asserts the flag is set on the Err arm; new `the_failed_flag_starts_clear`.

## Verification

- `cargo fmt --all -- --check`: clean.
- Battery (flock, nice 19, -j3, log /tmp/umber-exportflag-battery.log):
  clippy -p umber-app --all-targets -D warnings; cargo test -p umber-app
  --all-targets — both pins green, 0 failed across all suites.

## Records note

Queue row 250 + COORDINATION STATE line DEFERRED: docs/loop/queue.md and
.dragon/COORDINATION-next-wave.md remain held by sibling lock
`razul-bundled-assets` (created 18:17, no live umber process at claim,
its bundled-assets work still dirty/uncommitted — left alone).

## Not done / next

- Installers gap #2 (exe-relative runtime assets) — sibling lane.
- Row 242 History-panel split-redo wiring — needs document.rs/main.rs,
  sibling-locked. ROADMAP 175 merge-below-top (umber-paint, free).
- v0.1.0 tag stays partner-gated.
