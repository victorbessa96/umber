# Changelog

All notable changes to Umber are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); commits are
the source of truth.

## [Unreleased]

### Core-loop phase — CLOSED: Gate 0 PASSED (2026-10-12, both platforms)

The strategy review (docs/specs/2026-10-12-strategy-review.md) found the
engine crates real but the user-path joins missing; GUI/P2 frozen until
Gate 0 (the journey test) passes. The six criticals, in queue order:

- Strategy review + truth pass begins — six rows corrected (`d3516c6`)
- Viewport texture binding + per-layer compositor — criticals 1+6 (`e802f92`)
- App wires the viewport bind — paint reaches the screen (`9455885`)
- Stroke undo journal — critical 3; every stroke one entry, v1 clear() dies (`f78e6c1`)
- Compositor alpha is straight-alpha-over — CI's pin caught the divergent form (`d1aeeb3`)
- Pixel save/load — critical 4; painted maps persist through the AssetStore, reopen loses nothing (`d834d77`)
- Smudge test pins the stacking contract — 4-red streak root-caused: entries stack, one per stroke (`bed9e0a`)
- Paint target goes 4K, sized per texture set — critical 2; DEFAULT_TARGET_SIZE=4096, size combo, sidecar persistence (`133dd12`)
- Real pressure reaches the brush — critical 5; the pressured API pair + alpha shaping + can-fail test (`f867215`)
- The pressure source lives — raw `Event::Touch` per-event force; the multi_touch 2+ gate never carried a single pen (`6430623`)
- Gate 0 — the journey test, six legs joined: FBX → 4K pressured paint → compositor → undo/redo → save/reopen → ORM decode-verified; the compositor shader's isNan corpse dies on its first real compile (`15c072e`)
- Crash-safe paint saves — atomic manifest commit + AssetStore routing; reopen is old-or-new, never torn (`e4ad8b7`)
- Cumulative undo merge — overlapping strokes coalesce at push; the byte budget carries regions, not stroke counts (`37025d5`)
- The eyedropper — Pick on the strip, sample the mesh, paint with it (`7067cd3`)
- The XI2 stylus backend — pen pressure/tilt/proximity reach the brush on Linux; the claw's orphaned kill consumed and finished (`d2fd7b7`)
- Height→normal live in the lit viewport — a height stroke tilts the light per frame; the GPU-feature CI leg lands with it (`861ca9a`)
- Installers proven at the local boundary — real linux-gnu tarballs, shell installers, verified sha256s; the missing profile.dist fixed (`9f875b3`)
- The OCIO bundled build executed — the never-run C++ tree compiled, the ACES 2.0 built-ins materialized, and a false UI-name constant died on the real library's answer (`60b04d0`)
- ID-mask selection — click a region, paint only that region; the mask gate does the gating, the bake feeds the IDs (`e2bed70`)

### Wave 4 — painting completeness (2026-10-09/10)

The brush engine's professional floor: seam-aware stamping, presets,
the full baker surface, IBL, overlays, and the perf instrumentation.

- SeamGraph — UV-seam topology for seam-aware stamping (`fb984d1`)
- Seam mirror math — UV reflection across seam edges (`aca7f8d`)
- Seam-aware paint wiring — mirror dabs through push_event (`7106a79`)
- Median-split BVH — high-to-low transfer acceleration (`057f9cf`)
- Bake transfer support — cage lerp, distance clamps, match-by-name (`bb9ec90`)
- High→low transfer GPU pass — height + world-normal from a high mesh (`2cb7b08`)
- Brush preset data model — BrushPreset, ControlCurve, .umberbrush JSON (`f18a0ce`)
- Preset library loader — search paths, user-wins precedence (`0d49d83`)
- Brush properties panel — live editing, curve editor, save/save-as (`d58bdb4`)
- Starter brush library — twelve .umberbrush presets (`57ce764`)
- ID bake — FNV-hash region IDs from the position pass (`ab1dec5`)
- Bent normals — shared-sample AO accumulator with confidence (`60324a4`)
- Environment-map IBL — load-time convolve, mesh-pass integration (`8065908`)
- Wireframe + ground grid overlays (`09eb4e9`)
- Perf instrumentation — profiling spans, HUD, the stroke/undo soaks (`e4925a1`)

### Wave 5 — the node graph + multi-tile (2026-10-10)

The procedural center: the graph engine, MaterialX serialization,
the ~40-node set, UDIM, USD import, async bakes, the display panel.

- Painted-maps export bridge + full token sources (`b1aa05a`)
- Node-graph eval engine — typed values, registry, cached dirty eval (`c7185e2`)
- .mtlx serialization — declared custom nodedefs, byte-identical round-trip (`2a5c1be`)
- Node set — noise, gradients, patterns, filters, color ops, mask ops, spatial (`ddc62eb`, `8ec500c`, `e16a356`)
- Node-graph app bridge — Graph panel, export source, persistence (`abf04b6`)
- Tangent-space transfer normal — per-texel UV-derivative TBN (`388f661`)
- Specular IBL — cone-scaled prefilter + the Karis fit (`05a5f1c`)
- UDIM complete — tile math, model, bake windows, paint routing, export, UI (`82c694c`..`10cdffa`)
- USD .usda import — hand-parser loader, fan triangulation (`a17e1c8`)
- Async bake jobs — the UI no longer freezes (`7bf6140`)
- Display-transform panel — the viewer chain, persistence (`956ecdd`)

### Wave 6 — the named list closed (2026-10-10)

The graph canvas, CLI scripting, WASM plugins, installers, i18n,
and the AI automation surface — all six items in one morning.

- Graph canvas (noodle) editor — the visual graph surface (`ec0d9cc`)
- CLI batch mode — the JSON recipe driver (`a6d297f`)
- WASM plugin runtime — wasmtime bridge, guest SDK, three examples (`05f2e3c`)
- Plugin registry integration — the app-side loader, menu, mtlx declarations (`263df1f`)
- cargo-dist packaging — the release surface, measured 11 MB of 150 MB budget (`18e8c46`)
- i18n — fluent catalogs, pt-BR first target, the top-surface migration (`e493c60`)
- AI agent session — the JSONL automation surface (`7d2fcad`)

### The P1 tier closed (2026-10-10/11, the overnight)

Every P1 requirements row built, marked, or honestly boundary'd in one
overnight — ~30 kills across opencode, claude (opus + sonnet), and the
dragon's own authoring.

- Straight-line constraint + angle snap (`33b796f`) — the stroke filter, 9 tests
- Polygon fill (`1329e8d`) — the four pixel-mask fills, scanline even-odd, 10 tests
- History window (`9af4c2f`) — click-to-time-travel, the undo-jump semantics
- The smart layer tier (`4cbaf21`) — instancing with cycle rejection, anchors,
  smart materials + masks, three example presets ship in assets/smart/
- Paint-what-you-see (`b2306c6`) — the Screen projection, live raycast v1
  (the deferred Mari buffer = the named follow-up)
- USD export (`897e323`) — .usda + UsdShade/OpenPBR, the importer round-trip
  proven at runtime, 8/8 tests
- Bake mode UI (`3733e5e`) — skew correction (paintable), auto-rebake,
  the bake log with jump-to-setting, the cage viz
- Path tools (`cb3e435`) — the replay engine, per-vertex size/opacity, filled paths
- Mesh-map import (`00ee9d7`) — the folder-drop convention importer,
  every import a new layer
- The merge story (`a26d5e1`) — docs/merge-story.md: the five git-shaped
  properties + the trap-gate tooling (scripts/verify-kill.sh)
- Post-effects stack (`86e77ff`+`a523647`+`112b378`) — the chain model, the
  WGSL passes, the PostStack render-graph + panel; the mesh→HDR-texture
  wiring = the named follow-up (the panel says so)
- HDR surface output (`eaa776a`+`fb96668`) — the capability model with the
  eframe boundary PROVEN in-module (no format/colorspace hook in 0.36);
  the surface switch gates on an eframe upgrade
- Example-based texture synthesis (`b0dba14`) — the synth node: 3-level
  pyramid, neighborhood+coherence scoring, Tiled/RandomWalk/Inpaint,
  11 judgment calls stated, 7 tests

### The frontier — post-wave-6 (2026-10-10)

- 8K export — the audit, the presets, the end-to-end 8192² proof (`cd668a3`)
- PSD export — layered, the ag-psd round trip (`40909ea`)
- MaterialX document export — the OpenPBR texture-set document (`14d98fa`)
- .usdc + .usdz import — openusd 0.7.0, both arms, seven CI rounds to green (`19ccc42`..`f007ed1`)
- GPU display LUT — the chain lands on the pixels (`146d7a0`)
- Long-tail i18n — every panel string to the catalogs (`652d682`)
- Autosave journal — crash recovery, the idempotent replay (`015fbf0`)
- Nightly perf gate — the §12 regression workflow + compare script (`860b2e4`)
- Smudge + clone brushes — retouch ops, the write-back path, stroke history (`2fbfc7e`)
- Symmetry — mirror + radial across every consumer (`2689fa8`)
- Quick mask — the temporary painted gate (`e4492ba`)
- Perf harnesses — the reference-scene bench + input→photon session (`909ee0c`, `ae7e8f9`)

### The corrections (the honest record)

- `d565e64` — 14d98fa's .usdc claim was false; the sync-back race destroyed
  the work; re-made dragon-written (`19ccc42`). Pitfall 18 recorded.

### Engineering notes

- The remote-build flow (centralhost's 3070) with its pinned recipe; the
  `.git` rsync incident (Pitfall 16); the claw pattern library
  (limit→gate-the-tree, wedge→switch claws, four-claw failure→the fifth
  claw is the dragon).
- 599+ tests across the workspace; CI on Windows and Linux per commit;
  the seven-round usdc arc closed at runtime green.

## [Earlier] — Wave 3 (2026-10-09)

The bake & export pipeline: six mesh-map bakers, UV padding, engine
presets, the export driver, the headless CLI, and the app panels —
workspace 268 tests green, CI on Windows and Linux for every commit.

### Added — bake engine

- §6 default-color fill mode — fill_uncovered completes the padding row (`0694344`)
- §6 infinite dilation — dilate_map_filled, the full-coverage entry point (`a04e243`)
- §6 dithering option — Floyd-Steinberg at the f32->u8 boundary (`6edd477`)
- export feeds the real baked tangent normal (`d64a65c`)
- bake-all tangent-space normal — 6 maps, the full baker surface (`83009d7`)
- tangent-space normal bake — screen-space TBN from the position pair (`2d7fa0a`)
- --dilate N flag on bake-ao + bake-all — uniform seam-padding control (`f2c29ee`)
- bake-all world-space normal — 6 maps per invocation (`eff0a8f`)
- world-space normal bake — readback of the position pass's normal texture (`4b83a03`)
- Bakes panel in the app — bake engine exposed in the UI (`af8b31b`)
- bake-all dilation post-pass — seam-free AO export (`6b728ae`)
- UV-padding dilation — 8-neighbor nearest-donor ping-pong passes (`6df5824`)
- bake-all completes the P0 surface — thickness map wired (`f466e0d`)
- thickness bake — inward ray-cast min-distance; P0 baker set complete (`ed23d0b`)
- bake-all covers the full landed bake surface — curvature + position maps (`2335e00`)
- curvature bake — screen-space signed curvature from the position/normal pair (`f012420`)
- CLI bake-all — batch bake with TextureSetName_map convention naming (`5327294`)
- mesh-map naming convention — TextureSetName_map parse + format (`fd58791`)
- headless bake — umber-cli bake-ao: mesh → GPU AO bake → PNG (`74393f6`)
- UV→world position map + mesh AO — real bakes replace the parameter plane (`8dea62d`)
- AO bake engine slice — compute-shader raycast core (plane-bake proving the path) (`620847f`)

### Added — export pipeline

- §9 ICC profile embed — iCCP chunk via the png crate, no lcms2 (`1e3a139`)
- §9 ocio bridge — stub-gated built-in ACES 2.0 config surface (`6b1e1fe`)
- export feeds the real baked tangent normal (`d64a65c`)
- Export dialog in the app — presets callable from the UI (`51ae037`)
- CLI export command — presets callable headless, §10 path closed (`51f0295`)
- export driver — presets to files, the §6 pipeline glue (`5eb9cff`)
- format writers complete — TIFF, JPEG, 16-bit PNG (`73d8dc0`)
- engine presets complete — Unreal ORM, Unity HDRP/URP, Blender Principled (`a1490ad`)
- bake-all dilation post-pass — seam-free AO export (`6b728ae`)
- EXR float export — 32-bit float maps via the exr crate (`f3b6bd9`)
- export preset engine — packing, conventions, formats, glTF preset (`fafb4c7`)
- mesh-map naming convention — TextureSetName_map parse + format (`fd58791`)
- headless bake — umber-cli bake-ao: mesh → GPU AO bake → PNG (`74393f6`)

### Added — app & CLI

- .umber project I/O in the app — Open/Save Project in the File menu (`98e4c89`)
- bake-all tangent-space normal — 6 maps, the full baker surface (`83009d7`)
- --dilate N flag on bake-ao + bake-all — uniform seam-padding control (`f2c29ee`)
- Export dialog in the app — presets callable from the UI (`51ae037`)
- bake-all world-space normal — 6 maps per invocation (`eff0a8f`)
- CLI export command — presets callable headless, §10 path closed (`51f0295`)
- Bakes panel in the app — bake engine exposed in the UI (`af8b31b`)
- bake-all dilation post-pass — seam-free AO export (`6b728ae`)
- bake-all completes the P0 surface — thickness map wired (`f466e0d`)
- bake-all covers the full landed bake surface — curvature + position maps (`2335e00`)
- CLI bake-all — batch bake with TextureSetName_map convention naming (`5327294`)
- headless bake — umber-cli bake-ao: mesh → GPU AO bake → PNG (`74393f6`)

### Added — earlier waves

- wave-2-tail→3): 3D-viewport painting — Ctrl+drag ray-picks the mesh and paints at the hit UV (`708ae8a`)
- wave-2-tail): texture tile pool — lazy virtual-texture allocation over 512² PaintTarget tiles (`ce70c42`)
- wave-2-tail): color display transforms — CPU reference path (OCIO-lite) (`a30efe1`)
- wave-2): File → Export Painted Map (PNG) — paint target to deliverable file (`4e7bb39`)
- wave-2): OpenPBR Surface viewport shader — real-time subset, GGX specular + coat + metal lerp + emission (third-claw scope, orchestrator-landed) (`0071bb9`)
- wave-2): OpenPBR params foundation + display-orientation fix (claw salvage, post-504) (`bf208a0`)
- wave-2): PNG export — paint-target readback to file, with sRGB transfer (`c97e8cb`)
- wave-2): document UI — Layers + History panels on the real layer stack + undo journal (`e1321a1`)
- wave-2): live paint canvas in the UV view — paint target presented beneath the wireframe (`8a1c142`)
- wave-2): texture-display callback — paint target presented in-panel (quad pipeline + CallbackTrait mirror) (`089ee44`)
- wave-2): paint input path — UV-view strokes through conditioner+adapter into PaintThread (`a08af15`)
- wave-2): paint-thread slab — command channel + segment/dispatch orchestration (claude claw + nemotron review + orchestrator fixes) (`1dd4dff`)
- wave-2): dab adapter — brush stamps to paint dabs (the stroke→splat bridge, brush side) (`ec10258`)
- wave-2): GPU paint target + dab compositing compute pass — the paint core (claude claw + nemotron cross-review) (`8875362`)
- wave-2): 2D UV view panel — wireframe of loaded mesh's UV triangles on the 0..1 square (`8128dff`)
- wave-2): stroke conditioning (umber-brush) + layer stack/undo/.umber format (umber-core) (`579eb09`)
- wave-2): golden-image test harness — headless render target + tolerance comparison (umber-gpu) (`ef12bcc`)
- wave-1): stylus Windows Ink backend skeleton (claude claw) (`905a389`)
- wave-1): IBL environment lighting — hemispherical sky/ground irradiance in the viewport shader (`342e8f1`)
- wave-1): wgpu viewport render pass (claude claw + cross-review integration) (`93e0140`)
- wave-1): real glTF/GLB + FBX loaders (opencode claw + cross-review integration) (`fa393d2`)
- wave-1): 11-crate workspace skeleton — builds, 12 tests green, clippy+fmt clean (`ba9fecc`)

### Fixed

- wave-3): Open Project clears the undo journal with the stack swap (`bcb5983`)
- wave-3): bake-all --dilate writes each map with its base transfer (`b5a2e2d`)
- wave-2): adapt UV-view canvas call site to the simplified TextureDisplay::callback signature (`a0599c3`)
- wave-2): texture-display vertex stride — uv attribute offset 16→8 (`ac343df`)
- wave-2): request TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES on the eframe device (`c2dbf79`)
- wave-2): umber-core hardening round — path-traversal rejection + case-collision validation (claude claw, post-landing review) (`fc50afd`)
- clone URL in CONTRIBUTING (`6c8e309`)
- gitignore inline-comment bug (Cargo.lock pattern was a literal); real comments now (`b4a2da0`)
