# Kill report: C2-17 a UV-unwrapped FBX fixture; Gate 0's FBX leg is no longer vacuous

Date: 2026-10-15. Author: dragon. Lock: `c2-17-fbx-uv-fixture` over
`scripts/gen-fbx-uv-cube.py`, `crates/umber-mesh/tests/assets/cube_uv_ascii7400.fbx`,
`crates/umber-mesh/src/fbx.rs`, `crates/umber-app/tests/gate0_journey.rs`
and this report. Source: `docs/loop/audit-2026-10-15-c2.md` row 17 and
ROADMAP `wave-fbx-uv-fixture`.

## The gap

`cube_b7400.fbx` (ufbx's Blender 2.72 cube) has no `LayerElementUV`, so
every UV loaded as [0, 0]. Gate 0's FBX leg loaded it and then painted
"across the loaded mesh's UVs", which in practice meant a mesh whose
every corner sits at one point. The release gate's FBX claim was
vacuous.

## Built

- **`scripts/gen-fbx-uv-cube.py`**: the fixture's source. It writes a ±1
  cube as six quads, with per-face normals (ByPolygonVertex) and a real
  unwrap: a 3x2 atlas, one face per cell, each cell inset by 1/64 so no
  two faces share a texel at bake sizes of 64 and up. The output is ASCII
  FBX 7.4, which ufbx reads through the same path as binary. The script
  is deterministic: two runs are byte-identical, checked.
  No FBX authoring tool (Blender, assimp) exists on either host, so a
  committed generator is the honest provenance. The file is not
  hand-edited.
- **`cube_uv_ascii7400.fbx`** (2.6 KB), the generated file.
- **`loads_the_uv_unwrapped_fbx_cube`** (umber-mesh) pins: 12 triangles,
  every UV strictly inside its cell's inset, all six cells used, unit
  normals, and positions on the ±1 cube.
- **Gate 0 now loads the UV'd cube** and asserts the six-cell unwrap
  BEFORE painting on it. If the gate is ever pointed at a UV-less mesh
  again, it fails at leg 1 instead of passing vacuously. The module docs
  name the change. The binary cube stays as the binary-FBX loader's own
  fixture, with its no-UV pin.

## Verified

Gate 0 passes on the build host with the GPU legs running (15.4 s; a
skip finishes in milliseconds). Gate 1: umber-mesh 123, umber-app 483,
fmt and clippy clean.

## Deferred

- `margin_cli.rs` deliberately avoids the binary cube (its comment says
  why) and uses another mesh. Moving it to this fixture is optional and
  was not done.
- A BINARY UV'd FBX. The ASCII and binary paths share ufbx's parser
  after tokenizing, and the binary tokenizer is covered by
  `cube_b7400.fbx`.
