# Kill brief — 2026-11-09 — wave 2.1 kill: R-A2 v3 depth/occlusion culling

Implements `docs/loop/briefs/2026-11-09-projection-occlusion-design.md`.
Lock: `razul-projection-occlusion-w21`. Authorship: claw writes, dragon
reviews + batteries + measures.

## Surface (exactly these files)

- `crates/umber-mesh/src/depth.rs` (NEW)
- `crates/umber-mesh/src/lib.rs`
- `crates/umber-app/src/projection.rs`
- `crates/umber-app/src/display_panel.rs`
- `crates/umber-app/src/main.rs`
- `assets/i18n/en-US.ftl`
- `assets/i18n/pt-BR.ftl`

Nothing else. Do NOT commit/push. Do NOT run cargo.

## The change

1. `depth.rs` (new module in umber-mesh): `pub fn rasterize_depth(mesh:
   &MeshData, view_proj: glam::Mat4, width: u32, height: u32) -> Vec<f32>`.
   Screen-space triangle raster: project each position-valid triangle
   (`triangle_positions_finite` — W1.2 rule applies here too) through
   `view_proj`, clamp per-vertex `w <= NEAR_EPS` to `NEAR_EPS` for the
   projection only, bounding-box raster with screen-space barycentrics,
   per pixel keep the minimum `clip.w` (ties: last in index order wins,
   same winner convention as everywhere else). Buffer init
   `f32::INFINITY`. Never panics on malformed input. Register
   `pub mod depth;` + re-export in `lib.rs`. Doc comments name the route
   and the near-plane clamp decision.
2. `projection.rs`:
   - `ProjectionState.occlusion: bool`, default `true`; adopt keeps it;
     arm/disarm never touch it (same discipline as `cull_backfaces`).
   - `rasterize_with` builds `depth = rasterize_depth(mesh,
     camera.view_proj(rw/rh), rw, rh)` once per call (frozen and live
     paths share it — the camera passed in IS the right one in both) and
     threads `&depth` into `shade`.
   - In `shade`, after `screen` is computed and before sampling:
     `if state.occlusion { let px = screen clamped into [0,rw)x[0,rh);
     if depth[py*rw+px] < clip.w - OCCL_EPS*clip.w { return None; } }`
     (`OCCL_EPS` a named pub const, e.g. 1e-4 — pin the choice). Off-view
     screen px: clamp (the NDC guard already rejects outside [-1,1], so
     px is always in range — assert it).
   - Module docs updated to name the landed v3 behavior.
3. `display_panel.rs`: an "Occlusion" checkbox in the projection section
   after the Cull-backfaces one, same shape as the v2c one: enabled with
   an image, writes through a new `ProjectionRow::occlusion: &mut bool`.
4. `main.rs`: pass `occlusion: &mut state.occlusion` at the projection
   row construction (~line 1678 where `cull_backfaces` is passed).
5. Both `.ftl` catalogs, next to the cull keys (~en-US:1013):
   `.projection-occlusion` + `.projection-occlusion-hint`, en-US and a
   pt-BR translation. Catalog completeness test must stay green.

## Pins (add)

All named in the design brief: depth-vs-ray_intersect per-pixel parity
corpus; occluded texels write transparent (two-quad behind-wall scene);
self never occluded; epsilon both sides; near-plane clamp explicit cases;
compose with cull/tiling/frozen+live; adopt/arm/disarm flag discipline;
kittest panel pin. Reuse the existing `quad()`, `red_everywhere()`,
`frozen_pair()` test helpers in projection.rs.

## Rules

- No cargo at all (dragon batteries). No new deps. rustfmt-clean.
- The 64 MiB-at-4096² buffer is fine (named in the brief); do not add a
  resolution cap.
- Return: files changed, tests added by name, judgment calls (NEAR_EPS /
  OCCL_EPS values and why, tie rule), anything unverifiable without cargo.
