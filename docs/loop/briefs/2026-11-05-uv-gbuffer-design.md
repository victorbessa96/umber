# Design brief — UV G-buffer single source of truth (queue row 156, sub-item B), 2026-11-05

Status: IN PROGRESS (2026-11-05 ~17:xx). Kills 1, 2, 3, 4, 5 (ID half),
6 (CPU half) LANDED, plus a WGSL eps pin and a GPU-vs-CPU parity golden
(queue rows 172-178). Remaining: 5b (position bake subsumed by / reading
the G-buffer, rebinding AO/curvature/thickness), 6 GPU half (padding
coverage from the G-buffer when a device exists), 7 (app fill pick), 8
(4K ref-scene parity + the §12 perf gate). Parity result that changes §2b:
triangle ids are bit-exact on lavapipe but barycentrics are NOT 0 ULP
(worst 1.8e-7 abs); the bound is absolute (`UV_BARY_EPS`), see row 176.
Original design text below is unchanged.
Row 156 names "UV G-buffer (texel->barycentrics, single source of truth for
paint/composite/masks/bake/padding)". Row 156-A (hostile-input sweep) is
CLOSED (`2548e11`, queue row 166), so the mesh lane is free for this kill.
Survey method: `rg` over `crates/` for
`barycentric|bary|uv_to|texel|intersect|cross2|det|inv_denom|uv_triangle_at|rasterize_uv_geometry|tile_of_uv|local_texel`,
then reading each hit's surrounding contract comments.

## 1. Survey — every texel→barycentric / UV→triangle lookup site

### 1a. Core lookup sites (UV/texel → triangle + barycentrics)

| # | Site | Inputs → outputs | Duplication |
|---|------|------------------|-------------|
| S1 | `crates/umber-mesh/src/fill.rs:517` `rasterize_uv_geometry(mesh, tile, w, h) -> Vec<Option<TexelGeom>>` (barycentric solve at fill.rs:595-612, area eps `1e-12`, bary eps `-1e-6`, last-in-index-order wins) | per-texel center UV → `TexelGeom{pos, normal, uv, triangle}` (fill.rs:492) | **Axis A copy 1 (CPU).** Consumers: `evaluate_fill_tile` (fill.rs:667), export padding `covered[]` (`crates/umber-cli/src/main.rs:1027`, `crates/umber-app/src/export_dialog.rs:421`, `crates/umber-export/src/driver.rs:43`) |
| S2 | `crates/umber-mesh/src/polygon_fill.rs:442` `uv_triangle_at(mesh, uv) -> Option<usize>` (same `-e_i/area` solve, area `1e-12`, bary `-1e-6`) | one UV → triangle index only | **Axis A copy 2 (CPU)** — verbatim same math as S1's inner loop, minus pos/normal. Consumer: `crates/umber-app/src/fill_layer.rs:594` `pick_triangle` → `build_mask` (fill_layer.rs:614-627, Triangle/Mesh/UvChunk modes). Pinned by tests at polygon_fill.rs:712-718 |
| S3 | `crates/umber-mesh/src/raycast.rs:40` `ray_intersect(mesh, origin, dir) -> Option<RayHit>` (`RayHit{triangle, t, bary:(u,v)}` raycast.rs:14-21) + `interpolate_attr` (raycast.rs:26) + `uv_at` (raycast.rs:137) + `walk_surface_path` (raycast.rs:151) | 3D ray → nearest hit + barycentric UV | **Ray axis (Möller–Trumbore, eps `a<1e-8`, `t>1e-5`), brute-force** — mirrored again in `bvh.rs` (316, 383) and in the AO/transfer WGSL (bake_shaders.rs:153, 849, 1594, 1912). Consumers on the 3D paint hot path: `crates/umber-app/src/viewport.rs:1694 pick_uv`, `:1706 world_at`, `:1744 pick` (per pointer event; `status_bar.rs:13` documents the brute-force per-move cost); `polygon_fill.rs:490 pick_from_ray` |
| S6 | `crates/umber-gpu/src/bake_shaders.rs` `POSITION_BAKE_SHADER` — `cs_main:507`, `cross2:502`, `DET_EPS=1e-10` (496), `BARY_EPS=1e-6` (497), solve at 554-569, `TRI_CHUNK=256` shared-memory batches, last-writer-wins, per-texel UV with `(1-v)` flip | per-texel → world pos + coverage (Rgba32Float) | **Axis A copy 3 (GPU WGSL)** — today's de-facto G-buffer: position + coverage only, NO tri-id/bary channels |
| S7 | `crates/umber-gpu/src/bake_shaders.rs` `ID_BAKE_SHADER` — verbatim duplicate solve: `ID_DET_EPS=1e-10` (1348), `ID_BARY_EPS=1e-6` (1349), `cross2:1355`, `cs_main:1377`, test 1406-1413 | per-texel → winning triangle index (FNV-1a hashed to color) | **Axis A copy 4 (GPU)** — exists only because S6 stores no tri id ("Why the triangle index is re-derived here", doc ~1266-1290) |

### 1b. Supporting duplication axes

| # | Site | Role |
|---|------|------|
| S4 | `crates/umber-mesh/src/bake_support.rs:60` `Cage::lerp_at(tri, bary)` | **Axis B (barycentric interpolation)** CPU copy — shares the convention with `uv_at`/`interpolate_attr` (S3) and the GPU vertex-attribute interpolation |
| S8 | `crates/umber-gpu/src/renderer.rs:1370-1378,1535` wireframe duplicated-vertex buffer (+ `crates/umber-mesh/src/overlay.rs` BARY corners); CPU test oracle `barycentric_weights` renderer.rs:4123 mirroring GPU interpolation for the tangent goldens (renderer.rs:4033+) | **Axis B GPU** — the rasterizer's own barycentric interpolation; the G-buffer must agree with it by construction, not by parallel math |
| S5 | `crates/umber-mesh/src/udim.rs` — `FIRST_TILE=1001` (38), `tile_of_uv:48` (NaN→FIRST_TILE), `tile_of_triangle:62`, `triangles_for_tile:122`, `present_tiles:135` | **Axis C (tile routing)** — the tile axis every lookup must respect; mirrored by `crates/umber-bake/src/ao.rs:630` clamp `saturating_sub(FIRST_TILE).min(99)` |
| S9 | `crates/umber-gpu/src/bake_shaders.rs` AO/thickness/curvature/bent-normals/transfer (`cs_main_from_position:310` etc.) | consume S6's position map as coverage authority (alpha ≤ 0.5 = uncovered); never re-run containment (`inv_denom` appears only at 560 and 1408) |
| S10 | `crates/umber-app/src/fill_layer.rs:220` (`evaluate_fill_tile`), `:594` (`uv_triangle_at`), `:614-627` (`build_mask`) | app-side consumers of S1/S2 |
| S11 | `crates/umber-app/src/viewport.rs:1694/1706/1744` | app-side consumers of S3 on the stroke path |
| S12 | `crates/umber-app/src/display_panel.rs` | named in the row, but on inspection it holds only overlay **toggles** (wireframe/uv-seams, lines 190-217) — no lookup math; recorded as a non-site so the kill list doesn't chase it |
| S13 | `crates/umber-paint/src/paint_state.rs:5943` `local_texel` (V-flipped linear UV→tile-local texel) + tile routing at :1902/:1962 | the paint hot path's UV→texel step — linear, no triangle lookup today; the G-buffer must not add a per-stroke CPU walk here |

**Duplication verdict:** the 2D point-in-triangle barycentric solve (Axis A)
exists in **4 copies** — S1 and S2 on CPU (det eps `1e-12`), S6 and S7 in WGSL
(det eps `1e-10`) — with **two different epsilon values** (a live divergence,
see §4). Axis B (interpolation) exists in 3 forms (CPU `interpolate_attr`,
CPU `Cage::lerp_at`, GPU attribute interpolation + test oracle). Axis C
(tile routing) is already centralized in `udim.rs` with one mirror in
`ao.rs:630`.

## 2. Single source of truth

**Owning crate: `umber-mesh`** — pure CPU, no GPU deps (Cargo.toml deps:
glam, openusd, thiserror, serde, bytemuck, gltf, tobj, ufbx — no wgpu), and
already the bottom of the dependency cone: `umber-gpu` (renderer.rs:137
`umber_mesh::FIRST_TILE`), `umber-app` (fill_layer.rs:46), `umber-bake`
(ao.rs:22), `umber-paint` (paint_state.rs) all depend on it. Layering stays
clean; nothing below umber-mesh needs to know the answer.

### 2a. CPU API (new module `crates/umber-mesh/src/uv_gbuffer.rs`)

```rust
// The ONE epsilon pair, exported so the WGSL side quotes the same values.
pub const UV_DET_EPS: f32 = 1e-10;   // |area| below this = degenerate (unifies S1's 1e-12 with S6/S7's 1e-10)
pub const UV_BARY_EPS: f32 = 1e-6;   // edge-inclusive inside test, winding-agnostic

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvHit {
    pub triangle: usize,      // mesh.indices / 3; LAST in index order wins on overlap
    pub bary: [f32; 3],       // b0+b1+b2 == 1; b_i = -e_i / area (the fill.rs:602 correction)
}

/// The primitive: UV -> (triangle, barycentrics). None on miss / NaN /
/// malformed or UV-degenerate triangle. Never panics.
pub fn uv_to_barycentric(mesh: &MeshData, uv: [f32; 2]) -> Option<UvHit>;

/// The CPU G-buffer: one UvHit per texel of a tile, texel centers at
/// u = tu + (x+0.5)/w, v = tv + 1 - (y+0.5)/h (the fill.rs:527-529 inverse of local_texel).
pub struct UvGBuffer {
    pub tile: u16, pub width: u32, pub height: u32,
    pub hits: Vec<Option<UvHit>>,   // row-major, top-down; None = uncovered
}
impl UvGBuffer {
    pub fn build(mesh: &MeshData, tile: u16, width: u32, height: u32) -> UvGBuffer;
    pub fn at(&self, x: u32, y: u32) -> Option<UvHit>;
}

/// Axis B consolidation: one barycentric lerp used by uv_at / interpolate_attr / Cage::lerp_at.
pub fn lerp3<T: Copy>(a: T, b: T, c: T, bary: [f32; 3], f: impl Fn(T, T, T, f32, f32, f32) -> T) -> T;
```

Refactors that become behavior-identical delegates (Kill 2, Kill 3):
`rasterize_uv_geometry` (S1) keeps its per-triangle bbox culling and becomes
TexelGeom assembly on top of `uv_to_barycentric` (pos = lerp3 of positions,
normal recomputed from positions — both choices preserved);
`uv_triangle_at` (S2) becomes `uv_to_barycentric(mesh, uv).map(|h| h.triangle)`
(contract pinned by polygon_fill.rs:712-718 tests). Tile routing stays in
`udim.rs`; `UvGBuffer::build` takes `tile` and uses the `tile_of_uv` inverse.

### 2b. GPU-side path (umber-gpu)

- **G-buffer textures per tile**, written by one compute pass in umber-gpu
  (shape borrowed from `POSITION_BAKE_SHADER`'s chunked loop, but the WGSL
  solve quotes `UV_DET_EPS`/`UV_BARY_EPS` from the Rust consts — asserted
  equal in a unit test — so CPU and GPU run the *same* formula; WGSL f32 is
  IEEE f32 like Rust f32, so results are bit-parity on the CPU adapter):
  - `gbuffer_bary: Rgba32Float` = (b0, b1, b2, coverage) — b2 derivable but
    stored to keep the shader a pure gather;
  - `gbuffer_prim: R32Uint` = triangle index, `u32::MAX` = uncovered.
- **Consumers switch from re-deriving to gathering:** `ID_BAKE_SHADER` (S7)
  drops its duplicated `cross2` solve and reads `gbuffer_prim`;
  `POSITION_BAKE_SHADER` (S6) either reads bary from the G-buffer or is
  subsumed by it (its pos/normal channels become G-buffer channels, killing
  the "position map as coverage authority" indirection for S9's passes);
  paint/composite/mask/bake passes that today need coverage get it from the
  G-buffer's coverage channel instead of a CPU walk; export padding's
  `covered[]` (S1 consumers) switches to GPU readback when a device exists.
- **CPU fallback (no GPU present — headless CLI, tests, wasm without
  WebGPU):** `UvGBuffer::build` is the path. Rule: **the GPU G-buffer is a
  cache/accelerator of umber-mesh's `UvGBuffer`, never the authority**.
  umber-mesh owns the math; umber-gpu mirrors it. Parity is pinned by a
  golden test comparing GPU texture readback vs `UvGBuffer` on fixtures
  (single-tile quad/plane + a 2-tile UDIM mesh) through the existing
  `crates/umber-gpu/src/golden.rs` readback harness.

## 3. Migration plan — numbered kills, crate lanes mesh → gpu → app

Each kill is behavior-identical or golden-pinned. Battery per kill: rustfmt
clean, `clippy -p <crate> --all-targets -D warnings`, that crate's
`cargo test`, plus the named goldens; mesh-lane kills also re-run the row
156-A hostile corpus + panic_hunt suites (they must stay green).

1. **Kill 1 (mesh):** land `uv_gbuffer.rs` with `uv_to_barycentric`, `UvHit`,
   `UV_DET_EPS`/`UV_BARY_EPS`, `lerp3`, and `UvGBuffer::build`. Unit tests port
   the current S1/S2 semantics: NaN UV → None, empty mesh → None, degenerate
   (area < eps) skipped, edge-inclusive bary, last-in-index-order overlap,
   and a boundary test pinning the eps unification at 1e-10.
   Battery: `cargo test -p umber-mesh` (115 lib tests stay green) + hostile
   corpus/panic_hunt + clippy.
2. **Kill 2 (mesh):** refactor `rasterize_uv_geometry` (S1) and
   `uv_triangle_at` (S2) to delegate to `uv_to_barycentric`. Behavior-
   identical. Golden-pinned by the existing fill tests (fill.rs, fill_layer
   1494-1596) and polygon_fill.rs:712-718. Battery: mesh + app fill suites.
3. **Kill 3 (mesh):** consolidate Axis B — `interpolate_attr`/`uv_at`
   (raycast.rs) and `Cage::lerp_at` (bake_support.rs:60) route through
   `lerp3`; ray axis (S3) untouched otherwise. Battery: raycast + bake_support
   tests, `cargo test -p umber-mesh -p umber-bake`.
4. **Kill 4 (gpu):** add the G-buffer compute pass + textures in umber-gpu
   (renderer upload path), WGSL quoting umber-mesh's eps consts; pool the
   textures per tile via the existing `tile_pool.rs`. Verification: golden
   readback fixtures (single-tile + 2-tile UDIM) asserting GPU G-buffer ==
   CPU `UvGBuffer` bit-for-bit on the CPU adapter; `golden.rs` harness;
   clippy -p umber-gpu.
5. **Kill 5 (gpu):** switch `ID_BAKE_SHADER` (S7) to read `gbuffer_prim`, and
   `POSITION_BAKE_SHADER` (S6) to read bary from the G-buffer (or be subsumed
   by it). Golden-pinned by the bake determinism/golden tests (umber-bake
   ao.rs determinism tests, id.rs:429 fnv1a goldens, bake_shaders tests) and
   the gpu golden suite. Battery: `cargo test -p umber-gpu -p umber-bake` on
   lavapipe.
6. **Kill 6 (gpu→export/app padding):** export padding `covered[]`
   (`umber-cli/main.rs:1027`, `export_dialog.rs:421`, umber-export driver)
   reads the G-buffer when a device exists, else `UvGBuffer::build` —
   byte-identical padded outputs pinned by the row-151 dilation tests.
   Battery: `cargo test -p umber-export -p umber-cli`, app export tests.
7. **Kill 7 (app):** `fill_layer.rs:594` pick and any per-texel app ops route
   through `uv_to_barycentric` / the G-buffer; `display_panel.rs` unchanged
   (toggles only — S12). Verification: fill_layer tests, viewport pick tests
   (viewport.rs:2432-2592), kittest `panel_snapshot` 8/8
   (`cargo test -p umber-app`).
8. **Kill 8 (cross-cutting parity):** per-tile CPU-vs-GPU parity test on the
   ref scene at 4K (prim equal, bary within 0 ULP on the CPU adapter, ≤ 2 ULP
   tolerance on real GPUs for FMA contraction), run in the gpu golden lane.
   Battery: full `cargo test --workspace --no-fail-fast` + fmt + workspace
   clippy before the row closes.

## 4. Risks

- **Perf — §12 input-to-photon gate.** Budget < 20 ms @ 60 Hz
  (docs/specs/requirements.md:162-166); stage-3 baseline p95 **10.19 ms**,
  latest stage-5 row 10.18 ms (docs/perf/protocol.md:156, llvmpipe adapter —
  this host has no reachable real GPU). The G-buffer row must not add work
  to the stroke path: the 3D-viewport pick (viewport.rs:1744 → S3) stays a
  ray cast, and stroke UV→texel stays the linear `local_texel` routing
  (paint_state.rs:5943). **Hard rule: never build the CPU `UvGBuffer` (16.7 M
  texels × tri loop at 4K) on the paint hot path** — per-texel work moves to
  the one GPU pass, per-pointer work stays O(ray). Gate: after Kill 6, re-run
  the docs/perf/protocol.md input-to-photon session and compare against the
  10.19 ms stage-3 p95 baseline before the row closes.
- **Memory.** G-buffer at 4K/tile ≈ 16.7 M texels × (16 B bary + 4 B prim)
  ≈ 350 MB/tile worst case — must be per-active-tile and pool-capped
  (`tile_pool.rs`), never whole-mesh. Consider Rgba16Float bary packing in a
  later kill; v1 ships Rgba32Float + R32Uint for exact parity.
- **UDIM tiles.** G-buffer is per tile under `tile_of_uv`'s policy (NaN →
  FIRST_TILE, udim.rs:48); `ao.rs:630`'s clamp mirrors it — Kill 1 adds one
  shared clamp helper in `udim.rs` so the mirror dies. Texel centers
  `(x+0.5)/w < 1` never land exactly on an integer UV boundary, so no
  cross-tile bleed; but bary eps `1e-6` is absolute in bary space
  (scale-invariant, good) while `UV_DET_EPS = 1e-10` is in UV² space
  (scale-dependent): UV footprints with |area| < 1e-10 (islands smaller than
  ~1e-5 UV units) drop out — documented as the degenerate-by-contract
  boundary.
- **Degenerate triangles / epsilon divergence (live today).** CPU fill (S1,
  S2) uses area eps `1e-12`; GPU bakes (S6, S7) use `1e-10` — a triangle with
  `1e-12 < |area| < 1e-10` is covered by CPU fill but NOT by GPU bake.
  Kill 1 unifies to `UV_DET_EPS = 1e-10` everywhere and pins the change with
  a boundary test; this is the one intentional behavior change in the whole
  plan and it is golden-pinned, not silent.
- **Float precision at tile borders.** Large tiles (1100 → tu,tv up to 9)
  push UVs to ~10; f32 mantissa (~24 bit) still resolves 1e-6 relative at
  that magnitude, and texel centers never sit on tile borders, so border
  texels are safe. CPU↔GPU bit parity: same IEEE f32 formula + same eps gives
  0-ULP agreement on the CPU adapter; real GPUs may contract to FMA —
  golden tolerance ≤ 2 ULP (golden.rs already compares with tolerance).

## 5. Non-goals

- The 3D **ray axis** (raycast.rs `ray_intersect`, bvh.rs, the Möller–Trumbore
  in AO/transfer shaders): the G-buffer does not replace raycasting;
  viewport picking keeps per-pointer rays (a future depth-buffer pick row is
  separate).
- Wireframe overlay bary attribute (renderer.rs:1370, overlay.rs) — display
  only, stays as-is.
- Changing the overlap rule (last-in-index-order wins) — preserved exactly;
  overlap *detection/fix* is the Wave-3 seam work, not this row.
- Cage/transfer gate semantics (bake_support.rs `TransferClamps`), undo
  model, paint-thread architecture, wasm builds.
- Any perf re-architecture of the paint thread — this row adds zero
  per-stroke CPU work by construction (§4 rule).
