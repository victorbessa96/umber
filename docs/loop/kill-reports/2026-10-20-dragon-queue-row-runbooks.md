# M5b — dragon-queue-row runbooks (the steer's original blob incident)

Date: 2026-10-20 (wake-13472, session local-8bd85bc7). Doc-only,
battery-exempt. Lock: m5b-dragon-queue-runbooks.

## Context — the lane fork

The steer's M5 (2026-10-19 23:52) named the dragon hunt-queue rows as
the incident ("hunt next_step rows point at runbook files ... the
600-char blob is the incident"). At 10:55 the main session
(local-10e6ed18) claimed M5 and landed it 11:1x (ea83093) over the
umber-INTERNAL surfaces: STATE.md Next, queue rows C2-1/C2-3/C2-9,
C2-12, row 15, the Scratch count, the kill-ledger's Waiting cells,
plus .dragon/runbooks/host-back-recovery.md and
docs/loop/desktop-checks.md. The dragon-side rows — the rows the
steer's sentence was literally about — stayed blob-carriers. This
lane (claimed 11:2x; the same claim attempt was lock-refused at 10:59
against the then-live M5 lock, zero edits leaked) is the complement.

## The incident, measured (queue store, 11:3x)

- d0c76c4d: goal 832 chars + next_step 1143 chars at session start
  (the M1 landing text); every wake checkpoint re-embeds state into
  the same fields — this session's own 11:2x claim checkpoint was
  566 chars.
- 1971c1fd: goal 588 + next_step 528 (the 37th-check text; the 38th
  written this session).

## The kill

Two policy homes in .dragon/runbooks/ (untracked by convention,
siblings of host-back-recovery.md):

- hunt-umber-local-continuation.md — d0c76c4d's home: read order,
  the HARD-RULES envelope, the meta-lane menu (live status stays in
  the STATE block), the claim/land protocol, the partner-gated
  list, the recovery pointer, named follow-ups.
- hunt-centralhost-freeze-watch.md — 1971c1fd's home: the probe,
  the watcher composition rule (incl. the census method: the log's
  start lines are ground truth; a bare pgrep -f self-matches the
  querying shell — proven live twice on 10-20), recovery pointers
  (→ host-back-recovery.md, not duplicated), the closing gate
  (Bessa's memtest confirmation only), the recorded freeze state.

Both queue rows then shrink to pointer + current state:
- goal = what the hunt is + the runbook path (mandatory read).
- next_step = the last landing + the next-lane hint (state only).

## Census (what moved where)

| Policy element | Old home | New home |
|---|---|---|
| FREEZE-4 envelope (no cargo/batteries/pushes, fleet parked) | d0c76c4d goal | local-continuation §HARD RULES |
| One-lane claim-then-build protocol | d0c76c4d goal | same §HARD RULES |
| Verify-open-first / stale-row rule | d0c76c4d goal | same §HARD RULES |
| Host-free work classes | d0c76c4d goal | same §HARD RULES |
| No-second-watchers | d0c76c4d goal | same §HARD RULES + freeze-watch §watcher rules |
| Meta-lane menu (M1-M5) | d0c76c4d goal + STATE | same §Lane menu (live status → STATE block) |
| Partner-gated list | d0c76c4d goal | same §Partner-gated |
| Freeze history + signature | 1971c1fd goal | freeze-watch header |
| Probe shape | 1971c1fd next_step (implicit) | freeze-watch §probe |
| Watcher composition rule | 1971c1fd next_step | freeze-watch §watcher rules |
| Recovery order | 1971c1fd goal | host-back-recovery.md (pointer, not duplicated) |
| Closing gate | 1971c1fd next_step | freeze-watch §closing gate |
| Recorded freeze state (LAI commits, gpu-bake risk) | 1971c1fd goal | freeze-watch §recorded state |

New policy the runbooks consolidate (previously implicit in landing
notes only): the dcommit commit gate as the standard commit path,
the kill-locks claim step, the kill-ledger single-owner rule, the M2
job-list citation rule, the stale-row rule's committed-tree clause.

## Standing rule born here

A new recurring hunt on umber territory gets a runbook in
.dragon/runbooks/ at creation; its queue rows carry pointer + state,
never policy prose. Encoded in both runbooks (the local-continuation
§Lane menu M5 entry + the freeze-watch §probe "state only" line).

## Verification

- Runbooks on disk at the locked paths (write-verified: 3811 +
  4204 bytes).
- Commit via dcommit (staged set = this report; locks = the three
  files; the runbooks stay untracked by convention — the same shape
  as ea83093's host-back-recovery.md, which is in its lock list but
  not its committed paths).
- The queue readback after the row shrink is cited in the LANDED
  note and the rows' own next_step.
- No cargo, no battery, no push — rides the recovery push.

## Non-coverage (named honestly)

- Dragon hunt cb4b2026 (Learning-AI arm-phase, HOLD state) still
  embeds its policy in the row: cross-territory,
  sibling-coordinated — named follow-up in the local-continuation
  runbook, NOT done here.
- Other territories' hunt rows (none currently recurring) were not
  surveyed for the blob class.
