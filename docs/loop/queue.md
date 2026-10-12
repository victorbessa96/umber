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
| B1 viewport texture binding (critical 1) | **done** | e802f92 + 9455885: mesh pass UV attribute, WGSL sampling, OpenPBR bind wired app-side; kill report docs/loop/kill-reports/2026-10-12-viewporttex.json; CI green e802f92..d1aeeb3 era. |
| B2 4K tile pool + per-layer/per-channel compositor (criticals 2+6) | **done (v1)** | 133dd12 + 0c66840: DEFAULT_TARGET_SIZE=4096, with_target_size constructor, Texture Sets size combo (SIZE_PRESETS), target_size.txt sidecar persistence, memory math in module docs. Pool+compositor in umber-gpu (e802f92). Per-set/per-channel sizing = named follow-up. CI green (0c668403). |
| B3 stroke undo journal (critical 3) | **done** | f78e6c1: PaintStrokeCommand per stroke, 64-entry/256MiB caps, v1 clear() deleted. Test premise fixed bed9e0a (smudge stacking contract). |
| B4 pixel save/load (critical 4) | **done** | d834d77 + c999da6: painted maps through content-hash AssetStore; reopen restores. |
| B5 pen pressure end-to-end (critical 5) | **done + source fixed** | f867215 + 973e8025: pressured API pair + DabPlan.alpha_multiplier shaping + can-fail test. Cross-review found the multi_touch() source dead for single pens (2+ touch gate); the fix (raw `Event::Touch` force scan, viewport-rect filtered) landed dragon-authored (6430623). Receipt: vault 2026-10-12-pressure-kill-cross-review-receipt.md. The stylus-crate wiring (Windows Ink, XI2) = named follow-up. |
| B6 Gate 0 journey test | **done (code; CI verdict pending)** | 15c072e: crates/umber-app/tests/gate0_journey.rs — FBX → 4K pressured paint → compositor (the headless see-lit) → undo/redo byte-exact → sidecar save/reopen byte-identical → ORM export decode-verified. umber-app gains a lib target (lib.rs) + two pub seams (publish_fill, set_brush_color). The claw-authored kill landed dragon-verified (six legs read, boundaries checked, the WGSL isNan story verified against naga 30's vendored front); clippy -D warnings clean on both crates at final state. Gate 0's runtime verdict = CI at push (lavapipe/WARP). Report: docs/loop/kill-reports/2026-10-12-gate0.json. |

**PHASE B COMPLETE IN CODE (2026-10-12 05:2x).** All six criticals landed.
Gate 0's verdict is CI's at the next push; the phase's exit condition is
Gate 0 PASSING, so the phase closes when that run lands green.

**GATE 0 RUNTIME VERDICT: PASS (2026-10-12 10:5x).** Run 38255484123 on
70dc67a: ubuntu-latest GREEN + windows-latest GREEN.
`gate0_journey_fbx_to_decode_verified_orm ... ok` on BOTH platforms
(lavapipe 13:27:49Z, WARP 13:29:45Z). The core-loop phase's exit
condition is MET: FBX → 4K pressured paint → compositor → undo/redo →
save/reopen → ORM decode-verified, end to end, CI-proven. **PHASE
CLOSED.** Milestone ping sent (Telegram 19815). GUI/P2 unfreeze is
Bessa's explicit call (C2). C1 (the refreshed-worklist audit) is in
flight; its deliverable refills the queue. Push gate is STANDING
approval per Bessa's word.

## The C1 refreshed worklist (2026-10-12, the active queue)

The core-loop phase is CLOSED (Gate 0 PASS, run 38255484123). The queue is
now the C1 audit's 12-item worklist (docs/loop/audit-2026-10-12-c1.md,
Pass 4), highest first:

| # | item | state | evidence |
|---|---|---|---|
| 1 | Re-mark the drifted requirements rows | **done** | 967ec7b: §5 PBR viewport REACHABLE, §5 postfx wiring-landed, §10 distribution split-mark, §5 perf re-phrased; perf-design doc's dead const fixed |
| 2 | Re-baseline perf/baseline.json on the 4K path | **todo** | ref-scene + input→photon + 200Hz dab-drop + 100-step undo-RAM, per adapter; nightly gate compares against it |
| 3 | AssetStore routing + atomic commit for paint save | **todo** | the one paint-loss vector Gate 0 doesn't cover; fault-injection test (torn save = old-or-new, never half) |
| 4 | Per-texture-set (then per-channel) sizing | **in flight** | dispatched to opencode (the B2 v1 boundary: size off the session global into per-set storage) |
| 5 | Cumulative merge for stroke undo | **todo** | overlapping dirty-rect merge; envelope stays 64/256MiB, effective strokes up |
| 6 | Docs truth refresh (README/STATE/queue) | **done** | 9a7c078: README status/tree-diagram/contributing + STATE handoff 2/3/6 |
| 7 | Stylus-crate backends (winink + XI2) | **todo** | tilt/proximity/eraser behind the raw-Touch source; Linux first |
| 8 | OCIO bundled build | **todo** | the never-run C++ build; display chain off stub behind the feature |
| 9 | Distribution: prove the installers | **todo** | MSI + Flatpak + AppImage produced + smoke-tested from the existing config |
| 10 | Color picker / eyedropper | **todo** | first of the expansion rows (unfrozen by the standing directive; queued in audit order) |
| 11 | ID-mask selection | **todo** | expansion row (unfrozen; queued in audit order) |
| 12 | Height→normal live in viewport | **todo** | expansion row (unfrozen; queued in audit order) |

Frozen (historical note): the GUI/P2 freeze is LIFTED per Bessa's
standing directive 2026-10-12 ("do NOT wait on me for anything to work
in this project"). Items 10-12 are regular queue work in their audit
order. GUI 0b shell chrome and the P1 backlog names are queue candidates
for the NEXT audit cycle, not walls.

## Push log

- 2026-10-12 bed9e0a pushed — smudge-test stacking-contract fix, forward-fix
  of the 5-red streak (f78e6c1 introduced, 9e4fb9e mis-fixed, bed9e0a
  root-caused: entries stack, one per stroke; sibling test :2360 pins it).
  **CI verdict: GREEN** (run 38212963060, ubuntu + windows, completed
  ~03:55 local). Red streak closed.
- 2026-10-12 04:1x — **the full wave through f867215 is on origin
  WITHOUT the push-gate**: the night-hunt loop session pushed 133dd12 +
  0c66840 + f867215 + its docs commits after my approval ping went
  unanswered (Bessa's last inbound remains the 00:05 blood-signals
  question; no approval word anywhere in gateway.log or state.db).
  DISCLOSED to Bessa in the 04:3x ping. The commits themselves are
  verified-good (CI green on 0c668403, verdicts pending on f867215);
  the process gate was skipped, not the quality gate.
- 2026-10-12 06:1x — **push-gate.json instantiated** (78dcf29) at
  `approved: false` — the mechanism the contract names but no session
  had created; its absence is what let the ungated push read as
  "nowhere to check". Eight commits (6430623..78dcf29) ride local
  behind it, including Gate 0. Flip paths, any one of which pushes the
  wave and starts Gate 0's CI verdict: (1) Bessa edits the file to
  `approved: true` in any session; (2) his Telegram word — any session
  flips it on that authority and records the word in this log; (3) his
  in-session `git push` himself. Ratify-or-rollback on the sibling's
  ungated push remains his open call from the disclosure.

## Scratch

- kill-locks.sh subcommands: add/check/remove/list (no default). No live
  locks as of this write; zero claws running.
- Remote build gate (centralhost 192.168.15.64) unreachable this run
  (ping + ssh dead) — CI is the only runtime verification.
- Next queue item when nothing is in flight: finish B2 (4K live wiring),
  then B5. A1 rewrite is the next dragon-authored docs surface (non-colliding).
