# Audit lens D — formats, import/export, automation (2026-10-24, live-verified)

You are auditing Umber (~/Projects/umber), a Rust 3D texture-painting
app. READ-ONLY TASK: no cargo, no builds, no file edits outside your
report file. Every claim cites file:line from the LIVE tree.

## The lens: every byte that crosses the app's boundary

### 1. The mesh importers (crates/umber-mesh/src/)

FBX (fbx.rs, ufbx), glTF, OBJ (tobj), USD (usda.rs/usdc.rs, openusd
0.8), USDZ. For each: the live surface, the material mapping
(triangle_materials — landed), the panic-hunt hardening state, the
corpus/fuzz coverage (crates/umber-mesh/tests/panic_hunt_mesh.rs),
the known limits.

### 2. The texture importers

.spp (the chunk index fix, the corpus harness, the v1 flatten + the
staged a-d HBO stack rows — which staged parts are LANDED vs the
plan: read `docs/research/spp-import-audit-2026-10-16.md` R1-R4 and
verify each against the tree), PSD (a4bf81a flatten-on-import),
MaterialX (Kill A/B/S: the parse, stdlib verify, the import path),
SVG (the wave-5 row — landed or open?), images (PNG + what else).

### 3. The export paths (crates/umber-export/)

The map list, presets, tiled export, the EXR/PSD-out/USD/MaterialX
writers, 8K (cd668a3), the packing rules. Verify each writer's live
site + its decode-verified tests.

### 4. The save format (.umber)

The project.json + per-layer JSON + sidecars + the save v2
(layers/<id>/) + the pixel autosave + the crash-recovery path. The
completeness-by-destructuring guarantee (17900df). What a save
round-trip provably preserves (cite the persistence tests).

### 5. The agent protocol (the JSONL wire)

The step list (PROTOCOL.md or its docs home): projects, layers,
paint strokes/reads, paint_snapshot, undo, version pinning, the
exact-mode strokes, bake steps, export steps. The corpus-gated v2
lane state. What a headless agent CANNOT do today that the GUI can.

### 6. The brush preset formats (.umberbrush), masks (.umbermask),
smart templates, the plugin SDK (umber-wasm: the ABI, the consent
gate, the authoring guide docs/plugins/authoring-guide.md).

## The output

Write `docs/loop/audit-2026-10-24-lens-formats.md`:
- The format matrix: format | direction | live site | fidelity |
  tests | known limits.
- The .spp staged-import state (R1-R4: landed/absent each).
- The agent protocol's step inventory + the GUI-only remainder.
- "The named gaps": every partial with its seam, ordered by user
  pain.
- "The surprises": anything the docs claim that the tree refutes,
  or vice versa.
