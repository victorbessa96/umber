# Kill report: C2-12 the colour picker (g4-color-picker), widget half

Date: 2026-10-15. Author: dragon. Lock: `c2-12-color-picker` over
`crates/umber-gui/src/widgets/color_picker.rs` and this report. Source:
GUI plan `g4-color-picker` and audit C2-12.

## Built (`color_picker.rs`)

- **`ColorState`, the model:**
  - It stores LINEAR RGB for the engine, plus its own HSV in the
    sRGB-encoded space, so the SV square reads perceptually.
  - **The hue survives greys:** dragging saturation to 0 and back keeps
    the hue. Black keeps hue and saturation, and so does a grey typed as
    hex.
  - Hex is always sRGB (`#rrggbb`; `#rgb`, no `#`, any case, surrounding
    whitespace). A bad hex changes nothing, and non-ASCII input never
    splits a byte.
  - `previous` + `revert` give the current/previous pair.
  - Non-finite input is clamped out, so it never poisons the state.
- **The sRGB transfer** is the IEC 61966-2-1 piecewise curve, not gamma
  2.2. umber-gui stays egui-only (the layer rule), so it brings no
  colour-management crate.
- **`ColorPicker`, the widget:**
  - the SV square as a 17×17-vertex `egui::Mesh` and the hue strip as 36
    segments, so no texture upload per drag;
  - Ctrl/Cmd-drag on the strip snaps hue to 30° (`snap_hue`, `hue_at`);
  - the current/previous swatches (click previous to revert);
  - a hex field committed on focus loss.

## Tests (9)

- The sRGB round trip over all 256 codes, including the 0.5 → 0.214
  anchor that tells the curve from a plain gamma.
- HSV ↔ RGB round trips.
- The field round-trips the colour.
- **Hue survival** through grey, black and a hex grey. Mutation check:
  removing the keep-hue branches fails it.
- Hex parsing, including 7 bad forms that leave the state unchanged.
- Revert.
- The 30° snap and its wrap.
- Non-finite safety.
- **kittest:** a click at a known point of the square, with the rect
  recorded from the frame, moves s/v to that point and reports
  `changed()`, and leaves the hue untouched.

Verified in a registered remote copy (`pub mod color_picker;` added
there). umber-gui as a whole is green: 91 tests, with curve and
color_picker both registered. Clippy is clean.

## NOT done: lock-blocked (the sibling's `g1-axis-gizmo` holds `widgets/mod.rs`)

1. Register it in `widgets/mod.rs`: `pub mod color_picker;` + `pub use
   color_picker::{ColorPicker, ColorState};`. `curve` is in the same
   state.
2. Replace the three `color_edit_button_rgb` sites (`brush_panel.rs:487`,
   `fill_layer.rs:1125/1146`) with a swatch button opening a
   `ColorPicker` popup. They need step 1 first.

## Deferred (plan items not in this slice)

- HSV/RGB numeric sliders with tinted tracks.
- The inline eyedropper (it needs a viewport readback hook).
- Swatches persisted across sessions (`SwatchGrid`'s `RecentColors` is
  the existing store to wire).
- The Linear/Perceptual readout toggle.
- Floating, non-dockable placement.
