# Kill report: 2026-10-19 graph-history (wave-procedural-history, kill 1)

Authorship: dragon (designed + authored; cross-reviewed by the opencode
claw on nemotron-3-ultra-free — findings below). Kill-lock:
`graph-history` — `crates/umber-app/src/graph_panel.rs`,
`crates/umber-app/src/graph_panel/history.rs` (NEW),
`assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`, this report.
Design source: `docs/ROADMAP.md` wave 9 row `wave-procedural-history`
("the last non-undoable edit class") — with the row's premise
CORRECTED: the roadmap claimed "the graph panel's param edits ride
Document::run today"; verified FALSE (no journal reference anywhere in
the panel pre-kill) — BOTH param and topology edits were unjournaled,
so the command family covers both.

## Built

- NEW `crates/umber-app/src/graph_panel/history.rs` (child module,
  the `brush_panel/` registration pattern — no main.rs/lib.rs touch):
  `GraphCommand` — AddNode, RemoveNode, Connect (replace semantics,
  captures the replaced source), AddEdge (the list editor's raw
  insert, distinct from Connect by dispatch-site reachability),
  RemoveEdge, SetParam, SetOutput — each `impl Command` with
  `Doc = GraphPanel`, capture-on-apply (the `LayerCommand` precedent),
  exact-inverse apply/revert.
- `graph_panel.rs`: a `history: UndoStack<GraphCommand>` field
  (HISTORY_MAX 256, the `Document` precedent); `run_graph_command` /
  `undo_graph` / `redo_graph` / `can_undo_graph` / `can_redo_graph`
  (the `mem::replace` borrow split — the journal lives ON the panel
  the commands mutate); the eight edit methods
  (set_param, add_node, add_node_at, connect, remove_edge,
  remove_node, add_edge, set_output_node) now dispatch commands with
  their refusal conditions and return contracts unchanged;
  `load_graph` clears the journal (a loaded graph is a state
  replacement); `show()` gains `history_row` — panel-local Undo/Redo
  buttons (`add_enabled`, so they stay discoverable disabled).
- Both catalogs: `graph.undo` / `graph.redo` (en: Undo/Redo, pt:
  Desfazer/Refazer) under the `graph` group.
- 13 tests in `history.rs` (bin target): add undo/redo same-id,
  first-add output adoption undone, add_node_at position through the
  journal, connect-replacement restore, add_edge undo/redo, remove_node
  full restore (node+edges+output), remove-node deselection undone,
  set_param old-value restore, param-added removal on undo, set_output
  restore, new-edit-clears-redo, load_graph clears the journal, and a
  kittest driving the Undo button through `Harness` (the trust-button
  precedent's shape).

## Judgment calls

- **Panel-local journal (kill 1) before the unified timeline (kill
  2).** The unified Ctrl+Z + History-panel wiring needs `main.rs` (the
  key dispatch) and `history_panel.rs` — both outside this kill's
  surface (main.rs held by the eyedropper session all day). The
  panel's own buttons make the journal live NOW without touching held
  files.
- **AddEdge is its own command.** The list view's edge editor calls
  `add_edge` directly (graph_panel.rs:1141 pre-kill), so the raw
  insert is user-reachable and must journal; `connect`'s replace
  semantics are a different command. The pre-kill push behavior of
  `add_edge` (no dedup) is preserved exactly.
- **Node MOVES stay unjournaled.** The design doc's own
  editor-only-data rule: `Node::canvas` dirties nothing; `add_node_at`'
  s placement rides its AddNode command, but `set_node_canvas` on an
  existing node is view state. Undo-of-moves is a named deferral.
- **`next_id` never rewinds.** Ids stay unique across undo/redo cycles
  so a journal entry can never point at a re-minted id (documented
  boundary; the cross-review raised it, kept deliberately).
- **SetParam's revert takes its captured old; apply re-captures.**
  Sound for every `UndoStack` op shape (push/undo/redo/jump — jump is
  sequential undo/redo); a standalone double-revert is forbidden by
  the module contract and called out for the follow-up kill.
- **RemoveNode captures only what the removal moves** (output when it
  pointed at the node, selection likewise); everything else stays
  untouched by the revert.

## Cross-review (opencode/nemotron-3-ultra-free, adversarial)

- **BLOCKER #1 — AddNode redo minted a fresh id** instead of reusing
  the captured one (breaking the exact-inverse contract; the suite's
  own same-id test would have caught it at the battery, red). FIXED
  pre-battery: apply reuses the captured id on re-apply — the
  `LayerCommand::Add` precedent followed for real this time.
- MAJOR #2 (next_id not restored) — kept as the documented boundary
  above. MAJOR #3 (RemoveNode's defensive early-return can record a
  no-op entry) — matches the `Command` trait's own setter-swallow
  precedent ("the setters swallow unknown ids"); unreachable via the
  public dispatch single-threaded. MAJOR #4 (SetParam take vs a
  hypothetical double-revert) — the module contract now forbids the
  shape; noted for kill 2.
- MINOR: the `mem::replace` throwaway journal (no heap until push;
  the standard borrow split); the kittest's label depends on i18n
  (keys verified in both locales). NITs: the `debug_assert_ne!` in
  Connect::apply kept (documents the invariant); the unreachable!'s
  airtightness analyzed by the reviewer (push applies before storing;
  peek reads the back; eviction pops the front).
- The reviewer's compile-risk hunt: zero compile errors anticipated;
  every symbol grep-verified.

## Unverified

- EVERYTHING runtime: the host wedged (freeze #4, 18:14-18:20 window)
  before this kill's battery could run. Tests UNRUN; the kittest
  UNRUN; clippy UNRUN. The recovery battery (umber-app, default
  features, NO `--features gpu` inside any recovery window) is the
  gate.

## Deferred

- Kill 2 — the unified timeline: `unified_undo`/`unified_redo`
  consult the graph journal (main.rs:2137/2140 dispatch +
  history_panel.rs), History rows (`HistoryKind::from_graph_command`),
  jump-to-time-travel across three journals. main.rs + history_panel
  surfaces.
- Node-move undo (editor-only data today).
- Param-edit coalescing via `take_top_unreverted` (the drag-frame
  granularity question).
- `memory_bytes` / `on_evict` when a graph command ever pins GPU
  state.

## Process note

- PROTOCOL MISS, banked: the lock was added AFTER authoring began (the
  check+add should have been this kill's first tool call). No
  collision occurred — the surface was exclusively this session's by
  activity and no sibling wrote the graph files — but the miss is
  recorded here rather than buried.
- Freeze #4 parked the battery: this kill sits dirty in the shared
  tree with the four claw kills (A1/P1/G1/S1, all statically verified,
  reports complete) + the sibling's eyedropper. Recovery sequence per
  the coordination note; this kill rides the W-C3a push. NO
  `--features gpu` in the recovery window (the 94469e8 wgpu deadlock
  class).
- rustfmt clean locally (edition 2021) on both files.

## Addendum (2026-10-20 23:5x, local-4d2e8a71): the battery catch, fixed forward

The first real compile (the local degraded-mode battery) caught nine
E0624s with one root cause: `run_graph_command` and `history_row`
were private methods in the child module `graph_panel/history.rs`
while their callers live in the parent `graph_panel.rs` — module
privacy rustfmt and review passed and the type system did not.
Fixed as `pub(crate)` (bin-only module, the lib surface untouched);
manifest re-pinned with this note's reason. Third catch of the same
class tonight (usdc.rs Ok(()), paint_state.rs cfg-on-expression,
this): static gates parse, only a compile proves a file.
