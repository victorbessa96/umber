# Kill report — viewport strokes end + straight line (GUI plan G1)

Date: 2026-10-15. Author: dragon. Lock: `viewport-stroke-end` over
`crates/umber-app/src/viewport.rs`, `brush_cursor.rs`,
`tests/gate0_journey.rs`, `status_bar.rs`, `assets/i18n/*.ftl` and this
report. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g1-straight-line-preview`. Source: Painter C1 ("holding Shift draws a
dotted preview line, and a click paints it; Ctrl+Shift snaps the angle to
5° steps").

## Found first: viewport strokes never ended (a real bug)

Scouting for the straight line's anchor (the last stroke's end), the
viewport's paint arm turned out to call `begin_` / `extend_screen_point`
but never `end_stroke`. `PaintState::is_stroking()` stayed true after the
button came up, so every later Ctrl+drag EXTENDED the first stroke. The
consequences:
- one stroke per session;
- Ctrl+Z undid everything painted in 3D at once;
- the History panel never got a stroke row (rows are queued in
  `end_stroke`).

The UV view always ended its strokes (`uv_view.rs`, `else if
paint.is_stroking() { paint.end_stroke() }`). The viewport arm was added
in 708ae8a (2026-10-09) without that branch, and nothing caught it: Gate
0 drives `PaintState` directly and calls `end_stroke` itself.

## Built

- **The stroke ends on release.**
  - `ends_viewport_stroke(painting, stroking, from_viewport)` is a pure
    rule with a test: a stroke ends when the paint drag stops.
  - `stroke_from_viewport` marks the strokes THIS view began, so it never
    ends a UV-view stroke (both views share one session).
- **The straight line.**
  - Shift held between strokes draws a dashed preview (two-pass
    dark/light, theme independent) from the last viewport stroke's end to
    the pointer. Shift+click paints it as ONE stroke.
  - Ctrl+Shift snaps the direction to 5° rays; `straight_line_end`
    projects onto the snapped ray, keeping the distance along it.
  - The segment is sampled every 2 px through the real pick
    (`line_samples`). Samples that miss the mesh are skipped and the
    stroke keeps going, the drag-across-a-silhouette rule.
  - Each sample carries its front-face weight as pressure, like a drag.
  - The anchor is screen-space, Painter's own model and caveat: move the
    camera and it stays where the stroke ended on SCREEN.
- **Status hints:** holding Shift shows "LMB: straight line from the last
  stroke · +Ctrl: snap to 5°". Keys added in en-US and pt-BR.
- **Tests:** 3 pure tests (stroke end rule, 5° snap incl. 3°→0° and
  44°→45° plus the degenerate case, sample coverage).

## Judgment calls

1. **Shift+CLICK, not Shift+drag.** Under the classic nav preset
   Shift+LMB drag pans. A click never moves the camera in any preset, so
   the line cannot collide with navigation.
2. **The anchor is only viewport strokes.** UV-view strokes live in UV
   space. Joining a screen-space line to a UV-space end needs a
   projection back through the camera, which is the follow-up.

## Unverified

- Live behaviour on hardware (the preview draws and the line paints):
  manual. The viewport needs a GPU context the headless suite does not
  build.
- No Gate-0-style test drives the viewport's input arm. That is the gap
  that hid the stroke-end bug for six days. Adding a headless viewport
  input harness is the G0 kittest-harness kill's first item.
