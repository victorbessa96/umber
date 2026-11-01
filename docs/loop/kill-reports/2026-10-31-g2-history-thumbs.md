# Kill report: per-entry history thumbnails (g2-history-panel tail)

Date: 2026-10-31 (late session, dragon local). Lock: `g2-history-thumbs`.
Envelope: code only, no cargo/tests (Bessa's session constraint); rustfmt
write+check is charter-sanctioned. Battery + clippy + the new unit test run
at the next sanctioned wave push.

## Scope

GUI plan G2 `g2-history-panel` row: "Snapshot thumbnails per entry (56 px)"
(photocraft #6's content-keyed `compose::thumbnail(&snap.doc, 56)` adapted
to Umber: the row snapshots the layer its op touched, not the whole doc).

## What landed

- `umber-core/layers.rs`: `LayerCommand::target_layer(&self, &LayerStack)`
  — the id for every id-carrying variant; `Add` only after apply; `Reorder`
  resolves through the stack's applied position.
- `umber-paint/paint_state.rs`: `StrokeCommit.layer: Option<u64>` (the
  stroke's layer) + `PaintState::stroke_layer()` (from `active_target()`);
  both commit push sites in `end_stroke` fill it; 6 test literals updated.
- `history_panel.rs`: `HistoryEntry.target_layer: Option<u64>`;
  `record_undoable`/`record_stroke` take it (graph/context/grouped rows are
  `None`); `apply_stroke_commits` passes `commit.layer`. `history_ui_with_graph`
  takes `&mut Thumbnails`; each row draws its 56 px snapshot left of the label
  (newest-first log, capture via `entry_get` under the per-frame budget).
  `history_ui` (lib/kittest shape) keeps its signature with a per-call
  default cache.
- `document.rs`: `run` records `cmd.target_layer(&self.stack)`; `run_grouped`
  records `None` (a group spans layers).
- `thumbnails.rs`: `HISTORY_THUMB_PX = 56`, `entry_cache: BTreeMap<op_id, tex>`,
  `entry_get` (FROZEN on first capture — never rebuilt, so later edits can't
  rewrite a row's past; skips mid-stroke and unpainted layers), `prune_entries`
  (drops snapshots of truncated/cleared rows), `entry_len`, plus pinned unit
  test `prune_entries_keeps_only_live_rows`. `thumbnails` moved onto the lib
  surface (`pub mod thumbnails`) so the dual-compiled panel can name it.
- `main.rs`: the frame loop, right after `drain_stroke_commits`, prunes entry
  thumbs to live op ids and captures every new row's snapshot in its record
  frame (budget `MAX_REBUILDS_PER_FRAME`, so the freeze lands before any
  undo); the dock call site passes `self.thumbnails`.

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on all 7 touched .rs files.
- Greps: zero stale-arity `record_undoable`/`record_stroke` calls; every
  `HistoryEntry` literal carries `target_layer`; every `StrokeCommit` literal
  carries `layer`; the only `history_ui_with_graph` caller is the shell
  (kittests use the unchanged `history_ui`).
- One regex mishap mid-edit (an `impl HistoryEntry {` body caught by the
  literal-rewrite regex) was repaired in the same sitting; rustfmt parse
  gate now clean.
- NOT run: cargo build/clippy/tests, the new unit test, kittests. Owed at
  the next battery-capable session; commits stay local until then (wave
  gate).

## Known semantics / deferred tails

- A row whose layer had no pixels at record time (`owner_revision == 0`,
  e.g. a fresh Add) captures on the first later frame where the layer has
  pixels — the snapshot then shows that later content, not the empty state.
  Accepted: blank-swatch rows carry no information anyway.
- Base-target strokes (no layer) and graph/context/grouped rows get no
  thumbnail by design.
- 56 px snapshots read tile-1001's baseColor plane only (same plane the
  layer thumbs show); masks are not composited into history thumbs.
