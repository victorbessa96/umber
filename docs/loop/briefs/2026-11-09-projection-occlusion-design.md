# Design brief — 2026-11-09 — wave 2.1: R-A2 v3 depth/occlusion culling

Author: dragon (Razul). Status: LIVE. Plan of record:
`docs/loop/waves/2026-11-09-wave-plan.md` wave 2, item 2.1.

## The artifact being closed

The last named v1 R-A2 artifact: "a texel behind another surface still
writes". Today `rasterize_projection` shades every covered texel whose
covering triangle passes the backface test, regardless of whether another
surface lies between the camera and that texel's world point.

## The route decision (named here, per the wave plan — do not derive in code)

**CPU screen-space depth buffer, built once per rasterize call from the
SAME camera the raster projects through (frozen placement's or the live
one).** Not per-texel raycasts (O(texels) BVH raycasts would put the Apply
back on the multi-minute path the v2d one-pass kill just removed), and not
a GPU depth-texture readback (wgpu plumbing, feature-gated, and the
gateway is CPU-only on this host).

- New `crates/umber-mesh/src/depth.rs`: `pub fn rasterize_depth(mesh,
  view_proj: Mat4, width: u32, height: u32) -> Vec<f32>` — screen-space
  raster of the mesh into a `width*height` depth buffer, `f32::INFINITY`
  where empty, nearest surface wins per pixel. Depth metric: `clip.w`
  (the view-space forward distance, pre-divide — monotonic with distance
  for a perspective camera and divide-free). Per-vertex `w <= 0` (at or
  behind the eye) is CLAMPED to a small `NEAR_EPS` for projection only —
  no full near-plane clipping; the parity pin against `ray_intersect`
  covers only meshes fully in front of the camera, and the near-plane
  behavior is pinned by explicit cases (see pins), not fuzzed across the
  plane. umber-mesh takes a plain `glam::Mat4`; the app passes
  `camera.view_proj(rw/rh)` (camera lives in umber-gpu — layering forbids
  umber-mesh depending on it; check-layers stays GREEN).
- `projection.rs`: `rasterize_with` builds the depth buffer once
  (viewport `rw x rh`) and threads it into `shade`. Occlusion test, after
  the screen-px computation: `occluded = state.occlusion &&
  depth[py*rw+px] < clip.w - OCCL_EPS*clip.w` (relative epsilon, pinned
  both sides). The covering triangle itself yields `depth == clip.w`
  (same surface, same point), so self is never occluded; a strictly nearer
  surface occludes. Occluded -> `shade` returns `None` -> the v2d rule
  writes transparent, so the last-in-index-order winner semantics are
  unchanged.
- New `ProjectionState.occlusion: bool`, DEFAULT TRUE (the artist-facing
  fix for the named artifact), with the panel checkbox as the escape
  hatch — the exact v2c `cull_backfaces` pattern: checkbox in the
  projection section after "Cull back faces", writes through
  `ProjectionRow::occlusion`, `main.rs` passes `&mut state.occlusion`,
  adopt keeps the flag, arm/disarm never touch it, 2 new keys
  (`.projection-occlusion`, `.projection-occlusion-hint`) in BOTH
  `assets/i18n/en-US.ftl` and `pt-BR.ftl`. Composes orthogonally with
  tiling (v2a), follow-camera (v2b) and backface culling (v2c); the live
  (follow) path builds its depth buffer from the live camera.

## Cost class (a claim that owes a measurement, per the perf-kill rule)

One extra screen-space pass per Apply: O(viewport_px + triangles), same
class as the existing decal raster, zero extra memory beyond one
`rw*rh` f32 buffer (64 MiB at 4096² viewport — state it; the alternative
per-texel raycast is the 34-minute class and is rejected). The dragon
measures Apply wall-time at 1024² and 4096² viewport on an 8192-triangle
mesh post-land via the scratch probe harness and puts the numbers in the
kill report.

## Byte-change statement

Projector output changes ONLY on texels whose screen pixel is covered by
a strictly nearer surface than the covering triangle (occluded -> was
painted, now transparent). Meshes viewed with no overlapping-nearer
geometry: byte-identical. Fill, GPU bakes, export: untouched.

## Pins

- `depth_raster_matches_ray_intersect_per_pixel_on_a_corpus`: for meshes
  fully in front of the camera, per-pixel nearest depth equals
  `ray_intersect(eye, dir_through_pixel)` distance within tolerance; ties
  (two surfaces at equal depth) resolve identically or within eps.
- `occluded_texels_write_transparent`: two-quad scene, decal target behind
  a wall from the camera — the wall's screen footprint texels stay CLEAR.
- `self_is_never_occluded`: single quad, camera in front — fully painted.
- `occlusion_epsilon_pinned_both_sides`: contact/grazing cases on either
  side of the relative epsilon.
- `near_plane_clamp_case_pinned`: camera close to a surface; decal behind
  vs in front behave per the clamp rule (explicit expected values).
- Compose pins: occlusion x {cull_backfaces on/off} x {tiling on/off} x
  {frozen, live-follow}; `adopt` keeps the flag; arm/disarm never touch it.
- Panel pin (kittest) like v2c's: checkbox writes through, label resolves
  from both catalogs.
- W1.2 interaction: the depth raster must respect the same validity rules —
  malformed-position triangles cover nothing in the depth buffer too
  (reuse `triangle_positions_finite`), and post-W1.1 needles are not valid
  footprints in screen space either (same UV_COND_EPS rule does NOT apply
  — screen-space area conditioning uses its own epsilon, named in code).

## Gate

clippy `-D warnings` on umber-mesh + umber-app (`--all-targets`), their
suites, rustfmt, check-layers GREEN, scorecard current (regenerate if the
count moved), safe-push's own crate gates, release CI after push.

## Non-goals

- No GPU depth readback, no per-texel raycast route, no decals that
  respect per-texel normals beyond v2c's facing rule.
- Seam/symmetry expansion and the decal channel stay queued (wave 2.2,
  product questions).
