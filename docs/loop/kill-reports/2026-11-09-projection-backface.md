# Kill report — 2026-11-09 — R-A2 v2c: the decal projector culls back-facing surfaces

- **Author:** dragon (Razul) + claw claude/opus (`1.799264` USD, 291s,
  session `proj-backface`, result
  `~/.hermes/cache/scratch/proj-backface-result.json`).
- **Kill-lock:** `razul-projection-backface-297` over
  `crates/umber-mesh/src/uv_gbuffer.rs`, `crates/umber-mesh/src/lib.rs`,
  `crates/umber-app/src/projection.rs`,
  `crates/umber-app/src/display_panel.rs`, `crates/umber-app/src/main.rs`,
  `crates/umber-app/src/viewport.rs` (added by the dragon), and both
  `.ftl` catalogs. Released at the commit.
- **Design:** `docs/loop/briefs/2026-11-09-projection-backface-design.md`.
  **Kill brief:** `docs/loop/briefs/2026-11-09-projection-backface-kill-brief.md`.

## 1. Built

**Mesh seam** (`crates/umber-mesh/src/uv_gbuffer.rs`):

- NEW `pub fn world_and_face_normal_at_uv(mesh, uv) -> Option<([f32; 3], Option<[f32; 3]>)>`
  — one barycentric scan returns the world point AND the covering
  triangle's unit geometric normal. The outer `None` is a UV miss (the
  texel writes nothing); the inner `None` is a degenerate normal (the
  texel is KEPT — malformed geometry never silently kills paint).
- NEW `pub const FACE_NORMAL_EPS: f32 = 1e-12` (the cross product's length
  floor).
- The normal is `normalize((p1 - p0) x (p2 - p0))` over the positions IN
  INDEX ORDER — the renderer's own expression
  (`renderer.rs:2106`), matching the pipeline's `cull_mode: Back` +
  CCW-front convention. NOT the per-vertex `MeshData::normals` (optional
  and smooth-shaded: it would disagree with the viewport's silhouette).
- `world_at_uv` is now a thin wrapper over the new function (ONE scan
  implementation); its results are byte-identical for its existing
  callers — pinned with `to_bits` comparisons.
- Both names re-exported from `crates/umber-mesh/src/lib.rs`.

**App** (`crates/umber-app/src/projection.rs`):

- NEW `ProjectionState.cull_backfaces: bool`, default `true`.
- The shared per-texel core `rasterize_with` takes the eye from the SAME
  camera the raster projects through (`camera.eye()` — the frozen
  placement's or the live one under follow) and skips a texel when
  `dot(n, eye - p) < 0`. The cull runs right after the world-point
  lookup, so a culled texel skips the projection work too.
- `faces_away(n, eye, p)`: `true` only for `dot < 0.0`. A missing
  (degenerate) normal or a NaN dot KEEPS the texel; `dot == 0` (grazing)
  keeps it too, so the silhouette does not flicker.
- `adopt` keeps the flag (a placement mode, like `tiling` and
  `follow_camera`); `arm`/`disarm` never touch it.
- The module docs' cut-line section now states the v2c reality and names
  the depth-culling deferral.

**Panel** (`display_panel.rs`, `main.rs`, both catalogs):

- NEW `ProjectionRow::cull_backfaces: &'a mut bool` — the same borrow
  pattern `tiling`/`follow_camera` use.
- A "Cull back faces" checkbox after Follow camera, enabled with an
  image, with a hover hint. No response field: nothing to re-freeze.
- NEW keys, SAME names in BOTH catalogs:
  `overlays.projection-cull-backfaces` ("Cull back faces" /
  "Ocultar faces traseiras") and `overlays.projection-cull-backfaces-hint`
  (en-US: "Surfaces facing away from the camera do not take the decal.
  Uncheck to paint both sides." / pt-BR: "Superfícies voltadas para longe
  da câmera não recebem o decalque. Desmarque para pintar os dois
  lados.").

**Dragon-side doc fix** (outside the claw's surface, inside the
dragon's lock): `crates/umber-app/src/viewport.rs:226` still claimed "no
depth or backface culling". It now states the v2c reality (backface
culling landed, depth culling deferred) — the doc-comment-sync rule, in
the same kill.

## 2. Judgment calls

- **The return shape is the nested `Option`, not the design brief's flat
  `Option<([f32;3],[f32;3])>`.** The flat shape cannot express "the UV
  hit exists but the triangle is degenerate" — and the design's own rule
  ("a degenerate normal KEEPS the texel") needs that distinction. The
  brief offered the nested shape as the recommendation; the claw took it.
  No behavior deviation, only the shape the brief already sanctioned.
- **The default flip is user-visible and is named here:** before v2c a
  decal reached back faces; after v2c it does not, unless the artist
  unchecks "Cull back faces". The checkbox IS the escape hatch — it
  restores v1's "both faces write" behavior exactly (pinned).
- **The cull runs before the projection, not after**: a culled texel
  costs one barycentric scan and nothing else. The alternative (project,
  then discard) would do the inverse_map + bilinear sample for texels
  whose result is thrown away.
- **The claw used plain `[f32; 3]` arithmetic for the cross product**
  rather than glam, because `uv_gbuffer.rs` is glam-free in practice
  (the crate lists glam, but this module's style is plain arrays). The
  brief flagged that risk; the claw checked and matched the module.
- **The cull is per-TEXEL, not per-pixel**: the raster is target-texel
  driven (the plan's §3 decision), so the cull is the same granularity.
  A pixel-accurate cull would need a different raster direction — out of
  scope and named.

## 3. Gates

- `rustfmt --edition 2021 --check` clean on all 8 touched .rs files
  (three needed a formatting fix after the claw's hand-formatting; the
  dragon applied rustfmt to exactly those three files).
- `scripts/check-layers.py` → GREEN (16 crates). No new dependency:
  `Cargo.toml` / `Cargo.lock` are untouched (`git diff --stat` on them:
  empty).
- `scripts/scorecard.py --check` → README current.
- `cargo clippy -p umber-mesh -p umber-app --all-targets -- -D warnings`
  → 0 warnings, 0 errors.
- `cargo test -p umber-mesh` → 133 + 2 + 32 (1 ignored) + 9 + 0 suites,
  0 failed.
- `cargo test -p umber-app` → all suites 0 failed (lib 297, bin 772).
- **Focused re-verification** (the dragon's own script, not the claw's
  report): the projection module 37/37, the mesh `uv_gbuffer` module
  16/16, the panel kittest 1/1, both new i18n keys present exactly once
  in each catalog. Log: `~/.hermes/cache/scratch/verify-297.log`.

The 13 new pins, all green:

| pin | asserts |
| --- | --- |
| `face_normal_world_point_is_byte_identical_to_world_at_uv` | the world point is bit-identical (`to_bits`) over five UVs |
| `face_normal_is_unit_and_along_plus_z_for_the_ccw_quad` | unit length; `+z` for CCW; a tilted triangle normalizes to `(0,-12,3)/√153` |
| `reversing_the_winding_flips_the_face_normal` | the normal follows the INDEX order, not the geometry |
| `a_degenerate_triangle_keeps_its_world_point_but_has_no_normal` | collinear AND infinite positions → no normal, world point intact |
| `a_uv_miss_yields_nothing_from_the_normal_lookup` | a miss and a NaN UV → `None` |
| `culling_on_front_facing_quad_is_byte_identical_to_the_v1_chain` | the v1 regression guard: front-facing output unchanged with culling ON |
| `culling_on_reversed_quad_takes_nothing` | the same quad, reversed winding → an all-zero raster |
| `culling_off_restores_v1_on_the_reversed_quad` | the escape hatch restores v1 byte for byte |
| `culling_keeps_texels_of_degenerate_triangles` | degenerate geometry keeps its paint (cull-on == cull-off) |
| `tiling_composes_with_culling` | v2a + v2c compose; the back face still takes nothing |
| `follow_composes_with_culling_through_the_live_eye` | the LIVE eye decides (yaw = π culls; the frozen eye would not) |
| `adopt_keeps_cull_backfaces_and_arm_disarm_never_touch_it` | all four (cull × follow) combinations across arm/disarm/adopt-ok/adopt-refused/re-arm |
| `projection_cull_checkbox_writes_through_the_borrow` | the kittest: click off, click on, never Tile/Follow/arm; disabled without an image |

## 4. Unverified

- **No live viewport render.** The cull is proven by the CPU raster pins
  and by the source-level agreement with the renderer's cull convention
  (`renderer.rs:846` `cull_mode: Back` + `renderer.rs:2106`'s identical
  cross-product expression), but no screenshot shows a decal on a closed
  mesh with the back face clean. The desktop check is Bessa's.
- **The kittest's 640 px harness width**: the claw could not confirm the
  FOURTH checkbox (Arm / Tile / Follow camera / Cull back faces) is fully
  visible at that width; the panel draws them in one `ui.horizontal`, so
  a narrow panel may clip the last one. Named, not hidden.
- **The yaw = π live-camera pin** assumes `view_proj` behaves normally
  at that yaw; the pin passed, but it is a single fixture.
- The other crates' suites are untouched by this kill and were not
  re-run beyond umber-mesh/umber-app.

## 5. Deferred (named)

- **Depth / occlusion culling** (the plan §3's other artifact: a texel
  behind another FRONT-facing surface still writes). It needs the
  visible surface's depth per screen pixel — either the GPU frame's depth
  (a new seam into `umber-gpu`) or a CPU depth raster. The route is
  open; the design brief §5 carries the analysis.
- **Seam / symmetry expansion of the projection.** Named in the v1 cut
  line, but it is a PRODUCT question, not an engineering one: a decal is
  a screen-space placement, and mirroring it across the UV axis has no
  reference-tool meaning (Painter mirrors STROKES, not its projector).
  It needs Bessa's call on what a mirrored decal should even do.
- The decal channel, Mari's bake family, the 3D-projector family (plan
  §7) — untouched.
- The flag is not persisted (same as v2a/v2b).

## 6. Process note

- The claw worked against the tree at its NEW location
  (`/mnt/hdd/data/Projects/umber`); the brief named it explicitly, and
  Claude Code reported the new path's trust state as unaccepted (its
  permissions.allow entries were ignored — harmless: the brief forbade
  cargo anyway, and the writes landed through acceptEdits).
- The kill-lock was RE-ADDED with `crates/umber-app/src/viewport.rs`
  included (the claw named the stale doc line at viewport.rs:226 as
  outside its surface; the dragon extended its own lock and fixed the
  doc in the same kill, per the doc-comment-sync rule).
- The battery ran on the NEW disk: the cargo `target/` was rebuilt from
  scratch (76G deleted with the move), so the first battery paid the
  cold-cache cost — 26 minutes of the clippy/test legs, on the rotary
  disk. The build tax is real and is recorded in the migration report.
