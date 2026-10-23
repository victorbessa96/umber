# Kill report — 2026-10-23 w-c3b-local-close (the local-degraded port)

Authorship: dragon (Razul, wake session 2026-10-23). Kill-lock:
`w-c3b-local-port` (the port script + this report, committed ahead of
the battery). Design source: `scripts/w-c3b-close.sh` (the host
original — the port is that script's local-seams variant), itself
built on the `w-c3a-local-close.sh` pattern (96edd1e, proven live
through nine fires).

## Built

- `scripts/w-c3b-local-close.sh` — the LOCAL-degraded port of the
  W-C3b close. Local seams replace the host steps only (mode banner;
  local cargo census + the one-heavy-job voicesweep gate; local
  tree-state sentinel; local throttled battery with the 3 h verdict
  poll). Kept from the host original: the W-C3B claim interlock
  anchored at line start, the W-C3A-must-be-CLOSED gate, the four
  16-hex manifests FULL verification with the truncated-prefix parser,
  the push gate check, the tree-state-gated verdict reuse, the four
  sweep commit rows with their exact messages through
  `dcommit --path-limited`, ONE push + explicit run-id ci-watch.
- Two hardenings over BOTH predecessors, each born from a 10-23 red:
  1. **The post-rows scorecard gate** (step 5b): regenerate
     `scripts/scorecard.py`, require `--check` green, commit README.md
     if the span moved — the W-C3a close pushed a stale span and red
     the wave tip's layering gate (the fleet crossed the 2,200 floor;
     root-caused this session from run 39908194347's job log + a
     clean-tip worktree probe).
  2. **The ci run-id resolve now filters by workflow** (`--workflow
     ci`) and asserts the run's job list names the check matrix before
     arming ci-watch — two workflows fire on every push and the
     original's headSha-only filter could hand the watcher the
     packaging run (the stencil-wave-close defect class, pre-fixed
     here instead of post-fixed).
- One reasoned deviation from the host original: **the post-push
  packaging re-dispatch is kept, re-pointed** — the original deferred
  the flatpak verdict gate to w-c3a's close, whose dispatch
  (39908210580) is now CONSUMED-RED at the runner's bwrap/rofiles
  layer (the ubuntu-26.04 rotation; the job is pinned to ubuntu-24.04
  by the flatpak-runner-pin kill, riding this push). The verdict gate
  moves to this push's re-dispatch, on the pinned runner.

## Judgment calls

- LOCAL over host: identical ruling to the W-C3a port — the host is
  unreachable, fragile, memtest-owed, checking STOOD DOWN; the local
  path is Bessa's sanctioned slow-envelope mode, proven through the
  whole W-C3a close.
- The port + this report commit BEFORE the battery (battery-exempt
  tooling, rides this push), unlike W-C3a's row-11 shape — so a
  session death between authoring and close costs nothing: the close
  script's docs row lists only the host original + its report + the
  brief + the ledger, and re-running the close after any death
  resumes from the claim.
- The one-heavy-job gate in step 2 refuses while the
  kitten-voicesweep unit is active: the 10-22/23 reboots came from
  heavy-lane stacking, and the sweep is Bessa's own evidence bake —
  the battery waits for it, never contends with it.
- Launch shape hardened to `systemd-run --user --unit=w-c3b-local-close
  --collect -p MemoryHigh=8G -p MemorySwapMax=2G
  --setenv=DRAGON_SESSION=<id>`: own cgroup (oomd lands on the
  recoverable unit — fire 8's proven shape), survives the session
  (the 19:09 reboot + gateway-crash lesson), and attributes the
  dcommit rows (fail-closed by design).

## Unverified

- The battery verdict itself — that is the run's product; GREEN/RED
  lands in `/tmp/w-c3b-local-battery-r1.log` and the push record.
- The GPU-bearing suites in this battery run on the tower's Intel
  device; a red there is a REAL red (fix-forward per the sweep
  reports), not an environment artifact — same stance as the W-C3a
  port.
- The pinned-image CI verdict (c697bab's gate) and the pinned-runner
  flatpak verdict (flatpak-runner-pin's gate) both land on THIS push's
  runs; until then both pins are evidence-based, not run-verified.

## Deferred

- The host close (`scripts/w-c3b-close.sh`) stays canonical for when
  the host is verified again; the port complements it for the
  host-down lane. The host script is committed by the close's docs
  row as historical tooling either way.
- The 26.04 bwrap/rofiles mechanism stays undiagnosed (see the
  flatpak-runner-pin report); the pin is the fix, the mechanism is
  the named follow-up.

## Process note

- No cargo ran in the authoring of this port: `bash -n` + the
  `--dry-run` precondition pass are the static gates; the battery is
  the port's own first exercise, and its verdict is the run's to
  prove. The docs-pass edits that ride this push (queue/STATE/
  CHANGELOG/ledger/README + the flatpak pin) were all verified
  on-disk before this report was written.
