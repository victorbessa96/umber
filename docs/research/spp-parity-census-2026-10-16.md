# SPP parity census — where Painter is ahead, and the requisites to close it (2026-10-16)

Method: the full-spectrum comparison (brushes/bakers/fills, materials,
UI/UX, scripting/automation, ecosystem, platform) of the current
Umber tree (verified by grep against `crates/`, 2026-10-16) against
the Painter capability bar. Capability knowledge from exploration
results assembled via online sources (2026-10-16), plus the standing
research corpus (docs/research/01-05). Every "Umber lacks X" claim
below was grepped before writing; the row says what the grep found.

Priorities: P0 = blocks "finish a real PBR asset in Umber" or the
0.1.0 story · P1 = the Painter-user-adoption gap · P2 = beyond-parity
or 2+ years. Ordering inside the requisites is dependency-first.

## A. Painting tools and brush engine

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| A1 | Stencil / paint-through screen mask (move/scale/rotate, tiling modes, opacity) | ABSENT (only a wgpu depth-stencil comment); roadmap row exists, no code | R-A1 stencil engine + G6 overlay (the decal/logo workflow) | P1 |
| A2 | Projection paint (screen-space material projection, physical/particle variant) | ABSENT (only the pwys live-raycast stroke) | R-A2 screen-space projection fill (rides the pwys buffer design) | P1 |
| A3 | Path tools beyond paint-along: Filled path, Ribbon (stretched image/substance along path with start/middle/end segments, per-vertex size/opacity, self-overlap blending) | Path tool exists (paint/erase/smudge along path); NO filled-path, NO ribbon | R-A3 filled path (a stroke-class reuse); R-A4 ribbon as the bitmap-stamp engine's killer app | P1 |
| A4 | Text resource: system fonts as paintable text sources (TTF/OTF, size/color/background) | ABSENT (PSD import reports text layers as unsupported) | R-A5 font source node (rusttype/ab_glyph) → fill source + a Text tool | P1 |
| A5 | Vector/SVG fill sources with crop | SVG import deferred (requirements §7); no SVG source | R-A6 SVG source (usvg) as fill/procedural source | P2 |
| A6 | Dynamic strokes (per-stamp alpha/param variation with start/middle/end behavior) | ABSENT | R-A7 per-dab parameter curves keyed on stroke distance | P2 |
| A7 | Particle brushes (.pkfx emitters, receivers) | ABSENT | R-A8 particle emitter system — DECISION ROW: big surface, niche use; recommend decline for 0.x and revisit (Blender/photocraft also lack it) | P2-DECLINE? |
| A8 | Physical-size brush mode (real-world units) | ABSENT | R-A9 physical size flag (units per texture set; multiplies stroke size) | P2 |
| A9 | Alpha as bitmap/procedural stamp with per-stamp exposure | Bitmap stamps LANDED (C2-11); procedural alpha via graph nodes | — (closed) | — |
| A10 | Material Picker tool (P: sample material from surface, temporary tool) | Eyedropper exists (colour); no material sampling | R-A10 "pick all channels at cursor" (rides per-layer pixels; a 20-line readback + fill-layer apply) | P1 |

## B. Layers, channels, masks

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| B1 | Layer-stack FLATTEN (group → fill layer with baked bitmaps, optional smart-material save, export flattened, flatten across instances) | ABSENT (only the anchor prefix-flatten, an internal compositor op) | R-B1 flatten: group → single layer with pixels baked per channel; the export-flattened variant rides it | P0 (a core destructive-when-wanted workflow; small: the pool already owns per-layer planes) |
| B2 | Geometry masks (per-mesh-name and per-UV-tile include/exclude; hide-excluded; copy/paste) | ABSENT (no mesh-name/UDIM-tile scoping on layers) | R-B2 geometry mask on layers/effects: mask predicate = mesh names × UV-tile sets; rides UDIM tiles which exist | P1 |
| B3 | Effect stacks with PER-CHANNEL enable + effects on folders + paint effects (paint INTO a generator's mask input) | Effect stacks exist (graph-driven); per-channel effect enable + folder effects UNKNOWN-not-verified; paint-effect input ABSENT | R-B3 paintable effect inputs (a paint plane as a generator input — the anchors seam already exposes layer refs) | P1 |
| B4 | User channels User0-15 renameable, add/remove at any time, multi-channel add window | Mesh-map import routes "user channels"; add/remove UI unknown; renameable user channels ABSENT | R-B4 channel add/remove/rename in the Texture Set settings (the model already routes user channels) | P1 |
| B5 | Smart materials track .spsm files on a shelf with drag-drop anywhere, resources updater | Smart materials exist (bind mesh-map generators to newest bakes); NO shelf/Assets panel for materials (brush shelf only) | R-B5 the Assets panel (C3 below) is the vehicle; smart-material browser rows | P1 |
| B6 | Instanced layers ACROSS texture sets in one action; instance icons + navigation | Instancing exists in-set; cross-set instance UI unknown | R-B6 cross-set instancing where the pool model allows; else report | P2 |
| B7 | Height/AO/normal "mixing settings" per texture set (Replace/Combine; Sharp/Smooth height→normal; AO Multiply) | UNKNOWN-not-verified at the texture-set level | R-B7 audit row, then the mixing dropdowns in Texture Set settings | P1 |
| B8 | Per-channel blending UI: channel dropdown + per-channel blend/opacity copy-paste | 32 blend modes + per-channel routing landed (C2-9 stack UI); per-channel blend copy-paste UNKNOWN | — (verify + small UI row) | P2 |

## C. Assets / ecosystem (Painter's real moat)

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| C1 | Assets window: starter + user libraries, category filters, saved searches, drag-out, import dialog, custom library locations, thumbnails | ABSENT as a panel (brush preset shelf exists; mesh-map/PSD/spp import sections exist) | R-C1 the Assets panel: libraries of brushes/smart materials/masks/filters/meshes/env maps, filterable, drag-to-layer-stack; the GUI plan's corpus gap row — this is Umber's biggest single UX gap after the paint loop | P0 for 0.2 (the "empty tool" problem: capability exists, discoverability does not) |
| C2 | 1933 shipped sbsar procedurals + smart materials as STARTER CONTENT | umber-graph ships a node set + 2 smart-material examples; no asset library | R-C2 ship 30-50 curated original smart materials/filters built with the own engine (rides wave-community-templates) | P1 |
| C3 | Export presets as files, auto-embedded in project; community preset long tail | Engine presets shipped (unreal/unity/gltf/blender); user-defined output templates exist; file-based preset share UNKNOWN | — (verify; small) | P2 |
| C4 | Plugins marketplace/distribution (CCD) | WASM plugin SDK + authoring guide LANDED; no distribution story | R-C4 plugin directory + install flow (consent gate exists) | P2 |

## D. Baking

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| D1 | Linked bake groups across texture sets; sync/copy/paste baker settings; auto-rebake toggle per map; per-UV-tile bake exclusion | Auto-rebake landed; linked groups ABSENT; settings copy/paste ABSENT | R-D1 baker-settings sync + linked groups (a bake-request fan-out; small) | P1 |
| D2 | Auto-cage (experimental automatic cage generation) | Cage = distance-based + custom; auto-cage ABSENT | R-D2 auto-cage (normal-offset walk; experimental flag) | P2 |
| D3 | Bakers: opacity baker | Umber bakes AO/normal/curvature/position/thickness/ID/height/bent — opacity ABSENT (grep: requirements §3 lists it, umber-bake lacks it) | R-D3 opacity baker (alpha coverage raycast; small) | P2 |
| D4 | ID bake color-source options (vertex/material/file/mesh + random/hue-shift/gray) | ID bake exists; source options UNKNOWN | R-D4 ID source options (mesh/material split exists; add vertex-color when mesh importer exposes it) | P2 |
| D5 | Ground plane + offset on AO | ABSENT | R-D5 ground-plane occluder in the AO baker | P2 |

## E. Viewport / display

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| E1 | Camera presets + imported cameras; camera gate/resolution overlay | Ortho landed; presets/imported cameras ABSENT | R-E1 camera presets (glTF/USD cameras import via mesh loaders — expose them) | P2 |
| E2 | Iray path-traced render mode | ABSENT (out of 0.1 scope, GUI plan defers) | — (deferred by plan) | P2 |
| E3 | Warp projection + Warp-to-geometry (auto-conform decals to mesh) | Fill projections: UV/Tri-planar/Planar/Spherical/Cylindrical landed; warp ABSENT | R-E3 warp projection (interactive grid) — the decal workflow pairs with A1 | P2 |
| E4 | Displacement/tessellation preview (POM/subdiv, physical-size units) | Height→normal live landed; displacement preview = wave-tessellation row (P2) | — (already roadmapped) | P2 |
| E5 | Post-effects: glare, lens flare, lateral aberration, film grain, color correction | PostStack has DoF/bloom/vignette/sharpen/tonemap; glare/flare/aberration/grain ABSENT | R-E5 four more post nodes (compositor slots exist) | P2 |
| E6 | Wireframe overlay opacity/color, mesh grid options | Wireframe + grid landed (per scoreboard); opacity/color options UNKNOWN | — (verify) | P2 |

## F. UI/UX (the adoption gap, GUI-plan-relative)

Rows only where Painter does something the GUI plan (G0-G7) does not
already cover — the plan remains the plan of record for its rows.

| # | Painter has | Umber state | Req | P |
|---|---|---|---|---|
| F1 | First-run experience: Welcome/What's-New panels, Home screen | g5-home-screen + g5-first-run are PLANNED, not built | — (already planned; keep) | P1 |
| F2 | Workspaces (Painting/Rendering layouts, save/load/reset) | g5-workspaces partial (5f7bdd2; presets+persistence open) | — (in flight) | P1 |
| F3 | Per-tool contextual Property sections with grouped, collapsible parameter UIs (the "annotation-driven UI") | The graph's exposed-params UI exists (smart-material-params row, wave 9); the tool-properties regrouping is the g2-properties-context kill | — (in flight) | P1 |
| F4 | Alt+click mask solo-view; mask thumbnails; effect-presence underlines | g2-layer-gestures row covers it (not yet landed) | — (planned) | P1 |
| F5 | High-DPI + 4 localizations (EN/DE/FR/ZH) | i18n en-US + pt-BR; DPI atlas 2x | R-F1 the localization framework is there; more catalogs are content work | P2 |
| F6 | Baking MODE as a distinct app mode with its own window set | Bake mode = a panel + mode keys; a distinct workspace variant rides g5-workspaces | — (in flight) | P1 |

## G. Scripting / automation / interop

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| G1 | Python API: full app scripting, dock widgets, event bus, export presets | JSONL agent protocol v1 (projects, layers, undo, paint, snapshots); NO embedded scripting language | R-G1 DECISION ROW: an embedded script surface. Recommend: stay JSONL-first (the agent protocol is the automation API; it is versioned and testable) and add a `--script` file mode + a Python client LIBRARY over the protocol rather than embedding CPython. Embedding a language runtime in a GPL Rust app is a maintenance surface the project does not need | P1 (the client library), P2 (any embedding) |
| G2 | Remote control (WebSocket/remote-scripting server) | ABSENT | R-G2 the JSONL protocol over a local socket (the headless story exists; the transport is thin) | P1 |
| G3 | Send-to interop: Photoshop/Stager/Ae, Substance Connector (open source) | ABSENT; MaterialX + USD export are the interchange | R-G3 a "send to Blender" (open file + link textures via USD) one-click — the connector's open Apache-2.0 wire is a reference, not a dependency | P2 |
| G4 | Command-line mesh-map import on project creation (`--mesh-map TextureSetName_map`) | CLI has bake/export/bench/extract-spp/agent; project-creation CLI ABSENT | R-G4 `umber-cli new <mesh> --maps <dir>` (project creation headless; the app flow exists) | P1 |
| G5 | Export shaders-parameters JSON for material layering round-trip | Dynamic materials landed engine-side; a params-JSON export UNKNOWN | R-G5 dynmat params export (rides the smart-material-params row) | P2 |

## H. Platform / security / provenance

| # | Painter has | Umber state (verified) | Req | P |
|---|---|---|---|---|
| H1 | macOS build (Apple silicon, Metal) | Windows + Linux only (deliberate) | — (deliberate scope; revisit post-1.0) | — |
| H2 | Auto-update / CC desktop integration | AppImage + Flatpak (offline-deps kill open); no updater | R-H1 Flatpak finish + an AppImage update checker post-1.0 | P1 (flatpak), P2 (updater) |
| H3 | C2PA content provenance on exports | ABSENT | R-H2 c2pa-rs signing at export (opt-in; the identity story pairs with code signing) | P2 |
| H4 | Crash reporting service (Sentry) | Local crash reports + sticky notice (landed 2026-10-16); opt-in submission ABSENT (by local-first doctrine) | — (deliberate; document the doctrine) | — |

## The requisites, consolidated and dependency-ordered

**Tier 1 — the "finish an asset" and adoption-critical set (0.2 line):**
1. R-B1 layer-stack flatten (P0, small, pool-native)
2. R-C1 the Assets panel with libraries + drag-drop (P0, the biggest UX gap)
3. R-A1 stencil engine (P1, the decal workflow; pairs with g6-stencil-overlay)
4. R-A2 projection/paint-through fill (P1; rides pwys)
5. R-A3 filled path (P1, stroke-class reuse)
6. R-B2 geometry masks (P1; rides UDIM)
7. R-A10 material picker (P1, small)
8. R-D1 baker sync/linked groups (P1, small)
9. R-G4 headless project creation (P1, small)
10. R-B4 channel add/remove/rename UI (P1, small)

**Tier 2 — the Painter-power-user set (0.3-0.5):**
11. R-A4 ribbon (bitmap stamps' killer app)
12. R-A5 text/font sources
13. R-C2 starter content library (30-50 original smart materials)
14. R-B3 paintable effect inputs
15. R-E3 warp projection
16. R-E5 the four post nodes
17. R-G1 the agent-protocol client library (+ `--script`)
18. R-G2 JSONL over local socket
19. R-D3 opacity baker, R-D5 ground-plane AO, R-B7 mixing settings, R-A8 physical size

**Tier 3 — beyond-parity / horizon:**
20. R-A6 dynamic strokes, R-A5 SVG sources, R-A7 particle (decision: decline recommended), R-B6 cross-set instancing, R-C4 plugin distribution, R-D2 auto-cage, R-E1 camera presets, R-G3 send-to-Blender, R-G5 dynmat params export, R-H3 C2PA provenance

Plus the .spp v2 requisites R1-R7 from the import audit
(`docs/research/spp-import-audit-2026-10-16.md`): full-precision
channels, assbin mesh import, resource images, the HBO layer-stack
schema (flagship), baking configs, m3 verification, dense groups.

## What Umber already does better (the census's other half)

For honesty in the README and the parity scoreboard: original
node-graph + MaterialX round-trip (Painter has no MaterialX at all);
git-friendly diffable project format vs opaque HDF5; headless
first-class (the agent protocol paints through the same engine);
WASM-sandboxed plugins vs unrestricted Python/QML; local-first, no
account, no telemetry; GPL community-owned; the never-crash doctrine
with a fuzz corpus (Painter's mesh importers famously crash on bad
input); OCIO/ACES built in; 8K export on free GPUs; two first-class
platforms. Painter's moat is workflow density + content ecosystem,
not core tech — and the census above says exactly which density rows
close it.
