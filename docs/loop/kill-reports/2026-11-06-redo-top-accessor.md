# Kill report — `UndoStack::peek_redo` / `redo_len` (redo-top accessor, queue row 252)

- Date: 2026-11-06 ~23:1x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-redo-top-accessor` (crates/umber-core/src/undo.rs, STATE.md, this
  report).
- What was open: retouch.rs:658's named follow-up — "lifting
  [split_redo's RedoPending refusal] needs a redo-top accessor in
  `umber_core::undo`" — and ROADMAP's merge-below-top debt needs the same
  primitive (a below-top merge is only safe to reason about with the redo
  stack visible). `peek_undo` existed; the redo side had none.

## The change

- `UndoStack::peek_redo(&self) -> Option<&C>` (the command `redo()` would
  re-apply next, most-recently-undone first, no stack mutation) and
  `redo_len(&self) -> usize`. Additive only; no existing behavior touched.
- Pin `peek_redo_reads_the_next_redo_without_touching_the_stacks`: empty
  stack reads None/0, after undos the redo top is the most recently undone
  entry, counts track, and the accessor never applies anything
  (doc unchanged, undo side drained check).

## Verification

- Battery (flock, nice 19, -j3, /tmp/umber-redoconfig-battery.log): fmt OK,
  clippy -p umber-core --all-targets -D warnings 0, cargo test -p umber-core
  --all-targets all suites ok 0 failed, pin included.

## Records note

Queue row 252 + COORDINATION line DEFERRED: docs/loop/queue.md and
.dragon/COORDINATION-next-wave.md remain held by the LIVE sibling lock
`razul-bundled-assets` (three processes with cwd in ~/Projects/umber at
census; its dirty files untouched).

## Not done / next

- With the accessor landed, the split_redo `RedoPending` lift and the
  merge-below-top design are now unblocked primitives-wise (paint lane,
  free) — next design pass can use them.
- Row 242 History wiring + installers gap #2 stay sibling-locked;
  v0.1.0 tag partner-gated.
