# Kill report — 2026-11-09 — wave 2.1: R-A2 v3 depth/occlusion culling

Authorship: dragon (Razul), in-session after the dispatched claws failed
(opus subagent sa-0-deb780fd died provider-stalled after 3h11m with zero
files written; the fable-5.2 CLI lane hit its 5-hour rate limit at 22:16,
resets 01:50 — row 324). Kill lock: `razul-projection-occlusion-w21b`
(adopted from the dead session's `razul-projection-occlusion-w21`, holder
exit confirmed, fresh lock over the same 7 files).

Implements `docs/loop/briefs/2026-11-09-projection-occlusion-design.md`
and `...-kill-brief.md` (lock name in the brief says w21; the surface is
identical).

## Built

- `crates/umber-mesh/src/depth.rs` (NEW): `rasterize_depth(mesh, view_proj,
  width, height) -> Vec<f32>` — screen-space nearest-`clip.w` raster, one
  triangle-driven pass, `f32::INFINITY` init. `NEAR_EPS = 1e-6` clamps
  per-vertex `w <= NEAR_EPS` for the projection AND as stored depth (no
  near-plane clipping; the shade side still rejects `w <= 0` itself).
  `SCREEN_COND_EPS = 1e-10` conditions the screen-space barycentric solve
  (own epsilon, not `UV_COND_EPS`). W1.2: `triangle_positions_finite`
  triangles only; min wins per pixel, ties to the LAST triangle in index
  order (`d <= depth[at]`), the paint raster's convention, so a covering
  triangle reads its own depth. Never panics; zero-sized buffer returns
  empty. Unit tests: flat-quad depth == clip.w, per-pixel parity vs
  `ray_intersect` on an in-front corpus, near-plane clamp on a behind-eye
  vertex, malformed triangle covers nothing, zero-size buffer.
- `crates/umber-mesh/src/lib.rs`: `pub mod depth;` + re-export
  `rasterize_depth, NEAR_EPS, SCREEN_COND_EPS`. Layering untouched
  (umber-mesh still takes a plain `glam::Mat4`).
- `crates/umber-app/src/projection.rs`: `ProjectionState.occlusion: bool`
  default `true` (adopt keeps it, arm/disarm never touch it);
  `pub const OCCL_EPS: f32 = 1e-4` (relative; the covering triangle's own
  depth equals `clip.w` up to ~1e-6 raster interpolation error, so 1e-4
  leaves two orders of magnitude over float error while resolving surfaces
  nearer by >0.01% — pinned both sides). `rasterize_with` builds the depth
  buffer ONCE per Apply via `rasterize_depth(mesh, vp, dw, dh)` through the
  same camera (frozen and live paths share it); `shade` tests
  `depth[py*dw+px] < clip.w * (1 - OCCL_EPS)` after the NDC guard (screen
  clamped into range with `min(dw-1)`) and returns `None` -> the v2d write
  rule writes transparent, winner semantics unchanged. With the flag off
  the buffer is empty and the raster is byte-identical to pre-v3.
  `per_texel_reference` (the test oracle) applies the same depth rule so
  the byte-identity pins stay honest. Module docs name the landed v3
  behavior. New pins: `occluded_texels_write_transparent_and_off_restores_pre_v3`,
  `self_is_never_occluded`, `occlusion_epsilon_pinned_both_sides`,
  `near_plane_and_behind_eye_cases_do_not_panic`,
  `occlusion_composes_with_cull_tiling_frozen_live_and_flag_discipline`
  (frozen == live-through-frozen-camera == oracle for cull x tiling, adopt/
  arm/disarm flag discipline).
- `crates/umber-app/src/display_panel.rs`: `ProjectionRow::occlusion` borrow
  + "Occlusion" checkbox after "Cull back faces" (same shape, writes
  through, enabled with an image), kittest
  `projection_occlusion_checkbox_writes_through_the_borrow`, catalog keys
  added to `overlay_keys_exist_in_both_catalogs`.
- `crates/umber-app/src/main.rs`: `occlusion: &mut state.occlusion` at the
  projection row (~1678).
- Both `.ftl` catalogs: `.projection-occlusion` / `.projection-occlusion-hint`
  (en-US + pt-BR) next to the cull keys.

## Judgment calls

- `OCCL_EPS = 1e-4` relative (brief said "e.g. 1e-4 — pin the choice"): the
  dead claw's numpy margins showed sub-1e-3 depth deltas at 100px viewport
  between neighboring surfaces, but those are GENUINE surface separation,
  not interpolation error; self-depth error is ~1e-6 relative, so 1e-4 is
  the right order. Both sides pinned (`occlusion_epsilon_pinned_both_sides`).
- Depth metric is `clip.w` pre-divide (design brief), not NDC z: divide-free
  and monotonic for perspective; the parity pin compares against
  `ray_intersect` hits re-projected to `clip.w`.
- Depth buffer is viewport-sized (`rw x rh`), NOT tile-sized: the occlusion
  test is per screen pixel, and 64 MiB at 4096² is the named cost.
- Behind-eye vertices clamp to `NEAR_EPS` in the depth buffer too (they can
  only occlude at near-plane depth; they never paint — shade rejects
  `w <= 0` first). Pinned explicitly.
- The three `ProjectionRow` test constructors gained `occlusion: &mut true`;
  all `ProjectionState` literals elsewhere use `..Default::default()`.

## Unverified

- GPU-path interaction: none — the depth pass is CPU-only in umber-mesh; the
  wgpu viewport pass is untouched.
- Perf numbers: measured post-land (perf probe below), see Measurement.

## Deferred

- Wave 2.2 product questions (seam/symmetry, per-texel normals beyond v2c)
  stay queued. Wave 3.1a (abr patt reader) is next, analysis landed in
  `docs/research/2026-11-09-abr-patt-analysis.md` (row 322).

## Measurement (Apply wall-time, perf probe)

Throwaway probe (scratch integration test, not committed; release build,
8192-triangle grid mesh, 1024x1024 target tile, head-on camera):

| viewport | occlusion off | occlusion on | delta |
|---|---|---|---|
| 1024² | 182.5 ms | 113.6 ms | within warmup noise (off ran first) |
| 4096² | 124.3 ms | 314.5 ms | +190 ms |

The v3 cost is the one extra O(viewport_px + triangles) depth pass plus the
`rw*rh*4` buffer (64 MiB at 4096²): ~190 ms at 4096² viewport on this host,
noise-level at 1024². Painted-texel counts drop under occlusion (815168 ->
454518 at 1024²) exactly where the bumpy grid's nearer cells cover farther
ones — the intended byte change. Against the pre-v2d per-texel-lookup class
(34 min at 4096²), the whole Apply path remains single-digit-hundreds ms.

## Process note

- Opus claw sa-0-deb780fd (3h11m) died to provider stalls with zero files;
  adopted its lock per the adoption rule (ps-confirmed exit, fresh lock
  w21b). The fable lane (authorized by Bessa 2026-11-09 ~21:0x) was rate
  limited (5h limit, resets 01:50) at dispatch time, so the dragon
  implemented in-session. Queue row 324 records the re-dispatch; row 325
  closes the kill.
