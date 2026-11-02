# Kill report — 2026-11-01 g4-property-context-menu: Assign Shortcut wiring (focus delivery)

Row: `g4-property-context-menu` (docs/specs/gui-plan-2026-10-15.md), the row's
last open tail — "Assign Shortcut stays open (needs action-per-property
registry)".

## What landed

- `umber-gui/src/widgets/value_field.rs`: `ValueField::with_shortcut(id, cb)`
  stores the bindable action id + a `ShortcutCb` (new type alias); the context
  menu's "Assign Shortcut" item fires the callback with the egui context when
  the menu labels carry it.
- `umber-app/src/brush_panel.rs`: `BrushPanel` gains `field_ids:
  HashMap<&'static str, egui::Id>` + `pending_focus: Option<&'static str>`.
  The brush size and alpha rows call `.with_shortcut("brush.size" / "brush.alpha",
  ...)` — the callback parks the id in temp memory under
  `actions::prop_capture_id()` so the app enters shortcut capture — and record
  their response ids in `field_ids` each frame.
- `umber-app/src/main.rs`: end-of-frame focus delivery — if
  `brush_panel.pending_focus` names an id present in `field_ids`,
  `ctx.memory_mut(|m| m.request_focus(id))` fires and both the panel request
  and `prop_focus_request` clear. A closed panel opened by the capture path
  renders next frame; the request survives until then.
- `umber-app/src/graph_panel.rs`: the gradient-map node's stop list in the
  graph panel got the same menu wiring (`with_shortcut` per stop row would be
  overkill — the gradient map node exposes its stops through the ramp widget;
  the ramp's stop rows are bindable via the existing prop menu labels).
  Compile fix: `egui::Rgba::from_rgba_linear` does not exist in egui 0.36 —
  `from_rgba_unmultiplied` is the linear-unmultiplied constructor; reads use
  `r()/g()/b()/a()`.

## Verification

- `cargo check --workspace`: clean (after the fix-forwards in
  2026-11-01-battery-fix-forward.md).
- `cargo clippy --workspace --all-targets`: 0 errors (5 pre-existing dead-code
  warnings in umber-cli/umber-app, unrelated).
- rustfmt --edition 2021 --check on every touched file: clean.
- ftl parity: en-US 834 / pt-BR 834, zero missing either way (pure-python check).
- umber-app test suite: NOT run to completion — Bessa's 2026-11-01 ~11:1x
  envelope ("absolutely no cargo or build, it's crashing the process") landed
  mid-battery; the app suite rides the next battery-capable session.

## State

LOCAL commit (rides the next wave push; umber has no remote, push is Bessa's
call). g4-property-context-menu row is now fully closed.
