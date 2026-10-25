# Kill report — the pressure response curve (g4-curve-widget's promotion)

Executed 2026-10-25 01:0x by local-7de7d3ed, on Bessa's reopened
"continue" (the session's second wind past the deliberate close).

## The derivation's two-step find

1. **The lens's claim re-verified, half-refuted**: "the
   pressure-curve plot is not yet promoted" — but the CurveEdit
   widget IS promoted, at three sites (brush_panel's falloff
   editor, graph_panel's curves node, the falloff combo's
   same-row placement). The lens's words were imprecise, not
   wrong: the FALLOFF curve is fully shipped; what lacked a
   curve was the PRESSURE RESPONSE —
2. **The real residue**: the pressure response was a scalar
   (`pressure_gamma`, a powf on the multiplier) — and the panel
   DID reach it (the gamma row exists at brush_panel's params
   section; the earlier "no panel row" read was the grep's own
   miss, the key is `brush.pressure-gamma` not `pressure_gamma`).
   The residue: no user-drawn response curve, exactly the g4
   flagship gap.

## What landed

### umber-brush (dab_adapter.rs)

- **The model**: `pressure_curve: ControlCurve` on BrushParams,
  `#[serde(default)]` = identity (every pre-curve `.umberbrush`
  loads unchanged; the ControlCurve's custom serde is the bare
  4-point array, the falloff_custom precedent's own shape).
- **The apply**: both conversion arms
  (`stamps_to_dabs` + `stamps_to_dabs_wash`) shape the multiplier
  `params.pressure_curve.evaluate(m.powf(gamma))` — the curve
  composes AFTER the gamma (the scalar stays the legacy
  shortcut; the curve is the full control; identity evaluates
  byte-identically to the gamma alone).
- **The pins**: `identity_pressure_curve_is_gamma_alone` (the
  back-compat equivalence) + `shaped_pressure_curve_reweights_the_dab`
  (a lifted mid turns a half-press into 0.9 flow). The crate:
  161 green (159 + the two).

### umber-app (brush_panel.rs)

- **The editor**: `pressure_curve_editor` — the falloff editor's
  own widget + helpers (falloff_edit_model/apply_falloff_edit are
  class-generic over ControlCurve; reused as-is, no rename churn),
  CurveEdit 120px fixed-count under its own push_id.
- **The row**: under the gamma row, `brush.pressure-curve` — the
  pressure response becomes user-drawable in the GUI for the first
  time. ROW_LABEL_KEYS + the count pin 58→59.
- **i18n**: both catalogs (pt-BR: "Curva de pressão").

## The named follow-ups

- The curve's preset-library surface: a starter preset or two
  using shaped curves (the library's JSON carries them
  automatically; curation is the follow-up).
- The gamma+curve interaction's tooltip/doc: the panel hint that
  the curve composes after the gamma (the v1 relies on the row
  order to communicate it).
