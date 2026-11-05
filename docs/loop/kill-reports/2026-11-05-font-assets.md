# Kill report: font-assets landing — 2026-11-05

Closes the `font-assets` follow-up opened by row 153 (`4672947`), where the kittest
fonts gate became tolerant of absent TTFs and snapshot pixel comparison skipped loudly.

## What landed

- `assets/fonts/Inter-Regular.ttf`, `assets/fonts/JetBrainsMono-Regular.ttf` committed
  (both OFL-1.1; license texts alongside as `OFL-Inter.txt`, `OFL-JetBrainsMono.txt`).
  `umber_gui::fonts` already treats in-tree `assets/fonts/` as its default
  (`include_bytes!` under the embed feature, manifest-dir path otherwise), so the repo
  is now self-contained for tests and CI with no `UMBER_FONTS_DIR`.
- All 7 kittest snapshot baselines re-recorded under the realized fonts
  (`UPDATE_SNAPSHOTS=1 cargo test -p umber-app --test panel_snapshot`), verified by a
  second clean run: 8/8 pass with real pixel comparison. `.old`/`.diff` residue cleaned.
- Harness fixes required to settle with real fonts:
  - kittest 0.36's default `max_steps` is **4**; once Inter realizes, the font atlas
    rebuilds over several frames and layout keeps requesting repaints, so `toolbar_snapshot`
    never settled. `tests/support::builder` now sets `.with_max_steps(64)`.
  - `fonts_gate` steps 5 frames after `set_fonts` instead of 1 (metrics converge).
  - `settle` now formats kittest's `ExceededMaxStepsError` (repaint causes) into the
    assert message instead of a bare "did not settle".
- Vision check on the re-recorded `toolbar.png`: "New" label is real Latin glyphs
  (no tofu), tool slots paint as stroked vector shapes, orange active-state underline
  intact.

## Battery

- `cargo fmt --check` clean; `cargo clippy -p umber-app --all-targets -- -D warnings` clean.
- `cargo test -p umber-app`: all suites green (218 + 677 + 1 + 3 + 12 + 8 + 10 + 2),
  including 8/8 panel_snapshot with fonts comparing.

## Notes

- Desktop pack deferred per Bessa 2026-11-05: checks 1-5 stay owed, headless lane only.
- Next wave: queue row 154 (CommandSpec unification).
