# Kill report: Ctrl+LMB angle chord (g1-brush-resize-drag vertical tail)

Date: 2026-11-01 ~01:4x (dragon local). Lock: `g1-angle-chord`.
Envelope: code only, no cargo/tests (host envelope); rustfmt sanctioned.
Battery/clippy/new test at the next sanctioned wave push.

## Scope

GUI plan G1 `g1-brush-resize-drag` row: "Ctrl+LMB drag: horizontal = flow,
vertical = angle (once angle exists)". The angle param now exists
(`BrushParams::stamp_angle`, editor rows `brush.stamp-angle` /
`brush.angle-degrees`), so this kill wires the vertical half of the chord,
mirroring the flow half (`9d70bbc`).

## What landed

- `viewport.rs`: `angle_step(angle0, dy)` — linear, 400 px per full
  -180..=180 sweep, clamped to the editor's range; `ANGLE_PX_PER_FULL_SWEEP`;
  `Viewport::live_stamp_angle` handed up per frame; `flow_drag` widened to
  `(Pos2, alpha0, angle0)` (angle0 read from the live `stamp_angle`, Fixed
  or FollowStroke offset); the drag-start comment updated; the chord HUD
  now reads `Flow N%   Angle N°` (`hint.hud-angle` both catalogs); pinned
  unit test `angle_chord_sweeps_linearly_and_clamps`.
- `main.rs`: `pending_stamp_angle` stashed through `PanelViewer` /
  `AppState` exactly like `pending_flow_alpha` (field, wiring, reset,
  4-tuple through the dock closure); adopted at the brush sync.
- `brush_panel.rs`: `adopt_live_angle(degrees)` next to `adopt_live_alpha`
  — writes the active preset's `stamp_angle = Fixed { degrees }`, clamped,
  dirty-flagged only on change.
- i18n: `hint.hud-angle` = Angle / Ângulo (parity 828/828, 635 tr sites
  resolve, inline fileless check).

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on all 3 touched .rs files (one
  brace slip mid-edit caught by rustfmt's parser and repaired in the same
  sitting).
- Greps: all `flow_drag` destructure sites use the 3-tuple; all
  `live_stamp_angle`/`pending_stamp_angle`/`adopt_live_angle`/`angle_step`
  references present (18 hits across the three files).
- NOT run: cargo build/clippy/tests (envelope). The new unit test runs at
  the next battery. Commit stays local until the wave gate.

## Semantics / deferred tails

- The chord sets a FIXED stamp angle (absolute degrees), overriding
  FollowStroke/None/Random for the drag's duration — same "the preset owns
  it" rule as flow; the next drag re-reads the current mode's degrees as
  its origin, so FollowStroke keeps its offset as the starting point.
- HUD shows both values on one line at the drag origin.
