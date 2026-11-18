# Kill brief: D3 opacity baker (baker power row, SPP census D3)

- Status: DESIGN ONLY. Written 2026-11-16 by a Razul-dispatched subagent under kill-lock `razul-d3-opacity-baker` (docs only, no code touched).
- Census source: `docs/loop/audit-2026-10-24-lens-spp.md:37` — "D1 baker sync / D2 auto-cage / D3 opacity baker / D4 ID sources / D5 ground plane | per census | unchanged".
- ROADMAP scoreboard: `docs/ROADMAP.md:339` — "Baking | PARITY (per-set masked bakes closed; D1 baker sync LANDED 2026-11-11 row 377; D2-D5 power rows open — next-steps list item B)".
- Next-steps list: `docs/loop/next-steps-2026-11-16.md:27` — item B.5 "D3 opacity baker. Same loop.".
- D1 persistence pattern to mirror: `docs/loop/briefs/2026-11-11-rd1-baker-sync.md` (BakeSync additive fields, `..Default::default()` on load, no project version bump).
- Sibling brief structure to mirror: `docs/loop/briefs/2026-11-16-d2-auto-cage.md` (D2 auto-cage, landed brief format).

## What exists (measured on live tree)

Bake kind registry (`umber-bake` lib.rs + `bakes_panel.rs`):
- `BakeMap` enum at `crates/umber-bake/src/lib.rs:100-114` lists 11 maps (`AmbientOcclusion` through `Skew`); NO `Opacity` variant.
- `BakeSelection` enum at `crates/umber-app/src/bakes_panel.rs:191-216` lists 8 selectable maps (`Ao` through `BentNormal` + `Height`); NO `Opacity` variant. `ALL` array at `:220-229` is the panel's checklist order.
- `BakeSettingId` at `:407-420` enumerates settings rows the log jumps to; opacity would need a row (e.g. `OpacitySource`).
- `settings_snapshot()` at `:1358-1393` builds the per-baker param map for `BakeSync` — opacity params must be additive here.
- `apply_settings()` at `:1399-1432` loads params back; opacity params must be read with defaults.
- `plan_bake()` at `:348-388` fans out `BakeJob`s; a new `BakeSelection::Opacity` must be handled (single-tile only in v1, like curvature/position).
- `HighPoly` struct at `:1247-1300` carries transfer maps only; opacity is a LOW-mesh self-bake, not a transfer.

GPU shader pattern (mirror `ao.rs` / `thickness.rs`):
- `crates/umber-bake/src/ao.rs:47-66` `PlaneDesc` + `AoBakeParams` (WGSL-aligned, `bytemuck::Pod`); `BakeTarget` wraps `PaintTarget`.
- `crates/umber-bake/src/thickness.rs:105-135` `GpuTriangle` + `ThicknessUniform` (16-byte aligned); `build_triangles()` from `MeshData`.
- `crates/umber-gpu/src/bake_shaders.rs`: `AO_BAKE_SHADER` (`cs_main` plane + `cs_main_from_position` mesh-fed) and `THICKNESS_BAKE_SHADER` are the two-entry-point pattern to follow. The mesh-fed entry reads `position_tex`/`normal_tex` from the position pass.
- Dispatch convention: `dispatch_workgroups(width, height, 1)` — one workgroup per texel; `local_index` fans rays within the workgroup.

Per-set masked bakes context (`per-set-masked-bakes.md` §3):
- Bakes take `(raster mesh, occluder mesh)`; today same argument for self-bakes. Opacity rays would hit the WHOLE mesh (default `OccluderScope::WholeMesh`).
- Curvature/position/normal/ID already bake per-set from submesh (raster split landed); AO/thickness/bent/transfer keep whole-mesh occluders. Opacity follows the self-bake pattern (raster = set submesh, occluder = whole mesh by default).

Opacity semantics (SPP parity):
- Substance Painter's opacity baker produces a grayscale mask from cavity/AO or a user-provided ID/mask map — written to an alpha/opacity channel. In Umber: per-texel opacity derived from a chosen *source map* (cavity/AO baked on the fly, or an existing ID map), output as single-channel (R) or RGBA with alpha = opacity. Matches SPP's "opacity map from cavity" and "opacity map from mask" modes.

## Dispatch lock list

Lock name: **`razul-d3-opacity-baker`**

Locked files (exact surface at dispatch):
- `crates/umber-bake/src/opacity.rs` (new module — CPU params, GPU dispatch, unit tests)
- `crates/umber-bake/src/lib.rs` (export `OpacityBakeError`, `OpacityParams`, `bake_opacity_mesh`; add `Opacity` to `BakeMap`)
- `crates/umber-gpu/src/bake_shaders.rs` (new `OPACITY_BAKE_SHADER` constant: plane + mesh-fed entry points)
- `crates/umber-app/src/bakes_panel.rs` (`BakeSelection::Opacity` variant + `ALL`/`DEFAULT_ON`, `BakeSettingId::OpacitySource`, panel UI row, `settings_snapshot`/`apply_settings`, `plan_bake` handling)
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (new keys: `bakes.map-opacity`, `bakes.opacity-source`, `bakes.opacity-source-cavity`, `bakes.opacity-source-id`, `bakes.opacity-strength`, `bakes.opacity-invert`)
- `docs/loop/queue.md` + this brief's kill report

## Build (dispatch-time lock: `razul-d3-opacity-baker`)

1. **New baker module (`opacity.rs`)**:
   - `OpacityParams { source: OpacitySource, strength: f32, invert: bool, max_distance: f32, bias: f32, rays: u32 }` — mirrors `AoBakeParams`/`ThicknessParams` layout (WGSL-aligned structs, `bytemuck::Pod`).
   - `OpacitySource` enum: `Cavity` (bake AO on the fly, use as opacity), `IdMap` (sample an existing ID map's coverage), `Manual` (uniform value = `strength`).
   - `bake_opacity_mesh(mesh, params, width, height)` — two-pass like AO/thickness: (a) `bake_position_and_normal` for mesh-fed path, (b) `OPACITY_BAKE_SHADER` dispatch. Returns `Vec<u8>` RGBA8 (R = opacity, A = coverage).
   - `bake_opacity_tile(...)` for UDIM tile support (AO-only in v1; opacity is single-tile like curvature).
   - Unit test: `opacity_cavity_matches_ao_grayscale` — same mesh, `OpacitySource::Cavity` + `strength=1.0` + `invert=false` → output R channel byte-equal to AO bake (AO = 1-occlusion, opacity = occlusion when inverted; test pins the exact transform).

2. **GPU shader (`OPACITY_BAKE_SHADER` in `bake_shaders.rs`)**:
   - Plane entry `cs_main`: identical dispatch to AO plane path; reads `params.source` (u32 enum), `strength`, `invert`, `max_distance`, `bias`, `rays`. For `Cavity`: replicates AO's hemisphere loop but writes `opacity = (1.0 - ao) * strength` (inverted AO), then `invert` flips it. For `IdMap`: not supported in plane path (mesh-fed only).
     - **2026-11-18 resolution (cross-review D3 MED, brief `2026-11-18-d3-opacity-plane-dispatch.md`)**: the plane entry was never dispatched (the pipeline ran `cs_main` with a hardcoded origin plane while the mesh-fed entry sat unused) and cannot serve arbitrary `PlaneDesc`. Contract narrowed: opacity is mesh-fed only (`cs_main_from_position`); the plane entry was removed.
   - Mesh-fed entry `cs_main_from_position`: reads `position_tex`/`normal_tex` + optional `id_tex` (binding 6, `texture_2d<f32>`). For `Cavity`: same as AO but outputs opacity. For `IdMap`: samples `id_tex` at texel; coverage (`id_tex.a > 0.5`) → `opacity = strength`, else `0.0`. `invert` flips. Output `vec4(opacity, opacity, opacity, coverage)`.
   - Uniform struct `OpacityParams { source: u32, strength: f32, invert: u32, max_distance: f32, bias: f32, rays: u32, tri_count: u32, _pad: u32 }` (32 bytes, 16-byte aligned).

3. **Panel wiring (`bakes_panel.rs`)**:
   - Add `Opacity` to `BakeSelection` (after `BentNormal`, before `Height` — low-mesh self-bake). Update `ALL` array (9 entries) and `DEFAULT_ON` (keep 5 — opacity is opt-in).
   - Add `OpacitySource` to `BakeSettingId` (for log jump-to-setting).
   - Settings group: combo box "Opacity source" (`Cavity` / `ID map` / `Manual`) + slider "Strength" (0..1, step 0.01) + checkbox "Invert". Only enabled when `Opacity` is checked in Maps group.
   - `settings_snapshot()`: for `BakeSelection::Opacity` emit `source` (Int), `strength` (Float), `invert` (Bool) into the baker param map.
   - `apply_settings()`: read back those three keys with defaults (`Cavity`, `1.0`, `false`).
   - `plan_bake()`: `Opacity` uses `BakeSource::WholeMesh` for tile 1001 only (like curvature/position); skipped for other tiles.

4. **Persistence (R-D1 additive pattern)**:
   - No new `BakeSyncCarry` fields — opacity params live inside the per-baker `BTreeMap` that `settings_snapshot`/`apply_settings` already serialize. The `BakeParamCarry` enum (`Float`/`Int`/`Bool`) at `project.rs:350-357` covers all three types.
   - Old projects load with zero opacity settings (map unchecked, params absent) — `apply_settings` keeps current slider values for missing keys. Re-save writes no opacity block when unchecked.

## Kittest-first definition of done

- `opacity_cavity_matches_ao_grayscale`: same low mesh, `OpacitySource::Cavity`, `strength=1.0`, `invert=false` → output R channel == AO bake's R channel (AO = 1-occlusion; opacity = occlusion = 1-AO; test computes both and asserts byte equality after the `1 - ao` transform).
- `opacity_id_map_masks_coverage`: mesh with two materials, bake ID map first, then opacity with `OpacitySource::IdMap`, `strength=1.0` → texels where ID map has coverage (alpha=255) read opacity=255; uncovered texels read opacity=0; `invert=true` flips.
- `opacity_manual_uniform`: `OpacitySource::Manual`, `strength=0.37` (non-representable float) → every covered texel reads R=95 (0.37*255 rounded), A=255; uncovered reads (0,0,0,0). Bit-exact on readback.
- `opacity_off_is_byte_identical_to_today`: `BakeSelection::Opacity` unchecked + all other maps checked → the full bake plan (jobs, outputs, log) is byte-for-byte identical to a tree without the opacity module (no new jobs, no new log entries, no new files). Proves zero regression when the new baker is not selected.
- `opacity_mesh_fed_matches_plane`: same mesh baked via mesh-fed path (position+normal textures) and plane path (PlaneDesc covering the mesh bounds) with `OpacitySource::Cavity` → outputs byte-identical (same ray count, same seed order, same IEEE arithmetic — the AO/thickness precedent at `ao.rs:72` `lerp3` property).
  - **2026-11-18 resolution**: plane path removed (contract narrowed to mesh-fed); the test became `opacity_mesh_fed_double_bake_is_byte_identical` plus a source-level guard that the pipeline dispatches `cs_main_from_position`.

## Rules

- Battery before commit (all under `flock /tmp/umber-cargo.lock` + `UMBER_ASSETS_DIR=<repo root>`):
  - `rustfmt --edition 2021 --check` on touched `.rs`
  - `python3 scripts/check-layers.py`
  - `python3 scripts/scorecard.py --check`
  - `cargo clippy -p umber-bake -p umber-gpu -p umber-app --all-targets -- -D warnings`
  - `cargo test -p umber-bake -p umber-gpu -p umber-app` (0 failed)
- Both `.ftl` catalogs if keys added (ftl completeness is a battery gate).
- No weakened tests; new behavior gets new tests (kittest-first above).
- Edit only the locked surface at dispatch.
- Commit via `.dragon/dcommit.py --kill-lock razul-d3-opacity-baker --lock <paths> -- -m "..."`; push via `scripts/safe-push.py` in background.

## Rejected alternatives

| Alternative | Reason |
|-------------|--------|
| Separate alpha-only output texture | Would require a second `BakeTarget` + dilation pass + export slot; RGBA8 with R=opacity, A=coverage fits the existing `PaintTarget`/`BakeTarget` machinery and matches AO/thickness byte layout. |
| Opacity as a post-process on baked AO | Would need AO baked first, then a second dispatch to remap; the shader can compute `1 - ao` inline with zero extra cost (same ALU as AO's `clamp(1 - blocked)`). |
| ID map as a separate bake kind + composite | The ID map is already a baked map; sampling it in the opacity shader avoids a new bake kind, a new panel row, and a new persistence field. One shader branch, not two bakers. |
| Cavity-only (no ID/Manual sources) | SPP exposes all three; the ID-map branch is ~10 lines in the shader and gives artists the "mask-driven opacity" workflow Painter users expect. Manual is the trivial uniform fallback. |
| Per-tile opacity bakes (like AO) | Opacity is a coverage mask — it has no hemisphere sampling that benefits from tile windows. Single-tile raster (like curvature/position) is correct for v1; UDIM support is D4+ work. |

## Dragon verification

- Cross-review diff vs this brief, full battery on the landed commit, `ls-remote origin` hash check.
- Kill report at `docs/loop/kill-reports/2026-11-16-d3-opacity-baker.md` with measured numbers: opacity bake time (ms) on 512² mesh at 16 rays vs AO baseline, byte-identity proof for `Opacity` unchecked, bit-exact `Manual` strength=0.37 readback.