# Kill report — agent-stroke-conditioning (wire strokes paint exactly)

Date: 2026-10-15. Author: dragon. Lock: `agent-stroke-conditioning` over
`crates/umber-brush/src/wiring.rs`, `crates/umber-paint/src/paint_state.rs`,
`crates/umber-cli/src/agent.rs`, `crates/umber-cli/tests/agent_paint_ops.rs`,
`docs/specs/agent-paint-ops-design.md`, `docs/ROADMAP.md`, this report.
Source: ROADMAP row `wave-agent-stroke-conditioning`, found by the
harness-script kill's snapshot test.

## Root cause (computed, then pinned)

`paint_stroke` fed wire points through the live pointer conditioner:
one-euro (min_cutoff 1 Hz, beta 0.007) + lazy-mouse, over a synthetic
clock of 8 ms per event. At 1 Hz / 8 ms the one-euro's smoothing factor is
α ≈ 0.048 (≈ 0.098 once the speed term adds), so each event moves the
filtered cursor about a tenth of the way. A 2-point stroke 27.6 texels long
moved the cursor ~2.7 texels: less than one dab spacing (24 / 5 = 4.8),
so it emitted **zero dabs**. A 9-point stroke spanning 29 texels travelled
14 — it painted, but half as long as sent. Pointer smoothing is right for
a dense, noisy stylus stream and wrong for geometry an agent already
decided.

## Built

- `StrokeConditioner::set_exact(bool)` / `is_exact()`: exact mode skips the
  one-euro filters and the lazy-mouse leash, so the cursor is the event
  position and pressure is clamped, not filtered. Spacing and the line
  constraint still run. The switch resets the pipeline, so no filter state
  crosses it. The shared tail (line constraint, seeding, spacing) moved
  into `advance()` so both modes use one implementation.
- `PaintState::set_exact_input(bool)` (between strokes only). The GUI
  never calls it.
- `paint_stroke` turns exact input on before every stroke.
- Tests: `exact_mode_paints_a_short_stroke_the_filter_swallows` (the
  smoothed 2-event 50 px stroke emits < 2 dabs; exact emits exactly 10,
  the last on the end point), `exact_mode_is_unfiltered_and_the_switch_resets`,
  and the agent-level `a_two_point_stroke_paints_where_it_was_sent`
  (midpoint and both ends painted, a far texel untouched).

## Judgment calls

1. **Always exact on the wire, no `smoothing` field.** A wire stroke is
   already intended geometry; nobody sending points over JSONL wants them
   dragged toward each other. A field would be a knob with one sane
   setting. Revisit if a client streams raw tablet samples over the wire.
2. **A conditioner mode, not a second paint path.** Spacing, seam
   mirroring, symmetry and the mask / retouch hooks all live downstream of
   the conditioner, so one flag keeps every one of them shared.
3. **Spacing stays.** Dab density is the brush's property, not the
   input's, so exact mode still spaces dabs at radius / dabs-per-radius.

## Unverified

- Path replays (`replay_path`) still go through the smoothed path. They
  are also intended geometry, so the same flag probably belongs there;
  not changed here (the GUI replay's feel is a UX call).
