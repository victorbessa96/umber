# Umber Long-Horizon Roadmap — the Substance-Parity March and Beyond

Version: 2026-10-12 (post-wave-1-dispatch). Supersedes the wave column in
requirements.md as the PLAN of record; requirements.md stays the CONTRACT of
record (each kill's Doc-delta updates its rows, this roadmap orders the work).

Method: full census of all 12 requirements sections, every row classified
done / partial-with-named-boundary / open, every open item dependency-ordered
into waves. Parity target: Substance Painter's feature set (the [01]
inventory), exceeded where [04] (next-gen bar) points.

---

## Where we stand (2026-10-12, post-core-loop)

The core loop is Gate-0-proven end to end: mesh → 4K pressured paint →
compositor → undo/redo → save/reopen → export, both CI platforms. The
2026-10-12 core-loop day closed 11 of 12 audit worklist items plus the
follow-up kills (per-channel tile pool, height→normal live end to end,
icon-raster, ID-mask, OCIO bundled build run, installers proven). The
Substance-parity march starts from a real engine, not a shell.

## Wave 1 — IN FLIGHT (4 claws, per-kill worktrees)

| Kill | Closes | Parity |
|---|---|---|
| wave-masks | LayerMask pixels + compositor gate (critical 6) | paintable per-layer masks |
| wave-blends | the ~32-mode census (HSV + normal-map families) | full pro blend set |
| wave-history | click-to-time-travel | the history panel Substance users expect |
| wave-dist | Flatpak + AppImage + packaging CI | real Linux distribution |

Landing order: dist → history → masks → blends (compositor shared: masks
first, blends rebases if needed).

## Wave 2 — dispatch on wave-1 landing (briefs written THEN; anchors shift)

| Kill | Closes | Depends on | Notes |
|---|---|---|---|
| wave-postfx-persist | §5 postfx session-only boundary (C1 audit 1c) | nothing | the chain is wired; add project persistence + panel round-trip |
| wave-ocio-panel | §9 "in panel": ACES 2.0 configs selectable in Display panel | ocio-bundled (✓ 60b04d0) | the real-mode APIs are live-verified; panel UI + config adoption into the display chain |
| wave-perchan-res | §2 per-CHANNEL resolution (the deferred half of the sizing row) | wave-1 (same file as masks/blends) | set_sizes gains the channel dimension; the tilepool's TileTargets already isolates per-channel targets |
| wave-pwys | §1 Mari-style paint buffer (camera moves mid-stroke) | wave-1 | the deepest architectural row: stroke-space buffer decoupled from camera; DESIGN DOC FIRST |
| wave-perf-baseline | §5 perf floor NOT-REACHABLE: baseline.json on the 4K path | the nightly run (runner-starved, #2) | consume the nightly's artifact, seed baseline.json, wire the compare gate |
| wave-icons-boot | §11 icon-raster NOT-REACHABLE: boot swaps IconAtlas::empty() → load_baked_atlas | icon-raster (✓ cb4d2bf) | one-line boot change + the atlas path resolution; closes the row's reachability clause |

## Wave 3 — the interchange + depth wave

| Kill | Closes | Notes |
|---|---|---|
| wave-psd-import | §6 designed-deferred: flatten-on-import semantics | the ag-psd reader exists; blend-mode/mask/group flattening is the honest-verification surface; lands AFTER wave-blends so the blend semantics match |
| wave-stylus-win32 | §10 the Windows dual-path: Wintab32 polling beside Windows Ink | winink.rs decode exists (passive); the WndProc hook + dual-path switch is the kill |
| wave-8k-export | §2/§6 the 8K export path | export-time supersampling from the 4K working set; VRAM note honored (8K never a working size) |
| wave-hsv-goldens | wave-blends' GPU family goldens (if CPU-oracle-only lands) | the compositor's per-family goldens |
| wave-camera-ortho | §5 the named camera gap: orthographic projection | the turntable orbit exists; add ortho mode + the projection toggle |

## Wave 4 — the differentiators (P1s that Substance lacks or does worse)

| Kill | Closes | Notes |
|---|---|---|
| wave-materialx-roundtrip | MaterialX import (export ✓ built) | consuming DCC graphs = the differentiator [04]; the declared-custom-nodedef contract already ships |
| wave-abr | .abr brush preset import (§1 optional) | the one Adobe format that is legal |
| wave-dynamic-materials | §2 dynamic material layering (P2, 6+) | shader-declared sub-stacks; the deepest remaining material feature; design doc + likely 2 kills |
| wave-wasm-shaders | §5 custom user shaders = WGSL plugins (P2) | umber-wasm runtime ✓ built; the shader-replacement host hook is the kill |
| wave-spp-import | §7 (NEW): .spp IMPORT — best-effort read of user-provided Adobe project files | import-only (no .spp writing ever); clean-room from published community HDF5-layout docs; per-project import report; RATIFIER: Bessa (legal scope); slots after PSD-import (shared flatten machinery) |
| wave-svg-import | §7 SVG (later) | vector assets as fill/procedural sources |
| wave-tessellation | §5 displacement/tessellation preview (P2) | the height channel exists (tilepool); the preview pass + the displacement budget |
| wave-hdr-surface | §5 HDR output (eframe upgrade gate) | tracks eframe 0.36→next; the capability model + probe ✓ built |
| wave-ray-query | §3 wgpu ray-query acceleration | tracks wgpu stability; the compute-shader path ✓ built |

## Wave 5 — the moonshots (P2, 2+ years, from [04])

| Kill | Closes |
|---|---|
| wave-upstream-stylus | the stylus crate → winit upstream (the [03] survey's gap) |
| wave-community-nodes | the node-graph community marketplace (plugin-sdk ✓ built) |
| wave-collab | merge-story → live session merge (per-layer files are the substrate) |

## Standing debts (ride every wave, not wave-gated)

- CI: the perf-nightly runner starvation (GitHub-hosted queue; 38234644651
  cancelled + re-queued 38304995746 — still waiting; nothing fixable
  workflow-side).
- The nightly's baseline artifact → #2's gate (wave-perf-baseline consumes).
- AssetStore GC (§8's v1 boundary: sidecars authoritative, no store GC).
- Split-redo under merged undo entries (#5's named follow-up).
- Merge below the top undo entry (#5's deferred arm).
- Cached ID-mask bake (multi-ID, cross-tile — the #11 follow-ups).
- The winink live-HWND integration question (its module doc's open item).
- DPI re-raster (2× atlas baked; runtime DPI change = named follow-up).

## Never (hard walls — requirements.md's guardrails, restated)

- .spp WRITING — undocumented HDF5 + Adobe's format. (IMPORT reversed 2026-10-12: reading user-provided .spp files is interoperability, wave 4, pending Bessa's ratification — see decisions.md.) The .umber directory format stays the kill-shot.
- .sbsar execution — Adobe Additional Terms, no FOSS license. The original
  node graph + MaterialX interchange is the legal answer, fully built.
- Fake interchange: MaterialX exports declared custom nodedefs; consuming
  DCCs evaluate what they support. No full-fidelity claim is ever made.

## The parity scoreboard (updated per landing; Substance → Umber)

| Substance capability | Umber state |
|---|---|
| Painting core (stamps, flow, spacing, jitter, alignment) | ✓ complete |
| MyPaint-style input curves + stabilizer | ✓ complete |
| Smudge/clone/symmetry/path/polygon/quick mask | ✓ complete |
| Seam-aware stamping + 3D-space strokes | ✓ complete |
| Texture sets + channel stacks + UDIM | ✓ + per-channel pool (2026-10-12) |
| Layers: types, instancing, anchors, smart materials | ✓ complete |
| Blend modes | 15/32 → wave-1 blends closes |
| Masks (paintable per-layer) | metadata-only → wave-1 masks closes |
| Baking (all 8 maps, cage, skew, auto-rebake) | ✓ complete |
| Node graph + MaterialX | ✓ complete (original engine) |
| Viewport (OpenPBR, IBL, view modes, postfx) | ✓ + height→normal live (2026-10-12); postfx persistence = wave 2 |
| Color management (OCIO, ACES, LUT) | ✓ bundled build + LUT; panel selection = wave 2 |
| Export (templates, packing, presets, EXR, PSD, USD, MaterialX) | ✓ PSD-out complete; PSD-in = wave 3 |
| Import (FBX, USD, images, mesh maps) | ✓ complete |
| Stylus (pressure/tilt/proximity) | ✓ Linux XI2 + raw pressure; win32 dual-path = wave 3 |
| History panel | ✓ panel; time-travel = wave 1 |
| Distribution | ✓ shell+PS proven + CI; Flatpak/AppImage = wave 1 |
| Performance floor | pending baseline (nightly starved) |
| Paint-what-you-see buffer | live-raycast ✓; full buffer = wave 2 |
| Ortho camera | ✗ open = wave 3 |
| 8K export | ✗ open = wave 3 |
| Dynamic materials | ✗ open = wave 4 |
| Custom user shaders | runtime ✓; host hook = wave 4 |

## The wave protocol (standing, revised 2026-10-12 — cargo ONLY at wave end)

1. Dispatch: 4-5 claws, per-kill worktrees (kill-worktree.sh), staggered
   launches (60s+ apart — the opencode DB lock), briefs written fresh at
   dispatch from live anchors. Briefs say: NO cargo of any kind, mid-wave or
   at exit — the wave-end battery runs everything.
2. Consume: the dragon reads each kill's artifacts at exit (the report, the
   full diff, brace-balance, verify-kill's zero-cargo legs) and stages the
   branch — but runs NO cargo.
3. Gate ONCE at wave end: fmt --all once; clippy once per touched crate; the
   touched crates' test suites once; the wave's kills' own tests in that
   same battery. Fix what breaks (the claws are gone; the dragon fixes at
   the battery).
4. Land in order: fast-forward each branch (rebase when diverged), CI per
   landed branch delivers the cross-platform verdict.
5. Roll: the next wave's briefs get written at landing (anchors shift);
   this roadmap's rows update their status in the same commit.
