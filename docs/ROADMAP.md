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

## Where we stand (2026-10-14, the per-layer pool era)

The core loop is Gate-0-proven end to end: mesh → 4K pressured paint →
compositor → undo/redo → save/reopen → export, both CI platforms.
2026-10-13/14 closed the importers wave (the .spp reader crate, PSD
flatten-on-import), the dynamic-materials arc end to end (design → hostile
review → r2 → headless Kill A → UI Kill B — per-channel material layers
live), the never-crash lint gate + panic-hunt fuzz (850 garbage cases, the
four public seams), the File > New dirty-check, the agent protocol v0 +
its five wire edit steps (projects, layers, undo over JSONL), the
offscreen snapshot harness (egui_kittest), the importer corpus harness
(arms on real .spp files), LayerCommand::Rename, and the per-layer pool's
first three stages (the compositor's premultiplied mode; the plane
storage + save v2 round-trip; the selection + stroke gate). Stage 4
(per-layer pixels) is in flight. The two-gate wave model (local battery
per kill, one CI run per wave) runs the march now.

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
| wave-camera-ortho | OPEN | the named camera gap — moves to the post-pool list |
| wave-stylus-win32 | OPEN (wave 5 now) | Wintab32 dual-path |
| wave-8k-export | OPEN (wave 5 now) | the 8K export path |

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
| 4 per-layer pixels (Kill-B-class) | IN FLIGHT | the claw hunts: strokes/fills/materials/imports on own planes, dormancy + v2 header [B2], the composite-on-read flush [M7], the perf gate [M8] |
| 5 anchors + instances | queued | SelfAnchor/ForwardAnchor, CPU prefix flatten |

Post-pool probe kills (the r2's open verifications): eframe 0.36's GPU
error-handler behavior; wgpu 30's error-scope API shape; PaintCompositor
Arc threading; un-premultiply exactness on GPU vendors.

## Wave 5 — the pull-users + depth wave (post-pool)

| Kill | Closes | Notes |
|---|---|---|
| wave-camera-ortho | §5 the named camera gap | ortho mode + the projection toggle; small, headless-testable |
| wave-stylus-win32 | §10 the Windows dual-path | Wintab32 polling beside Windows Ink |
| wave-8k-export | §2/§6 the 8K export path | the per-channel pool carries it; export-side budget rows |
| wave-materialx-roundtrip | MaterialX import (export ✓ built) | consuming DCC graphs = the differentiator [04]; the declared-custom-nodedef contract already ships |
| wave-agent-paint-ops | the agent protocol's wire paint steps | strokes/fills over the JSONL session (v0's named seam); pairs with the --script snapshot arm |
| wave-harness-script | the snapshot harness's --script arm | drive the agent session, snapshot after each step — visual regression over driven state |
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
| wave-fuzz-corpus | the panic-hunt's structured corpus | today: 850 synthetic garbage cases; next: byte-replays of the corpus harness's real files' structure (headers valid, bodies broken — the middle ground that finds real parser bugs) |
| wave-a11y-first-pass | keyboard navigation + focus states | the icon toolbar + panels need tab order, focus rings, shortcut discoverability — Substance's UI is dense but fully keyboard-driven; ours isn't yet |
| wave-i18n-audit | the pt-BR completeness beyond the test | the completeness test pins key parity; a native-speaker review pass (Bessa's call who) for the strings' actual quality |
| wave-crash-reporter | the never-crash doctrine's last mile | a structured crash handler that writes the last 10 journal ops + the frame state to a user-readable file — the bug report a user can attach without a debugger |
| wave-benchmark-public | the public benchmark story | a `umber-cli bench` subcommand: the bake/export/paint paths timed on a reference mesh, so community perf reports are comparable |

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
| Blend modes | ✓ 32-mode census landed (d861482) |
| Masks (paintable per-layer) | ✓ landed (770162f); per-plane masks ride the pool |
| Dynamic materials | ✓ LANDED end to end (Kill A + Kill B, 2026-10-14); per-layer pixels = the pool arc, stage 4 in flight |
| Agent/automation | ✓ protocol v0 + wire edit steps (projects, layers, undo over JSONL) |
| Headless testing | ✓ the offscreen snapshot harness + the never-crash fuzz (2026-10-14) |
| Importers (.spp/PSD) | ✓ both landed; the corpus harness arms on real .spp files |
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
| Paint-what-you-see buffer | live-raycast ✓; the pwys stroke buffer stays a design-first wave-5 candidate |
| Ortho camera | ✗ open = wave 5 (post-pool) |
| 8K export | ✗ open = wave 5 (post-pool) |
| Per-layer pixel planes | stage 4 in flight (the pool arc; 1a/1b/2/3 landed) |
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
