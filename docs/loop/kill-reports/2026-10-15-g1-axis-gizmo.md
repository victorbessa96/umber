# Kill report — g1-axis-gizmo (GUI plan G1)

Date: 2026-10-15. Author: dragon. Lock: `g1-axis-gizmo` over
`crates/umber-app/src/axis_gizmo.rs` (new), `main.rs` (one `mod` line),
this report. Its `viewport.rs` hook rides the `viewport-stroke-end` lock
(also the dragon's), committed together. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g1-axis-gizmo`. Sources: Painter
A9 / C4 ("Axis gizmo: XYZ at the bottom-right of the 3D view"), Blender
C2.1 ("click a label to align to that axis, click it again for the
opposite side").

## Built

- `axis_gizmo.rs` (new), pure except `paint`:
  - `camera_basis(yaw, pitch)` gives right / up / back from
    `OrbitCamera::eye`'s convention.
  - `project_axes` projects the six world-axis tips to screen offsets with
    depths, sorted back to front so the nearest draws last.
  - `hit` maps a click to the tip under the pointer (9 px) and prefers
    the nearer tip when two overlap.
  - `snap_angles(axis, yaw)` puts the EYE on the axis's side. ±Y sit at
    the camera's pitch limit and keep the current yaw, so a top view does
    not spin.
  - `aligned_axis` and `click_target` implement the click-again-for-the-
    opposite rule.
  - `paint`: a dark disc in the bottom-right corner. Positive axes get a
    line, a solid tip and an X / Y / Z letter; negatives get a hollow tip.
    Conventional red / green / blue, and the hovered tip enlarges.
  - 5 tests: head-on projection, hit and overlap preference, snapping for
    all six axes, the flip rule, and basis orthonormality over four
    orientations.
- `viewport.rs`: the gizmo resolves its click FIRST in `ui()`.
  - A tip click snaps `camera.yaw` / `pitch` and sets `gizmo_clicked`.
  - The eyedropper, ID-mask and straight-line clicks all skip a frame the
    gizmo took, so one click never also samples a colour or paints.
  - Drawn after the line preview, clipped, only with a mesh loaded.

## Judgment calls

1. **The camera's public fields, no camera API.** `OrbitCamera.yaw` and
   `pitch` are public, and the orbit clamp is applied by `orbit()` only.
   `snap_angles` itself stays inside the limit (`POLE_PITCH` is just under
   `PITCH_LIMIT`), so a snapped view is always one the orbit could reach.
2. **No snap animation.** Blender eases between views (Smooth View).
   Easing must honour Reduce Motion (plan rule 7), and that preference
   arrives with the G3 preferences window. A snap is the honest v1.
3. **Not in the UV view.** Painter's 2D view has its own UV axis
   indicator. That is G6 uv-view-parity, not this kill.

## Unverified

- The live look and the click on hardware: manual (needs the GPU
  viewport).
