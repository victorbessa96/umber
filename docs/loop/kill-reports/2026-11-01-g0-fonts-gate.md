# Kill report: fonts gate in tests/support (g0-kittest-harness remnant)

Date: 2026-11-01 ~02:1x (dragon local). Lock: `g0-fonts-gate`.
Envelope: code only, no cargo/tests (host envelope); rustfmt sanctioned.

## Scope

GUI plan G0 `g0-kittest-harness` last open parenthetical: the fonts gate
("skip frames until named families exist"). No kittest file installed the
Umber fonts before this kill — the gate did not exist anywhere.

## What landed (`428cdc2`)

`crates/umber-app/tests/support/mod.rs`:

- `pub const UMBER_FONT_KEYS: [&str; 2] = [INTER_KEY, MONO_KEY]`
  (`umber_gui::fonts::{INTER_KEY, MONO_KEY}` = "Inter", "JetBrainsMono").
- `pub fn fonts_gate(harness, keys)`: installs `umber_gui::umber_fonts()`
  on the harness context if Inter is not already in the definitions, steps
  one frame so the atlas pass runs, then asserts every key is in
  `font_data` AND first in its family. Loud panic on failure, same
  discipline as `settle`.
- `settle()` now calls `fonts_gate(&UMBER_FONT_KEYS)` first, so every
  support-built harness (all migrated kittest files call settle) has the
  Umber fonts realized before any font-bearing snapshot compares.
- Module doc bullet added.

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on the touched file.
- API checked against the vendored sources: `Harness.ctx` is a pub field
  (egui_kittest 0.36.2 lib.rs:73), `Context::fonts` reader + `FontsView::
  definitions()` (epaint 0.36.2 fonts.rs:826), `FontDefinitions{font_data,
  families}`; font bytes resolve via `CARGO_MANIFEST_DIR/../../assets/fonts`
  when the `embedded-fonts` feature is off (default), and the TTFs exist in
  the repo.
- NOT run: the kittest suite itself (no cargo per envelope) — the gate
  executes inside every settle at the next battery-capable wave.

## Row status

`g0-kittest-harness` parenthetical is now fully closed: panel_snapshot
migration `e8b4b64` + fonts gate `428cdc2`, both LOCAL unpushed.
