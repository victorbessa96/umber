# Kill brief: 4.3 procedural history — graph topology edits as undoable commands

- Lock: `razul-procedural-history` over:
  - `crates/umber-core/src/journal.rs`
  - `crates/umber-core/src/undo.rs`
  - `crates/umber-app/src/history_panel.rs`
  - `crates/umber-graph/` (whole crate; register the lock as the dir path)
- Implements wave-plan row 4.3 (docs/loop/waves/2026-11-09-wave-plan.md). L6 (unified timeline) landed 2026-10-27: the journal already interleaves layers + strokes + graph by op_id, and `JournalOp` already has `graph_target`/`graph_undo`/`graph_redo` hooks (journal.rs:370,409-413). This kill makes topology EDITS (node add/remove, edge wire/unwire, param set on a node) first-class undoable commands in that journal, surfaced in the History panel's third arm.

## Measured state

- Read journal.rs (`JournalOp`, `HistoryKind`, the graph_* hooks), undo.rs, history_panel.rs, and the umber-graph crate's command/topology API before designing. Verify what "graph op in the timeline" already records today (probably only paint-graph strokes, not node-graph edits).

## Build

1. A `GraphCommand` (or extend the existing graph-op path) covering at minimum: AddNode { node_def, pos }, RemoveNode { id + removed edges }, WireEdge { from, to }, UnwireEdge { from, to }, SetNodeParam { id, key, value }. Each must be reversible: capture the inverse data at commit time (removed node's full definition + its edges for RemoveNode).
2. Journal integration: these ops append to the same journal as layer/stroke ops with op_id ordering, appear in the History panel with a proper label (HistoryKind::label + crash_label), and undo/redo through the existing graph_undo/graph_redo hooks (or extend them) so undoing a WireEdge restores the pre-edge graph state exactly.
3. Tests: add→wire→unwire→remove→undo-all→redo-all round-trip restores the exact graph; journal order interleaves correctly with a layer op; History panel lists the topology ops with labels. Deterministic, pure-CPU.
4. History panel: surface the third arm (graph ops) with labels + jump targets if not already visible.

## Rules

- Edit only the locked surface. No .ftl changes unless a new label needs both catalogs — if so, add to both en-US and pt-BR in the same kill and lock the two .ftl files too (re-add lock with the full list).
- No commit/push. Verify: `cargo test -p umber-core -p umber-graph` and `cargo clippy -p umber-core -p umber-graph -p umber-app --all-targets -- -D warnings`, rustfmt --check on touched .rs. All green before finishing.
- Keep diffs minimal; existing style; doc comments on new public items.

## Dragon verification

Full battery on the three crates + scorecard/layers + commit via dcommit with EVERY payload path locked + safe-push.
