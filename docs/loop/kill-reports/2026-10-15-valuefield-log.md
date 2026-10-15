# Kill report — ValueField log mode (wave-valuefield-log, part 1)

Date: 2026-10-15. Author: dragon. Lock: `valuefield-log` over
`crates/umber-gui/src/widgets/value_field.rs`,
`crates/umber-app/src/postfx_panel.rs`, this report. Source: ROADMAP wave
6a `wave-valuefield-log`; the postfx-migration kill's judgment call 1.

## Built

- `value_field.rs`: `ValueField::logarithmic(bool)`. With a finite range
  and `min > 0`, the scrub moves through log space: dragging the field's
  own width traverses the full range, so every decade gets the same drag
  distance (four decades over a 200-pt field = 50 pt per decade, from
  anywhere). The fill bar shows the log fraction. Fine / coarse modifiers
  scale the log step. An incapable range (min ≤ 0, infinite, zero width)
  falls back to linear: never NaN, never a dead field. The math is free
  functions (`log_capable`, `log_scrub`, `log_fill_fraction`), pinned by
  2 new unit tests (equal drag per decade, full width = full range,
  reverse drag, fine factor, non-positive restart, the fallbacks; the log
  fill at the ends, the midpoint and below min).
- `postfx_panel.rs`: DoF focus moves to `ValueField::logarithmic(true)`
  with `step(0.0)` (snapping would quantize the near decade) and 3
  decimals. The post-effects section has no raw `egui::Slider` left.

## Judgment calls

1. **Log scrub scaled by field width, not by `speed`.** A per-point log
   step tuned for one range is wrong for another; "the field's width is
   the whole range" is range-independent and matches the fill bar
   exactly, so the bar's edge tracks the pointer.

## Deferred

- The other raw sliders (fill_layer 6, path_tool 4, bakes 3, display 2,
  layers 1): part 2. They are linear and need no log mode.
