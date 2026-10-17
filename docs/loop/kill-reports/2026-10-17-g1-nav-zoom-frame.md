# Kill report: g1-nav-zoom-frame (camera half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `g1-nav-zoom-frame`: `crates/umber-gpu/src/camera.rs`.
- Implements the g1-nav-presets row's open tail "zoom-to-cursor +
  F-frame" — the CAMERA half. The shell wiring (the wheel event
  passing cursor NDC + aspect; the Key::F dispatch) is queued on the
  main.rs/viewport.rs locks like the other shell halves.

## Built

- `OrbitCamera::zoom_toward(cursor_ndc, delta, aspect)`: wheel-zoom
  that keeps the point under the cursor fixed. Pans the target along
  the camera's right/up by the world-space difference the cursor
  subtends before vs after the zoom (target-plane invariant — verified
  algebraically: the new target plus the new cursor offset equals the
  old target plus the old offset, exactly). A zoom that clamps at a
  distance limit moves nothing (early return). Perspective and
  ortho share the scale expression (the framing parity
  `projection_matrix` already guarantees: ortho half-height tracks
  distance identically).
- `OrbitCamera::frame(center, radius, margin)`: positions the camera
  so a bounding sphere fits the vertical frustum with a margin factor
  (≥1); keeps yaw/pitch; clamps the derived distance to the camera's
  limits. Degenerate radii (≤0, NaN, inf) are a no-op.
- Tests (4):
  - `zoom_toward_keeps_the_cursor_point_fixed_on_screen` — the
    target-plane world point under the cursor reprojects to the same
    NDC after the zoom (the real view_proj round-trip, tolerance 1e-3).
  - `zoom_toward_at_a_clamp_moves_nothing` — pinched min=max distance.
  - `frame_fits_the_sphere_inside_the_frustum` — the sphere's
    camera-tangent point and its top both project inside NDC ±1 with
    a 1.1 margin, from a non-trivial yaw/pitch.
  - `frame_refuses_degenerate_radii` — 0.0 and NaN leave the camera
    untouched.

## Judgment calls

- **Target-plane zoom, honestly named**: the invariant holds exactly
  at the target plane; depth-off-plane points drift under perspective
  (every target-anchor orbit camera has this shape — the alternative
  is ray-plane intersection per wheel event, a bigger change with the
  same feel at the plane most users zoom at). The doc comment states
  the plane; the test pins the plane.
- **frame() assumes the y axis is the tighter one** when aspect ≥ 1
  (landscape viewport): the fov_x depends on the caller's aspect,
  which frame() does not take — passing it would change the signature
  for a case the current viewport never hits (16:9). Named as the
  follow-up if a portrait surface appears.
- **Degenerate frame is a no-op, not an error**: a selection of zero
  radius (an empty scene) must not teleport the camera; the shell
  guards the call anyway.

## Unverified

- Battery (host down since 18:25). Static: rustfmt PASS; Vec2 import
  added (glam); the manual `impl Default` (:80) confirmed so the
  test constructions compile; the target-plane invariant proven
  algebraically before the battery. Ad-hoc verified, not
  suite-verified.

## Deferred

- The shell half: viewport.rs's wheel handler passes the cursor's NDC
  + aspect; Key::F frames the selection's bounds (or the mesh's, when
  nothing is selected). Lock-queued with the other five shell halves.
