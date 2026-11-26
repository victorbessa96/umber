# Kill brief: D5 ground plane (baker power row, SPP census D5)

- Status: DESIGN ONLY. Written 2026-11-16 by a Razul-dispatched subagent under kill-lock `razul-d5-ground-plane` (docs only, no code touched).
- Census source: `docs/loop/audit-2026-10-24-lens-spp.md:37` — "D1 baker sync / D2 auto-cage / D3 opacity baker / D4 ID sources / D5 ground plane | per census | unchanged".
- ROADMAP scoreboard: `docs/ROADMAP.md:339` — "Baking | PARITY (per-set masked bakes closed; D1 baker sync LANDED 2026-11-11 row 377; D2-D5 power rows open — next-steps list item B)".
- Next-steps list: `docs/loop/next-steps-2026-11-16.md:29` — item B.7 "D5 ground plane. Same loop."
- Sibling brief structure to mirror: `docs/loop/briefs/2026-11-16-d2-auto-cage.md` (D2 auto-cage, landed brief format), `docs/loop/briefs/2026-11-16-d3-opacity-baker.md` (D3 opacity baker), `docs/loop/briefs/2026-11-16-d4-id-sources.md` (D4 ID sources).
- D1 persistence pattern to mirror: `docs/loop/briefs/2026-11-11-rd1-baker-sync.md` (BakeSync additive fields, `..Default::default()` on load, no project version bump).

## What exists (measured on live tree)

Bake kind registry (`umber-bake` lib.rs + `bakes_panel.rs`):
- `BakeMap` enum at `crates/umber-bake/src/lib.rs:100-114` lists 11 maps (`AmbientOcclusion` through `Skew`); **NO `GroundPlane` variant**.
- `BakeSelection` enum at `crates/umber-app/src/bakes_panel.rs:218-249` lists 10 selectable maps (`Ao` through `Opacity`); **NO ground plane variant**. `ALL` array at `:254-265` is the panel's checklist order.
- `BakeSettingId` at `:447-468` enumerates settings rows the log jumps to; ground plane would need rows (e.g. `GroundPlaneSize`, `GroundPlaneHeight`, `GroundPlaneSubdivs`, `GroundPlaneMaps`).
- `settings_snapshot()` at `:1469-1551` builds the per-baker param map for `BakeSync` — ground plane params must be additive here.
- `apply_settings()` at `:1557-1682` loads params back; ground plane params must be read with defaults.
- `plan_bake()` at `:388-428` fans out `BakeJob`s; a new `BakeSelection::GroundPlane` must be handled (single-tile only in v1, like curvature/position/opacity/IdMulti).

High-poly panel group + transfer maps (C2-29):
- `HighPoly` struct at `:1247-1300` carries transfer maps only (`normal`, `height`); ground plane is a separate concept — a generated mesh beneath the scene, not a high-poly source.

Per-set masked bakes context (`per-set-masked-bakes.md` §3):
- Bakes take `(raster mesh, occluder mesh)`; today same argument for self-bakes. Ground plane is a **new raster target** (the generated plane) with the scene mesh as occluder — matches the per-set masked bakes' split: `raster = plane`, `occluder = whole mesh by default` (or `OwnSet` when the user toggles the occluder scope row).

Ground plane semantics (SPP parity):
- Substance Painter's "ground plane" in the viewport is a visual aid only — it does not bake. Marmoset Toolbag bakes AO/height onto a ground plane via the "Ground Plane" baker (separate from the mesh bakers). In Umber: **bake AO, height, and ID onto a generated ground plane mesh** beneath the high-poly source (or low mesh when no high-poly loaded), with configurable plane size, height offset below the scene bounds, and subdivisions for tessellation. The baked maps composite with the existing per-set bakes (same output naming, same dilation, same export pipeline). The ground plane is a **mesh-fed bake** (like AO mesh-fed path) — it uses the plane's own UV layout (planar, 0..1 across the plane extents) and bakes selected maps against the scene as occluder.

GPU shader pattern (mirror `ao.rs` / `thickness.rs` / `opacity.rs`):
- `crates/umber-bake/src/ao.rs:47-66` `PlaneDesc` + `AoBakeParams` (WGSL-aligned, `bytemuck::Pod`); `BakeTarget` wraps `PaintTarget`.
- `crates/umber-bake/src/thickness.rs:105-135` `GpuTriangle` + `ThicknessUniform` (16-byte aligned); `build_triangles()` from `MeshData`.
- `crates/umber-gpu/src/bake_shaders.rs`: `AO_BAKE_SHADER` (`cs_main` plane + `cs_main_from_position` mesh-fed) and `THICKNESS_BAKE_SHADER` are the two-entry-point pattern to follow. The mesh-fed entry reads `position_tex`/`normal_tex` from the position pass.
- Dispatch convention: `dispatch_workgroups(width, height, 1)` — one workgroup per texel; `local_index` fans rays within the workgroup.

Viewport ground-plane preview (D2 cage overlay precedent):
- `Viewport::show_cage` / `set_show_cage` at `viewport.rs:876-886` (bool, default `false`).
- `Viewport::set_cage_wire` at `:891-921`: builds displaced-positions wire buffer via `umber_mesh::cage_wire_vertices_from_indices`, uploads to GPU.
- D5 should add `Viewport::show_ground_plane` / `set_show_ground_plane` and `Viewport::set_ground_plane_wire` mirroring the cage pattern — the viewport preview shows the generated plane wireframe at the configured height offset, synced from the Bakes panel when ground plane params change.

## Dispatch lock list

Lock name: **`razul-d5-ground-plane`**

Locked files (exact surface at dispatch):
- `crates/umber-mesh/src/bake_support.rs` (ground plane mesh generator + tests)
- `crates/umber-bake/src/ground_plane.rs` (new module — CPU params, GPU dispatch, unit tests)
- `crates/umber-bake/src/lib.rs` (export `GroundPlaneBakeError`, `GroundPlaneParams`, `GroundPlaneMap`, `bake_ground_plane_mesh`; add `GroundPlane` to `BakeMap`)
- `crates/umber-gpu/src/bake_shaders.rs` (new `GROUND_PLANE_BAKE_SHADER` constant: mesh-fed entry point)
- `crates/umber-app/src/bakes_panel.rs` (`BakeSelection::GroundPlane` variant + `ALL`/`DEFAULT_ON`, `BakeSettingId::GroundPlaneSize`/`GroundPlaneHeight`/`GroundPlaneSubdivs`/`GroundPlaneMaps`, panel UI group, `settings_snapshot`/`apply_settings`, `plan_bake` handling)
- `crates/umber-app/src/viewport.rs` (`show_ground_plane`/`set_show_ground_plane` + `set_ground_plane_wire` + `clear_ground_plane_wire`)
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (new keys: `bakes.ground-plane`, `bakes.ground-plane-size`, `bakes.ground-plane-height`, `bakes.ground-plane-subdivs`, `bakes.ground-plane-maps`, `bakes.ground-plane-map-ao`, `bakes.ground-plane-map-height`, `bakes.ground-plane-map-id`, `bakes.show-ground-plane`, `bakes.ground-plane-hint`)
- `docs/loop/queue.md` + this brief's kill report

## Build (dispatch-time lock: `razul-d5-ground-plane`)

### 1. Ground plane mesh generator (CPU) — `umber-mesh/src/bake_support.rs`

Add ground plane generation alongside `Cage`:

```rust
/// Parameters for generating a ground plane mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundPlaneDesc {
    /// Full width/depth of the plane in world units (square).
    pub size: f32,
    /// Height offset below the scene's minimum Y (world units). Positive = below scene.
    pub height_offset: f32,
    /// Subdivisions per axis (tessellation). `subdivs = 1` → 2 triangles (1 quad).
    /// Each subdivision doubles the triangle count per axis.
    pub subdivisions: u32,
}

impl Default for GroundPlaneDesc {
    fn default() -> Self {
        Self {
            size: 10.0,
            height_offset: 0.1,
            subdivisions: 32,
        }
    }
}

/// Generates a ground plane `MeshData` centered at (0, -height_offset, 0) with
/// planar UVs covering [0, 1] × [0, 1] across the plane extents.
/// Returns `Err` if `subdivisions == 0` or `size <= 0.0`.
pub fn generate_ground_plane(desc: &GroundPlaneDesc) -> Result<MeshData, GroundPlaneError>
```

- Unit test: `ground_plane_uvs_cover_unit_square` — `subdivisions=2` produces 9 vertices, 8 triangles; UVs at corners are exactly `(0,0)`, `(1,0)`, `(0,1)`, `(1,1)`; interior vertices bilinear.
- Unit test: `ground_plane_height_offset_is_below_scene` — `height_offset=0.5` → all vertex Y == `-0.5`; `size=4.0` → X/Z span `[-2, 2]`.
- Unit test: `ground_plane_subdiv_zero_errors` — `subdivisions=0` returns `Err`.
- Unit test: `ground_plane_size_zero_errors` — `size=0.0` returns `Err`.

### 2. New baker module (`ground_plane.rs`)

**New types:**
- `GroundPlaneMap` enum (which map(s) to bake onto the plane):
  - `Ao` — ambient occlusion (hemisphere rays from plane texels against scene).
  - `Height` — distance from plane to first scene hit along `+Y` (toward the scene above).
  - `Id` — triangle ID of first hit (FNV-1a hash, same as `Id` baker).
- `GroundPlaneParams { desc: GroundPlaneDesc, maps: Vec<GroundPlaneMap>, rays: u32, max_distance: f32, bias: f32 }` — mirrors `AoBakeParams` layout (WGSL-aligned, `bytemuck::Pod`). `rays`/`max_distance`/`bias` used by AO; `Height` uses `max_distance` as ray length; `Id` ignores them.
- `validate_ground_plane(mesh: &MeshData, params: &GroundPlaneParams) -> Result<(), GroundPlaneBakeError>` — checks `mesh.triangles` non-empty (scene must have geometry), `desc.size > 0`, `desc.subdivisions > 0`, `maps` non-empty, `rays >= 1` for AO.

**CPU reference implementation:**
- `fn ground_plane_ao_cpu(plane: &MeshData, scene: &MeshData, params: &GroundPlaneParams, texel: (u32, u32)) -> f32` — single-texel AO via hemisphere sampling against scene triangles (reuses `ao.rs` hemisphere math).
- `fn ground_plane_height_cpu(plane: &MeshData, scene: &MeshData, params: &GroundPlaneParams, texel: (u32, u32)) -> f32` — ray along `+Y` from plane position, returns hit distance normalized to `max_distance`.
- `fn ground_plane_id_cpu(plane: &MeshData, scene: &MeshData, texel: (u32, u32)) -> [u8; 4]` — ray along `+Y`, returns FNV-1a hash of hit triangle index as RGBA8 (A=coverage).

**New GPU entry point:**
- `pub fn bake_ground_plane_mesh(device, queue, scene_mesh, params) -> Result<BakeOutputs, GroundPlaneBakeError>` — two-pass:
  1. Generate plane mesh via `generate_ground_plane(&params.desc)`.
  2. Rasterize plane's own UV layout into position + normal textures (`bake_position_and_normal`).
  3. Dispatch `GROUND_PLANE_BAKE_SHADER` (mesh-fed entry) reading `position_tex`/`normal_tex` + scene triangle buffer. Shader loops over `params.maps` bitmask and writes selected maps to output texture array layers (or separate dispatches per map — single dispatch with array is preferred for atomicity).
- Returns `BakeOutputs { ao: Option<Vec<u8>>, height: Option<Vec<u8>>, id: Option<Vec<u8>> }` — each `Vec<u8>` is RGBA8 row-major (`width * height * 4`).

**Unit tests (CPU-only, no GPU feature):**
- `ground_plane_ao_cpu_matches_ao_bake_on_flat_scene`: plane at Y=-0.1, scene = single quad at Y=0 → AO at plane center == 0.5 (hemisphere half-occluded by infinite plane). Exact value pinned by CPU reference.
- `ground_plane_height_cpu_returns_distance`: scene quad at Y=0, plane at Y=-0.5 → height = 0.5 exactly (normalized to `max_distance=1.0` → 0.5).
- `ground_plane_id_cpu_hashes_hit_triangle`: scene with two triangles, plane below both → hit triangle index hashed via FNV-1a matches `id.rs` CPU mirror.
- `ground_plane_validate_rejects_empty_scene`: scene mesh with zero triangles → `SceneHasNoGeometry`.
- `ground_plane_validate_rejects_zero_subdivs`: `subdivisions=0` → `InvalidSubdivisions`.

### 3. GPU shader (`GROUND_PLANE_BAKE_SHADER` in `bake_shaders.rs`)

**Uniform struct (WGSL-aligned, 16-byte aligned):**
```wgsl
struct GroundPlaneParams {
  maps_bitmask: u32,      // bit 0=AO, bit 1=Height, bit 2=Id
  rays: u32,
  max_distance: f32,
  bias: f32,
  tri_count: u32,
  _pad: vec3<u32>,        // 16-byte alignment
}
```
(32 bytes total)

**Mesh-fed entry `cs_main_from_position`:**
- `@workgroup_size(8, 8)` dispatch `dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1)`.
- Reads `position_tex` (binding 2, `texture_2d<f32>`), `normal_tex` (binding 3), scene triangles (binding 1, `buffer<GpuTriangle>`), uniform `GroundPlaneParams` (binding 0).
- For each texel: reconstruct world position from `position_tex`, normal from `normal_tex` (plane normal is constant `+Y` but read from texture for parity).
- If `maps_bitmask & 1` (AO): hemisphere loop (reuse `hemisphere_sample` + ray-triangle from `AO_BAKE_SHADER`), accumulate occlusion, write `ao = 1 - occlusion` to output layer 0 (R channel, A=coverage).
- If `maps_bitmask & 2` (Height): ray along `+normal` (toward the scene above) against scene triangles, find closest hit `t`, write `height = t / max_distance` to output layer 1 (R channel, A=coverage).
- If `maps_bitmask & 4` (Id): same ray as Height, on hit compute `hash_triangle(tri)` (reuse FNV-1a from `ID_BAKE_SHADER`), write RGB=hash color, A=255 to output layer 2.
- Unselected maps write zeros to their layer.
- Output: `texture_storage_3d<Rgba8Unorm, write>` with depth = 3 (layers: AO, Height, Id) — or three separate `texture_storage_2d` bindings if array not supported on target. Single 3-layer texture is preferred (one dispatch, atomic per texel across maps).

**Plane entry `cs_main` (stub for v1):**
- Identical dispatch but reads `PlaneUniform` (origin, u_axis, v_axis, extent) instead of position/normal textures.
- Not used in v1 (ground plane always mesh-fed since we generate the plane mesh CPU-side); kept for parity with AO/thickness/opacity pattern.

### 4. Panel wiring (`bakes_panel.rs`)

- Add `GroundPlane` to `BakeSelection` (after `Opacity`, before `Height` — low-mesh self-bake category). Update `ALL` array (11 entries) and `DEFAULT_ON` (keep 5 — ground plane is opt-in).
- Add `GroundPlaneSize`, `GroundPlaneHeight`, `GroundPlaneSubdivs`, `GroundPlaneMaps` to `BakeSettingId`.
- Settings group (enabled when `GroundPlane` checked in Maps group):
  - Slider "Size" (`bakes.ground-plane-size`): 0.1..100.0, step 0.1, default 10.0.
  - Slider "Height offset" (`bakes.ground-plane-height`): 0.0..10.0, step 0.01, default 0.1.
  - Slider "Subdivisions" (`bakes.ground-plane-subdivs`): 1..128, step 1, default 32.
  - Checkbox group "Maps" (`bakes.ground-plane-maps`): three checkboxes `Ao` (`bakes.ground-plane-map-ao`), `Height` (`bakes.ground-plane-map-height`), `Id` (`bakes.ground-plane-map-id`). At least one required (validated in `validate_ground_plane`).
- `settings_snapshot()`: for `BakeSelection::GroundPlane` emit `size` (Float), `height_offset` (Float), `subdivisions` (Int), `maps_bitmask` (Int), `rays` (Int), `max_distance` (Float), `bias` (Float) into the baker param map.
- `apply_settings()`: read back those keys with defaults (`size=10.0`, `height_offset=0.1`, `subdivisions=32`, `maps_bitmask=1` (AO only), `rays=16`, `max_distance=100.0`, `bias=0.001`).
- `plan_bake()`: `GroundPlane` uses `BakeSource::WholeMesh` for tile 1001 only (like curvature/position/opacity/IdMulti); skipped for other tiles.

Viewport sync (in `BakesPanel::show` or app shell):
- When ground plane params change (size/height/subdivs), regenerate plane mesh and call `viewport.set_ground_plane_wire(gpu, &plane_mesh, &plane_offsets)` — the plane has no cage offsets, so pass empty slice; the viewport wire function builds a flat wire at the plane's Y.
- Add `show_ground_plane: bool` to `BakesPanel` state (default `false`), synced to `viewport.set_show_ground_plane()`.

### 5. Persistence (R-D1 additive pattern)

- No new `BakeSyncCarry` fields — ground plane params live inside the per-baker `BTreeMap` that `settings_snapshot`/`apply_settings` already serialize. The `ParamValue` enum (`Int`/`Float`/`Bool`/`String`/`List`/`Map`) covers all needed types.
- Old projects load with zero ground plane settings (map unchecked, params absent) — `apply_settings` keeps current slider values for missing keys. Re-save writes no ground plane block when unchecked.

## Kittest-first definition of done

- `ground_plane_ao_cpu_matches_ao_bake_on_flat_scene`: plane at Y=-0.1, scene = single quad at Y=0 spanning [-5,5]×[-5,5] → AO at plane center (texel 256,256 at 512²) reads R≈0 (fully occluded), A=255; exact value asserted against CPU reference `ground_plane_ao_cpu`. (Corrected 2026-11-18, cross-review D5 MED: the original "R=128 (0.5 occlusion)" was wrong physics — a 10×10 quad at distance 0.1 subtends ~99% of the upper hemisphere, so occlusion ≈ 1 and AO ≈ 0. The implementation was right; the brief's expectation was not.)
- `ground_plane_height_cpu_returns_normalized_distance`: plane at Y=-0.5, scene quad at Y=0, `max_distance=10.0` → height R = 0.05 * 255 = 13 (rounded), A=255; uncovered texels (outside scene bounds) read (0,0,0,0).
- `ground_plane_id_cpu_hashes_hit_triangle`: scene with triangle index 42 covering plane center → ID map at center reads RGB = FNV-1a(42) as 24-bit color, A=255.
- `ground_plane_gpu_ao_matches_cpu_texel_for_texel`: same flat scene, 256², AO only, 16 rays → GPU output layer 0 byte-identical to `ground_plane_ao_cpu` over all texels (extends `ao.rs` CPU-GPU parity precedent).
- `ground_plane_gpu_height_matches_cpu_texel_for_texel`: same scene, Height only, `max_distance=10.0` → GPU layer 1 byte-identical to `ground_plane_height_cpu`.
- `ground_plane_gpu_id_matches_cpu_texel_for_texel`: same scene, Id only → GPU layer 2 byte-identical to `ground_plane_id_cpu`.
- `ground_plane_multi_map_single_dispatch`: AO+Height+Id bitmask = 7 → single dispatch produces three layers, each matching its single-map test above (proves no cross-talk between layers).
- `ground_plane_off_is_byte_identical_to_today`: `BakeSelection::GroundPlane` unchecked + all other maps checked → full bake plan (jobs, outputs, log) byte-for-byte identical to tree without the ground plane module (no new jobs, no new log entries, no new files). Proves zero regression when the new baker is not selected.
- `ground_plane_viewport_wire_matches_params`: `BakesPanel` with ground plane params (size=5.0, height_offset=0.2, subdivs=4) → `viewport.set_ground_plane_wire` called with mesh whose vertex Y == -0.2, X/Z span [-2.5, 2.5], subdivs=4 → 25 vertices, 32 triangles; `show_ground_plane` toggles visibility.

## Rules

- Battery before commit (all under `flock /tmp/umber-cargo.lock` + `UMBER_ASSETS_DIR=<repo root>`):
  - `rustfmt --edition 2021 --check` on touched `.rs`
  - `python3 scripts/check-layers.py`
  - `python3 scripts/scorecard.py --check`
  - `cargo clippy -p umber-bake -p umber-gpu -p umber-app -p umber-mesh --all-targets -- -D warnings`
  - `cargo test -p umber-bake -p umber-gpu -p umber-app -p umber-mesh` (0 failed)
- Both `.ftl` catalogs if keys added (ftl completeness is a battery gate).
- No weakened tests; new behavior gets new tests (kittest-first above).
- Edit only the locked surface at dispatch.
- Commit via `.dragon/dcommit.py --kill-lock razul-d5-ground-plane --lock <paths> -- -m "..."`; push via `scripts/safe-push.py` in background.

## Rejected alternatives

| Alternative | Reason |
|-------------|--------|
| Separate bake kinds per map (GroundAo, GroundHeight, GroundId) | Would need 3 bake kinds + 3 panel rows + 3 persistence fields; one baker with map bitmask avoids N× surface area. The maps share the same plane geometry and scene occluder — one dispatch, three layers. |
| Ground plane as a post-process on baked AO | Would need AO baked first, then a second pass to composite; the shader computes all three maps in one raycast loop (AO hemisphere + single up-ray for height/id). Zero extra dispatches. |
| Plane UVs from world-space triplanar | Planar UVs (0..1 across plane extents) are the standard for ground plane bakes (Marmoset, xNormal). Triplanar would distort the bake at plane edges and complicate the UV layout for no artist benefit. |
| Height map as world-space position Y | Height = distance to scene along +Y is the actionable value for displacement/parallax. World Y is recoverable (plane_y + height) but not the primary use case. |
| Per-tile ground plane bakes (like AO) | Ground plane is a single planar surface covering the scene bounds — no UDIM benefit. Single-tile raster (like curvature/position) is correct for v1. |
| Ground plane only for high-poly transfer | Ground plane bakes against the **scene mesh** (low or high), not a high-poly source. It is a self-bake category, not a transfer. The high-poly group is for transfer maps only. |

## Dragon verification

- Cross-review diff vs this brief, full battery on the landed commit, `ls-remote origin` hash check.
- Kill report at `docs/loop/kill-reports/2026-11-16-d5-ground-plane.md` with measured numbers: ground plane bake time (ms) on 512² plane at 16 rays vs AO baseline, byte-identity proof for `GroundPlane` unchecked, CPU-GPU parity texel-for-texel on all three maps, viewport wireframe sync verified.