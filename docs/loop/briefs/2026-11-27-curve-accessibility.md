# Kill brief: curve keyboard access and graphic contrast

- Lock: `razul-curve-accessibility`.
- Trigger: UI/UX audit findings 1 and 2 at `ec979743` remain live: shared and legacy pressure curves are pointer-only, and graph output plus shared curve handles fail the Light-theme non-text contrast floor.

## Required behavior

- The shared `CurveEdit` exposes a named slider for every control point. Tab reaches the point; Left/Right adjusts x and Up/Down adjusts y through the existing clamped `move_point` path. Delete/Backspace performs the same protected removal as secondary click when variable count is allowed.
- The legacy four-point brush editor keeps its spline, insert/delete semantics, and animation. Each handle gains a named slider role, arrow-key adjustment through `clamp_control_point`, protected Delete/Backspace behavior, and visible focus.
- Add `theme::graphic_fg()` for meaningful non-text strokes. Graph output borders, pending noodles, output ports, and non-hover shared-curve handles use it instead of hard-coded gold/white.
- All four themes clear 3:1 for the chosen graphic foreground on both panel and raised surfaces.

## Tests

- RED: a real kittest tabs to `curve point 2` and presses ArrowUp; before the fix no such accessible control exists and the curve cannot change by keyboard.
- RED: the current gold/white choices fail the all-theme contrast assertion on Light.
- GREEN: shared and legacy keyboard paths change the real curve exactly once per key, preserve invariants, and leave protected endpoints alone. Kittest verifies accessible names and focus.
- Full `umber-gui` and `umber-app` suites, rustfmt, strict clippy, layer/scorecard/parity checks, and independent review before commit.

## Constraints

No redesign of curve interpolation or spline behavior, no new product feature, no release/tag/upstream PR, no centralhost. Pointer behavior remains intact.
