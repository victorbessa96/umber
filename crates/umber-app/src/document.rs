//! App-side document state: the layer stack plus its undo journal, and
//! the egui panels that view/drive them (Layers + History).

use std::path::{Path, PathBuf};

use umber_core::journal::{Journal, JournalOp};
use umber_core::layers::{Layer, LayerCommand, LayerKind, LayerStack};
use umber_core::undo::UndoStack;

/// The open document: layer stack + undo history + autosave journal.
///
/// Wave 2 scope: one texture-set layer stack; multi-set documents and
/// the paint-channel binding to GPU targets arrive with the tile pool.
pub struct Document {
    pub stack: LayerStack,
    pub history: UndoStack<LayerCommand>,
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
            journal: None,
            journal_high_water: 0,
        }
    }
}

impl Document {
    /// Runs `command` through the journal (applies + records for undo),
    /// then appends the applied edit to the autosave journal.
    pub fn run(&mut self, command: LayerCommand) {
        self.history.push(command, &mut self.stack);
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
pub fn layers_ui(ui: &mut egui::Ui, doc: &mut Document) {
    if ui.button("＋ Add paint layer").clicked() {
        let n = doc.layers().len() + 1;
        doc.run(LayerCommand::add(format!("Paint {n}"), LayerKind::Paint));
    }
    let mut remove_id = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Snapshot the display rows first so command runs below can't
        // borrow-conflict with the iteration (E0502).
        #[derive(Clone)]
        struct Row {
            id: u64,
            name: String,
            kind: &'static str,
            visible: bool,
        }
        let rows: Vec<Row> = doc
            .layers()
            .iter()
            .rev()
            .map(|layer| Row {
                id: layer.id,
                name: layer.name.clone(),
                kind: match layer.kind {
                    LayerKind::Paint => "paint",
                    LayerKind::Fill => "fill",
                    LayerKind::Folder { .. } => "folder",
                },
                visible: layer.visible,
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
                ui.label(&layer.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(layer.kind)
                            .small()
                            .color(egui::Color32::from_gray(140)),
                    );
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
}

/// Draws the History panel: journal size + undo/redo buttons.
pub fn history_ui(ui: &mut egui::Ui, doc: &mut Document) {
    ui.horizontal(|ui| {
        let can_undo = doc.history.can_undo();
        let can_redo = doc.history.can_redo();
        if ui
            .add_enabled(can_undo, egui::Button::new("↶ Undo"))
            .clicked()
        {
            doc.undo();
        }
        if ui
            .add_enabled(can_redo, egui::Button::new("↷ Redo"))
            .clicked()
        {
            doc.redo();
        }
    });
    ui.separator();
    // The journal doesn't expose entry labels yet (Wave 2 core keeps
    // commands opaque); show the live counts instead of a fake list.
    let n = doc.history.len();
    let entry_word = if n == 1 { "entry" } else { "entries" };
    ui.label(format!("{n} {entry_word} in journal"));
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
}
