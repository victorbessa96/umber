# Umber core-loop queue — single source of loop truth

Created 2026-10-12 (first v1.0 run; the contract landed 02:30 and no prior
session had instantiated it). Maintained by the dragon every wakeup.
Roadmap truth: docs/specs/requirements.md. STATE truth: STATE.md.

Wave boundary: 2026-10-12 — strategy review accepted (d3516c6), core-loop
phase opened. GUI/P2 FROZEN pending Gate 0. No tags, no release claims.

## Phase A — truth pass (dragon-authored)

| item | state | evidence |
|---|---|---|
| A0 requirements un-tick/split, ✓code vs ✓reachable | **done** | d3516c6 (prior session): six row corrections (OCIO, stylus, distribution, camera, perf floor, masks, history). Rows now carry the ✓code/NOT REACHABLE split with file:line evidence. |
| A1 STATE.md rewrite from current truth | **done** | e116729: one status line (CORE-LOOP-IN-FLIGHT), the queue summary, the contradiction + Oct-9 grant language removed, history preserved in CHANGELOG/DECISIONS pointers. |
| A2 roadmap reconciliation (requirements/SPEC/STATE/README one story) | **done** | 3e0278f: README status → core-loop truth; SPEC dated amendment note (execution reordered; deliverables unchanged; still DRAFT pending ratification). |
| A3 docs sweep (stale wave claims) | **done** | 3e0278f: README + SPEC were the live stale surfaces; wave3-delivery-audit.md is dated history, preserved. |

**PHASE A COMPLETE (2026-10-12 04:0x).** All four items landed; queue order
recorded in STATE.md; the strategy-review queue is the active wave.

## Phase B — core-loop kills (B1..B6)

| item | state | evidence |
|---|---|---|
| B1 viewport texture binding (critical 1) | **done** | e802f92 + 9455885: mesh pass UV attribute, WGSL sampling, OpenPBR bind wired app-side; kill report /tmp/umber-viewporttex-*.json; CI green e802f92..d1aeeb3 era. |
| B2 4K tile pool + per-layer/per-channel compositor (criticals 2+6) | **in flight** | Pool + compositor exist (e802f92, tilepool report: 1MiB/tile, 64MiB/layer-channel, memory model prototyped). GATE OPEN: live paint target still TARGET_SIZE=512 (paint_state.rs:233); per-layer channel addressability at the paint path unverified. Remaining: wire 4K as the live target + channel routing. |
| B3 stroke undo journal (critical 3) | **done** | f78e6c1: PaintStrokeCommand per stroke, 64-entry/256MiB caps, v1 clear() deleted. Test premise fixed bed9e0a (smudge stacking contract). |
| B4 pixel save/load (critical 4) | **done** | d834d77 + c999da6: painted maps through content-hash AssetStore; reopen restores. |
| B5 pen pressure end-to-end (critical 5) | **todo** | begin_stroke still hardcodes 1.0; stylus crate has zero uses in umber-app. Linux first. |
| B6 Gate 0 journey test | **todo** | FBX → 4K pressure paint → lit 3D → undo/redo → save/reopen → export decode-verified. Phase exit gate. |

## Push log

- 2026-10-12 bed9e0a pushed — smudge-test stacking-contract fix, forward-fix
  of the 5-red streak (f78e6c1 introduced, 9e4fb9e mis-fixed, bed9e0a
  root-caused: entries stack, one per stroke; sibling test :2360 pins it).
  **CI verdict: GREEN** (run 38212963060, ubuntu + windows, completed
  ~03:55 local). Red streak closed.
- 2026-10-12 e116729 + 3e0278f (Phase A docs) committed locally — **push
  approval requested from Bessa** (wave push; the loop does not push
  without the gate).

## Scratch

- kill-locks.sh subcommands: add/check/remove/list (no default). No live
  locks as of this write; zero claws running.
- Remote build gate (centralhost 192.168.15.64) unreachable this run
  (ping + ssh dead) — CI is the only runtime verification.
- Next queue item when nothing is in flight: finish B2 (4K live wiring),
  then B5. A1 rewrite is the next dragon-authored docs surface (non-colliding).
