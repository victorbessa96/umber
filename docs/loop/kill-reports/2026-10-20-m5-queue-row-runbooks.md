# Kill report — M5 queue-row runbooks

Date: 2026-10-20. Author: dragon (local-10e6ed18). Lock:
`m5-queue-row-runbooks` (docs/loop/queue.md, STATE.md,
docs/loop/desktop-checks.md, this report,
.dragon/runbooks/host-back-recovery.md). Doc-only, battery-exempt
under the freeze-4 envelope; the census IS the verification.

## Built

1. `.dragon/runbooks/host-back-recovery.md` (untracked by convention)
   — the HOST_BACK recovery runbook: the stand-down check (host
   reflog / pgrep cargo / the coordination STATE block), watcher
   verify + the NOTIFY re-arm rule, the recurrence-class journal
   check (`journalctl -b -1` grep `Hardware Error|MCE`), the
   no-gpu-inside-the-window rule, the parked fleet's per-kill battery
   scopes (A1 umber-app, P1 umber-paint + umber-brush docs-only, G1
   umber-gui, S1 stylus default-features, graph-history umber-app,
   C1 snapshot-dir umber-cli droppable, the eyedropper NOT fleet
   cargo), per-kill-then-combined gates, the ONE push + explicit
   run-id ci-watch, the packaging re-dispatch AFTER the push, the D3
   docs-pass-rides-the-NEXT-push rule, the memtest debt. Sources: the
   policy prose extracted from STATE.md's Next + the ledger's Waiting
   rows + the queue Scratch; the load-bearing precedents cited
   (stencil-wave-close.sh's two fixed defects — the line-start
   claim-grep + the tree-state sentinel; the build-host-outage
   protocol; the M2 job-list rule; the 16-hex pin convention).
2. `docs/loop/desktop-checks.md` — the W-C3a plan's D1
   `desktop-check-pack` landed: four instrumented manual checks
   (quit guard, kill -9 recovery, set-switch on the two-material
   fixture, Windows real-pen flags post-fleet), each with steps,
   expected observation, a Result line, and the label citations
   grep-verified against en-US.ftl (:51, :55, :116-120, :811-813);
   the fixture verified live
   (crates/umber-mesh/tests/assets/cube_2mat_ascii7400.fbx); the
   quit-guard window verified in code (id "quit-guard" reusing the
   shell.new-confirm-* keys on the CancelClose arm).
3. Row conversions (the lane's charter): STATE.md Next → the
   one-line order + the runbook pointer; the ledger's Waiting rows
   1-2 → short + runbook pointers (worktree-side per the
   single-owner rule); queue rows C2-1/C2-3/C2-9 → desktop-checks.md
   pointers; C2-12 → the fleet-table pointer with the deferral kept
   TRUE on the committed tree; the Scratch tail-count line (13
   commits through M1 + this pass).

## Judgment calls

- D1 lands INSIDE M5: the plan's own lane-D kill was the runbook
  the C2 rows needed to point at; claiming it separately would have
  left the lane pointing at nothing. The plan draft itself is
  untouched (the untracked planner artifact; the D3 wave-close pass
  ticks its D1 row from this report).
- Row 15 ticked against M1's held-current verdict: M1 verified the
  marker by grepping the `--script` literal; the landing was a
  PROTOCOL step (judgment 1 of the 2026-10-14 harness-script report:
  a JSONL script piped into `umber-cli agent`), so the literal was
  the wrong probe. Evidence used: the report + `paint_snapshot` in
  the COMMITTED tree (`git show HEAD:crates/umber-cli/src/agent.rs`,
  8 hits, dispatch at :1006) — worktree bytes never cited.
- The ledger's In-flight Next cells left as-is: they are terse
  actions, not policy blobs; the scopes they name live once, in the
  runbook's fleet table, and the Waiting row 2 points there.
- The W-C3a executable close script (the stencil-wave-close.sh
  pattern applied to this fleet) stays a NAMED FOLLOW-UP (the M3
  "recovery-runbook adoption" tail); M5 lands the doc runbook only.

## Unverified

- Nothing here claims runtime behavior. The desktop checks are
  INSTRUMENTS: their Result lines stay empty until a desktop session
  runs them; the queue rows close only then.
- The recovery runbook's steps are re-derivations from the recorded
  precedents, not an executed run — freeze-4 is still dark; the
  runbook's first live execution happens at HOST_BACK.

## Deferred

- The executable W-C3a close script (see Judgment calls).
- The headless C2-9 variant (needs the set-switch glue extracted to
  a lib fn — the plan's §5 tail item 4).
- Check 4 (Windows real-pen flags) is gated on the parked fleet's S1
  commit.

## Gates

- Doc-only, battery-exempt (freeze-4 envelope: NO cargo, NO
  batteries, NO pushes). The census IS the verification: every label
  cited at its grep line, the fixture ls-verified, the
  committed-tree paint_snapshot count from `git show HEAD`. Rides
  the ONE recovery push.
