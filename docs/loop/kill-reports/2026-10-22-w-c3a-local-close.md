# Kill report — 2026-10-22 w-c3a-local-close (the local-degraded port)

Authorship: dragon (Razul, session local-7de7d3ed). Kill-lock:
`w-c3a-local-port` (the port script + this report ride the close's row
11). Design source: `scripts/w-c3a-close.sh` (the host close — the port
is that script verbatim except the local seams).

## Built

- `scripts/w-c3a-local-close.sh` — the LOCAL-degraded port of the
  W-C3a close. Header carries the commission verbatim (Bessa
  2026-10-22 ~13:1x: "Continue working autonomously cargo lifted, but
  do it slow to not overload the machine") + the host facts (unreachable
  at the original script's own 13:17 dry-run probe; fragile boot;
  memtest86+ owed; checking stood down). The SLOW THROTTLE ENVELOPE:
  `CARGO_BUILD_JOBS=3`, `UMBER_TEST_THREADS=2` (GPU suites stay at
  wave-battery's `--test-threads=1`), `UMBER_SUITE_TIMEOUT=2400`,
  `nice -n 19`, one cargo at a time under `/tmp/umber-cargo.lock`,
  nothing parallel — the 8-thread tower at desktop load keeps headroom.
  Local seams replace steps 0/2/3/4 only (mode banner; local
  cargo-census pre-flight; local tree-state sentinel; local throttled
  battery with a 3 h verdict poll); everything else is the original
  verbatim: the claim interlock anchored at line start, the
  nine-manifest FULL verification with the truncated-prefix parser, the
  push gate check, the A2 baseline skip logic, the tree-state-gated
  verdict reuse, the ten original commit rows with their exact messages
  through `dcommit --path-limited`, ONE push + explicit run-id ci-watch,
  the packaging re-dispatch. Row 11 lands the port + this report.

## Judgment calls

- LOCAL over host: the host is unreachable + fragile + memtest-owed +
  checking stood down; the local path is the 10-21 sanctioned mode, and
  the GPU-bearing panel_snapshot suite is PROVEN green on this tower
  (8/8 PASS, 2026-10-21 00:23) — the suites are backend-portable by
  design (CI runs them on software adapters green).
- The port over manual steps: the 10-21 sessions ran these steps by
  hand and died un-landed twice; an idempotent, re-runnable script
  makes a session death cost a re-run, not a re-derivation — the same
  ruling that produced the stencil-wave-close script.
- Throttle over speed: -j3 on an 8-thread machine already at load ~5
  with the desktop live; the battery takes what it takes (the 3 h poll
  cap replaces the original's 90 min).
- The dead 10-30 claim is adopted through an evidence-carrying
  RELINQUISHED line (holder exit: its battery died with the session
  10-21 00:49, zero commits landed, the locks registry was emptied
  10-22 01:47 with no owner, ps census clean at 13:17) — the
  interlock's grammar requires relinquish-or-clear, and the line's own
  text carries the adoption record so history stays honest.

## Unverified

- The battery verdict itself — that is the run's product; GREEN/RED
  lands in `/tmp/w-c3a-local-battery-r3.log` and the push record.
- The GPU-bearing suites other than panel_snapshot run on this
  tower's Intel device for the first time in this close; a red there
  is a REAL red (fix-forward per the adoption note), not an
  environment artifact — the suites are skip-graceful only on
  device-request failure and the device exists.

## Deferred

- The host close (`w-c3a-close.sh`) stays canonical for when the host
  is verified again; the port does not replace it, it complements it
  for the host-down lane.

## Process note

- No cargo ran in the authoring of this port: `bash -n` + the `--dry-run`
  precondition pass are the static gates; the battery is the port's own
  first exercise, and its verdict is the run's to prove.
