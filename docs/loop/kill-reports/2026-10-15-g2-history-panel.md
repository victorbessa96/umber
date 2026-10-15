# Kill report — g2-history-panel (GUI plan G2)

Date: 2026-10-15. Author: dragon. Lock: `g2-history-panel` over
`crates/umber-app/src/history_panel.rs`, `tests/panel_snapshot.rs`,
`tests/snapshots/` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g2-history-panel`. Source:
Blender A10.1 ("newest on top and a dot on the current state; clicking any
entry jumps to it; the first new edit truncates the future").

## What already existed (not rebuilt)

Clicking any row jumps the journal to it (layer rows through
`Document::jump_to_depth`, stroke rows through
`PaintState::jump_to_stroke_depth`), the current row is selected, and a
new edit truncates the redo branch (the journal's own rule). That covered
most of A10.1. The gaps were order and the future rows.

## Built

- **`RowState { Current, Past, Future, Context }`** and the pure
  `row_state(entry, layer_len, stroke_len)`:
  - a row is Future once its journal is shorter than the row's depth (its
    edit undone), Current at exactly its depth, else Past;
  - stroke rows read as Context with no paint session (nothing to drive);
    bakes and exports are always Context.
- **The panel:**
  - **newest on top** (the log reads as a stack, the latest edit nearest
    the controls);
  - **a ● dot** on the current row;
  - **undone rows greyed and italic.** These are what the next edit will
    discard, so the user sees what a click there would bring back and
    what a new edit would lose.
- Test `row_state_marks_current_past_future_and_context`: live state;
  layer undo making a row Future; stroke undo; no-session context.

## Judgment calls

1. **No per-entry snapshot thumbnails.** The plan row lists 56 px
   snapshots per entry (photocraft). Every entry would need a stored
   composite thumbnail (memory per edit) captured at commit time from the
   paint session. That is a real feature with its own memory budget,
   re-queued as `g2-history-thumbnails`; the layer thumbnails
   (`c9c4b43`) are the reusable half.
2. **Order flip only in the view.** `SessionHistory` stays append-only
   (oldest first): its tests and the drain logic rely on that order, and
   the newest-on-top rule is a display decision.

## Unverified

- No snapshot of the panel with rows (the existing snapshot suite covers
  the Display and PSD panels; `history_panel` is a bin-only module, the
  same blocker the slider sweep hit, which the G0 kittest-harness kill
  owns).
