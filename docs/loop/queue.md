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
| 2 | Re-baseline perf/baseline.json on the 4K path | **in flight** | the scheduled nightly (run 38234644651) is cooking the lavapipe harness numbers RIGHT NOW — on completion its artifact seeds the first baseline.json; the compare step's first-baseline handling rides the same kill |
| 3 | AssetStore routing + atomic commit for paint save | **done (e4ad8b7)** | atomic manifest commit + store-backed recovery; 7 tests incl. interrupted-save + byte-identity; reopen = old-or-new, never torn |
| 4 | Per-texture-set (then per-channel) sizing | **done (2564c17)** | the set model + per-tile routing at the set's size + per-set panel + target_sizes.txt with v1 back-compat; the two-sets/two-sizes round-trip test pins it; per-channel sizing = named follow-up |
| 5 | Cumulative merge for stroke undo | **done (37025d5)** | merge-at-push for intersecting rects; the 8-stroke soak pins 5632B vs 16384B unmerged; envelope unchanged (64/256MiB) |
| 6 | Docs truth refresh (README/STATE/queue) | **done** | 9a7c078: README status/tree-diagram/contributing + STATE handoff 2/3/6 |
| 7 | Stylus-crate backends (winink + XI2) | **done (d2fd7b7)** | XI2 backend + StylusPoller wiring landed (Linux half; claw-authored, orphan-consumed after wrapper death); winink = the Windows half, named follow-up |
| 8 | OCIO bundled build | **done (60b04d0) — the vendored C++ build ran clean (2m30s); real-mode tests pin the ACES 2.0 pair; the UI-name constants corrected to the registry's enum names** | the never-run C++ build; display chain off stub behind the feature |
| 9 | Distribution: prove the installers | **done (9f875b3) — local boundary PROVEN: real tarballs + installers + verified checksums; windows zip + PS installer + end-to-end release fetch = CI's on first tag** | MSI + Flatpak + AppImage produced + smoke-tested from the existing config |
| 10 | Color picker / eyedropper | **done (7067cd3)** | Pick on the strip + viewport sampling + set_brush_color; display-chain-aware sample = the named follow-up |
| 11 | ID-mask selection | **done (e2bed70) — click region → only that ID paints; 5 headless + 2 GPU goldens green under lavapipe; cached-bake + multi-tile = named follow-ups** | expansion row (unfrozen; queued in audit order) |
| 12 | Height→normal live in viewport | **done end-to-end (be11fe5) — the (c) seam closes it: the height channel binds through set_height_texture, NOT-REACHABLE dead, 317/317 green** | expansion row (unfrozen; queued in audit order) |

| 13 | never-crash lint gate (absorbed from storytold/photocraft) | **done (`4bf0d94`)** | clippy.toml + cfg_attr(not(test), deny(...)) on umber-core/color/spp; the census found the engines already clean (a pin) but caught 3 real expects in layers.rs's instance sweep (now typed errors); the deny FIRES (probe-proven — a planted unwrap lints). The panic-hunt fuzz LANDED (`fc4ba4c`): 850 deterministic garbage + mutated-valid cases through the four public seams (mtlx, psd, spp, project loader), zero panics, 0.01s in CI |
| 14 | control protocol v0 (absorbed) | **done (`9c43666`)** | the agent session drives projects: project_open/state/save, set_active, undo/redo — all strict-typed (deny_unknown_fields), 14 tests incl. the save→open→census round-trip; the wire edit step (paint ops over the protocol) = the named follow-up. Export retarget rides export_mesh |
| 15 | offscreen snapshot harness (absorbed) | **done (`4210f17`)** | egui_kittest 0.36.2 (snapshot+wgpu features): display_panel + psd_import render headless into a Harness, PNGs compared against committed baselines (UPDATE_SNAPSHOTS=true regenerates; CI compares). Verified real renders (vision-checked — widgets visible, not blank). More panels ride as their state becomes headless-constructible — the named follow-up. The --script control-call arm still owed |
| 16 | corpus pattern for importers (absorbed) | **infrastructure landed (`8634918`); corpus itself BLOCKED on real samples** | crates/spp/tests/corpus.rs: the manifest schema (file/sha256/notes — never bytes), UMBER_SPP_CORPUS override, absent-dir skip with a loud note, present-dir drift = HARD failure, every oracle through umber_spp::open. Pure-Rust sha256 (FIPS vectors pinned). Drop procedure staged at ~/corpora/umber/spp/ — the first real .spp Bessa drops lights it up |

| 17 | .spp import — reader crate (ruling 1) | **done (`30b966e`)** | umber-spp: the HDF5-subset reader (superblock v0-v3, OHDR v1/v2+OCHK, the strict datatype allow-list, deflate/shuffle/fletcher32, chunked v1 B-trees, symbol-table groups), HBO detection-only with era labels, the first-class import report; 21 synthetic tests from the research doc's own grids; real-file corpus = the named follow-up (research doc §8) |
| 18 | PSD import — flatten-on-import (ruling 5 #2) | **done (`a4bf81a`)** | composite-first + honest fallback rasterizer, every deviation reported; 18 byte-pinned tests + the Assets panel section; the #[default]-Composite report-path defect found by the battery's empirical probe |
| 19 | Dynamic materials design doc (ruling 5 #3) | **done (`83aa242` → r2 `7ecea60` → Kill A `f85cb0a`+`30985d8` → Kill B `2c6497e`, CI GREEN)** | the full arc: the claude-opus adversarial review (19 findings, 4 BLOCKERs — the artifact: docs/loop/artifacts/2026-10-13-dynmat-review.json), the r2 rewrite, the headless driver (core model + eval + resamplers + composite; the two CI fixture lies fixed), the UI wiring (Layers rows, frame drain, Save as material, 16 ftl keys). Named follow-ups: per-layer stroke targeting (design-level — v1 strokes are pool-flat), the per-layer param editor (cut §6) |
| 20 | Toolbar icon-first (the redirect: UI/GUI improvements) | **done (`40c755f`, CI GREEN)** | the Panel family's glyphs consumed: file/history/view/mask as icon buttons with tooltips; icon_button_enabled (the gated variant); dock-tabs honestly stay text (no image WidgetText in egui 0.36 — boundary recorded) |
| 21 | File > New (the redirect: missing tools) | **done (`cd60ede` + the dirty-check `2124b66`, CI GREEN)** | the last disabled shell action live; AppState::reset_for_new test-pinned field-exact; the dirty-check CLOSED: Document.dirty rides run()/attach (undo stays conservative — test-pinned), the centered confirm window (Save…/Discard/Cancel — egui 0.36 has no true modal; the Perf HUD Window precedent), 5 ftl keys both locales |
| 22 | Per-layer pool — the design arc (the stroke gate's real fix; closes the four dynmat v1 warts) | **stage 1a done (`b1ceb2c`, battery GREEN); r1 doc `48f2c53` → review `9b04ff8` (NOT-READY: 3 BLOCKERs, 12 MAJORs — the artifact: docs/loop/artifacts/2026-10-14-perlayer-pool-review.json); r2 rewrite IN FLIGHT** | the architecture HOLDS (composite-on-read, the premultiplied flag + bottom-copy seed — F3 verified by the hostile reviewer, the 50-vs-100 arithmetic exact); stage 1a landed byte-identical (flag off = existing goldens unchanged, 138 gpu +9 / 539 app). The r2 folds every binding fix in (B1's committed-hash save skip, B2's dormancy-moves-into-stage-4, B3's reader census), then stages 1b-5 build per §7 |


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
