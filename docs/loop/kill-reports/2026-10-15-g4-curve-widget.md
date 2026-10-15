# Kill report: g4-curve-widget (the general curve editor)

Date: 2026-10-15. Author: dragon. Lock: `g4-curve-widget` over
`crates/umber-gui/src/widgets/curve.rs` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g4-curve-widget` (B:D18, B:C6.1).

## Status: written + ad-hoc verified, NOT registered

`widgets/mod.rs` and `lib.rs` sit under the live `g1-axis-gizmo` lock (a
sibling session, re-registered 10:46). The kill does not write past it.
The module was verified in a remote copy of HEAD with the one-line
`pub mod curve;` added there only: 10/10 `curve::` tests pass, and
`cargo clippy -p umber-gui --all-targets -D warnings` is clean. That is
not a suite verification. The file is committed unregistered (so it
does not compile into the crate yet). The registration is two lines once
the lock frees:

```rust
pub mod curve;                                   // widgets/mod.rs
pub use curve::{Curve, CurveEdit, CurveError};   // widgets/mod.rs
```

## Built

- `Curve`, the model: `n >= 2` points in `[0, 1]²`, x strictly
  increasing by at least `MIN_GAP` (0.01). `new` validates; `evaluate`
  is clamped piecewise-linear (the same rule as umber-brush's
  `ControlCurve::evaluate`); `move_point` clamps into the point's x
  window; `insert` refuses near-duplicates and points past the ends;
  `remove` protects the endpoints and the last two points;
  `to_text`/`parse` give the clipboard form `x,y;x,y`; `assign` honours
  a fixed point count. No edit can produce an invalid curve.
- Presets `preset_linear/smooth/ease(count)`, sampled at any count, so a
  four-point brush curve keeps four points.
- `CurveEdit`: grid, polyline, handles. Drag to move, click empty space
  to add, right-click a handle to delete, hover Ctrl/Cmd+C/V for the
  clipboard. `fixed_count(true)` turns off insert and delete for frozen
  models (the brush preset's `[(f32, f32); 4]`).

## Tests (10)

Every validation error class; exact interpolation values at and between
points; move clamps at both neighbours and both ends, with no-change
reported as `false`; insert/remove round-trip back to the original
curve; text round-trip plus four rejection shapes; assign with a fixed
count; preset samples pinned to 7/27 (smoothstep at 1/3) and 19/27
(ease-out at 1/3); screen mapping round-trip and nearest-pick; two
headless egui tests: click adds a point at (0.25, 0.75) (but not in
fixed mode) and right-click removes it, and a paste replaces the curve
while a junk paste changes nothing.

## Judgment calls

- **The brush panel is not migrated in this kill.** `brush_panel.rs`
  keeps its own four-point editor (the cosmetic Hermite display spline,
  the collinearize-as-delete trick). Moving it onto `CurveEdit` with
  `fixed_count(true)` is the follow-up kill: it touches `brush_panel.rs`
  and its snapshot, a separate surface.
- **Linear display, not a spline.** The widget draws exactly what
  `evaluate` returns. The brush panel's spline can disagree with the
  engine between points, and its own docs call it cosmetic.
- **No umber-brush dependency.** The layer gate allows umber-gui only
  egui. Consumers convert through `points()`.

## Deferred

- The registration (above), then the brush-panel migration, falloff
  Custom, and Levels as consumers.
