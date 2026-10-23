# Kill report — the W-C3b close script (w-c3b-close-script)

Date: 2026-10-20. Session: local-4d2e8a71 (dragon). Doc/tooling
lane, battery-exempt; rides the W-C3b docs row at the recovery.

## What landed

`scripts/w-c3b-close.sh` — the W-C3b (free-file boundary sweep)
recovery runbook, executable. Built on the w-c3a-close.sh pattern
with both recorded defect classes pre-fixed: the claim-grep anchors
at line start (`tac | grep -m1`), and the battery-verdict reuse gates
on a tree-state sentinel (HEAD + porcelain + the dirty files' BYTES)
so a finished verdict resumes while a fix-forward forces a fresh
battery.

## The design deltas from w-c3a (each named, none improvised)

1. **The W-C3A gate (step 0c) — the new element.** Two waves parked on
   one tree close SERIALLY: this script refuses to run while the
   W-C3A recovery is unclaimed (no sentinel line), CLAIMED, or
   RELINQUISHED — only its CLOSED line opens this one. In dry-run
   mode the gate warns and continues so a pre-recovery dry-run still
   verifies this wave's own preconditions (the w-c3a dry-run parity);
   a REAL run dies at the gate.
2. **Its own sentinel + battery log.** `.dragon/battery-sentinel-c3b`
   and `/tmp/w-c3b-recovery-battery.log` — a W-C3a re-run or verdict
   can never be mistaken for this wave's.
3. **Battery scope: `umber-mesh umber-core umber-gpu umber-app`.**
   The sweep's crates — three of which sit OUTSIDE the W-C3a
   battery's scope entirely (the fleet never touched mesh/core/gpu);
   umber-app is shared, and this wave's panel bytes ride BOTH
   batteries' compiles. Default features only; never `--features gpu`
   inside the recovery window (the 94469e8 deadlock class).
4. **No baselines step, no packaging dispatch.** A2's PNG generation
   was the fleet's; the flatpak verdict-gate dispatch is w-c3a's
   close's. This wave's push trigger runs the AppImage+lint legs on
   its own tip.
5. **Cross-wave dirt named up front.** The w-c3a battery rsyncs the
   whole tree, so this sweep's seven dirty files ride its umber-app
   compile — the header states that a red inside them during the
   W-C3a recovery routes to THIS wave's owner, and the ownership map
   in step 1 prints them with the routing rule.

## Verification (host-free, per the w-c3a standard)

- `bash -n` clean (after one live fix: the first draft's `elif case`
  shape was invalid bash — caught by `bash -n`, restructured to
  if/else + case).
- Dry-run ×2 byte-identical, exit 0, all rows printed.
- All THREE manifests fully hash-verified by the script's own
  truncated-prefix parser: 2 + 4 + 4 rows, ZERO drift.
- Lock census correct (3 live locks; each released at its commit row).
- The ownership map prints exactly the sweep's seven parked files.
- The claim interlock verified against the note's real bytes (the
  W-C3B sentinel line is absent pre-claim, as expected; a mid-line
  prose mention of "W-C3B" in the STATE block does NOT match — the
  anchored grep held).

## Unverified

- The real-run legs (SSH, rsync, battery, push) execute only at
  HOST_BACK; the dry-run covers every host-free precondition.
- The 60-minute battery cap (120×30s) is an estimate for four crates;
  the fleet's six-crate battery bounded at 90 min — if the poll
  exhausts, the script says WAIT, and a re-run reuses the landed
  verdict.

## Judgment calls

- The W-C3A gate reads the note's sentinel lines rather than git
  state: the coordination note is the recovery's single source of
  truth, and a CLOSED line is written only by w-c3a-close.sh itself
  after its push — the gate cannot be satisfied by accident.
- The docs row commits `kill-ledger.md` with whatever W-C3b rows
  exist at close time; the sweep's ledger rows are written by the
  post-CI docs pass (the ledger is the W-C3a docs row's dirty surface
  until its close lands — the single-owner rule).
