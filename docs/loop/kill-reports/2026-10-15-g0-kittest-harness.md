# Kill report: g0-kittest-harness (the shared kittest support module)

Date: 2026-10-15. Author: dragon. Lock: `g0-kittest-harness` over
`crates/umber-app/tests/{support/mod.rs, panel_kittest.rs}` and this
report. Plan: `docs/specs/gui-plan-2026-10-15.md` row `g0-kittest-harness`
(PC:#19).

## Built

- `tests/support/mod.rs` (`mod support;` from any test file):
  - `ensure_i18n` runs once per binary.
  - `builder<S>()` installs i18n and applies a fixed 1/60 s step
    (`with_step_dt`) and a fixed 480x640 size, so frame counts and
    layout mean the same on every host.
  - `harness` / `harness_state` build through it.
  - `settle` runs to idle and fails loudly if the UI never settles.
  - AccessKit helpers: `click_label`, `has_label`.
- `tests/panel_kittest.rs`, the first consumer, with three behavior
  tests on the Display panel:
  - Reset restores the default transform after an edit.
  - Reset reports DISABLED through AccessKit at the default and enabled
    after an edit.
  - Labels render translated, never as raw keys.

## Verification

- 3/3 pass on the build host.
- **Mutation check:** forcing the Reset button always enabled
  (`!settings.is_identity()` → `true`) fails
  `display_reset_is_disabled_at_the_default` and only that test. The
  test can fail, which the earlier click-based draft could not: a reset
  of the default is the default either way.
- Gate 1 (umber-app): fmt, clippy, 458 tests.

## Judgment calls

- **`panel_snapshot.rs` is not migrated onto the module.** The sibling
  session's live `g1-axis-gizmo` lock holds that file. Its two snapshots
  keep their own `ensure_i18n` until the lock frees. Moving them over is
  a three-line follow-up, and the plan row's "the 2 existing snapshots
  still pass through it" lands then.
- **No fonts gate yet.** The plan row lists one (skip frames until the
  named font families exist). `umber_gui` still loads its fonts at
  runtime from `assets/fonts/`, and the stock fonts render every test
  label, so a gate would wait on nothing today. It belongs with the
  `embedded-fonts` feature.
- **No theme sweep helper.** Its consumer is the G7 accesskit and
  density sweep, so it ships with that kill.

## Deferred

- Migrating `panel_snapshot.rs` (after the g1-axis-gizmo lock).
- The fonts gate and the theme sweep, with their first consumers.
