# Kill report: C2-7 one undo timeline (strokes + layer ops), the panel half

Date: 2026-10-15. Author: dragon. Lock: `c2-7-one-timeline` over
`crates/umber-app/src/history_panel.rs` and this report. Source: audit C2-7
and the history module's own named follow-up ("coordinated travel").

## The gap

Layer ops and brush strokes live on two journals. Ctrl+Z and the History
panel's Undo stepped only the layer journal. A painter who painted a
stroke and pressed Ctrl+Z got their last layer op undone, not the
stroke; strokes needed their own button.

## Built (`history_panel.rs`)

- **`unified_undo_target(entries, layer_len, stroke_len)`**: the journal
  ONE undo steps is the one holding the NEWEST applied row by `op_id`.
  The log already records the interleaving of both journals in one
  sequence; until now nothing read it.
- **`unified_redo_target`**: the journal whose next undone step (`depth
  == len + 1`) is the OLDEST, so a run of undos replays in exact reverse.
- **`unified_undo` / `unified_redo(doc, paint)`**: drain pending stroke
  commits FIRST, so a stroke painted this frame is on the timeline before
  the choice, then step the chosen journal. Without a paint session, stroke
  rows are not candidates. Context rows (bakes, exports) are never
  positions.
- **The History panel's Undo/Redo buttons** now step the unified timeline,
  and their enabled state comes from the same targets.

## Tests (4)

- The pure rule: an interleaved L1 S1 L2 S2 log undoes as Stroke, Layer,
  Stroke, Layer, and redoes as the exact reverse.
- No paint session skips stroke rows; context rows are skipped; a merged
  stroke (shared depth) is one step either way.
- A real `Document`: undo/redo to both ends.
- **GPU, through a real `PaintState`:** layer op, stroke, layer op. Three
  unified undos take Second, then the STROKE, then First; three redos
  replay them.

  **Mutation check:** routing the stroke step to the layer journal fails
  it.

Gate 1: umber-app 496. fmt and clippy clean.

## Not done: one call site, lock-blocked

Ctrl+Z / Ctrl+Shift+Z are dispatched in `main.rs` (`ShellAction::Undo =>
self.state.doc.undo()`), which is under the sibling's lock. The swap is
two lines:

```rust
ShellAction::Undo => {
    let _ = history_panel::unified_undo(&mut self.state.doc, self.state.paint.as_deref_mut());
}
ShellAction::Redo => {
    let _ = history_panel::unified_redo(&mut self.state.doc, self.state.paint.as_deref_mut());
}
```

Until it lands, the panel buttons are unified and the shortcuts still
step layers only.

## Deferred

- Click-to-time-travel on a row still drives one journal. Coordinated
  travel to a row means unified steps until both journals sit at the
  row's moment. That is now a loop over `unified_undo`/`redo`, and the
  next kill.
- Mask strokes and retouch tools ride the stroke journal already, so they
  are on the timeline. Smudge/clone reuse the same commit path; verified
  by reading, not by a dedicated test.
- An opacity-drag coalesce (C2-8's note) is separate: it changes how many
  layer rows a drag records, not which journal undo picks.
