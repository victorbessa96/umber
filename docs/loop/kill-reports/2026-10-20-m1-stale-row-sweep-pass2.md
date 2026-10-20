# M1 stale-row sweep — pass 2 (2026-10-20)

Meta-lane M1, second standing pass (steer 23:52; the recurring-lane
protocol lives in the pass-1 report). Claimed 13:3x local by
wake-13490 / local-4fcadcb6, on fresh intel (the live STATE read at
claim). Doc-only, battery-exempt per the envelope — the census IS the
verification. Lock: m1-pass-2 (queue.md, STATE.md, kill-ledger worktree
rows, this report). Committed via dcommit THROUGH the newly installed
pre-commit hook (the gate's second dogfood leg); rides the recovery
push.

## Why a pass now

Four lanes landed since pass 1 (07:56): M5 (ea83093), M5b (6d05bf2),
the W-C3a close script (17f56a8), the dcommit pre-commit-hook
(a178913) — plus the umber-wave-ops skill split (skills-dir side,
v1.6.3 → v1.7.0). Every one rewrote surfaces pass 1 had swept.

## Ticks (each re-derived live before the edit)

1. STATE.md status line — the doc/tooling tail "12 commits,
   4d69808..86fe4a4" → **17 commits, 4d69808..a178913**
   (`git rev-list --count aac1c4a..HEAD` = 17, live at the sweep).
2. queue.md Scratch tail-count line — "13 commits through the M1
   sweep 8cd3bf4 + the M5 runbook pass" → 17 through a178913, the
   new tail items named.
3. queue.md Scratch commit-gate row — was convention-only ("commits
   go through dcommit"); now records the ENFORCEMENT:
   `scripts/hooks/pre-commit` committed (a178913) + installed at
   `.git/hooks/pre-commit`, bare commits refuse with the dcommit
   usage, dcommit stamps `DCOMMIT_INVOCATION=1`, the partner override
   is `git commit --no-verify` (deliberate, visible in history).
4. queue.md row B6 — state cell "done (code; CI verdict pending)"
   stale since 10-12: the queue's own GATE 0 section records the
   verdict PASS (run 38255484123, ubuntu + windows GREEN). Pass 1
   missed the row; pass 2 catches it — the recurring lane working as
   designed.
5. kill-ledger Waiting row 1 — "13 commits through the M1 sweep +
   the M5 pass" → 17 through a178913 (worktree-side per the
   single-owner rule; rides the next ledger commit).

## Held current, verified live (no tick)

- desktop-checks.md: all four Result lines empty — no desktop session
  ran them; C2-1/C2-3/C2-9 stay open on their own terms (runtime acts
  stay pending until a report records them).
- Design-doc DRAFT hits (committed tree): per-layer-pool r1 is the
  history-section note (the doc itself is LANDED+CLOSED, ticked
  10-19); release-checklist gate 7 (SPEC ratification) and the v1.1
  amendment draft are Bessa-gated by design.
- The In-flight fleet rows: all PARKED, HOST dark, scopes in the
  recovery runbook — current.
- The Waiting blocked-ons: HOST_BACK, PUSH FIRST (the packaging
  re-dispatch), Bessa's .spp ratification, memtest86+, the rasdaemon
  interim decode — all live.
- STATE.md handoff item 5's "no live locks ... as of this write" and
  the historical push-log "in flight at this write" lines: dated
  snapshots, kept as records.

## Claim-collision record (verify-open-first, live)

This session's first target was the dcommit hook sample (locked 13:2x
on the 12:5x STATE read). The fresh read at claim showed the main
session had claimed (13:0x) and landed (13:2x, a178913) it mid-scout.
Lock released minutes after claim; zero bytes built. The lock
registry + fresh-STATE-read discipline held the line at the last
checkpoint. Second incident of the 10-15 double-strike class seen
from the wake side; the fix that matters (claim-first, fresh read at
claim) worked — and the anchor-asserted coordination patch aborted
clean when the sibling's skill-split landing rewrote the block
between read and write (fail-safe, nothing half-written).

## Watcher side-record

The 13:2x re-arm race: this session's gated notify launch found the
main session's watcher live (pid 225169, armed 13:22:00) and stood
down — the pgrep launch gate held under a real race, no stack (the
composition rule's first live proof). Census method: the log's start
lines + the exact-path ps, never a bare `pgrep -f`.

## Census

5 ticks, 7 held-current verdicts, 0 unverified claims in the touched
rows. No new CI claims (M2: the push-log row's cited job lists remain
the live verdicts). Doc-only; no code touched; no battery owed.

## Non-coverage

- host-back-recovery.md's "13 commits at this write" tail list
  self-describes as re-derive-at-execution-time — left as-is
  (runbooks are untracked policy homes; the recovery session
  re-derives).
- The worktree-only sibling bytes (eyedropper + app-code dirt) are
  not landed state and were not swept (the committed tree is the
  queue's truth).
