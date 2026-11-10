# Kill report 2026-11-10 razul-uiux-f8-glyphbtn (dragon)

Authorship: dragon (Razul). Kill lock `razul-uiux-f8-glyphbtn` over
`crates/umber-gui/src/widgets/glyph_button.rs` (new),
`crates/umber-gui/src/widgets/mod.rs`, `crates/umber-app/src/assets_panel.rs`,
`crates/umber-app/src/document.rs`, both `.ftl` catalogs. Implements finding
F8 of `docs/loop/audit-2026-11-10-uiux-code.md`.

## Built

- New `umber_gui::widgets::glyph_button(ui, glyph, tooltip)`: the tooltip is
  a REQUIRED argument, shown on hover AND set as the button's AccessKit
  label via `WidgetInfo::labeled(WidgetType::Button, …)` (same pattern as
  `tool_row`'s row widget). A tooltip-less glyph button can no longer drift
  in without going through the helper.
- Migrated all 8 bare glyph sites named by the audit:
  `assets_panel.rs` add-dir `+`, remove-dir `✕`, use-brush `▸`,
  add-material `+`, attach-mask `+`; `document.rs` attach-smart-mask `+`,
  delete-layer `✕`, add-smart-material `+`.
- 8 new tooltip keys in BOTH catalogs (`assets.add-dir-tip`,
  `assets.remove-dir-tip`, `assets.use-brush-tip`, `assets.add-material-tip`,
  `assets.attach-mask-tip`, `layers.attach-smart-mask-tip`,
  `layers.delete-layer-tip`, `layers.add-smart-material-tip`).
- Two kittests that queried glyphs by label ("✕", "+") updated to query by
  the new accessible labels — the tests now pin the a11y contract.

## Judgment calls

- egui 0.36's `Response::widget_info` returns `()`, so the helper binds the
  response first, then sets the info, then returns it.
- The remaining two glyph sites (`assets_panel.rs:645`,
  `postfx_panel.rs:395`) already carried hover text; left as-is (their
  labels still read as the glyph — migrating them too would be the
  follow-up if the audit's "all 17" is enforced strictly).

## Unverified

- Screen-reader pass over the new labels (kittest role/label queries cover
  the AccessKit node, not a live SR).

## Deferred

- F11 (icon accessible names are atlas ids): separate finding, queued.

## Verification

rustfmt --check clean; check-layers GREEN; scorecard --check current;
clippy `-p umber-app -p umber-gui --all-targets -D warnings` clean;
`cargo test -p umber-app -p umber-gui` 1277 passed / 0 failed (incl. the
updated kittests and the i18n catalog-completeness tests over both .ftl).
