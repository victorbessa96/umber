# Kill report — front-face-falloff (viewport strokes fade on grazing faces)

Date: 2026-10-14. Author: dragon. Lock: `front-face-falloff` over
`crates/umber-mesh/src/raycast.rs`, `crates/umber-mesh/src/lib.rs`
(added mid-kill: the re-export line), `crates/umber-app/src/viewport.rs`,
this report. Source: ROADMAP wave 8b row `wave-front-face-falloff`;
Blender brush Normal Falloff ("weakens the brush strength for faces
whose normal points towards the screen edges instead of the camera").

## Built

- `umber_mesh::face_normal(mesh, hit)`: the hit triangle's unit
  geometric normal, `None` for a degenerate or out-of-range triangle
  (never a NaN).
- `umber_mesh::front_facing_weight(facing, start_deg)`: full strength
  within `start_deg` of head-on, then linear in ANGLE to 0 edge-on.
  `|cos|` (an open mesh's back face counts the same), `90` = off, and
  non-finite input gives 1.0.
- `Viewport::pick` returns `(uv, weight)`; `pick_uv` is now a thin wrapper
  (every existing caller unchanged). The paint drag multiplies the
  resolved pressure by the weight.
- `Viewport::set_front_face_falloff_deg(Option<f32>)`: `None` = off.

## Judgment calls

1. **Rides the pressure channel.** Scaling pressure gives the falloff
   every downstream rule a lighter pen already gets (pressure curve,
   wash, symmetry) with zero new paint-engine surface. Alternative
   rejected: a new per-dab weight through `PaintState`, a second
   pressure-shaped channel for the same effect.
2. **`Option<f32>`, not `f32`.** `Viewport` derives `Default`; a bare
   `f32` field defaults to `0.0`, which is fully ON and would silently
   weaken every stroke. `None` makes the derived default OFF; a test pins
   weight 1.0 on a fresh viewport.
3. **Linear in angle**, not in cosine: the start angle the user sets is
   an angle, and the band from it to 90° reads evenly.
4. **Geometric face normal**, not interpolated vertex normals: always
   present (even for meshes without vertex normals) and it's what
   decides whether the dab is stretched. Faceted on low-poly meshes;
   noted.

## Unverified

- No UI yet, so no user can turn it on. Shell wiring is deferred (below).
- Real-mesh feel (the start angle that suits most work) is unmeasured;
  Blender's default is near 80°.

## Deferred

- The brush-panel control (a slider, off by default) and the per-frame
  `viewport.set_front_face_falloff_deg(...)` sync in `main.rs`. Both are
  outside this lock: `main.rs` is held by the in-flight gpu-safe-start
  kill. Follow-up once that lock is released; it needs i18n keys in both
  catalogs.
- Persisting the angle with brush presets (`.umberbrush`).
