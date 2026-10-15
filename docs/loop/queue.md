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
| 2 | Re-baseline perf/baseline.json on the 4K path | **done (seeded `fd49627`) — but from a local Intel HD 530 over GL, not the lavapipe the nightly runs; the CI reseed = C2-14** | the nightly's first artifact was to seed it; the runner label never resolved (C2-14) |
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
| 22 | Per-layer pool — the design arc (the stroke gate's real fix; closes the four dynmat v1 warts) | **done — all five stages landed 2026-10-14** | 1a+1b+2 (`b1ceb2c`, `f2aff9f`), 3 selection + stroke gate (`ed64a58`), 4 per-layer pixels (`dd6e1cd`), 5 anchors + instances (`ec6d552`); the arc closed in `cfb205c`. Review artifact: docs/loop/artifacts/2026-10-14-perlayer-pool-review.json |
| 23 | The GUI plan G0-G7 (docs/specs/gui-plan-2026-10-15.md) | **in flight — the active worklist** | Each kill carries its ✓ + SHA in the plan's own tables (the single source for per-kill state). Two sessions work it in parallel under kill locks. 2026-10-15 dragon kills: g2-bakes-margin-ui `741ec47`, g0-focus-dispatch `cfc8158`, g4-gradient-ramp `9fb1066`, g4-valuefield-grammar `22be3a8`, g4-curve-widget `ca1b0d0` (UNREGISTERED: widgets/mod.rs is under the g1-axis-gizmo lock), g5 Recent `ede999b`, g5 Revert/Window/About `8a53645` + build.rs commit hash `8920890`, g5-focus-modes `64a9754`, g5-workspaces `5f7bdd2`, g0-kittest-harness `ef4d5fd`, g6-display-mode-keys `a420e07`, g4-property-context-menu `ce21db8` + 48-site sweep `b0478b2`. Wave W-G5 is ONE push (Bessa 2026-10-15: larger waves, complete before pushing) |


## The C2 refreshed worklist (2026-10-15, the ACTIVE queue)

Source: `docs/loop/audit-2026-10-15-c2.md` (Pass 4, 28 items, evidence
per row). The C1 list above is complete. The GUI plan (item 23) continues
in parallel, but C2's Tier 1-2 outranks the remaining GUI polish: a
painter cannot yet finish a real PBR asset in the GUI.

| # | item | tier | state |
|---|---|---|---|
| C2-1 | Quit guard (dirty close asks Save / Discard / Cancel) | 1 data | ✓ done (CancelClose + the confirm window; GUI smoke pending) |
| C2-2 | Panic hook + crash report + next-launch offer | 1 data | ✓ done (hook first in main(), pending reports offered once as sticky notices) |
| C2-3 | Pixel autosave + "Recover unsaved work?" | 1 data | ✓ done (120 s full snapshots to the state dir; launch prompt; desktop kill-9 smoke pending) |
| C2-4 | Ctrl+S silent re-save; Save As = Ctrl+Shift+S | 1 data | ✓ done |
| C2-5 | Paint-channel targeting (Brush panel picker → `set_active_channel`) | 2 loop | ✓ done (the Channel picker; export of the painted channels = C2-6) |
| C2-6 | Export every painted channel through the presets | 2 loop | ✓ done (neutrals + partner fill; GPU paint→glTF→ORM test) |
| C2-7 | One undo timeline (strokes + masks + layer ops on Ctrl+Z) | 2 loop | ◐ unified_undo/redo + the History buttons landed (GPU-tested); coordinated row travel landed (a click moves both journals); Ctrl+Z dispatch wired ✓ — C2-7 done |
| C2-8 | Layers panel parity (reorder, rename, opacity, blend, duplicate, folders) | 2 loop | ◐ done in buttons + a properties row (drag reorder deferred; duplicate copies pixels ✓) |
| C2-9 | Per-texture-set layer stacks | 2 loop | ✓ done in 4 stages (`per-set-stacks-design.md`); parked material eval + a desktop check open |
| C2-10 | Smart-material / smart-mask shelf | 2 loop | ✓ v1: shelf + undoable apply, mesh-map generators bound from the newest bakes, `.umbermask` composed onto a layer's mask; anchor sources + graph-evaluated generators open |
| C2-11 | Bitmap stamps → angle → jitter quartet | 2 loop | ◐ engine + brush halves landed (stamp dabs, per-dab angle, StampAngle fixed/follow/random, PaintState::set_stamp); ✓ done (engine, brush, jitter, panel rows + stamp picker; stamp-image persistence with presets open) |
| C2-12 | The colour picker (g4-color-picker) | 2 loop | ✓ done (widget + color_button popup at all 3 sites; sliders / eyedropper / persisted swatches deferred) |
| C2-13..19 | Honest claims: checklist, perf nightly runner + leak-proof gate + real budget, UV'd FBX fixture, the doc-drift sweep, the §10 MSI row | 3 claims | 13 + 18 done (`c85a5a8`); 14 + 15 landed (the nightly runs + the leak-proof gate; the baseline reseed waits on its first run); 19 done (MSI dropped from the §10 row, cargo-dist 0.32.0); 17 done (generated UV'd FBX, Gate 0 asserts the unwrap); 16 (real-GPU budget row: Bessa's host) todo |
| C2-20..22 | Security: panic-hunt the uncovered seams, plugin consent + size cap, Windows signing decision | 4 security | ◐ C2-20 ✓ (7 seams + the agent wire, no panic found), C2-21 ✓ (blake3 consent + 8 MiB cap); C2-22 (Windows signing) is Bessa's decision |
| C2-23..25 | Release path: SPEC ratification and the dogfood asset (Bessa), a `v0.1.0-alpha.1` dry-run tag | 5 release | 25: local `dist plan` ran — the tag needs a `0.1.0-alpha.1` version-bump commit first (checklist item 9); the push is Bessa's call. 23 + 24 Bessa |
| C2-26..28 | User docs + a generated keyboard reference, plugin SDK guide, kittest breadth | 6 docs/tests | ◐ C2-27 ✓ (`docs/plugins/authoring-guide.md`, its example built + loaded); C2-26 ✓ (`docs/user/keyboard.md` generated + drift-tested; `docs/user/getting-started.md`, claims code-checked); C2-28 ✓ (kittests: Bakes checklist, Brush channel combo, Layers add/raise, Graph Trust & load; the Export path stays gate0_journey's) |
| C2-29 | Bakes panel: all nine mesh-map kinds (normal + cage, world normal, ID, bent normal), not four | 2 loop | ✓ 9 of 9 (High poly group: pick, cage, Bake from high; `2026-10-15-c2-29-transfer.md`); Export writes the newest baked normal ✓ |
| C2-30 | Windows Ink default-on for the Windows build (or documented as opt-in) | 5 release | todo (a release decision) |

Frozen (historical note): the GUI/P2 freeze is LIFTED per Bessa's
standing directive 2026-10-12 ("do NOT wait on me for anything to work
in this project"). Items 10-12 are regular queue work in their audit
order. GUI 0b shell chrome and the P1 backlog names are queue candidates
for the NEXT audit cycle, not walls.

## Push log

- 2026-10-15 **W-G5, one push** (Bessa: larger waves, complete before pushing): 17 commits 45fa138..c5372f3, ten kills + docs. Wave gate on the build host in a clean clone of the exact tip: 8 crates, 1,037 tests, fmt + clippy -D warnings, 0 `skipping:` lines. The gate found and fixed a battery bug on the way (c5372f3: the default crate scope was one string). Wave W-C2e staged at `f5f1fee` (C2-29 High poly transfer + Export's baked normal, C2-10 mesh-map generators + `.umbermask` composition, the CLI transfer-offset fix, the C2-25 dist-plan finding), wave gate GREEN on the clean clone over every crate (core 108, color 20, graph 141, spp 23, cli 68, gpu 145, app 529, brush 147, paint 163, gui 92, mesh 123, wasm 36); pushes once W-C2d's CI is green. Wave W-C2d pushed `fd91398` (C2-3 pixel autosave + recovery, C2-11 panel rows + stamp picker, C2-9 per-texture-set stacks in four stages), wave gate GREEN on the clean clone over every crate (core 108, color 20, graph 141, spp 23, cli 68, gpu 145, app 524, brush 147, paint 163, gui 92, mesh 123, wasm 36); CI run 38797679021 watching. W-C2c CI 38792464589: GREEN. Wave W-C2c pushed `6eaf0d0` (C2-1 quit guard, C2-4 silent save, C2-7 one timeline + row travel, C2-11 stamps/angle/jitter, C2-12 picker at all sites, the unlocked wiring: crash reporter live, curve + picker registered, duplicate copies pixels; the stroke-dirty fix), wave gate GREEN on the clean clone over every crate (core 107, color 20, graph 141, spp 23, cli 68, gpu 145, app 513, brush 147, paint 162, gui 92, mesh 123, wasm 36); CI run 38792464589 watching. W-C2b CI 38779956572: GREEN. Wave W-C2b staged locally at `9f80ed8` (22 commits: color picker widget, copy_owner_planes, the keyboard reference + getting-started guide, the plugin authoring guide, C2-29 all bake maps, C2-28 kittests; audit rows C2-29/30), wave gate GREEN on the clean clone: core 107, color 20, graph 141, spp 23, cli 68, gpu 145, app 492, paint 161, wasm 36, mesh 123, gui 72. Pushed `d0e75df` once W-C2a's checks were all green; CI run 38779956572 watching. Wave W-C2a pushed `e116500` (22 commits: C2-5/6/8/10/17/19/20/21 + docs), Gate 1 = every crate's battery on the committed HEAD (core 107, color 20, graph 141, spp 23, cli 68, gpu 145, app 483, mesh 123, wasm 35, gui 72, paint 160, brush 143, export 83, bake 90); CI run 38772263265: **GREEN** (completed success, both platforms). Previous: CI run 38760269560: **every check GREEN on both platforms** (ubuntu job success at 15:34Z; on windows Format, Clippy, Tests, the winink tests and the release build all succeeded, with only the post-job cache save still running at 16:23Z). Finding: Windows runs take ~60 min, ~10 of it in `Swatinem/rust-cache`'s post-save step. That is a CI-cost item for the C2 Tier 6 list.
- 2026-10-15 the GUI-plan wave: 396bcf9 (margin + focus + ramp) **CI GREEN both platforms** (run 38744092732). 979a523 / 32804d7 / ef492f0 went red on Windows only (the sibling's catalog tests split .ftl text on `\n` under a CRLF checkout); fixed forward in 121afdd (`I18n::has_key` + `.gitattributes` pins LF). Later pushes 0434e80, ca1b0d0, 9a69bb1 in flight at this write.
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
