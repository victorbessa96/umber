# Kill report — History-panel split-redo wiring (queue row 262)

- **Date / session:** 2026-11-07, local-1625-rz6f2a, kill-lock `razul-splitredo-panel` (app files + both i18n catalogs + the two History-panel snapshots).
- **Commit:** `22e0efbf` (pushed via safe-push; clippy + full test gates on umber-app and reverse deps).
- **Closes:** the row 242 follow-up "App History-panel wiring" for split-redo (engine landed `850b839`, RedoPending lift `1347e13`). The sibling's `razul-bundled-assets` lock freed this morning, opening document.rs/main.rs.
- **Change:**
  - New `ShellAction::SplitRedo` (`edit.split_redo`): toolbar id/from_id/label_key round-trip (pinned by `ids_are_unique_dotted_and_round_trip`), `ALL_ACTIONS` + the exhaustive guard + the 44-count pin, `availability` names `action.unavailable-nothing-to-redo` when there is no paint session or no redo, reference text, tooltip key.
  - `commands.rs` dispatch arm: calls `PaintState::split_redo_stroke()` and posts the outcome as a notice — `history.split-done` (with the parts count) on `Split`, `history.split-refused` otherwise, `action.unavailable-no-mesh` with no paint session.
  - `history_panel.rs`: a "⇥ Split redo" button in the controls row next to Undo/Redo, enabled when `can_redo_stroke()`; the click dispatches after the group (collect-then-act pattern) via `split_redo_stroke()`.
  - i18n: `history.split-redo` / `history.split-done` / `history.split-refused` + `cmd-edit-split-redo` in BOTH catalogs (parity-gated).
  - The two History-panel snapshot goldens regenerated (`UPDATE_SNAPSHOTS=1`) — the new button is the only pixel diff.
- **Battery:** `/tmp/umber-splitpanel-battery7.log` — clippy `-D warnings` clean; `cargo test -p umber-app --all-targets`: 1048 passed / 0 failed across all 10 suites (lib, bin, 8 integration incl. panel_snapshot 8/8). Iteration history: E0308 twice (an early `return` and a missing `;` on the `post()`-returning match — fixed), `every_action_is_listed` count pin 43→44, then the two panel snapshots.
- **Not done:** no default keyboard binding (palette + panel button only); the split does not add session-log rows (the merged stroke's row already exists; the journal itself is the stroke history).
