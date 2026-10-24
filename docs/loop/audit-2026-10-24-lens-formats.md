# Umber audit — lens D: formats, import/export, automation (2026-10-24)

Author: dragon, solo. Every claim cited against the live tree.

## The format matrix

| Format | Direction | Live site | Fidelity | Tests |
|---|---|---|---|---|
| FBX (ufbx) | in | umber-mesh/src/fbx.rs (face_material per-triangle attach, :56-143) | meshes + materials + UVs | panic-hunt corpus + the UV'd fixture (cube_uv_ascii7400) |
| glTF | in | gltf.rs:23-40 (per-primitive all-or-nothing attach) | meshes + materials | panic-hunt |
| OBJ (tobj) | in | lib.rs:240-260 (per-model material pass) | meshes + materials | panic-hunt |
| USDA/USDC/USDZ | in | usda.rs/usdc.rs (openusd 0.8) | meshes; materials honestly empty (subsets later) | panic-hunt + the abort fix regression |
| .spp | in | crates/spp (the chunk-index b-tree fix, 10-16) | v1 flattened: materials + layers; the staged HBO probes ARMED (the hbo_file/probe tests, spp's test module :249-530) | the corpus harness (real files, nonzero floors) |
| PSD | in | a4bf81a flatten-on-import | raster layers; text reported unsupported | the import suite |
| MaterialX | in | Kill A bb287cd + S (stdlib verification) | parse + report + the round-trip's own nodedef honesty | the mtlx suite + mutated-valid |
| PNG/EXR/PSD/USD/MaterialX | out | umber-export (the writers + the presets + packing) | decode-verified (Gate 0) | export suite |
| 8K presets | out | cd668a3 | bake→export→decode proven at 8192² | the 8K leg |
| .umber | rw | the project.json + per-layer JSON + save v2 (layers/<id>/, f2aff9f) + the exhaustive-destructuring guarantee (17900df) | byte-exact round-trip (24 persistence tests) | persistence suite |
| .umberbrush | rw | umber-brush/src/preset.rs (falloff/stamps/methods persisted; stamp IMAGES still owed — the 6c row) | | preset suite |
| .umbermask | rw | the mask presets compose onto layers (C2-10) | | |

## The .spp staged-import state (the audit R1-R4)

- R-chunk-index: **LANDED 10-16** (the b-tree offset fix + the
  corpus content gate).
- R1 full-precision channels (f16→planes): still staged.
- R2 the assbin mesh: still staged.
- R3 ResourceImage decode: still staged.
- R4 the HBO layer-stack schema: **test-armed but not imported** —
  spp's own test module carries hbo_file/probe_hbo_summary_line
  (:249, :526-530) probing the format's shape; the import path
  itself still writes the v1 flatten. The staged a-d lanes hold.

## The agent protocol (the JSONL wire)

The live step vocabulary from the CLI: bake, bake_transfer,
export, inspect, maps, margin (Adjacent/Extend), the baker list
(ao/curvature/id/dilate/extend/gltf/bilinear...), paint strokes/
reads/undo, paint_snapshot, layer ops, project ops, version
pinning; the exact-mode strokes (no pointer smoothing); the
--script file mode (a JSONL piped into `umber-cli agent`).

**The GUI-only remainder** (what the wire cannot do today): the
interactive-only surfaces — the viewport navigation, the panel
state, the live brush-cursor, the stylus conditioning parameters
as live state, the Preferences/keymap, the workspace layout, the
desktop smokes' quit-guard + recovery paths.

**The owed protocol rows** (census G): the client LIBRARY over
the protocol (R-G1's P1 half), the local-socket transport
(R-G2), `umber-cli new` headless project creation (R-G4) — all
still absent.

## The plugin SDK

umber-wasm: the consent gate (blake3 hash before load,
plugins.rs:22), the frozen v1 ABI (exports, MBER/MDEF, TLV kinds,
error codes, no imports), the authoring guide
(docs/plugins/authoring-guide.md, b0c1aa0). Distribution (C4)
remains the open half.

## The named gaps

1. The .spp staged R1-R4 (above) — the flagship import stays the
   v1 flatten, honestly reported, never silent-partial.
2. The agent client library + socket transport + `cli new`.
3. Stamp images not persisted with presets (the .umberbrush gap).
4. Text layers (PSD-in reports unsupported; no font source).
5. SVG sources (deferred by requirements §7).

LENS-D-DONE
