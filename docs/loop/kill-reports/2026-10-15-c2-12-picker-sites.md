# Kill report: C2-12 the colour picker at its three call sites

Date: 2026-10-15. Author: dragon. Lock: `c2-12-picker-sites` over
`crates/umber-gui/src/widgets/color_picker.rs`,
`crates/umber-app/src/{brush_panel,fill_layer}.rs` and this report.
Completes C2-12: the widget landed earlier (`8257c48`) and was
registered in `6ab4869`.

## Built

- **`color_picker::color_button(ui, &mut [f32; 3])`**, a drop-in for
  `Ui::color_edit_button_rgb` with the same linear-RGB contract:
  - a swatch that opens a `ColorPicker` popup below it (foreground
    `Area` + popup frame);
  - the popup's `ColorState` lives in egui memory under the button's id
    while it is open, so the hue survives greys from frame to frame and
    "previous" is the colour the popup OPENED with;
  - a click outside or Escape closes it;
  - it writes the colour back and reports `changed()` when the picker
    moved it.
- **The three sites swapped**: the brush colour (`brush_panel.rs`) and the
  fill's solid colour and gradient stops A and B (`fill_layer.rs`). egui's
  stock picker is gone from the app.

## Tests

`the_color_button_opens_a_picker_and_writes_the_colour_back` (kittest):
- clicking the swatch opens the popup, whose hex field shows `#ff0000`;
  the query is `query_all_by_value`, because a TextEdit exposes its value
  on more than one AccessKit node;
- a click in the SV square writes a different colour back and reports
  the change.

**Mutation check:** dropping the write-back fails it. Gate 1: umber-gui 92,
umber-app 512. fmt and clippy clean.

## Deferred (the plan row's remainder)

HSV/RGB numeric sliders, the inline eyedropper, persisted swatches (wire
`RecentColors`), the Linear/Perceptual readout, and the brush colour's
alpha (the brush keeps its separate Alpha field, as before).
