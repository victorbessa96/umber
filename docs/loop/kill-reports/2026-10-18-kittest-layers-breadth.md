# Kill report: kittest layers breadth (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, wakeup #4). Kill-lock
`kittest-layers-breadth`, holding
`crates/umber-app/tests/panel_kittest.rs` (+ `layers_panel.rs`
defensively; NOT edited — no seam was needed).

Context: C2-28's named follow-up — "More panels ride as their state
becomes headless-constructible". The Layers panel's breadth rows
(duplicate ⧉, the selection-gated properties rows, the blend combo)
had zero kittest coverage; `panel_kittest.rs` carried nine tests, the
layers ones gesture-level only (mask thumb, drag handle, row eye).

## Built (insert-only, three tests + one shared closure)

- `layers_ui_closure(active)` — the row-eye test's draw closure
  factored once (empty materials context, Base visible, empty
  badges/thumbs/gestures), parameterized on the active plane — the
  axis under test.
- `the_duplicate_button_copies_the_layer_row` — the ⧉ glyph button
  (the row's deliberate no-tooltip duplicate, its catalog key waiting
  on the stencil i18n lock) drives `duplicate_layer`: two layers, the
  copy named `"one copy"` (`duplicate_name`), same kind, settings
  carried. The button is located by its accessible label — egui's
  text→label auto-mapping (the assumption is named here; battery
  catches a mapping difference loudly).
- `the_properties_rows_render_for_the_selected_layer_only` — the
  opacity/blend/rename rows (all three `history.op-*` labels) render
  when the LAYER is the active plane and stay hidden while Base is —
  the selection gate, both legs.
- `the_blend_combo_sets_the_selected_layers_mode_and_undo_restores`
  — the channel-picker idiom: open the (single) ComboBox in the
  layers draw, click the `Multiply` option (the options carry the
  modes' own Debug spellings — no i18n involvement), the layer's
  `blend_mode` becomes Multiply, and ONE History undo restores
  Normal — pinning the one-command-per-change contract at the UI
  level.

## Judgment calls

1. No new i18n keys — both catalogs are stencil-locked; every key the
   tests reference (`history.undo`, `.op-set-opacity`,
   `.op-set-blend`, `.op-rename`) was verified present in BOTH
   catalogs first (Fluent's group+attribute scheme — a flat dotted
   grep finds nothing; the key set was built the correct way).
2. The opacity VALUE is not kittest-driven: `ValueField`'s own test
   suite pins the field as a Slider ROLE whose numeric VALUE exposure
   is "egui's accesskit mapping, not ours to pin" — driving the value
   would pin a mapping the widget's own tests refuse to own. The row
   is covered at the render/selection level instead; the command path
   is covered by the blend test's undo leg.
3. No undo leg on the duplicate test: `duplicate_layer` is a
   multi-command sequence (add, reorder, setters-when-nondefault) —
   exact step counts are its own contract, not this breadth kill's.

## Unverified

- No compile/test run (host down since 10-17 21:48; the outage
  contract parks the battery). Compile risk swept statically: every
  path fully qualified (`umber_app::document::*`,
  `umber_app::paint_state::*`) — the file's per-fn `use` idiom means
  top-level items cannot borrow those names; every API
  (`Document::layers()` → `&[Layer]`, `duplicate_name`, `layers_ui`'s
  8-arg signature, `MaterialPanelContext`'s 5 fields) matched
  against the row-eye test's live usage verbatim. The kittest legs
  run at the battery (egui + the app crate build, no GPU).
- The two named assumptions (the ⧉ label auto-mapping; `Multiply` in
  a Paint layer's option list — `layer_blend_options(Paint)` is the
  full separable set per its own folder-only-Passthrough test) fail
  loudly if wrong, at the battery.

## Gates run (host-free, this session)

- `rustfmt --check --edition 2021`: CLEAN on the file.
- `scripts/verify-kill.sh`: PASS (brace census, trap greps,
  doc-attachment, lock audit).
- i18n census: all four referenced keys present in BOTH catalogs
  (group-header + `.attr` membership, the dotted-key scheme).

State: STATICALLY VERIFIED, IN FLIGHT — battery (scope: umber-app) +
commit + push parked on the build host's return, its own commit row
per the manifest labels.

## Addendum — battery-caught defect in this kill's blend-combo kittest, fixed + re-verified green (2026-10-19, wake session)

The local battery's first full run (2026-10-19 04:3x -03,
/tmp/local-battery.log, BATTERY_EXIT=101) caught this kill's
blend-combo kittest RED ON ARRIVAL: its harness drew the
`layers_ui_closure` only, so the `history.undo` query — the ↶ Undo
button lives in the HISTORY panel — had no node to find and panicked
at `tests/support/mod.rs:81` (819 tests green ahead of it across
stylus / umber-app lib / umber main / gate0_journey). Root cause: the
kill's gates were rustfmt + verify-kill (both honest passes) but NO
test execution existed at park time, so a red-on-arrival test was
invisible to every gate — precisely the gap the battery exists to
close. Fix: the test's closure now draws `layers_ui` AND
`history_panel::history_ui` in ONE harness (the combo in the layers
properties rows, the ↶ Undo button in the history panel, one
document). Gates: rustfmt CLEAN; re-verified live: the test passes
(0.14s) and the full `panel_kittest` binary is 12/12 (2026-10-19,
`cargo test -p umber-app --test panel_kittest`). The
`panel_kittest.rs` row re-pinned below; this report's row re-pinned
with it. Recovery row UNCHANGED: same file, same battery scope
(umber-app), same lock release at the combined commit.
