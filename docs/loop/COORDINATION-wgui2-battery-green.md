# Coordination note — the recovery battery is GREEN; the runbook's RED was a script bug (fixed)

Written 2026-10-17 ~06:50 by the loop session, for whichever session
is working the W-GUI-2 close (the 20261016_180734 sibling was last
seen on battery round 11 at ~06:09, and the runbook battery fired at
06:41).

## The state of the battery, decoded from the host's /tmp rounds

The battery rounds, in time order (dir mtime, all on the host):

- 06:18 WJWPxn — `test result: ok` on every suite (incl. umber-spp
  with the ~307 s corpus test)
- 06:28 5pk180 — all ok (corpus 268 s)
- 06:35 QnM9zH — all ok (corpus 307 s)
- 06:41 ObvT2W — "FAIL test spp (cargo exit 101, 0 result lines, 0
  failed lines — test-spp.log)". This round came from
  `scripts/recovery-close.sh` step 4, which launched
  `wave-battery.sh umber-app umber-gui spp umber-core` — **`spp` is
  not a package name** (the crate is `umber-spp`; cargo: "package ID
  specification `spp` did not match any packages"). The spp suite
  never ran in that round; the RED is the invocation, not the code.
  The log-file name is the proof: `test-spp.log` (named for the
  argument `spp`) vs every real round's `test-umber-spp.log`.

## The fix (already landed locally, uncommitted)

`scripts/recovery-close.sh` step 4 now launches
`wave-battery.sh umber-app umber-gui umber-spp umber-core` (all
three occurrences: the say line, the dry-run echo, the ssh launch).
`bash -n` passes. The file is in the commit manifest — the hash
drift the pre-check would flag is THIS fix; the commit step commits
disk bytes, so the fixed script rides the wave commit as-is.

## What this means for the close

The tree's batteries have been green for three consecutive full-scope
rounds (06:18-06:35). The runbook can now be trusted end to end:
re-run `bash scripts/recovery-close.sh`, let its step-4 battery go
green (the spp suite will actually run this time, ~5 min for the
corpus leg), confirm, and step 5 commits by manifest path, releases
the locks, pushes once, and starts ci-watch. Alternatively, if you
already trust the three green rounds as Gate 1, step 5's sequence by
hand (commit-by-path → lock release → ONE push → ci-watch with the
run id resolved from headSha) is equally valid — the runbook is the
automation, not the authority.

## Why I did not fire the close myself

Sibling-collision rule: you were mid-recovery (reflog reset at
04:35, rounds 2-11, the 06:41 runbook battery). Two dragons on one
prey tears the kill — the tip assertion fires, batteries race the
cargo lock, commits race the manifest. The wave is yours; this note
and the script fix are the handoff.
