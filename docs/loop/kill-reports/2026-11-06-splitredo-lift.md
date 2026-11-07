# Kill report — split_redo RedoPending lift (queue row 254)

- Date: 2026-11-06 ~00:2x -03. Session: local-1625-rz6f2a (Razul).
  Kill-lock: `razul-splitredo-lift` (crates/umber-core/src/undo.rs,
  crates/umber-paint/src/retouch.rs, STATE.md, this report).
- Unblocks: the ROADMAP split-redo debt's last refusal. `peek_redo`/`redo_len`
  landed in row 252 (`03181ec`); this kill adds the mutation-side pair.

## The change

- `UndoStack::take_redo()` / `restore_redo(entries)` (undo.rs): drain the
  redo stack oldest-first WITHOUT eviction or budget release, and put it
  back in order, so the next `redo()` re-applies the same entry. Pin
  `take_redo_restore_redo_roundtrip_preserves_order_and_budget` (order, no
  double-count, push-in-between sees an empty redo).
- `split_redo` (retouch.rs): the old `RedoPending` refusal is gone. The
  split now `take_redo()`s after its opening `redo()`, pushes the parts,
  `restore_redo`s the preserved entries BELOW the split parts (the undo
  loop then lands parts 2..=n on top, so redo order is part2..partn, then
  the older undone entries), and on the refusal paths restores before the
  final `undo()` so the redone merged entry stays the next redo.
  `SplitOutcome::RedoPending` is retained (doc updated) so the
  sibling-locked History-panel match arms keep compiling; it is no longer
  returned.
- Rewrote the refusal test: a merged entry with another entry undone above
  it now splits (`Split { parts: 4 }`), leaves `redo_len() == 1 + 3` (the
  split consumes the merged redo itself; the other older redo plus 3 parts
  remain), and fully redoing the preserved stack reproduces the pre-undo
  image byte-exactly.

## Verification

- /tmp/umber-splitlift-battery4.log: clippy -p umber-core -p umber-paint
  --all-targets -D warnings 0; core lib 187/0 (bundled:: skipped — that
  failure exists only in the worktree because the sibling's dirty lib.rs is
  uncommitted; the commit tree is clean, proven by safe-push gates); paint
  194/0 incl. all three split_redo tests.

## Records note

Queue row 254 + COORDINATION line DEFERRED: docs/loop/queue.md and
.dragon/COORDINATION-next-wave.md remain held by the live sibling lock
`razul-bundled-assets`.

## Next

- merge-below-top design (retouch.rs:34 debt) now has both primitives it
  needs (peek_redo + take_redo/restore_redo).
- Row 242 History-panel wiring + installers gap #2: sibling-locked.
- v0.1.0 tag: partner-gated.
