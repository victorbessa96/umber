# Kill report — record_stroke below-top splice wiring (queue row 257)

- **Date / session:** 2026-11-07, local-1625-rz6f2a, kill-lock `razul-recordstroke-below`.
- **Commit:** `bbdc08b` (pushed via safe-push proc_95bba5ac8a64, gates: clippy + test on umber-core/umber-paint and reverse deps).
- **Change:**
  - `crates/umber-core/src/undo.rs`: new read-only accessor `UndoStack::peek_undo_at(k)` — the entry `k` below the undo top (`k = 0` is `peek_undo`), for the paint crate's merge scan.
  - `crates/umber-paint/src/retouch.rs` `record_stroke`: after the unchanged top fast path, a bounded scan (`MERGE_WINDOW = 8`) below the top splices the stroke into an older entry through `merge_below_top(k, writes, |t| Some(merge_stroke(t.clone(), tiles)))` (row 256 primitive). Guards: (1) redo stack must be empty — a fresh edit otherwise clears redo via the plain push, and that semantic is preserved; (2) the stroke must intersect NO entry above the merge target (same tile/channel + rect intersection) — redoing those entries would clobber the merged after over the overlap, so such strokes stack instead. Falls through to the plain `stroke_command` push otherwise.
- **Pins (both byte-exact undo/redo roundtrips):**
  - `record_stroke_merges_below_top_when_the_stroke_skips_the_top_entry` — disjoint fills stack (len 2), a third stroke overlapping only the older entry splices (len stays 2), full undo returns to the base image, full redo is byte-exact.
  - `record_stroke_stacks_when_the_stroke_touches_an_entry_above_the_target` — a stroke intersecting both entries stacks (len 3), guard refused the splice; roundtrip byte-exact.
- **Battery:** `/tmp/umber-below-battery4.log` — fmt OK; `cargo clippy -p umber-core -p umber-paint --all-targets -- -D warnings` 0; core lib 189/0 (`--skip bundled::`, sibling-dirty worktree); paint `--all-targets` 196/0. Iteration history: battery/2 died on a redundant-closure lint (fixed with `t.clone()` in the build closure); battery3 died on one wrong intermediate assertion in the new pin (the splice changes what "undo the top entry" restores — assertion removed, the full roundtrip pins byte-exactness instead) plus an unused-var warning.
- **Docs:** ROADMAP merge-below-top debt marked WIRED (`bbdc08b`, row 257); STATE.md row-257 entry. Queue row + COORDINATION line deferred with the sibling lock (docs/loop/queue.md held).
- **Remaining on this thread:** the History-panel "split" affordance wiring (row 242 follow-up) — blocked on document.rs/main.rs under the sibling's lock.
