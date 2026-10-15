# Umber Long-Horizon Roadmap — the Substance-Parity March and Beyond

Version: 2026-10-14 (the per-layer pool era). Supersedes the wave column in
requirements.md as the PLAN of record; requirements.md stays the CONTRACT of
record (each kill's Doc-delta updates its rows, this roadmap orders the work).
Wave numbering here is the ROADMAP's thematic march; the loop's wave counter
(the two-gate model) is separate — a loop wave batches 3-5 of these kills.

Method: full census of all 12 requirements sections, every row classified
done / partial-with-named-boundary / open, every open item dependency-ordered
into waves. Parity target: Substance Painter's feature set (the [01]
inventory), exceeded where [04] (next-gen bar) points.

---


> **2026-10-15: the C2 gap audit** (`docs/loop/audit-2026-10-15-c2.md`) is the active worklist. Its headline: a painter cannot yet finish a real PBR asset in the GUI (painting reaches Base Color only, export writes Base Color only from paint, no quit guard or pixel autosave, two undo timelines, one layer stack per document). Its Tier 1-2 rows outrank the remaining GUI polish.
## Where we stand (2026-10-15)

The core loop is Gate-0-proven end to end: mesh → 4K pressured paint →
compositor → undo/redo → save/reopen → export, both CI platforms. The
per-layer pool arc is complete (every layer owns its pixel planes;
stages 1a-5, 2026-10-14). The paint engine is a headless crate
(`umber-paint`, PR #8), so the JSONL agent session paints through the
same engine and persistence the GUI uses — `paint_stroke`, `paint_read`,
`paint_snapshot`, undo/redo, and a `version` step for protocol pinning.

2026-10-14/15 also landed: MaterialX import Kill A + the stdlib
verification (Kill S); the photocraft + Blender absorption pass (the
ledger is `docs/research/absorption-2026-10-14-photocraft-blender.md`):
falloff curves, front-face falloff, the Adjacent Faces bake margin, a
crash-safe GPU startup with backend fallback, save-format completeness by
exhaustive destructuring, the crate layering gate and the docs-only CI
path filter; and the mesh-importer panic hunt + hardening (24 findings,
23 closed; `.usdz` import works for the first time). The two-gate wave
model (local battery per kill, one CI run per wave) runs the march.

## Waves 1-3 of the march — LANDED (2026-10-13/14)

| Kill | State | Proof |
|---|---|---|
| wave-masks (per-layer paintable masks) | ✓ landed | 770162f + 21e1290; compositor gate both paths |
| wave-blends (the 32-mode census) | ✓ landed | d861482 + the HSV/normal-map family goldens |
| wave-history (click-to-time-travel) | ✓ landed | the History panel's journey rows |
| wave-dist (Flatpak + AppImage) | ✓ packaging proven | the shell+PS+CI stack; images published per release |
| wave-postfx-persist | ✓ landed | f35cf48 (project persistence + panel round-trip) |
| wave-ocio-panel | ✓ landed | 9a1a808 (ACES configs selectable; OCIO adoption) |
| wave-perchan-res | ✓ landed | f4e7524 (per-channel sizing end to end) |
| wave-icons-boot | ✓ landed | the icon-first toolbar (40c755f) + the boot atlas |
| wave-perf-baseline | ✓ landed | fd49627 + the nightly's fixed invocation (17:08 fix) |
| wave-spp-import | ✓ landed | 30b966e + 0a5c20d; the corpus harness arms on real files |
| wave-psd-import | ✓ landed | a4bf81a (flatten-on-import, every deviation reported) |
| wave-dynamic-materials | ✓ LANDED END TO END | 83aa242 → review → 7ecea60 → Kill A f85cb0a → Kill B 2c6497e |
| wave-camera-ortho | ✓ LANDED (`40f3e5d`) | see the post-pool row below |
| wave-stylus-win32 | OPEN (wave 5 now) | Wintab32 dual-path |
| wave-8k-export | ✓ built earlier (`cd668a3`, 2026-10-10) | the 8K presets + bake→export→decode proof at 8192²; roadmap row was stale |

## The per-layer pool arc — THE CURRENT WAVE (2026-10-14, in flight)

The pool redesign that closes the four v1 boundary warts (order not
preserved, re-eval compounds, nothing un-publishes, anchors self-feedback)
and gives every layer its own pixels — the architectural heart Substance
users know as "real layers". Design r2 hostile-reviewed READY TO IMPLEMENT
(docs/specs/per-layer-pool-design.md).

| Stage | State | Proof |
|---|---|---|
| 1a compositor premultiplied mode | ✓ landed | b1ceb2c (byte-identical, flag off) |
| 1b plane storage + budgets | ✓ landed | f2aff9f |
| 2 save v2 (layers/<id>/ round-trip) | ✓ landed | f2aff9f (24 persistence tests byte-identical) |
| 3 selection + stroke gate | ✓ landed | ed64a58 (explicit, typed refusals, the Base row) |
| 4 per-layer pixels (Kill-B-class) | ✓ landed (2026-10-14) | strokes/fills/materials/imports on own planes, dormancy + v2 header [B2], the composite-on-read flush [M7]; the perf gate [M8] re-measures in the wave-5 perf row |
| 5 anchors + instances | ✓ landed (2026-10-14) | `ec6d552`: downward CPU prefix flatten, typed Self/ForwardAnchor, instances render |

Post-pool probe kills (the r2's open verifications): eframe 0.36's GPU
error-handler behavior; wgpu 30's error-scope API shape; PaintCompositor
Arc threading; un-premultiply exactness on GPU vendors.

## Wave 5 — the pull-users + depth wave (post-pool)

| Kill | Closes | Notes |
|---|---|---|
| wave-camera-ortho | §5 the named camera gap | ✓ landed (40f3e5d); follow-up: ortho-aware DoF |
| wave-stylus-win32 | §10 the Windows dual-path | Wintab32 polling beside Windows Ink |
| wave-8k-export | §2/§6 the 8K export path | ✓ already built (`cd668a3`); requirements row 99 carries it. Open remainder: none named |
| wave-materialx-roundtrip | MaterialX import (export ✓ built) | research ✓ (bf4718e); Kill A ✓ (bb287cd, parse + report); next S (stdlib verification) → B → C |
| wave-agent-paint-ops | the agent protocol's wire paint steps | strokes/fills over the JSONL session (v0's named seam); pairs with the --script snapshot arm — ✓ LANDED (`77118f0` + `216018e`; `paint_snapshot` `3e003ec`, exact wire strokes `1ff0921`) |
| wave-harness-script | the snapshot harness's --script arm | ✓ landed 2026-10-14: `paint_snapshot` (whole plane → PNG + blake3) — a script is a JSONL file piped into `umber-cli agent`. **Found:** a 2-point wire stroke can paint nothing (the live conditioner swallows it) — follow-up row `wave-agent-stroke-conditioning` |
| wave-fbx-uv-fixture | the FBX test asset has no UVs — ✓ LANDED 2026-10-15 (C2-17: `cube_uv_ascii7400.fbx` from `scripts/gen-fbx-uv-cube.py`; Gate 0 asserts the unwrap) | `cube_b7400.fbx` (ufbx's Blender 2.72 cube) carries NO UV layer — every UV loads as [0,0] (pinned in `fbx.rs` 2026-10-15). Gate 0's FBX leg paints by UV directly so it never noticed; any bake/paint-by-mesh test on it is vacuous. Add a UV-unwrapped FBX fixture (e.g. ufbx's corpus has UV'd cubes) and point Gate 0 + the margin CLI test at it |
| wave-agent-stroke-conditioning | agent strokes that paint exactly what was sent | ✓ landed 2026-10-15: wire strokes run the conditioner in EXACT mode (no one-euro / lazy-mouse; spacing kept). Root cause: 1 Hz one-euro at synthetic 8 ms steps moves the cursor ~10%/event, so a 2-point stroke never travelled one dab spacing. Open: path replays still smoothed (UX call) |
| wave-svg-import | §7 SVG (later) | vector assets as fill/procedural sources |

## Wave 6 — the differentiators (P1s that Substance lacks or does worse)

| Kill | Closes | Notes |
|---|---|---|
| wave-abr | .abr brush preset import (§1 optional) | the one Adobe format that is legal |
| wave-wasm-shaders | §5 custom user shaders = WGSL plugins (P2) | umber-wasm runtime ✓ built; the shader-replacement host hook is the kill |
| wave-tessellation | §5 displacement/tessellation preview (P2) | the height channel exists (tilepool); the preview pass + the displacement budget |
| wave-hdr-surface | §5 HDR output (eframe upgrade gate) | tracks eframe 0.36→next; the capability model + probe ✓ built |
| wave-ray-query | §3 wgpu ray-query acceleration | tracks wgpu stability; the compute-shader path ✓ built |

## Wave 8 — the ecosystem + quality horizon (suggested 2026-10-14, dragon-authored)

The march's plan of record had no rows for these; each closes a real gap
between "feature-complete" and "a tool a studio trusts".

| Kill | Closes | Notes |
|---|---|---|
| wave-perf-protocol | the perf discipline as a standing gate | docs/perf/protocol.md gains the regression-triage flow: every nightly red names its owning wave; the baseline becomes a release artifact, not a CI afterthought |
| wave-plugin-sdk-docs | the WASM plugin SDK's author docs | umber-wasm's runtime is built; the marketplace (wave 7) needs the author guide + the API stability contract first |
| wave-fuzz-corpus | the panic-hunt's structured corpus | today: 850 synthetic garbage cases over mtlx / PSD / .spp / project dirs; next: (a) the **mesh importers** (FBX, glTF, USD/USDC — untrusted files with no garbage coverage yet; found writing SECURITY.md's threat table, 2026-10-14), (b) byte-replays of the corpus harness's real files' structure (headers valid, bodies broken — the middle ground that finds real parser bugs) |
| wave-a11y-first-pass | keyboard navigation + focus states | the icon toolbar + panels need tab order, focus rings, shortcut discoverability — Substance's UI is dense but fully keyboard-driven; ours isn't yet |
| wave-i18n-audit | the pt-BR completeness beyond the test | the completeness test pins key parity; a native-speaker review pass (Bessa's call who) for the strings' actual quality |
| wave-crash-reporter | the never-crash doctrine's last mile | a structured crash handler that writes the last 10 journal ops + the frame state to a user-readable file — the bug report a user can attach without a debugger |
| wave-benchmark-public | the public benchmark story | a `umber-cli bench` subcommand: the bake/export/paint paths timed on a reference mesh, so community perf reports are comparable |

## Wave 8b — absorbed from photocraft + Blender (2026-10-14)

Every row verified against Umber's code first; the ledger with sources,
the already-have list and the declines is
[`docs/research/absorption-2026-10-14-photocraft-blender.md`](research/absorption-2026-10-14-photocraft-blender.md).

| Kill | Closes | Notes |
|---|---|---|
| wave-bake-margin-adjacent | bake/export margin that stays continuous across UV seams | ✓ LANDED end to end (engine `3818da0`; CLI `bca634f` + agent step `c237e4f`; Bakes panel `741ec47`) |
| wave-brush-falloff-curves | brush profile beyond one `hardness` scalar | Blender's presets (Smooth, Sphere, Root, Sharp, Linear, Constant, Inverse Square) + a custom curve; saved in `.umberbrush` with `#[serde(default)]` so old presets keep loading — ✓ landed `d0309e1` + panel combo `9a47fa0` |
| wave-brush-dynamics-angle-jitter | per-dab jitter (+ angle once stamps exist) | size, opacity and position jitter per dab (`BrushParams` has none; surface = `dab_adapter.rs`, seeded per stroke so replays are deterministic). **Angle / rake / random rotation is blocked:** the paint shader stamps a radially symmetric disc (no stamp texture binding in `PAINT_COMPUTE_SHADER`), so rotation is invisible until bitmap alpha stamps land — verified 2026-10-14 |
| wave-front-face-falloff | stretched paint on grazing faces | attenuate dab strength by the view-to-normal angle past a threshold (Blender's Normal Falloff) — ✓ landed `cabe283` + panel control `9a47fa0` |
| wave-stroke-methods | the missing stroke idioms | ✓ LANDED end to end (engine `ee2dce5`; wiring, picker and `.umberbrush` persistence `51d9047`) |
| wave-stencil | paint-through screen stencil | an image overlay you move/scale/rotate and paint through — the logo/decal workflow |
| wave-brush-cavity-mask | edge-wear without building a mask layer | ✓ LANDED end to end (module `405e0e2`; gate composition + Brush-panel Cavity section `4bebc72`) |
| wave-gpu-safe-start | a driver crash at device creation repeating every launch | photocraft's startup marker: write before device creation, clear after the first frames; an uncleared marker steps the backend down next launch; `--safe-gpu` override — ✓ landed `a221a85` |
| wave-save-completeness | a new document field silently not saved | destructure every doc struct in the save converters with no `..`: a new field fails to compile until the save path handles it — ✓ landed `17900df` (project save/load) |
| wave-scorecard | handwritten headline numbers drifting | generate README/STATE counts (crates, tests, rows) from the tree; `--check` in the battery + CI (this pass found 4 drifted docs) |
| wave-ci-path-filter | Windows CI time on docs-only PRs | a `platforms` job: the Windows leg runs when platform-sensitive paths change or a `full-ci` label is set; master always runs both — ✓ landed `908135e` |
| wave-release-phases | integrators broken by late protocol changes | alpha → beta (agent protocol + file format frozen) → RC, written into the 0.1.0 release checklist — ✓ landed `2c19dcd` (+ the agent `version` step) |

## Next waves (2026-10-15 — the plan of record for the next stretch)

Every open item from the 2026-10-14/15 kills, ordered. The live checklist
is the session to-do; each row here names its blocker or its file surface.

### Wave 6a — finish what is half-built (engine landed, wiring owed)

| Kill | Closes | Surface |
|---|---|---|
| wave-openusd-0.8 | the last importer crash: openusd 0.7.0's multi-TB allocation ABORT (#19) | bump `openusd` 0.7.0 → 0.8.0 (fixes all six openusd findings upstream, PR mxpv/openusd#138; `docs/upstream/openusd-0.7.0.md`); adapt `umber-mesh/src/usdc.rs` to the `CrateData::open(impl Into<AssetBuffer>)` change; un-ignore `bug_usdc_huge_count_allocation_aborts` — ✓ LANDED (`5b5b0ba`) |
| wave-stroke-methods-wiring | stroke methods reach the brush | ✓ LANDED 2026-10-15 (`51d9047`): the preset's brush finally reaches the stroke; method picker + size in presets; Drag Dot's release dab flushes into the same undo step |
| wave-cavity-wiring | the cavity mask paints | ✓ LANDED 2026-10-15 (`4bebc72`): one composed gate over quick mask × curvature; Brush-panel Cavity section |
| wave-margin-ui | Adjacent Faces from the app | ✓ agent `bake` step `margin` + `MarginType` folded into `BakeFlags` LANDED 2026-10-15; the Bakes panel Margin type combo LANDED 2026-10-15 (g2-bakes-margin-ui; whole-mesh + per-tile AO seam graphs) |
| wave-fbx-uv-fixture | a UV'd FBX test asset | see its row below |
| wave-valuefield-log | the last raw sliders | `ValueField` log mode; migrate DoF focus + fill_layer (6), path_tool (4), bakes (3), display (2), layers (1) — ✓ LANDED (`7c21424` log mode + DoF; `66b805d` the last 11 raw sliders) |

### Wave 6b — the GUI plan

`docs/specs/gui-plan-2026-10-15.md` (research index
`docs/research/gui-research-2026-10-15.md` over three slices: Painter,
Blender, photocraft; brief `docs/research/gui-plan-prompt-2026-10-15.md`).
It supersedes phases 1-3 of `docs/specs/gui-audit-and-roadmap.md`, whose
Phase 0 is built. Eight waves, ~45 kills, each with its file surface and
its test:

| Wave | Theme | Kills |
|---|---|---|
| G0 | foundations | action registry, focus-aware dispatch, kittest harness, toasts, both-branch style |
| G1 | the first five minutes | brush cursor, Ctrl+RMB size/hardness drag, status-bar tool hints, straight-line preview, Painter/Blender navigation presets, axis gizmo |
| G2 | panels | layer thumbnails + gestures, history panel, texture-set list, context Properties + search, raw-slider sweep, bakes margin UI, stroke-method/cavity UI |
| G3 | find anything | command palette, preferences, keymap editor, rich tooltips |
| G4 | flagship widgets | colour picker, curve, gradient ramp, field grammar, property context menu |
| G5 | shell | menu completion, workspaces, Hide UI, home screen, first-run setup, tab overflow |
| G6 | viewport depth | overlays popover, contextual toolbar, display-mode keys, UV-view parity, stencil overlay |
| G7 | accessibility + polish | AccessKit sweep, keyboard-only, UI scale/density, polish batch |

### Wave 6c — new capability

| Kill | Closes | Notes |
|---|---|---|
| wave-stencil | paint-through screen stencil | 8b row |
| wave-bitmap-stamps | stamp textures | prerequisite of angle / rake / random rotation and the jitter quartet (`wave-brush-dynamics-angle-jitter`) |
| wave-scorecard | generated headline counts | 8b row |
| wave-stylus-win32 | the Windows tablet dual-path | wave 5 row |
| wave-replay-exact | path replays paint exactly | a UX call (replays may want smoothing); default-off until decided |

### Release track

The 0.1.0 gates (`docs/specs/release-checklist-0.1.0.md`): perf
re-baseline at the stage-5 frame path, the SPEC v1.1 ratification packet,
the checklist walkthrough. Upstream bug drafts (`docs/upstream/`) wait on
the owner's go-ahead to file.

## Wave 9 — the far horizon (suggested 2026-10-14 — the differentiators beyond parity)

| Kill | Closes | Notes |
|---|---|---|
| wave-procedural-history | node-graph edits as first-class undo | the graph panel's param edits ride Document::run today; the GRAPH TOPOLOGY edits (add/remove/rewire nodes) need their own journaled command family — the last non-undoable edit class |
| wave-smart-material-params | exposed parameter UI for materials | the dynmat design's cut §6 row: per-layer param editors driven by the definition's declared exposed params — Substance's "instance parameters" |
| wave-bake-farm | distributed bake jobs | the bake is async + headless already (the agent session drives it); a job-queue shape over the protocol = multi-machine bake on a LAN |
| wave-texture-inpainting | the healing-brush class | inpainting (patch-match based fill from the sampled region) — the one retouch tool class the engine lacks; rides the composite-sample seam stage 4 builds |
| wave-live-mesh-edit | re-topo / live mesh reload | today a mesh reload resets the session; a UV-stable reload (matching by UV islands) keeps paint across topology edits — the deepest engineering row here |
| wave-community-templates | the .umber template library | ship 10 curated material templates (weathered steel, leather, sci-fi panel — built with the own engine) as first-class File > New options; the fastest way to show what the tool does

## Wave 7 — the moonshots (P2, 2+ years, from [04])

| Kill | Closes |
|---|---|
| wave-upstream-stylus | the stylus crate → winit upstream (the [03] survey's gap) |
| wave-community-nodes | the node-graph community marketplace (plugin-sdk ✓ built) |
| wave-collab | merge-story → live session merge (per-layer files are the substrate) |

## Standing debts (ride every wave, not wave-gated)

- The perf nightly's invocation was FIXED (2026-10-13 17:08); the
  baseline artifact seeds the compare gate — the pool's stage-4 perf
  gate re-measures under the new frame path.
- AssetStore GC (§8's v1 boundary: sidecars authoritative, no store GC).
- Split-redo under merged undo entries (#5's named follow-up).
- Merge below the top undo entry (#5's deferred arm).
- Cached ID-mask bake (multi-ID, cross-tile — the #11 follow-ups).
- The winink live-HWND integration question (its module doc's open item).
- DPI re-raster (2× atlas baked; runtime DPI change = named follow-up).

## Never (hard walls — requirements.md's guardrails, restated)

- .spp WRITING — undocumented HDF5 + Adobe's format. IMPORT is RATIFIED (Bessa, 2026-10-12 ruling 1 in docs/loop/decisions.md) and landed in wave 3 (`30b966e` + `0a5c20d`): reading user-provided .spp files is interoperability. The .umber directory format stays the kill-shot.
- .sbsar execution — Adobe Additional Terms, no FOSS license. The original
  node graph + MaterialX interchange is the legal answer, fully built.
- Fake interchange: MaterialX exports declared custom nodedefs; consuming
  DCCs evaluate what they support. No full-fidelity claim is ever made.

## The parity scoreboard (updated per landing; Substance → Umber)

| Substance capability | Umber state |
|---|---|
| Painting core (stamps, flow, spacing, falloff curves) | ✓ complete — **angle / alignment / jitter NOT built** (the old "✓ complete" covered them falsely; corrected 2026-10-15, row `wave-brush-dynamics-angle-jitter`) |
| MyPaint-style input curves + stabilizer | ✓ complete |
| Smudge/clone/symmetry/path/polygon/quick mask | ✓ complete |
| Seam-aware stamping + 3D-space strokes | ✓ complete |
| Texture sets + channel stacks + UDIM | ✓ + per-channel pool (2026-10-12) |
| Layers: types, instancing, anchors, smart materials | ✓ complete |
| Blend modes | ✓ 32-mode census landed (d861482) |
| Masks (paintable per-layer) | ✓ landed (770162f); per-plane masks ride the pool |
| Dynamic materials | ✓ LANDED end to end (Kill A + Kill B, 2026-10-14); per-layer pixels ✓ (the pool arc, stages 4 `dd6e1cd` + 5 `ec6d552`) |
| Agent/automation | ✓ JSONL protocol (`PROTOCOL_VERSION` 1): projects, layers, undo, paint strokes/reads/undo, `paint_snapshot` (whole plane → PNG + blake3); wire strokes paint exactly (no pointer smoothing) |
| Headless testing | ✓ offscreen panel snapshots; the never-crash fuzz over mtlx/PSD/.spp/projects AND the mesh importers (5,400+ cases, 2026-10-14/15); scripted sessions via `paint_snapshot` |
| Importers (.spp/PSD) | ✓ both landed; the corpus harness arms on real .spp files |
| Baking (all 8 maps, cage, skew, auto-rebake) | ✓ complete; + Adjacent Faces margin (cross-seam fill, `3818da0`) on the CLI, the agent and the Bakes panel (`741ec47`) |
| Node graph + MaterialX | ✓ complete (original engine) |
| Viewport (OpenPBR, IBL, view modes, postfx) | ✓ + height→normal live (2026-10-12); postfx persistence = wave 2 |
| Color management (OCIO, ACES, LUT) | ✓ bundled build + LUT; panel selection = wave 2 |
| Export (templates, packing, presets, EXR, PSD, USD, MaterialX) | ✓ PSD-out complete; PSD-in = wave 3 |
| Import (FBX, USD, images, mesh maps) | ✓ FBX/glTF/OBJ/USDA/USDC/**USDZ** (usdz never loaded until `6c68ce7`); importers hardened against the panic hunt's 24 findings (all 24 closed: the last, an openusd allocation abort, by the 0.8.0 bump `5b5b0ba`) |
| Stylus (pressure/tilt/proximity) | ✓ Linux XI2 + raw pressure; win32 dual-path = wave 3 |
| History panel | ✓ panel; time-travel = wave 1 |
| Distribution | ✓ shell+PS proven + CI; Flatpak/AppImage = wave 1 |
| Performance floor | seeded (`fd49627`) on a local Intel HD 530 / GL, NOT the nightly's lavapipe; the nightly has never run (no self-hosted runner, no fallback) — C2-14..16 |
| Paint-what-you-see buffer | live-raycast ✓; the pwys stroke buffer stays a design-first wave-5 candidate |
| Ortho camera | ✓ landed (40f3e5d) |
| 8K export | ✓ built (`cd668a3`) |
| Per-layer pixel planes | ✓ the pool arc complete (stages 1a-5, 2026-10-14) |
| Custom user shaders | runtime ✓; host hook = wave 6 |
| Rename layers | ✓ LayerCommand::Rename + the wire step (2026-10-14) |
| Dirty-check confirm | ✓ the File > New gate (2026-10-14) |

## The wave protocol (standing, REVISED 2026-10-14 — the two-gate model)

Bessa's ruling: big development waves, tests+CI after. Per-push CI is
retired. Gate 1 = scripts/wave-battery.sh per kill (fmt + clippy -D
warnings + full suites, local, the flock at /tmp/umber-cargo.lock).
Gate 2 = ONE push per wave + ONE CI run (scripts/ci-watch.sh backgrounded
with a notify pattern — verdicts ping between turns, nothing foreground-
sleeps). Kills batch as unpushed commits between gates. The 2026-10-12
per-kill-worktree variant is retired; the MAIN-tree kill-locks pattern
(claw dispatches on locked surfaces) replaced it. The old protocol's
steps, kept where still true:

1. Dispatch: claws on MAIN-tree locked surfaces (kill-locks.sh), staggered
   launches (60s+ apart — the opencode DB lock), briefs written fresh from
   live anchors. NO cargo of any kind for claws.
2. Consume: the dragon reads each kill's artifacts at exit (the report, the
   full diff, verify-kill) and runs Gate 1's battery; the dragon fixes what
   breaks.
3. Land: kills commit as they pass Gate 1; the wave's push + ONE CI run is
   Gate 2 (ci-watch.sh pings the verdict).
4. Roll: the next wave's briefs get written at landing (anchors shift);
   this roadmap's rows update their status in the same commit.
