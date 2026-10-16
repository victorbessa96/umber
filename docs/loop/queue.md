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
| 16 | corpus pattern for importers (absorbed) | **ARMED 2026-10-15**: 3 real Painter projects + `manifest.json` in `~/corpora/umber/spp/` (dev machine); copy to the host to run it there (`docs/loop/build-host.md`). h5py: 330 datasets, 196 chunked at 65,536 elements, zero filters. Infrastructure `8634918` | crates/spp/tests/corpus.rs: the manifest schema (file/sha256/notes — never bytes), UMBER_SPP_CORPUS override, absent-dir skip with a loud note, present-dir drift = HARD failure, every oracle through umber_spp::open. Pure-Rust sha256 (FIPS vectors pinned). Drop procedure staged at ~/corpora/umber/spp/ — the first real .spp Bessa drops lights it up |

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
| C2-9 | Per-texture-set layer stacks | 2 loop | ✓ done in 4 stages (`per-set-stacks-design.md`); materials + fills scoped to their own set's tiles ✓ (`2026-10-15-c2-9-parked-materials.md`); Export writes each set under its own name ✓; per-set MASKED bakes designed (`per-set-masked-bakes.md`: `MeshData.triangle_materials`, 5 host-gated kills); a desktop check open |
| C2-10 | Smart-material / smart-mask shelf | 2 loop | ✓ v1: shelf + undoable apply, mesh-map generators bound from the newest bakes, `.umbermask` composed onto a layer's mask; noise generators bound to the fill's own noise, mask anchors read the named layer's plane |
| C2-11 | Bitmap stamps → angle → jitter quartet | 2 loop | ◐ engine + brush halves landed (stamp dabs, per-dab angle, StampAngle fixed/follow/random, PaintState::set_stamp); ✓ done (engine, brush, jitter, panel rows + stamp picker; stamp-image persistence with presets open) |
| C2-12 | The colour picker (g4-color-picker) | 2 loop | ✓ done (widget + color_button popup at all 3 sites; sliders / eyedropper / persisted swatches deferred) |
| C2-13..19 | Honest claims: checklist, perf nightly runner + leak-proof gate + real budget, UV'd FBX fixture, the doc-drift sweep, the §10 MSI row | 3 claims | 13 + 18 done (`c85a5a8`); 14 + 15 landed (the nightly runs + the leak-proof gate; the baseline reseed waits on its first run); 19 done (MSI dropped from the §10 row, cargo-dist 0.32.0); 17 done (generated UV'd FBX, Gate 0 asserts the unwrap); 16 (real-GPU budget row: Bessa's host) todo |
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
  | W-C2g (staging) | — | committed: G5 Close + Keyboard Shortcuts (`ae7d1e2`), session log + Show Log (`42fd6c4`), crash-report log pointer (`fe93639`), "The menus" guide (`da05190`), per-set masked bakes design (`4ca2854`), C2-22 signing policy (`ac4ee3b`), C2-30 closed by source trace (`795879d`), C2-23 SPEC brief + A6–A10 (`770deb2`), C2-16 3070 runbook (`a5f2492`), the alpha-tag steps (`7514af6`, `374153a`), the build-host page (`e731144`). UNCOMMITTED + UNRUN: g5-workspace-persist (+ the hidden-panel restore fix and `on_exit` resolved from fingerprints), g0-kittest-breadth (+ lib surface; static fixes: a duplicated `is_open`/`set_query`, wrong `shell.*` keys → `history.*`), c2-28 export kittest, mask-compose-perf, showlog-no-cmd (security: cmd.exe re-parsed the path), **sec-name-traversal (security: mesh material names reached bake/export paths unsanitized — audit C2-31)**, sec-sidecar-channel (`C:`/dot-only/stream channel segments in the paint manifest), sec-import-channel (`UnsafeChannel` import reason), wave-benchmark-public (BUILT, unrun: `umber-cli bench`, 49k-tri generated cube, six bakers + encodes timed, JSON line; shares umber-cli main.rs with spp-extract-cli → commit together), C2-32 (map import puts sRGB colour PNGs into the linear plane unconverted → washed-out export; one-function fix in main.rs after g5-workspace-persist; lands with the spp extract writer going sRGB), wave-abr (BUILT, unrun: .abr → stamped/radial .umberbrush + tip PNG via ag-psd, preflight against the samp alloc), spp-texture-cache-reader (BUILT, unrun: Bessa 'build what's needed to load the .spp files'; 61 channels in 10 named sets from the 3 corpus files, Python-port verified; see its kill report. Was: NEEDS BESSA's call: real `.spp` keep flattened 4K channels in `texture-cache/` as QDataStream + LZ4 + GL format, which the importer skips; decoded and verified on BessaCreatureV2, see research §7a; needs `lz4_flex` + a flag→channel-name source), spp-hint-honest (the `spp-import.hint` in both catalogs claims channels import; real Painter files yield none: reword after sec-import-channel commits, which holds en-US.ftl), g5-tab-overflow-core (umber-gui elide + scroll-into-view; the main.rs wiring follows g5-workspace-persist), spp-inflate-bound (a deflate bomb in a `.spp` inflated to 256 MiB before the footprint check), win-version-resource (alpha step 0; commits only together with its workspace dep, after g5-workspace-persist), i18n-coverage-test (pt-BR missed 21 keys; translated in `120a974`, and en-US's `reason-unsafe-channel` committed with it so both catalogs match at HEAD) | GATE 2 COMPLETE 2026-10-16: wave pushed (`ddb59e0` → `eca286b`, then docs `c1507cc`), CI run 38872572891 on `eca286b` **GREEN** (ubuntu + windows both, WARP included). Two build-infra flushes surfaced and fixed during the push: the stale local Cargo.lock (the host-resolved lock is authoritative — never rsync over it) and the export sanitizer eating the `<UDIM>` token (angle brackets cannot climb paths; `path_safe_value` keeps them). The perf-nightly's first completed run (38869909446, PRE-wave head `06c1711`) is RED on viewport_mean x15.3 — recorded in `docs/perf/protocol.md` as a standing triage, NOT this wave's. | HOST RETURNED 2026-10-15 23:41 and the whole parked set ran in ONE battery (2026-10-16 02:5x, `scripts/wave-battery.sh umber-app umber-gui umber-cli umber-core umber-spp`): **GREEN** — 651+97+74+109+36 tests, clippy clean, layers clean, fmt applied. The run also flushed out and fixed real bugs: the umber-app serde deps (serde/serde_json landed in dev-dependencies), `crate::crash` visibility on the lib target, the epaint TexturesDelta drop bomb in the tab-overflow tests, per-brush `dropped` surfacing in the ABR report, and the workspaces layouts ±∞→null serde repair (scoped to x/y corners). THE CORPUS GATE LIT UP: umber-spp now opens all 3 real Painter files and imports texture sets — 17 spec fixes measured on the real bytes (root symbol-table entry, SNOD stride, off-wide keys, v1 continuation blocks, H5T_STD_U8BE size-1, unlimited maxdims, layout v1→v3 normalize, chunk-key shape, padded edge chunks, never-written contiguous, the lz4 off0 end marker + prefix decode). Kill reports updated with VERIFIED sections; the builder now writes the same shapes the corpus pins | — |
| PACKAGING (2026-10-16, post-wave dispatch): lint job GREEN, AppImage GREEN (staged binary, licence notices, payload-integrity smoke) after two real fixes: the metainfo's invalid `social-money` OARS id (removed from both copies, sync-checked) and the missing `org.freedesktop.Sdk.Extension.rust-stable//24.08` install the manifest requests. Flatpak bundle job still RED: the flatpak sandbox resolves no network and the manifest has NO vendored cargo deps, so `cargo --locked` cannot fetch flate2 (job 116917077139). Named follow-up kill: vendor the registry into the Flatpak source tarball or pre-fetch via a network-enabled build step (flatpak-builder's own pattern). Runs: 38883914616 (lint+AppImage success), 38883955783 (Flatpak red). | — |
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

## Scratch

- kill-locks.sh subcommands: add/check/remove/list (no default). No live
  locks as of this write; zero claws running.
- Remote build gate (centralhost 192.168.15.64) unreachable this run
  (ping + ssh dead) — CI is the only runtime verification.
- Next queue item when nothing is in flight: finish B2 (4K live wiring),
  then B5. A1 rewrite is the next dragon-authored docs surface (non-colliding).
