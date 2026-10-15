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

---

# Supplement — the plan-row gaps this wave closed (same day)

Author: dragon (the second wake session; see the process note). This
section lands the two plan-row clauses the kill above left open, against
its landed implementation — no code of that kill was changed beyond the
gates below.

## Built (supplement)

- **"Hideable"** (plan row's clause): `Viewport::show_gizmo` (default
  `false` from the derived `Default`; the SHELL holds the user-facing
  default) + `set_show_gizmo` / `show_gizmo` accessors; `UmberApp`
  shell state `show_gizmo: true` synced into the viewport each frame
  like wireframe/grid; View menu row `menu.gizmo` (both catalogs:
  "Show Axis Gizmo" / "Mostrar gizmo de eixos"). The gate covers all
  three gizmo touchpoints: click (no snap while hidden), hover (no
  highlight), draw (nothing painted) — a hidden gizmo consumes no
  clicks and leaves the viewport input surface untouched.
- **The snapshot** (plan row's Test column): `axis_gizmo` is now on the
  umber-app library surface (`pub mod axis_gizmo` in lib.rs — the
  `display_panel` precedent) so the kittest harness can render it;
  `axis_gizmo_snapshot` (tests/panel_snapshot.rs) draws the landed
  gizmo under the default camera through the exact call `viewport.rs`
  makes and pins the PNG. `axis_gizmo_click_chain_snap_the_view`
  drives the landed click chain headless (hit on the +X tip →
  `click_target` → `snap_angles` → eye-on-+X; aligned click flips to
  the opposite side).

## Judgment calls (supplement)

1. **The derived-Default flip.** `Viewport::default()` now hides the
   gizmo (`show_gizmo: bool` → `false`); the user-facing default ON
   lives in the shell's constructor (`show_gizmo: true`), where every
   other user-facing default lives. Existing tests that construct
   `Viewport::default()` keep clean-slate expectations.
2. **No parallel widget in umber-gui.** The first session's draft
   widget (engine-pure `AxisPose` + per-handle interact rects) was
   fully dismantled once the landed kill was discovered mid-flight —
   see the process note. The landed `umber-app::axis_gizmo` is the one
   implementation; the plan row's `crates/umber-gui/src/widgets/
   axis_gizmo.rs` surface line was a dispatch-time path guess, and the
   skill's resolve-paths-at-dispatch rule fired too late (the parallel
   session's brief already said `umber-app`).

## Process note (the double-strike)

Two dragon-loop wake sessions picked the same queue item (`6b69de3d`,
g1-axis-gizmo) within the same window. The other session landed first
(commit `379ddb9`, CI-green via the W-G5 wave run 38760269560). This
session discovered the landed kill only when its write to the kill
report path was refused for staleness. Response: the duplicate core
(umber-gui widget, its re-exports, viewport wiring, UV arrows, camera
pub, navigation snap) was reverted wholesale; only the two genuinely
unshipped plan clauses (hideable + snapshot) were kept, re-targeted
against the landed implementation. LESSON (for the dispatch layer, not
just this session): a queue item claimed by a live session should
transition state (active → running) so a second wake cannot pick it
up; the queue's `next_step` field is the cheaper guard and was not set
by either session before striking.

## Unverified (supplement)

- ~~The `axis_gizmo.png` snapshot itself~~ — RESOLVED during the kill:
  generated on the build host via `UPDATE_SNAPSHOTS=1` (the initial-run
  red the report predicted), pulled back, and pixel-verified against the
  derived geometry (six tips at the `camera_basis(0.6, 0.4)` positions,
  red/green/blue on ±X/±Y/±Z, positive solid + lettered, negative
  hollow, front tips over back) before committing.
- The live-shell rendering of the hidden-gizmo input neutrality beyond
  the three gated paths: nothing else in `viewport.rs` reads gizmo
  state, but the full interaction matrix (hidden gizmo + eyedropper +
  ID-mask + straight-line in one frame) is not separately pinned; the
  unit tests cover each path alone. The kittest snapshot harness
  renders the gizmo through `paint` only — a kittest frame of the whole
  `Viewport::ui` would need the GPU context, which is the standing
  exclusion for the viewport in this harness.
