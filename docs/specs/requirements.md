# Umber — Requirements

> Wave-0 consolidation of the research corpus (docs/research/01–05). This is the component-and-capability contract that drives the wave plan and the eventual per-subsystem specs. Provenance: every section cites its research source. Priority tags map to waves.

**Sources:** [01] Substance Painter inventory · [02] competitor landscape · [03] Rust ecosystem survey · [04] next-gen capability bar · [05] brush-engine architecture.

**Priorities:** P0 = professional floor (cannot ship v0.1 without) · P1 = differentiator (wins users, post-0.1 but on the roadmap) · P2 = moonshot (2+ years). Wave tags show where each lands.

---

## 1. Painting tools (brush engine) — [01][05]

| Requirement | Priority | Wave |
|---|---|---|
| Stamp-based brush engine: size, flow (per-stamp, pressure-bindable), stroke opacity (per-stroke, end-of-stroke), spacing (dabs-per-radius, residual partial-dab carry), angle, alignment (camera/tangent-wrap/tangent-planar/UV), jitter quartet (size/flow/angle/position) | P0 | 2 |  ✓ built (`579eb09` conditioning/spacing carry; `d58bdb4`-`57ce764` preset surface; jitter+alignment params in BrushParams) —
| Pressure dynamics mapped through input→curve model (MyPaint-style); log-radius; `opaque_linearize` flow semantics (per-stamp flow with per-stroke accumulation = Krita wash-mode; Build-up mode as alternative) | P0 | 2 | ✓ built (`1c730da`) — WashMode { BuildUp, Wash }, the opaque_linearize math, the preset round-trip |
| Lazy mouse (radius-based stroke offset smoothing) + one-euro filter smoothing (Casiez CHI'12) with Krita-style stabilizer panel (samples@min/max speed, delay dead-zone) | P0 | 2 | ✓ built (lazy_mouse.rs, the stabilizer panel rows) |
| Hardness/stamp falloff (radial gradient dabs); alpha source: bitmap textures + procedural | P0 | 2 |  ✓ built (umber-brush radial falloff + umber-graph procedural sources) —
| Eraser (sets layer alpha to zero — non-destructive; per-channel enable) | P0 | 2 | ✓ built (`21e1290`) — v1 boundary: seam-mirroring skipped (the retouch gate) |
| Fill layer + fill projections: UV, Tri-planar, Planar, Spherical, Cylindrical | P0 | 3 (fill layer ships with its projections — no projection-less staging) | ✓ built (`12124f3`) — the five projections, WriteRegion path, panel, 12 tests |
| Smudge (sample+blur blend), Clone (source offset + follow-stroke) | P1 | 3 | ✓ built (`2fbfc7e`) |
| Symmetry: mirror (axis+offset+manipulator) and radial (count, flip U/V) — brush + fill-layer projections | P1 | 3 | ✓ built (`2689fa8`) |
| Straight-line constraint; angle snap | P1 | 3 | ✓ built (`33b796f`) |
| Polygon fill (triangle/polygon/mesh/UV-chunk pixel-mask fills) | P1 | 3 | ✓ built (`1329e8d`) |
| Quick mask (temporary painted mask, invert) | P1 | 4 | ✓ built (`e4492ba`) |
| Path tools (paint/erase/smudge along 3D curve; per-vertex size+opacity; filled path) | P1 | 5 | ✓ built (`cb3e435`) |
| Brush presets (.abr import optional; native preset files) | P0 | 2 | ✓ built (`f18a0ce`/`0d49d83`/`57ce764`) |
| Seam-aware stamping: strokes crossing UV seams land consistently in both islands; 3D-space-neighbor padding/dilation | P0 | 2 | ✓ built (`fb984d1`..`7106a79`) |
| Stroke evaluated in 3D space, not per-tile (works single-tile; UDIM cross-tile continuity arrives with UDIM in Wave 4) | P0 | 2 | ✓ built (`708ae8a`, UDIM `82c694c`..`10cdffa`) |
| Paint-what-you-see projection modes beyond UV (Mari-style paint buffer) | P1 | 4 | ✓ built (`b2306c6`) — v1 live-raycast; the deferred Mari buffer = the named follow-up (the camera can't move mid-stroke today) |

## 2. Layer + channel system — [01]

| Requirement | Priority | Wave |
|---|---|---|
| Texture sets (per material/mesh UV set); per-set channel stack: baseColor, roughness, metallic, normal, height, emissive, opacity, AO + user channels | P0 | 1 (model), 2 (full) |  ✓ built (`7d5251f` tile-aware TextureSet + `579eb09` channel stack) —
| Layer types: paint layer, fill layer; folders with passthrough | P0 | 2 |  ✓ built (`579eb09` stack + `12124f3`/`21e1290` fill/eraser kinds) —
| Per-channel blending mode + opacity; blending computed in linear space per channel | P0 | 2 |  ✓ built (`21e1290` core-12 math + blend census 15) —
| Blending modes: full pro set (~32: normal, passthrough, multiply, screen, overlay family, HSV modes, normal-map combine/detail) | P0 core 12, full set 4 | 2/4 | ✓ core-12 math verified + Darken/Lighten/Subtract/Difference kept = 15 built (14 separable + Passthrough; `21e1290`); HSV + normal-map families deferred (Wave 4 compositor scope); the remainder open |
| Masks: grayscale paint mask, bitmap mask, black/white; mask stacks with effects | P0 | 2 | ✗-as-marked: METADATA ONLY (LayerMask in layers.rs); no mask pixels exist behind it — the review's critical 6 territory; part of the channel-pool work | ✓ built (`PENDING`) — selection-time ID mask only: click → `umber_bake::id` bake → `umber_brush::id_mask::select_id` fills the active set's QuickMask (1.0 on the clicked ID) and arms the existing gate (`PaintState::bake_and_select_id`); LayerMask stays metadata-only — v1 boundary: click-time full-size bake (no cached store), single tile, single ID |
| Layer instancing (source-edits propagate, cycle detection) | P1 | 4 | ✓ built (`4cbaf21`) |
| Anchors (reference any layer/channel/mask as generator input within same texture set) | P1 | 4 | ✓ built (`4cbaf21`) |
| Per-channel resolution; in-app 4K default (8K export → §6, single source of truth) | P0 | 2 |  ✓ code (`133dd12`): DEFAULT_TARGET_SIZE=4096 (paint_state.rs:266), per-texture-set size via the Texture Sets panel's SIZE_PRESETS combo, persisted in the project; the 8K-only VRAM note preserved; per-set/per-channel sizes = deferred v1 boundary (module docs) — reachability = Gate 0; per-SET sizes ✓ built (`PENDING`) — each set owns its size in the paint session + `paint/target_sizes.txt` (v1 global migrates onto every set), per-CHANNEL sizes stay the boundary (paint_state module docs) |
| UDIM multi-tile texture sets; per-tile resolution; cross-tile painting | P0 | 4 (data model from 1) | ✓ built (`82c694c`..`10cdffa`) |
| Smart materials (folder presets applied with mesh-map-driven generators) | P1 | 4 | ✓ built (`4cbaf21`) — worn-metal + painted-wood examples ship |
| Dynamic material layering (shader-declared sub-stacks) | P2 | 6+ |
| Storage formats per channel: sRGB8/RGB16/RGB16F/RGB32F | P0 | 2 |  ✓ built (`579eb09` tile store formats) —

## 3. Baking engine — [01][04]

| Requirement | Priority | Wave |
|---|---|---|
| Bakers: AO, normal (mesh + high→low), curvature, position, thickness, ID (material/vertex/mesh), height, bent normals, opacity | P0 (AO/normal/curvature/position/ID first) | 3 |  ✓ built (`620847f`→`ed23d0b` the P0 baker set; `83099d7` bake-all tangent normal) —
| Common settings: output size, dilation width, antialiasing (supersampling), ignore backfaces, low-as-high toggle, match by mesh name | P0 | 3 |  ✓ built (`af8b31b` settings rows; dilation `6df5824`+`a04e243`) —
| Cage: distance-based + custom offset; front/back projection distance | P0 | 3 |  ✓ built (`3733e5e` cage viz + settings; probe distances in `620847f`) —
| Compute-shader raycast/bvh bake path (no RT hardware dependency); wgpu ray-query acceleration when stable | P0 | 3 |  ✓ built (`620847f` compute-shader raycast core; ray-query deferred to wgpu stability) —
| Skew correction: paintable skew map + edge protection (Marmoset-class bake polish [02]) | P1 | 5 | ✓ built (`3733e5e`) |
| Auto-rebake of affected regions on parameter change | P1 | 5 | ✓ built (`3733e5e`) |
| Baking mode UI: per-set/per-map checklist, bake log with jump-to-setting, cage visualization | P1 | 3 (log) / 5 (viz) | ✓ built (`3733e5e`) |

## 4. Procedural system (original — .sbsar is legally untouchable) — [01][04]

| Requirement | Priority | Wave |
|---|---|---|
| Original node-graph engine: DAG, topological eval, dirty-region propagation; graphs serialize as **MaterialX documents** — nodes with MaterialX standard-node equivalents map to standard nodes; painter-specific nodes ship as **declared custom nodedefs** (consuming DCCs read the document and evaluate what they support; no automatic full-fidelity interchange is claimed) | P0 | 4 | ✓ built (`c7185e2` DAG/topo/dirty eval; `2a5c1be` MaterialX serialization with declared custom nodedefs; `8590ae2` GPU arm) —
| Node set v1 (~40 core): noise family (perlin/value/worley), gradients, patterns, blur/sharpen, levels, curves, color ops, direction warp, flood fill from masks, edge detect, histogram ops | P0 | 4 |  ✓ built (`c7185e2`→`2a5c1be` engine+mtlx+node set; `8590ae2` GPU arm) —
| Generator nodes driven by baked mesh maps (AO/curvature/position/WS-normal inputs plumbed automatically) | P0 | 4 |  ✓ built (`abf04b6` externals + `b2306c6` live raycast sources) —
| Smart masks (effect-stack presets) | P1 | 4 | ✓ built (`4cbaf21`) — the edge-wear example ships |
| Example-based texture synthesis (EmbarkStudios/texture-synthesis style) as first-class generator primitive | P1 | 4 | ✓ built (`b0dba14`) — the synth node: 3-level pyramid, neighborhood+coherence scoring, Tiled/RandomWalk/Inpaint, 11 judgment calls stated in-module, 7 tests |
| Graph evaluated on GPU (WGSL compute passes per dirty tile) | P0 | 4 | ✓ built (`8590ae2`) — GraphGpuEvaluator, 20 entry points, total hybrid routing (the per-pixel families GPU; the globals/assets/guests CPU through the same registry) |
| Community extension API for nodes (see plugins §10) | P1 | 6 |  ✓ built (umber-plugin-sdk: umber_node/Image/Params surface; the wasmtime host in umber-wasm) —

## 5. Viewport + shading — [01][03][04]

| Requirement | Priority | Wave |
|---|---|---|
| PBR viewport: **OpenPBR 1.1 über-shader in WGSL** (native channel model = OpenPBR parameter names; spec is ASWF/Apache-2.0 with reference C++ to transliterate) | P0 | 1 (basic Blinn-Phong-ish PBR), 2 (OpenPBR + conformance suite) |  ✓ code, REACHABLE: the über-shader exists AND the mesh pass samples the bound paint tile (UV attr `renderer.rs:37`, `paint_tex` sampling `shaders.rs:135/202`, app bind `viewport.rs:401` — critical 1 closed by B1, `e802f92`+`9455885`); paint is visible in 3D, Gate-0-verified end to end (`15c072e`, CI run 38255484123 both platforms) — the C1 audit's 1b re-mark: the old NOT-REACHABLE clause was falsified by the core-loop work |
| IBL environment lighting (HDR env maps, exposure, rotation, blur) | P0 | 1 (bundled env maps; HDR file import lands W2 with image import) |  ✓ built (`342e8f1` hemispherical; `8065908` env-map convolve; `05a5f1c` specular prefilter) —
| View modes: lit / unlit single-channel solo (±display transform) / mesh-map preview | P0 | 1 | ✓ built (`f1159d1`) — v1 boundary: solo/preview render from bound data (texture bindings follow-up); audit worklist #12 (height→normal live) partly closes that follow-up for Lit — ✓ code, golden-pinned (`PENDING`): a real height texture (mesh-pass binding 12, gated `PaintParams::HAS_HEIGHT_TEXTURE`) perturbs the Lit normal per frame via a screen-space surface-gradient (`shaders.rs::height_perturbed_normal`, `renderer.rs::set_height_texture`/`MeshBuffers::write_paint_params`, golden tests `height_stroke_perturbs_the_lit_normal` + the flag-off regression). v1 boundary: world-space/no-TBN gradient (`dpdx`/`dpdy` of interpolated position, correct per-facet, not across a curved mesh — true per-vertex tangents are the named follow-up); Solo/Preview's height arm is untouched (still mid-gray — see `view_mode.rs`'s doc); NOT REACHABLE from the app — `PaintState` has no per-channel paint target to bind from (`viewport.rs::bind_paint_target`'s doc), so this is checked-and-confirmed-blocked, not wired. See `docs/loop/kill-reports/2026-10-12-height-normal.md`. |
| 2D UV view alongside 3D view (synchronized) | P0 | 2 |  ✓ built (`8128dff` panel + `8a1c142` live paint canvas) —
| Camera: orbit/pan/zoom, perspective + orthographic | P0 | 1 |  ✓ code (`93e0140` turntable orbit only — ORTHO UNBUILT; the review's named gap)
| Wireframe overlay; grid | P1 | 2 | ✓ built (`09eb4e9`) |
| Displacement/tessellation preview | P2 | 6+ |
| Post-effects stack (DoF, bloom, vignette, sharpen, tone-map) | P1 | 5 | ✓ built (`86e77ff`+`a523647`+`112b378`) — the model + WGSL + PostStack + panel; the mesh pass → HDR texture → PostStack → LUT wiring LANDED (`postfx_viewport.rs:6-13`, viewport.rs:266-324); v1 boundary: the chain is session-only (not project-persisted yet) — the C1 audit's 1c re-mark: the spent 'next-step wiring' clause replaced with the wiring's actual state |
| HDR surface output (wgpu 30 SurfaceColorSpace; DX12/Metal, Vulkan driver-dependent) | P1 | 5 | ✓ built (`eaa776a`) — the capability model + probe + panel, with the honest v1 boundary PROVEN in-module: eframe 0.36 exposes no format/colorspace hook (preferred_framebuffer_format hardcodes 8-bit SDR); the surface switch gates on an eframe upgrade |
| Custom user shaders (WGSL plugins replacing GLSL import) | P2 | 6 |
| Performance floor: 60fps viewport on mid-range GPU with 1M-tri mesh + 4K texture set | P0 | 1 |  ✓ code (`e4925a1` facade+HUD+soak; `input_photon.rs` protocol) — NOT REACHABLE: perf/baseline.json absent, and the pre-core-loop baselines ran at 512² so they measured the wrong path; re-baseline on the 4096-default path = worklist #2 |

## 6. Export pipeline — [01][04]

| Requirement | Priority | Wave |
|---|---|---|
| Template-driven export: naming tokens ($mesh/$textureSet/$udim/$colorSpace/$srcMap/$layerName), folder mapping, saved-as-files presets | P0 | 3 |  ✓ built (`51ae037` dialog + `1cf058a` $udim token) —
| Channel packing (ORM etc.); per-output RGB/R/G/B/A/grayscale slot mapping | P0 | 3 |  ✓ built (`a1490ad` ORM packing) —
| Engine presets: glTF metal-rough, Unreal (packed, DX normals), Unity (HDRP/URP, GL normals), Blender Principled + the long tail as community presets | P0 first 4 | 3 |  ✓ built (`a1490ad` the four presets) —
| Bit depths 8/16/32F; formats: PNG, EXR, TIFF, JPEG (via image/exr crates); dithering option | P0 | 3 |  ✓ built (PNG/EXR via image+exr; dithering closed per `08cf3ab`) —
| Normal-convention conversion (DirectX vs OpenGL Y-flip) at export | P0 | 3 |  ✓ built (`a1490ad` DX/GL per preset) —
| Padding: dilation (finite/infinite), transparent/default-color fill; 3D-neighbor aware | P0 | 3 |  ✓ built (`6df5824` UV-padding + `a04e243` infinite) —
| 8K export (up-sampled from 4K in-app) — canonical row for this requirement (also referenced in §2) | P1 | 5 |  ✓ built (`cd668a3`) —
| PSD export (layered) | P2 | 6+ |  ✓ built (`40909ea` ag-psd round trip) —
| PSD import (flatten-on-import; layer maps to texture-set layers) — DESIGNED, deferred: the ag-psd reader exists, but flattening semantics (blend modes, masks, groups) are a large honest-verification surface; deferred until a user need arrives, NOT a technical wall | P3 (designed) | later |
| MaterialX document export (OpenPBR nodedef + image/UDIM tokens) — "beyond raster" interchange | P0 | 3 (basic), 4 (full) |  ✓ built (`14d98fa` + `897e323` mtlx+USD) —
| USD export (textures + .usda + material binding via UsdShade→OpenPBR) | P1 | 5 | ✓ built (`897e323`) — the importer round-trip proven at runtime, 8/8 tests |

## 7. Import — [01][03]

| Requirement | Priority | Wave |
|---|---|---|
| glTF/GLB (gltf crate), OBJ (tobj), PLY | P0 | 1 |  ✓ built (`fa393d2` glTF/GLB+FBX; OBJ/PLY in umber-mesh) —
| FBX via ufbx (MIT C lib, official Rust bindings — the only credible route; Autodesk FBX SDK is non-redistributable) | P0 | 1 |  ✓ built (`fa393d2`) —
| USD import (openusd pure-Rust: usda first, usdc when stable) | P1 | 5 |  ✓ built (`a17e1c8` usda hand-parser + `19ccc42` usdc/usdz) —
| Image formats: png/jpeg/tiff/exr/hdr (image + exr crates); SVG (later) | P0 (png/exr) | 2 |  ✓ built (png/exr first-class via image+exr; SVG later holds) —
| Mesh-map import by naming convention (TextureSetName_map) | P1 | 3 | ✓ built (`00ee9d7`) — the folder-drop importer, every import a new layer, the unmatched report |
| **Never:** .spp (undocumented HDF5), .sbsar execution (Adobe Developer Additional Terms — closed, no FOSS license exists) | hard wall | — |

## 8. Project format (.umber — the .spp kill-shot) — [04]

| Requirement | Priority | Wave |
|---|---|---|
| Project = directory; `project.json` (scene, texture sets, settings) + per-layer JSON files (diffable, mergeable) | P0 | 2 |  ✓ code (`579eb09` .umber format + `015bf0` journal + `d834d77` the paint sidecars); reachability = Gate 0
| Textures as content-hash-named PNG/EXR tiles (git/LFS friendly; assets referenced by hash, never embedded blobs) | P0 | 2 |  ✓ code (AssetStore round-trip tested in isolation); paint saves route every sidecar artifact through the AssetStore (content-hash blobs + `paint/manifest.txt` provenance) under a crash-safe manifest commit — ✓ built (`PENDING`) — v1: the sidecar files stay authoritative for reopen; no store GC
| Save/load round-trip determinism (CI-tested: save→load→export identical bytes) | P0 | 2 | ✓ code (`d834d77`): paint pixels persist as PNG sidecars (tile-1001 v1, Linear transfer, WriteRegion reload; the GPU round-trip test rides CI/lavapipe; AssetStore routing + atomic commit ✓ built (`PENDING`), old-or-new reopen pinned by a fault-injection test) — reachability = Gate 0 |
| Autosave + crash recovery (journal) | P1 | 5 | ✓ built (`015fbf0`) |
| Merge story documented (per-layer files = two artists editing different layers never conflict) | P1 | 2 (doc) | ✓ built (`a26d5e1`) — docs/merge-story.md: the five properties, the workflow, the honest limit |

## 9. Color management — [01][03][04]

| Requirement | Priority | Wave |
|---|---|---|
| Scene-linear working space, OCIO v2 configs (ocio-rs, vendored build) incl. ACES 2.0 CG/Studio built-ins | P0 | 2 (viewport), 3 (full configs) |
| Per-channel color-managed flags (baseColor yes; roughness/metallic/normal = data) | P0 | 2 |  ✓ built (`21e1290` census's per-channel flags) —
| ICC profile embed on export (lcms2) | P1 | 3 | ✓ built (`1e3a139` the iCCP writer — png crate writes the chunk natively, NO lcms2 needed; `driver.rs` wires it: every color-managed PNG8 embeds the bundled sRGB profile, data maps stay unprofiled per the §9 rule; 80/80 export tests) |
| Display transform via GPU LUT (OCIO GPU shader extraction → 3D texture) | P0 | 2 |  ✓ built (`146d7a0` the GPU display LUT on the pixels; `956ecdd` panel) —

## 10. Platform, input, plugins, distribution — [03]

| Requirement | Priority | Wave |
|---|---|---|
| Windows 10/11 + Linux (Vulkan 1.3; DX12 on Windows) | P0 | 1 |  ✓ built (CI both platforms green through the era) —
| CI on GitHub Actions: windows-latest + ubuntu; headless GPU tests via **lavapipe (Linux) and WARP (Windows)** — the Windows path is named, not silently assumed | P0 | 1 |  ✓ built (ci.yml: lavapipe named+installed; WARP on windows runner) —
| Stylus/tablet: **owned `stylus` crate** — Windows dual-path (Windows Ink WM_POINTER + Wintab32 polling), Linux (Wayland zwp_tablet_v2 + X11 XI2); pressure/tilt/proximity/hover; fork/absorb octotablet; upstream to winit | P0 | 1 (windows-ink basic) → 2 (dual) → 5 (matrix) |
| Pressure into paint pipeline bypassing egui input (winit raw events → canvas) | P0 | 2 |  ✓ code (`f867215` + `6430623`): the digitizer's per-event force flows via the RAW `Event::Touch` stream (single pens included — `multi_touch()` returns None below 2 touches and never carried a pen; cross-review finding on f867215, fixed same-night) → begin/extend_screen_point_pressured → begin_stroke_styled (None/0.0→1.0 fallback; the can-fail alpha-difference test rides CI); the stylus crate's OWNED pen backends (winink, XI2) = the follow-up for tilt/proximity/eraser — reachability = Gate 0 |
| Plugin runtime: wasmtime + WASI 0.3 component model (sandboxed; host-API for stamps/ops — plugins orchestrate, never execute inner paint loops) | P1 | 6 |  ✓ built (umber-wasm + umber-plugin-sdk; `plugins.rs` host API) —
| **AI-assisted texturing — P2 research track, out of v0.1** (docs/research/04 §13: Painter has no first-party AI; local-first inpainting/seamless-tile generation via candle or sidecar is the open flank). v0.1 ships only the plugin/provider-interface design hook (Wave 6); delivery is post-0.1 | P2 | 6 (interface only) | ✓ built (the interface: `provider.rs` — TextureProvider + the WASM-safe FnProvider, ProviderRegistry with local-first resolution, GenerationRequest/Result/Provenance, no_std zero-dep, 11 tests; delivery itself stays post-0.1 per this row's own boundary) |
| Scripting/automation: headless CLI (bake/export/render-batch), project format is the API | P1 | 6 |  ✓ built (`5327294` bake-all CLI + wave-6 batch.rs) —
| Distribution: cargo-dist (MSI + GH releases), Flatpak + AppImage on Linux; CI on windows-latest + ubuntu with lavapipe GPU tests | P0 (CI) | 1 (CI), 6 (installers) |  ✓ code, NOT PROVEN: the config exists (`dist-workspace.toml:7-9`, cargo-dist 0.30.0 shell+powershell installer targets; `release.yml:1`) but NO installer has ever been produced or smoke-tested — the C1 audit's 1d split-mark (config built / installers unproven); the review's own caveat: never claim installers the dist config can't produce. Proving them = worklist #9 |
| Profiling: tracy-client + profiling facade from day one | P0 | 1 |  ✓ built (`e4925a1` facade spans + HUD; tracy feature-gated) —

## 11. UI shell — [01]

| Requirement | Priority | Wave |
|---|---|---|
| Dockable panels (egui_dock): layer stack, properties, assets/shelf, texture-set list, history, display settings, export dialog | P0 | 1 (shell) → 2 (dense) |  ✓ built (`ba9fecc`→`e1321a1` dock shell; every panel listed exists in main.rs) —
| Brush properties panel: input→curve editor (MyPaint model), flow×opacity, stabilizer, jitter quartet [05] | P0 | 2 |  ✓ built (`d58bdb4` live preset editing + curve editor; stabilizer rows in brush_panel) —
| History window (session-global, click-to-time-travel; not persisted) | P1 | 2 | ✓ built (`9af4c2f`) — the Bakes/Export record_context wiring = the named follow-up |
| Dark pro theme, 4K-DPI scaling | P0 | 1 |  ✓ built (egui native DPI; `77ec811`→`c51fc1a` the umber theme tokens) —
| Node-canvas widget (custom, for graph editor) | P0 | 4 |  ✓ built (`abf04b6` graph panel + graph_canvas.rs) —
| Localization (EN first; architecture i18n-ready) | P1 | 5 |  ✓ built (`e493c60` fluent catalog, 396 keys en-US + pt-BR) —

## 12. Performance budgets — [02][05]

| Budget | Target |
|---|---|
| Input-to-photon (stylus event → composited pixel on screen) | < 20 ms at 60Hz, measured by timestamp instrumentation (protocol in docs/perf); drain-events-then-acquire frame loop [05] |
| Stroke throughput | 4K texture set, no dropped dabs at 200Hz input streams |
| Viewport | 60 fps sustained (frame-time P95 < 16.7ms) on the reference GPU class (one named mid-range GPU — e.g. RTX 3060 / RX 6600 class, current driver), 1M-tri mesh, 4K set |
| Undo RAM | 100 steps at 4K within ~2 GB (dirty-tile snapshots + cumulative merge), soak-tested in CI [05] | ✓ code (`f78e6c1`): every stroke (paint + retouch) snapshots dirty rects on the shared UndoStack; the envelope is 64 entries / 256 MiB (both caps pre-existing, kept) — inside the 2 GB/100-step budget, with the byte budget the RAM guard at 4K (a full-tile 4K entry is 128 MiB; the third evicts the first); cumulative merge ✓ built (`PENDING`): a stroke overlapping the top entry coalesces into it (oldest before + newest after over the bounding box; size guard: never when the box outgrows the pair), envelope still 64 / 256 MiB — v1 boundary: top-entry-only merge, a merged entry undoes/redoes as one unit (split redo = follow-up); the CI soak = the entry/byte-cap tests + `retouch.rs` `overlapping_strokes_merge_into_one_entry_with_exact_bytes` — reachability = Gate 0 |
| Installer size | target < 150 MB (wasmtime + vendored OCIO + codecs make smaller dishonest — tracked per release, not gated); no runtime deps beyond installer contents |

---

## Explicit non-goals (guardrails)

- Mesh modeling/editing, sculpting — out of scope for v0.1 horizon [02: "basic everything-adjacent features diluting the core"]
- .sbsar/.spp execution — permanent legal wall
- Cloud/accounts/telemetry — none, ever
- macOS — later, contributor-driven
- Real-time collaboration (CRDT) — P2 research only [04]
- In-app generative AI — P2 research track, post-0.1 (plugin/provider interface only in v0.1) [04 §13]
