# Kill report: the lock-blocked wiring, landed once `g1-axis-gizmo` freed

Date: 2026-10-15. Author: dragon. Lock: `wire-unlocked` over
`crates/umber-gui/src/{lib,widgets/mod}.rs`, `crates/umber-app/src/main.rs`
and this report. The sibling session pushed `97b26d4` and released
`g1-axis-gizmo`. Every block below was written in an earlier kill report
as "waits on the lock".

## Landed

1. **The curve editor and the colour picker are registered**
   (`widgets/mod.rs` + the `lib.rs` re-exports: `Curve`, `CurveEdit`,
   `CurveError`, `ColorPicker`, `ColorState`). Their test suites now run
   in the battery: umber-gui 72 → 91, 10 curve + 9 picker tests. Both
   were ad-hoc verified before; now they are suite-verified.
2. **The crash reporter is live (C2-2):** `mod crash;` and
   `crash::install(gpu_startup::state_dir())` are the first line of
   `main()`. On launch, `pending_crash_notices()` posts one sticky Error
   notice per unseen report, with its path and a note that it holds no
   document data, then acknowledges it (renamed `.seen`), so each report
   is offered exactly once. Its 5 tests now run in the suite
   (umber-app 505 → 510).
3. **Ctrl+Z / Ctrl+Shift+Z step the one timeline (C2-7)** through
   `history_panel::unified_undo` / `unified_redo`. A stroke is undone by
   the same key as a layer op, newest first.
4. **Duplicate with content (C2-8):** the Layers panel's
   `outcome.duplicated` calls `PaintState::copy_owner_planes`, so a
   duplicated paint layer carries its pixels. A budget refusal is logged,
   and the copy keeps its settings.

## Verified

Gate 1, in a fresh clone of the pushed `d34d36a` plus these three files:
umber-gui 91, umber-app 510, fmt and clippy clean. The remote file
hashes matched local before the commit.

## Unverified

- The crash notice's first-frame path and the Ctrl+Z dispatch are shell
  glue over functions with their own tests (`crash::pending` /
  `acknowledge`, `unified_undo`). No kittest drives `main.rs`'s `App`.
- The notice text is untranslated until the catalogs take a crash key.

## Still open

- The colour picker's three call sites (brush colour and two fill
  colours) need a swatch-button → popup wrapper. Next kill.
- C2-1 quit guard, C2-3 pixel autosave, C2-4 silent Ctrl+S: main.rs is
  free now.
