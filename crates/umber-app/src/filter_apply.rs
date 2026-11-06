//! App-side smart-filter application to the active layer — the app half
//! of the landed `SmartFilter` lane (umber-core 3b1f4ae ships the
//! `.umberfilter` load/save + deterministic CPU `apply`; the CLI got the
//! `import kind` half in 3a99f93; the Assets panel's Filters category feeds
//! this module).
//!
//! Undo shape: the paint stroke journal cannot carry a whole-plane publish
//! (`publish_plane` is not stroke-captured — `stroke_len` does not move
//! across it), so a filter apply is ONE undo step on the shell's own
//! plane-op stack ([`FilterUndoStack`]). Each step pairs its op with the
//! session-history row's op id, so the shell's Undo/Redo arms
//! (commands.rs) interleave filter steps with the layer/stroke/graph
//! journals in true newest-first order.

use crate::paint_state::{PaintState, PlaneOwner, BASE_COLOR_CHANNEL};
use umber_core::smart::{SmartError, SmartFilter};
use umber_core::undo::Command;

/// One touched baseColor plane of the layer: the tile plus its
/// before/after RGBA bytes.
#[derive(Debug, Clone)]
pub struct PlaneSnapshot {
    pub tile: u16,
    pub width: u32,
    pub height: u32,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

/// A whole-layer filter application as ONE [`Command`] over the paint
/// session: `apply` publishes every `after` plane, `revert` every `before`.
#[derive(Debug)]
pub struct FilterPlaneOp {
    pub layer: u64,
    pub filter_name: String,
    pub planes: Vec<PlaneSnapshot>,
}

impl FilterPlaneOp {
    fn publish(&self, paint: &mut PaintState, after: bool) {
        for p in &self.planes {
            let data = if after { &p.after } else { &p.before };
            if paint
                .publish_plane(
                    PlaneOwner::Layer(self.layer),
                    p.tile,
                    BASE_COLOR_CHANNEL,
                    p.width,
                    p.height,
                    data.clone(),
                )
                .is_err()
            {
                log::warn!(
                    "filter '{}': plane publish refused on tile {}",
                    self.filter_name,
                    p.tile
                );
            }
        }
    }
}

impl Command for FilterPlaneOp {
    type Doc = PaintState;

    fn apply(&mut self, paint: &mut PaintState) {
        self.publish(paint, true);
    }

    fn revert(&mut self, paint: &mut PaintState) {
        self.publish(paint, false);
    }

    fn memory_bytes(&self) -> usize {
        self.planes
            .iter()
            .map(|p| p.before.len() + p.after.len())
            .sum()
    }
}

/// Builds the op for `layer` WITHOUT publishing: reads each materialized
/// baseColor plane of the layer across the active tile set, runs the
/// filter over a copy, and pairs before/after. `Ok(None)` when the layer
/// has no materialized plane (nothing to filter); `Err` when the filter
/// itself refuses (an empty op list — load-time validation already passed
/// the rest).
pub fn build_filter_op(
    paint: &mut PaintState,
    layer: u64,
    filter: &SmartFilter,
) -> Result<Option<FilterPlaneOp>, SmartError> {
    let mut planes = Vec::new();
    for tile in paint.tiles_present() {
        let Some(plane) = paint.read_plane(PlaneOwner::Layer(layer), tile, BASE_COLOR_CHANNEL)
        else {
            continue;
        };
        let (width, height) = paint.tile_size(tile);
        let mut after = plane.data.clone();
        filter.apply(&mut after, width as usize, height as usize)?;
        planes.push(PlaneSnapshot {
            tile,
            width,
            height,
            before: plane.data,
            after,
        });
    }
    if planes.is_empty() {
        return Ok(None);
    }
    Ok(Some(FilterPlaneOp {
        layer,
        filter_name: filter.name.clone(),
        planes,
    }))
}

/// The shell's undo stack for whole-layer filter applies. Each entry pairs
/// the op with its session-history row's op id so the Undo/Redo arms can
/// tell "the newest row is a filter step" apart from the journal rows.
#[derive(Debug, Default)]
pub struct FilterUndoStack {
    undo: Vec<(u64, FilterPlaneOp)>,
    redo: Vec<(u64, FilterPlaneOp)>,
}

impl FilterUndoStack {
    /// Pushes one applied filter as a single undo step. `op_id` is the
    /// session-history row recorded for the apply; the op's own
    /// [`Command::apply`] publishes the `after` planes here (the one and
    /// only publish — `build_filter_op` never publishes).
    pub fn push(&mut self, op_id: u64, mut op: FilterPlaneOp, paint: &mut PaintState) {
        op.apply(paint);
        self.redo.clear();
        self.undo.push((op_id, op));
        // Same spirit as the journals' caps: the oldest step falls off.
        const CAP: usize = 32;
        while self.undo.len() > CAP {
            self.undo.remove(0);
        }
    }

    /// True when `last` (the newest session-history row) is this stack's
    /// newest undo step — the Undo arm checks this before the journals.
    pub fn newest_is_ours(&self, last: Option<&crate::history_panel::HistoryEntry>) -> bool {
        match (last, self.undo.last()) {
            (Some(e), Some((id, _))) => {
                e.kind == crate::history_panel::HistoryKind::FilterApply && e.op_id == *id
            }
            _ => false,
        }
    }

    /// Undoes the newest filter step (reverts its planes) and returns its
    /// row's op id for the caller to pop from the session history.
    pub fn undo(&mut self, paint: &mut PaintState) -> Option<u64> {
        let (id, mut op) = self.undo.pop()?;
        op.revert(paint);
        self.redo.push((id, op));
        Some(id)
    }

    /// True when the next redo belongs to this stack: the undone row's id
    /// is exactly the one before the next op id the history will assign
    /// (op ids are monotone and never reused, so anything newer in the log
    /// means a journal step rightly goes first).
    pub fn redo_is_next(&self, next_op_id: u64) -> bool {
        self.redo
            .last()
            .is_some_and(|(id, _)| *id + 1 == next_op_id)
    }

    /// Redoes the next filter step (re-publishes its planes) and re-files
    /// it under `new_id` — the fresh op id of the row the caller just
    /// re-recorded. Returns false when the redo stack is empty.
    pub fn redo(&mut self, paint: &mut PaintState, new_id: u64) -> bool {
        if let Some((_, mut op)) = self.redo.pop() {
            op.apply(paint);
            self.undo.push((new_id, op));
            true
        } else {
            false
        }
    }

    /// Test hook: the number of undoable filter steps held.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
}
