# M4 — the COORDINATION STATE header (2026-10-20)

Lane: meta-hunt M4, seeded by the PARTNER STEER 23:52 (local-autonomy
continuation under FREEZE-4). Claimed + landed by the wake-13423
session (05:4x local). Doc-only, battery-exempt — the census IS the
verification. No cargo, no host, no push; the report commit rides the
recovery push with the rest of the wave.

## The incident (why this lane exists)

The flatpak-vendor-dir claim at 16:2x was made on STALE INTEL: the
sibling's 4d69808 strip-components fix had already committed, but the
claiming session missed it because discovering live state required
probing the TAIL of a 480-line append-only log (this file was 583
lines by the next session's read). The claim cost a wasted claude
dispatch that had to be reaped. Root cause: state discovery scales
with log length; claims need O(1) state, not O(log).

## The kill

A STATE block now lives at the TOP of
`.dragon/COORDINATION-next-wave.md`, above the append-only body:

- HOST line (dark/freezed state + the envelope + the reopen gate)
- Live locks (with the re-verify caveat)
- Parked fleet (HOST_BACK-gated, names + where the battery scope lives)
- Open meta lanes (M1/M5 open, M2 standing rule, M3 + follow-ups, M4)
- Partner-gated list (never autonomous)
- Recent events (newest first, 5 entries)
- Watcher state (who's alive, the re-arm rule)

Maintenance protocol, encoded in the block's own header line:
sessions REWRITE the block on every claim/land (it is the one mutable
surface in the file); the body below stays append-only. Pointers,
not prose — the M5 principle applied to M4's own artifact (battery
scope, manifest, steer sections are all pointed at, never restated).

## The census

Every row in the block traced to evidence before landing:

| Block claim | Evidence |
|---|---|
| HOST dark, ARP FAILED | live probe 05:38 local: ping 100% loss (3/3), `ip neigh` FAILED state |
| Live locks NONE | claim-pairing census: every body claim event (10 incl. the W-C3a batch) resolves LANDED/RESOLVED or rides the parked fleet — grep :13-564; flatpak-vendor-dir RESOLVED BY THE SIBLING, claude dispatch REAPED |
| Parked fleet 6 names | parked-fleet manifest section (body) + 6 untracked kill reports on disk (ls docs/loop/kill-reports/) |
| eyedropper its owner's | manifest's attribution-discipline paragraph |
| M1/M5 open | grep: no M1/M5 claim sections exist in body or on disk |
| M2 standing rule | steer 23:52 section, line 511ff |
| M3 LANDED 0cc7030 | git log HEAD; ledger row in kill-ledger.md; report on disk |
| Drift-hardening NOT open | **the tree wins**: dc4b355 landed it (median-of-3 + same-tip canary) — the body's 14:5x "is mine, next" prose is STALE. Recorded here as the canonical example of why the block must cite tree state, not body prose. |
| Partner-gated 3 items | steer's Partner-gated paragraph, verbatim names |
| Watcher alive | `ps` 05:38: pid 3070325, etime 6h04m, watch-centralhost.sh 4320 |

## Body integrity

The pre-M4 BODY — the file minus its title line, 33,610 bytes — is a
contiguous byte-identical substring of the post-M4 file at byte
1824: the STATE block (bytes 57-1824) is a pure insertion between
title and body, plus the M4 claim/land section appended at the tail
(1,474 bytes). The append-only history is untouched. Verified
programmatically with an asserting check. Precision note: the first
check run in the landing chain was mis-designed (it compared the
full pre-M4 file, title included, which a between-title-and-body
insertion can never match) and printed False while the commit
proceeded on a toothless `&&` guard — this fixup records the exact
check shape and the miss; the same python now asserts, so a red
check blocks the chain.

## Commit shape

This report is committed via `.dragon/dcommit.py` (index mode,
--lock this file) — the M3 follow-up "adopt it in practice",
dogfooded on a clean single-owner file. The coordination file itself
stays UNTRACKED by standing convention (M3's commit deliberately
excluded it; every session reads the working tree, which is how the
coordination surface already functions) — the STATE block is read
from the live tree by every claimant. The kill-ledger row is
appended worktree-side (single-owner rule: kill-ledger.md carries
the sibling's uncommitted rows; the row rides the next ledger commit).

## M2 compliance

This report makes no CI green/red claims; where it names commits it
cites git log output directly (0cc7030, dc4b355). No run conclusions
anywhere.

## Named follow-ups (unclaimed)

- A maintenance-drift check: if a session lands a claim without
  rewriting the STATE block, the next claimant's re-verify line is
  the only net — a possible future lint (block-updated timestamp
  vs newest body section) if drift shows up in practice.
- M1 and M5 remain open per the block.
