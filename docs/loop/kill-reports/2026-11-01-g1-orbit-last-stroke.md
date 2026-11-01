# Kill report: orbit around the last stroke (g1-nav-presets final tail)

Date: 2026-11-01 ~02:2x (dragon local). Lock: `g1-orbit-last-stroke`.
Envelope: code only, no cargo/tests (host envelope); rustfmt sanctioned.

## Scope

GUI plan G1 `g1-nav-presets` row's last open item: "orbit around the last
stroke". Zoom-to-cursor (`1730125`), wheel invert (`6dd5bef`) and RMB-cancel
(`890c2f7`) already landed; this closes the row.

## What landed (`173fc3b`)

`crates/umber-app/src/viewport.rs`:

- `Viewport::world_at(rect, mesh, pos) -> Option<glam::Vec3>` — unprojects
  the screen point through the camera (same near/far + perspective/ortho
  origin rules as `pick`), ray-intersects the mesh, and returns the
  barycentric world hit point.
- The `CameraOp::Orbit` arm of the nav-drag match: on drag start, if a mesh
  is loaded and a `last_stroke_end` exists, the camera's `target` is set to
  the world point under that stroke end before orbiting — the view pivots
  around what was just painted instead of the mesh centre. Once per drag
  (`drag_started_by`); panning/zooming still move the target as before.
- Pinned unit test `world_at_center_hits_quad_center` (head-on viewport +
  test quad: the center ray's world hit is (0, 0, 0)).

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on viewport.rs.
- APIs checked against the tree: `RayHit { triangle, t, bary }`
  (umber-mesh raycast.rs), `mesh.indices/positions` shapes, `OrbitCamera
  .target` pub field (umber-gpu camera.rs:57), `Viewport::mesh_data:
  Option<MeshData>` (viewport.rs:325), `world_from_ndc` + `cursor_ndc`.
- NOT run: cargo/tests (envelope) — the new test runs at the next battery.

## Row status

`g1-nav-presets` parenthetical is now fully closed (zoom-to-cursor +
wheel invert + RMB cancel + orbit-around-last-stroke).
