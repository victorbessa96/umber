# Design brief: quick-mask undo (2026-11-18, kill `razul-quickmask-undo`)

Named open follow-up in ROADMAP row 234 ("Undoable mask strokes — needs a new
journal op type; the journal is LayerCommand-typed, the mask lives in
PaintState"). Resolves it using the **filter-apply precedent**, not a new
journal: `crates/umber-app/src/filter_apply.rs` already proves the shape —
a `Command<Doc = PaintState>` op carrying whole-plane before/after bytes on
the shell's own plane-op stack (`FilterUndoStack`), interleaved with the
layer/stroke/graph journals by op id in true newest-first order.

## Finding

Quick-mask edits (brush strokes while `quick_mask().is_editing()`, and
`select_id_mask`) mutate the active set's `QuickMask` 8-bit plane in
`PaintState` (`masks: BTreeMap<String, QuickMask>`, paint_state.rs:1418) with
no undo capture: `undo_stroke` (paint_state.rs:3065) only covers paint
strokes, and the mask is not stroke-captured either.

## Decision

One undo step per **mask-edit stroke** (and per `select_id_mask` call), as a
`QuickMaskOp` on a shell-side stack, mirroring `FilterPlaneOp` exactly:

- `QuickMaskOp { set: String, before: Vec<u8>, after: Vec<u8>, width: u32, height: u32, label: String }`
  — the active set's whole plane before/after (8-bit plane = the persistence
  bytes, byte-stable; memory_bytes = before.len() + after.len()).
- `impl Command for QuickMaskOp { type Doc = PaintState; }` — apply/revert
  write the plane back through a new `PaintState::restore_quick_mask_plane(set, w, h, bytes)`
  (validates size, replaces `masks[set]`, bumps the mask GPU texture — the
  same path the persistence loader uses, so restore == reload semantics).
- Capture points in `paint_state.rs`: `begin_stroke` already snapshots the
  mask gate (line ~2183 `stroke_mask_edit`); when `stroke_mask_edit` is true,
  snapshot `quick_mask().plane` (bytes) into a pending slot; `end_stroke`
  reads the plane back and, if bytes differ, emits the op to the shell.
  `select_id_mask` (2864) does the same around its own mutation (one op,
  label "ID select").
- Shell: `QuickMaskUndoStack` (new `quick_mask_undo.rs`, cloned from
  `FilterUndoStack`'s shape: `undo/redo: Vec<(u64, QuickMaskOp)>` + op-id
  pairing), held next to `filter_undos` in main.rs:1840; `commands.rs`
  Undo/Redo arms check both stacks by op id, same as filter steps;
  `history_panel.rs` renders the row label ("Quick mask stroke" /
  "ID select") — reuse the existing plane-op row arm.

## Scope / non-goals

- Whole-plane before/after (not delta) — matches filter ops and the
  persistence format; a 4K 8-bit plane pair is 32 MiB worst case per step,
  capped by the stack's `max_bytes` like the others.
- No per-dab granularity, no merge of consecutive mask strokes (filter ops
  don't merge either).
- Refused mid-stroke stays refused (`select_id_mask` refuses while
  `stroking`) — no behavior change there.
- No format bump: nothing persists differently (the plane bytes already
  round-trip via `save_quick_masks`).

## Tests

- `quick_mask_stroke_is_one_undo_step`: begin/edit/end a mask stroke →
  exactly one op; undo restores before-bytes; redo restores after-bytes;
  plane bytes equal the snapshots.
- `select_id_mask_pushes_one_op` and undo/redo round-trip.
- `restore_quick_mask_plane_rejects_wrong_size`.
- Op-id interleave: mask op between two layer ops undoes in newest-first
  order (mirror the existing `FilterUndoStack::redo_is_next` test).

## Battery

rustfmt --check, check-layers, scorecard --check,
clippy -p umber-paint -p umber-app --all-targets -D warnings (plus gpu-feature
leg for umber-bake consumers if touched — not expected),
cargo test -p umber-paint -p umber-app under flock, then kill report + queue
row 541 + dcommit --path-limited + safe-push (both session ids) + ls-remote
verify + lock release.
