# Umber core-loop queue — single source of loop truth

Created 2026-10-12 (first v1.0 run; the contract landed 02:30 and no prior
session had instantiated it). Maintained by the dragon every wakeup.
Roadmap truth: docs/specs/requirements.md. STATE truth: STATE.md.

Wave boundary: 2026-10-12 — strategy review accepted (d3516c6), core-loop
phase opened. GUI/P2 FROZEN pending Gate 0 (superseded the same week: Gate 0
PASS run 38255484123 + the freeze LIFTED per Bessa's standing directive —
see the C1 section below). No tags, no release claims.

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
| B6 Gate 0 journey test | **done (Gate 0 PASS — run 38255484123 both platforms)** | 15c072e: crates/umber-app/tests/gate0_journey.rs — FBX → 4K pressured paint → compositor (the headless see-lit) → undo/redo byte-exact → sidecar save/reopen byte-identical → ORM export decode-verified. umber-app gains a lib target (lib.rs) + two pub seams (publish_fill, set_brush_color). The claw-authored kill landed dragon-verified (six legs read, boundaries checked, the WGSL isNan story verified against naga 30's vendored front); clippy -D warnings clean on both crates at final state. Gate 0's runtime verdict LANDED 10-12: PASS run 38255484123 (ubuntu + windows GREEN — the queue's own GATE 0 section; the stale "verdict pending" cell ticked in the 2026-10-20 M1 pass-2 sweep). Report: docs/loop/kill-reports/2026-10-12-gate0.json. |

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
| 2 | Re-baseline perf/baseline.json on the 4K path | **done — superseded by the 10-19 reseed**: the fd49627 Intel-GL seed was replaced wholesale when the four-leg bisect convicted the runner FLEET, not the code (leg log in docs/perf/protocol.md) — baseline.json reseeded to same-tip fleet-era means (`c53a08d` + `b1d3660`), the nightly drift-hardened (`dc4b355`: median-of-3 + same-tip canary), C2-14 CLOSED (row C2-13..19), and the first hardened nightlies ran green on the pushed tip (39306559332 + 39391362339) | ticked in the 2026-10-20 M1 sweep: the old "CI reseed = C2-14" pointer died when C2-14 closed and the reseed landed |
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
| 14 | control protocol v0 (absorbed) | **done (`9c43666`)** | the agent session drives projects: project_open/state/save, set_active, undo/redo — all strict-typed (deny_unknown_fields), 14 tests incl. the save→open→census round-trip; the wire edit step (paint ops over the protocol) ✓ LANDED — agent.rs carries the paint-steps design (`PaintStroke(PaintStrokeStep)`, the optional absolute `radius` per the paint-ops design doc, the fleet wave's agent-brush-radius + export-naming kills) — the old follow-up note was stale, ticked in the 2026-10-19 sweep. Export retarget rides export_mesh |
| 15 | offscreen snapshot harness (absorbed) | **done (`4210f17`)** | egui_kittest 0.36.2 (snapshot+wgpu features): display_panel + psd_import render headless into a Harness, PNGs compared against committed baselines (UPDATE_SNAPSHOTS=true regenerates; CI compares). Verified real renders (vision-checked — widgets visible, not blank). More panels ride as their state becomes headless-constructible — the named follow-up. The --script control-call arm **done as protocol form 2026-10-14**: a JSONL script piped into `umber-cli agent` (judgment 1 of `docs/loop/kill-reports/2026-10-14-harness-script.md` — not a `--script` CLI flag; `agent.rs` carries the `paint_snapshot` step in the committed tree). Residuals: per-adapter golden hashes (deliberate deferral) — `snapshot-dir` ✓ LANDED with the W-C3a fleet (`db63487`, pushed e610ed7..96edd1e; ticked in the 10-23 docs pass). Ticked in the 2026-10-20 M5 pass — the earlier "still owed" note grepped the flag literal and missed the landing shape |
| 16 | corpus pattern for importers (absorbed) | **LIVE — the corpus is real and the gate has run green**: the 3 .spp dropped 10-14 (BessaCreatureV2 / Bessa Synth / HydrageaBessa — ls-verified 10-20 at ~/corpora/umber/spp/), the host rsync completed 2026-10-16 08:31 with all three hash-matching local AND manifest (the W-C2g row), and the corpus gate ran green against these bytes on the host. The old "ARMED, awaiting the first real .spp" note was stale — ticked in the 2026-10-20 M1 sweep. h5py: 330 datasets, 196 chunked at 65,536 elements, zero filters. Infrastructure `8634918` | crates/spp/tests/corpus.rs: the manifest schema (file/sha256/notes — never bytes), UMBER_SPP_CORPUS override, absent-dir skip with a loud note, present-dir drift = HARD failure, every oracle through umber_spp::open. Pure-Rust sha256 (FIPS vectors pinned).

| 17 | .spp import — reader crate (ruling 1) | **done (`30b966e`)** | umber-spp: the HDF5-subset reader (superblock v0-v3, OHDR v1/v2+OCHK, the strict datatype allow-list, deflate/shuffle/fletcher32, chunked v1 B-trees, symbol-table groups), HBO detection-only with era labels, the first-class import report; 21 synthetic tests from the research doc's own grids; the real-file corpus ✓ LANDED — `spp/tests/corpus.rs`'s pinned-oracle pattern (`UMBER_SPP_CORPUS`, never-committed files, CI-mountable) + the W-C2g corpus gate ran green on 3 real Painter files hash-matched on the host — the old follow-up note was stale, ticked in the 2026-10-19 sweep |
| 18 | PSD import — flatten-on-import (ruling 5 #2) | **done (`a4bf81a`)** | composite-first + honest fallback rasterizer, every deviation reported; 18 byte-pinned tests + the Assets panel section; the #[default]-Composite report-path defect found by the battery's empirical probe |
| 19 | Dynamic materials design doc (ruling 5 #3) | **done (`83aa242` → r2 `7ecea60` → Kill A `f85cb0a`+`30985d8` → Kill B `2c6497e`, CI GREEN)** | the full arc: the claude-opus adversarial review (19 findings, 4 BLOCKERs — the artifact: docs/loop/artifacts/2026-10-13-dynmat-review.json), the r2 rewrite, the headless driver (core model + eval + resamplers + composite; the two CI fixture lies fixed), the UI wiring (Layers rows, frame drain, Save as material, 16 ftl keys). Named follow-ups: per-layer stroke targeting (design-level — v1 strokes are pool-flat), the per-layer param editor (cut §6) |
| 20 | Toolbar icon-first (the redirect: UI/GUI improvements) | **done (`40c755f`, CI GREEN)** | the Panel family's glyphs consumed: file/history/view/mask as icon buttons with tooltips; icon_button_enabled (the gated variant); dock-tabs honestly stay text (no image WidgetText in egui 0.36 — boundary recorded) |
| 21 | File > New (the redirect: missing tools) | **done (`cd60ede` + the dirty-check `2124b66`, CI GREEN)** | the last disabled shell action live; AppState::reset_for_new test-pinned field-exact; the dirty-check CLOSED: Document.dirty rides run()/attach (undo stays conservative — test-pinned), the centered confirm window (Save…/Discard/Cancel — egui 0.36 has no true modal; the Perf HUD Window precedent), 5 ftl keys both locales |
| 22 | Per-layer pool — the design arc (the stroke gate's real fix; closes the four dynmat v1 warts) | **done — all five stages landed 2026-10-14** | 1a+1b+2 (`b1ceb2c`, `f2aff9f`), 3 selection + stroke gate (`ed64a58`), 4 per-layer pixels (`dd6e1cd`), 5 anchors + instances (`ec6d552`); the arc closed in `cfb205c`. Review artifact: docs/loop/artifacts/2026-10-14-perlayer-pool-review.json |
| 23 | The GUI plan G0-G7 (docs/specs/gui-plan-2026-10-15.md) | **done — every plan row carries its ✓ + commit SHA** (all G0-G7 ticked in the plan's own tables; g6-stencil-overlay closed via the stencil wave `0955ac8` + `2bb8c60`). The per-row "(… open)" parentheticals (g0-notices app wiring, g1 OS-bitmap cursor, mask thumbnails, …) remain the named follow-up surface — honestly open, not claimed closed here. Ticked in the 2026-10-20 M1 sweep | Each kill carries its ✓ + SHA in the plan's own tables (the single source for per-kill state). Two sessions work it in parallel under kill locks. 2026-10-15 dragon kills: g2-bakes-margin-ui `741ec47`, g0-focus-dispatch `cfc8158`, g4-gradient-ramp `9fb1066`, g4-valuefield-grammar `22be3a8`, g4-curve-widget `ca1b0d0` (UNREGISTERED: widgets/mod.rs is under the g1-axis-gizmo lock), g5 Recent `ede999b`, g5 Revert/Window/About `8a53645` + build.rs commit hash `8920890`, g5-focus-modes `64a9754`, g5-workspaces `5f7bdd2`, g0-kittest-harness `ef4d5fd`, g6-display-mode-keys `a420e07`, g4-property-context-menu `ce21db8` + 48-site sweep `b0478b2`. Wave W-G5 is ONE push (Bessa 2026-10-15: larger waves, complete before pushing) |


## The C2 refreshed worklist (2026-10-15, the ACTIVE queue)

Source: `docs/loop/audit-2026-10-15-c2.md` (Pass 4, 28 items, evidence
per row). The C1 list above is complete. The GUI plan (item 23) continues
in parallel, but C2's Tier 1-2 outranks the remaining GUI polish: a
painter cannot yet finish a real PBR asset in the GUI.

| # | item | tier | state |
|---|---|---|---|
| C2-1 | Quit guard (dirty close asks Save / Discard / Cancel) | 1 data | ✓ done (CancelClose + the confirm window); the owed GUI smoke is instrumented: `docs/loop/desktop-checks.md` check 1 — the row closes when its Result line fills |
| C2-2 | Panic hook + crash report + next-launch offer | 1 data | ✓ done (hook first in main(), pending reports offered once as sticky notices) |
| C2-3 | Pixel autosave + "Recover unsaved work?" | 1 data | ✓ done (120 s full snapshots to the state dir; launch prompt); the kill-9 smoke is instrumented: `docs/loop/desktop-checks.md` check 2 — the row closes when its Result line fills |
| C2-4 | Ctrl+S silent re-save; Save As = Ctrl+Shift+S | 1 data | ✓ done |
| C2-5 | Paint-channel targeting (Brush panel picker → `set_active_channel`) | 2 loop | ✓ done (the Channel picker; export of the painted channels = C2-6) |
| C2-6 | Export every painted channel through the presets | 2 loop | ✓ done (neutrals + partner fill; GPU paint→glTF→ORM test) |
| C2-7 | One undo timeline (strokes + masks + layer ops on Ctrl+Z) | 2 loop | ✓ done — unified_undo/redo + the History buttons landed (GPU-tested); coordinated row travel landed (a click moves both journals); Ctrl+Z dispatch wired ✓ (the ◐ marker was stale — the row's own text said done; ticked in the 2026-10-19 stale-row sweep) |
| C2-8 | Layers panel parity (reorder, rename, opacity, blend, duplicate, folders) | 2 loop | ✓ CLOSED 2026-10-19 (the row-closure census, every claim grep-verified live): drag reorder LANDED — the `LayerDrag` drag-handle (`drag_handle` :1140, the ↕ payload) → hover-gap Drop gesture (:1175-1196, release + payload-clear) → `drop_to_reorder` gap math with its own-gap/edge tests (:1334, :1480) → `doc.run(LayerCommand::reorder)` — the undo-integrated document op (:954-960, the "drag E below A" gesture test applies it) — plus the drag-handle kittest (panel_kittest:197, role + label) and the `history.op-reorder` i18n key; duplicate copies pixels ✓ (prior); folders ✓ `LayerGesture::Group` → `group_selected` (:936); opacity + blend ✓ the channel sub-row's clamped slider + combo (`channel_blend_options`); rename in the row's name edit. The old "drag reorder deferred" note was STALE — the kill had landed under the wave commits and the row never ticked |
| C2-9 | Per-texture-set layer stacks | 2 loop | ✓ done in 4 stages (`per-set-stacks-design.md`); materials + fills scoped to their own set's tiles ✓ (`2026-10-15-c2-9-parked-materials.md`); Export writes each set under its own name ✓; per-set MASKED bakes designed (`per-set-masked-bakes.md`: `MeshData.triangle_materials`, 5 host-gated kills); the desktop check is instrumented: `docs/loop/desktop-checks.md` check 3 (a headless variant needs the set-switch glue extracted to a lib fn — the W-C3a plan's §5 tail item 4) |
| C2-10 | Smart-material / smart-mask shelf | 2 loop | ✓ v1: shelf + undoable apply, mesh-map generators bound from the newest bakes, `.umbermask` composed onto a layer's mask; noise generators bound to the fill's own noise, mask anchors read the named layer's plane |
| C2-11 | Bitmap stamps → angle → jitter quartet | 2 loop | ✓ done (engine, brush, jitter, panel rows + stamp picker; stamp-image persistence ✓ landed `58442f0` — `picked_stamp` retained per session, Save/Save As writes `<stem>.stamp.png` beside the preset, `load_stamp_for_active` restores it; the old 'open' note was stale — ticked in the 2026-10-19 sweep) |
| C2-12 | The colour picker (g4-color-picker) | 2 loop | ✓ done (widget + color_button popup at all 3 sites); sliders / persisted swatches ✓ LANDED with the W-C3a fleet (G1 `21ada34` + A3 `5b38c96`, pushed e610ed7..96edd1e — the old parked-fleet pointer is consumed history); the eyedropper's display-chain pick landed the same wave (B10 `8e33c8f`, adopted); its kittest GPU-pick fixture = the named follow-up |
| C2-13..19 | Honest claims: checklist, perf nightly runner + leak-proof gate + real budget, UV'd FBX fixture, the doc-drift sweep, the §10 MSI row | 3 claims | **ALL SEVEN CLOSED.** 13 + 18 done (`c85a5a8`); 14 + 15 landed (the nightly runs + the leak-proof gate; the baseline reseeded 2026-10-19 `c53a08d`/`b1d3660` after the runner-fleet drift conviction, docs/perf/protocol.md); 19 done (MSI dropped from the §10 row, cargo-dist 0.32.0); 17 done (generated UV'd FBX, Gate 0 asserts the unwrap); **16 ✓ (2026-10-19, the RTX 3070 — C2-16 closed: viewport p95 0.2034ms vs the 16.7ms budget (~82x inside, 1M tris + the 4K set, the bench's own `within_budget: true`); input-photon p95 10.08ms vs 20ms; the 200Hz soak zero drops — the §12 budgets' first real-hardware record, kill report `2026-10-19-real-gpu-budget`, protocol note's real-GPU section)** |
| C2-20..22 | Security: panic-hunt the uncovered seams, plugin consent + size cap, Windows signing decision | 4 security | ◐ C2-20 ✓ (7 seams + the agent wire, no panic found), C2-21 ✓ (blake3 consent + 8 MiB cap); C2-22 (Windows signing) is Bessa's decision |
| C2-23..25 | Release path: SPEC ratification and the dogfood asset (Bessa), a `v0.1.0-alpha.1` dry-run tag | 5 release | 25: local `dist plan` ran — the tag needs a `0.1.0-alpha.1` version-bump commit first (checklist item 9); the push is Bessa's call. 23: ratification brief + amendment A6–A10 written (`spec-ratification-brief.md`), awaiting Bessa's tick in SPEC.md; 24 Bessa (a two-material prop, e.g. sword blade + hilt) |
| C2-26..28 | User docs + a generated keyboard reference, plugin SDK guide, kittest breadth | 6 docs/tests | ◐ C2-27 ✓ (`docs/plugins/authoring-guide.md`, its example built + loaded); C2-26 ✓ (`docs/user/keyboard.md` generated + drift-tested; `docs/user/getting-started.md`, claims code-checked); C2-28 ✓ (kittests: Bakes checklist, Brush channel combo, Layers add/raise, Graph Trust & load; the Export path stays gate0_journey's) |
| C2-29 | Bakes panel: all nine mesh-map kinds (normal + cage, world normal, ID, bent normal), not four | 2 loop | ✓ 9 of 9 (High poly group: pick, cage, Bake from high; `2026-10-15-c2-29-transfer.md`); Export writes the newest baked normal ✓ |
| C2-30 | Windows Ink default-on for the Windows build (or documented as opt-in) | 5 release | ✓ closed without a change: the default build already has Windows pen pressure through winit's WM_POINTER path; `winink` is hook-less (dead code if enabled). Tilt/eraser = a future WndProc kill (`2026-10-15-c2-30-winink.md`) |

Frozen (historical note): the GUI/P2 freeze is LIFTED per Bessa's
standing directive 2026-10-12 ("do NOT wait on me for anything to work
in this project"). Items 10-12 are regular queue work in their audit
order. GUI 0b shell chrome and the P1 backlog names are queue candidates
for the NEXT audit cycle, not walls.

## Push log

- 2026-10-15 **the C2 waves** (Bessa: larger waves, complete before pushing). One row per wave, newest first; the gate is the full battery on a clean clone of the exact tip.

  | wave | tip | what landed | gate | CI |
  |---|---|---|---|---|
  | 2026-10-29 wave: g5-viewport-presets + l6-low-followups (local-4e9bde4c) | `ad6f44b` (+ the docs commit) | One code commit for both kills (shared main.rs): F1/F2/F4/F5 viewport-preset registry actions over Panel::Viewport/UvView (`workspaces::apply_viewport_preset`; palette + tooltips + both i18n catalogs + keyboard.md regen; 7 new unit tests) + the pi review's 2 LOWs (History-arm same-frame graph drain in the dock viewer; the load test's undrained-pending pin on bump_epoch) + 2 kill reports | battery GREEN umber-app: layers + fmt + clippy -D warnings + 879 tests 0 failed (lib 205 / bin 641 / integration 67) | CI run on the docs tip — see the coordination STATE block |
  | 2026-10-30 wave: g7-keyboard-only (local-20261029_142722_5f7fd3) | `e008dcc` (+ this docs commit) | One code commit: focus ring token `FOCUS_RING_WIDTH=2.0` pinned on `widgets.active.bg_stroke` (theme.rs + unit test); `actions::esc_closes_overlays` + `App::close_overlays_on_esc` closing the Window dialogs + pending confirms on Esc (recovery window deliberately excluded; menus/popups/palette close natively) + `QuickSetup::close`; `toolbar::menu_digit_pick` — Digit1-9 pick the nth enabled item in File/Edit/Window via the `action_item` items sink + 3 new kittests + kill report | battery GREEN: layers + fmt + clippy -D warnings + gpu leg + umber-app 885 tests 0 failed | CI run on the docs tip — see the coordination STATE block |
  | W-C3b sweep + the flatpak five-wall chain (PUSHED through 10-23 evening — origin `3f1f307`) | `3f1f307` | The parked W-C3b four (d36cfb1 usdz-sublayer-walk, b5a60b1 blend-replace-opacity, 230d730 solo-height, d7eea57 tangent-frame) + f9a4f7a swatches-race + 07bcd2b gpu-golden-import + 82975df/089b1a4 close tooling, carrying c697bab + f62d112 + f4eff95 + 211195f forward; battery GREEN 1,272 tests / 4 crates locally (mesh 145 + core 126 + gpu 151 + app 850; the two mid-close fix-forwards — tangents.rs iterator-enumerate + compositor.rs Replace-alpha — solo-verified 113/113 + 151/151 first); THEN the packaging chain: 01f69a6 vendor parser → e3ae21d runtime 26.08 → e76be0c install trio → 3824b41+334af5b rofiles (attribution corrected) → 3f1f307 svg loader | the local close port's battery + the post-rows scorecard --check | ci 40017411324 red on the gpu-feature leg (E0433, fixed 07bcd2b) then GREEN at every tip from 40020097585 (the c697bab pin's verdict — the segv gone); flatpak dispatch chain 40017442313/40023752815/40025359694/40026754260/40035240398 red (the five walls, each root-caused from its own run object) → **40040567126 SUCCESS on 3f1f307 — Umber.flatpak 12,635,392 bytes uploads, the lane GREEN end to end for the first time** |
  | W-C3a fleet + the local close (PUSHED 02:52 10-23 — origin `96edd1e`) | `96edd1e` | Eleven rows `8f85074..96edd1e`: the parked W-C3a fleet all nine (A1 export-fit, P1 idmask tile-cache, G1 picker sliders+recents, S1 winink pen-flags, C1 snapshot-dir, graph-history k1, B10 eyedropper adopted, A2 snapshot breadth, A3 picker app sites) + the docs row + the local-port tooling; battery GREEN 1,304 tests / 6 crates locally (the slow-throttle close: -j3, nice 19, the 8G/2G scope shield; nine fires, two reboots, five oomd scope kills to land; the G1 sRGB-endpoint ULP red fixed production-side mid-close) | **red on ubuntu** 39908194347: windows green, change classifier green; the ubuntu leg = the runner-image SIGSEGV (ubuntu-latest rotated 24.04→26.04 on 10-21, crash site wanders — root-caused + pinned `c697bab`, rides the W-C3b push); the layering gate = the stale scorecard (README regenerated in the 10-23 docs pass, rides the W-C3b push); the flatpak dispatch 39908210580 red at bwrap/rofiles on ubuntu-26.04 — the job pinned (rides the W-C3b push) |
  | the doc/tooling tail (PUSHED 02:00 10-23 on Bessa's explicit order — origin `e610ed7`) | `e610ed7` | 28 commits `aac1c4a..e610ed7`: the standing charter + supersession banners, the M1/M3/M4/M5/M5b meta-lanes, the commit-gate hook, the W-C3a/W-C3b close tooling, the W-C4 plan, the tangent-TBN static prep | docs/tooling only, battery-exempt | **red on ubuntu** 39903281308 (the same runner-image SIGSEGV's first tip sighting; layering gate + windows green); packaging push-run 39903281330 green (lint+AppImage — the Flatpak job skipped by trigger design, says nothing about it) |
  | stencil-chain + perf-instrument (2026-10-19 PM; PUSHED 15:54 local — origin `aac1c4a`, ticked in the 2026-10-20 M1 sweep) | `aac1c4a` | The stencil engine's whole remaining chain: the gate_plane paint hook `3a8b4cb` (Amendment A), the app wiring `6c95e26`, the viewport shell path + N `2bb8c60`, the set-switch re-stage `dcc5e89`, the cross-tile observer `45ad4e4` (the opencode claw on deepseek-v4.1-flash — the residual seam closed, the mid-stroke window BY DESIGN), the design closure `450b3e1`; the perf instrument rebuilt: reseed `c53a08d`/`b1d3660` + the drift-hardened nightly `dc4b355` (median-of-3 + same-tip canary) after the four-leg bisect convicted the RUNNER FLEET (code innocent — leg log in docs/perf/protocol.md); C2-16 closed `dc0c599` (the RTX 3070: viewport 82x inside budget, i2p half, zero drops — C2-13..19 ALL SEVEN CLOSED); the flatpak archive-type fix `5e7f4dd`; the per-set-stacks closure + the wave docs rows | batteries green at every kill (822 at the chain's close; docs rows battery-exempt) | **green** — CI 39306460278 on aac1c4a: all four jobs success (layering gate, change classifier, fmt+clippy+test ubuntu-latest, fmt+clippy+test windows-latest); packaging push-run 39306460702 green (AppImage + lint; Flatpak bundle SKIPPED by trigger design); the first HARDENED nightly on the pushed tip GREEN ×2 (perf job success: 39306559332 dispatched 10-19, 39391362339 scheduled 10-20 05:09 local) — the drift gate's live proof LANDED (ticked in the 2026-10-20 M1 sweep) |
  | stencil-wave (host-return #3, 2026-10-19) | `0955ac8` | The ENTIRE freeze-3 parked fleet in one close: the stencil engine (core transform+rasterize port-verified, app state/chords/overlay with 12 i18n keys both locales, viewport quad math + stencil_owns gating), e0063 (the perf-harness triangle_materials default — every nightly's build-step red), the agent paint-ops pair (set_brush_radius_texels seam + paint_stroke.radius refusal + ExportAsk.set_override naming), stamp sidecar save (C2-11's tail), the hermetic Flatpak manifest (621 CDN-verified sources + template/generator + drift gate), kittest layers breadth, masked-bakes ray half, five design closures (pcpool, dynmat, per-set-stacks, masked-bakes, falloff) + the executable recovery runbook. Closed through `scripts/stencil-wave-close.sh`: battery GREEN 1,434 tests / 6 crates after ONE clippy fix-forward (viewport needless_option_as_deref); two recovery-script defects fixed live (probe self-match; vacuous byte-blind verdict-reuse — addenda 1+2 in its report) | green, 1,434 | **green** 39263817670 both platforms; packaging push-run **green** 39263817851 (the flatpak verdict gate); perf-nightly 39263843229 build **green** (e0063) but compare **red** on real numbers → live-regression hunt named, confirmation 39268968173 |
  | W-GUI-1 (2026-10-16) | `5e0d89e` | Four GUI-plan kills in one wave: g3-keymap-editor (claw: capture/conflict/presets/delta, one `with_table` funnel), g5-home-screen (claw + dragon wiring: the home card with recents + recover, actions drained post-dock), g6-uv-view-parity (dragon: UDIM grid + tile numbers, shader-side channel isolation with the `isolate_picks_its_channel` golden, shift-hold pixel inspector), g6-contextual-toolbar (dragon: the strip editing the panel's own state, StripEdit channel). 18+9+7 ftl keys; the no-mesh literal now i18n'd. Kittest capture flow + strip render test = named follow-ups with the wiring | green, app 687 + gpu 145 | **green** 38954115915 (both platforms) |
    | W-C2g (host-return) | `1031b27` | The ENTIRE parked set landed in one battery (`ddb59e0`: 967 tests green, clippy clean): g5-workspace-persist, kittest breadth + lib surface, c2-28, the 4 sec-* kills, spp-extract-cli, wave-benchmark-public, wave-abr, spp-texture-cache-reader (corpus gate LIVE — 3 real Painter files, 17 spec fixes), win-version-resource, crash-recent-ops. Push flushes: the host lock (`ed56755`), the UDIM sanitizer (`eca286b`). Post-wave: packaging lint+AppImage GREEN after the OARS-id + SDK-extension fixes (`6758619`); Flatpak offline-deps = named kill. Perf-nightly's first completed run RED on viewport x15.3 at PRE-wave head `06c1711` (triage in docs/perf/protocol.md, not this wave's). The interrupted corpus rsync from the outage window completed 2026-10-16 08:31 — all three .spp files hash-match local AND manifest on the host (the corpus gate ran green against these bytes) | green, app 651 + gui 97 + cli 74 + core 109 + spp 36 | **green** 38872572891/`eca286b` + 38885801883/`1031b27` (both platforms); packaging 38883914616 lint+AppImage green, 38883955783 Flatpak red (offline deps) |
  | W-C2f | `06c1711` | W-C2e's Windows red fixed (the panel transfer test's WARP deadline); Windows CI skips four WARP-slow GPU tests; C2-10 noise generators + mask anchors; C2-9 materials/fills scoped to their set's tiles, Export named per set | green, app 533 | **green** 38807725408 (Windows tests 41 → 17 min) |
  | W-C2e | `ca51214` | C2-29 High poly transfer + Export's baked normal; C2-10 mesh-map generators + `.umbermask`; the CLI transfer-offset fix; the C2-25 dist-plan finding | green, app 529 | **red on Windows** 38804748176 (that test's 60 s WARP deadline), Linux green |
  | W-C2d | `fd91398` | C2-3 pixel autosave + recovery; C2-11 panel rows + stamp picker; C2-9 per-texture-set stacks (4 stages) | green, app 524 | **green** 38797679021 |
  | W-C2c | `6eaf0d0` | C2-1 quit guard, C2-4 silent save, C2-7 one timeline + row travel, C2-11 stamps/angle/jitter, C2-12 picker at all sites; the unlocked wiring; the stroke-dirty fix | green, app 513 | **green** 38792464589 |
  | W-C2b | `d0e75df` | colour picker widget, `copy_owner_planes`, keyboard reference + getting-started, plugin authoring guide, C2-29 all bake maps, C2-28 kittests; audit rows C2-29/30 | green, app 492 | **green** 38779956572 |
  | W-C2a | `e116500` | C2-5/6/8/10/17/19/20/21 + docs | green, app 483 | **green** 38772263265 |
  | W-G5 | `c5372f3` | 17 commits `45fa138..c5372f3`, ten kills + docs; the gate fixed a battery bug (the default crate scope was one string) | green, 8 crates / 1,037 tests | — |

  Windows CI cost, measured on 38797679021: tests were 41 of 52 min (GPU tests on WARP, software D3D12). The cache save is 1 s. The earlier "~10 min in rust-cache's post-save" note (run 38760269560) was a misread and is corrected here; see `2026-10-15-ci-windows-skip-slow.md`.
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

## Wave log — W-GUI-2 (2026-10-16, host-outage parked)

State at 21:55. Build host (centralhost 192.168.15.64) SSH-refused
since ~18:25; ping dead; SSH watcher (proc_fc3e0b60f904) probing.
W-GUI-1 fully closed earlier (5e0d89e pushed, CI 38954115915 GREEN).

| kill | author | landed | verification class |
|---|---|---|---|
| g2-layer-gestures (module) | claw | yes | ad-hoc: 15/15 module tests on host BEFORE the outage |
| g6-overlays-popover | claw | yes | ad-hoc: 17/17 + clippy clean on host BEFORE the outage |
| g5-tab-overflow (dock wiring) | claw | yes | static: rustfmt + python width-math port + registry-resolved APIs; 4 headless tests UNRUN |
| g5-first-run | dragon | yes | static (first 3/3+13/13 on host pre-outage; F1/F2/F8/F9 fixes re-landed after, statically verified 17/17) |
| g2-gesture-integration | claw | yes | static: 6/6 claims tree-derived, rustfmt; 5 shell tests + kittest UNRUN |
| g2-context-rail (widget half) | claw | yes | static: 13 tests present, all APIs registry-verified; UNRUN |
| g7-accesskit-sweep (umber-gui half) | dragon | yes | static: 9 insertions + 6 kittests, APIs registry-verified; UNRUN |
| g7-density-scale (data half) | dragon | yes | static: 2 new tests + 1 re-pin; UNRUN |
| icons-a11y (sweep deferred item) | dragon | yes | static: 1 kittest; UNRUN |
| held-key-tools (data half) | dragon | yes | static: 5 tests; UNRUN |
| polish-widgets (type_ahead + dialog_buttons) | dragon | yes | static + python port (5+4 tests, cycle-rule + redeclared-index defects found AND fixed by the port); UNRUN |

Cross-review (deleg_17a9b405, artifact
docs/loop/artifacts/2026-10-16-wgui2-cross-review.md): 1 BLOCKER +
10 findings, ALL FIXED and re-derived (17/17 checks) — including the
F1 double-join (Quick Setup re-opens every launch) and the F3 dead
popover toggles. Full disposition in the artifact.

Locks held (release at commit): wgui2-umber-gui-widgets (all
umber-gui surfaces, 13 files), wgui2-gesture-integration
(document/main/layers_panel), wgui2-held-key-tools (toolbar/actions).

Commit surface: 38 dirty/untracked files, hash-pinned in
~/.hermes/cache/scratch/wgui2-commit-manifest.txt (the scratch
manifest also lists this wave's 18 kill reports).

## Audit (2026-10-17)

The full-project audit landed:
`docs/loop/audit-2026-10-17-full-project.md` — measured census (166.5k
LOC, 2,061 tests, 12.4/kLOC), the strengths (loop under stress,
security honesty, docs aliveness), the gaps (13 uncompiled kills =
THIS runbook; the stale-baseline perf red below; 5 no-test files
named; the shell-halves queue), and the ranked improvements
(reseed-push rule refinement, automated recovery script, the
same-kill registration rule, the brief-template verification ladder,
the two-green-nightlies reseed).

**Perf-nightly red disposition (run 38957356307, 19:46, on 5e0d89e):**
viewport_mean 45.54ms / p95 57.24ms vs origin baseline 5.49/6.24ms —
the EXACT numbers the reseed commit (9acdc1c) recorded as the
hardware-class mismatch. 9acdc1c sits LOCAL, unpushed (rides this
wave's push per the one-push rule), so every nightly until the
close-out push compares against the stale GL-seed baseline and reads
red. NOT a code regression. No re-triage; the runbook's push closes
it. The audit proposes the rule refinement (a live-red config commit
may break the wave gate) for Bessa's ruling.

## Loop convergence (2026-10-17 01:35, iteration #6)

The self-paced /loop ran 6 iterations through the outage. Each probed
the host and landed what was landable; the landable set is now
EXHAUSTED:

- 18 kills on the recovery manifest (60 rows), every one statically
  verified; both C2-32 follow-ups closed (psd-publish built;
  spp-extract's writer verified already-landed by tree
  re-derivation); polish-widgets' registration Deferred ticked.
- Every remaining ledger row is one of: HOST-gated (the battery, the
  corpus, the 148-site field pass — the executable runbook below),
  LOCK-gated (six shell halves on the main.rs/viewport.rs locks,
  released at the wave commit), or PARTNER-gated (release decisions).
- The SSH watcher (proc_4aa810e2a82e) fires the instant port 22
  answers; the /loop wakeups would only add probe latency on top of
  it.

The loop's terminal instruction to whatever session next runs (the
watcher's firing session, the morning session, or Bessa's):
**`bash scripts/recovery-close.sh`** — the whole wave closes in one
command. The manual sequence below is the documentation of what it
does.

## W-GUI-2 CLOSED (2026-10-17 09:5x)

Host returned 04:21 (Bessa power-cycled; the ARP diagnosis was
right). The recovery: 12 battery rounds of fix-forward → **BATTERY
GREEN: 1,589 tests, 8 crates, all clippy** → committed `d9fa931`
(the 19-kill wave + cross-review fixes + the baseline reseed) →
pushed with the fix-forward tail `edb5ffa` and the CI-catch `eecf4c6`
(scorecard + fmt wrap) → **CI 39021293485 GREEN both platforms**.
All 11 wave locks released. The host's installed binary refreshed
(`~/.local/bin/umber`).

## Host diagnosis (2026-10-17 03:40 — for Bessa's eyes)

The ~9h outage is NOT the LAN and NOT a routing issue:
- The gateway (192.168.15.1) pings at 1.4 ms, 0% loss.
- centralhost's NIC **answers ARP** (`ip neigh` shows REACHABLE at MAC
  18:c0:4d:97:a7:41) — so the machine has power and a live-enough
  kernel to respond at the link layer.
- But ICMP is 100% loss and port 22 refuses — the box is wedged at
  the OS level (a hard hang, or reboot-looping before sshd starts).

**Likely needs a physical power-cycle.** Bessa, when you see this:
hold the power button / pull the plug on centralhost, count to five,
boot it. The FIRST thing to run after it answers (from any session):
`bash scripts/recovery-close.sh` — the whole parked wave (18 kills,
uncommitted) closes in one command: sync → battery → commit → ONE
push → CI watch.

## RECOVERY RUNBOOK (fires when port 22 answers)

**The executable form is live**: `bash scripts/recovery-close.sh
[--dry-run]` — encodes every step below (host probe → manifest
preconditions + hash spot-check → host verify-tree cleanup → rsync
with tip assertion → detached wave battery with BATTERY_EXIT sentinel →
confirm-green → commit-by-manifest-path → lock release → push →
explicit run-id resolution → ci-watch). Idempotent and re-runnable;
refuses to guess past any failure. The manual sequence below remains
as the documentation of what the script does:

1. Clean the host's leftover verify trees (~/Projects/umber-verify-*;
   the tabdock one holds an orphaned cargo verdict).
2. rsync the working tree (exclude /target/ /.dragon/ /Cargo.lock).
3. Wave gate — battery over the merged tip naming every touched
   crate: umber-app, umber-gui (+ the spp/docs-only commits need no
   battery). Verify the battery's scope from its LOG.
4. On green: commit by explicit path (manifest is the add list),
   release the seven locks, ONE push, ci-watch with the explicit
   run id resolved from headSha.
5. Tick gui-plan rows + STATE + this log + CHANGELOG in a docs
   commit (rides the NEXT wave's push per the one-push rule).
6. DONE (21:58): type_ahead.rs + dialog_buttons.rs registered in
   mod.rs + lib.rs with re-exports; locks consolidated — five
   fragment locks merged into wgui2-umber-gui-widgets (13 files);
   umber-app still holds gesture-integration + held-key-tools.

W-GUI-3 candidates (queued after close-out): the five main.rs shell
halves (ContextRail wiring, a11y umber-app half, density Preferences
row, held-key poll, popover consumers), g6-stencil-overlay (needs
the stencil engine), per-set masked bakes.

**Status 2026-10-17 (post-W-GUI-3 landing, `7b5b33e`+`efd0cbf`):**
the six shell halves above are DONE (the wave landed; the CI catch
was the crate-root re-export, fixed same-push). The remaining
W-GUI-4 candidates, in dependency order:

1. **falloff Custom** — ✓ LANDED `4a2e3c2` (battery GREEN: 1,279
   tests, 5 crates; design doc + kill report + panel editor).
2. **Levels' CurveEdit consumer** — ✓ LANDED `7208a7b` (the curves
   node edits as one in-row CurveEdit; battery GREEN: 905 tests).
3. **per-set masked bakes** — ✓ LANDED end to end:
   `a23bcac` (triangle_materials + submesh_for_material + the three
   importer fills), `5aae0ec` (the bakes-panel consumer: each enabled
   set bakes from its own submesh), `c59034b` (with_occluder: the
   AO/thickness raster-vs-occluder split), `59c679a`+`73a57cd` (the
   empty-occluder refusal, SPLIT-path-gated — CI green 39056807314).
   Battery green at every step. Remaining: the multi-material FBX
   fixture (named follow-up).
4. **g6-stencil-overlay** — still blocked on the stencil engine.
5. **The rail's row-label registry** — ✓ LANDED `8eb02c9` (every
   Properties body's pure label fn + the shell's per-frame
   set_labels; Ctrl+F greys non-matching rows and jumps to hits;
   insert-only, battery GREEN: 914 tests).

Plus the `a394876` E0063 tail (52 MeshData test literals across 7
crates gain triangle_materials) and the scorecard fix `6f3c58b`.
W-GUI-4 is CLOSED except g6-stencil-overlay (stencil-engine
blocked), the perf reseed (awaiting green nightlies on the reseeded
baseline), and the partner-gated release lane.

## Scratch

- (Rewritten 2026-10-20 — the previous contents described the 10-16
  outage, the W-GUI-2 locks, and recovery-close.sh, all closed 10-17;
  that history lives in the sections above and CHANGELOG.md.)
- Host + envelope (ticked 2026-10-22 M1 pass-3): centralhost came back
  2026-10-21 ~13:32 (plain boot, FRAGILE; memtest86+ owed at Bessa's
  next physical access; host checking STOOD DOWN by his order). The
  2026-10-22 envelope (Bessa): full autonomy except cargo build/test —
  the parked fleet closes block on the prohibition, no batteries
  anywhere while it stands. The live coordination surface (STATE block,
  parked fleet, watchers): `.dragon/COORDINATION-next-wave.md`.
- The doc/tooling tail (everything after origin `aac1c4a` — 23 commits through e50bbe9, counted live at the pass-3 sweep: the M1/M4/M5/M5b passes, the W-C3a close script, the commit-gate hook, the W-C4 plan, the standing-charter supersession banner ff92379, the tangent-TBN static prep e50bbe9) PUSHED 10-23 (aac1c4a..e610ed7 02:00 on Bessa's explicit order + the wave push e610ed7..96edd1e 02:52; CI dispositions in the push log above); the runbook is consumed history — the recovery order + the fleet's battery scopes live in `.dragon/runbooks/host-back-recovery.md` (count re-ticked in the 2026-10-22 M1 pass-3 sweep).
- kill-locks.sh (scripts/): add/check/remove/list. Live locks: NONE
  (the locks dir was emptied 10-22 01:47 with no owner; re-add per-kill
  locks at close time — lockless ≠ unowned, the fleet bytes belong to
  their kills).
- The commit gate for this shared tree: `python3 .dragon/dcommit.py`
  (fail-closed, lock-scoped; `.dragon/commit-ledger.jsonl` is the
  ownership record) — now ENFORCED, not just conventional:
  `scripts/hooks/pre-commit` (committed, a178913) is installed at
  `.git/hooks/pre-commit`; a bare `git commit` refuses with the dcommit
  usage, dcommit stamps `DCOMMIT_INVOCATION=1`, and the partner
  override is `git commit --no-verify` (deliberate, visible in history).

## Stencil wave + freeze-3 outage (2026-10-18 ~02:00 — CLOSED 2026-10-19 through the host-return recovery; kept as the wave's historical record)

**CLOSED 2026-10-19 (~11:00 -03).** The host returned 08:42:55 (watcher
probe 1188); boot journal clean this boot (zero Hardware Error/MCE
beyond the boot banner — memtest86+ remains OWED at Bessa's next
physical access). The recovery ran as ONE session per the runbook:
claim through the interlock, all 51 manifest rows zero-drift, rsync,
default-leg battery (NO gpu features). The first battery RED was ONE
clippy red — viewport.rs `needless_option_as_deref` (:1033, an
identity rebind on an already-`Option<&mut T>`) — with ALL 1,434 tests
green in the red run; fixed, manifest re-pinned (74e125a4), fresh
battery GREEN (1,434 tests, 6 crates, clippy clean all-6). TWO defects
in the recovery script itself were found and fixed live: the step-2
host probe matched its own compound wrapper (`pgrep -af cargo` →
`pgrep -ax` exact-name), and the verdict-reuse path was vacuous by
construction plus byte-blind (launch-time sentinel captured before the
overwrite; tree_state now hashes dirty-file BYTES over the exact rsync
surface) — harness-verified 3/3 + 5/5 + 10/10, production-proven on
the clean run, documented as addenda 1+2 in the script's own report.
Push `ff9ac02..0955ac8` (7 rows), CI 39263817670 GREEN both
platforms. CORRECTION (flatpak-archive-type kill, same day): the
earlier record here — "packaging push-run GREEN 4m15s = the flatpak
kill's verdict gate (the failed dispatch 39263847856 was a CDN
transient — ab_glyph fetched as an error page)" — was wrong on both
halves. The push-run 39263817851 SKIPS the Flatpak job by trigger
design (its green is lint+AppImage); the dispatch failed at
EXTRACTION — "Unknown archive format of ab_glyph-0.2.32.crate" —
AFTER the sha256 verified (the download was the real 20,967-byte
crate). Real defect: flatpak-builder's archive-format guess from
the URL suffix (.crate is not in its table); fix = archive-type:
tar-gzip per source, landed in the flatpak-archive-type kill; the
post-push packaging re-dispatch is the verdict gate. perf-nightly
39263843229: BUILD GREEN (e0063 VERIFIED — the nightly's first full
run since the reseed) but COMPARE RED on real numbers (viewport_mean
x1.83, viewport_p95 x1.59, input_photon_p50 x2.32 vs the authentic
5e0d89e-seeded baseline) → the live-regression hunt is NAMED (triage
in docs/perf/protocol.md; confirmation re-run 39268968173 on the same
tip). All 16 kill locks released at their commits; the consumed
coordination notes + manifests + sentinel deleted at this docs pass
(the per-set-stacks closure row f0d49ad and this docs pass ride the
NEXT push). The wave's named next kill: the gate_plane paint hook
(umber-paint). Historical record below.

The "Host diagnosis" section and its `recovery-close.sh` instruction
above belong to the 10-16 outage and are superseded (the stencil wave
closed through `scripts/stencil-wave-close.sh`, now consumed).

Host state: wedged since ~21:48 10-17 (event C — the third wedge of
10-17; dossier: vault `projects/umber/2026-10-17-centralhost-freeze3-evidence.md`,
entry in docs/loop/build-host.md). ARP REACHABLE, ICMP 100% loss,
port 22 dead. Needs Bessa's power-cycle. **Recommendation for Bessa:
boot straight into memtest86+ at next power-on, skip the OS** — two
of the three 10-17 wedges hit idle boots within minutes; the memtest
debt is now critical. If booting the OS instead to unblock the wave:
the alive window may be MINUTES — execute the recovery in ONE
continuous session, and run NO `--features gpu` verification inside
it (the wgpu deadlock class, 94469e8, is the one wedge cause under
our control; the default-leg battery is the green-history path).

### Parked kills awaiting the host (dated table)

| Kill | Files | Author | Verification class |
|---|---|---|---|
| stencil-core | NEW umber-core/src/stencil.rs + lib.rs mod | claw kill, cross-reviewed 5e730b4 | math PORT-VERIFIED (5 contract cases, 3 mutations caught); serde caveat resolved host-free (workspace derive, root Cargo.toml:52); tests UNRUN |
| stencil-app | NEW umber-app/src/stencil.rs + main.rs wiring + display_panel.rs surface | claw kill, cross-reviewed 5e730b4 | claim-chain verified line-anchored; honest design-gap seam + pending notice; 12 i18n keys landed both locales (cae97d4/7f1aa5a, CI green); tests UNRUN |
| stencil-viewport | viewport.rs quad math + eyedropper/id-mask gating | claw kill, cross-reviewed 5e730b4 | quad math + blast radius verified; tests UNRUN |
| e0063-refscene | umber-gpu/src/ref_scene.rs (4-line triangle_materials default) | sibling e78fd809 (10-18 ~01:5x) | 5-check gate green host-free (brace census, rustfmt, trap greps, doc-attach, lock audit); tests UNRUN |
| stencil-recovery-close | scripts/stencil-wave-close.sh (the runbook, executable — one-command recovery: claim interlock, both-manifest full verify, hardened rsync, default-leg battery with sentinel poll, e0063 own-row + wave commits, ONE push, ci-watch, perf-nightly re-dispatch) | sibling d9bc9a66 (10-18 ~14:00) | dry-run ×2 byte-identical host-free (18/18 manifest rows verified live by the script's own verifier, push gate, claim-grep 3/3 unit cases); the real run is host-gated by design |
| agent-brush-radius | umber-paint/src/paint_state.rs (`set_brush_radius_texels` seam) + umber-cli/src/agent.rs (`paint_stroke.radius`) + tests/agent_paint_ops.rs | sibling local-d3deea (10-18 ~13:3x, wakeup #1; the paint-ops design doc's named follow-up) | rustfmt clean 3/3, verify-kill 3/3 PASS, interface census done; tests UNRUN (GPU legs at battery) |
| agent-export-naming | umber-cli/src/main.rs (`ExportAsk.set_override`) + tests/agent_export_naming.rs (the first CLI export integration test); agent.rs rides the agent-brush-radius row | sibling local-d3deea (10-18 ~13:5x, wakeup #1; the module docs' named deferral — export unified to bake's active-set rule) | rustfmt clean, verify-kill 3/3 PASS, both ExportAsk constructors compiler-enforced; tests UNRUN (GPU legs at battery) |
| stamp-preset-persist | umber-app/src/brush_panel.rs (picked-stamp retention + `<stem>.stamp.png` sidecar on Save/Save As + 2 tests) | sibling local-d3deea (10-18 ~14:4x, wakeup #2; C2-11's open tail — the save half of the .abr sidecar convention) | rustfmt clean, verify-kill PASS, coverage pin corrected by the python port (9, not 10); tests UNRUN (pure fs, at battery) |
| flatpak-offline-deps | flatpak/{org.umber.Umber.json (GENERATED, 621 crate sources) + .json.in template + gen-cargo-sources.py + vendor-checksums.py} + packaging.yml drift gate | sibling local-d3deea (10-18 ~15:2x, wakeup #3; CI run 38883955783's red root-caused live — the sandbox has no DNS) | json.tool VALID, generator --check idempotent, 621↔621 lock cross-check verbatim, the `.crate`-URL + checksum contract verified against the LIVE CDN; the real gate = the packaging workflow re-dispatch post-recovery |
| kittest-layers-breadth | umber-app/tests/panel_kittest.rs (3 kittests + shared closure: ⧉ duplicate, selection-gated properties rows, blend combo + one-undo) | sibling local-d3deea (10-18 ~15:5x, wakeup #4; C2-28's named follow-up) | rustfmt clean, verify-kill PASS, all 4 i18n keys verified present in BOTH catalogs (group+.attr); kittest legs UNRUN (at battery) |
| pcpool-design-review | docs/specs/per-channel-tile-pool-design.md (status DRAFT→LANDED/CLOSED + the verified-truth closure tail) + the closure kill report | sibling local-d3deea (10-18 ~17:5x, Bessa live-steer pass; the doc was stale-DRAFT — the implementation landed 10-12 and the tree evolved a generation past it) | 21/21 closure-claims census vs the live tree (all anchors grep-verified: TileTargets, the viewport seam, the migration test, successors); doc-only — NO battery legs |
| dynmat-design-closure | docs/specs/dynamic-materials-design.md (status READY-TO-IMPLEMENT→LANDED/CLOSED + the verified-truth tail incl. the material_layer move + re-export) + the closure kill report | sibling local-d3deea (10-18 ~17:5x, same closure pattern; Kill A 10-13 + Kill B 10-14 both landed — every symbol grep-verified live) | 23/23 closure-claims census vs the live tree (Kill A/B symbols, both catalogs' keys, the landing reports, the umber-paint move + umber-app re-export); doc-only — NO battery legs |
| per-set-stacks-closure | docs/specs/per-set-stacks-design.md (status DRAFT→LANDED/CLOSED + the verified-truth tail: both hazards resolved as designed, the reconciling set_layers rename cited to its report) + the closure kill report | sibling local-d3deea (10-18 ~19:2x, Bessa live-steer pass 2; all four C2-9 stages + two follow-ups landed 10-15 — seven reports) | census vs the live tree (from_parts/next_id, set_layers + set_of_tile + the union census, activate_set/active_set, the seven reports); doc-only — NO battery legs |
| masked-bakes-ray-half | bakes_panel.rs + bake_sources.rs (the ray half BUILT: 2 with_occluder helpers, 3 mapped arms + margins, the (u16,bool) tile-seams key, bake_extra_map's occluder param, the GPU cross-material occlusion pin + control) + the brief + the built report | sibling local-d3deea (10-18 ~19:5x, Bessa steer pass 3 — the brief AND its build in one evening; unmapped meshes keep byte-for-byte output) | rustfmt clean both, verify-kill both PASS, every signature matched live (incl. the &self submesh_for_material fix); the new GPU test + the full suite run at the battery (umber-app + umber-bake) |
| falloff-design-closure | docs/specs/falloff-custom-design.md (status DRAFT-panel-round→LANDED/CLOSED + the verified-truth tail: engine + panel both in the tree, 3 open questions resolved, the serde VERIFY answered) + the closure kill report | sibling local-d3deea (10-18 ~20:5x, Bessa steer pass 4; the fourth stale-design closure — the ENTIRE design was live: LUT binding 5, id 10, the in-place CurveEdit row, the catalog key, every planned test) | census vs the live tree (set_falloff_lut, the WGSL binding + case-10, the variant + eval arm, the panel Custom branch, the key); doc-only — NO battery legs |

Static state of the parked bytes (verified 10-17 23:3x, re-verified
01:5x): `cargo fmt --all --check` CLEAN on the exact parked bytes;
i18n 735 keys per catalog, en==pt pair complete, all 168 referenced
code keys present both locales; `mod stencil` registered in both
crates; zero byte drift vs the 20:46 cross-review. Remaining
unknowns: compile + suites ONLY.

Manifests (sha-pinned, re-verify before rsync — mismatch = drift):
`.dragon/stencil-wave-recovery-manifest.txt` (16 pins, twice-verified),
`.dragon/e0063-refscene-manifest.txt` (refscene kill). Locks live:
stencil-core, stencil-app, stencil-viewport, e0063-refscene,
agent-brush-radius, agent-export-naming, stamp-preset-persist,
flatpak-offline-deps, kittest-layers-breadth,
pcpool-design-review, dynmat-design-closure,
per-set-stacks-closure, masked-bakes-ray-half,
falloff-design-closure —
release at the wave commit (agent-brush-radius + agent-export-naming
are ONE combined commit row — agent.rs carries both kills' hunks;
manifests: `.dragon/agent-brush-radius-manifest.txt`,
`.dragon/agent-export-naming-manifest.txt`,
`.dragon/stamp-preset-persist-manifest.txt`,
`.dragon/flatpak-offline-deps-manifest.txt`,
`.dragon/kittest-layers-breadth-manifest.txt`,
`.dragon/pcpool-design-review-manifest.txt`). Full
coordination map + watcher state:
`.dragon/COORDINATION-stencil-wave.md` (the loop sessions' channel).

### Recovery runbook (stencil wave — full detail in the coordination note)

**Executable form (2026-10-18 ~14:00): `nohup bash scripts/stencil-wave-close.sh > /tmp/stencil-close.log 2>&1 &`**
— encodes steps 1-6 below (claim interlock on the note, both-manifest
full verify, hardened rsync — adds W-GUI-2's /.git/ /.dragon/
/Cargo.lock exclusions, host-side .git stays behind, default-leg
battery with a bounded BATTERY_EXIT poll + a tree-state sentinel so a
finished verdict is reused but a fix-forward forces a fresh battery,
e0063 own-row commit then the wave commit-by-manifest, ONE push,
ci-watch, perf-nightly re-dispatch). Step 7 (the docs pass) stays
manual post-CI-green. Dry-run verified host-free: ×2 byte-identical,
18/18 manifest rows clean, claim-grep 3/3. Battery scope = umber-app
umber-core umber-gpu; the agent-rows' crates (umber-paint, umber-cli)
extend CRATES when those kills pin + announce.

1. Claim first: append "RECOVERY CLAIMED by <session>" to
   `.dragon/COORDINATION-stencil-wave.md`, then execute 2-7 without pause.
2. Host pre-flight: reflog -5, pgrep -af cargo, porcelain count — stand
   down if a sibling is mid-recovery.
3. Verify manifest hashes (both manifests).
4. rsync -az --delete --exclude=/target/ → host.
5. Gate 1: scripts/wave-battery.sh on the host (default features only).
6. Gate 2: commit-by-path per manifest labels (code kills + wave docs +
   the docs ticks this section owes), release the four locks, ONE push,
   scripts/ci-watch.sh.
7. Green → wave closed; docs pass deletes the consumed coordination
   notes and manifests, ticks this section, STATE.md, CHANGELOG.md.
