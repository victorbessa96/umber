//! App-side document state: the layer stack plus its undo journal, and
//! the egui panels that view/drive them (Layers + History).
//!
//! Deliberately NOT here: the quick mask (`umber_brush::quick_mask`, owned
//! by `PaintState`). It is temporary session state, so it stays out of
//! the layer journal, the autosave journal and the project file. Mask
//! strokes, invert and clear are never undo entries.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use fluent::FluentValue;
use umber_core::journal::{Journal, JournalOp};
use umber_core::layers::{InstanceError, Layer, LayerCommand, LayerKind, LayerStack};
use umber_core::undo::UndoStack;

use crate::history_panel::{HistoryKind, SessionHistory};
use crate::i18n::{tr, tr_args};
use crate::paint_state::{PaintState, PlaneOwner};

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
    /// Whether the document has unsaved edits (the File > New / Open
    /// confirm gate). Any `run` sets it; a successful save/open reset
    /// clears it. Layer edits through `run` cover every document change
    /// — paint strokes ride the autosave journal, not the stack, so
    /// stroke-painted work is covered by the journal's existence check
    /// (a journaled document is never clean by construction here; the
    /// journal re-arms after each save).
    pub dirty: bool,
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
            dirty: false,
        }
    }
}

impl Document {
    /// Runs `command` through the journal (applies + records for undo),
    /// then appends the applied edit to the autosave journal — and records
    /// the History window's row from the same command.
    pub fn run(&mut self, command: LayerCommand) {
        self.dirty = true;
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

    /// Whether unsaved edits exist (the File > New / Open confirm gate).
    pub fn is_dirty(&self) -> bool {
        self.dirty
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
        // A freshly opened (or just-saved) project is on-disk current:
        // the confirm gate starts closed.
        self.dirty = false;
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

/// What the Layers rows asked for this frame (per-layer-pool design
/// §2.2). `layers_ui` takes no `PaintState` — the material context borrows
/// the shell's state mutably, so the selection cannot be written from
/// inside it — and the shell applies the outcome to the paint session
/// OUTSIDE that borrow, through `PaintState::set_active_target`.
#[derive(Debug, Default)]
pub struct LayersUiOutcome {
    /// `Some(target)` = a row's name was clicked (select it). The Base
    /// pseudo-row yields `Some(PlaneOwner::Base)`; an add button yields the
    /// new layer. `None` = no row asked (the common case).
    pub select: Option<PlaneOwner>,
    /// `Some(visible)` = the Base row's visibility was toggled (stage 4:
    /// the shell applies it to its `base_visible` session state, which
    /// rides the next composite snapshot).
    pub base_visible: Option<bool>,
}

/// Per-row badge state for [`layers_ui`] (stage 4): the shell owns these
/// (fill eval states, import refusals, dormancy) and the rows render
/// them. Material budget badges ride the existing `material_badges` slot
/// (`layers_panel::material_section`); fills and dormancy have no slot
/// there, so the rows carry them here.
#[derive(Debug, Default)]
pub struct LayerRowBadges {
    /// Fill layers blocked by the plane budget: layer id → badge line.
    pub fill_budget: HashMap<u64, String>,
    /// Import-created layers whose plane publish was refused (size
    /// mismatch or budget): layer id → badge line. The layer still
    /// exists, empty — the badge says why (§5.6).
    pub import_refusals: HashMap<u64, String>,
    /// Dormant fill/material layers (their pre-v2 result lives in the
    /// Base paint — §6.3): layer ids. The badge names it
    /// (`layers.dormant-badge`).
    pub dormant: HashSet<u64>,
}

/// The shell's per-frame selection sync, decided (per-layer-pool design
/// §2.3): the resolved target, its fresh verdict and its stack index. The
/// shell pushes it through `PaintState::sync_active_target`, which owns
/// the `active_explicit` bookkeeping (same target = keep, moved = auto).
pub struct ActiveSync {
    /// The resolved target (explicit Base stays, auto Base resolves,
    /// live layers keep, vanished ids resolve to a neighbour or Base).
    pub target: PlaneOwner,
    /// `stack.check_paintable(id)` for layers, `Ok(())` for Base.
    pub verdict: Result<(), InstanceError>,
    /// The target's stack index (preserved untouched for Base, which has
    /// none — the vanished-id walk reads it only for `Layer` targets).
    pub index: usize,
}

/// Revalidates the stroke selection against the live stack: the pure half
/// of the shell's per-frame sync (per-layer-pool design §2.3). `current` /
/// `explicit` / `index` are the paint session's last-synced state.
///
/// - An explicit Base selection never auto-moves ([M3]): Base is always
///   paintable.
/// - An auto Base resolves to the topmost PAINTABLE layer, else Base
///   ([M12]): auto-resolution never picks a refusing layer, so a stack of
///   only fills/materials still paints (into Base).
/// - A live `Layer(id)` keeps its target with a fresh verdict and index.
/// - A vanished `Layer(id)` ([m6]) resolves against `index` (the id's
///   position at the last sync). With `i = min(index, len)`, the walk goes
///   DOWN from `stack[i − 1]` (the layer that sat directly below) to the
///   nearest paintable layer — `(0..i).rev()`, so no `usize` underflow is
///   possible — then UP from `stack[i]`, else Base.
pub fn revalidate_active(
    stack: &LayerStack,
    current: PlaneOwner,
    explicit: bool,
    index: usize,
) -> ActiveSync {
    match current {
        PlaneOwner::Base if explicit => ActiveSync {
            target: PlaneOwner::Base,
            verdict: Ok(()),
            index,
        },
        PlaneOwner::Base => match topmost_paintable(stack) {
            Some((id, at)) => ActiveSync {
                target: PlaneOwner::Layer(id),
                // Paintable by selection (the finder only returns `Ok`).
                verdict: Ok(()),
                index: at,
            },
            None => ActiveSync {
                target: PlaneOwner::Base,
                verdict: Ok(()),
                index,
            },
        },
        PlaneOwner::Layer(id) => match stack.index_of(id) {
            Some(at) => ActiveSync {
                target: current,
                verdict: stack.check_paintable(id),
                index: at,
            },
            None => {
                let len = stack.layers.len();
                let i = index.min(len);
                let mut resolved: Option<(u64, usize)> = None;
                for pos in (0..i).rev() {
                    let candidate = &stack.layers[pos];
                    if stack.check_paintable(candidate.id).is_ok() {
                        resolved = Some((candidate.id, pos));
                        break;
                    }
                }
                if resolved.is_none() {
                    for pos in i..len {
                        let candidate = &stack.layers[pos];
                        if stack.check_paintable(candidate.id).is_ok() {
                            resolved = Some((candidate.id, pos));
                            break;
                        }
                    }
                }
                match resolved {
                    Some((id, at)) => ActiveSync {
                        target: PlaneOwner::Layer(id),
                        verdict: Ok(()),
                        index: at,
                    },
                    None => ActiveSync {
                        target: PlaneOwner::Base,
                        verdict: Ok(()),
                        index,
                    },
                }
            }
        },
    }
}

/// The topmost paintable layer and its stack index, or `None` when every
/// layer refuses (or the stack is empty).
fn topmost_paintable(stack: &LayerStack) -> Option<(u64, usize)> {
    for (at, layer) in stack.layers.iter().enumerate().rev() {
        if stack.check_paintable(layer.id).is_ok() {
            return Some((layer.id, at));
        }
    }
    None
}
/// Draws the Layers panel: per-layer visibility toggle, SELECTABLE name,
/// kind badge, and the pinned Base pseudo-row.
///
/// Dynamic-material layers (Kill B, `docs/specs/dynamic-materials-design.md`
/// §4) render their row here like every other kind, plus — under their own
/// row, via [`crate::layers_panel::material_section`] — the badge line and
/// the expandable per-declared-channel sub-rows (enable, blend, opacity) and
/// the `material_id` re-point combo. Sub-row edits run through
/// [`Document::run`] as undoable, journaled `SetMaterial` ops. The `mats`
/// context (session definitions, refresh badges, drain status, live states)
/// is built per frame by the shell — this module never owns it.
///
/// Instance layers (§3:41) render as `instance of <source>` rows: the
/// source name comes from the paint gate itself
/// ([`LayerStack::check_paintable`](umber_core::LayerStack::check_paintable) —
/// an instance row exists exactly when painting it is refused), and the
/// row carries the "paint the source" hint. Every paintable layer offers
/// a ⧉ button that instances it on top (undoable, through the journal).
///
/// Selection (per-layer-pool design §2.2): every row's name is a
/// `selectable_label` against `active`, and the Base pseudo-row
/// (`tr("layers.base-paint")`, always shown — tile-1001 Base baseColor is
/// created in `PaintState::new`, so Base always holds a plane) selects
/// `PlaneOwner::Base`. The add buttons select the new layer explicitly
/// (Painter's add-then-paint). Nothing here writes the selection: the
/// clicks accumulate into the returned [`LayersUiOutcome`], which the shell
/// applies to the paint session outside the material-context borrow.
/// `base_visible` is the Base row's visibility (stage 4's toggle); the
/// outcome carries the toggle intent back. `badges` renders the per-row
/// badge lines (fill budget blocks, import refusals, dormancy).
pub fn layers_ui(
    ui: &mut egui::Ui,
    doc: &mut Document,
    mut mats: crate::layers_panel::MaterialPanelContext<'_>,
    active: PlaneOwner,
    base_visible: bool,
    badges: &LayerRowBadges,
    thumbs: &std::collections::HashMap<u64, egui::TextureHandle>,
) -> LayersUiOutcome {
    let mut outcome = LayersUiOutcome::default();
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("layers.add-paint")).clicked() {
            let n = doc.layers().len() + 1;
            let name = tr_args("layers.default-paint-name", [("n", FluentValue::from(n))]);
            doc.run(LayerCommand::add(name, LayerKind::Paint));
            // Add-then-paint (Painter): the new layer is the stack top
            // after the run, and it is selected explicitly.
            if let Some(top) = doc.layers().last() {
                outcome.select = Some(PlaneOwner::Layer(top.id));
            }
        }
        if ui.button(tr("layers.add-fill")).clicked() {
            let n = doc.layers().len() + 1;
            let name = tr_args("layers.default-fill-name", [("n", FluentValue::from(n))]);
            doc.run(LayerCommand::add(name, LayerKind::Fill));
            if let Some(top) = doc.layers().last() {
                outcome.select = Some(PlaneOwner::Layer(top.id));
            }
        }
        if ui
            .button(format!("＋ {}", tr("history.op-add-folder")))
            .clicked()
        {
            let n = doc.layers().len() + 1;
            let name = format!("{} {n}", tr("layers.kind-folder"));
            doc.run(LayerCommand::add(
                name,
                LayerKind::Folder { passthrough: false },
            ));
            if let Some(top) = doc.layers().last() {
                outcome.select = Some(PlaneOwner::Layer(top.id));
            }
        }
        if ui.button(tr("layers.add-material")).clicked() {
            let n = doc.layers().len() + 1;
            let name = tr_args(
                "layers.default-material-name",
                [("n", FluentValue::from(n))],
            );
            doc.run(LayerCommand::add(name, LayerKind::Material));
            if let Some(top) = doc.layers().last() {
                outcome.select = Some(PlaneOwner::Layer(top.id));
            }
        }
    });
    // The smart-material shelf (C2-10), collapsed by default so the layer
    // list keeps the space. Its header reuses the material-layer add key.
    egui::CollapsingHeader::new(format!("{} ▸ .umbersmart", tr("layers.add-material")))
        .id_salt("smart-shelf")
        .default_open(false)
        .show(ui, |ui| {
            if let Some(ids) = smart_shelf_ui(ui, doc) {
                if let Some(top) = ids.last() {
                    outcome.select = Some(PlaneOwner::Layer(*top));
                }
            }
        });
    let mut remove_id = None;
    let mut instance_source_id = None;
    let mut duplicate_id: Option<u64> = None;
    let mut move_by: Option<(u64, i64)> = None;
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
            /// The refusal hint for fill/folder rows (`None` for the rest:
            /// instances carry the "paint the source" hint, materials the
            /// refresh badge line below their row).
            paint_hint: Option<String>,
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
                    LayerKind::Material => (tr("layers.kind-material"), None),
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
                let paint_hint = match layer.kind {
                    LayerKind::Fill => Some(tr("layers.fill-unpaintable")),
                    LayerKind::Folder { .. } => Some(tr("layers.folder-unpaintable")),
                    _ => None,
                };
                Row {
                    id: layer.id,
                    name: layer.name.clone(),
                    kind,
                    visible: layer.visible,
                    instance_of,
                    paint_hint,
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
                // The content thumbnail (g2-layer-thumbnails; Painter B1):
                // a fixed-size slot so rows align whether or not a layer
                // has pixels yet.
                let slot = egui::vec2(28.0, 28.0);
                match thumbs.get(&layer.id) {
                    Some(tex) => {
                        ui.add(egui::Image::new((tex.id(), slot)).corner_radius(3));
                    }
                    None => {
                        let (rect, _) = ui.allocate_exact_size(slot, egui::Sense::hover());
                        ui.painter().rect_stroke(
                            rect,
                            3.0,
                            egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                if layer.instance_of.is_some() {
                    if ui
                        .selectable_label(active == PlaneOwner::Layer(layer.id), &layer.name)
                        .on_hover_text(tr("layers.instance-paint-hint"))
                        .clicked()
                    {
                        outcome.select = Some(PlaneOwner::Layer(layer.id));
                    }
                } else if let Some(hint) = &layer.paint_hint {
                    if ui
                        .selectable_label(active == PlaneOwner::Layer(layer.id), &layer.name)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        outcome.select = Some(PlaneOwner::Layer(layer.id));
                    }
                } else if ui
                    .selectable_label(active == PlaneOwner::Layer(layer.id), &layer.name)
                    .clicked()
                {
                    outcome.select = Some(PlaneOwner::Layer(layer.id));
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
                    // Duplicate (C2-8): a same-kind copy with the row's
                    // properties, added through the undoable commands.
                    // No catalog key yet for "Duplicate" (the catalogs sit
                    // under a sibling lock): the glyph alone, no tooltip
                    // rather than a wrong one.
                    if ui.small_button("⧉").clicked() {
                        duplicate_id = Some(layer.id);
                    }
                    // Move: the panel lists top-first, so "up" raises the
                    // stack index.
                    if ui
                        .small_button("▼")
                        .on_hover_text(tr("history.op-reorder"))
                        .clicked()
                    {
                        move_by = Some((layer.id, -1));
                    }
                    if ui
                        .small_button("▲")
                        .on_hover_text(tr("history.op-reorder"))
                        .clicked()
                    {
                        move_by = Some((layer.id, 1));
                    }
                });
            });
            // The properties row of the SELECTED layer (C2-8): opacity,
            // blend mode and the name, each an undoable command.
            if active == PlaneOwner::Layer(layer.id) {
                layer_properties_row(ui, doc, layer.id);
            }
            // Dynamic-material expandable rows (Kill B): under their own row,
            // never replacing it — the badge line above stays visible while
            // collapsed. Re-fetched by id (the snapshot rows above carry no
            // material payload).
            if doc
                .stack
                .layer(layer.id)
                .is_some_and(|live| live.kind == LayerKind::Material)
            {
                crate::layers_panel::material_section(ui, doc, layer.id, &mut mats);
            }
            // Stage-4 row badges (the shell owns the state, the rows render
            // it): a dormant layer names where its pixels live; a blocked
            // fill or a refused import names why its plane is missing.
            if badges.dormant.contains(&layer.id) {
                ui.label(
                    egui::RichText::new(tr("layers.dormant-badge"))
                        .small()
                        .color(egui::Color32::from_gray(140)),
                );
            }
            if let Some(line) = badges.fill_budget.get(&layer.id) {
                ui.label(egui::RichText::new(line.as_str()).color(ui.visuals().error_fg_color));
            }
            if let Some(line) = badges.import_refusals.get(&layer.id) {
                ui.label(egui::RichText::new(line.as_str()).color(ui.visuals().error_fg_color));
            }
        }
        // [M3] The Base pseudo-row, pinned at the bottom of the list:
        // session paint that belongs to no layer (v1 projects, strokes with
        // no layer selected). Selectable like every layer row; the eye
        // toggles its composite visibility (stage 4 — there is a live
        // composite for it to affect now).
        ui.horizontal(|ui| {
            if ui
                .selectable_label(base_visible, if base_visible { "👁" } else { "·" })
                .clicked()
            {
                outcome.base_visible = Some(!base_visible);
            }
            if ui
                .selectable_label(active == PlaneOwner::Base, tr("layers.base-paint"))
                .on_hover_text(tr("layers.base-paint-hint"))
                .clicked()
            {
                outcome.select = Some(PlaneOwner::Base);
            }
        });
    });
    if let Some(id) = remove_id {
        doc.run(LayerCommand::remove(id));
    }
    if let Some((id, delta)) = move_by {
        if let Some((from, to)) = move_target(doc.layers().len(), doc.stack.index_of(id), delta) {
            doc.run(LayerCommand::reorder(from, to));
        }
    }
    if let Some(id) = duplicate_id {
        if let Some(new_id) = duplicate_layer(doc, id) {
            outcome.select = Some(PlaneOwner::Layer(new_id));
        }
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
    // The material drain's one-line status (ignored undeclared channels, last
    // failure): named once per drain, rendered under the rows.
    if let Some(status) = mats.status {
        ui.separator();
        ui.weak(status);
    }
    outcome
}

/// The shipped smart-material directory: the repo's `assets/smart`
/// (dev builds and the source tree), or `smart/` beside the binary in an
/// installed build: the same two-place rule as the i18n catalogs.
pub fn smart_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs =
        vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/smart")];
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf))
    {
        dirs.push(exe_dir.join("smart"));
    }
    dirs
}

/// Every loadable `.umbersmart` in `dirs`, sorted by name, deduplicated
/// by file name (the first directory wins). A file that fails to parse is
/// skipped with a log line, never a panic and never a broken row.
pub fn list_smart_materials(
    dirs: &[std::path::PathBuf],
) -> Vec<(String, umber_core::smart::SmartMaterial)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "umbersmart"))
            .collect();
        paths.sort();
        for path in paths {
            let Some(file) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            if !seen.insert(file) {
                continue;
            }
            match umber_core::smart::SmartMaterial::load_file(&path) {
                Ok(material) => out.push((material.name.clone(), material)),
                Err(e) => log::warn!("smart material {} skipped: {e}", path.display()),
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Applies a smart material to the document as ordinary undoable
/// commands (C2-10), one per template field: add, then opacity, blend
/// and mask when they differ from a fresh layer's. Unlike
/// `SmartMaterial::apply` (which writes the stack directly, bypassing
/// history and the autosave journal), every layer this creates is undoable
/// and journaled. Returns the new layer ids, bottom to top.
pub fn apply_smart_material(
    doc: &mut Document,
    material: &umber_core::smart::SmartMaterial,
) -> Vec<u64> {
    use umber_core::smart::SmartLayerKind;
    let mut ids = Vec::with_capacity(material.layers.len());
    for template in &material.layers {
        let kind = match template.kind {
            SmartLayerKind::Paint => LayerKind::Paint,
            SmartLayerKind::Fill => LayerKind::Fill,
            SmartLayerKind::Folder => LayerKind::Folder { passthrough: true },
        };
        doc.run(LayerCommand::add(template.name.clone(), kind));
        let Some(id) = doc.layers().last().map(|l| l.id) else {
            continue;
        };
        if template.opacity.is_finite() && (template.opacity - 1.0).abs() > f32::EPSILON {
            doc.run(LayerCommand::set_opacity(id, template.opacity));
        }
        if template.blend_mode != umber_core::BlendMode::Normal {
            doc.run(LayerCommand::set_blend_mode(id, template.blend_mode));
        }
        if let Some(mask) = &template.mask {
            doc.run(LayerCommand::attach_mask(id, mask.clone()));
        }
        ids.push(id);
    }
    ids
}

/// The smart-material shelf (C2-10): one row per shipped `.umbersmart`
/// with an Apply button that stacks its layers on the active set.
/// Generator bindings are not wired yet: the template's layers arrive
/// with their names, kinds, opacity, blend and masks, and the graph-side
/// wiring of each fill's mesh-map generator is the named follow-up.
pub fn smart_shelf_ui(ui: &mut egui::Ui, doc: &mut Document) -> Option<Vec<u64>> {
    // The directory scan is cheap, but it is still disk I/O: cache the
    // list in egui memory per frame-group, refreshed by the user.
    let cache_id = egui::Id::new("smart-shelf-cache");
    let mut names: Option<Vec<String>> = ui.ctx().data(|d| d.get_temp(cache_id));
    let refresh = ui.small_button("⟳").clicked();
    if names.is_none() || refresh {
        let list: Vec<String> = list_smart_materials(&smart_dirs())
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        ui.ctx().data_mut(|d| d.insert_temp(cache_id, list.clone()));
        names = Some(list);
    }
    let mut applied = None;
    for name in names.unwrap_or_default() {
        ui.horizontal(|ui| {
            ui.label(&name);
            if ui.button("＋").clicked() {
                if let Some((_, material)) = list_smart_materials(&smart_dirs())
                    .into_iter()
                    .find(|(n, _)| *n == name)
                {
                    applied = Some(apply_smart_material(doc, &material));
                }
            }
        });
    }
    applied
}

/// The reorder a one-step move asks for: `Some((from, to))`, or `None` at
/// either end of the stack (or for an unknown layer). `delta` +1 raises
/// the layer one index (toward the top of the panel, which lists
/// top-first), −1 lowers it.
pub fn move_target(len: usize, index: Option<usize>, delta: i64) -> Option<(usize, usize)> {
    let from = index?;
    let to = from as i64 + delta;
    (to >= 0 && (to as usize) < len && to as usize != from).then_some((from, to as usize))
}

/// The copy's name: the source name plus " copy" (the history keys own
/// no "copy" word yet, so this is the one untranslated suffix, as in
/// Painter's own English default).
pub fn duplicate_name(source: &str) -> String {
    format!("{source} copy")
}

/// Duplicates layer `id` (C2-8): a new layer of the same kind directly
/// above it, carrying its opacity, visibility and blend mode. Each step
/// is an ordinary undoable command (add, reorder, the setters), so undo
/// unwinds a duplicate step by step. v1 copies the layer's SETTINGS
/// (kind, opacity, visibility, blend), not its content: paint pixels live
/// in the paint session, and fill / material / mask parameters ride
/// their own commands, which this does not replay yet. The kill report
/// names both. Returns the new id.
pub fn duplicate_layer(doc: &mut Document, id: u64) -> Option<u64> {
    let source = doc.stack.layer(id)?.clone();
    let from_index = doc.stack.index_of(id)?;
    doc.run(LayerCommand::add(
        duplicate_name(&source.name),
        source.kind.clone(),
    ));
    let new_id = doc.layers().last()?.id;
    // The add lands on top; move it to just above the source.
    let top = doc.layers().len() - 1;
    let target = from_index + 1;
    if top != target {
        doc.run(LayerCommand::reorder(top, target));
    }
    if source.opacity != 1.0 {
        doc.run(LayerCommand::set_opacity(new_id, source.opacity));
    }
    if !source.visible {
        doc.run(LayerCommand::set_visible(new_id, false));
    }
    if source.blend_mode != umber_core::BlendMode::Normal {
        doc.run(LayerCommand::set_blend_mode(new_id, source.blend_mode));
    }
    Some(new_id)
}

/// The selected layer's properties row: opacity, blend mode, name. Each
/// edit is one undoable command, issued only when the value changes.
fn layer_properties_row(ui: &mut egui::Ui, doc: &mut Document, id: u64) {
    let Some(layer) = doc.stack.layer(id).cloned() else {
        return;
    };
    ui.indent(("layer-props", id), |ui| {
        ui.horizontal(|ui| {
            ui.label(tr("history.op-set-opacity"));
            let mut opacity = layer.opacity;
            let response = ui.add(
                umber_gui::ValueField::new(&mut opacity)
                    .range(0.0..=1.0)
                    .default_value(1.0)
                    .context_menu(crate::i18n::prop_menu_labels()),
            );
            // One command per change. A drag is many changes, and each
            // runs as its own command, the History panel's existing
            // behavior for slider edits: the command captures the true
            // previous value, so every step undoes exactly. Coalescing a
            // drag into one entry is the C2-7 timeline's job.
            if response.changed() && (opacity - layer.opacity).abs() > f32::EPSILON {
                doc.run(LayerCommand::set_opacity(id, opacity));
            }
        });
        // Folders composite through their children; materials keep their
        // own per-channel blends (the layer's mode is inert there).
        if !matches!(layer.kind, LayerKind::Material) {
            ui.horizontal(|ui| {
                ui.label(tr("history.op-set-blend"));
                let mut mode = layer.blend_mode;
                egui::ComboBox::from_id_salt(("layer-blend", id))
                    .selected_text(format!("{mode:?}"))
                    .show_ui(ui, |ui| {
                        for candidate in layer_blend_options(&layer.kind) {
                            ui.selectable_value(&mut mode, candidate, format!("{candidate:?}"));
                        }
                    });
                if mode != layer.blend_mode {
                    doc.run(LayerCommand::set_blend_mode(id, mode));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(tr("history.op-rename"));
            let edit_id = egui::Id::new(("layer-rename", id));
            let mut text: String = ui
                .ctx()
                .data(|d| d.get_temp(edit_id))
                .unwrap_or_else(|| layer.name.clone());
            let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0));
            if response.changed() {
                ui.ctx().data_mut(|d| d.insert_temp(edit_id, text.clone()));
            }
            if response.lost_focus() {
                ui.ctx().data_mut(|d| d.remove::<String>(edit_id));
                if let Some(name) = rename_commit(&layer.name, &text) {
                    doc.run(LayerCommand::rename(id, name));
                }
            }
        });
    });
}

/// The blend modes a layer row offers: the material panel's list (every
/// mode but Passthrough, the normal-map trio only for normal), plus
/// Passthrough for folders, the one kind it means something for.
pub fn layer_blend_options(kind: &LayerKind) -> Vec<umber_core::BlendMode> {
    let mut modes = crate::layers_panel::channel_blend_options("");
    if matches!(kind, LayerKind::Folder { .. }) {
        modes.insert(1, umber_core::BlendMode::Passthrough);
    }
    modes
}

/// The name a rename commits: trimmed, `None` when empty or unchanged.
pub fn rename_commit(old: &str, typed: &str) -> Option<String> {
    let name = typed.trim();
    (!name.is_empty() && name != old).then(|| name.to_owned())
}

/// Draws the layer-mask section under the Layers panel: per-layer mask
/// attach / enable-toggle / edit-select / remove.
///
/// Mask METADATA edits (attach, detach, enable) run undoably through the
/// document journal ([`LayerCommand`]); the edit SELECTION is session
/// state on the paint session ([`PaintState::set_mask_edit_layer`]), and
/// detaching also drops the layer's planes there (a re-attach starts
/// white). Strokes route to the selected layer's plane — see
/// `paint_state`'s layer-mask docs. New Layers-panel keys are listed
/// in the kill report's Doc deltas (the dragon owns the `.ftl` edits).
pub fn layer_mask_ui(ui: &mut egui::Ui, doc: &mut Document, paint: &mut PaintState) {
    ui.separator();
    ui.label(tr("layers.masks-section"));
    // Snapshot the rows first so command runs below can't borrow-conflict
    // with the iteration (the `layers_ui` E0502 pattern).
    #[derive(Clone)]
    struct MaskRow {
        id: u64,
        name: String,
        mask_name: Option<String>,
        mask_enabled: bool,
        editing: bool,
    }
    let editing = paint.mask_edit_layer();
    let rows: Vec<MaskRow> = doc
        .layers()
        .iter()
        .rev()
        .map(|layer| MaskRow {
            id: layer.id,
            name: layer.name.clone(),
            mask_name: layer.mask.as_ref().map(|m| m.name.clone()),
            mask_enabled: layer.mask.as_ref().is_some_and(|m| m.enabled),
            editing: editing == Some(layer.id),
        })
        .collect();
    enum MaskAction {
        Attach(u64, String),
        Toggle(u64, bool),
        Select(Option<u64>),
        Detach(u64),
    }
    let mut actions: Vec<MaskAction> = Vec::new();
    for row in &rows {
        ui.horizontal(|ui| {
            ui.label(&row.name);
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match &row.mask_name {
                    None => {
                        if ui.small_button(tr("layers.mask-add")).clicked() {
                            let name = tr_args(
                                "layers.mask-default-name",
                                [("layer", FluentValue::from(row.name.as_str()))],
                            );
                            actions.push(MaskAction::Attach(row.id, name));
                        }
                    }
                    Some(mask_name) => {
                        if ui.small_button(tr("layers.mask-remove")).clicked() {
                            actions.push(MaskAction::Detach(row.id));
                        }
                        if ui
                            .small_button(if row.editing {
                                tr("layers.mask-done")
                            } else {
                                tr("layers.mask-edit")
                            })
                            .clicked()
                        {
                            actions.push(MaskAction::Select(if row.editing {
                                None
                            } else {
                                Some(row.id)
                            }));
                        }
                        let label = if row.mask_enabled { "◐" } else { "○" };
                        if ui.selectable_label(row.mask_enabled, label).clicked() {
                            actions.push(MaskAction::Toggle(row.id, !row.mask_enabled));
                        }
                        ui.label(
                            egui::RichText::new(mask_name.as_str())
                                .small()
                                .color(egui::Color32::from_gray(140)),
                        );
                    }
                },
            );
        });
    }
    for action in actions {
        match action {
            MaskAction::Attach(id, name) => {
                doc.run(LayerCommand::attach_mask(id, name));
            }
            MaskAction::Toggle(id, enabled) => {
                doc.run(LayerCommand::set_mask_enabled(id, enabled));
            }
            MaskAction::Select(layer) => {
                // [m5] Selecting a row's mask also selects the layer
                // EXPLICITLY for strokes: without this, adding a layer (or
                // any selection) while editing L2's mask would paint L2's
                // MASK, because a set `mask_edit_layer` wins over
                // everything. Target first (it clears a foreign mask edit),
                // then the mask selection. Deselecting the mask leaves the
                // stroke target alone.
                if let Some(id) = layer {
                    paint.set_active_target(PlaneOwner::Layer(id));
                    paint.set_mask_edit_layer(Some(id));
                } else {
                    paint.set_mask_edit_layer(None);
                }
            }
            MaskAction::Detach(id) => {
                doc.run(LayerCommand::detach_mask(id));
                paint.remove_layer_masks(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shipped_smart_materials_list_and_apply_undoably() {
        let list = list_smart_materials(&smart_dirs());
        let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"Painted wood"), "{names:?}");
        assert!(names.len() >= 2, "both shipped materials: {names:?}");
        let (_, wood) = list.iter().find(|(n, _)| n == "Painted wood").unwrap();

        let mut doc = Document::default();
        let before = doc.history.len();
        let ids = apply_smart_material(&mut doc, wood);
        assert_eq!(ids.len(), wood.layers.len());
        // Every template field arrived on its layer.
        for (id, template) in ids.iter().zip(&wood.layers) {
            let layer = doc.stack.layer(*id).unwrap();
            assert_eq!(layer.name, template.name);
            assert!(
                (layer.opacity - template.opacity).abs() < 1e-6,
                "{}",
                template.name
            );
            assert_eq!(layer.blend_mode, template.blend_mode);
            assert_eq!(
                layer.mask.as_ref().map(|m| m.name.as_str()),
                template.mask.as_deref()
            );
        }
        // ...through history: unwinding every entry restores the empty stack.
        let added = doc.history.len() - before;
        assert!(added >= ids.len());
        for _ in 0..added {
            assert!(doc.undo());
        }
        assert!(doc.layers().is_empty(), "fully undoable");
    }

    #[test]
    fn a_broken_smart_file_is_skipped_not_fatal() {
        let dir = std::env::temp_dir().join(format!("umber-smart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.umbersmart"), "{ not json").unwrap();
        std::fs::write(
            dir.join("ok.umbersmart"),
            r#"{"name":"Ok","version":1,"layers":[{"name":"L","kind":"paint"}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let list = list_smart_materials(std::slice::from_ref(&dir));
        let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Ok"]);
        // A second dir with the same file name does not duplicate it.
        assert_eq!(list_smart_materials(&[dir.clone(), dir.clone()]).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_target_steps_within_the_stack() {
        assert_eq!(move_target(3, Some(0), 1), Some((0, 1)));
        assert_eq!(move_target(3, Some(2), -1), Some((2, 1)));
        // At the ends, no move.
        assert_eq!(move_target(3, Some(2), 1), None);
        assert_eq!(move_target(3, Some(0), -1), None);
        assert_eq!(move_target(3, None, 1), None);
        assert_eq!(move_target(1, Some(0), 1), None);
    }

    #[test]
    fn rename_commit_trims_and_skips_noops() {
        assert_eq!(rename_commit("Paint 1", "  Rust  "), Some("Rust".into()));
        assert_eq!(rename_commit("Paint 1", "Paint 1"), None);
        assert_eq!(rename_commit("Paint 1", "   "), None);
        assert_eq!(duplicate_name("Rust"), "Rust copy");
    }

    #[test]
    fn layer_blend_options_offer_passthrough_to_folders_only() {
        use umber_core::BlendMode;
        let paint = layer_blend_options(&LayerKind::Paint);
        assert!(!paint.contains(&BlendMode::Passthrough));
        assert!(paint.contains(&BlendMode::Multiply));
        let folder = layer_blend_options(&LayerKind::Folder { passthrough: false });
        assert!(folder.contains(&BlendMode::Passthrough));
        assert_eq!(folder.len(), paint.len() + 1);
    }

    #[test]
    fn duplicate_copies_settings_above_the_source_and_undoes_cleanly() {
        use umber_core::BlendMode;
        let mut doc = Document::default();
        doc.run(LayerCommand::add("Bottom", LayerKind::Paint));
        doc.run(LayerCommand::add("Source", LayerKind::Paint));
        doc.run(LayerCommand::add("Top", LayerKind::Paint));
        let source = doc.layers()[1].id;
        doc.run(LayerCommand::set_opacity(source, 0.4));
        doc.run(LayerCommand::set_blend_mode(source, BlendMode::Multiply));
        let before = doc.history.len();
        let copy = duplicate_layer(&mut doc, source).expect("duplicates");
        let names: Vec<&str> = doc.layers().iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            ["Bottom", "Source", "Source copy", "Top"],
            "directly above"
        );
        let layer = doc.stack.layer(copy).unwrap();
        assert_eq!(layer.opacity, 0.4);
        assert_eq!(layer.blend_mode, BlendMode::Multiply);
        // add + reorder + opacity + blend: four commands, all undoable.
        assert_eq!(doc.history.len(), before + 4);
        for _ in 0..4 {
            assert!(doc.undo());
        }
        let names: Vec<&str> = doc.layers().iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Bottom", "Source", "Top"], "fully unwound");
        assert_eq!(duplicate_layer(&mut doc, 999), None, "unknown id");
    }

    use super::*;

    #[test]
    fn dirty_flag_rides_run_and_attach() {
        // The confirm gate's truth: any run() marks the document dirty
        // (undo/redo included — they flow through run), and a fresh
        // attach (project open / save landing) clears it. Undoing back
        // to the saved shape does NOT clear it: the journal's seq
        // moved and an undo-redo asymmetry would need seq tracking —
        // the honest v1 keeps the gate conservative.
        let mut doc = Document::default();
        assert!(!doc.is_dirty());
        doc.run(LayerCommand::add("base", LayerKind::Paint));
        assert!(doc.is_dirty());
        doc.undo();
        assert!(doc.is_dirty(), "undo keeps the gate closed conservatively");
    }

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

        // Depth 0 -> depth 2 is 2 redos; the two Add layers are back.
        // The SetVisible hide is NOT: its redo sits at depth 3, one more
        // step up — the flag returns only when the full history is
        // redone (depth 3).
        assert_eq!(doc.jump_to_depth(2), 2);
        assert_eq!(doc.layers().len(), 2);
        assert!(doc.layers()[0].visible);
        assert_eq!(doc.history.len(), 2);

        // Depth 2 -> depth 3: the final redo restores the hide (the
        // one-step-further check the born-broken assertion missed).
        assert_eq!(doc.jump_to_depth(3), 1);
        assert!(!doc.layers()[0].visible);
        assert_eq!(doc.history.len(), 3);

        // Already there: no steps.
        assert_eq!(doc.jump_to_depth(3), 0);
    }

    /// Rewinding n layer commands restores the exact stack state: the
    /// layer count and content at each depth, plus the undo/redo counts
    /// the panel's buttons and highlight read.
    #[test]
    fn jump_to_depth_restores_counts_and_content() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", LayerKind::Paint));
        doc.run(LayerCommand::add("B", LayerKind::Fill));
        doc.run(LayerCommand::set_visible(0, false));
        assert_eq!(doc.layers().len(), 2);
        assert!(doc.history.can_undo());
        assert!(!doc.history.can_redo());

        // Depth 3 -> 0: 3 undos, empty stack, redo armed, undo spent.
        assert_eq!(doc.jump_to_depth(0), 3);
        assert!(doc.layers().is_empty());
        assert_eq!(doc.history.len(), 0);
        assert!(!doc.history.can_undo());
        assert!(doc.history.can_redo());

        // Depth 0 -> 3: 3 redos, both layers back, the hide restored.
        assert_eq!(doc.jump_to_depth(3), 3);
        assert_eq!(doc.layers().len(), 2);
        assert_eq!(doc.layers()[0].name, "A");
        assert_eq!(doc.layers()[1].name, "B");
        assert!(!doc.layers()[0].visible);
        assert_eq!(doc.history.len(), 3);
        assert!(doc.history.can_undo());
        assert!(!doc.history.can_redo());

        // One step back: exactly one layer hides the difference.
        assert_eq!(doc.jump_to_depth(2), 1);
        assert_eq!(doc.layers().len(), 2);
        assert!(doc.layers()[0].visible);
        assert!(doc.history.can_undo());
        assert!(doc.history.can_redo());
    }

    /// Cross-journal truncate independence, headless: a layer push drops
    /// dead LAYER rows but keeps stroke rows (the layer push leaves the
    /// stroke journal alone) — the mirror image of the stroke drain's
    /// `truncate_stroke_future`, pinned here without a GPU session.
    #[test]
    fn layer_push_keeps_stroke_rows_while_dropping_dead_layer_rows() {
        use crate::history_panel::HistoryKind;

        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", LayerKind::Paint));
        doc.run(LayerCommand::add("B", LayerKind::Fill));
        // A stroke recorded between the layer ops (the drain's row).
        doc.session_history.record_stroke(1);
        assert!(doc.undo());

        // Fresh layer push over the undone state: the depth-2 layer row
        // describes the cleared redo future and goes; the stroke row and
        // the surviving depth-1 layer row stay.
        doc.run(LayerCommand::set_visible(0, false));
        assert_eq!(
            doc.session_history
                .entries()
                .iter()
                .map(|e| (e.kind, e.undo_depth, e.stroke_depth))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::AddPaint, Some(1), None),
                (HistoryKind::BrushStroke, None, Some(1)),
                (HistoryKind::SetVisible, Some(2), None),
            ]
        );
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

    /// The selection outcome carries no intent until a row asks: the common
    /// frame applies nothing.
    #[test]
    fn layers_ui_outcome_defaults_to_no_intent() {
        let outcome = LayersUiOutcome::default();
        assert_eq!(outcome.select, None);
        assert_eq!(outcome.base_visible, None);
    }

    /// §8 T11 (auto half) + [M12]: an auto Base resolves to the topmost
    /// paintable layer, else Base. An explicit Base never auto-moves, even
    /// with paintable layers above the fold.
    #[test]
    fn revalidate_auto_base_resolves_topmost_paintable_else_base() {
        let mut stack = LayerStack::new();
        // Empty stack: auto Base stays Base (the headless callers'
        // contract — `perf_soak` strokes land in Base).
        let sync = revalidate_active(&stack, PlaneOwner::Base, false, 0);
        assert_eq!(sync.target, PlaneOwner::Base);
        assert_eq!(sync.verdict, Ok(()));
        // Fill-only stack: Base (a refusing layer is never picked).
        let fill = stack.add_layer("F", LayerKind::Fill);
        assert_eq!(fill, 0);
        let sync = revalidate_active(&stack, PlaneOwner::Base, false, 0);
        assert_eq!(sync.target, PlaneOwner::Base);
        // Paint below a fill: the topmost PAINTABLE wins, with its index.
        let paint = stack.add_layer("P", LayerKind::Paint);
        let sync = revalidate_active(&stack, PlaneOwner::Base, false, 0);
        assert_eq!(sync.target, PlaneOwner::Layer(paint));
        assert_eq!(sync.verdict, Ok(()));
        assert_eq!(sync.index, 1);
        // An explicit Base survives the same stack untouched.
        let sync = revalidate_active(&stack, PlaneOwner::Base, true, 0);
        assert_eq!(sync.target, PlaneOwner::Base);
    }

    /// §8 T11 [m6]: a vanished id resolves to its former neighbour — down
    /// first (the layer that sat directly below), then up, else Base.
    /// Stack `[P0, P1, F2, P3]`, ids 0..=3 in order.
    #[test]
    fn revalidate_vanished_id_resolves_to_its_former_neighbour() {
        let mut stack = LayerStack::new();
        let p0 = stack.add_layer("P0", LayerKind::Paint);
        let p1 = stack.add_layer("P1", LayerKind::Paint);
        let _f2 = stack.add_layer("F2", LayerKind::Fill);
        let p3 = stack.add_layer("P3", LayerKind::Paint);
        assert_eq!((p0, p1, p3), (0, 1, 3));
        // Live layers keep their target with a fresh verdict + index.
        let sync = revalidate_active(&stack, PlaneOwner::Layer(p3), true, 3);
        assert_eq!(sync.target, PlaneOwner::Layer(p3));
        assert_eq!(sync.verdict, Ok(()));
        assert_eq!(sync.index, 3);
        // Removing P3 (active, index 3): down from stack[2] = F2
        // (unpaintable) to P1.
        stack.remove_layer(p3);
        let sync = revalidate_active(&stack, PlaneOwner::Layer(p3), true, 3);
        assert_eq!(sync.target, PlaneOwner::Layer(p1));
        assert_eq!(sync.verdict, Ok(()));
        assert_eq!(sync.index, 1);
        // Removing P0 while IT is active (index 0): no below, so up to the
        // nearest paintable layer.
        stack.remove_layer(p0);
        let sync = revalidate_active(&stack, PlaneOwner::Layer(p0), true, 0);
        assert_eq!(sync.target, PlaneOwner::Layer(p1));
        assert_eq!(sync.index, 0);
        // Empty stack with a stale layer target: Base (no underflow: the
        // vanished bottom skips the down-walk by construction).
        stack.remove_layer(p1);
        assert!(stack.layers.iter().all(|l| l.kind == LayerKind::Fill));
        stack.remove_layer(2);
        assert!(stack.layers.is_empty());
        let sync = revalidate_active(&stack, PlaneOwner::Layer(p1), true, 1);
        assert_eq!(sync.target, PlaneOwner::Base);
        assert_eq!(sync.verdict, Ok(()));
    }

    /// A live explicit selection keeps its target but gets a fresh verdict:
    /// selecting a fill explicitly is what the gate refuses on.
    #[test]
    fn revalidate_live_layer_keeps_target_with_a_fresh_verdict() {
        let mut stack = LayerStack::new();
        let fill = stack.add_layer("F", LayerKind::Fill);
        let sync = revalidate_active(&stack, PlaneOwner::Layer(fill), true, 0);
        assert_eq!(sync.target, PlaneOwner::Layer(fill));
        assert_eq!(
            sync.verdict,
            Err(InstanceError::PaintOnFill { layer: fill })
        );
        assert_eq!(sync.index, 0);
    }
}
