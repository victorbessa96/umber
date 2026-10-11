//! The History window: requirements.md §11 P1 row — "History window
//! (session-global, click-to-time-travel; not persisted)".
//!
//! The session history model ([`SessionHistory`]) MIRRORS the undo journal
//! (`umber_core::undo::UndoStack`, owned by [`Document`](crate::document::Document)):
//! every [`Document::run`](crate::document::Document::run) records one
//! [`HistoryEntry`] from the same [`LayerCommand`](umber_core::layers::LayerCommand)
//! that was just pushed. It is deliberately NOT the autosave journal
//! (`umber_core::journal` — the append-only `journal.jsonl` crash-recovery
//! log beside `project.json`): that one records effective forward edits for
//! disk replay, while this one records session op labels for display +
//! click-to-time-travel. Do not conflate the two.
//!
//! Session-global and NOT persisted: the model lives in RAM only, is never
//! serialized into the `.umber` project, and is cleared together with the
//! undo journal on the Open-Project path
//! ([`Document::load_stack`](crate::document::Document::load_stack)) —
//! history entries capture ids/indices valid only against the stack they
//! were recorded against, exactly like the journal entries they mirror.
//!
//! Click-to-time-travel: each undoable entry carries the undo depth to jump
//! to (`undo_depth`: the undo-stack length just after its op was pushed).
//! Clicking entry N calls
//! [`Document::jump_to_depth`](crate::document::Document::jump_to_depth)`(N)`,
//! which steps N undos/redos through the normal undo/redo chokepoints (so
//! the autosave journal records every intermediate edit, as required by
//! `journal.rs`' read-before-revert rule) — a loop of single steps, not a
//! truncate shortcut. Non-undoable session events (bakes, exports, and —
//! until stroke commits become layer-journal ops — brush strokes) are
//! recorded with `undo_depth: None` for timeline context: they render but
//! never jump.
//!
//! Capacity note: the log itself is unbounded (rows are a kind + two small
//! integers), while the journal it mirrors is capped at 256 entries. Past
//! 256 pushes the journal evicts its oldest command, so the oldest rows'
//! depths no longer exist in the journal — clicking one saturates at the
//! journal's bottom instead of failing (`jump_to_depth` stops when `undo`
//! returns `false`). Rows are never rewritten to hide this: the log is the
//! session's honest timeline, and saturation is the documented behavior.

use fluent::FluentValue;
use umber_core::layers::{LayerCommand, LayerKind};

use crate::document::Document;
use crate::i18n::{tr, tr_args};

/// What one history row describes: every undoable [`LayerCommand`] shape
/// plus the non-undoable session events recorded for context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    /// [`LayerCommand::Add`] of a paint layer.
    AddPaint,
    /// [`LayerCommand::Add`] of a fill layer.
    AddFill,
    /// [`LayerCommand::Add`] of a folder.
    AddFolder,
    /// [`LayerCommand::Add`] of an instance layer (§3:41).
    AddInstance,
    /// [`LayerCommand::Remove`].
    Remove,
    /// [`LayerCommand::Reorder`].
    Reorder,
    /// [`LayerCommand::SetOpacity`].
    SetOpacity,
    /// [`LayerCommand::SetVisible`].
    SetVisible,
    /// [`LayerCommand::SetBlendMode`].
    SetBlendMode,
    /// [`LayerCommand::SetFill`].
    SetFill,
    /// [`LayerCommand::SetInstanceSource`] (instance retarget).
    SetInstanceSource,
    /// A brush stroke committed by the paint session. Context-only in v1:
    /// strokes go through `PaintState`, not the layer journal, so there is
    /// no command to revert yet (the stroke becomes undoable with the
    /// tile-pool slice, at which point it records a depth like the rest).
    BrushStroke,
    /// A finished mesh-map bake. Context-only: baking writes PNGs to disk
    /// and never touches the layer stack, so there is nothing to undo.
    Bake,
    /// A finished texture export. Context-only, same reason as [`Self::Bake`].
    Export,
}

impl HistoryKind {
    /// Maps an applied command to its row kind. Total over [`LayerCommand`]:
    /// `Add` splits on the created layer's kind (the spec's "Fill layer"
    /// example is an `Add` of `LayerKind::Fill`, distinct from a `SetFill`
    /// param edit).
    pub fn from_command(command: &LayerCommand) -> Self {
        match command {
            LayerCommand::Add { kind, .. } => match kind {
                LayerKind::Paint => Self::AddPaint,
                LayerKind::Fill => Self::AddFill,
                LayerKind::Folder { .. } => Self::AddFolder,
                LayerKind::Instance { .. } => Self::AddInstance,
            },
            LayerCommand::Remove { .. } => Self::Remove,
            LayerCommand::Reorder { .. } => Self::Reorder,
            LayerCommand::SetOpacity { .. } => Self::SetOpacity,
            LayerCommand::SetVisible { .. } => Self::SetVisible,
            LayerCommand::SetBlendMode { .. } => Self::SetBlendMode,
            LayerCommand::SetFill { .. } => Self::SetFill,
            LayerCommand::SetInstanceSource { .. } => Self::SetInstanceSource,
        }
    }

    /// The i18n'd op name rendered in the row. Resolved at draw time (not
    /// stored) so a mid-session locale switch re-renders every row.
    pub fn label(self) -> String {
        match self {
            Self::AddPaint => tr("history.op-add-paint"),
            Self::AddFill => tr("history.op-add-fill"),
            Self::AddFolder => tr("history.op-add-folder"),
            Self::AddInstance => tr("history.op-add-instance"),
            Self::Remove => tr("history.op-remove"),
            Self::Reorder => tr("history.op-reorder"),
            Self::SetOpacity => tr("history.op-set-opacity"),
            Self::SetVisible => tr("history.op-set-visible"),
            Self::SetBlendMode => tr("history.op-set-blend"),
            Self::SetFill => tr("history.op-set-fill"),
            Self::SetInstanceSource => tr("history.op-set-instance-source"),
            Self::BrushStroke => tr("history.op-brush-stroke"),
            Self::Bake => tr("history.op-bake"),
            Self::Export => tr("history.op-export"),
        }
    }
}

/// One row of the History window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// What the row describes (drives the i18n'd label).
    pub kind: HistoryKind,
    /// Monotonic session counter, assigned in record order (0, 1, 2, …).
    /// Display/investigation order only — it is NOT the autosave journal's
    /// `seq` (no-op edits still take an op id here while journaling no
    /// line there).
    pub op_id: u64,
    /// The undo depth to jump to when this row is clicked: the undo-stack
    /// length just after its op was pushed. `None` for context-only events
    /// (bakes, exports, v1 brush strokes), which display but don't jump.
    pub undo_depth: Option<usize>,
}

impl HistoryEntry {
    /// The depth a click on this row travels to — `None` means the row is
    /// display-only and the panel issues no jump. Keyed off `undo_depth`,
    /// not `kind`, so any future context kind is display-only by
    /// construction.
    pub fn jump_target(&self) -> Option<usize> {
        self.undo_depth
    }
}

/// The session history model: an append-only [`HistoryEntry`] log mirroring
/// the undo journal, plus context events. RAM-only, never persisted (see
/// the module docs).
#[derive(Debug, Default)]
pub struct SessionHistory {
    entries: Vec<HistoryEntry>,
    next_op_id: u64,
}

impl SessionHistory {
    /// An empty session log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one undoable op at `depth` (the undo-stack length just after
    /// its push). Called from the same chokepoint that pushed the command.
    pub fn record_undoable(&mut self, kind: HistoryKind, depth: usize) {
        self.entries.push(HistoryEntry {
            kind,
            op_id: self.next_op_id,
            undo_depth: Some(depth),
        });
        self.next_op_id += 1;
    }

    /// Records one non-undoable session event (bake, export, v1 brush
    /// stroke) for timeline context. Always stored with `undo_depth: None`,
    /// whatever the kind — the panel renders it but never jumps to it.
    pub fn record_context(&mut self, kind: HistoryKind) {
        self.entries.push(HistoryEntry {
            kind,
            op_id: self.next_op_id,
            undo_depth: None,
        });
        self.next_op_id += 1;
    }

    /// Drops the dead future after a fresh push over undone state: a push
    /// clears the journal's redo stack, so entries recorded past `cursor`
    /// (the undo depth just before the push) no longer have commands to
    /// redo to — clicking them would travel nowhere real. Context rows
    /// (`undo_depth: None`) survive: they describe wall-clock session
    /// events, not journal positions.
    pub fn truncate_future(&mut self, cursor: usize) {
        self.entries
            .retain(|e| e.undo_depth.is_none_or(|d| d <= cursor));
    }

    /// Clears the whole log (the Open-Project path, alongside the journal's
    /// own `clear`).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_op_id = 0;
    }

    /// Every recorded row, oldest first (record order == display order).
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Number of recorded rows (undoable + context).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Draws the History panel: undo/redo buttons, the journal-size line, then
/// the session log — oldest first, one row per entry (`depth · label` for
/// undoable ops, weak `· label` for context rows), the row at the live undo
/// depth highlighted. Clicking an undoable row time-travels the document to
/// its depth; context rows are display-only.
///
/// The egui drawing itself is untested by construction; the model (record
/// order, jump math, display-but-don't-jump, truncate equivalence) is
/// covered headless in this module's + `document`'s tests.
pub fn history_ui(ui: &mut egui::Ui, doc: &mut Document) {
    ui.horizontal(|ui| {
        let can_undo = doc.history.can_undo();
        let can_redo = doc.history.can_redo();
        if ui
            .add_enabled(can_undo, egui::Button::new(tr("history.undo")))
            .clicked()
        {
            doc.undo();
        }
        if ui
            .add_enabled(can_redo, egui::Button::new(tr("history.redo")))
            .clicked()
        {
            doc.redo();
        }
    });
    ui.separator();
    let n = doc.history.len();
    ui.label(tr_args(
        "history.journal",
        [("count", FluentValue::from(n))],
    ));
    ui.separator();
    if doc.session_history.is_empty() {
        ui.weak(tr("history.empty"));
        return;
    }
    let depth = doc.history.len();
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Snapshot the rows first so the click-jump below can't
        // borrow-conflict with the iteration (the layers panel's E0502
        // pattern): collect-then-act.
        struct Row {
            text: String,
            target: Option<usize>,
            current: bool,
        }
        let rows: Vec<Row> = doc
            .session_history
            .entries()
            .iter()
            .map(|entry| match entry.jump_target() {
                Some(d) => Row {
                    text: format!("{d} · {}", entry.kind.label()),
                    target: Some(d),
                    current: d == depth,
                },
                None => Row {
                    text: format!("· {}", entry.kind.label()),
                    target: None,
                    current: false,
                },
            })
            .collect();
        let mut jump_to = None;
        for row in &rows {
            match row.target {
                Some(target) => {
                    if ui.selectable_label(row.current, &row.text).clicked() {
                        jump_to = Some(target);
                    }
                }
                None => {
                    ui.label(egui::RichText::new(&row.text).weak());
                }
            }
        }
        if let Some(target) = jump_to {
            doc.jump_to_depth(target);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::layers::LayerKind;

    #[test]
    fn from_command_labels_every_variant() {
        // Every LayerCommand shape maps to a distinct kind, and every kind
        // resolves to a non-empty i18n'd label (test threads run the
        // built-in en-US catalog).
        let cases = [
            (
                LayerCommand::add("p", LayerKind::Paint),
                HistoryKind::AddPaint,
                "Add paint layer",
            ),
            (
                LayerCommand::add("f", LayerKind::Fill),
                HistoryKind::AddFill,
                "Add fill layer",
            ),
            (
                LayerCommand::add("g", LayerKind::Folder { passthrough: true }),
                HistoryKind::AddFolder,
                "Add folder layer",
            ),
            (
                LayerCommand::add("i", LayerKind::Instance { source_id: 0 }),
                HistoryKind::AddInstance,
                "Add instance layer",
            ),
            (
                LayerCommand::set_instance_source(0, 1),
                HistoryKind::SetInstanceSource,
                "Set instance source",
            ),
            (LayerCommand::remove(0), HistoryKind::Remove, "Remove layer"),
            (
                LayerCommand::reorder(0, 1),
                HistoryKind::Reorder,
                "Reorder layer",
            ),
            (
                LayerCommand::set_opacity(0, 0.5),
                HistoryKind::SetOpacity,
                "Set opacity",
            ),
            (
                LayerCommand::set_visible(0, false),
                HistoryKind::SetVisible,
                "Set visibility",
            ),
            (
                LayerCommand::set_blend_mode(0, umber_core::layers::BlendMode::Multiply),
                HistoryKind::SetBlendMode,
                "Set blend mode",
            ),
        ];
        for (command, kind, label) in cases {
            assert_eq!(HistoryKind::from_command(&command), kind);
            assert_eq!(kind.label(), label);
        }
        // SetFill needs real FillParams; cover its label directly.
        assert_eq!(HistoryKind::SetFill.label(), "Fill layer");
        assert_eq!(HistoryKind::BrushStroke.label(), "Brush stroke");
        assert_eq!(HistoryKind::Bake.label(), "Bake");
        assert_eq!(HistoryKind::Export.label(), "Export");
    }

    #[test]
    fn recording_order_assigns_monotonic_op_ids_and_depths() {
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_context(HistoryKind::Bake);
        log.record_undoable(HistoryKind::SetOpacity, 2);

        let entries = log.entries();
        assert_eq!(entries.len(), 3);
        // Record order is display order; op ids are 0, 1, 2 regardless of
        // undoability.
        assert_eq!(
            entries.iter().map(|e| e.op_id).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            entries.iter().map(|e| e.undo_depth).collect::<Vec<_>>(),
            vec![Some(1), None, Some(2)]
        );
        // Jump targets follow undo_depth, never kind.
        assert_eq!(entries[0].jump_target(), Some(1));
        assert_eq!(entries[1].jump_target(), None);
        assert_eq!(entries[2].jump_target(), Some(2));
    }

    #[test]
    fn context_entries_display_but_dont_jump_even_for_undoable_kinds() {
        // A programming slip that records an undoable kind via
        // record_context (no depth) must still be display-only: the panel
        // keys off undo_depth, not kind.
        let mut log = SessionHistory::new();
        log.record_context(HistoryKind::AddPaint);
        let entry = &log.entries()[0];
        assert_eq!(entry.kind, HistoryKind::AddPaint);
        assert_eq!(entry.jump_target(), None);
    }

    #[test]
    fn truncate_future_drops_dead_redo_rows_and_keeps_context() {
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_undoable(HistoryKind::AddFill, 2);
        log.record_context(HistoryKind::Bake);
        log.record_undoable(HistoryKind::SetOpacity, 3);

        // Undone back to depth 1, then a fresh push: the depth-2..3 rows
        // describe commands the cleared redo stack no longer holds.
        log.truncate_future(1);
        assert_eq!(
            log.entries()
                .iter()
                .map(|e| (e.kind, e.undo_depth))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::AddPaint, Some(1)),
                // The bake happened on the wall clock; it stays.
                (HistoryKind::Bake, None),
            ]
        );
        // Truncating at the live depth keeps everything.
        log.truncate_future(1);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn clear_resets_entries_and_restarts_op_ids() {
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_context(HistoryKind::Export);
        log.clear();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        log.record_undoable(HistoryKind::Remove, 1);
        assert_eq!(log.entries()[0].op_id, 0);
    }
}
