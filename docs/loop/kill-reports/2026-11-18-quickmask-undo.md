# Kill report: quick-mask undo (2026-11-18, lock `razul-quickmask-undo`)

Brief: `docs/loop/briefs/2026-11-18-quickmask-undo.md` (`99884f27`, docs-first).
Closes the ROADMAP row 234 named follow-up ("undoable mask strokes").

## Design

Filter-op precedent (`filter_apply.rs`), not a new journal variant: the
journal stays LayerCommand-typed; mask edits are `Command<Doc = PaintState>`
ops on a shell-side stack, interleaved by op id like filter steps.

## Changes

- `crates/umber-paint/src/paint_state.rs`: `MaskUndoEvent { set, before,
  after, id_select }` + `pending_mask_events` + `mask_stroke_before`.
  `begin_stroke` snapshots the whole active mask for mask-mode strokes;
  `end_stroke` queues one event when the generation moved; `select_id_mask`
  queues one event per click (refused mid-stroke, unchanged). All four
  journal-clear sites also clear the mask queue. `take_mask_events()` drain.
  New GPU-gated test `id_mask_click_queues_one_undo_event_and_restores`
  (runs in CI's lavapipe step; skips locally under the capability gate).
- `crates/umber-app/src/quick_mask_undo.rs` (new): `QuickMaskOp`
  (apply/revert via the existing `restore_quick_mask`, exact plane bytes;
  memory_bytes from the persist-blob footprint x4 for f32) + `QuickMaskUndoStack`
  mirroring `FilterUndoStack` (cap 32, op-id pairing, newest_is_ours /
  redo_is_next). Two CPU tests.
- `main.rs`: `mod quick_mask_undo` (bin) + `pub mod` (lib), `mask_undos`
  field, per-frame drain next to `drain_stroke_commits` recording
  `HistoryKind::QuickMask` rows.
- `commands.rs`: Undo/Redo arms check `mask_undos` exactly like
  `filter_undos` (newest-first interleave with journals).
- `history_panel.rs`: `HistoryKind::QuickMask` + tr/crash labels.
- i18n: `history.op-quick-mask` in en-US ("Quick mask") and pt-BR
  ("Máscara rápida").

## Battery

rustfmt clean; check-layers GREEN (16); scorecard current;
clippy `-p umber-paint -p umber-app --all-targets -D warnings` 0 errors;
cargo test 13 suites ok / 0 failed (new app tests green; GPU mask test
CI-bound). safe-push green; ls-remote verified; lock released; queue row 541.
