//! App-side document state: the layer stack plus its undo journal, and
//! the egui panels that view/drive them (Layers + History).
//!
//! Deliberately NOT here: the quick mask (`umber_brush::quick_mask`, owned
//! by `PaintState`). It is temporary session state, so it stays out of
//! the layer journal, the autosave journal and the project file. Mask
//! strokes, invert and clear are never undo entries.

use std::path::{Path, PathBuf};

use fluent::FluentValue;
use umber_core::journal::{Journal, JournalOp};
use umber_core::layers::{Layer, LayerCommand, LayerKind, LayerStack};
use umber_core::undo::UndoStack;

use crate::history_panel::{HistoryKind, SessionHistory};
use crate::i18n::{tr, tr_args};

/// The open document: layer stack + undo history + autosave journal.
///
/// Wave 2 scope: one texture-set layer stack; multi-set documents and
/// the paint-channel binding to GPU targets arrive with the tile pool.
pub struct Document {
    pub stack: LayerStack,
    pub history: UndoStack<LayerCommand>,
    /// The session history log (the History window's model): one row per
    /// [`run`](Self::run), mirroring the undo journal, plus context rows
    /// via [`record_session_event`](Self::record_session_event). RAM-only,
    /// never persisted — see `history_panel`'s module docs.
    pub session_history: SessionHistory,
    /// The autosave journal (umber_core::journal), attached once the
    /// document has a project directory (after a Save or an Open). A
    /// never-saved document has nowhere to journal to and is not
    /// journaled — the v1 boundary.
    journal: Option<DocumentJournal>,
    /// The high-water mark of the project last attached, kept apart from
    /// the journal so a failed attach can't make the next save stamp a
    /// *lower* `journal_seq` (which would re-arm already-saved entries).
    journal_high_water: u64,
}

/// An attached journal plus the texture set this document's stack is.
struct DocumentJournal {
    journal: Journal,
    texture_set: String,
    dir: PathBuf,
}

/// Undo journal capacity (entries). Sized for long sessions; the memory
/// budget system (bytes) arrives with GPU-tile snapshots.
const HISTORY_MAX_ENTRIES: usize = 256;

impl Default for Document {
    fn default() -> Self {
        Self {
            stack: LayerStack::default(),
            history: UndoStack::new(HISTORY_MAX_ENTRIES),
            session_history: SessionHistory::new(),
            journal: None,
            journal_high_water: 0,
        }
    }
}

impl Document {
    /// Runs `command` through the journal (applies + records for undo),
    /// then appends the applied edit to the autosave journal — and records
    /// the History window's row from the same command.
    pub fn run(&mut self, command: LayerCommand) {
        // The cursor before the push: a push clears the redo stack, so any
        // history rows past this depth describe a future that no longer
        // exists and must go (context rows survive — see `truncate_future`).
        let cursor = self.history.len();
        self.history.push(command, &mut self.stack);
        self.session_history.truncate_future(cursor);
        let depth = self.history.len();
        // `peek_undo` is `Some` exactly after a push; a `None` records no
        // row (better a missing row than a mislabeled jumpable one).
        if let Some(kind) = self.history.peek_undo().map(HistoryKind::from_command) {
            self.session_history.record_undoable(kind, depth);
        }
        let op = self
            .history
            .peek_undo()
            .and_then(|c| JournalOp::applied(c, &self.stack));
        self.autosave(op);
    }

    /// One-level undo; returns whether anything was undone. The
    /// autosave journal gets the edit the undo performed (read before
    /// the revert, which consumes the command's captured state).
    pub fn undo(&mut self) -> bool {
        let op = self
            .history
            .peek_undo()
            .and_then(|c| JournalOp::reverting(c, &self.stack));
        if !self.history.undo(&mut self.stack) {
            return false;
        }
        self.autosave(op);
        true
    }

    /// One-level redo; returns whether anything was redone.
    pub fn redo(&mut self) -> bool {
        if !self.history.redo(&mut self.stack) {
            return false;
        }
        let op = self
            .history
            .peek_undo()
            .and_then(|c| JournalOp::applied(c, &self.stack));
        self.autosave(op);
        true
    }

    /// Click-to-time-travel: moves the document to undo depth `target` by
    /// stepping through [`undo`](Self::undo)/[`redo`](Self::redo) — a loop
    /// of single steps, not a truncate shortcut, so the document passes
    /// through each intermediate state and the autosave journal records
    /// every intermediate edit. Saturates at depth 0 and at the top of the
    /// redo stack. Returns how many steps were actually taken.
    ///
    /// Why not delegate to `UndoStack::jump`: the reverting autosave op
    /// must be read *before* each undo (revert consumes the command's
    /// captured state) and the applied op *after* each redo, so each step
    /// must go through this type's chokepoints. The step sequence is
    /// identical to `UndoStack::jump`'s — the history tests assert the
    /// equivalence.
    pub fn jump_to_depth(&mut self, target: usize) -> usize {
        let mut steps = 0;
        while self.history.len() > target {
            if !self.undo() {
                break;
            }
            steps += 1;
        }
        while self.history.len() < target {
            if !self.redo() {
                break;
            }
            steps += 1;
        }
        steps
    }

    /// Records one non-undoable session event (a finished bake, an export,
    /// a v1 brush stroke) as a context row in the History window. Display
    /// only: the row carries no depth and the panel never jumps to it.
    pub fn record_session_event(&mut self, kind: HistoryKind) {
        self.session_history.record_context(kind);
    }

    /// Appends `op` to the attached autosave journal, if any. A failed
    /// append is logged, not fatal: the journal is an optimization over
    /// losing the edits since the last save, never a gate on editing.
    fn autosave(&mut self, op: Option<JournalOp>) {
        let (Some(attached), Some(op)) = (self.journal.as_mut(), op) else {
            return;
        };
        if let Err(err) = attached.journal.record(&attached.texture_set, op) {
            log::warn!("autosave journal append failed: {err}");
        }
    }

    /// Attaches the autosave journal of the project at `dir` (whose
    /// `project.json` was just saved or loaded with `journal_seq` ==
    /// `high_water`); this document's stack is its `texture_set`.
    ///
    /// Replaces any previously attached journal. When that one lived in a
    /// different directory (Save As), it is truncated: its entries are
    /// in the state just saved to `dir`, and left behind they would
    /// "recover" into the old project on its next open.
    pub fn attach_journal(
        &mut self,
        dir: &Path,
        texture_set: &str,
        high_water: u64,
    ) -> std::io::Result<()> {
        self.journal_high_water = high_water;
        if let Some(old) = self.journal.take() {
            if old.dir != dir {
                if let Err(err) = umber_core::journal::truncate_journal(&old.dir) {
                    log::warn!(
                        "previous autosave journal not cleared ({}): {err}",
                        old.dir.display()
                    );
                }
            }
        }
        self.journal = Some(DocumentJournal {
            journal: Journal::open(dir, high_water)?,
            texture_set: texture_set.to_string(),
            dir: dir.to_path_buf(),
        });
        Ok(())
    }

    /// The high-water mark a save of this document stamps into
    /// `project.json`: the attached journal's last seq, never below the
    /// mark of the project last attached (0 for a never-saved document).
    pub fn journal_seq(&self) -> u64 {
        self.journal
            .as_ref()
            .map_or(0, |j| j.journal.last_seq())
            .max(self.journal_high_water)
    }

    /// Replaces the document's layer stack wholesale — the
    /// Open-Project path. The journal holds ids and indices captured
    /// against the *old* stack, so it is cleared with the swap:
    /// undo history is session-scoped by design and does not follow
    /// the file across an open. The autosave journal is detached too —
    /// it belongs to the previous project's directory; the caller
    /// attaches the loaded project's ([`Self::attach_journal`]).
    pub fn load_stack(&mut self, stack: LayerStack) {
        self.history.clear();
        self.session_history.clear();
        self.journal = None;
        self.journal_high_water = 0;
        self.stack = stack;
    }

    /// Layers bottom-to-top (stack order == draw order).
    pub fn layers(&self) -> &[Layer] {
        &self.stack.layers
    }

    /// Approximate in-memory size of the document in bytes (layer metadata
    /// only — see [`umber_core::layers::LayerStack::memory_bytes`]).
    ///
    /// An honest approximation, documented, not the §12 budget: the undo
    /// soak test samples this after each push to assert sub-linear growth
    /// plus an empirical ceiling (2x the observed size, recorded in the
    /// test output — the §12 ~2GB @ 4K target needs tile accounting first).
    pub fn in_memory_bytes(&self) -> usize {
        self.stack.memory_bytes()
    }
}

/// Draws the Layers panel: per-layer visibility toggle, name, kind badge.
///
/// Instance layers (§3:41) render as `instance of <source>` rows: the
/// source name comes from the paint gate itself
/// ([`LayerStack::check_paintable`](umber_core::LayerStack::check_paintable) —
/// an instance row exists exactly when painting it is refused), and the
/// row carries the "paint the source" hint. Every paintable layer offers
/// a ⧉ button that instances it on top (undoable, through the journal).
pub fn layers_ui(ui: &mut egui::Ui, doc: &mut Document) {
    use umber_core::InstanceError;

    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("layers.add-paint")).clicked() {
            let n = doc.layers().len() + 1;
            let name = tr_args("layers.default-paint-name", [("n", FluentValue::from(n))]);
            doc.run(LayerCommand::add(name, LayerKind::Paint));
        }
        if ui.button(tr("layers.add-fill")).clicked() {
            let n = doc.layers().len() + 1;
            let name = tr_args("layers.default-fill-name", [("n", FluentValue::from(n))]);
            doc.run(LayerCommand::add(name, LayerKind::Fill));
        }
    });
    let mut remove_id = None;
    let mut instance_source_id = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Snapshot the display rows first so command runs below can't
        // borrow-conflict with the iteration (E0502).
        #[derive(Clone)]
        struct Row {
            id: u64,
            name: String,
            kind: String,
            visible: bool,
            /// `Some(source name)` for instance rows, `None` otherwise.
            instance_of: Option<String>,
        }
        let rows: Vec<Row> = doc
            .layers()
            .iter()
            .rev()
            .map(|layer| {
                let (kind, instance_of) = match layer.kind {
                    LayerKind::Paint => (tr("layers.kind-paint"), None),
                    LayerKind::Fill => (tr("layers.kind-fill"), None),
                    LayerKind::Folder { .. } => (tr("layers.kind-folder"), None),
                    LayerKind::Instance { source_id } => {
                        let source_name = match doc.stack.check_paintable(layer.id) {
                            Err(InstanceError::PaintOnInstance { source_id, .. }) => doc
                                .stack
                                .layer(source_id)
                                .map(|l| l.name.clone())
                                .unwrap_or_else(|| source_id.to_string()),
                            _ => source_id.to_string(),
                        };
                        let of = tr_args(
                            "layers.instance-of",
                            [("source", FluentValue::from(source_name.as_str()))],
                        );
                        (tr("layers.kind-instance"), Some(of))
                    }
                };
                Row {
                    id: layer.id,
                    name: layer.name.clone(),
                    kind,
                    visible: layer.visible,
                    instance_of: instance_of,
                }
            })
            .collect();
        for layer in &rows {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(layer.visible, if layer.visible { "👁" } else { "·" })
                    .clicked()
                {
                    doc.run(LayerCommand::set_visible(layer.id, !layer.visible));
                }
                if layer.instance_of.is_some() {
                    ui.label(&layer.name)
                        .on_hover_text(tr("layers.instance-paint-hint"));
                } else {
                    ui.label(&layer.name);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(layer.kind.as_str())
                            .small()
                            .color(egui::Color32::from_gray(140)),
                    );
                    if let Some(of) = &layer.instance_of {
                        ui.label(
                            egui::RichText::new(of.as_str())
                                .small()
                                .color(egui::Color32::from_gray(140)),
                        );
                    } else if ui.small_button(tr("layers.instance-this")).clicked() {
                        instance_source_id = Some(layer.id);
                    }
                    if ui.small_button("✕").clicked() {
                        remove_id = Some(layer.id);
                    }
                });
            });
        }
    });
    if let Some(id) = remove_id {
        doc.run(LayerCommand::remove(id));
    }
    if let Some(source) = instance_source_id {
        if let Some(src) = doc.stack.layer(source) {
            let name = tr_args(
                "layers.instance-name",
                [("source", FluentValue::from(src.name.as_str()))],
            );
            doc.run(LayerCommand::add(
                name,
                LayerKind::Instance { source_id: source },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_undo_roundtrip() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("base", LayerKind::Fill));
        assert_eq!(doc.layers().len(), 1);
        assert!(doc.undo());
        assert_eq!(doc.layers().len(), 0);
        assert!(doc.redo());
        assert_eq!(doc.layers().len(), 1);
    }

    #[test]
    fn fresh_document_is_empty() {
        let doc = Document::default();
        assert!(doc.layers().is_empty());
        assert!(!doc.history.can_undo());
        assert!(!doc.history.can_redo());
    }

    /// The Open-Project contract: the journal does not follow the
    /// file. Undo entries recorded against the previous document
    /// must never replay against the loaded stack — both stacks
    /// allocate layer ids from 0, so a stale `Add` revert would
    /// delete the loaded project's layer 0 by coincidental id.
    #[test]
    fn load_stack_clears_journal_no_stale_undo() {
        let mut doc = Document::default();
        // Arm the journal in document A: one Add, one Remove.
        doc.run(LayerCommand::add("A-base", LayerKind::Fill));
        doc.run(LayerCommand::add("A-detail", LayerKind::Paint));
        assert!(doc.history.can_undo());

        // Load document B's stack — the Open Project path.
        let mut b = LayerStack::new();
        let b0 = b.add_layer("B-base", LayerKind::Fill);
        let b1 = b.add_layer("B-top", LayerKind::Paint);
        doc.load_stack(b);

        // Journal is fresh: undo is a no-op, never replays A's Add.
        assert!(!doc.history.can_undo());
        assert!(!doc.history.can_redo());
        assert!(!doc.undo());
        let ids: Vec<u64> = doc.layers().iter().map(|l| l.id).collect();
        assert_eq!(
            ids,
            [b0, b1],
            "undo after open project must not touch the loaded stack"
        );
        // And the loaded stack still behaves: a new Add works and is
        // itself undoable.
        doc.run(LayerCommand::add("B-new", LayerKind::Paint));
        assert_eq!(doc.layers().len(), 3);
        assert!(doc.undo());
        assert_eq!(doc.layers().len(), 2);
    }

    /// Design test 1: an op through the app's chokepoints lands as a
    /// parseable, newline-terminated JSONL line with the entry shape —
    /// undo journaled as the edit it performed, not as "undo".
    #[test]
    fn document_ops_land_as_journal_lines() {
        use umber_core::journal::{read_journal, JournalEntry, JOURNAL_FILE_NAME};

        let dir =
            std::env::temp_dir().join(format!("umber-app-doc-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut doc = Document::default();
        doc.attach_journal(&dir, "Body", 0).unwrap();
        doc.run(LayerCommand::add("Paint 1", LayerKind::Paint));
        doc.run(LayerCommand::set_visible(0, false));
        assert!(doc.undo());
        assert!(doc.redo());

        let text = std::fs::read_to_string(dir.join(JOURNAL_FILE_NAME)).unwrap();
        assert!(text.ends_with('\n'));
        assert_eq!(text.lines().count(), 4);
        assert!(
            text.lines().next().unwrap().contains(r#""type":"add""#),
            "{text}"
        );
        let body = |seq: u64, op: JournalOp| JournalEntry {
            seq,
            texture_set: "Body".into(),
            op,
        };
        assert_eq!(
            read_journal(&dir).unwrap(),
            vec![
                body(
                    1,
                    JournalOp::Add {
                        id: 0,
                        name: "Paint 1".into(),
                        kind: LayerKind::Paint,
                    }
                ),
                body(
                    2,
                    JournalOp::SetVisible {
                        id: 0,
                        value: false
                    }
                ),
                body(3, JournalOp::SetVisible { id: 0, value: true }),
                body(
                    4,
                    JournalOp::SetVisible {
                        id: 0,
                        value: false
                    }
                ),
            ]
        );
        assert_eq!(doc.journal_seq(), 4);

        // An unattached document (the never-saved case) writes nothing.
        let mut loose = Document::default();
        loose.run(LayerCommand::add("x", LayerKind::Fill));
        assert_eq!(loose.journal_seq(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Red-state proof of the exact corruption `load_stack` closes:
    /// swapping the stack raw, journal untouched (the pre-fix
    /// `open_project` path), replays document A's `Add{id: 0}` revert
    /// as a deletion of the loaded stack's layer 0.
    #[test]
    fn raw_stack_swap_keeps_stale_journal_and_corrupts() {
        let mut doc = Document::default();
        // Document A: one Add — the journal holds `Add{id: Some(0)}`.
        doc.run(LayerCommand::add("A-only", LayerKind::Fill));

        // Document B loaded via the RAW swap the fix replaces.
        let mut b = LayerStack::new();
        let b0 = b.add_layer("B-keep-me", LayerKind::Fill);
        let b1 = b.add_layer("B-top", LayerKind::Paint);
        doc.stack = b;

        // One undo click on the stale journal: Add.revert deletes by
        // id 0 — which is now document B's innocent bottom layer.
        assert!(doc.undo());
        assert_eq!(
            doc.layers().iter().map(|l| l.id).collect::<Vec<_>>(),
            [b1],
            "pre-fix path: stale Add (id 0) revert silently deleted the loaded layer {b0}"
        );
    }

    /// History-window model, recording order: every `run` appends one row
    /// mirroring the pushed command (kind + depth), in order, with
    /// monotonic op ids — interleaved with context rows that carry no
    /// depth.
    #[test]
    fn history_rows_mirror_each_run_in_order() {
        use crate::history_panel::HistoryKind;

        let mut doc = Document::default();
        doc.run(LayerCommand::add("Base", LayerKind::Paint));
        doc.record_session_event(HistoryKind::Bake);
        doc.run(LayerCommand::set_visible(0, false));
        doc.run(LayerCommand::add("Fill 2", LayerKind::Fill));

        let rows = doc.session_history.entries();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![
                HistoryKind::AddPaint,
                HistoryKind::Bake,
                HistoryKind::SetVisible,
                HistoryKind::AddFill,
            ]
        );
        assert_eq!(
            rows.iter().map(|e| e.op_id).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            rows.iter().map(|e| e.undo_depth).collect::<Vec<_>>(),
            vec![Some(1), None, Some(2), Some(3)]
        );
    }

    /// History-window jump math: three ops recorded, jumping to depth 0
    /// performs exactly 3 undos and restores the pre-session stack; jumping
    /// back up redoes through the same states single steps would.
    #[test]
    fn jump_to_depth_travels_by_n_undos_and_redos() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", LayerKind::Paint));
        doc.run(LayerCommand::add("B", LayerKind::Fill));
        doc.run(LayerCommand::set_visible(0, false));
        assert_eq!(doc.layers().len(), 2);

        // Depth 3 -> depth 0 is 3 undos.
        assert_eq!(doc.jump_to_depth(0), 3);
        assert!(doc.layers().is_empty());
        assert_eq!(doc.history.len(), 0);

        // Depth 0 -> depth 2 is 2 redos; the hidden flag is back.
        assert_eq!(doc.jump_to_depth(2), 2);
        assert_eq!(doc.layers().len(), 2);
        assert!(!doc.layers()[0].visible);
        assert_eq!(doc.history.len(), 2);

        // Already there: no steps.
        assert_eq!(doc.jump_to_depth(2), 0);
    }

    /// Journal-truncate equivalence, both halves:
    /// (a) a jump lands the document in exactly the state stepwise
    /// undo/redo reaches (same layers, same undo depth) — the jump is N
    /// single steps, never a truncate shortcut;
    /// (b) a fresh push after undos truncates the redo future in the
    /// journal AND drops its history rows (context rows survive).
    #[test]
    fn jump_matches_stepwise_and_push_truncates_dead_future() {
        use crate::history_panel::HistoryKind;

        fn drive(target: usize) -> Document {
            let mut doc = Document::default();
            doc.run(LayerCommand::add("A", LayerKind::Paint));
            doc.run(LayerCommand::add("B", LayerKind::Fill));
            doc.run(LayerCommand::set_visible(0, false));
            doc.jump_to_depth(target);
            doc
        }
        fn step(target: usize) -> Document {
            let mut doc = Document::default();
            doc.run(LayerCommand::add("A", LayerKind::Paint));
            doc.run(LayerCommand::add("B", LayerKind::Fill));
            doc.run(LayerCommand::set_visible(0, false));
            while doc.history.len() > target {
                assert!(doc.undo());
            }
            while doc.history.len() < target {
                assert!(doc.redo());
            }
            doc
        }
        for target in 0..=3usize {
            let (jumped, stepped) = (drive(target), step(target));
            assert_eq!(jumped.stack, stepped.stack, "layers at depth {target}");
            assert_eq!(jumped.history.len(), stepped.history.len());
        }

        // Half (b): undo once (depth 3 -> 2), record a bake for context,
        // then push a fresh op — the redo future truncates in both logs.
        let mut doc = drive(3);
        assert!(doc.undo());
        doc.record_session_event(HistoryKind::Bake);
        assert!(doc.history.can_redo());
        doc.run(LayerCommand::set_visible(1, false));
        assert!(!doc.history.can_redo(), "fresh push clears the redo stack");
        assert_eq!(
            doc.session_history
                .entries()
                .iter()
                .map(|e| (e.kind, e.undo_depth))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::AddPaint, Some(1)),
                (HistoryKind::AddFill, Some(2)),
                // Wall-clock context survives the truncate.
                (HistoryKind::Bake, None),
                // The dead depth-3 SetVisible row is gone; the fresh op
                // reuses depth 3 (cursor 2 + 1).
                (HistoryKind::SetVisible, Some(3)),
            ]
        );
    }

    /// Context rows are display-only at the Document level too: they carry
    /// no jump target, so the panel can never route a click on them into
    /// `jump_to_depth`.
    #[test]
    fn context_rows_carry_no_jump_target() {
        use crate::history_panel::HistoryKind;

        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", LayerKind::Paint));
        doc.record_session_event(HistoryKind::Bake);
        doc.record_session_event(HistoryKind::Export);
        doc.record_session_event(HistoryKind::BrushStroke);

        let targets: Vec<Option<usize>> = doc
            .session_history
            .entries()
            .iter()
            .map(|e| e.jump_target())
            .collect();
        assert_eq!(targets, vec![Some(1), None, None, None]);
    }

    /// The Open-Project contract covers the history log too: loading a new
    /// stack clears its rows (and restarts op ids), so stale depths can
    /// never route a click into the new document's journal.
    #[test]
    fn load_stack_clears_session_history() {
        use crate::history_panel::HistoryKind;

        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", LayerKind::Fill));
        doc.record_session_event(HistoryKind::Bake);
        assert_eq!(doc.session_history.len(), 2);

        let mut b = LayerStack::new();
        b.add_layer("B-base", LayerKind::Fill);
        doc.load_stack(b);

        assert!(doc.session_history.is_empty());
        doc.run(LayerCommand::add("B-new", LayerKind::Paint));
        assert_eq!(doc.session_history.entries()[0].op_id, 0);
        assert_eq!(doc.session_history.entries()[0].undo_depth, Some(1));
    }
}
