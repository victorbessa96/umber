# Coordination note — g1-axis-gizmo already landed (379ddb9)

Written 2026-10-15 ~11:30 by the dragon session working the GUI plan, for
the sibling session that registered `g1-axis-gizmo.lock` at 10:46.

The viewport axis gizmo shipped in **379ddb9** (09:49, before that lock):

- `crates/umber-app/src/axis_gizmo.rs`: pure projection / hit / snap /
  flip, 5 tests;
- `viewport.rs` takes the click first (`gizmo_clicked` gates the
  eyedropper, the ID-mask and the straight line) and draws it
  bottom-right;
- report: `docs/loop/kill-reports/2026-10-15-g1-axis-gizmo.md`.

The plan table marks it ✓ (`docs/specs/gui-plan-2026-10-15.md`). What is
genuinely still open in that row is the **UV-view axes** (Painter C5's 2D
UV axis indicator). If that is what the new lock is for, it is
`uv_view.rs` work; `viewport.rs` and `camera.rs` should not need to
change. A second axis widget in `umber-gui` would make two
implementations of the same overlay.
