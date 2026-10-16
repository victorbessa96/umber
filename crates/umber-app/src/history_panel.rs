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
//! truncate shortcut. Non-undoable session events (bakes, exports, and
//! brush strokes recorded without a stroke depth) carry `undo_depth: None`
//! for timeline context: they render but never jump.
//!
//! Stroke rows travel the same way on the SECOND journal: every committed
//! brush stroke queues one
//! [`StrokeCommit`](crate::paint_state::StrokeCommit) on the paint session
//! (the only chokepoint that sees the stroke journal on both sides of the
//! push), the app's per-frame
//! [`drain_stroke_commits`](drain_stroke_commits) turns commits into
//! `BrushStroke` rows carrying `stroke_depth`, and clicking one calls the
//! session's n-single-steps loop
//! ([`PaintState::jump_to_stroke_depth`](crate::paint_state::PaintState::jump_to_stroke_depth)
//! — housed on the session rather than here so both crate targets drive
//! and test the same loop). A merged stroke shares its predecessor's depth
//! (one undo step, one shared target — the merged-entry semantics); an
//! identity stroke queues nothing and leaves no row.
//!
//! One timeline (C2-7, 2026-10-15): the row ORDER (one `op_id` sequence
//! across both journals) IS the timeline. Clicking any row travels BOTH
//! journals to that moment ([`travel_to`]): every row up to it applied,
//! every row after it undone. A layer row rewinds the strokes painted
//! after it, and a stroke row the layer ops after it. The Undo/Redo
//! buttons step the same timeline one row at a time ([`unified_undo`] /
//! [`unified_redo`]). The `history.travel-note` key, which described the
//! old per-journal boundary, is no longer rendered.
//!
//! Capacity note: the log itself is unbounded (rows are a kind + small
//! integers), while the layer journal is capped at 256 entries and the
//! stroke journal at 64 entries / 256 MiB. Past either cap the journal
//! evicts its oldest command, so the oldest rows' depths no longer exist
//! in the journal — clicking one saturates at the journal's bottom instead
//! of failing (the jump loops stop when the single step returns `false`).
//! A resize clears the stroke journal wholesale (the epoch bump): the next
//! drain drops every stroke row, since old depths would alias the NEW
//! history's entries. Rows are otherwise never rewritten to hide staleness:
//! the log is the session's honest timeline, and saturation is the
//! documented behavior.
//!
//! GUI Phase 1 kill 2 (docs/specs/gui-audit-and-roadmap.md, the
//! panel-migration wave): the panel's sections render as
//! [`umber_gui::ParamGroup`] collapsibles over the same buttons, labels
//! and rows as before. Chrome only: no model, jump math, or row text
//! changed, and the layer-only model tests below are untouched by it.
//!
//! Stroke click-to-time-travel (this kill, requirements §11's
//! click-to-time-travel row): `HistoryEntry` gains the stroke journal's
//! depth (`stroke_depth`), the drain/apply/highlight helpers below, and the
//! panel's paint-session half; the n-steps loop itself lives on the session
//! (`PaintState::jump_to_stroke_depth`, so both crate targets test one
//! loop). The layer-only rows, their jump math and their tests behave
//! exactly as before.
//!
//! Deliberately NOT migrated (v1 boundaries of this pass):
//! - No [`umber_gui::ValueField`]: the panel has no editable numerics —
//!   the journal count and the `depth · label` row prefixes are read-only
//!   displays, and a scrub field with no write-back target would be
//!   decoration, not a widget.
//! - No theme-token color swaps: the panel authors no colors (context rows
//!   use stock `weak`, the current row uses stock `selectable_label`), so
//!   there is nothing hand-rolled to re-token. An accent highlight for the
//!   current-depth row awaits the app-wide `umber_style` wiring.
//! - The Layers panel UI lives in [`crate::document::layers_ui`], owned by
//!   a concurrent kill — read for the E0502 collect-then-act pattern, not
//!   edited here.
//! - Group ids (`history-controls`, `history-log`) are panel-local
//!   literals in the [`ParamGroup::new`] calls below.

use fluent::FluentValue;
use umber_core::layers::{LayerCommand, LayerKind};
use umber_gui::ParamGroup;

use crate::document::Document;
use crate::i18n::{tr, tr_args};
use crate::paint_state::{PaintState, StrokeCommit};

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
    /// [`LayerCommand::Add`] of a material layer.
    AddMaterial,
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
    /// [`LayerCommand::Rename`] (a layer renamed).
    Rename,
    /// [`LayerCommand::SetFill`].
    SetFill,
    /// [`LayerCommand::SetMaterial`] (a material layer's definition,
    /// channel controls or param overrides changed).
    SetMaterial,
    /// [`LayerCommand::SetInstanceSource`] (instance retarget).
    SetInstanceSource,
    /// [`LayerCommand::AttachMask`] (a layer gains a mask).
    AttachMask,
    /// [`LayerCommand::DetachMask`] (a layer loses its mask).
    DetachMask,
    /// [`LayerCommand::SetMaskEnabled`] (a mask flips enabled).
    SetMaskEnabled,
    /// A brush stroke committed by the paint session. Undoable when recorded
    /// with a stroke depth (the per-frame drain's commit): clicking the row
    /// drives the stroke journal to it. Context-recorded strokes (no depth)
    /// stay display-only, exactly like bakes and exports.
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
                LayerKind::Material => Self::AddMaterial,
                LayerKind::Instance { .. } => Self::AddInstance,
            },
            LayerCommand::Remove { .. } => Self::Remove,
            LayerCommand::Reorder { .. } => Self::Reorder,
            LayerCommand::SetOpacity { .. } => Self::SetOpacity,
            LayerCommand::SetVisible { .. } => Self::SetVisible,
            LayerCommand::SetBlendMode { .. } => Self::SetBlendMode,
            LayerCommand::Rename { .. } => Self::Rename,
            LayerCommand::SetFill { .. } => Self::SetFill,
            LayerCommand::SetMaterial { .. } => Self::SetMaterial,
            LayerCommand::SetInstanceSource { .. } => Self::SetInstanceSource,
            LayerCommand::AttachMask { .. } => Self::AttachMask,
            LayerCommand::DetachMask { .. } => Self::DetachMask,
            LayerCommand::SetMaskEnabled { .. } => Self::SetMaskEnabled,
        }
    }

    /// The i18n'd op name rendered in the row. Resolved at draw time (not
    /// stored) so a mid-session locale switch re-renders every row.
    pub fn label(self) -> String {
        match self {
            Self::AddPaint => tr("history.op-add-paint"),
            Self::AddFill => tr("history.op-add-fill"),
            Self::AddFolder => tr("history.op-add-folder"),
            Self::AddMaterial => tr("history.op-add-material"),
            Self::AddInstance => tr("history.op-add-instance"),
            Self::Remove => tr("history.op-remove"),
            Self::Reorder => tr("history.op-reorder"),
            Self::SetOpacity => tr("history.op-set-opacity"),
            Self::SetVisible => tr("history.op-set-visible"),
            Self::SetBlendMode => tr("history.op-set-blend"),
            Self::Rename => tr("history.op-rename"),
            Self::SetFill => tr("history.op-set-fill"),
            Self::SetMaterial => tr("history.op-set-material"),
            Self::SetInstanceSource => tr("history.op-set-instance-source"),
            Self::AttachMask => tr("history.op-attach-mask"),
            Self::DetachMask => tr("history.op-detach-mask"),
            Self::SetMaskEnabled => tr("history.op-set-mask-enabled"),
            Self::BrushStroke => tr("history.op-brush-stroke"),
            Self::Bake => tr("history.op-bake"),
            Self::Export => tr("history.op-export"),
        }
    }

    /// A fixed English name for crash reports (`crash::note_op`). Not
    /// [`Self::label`]: a report is read in any bug tracker, so it must not
    /// depend on the user's locale, and a panic hook must not run the i18n
    /// machinery. `&'static str` keeps user text out of reports.
    pub fn crash_label(self) -> &'static str {
        match self {
            Self::AddPaint => "add paint layer",
            Self::AddFill => "add fill layer",
            Self::AddFolder => "add folder",
            Self::AddMaterial => "add material layer",
            Self::AddInstance => "add instance layer",
            Self::Remove => "remove layer",
            Self::Reorder => "reorder layer",
            Self::SetOpacity => "set opacity",
            Self::SetVisible => "set visibility",
            Self::SetBlendMode => "set blend mode",
            Self::Rename => "rename layer",
            Self::SetFill => "edit fill",
            Self::SetMaterial => "edit material",
            Self::SetInstanceSource => "retarget instance",
            Self::AttachMask => "attach mask",
            Self::DetachMask => "detach mask",
            Self::SetMaskEnabled => "toggle mask",
            Self::BrushStroke => "brush stroke",
            Self::Bake => "bake",
            Self::Export => "export",
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
    /// The layer-journal depth to jump to when this row is clicked: the
    /// undo-stack length just after its op was pushed. `None` for stroke
    /// rows and context-only events, which never drive the layer journal.
    pub undo_depth: Option<usize>,
    /// The stroke-journal depth to jump to when this row is clicked: the
    /// stroke-stack length just after its stroke was pushed. `None` for
    /// layer rows and context-only events. A merged stroke shares its
    /// predecessor's depth (one undo step, one shared target).
    pub stroke_depth: Option<usize>,
}

impl HistoryEntry {
    /// The layer depth a click on this row travels to — `None` means the
    /// row is display-only for the layer journal and the panel issues no
    /// layer jump. Keyed off `undo_depth`, not `kind`, so any future
    /// context kind is display-only by construction.
    pub fn jump_target(&self) -> Option<usize> {
        self.undo_depth
    }

    /// The stroke depth a click on this row travels to — `None` means the
    /// row never drives the stroke journal. Keyed off `stroke_depth`, not
    /// `kind`, for the same reason as [`Self::jump_target`].
    pub fn stroke_target(&self) -> Option<usize> {
        self.stroke_depth
    }
}

/// The session history model: an append-only [`HistoryEntry`] log mirroring
/// the undo journal, plus context events. RAM-only, never persisted (see
/// the module docs).
#[derive(Debug, Default)]
pub struct SessionHistory {
    entries: Vec<HistoryEntry>,
    next_op_id: u64,
    /// The stroke-history generation this log's stroke rows were recorded
    /// against (see [`PaintState::stroke_epoch`](crate::paint_state::PaintState::stroke_epoch)).
    /// Starts at 0, matching a fresh paint session.
    stroke_epoch: u64,
}

impl SessionHistory {
    /// An empty session log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one undoable layer op at `depth` (the layer-journal length
    /// just after its push). Called from the same chokepoint that pushed
    /// the command. Never carries a stroke target: a layer push leaves the
    /// stroke journal (and its redo future) alone.
    pub fn record_undoable(&mut self, kind: HistoryKind, depth: usize) {
        self.entries.push(HistoryEntry {
            kind,
            op_id: self.next_op_id,
            undo_depth: Some(depth),
            stroke_depth: None,
        });
        self.next_op_id += 1;
    }

    /// Records one committed brush stroke at `depth` (the stroke-journal
    /// length just after its entry was pushed — the per-frame drain's
    /// commit depth). Called only from
    /// [`apply_stroke_commits`](apply_stroke_commits), never directly from
    /// the stroke path. Never carries a layer target.
    pub fn record_stroke(&mut self, depth: usize) {
        self.entries.push(HistoryEntry {
            kind: HistoryKind::BrushStroke,
            op_id: self.next_op_id,
            undo_depth: None,
            stroke_depth: Some(depth),
        });
        self.next_op_id += 1;
    }

    /// Records one non-undoable session event (bake, export, or a brush
    /// stroke with no entry behind it) for timeline context. Always stored
    /// with both depths `None`, whatever the kind — the panel renders it
    /// but never jumps to it.
    pub fn record_context(&mut self, kind: HistoryKind) {
        self.entries.push(HistoryEntry {
            kind,
            op_id: self.next_op_id,
            undo_depth: None,
            stroke_depth: None,
        });
        self.next_op_id += 1;
    }

    /// Drops the dead layer future after a fresh push over undone state: a
    /// push clears the journal's redo stack, so entries recorded past `cursor`
    /// (the undo depth just before the push) no longer have commands to
    /// redo to — clicking them would travel nowhere real. Stroke rows
    /// (`stroke_depth: Some`, `undo_depth: None`) and context rows survive:
    /// a layer push leaves the stroke journal alone.
    pub fn truncate_future(&mut self, cursor: usize) {
        self.entries
            .retain(|e| e.undo_depth.is_none_or(|d| d <= cursor));
    }

    /// Drops the dead stroke future after fresh strokes over undone stroke
    /// state: the stroke-side twin of [`Self::truncate_future`]. Layer rows
    /// and context rows survive: a stroke push leaves the layer journal
    /// alone.
    pub fn truncate_stroke_future(&mut self, cursor: usize) {
        self.entries
            .retain(|e| e.stroke_depth.is_none_or(|d| d <= cursor));
    }

    /// The stroke-history generation this log's stroke rows were recorded
    /// against.
    pub fn stroke_epoch(&self) -> u64 {
        self.stroke_epoch
    }

    /// Adopts `epoch`, dropping every stroke row when it changed: a resize
    /// clears the stroke journal wholesale, so old stroke depths would
    /// alias the NEW history's entries (clicking one would redo into
    /// unrelated commands). Layer and context rows survive — the layer
    /// journal is untouched by a resize. Returns whether the epoch changed.
    pub fn adopt_stroke_epoch(&mut self, epoch: u64) -> bool {
        if epoch == self.stroke_epoch {
            return false;
        }
        self.stroke_epoch = epoch;
        self.entries.retain(|e| e.stroke_depth.is_none());
        true
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

/// Whether `entry` is the live row: a layer row at the live layer depth, a
/// stroke row at the live stroke depth (`None` — no paint session — is
/// never current), context rows never. Pure so the highlight rule is
/// headless-testable; [`history_ui`] is the only caller.
pub fn entry_is_current(entry: &HistoryEntry, layer_len: usize, stroke_len: Option<usize>) -> bool {
    match (entry.jump_target(), entry.stroke_target()) {
        (Some(depth), None) => depth == layer_len,
        (None, Some(depth)) => stroke_len.is_some_and(|len| len == depth),
        // Context rows (neither depth) never highlight: a row that
        // constrains no journal is not a position. No dual-depth row exists
        // in v1 (see the module docs' mixed-journal boundary); if one ever
        // does, the follow-up defines its highlight there, not here.
        _ => false,
    }
}

/// How a history row reads (GUI plan G2 `g2-history-panel`; Blender
/// A10.1: "newest on top, a dot on the current state, greyed future").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// The journal position this row names is the live state.
    Current,
    /// Applied, behind the live state.
    Past,
    /// Undone: ahead of the live state on its journal. A new edit
    /// truncates it (the stack drops the redo branch).
    Future,
    /// Context (a bake, an export): no journal position.
    Context,
}

/// The row state from the entry's own journal depth and that journal's
/// live length. A layer row `d` is Future once the journal is shorter
/// than `d` (its edit undone), Current at exactly `d`, else Past. The
/// same for stroke rows against the stroke journal; with no paint session,
/// stroke rows read as context (nothing to drive).
pub fn row_state(entry: &HistoryEntry, layer_len: usize, stroke_len: Option<usize>) -> RowState {
    let classify = |depth: usize, len: usize| match depth.cmp(&len) {
        std::cmp::Ordering::Equal => RowState::Current,
        std::cmp::Ordering::Less => RowState::Past,
        std::cmp::Ordering::Greater => RowState::Future,
    };
    match (entry.jump_target(), entry.stroke_target()) {
        (Some(d), None) => classify(d, layer_len),
        (None, Some(d)) => stroke_len.map_or(RowState::Context, |len| classify(d, len)),
        _ => RowState::Context,
    }
}

/// Which journal one unified undo/redo step drives (C2-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Journal {
    /// The layer journal (`Document::undo` / `redo`).
    Layer,
    /// The paint session's stroke journal (`undo_stroke` / `redo_stroke`).
    Stroke,
}

/// The journal ONE undo steps back on the unified timeline (C2-7): the
/// one holding the NEWEST applied row, by `op_id` (the log's single
/// interleaving record of both journals). `None` when nothing applied
/// remains. Without a paint session, stroke rows are not candidates.
/// Pure, so the rule is headless-testable.
pub fn unified_undo_target(
    entries: &[HistoryEntry],
    layer_len: usize,
    stroke_len: Option<usize>,
) -> Option<Journal> {
    entries
        .iter()
        .rev()
        .find_map(|e| match (e.jump_target(), e.stroke_target()) {
            (Some(d), None) if d >= 1 && d <= layer_len => Some(Journal::Layer),
            (None, Some(d)) if stroke_len.is_some_and(|len| d >= 1 && d <= len) => {
                Some(Journal::Stroke)
            }
            _ => None,
        })
}

/// The journal ONE redo replays on the unified timeline: the one whose
/// next undone step is the OLDEST, by `op_id`, so a run of unified undos
/// replays in reverse exactly. A journal's next redo step is the first
/// row past its live length (`depth == len + 1`).
pub fn unified_redo_target(
    entries: &[HistoryEntry],
    layer_len: usize,
    stroke_len: Option<usize>,
) -> Option<Journal> {
    entries
        .iter()
        .find_map(|e| match (e.jump_target(), e.stroke_target()) {
            (Some(d), None) if d == layer_len + 1 => Some(Journal::Layer),
            (None, Some(d)) if stroke_len.is_some_and(|len| d == len + 1) => Some(Journal::Stroke),
            _ => None,
        })
}

/// One unified undo (C2-7): Ctrl+Z on ONE timeline across layer ops and
/// strokes. Drains pending stroke commits first, so a stroke painted this
/// frame is on the timeline before the choice is made. `false` when
/// nothing could be undone.
pub fn unified_undo(doc: &mut Document, mut paint: Option<&mut PaintState>) -> bool {
    if let Some(p) = paint.as_deref_mut() {
        drain_stroke_commits(doc, p);
    }
    let stroke_len = paint.as_ref().map(|p| p.stroke_len());
    match unified_undo_target(doc.session_history.entries(), doc.history.len(), stroke_len) {
        Some(Journal::Layer) => doc.undo(),
        Some(Journal::Stroke) => paint.is_some_and(|p| p.undo_stroke()),
        None => false,
    }
}

/// One unified redo, the inverse of [`unified_undo`].
pub fn unified_redo(doc: &mut Document, mut paint: Option<&mut PaintState>) -> bool {
    if let Some(p) = paint.as_deref_mut() {
        drain_stroke_commits(doc, p);
    }
    let stroke_len = paint.as_ref().map(|p| p.stroke_len());
    match unified_redo_target(doc.session_history.entries(), doc.history.len(), stroke_len) {
        Some(Journal::Layer) => doc.redo(),
        Some(Journal::Stroke) => paint.is_some_and(|p| p.redo_stroke()),
        None => false,
    }
}

/// Coordinated travel (C2-7): the journal lengths that put the timeline AT
/// the row with `op_id`: every row up to and including it applied, every
/// row after it undone. For each journal that is the depth of its last
/// row at or before `op_id` (0 when it has none there). `None` for an
/// unknown id or a context row (not a position).
pub fn travel_target(entries: &[HistoryEntry], op_id: u64) -> Option<(usize, usize)> {
    let row = entries.iter().find(|e| e.op_id == op_id)?;
    if row.jump_target().is_none() && row.stroke_target().is_none() {
        return None;
    }
    // Newest-first over the rows at or before the target: the first depth
    // found per journal is that journal's last one there.
    let mut upto = entries.iter().rev().filter(|e| e.op_id <= op_id);
    let layer = upto
        .clone()
        .find_map(HistoryEntry::jump_target)
        .unwrap_or(0);
    let stroke = upto.find_map(HistoryEntry::stroke_target).unwrap_or(0);
    Some((layer, stroke))
}

/// Travels the whole timeline to row `op_id`: drives BOTH journals to
/// [`travel_target`]'s lengths through their normal single-step paths
/// (the layer journal's `jump_to_depth`, the session's
/// `jump_to_stroke_depth`), so every intermediate edit is journaled.
/// Without a paint session only the layer journal moves. `false` for a
/// non-position row.
pub fn travel_to(doc: &mut Document, paint: Option<&mut PaintState>, op_id: u64) -> bool {
    let Some((layer, stroke)) = travel_target(doc.session_history.entries(), op_id) else {
        return false;
    };
    doc.jump_to_depth(layer);
    if let Some(session) = paint {
        session.jump_to_stroke_depth(stroke);
    }
    true
}

/// The per-frame stroke-commit drain: called from the app's frame loop with
/// the document AND the paint session — every frame, not only when the
/// History tab is visible (egui_dock skips hidden tabs), so strokes painted
/// while History is hidden still get rows. Takes the session's queued
/// commits and feeds them to [`apply_stroke_commits`].
pub fn drain_stroke_commits(doc: &mut Document, paint: &mut PaintState) {
    let epoch = paint.stroke_epoch();
    let commits = paint.take_stroke_commits();
    apply_stroke_commits(doc, epoch, commits);
}

/// Turns taken stroke commits into `BrushStroke` rows: adopts a new stroke
/// epoch first (dropping rows a resize evicted), then records one row per
/// commit — truncating the dead stroke-redo future at the commit's cursor
/// first, exactly like [`Document::run`] does for layers. Split from
/// [`drain_stroke_commits`] so the mapping is headless-testable without a
/// paint session: hand-made commits (including merged `cursor == depth`
/// pairs) pin the row math in this module's tests, while the GPU-backed
/// commit pairs are pinned in `paint_state`'s.
pub fn apply_stroke_commits(doc: &mut Document, epoch: u64, commits: Vec<StrokeCommit>) {
    doc.session_history.adopt_stroke_epoch(epoch);
    // A committed stroke is an unsaved edit: the quit / New / Open confirm
    // gates read `is_dirty`, and before this a stroke-only session read
    // clean, so closing the window dropped the paint with no prompt.
    if !commits.is_empty() {
        doc.dirty = true;
    }
    for commit in commits {
        doc.session_history.truncate_stroke_future(commit.cursor);
        doc.session_history.record_stroke(commit.depth);
    }
}

/// Draws the History panel: an undo/redo + journal-size group, then the
/// session-log group — oldest first, one row per entry (`depth · label`
/// for layer rows at their layer depth and stroke rows at their stroke
/// depth, weak `· label` for context rows), the row at each journal's live
/// depth highlighted. Clicking a layer row time-travels the document to its
/// depth; clicking a stroke row time-travels the paint session to its
/// stroke depth (each on its own — see the module docs' mixed-journal
/// boundary); context rows are display-only. Without a paint session,
/// stroke rows render display-only too: there is no journal to drive.
///
/// The egui drawing itself is untested by construction; the model (record
/// order, jump math, display-but-don't-jump, truncate equivalence,
/// highlight rule) is covered headless in this module's + `document`'s +
/// `paint_state`'s tests.
pub fn history_ui(ui: &mut egui::Ui, doc: &mut Document, mut paint: Option<&mut PaintState>) {
    let controls_title = tr("history.section-controls");
    ParamGroup::new("history-controls", controls_title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        ui.horizontal(|ui| {
            // ONE timeline (C2-7): the panel's Undo/Redo step whichever
            // journal holds the newest applied / oldest undone row.
            let stroke_len = paint.as_ref().map(|p| p.stroke_len());
            let entries = doc.session_history.entries();
            let can_undo = unified_undo_target(entries, doc.history.len(), stroke_len).is_some();
            let can_redo = unified_redo_target(entries, doc.history.len(), stroke_len).is_some();
            if ui
                .add_enabled(can_undo, egui::Button::new(tr("history.undo")))
                .clicked()
            {
                unified_undo(doc, paint.as_deref_mut());
            }
            if ui
                .add_enabled(can_redo, egui::Button::new(tr("history.redo")))
                .clicked()
            {
                unified_redo(doc, paint.as_deref_mut());
            }
        });
        let n = doc.history.len();
        ui.label(tr_args(
            "history.journal",
            [("count", FluentValue::from(n))],
        ));
    });
    let log_title = tr("history.section-log");
    ParamGroup::new("history-log", log_title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        if doc.session_history.is_empty() {
            ui.weak(tr("history.empty"));
            return;
        }
        let depth = doc.history.len();
        let stroke_len = paint.as_ref().map(|p| p.stroke_len());
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Snapshot the rows first so the click-jump below can't
            // borrow-conflict with the iteration (the layers panel's E0502
            // pattern): collect-then-act.
            struct Row {
                text: String,
                op_id: u64,
                layer_target: Option<usize>,
                stroke_target: Option<usize>,
                current: bool,
                state: RowState,
            }
            // Newest on top (Blender A10.1): the log reads like a stack,
            // with the most recent edit nearest the toolbar.
            let rows: Vec<Row> = doc
                .session_history
                .entries()
                .iter()
                .rev()
                .map(|entry| {
                    let current = entry_is_current(entry, depth, stroke_len);
                    let state = row_state(entry, depth, stroke_len);
                    match (entry.jump_target(), entry.stroke_target()) {
                        (Some(target), None) => Row {
                            text: format!("{target} · {}", entry.kind.label()),
                            op_id: entry.op_id,
                            layer_target: Some(target),
                            stroke_target: None,
                            current,
                            state,
                        },
                        // A stroke row without a paint session has no
                        // journal to drive: display-only, like context.
                        (None, Some(target)) if paint.is_some() => Row {
                            text: format!("{target} · {}", entry.kind.label()),
                            op_id: entry.op_id,
                            layer_target: None,
                            stroke_target: Some(target),
                            current,
                            state,
                        },
                        _ => Row {
                            text: format!("· {}", entry.kind.label()),
                            op_id: entry.op_id,
                            layer_target: None,
                            stroke_target: None,
                            current: false,
                            state: RowState::Context,
                        },
                    }
                })
                .collect();
            let mut travel: Option<u64> = None;
            for row in &rows {
                // The current row carries a dot; undone (future) rows are
                // greyed: a new edit would discard them.
                let label = |row: &Row| {
                    let text = match row.state {
                        RowState::Current => format!("● {}", row.text),
                        _ => format!("   {}", row.text),
                    };
                    match row.state {
                        RowState::Future => egui::RichText::new(text).weak().italics(),
                        _ => egui::RichText::new(text),
                    }
                };
                // Coordinated travel (C2-7): a click puts the WHOLE
                // timeline at this row, both journals at once.
                if row.layer_target.is_some() || row.stroke_target.is_some() {
                    if ui.selectable_label(row.current, label(row)).clicked() {
                        travel = Some(row.op_id);
                    }
                } else {
                    ui.label(egui::RichText::new(&row.text).weak());
                }
            }
            if let Some(op_id) = travel {
                travel_to(doc, paint, op_id);
            }
        });
        // `history.travel-note` described the two-journal boundary that
        // coordinated travel retired; it stays unused until the catalogs
        // (under a sibling lock) can drop or reword it.
    });
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_committed_stroke_makes_the_document_dirty() {
        // The quit guard's input: a stroke-only session must not read clean.
        let mut doc = Document::default();
        assert!(!doc.is_dirty());
        apply_stroke_commits(&mut doc, 0, Vec::new());
        assert!(!doc.is_dirty(), "no commits, still clean");
        apply_stroke_commits(
            &mut doc,
            0,
            vec![StrokeCommit {
                cursor: 0,
                depth: 1,
            }],
        );
        assert!(doc.is_dirty(), "a stroke is an unsaved edit");
    }

    #[test]
    fn travel_target_puts_both_journals_at_the_row() {
        // op 0 L1, op 1 S1, op 2 L2, op 3 S2, op 4 a bake.
        let mut log = mixed_log(&[(true, 1), (false, 1), (true, 2), (false, 2)]);
        log.push(HistoryEntry {
            kind: HistoryKind::Bake,
            op_id: 4,
            undo_depth: None,
            stroke_depth: None,
        });
        assert_eq!(travel_target(&log, 0), Some((1, 0)), "L1: no stroke yet");
        assert_eq!(travel_target(&log, 1), Some((1, 1)));
        assert_eq!(travel_target(&log, 2), Some((2, 1)), "L2 keeps S1");
        assert_eq!(travel_target(&log, 3), Some((2, 2)));
        assert_eq!(
            travel_target(&log, 4),
            None,
            "a context row is not a position"
        );
        assert_eq!(travel_target(&log, 99), None);
    }

    #[test]
    fn clicking_a_layer_row_rewinds_the_stroke_painted_after_it() {
        // GPU: First, a stroke, Second. Travelling to First must undo the
        // stroke too (the old per-journal click left it painted).
        let Some((device, queue)) = paint_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let mut paint = PaintState::with_target_size(device, queue, 64).expect("paint builds");
        let mut doc = Document::default();
        doc.run(LayerCommand::add("First", LayerKind::Paint));
        paint.begin_stroke(egui::pos2(0.2, 0.2));
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            paint.extend_stroke(egui::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        paint.end_stroke();
        paint.process_pending().expect("drains");
        drain_stroke_commits(&mut doc, &mut paint);
        doc.run(LayerCommand::add("Second", LayerKind::Paint));
        let first = doc.session_history.entries()[0].op_id;
        assert!(travel_to(&mut doc, Some(&mut paint), first));
        assert_eq!(
            (doc.layers().len(), paint.stroke_len()),
            (1, 0),
            "at First: no stroke"
        );
        let last = doc.session_history.entries().last().unwrap().op_id;
        assert!(travel_to(&mut doc, Some(&mut paint), last));
        assert_eq!(
            (doc.layers().len(), paint.stroke_len()),
            (2, 1),
            "back to the newest"
        );
    }

    /// A paint-capable device (the `bake_sources` tests' pattern).
    fn paint_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let features = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        if !adapter.features().contains(features) {
            eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: features,
            ..Default::default()
        }))
        .ok()
    }

    #[test]
    fn unified_undo_interleaves_a_real_stroke_and_layer_op() {
        // GPU: layer op, then a stroke, then a layer op. Three unified
        // undos must take the layer op, then the STROKE, then the first
        // layer op: the order they happened, across both journals.
        let Some((device, queue)) = paint_device() else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        let mut paint = PaintState::with_target_size(device, queue, 64).expect("paint builds");
        let mut doc = Document::default();
        doc.run(LayerCommand::add("First", LayerKind::Paint));
        paint.begin_stroke(egui::pos2(0.2, 0.2));
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            paint.extend_stroke(egui::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        paint.end_stroke();
        paint.process_pending().expect("drains");
        drain_stroke_commits(&mut doc, &mut paint);
        assert_eq!(paint.stroke_len(), 1, "the stroke committed");
        doc.run(LayerCommand::add("Second", LayerKind::Paint));

        assert!(unified_undo(&mut doc, Some(&mut paint)));
        assert_eq!(
            (doc.layers().len(), paint.stroke_len()),
            (1, 1),
            "the newest: Second"
        );
        assert!(unified_undo(&mut doc, Some(&mut paint)));
        assert_eq!(
            (doc.layers().len(), paint.stroke_len()),
            (1, 0),
            "then the stroke"
        );
        assert!(unified_undo(&mut doc, Some(&mut paint)));
        assert_eq!(
            (doc.layers().len(), paint.stroke_len()),
            (0, 0),
            "then First"
        );
        assert!(!unified_undo(&mut doc, Some(&mut paint)));
        // Redo replays the same three in reverse.
        assert!(unified_redo(&mut doc, Some(&mut paint)));
        assert_eq!((doc.layers().len(), paint.stroke_len()), (1, 0));
        assert!(unified_redo(&mut doc, Some(&mut paint)));
        assert_eq!((doc.layers().len(), paint.stroke_len()), (1, 1));
        assert!(unified_redo(&mut doc, Some(&mut paint)));
        assert_eq!((doc.layers().len(), paint.stroke_len()), (2, 1));
    }

    /// A log interleaving layer and stroke rows in `op_id` order.
    fn mixed_log(rows: &[(bool, usize)]) -> Vec<HistoryEntry> {
        rows.iter()
            .enumerate()
            .map(|(i, (layer, depth))| HistoryEntry {
                kind: if *layer {
                    HistoryKind::AddPaint
                } else {
                    HistoryKind::BrushStroke
                },
                op_id: i as u64,
                undo_depth: layer.then_some(*depth),
                stroke_depth: (!layer).then_some(*depth),
            })
            .collect()
    }

    #[test]
    fn unified_undo_walks_both_journals_newest_first_and_redo_replays() {
        // L1, S1, L2, S2: two layer ops and two strokes, interleaved.
        let log = mixed_log(&[(true, 1), (false, 1), (true, 2), (false, 2)]);
        // Simulate four unified undos, applying each to the right length.
        let (mut l, mut s) = (2usize, 2usize);
        let mut order = Vec::new();
        while let Some(j) = unified_undo_target(&log, l, Some(s)) {
            order.push(j);
            match j {
                Journal::Layer => l -= 1,
                Journal::Stroke => s -= 1,
            }
        }
        use Journal::{Layer, Stroke};
        assert_eq!(
            order,
            [Stroke, Layer, Stroke, Layer],
            "newest first, across journals"
        );
        assert_eq!((l, s), (0, 0));
        // Redo replays the exact reverse.
        let mut replay = Vec::new();
        while let Some(j) = unified_redo_target(&log, l, Some(s)) {
            replay.push(j);
            match j {
                Journal::Layer => l += 1,
                Journal::Stroke => s += 1,
            }
        }
        assert_eq!(replay, [Layer, Stroke, Layer, Stroke]);
        assert_eq!((l, s), (2, 2));
    }

    #[test]
    fn unified_undo_without_paint_skips_stroke_rows_and_context_rows() {
        let mut log = mixed_log(&[(true, 1), (false, 1)]);
        log.push(HistoryEntry {
            kind: HistoryKind::Bake,
            op_id: 9,
            undo_depth: None,
            stroke_depth: None,
        });
        // No paint session: only the layer row is undoable.
        assert_eq!(unified_undo_target(&log, 1, None), Some(Journal::Layer));
        assert_eq!(unified_undo_target(&log, 0, None), None);
        // A merged stroke shares its predecessor's depth: still one step.
        let merged = mixed_log(&[(false, 1), (false, 1)]);
        assert_eq!(
            unified_undo_target(&merged, 0, Some(1)),
            Some(Journal::Stroke)
        );
        assert_eq!(
            unified_redo_target(&merged, 0, Some(0)),
            Some(Journal::Stroke)
        );
        assert_eq!(unified_redo_target(&merged, 0, Some(1)), None);
    }

    #[test]
    fn unified_undo_steps_a_real_document() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", umber_core::LayerKind::Paint));
        doc.run(LayerCommand::add("B", umber_core::LayerKind::Paint));
        assert!(unified_undo(&mut doc, None));
        assert_eq!(doc.layers().len(), 1);
        assert!(unified_undo(&mut doc, None));
        assert!(!unified_undo(&mut doc, None), "nothing left");
        assert!(unified_redo(&mut doc, None));
        assert!(unified_redo(&mut doc, None));
        assert_eq!(doc.layers().len(), 2);
        assert!(!unified_redo(&mut doc, None));
    }

    use super::*;

    #[test]
    fn every_kind_has_a_distinct_plain_crash_label() {
        use HistoryKind::*;
        let all = [
            AddPaint,
            AddFill,
            AddFolder,
            AddMaterial,
            AddInstance,
            Remove,
            Reorder,
            SetOpacity,
            SetVisible,
            SetBlendMode,
            Rename,
            SetFill,
            SetMaterial,
            SetInstanceSource,
            AttachMask,
            DetachMask,
            SetMaskEnabled,
            BrushStroke,
            Bake,
            Export,
        ];
        let labels: Vec<&str> = all.iter().map(|k| k.crash_label()).collect();
        let unique: std::collections::BTreeSet<&str> = labels.iter().copied().collect();
        assert_eq!(unique.len(), all.len(), "{labels:?}");
        // Plain ASCII words: readable in any tracker, no locale, no markup.
        assert!(labels
            .iter()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_lowercase() || c == ' ')));
    }
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
                LayerCommand::add("m", LayerKind::Material),
                HistoryKind::AddMaterial,
                "Add material layer",
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
        // SetFill needs real FillParams; SetMaterial needs real
        // MaterialParams; cover their labels directly.
        assert_eq!(HistoryKind::SetFill.label(), "Fill layer");
        assert_eq!(HistoryKind::SetMaterial.label(), "Set material");
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
    fn stroke_rows_carry_stroke_targets_not_layer_targets() {
        // The drain's contract, headless: one row per commit, depths in
        // order, op ids monotonic — and a stroke row never drives the
        // layer journal, nor a layer row the stroke journal.
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_stroke(1);
        log.record_stroke(1);
        log.record_stroke(2);

        let entries = log.entries();
        assert_eq!(
            entries.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![
                HistoryKind::AddPaint,
                HistoryKind::BrushStroke,
                HistoryKind::BrushStroke,
                HistoryKind::BrushStroke,
            ]
        );
        assert_eq!(
            entries.iter().map(|e| e.op_id).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        // The merged second stroke shares the first's depth: two rows, one
        // undo step, one shared target.
        assert_eq!(
            entries
                .iter()
                .map(|e| e.stroke_target())
                .collect::<Vec<_>>(),
            vec![None, Some(1), Some(1), Some(2)]
        );
        assert_eq!(
            entries.iter().map(|e| e.jump_target()).collect::<Vec<_>>(),
            vec![Some(1), None, None, None]
        );
    }

    #[test]
    fn truncate_stroke_future_drops_dead_stroke_rows_and_keeps_layers() {
        // The stroke-side twin of the layer truncate: a fresh stroke over
        // undone stroke state clears the stroke redo stack, so stroke rows
        // past the cursor describe no command. Layer and context rows
        // survive — the stroke push leaves the layer journal alone.
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_stroke(1);
        log.record_context(HistoryKind::Bake);
        log.record_stroke(2);

        log.truncate_stroke_future(1);
        assert_eq!(
            log.entries()
                .iter()
                .map(|e| (e.kind, e.undo_depth, e.stroke_depth))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::AddPaint, Some(1), None),
                (HistoryKind::BrushStroke, None, Some(1)),
                // Wall-clock context survives the truncate.
                (HistoryKind::Bake, None, None),
            ]
        );
        // Truncating at the live depth keeps everything.
        log.truncate_stroke_future(1);
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn adopt_stroke_epoch_drops_only_stroke_rows() {
        // A resize clears the stroke journal wholesale: old stroke depths
        // would alias the NEW history's entries, so the drain drops those
        // rows on the epoch change. Layer and context rows stay — the
        // layer journal is untouched by a resize.
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_stroke(1);
        log.record_context(HistoryKind::Export);
        log.record_stroke(2);
        assert_eq!(log.stroke_epoch(), 0);

        assert!(!log.adopt_stroke_epoch(0), "same epoch adopts nothing");
        assert_eq!(log.len(), 4);
        assert!(log.adopt_stroke_epoch(1), "the resize's bump");
        assert_eq!(log.stroke_epoch(), 1);
        assert_eq!(
            log.entries().iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![HistoryKind::AddPaint, HistoryKind::Export]
        );
        // New strokes record against the new epoch at depth 1 again.
        log.record_stroke(1);
        assert_eq!(log.entries()[2].stroke_target(), Some(1));
    }

    #[test]
    fn row_state_marks_current_past_future_and_context() {
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_undoable(HistoryKind::SetOpacity, 2);
        log.record_stroke(1);
        log.record_context(HistoryKind::Bake);
        let e = log.entries();
        // Live: layers at 2, strokes at 1.
        assert_eq!(row_state(&e[0], 2, Some(1)), RowState::Past);
        assert_eq!(row_state(&e[1], 2, Some(1)), RowState::Current);
        assert_eq!(row_state(&e[2], 2, Some(1)), RowState::Current);
        assert_eq!(row_state(&e[3], 2, Some(1)), RowState::Context);
        // Undo one layer edit: row 2 becomes Future (greyed), row 1 Current.
        assert_eq!(row_state(&e[1], 1, Some(1)), RowState::Future);
        assert_eq!(row_state(&e[0], 1, Some(1)), RowState::Current);
        // Undo the stroke: its row is Future.
        assert_eq!(row_state(&e[2], 2, Some(0)), RowState::Future);
        // No paint session: stroke rows read as context.
        assert_eq!(row_state(&e[2], 2, None), RowState::Context);
    }

    #[test]
    fn entry_is_current_tracks_each_journal() {
        let mut log = SessionHistory::new();
        log.record_undoable(HistoryKind::AddPaint, 1);
        log.record_stroke(1);
        log.record_context(HistoryKind::Bake);
        let entries = log.entries();

        // Live state (layer 1, strokes 1): one current row per journal.
        assert!(entry_is_current(&entries[0], 1, Some(1)));
        assert!(entry_is_current(&entries[1], 1, Some(1)));
        assert!(!entry_is_current(&entries[2], 1, Some(1)));
        // An undone layer moves the highlight off the layer row only.
        assert!(!entry_is_current(&entries[0], 0, Some(1)));
        assert!(entry_is_current(&entries[1], 0, Some(1)));
        // An undone stroke moves it off the stroke row only.
        assert!(!entry_is_current(&entries[1], 1, Some(0)));
        // No paint session: stroke rows are never current (they render
        // display-only), layer rows still track.
        assert!(!entry_is_current(&entries[1], 1, None));
        assert!(entry_is_current(&entries[0], 1, None));
    }

    #[test]
    fn apply_commits_maps_depths_to_rows_in_order() {
        use crate::document::Document;

        let mut doc = Document::default();
        apply_stroke_commits(
            &mut doc,
            0,
            vec![
                StrokeCommit {
                    cursor: 0,
                    depth: 1,
                },
                StrokeCommit {
                    cursor: 1,
                    depth: 2,
                },
            ],
        );
        let rows = doc.session_history.entries();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter().map(|e| (e.kind, e.op_id)).collect::<Vec<_>>(),
            vec![(HistoryKind::BrushStroke, 0), (HistoryKind::BrushStroke, 1),]
        );
        assert_eq!(
            rows.iter().map(|e| e.stroke_target()).collect::<Vec<_>>(),
            vec![Some(1), Some(2)]
        );
        // Stroke rows never drive the layer journal.
        assert!(rows.iter().all(|e| e.jump_target().is_none()));
        assert_eq!(doc.session_history.stroke_epoch(), 0);
    }

    #[test]
    fn apply_commits_truncates_the_dead_future_and_shares_merged_depths() {
        use crate::document::Document;

        let mut doc = Document::default();
        apply_stroke_commits(
            &mut doc,
            0,
            vec![
                StrokeCommit {
                    cursor: 0,
                    depth: 1,
                },
                StrokeCommit {
                    cursor: 1,
                    depth: 2,
                },
            ],
        );
        // A merged successor coalesces into the top entry (cursor ==
        // depth): its row shares the predecessor's depth — three rows, two
        // undo steps, one shared target.
        apply_stroke_commits(
            &mut doc,
            0,
            vec![StrokeCommit {
                cursor: 2,
                depth: 2,
            }],
        );
        assert_eq!(
            doc.session_history
                .entries()
                .iter()
                .map(|e| (e.kind, e.stroke_target(), e.op_id))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::BrushStroke, Some(1), 0),
                (HistoryKind::BrushStroke, Some(2), 1),
                (HistoryKind::BrushStroke, Some(2), 2),
            ]
        );
        // A fresh stroke over undone stroke state truncates the dead redo
        // future at its cursor first: both depth-2 rows describe evicted
        // entries and go, the new stroke reuses depth 2 with the next op
        // id (ids stay monotonic across the truncation).
        apply_stroke_commits(
            &mut doc,
            0,
            vec![StrokeCommit {
                cursor: 1,
                depth: 2,
            }],
        );
        assert_eq!(
            doc.session_history
                .entries()
                .iter()
                .map(|e| (e.kind, e.stroke_target(), e.op_id))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::BrushStroke, Some(1), 0),
                (HistoryKind::BrushStroke, Some(2), 3),
            ]
        );
    }

    #[test]
    fn apply_commits_adopts_a_new_epoch_and_keeps_layer_rows() {
        use crate::document::Document;

        let mut doc = Document::default();
        doc.run(LayerCommand::add("A", LayerKind::Paint));
        apply_stroke_commits(
            &mut doc,
            0,
            vec![StrokeCommit {
                cursor: 0,
                depth: 1,
            }],
        );
        assert_eq!(doc.session_history.len(), 2);

        // The resize's epoch: the evicted stroke row goes, the layer row
        // (the layer journal is untouched by a resize) stays, and the new
        // stroke records at depth 1 again with the next op id.
        apply_stroke_commits(
            &mut doc,
            1,
            vec![StrokeCommit {
                cursor: 0,
                depth: 1,
            }],
        );
        assert_eq!(
            doc.session_history
                .entries()
                .iter()
                .map(|e| (e.kind, e.undo_depth, e.stroke_depth, e.op_id))
                .collect::<Vec<_>>(),
            vec![
                (HistoryKind::AddPaint, Some(1), None, 0),
                (HistoryKind::BrushStroke, None, Some(1), 2),
            ]
        );
        assert_eq!(doc.session_history.stroke_epoch(), 1);
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
