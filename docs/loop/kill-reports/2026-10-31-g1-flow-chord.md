# Kill report: g1-flow-chord (Ctrl+LMB flow/alpha drag)

- Date: 2026-10-31 ~22:4x -03
- Author: dragon (session local-2d130065fac61c2d5f72a1ce25fc4e95b82662dae00a029d3b6731057ca3fee1), ADOPTING the parked bytes of the dead ~21:35 session (holder exit confirmed by ps census at 22:2x: no hermes/cargo/claw process on the umber tree, file mtimes static since 21:35:40, /tmp/umber-kill-locks absent)
- Kill lock: `g1-flow-chord` (viewport.rs, main.rs, brush_panel.rs, both .ftl, this report, queue.md, STATE.md) — taken before the first edit, released at the commits
- Implements: docs/specs/gui-plan-2026-10-15.md G1 row `g1-brush-resize-drag` — "Ctrl+LMB drag: horizontal = flow" + "HUD readout at the drag origin" (the flow half; the vertical=angle half awaits the angle param)

## Built

- `crates/umber-app/src/viewport.rs`
  - `FLOW_PX_PER_DOUBLING = 200.0` + `flow_alpha_step(alpha0, dx) = (alpha0 * 2^(dx/200)).clamp(0.01, 1.0)` — exponential horizontal scaling, clamped to the preset editor's alpha range.
  - `Viewport::live_flow_alpha: Option<f32>` (handed up to the shell each frame) + `Viewport::flow_drag: Option<(egui::Pos2, f32)>` (pointer origin + alpha at press).
  - The chord in the input block: Ctrl/Cmd (without Alt) + LMB dragged, stencil not owning the pointer, paint present — same never-nav rule as the Ctrl+RMB resize chord. `drag_started` captures the press origin; per-frame the live alpha is computed from the horizontal delta.
  - HUD branch in `paint_brush_cursor`: while the flow drag runs, the ring stays at the drag origin with a numeric `Flow N%` readout beside it (same contract as the resize HUD), `hint.hud-flow` in both catalogs.
  - Unit test `flow_chord_scales_alpha_exponentially_and_clamps` (identity at dx=0, x2 at +200 px, x0.5 at -200 px, clamps at 0.01 / 1.0).
- `crates/umber-app/src/brush_panel.rs` — `adopt_live_alpha(alpha)` on the `adopt_live_size` precedent: clamps 0.01..=1, writes the active preset's `params.alpha`, marks dirty, no-op at the same value.
- `crates/umber-app/src/main.rs` — `AppState::pending_flow_alpha: Option<f32>` (cleared in `reset_for_new`); the dock closure now returns `(home_actions, pick_notices, flow_alpha)` with `flow_alpha = viewer.viewport.live_flow_alpha.take()`; `UmberApp::update` stashes it into `state.pending_flow_alpha`; the per-frame brush sync adopts it into the active preset via the new `PanelViewer::pending_flow_alpha` field when not stroking (so `sync_brush_params` does not snap it back).
- `assets/i18n/en-US.ftl` / `pt-BR.ftl` — `hint.hud-flow = Flow` / `Fluxo` in the hint group, both catalogs.

## Judgment calls

- Flow maps to the preset's `alpha` (the preset owns alpha; a paint-side-only edit would be snapped back by the per-frame sync). Rejected: setting alpha on `PaintState` directly.
- HUD shows flow as a percent (Painter shows flow as %); format matches the resize HUD's shape.
- Two compile-blocking scope errors in the parked bytes fixed on adoption (the rule's "pins wrong in both directions"): `PanelViewer` has no `state` field, so the sync site's `self.state.pending_flow_alpha` became the new borrowed field `pending_flow_alpha: &'a mut Option<f32>` (wired `&mut self.state.pending_flow_alpha` at the constructor — disjoint-field borrow, same pattern as `brush`/`paint`); and `AppState::reset_for_new` uses direct fields, so `self.state.pending_flow_alpha = None` became `self.pending_flow_alpha = None`. The `UmberApp::update` stash at ~4530 was already correct.
- `flow_drag` widened from `(f32, f32)` to `(egui::Pos2, f32)` so the HUD can anchor the ring at the drag origin (the G1 row's "HUD readout at the drag origin" tail was otherwise unimplementable).

## Unverified

- No cargo / no tests under the current envelope (Bessa: code only this session). Verified statically instead: `rustfmt --edition 2021 --check` clean on all three touched .rs files; ftl parity scripted 779/779 keys both directions with every `tr("hint.hud-*")` call site resolving in both catalogs; symbol census grepped (`brush_params` paint_state.rs:2114, `cursor_radius_px` viewport.rs:1534, `brush_cursor::{shape,paint}` brush_cursor.rs:142/161, `stencil_owns` in scope at the chord, `adopt_live_alpha` defined + called once). The new unit test and the full battery (fmt + clippy -D warnings + umber-app suite) run at the next sanctioned battery; CI at the next wave push. This kill is committed LOCAL only and rides that push.

## Deferred

- Vertical = angle chord: gated on the angle param (named in the chord's comment).
- "The whole drag is one undoable step" (G1 row): brush params are not journal entries; the resize chord shares this tail.
- OS-bitmap cursor path (`set_cursor_image` behind a plugin) — separate G1 row tail, still open.

## Process note

- Adopted dead-session bytes per the adoption rule: holder exit confirmed (ps census, static mtimes, empty locks dir), fresh lock, static gates re-derived rather than trusted. No sibling touched the tree during the kill (the only other live session, 18296, works the hermes-agent tree, not umber). No push this session (no battery possible); the two commits ride the next wave's Gate 2.
