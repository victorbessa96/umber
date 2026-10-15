# Kill report — postfx-panel migration (the GUI audit's Phase 1, one panel)

Date: 2026-10-15. Author: dragon. Lock: `postfx-migration` over
`crates/umber-app/src/postfx_panel.rs`, this report. Source:
`docs/specs/gui-audit-and-roadmap.md` Phase 1 (panel migration: raw
sliders → the design-language widgets).

## Built

- The post-effects section's 10 raw `egui::Slider`s become 9
  `umber_gui::ValueField` rows in the house grammar (muted label + field,
  the brush panel's shape) through a local `field_row` helper: aperture,
  bloom threshold / intensity / radius, vignette strength / softness,
  sharpen amount / radius, tone-map exposure. Units where they mean
  something (`px` on the radii, `EV` on exposure); steps tuned to each
  range.
- The ranges are unchanged and still mirror `PostEffect::clamped`, so the
  write-back clamp stays a no-op.
- The 7 existing postfx tests pass unchanged (they pin the model, which
  this kill does not touch).

## Judgment calls

1. **DoF focus stays a logarithmic `egui::Slider`.** Its 0.01..=100 range
   spans four decades; a linear drag-scrub field makes the near range,
   where focus lives on a small mesh, unreachable. `ValueField` has no log
   mode. Adding one is the widget crate's change and goes in the GUI plan,
   not this panel.

## Unverified

- No snapshot of the Display panel's post-effects section with a chain
  loaded (the existing `display_panel` snapshot renders an empty chain).
