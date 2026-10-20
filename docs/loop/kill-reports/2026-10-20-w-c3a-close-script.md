# Kill report — w-c3a-close-script (the M3 recovery-runbook-adoption tail)

Date: 2026-10-20. Author: dragon (local-10e6ed18). Lock:
`w-c3a-close-script` (scripts/w-c3a-close.sh, this report, the six
`.dragon/<kill>-manifest.txt` pins). Doc/tooling only, battery-exempt
under the freeze-4 envelope; no cargo anywhere.

## Built

1. `scripts/w-c3a-close.sh` — the W-C3a fleet's one-command recovery,
   the stencil-wave-close.sh pattern with BOTH recorded defects
   pre-fixed at design time:
   - the claim interlock greps `^W-C3A RECOVERY
     (CLAIMED|RELINQUISHED|CLOSED)` via `tac | grep -m1`, anchored
     at line start — the note's own prose quotes sentinel-shaped
     strings mid-line and must never match;
   - the battery-verdict reuse gates on the tree-state sentinel
     (HEAD + porcelain + the dirty files' BYTES over the rsync
     surface), so a poll that died mid-flight resumes the finished
     verdict while a fix-forward forces a fresh battery.
2. Six 16-hex manifests pinned from each kill report's file list
   (`.dragon/{channel-export-resample,idmask-tile-cache,
   picker-sliders-recents,winink-pen-flags,snapshot-dir,
   graph-history}-manifest.txt` — 29 pins): the drift guard between
   now and HOST_BACK; full verification at run time by the script's
   own truncated-prefix parser (`sha256sum -c` rejects the
   convention by design).
3. The commit rows: six fleet kills, each by its manifest + each
   releasing its lock at its commit, EACH THROUGH dcommit
   `--path-limited` — the M3 gate dogfooded at wave scale, every
   fleet commit ledger-attributed to the running session; then the
   docs/tooling row (the plan draft, the kill-ledger's accumulated
   worktree rows per the single-owner rule, the script, this
   report).
4. The eyedropper routed around BY CONSTRUCTION: viewport.rs +
   main.rs are in no manifest and no row; the path-limited pathspec
   cannot pick them up (proven on the scratch repo: the foreign-dirt
   file stayed out of the commit); the caveat prints at claim and
   at battery launch.

## Verification (host-free; the census IS the verdict class)

- `bash -n` clean; `--dry-run` ×2 byte-identical; the dry-run
  verifies all six manifests (29 rows, 0 drift), the push gate
  (approved, standing), the fleet lock census (absent from /tmp —
  the registry was cleaned since the fleet parked; the recovery
  claim governs), the eyedropper caveat, and the unpushed-tail note
  (local 6d05bf2 vs origin aac1c4a).
- The claim-grep unit cases 4/4 against the note's own bytes: real
  bytes → no match; an appended line-start claim → match; a
  MID-LINE mention does not steal the match; a later RELINQUISHED
  clears it.
- The dcommit `--path-limited` invocation shape proven on a scratch
  repo: committed EXACTLY the lock paths (2/2), the foreign-dirt
  file stayed dirty and uncommitted, the ledger row carries
  mode=path-limited + the session id.

## Judgment calls

- The battery scope is the plan's §3 shape (umber-app umber-paint
  umber-brush umber-gui umber-cli stylus) with snapshot-dir IN:
  the plan called C1 droppable if the window runs short, but the
  kill is coded and reported — parking a finished kill through
  another freeze to save one battery crate is the worse trade. Each
  row commits independently, so a mid-run stop still lands the
  earlier rows.
- perf-nightly is NOT dispatched post-push: no W-C3a kill owes it
  (the stencil case owed it for e0063's compile gate); the hardened
  nightly runs on schedule and the drift gate is live on every
  pushed tip. A manual dispatch remains available.
- The fleet locks absent from /tmp/umber-kill-locks print as a
  census NOTE, not a refusal — the claim + the stand-down checks
  (host reflog, live cargo, the note's claim line) are the guards
  that matter at recovery time.
- The docs pass stays manual post-CI-green (the one-push rule: docs
  ticks ride the NEXT wave's push) — printed by the script at
  close.

## Unverified

- The real run: everything past step 3 (rsync, battery, rows, push)
  executes only at HOST_BACK — the freeze-4 envelope forbids
  batteries and pushes now. The first live execution IS the
  recovery; the script refuses to guess past any failure and every
  refusal path relinquishes the claim.
- The fleet's tests remain UNRUN (parked through the freeze) — the
  battery at recovery is their first execution; the kill reports
  name their static verification classes honestly.

## Deferred

- The dcommit pre-commit-hook sample (the other M3 tail) — still
  unclaimed.
- The eyedropper's own close (its owner's lane; the adoption rules
  if its holder stays dead: holder-exit confirmed, fresh lock, full
  gate sequence, expect wrong pins in both directions).
- M1 pass 2 (the standing stale-row lane).

## Gates

- Doc/tooling only, battery-exempt, no cargo; rides the recovery
  push. The verification class is the stencil precedent's: dry-run
  ×2 byte-identical + the interlock unit cases + the scratch-repo
  invocation proof.
