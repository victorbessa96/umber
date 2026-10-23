# Kill report — a3-picker-app-sites (the W-C3a plan's lane A3)

Date: 2026-10-20. Author: dragon (local-10e6ed18). Lock:
`a3-picker-app-sites` (brush_panel.rs, fill_layer.rs, preferences.rs,
both catalogs, this report). Coded host-free; UNRUN until the
recovery battery. Consumes G1 `picker-sliders-recents`' parked
`color_button_with` (its bytes are in this same tree, battery-green
together at recovery).

## Built

1. `preferences.rs` — the shared-recents core:
   - `SWATCHES_FILE`/`swatches_path()` — the `swatches` file beside
     `preferences` in the same state dir; plain text, one `#rrggbb`
     per line, most recent first, unknown lines skipped (the
     key=value-free shape the plan named).
   - `parse_hex_swatch` — exactly six hex digits after `#`
     (case-insensitive), else the line is skipped.
   - `load_swatches`/`save_swatches` — the file is most-recent-first;
     loading rebuilds oldest-first so `push` re-lands the newest at
     the front; the capacity cap trims an over-long file on load;
     saving mirrors `Preferences::save` (create the dir, errors log,
     the session keeps its values).
   - `picker_button(ui, linear)` — the shared colour-button row:
     `PickerLabels` built from the new `picker.*` keys through
     `tr()`, the recents held session-wide in egui memory under one
     fixed Id (`umber::picker-recents`), seeded from the swatch file
     on first use, written back to memory every call and persisted to
     the file on any change (clone-compare against the pre-call
     snapshot; `RecentColors` is `PartialEq`).
   - Four codec tests (per-test unique temp dirs): round-trip
     most-recent-first; garbage + uppercase-hex lines skipped;
     a 40-line file trims to the 16-cap keeping the file's order;
     a missing file loads empty.
2. The three sites migrate to `crate::preferences::picker_button` —
   one line each: `brush_panel.rs` (the preset colour row),
   `fill_layer.rs` ×2 (Solid's colour, Gradient's A/B stops). No
   signature changes; the changed-handling at each site is untouched.
3. Both catalogs grow the shared `picker` group (`.r .g .b .h .s
   .v`) — channel letters, the international convention, identical
   attribute sets in both locales (python-verified host-free; a
   locale may localize).

## Judgment calls

- The dance is centralized in `preferences.rs` (the plan's
  "no new module" honored): each site is a one-line swap, and the
  seeding/persistence logic exists once. `preferences` owns the
  swatch file, so the widget stays file-agnostic (G1's boundary).
- egui 0.36.2's `IdTypeMap::get_temp`/`insert_temp` (registry-resolved
  from the vendored egui source, not guessed): the Clone-based pair —
  `RecentColors` is Clone + Send + Sync (Vec<Color32> + usize), so
  the memory slot never fights a borrow.
- The stock `color_button` is NOT converted to a delegate: after the
  migration its consumers are exactly its kittests (grep-verified —
  no app call site remains), which satisfies rule 6. The delegate
  conversion is a named follow-up for when the kittests migrate;
  doing it blind (no way to run the pinned popup kittests host-free)
  risks breaking them at the battery for zero user-visible gain.
- graph-history's manifest re-pinned for the additive catalog keys
  (recorded in its manifest header): the only bytes of its surface
  this kill touches are the two ftl files, appended-to only.

## Unverified

- UNRUN (no compile host-free): the four codec tests, the three
  migrated sites, and the i18n pair-completeness over the new keys
  all verify at the recovery battery (umber-app + the i18n test).
- Every API was read from the live tree before use:
  `color_button_with`'s signature from G1's parked bytes,
  `RecentColors`/`PickerLabels` shapes, `tr() -> String`, egui's
  IdTypeMap accessors from the vendored 0.36.2 source.
- Post-report correction (2026-10-20, the ad-hoc verification pass):
  the garbage-lines test's fixture originally wrote the uppercase
  sample as "uppercase #AB12CD" — a line that never starts with `#`,
  so the parser correctly skipped it and the test's three-colour
  expectation was wrong. Caught by the python codec port BEFORE any
  run; the fixture is now a plain `#AB12CD` line (the fix is the
  test only, the codec unchanged), the manifest re-pinned, and the
  corrected fixture passes the port byte-for-byte.

## Deferred

- The stock-button delegate conversion (see Judgment calls).
- The swatches panel UI (a visible swatch shelf beyond the popup's
  grid) — never in this lane's contract.

## Gates

- Battery: umber-app. The close script's a3 commit row carries this
  manifest's paths (the two catalogs shared with graph-history's
  row — both rows commit from the same bytes; graph-history's row
  lands first, a3's row then commits the remaining paths; a file
  listed in two manifests is committed once, by the earlier row, and
  the later row's `git add` of it is a no-op for identical bytes).
