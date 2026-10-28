# Kill report: 2026-10-27 L6 — the third journal joins the unified timeline

**Status: Gate 1 GREEN (r5, umber-app 873 tests) — committing; the wave
push is the remaining gate.**

- **Kill**: `l6-unified-timeline` (the W-C4 runway's L6; kill 2 of the
  procedural-history arc — kill 1 was `2a84fb8`, the graph journal itself).
- **Wave**: the fresh charter cycle, opened by Bessa's launch word
  2026-10-27 ("Continue working autonomously in the Umber project. Feel
  free to use all the claws").
- **Authorship**: the opencode claw (`deepseek-v4.1-flash-free`, canary
  green this date) authored the bytes per the brief
  `docs/loop/briefs/2026-10-27-l6-unified-timeline.md`; the dragon derived
  the design, holds the lock, reviewed every hunk, fixed forward three
  battery reds, gates, and commits. The pi claw (glm-5.3) ran the
  different-family cross-review.
- **Kill-lock**: `l6-unified-timeline` — history_panel.rs, graph_panel.rs,
  graph_panel/history.rs, main.rs, assets/i18n/en-US.ftl,
  assets/i18n/pt-BR.ftl, this report. Registered 18:22:02 by this
  session's first turn.
- **Design sources**: the frozen derivation (this session, 2026-10-27
  17:11-18:22, recorded in the coordination claim entry); the graph-history
  kill report (2026-10-19) naming `unified_undo/redo` consulting the graph
  journal as the arc's kill 2; ROADMAP's L6 row.

## Process notes (fact, not narrative)

1. **The turn death and recovery.** This session's first turn
   (17:11→~18:46) ran the census, derived the full L6 design, registered
   the lock at 18:22:02, and died silently mid-brief — zero repo bytes,
   no claim entry, the brief never landed on disk. Hermes redelivered
   Bessa's message at 19:00:25; the fresh turn recovered the frozen
   design from the transcript (state.db) rather than re-deriving. Lesson
   applied: every load-bearing intermediate lands as a file before the
   next step runs.
2. **The claude lane is auth-dead.** The first dispatch (`claude -p …
   --permission-mode acceptEdits`) died in 186ms: "Failed to authenticate:
   OAuth session expired and could not be refreshed" — Bessa's Pro sub
   needs his interactive re-login; the workspace-trust flag for
   `~/Projects/umber` is also unset (a partner gate; not self-opened).
   The kill fell back to opencode on `deepseek-v4.1-flash-free`
   (canary-proven; the skill's standing pick for bounded code kills
   against a pinned design). The claude lane stays down until Bessa
   re-auths.
3. **The free roster rotated again**: opencode's default is now
   `flint-40b-free`; `muse-spark` is gone; `nemotron-3-ultra-free` is
   currently "Model is unavailable". Pinned only after canaries answered.
4. **Gate 1's reds, in order**: r1 RED (4 borrow errors — the closure-
   capture + elided-object-lifetime trap on `&mut dyn GraphJournal`; fixed
   by the toast-pattern hoist + the `dyn GraphJournal + '_` object-
   lifetime split on the three callee signatures), r2 RED (the same root,
   deeper — sequential fn-body dispatches; the signature split completed
   the fix), r3 RED (two `needless_option_as_deref` lints at travel_to's
   last-use site + the queue test's fixture lie — 633 passed/1 failed;
   production right, the assert's assumption wrong), r4 **killed by
   systemd-oomd** (see below), r5 = the gate of record, scope-capped.
5. **The r4 oomd death + two dragon missteps, recorded plainly.**
   oomd killed the r4 battery's whole scope at 23:20:09 ("killed 15
   process(es)", the journal line quoted in the wave coordination) —
   memory pressure during the umber-app test-binary compile+link, with
   the pi review's resident node process as the marginal load (r3
   survived the identical leg on a quieter box). The dragon's two
   missteps around it: (a) both scope-cap attempts hit the WRONG unit
   (a /proc cgroup hunt for a wrapper pid that dies instantly — the
   scope name was in the launch output all along:
   `hermes-worker-proc_<session-id>.scope`); (b) the two post-r4 lint
   fixes landed while r4's test leg was still in flight — the
   no-mid-battery-edit rule, broken; the contaminated verdict was
   dispositioned and r5 is the gate of record for the fixed bytes.
6. **The first pi cross-review died at the model's output token limit**
   mid-synthesis (findings never materialized — one text block in the
   transcript); the terse-format re-dispatch stalled silent and was
   killed at the battery's expense (the memory-margin call); the
   different-family cross-review is re-flown after the battery lands
   or rides the claude lane's revival — the debt is named either way.

## Built

Per file, as verified by the dragon's hunk-by-hunk read of the diff
(1442+/158- at first landing, plus the fix-forward deltas):

- `crates/umber-app/src/history_panel.rs` — the core. `HistoryKind` +7
  variants (GraphAddNode/GraphRemoveNode/GraphConnect/GraphDisconnect/
  GraphSetParam/GraphSetOutput/GraphLoad) with `tr()` wiring and
  lowercase `label()` arms; `HistoryEntry.graph_depth: Option<usize>` +
  `graph_target()` (the stroke twin; all 7 construction sites updated —
  verified by grep, zero sites outside this file); `GraphCommit { kind,
  cursor, depth }` (the StrokeCommit shape; the epoch deliberately NOT on
  the lib type); the `GraphJournal` trait (graph_depth/undo/redo/
  jump_to_depth, with the `Some(0)`-not-`None` semantics documented);
  `SessionHistory` gains `graph_epoch` + `record_graph` +
  `truncate_graph_future` + `adopt_graph_epoch` (the three stroke twins);
  `Journal::Graph`; the three-arm `unified_undo_target`/`unified_redo_target`
  (graph guards mirror the stroke windows: `1 <= d <= len` / `d == len+1`;
  the depth-window composition rationale documented); `unified_undo`/
  `unified_redo`/`travel_to` take the graph arm
  (`Option<&mut (dyn GraphJournal + '_)>` — the object lifetime split is
  the fix-forward, see Judgment calls); `travel_target` →
  `(layer, stroke, graph)` triple; `entry_is_current`/`row_state` third
  arms; `apply_graph_commits` (adopt epoch → per-commit truncate →
  record); `history_ui` kept as the lib-shape wrapper (kittest
  harnesses keep compiling — it forwards to `history_ui_with_graph` with
  `None`); `history_ui_with_graph` renders graph rows and travels all
  three journals on click. New tests: the three-arm interleave walk
  (S2,G2,L2,S1,G1,L1 + exact redo reverse), the graph arm's boundary
  windows (the depth-0 load never targets; `None` journal = not
  candidates), the load row's Current/Past states, the FakeGraph
  travel double (records every jump step — proves no redundant steps).
- `crates/umber-app/src/graph_panel.rs` — bin-side. `graph_kind(&cmd)`
  free fn (total over GraphCommand; Connect+AddEdge share
  GraphConnect — one user action); `graph_queue: GraphQueue` field;
  `impl GraphJournal for GraphPanel` (drives the SAME UndoStack the
  panel buttons drive — no second history); `load_graph` bumps the
  epoch + queues ONE GraphLoad commit at (0,0). Tests: the total
  mapping table; the queue's cursor/depth pairs end-to-end through
  `apply_graph_commits` with a real Document (undo → fresh push → the
  dead depth-2 row truncated, op ids prove it); the load's epoch bump +
  one-row queue; the trait-view-is-the-buttons-journal pin (redo
  restores exact node ids through the trait).
- `crates/umber-app/src/graph_panel/history.rs` — `GraphQueue { epoch,
  pending }` (push/take/bump_epoch; the epoch lives bin-side so the lib
  never learns the panel exists); `run_graph_command` reads cursor/depth
  around the push (the 256-cap eviction → merged-row semantics
  documented); `graph_history_len`/`jump_to_graph_depth` (single-step
  loop, saturating — never a truncate shortcut)/`graph_epoch`/
  `take_graph_commits`; module docs rewritten for the L6 truth.
- `crates/umber-app/src/main.rs` — the shell wiring. The frame-top
  `drain_graph_commits()` (before any dispatch this frame — the stroke
  drain's rule, hoisted because the lib cannot reach the bin-only
  panel); the Undo/Redo dispatch arms drain before deciding; the
  History tab call site builds the dyn bridge; `shell_state` derives
  Undo/Redo availability from the unified targets (the claw's find
  beyond the brief: the old `doc.history.can_undo()` gate refused
  Ctrl+Z in graph-only/stroke-only sessions).
- `assets/i18n/en-US.ftl` + `pt-BR.ftl` — 7 `.op-graph-*` keys under the
  existing `history` group, both catalogs (verified once each by grep).

## Judgment calls

The claw's (from its report, re-verified by the dragon):

1. **The load is one row, never a target**: `GraphLoad` records depth 0 —
   a real position (clicking it undoes everything after the load) that
   `unified_*_target`'s `d >= 1` guards can never step on. Documented in
   the kind's doc + the module docs.
2. **The epoch lives on the bin-side queue**, not on `GraphCommit` — the
   lib's commit type stays the journal's `(cursor, depth)` + kind.
3. **`graph_depth()` returns `Some(len)` even at 0** — `None` means "no
   journal to drive" (the paint presence semantics); `None` at an empty
   journal would strand the redo after undoing the first command.
4. **Connect and AddEdge share `GraphConnect`** — the canvas connect and
   the list editor's raw insert are one user-meaningful action.
5. **`apply_graph_commits` does NOT set `doc.dirty`** — a load routes
   through `load_graph`, and Open-Project marks the document clean;
   marking dirty there would make every freshly-opened project read
   unsaved. The graph-edit-quit-gate gap is pre-existing, named in the
   fn doc.
6. **Availability rewiring** (`shell_state`) — not named in the brief, but
   without it the kill ships Ctrl+Z that is refused in exactly the
   sessions it newly makes undoable.
7. **Drain placement**: frame-top + inside the Undo/Redo arms; the
   lib-side `unified_*` drains strokes only (it cannot reach the panel),
   so the shell drains graphs — both drains land before any decision.

The dragon's fix-forwards (each from a battery FAIL line):

8. **r1/r2 RED — the closure-capture + elided-object-lifetime trap.**
   `Option<&mut dyn GraphJournal>` elides to `&'1 mut (dyn GraphJournal +
   '1)`; a trait object's lifetime is invariant, so EVERY reborrow of the
   binding requires the full `'1` — first inside the egui closures
   (E0499/E0597), then, after hoisting, between sequential fn-body
   dispatches. Two-part fix: (a) the toast kill's collect-then-act
   pattern — clicks recorded in the closures, all dispatches after them
   (the closures stay shared-borrow-only); (b) the three callee
   signatures split the object lifetime — `Option<&mut (dyn GraphJournal
   + '_)>` — so each call's borrow of the binding ends at return.
   `Option<&mut PaintState>` never tripped because the concrete type
   lets reborrows shrink; the dyn object does not. The trap is now a
   CLAUDE.md-class lesson and is recorded in the fn's doc comment.
9. **r3 RED — two `needless_option_as_deref` lints** at `travel_to`'s
   call: the last use of both bindings, so they move instead of
   reborrowing (`travel_to(doc, paint, graph, op_id)`).
10. **r3 RED — the queue test's fixture lie.** `the_graph_queue_pins_
    the_cursor_and_depth_pair` drained the first commit with its first
    `take_graph_commits()` and then expected `both` to carry two rows
    into the doc — the drain semantics it was testing. Production was
    right, the assert's assumption wrong (the skill's inverse-case
    pitfall). Fixed by binding the first take (`first`), naming the
    second (`second`), and applying both in order — the truncation back
    half now proves what it always meant to prove. Named here as the
    claw's one test-honesty miss, caught by the battery.

## Unverified

- **Runtime behavior**: a live graph edit + Ctrl+Z round-trip in the
  running app; graph rows highlighting and jumping in a real History
  panel; the real `GraphPanel` (vs. the FakeGraph double) under
  `travel_to`. The battery pins the model headless; the GUI itself is
  untested by construction (the module's standing note).
- **The i18n completeness test's directional pin** covers the new keys
  only insofar as the suite exercises it (it does — the battery runs the
  umber-app suite).
- **The pi cross-review's second attempt** (terse format) — verdict
  pending at report-writing time; disposition rides the wave close.

## Deferred

- Graph edits don't set `doc.dirty` (the pre-existing quit-gate gap;
  named in `apply_graph_commits`' doc + module docs).
- Graph rows dropped on texture-set switch — the epoch adopt, exactly as
  stroke rows already are (mirrored, not new).
- The panel-local Undo/Redo buttons don't drain their own queue —
  harmless: the frame-top drain owns it.
- No crash-report op note for graph commands (`crash::note_op` still
  sees only layer/session events).
- `history.travel-note` stays unrendered in both catalogs (the comment
  at history_panel.rs notes it; its retirement wants a catalog-focused
  pass).
- The claude lane's revival (Bessa's re-auth + the workspace-trust
  dialog) — partner-gated.

## Gates run

- verify-kill (dragon): PASS modulo the pre-existing main.rs brace-count
  false positive (HEAD reads 927/928 — the same -1; the claw's added
  lines contribute one matched pair; rustfmt parses clean) and two
  pre-existing thiserror-name WARNs (enum variant fields named `source`).
- Layering gate: GREEN (16 crates), run by the claw AND re-run by the
  dragon.
- `cargo fmt --all --check`: PASS (the battery's leg, rounds 1-4).
- Gate 1 (battery, `wave-battery.sh umber-app`): r1 RED (4 borrow
  errors), r2 RED (same root, deeper), r3 RED (2 lints + the fixture
  lie; 633 passed/1 failed), r4 KILLED by systemd-oomd mid-test-leg
  (uncapped scope + the pi claw's resident memory; disposition and
  missteps under Process notes), **r5 GREEN: clippy PASS, feature-on
  gpu leg PASS, umber-app 873 tests PASS (+22 over the 851 baseline),
  fmt + layers PASS — the gate of record for the committed bytes.**
