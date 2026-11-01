# Kill report: per-field defaults in the property context menu (g4-property-context-menu tail)

Date: 2026-11-01 ~11:3x (dragon local). Lock: `g4-prop-defaults`.
Envelope: code only, no cargo/tests (host envelope); rustfmt sanctioned.

## Scope

GUI plan G4 `g4-property-context-menu` row's open items: "Assign Shortcut +
per-field defaults". This kill lands the per-field-defaults half; Assign
Shortcut stays open (it needs an action-per-property registry — named in the
row, not parked silently).

## What landed (`f030012`)

`crates/umber-gui/src/widgets/value_field.rs`:

- `MenuLabels` gains `set_default: Option<Cow<str>>` (None = not offered).
- `ValueField` gains `default_cb: Option<Rc<dyn Fn(f64)>>` +
  `.with_default_cb(...)`; the context menu shows "Set as Default" when both
  the label and the callback are present, and clicking it reports the
  field's current STORED value to the callback.
- Test-side `MenuLabels` literal updated (`set_default: None`).

App side:

- `i18n::prop_menu_labels()` passes `propmenu.set-default` ("Set as Default"
  / "Definir como padrão"; catalogs now 829/829, parity clean, 636 tr sites
  resolve — inline fileless check).
- `Preferences` gains persisted `ui_scale_default: f32` (line
  `ui_scale_default=`, lenient parse clamped to SCALE_MIN..=SCALE_MAX,
  Default 1.0) — the first per-field default.
- `main.rs` preferences scale row: the field now carries
  `.default_value(p.ui_scale_default)` (so Reset/Backspace restore the
  persisted default, not a pinned 1.0) and a `.with_default_cb` that reports
  through ctx temp data (the field itself mutably borrows `p.ui_scale`, so
  the callback cannot capture `p`); the value is applied to
  `p.ui_scale_default` right after `ui.add(field)`.

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on all 4 touched .rs files.
- ftl parity 829/829 both directions, 0 unresolved tr() sites (inline).
- NOT run: cargo/tests (envelope) — menu flow + parse round-trip run at the
  next battery.

## Row status

`g4-property-context-menu`: per-field defaults landed (`f030012`, LOCAL
unpushed); Assign Shortcut remains open pending an action-per-property
registry.
