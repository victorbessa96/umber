# Umber Long-Horizon Roadmap — the Substance-Parity March and Beyond

Version: 2026-10-24 (the consolidation era). Supersedes the
2026-10-14 edition. requirements.md stays the CONTRACT of record
(each kill's Doc-delta updates its rows); this roadmap ORDERS the
work. The 2026-10-24 full-project audit
(`docs/loop/audit-2026-10-24-full-project.md` + its six lens
reports beside it) is the census this refresh draws from — every
row here re-verified against the live tree, not carried from
older docs.

Method: the audit's finding first — the project has crossed from
feature-march to consolidation. The core painter's path closed
10-15; the depth week since was quality on already-ticked rows.
The open risk is no longer "can a painter finish an asset" but
"what does the release gate need, and what stays gated on the
partner."

---

## Where we stand (2026-10-24)

**186,170 LOC across 16 crates, 2,251 test functions** (12.1/kLOC,
no crate below 7). The core loop is Gate-0-proven end to end:
mesh → 4K pressured paint → compositor → undo/redo → save/reopen →
export, both CI platforms, verified as recently as the 10-24 green
battery runs (umber-paint 178 + umber-app 851). The perf nightly is
GREEN on the pinned 24.04 image (run 40075780768, 10-24 08:51Z —
the stale-baseline class is dead, killed by `ec7ccb8`'s pin). The
requirements contract: 90 of 100 rows ticked, stable since 10-17.
The scorecard generator's `--check` passes.

The night of 10-23/24 landed the W-C4 runway's first four kills
(all battery-green + CI-green on the full matrix): the set-switch
glue (C2-9's headless half), the per-channel dab density (both
arms), the ID-mask pre-warm (the first-click hitch), and the
paint-tiles auto-map by material (the composite's per-set rows
line up at load). The stencil lane is fully closed (W-C3a/C3b +
FREEZE-4). The flatpak chain landed with the honest appstream-
compose revert. The two-gate wave model runs the march; the
battery's machine-fit defaults + the feature-on RUN leg hardened
it this same week.

## ✅ LANDED — the completed archive (2026-10-10 → 2026-10-24)

Every closed row of the march, consolidated. Proof cites the
commit or the live tree; details live in each era's kill reports
under `docs/loop/kill-reports/`.

### The engine core (the march's waves 1-3 + the pool arc)

| Kill | Proof |
|---|---|
| Per-layer paintable masks | 770162f + 21e1290 |
| The 32-mode blend census | d861482 + the HSV/normal-map family goldens |
| History panel + click-to-time-travel | the History panel's journey rows |
| Postfx persistence | f35cf48 |
| OCIO panel + ACES configs | 9a1a808 |
| Per-channel resolution end to end | f4e7524 |
| Icon-first toolbar + boot atlas | 40c754f |
| Perf baseline + the nightly's fixed invocation | fd49627 |
| .spp import (chunk-index fixed 10-16) | 30b966e + 0a5c20d + the b-tree fix; corpus-armed |
| PSD import (flatten-on-import) | a4bf81a |
| Dynamic materials end to end | 83aa242 → 7ecea60 → Kill A f85cb0a → Kill B 2c6497e |
| Ortho camera | 40f3e5d |
| 8K export | cd668a3 |
| The per-layer pool arc (stages 1a-5) | b1ceb2c, f2aff9f, ed64a58, dd6e1cd, ec6d552 |
| Distribution (Flatpak + AppImage proven) | the shell+PS+CI stack |

### The C2 close — the painter's path (2026-10-15, W-C2a..f)

Every channel paints and exports (C2-5/6); a quit guard + silent
Ctrl+S (C2-1/4); pixel autosave + recovery (C2-3); one undo
timeline with row travel (C2-7); a stack + undo + History +
channel list per texture set (C2-9). All same-day, all CI green.

### The depth arcs (2026-10-14 → 10-16)

| Kill | Proof |
|---|---|
| Bake margin: Adjacent Faces across seams | engine 3818da0; CLI bca634f + agent c237e4f; Bakes panel 741ec47 |
| Brush falloff curves + custom | d0309e1 + panel 9a47fa0 |
| Bitmap stamps + angle modes + jitter | 47d8fed + 487fd49 + 90ec048 (C2-11) |
| Front-face falloff | cabe283 + 9a47fa0 |
| Stroke methods end to end | ee2dce5 + 51d9047 |
| Stencil: paint-through screen mask | the full lane below |
| Brush cavity mask | 405e0e2 + 4bebc72 |
| GPU safe-start + backend fallback | a221a85 |
| Save completeness by exhaustive destructuring | 17900df |
| Release phases (alpha/beta/RC) + the agent `version` step | 2c19dcd |
| openusd 0.8 (the last importer abort) | 5b5b0ba |
| The mesh-importer panic hunt | 24 findings, 23→24 closed |
| Crash reporter core + last-10-ops ring | ddb59e0 (C2-2) |
| `umber-cli bench` public benchmark | landed 10-16 (the GPU/lavapipe timing lines owed to protocol.md) |
| The WASM plugin SDK authoring docs | b0c1aa0 |
| The procedural history panel k1 (wave-procedural-history's first half) | 2a84fb8 (10-23): graph_panel/history.rs, 651 lines + the .ftl keys + the kill report |
| The kittest harness + the panel snapshots | the G0 foundation |

### The stencil lane (W-C3a/C3b + FREEZE-4 — fully closed)

Engine + app 0955ac8; paint hook 3a8b4cb; panel wiring 6c95e26;
viewport shell + N 2bb8c60; set-switch re-stage dcc5e89; cross-tile
observer 45ad4e4; the tangent-wiring third site + its postfx wall
(CI-green); the C2-9/M-C3a panel seats; the 2026-10-12
heightnormal amendment (480ad62). The stencil session module moved
bin→lib with zero path churn (the W-C4 L1).

### The W-C4 runway's first kills (the night of 10-23/24)

| Kill | Proof |
|---|---|
| L1: the set-switch glue (C2-9's headless half) | b79f0c6 — umber_app::set_switch, the five-pin headless test |
| L2: per-channel dab density (both arms) | 306a502 — texels_per_uv_for_channel; the write-only field deleted |
| L3: ID-mask bake-on-tool-entry | 6430374 — prewarm_id_bake_cache; the first click hits |
| L5c: paint tiles auto-map by material | 0ec64bd — auto_assign_tiles_by_material at set_mesh's tail |
| R-B1: the selected-layers flatten (the census P0) | 2026-10-24: bake_layers + the panel button + the shell handler; battery GREEN 179+851; report in kill-reports |
| R-C1 v1: the Assets panel's library browser (the census's bigger P0) | 2026-10-24: the search + three categories over the existing engines + the compose-flow extraction; battery GREEN 851; report in kill-reports |

### The per-set masked bakes (the spec's stages — all landed)

§1+§2 triangle_materials + all three importers (a23bcac + the
gltf/fbx/obj attach); §3 raster half (the per-set submesh bakes,
bakes_panel.rs:638/1781); §3 ray half — the with_occluder split +
`OccluderScope` (0955ac8); §4 the per-set fan-out; §5 the
auto-mapping (0ec64bd). Remaining: only the OccluderScope PANEL
row + i18n keys (see the open waves).

### The tooling + machinery (the loop's own hardening)

wave-battery.sh (machine-fit defaults + the feature-on RUN leg);
ci-watch.sh; scorecard.py (+`--check`); the perf-nightly image pin
(ec7ccb8); pre-push.sh; verify-kill.sh + land-kill.sh; the kill
locks + the main-tree pattern; the brief-template; the
freeze-recovery close scripts; the loop restart machinery
(2026-10-24 04:05).

## The OPEN waves (dependency-ordered; each row names its gate)

### Ungated — the runway continues (the next kills)

| Kill | Closes | Notes |
|---|---|---|
| L6: the unified timeline | graph_panel/history.rs LANDED (2a84fb8, 10-23: the per-node procedural history panel k1, 651 lines + its .ftl keys) → the merge with the paint journal into one view | the graph-history kill 2; builds on the landed k1 |
| The OccluderScope panel row + i18n keys | L5(b): the enum landed with the ray half; the PANEL row + its keys remain | UNGATED 2026-10-24: the .ftl lock is LIFTED (the freeze-3 recovery landed; post-recovery commits add keys — 2a84fb8 added 12+12) |
| The AssetStore GC shell caller | the 10-16 engine's missing consumer | ESTIMATE CORRECTED 2026-10-24: NOT the five-minute row — the engine's `live` set (`HashSet<(hash, ext)>`) must be enumerated from the store's CONSUMERS, and the core model carries NO AssetRefs (the mesh-map import holds them in session state, mesh_map_import.rs:319's `pub asset`); the reopen flow's ref-reconstruction (the journal/sidecar shapes) must be derived first. The call site stays simple (post-save, report-only, the GcReport log line); the live-set builder is the real kill. Derive before cutting. |
| L2b: the four same-class density arms | the symmetry snapshot (:1753), clone offset (:1812), fill raster (:4199), seam mirror radius (:4769) | mapped in the L2 report; each migrates to texels_per_uv_for_channel |
| L3b: the multi-entry ID cache | the multi-tile pre-warm | the L3 report's named follow-up |
| Quick mask persistence | the session-only mask (quick_mask.rs:122-360) saves + undos | lens A's gap 1: the ftl's own string admits "not saved and not undoable" — BUT the module docs own "temporary BY DEFINITION"; the Substance-contrast makes this a DESIGN question (SPP persists it), not a mechanical row — argue when asked |
| Symmetry persistence | **LANDED 2026-10-24**: the `.umber` carries it (core's `ProjectSymmetry` mirror + the app's boundary conversions + the additive-carry rule; the `.umberbrush` half = the documented non-goal — a preset is a brush shape, not a tool state); report in kill-reports | lens A's gap 2 closed; the quick mask's design question stays the open half of the pair |
| wave-a11y-first-pass | keyboard navigation + focus states | G7's core; Substance's UI is fully keyboard-driven, ours isn't yet |
| **The Assets panel (R-C1, P0)** | **LANDED 2026-10-24 (the v1 discoverability layer)**: the library browser (search + Brushes/Materials/Masks) over the existing engines; the `select_preset_by_name` seam; the smart-mask compose flow extracted (one path, two doors); the placeholder dead; report `docs/loop/kill-reports/2026-10-24-rc1-assets-panel.md`. The named follow-ups: drag-to-stack, thumbnails, saved searches, custom dirs, the filter/mesh/env categories. |
| **Layer-stack flatten (R-B1, P0)** | ~~group → baked fill layer~~ → LANDED 2026-10-24 as the SELECTED-LAYERS flatten: `bake_layers` (the isolated fold) + the Layers panel button + the shell's handler; fully undoable (tombstones + journal reverts); report `docs/loop/kill-reports/2026-10-24-rb1-flatten.md`. The folder kind + the compound one-step = the named follow-ups. |
| Material picker (R-A10) | **LANDED 2026-10-24 (the v1)**: Ctrl+Alt+click picks the ACTIVE channel's composite texel at the cursor into the selected fill layer's Solid (the inspector's mirrored-row convention, the panel's own journalable SetFill path; the status-bar hint in both catalogs); report in kill-reports. The two landed halves: the channel target (0f55ebd) + the pick gesture. Follow-ups: the all-channels mode, the e2e kittest, the pick toast. | the census's "small" row grew a prerequisite + this v1 across two kills; both banked |
| Projection fill (R-A2) | screen-space material projection | the decal workflow's other half; pairs with the landed stencil; rides the pwys design |
| Geometry masks (R-B2) | per-mesh/per-tile include/exclude on layers/effects | rides UDIM |
| Baker sync (R-D1) + channel manage (R-B4's tail) + the four post nodes (R-E5) | the power-user tail | each small; census-verified open |
| wave-fuzz-corpus (b) | coverage-guided fuzzing | cargo-fuzz + nightly + a corpus budget |
| wave-abr | .abr brush preset import | the one legal Adobe format |
| wave-svg-import | SVG vector assets | the wave-5 row, still open |
| wave-replay-exact | path replays paint exactly | a UX call (default-off until decided) |
| Split-redo + merge-below-top | the undo journal's two named follow-ups | retouch.rs:21's debts |
| wave-procedural-history | graph topology edits as undoable commands | Wave 9 |
| wave-smart-material-params | exposed parameter UI | Wave 9 |
| wave-community-templates | 10 curated .umber templates | Wave 9 — the fastest show-off |

### Gated — named walls, no derivation until the gate opens

| Lane | The gate |
|---|---|
| L4: winink live hook | S1's parked bytes (winink.rs) are NOT in the tree — the lane cannot derive until they land |
| L7: the color_button delegate conversion | G1-gated |
| wave-stylus-win32 | the Windows dual-path; real-pen verification = D1's Windows leg (partner-gated) |
| wave-wasm-shaders / tessellation / hdr / ray-query | the eframe/wgpu upgrade postures (the probes built) |
| The .spp staged import R1-R4 | the fidelity report's staged lanes (the formats lens reports which are landed) |

### Partner-gated — his calls, queued and named

The release decisions (C2-16, 22, 23-25, 30); the SPEC v1.1
ratification packet; the release checklist walkthrough; the
upstream bug drafts (docs/upstream/); the i18n native-speaker
pass; the .ftl lock lift (if it stands).

## Standing debts (refreshed 2026-10-24, live-verified)

- ~~The cached ID-mask bake~~ **LANDED SILENTLY** — the P1 tile
  cache + the L3 pre-warm; this row was stale for a week.
- Split-redo under merged undo entries — OPEN.
- Merge below the top undo entry — OPEN.
- The winink live-HWND question — OPEN, now deeper-gated (the
  bytes are absent).
- DPI runtime re-raster — OPEN (the manual ui_scale exists;
  the runtime-change re-raster does not).
- The AssetStore GC's shell caller — OPEN (engine landed 10-16,
  caller still queued).
- Stamp images persisted with presets — OPEN (the 6c row).

## Never (hard walls — restated)

- .spp WRITING (import is ratified + landed; the .umber directory
  format is the kill-shot).
- .sbsar execution (Adobe Additional Terms; the node graph +
  MaterialX interchange is the legal answer, fully built).
- Fake interchange: MaterialX exports declared custom nodedefs;
  no full-fidelity claim is ever made.

## The parity scoreboard (2026-10-24 — from the SPP lens, live-verified)

| SPP family | Verdict |
|---|---|
| Painting core (stamps/flow/spacing/falloff/jitter/angle/methods/stabilizers) | PARITY |
| Stencil/projection | PARITY on stencil (the lane closed); projection fill GAP (R-A2) |
| Retouch + masks (quick/cavity/ID) | PARITY (persistence gaps on quick mask + symmetry) |
| Path tools | PARITY (paint-along + filled + polygon + straight-line); ribbon GAP |
| Fill/material layers + anchors + instances | PARITY (materials composite-only by design) |
| Texture sets / channels / UDIM | PARITY (the auto-assign landed; channel add/remove/rename open) |
| Baking | PARITY (per-set masked bakes closed; D1-D5 power rows open) |
| Viewport | PARITY (four post nodes + displacement preview open) |
| Color management | PARITY |
| Export/Import | PARITY (the .spp v2 staged lanes; text unsupported) |
| History/undo | PARITY (the unified timeline = L6) |
| UI/UX density | PARTIAL (36/43 GUI kills; the color picker + keyboard-only are the felt gaps) |
| Assets/ecosystem | **GAP — the moat** (the Assets panel + starter content) |
| Scripting/automation | **EXCEEDS** (headless-first, versioned JSONL) + the client-library gap |
| Interchange (MaterialX, diffable format) | **EXCEEDS** |
| Community/local-first/no-telemetry/never-crash | **EXCEEDS** |

The census-delta arithmetic: Tier-1's 10 rows — 2 closed this week
(stencil, filled path); 8 remain, led by the two P0s above. The
full comparison: `docs/loop/audit-2026-10-24-lens-spp.md`.

## The wave protocol (standing, the two-gate model)

Bessa's ruling: big development waves, tests+CI after. Gate 1 =
scripts/wave-battery.sh per kill (fmt + clippy -D warnings + full
suites, local, the flock at /tmp/umber-cargo.lock, the machine-fit
defaults). Gate 2 = ONE push per wave + ONE CI run
(scripts/ci-watch.sh backgrounded — verdicts ping between turns,
nothing foreground-sleeps). Kills batch as unpushed commits between
gates. Briefs are written fresh from live anchors at dispatch
time; kill reports + the touched docs' rows update in the same
commit as the kill.
