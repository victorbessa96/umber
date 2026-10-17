# Kill report: g7-accesskit-sweep (umber-gui half)

- Authorship: dragon (solo, host-down pass; the cross-review claw holds
  no umber-gui surface).
- Kill-lock `wgui2-accesskit-sweep`:
  `crates/umber-gui/src/widgets/{color_picker,curve,ramp,swatch_grid,
  tool_row,value_field}.rs`.
- Implements GUI plan row `g7-accesskit-sweep` (gui-plan:149):
  "widget_info on every painter-drawn widget: ToolRow, SwatchGrid,
  layer rows, picker, ramp, curve, gizmo" — the umber-gui half.
  (context_rail already carried its own info from the g2-context-rail
  kill.)

## Built

Per-widget `Response::widget_info` with the semantic `WidgetType`
(egui 0.36.2 `data/output.rs:658` `WidgetInfo::labeled`), each at the
interactive Response's allocation:

- `color_picker.rs` (4): SV square (Slider, "saturation / value"), hue
  strip (Slider, "hue"), the previous-colour restore swatch (Button,
  "previous colour"), the inline `color_button` (Button, "colour").
- `value_field.rs` (1): the number field (Slider, value =
  `Numeric::to_f64(self.value)`, label = `self.unit`).
- `curve.rs` (1): the editor canvas (Slider, "curve").
- `ramp.rs` (1): the gradient strip (Slider, "gradient").
- `swatch_grid.rs` (1): the grid body (Button, "colour palette").
- `tool_row.rs` (1): the row's union Response (Button, the ACTIVE
  tool's tooltip — re-derived per frame, so the label tracks the
  selection).

Each site's comment carries the `g7-accesskit-sweep` marker.

Tests (kittest, in-file, one per widget; the `get_by_` family panics
on miss, so the query is the assertion):

- `the_row_reports_its_active_tool_to_accesskit` (tool_row) —
  Button + the active tool's tooltip via `get_by_role_and_label`.
- `the_field_reports_a_slider_role_with_its_value` (value_field) —
  Slider role; value contains "4.5".
- `the_inline_swatch_reports_a_button_role_to_accesskit`
  (color_picker) — Button/"colour".
- `the_curve_canvas_reports_a_slider_role_to_accesskit` (curve).
- `the_ramp_strip_reports_a_slider_role_to_accesskit` (ramp).
- `the_palette_grid_reports_a_button_role_to_accesskit`
  (swatch_grid).

## Judgment calls

- **English-only labels in the widget layer.** The widgets hold no
  i18n (the house rule: panels localize; widgets receive translated
  strings). The accesskit labels are therefore English constants —
  the app-side consumers (panels) already carry localized labels of
  their own. Named as the follow-up: a `WidgetLabels` bag threaded
  like `MenuLabels` if localized a11y labels are ever wanted at the
  widget layer.
- **Slider for editor canvases** (curve/ramp/SV/hue/value): a drag
  editor is closer to Slider than Button in the accesskit tree —
  ATs announce value changes; Button would misreport the interaction
  class.
- **`self.unit` as value_field's label**: the field's only built-in
  text. Panel-level field naming (the `tr()` strings) lives outside
  the widget; the follow-up above covers richer naming.
- **swatch_grid labels the BODY, not per-cell**: the grid is one
  Response with internal hit-testing; per-cell a11y needs per-cell
  allocation — a shape change named as the deferred item, not
  smuggled into a sweep.
- **`Numeric::to_f64`** (emath 0.36.2 numeric.rs:12) — `f64::from`
  only covers f32.

## Unverified

- The six kittests are UNRUN (build host down since 18:2x; the
  battery is the first run). Static verification: rustfmt PASS, all
  APIs resolved against the pinned registry (WidgetInfo::labeled
  output.rs:658; Response::widget_info response.rs:869; Queryable's
  get_by_role_and_label query.rs:147+; Harness::new_ui lib.rs:946;
  IconAtlas::empty existing; Curve/CurveEdit/Ramp/RampEdit/
  SwatchGrid constructors re-read from the files themselves).

## Deferred

- `icons.rs`'s `icon_button` (painter-drawn, app-wide): its own
  micro-kill — icons.rs is outside this lock and every call site
  inherits from one edit there.
- Per-cell swatch responses (shape change).
- Localized a11y labels at the widget layer (needs the same threading
  as MenuLabels).
- The panel-layer widgets (layer rows, gizmo, the app's panels) —
  the sweep's umber-app half, on surfaces held by the
  gesture-integration lock.

## Process note

- Build-host outage (18:2x) parked the battery; this kill is the
  host-free half of the row. The wave gate runs the six kittests with
  the rest of W-GUI-2 when the host returns.
