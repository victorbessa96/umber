# Kill report — falloff-ui (the two brush controls, wired)

Date: 2026-10-15. Author: dragon. Lock: `falloff-ui` over
`crates/umber-app/src/brush_panel.rs`, `crates/umber-app/src/main.rs`
(added: the one-line per-frame sync), `assets/i18n/en-US.ftl`,
`assets/i18n/pt-BR.ftl`, this report. Source: the deferred UI rows of the
`falloff-curves` and `front-face-falloff` kills (2026-10-14).

## Built

- **Falloff curve** combo in the brush panel, under the wash mode: every
  `FalloffCurve::ALL` entry, stored in the preset (`preset.params.falloff`,
  so it saves with the `.umberbrush`). `falloff_key` is an exhaustive
  match, so a new curve fails to compile until it has a label.
- **Front-face falloff** in the Projection group: a checkbox (off by
  default) and, when on, the start angle (0–89°, seeded at 80° on first
  enable, near Blender's default). Panel state like `projection`, synced
  to the viewport each frame next to the view-mode sync.
- 11 keys in each catalog (`brush.falloff`, the nine curve names,
  `brush.front-face-falloff`); pt-BR translated, the parity test covers
  both.
- Tests: every curve has a distinct key that resolves in en-US; the
  panel's front-face default is `None`.

## Judgment calls

1. **Curve in the preset, front-face in the panel.** The curve is part of
   a brush's character (Blender stores it per brush), so it travels with
   the preset. Front-face falloff is a viewing-condition setting, like
   the projection mode beside it, so it stays session state.
2. **Range 0–89°.** 90 is "off" in the engine, which the checkbox already
   expresses; allowing 90 in the field would be a second off switch.

## Unverified

- No snapshot test of the panel with the new rows (the panel's existing
  snapshot coverage does not include the brush panel).
- pt-BR wording ("Atenuação de faces oblíquas", the curve names) wants
  the native-speaker review row `wave-i18n-audit` already tracks.
