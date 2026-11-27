# Kill brief: glyph-button accessible names

- Lock: `razul-glyph-accessibility`.
- Trigger: UI/UX audit finding 4 remains live. Several glyph-only buttons expose only symbols to AccessKit; the census also found duplicate and post-effect move arrows beyond the named examples.

## Required behavior

- Route layer duplicate/reorder, smart-shelf refresh, saved-search delete, asset refresh, and post-effect remove through the mandatory-tooltip `glyph_button` helper.
- Give post-effect move arrows localized `WidgetInfo::labeled` names with the correct enabled state; do not alter their layout or enabled behavior.
- Add a recursive app-source census that rejects bare `small_button` calls using the known glyph literals. Existing text buttons are out of scope.

## Verification and constraints

Run full `umber-gui` and `umber-app` suites, strict clippy, rustfmt, and independent review. Confirm the census fails on the old bare calls and passes after migration. No visual redesign, release/tag/upstream PR, or centralhost work.
