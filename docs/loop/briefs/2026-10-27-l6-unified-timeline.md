# L6 — the third journal joins the unified timeline (kill brief, 2026-10-27)

Kill: `l6-unified-timeline`. Wave: the fresh charter cycle (Bessa's launch
word, 2026-10-27). Arc: kill 2 of the procedural-history arc (kill 1 =
`2a84fb8`, the graph journal itself).

You are a code claw authoring ONE kill in `~/Projects/umber`. The dragon
(Razul) registered the kill-lock BEFORE this dispatch and holds it; you NEVER
touch locks, git, or builds. Read `CLAUDE.md` at the repo root first — every
hard rule and trap there binds you. This brief adds the kill's specific
contract.

## The window (the ONLY files you may edit — all lock-held)

1. `crates/umber-app/src/history_panel.rs`
2. `crates/umber-app/src/graph_panel.rs`
3. `crates/umber-app/src/graph_panel/history.rs`
4. `crates/umber-app/src/main.rs`
5. `assets/i18n/en-US.ftl`
6. `assets/i18n/pt-BR.ftl`

The seventh locked path, `docs/loop/kill-reports/2026-10-27-l6-unified-timeline.md`,
is the DRAGON's report — do not create or edit it. If you believe the design
needs a file outside this window, STOP and say so in your report instead of
creating it. New modules are forbidden for the same reason: the window is the
surface.

## The problem

The History window's unified timeline (`history_panel.rs`) currently
consults exactly two journals: the document/layer journal (`Document`,
umber-core) and the paint stroke journal (`PaintState::retouch_history`,
umber-paint, threaded through `StrokeCommit`). The graph panel carries its
own procedural-history journal — `graph_panel/history.rs` (651 lines,
panel-local `UndoStack<GraphCommand>`, landed `2a84fb8`) with its own local
Undo/Redo buttons. A user who moves a graph node and then hits the unified
History window sees nothing: the graph journal is invisible to the timeline,
and unified undo skips graph work entirely. L6 merges it: the graph journal
becomes the timeline's third arm, interleaved with layers and strokes by
op_id, jump-to-any-moment drives all three journals.

## The design (frozen by the dragon — follow it; where the tree contradicts
a detail below, the tree wins and you report the deviation)

### The lib/bin split is the load-bearing constraint

`history_panel` compiles in BOTH targets (`crates/umber-app/src/lib.rs:60`
`pub mod history_panel;` AND `main.rs:135 mod history_panel;`), while
`graph_panel` is BIN-ONLY (`main.rs:134 mod graph_panel;`). So nothing in
`history_panel.rs` may name `GraphPanel`, `GraphCommand`, or any
`graph_panel` item — a lib-side reference to a bin-only module is a compile
error in the lib target, which is also what the integration tests in
`crates/umber-app/tests/` link against.

The bridge is a small trait, defined in `history_panel.rs`:

```rust
/// The graph journal as the unified timeline sees it — the bin target's
/// `GraphPanel` implements this; the lib cannot name the panel itself.
pub trait GraphJournal {
    /// Journal length at the current cursor, or None while no graph ops
    /// have been recorded (mirror the paint arm's Option semantics).
    fn graph_depth(&self) -> Option<usize>;
    fn graph_undo(&mut self) -> bool;
    fn graph_redo(&mut self) -> bool;
    /// Drive the journal to the given depth (the `jump_to_stroke_depth`
    /// loop's twin; read that fn before writing this one).
    fn graph_jump_to_depth(&mut self, depth: usize);
}
```

`impl GraphJournal for GraphPanel` lives in the BIN target (in
`graph_panel.rs`, beside the panel). Every lib-side fn that today threads
`Option<&mut PaintState>` gains the graph arm the same way — as
`Option<&mut dyn GraphJournal>` (check each call site's borrow: doc, paint,
graph are disjoint at both known sites, but re-verify at every site you
touch).

### The commit queue (the StrokeCommit pattern, mirrored)

Read `crates/umber-paint/src/paint_state.rs:1087-1094` first —
`StrokeCommit { pub cursor, pub depth }` with the documented semantics:
cursor = journal length BEFORE the op recorded (the dead-redo truncation
point), depth = length AFTER (the row's jump target). `Document::run`
records the same pair for layer commands. Mirror this exactly for graph
commands:

- `pub struct GraphCommit { pub kind: HistoryKind, pub cursor: usize, pub depth: usize }`
  lives in `history_panel.rs` (lib — the bin constructs it).
- `GraphPanel` (bin) queues a `GraphCommit` around every journal push in
  `graph_panel/history.rs`'s command path — the cursor/depth pair read off
  the panel's `UndoStack` at push time, `kind` from the mapping below.
- A load-graph epoch: `load_mtlx` routes through `load_graph`; a graph
  LOAD is one timeline row (not N), and a re-load after undo must not
  resurrect stale rows. The dragon verified the existing epoch bump covers
  the path — read `load_graph`'s journal handling and keep its semantics;
  if the queue needs an epoch field to suppress stale drains, add it to
  the queue bin-side (NOT to `GraphCommit`) and say so in your report.
- `main.rs`'s frame loop drains the panel's pending queue into
  `SessionHistory` each frame, exactly the way pending `StrokeCommit`s
  drain. Read that drain site first and mirror it; the drain is main-side
  (bin), so it may call `GraphPanel` directly — no trait needed there.

### HistoryKind and the mapping

`HistoryKind` (`history_panel.rs:107`) gains one variant per user-meaningful
graph command, derived from `GraphCommand` in `graph_panel/history.rs`
(read the enum; the dragon expects roughly seven: node add, node remove,
connect, disconnect, param set, and the load/reload class — derive the
REAL set from the tree, not this list). The mapping
`GraphCommand -> HistoryKind` lives BIN-SIDE (in `graph_panel.rs` beside
the panel, or `main.rs`) — history_panel cannot name `GraphCommand`, which
is why it is not a `From` impl there.

### The unified machinery (all in history_panel.rs)

- `HistoryEntry` (`:243`) gains `graph_depth` with the SAME type and
  Option-semantics as the existing per-journal depth fields (read the
  struct; mirror `stroke_depth` exactly). ALL construction sites are in
  this file — `:303`, `:318`, `:332`, `:766`, `:893`, `:943` — every one
  updates. Zero sites exist outside this file (dragon-verified tree-wide).
- `Journal` (`:455`) gains a `Graph` arm; the interleaving/targeting fns
  (`:476-497`), the undo/redo dispatch (`:512-526`), and the travel loops
  (`:916-933`) each gain the third arm. Study how `op_id` interleaving
  decides which journal owns the next undo BEFORE writing — the graph arm
  must compose with that ordering, not parallel it ad hoc.
- `unified_undo`/`unified_redo` signatures (see `main.rs:2255/:2258` call
  sites) gain the graph parameter; `history_ui` (`main.rs:1196` call site)
  renders graph rows like stroke rows (icon/kind/depth from the entry; the
  jump highlight pattern the panel already uses).

### i18n

Every new user-facing string goes through `tr()` with keys in BOTH
`assets/i18n/en-US.ftl` and `assets/i18n/pt-BR.ftl` (workspace root — see
CLAUDE.md rule 7 for the Fluent group/attribute convention). The new
HistoryKind labels need keys; the completeness test pins the pair
directionally, so a missing pt-BR key fails the battery. pt-BR translations:
keep them natural, e.g. "Adicionar nó" / "Conectar" / "Remover nó" /
"Definir parâmetro" — you get the shape.

### Tests (must be able to fail)

- Lib-side (inline `#[cfg(test)]` in `history_panel.rs`): a minimal fake
  `GraphJournal` (a test struct with a Vec-based depth) pins — the
  three-arm targeting order (a layer op, a graph op, a stroke op interleave
  by op_id and unified undo walks them in order), `travel_to` driving the
  fake's depth, and the `Journal::Graph` arm's boundary conditions (the
  `:476-497` patterns: graph depth exactly one past the row's target).
- Bin-side (wherever `GraphPanel` is testable in the bin target — find the
  existing graph panel test home and follow it): the
  `GraphCommand -> HistoryKind` mapping pins, and the queue's
  cursor/depth pair around a command push (before = N, after = N+1, and a
  second push after one undo truncates at the cursor).
- Update every existing test that constructs `HistoryEntry` or asserts
  two-arm behavior — the fanout sites are known; grep, don't guess.

## Never-dos (binding)

- NO cargo, rustc, rustfmt, or any build/test invocation. You cannot run
  the suite; your "tests will pass" is hand-tracing. The dragon runs every
  gate.
- NO git of any kind (add/commit/push/stash/checkout/reset/diff --cached).
- NO lock operations: never call `scripts/kill-locks.sh`, never touch
  `/tmp/umber-kill-locks/`, never write to `/tmp` at all.
- NEVER create or edit a file outside the 6-file window above.
- Extend, don't revert: concurrent agents edit this tree; if a file
  carries bytes you did not write, build on them.
- Don't guess APIs: read `docs/claw-artifacts/` and the real sources first
  (`graph_panel/history.rs`, `paint_state.rs`, umber-core's `UndoStack`).
- All the CLAUDE.md Rust traps bind; re-check your diff against every one.
- Tests must be able to fail: pin exact values derived from the arithmetic.

## Deliverable

Code in the window above, complete and compiling by hand-verification
(borrow structure traced, every HistoryEntry site updated, both catalogs
updated), plus your end-of-task report in EXACTLY the CLAUDE.md "Report"
shape (Built / Judgment calls / Unverified / Deferred / Gates run — Gates
run: state plainly that no gate ran because the claw runs no builds). A
session that only reads has failed the kill; so has one that edits outside
the window. Where the tree contradicted this brief, the deviation is named
in Judgment calls with file:line.
