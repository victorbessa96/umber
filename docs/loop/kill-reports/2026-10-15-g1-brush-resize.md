# Kill report — g1-brush-resize (GUI plan G1)

Date: 2026-10-15. Author: dragon. Lock: `g1-brush-resize` over
`crates/umber-paint/src/paint_state.rs`, `crates/umber-app/src/{viewport,
brush_cursor, actions, toolbar, main, status_bar}.rs`, `assets/i18n/*.ftl`
and this report. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g1-brush-resize-drag`. Sources: Painter C1 ("Ctrl+right-drag: horizontal
changes Size, vertical changes Hardness"; "`[` and `]` change size"),
Blender C4.2 (F-drag size with a numeric readout), photocraft #20 (the
drag HUD at the origin).

## Built

- `paint_state.rs`: a live `brush_size` multiplier on
  `BRUSH_RADIUS_TEXELS`.
  - `brush_size()` / `set_brush_size()` clamp through the path tool's
    `sanitize_size`, so the range is 0.05..=8 and non-finite resets to 1.
  - `set_brush_hardness()` clamps to 0..=1 and ignores non-finite input.
  - `brush_radius_texels()` now includes the size.
  - The viewport's two live-stroke entry points pass `brush_size` as the
    stroke's radius scale. That is the same seam path replays use, so
    replays are untouched: they keep their own per-point scale.
  - Test `brush_size_scales_the_live_radius_and_clamps`, GPU-gated like
    every PaintState test.
- `[` / `]`: `ShellAction::BrushSmaller` / `BrushLarger`.
  - Two registry rows; `SHORTCUT_KEYS` grows to 11 and its pin test holds.
  - Availability is "load a mesh first" with no paint session.
  - Dispatch steps the size by `SIZE_STEP` = 1.1×, so ten presses give
    about 2.6×.
- Ctrl+RMB drag (`viewport.rs`): horizontal is exponential in size
  (150 px doubles it); vertical is linear in hardness (200 px sweeps the
  full range, up = harder).
  - The values are relative to the drag ORIGIN, not accumulated, so
    returning the pointer there restores the exact start values.
  - The ring holds still at the origin and resizes live, with a
    "Size ×1.23  Hardness 0.50" HUD beside it.
  - It runs before the camera, and no preset binds RMB without Alt.
- `brush_cursor.rs`: pure `step_size` and `resize_drag`, plus 2 tests
  (multiplicative and reversible; axes, clamps, and back-to-origin
  identity). The radius probe is factored into
  `Viewport::cursor_radius_px`, shared by the hover ring and the drag
  ring.
- Status hints:
  - Paint shows `[ ]` brush size.
  - Holding Ctrl shows "RMB drag: size / hardness".
  - The HUD labels are in i18n (en-US, pt-BR).

## Judgment calls

1. **The size is a session value, not a preset field.** Painter's size
   lives on the tool; Umber's `BrushParams` has no size today, because
   the radius was a constant. Making size a preset field is the
   `.umberbrush` schema change that the brush-wiring kill already owns
   (stroke methods + cavity), so it rides there.
2. **The hardness edit writes the session brush**, which the next
   preset sync can overwrite. The same rule as judgment call 1: the
   panel/preset round-trip is the brush-wiring kill's.
3. **A drag mid-stroke keeps the stroke's size.** The size is read at
   `begin_stroke`, so a resize never kinks a stroke in flight.

## Unverified

- Live feel of the 150 px / 200 px constants: manual.
- No kittest for the `[` dispatch path: the viewport needs a GPU context.
  The dispatch arm is three lines over the tested pure functions.
