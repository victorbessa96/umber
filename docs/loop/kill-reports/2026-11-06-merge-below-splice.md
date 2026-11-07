# Kill report — merge-below-top splice primitive (queue row 256)

- Date: 2026-11-06 ~01:1x -03. Session: local-1625-rz6f2a (Razul).
  Kill-lock: `razul-merge-below-splice` (crates/umber-core/src/undo.rs,
  docs/ROADMAP.md, STATE.md, this report).
- The merge-below-top debt (retouch.rs:34) is now unblocked at the
  primitive level: a below-top rewrite no longer needs to invalidate the
  entries above or the redo stack.

## The change (umber-core only; record_stroke wiring is the named next kill)

- `UndoStack::push_unapplied(command)`: like `push` (budget accounting,
  redo clearing, eviction) but no `apply` — for re-seating entries whose
  effect is already in the document.
- `UndoStack::merge_below_top(k, doc, build)`: rewrites the entry `k`
  below the undo top by folding `build(&target)` into it. The entries
  above are taken unreverted (`take_top_unreverted`) and re-seated with
  `push_unapplied`; the merged entry goes through `push` (its apply lands
  the doc where it already is, same as the top-merge path); any pending
  redo is preserved via `take_redo`/`restore_redo`. Out-of-range `k` or a
  declined `build` (`None`) re-seats everything and returns false with
  the journal untouched.
- Pins: `merge_below_top_rewrites_the_target_and_keeps_above_entries_and_redo`
  (splice at k=1 with a pending redo: target replaced, above re-seated,
  redo preserved, full undo/redo roundtrip consistent) and
  `merge_below_top_declined_or_out_of_range_leaves_the_journal_untouched`
  (both refusal paths: stacks, budget, and doc unchanged).
- ROADMAP debt line updated: splice landed, record_stroke wiring next.

## Verification

- /tmp/umber-mergebelow-battery4.log: clippy -p umber-core --all-targets
  -D warnings 0; core 189/0 with both pins (bundled:: skipped — its failure
  exists only in the worktree against the sibling's dirty lib.rs; the
  commit tree is clean, proven by safe-push gates).

## Records note

Queue row 256 + COORDINATION line DEFERRED (sibling lock
`razul-bundled-assets` still live at census).

## Next

- Wire `record_stroke` (retouch.rs:613) to try below-top entries through
  `merge_below_top` (bounded depth window, same can_merge eligibility),
  keeping the top-merge fast path.
- Row 242 History wiring + installers gap #2: sibling-locked. v0.1.0 tag:
  partner-gated.
