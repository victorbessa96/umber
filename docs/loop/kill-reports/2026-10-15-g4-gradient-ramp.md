# Kill report: g4-gradient-ramp (the colour ramp widget)

Date: 2026-10-15. Author: dragon. Lock: `g4-gradient-ramp` over
`crates/umber-gui/src/widgets/{ramp,mod}.rs`, `crates/umber-gui/src/lib.rs`
and this report. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g4-gradient-ramp` (B:D19, Blender's ColorRamp).

## Built

- `widgets/ramp.rs`:
  - `Ramp`, the model: sorted `Stop { position, color: Rgba }`, an
    active-stop index, and `Interpolation { Linear, Ease, Constant }`.
    `evaluate(t)` is the only colour read. `add_stop` (takes the colour
    the ramp already has there, becomes active), `remove_active` (refuses
    the last stop), `move_stop` and `active_mut` (stable re-sort, the
    active index follows its stop), `flip`, `distribute`.
  - `RampEdit`, the widget: a gradient strip (a vertex-grid `Mesh`
    coloured by `evaluate`, with extra hard-edge columns in Constant
    mode) and a triangle handle per stop (accent outline = active). Drag
    a handle to move it, click to make it active, Ctrl/Cmd+click on the
    strip to add a stop.
  - Free functions `pick_stop` (nearest handle in radius; on a tie the
    active stop, then the later one) and `position_at`.
- `widgets/mod.rs`, `lib.rs`: module + re-exports (`Ramp`, `RampEdit`;
  `Interpolation` and `Stop` through `widgets`).

## Tests (12, in `ramp.rs`)

The Ease pin is exact: smoothstep(0.25) = 0.15625. Linear-light blending
is pinned with a midpoint of (0.5, 0.5, 0) for red→green; an sRGB blend
would give a different value. `add_stop` is pinned to leave nine sampled
points of the gradient unchanged. The move test drags red past the middle
stop and checks that the order AND the active stop are right. There are
also flip/distribute, pick tie-breaks, mesh vertex count and colours, and
one headless egui test where a Ctrl+click at 25 % of the strip adds a
third stop at about 0.25 and makes it active.

## Judgment calls

- **Linear light.** Stops hold `egui::Rgba` (linear). The paint engine
  composites in linear light, so a ramp feeding a gradient-map filter
  must blend there. The strip converts to `Color32` (sRGB) only for
  display.
- **The active index is the accessibility path.** Tiny handles are not
  the only way in: the consuming panel edits `active_stop()` /
  `active_mut()` through ordinary Position and Color fields (the plan's
  "accessibility fallback for tiny targets").
- **No consumer in this kill.** The gradient-map filter and the
  generators that will use the ramp do not exist yet. The widget lands in
  `umber-gui` ahead of them, like the notices widget did before its app
  wiring.

## Unverified

- Drag-to-move through real pointer events has no headless test (the
  model's `move_stop` is pinned; the drag plumbing is three lines over
  it). The handle drawing has no snapshot test yet.

## Deferred

- Position/Color field rows, the flip/distribute buttons, and the
  interpolation combo inside a panel: they land with the first consumer.
- An AccessKit role on the strip (g7-accesskit-sweep lists the ramp).
