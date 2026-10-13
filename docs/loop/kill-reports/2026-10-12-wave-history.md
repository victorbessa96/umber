# History click-to-time-travel — stroke rows + dual-journal travel

Date: 2026-10-12. Requirements §11 P1 row: "History window (session-global,
click-to-time-travel; not persisted)".

## Starting state (verified before writing)

- Layer click-to-time-travel already worked: `history_panel::history_ui`
  drove `Document::jump_to_depth` (n single steps through the undo/redo
  chokepoints, autosave-journaled), rows highlighted at the live layer
  depth, dead-redo rows truncated on push, saturation past the 256-entry
  eviction window. Nothing about that path changed behavior.
- Stroke rows did not exist in production: `HistoryKind::BrushStroke` was
  context-only (`undo_depth: None`), and no production caller ever recorded
  one — `record_session_event(BrushStroke)` appeared only in
  `document.rs`'s tests. Strokes undid only via the History tab's separate
  single-step Undo/Redo-stroke buttons. Bakes/exports are still
  context-only (no production `record_session_event` callers yet — the
  requirements table's named follow-up, untouched by this kill).

## 1. Built

Files touched (6 + this report):

- `crates/umber-app/src/paint_state.rs` — `StrokeCommit { cursor, depth }`
  (pub, lib surface); `pending_commits` queue + `stroke_epoch` on
  `PaintState`; `end_stroke` queues one commit per recording path
  (retouch and paint-capture; identity/mask-mode strokes queue nothing);
  `record_paint_capture` now returns its `record_stroke` bool (merge
  counts as recorded); new `stroke_len` / `stroke_epoch` /
  `take_stroke_commits` accessors; new `jump_to_stroke_depth` (the
  n-single-steps loop over `undo_stroke`/`redo_stroke`, saturating);
  `resize_set` clears pending commits and bumps the epoch beside the
  existing history clear. 3 tests added.
- `crates/umber-app/src/history_panel.rs` — `HistoryEntry.stroke_depth`
  + `stroke_target()`; `SessionHistory::record_stroke`,
  `truncate_stroke_future`, `stroke_epoch` / `adopt_stroke_epoch`;
  `entry_is_current` (pure highlight rule); `drain_stroke_commits` (frame
  loop entry) split from `apply_stroke_commits` (headless-testable
  mapping); `history_ui` takes `Option<&mut PaintState>`, renders stroke
  rows clickable with per-journal highlight, display-only without a paint
  session, plus the `history.travel-note` line. 7 tests added.
- `crates/umber-app/src/document.rs` — 2 tests added, no production-code
  change (`run`'s `truncate_future` already keeps stroke rows, pinned by
  the new test).
- `crates/umber-app/src/main.rs` — History arm passes the paint session;
  per-frame `drain_stroke_commits` in `ui()` (hidden-tab-safe, same
  rationale as the bake poll); refreshed one stale comment.
- `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl` — one additive key
  each: `history.travel-note` (see "tr() deltas").

Tests added (12): `paint_state.rs` 3 (commit-pair exactness incl.
cursor, jump-travel byte-exact both directions + no-op-at-current,
undone-state cursor, resize epoch/pending clear); `history_panel.rs` 7
(stroke targets vs layer targets, merged shared depth, both truncate
directions, epoch adopt, 4-way highlight matrix, feed mapping incl.
merged pairs, epoch feed with layer-row survival); `document.rs` 2
(counts + content at depths 0/2/3, cross-journal truncate independence).

## Row→position mapping (as built)

- A layer row maps to exactly one number: its `undo_depth` (layer-journal
  length after its push). Click → `Document::jump_to_depth`, which steps
  `undo`/`redo` through the layer chokepoints. The stroke journal is not
  touched.
- A stroke row maps to exactly one number: its `stroke_depth`
  (stroke-journal length after its entry was pushed — the drain commit's
  `depth`). Click → `PaintState::jump_to_stroke_depth`, which steps the
  existing `undo_stroke`/`redo_stroke` API. The layer journal is not
  touched. A merged stroke shares its predecessor's depth: N wall-clock
  rows, one undo step, one shared target (pinned by
  `apply_commits_truncates_the_dead_future_and_shares_merged_depths`).
- A context row (bakes, exports, entry-less strokes) maps to nothing:
  both depths `None`, rendered weak, never clickable. A stroke row with no
  paint session (GPU-less fallback) renders the same way.
- Highlight: `entry_is_current` — layer row iff its depth == live layer
  depth; stroke row iff its depth == live stroke depth (never with no
  session); context never. Each journal's live row highlights
  independently, so a mixed session can show two highlighted rows (one per
  journal) — that is the honest display of two independent positions.

## Interleaving decision (the §3 mixed-document boundary)

Implemented the two homogeneous cases; mixed-target travel is the
documented follow-up — nothing is faked:

- Each click drives exactly one journal and leaves the other where it is.
  The single `op_id` sequence across both journals preserves the
  interleaving as a timeline record, but the panel does NOT reconstruct
  the clicked row's wall-clock moment across journals.
- Why this is safe rather than dishonest: the journals are independent
  (layer pushes never clear stroke redo and vice versa — pinned both
  directions by `layer_push_keeps_stroke_rows_...` and the feed/truncate
  tests), so single-journal travel cannot corrupt the other journal's
  state. Coordinated travel (driving BOTH journals to the clicked row's
  contemporaneous depths) needs dual-depth rows plus an ordering rule and
  is recorded as the follow-up in three places: the module docs'
  mixed-journal boundary section, the panel's `history.travel-note` line
  (user-visible), and "Deferred" below.
- Stale-depth behavior is saturation, never failure or rewrite: evicted
  depths (past 64 entries / 256 MiB on strokes, 256 entries on layers)
  stop the loop at the journal's bottom; a resize drops stroke rows via
  the epoch so old depths can never alias the new history's entries
  (clicking one would redo into unrelated commands — the aliasing case
  the epoch exists to close).

## tr() deltas

- Added `history.travel-note` — en-US: "Layer rows rewind layers, stroke
  rows rewind paint — each travels on its own." pt-BR: "Linhas de camada
  rebobinam camadas, linhas de traço rebobinam a pintura — cada uma por
  si." No existing key changed value; the completeness test's new call
  site resolves in en-US.

## Doc deltas

- `history_panel.rs` module docs: stroke-travel section, mixed-journal
  v1-boundary section, extended capacity note (both caps + epoch),
  this-kill paragraph after the Phase-1-kill-2 note (theirs untouched).
- `paint_state.rs` module docs: resize bullet extended (epoch + pending
  drop); `StrokeCommit`, `end_stroke`, `jump_to_stroke_depth` (incl. why
  the loop is housed on the session: the `lib.rs` test surface has no
  history module, so a panel-housed loop would be untestable from lib
  tests) documented.
- Judgment calls are in those docs; the follow-ups below repeat the
  mixed-travel one for the dragon's tracking.

## 2. Judgment calls

1. One journal per row (not dual-depth rows). Dual depths would let a
   future coordinated-travel read both targets off the clicked row, but
   `Document::run` doesn't know the stroke depth and the stroke path
   doesn't know the layer depth — plumbing both everywhere doubles the
   surface for a follow-up that isn't this kill. v1 rows constrain one
   journal each; the follow-up adds the second depth.
2. The n-steps loop lives on `PaintState::jump_to_stroke_depth`, called
   once per row click, instead of a free loop in the panel. The brief
   prefers panel-housed looping; the `lib.rs` surface (only `paint_state`
   + `retouch`, no history module) forced the call: a panel-housed loop
   over a real session is unreachable from lib tests, leaving the travel
   body uncovered. The body IS the preferred loop (single-step API only,
   stacks as source of truth, no snapshot-restore) — only its address
   moved, and the reason is written on the method.
3. `end_stroke` queues commits; a per-frame drain (not the stroke call
   sites) writes rows. `uv_view`/`viewport` own the stroke-end call sites
   but not the document, and they are outside this kill's surface — the
   queue + always-run drain (hidden-tab-safe, same pattern as the bake
   poll) is the only wiring that stays inside the owned files and never
   misses a stroke painted while History is hidden.
4. Merged strokes get one row each at the shared depth (timeline stays
   honest: every wall-clock stroke that wrote texels is visible), rather
   than coalescing rows. Clicking any of them travels to the one shared
   step.
5. Epoch (u64, bumped on wholesale clear) instead of diffing lengths: a
   length comparison cannot distinguish "undone" from "cleared", and only
   the cleared case must drop rows. `clear()` on the session log does NOT
   reset its epoch — after the Open-Project clear there are no rows, so
   the value is unobservable, and the open path always resizes (bumping
   the paint epoch) before the next drain.
6. The separate single-step stroke buttons stay. They are now redundant
   with clickable rows for travel but remain the one-entry-at-a-time
   control; removing working UI is out of scope.

## 3. Unverified

- No `cargo` ran (brief wall: no cargo, no scripts) — nothing compiled,
  clippy-checked, or test-executed locally; CI (ubuntu + windows) is the
  only gate. Formatting, borrow splits (especially the `history_ui`
  collect-then-act with the added `paint` reborrow and the `end_stroke`
  field splits), intra-doc links, and the 12 new tests are all
  hand-checked only. Highest-risk spots: `paint.as_ref()` then
  `paint.as_mut()` sequencing in `history_ui`; `session.jump_to_stroke_depth`
  auto-deref through `Option<&mut>`; `record_paint_capture`'s changed
  tail type.
- GPU behavior: the three `paint_state` tests need a real adapter
  (graceful-skip pattern, same as the file's existing GPU tests) — they
  run on CI's lavapipe/WARP, skip silently on headless-no-GPU runners.
  Stroke-merge avoidance in those tests rests on the ~200-texel spacing
  the existing `two_paint_strokes` test already relies on.
- `pt-BR.travel-note` wording is this author's Portuguese; a native read
  is welcome but the en-US fallback covers any issue.
- Egui rendering itself (row text, highlight display, click routing) is
  untested by construction, per the file's standing contract.

## 4. Deferred (follow-ups, where recorded)

1. Coordinated mixed-journal travel — driving BOTH journals to the
   clicked row's wall-clock moment (dual-depth rows + ordering rule).
   Recorded: `history_panel` module docs, panel `travel-note`, §2 above.
2. Bakes/Export `record_session_event` production wiring (the
   requirements table's named follow-up for this row). Untouched.
3. Split redo under merged storage (the `retouch` module docs'
   follow-up): a merged entry still undoes/redoes as one unit.
4. The `history.strokes` label ("Smudge / clone / eraser strokes") now
   under-describes the buttons (paint strokes share the journal since the
   tile-pool landing) — pre-existing staleness from that kill, left for
   its surface owner; changing the value would also need both ftls.
5. Ctrl+Z / toolbar Undo still drive layers only; no stroke binding was
   added (out of scope).

## Requirements row

`docs/specs/requirements.md` §11's History row is already marked
`✓ built (9af4c2f)` with the Bakes/Export follow-up named — this kill
extends that same row's "click-to-time-travel" half to strokes without
changing the row's status or its named follow-up, so no table edit was
made (file also outside this kill's surface).

## 5. Gates run

- `bash scripts/verify-kill.sh`: NOT RUN (brief wall: no scripts).
- `cargo fmt --all`: NOT RUN (brief wall: no cargo). New code follows
  the file's existing formatting by hand.
- `cargo clippy -p umber-app --all-targets -- -D warnings`: NOT RUN
  (brief wall). Trap self-check performed instead: no `source` field, no
  `matches!`, no items in `impl` blocks, no format-string field access,
  no new modules/deps, `is_none_or`/`is_some_and` (not `is_some_and` for
  absent-true), field shorthand, no ignored `#[must_use]` (none of the
  touched APIs carry it), intra-doc links use in-scope/imported paths
  except two `crate::history_panel` links in `paint_state.rs` that were
  deliberately rewritten as plain text for the `lib.rs` target (which has
  no history module).
- `cargo test`: NOT RUN. The 12 new tests pin exact values (depths,
  cursors, op ids, counts, byte vectors); range checks are used nowhere;
  GPU tests use the file's established graceful-skip pattern.
