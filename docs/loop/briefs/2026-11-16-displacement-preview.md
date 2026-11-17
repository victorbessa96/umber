# Kill brief: Displacement preview (Viewport scoreboard row, next-steps item 8)

- Status: DESIGN ONLY. Written 2026-11-16 by a Razul-dispatched subagent under kill-lock `razul-disp-preview` (docs only, no code touched).
- Census source: `docs/loop/next-steps-2026-11-16.md:30` — item B.8 \"Displacement preview in the viewport (scoreboard Viewport row)\".
- ROADMAP scoreboard: `docs/ROADMAP.md:340` — \"Viewport | PARITY (four post nodes LANDED 2026-11-11 rows 375/376; displacement preview open — next-steps list item B)\".
- Next-steps list: `docs/loop/next-steps-2026-11-16.md:30` — item B.8 \"Displacement preview in the viewport (scoreboard Viewport row). Same loop.\"
- Sibling brief structure to mirror: `docs/loop/briefs/2026-11-16-d2-auto-cage.md` (D2 auto-cage), `docs/loop/briefs/2026-11-16-d3-opacity-baker.md` (D3 opacity baker), `docs/loop/briefs/2026-11-16-d4-id-sources.md` (D4 ID sources), `docs/loop/briefs/2026-11-16-d5-ground-plane.md` (D5 ground plane).
- D1 persistence pattern to mirror: `docs/loop/briefs/2026-11-11-rd1-baker-sync.md` (BakeSync additive fields, `..Default::default()` on load, no project version bump).

## What exists (measured on live tree)

Viewport overlay toggles (umber-app):
- `Viewport::show_cage` / `set_show_cage` at `viewport.rs:886-896` (bool, default `false`).
- `Viewport::show_ground_plane` / `set_show_ground_plane` at `viewport.rs:940-949` (bool, default `false`).
- Test `cage_overlay_defaults_off_and_toggles` at `viewport.rs:3879-3883`: fresh viewport → `!show_cage()`, toggle on/off round-trips.
- Cage wire overlay drawn at `viewport.rs:1581-1600` after mesh + wireframe, skipped entirely when off (byte-identical-off contract).
- Ground plane wire overlay drawn at `viewport.rs:1601-1620` same pattern.

Height map in mesh pass (umber-gpu):
- `PaintParams::HAS_HEIGHT_TEXTURE` flag bit at `renderer.rs:131` — gates height texture binding (binding 12).
- `PaintParams::height_strength` at `renderer.rs:115` — world units of displacement per unit height (1.0 = unscaled).
- `height_perturbed_normal` at `shaders.rs:278-311` — computes perturbed normal from height gradient (finite-difference u+1/v+1 texels) carried through per-vertex tangent frame (`tangent.w` = handedness). Used in fragment shader `shade` for lit normal.
- `MESH_SHADER` vertex shader `vs_main` at `shaders.rs:191-200` — passes `position`, `normal`, `uv`, `tangent` through; clip position = `camera.view_proj * vec4(in.position, 1.0)`. **No vertex displacement today.**

Height bake (umber-bake):
- `TransferMap::Height` at `transfer.rs:63-66` — normalized hit distance along ray, centered on low surface.
- `TRANSFER_BAKE_SHADER` at `bake_shaders.rs:1292+` — bakes height from high-poly to low-poly UV map (with cage support from D2).
- CPU `Cage::lerp_at` at `bake_support.rs:75-89` — barycentric interpolation of per-vertex cage offsets (used by transfer bake; displacement preview will use a similar per-vertex displacement derived from the baked height map).

Viewport UI (Display panel / Overlays popover):
- `OverlaySettings` at `display_panel.rs` carries wireframe, grid, UV seams, face orientation, channel readout.
- Overlays popover (GUI plan g6-overlays-popover) edits `OverlaySettings`; viewport syncs via `set_overlays` / `take_overlay_edit`.

## Dispatch lock list

Lock name: **`razul-disp-preview`**

Locked files (exact surface at dispatch):
- `crates/umber-gpu/src/shaders.rs` (MESH_SHADER: vertex displacement in `vs_main`, `DisplacementParams` uniform, new binding)
- `crates/umber-gpu/src/renderer.rs` (`DisplacementParams` struct, `GpuContext` displacement buffer + bind group, `mesh_bind_group_full` wiring)
- `crates/umber-app/src/viewport.rs` (`show_displacement` / `set_show_displacement`, `displacement_strength` / `set_displacement_strength`, displacement buffer build from height map)
- `crates/umber-app/src/display_panel.rs` (OverlaySettings: `displacement` bool + `displacement_strength` f32; overlay rows for toggle + slider)
- `crates/umber-mesh/src/bake_support.rs` (CPU reference: `displace_vertex_cpu(height_map, uv, strength, normal)` — mirrors shader arithmetic exactly)
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (new keys: `overlays.displacement`, `overlays.displacement-strength`, `overlays.displacement-hint`)
- `docs/loop/queue.md` + this brief's kill report

## Build (dispatch-time lock: `razul-disp-preview`)

### 1. Shader displacement (WGSL) — `shaders.rs`

**New uniform (binding 13, group 0 — additive after height_tex at binding 12):**
```wgsl
struct DisplacementParams {
  enabled: u32,      // 0 = off (byte-identical to current vs_main), 1 = on
  strength: f32,     // world units of displacement per unit height (scales height_strength)
  _pad: vec2<u32>,   // 16-byte alignment
}
@group(0) @binding(13) var<uniform> displacement_params: DisplacementParams;
```
Total 16 bytes (16-byte aligned).

**Vertex shader `vs_main` modification:**
```wgsl
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let pos = in.position;
    // Displacement: sample height map at vertex UV, displace along normal
    if (displacement_params.enabled != 0u) {
        let tile_uv = paint_tile_uv(in.uv);
        let dims = textureDimensions(height_tex);
        let dims_f = vec2<f32>(dims);
        let max_texel = vec2<i32>(dims) - vec2<i32>(1, 1);
        let texel = clamp(vec2<i32>(floor(tile_uv * dims_f)), vec2<i32>(0, 0), max_texel);
        let h = textureLoad(height_tex, texel, 0).r; // height in [0, 1]
        let disp = (h - 0.5) * 2.0 * displacement_params.strength * paint_params.height_strength;
        pos = pos + in.normal * disp;
    }
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.world_normal = in.normal;
    out.world_pos = pos; // displaced world position for fragment
    out.uv = in.uv;
    out.tangent = in.tangent;
    return out;
}
```
- Height sampled at vertex UV (not fragment) — vertex displacement is a per-vertex operation.
- `h - 0.5` centers the height map at 0 (mid-gray = no displacement); `* 2.0` maps [0,1] → [-1,1]; scaled by `displacement_strength` (user slider) × `height_strength` (per-tile paint param, already in `PaintParams`).
- When `enabled == 0`, vertex position is unchanged — **byte-identical to current `vs_main`** (no branches in the off path that affect numerics).
- Displaced `world_pos` passed to fragment so lighting uses displaced position (correct for parallax feel).

**Fragment shader:** unchanged except `world_pos` now displaced — `height_perturbed_normal` still runs on the displaced normal (which is the original normal; true displaced normal would need per-vertex height gradient, v2 work). The visual is vertex displacement + normal perturbation = coherent look.

### 2. Renderer wiring — `renderer.rs`

**New `DisplacementParams` (CPU mirror of WGSL):**
```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DisplacementParams {
    pub enabled: u32,
    pub strength: f32,
    pub _pad: [u32; 2],
}
const _: () = assert!(size_of::<DisplacementParams>() == 16);
```

**`GpuContext` additions:**
- `displacement_params_buffer: wgpu::Buffer` (uniform, 16 bytes)
- `displacement_bind_group: wgpu::BindGroup` (layout with binding 13)
- `update_displacement_params(enabled: bool, strength: f32)` — writes uniform, called from `Viewport` each frame when toggle/strength change.

**Pipeline layout:** add binding 13 to `bind_group_layout` (group 0). The mesh pipeline already has bindings 0–12; binding 13 is additive — existing bind groups for wire/cage/ground-plane use `fallback_wire_bind_group` which binds dummy resources for all slots, so they continue working without the displacement buffer bound (the dummy buffer reads as 0 → `enabled=0`).

**`mesh_bind_group_full`:** add `displacement_params_buffer` at binding 13 entry. Called from `Viewport::ui` when building the mesh callback.

### 3. Viewport state + buffer build — `viewport.rs`

**New fields in `Viewport`:**
```rust
show_displacement: bool,          // default false (byte-identical-off)
displacement_strength: f32,       // default 1.0 (unscaled)
displacement_buffer: Option<wgpu::Buffer>, // optional: if we want to pre-upload height map to a separate buffer for vertex fetch (v1: sample from height_tex directly in vertex shader — no extra buffer needed)
```
Actually, the vertex shader samples `height_tex` directly (binding 12), so no extra vertex buffer needed. The height texture is already bound for the fragment `height_perturbed_normal`.

**New methods:**
```rust
pub fn set_show_displacement(&mut self, on: bool) { self.show_displacement = on; }
pub fn show_displacement(&self) -> bool { self.show_displacement }
pub fn set_displacement_strength(&mut self, strength: f32) { self.displacement_strength = strength.clamp(0.0, 10.0); }
pub fn displacement_strength(&self) -> f32 { self.displacement_strength }
```

**In `ui` (before mesh callback build):**
```rust
if self.show_displacement && self.mesh.is_some() && self.paint_params.flags & PaintParams::HAS_HEIGHT_TEXTURE != 0 {
    gpu.update_displacement_params(true, self.displacement_strength);
} else {
    gpu.update_displacement_params(false, 0.0);
}
```
Then `mesh_bind_group_full` includes the displacement uniform.

### 4. Display panel overlay rows — `display_panel.rs`

**`OverlaySettings` additions:**
```rust
pub displacement: bool,
pub displacement_strength: f32,
```

**`overlay_rows` additions (in the Overlays popover, after wireframe/grid/UV seams):**
```rust
ui.checkbox(&mut settings.displacement, tr("overlays.displacement"));
if settings.displacement {
    ui.add(
        ValueField::new(&mut settings.displacement_strength)
            .range(0.0..=10.0)
            .step(0.01)
            .decimals(2)
            .default_value(1.0)
    ).on_hover_text(tr("overlays.displacement-hint"));
}
```

**Viewport sync (in `Viewport::set_overlays`):**
```rust
self.show_displacement = overlays.displacement;
self.displacement_strength = overlays.displacement_strength.clamp(0.0, 10.0);
```

**`take_overlay_edit`:** returns `Some` when displacement or strength changed (shell writes back to `ViewToggles` — not yet implemented, but the edit is captured).

### 5. CPU reference parity — `bake_support.rs`

**New function (pure, no GPU):**
```rust
/// CPU mirror of the vertex shader displacement arithmetic.
/// Returns displaced position = `pos + normal * disp` where
/// `disp = (h - 0.5) * 2.0 * strength * height_strength`.
/// `h` is the height map value at `uv` (bilinear would match fragment,
/// but vertex shader uses point-sample at texel center — this fn does the same).
/// Returns `None` if `uv` out of bounds or `height_map` empty.
pub fn displace_vertex_cpu(
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    height_map: &[u8], // RGBA8, row-major, width*height*4
    width: u32,
    height: u32,
    strength: f32,
    height_strength: f32,
) -> Option<[f32; 3]> {
    // Tile UV (mirrors paint_tile_uv)
    let tile_uv = [
        (uv[0] - paint_params.uv_offset[0]) * paint_params.uv_scale[0],
        (uv[1] - paint_params.uv_offset[1]) * paint_params.uv_scale[1],
    ];
    // Point sample at texel center (matches vertex shader textureLoad)
    let dims_f = [width as f32, height as f32];
    let texel = [
        (tile_uv[0] * dims_f[0]).floor() as i32,
        (tile_uv[1] * dims_f[1]).floor() as i32,
    ];
    let max_texel = [width as i32 - 1, height as i32 - 1];
    let texel = [
        texel[0].clamp(0, max_texel[0]) as usize,
        texel[1].clamp(0, max_texel[1]) as usize,
    ];
    let idx = (texel[1] * width as usize + texel[0]) * 4;
    let h = height_map[idx] as f32 / 255.0; // R channel
    let disp = (h - 0.5) * 2.0 * strength * height_strength;
    Some([
        pos[0] + normal[0] * disp,
        pos[1] + normal[1] * disp,
        pos[2] + normal[2] * disp,
    ])
}
```
(Uses the same `paint_params` conventions — in practice `paint_params` is passed or the offset/scale are args; the key is IEEE operation order matches WGSL exactly.)

**Unit test:** `displace_vertex_cpu_matches_shader_arithmetic` — same inputs → output bit-identical to a hand-computed reference using the exact operation order (IEEE `*` commutative, sum order fixed).

### 6. Persistence (R-D1 additive pattern)

- `OverlaySettings` is serialized via `BakeSync` / project file (already handled by the existing settings sync machinery). New fields `displacement` (bool) and `displacement_strength` (f32) are additive — `..Default::default()` on load gives `false` + `1.0`. No project version bump.
- Old projects load with displacement off, strength 1.0 (same as a fresh session).

## Kittest-first definition of done

- `displacement_off_is_byte_identical_to_today`: fresh viewport, mesh with height map bound, `show_displacement = false` → rendered frame (color + depth) byte-identical to a tree without the displacement code (proves zero regression when off). Test captures the paint callback output via `egui_kittest` harness.
- `displacement_enabled_displaces_vertices`: plane mesh (4 verts, UV 0..1), height map = gradient 0→1 along U, `strength=1.0`, `height_strength=1.0` → vertex X positions displaced by `±0.5` (h=0 → -0.5, h=1 → +0.5). Test reads back vertex positions from the GPU callback's vertex buffer (or compares rendered depth against CPU `displace_vertex_cpu`).
- `displacement_strength_scales_linearly`: same plane, `strength=0.5` → displacement half of `strength=1.0` (bit-exact: `0.5` is exact in binary FP).
- `displacement_height_strength_combines`: `strength=2.0`, `height_strength=0.5` → combined scale = 1.0 (matches `strength=1.0`, `height_strength=1.0`).
- `displacement_centered_at_midgray`: height map all 128 (0.5) → zero displacement (vertex positions unchanged).
- `displacement_cpu_gpu_parity_texel_for_texel`: same plane + height map, CPU `displace_vertex_cpu` over all 4 vertices vs GPU vertex positions (via transform feedback or depth readback) → byte-identical.
- `displacement_without_height_texture_is_noop`: `show_displacement = true` but no height texture bound (`HAS_HEIGHT_TEXTURE` clear) → vertex positions unchanged (shader branch not taken, `height_tex` is fallback 1×1 = 0.5 → `h-0.5=0`).

## Rules

- Battery before commit (all under `flock /tmp/umber-cargo.lock` + `UMBER_ASSETS_DIR=<repo root>`):
  - `rustfmt --edition 2021 --check` on touched `.rs`
  - `python3 scripts/check-layers.py`
  - `python3 scripts/scorecard.py --check`
  - `cargo clippy -p umber-gpu -p umber-app -p umber-mesh --all-targets -- -D warnings`
  - `cargo test -p umber-gpu -p umber-app -p umber-mesh` (0 failed)
- Both `.ftl` catalogs if keys added (ftl completeness is a battery gate).
- No weakened tests; new behavior gets new tests (kittest-first above).
- Edit only the locked surface at dispatch.
- Commit via `.dragon/dcommit.py --kill-lock razul-disp-preview --lock <paths> -- -m "docs(umber): displacement preview design brief (next-steps item 8)"`; push via `scripts/safe-push.py` in background.

## Rejected alternatives

| Alternative | Reason |
|-------------|--------|
| Fragment-shader displacement (parallax occlusion mapping) | Requires per-fragment ray marching against height map — heavy ALU, not a preview. Vertex displacement is one sample per vertex, trivial cost, and matches the \"preview\" scope (visualizing the baked height map on the mesh). |
| Separate displacement map (not the height channel) | The baked height map IS the displacement source. Adding a second map doubles bake storage, panel UI, and export slots for no v1 gain. The height channel already exists and is bound. |
| Tessellation shader for true micropoly displacement | Needs hull/domain shaders, dynamic tessellation factors — wgpu support is spotty and the architecture doc reserves this for v3. Vertex displacement on the base mesh is the correct v1 preview. |
| CPU-side mesh deformation (re-upload vertex buffer) | Would need a copy of the mesh per displacement setting, re-upload on every strength change. Shader uniform + vertex texture fetch is one draw, zero CPU-GPU sync. |
| Displacement in the position bake pass only | Position bake is a UV raster; displacement preview is a viewport visualization. They serve different stages (bake vs. view) and the viewport needs it live while painting. |

## Dragon verification

- Cross-review diff vs this brief, full battery on the landed commit, `ls-remote origin` hash check.
- Kill report at `docs/loop/kill-reports/2026-11-16-displacement-preview.md` with measured numbers: displacement toggle on/off frame time delta (ms) on 50k-tri mesh at 4K, byte-identity proof for `enabled=false`, CPU-GPU vertex parity on 4-vertex plane + gradient height map.