# Kill report: panel_snapshot.rs migrated onto tests/support (g0-kittest-harness tail)

Date: 2026-11-01 ~02:0x (dragon local). Lock: `g0-kittest-migration`.
Envelope: code only, no cargo/tests (host envelope); rustfmt sanctioned.

## Scope

GUI plan G0 `g0-kittest-harness` open parenthetical: "panel_snapshot
migration + fonts gate open". This kill closes the migration half:
`crates/umber-app/tests/panel_snapshot.rs` was the last kittest file still
carrying its own private `ensure_i18n()` and raw `egui_kittest::Harness`
builders instead of the shared module every other kittest file uses.

## What landed (`e8b4b64`)

- `mod support;` declared (same pattern as panel_kittest.rs).
- Private `ensure_i18n()` fn deleted; the 6 per-test calls removed —
  `support::builder()` / `support::harness()` install the catalogs.
- All 6 harness constructions routed through `support::builder::<()>()`
  (custom `.with_size(...)` still overrides the house SIZE) or
  `support::harness(...)`; `harness.run_ok()` -> `support::settle(&mut
  harness)` (same step, with a loud failure message on an unsettled UI).
- 14 `support::` refs; no `Harness::`/`use egui_kittest` code refs left.

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on the migrated file.
- NOT run: the snapshot suite itself (no cargo per envelope) — the 6
  snapshot tests + the axis-gizmo click-chain test run at the next
  battery-capable wave. Note: `support::builder` fixes STEP_DT + SIZE
  where the old raw builders left them at kittest defaults, so snapshot
  PNGs may shift on the next UPDATE_SNAPSHOTS run; baselines regenerate
  on the build host before the wave battery per the A2 note in the file.

## Still open on this row

- The fonts gate (skip frames until the named font families exist) —
  the remaining parenthetical; lands when a font-bearing snapshot needs it.
