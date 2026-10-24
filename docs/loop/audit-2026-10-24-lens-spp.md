# Umber audit — lens C: the SPP parity comparison (2026-10-24)

Author: dragon, solo (the claw lanes hit their walls: opencode's
free-tier limit; pi kept for the mechanical lenses). Baseline:
`docs/research/spp-parity-census-2026-10-16.md` — every row
re-verified against TODAY's tree; the census is 8 days and ~112
commits stale and this pass catches what changed.

## The headline verdict

**Umber holds core-tech parity on the painting surface and a
genuine lead on interchange/automation — the moat Substance
Painter still holds is workflow density and the content
ecosystem, and it narrowed this week by exactly two Tier-1 rows:
the stencil (A1) and the filled path (A3) both landed.** The
remaining P0 gaps are the census's own two: the layer-stack
FLATTEN (B1) and the Assets panel (C1) — capability exists
behind both; the workflows do not.

## The census delta (10-16 → 10-24, verified live)

| Census row | 10-16 state | 10-24 state | The evidence |
|---|---|---|---|
| A1 stencil / paint-through screen mask | ABSENT | **LANDED** | the full stencil lane: engine + app 0955ac8, paint hook 3a8b4cb, panel 6c95e26, viewport + N 2bb8c60, set-switch re-stage dcc5e89 + the W-C4 L1's lib move (b79f0c6), cross-tile observer 45ad4e4 — stencil.rs in-tree, the S/LMB/RMB/MMB chords live |
| A3 filled path | ABSENT | **LANDED** | `PaintState::replay_filled_path` (paint_state.rs:4315) + the polygon raster (`umber-mesh/src/polygon_fill.rs:265`); the fill-layer side's PolygonFillTool (fill_layer.rs:468) |
| A9 bitmap stamps | closed | closed | C2-11 (lens A's table) |
| A2 projection fill (pwys) | ABSENT | **still ABSENT** | no pwys/projection_fill symbol in any crate |
| A10 material picker | ABSENT | **still ABSENT** | no pick_material/pick_all_channels |
| A4 text sources / A5 SVG / A6 dynamic strokes / A7 particles / A8 physical size | ABSENT | still ABSENT | (A7 = the recommended decline stands) |
| B1 layer-stack flatten | ABSENT | **still ABSENT** | no flatten fn in layers_panel/paint_state — the census's P0 #1 |
| B2 geometry masks / B3 paintable effect inputs | ABSENT | still ABSENT | |
| B4 channel add/remove/rename | ABSENT | **PARTIAL** | per-channel SIZE overrides landed (channel_override + clear_channel_size, main.rs:2611/2635 + resize_channel); add/remove/rename still absent |
| B5/B6/B7/B8 | per census | unchanged | |
| C1 Assets panel | ABSENT (P0) | **still ABSENT** | no assets module — the census's biggest UX gap stands |
| C2 starter content | absent | still absent | rides wave-community-templates |
| C3/C4 | per census | unchanged | |
| D1 baker sync / D2 auto-cage / D3 opacity baker / D4 ID sources / D5 ground plane | per census | unchanged | (the per-set masked bakes closed the SHARING gap D-adjacent — per-set bakes no longer byte-identical across sets) |
| E1-E6 | per census | unchanged | E5's post nodes: DoF/bloom/vignette/sharpen/tonemap; glare/flare/aberration/grain still absent |
| F1 home screen / F4 mask gestures / F5 localization framework | PLANNED | **LANDED** | g5-home-screen 5e0d89e, g2-layer-gestures d9fa931 (lens B's scorecard) |
| F2 workspaces | partial | partial (5f7bdd2's presets + persistence tail) | |
| G1 embedded scripting | DECISION: JSONL-first | stands (the protocol is the API; the client library still owed) | |
| G2 socket transport / G3 send-to / G4 cli new / G5 params export | ABSENT | **all still ABSENT** | no `umber-cli new` |
| H1-H4 | per census | unchanged | H2's flatpak: the chain landed this week with the honest appstream-compose revert |
| The .spp v2 requisites R1-R7 | staged | **R-chunk-index LANDED** (the b-tree offset fix); the rest staged | the formats lens (D) carries the detail |

**Tier-1 arithmetic: 2 of 10 closed this week (A1, A3) + the
masked-bakes sharing row; 8 remain, led by the two P0s.**

## The parity scorecard (the capability families, verdict per family)

| SPP family | Verdict | The boundary |
|---|---|---|
| Painting core (stamps, flow, spacing, falloff, jitter, angle, methods, stabilizers) | **PARITY** | lens A's 10/10; the four stroke methods + the full dynamics table |
| Stencil/projection | **PARITY on stencil**; projection fill GAP (A2) | the decal workflow's other half |
| Retouch (smudge/clone/erase/masks) | **PARITY** | split-redo remains an internal debt |
| Path tools | **PARITY** (paint-along + filled + polygon + straight-line) | ribbon (A4's stretch-along-path) GAP |
| Quick mask / cavity / ID-mask | **PARITY** | quick-mask + symmetry PERSISTENCE gaps (lens A) |
| Symmetry | **PARITY engine** | not persisted |
| Fill/material layers + anchors + instances | **PARITY** (materials composite-only by design) | |
| Texture sets / channels / UDIM | **PARITY** | channel add/remove/rename (B4's tail); the auto-assign LANDED |
| Baking (8 maps, cage, skew, margins, per-set) | **PARITY** | D1-D5's power-user rows |
| Viewport (OpenPBR, IBL, postfx, modes) | **PARITY** | the four post nodes (E5); displacement preview |
| Color management (OCIO/ACES/LUT) | **PARITY** | |
| Export (templates, packing, presets, EXR/PSD/USD/MaterialX, 8K) | **PARITY** | |
| Import (.spp/PSD/FBX/glTF/OBJ/USD/USDZ) | **PARITY** | the .spp v2 staged lanes; text layers reported unsupported |
| History/undo | **PARITY** | the unified timeline (L6) merges the two views |
| UI/UX density | **PARTIAL** | 36/43 GUI kills; the color picker + keyboard-only are the felt gaps |
| Assets/ecosystem | **GAP — the moat** | the Assets panel (C1) + starter content (C2) |
| Scripting/automation | **EXCEEDS (headless-first)** + GAP (the client library + socket) | the JSONL protocol is versioned + testable; Painter's Python is not |
| Interchange (MaterialX round-trip, diffable format) | **EXCEEDS** | Painter has no MaterialX; HDF5 is opaque |
| Licensing/community/local-first/no-telemetry | **EXCEEDS** | the census's own half |

## The remaining-gap list, ordered by a painter's pain

1. **The Assets panel (C1, P0)** — capability exists, discoverability
   does not: the "empty tool" problem.
2. **Layer-stack flatten (B1, P0)** — the destructive-when-wanted
   workflow every Substance user reaches for.
3. **The color picker (g4)** — the stock egui picker under the
   single most-touched widget.
4. **Projection fill (A2)** — the decal workflow's other half;
   pairs with the landed stencil.
5. **Geometry masks (B2)** — per-mesh/tile scoping.
6. **Material picker (A10)** — small, rides per-layer pixels.
7. **Keyboard-only + the desktop smokes unfired** — the trust
   surface.
8. **Baker sync (D1), channel manage (B4's tail), the four post
   nodes (E5)** — the power-user tail.

LENS-C-DONE
