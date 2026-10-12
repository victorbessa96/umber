//! App-side paint state: pointer strokes → conditioned dabs → PaintThread.
//!
//! The first painting surface is the 2D UV view (direct UV→texel mapping —
//! no ray-picking needed); 3D-viewport painting (UV projection from the
//! depth buffer) arrives in a later wave. This module owns the per-stroke
//! [`StrokeConditioner`] and the [`PaintThread`] session, and translates
//! pointer events from the UV view into staged paint commands.
//!
//! Seam-aware stamping (docs/specs/seam-aware-stamping-design.md, slice 3):
//! once a mesh is handed over via [`PaintState::set_mesh`], dab positions
//! near a UV seam are mirrored into the neighboring island's UV coordinates
//! through the [`SeamGraph`](umber_mesh::seam::SeamGraph) edge
//! correspondence, so a stroke paints every UV image of the same 3D
//! neighborhood.
//!
//! Per-tile paint routing (docs/specs/udim-design.md, slice 4): the
//! session holds one [`PaintThread`] per UDIM tile, created lazily the
//! first time an event routes there. A pointer event at UV `(u, v)` lands
//! in `tile_of_uv([u, v])`'s target at tile-LOCAL texel coordinates (see
//! [`local_texel`]); the last tile an original (non-mirror) event routed
//! to is the ACTIVE tile, which [`PaintState::paint_target`] returns so
//! every existing single-target caller stays source-compatible. Routing
//! is live only once a multi-tile mesh is handed over via
//! [`PaintState::set_mesh`]; without a mesh, or with a single-tile mesh,
//! everything routes to tile 1001 — the pre-UDIM behavior,
//! byte-identical. V1 boundary (the design's named follow-up): routing is
//! per-event, so a stroke dragging across a tile seam paints each event
//! into the tile under the pointer; there is no cross-tile stroke
//! continuity.
//!
//! Retouch tools (docs/specs/smudge-clone-design.md; the eraser is §2's P0
//! row): with a smudge, clone or eraser [`BrushTool`] active, the
//! conditioned dabs skip the GPU splat and run on a per-stroke CPU mirror
//! instead ([`crate::retouch`]). The hook is in `push_event_styled`, right after `DabAdapter::stamps_to_dabs`: the
//! first event a stroke routes to a tile drains that tile's queue and
//! reads it back (`PaintTarget::read_back_rgba8`), every event's dirty
//! rect is written back as a `PaintThreadCommand::WriteRegion`, and
//! `end_stroke` records the stroke as ONE entry in the retouch history.
//! Retouch strokes do not seam-mirror in v1.
//!
//! Paint-stroke undo (CORE-LOOP CRITICAL 3, the 2026-10-12 strategy
//! review): paint-tool strokes record ONE [`PaintStrokeCommand`] per stroke
//! on the SAME `retouch_history` — the v1 `clear()` on every paint stroke
//! is gone. The capture mirrors the retouch path with one structural
//! difference (paint composites on the GPU, so there is no CPU mirror
//! holding the after-bytes):
//!
//! * Lazy per-tile before snapshot: the first staged batch a stroke routes
//!   to a tile drains that tile's queue and reads it back
//!   (`PaintTarget::read_back_rgba8`), so earlier strokes' dabs are in the
//!   bytes. Tiles the stroke never stages to cost nothing.
//! * Dirty accumulation: the union of the staged dabs' shader-identical
//!   footprint boxes ([`umber_brush::retouch::footprint`]), taken over the
//!   FINAL dabs (after symmetry expansion and mask gating), so symmetric
//!   siblings are covered; seam mirrors recurse through the same staging,
//!   so their tiles accumulate too. Under an active mask gate the union is
//!   conservative (footprint ⊇ written texels) — undo stays byte-exact.
//! * At `end_stroke` each touched tile is drained and read back, and the
//!   AFTER bytes are sliced out of the same rect. A tile whose
//!   before == after (a fully gated-away stroke) contributes no region, and
//!   a stroke with no regions records no entry — the identity rule, same as
//!   retouch. Undo/redo apply through `retouch_writes` (`WriteRegion`), the
//!   same drain as retouch strokes.
//!
//! Judgment calls (v1 boundaries):
//!
//! * Last-stroke undo is byte-exact. Undoing an OLDER stroke under newer
//!   paint overwrites the overlap (linear-undo last-write-wins) — the same
//!   property retouch strokes already had among themselves; no attempt is
//!   made to merge overlapping rects.
//! * Cost is two full-target readbacks per touched tile per stroke (before
//!   at first touch, after at end); retouch needs one because the after
//!   lives in its CPU mirror. Paint has no CPU composite step to keep an
//!   after in, so the readbacks are inherent, not a shortcut.
//! * Memory: RGBA8 is 4 bytes/texel. At 512² a full-tile rect is
//!   512·512·4 = 1 MiB per side, so a worst-case entry (before + after) is
//!   ~2 MiB and the 64-entry cap binds first (64 × 2 MiB ≈ 128 MiB, inside
//!   the 256 MiB budget). At 4K a full-tile side is 64 MiB and one entry is
//!   ~128 MiB: two such entries fill the budget and the third evicts the
//!   oldest, while the 64-entry cap alone would admit ~8 GiB — so at 4K the
//!   BYTE budget is the RAM guard and the entry cap is the stroke-count UX.
//!   A typical stroke is small (brush r = 24 texels ⇒ footprint ≈ 49×49×4
//!   ≈ 10 KiB per side; a 16-event union is tens of KiB). V1 keeps BOTH
//!   existing caps unchanged ([`RETOUCH_HISTORY_MAX_ENTRIES`] +
//!   [`RETOUCH_HISTORY_MAX_BYTES`]) and wires no `on_evict` hook — evicted
//!   entries are plain `Vec<u8>`, whose drop needs no cleanup. Against the
//!   review's §12:167 row (2 GB / 100-step budget): this 256 MiB / 64-step
//!   envelope sits well inside it; the entry cap must never be raised
//!   toward 100 without the byte budget beside it.
//! * Mask-mode strokes still leave the history alone (they touch the mask,
//!   not the layer), and [`Self::resize_target`] still clears it (rects
//!   recorded at the old size no longer fit).
//!
//! Symmetry (§2's P1 row, `umber_brush::symmetry`): the panel's
//! [`SymmetrySettings`] are adopted between strokes ([`PaintState::sync_symmetry`])
//! and snapshot into tile-local texel transforms at `begin_stroke`. The
//! hook sits right after `DabAdapter::stamps_to_dabs` — AFTER the stroke
//! conditioner, so siblings are exact images of the smoothed path and the
//! one-euro/spacing state only ever sees the pointer (unlike the seam
//! mirrors, which recurse through the conditioner). Paint dabs expand into
//! the SAME `Stage` command (batch counts unchanged, dabs × siblings);
//! retouch dabs hand the primaries to [`RetouchStroke`], which stamps the
//! siblings with per-sibling smudge motion and mirrored clone sources.
//! Symmetry is tile-local (each tile mirrors about its own center line)
//! and v1 applies it to ORIGINAL events only: symmetric siblings do not
//! seam-mirror and seam mirrors do not symmetrize (named follow-up).
//! Symmetry off skips the expansion entirely — byte-identical staging.
//! The settings are session state: not in the project file (follow-up).
//!
//! Straight-line constraint + angle snap (§2's P1 row,
//! `umber_brush::line_constraint`): the panel's [`LineConstraintMode`] is
//! adopted between strokes ([`PaintState::sync_line_constraint`]) and set
//! on the live conditioner, whose hook sits AFTER lazy mouse and BEFORE
//! spacing — every stamp projects onto the stroke-start-anchored line (or
//! the nearest snap-multiple ray) before the dab planner sees it, so
//! spacing and the dab adapter downstream are untouched. Off (the default)
//! is the identity: staged dabs are byte-identical. Like symmetry, the
//! mode is session state persisted in the paint session (not the project
//! file); a mid-stroke change is refused so one stroke never splits.
//!
//! Wash mode (`umber_brush::WashMode`, §2 line 16): the active preset's
//! mode is adopted between strokes ([`PaintState::sync_wash_mode`]) and
//! renormalizes staged paint dabs pre-splat ([`apply_wash_mode`], right
//! after the retouch hook so smudge/clone/eraser and mask strokes keep
//! build-up alphas). CPU-side only: no extra GPU surface, no shader arm.
//!
//! Quick mask (§2's P1 row, `umber_brush::quick_mask` — semantics,
//! convention and the not-persisted/not-undoable rule live there): the
//! session owns one [`QuickMask`]. `begin_stroke` snapshots its two modes
//! for the whole stroke:
//!
//! * Mask mode (`editing`): the stroke paints the MASK. The hook sits
//!   right after symmetry expansion in `push_event_styled` and BEFORE the
//!   retouch hook, so mask mode wins whatever tool is selected: the dabs
//!   (and their symmetry siblings and seam mirrors) stamp the CPU mask,
//!   the touched rect is queued onto the tile's GPU mask
//!   (`PaintThreadCommand::WriteMask`), and nothing is staged. The layer
//!   and the retouch history are untouched.
//! * Gating (`active && !editing`): every layer dab is gated per texel
//!   AFTER symmetry expansion. Paint dabs carry the shader's `mask_gate`
//!   flag, so primaries, siblings and seam mirrors are all multiplied by
//!   their tile's GPU mask. Before the first gated dab on a tile, the
//!   tile's whole plane is uploaded (an invert/clear/resize marks every
//!   tile stale). Smudge/clone/eraser dabs multiply the CPU plane inside
//!   the retouch ops.
//!
//! Mask off (the default) changes nothing: no plane is allocated, no
//! upload is queued, and the staged dabs are byte-identical.
//!
//! Paint paths (§2 line 26's P1 row, `umber_brush::path`): a
//! [`PaintPath`](umber_brush::path::PaintPath) replays as ONE stroke
//! through this same pipeline ([`Self::replay_path`] — paint/erase/smudge
//! route to the consumer the op mode already selects), and a closed path
//! fills its interior ([`Self::replay_filled_path`]: the polygon fill's
//! even-odd rasterizer over the active tile, one stroke per covered row
//! run). Per-point size rides a per-event radius scale
//! ([`Self::push_event_styled`], which [`push_event_styled`](Self::push_event_styled)
//! delegates to at scale `1.0` — byte-identical); per-point opacity is
//! folded into the event pressure by the path itself (both are flow
//! multipliers, so spacing interpolates them for free). Replays refuse
//! while a live stroke runs, and restore the session tool afterwards.

use std::collections::{BTreeMap, BTreeSet};

use egui::Pos2;
use umber_brush::path::{sanitize_size, PaintPath, PathOp};
use umber_brush::retouch::{footprint, DirtyRect, RetouchDab};
use umber_brush::symmetry::{expand_dabs, texel_transforms};
use umber_brush::{
    wash_scale, BrushParams, BrushTool, DabAdapter, LineConstraintMode, MaskPaint, QuickMask,
    RgbaBuffer, StrokeConditioner, StrokeEvent, SymTransform, SymmetrySettings, WashMode,
};
use umber_core::undo::UndoStack;
use umber_gpu::paint::Dab;
use umber_gpu::paint_thread::{FrameStats, PaintThread, PaintThreadCommand};
use umber_mesh::polygon_fill::rasterize_polygon_mask;
use umber_mesh::seam::{build_seam_graph, SeamGraph};
use umber_mesh::udim::{tile_of_triangle, tile_of_uv, FIRST_TILE};

use crate::retouch::{PaintStrokeCommand, PaintWrites, RetouchStroke, TileRegion};

/// Paint-what-you-see projection (§2's P1 row, Mari-style paint buffer).
///
/// * [`PaintProjection::Uv`]: pointer positions are already UVs (the 2D UV
///   view). The 3D viewport does not paint — Ctrl+drag orbits.
/// * [`PaintProjection::Screen`] (the default: today's behavior, both
///   surfaces paint): a viewport pointer resolves through the camera + mesh
///   raycast to a UV before conditioning, via
///   [`PaintState::begin_screen_point`]/[`PaintState::extend_screen_point`];
///   misses skip the stamp (no orphan dabs) and seam mirrors fan out from
///   the resolved UV exactly like a UV-view stroke.
///
/// V1 judgment — live raycast, not a deferred Mari buffer: a true paint
/// buffer would snapshot screen-space points mid-stroke and resolve them to
/// UVs at stroke end, but that breaks the live preview (the UV view and the
/// export bridge read the tile targets every frame), the per-frame
/// `process_pending` drain the retouch readback depends on, and the per-event
/// staged-batch accounting the seam/symmetry hooks rely on. It is also
/// unnecessary in the current wiring: `Viewport::ui` skips `handle_input`
/// while a paint drag is in flight, so the camera cannot move mid-stroke
/// and there is nothing to tear. The deferred, camera-snapshotting buffer is
/// the named follow-up for when camera animation during strokes lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaintProjection {
    /// 2D UV editing only; the 3D viewport orbits.
    Uv,
    /// Paint what you see (the default): viewport strokes raycast to UVs.
    #[default]
    Screen,
}

impl PaintProjection {
    /// Whether Ctrl+drag in the 3D viewport paints (`Screen`) or orbits
    /// (`Uv`). The UV view paints in both modes.
    pub fn viewport_paints(self) -> bool {
        matches!(self, Self::Screen)
    }
}

/// Errors from constructing or driving the paint session.
#[derive(Debug, thiserror::Error)]
pub enum PaintStateError {
    /// The GPU compositor rejected the device (missing storage-texture
    /// feature) or a dispatch failed.
    #[error(transparent)]
    Paint(#[from] umber_gpu::paint::PaintError),
    /// The stroke conditioner's parameters were invalid.
    #[error(transparent)]
    Conditioning(#[from] umber_brush::ConditionerError),
}

/// Paint-target edge length in texels (Wave 2: single square tile).
const TARGET_SIZE: u32 = 512;

/// Brush radius in target texels.
const BRUSH_RADIUS_TEXELS: f32 = 24.0;

/// Synthetic event-clock step: nominal nanoseconds between consecutive
/// pointer events fed to the stroke conditioner.
///
/// The conditioner needs strictly advancing timestamps: `OneEuroFilter`
/// holds its estimate on duplicate/backwards timestamps, so feeding every
/// event `time_ns: 0` (as this module historically did) freezes the filter
/// and no stroke ever emits a dab. Real tablet timestamps are not plumbed
/// through the UV view yet (a later stylus-wave slice); until then each
/// `push_event_styled` call advances a per-stroke logical clock by one step
/// (8 ms, the same regime as the brush crate's own tests). Deterministic:
/// the same event sequence always conditions identically.
const EVENT_STEP_NS: u64 = 8_000_000;

/// Mirror-gate margin over the brush footprint: a dab is mirrored across a
/// seam edge when its UV position lies within
/// `BRUSH_RADIUS_TEXELS * SEAM_MIRROR_MARGIN / texels_per_uv` of the edge's
/// UV segment. Must exceed 1.0 so a dab whose footprint can reach the
/// opposite island is always expanded; start at 1.5× per the design doc.
pub const SEAM_MIRROR_MARGIN: f32 = 1.5;

/// Retouch-history capacity (strokes) and memory budget (bytes of
/// before + after dirty-rect snapshots).
const RETOUCH_HISTORY_MAX_ENTRIES: usize = 64;
const RETOUCH_HISTORY_MAX_BYTES: usize = 256 * 1024 * 1024;

/// One tile's slice of a live paint stroke's undo capture: the
/// stroke-start snapshot plus the union of every staged dab footprint on
/// that tile (see the module docs). The snapshot is taken lazily on the
/// stroke's first staged batch per tile, so untouched tiles cost nothing.
struct PaintTileCapture {
    /// The tile's texels before the stroke touched it (drained first, so
    /// earlier strokes' dabs are in the bytes).
    before: RgbaBuffer,
    /// Union of the staged dab footprints, or `None` when the tile was
    /// snapshotted but nothing staged yet.
    dirty: Option<DirtyRect>,
}

/// The live paint-tool stroke's undo capture (`Some` between
/// `begin_stroke` and `end_stroke` while the paint tool is active; `None`
/// for retouch and mask-mode strokes, which have their own paths).
#[derive(Default)]
struct PaintStrokeCapture {
    tiles: BTreeMap<u16, PaintTileCapture>,
}

/// The paint session: one GPU target per UDIM tile plus the live stroke
/// conditioner.
pub struct PaintState {
    /// Device/queue handles kept for lazy per-tile target creation.
    device: umber_gpu::WgpuDevice,
    queue: umber_gpu::WgpuQueue,
    /// One paint thread per UDIM tile. Tile [`FIRST_TILE`] is created in
    /// [`Self::new`] and never removed; other tiles appear on first route.
    targets: BTreeMap<u16, PaintThread>,
    /// The tile the last original (non-mirror) event routed to; always a
    /// key of `targets`.
    active_tile: u16,
    /// Edge lengths new tile targets are created at (tracks
    /// [`Self::resize_target`]).
    target_size: (u32, u32),
    /// Whether the loaded mesh spans more than one UDIM tile. Routing by
    /// `tile_of_uv` is live only then; otherwise every event lands in
    /// [`FIRST_TILE`] (an edge click at `u == 1.0` must not spawn a blank
    /// tile 1002 on a single-tile mesh).
    multi_tile: bool,
    /// Dab parameters every staged stroke uses (v1: the default brush;
    /// tests override the color to tell tiles' paint apart).
    brush: BrushParams,
    /// How the live stroke's overlapping dabs accumulate, adopted from
    /// the active preset between strokes ([`Self::sync_wash_mode`]).
    /// `BuildUp` (the default) stages adapter dabs unchanged;
    /// `Wash` renormalizes them pre-splat (see `push_event_styled`).
    wash_mode: WashMode,
    conditioner: StrokeConditioner,
    /// Set on pointer-down inside the UV square; cleared on pointer-up.
    stroking: bool,
    /// UV→texel scale (target texels per UV unit); constant while the
    /// target is TARGET_SIZE².
    texels_per_uv: f32,
    last_stats: FrameStats,
    /// Seam topology of the loaded mesh (`None` until [`Self::set_mesh`]).
    seam_graph: Option<SeamGraph>,
    /// Armed by [`Self::begin_stroke`] when the stroke starts near a seam;
    /// gates the per-event mirror expansion (seam-blind strokes skip the
    /// mirror call entirely — the zero-cost contract).
    stroke_near_seam: bool,
    /// Per-stroke synthetic event clock (see [`EVENT_STEP_NS`]): advanced
    /// once per [`Self::push_event_styled`] call, reset by
    /// [`Self::begin_stroke`].
    event_seq: u64,
    /// The active tool (synced from the brush panel between strokes).
    tool: BrushTool,
    /// The live retouch stroke (`Some` between `begin_stroke` and
    /// `end_stroke` while a smudge/clone tool is active).
    retouch: Option<RetouchStroke>,
    /// The live paint stroke's undo capture (`Some` between
    /// `begin_stroke` and `end_stroke` while the paint tool is active).
    paint_capture: Option<PaintStrokeCapture>,
    /// The alt-click clone source (UV). Kept across strokes: fixed mode
    /// re-resolves the offset against every stroke's start (each stroke
    /// samples this same point); follow mode resolves it once, on the
    /// first stroke after the alt-click, and keeps that offset.
    clone_source: Option<Pos2>,
    /// Set by an alt-click, cleared by the first clone stroke after it.
    clone_source_fresh: bool,
    /// A clone offset resolved from `clone_source`, waiting to be written
    /// back into the panel's params ([`Self::sync_tool`]).
    resolved_clone_offset: Option<(f32, f32)>,
    /// One entry per paint or retouch stroke (the design's undo-per-stroke
    /// contract), separate from the document's layer journal in v1.
    retouch_history: UndoStack<PaintStrokeCommand>,
    /// Region writes the retouch history's undo/redo produced, drained
    /// into the tiles' queues right after each call.
    retouch_writes: PaintWrites,
    /// The symmetry settings (synced from the brush panel between
    /// strokes; the UV view's mirror manipulator edits the offset here).
    symmetry: SymmetrySettings,
    /// The straight-line / angle-snap mode (synced from the brush panel
    /// between strokes via [`Self::sync_line_constraint`]; the live
    /// conditioner is set at the same time).
    line_constraint: LineConstraintMode,
    /// Where strokes may start (§2's P1 row): UV view only, or UV view +
    /// the raycast viewport ([`PaintProjection`]).
    projection: PaintProjection,
    /// Set when the manipulator changed `symmetry`, so the next
    /// [`Self::sync_symmetry`] hands it back to the panel instead of
    /// adopting the panel's (stale) copy.
    symmetry_from_view: bool,
    /// The live stroke's tile-local texel transforms (identity first),
    /// snapshot at `begin_stroke`; `None` = symmetry off for this stroke.
    stroke_symmetry: Option<Vec<SymTransform>>,
    /// The quick mask (session state: not in the project file, not in
    /// any undo history).
    quick_mask: QuickMask,
    /// Tiles whose GPU mask texture matches `quick_mask`'s plane. Emptied
    /// by every whole-mask change (invert, clear, resize); a gated dab's
    /// tile is brought back in sync before it is staged.
    mask_synced: BTreeSet<u16>,
    /// The live stroke paints the mask (mask mode at `begin_stroke`).
    stroke_mask_edit: bool,
    /// The live stroke is gated by the mask (gating at `begin_stroke`).
    stroke_mask_gate: bool,
    /// Queued `WriteMask` commands since construction (test
    /// observability for the mask-stroke and zero-cost contracts).
    #[cfg(test)]
    mask_uploads: usize,
    /// Staged `Stage` batches since construction (test observability for
    /// the seam-blind zero-cost contract).
    #[cfg(test)]
    staged_batches: usize,
    /// Per-tile split of `staged_batches` (test observability for the
    /// routing contract).
    #[cfg(test)]
    staged_batches_per_tile: BTreeMap<u16, usize>,
    /// Staged dabs since construction (test observability for the stroke
    /// soak's zero-drop assert — staged here must equal composited at
    /// drain — and the symmetry sibling count).
    #[cfg(test)]
    staged_dabs: u64,
}

impl PaintState {
    /// Creates the tile-1001 paint target on `device`/`queue` (cloned
    /// inside `PaintThread`); further tiles are created lazily on first
    /// route.
    ///
    /// # Errors
    ///
    /// Returns the compositor's error when the device lacks the storage-
    /// texture feature (see `PaintCompositor::new`), or a conditioning
    /// parameter error at construction.
    pub fn new(
        device: umber_gpu::WgpuDevice,
        queue: umber_gpu::WgpuQueue,
    ) -> Result<Self, PaintStateError> {
        let first = PaintThread::new(device.clone(), queue.clone(), TARGET_SIZE, TARGET_SIZE)?;
        Ok(Self {
            device,
            queue,
            targets: BTreeMap::from([(FIRST_TILE, first)]),
            active_tile: FIRST_TILE,
            target_size: (TARGET_SIZE, TARGET_SIZE),
            multi_tile: false,
            brush: BrushParams::default(),
            wash_mode: WashMode::BuildUp,
            conditioner: StrokeConditioner::new(
                umber_brush::OneEuroParams::default(),
                umber_brush::OneEuroParams::default(),
                0.0,
                5.0,
            )?,
            stroking: false,
            texels_per_uv: TARGET_SIZE as f32,
            last_stats: FrameStats::default(),
            seam_graph: None,
            stroke_near_seam: false,
            event_seq: 0,
            tool: BrushTool::Paint,
            retouch: None,
            paint_capture: None,
            clone_source: None,
            clone_source_fresh: false,
            resolved_clone_offset: None,
            retouch_history: UndoStack::new(RETOUCH_HISTORY_MAX_ENTRIES)
                .with_memory_budget(RETOUCH_HISTORY_MAX_BYTES),
            retouch_writes: PaintWrites::default(),
            symmetry: SymmetrySettings::default(),
            line_constraint: LineConstraintMode::Off,
            projection: PaintProjection::default(),
            symmetry_from_view: false,
            stroke_symmetry: None,
            quick_mask: QuickMask::new(TARGET_SIZE, TARGET_SIZE),
            mask_synced: BTreeSet::new(),
            stroke_mask_edit: false,
            stroke_mask_gate: false,
            #[cfg(test)]
            mask_uploads: 0,
            #[cfg(test)]
            staged_batches: 0,
            #[cfg(test)]
            staged_batches_per_tile: BTreeMap::new(),
            #[cfg(test)]
            staged_dabs: 0,
        })
    }

    /// Hands a mesh over for seam-aware stamping: builds its [`SeamGraph`]
    /// once (O(triangles)) and stores it for the stroke path, and arms
    /// per-tile routing when the mesh's triangles span more than one UDIM
    /// tile. Call once per mesh load, alongside the existing mesh handoff.
    pub fn set_mesh(&mut self, mesh: &umber_mesh::MeshData) {
        self.seam_graph = Some(build_seam_graph(mesh));
        let tiles = tile_of_triangle(mesh);
        self.multi_tile = tiles.windows(2).any(|w| w[0] != w[1]);
        self.stroke_near_seam = false;
    }

    /// Whether a stroke is currently active.
    pub fn is_stroking(&self) -> bool {
        self.stroking
    }

    /// Begin a stroke at `uv` (0..1 UV space).
    pub fn begin_stroke(&mut self, uv: Pos2) {
        self.begin_stroke_styled(uv, 1.0, 1.0);
    }

    /// Begin a stroke at `uv` with an explicit pressure and brush-radius
    /// scale (the path replay's per-point style; live strokes use `1.0`
    /// for both through [`Self::begin_stroke`]).
    pub fn begin_stroke_styled(&mut self, uv: Pos2, pressure: f32, radius_scale: f32) {
        self.conditioner.reset();
        self.event_seq = 0;
        self.stroking = true;
        // Stroke-start seam neighborhood check (design slice 3, item 4):
        // arm mirror expansion once per stroke; strokes starting far from
        // any seam skip the per-event mirror call entirely (zero cost).
        //
        // Deviation from the design doc's literal text (which names
        // `seam_edges_near`): that query takes a 3D point, but the app
        // tracks UV positions and no UV→3D inverse mapping exists in
        // `PaintState`. UV-space segment gating is the app-side equivalent:
        // the mirror math itself (`mirror_positions`) gates on UV-segment
        // distance, so checking the same distance up front arms exactly the
        // strokes that can produce mirrors.
        self.stroke_near_seam = self.seam_near_uv([uv.x, uv.y], self.seam_radius_uv());
        // Snapshot the symmetry for the whole stroke (before the retouch
        // stroke opens — it takes the same transforms).
        self.stroke_symmetry = self
            .symmetry
            .is_active()
            .then(|| texel_transforms(&self.symmetry, self.texels_per_uv));
        // Snapshot the quick-mask modes too: X / I mid-stroke never split
        // one stroke between the mask and the layer.
        self.stroke_mask_edit = self.quick_mask.is_editing();
        self.stroke_mask_gate = self.quick_mask.gates();
        self.retouch = if self.stroke_mask_edit {
            // A mask stroke paints only the mask: no retouch mirror, and
            // the retouch history stays valid (the layer is untouched).
            self.paint_capture = None;
            None
        } else if self.tool.is_retouch() {
            self.paint_capture = None;
            Some(self.begin_retouch(uv))
        } else {
            // A paint-tool stroke opens its own undo capture (one entry
            // per stroke on the shared history — see the module docs); the
            // before snapshot is taken lazily per tile at the first staged
            // batch, so a stroke that stages nothing records nothing.
            self.paint_capture = Some(PaintStrokeCapture::default());
            None
        };
        self.push_event_styled(uv, pressure, radius_scale, true);
    }

    /// Opens the retouch stroke starting at `uv`. Clone: the alt-click
    /// source resolves into the offset here (`source - start`, UV — every
    /// stroke in fixed mode, the first stroke after the click in follow
    /// mode), then the tracker starts in the start tile's local texels —
    /// the UV offset scaled by the texel density with V flipped, matching
    /// [`local_texel`].
    fn begin_retouch(&mut self, uv: Pos2) -> RetouchStroke {
        if let BrushTool::Clone(mut params) = self.tool {
            if let Some(source) = self.clone_source {
                if self.clone_source_fresh || !params.follow {
                    params.source_offset = (source.x - uv.x, source.y - uv.y);
                    self.resolved_clone_offset = Some(params.source_offset);
                    self.tool = BrushTool::Clone(params);
                }
                self.clone_source_fresh = false;
            }
        }
        let mut stroke = RetouchStroke::new(self.tool);
        if let Some(transforms) = &self.stroke_symmetry {
            stroke.set_symmetry(transforms.clone());
        }
        if let BrushTool::Clone(params) = self.tool {
            let tile = self.route_tile(uv);
            let start = local_texel([uv.x, uv.y], tile, self.texels_per_uv);
            let offset = clone_offset_texels(params.source_offset, self.texels_per_uv);
            stroke.begin_clone(tile, start, offset, params.follow);
        }
        stroke
    }

    /// Adopts the panel's tool (between strokes only — a tool swap
    /// mid-stroke would split one stroke across two ops), first writing a
    /// freshly resolved alt-click clone offset back into the panel's
    /// params so the panel shows the offset the next stroke uses.
    pub fn sync_tool(&mut self, panel_tool: &mut BrushTool) {
        if let (BrushTool::Clone(params), Some(offset)) =
            (&mut *panel_tool, self.resolved_clone_offset.take())
        {
            params.source_offset = offset;
        }
        if !self.stroking {
            self.tool = *panel_tool;
        }
    }

    /// Adopts the active preset's wash mode (between strokes only — a
    /// mode swap mid-stroke would split one stroke across two ops).
    /// Only the mode crosses here: the rest of the preset's params are
    /// still panel-local in v1 (the session paints with its own brush),
    /// and the overlap count comes from the live conditioner density
    /// (the density that actually spaced the stroke's dabs), not the
    /// preset's spacing.
    pub fn sync_wash_mode(&mut self, mode: WashMode) {
        if !self.stroking {
            self.wash_mode = mode;
        }
    }

    /// Syncs the symmetry settings with the panel: a manipulator edit made
    /// in the UV view since the last sync is handed BACK to the panel
    /// (the clone-offset pattern — else the panel's copy would overwrite
    /// it); otherwise the panel's settings are adopted. Adoption takes
    /// effect at the next `begin_stroke` (a live stroke keeps its
    /// snapshot), so a mid-stroke edit never splits one stroke.
    pub fn sync_symmetry(&mut self, panel: &mut SymmetrySettings) {
        if std::mem::take(&mut self.symmetry_from_view) {
            *panel = self.symmetry;
        } else {
            self.symmetry = *panel;
        }
    }

    /// The current symmetry settings (the UV view draws its guides from
    /// these).
    pub fn symmetry(&self) -> &SymmetrySettings {
        &self.symmetry
    }

    /// Adopts the panel's straight-line / angle-snap mode (between strokes
    /// only — a mode swap mid-stroke would split one stroke across two
    /// projections) and sets it on the live conditioner, whose anchor
    /// re-seeds at the next `begin_stroke`.
    pub fn sync_line_constraint(&mut self, mode: LineConstraintMode) {
        if !self.stroking {
            self.line_constraint = mode;
            self.conditioner.set_line_constraint(mode);
        }
    }

    /// The current straight-line / angle-snap mode.
    pub fn line_constraint(&self) -> LineConstraintMode {
        self.line_constraint
    }

    /// The current paint projection (which surfaces may start strokes).
    pub fn projection(&self) -> PaintProjection {
        self.projection
    }

    /// Adopts the panel's paint projection (between strokes only — a mode
    /// swap mid-stroke would split one stroke across two surfaces).
    pub fn sync_projection(&mut self, mode: PaintProjection) {
        if !self.stroking {
            self.projection = mode;
        }
    }

    /// Begins a Screen-projection stroke at the viewport raycast result:
    /// `Some(uv)` starts at the hit UV, `None` (a miss) starts nothing —
    /// no orphan stroke, no orphan dabs. The viewport calls this on the
    /// opening event of a paint drag instead of [`Self::begin_stroke`].
    pub fn begin_screen_point(&mut self, hit: Option<Pos2>) {
        if let Some(uv) = hit {
            self.begin_stroke(uv);
        }
    }

    /// Extends a Screen-projection stroke to the viewport raycast result:
    /// `Some(uv)` conditions the hit UV through the existing pipeline
    /// (seam mirrors, symmetry, wash, mask — all inherited, since this
    /// funnels into [`Self::extend_stroke`]); `None` (a miss) skips the
    /// stamp but keeps the stroke alive, so dragging off the mesh and back
    /// on continues the same stroke. A hit with no stroke in flight (the
    /// drag opened on a miss) starts the stroke there.
    pub fn extend_screen_point(&mut self, hit: Option<Pos2>) {
        if let Some(uv) = hit {
            if self.stroking {
                self.extend_stroke(uv);
            } else {
                self.begin_stroke(uv);
            }
        }
    }

    /// The mirror manipulator: moves the mirror line to `offset`
    /// (tile-local UV, clamped to `0..=1`) and queues the hand-back to
    /// the panel.
    pub fn set_mirror_offset(&mut self, offset: f32) {
        let offset = offset.clamp(0.0, 1.0);
        if self.symmetry.mirror.offset != offset {
            self.symmetry.mirror.offset = offset;
            self.symmetry_from_view = true;
        }
    }

    /// Whether the clone tool is active (the UV view's alt-click gate).
    pub fn clone_tool_active(&self) -> bool {
        matches!(self.tool, BrushTool::Clone(_))
    }

    /// The alt-click affordance: arms `uv` as the clone source. The next
    /// clone stroke resolves it into the offset (`source - stroke start`).
    pub fn set_clone_source(&mut self, uv: Pos2) {
        self.clone_source = Some(uv);
        self.clone_source_fresh = true;
    }

    /// The quick mask (the UV view draws its overlay from it, the panel
    /// its rows).
    pub fn quick_mask(&self) -> &QuickMask {
        &self.quick_mask
    }

    /// The X shortcut: enters or leaves mask mode (entering arms gating,
    /// see `umber_brush::quick_mask`); returns the new state. A live
    /// stroke keeps the mode it began with.
    pub fn toggle_mask_mode(&mut self) -> bool {
        self.quick_mask.toggle_editing()
    }

    /// Enters (`true`) or leaves mask mode — the panel's toggle.
    pub fn set_mask_mode(&mut self, editing: bool) {
        self.quick_mask.set_editing(editing);
    }

    /// Arms or disarms gating. Disarming keeps the mask for the session.
    pub fn set_mask_active(&mut self, active: bool) {
        self.quick_mask.set_active(active);
    }

    /// What mask-mode strokes paint (white allows, black blocks).
    pub fn set_mask_paint(&mut self, paint: MaskPaint) {
        self.quick_mask.set_paint(paint);
    }

    /// Inverts the mask (the I shortcut in mask mode, or the panel
    /// button). Refused (`false`) mid-stroke: a live gated stroke must
    /// keep reading one mask.
    pub fn invert_mask(&mut self) -> bool {
        if self.stroking {
            return false;
        }
        self.quick_mask.invert();
        self.mask_synced.clear();
        true
    }

    /// Zeroes the mask (the panel's Clear). Refused (`false`) mid-stroke,
    /// like [`Self::invert_mask`].
    pub fn clear_mask(&mut self) -> bool {
        if self.stroking {
            return false;
        }
        self.quick_mask.clear();
        self.mask_synced.clear();
        true
    }

    /// Reverts the last stroke (paint or retouch — one entry per stroke);
    /// `false` when there is none.
    pub fn undo_stroke(&mut self) -> bool {
        let undone = self.retouch_history.undo(&mut self.retouch_writes);
        self.flush_retouch_writes();
        undone
    }

    /// Re-applies the last undone stroke (paint or retouch); `false` when
    /// there is none.
    pub fn redo_stroke(&mut self) -> bool {
        let redone = self.retouch_history.redo(&mut self.retouch_writes);
        self.flush_retouch_writes();
        redone
    }

    /// Whether [`Self::undo_stroke`] would do anything.
    pub fn can_undo_stroke(&self) -> bool {
        self.retouch_history.can_undo()
    }

    /// Whether [`Self::redo_stroke`] would do anything.
    pub fn can_redo_stroke(&self) -> bool {
        self.retouch_history.can_redo()
    }

    /// Queues the retouch history's pending region writes on their tiles.
    fn flush_retouch_writes(&mut self) {
        for (tile, rect, rgba) in self.retouch_writes.writes.drain(..) {
            if let Some(thread) = self.targets.get_mut(&tile) {
                let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                    x: rect.x0,
                    y: rect.y0,
                    width: rect.width(),
                    height: rect.height(),
                    rgba,
                }]);
            }
        }
    }

    /// Stages a fill layer's evaluated tile bytes into `tile`'s GPU target
    /// (the fill layer's tile/texture-set plumbing: evaluation is CPU-side
    /// in `fill_layer`, the upload rides the same `WriteRegion` queue as
    /// retouch strokes — one full-tile write, in FIFO order with the
    /// tile's staged dabs).
    ///
    /// Creates the tile target lazily on first use (like pointer routing).
    /// Returns `false` — dropping the write with a log — when the target
    /// cannot be created or `rgba` is not `width`x`height` RGBA8 (the fill
    /// must re-evaluate at the target size first; a torn upload is worse
    /// than a dropped one).
    pub(crate) fn publish_fill(
        &mut self,
        tile: u16,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> bool {
        if rgba.len() != width as usize * height as usize * 4 {
            log::warn!(
                "fill: dropping {width}x{height} upload for tile {tile} ({} bytes)",
                rgba.len()
            );
            return false;
        }
        if !self.ensure_tile(tile) {
            return false;
        }
        if let Some(thread) = self.targets.get_mut(&tile) {
            let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                x: 0,
                y: 0,
                width,
                height,
                rgba,
            }]);
            true
        } else {
            false
        }
    }

    /// Extend the stroke to `uv`.
    pub fn extend_stroke(&mut self, uv: Pos2) {
        self.extend_stroke_styled(uv, 1.0, 1.0);
    }

    /// Extend the stroke to `uv` with an explicit pressure and
    /// brush-radius scale (the path replay's per-point style; live strokes
    /// use `1.0` for both through [`Self::extend_stroke`]).
    pub fn extend_stroke_styled(&mut self, uv: Pos2, pressure: f32, radius_scale: f32) {
        if !self.stroking {
            return;
        }
        self.push_event_styled(uv, pressure, radius_scale, true);
    }

    /// End the stroke (flushes nothing extra: `process_pending` drains
    /// all). A retouch stroke that changed texels becomes ONE retouch
    /// history entry, and so does a paint-tool stroke that wrote texels
    /// (its before snapshot plus the drained after-bytes — see the module
    /// docs).
    pub fn end_stroke(&mut self) {
        self.stroking = false;
        if let Some(command) = self.retouch.take().and_then(RetouchStroke::finish) {
            self.retouch_history.push(command, &mut self.retouch_writes);
            // `push` applies the command (queues its "after" bytes); the
            // stroke already uploaded them event by event.
            self.retouch_writes.writes.clear();
        }
        if let Some(capture) = self.paint_capture.take() {
            if let Some(command) = self.finish_paint_capture(capture) {
                self.retouch_history.push(command, &mut self.retouch_writes);
                // Same as above: the after-bytes are already on the GPU
                // (drained below to read them), so the queued re-apply is
                // dropped.
                self.retouch_writes.writes.clear();
            }
        }
    }

    /// Ends a paint stroke's undo capture: drains each touched tile, reads
    /// it back, and slices the AFTER bytes out of the stroke's dirty rect
    /// next to the BEFORE snapshot. Tiles with no dirty rect, an unreadable
    /// tile, a rect that no longer fits the target (a resize landed
    /// mid-stroke), or byte-identical before/after (a fully gated-away
    /// stroke) contribute no region; no regions means no entry (`None`).
    fn finish_paint_capture(&mut self, capture: PaintStrokeCapture) -> Option<PaintStrokeCommand> {
        let mut regions = Vec::new();
        for (tile, entry) in capture.tiles {
            let Some(rect) = entry.dirty else {
                continue;
            };
            let Some(after_image) = self.read_tile(tile) else {
                continue;
            };
            if rect.x1 > after_image.width || rect.y1 > after_image.height {
                continue;
            }
            let before = entry.before.region(rect);
            let after = after_image.region(rect);
            if before == after {
                continue;
            }
            regions.push(TileRegion {
                tile,
                rect,
                before,
                after,
            });
        }
        (!regions.is_empty()).then_some(PaintStrokeCommand { regions })
    }

    /// Replays `path` as ONE stroke in the `op` mode: each point becomes a
    /// styled event (its folded pressure, its radius scale) through the
    /// EXISTING pipeline — routing, seam mirrors, symmetry, wash, mask,
    /// then the splat for paint or the CPU mirror for erase/smudge (the
    /// consumer `op` already selects). A closed path appends its sealing
    /// segment, like `umber_brush::path`'s headless replay.
    ///
    /// `false` (nothing staged) with fewer than 2 points or while a live
    /// stroke runs — a replay never splits a stroke. The session tool is
    /// restored afterwards, so replaying never disturbs the panel tool.
    pub fn replay_path(&mut self, path: &PaintPath, op: &PathOp) -> bool {
        if self.stroking || path.len() < 2 {
            return false;
        }
        let saved = self.tool;
        self.tool = op.to_brush_tool();
        let points = path.points();
        let first = &points[0];
        self.begin_stroke_styled(
            Pos2::new(first.uv[0], first.uv[1]),
            first.effective_pressure(),
            first.sanitized_size(),
        );
        for point in &points[1..] {
            self.extend_stroke_styled(
                Pos2::new(point.uv[0], point.uv[1]),
                point.effective_pressure(),
                point.sanitized_size(),
            );
        }
        if path.is_closed() {
            self.extend_stroke_styled(
                Pos2::new(first.uv[0], first.uv[1]),
                first.effective_pressure(),
                first.sanitized_size(),
            );
        }
        self.end_stroke();
        self.tool = saved;
        true
    }

    /// Stamps a closed path's interior on the active tile: the loop runs
    /// through the polygon fill's even-odd rasterizer
    /// ([`rasterize_polygon_mask`], reused — no duplicated parity math),
    /// and each covered row run replays as one stroke at the path's mean
    /// style through the same pipeline as [`Self::replay_path`].
    ///
    /// `false` (nothing staged) unless the path can fill, while a live
    /// stroke runs, or when the loop covers no texel of the active tile.
    /// V1 boundaries: the active tile only (a loop spanning tiles fills
    /// the tile under the last pointer — per-event routing is the named
    /// follow-up), and one stroke per row run (a retouch fill lands as one
    /// undo entry per run — coalescing them is a follow-up).
    pub fn replay_filled_path(&mut self, path: &PaintPath, op: &PathOp) -> bool {
        if self.stroking || !path.can_fill() {
            return false;
        }
        let (width, height) = self.target_size;
        if width == 0 || height == 0 {
            return false;
        }
        let tile = self.active_tile;
        let mask = rasterize_polygon_mask(&path.loop_uv(), tile, width, height);
        let runs = fill_runs(&mask, width as usize, height as usize);
        if runs.is_empty() {
            return false;
        }
        let saved = self.tool;
        self.tool = op.to_brush_tool();
        let (pressure, size) = path.mean_style();
        for run in &runs {
            let start = texel_center_uv(tile, run.x0, run.y, self.texels_per_uv);
            self.begin_stroke_styled(Pos2::new(start[0], start[1]), pressure, size);
            for x in run.x0 + 1..=run.x1 {
                let uv = texel_center_uv(tile, x, run.y, self.texels_per_uv);
                self.extend_stroke_styled(Pos2::new(uv[0], uv[1]), pressure, size);
            }
            self.end_stroke();
        }
        self.tool = saved;
        true
    }

    /// Drains pending paint commands on EVERY tile's thread (mirrors and
    /// strokes that left a tile may have queued work off the active tile)
    /// and returns the cumulative stats summed across tiles. With one tile
    /// this is exactly that thread's stats.
    ///
    /// # Errors
    ///
    /// Propagates GPU-processing failures.
    pub fn process_pending(&mut self) -> Result<FrameStats, umber_gpu::paint::PaintError> {
        let mut total = FrameStats::default();
        for thread in self.targets.values_mut() {
            let stats = thread.process_pending()?;
            total.dabs_composited += stats.dabs_composited;
            total.dispatches += stats.dispatches;
            total.clears += stats.clears;
            total.segments += stats.segments;
        }
        self.last_stats = total;
        Ok(self.last_stats)
    }

    /// Cumulative stats of the last processed frame.
    pub fn last_stats(&self) -> FrameStats {
        self.last_stats
    }

    /// Read-only access to the ACTIVE tile's paint target (for display
    /// callbacks and the single-target export paths).
    pub fn paint_target(&self) -> &umber_gpu::paint::PaintTarget {
        self.targets
            .get(&self.active_tile)
            .or_else(|| self.targets.get(&FIRST_TILE))
            .expect("tile 1001 is created in PaintState::new and never removed")
            .paint_target()
    }

    /// Read-only access to `tile`'s paint target, or `None` when nothing
    /// has routed to that tile yet (the per-tile export reads each
    /// present tile through this).
    pub fn tile_target(&self, tile: u16) -> Option<&umber_gpu::paint::PaintTarget> {
        self.targets.get(&tile).map(PaintThread::paint_target)
    }

    /// The tiles this session holds a paint target for, ascending —
    /// tile 1001 always, plus every tile an event (or seam mirror) has
    /// routed to. The per-tile export iterates exactly these.
    pub fn tiles_present(&self) -> Vec<u16> {
        // BTreeMap keys iterate sorted.
        self.targets.keys().copied().collect()
    }

    /// Overrides the stroke color (test hook: the per-tile export test
    /// paints each tile a different color).
    #[cfg(test)]
    pub(crate) fn set_brush_color(&mut self, color: [f32; 4]) {
        self.brush.color = color;
    }

    /// The tile the last original pointer event routed to (v1: the tile
    /// the UV view displays, and whose quick mask it overlays).
    pub fn active_tile(&self) -> u16 {
        self.active_tile
    }

    /// Overrides the whole brush (test hook: the quick-mask tests paint
    /// hard full-alpha masks so their values are exactly 0 or 1).
    #[cfg(test)]
    pub(crate) fn set_brush_params(&mut self, params: BrushParams) {
        self.brush = params;
    }

    /// The edge lengths every tile's paint target has (as of the last
    /// [`Self::resize_target`]; applied on the next drain). The Export
    /// dialog's painted-source badge gates on this matching the export
    /// size.
    pub fn target_size(&self) -> (u32, u32) {
        self.target_size
    }

    /// Switches every tile's paint target to `width`x`height` (contents
    /// dropped); tiles created later use the same size. The stroke-soak
    /// test uses this to run against a 4K target, the 8K test against
    /// 8192²; the app itself stays on [`TARGET_SIZE`].
    pub fn resize_target(&mut self, width: u32, height: u32) {
        self.texels_per_uv = width as f32;
        self.target_size = (width, height);
        // Snapshots recorded at the old size no longer fit the targets.
        self.retouch_history.clear();
        // Nor does the mask: it resets with the targets' contents (the
        // `Resize` command drops each target's GPU mask too).
        self.quick_mask.resize(width, height);
        self.mask_synced.clear();
        for thread in self.targets.values_mut() {
            let _ = thread.publish(vec![PaintThreadCommand::Resize { width, height }]);
        }
    }

    /// The tile an event at `uv` routes to: `tile_of_uv` when the loaded
    /// mesh spans several tiles, else [`FIRST_TILE`].
    fn route_tile(&self, uv: Pos2) -> u16 {
        if self.multi_tile {
            tile_of_uv([uv.x, uv.y])
        } else {
            FIRST_TILE
        }
    }

    /// Ensures `tile` has a paint thread, creating it at the current
    /// target size on first use. Returns `false` (event dropped, logged)
    /// if creation fails — unreachable in practice, since tile 1001 was
    /// built on the same device.
    fn ensure_tile(&mut self, tile: u16) -> bool {
        if self.targets.contains_key(&tile) {
            return true;
        }
        let (width, height) = self.target_size;
        match PaintThread::new(self.device.clone(), self.queue.clone(), width, height) {
            Ok(thread) => {
                self.targets.insert(tile, thread);
                true
            }
            Err(err) => {
                log::warn!("paint target for tile {tile} failed: {err:#}");
                false
            }
        }
    }

    /// Brush footprint in UV units: the mirror gate distance
    /// (`BRUSH_RADIUS_TEXELS / texels_per_uv`, plus [`SEAM_MIRROR_MARGIN`).
    fn seam_radius_uv(&self) -> f32 {
        mirror_radius_uv(self.texels_per_uv)
    }

    /// UV-space seam proximity gate: true iff `uv` lies within `radius_uv`
    /// of any seam edge's `uv_a` or `uv_b` segment. `None` graph (no mesh
    /// loaded) is never near a seam.
    fn seam_near_uv(&self, uv: [f32; 2], radius_uv: f32) -> bool {
        self.seam_graph
            .as_ref()
            .is_some_and(|graph| uv_near_seam(graph, uv, radius_uv))
    }

    /// The stroke-event seam (every caller): position, pressure, an optional
    /// conditioner and the adapter both run at
    /// `BRUSH_RADIUS_TEXELS * sanitize_size(radius_scale)`, so a replayed
    /// path's per-point size varies the dab footprint through the SAME
    /// spacing math live strokes use. Scale `1.0` is byte-identical to
    /// brush-radius scale (`1.0` = the live-stroke default).
    fn push_event_styled(
        &mut self,
        uv: Pos2,
        pressure: f32,
        radius_scale: f32,
        expand_seams: bool,
    ) {
        profiling::scope!("stroke_eval");
        // Per-tile routing (UDIM slice 4): an ORIGINAL event picks the
        // active tile; mirrors route by their own UV below but never move
        // the active tile (the view keeps showing the tile under the
        // pointer). Crossing into another tile mid-stroke resets the
        // conditioner: local coordinates jump at the seam, and smoothing
        // or spacing across that jump would streak the new tile (v1:
        // per-event routing, no cross-tile continuity).
        if expand_seams {
            let tile = self.route_tile(uv);
            if !self.ensure_tile(tile) {
                return;
            }
            if tile != self.active_tile {
                self.conditioner.reset();
                self.active_tile = tile;
            }
        }
        // Seam-aware expansion at the UV layer, before texel conversion
        // (design slice 3, item 2): mirror positions become ADDITIONAL
        // `push_event_styled` calls at the mirrored UVs with
        // `expand_seams = false`, so mirrors recurse through the SAME texel
        // conversion + conditioning path (spacing, one-euro, lazy-mouse)
        // without re-expanding (recursion terminates by construction).
        //
        // The gate is two-deep: the stroke-level `stroke_near_seam` arm
        // (set by `begin_stroke`) skips the mirror call for seam-blind
        // strokes, and `mirror_positions` re-gates on UV-segment distance,
        // so far-from-seam events stage no extra batches.
        // Retouch strokes skip the mirror fan-out (v1): a smudge/clone dab
        // reads its own tile's mirror, and a mirrored clone source has no
        // defined counterpart across the seam.
        // Mask strokes do fan out: the mask covers the same 3D neighborhood
        // in both islands, whatever tool is selected.
        if expand_seams
            && self.stroke_near_seam
            && (self.stroke_mask_edit || !self.tool.is_retouch())
        {
            let radius_uv = self.seam_radius_uv();
            // Collect the mirrors first to end the graph borrow before the
            // recursive calls re-borrow `self`.
            let mirrors: Vec<[f32; 2]> = self
                .seam_graph
                .as_ref()
                .map(|graph| {
                    umber_mesh::seam_mirror::mirror_positions(&[[uv.x, uv.y]], graph, radius_uv)
                })
                .unwrap_or_default()
                .into_iter()
                .map(|mapping| mapping.mirrored)
                .collect();
            for mirrored in mirrors {
                self.push_event_styled(
                    Pos2::new(mirrored[0], mirrored[1]),
                    pressure,
                    radius_scale,
                    false,
                );
            }
        }
        // Mirrors across a tile seam land in the other tile: route by the
        // mirrored UV's own tile.
        let tile = if expand_seams {
            self.active_tile
        } else {
            let tile = self.route_tile(uv);
            if !self.ensure_tile(tile) {
                return;
            }
            tile
        };
        let texel = local_texel([uv.x, uv.y], tile, self.texels_per_uv);
        // Synthetic event clock (see EVENT_STEP_NS): every event — original
        // or mirror — advances time so the conditioner actually progresses.
        self.event_seq += 1;
        let event = StrokeEvent {
            pos: texel,
            pressure,
            tilt: None,
            time_ns: self.event_seq * EVENT_STEP_NS,
            in_proximity: true,
            contact: true,
        };
        let stamps = self
            .conditioner
            .push(&event, BRUSH_RADIUS_TEXELS * sanitize_size(radius_scale));
        if stamps.is_empty() {
            return;
        }
        let radius = BRUSH_RADIUS_TEXELS * sanitize_size(radius_scale);
        let dabs = DabAdapter::stamps_to_dabs(&stamps, radius, &self.brush);
        if dabs.is_empty() {
            return;
        }
        // The quick-mask edit hook: a mask-mode stroke stamps the MASK
        // with the same dabs a paint stroke would stage (symmetry siblings
        // included) and stages nothing. It runs before the retouch hook, so
        // mask mode wins over a selected smudge/clone tool.
        if self.stroke_mask_edit {
            let dabs = match (&self.stroke_symmetry, expand_seams) {
                (Some(transforms), true) => expand_dabs(&dabs, transforms),
                _ => dabs,
            };
            self.mask_event(tile, &dabs);
            return;
        }
        // The retouch hook: smudge/clone/eraser dabs (same positions,
        // radius, pressure-shaped alpha and hardness as paint dabs) run on
        // the CPU mirror instead of the GPU splat. Only the primaries go
        // in: the retouch stroke stamps their symmetry siblings itself (it
        // owns the per-sibling smudge chains and clone sources; the
        // motion-independent eraser needs no per-sibling state).
        if self.tool.is_retouch() {
            let retouch_dabs: Vec<RetouchDab> = dabs
                .iter()
                .map(|d| RetouchDab {
                    pos: d.pos,
                    radius: d.radius,
                    alpha: d.alpha,
                    hardness: d.hardness,
                })
                .collect();
            self.retouch_event(tile, &retouch_dabs);
            return;
        }
        // Wash mode (MyPaint opaque_linearize, CPU-side pre-splat): the
        // stroke's dabs are scaled so their overlap accumulates to the
        // stroke opacity instead of past it. This lands AFTER the retouch
        // hook, so smudge/clone/eraser and mask-mode strokes keep their
        // own (build-up) dab alphas untouched — and BEFORE symmetry
        // expansion, so siblings inherit the normalized alpha. No new GPU
        // surface and no shader arm: the scale is one scalar per event
        // batch, derived from the live conditioner density (the spacing
        // that actually produced these stamps).
        let dabs = apply_wash_mode(
            dabs,
            self.brush.alpha,
            self.wash_mode,
            self.conditioner.dabs_per_radius(),
        );
        // The symmetry hook: every original event's dab is followed by its
        // siblings (same stamp, transformed position) in the same batch.
        // Seam mirrors (`expand_seams == false`) are not symmetrized (v1).
        let dabs = match (&self.stroke_symmetry, expand_seams) {
            (Some(transforms), true) => expand_dabs(&dabs, transforms),
            _ => dabs,
        };
        // The quick-mask gate, AFTER symmetry expansion: every staged dab
        // (primary, sibling, seam mirror) carries the shader's gate flag,
        // and the shader multiplies its per-texel coverage by this tile's
        // mask texture, which is synced first so it holds the current
        // plane.
        let dabs = if self.stroke_mask_gate {
            self.sync_mask_tile(tile);
            dabs.into_iter().map(|d| d.with_mask_gate(true)).collect()
        } else {
            dabs
        };
        // The paint-stroke undo capture (see the module docs): the staged
        // dabs' footprints union into the stroke's dirty rect, snapshotting
        // the tile's before-bytes on first touch. Runs on the FINAL dabs
        // (post symmetry expansion and mask gating) so siblings are
        // covered; seam mirrors recurse through this same staging for their
        // own tiles.
        if self.paint_capture.is_some() {
            self.capture_paint_event(tile, &dabs);
        }
        // Stage as one command; the thread splits into capacity segments
        // and overlap-safe dispatches.
        #[cfg(test)]
        let staged_n = dabs.len() as u64;
        if let Some(thread) = self.targets.get_mut(&tile) {
            let _ = thread.publish(vec![PaintThreadCommand::Stage { dabs }]);
        }
        // Test observability: count staged batches for the seam-blind
        // zero-cost contract (a publish here means one more `Stage`
        // command queued, whether or not the channel accepted it — the
        // channel is unreachable-closed in practice).
        #[cfg(test)]
        {
            self.staged_batches += 1;
            *self.staged_batches_per_tile.entry(tile).or_default() += 1;
        }
        // Soak/symmetry observability: total staged dabs.
        #[cfg(test)]
        {
            self.staged_dabs += staged_n;
        }
    }

    /// One event's staged paint dabs for the stroke's undo capture: the
    /// tile's before-bytes are snapshotted on the stroke's first staged
    /// batch there (drained first, so earlier strokes are in the bytes),
    /// then every dab's shader-identical footprint unions into the dirty
    /// rect. A tile whose readback fails records nothing — the stroke still
    /// paints there, but that tile has no before to restore (a dropped
    /// readback must never drop a stamp).
    fn capture_paint_event(&mut self, tile: u16, dabs: &[Dab]) {
        if self.paint_capture.is_none() {
            return;
        }
        let needs_snapshot = self
            .paint_capture
            .as_ref()
            .is_some_and(|capture| !capture.tiles.contains_key(&tile));
        if needs_snapshot {
            let image = self.read_tile(tile);
            let (Some(capture), Some(image)) = (self.paint_capture.as_mut(), image) else {
                return;
            };
            capture.tiles.insert(
                tile,
                PaintTileCapture {
                    before: image,
                    dirty: None,
                },
            );
        }
        let Some(capture) = self.paint_capture.as_mut() else {
            return;
        };
        let Some(entry) = capture.tiles.get_mut(&tile) else {
            return;
        };
        // The snapshot's own dims (not the session's): they are the target
        // the rect must fit at `end_stroke`.
        let (width, height) = (entry.before.width, entry.before.height);
        for dab in dabs {
            if let Some(rect) = footprint(dab.pos, dab.radius, width, height) {
                entry.dirty = DirtyRect::union_opt(entry.dirty, Some(rect));
            }
        }
    }

    /// One event's retouch dabs on `tile`: mirror the tile on the stroke's
    /// first touch, run the dabs on the mirror, and queue the event's dirty
    /// rect back onto the tile's GPU target.
    fn retouch_event(&mut self, tile: u16, dabs: &[RetouchDab]) {
        // Taken for the call: `read_tile` needs `&mut self`.
        let Some(mut stroke) = self.retouch.take() else {
            return;
        };
        if stroke.accepts(tile) && !stroke.has_tile(tile) {
            if let Some(image) = self.read_tile(tile) {
                stroke.insert_tile(tile, image);
            }
        }
        // The quick-mask gate: the retouch ops multiply their per-texel
        // coverage by the tile's CPU plane (allocated empty if untouched).
        let mask = if self.stroke_mask_gate {
            self.quick_mask.ensure_plane(tile);
            self.quick_mask.view(tile)
        } else {
            None
        };
        if let Some(rect) = stroke.apply_masked(tile, dabs, mask) {
            let rgba = stroke.region(tile, rect);
            if let Some(thread) = self.targets.get_mut(&tile) {
                let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                    x: rect.x0,
                    y: rect.y0,
                    width: rect.width(),
                    height: rect.height(),
                    rgba,
                }]);
            }
        }
        self.retouch = Some(stroke);
    }

    /// One mask-mode event on `tile`: stamps every dab into the quick mask
    /// and queues the touched rect onto the tile's GPU mask (the whole
    /// plane if that copy is stale).
    fn mask_event(&mut self, tile: u16, dabs: &[Dab]) {
        let mut dirty = None;
        for dab in dabs {
            let stamp = RetouchDab {
                pos: dab.pos,
                radius: dab.radius,
                alpha: dab.alpha,
                hardness: dab.hardness,
            };
            dirty = DirtyRect::union_opt(dirty, self.quick_mask.stamp(tile, &stamp));
        }
        if let Some(rect) = dirty {
            if self.mask_synced.contains(&tile) {
                self.publish_mask(tile, rect);
            } else {
                self.sync_mask_tile(tile);
            }
        }
    }

    /// Brings `tile`'s GPU mask in line with the CPU plane before gated
    /// dabs read it: a tile never uploaded (or marked stale by an
    /// invert/clear/resize) gets its whole plane. A no-op when in sync.
    fn sync_mask_tile(&mut self, tile: u16) {
        if self.mask_synced.contains(&tile) {
            return;
        }
        let (width, height) = self.quick_mask.dimensions();
        let full = DirtyRect {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        };
        self.publish_mask(tile, full);
    }

    /// Queues `rect` of `tile`'s quick mask (8-bit) as a `WriteMask` on
    /// the tile's thread, in FIFO order with its `Stage` commands.
    fn publish_mask(&mut self, tile: u16, rect: DirtyRect) {
        if rect.width() == 0 || rect.height() == 0 {
            return;
        }
        let values = self.quick_mask.gpu_bytes(tile, rect);
        let Some(thread) = self.targets.get_mut(&tile) else {
            return;
        };
        let _ = thread.publish(vec![PaintThreadCommand::WriteMask {
            x: rect.x0,
            y: rect.y0,
            width: rect.width(),
            height: rect.height(),
            values,
        }]);
        self.mask_synced.insert(tile);
        #[cfg(test)]
        {
            self.mask_uploads += 1;
        }
    }

    /// `tile`'s current texels as a CPU image: drains the tile's queue
    /// first (so earlier strokes' dabs are in the texture), then the
    /// blocking full-target `PaintTarget::read_back_rgba8`. `None` (logged;
    /// the stroke skips the tile) when the drain or the readback fails.
    pub(crate) fn read_tile(&mut self, tile: u16) -> Option<RgbaBuffer> {
        let thread = self.targets.get_mut(&tile)?;
        if let Err(err) = thread.process_pending() {
            log::warn!("retouch: draining tile {tile} failed: {err:#}");
            return None;
        }
        let target = thread.paint_target();
        let (width, height) = target.dimensions();
        match target.read_back_rgba8(&self.device, &self.queue) {
            Ok(bytes) => RgbaBuffer::new(width, height, bytes),
            Err(err) => {
                log::warn!("retouch: reading tile {tile} back failed: {err:#}");
                None
            }
        }
    }

    /// Staged `Stage`-batch count since construction (test observability;
    /// see the seam-blind zero-cost contract in the wiring tests below).
    #[cfg(test)]
    pub(crate) fn staged_batch_count(&self) -> usize {
        self.staged_batches
    }

    /// Staged `Stage`-batch count routed to `tile` since construction
    /// (test observability for the routing contract).
    #[cfg(test)]
    pub(crate) fn staged_batch_count_for(&self, tile: u16) -> usize {
        self.staged_batches_per_tile
            .get(&tile)
            .copied()
            .unwrap_or(0)
    }

    /// Number of tile targets created so far (test observability for lazy
    /// creation).
    #[cfg(test)]
    pub(crate) fn tile_count(&self) -> usize {
        self.targets.len()
    }

    /// Staged dabs since construction (test observability; see the
    /// stroke soak's zero-drop assert in `perf_soak` and the symmetry
    /// staging test).
    #[cfg(test)]
    pub(crate) fn staged_dab_count(&self) -> u64 {
        self.staged_dabs
    }

    /// Queued `WriteMask` commands since construction (test
    /// observability; see the quick-mask tests).
    #[cfg(test)]
    pub(crate) fn mask_upload_count(&self) -> usize {
        self.mask_uploads
    }
}

/// Tile-local texel coordinates of `uv` inside `tile` on a target with
/// `texels_per_uv` texels per UV unit: the tile's integer UV offsets
/// (`(tile - 1001) % 10`, `(tile - 1001) / 10`) are subtracted before the
/// usual V-flipped texel mapping. For tile 1001 the offsets are zero and
/// the result is bit-identical to the pre-UDIM `[u * tpv, (1 - v) * tpv]`.
fn local_texel(uv: [f32; 2], tile: u16, texels_per_uv: f32) -> [f32; 2] {
    let index = tile.saturating_sub(FIRST_TILE);
    let tile_u = f32::from(index % 10);
    let tile_v = f32::from(index / 10);
    [
        (uv[0] - tile_u) * texels_per_uv,
        (1.0 - (uv[1] - tile_v)) * texels_per_uv,
    ]
}

/// A UV-space clone offset in tile-local texels: scaled by the texel
/// density, V negated (texel rows grow downward — [`local_texel`]'s flip).
fn clone_offset_texels(offset_uv: (f32, f32), texels_per_uv: f32) -> [f32; 2] {
    [offset_uv.0 * texels_per_uv, -offset_uv.1 * texels_per_uv]
}

/// A tile's integer UV offsets (the inverse of [`local_texel`]'s
/// subtraction): `(tile - 1001) % 10` in U, `(tile - 1001) / 10` in V.
fn tile_uv_offset(tile: u16) -> (f32, f32) {
    let index = tile.saturating_sub(FIRST_TILE);
    (f32::from(index % 10), f32::from(index / 10))
}

/// Global UV of a tile-local texel CENTER (the exact inverse of
/// [`local_texel`], so a filled path's scan rows land on the texels the
/// rasterizer covered).
fn texel_center_uv(tile: u16, x: u32, y: u32, texels_per_uv: f32) -> [f32; 2] {
    let (tile_u, tile_v) = tile_uv_offset(tile);
    [
        tile_u + (x as f32 + 0.5) / texels_per_uv,
        tile_v + 1.0 - (y as f32 + 0.5) / texels_per_uv,
    ]
}

/// One maximal x-contiguous covered span in a tile mask row: the filled
/// path replays each as one stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FillRun {
    y: u32,
    x0: u32,
    x1: u32,
}

/// Row runs of the covered texels in `mask` (row-major, `width`x`height`):
/// maximal x-contiguous `true` spans, rows top-down. A short `mask`
/// degrades to no runs (never a panic — a torn rasterizer output must not
/// crash the fill).
fn fill_runs(mask: &[bool], width: usize, height: usize) -> Vec<FillRun> {
    let mut runs = Vec::new();
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let covered = mask.get(y * width + x).copied().unwrap_or(false);
            if !covered {
                x += 1;
                continue;
            }
            let x0 = x;
            x += 1;
            while x < width && mask.get(y * width + x).copied().unwrap_or(false) {
                x += 1;
            }
            runs.push(FillRun {
                y: y as u32,
                x0: x0 as u32,
                x1: (x - 1) as u32,
            });
        }
    }
    runs
}

/// Brush footprint in UV units for a target with `texels_per_uv` texels
/// per UV unit: `BRUSH_RADIUS_TEXELS * SEAM_MIRROR_MARGIN / texels_per_uv`.
/// Free function so the radius math is unit-testable without a GPU-backed
/// [`PaintState`].
fn mirror_radius_uv(texels_per_uv: f32) -> f32 {
    (BRUSH_RADIUS_TEXELS * SEAM_MIRROR_MARGIN) / texels_per_uv
}

/// The wash-mode hook in `push_event_styled`, factored out so it is
/// unit-testable without a GPU-backed [`PaintState`]: build-up dabs pass
/// through untouched; wash dabs have every alpha scaled by
/// `wash_scale(stroke_alpha, overlap)` so their overlap accumulates to
/// the stroke opacity. Positions, radii, colors, hardness and mask gates
/// are never touched — only the flow multiplier.
fn apply_wash_mode(dabs: Vec<Dab>, stroke_alpha: f32, mode: WashMode, overlap: f32) -> Vec<Dab> {
    if !matches!(mode, WashMode::Wash) {
        return dabs;
    }
    let scale = wash_scale(stroke_alpha, overlap);
    dabs.into_iter()
        .map(|mut d| {
            d.alpha *= scale;
            d
        })
        .collect()
}

/// UV-space seam proximity: true iff `uv` lies within `radius_uv` of any
/// seam edge's `uv_a` segment OR `uv_b` segment (segment distance, the same
/// gate math as `seam_mirror`'s internal check). Free function so the gate
/// is unit-testable without a GPU-backed [`PaintState`]; the
/// [`PaintState::seam_near_uv`] method wraps it with the stored graph.
fn uv_near_seam(graph: &SeamGraph, uv: [f32; 2], radius_uv: f32) -> bool {
    graph.edges.iter().any(|edge| {
        dist_to_segment(uv, edge.uv_a[0], edge.uv_a[1]) <= radius_uv
            || dist_to_segment(uv, edge.uv_b[0], edge.uv_b[1]) <= radius_uv
    })
}

/// Euclidean distance from `p` to the segment `a--b` (not the infinite
/// line; extrapolation past the endpoints counts only via endpoint
/// closeness). Zero-length segments degrade to distance-to-`a`, never NaN.
fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len_sq = ab[0] * ab[0] + ab[1] * ab[1];
    if len_sq == 0.0 {
        let dx = p[0] - a[0];
        let dy = p[1] - a[1];
        return (dx * dx + dy * dy).sqrt();
    }
    let s = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len_sq;
    let closest = if s <= 0.0 {
        a
    } else if s >= 1.0 {
        b
    } else {
        [a[0] + ab[0] * s, a[1] + ab[1] * s]
    };
    let dx = p[0] - closest[0];
    let dy = p[1] - closest[1];
    (dx * dx + dy * dy).sqrt()
}

/// Extracts a UV-space position from a pointer interaction on a UV-square
/// `rect` (same layout math as `uv_view::square_for`), or `None` outside.
pub fn uv_from_pointer(rect: egui::Rect, pos: Pos2) -> Option<Pos2> {
    if !rect.contains(pos) {
        return None;
    }
    Some(Pos2::new(
        (pos.x - rect.left()) / rect.width(),
        1.0 - (pos.y - rect.top()) / rect.height(),
    ))
}

/// Seam-free 2x1 strip with continuous UVs spanning [0,2]x[0,1]
/// (position == UV): the left quad's triangles start at u=0 (tile 1001),
/// the right quad's at u=1 (tile 1002). Shared vertices, so no seam
/// edges — tile routing is the only thing under test. Shared by the
/// routing tests here and the per-tile export test in `bake_sources`.
#[cfg(test)]
pub(crate) fn two_tile_strip() -> umber_mesh::MeshData {
    let uvs = vec![
        [0.0, 0.0], // 0
        [1.0, 0.0], // 1
        [2.0, 0.0], // 2
        [0.0, 1.0], // 3
        [1.0, 1.0], // 4
        [2.0, 1.0], // 5
    ];
    umber_mesh::MeshData {
        positions: uvs
            .iter()
            .map(|uv: &[f32; 2]| [uv[0], uv[1], 0.0])
            .collect(),
        normals: vec![[0.0, 0.0, 1.0]; 6],
        uvs,
        indices: vec![0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4],
        material_names: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_mesh::seam::build_seam_graph;
    use umber_mesh::seam_mirror::mirror_positions;

    #[test]
    fn uv_mapping_round_trips_and_rejects_outside() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        // Bottom-left of the square = UV origin (0,0).
        let uv = uv_from_pointer(rect, egui::pos2(0.0, 100.0)).expect("inside");
        assert!((uv.x - 0.0).abs() < 1e-6 && (uv.y - 0.0).abs() < 1e-6);
        // Top-right = UV (1,1).
        let uv = uv_from_pointer(rect, egui::pos2(100.0, 0.0)).expect("inside");
        assert!((uv.x - 1.0).abs() < 1e-6 && (uv.y - 1.0).abs() < 1e-6);
        // Outside rejected.
        assert!(uv_from_pointer(rect, egui::pos2(101.0, 50.0)).is_none());
    }

    #[test]
    fn clone_offset_texels_matches_local_texel_deltas() {
        // The tracker's texel offset must land exactly where local_texel
        // maps the alt-clicked source (V flip included).
        let start = [0.25, 0.75];
        let source = [0.5, 0.25];
        let offset_uv = (source[0] - start[0], source[1] - start[1]);
        let a = local_texel(start, FIRST_TILE, 512.0);
        let b = local_texel(source, FIRST_TILE, 512.0);
        let off = clone_offset_texels(offset_uv, 512.0);
        assert_eq!([a[0] + off[0], a[1] + off[1]], b);
    }

    #[test]
    fn default_brush_params_are_sane() {
        let p = BrushParams::default();
        assert!(p.alpha > 0.0 && p.alpha <= 1.0);
        assert!(p.hardness >= 0.0 && p.hardness <= 1.0);
    }

    #[test]
    fn wash_hook_passes_build_up_through_untouched() {
        let dabs = vec![
            Dab::new([1.0, 2.0], 24.0, [1.0, 0.0, 0.0, 1.0], 0.4, 0.5),
            Dab::new([3.0, 4.0], 24.0, [1.0, 0.0, 0.0, 1.0], 0.7, 0.5),
        ];
        let back = apply_wash_mode(dabs.clone(), 0.8, WashMode::BuildUp, 5.0);
        assert_eq!(back.len(), 2);
        for (got, want) in back.iter().zip(dabs.iter()) {
            assert!((got.alpha - want.alpha).abs() < 1e-7);
            assert_eq!(got.pos, want.pos);
            assert_eq!(got.color, want.color);
        }
    }

    #[test]
    fn wash_hook_normalizes_to_stroke_opacity() {
        // Stroke opacity 0.5, overlap 5 (the live conditioner density):
        // every dab alpha scales by wash_scale, so five overlapping dabs
        // fold to 0.5 — while the same dabs unscaled fold past it.
        let dabs = vec![Dab::new([8.0, 8.0], 24.0, [1.0, 0.0, 0.0, 1.0], 0.5, 1.0); 5];
        let washed = apply_wash_mode(dabs.clone(), 0.5, WashMode::Wash, 5.0);
        let scale = wash_scale(0.5, 5.0);
        for dab in &washed {
            assert!((dab.alpha - 0.5 * scale).abs() < 1e-7);
        }
        let folded: f32 = 1.0 - (1.0 - washed[0].alpha).powi(5);
        assert!(
            (folded - 0.5).abs() < 1e-5,
            "five wash dabs must fold to the stroke opacity, got {folded}"
        );
        let buildup: f32 = 1.0 - (1.0f32 - 0.5f32).powi(5);
        assert!(
            (buildup - 0.96875).abs() < 1e-6 && (buildup - folded).abs() > 0.4,
            "build-up must accumulate well past the wash ceiling"
        );
    }

    // --- Paint-what-you-see projection (§2's P1 row) ---
    //
    // The viewport raycasts each pointer to a hit UV (or a miss); these
    // tests pin the session side of that contract. The ray side
    // (`Viewport::pick_uv`: a known ray hits the known UV, corners miss)
    // is pinned in `viewport.rs`.

    #[test]
    fn projection_default_is_screen_and_routes_the_viewport() {
        // Screen is today's behavior (both surfaces paint), so existing
        // sessions are untouched; Uv parks viewport painting (orbit).
        assert_eq!(PaintProjection::default(), PaintProjection::Screen);
        assert!(PaintProjection::Screen.viewport_paints());
        assert!(!PaintProjection::Uv.viewport_paints());
    }

    /// Drives `points` through the Screen entry points: the first through
    /// `begin_screen_point`, the rest through `extend_screen_point`.
    fn drive_screen(state: &mut PaintState, points: &[Option<[f32; 2]>]) {
        let mut first = true;
        for point in points {
            let hit = point.map(|[x, y]| egui::pos2(x, y));
            if first {
                state.begin_screen_point(hit);
                first = false;
            } else {
                state.extend_screen_point(hit);
            }
        }
    }

    #[test]
    fn screen_miss_opens_no_stroke_and_stages_nothing() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        state.begin_screen_point(None);
        assert!(!state.is_stroking(), "a miss must not open a stroke");
        state.extend_screen_point(None);
        assert!(!state.is_stroking());
        assert_eq!(state.staged_batch_count(), 0, "no hits, no stages");
        assert_eq!(state.staged_dab_count(), 0);
        state.end_stroke();
    }

    #[test]
    fn screen_hit_after_opening_miss_starts_the_stroke_there() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        // The drag opens off the mesh, then moves onto it: the stroke
        // starts at the first hit, with no orphan state from the misses.
        drive_screen(&mut state, &[None, None, Some([0.3, 0.3])]);
        assert!(state.is_stroking());
        state.end_stroke();
    }

    #[test]
    fn screen_misses_leave_no_trace_in_the_conditioned_stream() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Misses never touch the conditioner, so a screen stroke with
        // interleaved misses stages EXACTLY what the hits-only UV stroke
        // stages (same batches, same dabs, same order).
        let hits = [[0.3, 0.3], [0.32, 0.32], [0.34, 0.34], [0.36, 0.36]];
        let mut direct =
            PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        direct.begin_stroke(egui::pos2(hits[0][0], hits[0][1]));
        for [x, y] in &hits[1..] {
            direct.extend_stroke(egui::pos2(*x, *y));
        }
        direct.end_stroke();

        let mut screen = PaintState::new(device, queue).expect("paint state builds");
        drive_screen(
            &mut screen,
            &[
                Some(hits[0]),
                None,
                Some(hits[1]),
                None,
                None,
                Some(hits[2]),
                Some(hits[3]),
            ],
        );
        assert!(
            screen.is_stroking(),
            "hits keep the stroke alive across misses"
        );
        screen.end_stroke();
        assert_eq!(
            screen.staged_batch_count(),
            direct.staged_batch_count(),
            "misses stage nothing"
        );
        assert_eq!(
            screen.staged_dab_count(),
            direct.staged_dab_count(),
            "the hit stream conditions identically"
        );
    }

    #[test]
    fn projection_sync_applies_between_strokes_only() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        assert_eq!(state.projection(), PaintProjection::Screen);
        state.sync_projection(PaintProjection::Uv);
        assert_eq!(state.projection(), PaintProjection::Uv);
        // A mid-stroke swap would split one stroke across two surfaces.
        state.begin_stroke(egui::pos2(0.5, 0.5));
        state.sync_projection(PaintProjection::Screen);
        assert_eq!(state.projection(), PaintProjection::Uv);
        state.end_stroke();
        state.sync_projection(PaintProjection::Screen);
        assert_eq!(state.projection(), PaintProjection::Screen);
    }

    #[test]
    fn screen_stroke_inherits_seam_mirrors() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Same march as `seam_stroke_stages_original_plus_mirror_near_seam`
        // but through the Screen entry points: the resolved hit UVs fan
        // out through the shared `push_event_styled` seam path, so a
        // near-seam screen stroke stages original + mirror per event.
        let start = NEAR_SEAM_UV;
        let step = [0.02, 0.02];
        let march: Vec<Option<[f32; 2]>> = (0..=15)
            .map(|i| Some([start[0] + step[0] * i as f32, start[1] + step[1] * i as f32]))
            .collect();

        let mut base = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        base.set_mesh(&single_island_quad());
        drive_screen(&mut base, &march);
        let base_converged = base.staged_batch_count();
        assert!(base_converged >= 1);
        base.extend_screen_point(Some(egui::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        )));
        assert_eq!(
            base.staged_batch_count() - base_converged,
            1,
            "seam-free screen extend stages exactly the original"
        );
        base.end_stroke();

        let mut seam = PaintState::new(device, queue).expect("paint state builds");
        seam.set_mesh(&two_island_quad());
        drive_screen(&mut seam, &march);
        let seam_converged = seam.staged_batch_count();
        assert!(
            seam_converged > base_converged,
            "near-seam screen stroke must stage MORE than the baseline"
        );
        seam.extend_screen_point(Some(egui::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        )));
        assert_eq!(
            seam.staged_batch_count() - seam_converged,
            2,
            "near-seam screen extend stages original + mirror"
        );
        seam.end_stroke();
    }

    // --- Seam-aware wiring tests (slice 3) ---
    //
    // Fixtures mirror `umber_mesh::seam`'s split-quad layout exactly: quad
    // corners A=(0,0,0) B=(1,0,0) C=(1,1,0) D=(0,1,0), diagonal A-C. The
    // two-island variant duplicates the diagonal's vertices with
    // island-offset UVs, so the seam's `uv_a` segment is (0,0)→(1,1) and
    // `uv_b` is (10,10)→(11,11).

    /// Quad with continuous UVs: no seam edges.
    fn single_island_quad() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
        }
    }

    /// Same quad with the diagonal A-C cut across two UV islands: exactly
    /// one seam edge (`uv_a` (0,0)→(1,1), `uv_b` (10,10)→(11,11)).
    fn two_island_quad() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [0.0, 0.0, 0.0], // 0: A (tri 0)
                [1.0, 0.0, 0.0], // 1: B
                [1.0, 1.0, 0.0], // 2: C (tri 0)
                [0.0, 0.0, 0.0], // 3: A (tri 1, duplicated)
                [1.0, 1.0, 0.0], // 4: C (tri 1, duplicated)
                [0.0, 1.0, 0.0], // 5: D
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uvs: vec![
                [0.0, 0.0],   // A in tri 0
                [1.0, 0.0],   // B
                [1.0, 1.0],   // C in tri 0
                [10.0, 10.0], // A in tri 1 (other island)
                [11.0, 11.0], // C in tri 1 (other island)
                [11.0, 10.0], // D
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec![],
        }
    }

    /// A UV on the `uv_a` diagonal, ~0.021 UV units off the seam line:
    /// inside the mirror gate (radius 36/512 ≈ 0.070).
    const NEAR_SEAM_UV: [f32; 2] = [0.5, 0.53];
    /// A UV ~0.57 UV units from either island's segment: outside the gate.
    const FAR_FROM_SEAM_UV: [f32; 2] = [0.9, 0.1];

    fn gate_radius() -> f32 {
        mirror_radius_uv(TARGET_SIZE as f32)
    }

    #[test]
    fn mirror_radius_is_footprint_times_margin() {
        assert_eq!(SEAM_MIRROR_MARGIN, 1.5);
        assert!((gate_radius() - 36.0 / 512.0).abs() < 1e-6);
    }

    #[test]
    fn seam_gate_fires_near_seam_and_not_far() {
        let graph = build_seam_graph(&two_island_quad());
        assert_eq!(
            graph.edges.len(),
            1,
            "fixture must carry exactly one seam edge, got {:?}",
            graph.edges
        );
        let r = gate_radius();
        // The gate must fire near the seam's uv_a segment ...
        assert!(
            uv_near_seam(&graph, NEAR_SEAM_UV, r),
            "UV {NEAR_SEAM_UV:?} ~0.021 from the seam must be inside radius {r}"
        );
        // ... and stay silent far from both islands' segments. BOTH
        // directions are asserted: a gate stuck true/false fails here.
        assert!(
            !uv_near_seam(&graph, FAR_FROM_SEAM_UV, r),
            "UV {FAR_FROM_SEAM_UV:?} ~0.57 from the seam must be outside radius {r}"
        );
        // Seam-free mesh: the gate never fires (empty edge list).
        let plain = build_seam_graph(&single_island_quad());
        assert!(
            plain.edges.is_empty(),
            "single-island quad must yield zero seam edges, got {:?}",
            plain.edges
        );
        assert!(!uv_near_seam(&plain, NEAR_SEAM_UV, r));
        assert!(!uv_near_seam(&plain, FAR_FROM_SEAM_UV, r));
    }

    #[test]
    fn mirror_positions_fans_out_near_seam_only() {
        // Pins the exact call `push_event_styled` makes: one position, the
        // wiring radius, the two-island graph.
        let graph = build_seam_graph(&two_island_quad());
        let r = gate_radius();
        let near = mirror_positions(&[NEAR_SEAM_UV], &graph, r);
        assert_eq!(
            near.len(),
            1,
            "near-seam position must yield exactly one mirror, got {near:?}"
        );
        assert_eq!(near[0].source, 0);
        // Mirror lands in the OTHER island (`uv_b` lives at UV ~10–11).
        assert!(
            near[0].mirrored[0] > 9.0 && near[0].mirrored[1] > 9.0,
            "mirror must land in the neighbor island, got {:?}",
            near[0].mirrored
        );
        // Offset magnitude preserved across the map (La == Lb here): the
        // mirror's distance from the `uv_b` segment equals the source's
        // distance from `uv_a`.
        let d_src = dist_to_segment(NEAR_SEAM_UV, [0.0, 0.0], [1.0, 1.0]);
        let d_mir = dist_to_segment(near[0].mirrored, [10.0, 10.0], [11.0, 11.0]);
        assert!(
            (d_src - d_mir).abs() < 1e-4,
            "mirror offset {d_mir} must match source offset {d_src}"
        );
        // Far position: no mirrors (the zero-cost contract at the math
        // layer — a far event fans out to nothing).
        let far = mirror_positions(&[FAR_FROM_SEAM_UV], &graph, r);
        assert!(
            far.is_empty(),
            "far position must yield zero mirrors, got {far:?}"
        );
    }

    /// Requests a paint-capable device (graceful skip where no wgpu
    /// adapter exists — the same pattern as `umber_gpu::paint`'s GPU
    /// tests). Returns `None` when the test must be skipped.
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return None;
        };
        if !adapter
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ..Default::default()
        }))
        .ok()
    }

    /// Drives a stroke that stays inside the mirror gate the whole time:
    /// `begin` at `start`, then `steps` extends marching `step` per event.
    fn drive_stroke(state: &mut PaintState, start: [f32; 2], step: [f32; 2], steps: usize) {
        state.begin_stroke(egui::pos2(start[0], start[1]));
        for i in 1..=steps {
            state.extend_stroke(egui::pos2(
                start[0] + step[0] * i as f32,
                start[1] + step[1] * i as f32,
            ));
        }
    }

    #[test]
    fn seam_stroke_stages_original_plus_mirror_near_seam() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // March along the seam diagonal (stays ~0.021 off the line, gate
        // radius ~0.070); 15 events converge the conditioner, then ONE
        // measured extend pins the steady-state per-event cost.
        let start = NEAR_SEAM_UV;
        let step = [0.02, 0.02];

        // Baseline: seam-free mesh. The measured extend stages exactly the
        // original — 1 batch.
        let mut base = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        base.set_mesh(&single_island_quad());
        drive_stroke(&mut base, start, step, 15);
        let base_converged = base.staged_batch_count();
        assert!(
            base_converged >= 1,
            "converged baseline stroke must stage something (got {base_converged})"
        );
        base.extend_stroke(egui::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        assert_eq!(
            base.staged_batch_count() - base_converged,
            1,
            "seam-free extend stages exactly the original (1 batch)"
        );

        // Seam mesh, identical stroke: the measured extend stages original
        // + mirror — 2 batches (the mirror recurses with expand=false, so
        // no further fan-out: recursion terminates).
        let mut seam = PaintState::new(device, queue).expect("paint state builds");
        seam.set_mesh(&two_island_quad());
        drive_stroke(&mut seam, start, step, 15);
        let seam_converged = seam.staged_batch_count();
        assert!(
            seam_converged > base_converged,
            "near-seam stroke must stage MORE than the seam-free baseline \
             (seam {seam_converged} vs base {base_converged})"
        );
        seam.extend_stroke(egui::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        assert_eq!(
            seam.staged_batch_count() - seam_converged,
            2,
            "near-seam extend stages original + mirror (2 batches)"
        );
    }

    #[test]
    fn stroke_far_from_seam_pays_nothing() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // March through (0.9,0.1)→(0.58,0.42): closest approach ~0.11 UV,
        // still outside the ~0.070 gate the whole way.
        let start = FAR_FROM_SEAM_UV;
        let step = [-0.02, 0.02];

        let mut base = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        base.set_mesh(&single_island_quad());
        drive_stroke(&mut base, start, step, 15);
        let base_converged = base.staged_batch_count();
        base.extend_stroke(egui::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        let base_total = base.staged_batch_count();

        // Same stroke on the seam mesh: the stroke-start arm stays off, the
        // mirror call never runs, so the event stream — and every staged
        // batch — is IDENTICAL to the seam-free baseline (structural
        // equality, not just determinism: `push_event_styled` never consults
        // the graph when `stroke_near_seam` is false).
        let mut far = PaintState::new(device, queue).expect("paint state builds");
        far.set_mesh(&two_island_quad());
        drive_stroke(&mut far, start, step, 15);
        assert_eq!(
            far.staged_batch_count(),
            base_converged,
            "far-from-seam convergence must match the baseline exactly"
        );
        far.extend_stroke(egui::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        assert_eq!(
            far.staged_batch_count(),
            base_total,
            "far-from-seam totals must match the baseline exactly (zero-cost contract)"
        );
        assert_eq!(
            far.staged_batch_count() - base_converged,
            base_total - base_converged,
            "far-from-seam extend stages exactly what the baseline stages"
        );
    }

    // --- Retouch tools (smudge + clone) ---

    #[test]
    fn smudge_stroke_writes_back_and_undoes_as_one_entry() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        // A vertical paint stroke gives the smudge an edge to drag. Since the
        // stroke-undo work, THIS stroke records its own history entry — undo
        // it before the smudge begins so the smudge's own entry is the only
        // one (the test's "one entry" premise, restored).
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        state.process_pending().expect("drain succeeds");
        assert!(state.undo_stroke(), "the setup stroke has an entry");
        assert!(
            !state.can_undo_stroke(),
            "the setup stroke was the only one"
        );
        state.process_pending().expect("drain succeeds");
        let painted_batches = state.staged_batch_count();
        let before = state
            .paint_target()
            .read_back_rgba8(&device, &queue)
            .expect("readback");

        let mut tool = BrushTool::Smudge(umber_brush::SmudgeParams {
            strength: 1.0,
            smear: 0.5,
        });
        state.sync_tool(&mut tool);
        // A horizontal smudge across the painted line.
        drive_stroke(&mut state, [0.2, 0.5], [0.01, 0.0], 20);
        state.end_stroke();
        assert_eq!(
            state.staged_batch_count(),
            painted_batches,
            "retouch dabs never stage GPU splats"
        );
        assert!(state.can_undo_stroke(), "the smudge stroke is one entry");
        state.process_pending().expect("drain succeeds");
        let after = state
            .paint_target()
            .read_back_rgba8(&device, &queue)
            .expect("readback");
        assert_ne!(after, before, "the smudge must reach the GPU target");

        assert!(state.undo_stroke());
        assert!(!state.can_undo_stroke(), "one stroke, one entry");
        state.process_pending().expect("drain succeeds");
        let undone = state
            .paint_target()
            .read_back_rgba8(&device, &queue)
            .expect("readback");
        assert_eq!(undone, before, "undo restores the pre-stroke texels");
    }

    // --- Paint-stroke undo (CORE-LOOP CRITICAL 3) ---

    /// A paint stroke is ONE history entry: undo restores the exact
    /// pre-stroke bytes, redo the exact post-stroke bytes.
    #[test]
    fn paint_stroke_undoes_and_redoes_byte_exact() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        let blank = read_back(&mut state, &device, &queue);
        assert!(!state.can_undo_stroke(), "a fresh session has no entries");

        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(
            state.staged_dab_count() > 0,
            "the stroke must stage dabs (got {})",
            state.staged_dab_count()
        );
        assert!(state.can_undo_stroke(), "the paint stroke is one entry");
        let painted = read_back(&mut state, &device, &queue);
        assert_ne!(painted, blank, "the stroke must reach the GPU target");

        assert!(state.undo_stroke());
        assert!(!state.can_undo_stroke(), "one stroke, one entry");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "undo restores the pre-stroke texels"
        );

        assert!(state.redo_stroke());
        assert_eq!(
            read_back(&mut state, &device, &queue),
            painted,
            "redo restores the post-stroke texels"
        );
        assert!(!state.can_redo_stroke());
    }

    /// Two paint strokes are two entries: one undo reverts only the
    /// latest, leaving the first stroke's paint byte-identical in place.
    #[test]
    fn two_paint_strokes_undo_reverts_only_the_latest() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        let blank = read_back(&mut state, &device, &queue);

        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        let first = read_back(&mut state, &device, &queue);
        assert_ne!(first, blank, "the first stroke must paint");

        // Far from the first stroke: ~200 texels apart at r = 24, so the
        // second stroke's dirty rect cannot cover the first stroke's paint.
        drive_stroke(&mut state, [0.7, 0.7], [0.0, 0.02], 15);
        state.end_stroke();
        let second = read_back(&mut state, &device, &queue);
        assert_ne!(second, first, "the second stroke must paint");

        assert!(state.undo_stroke());
        assert!(state.can_undo_stroke(), "the first stroke's entry remains");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            first,
            "one undo reverts only the latest stroke"
        );

        assert!(state.undo_stroke());
        assert!(!state.can_undo_stroke(), "two strokes, two entries");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "the second undo reaches the blank target"
        );

        assert!(state.redo_stroke());
        assert_eq!(read_back(&mut state, &device, &queue), first);
        assert!(state.redo_stroke());
        assert_eq!(read_back(&mut state, &device, &queue), second);
        assert!(!state.can_redo_stroke());
    }

    // --- Symmetry ---

    /// Test (b), end to end: with the mirror on, the same stroke stages
    /// the same `Stage` batches (one per event — siblings ride in the
    /// original's batch) carrying exactly twice the dabs (the stroke stays
    /// clear of the axis, so no sibling dedups); with it off, the staging
    /// is unchanged. The conditioner sees only the pointer either way.
    #[test]
    fn mirrored_stroke_stages_twice_the_dabs_in_the_same_batches() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let start = [0.2, 0.3];
        let step = [0.0, 0.02];

        let mut off = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        drive_stroke(&mut off, start, step, 15);
        off.end_stroke();
        assert!(off.staged_dab_count() > 0);

        let mut on = PaintState::new(device, queue).expect("paint state builds");
        let mut settings = SymmetrySettings::default();
        settings.mirror.enabled = true;
        on.sync_symmetry(&mut settings);
        assert!(on.symmetry().mirror.enabled);
        drive_stroke(&mut on, start, step, 15);
        on.end_stroke();
        assert_eq!(on.staged_batch_count(), off.staged_batch_count());
        assert_eq!(on.staged_dab_count(), 2 * off.staged_dab_count());

        // Turning it back off restores single staging for the next stroke.
        settings.mirror.enabled = false;
        on.sync_symmetry(&mut settings);
        let (batches, dabs) = (on.staged_batch_count(), on.staged_dab_count());
        drive_stroke(&mut on, start, step, 15);
        on.end_stroke();
        assert_eq!(on.staged_batch_count() - batches, off.staged_batch_count());
        assert_eq!(on.staged_dab_count() - dabs, off.staged_dab_count());
    }

    #[test]
    fn manipulator_offset_is_handed_back_to_the_panel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        let mut panel = SymmetrySettings::default();
        panel.mirror.enabled = true;
        state.sync_symmetry(&mut panel);
        // The UV view drags the line: the panel adopts it next sync...
        state.set_mirror_offset(0.3);
        state.sync_symmetry(&mut panel);
        assert_eq!(panel.mirror.offset, 0.3);
        // ... and afterwards the panel leads again.
        panel.mirror.offset = 0.6;
        state.sync_symmetry(&mut panel);
        assert_eq!(state.symmetry().mirror.offset, 0.6);
        // Out-of-tile drags clamp to the tile.
        state.set_mirror_offset(1.7);
        assert_eq!(state.symmetry().mirror.offset, 1.0);
    }

    // --- Quick mask ---
    //
    // Every mask here is painted with a hard, full-alpha brush, so its
    // values are exactly 0 or 1 and "blocked" (mask == 0.0, uploaded as
    // byte 0) is a crisp texel set the before/after compares can use.
    // Probe texels sit on row v = 0.5 (y = 256): x = 102 is u ≈ 0.2,
    // x = 358 is u ≈ 0.7, x = 409 is u = 0.2's mirror about u = 0.5.

    fn hard_brush() -> BrushParams {
        BrushParams {
            alpha: 1.0,
            hardness: 1.0,
            ..BrushParams::default()
        }
    }

    /// Drains every queue, then reads tile 1001 back.
    fn read_back(state: &mut PaintState, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        state.process_pending().expect("drain succeeds");
        state
            .paint_target()
            .read_back_rgba8(device, queue)
            .expect("readback")
    }

    fn texel_at(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = (y * TARGET_SIZE + x) as usize * 4;
        [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
    }

    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    /// Texels of tile 1001 that differ between `a` and `b`, split by the
    /// quick mask there: `(blocked, allowed)`.
    fn changed_by_mask(state: &PaintState, a: &[u8], b: &[u8]) -> (usize, usize) {
        let mask = state.quick_mask();
        let mut out = (0, 0);
        for (i, (ta, tb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
            if ta == tb {
                continue;
            }
            let (x, y) = (i as u32 % TARGET_SIZE, i as u32 / TARGET_SIZE);
            if mask.sample(FIRST_TILE, x, y) == 0.0 {
                out.0 += 1;
            } else {
                out.1 += 1;
            }
        }
        out
    }

    /// Mask mode on, one hard white stroke allowing u ≈ 0.6..0.9 along
    /// v = 0.5, mask mode off. Everything else stays blocked (empty).
    fn mask_right_window(state: &mut PaintState) {
        assert!(state.toggle_mask_mode());
        drive_stroke(state, [0.6, 0.5], [0.02, 0.0], 15);
        state.end_stroke();
        assert!(!state.toggle_mask_mode());
    }

    /// Tests (a), (c), (e), (f) end to end:
    /// * (c) a mask-mode stroke stages nothing and leaves the layer
    ///   byte-identical; it edits the mask and uploads it;
    /// * (a) after leaving mask mode, a paint stroke across the whole row
    ///   leaves every blocked texel byte-identical and paints the window;
    /// * (e) deactivating stops gating (the same stroke paints the blocked
    ///   probe) and the mask survives exit and re-entry;
    /// * (f) clear zeroes it.
    #[test]
    fn quick_mask_gates_paint_and_mask_mode_paints_only_the_mask() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        assert!(state.quick_mask().coverage(FIRST_TILE).is_none());
        assert_eq!(state.mask_upload_count(), 0, "no mask, no upload");

        // (c)
        mask_right_window(&mut state);
        assert_eq!(
            state.staged_batch_count(),
            0,
            "a mask stroke stages nothing"
        );
        assert_eq!(state.staged_dab_count(), 0);
        assert!(state.mask_upload_count() > 0, "the mask reached the GPU");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "the layer is untouched by a mask stroke"
        );
        let mask = state.quick_mask();
        assert_eq!(
            mask.sample(FIRST_TILE, 358, 256),
            1.0,
            "white allowed the window"
        );
        assert_eq!(
            mask.sample(FIRST_TILE, 102, 256),
            0.0,
            "the rest stays blocked"
        );
        let painted_mask = mask.coverage(FIRST_TILE).expect("plane").to_vec();

        // (a)
        assert!(state.quick_mask().gates());
        drive_stroke(&mut state, [0.1, 0.5], [0.05, 0.0], 16);
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let (blocked, allowed) = changed_by_mask(&state, &blank, &gated);
        assert_eq!(blocked, 0, "blocked texels must be byte-identical");
        assert!(allowed > 0, "the window paints");
        assert_eq!(texel_at(&gated, 102, 256), CLEAR);
        assert_ne!(texel_at(&gated, 358, 256), CLEAR);

        // (e)
        state.set_mask_active(false);
        assert!(!state.quick_mask().gates());
        drive_stroke(&mut state, [0.1, 0.5], [0.05, 0.0], 16);
        state.end_stroke();
        let ungated = read_back(&mut state, &device, &queue);
        assert_ne!(
            texel_at(&ungated, 102, 256),
            CLEAR,
            "ungated paints everywhere"
        );
        let survived = |state: &PaintState| {
            state.quick_mask().coverage(FIRST_TILE) == Some(painted_mask.as_slice())
        };
        assert!(survived(&state), "deactivating keeps the mask");
        state.set_mask_mode(true);
        assert!(
            survived(&state),
            "re-entering mask mode shows the same mask"
        );
        state.set_mask_mode(false);
        assert!(state.quick_mask().gates(), "and re-arms gating");

        // (f)
        assert!(state.clear_mask());
        let plane = state.quick_mask().coverage(FIRST_TILE).expect("plane");
        assert!(plane.iter().all(|&v| v == 0.0), "clear zeroes");
    }

    /// Test (b), end to end: inverting flips the gating — the window the
    /// mask allowed is the one place the same stroke now leaves alone.
    #[test]
    fn inverted_quick_mask_flips_paint_gating() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        mask_right_window(&mut state);
        assert!(state.invert_mask());
        assert_eq!(state.quick_mask().sample(FIRST_TILE, 358, 256), 0.0);

        drive_stroke(&mut state, [0.1, 0.5], [0.05, 0.0], 16);
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let (blocked, allowed) = changed_by_mask(&state, &blank, &gated);
        assert_eq!(blocked, 0, "the inverted window is byte-identical");
        assert!(allowed > 0);
        assert_ne!(texel_at(&gated, 102, 256), CLEAR, "what was blocked paints");
        assert_eq!(
            texel_at(&gated, 358, 256),
            CLEAR,
            "what was allowed is blocked"
        );
        // Invert/clear are refused mid-stroke (one stroke, one mask).
        state.begin_stroke(egui::pos2(0.5, 0.5));
        assert!(!state.invert_mask() && !state.clear_mask());
        state.end_stroke();
    }

    /// Test (d), end to end: the gate applies AFTER symmetry expansion.
    /// With only the left side allowed, a mirrored stroke there paints its
    /// primary but its sibling (on the blocked right side) writes nothing;
    /// with the mask deactivated, the same sibling paints.
    #[test]
    fn quick_mask_gates_symmetry_siblings_on_the_gpu() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        // Allow u ≈ 0.1..0.4 along v = 0.5 (symmetry still off).
        assert!(state.toggle_mask_mode());
        drive_stroke(&mut state, [0.1, 0.5], [0.02, 0.0], 15);
        state.end_stroke();
        assert!(!state.toggle_mask_mode());
        assert!((256..TARGET_SIZE).all(|x| state.quick_mask().sample(FIRST_TILE, x, 256) == 0.0));

        let mut settings = SymmetrySettings::default();
        settings.mirror.enabled = true;
        state.sync_symmetry(&mut settings);
        drive_stroke(&mut state, [0.15, 0.5], [0.01, 0.0], 15);
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let (blocked, allowed) = changed_by_mask(&state, &blank, &gated);
        assert_eq!(blocked, 0, "no sibling texel lands on the blocked side");
        assert!(allowed > 0);
        assert_ne!(texel_at(&gated, 102, 256), CLEAR, "the primary paints");
        assert_eq!(texel_at(&gated, 409, 256), CLEAR, "its sibling is gated");

        // Control: ungated, the sibling paints.
        state.set_mask_active(false);
        drive_stroke(&mut state, [0.15, 0.5], [0.01, 0.0], 15);
        state.end_stroke();
        let ungated = read_back(&mut state, &device, &queue);
        assert_ne!(texel_at(&ungated, 409, 256), CLEAR);
    }

    // --- Per-tile routing tests (UDIM slice 4) ---

    #[test]
    fn tiles_present_lists_routed_tiles_sorted() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Unrouted session: tile 1001 only.
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        state.set_mesh(&two_tile_strip());
        assert_eq!(state.tiles_present(), vec![1001]);
        // A routed 1002 event adds 1002; listing is ascending (1002 was
        // created after 1001 but the order is by tile, not creation).
        state.begin_stroke(egui::pos2(1.2, 0.5));
        state.end_stroke();
        assert_eq!(state.tiles_present(), vec![1001, 1002]);
        // Re-routing to an existing tile adds nothing.
        state.begin_stroke(egui::pos2(0.5, 0.5));
        state.end_stroke();
        assert_eq!(state.tiles_present(), vec![1001, 1002]);
    }

    #[test]
    fn two_tile_strip_spans_tiles_1001_and_1002() {
        // Fixture sanity: the routing tests rely on these exact tiles and
        // on the strip being seam-free.
        let mesh = two_tile_strip();
        assert_eq!(tile_of_triangle(&mesh), vec![1001, 1001, 1002, 1002]);
        assert!(build_seam_graph(&mesh).edges.is_empty());
    }

    #[test]
    fn local_texel_subtracts_tile_offsets() {
        // (1.2, 0.5) in tile 1002 at 512 tpv: tile_u = 1, tile_v = 0, so
        // x = (1.2 - 1.0) * 512 and y = (1 - 0.5) * 512 = 256 exactly.
        // NOTE: 1.2_f32 - 1.0 is 0.20000005 in f32 (exact subtraction of
        // the rounded 1.2), so x is 102.400024, NOT the literal 102.4_f32 —
        // assert the derived f32 expression exactly.
        let [x, y] = local_texel([1.2, 0.5], 1002, 512.0);
        assert_eq!(x, (1.2_f32 - 1.0) * 512.0);
        assert_ne!(x, 102.4_f32);
        assert!((x - 102.4).abs() < 1e-3);
        assert_eq!(y, 256.0);

        // V offset adds tens: (0.5, 1.2) in tile 1011 → tile_v = 1, so
        // y = (1 - (1.2 - 1.0)) * 512.
        let [x, y] = local_texel([0.5, 1.2], 1011, 512.0);
        assert_eq!(x, 256.0);
        assert_eq!(y, (1.0 - (1.2_f32 - 1.0)) * 512.0);

        // Tile 1001: bit-identical to the pre-UDIM mapping.
        for uv in [[0.0, 0.0], [0.37, 0.81], [1.0, 1.0], [0.123_456, 0.999]] {
            let old = [uv[0] * 512.0, (1.0 - uv[1]) * 512.0];
            let new = local_texel(uv, 1001, 512.0);
            assert_eq!(new[0].to_bits(), old[0].to_bits(), "x at {uv:?}");
            assert_eq!(new[1].to_bits(), old[1].to_bits(), "y at {uv:?}");
        }
    }

    #[test]
    fn stroke_at_tile_1002_lands_in_tile_1002_only() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        state.set_mesh(&two_tile_strip());
        assert_eq!(state.staged_batch_count_for(1001), 0);
        assert_eq!(state.staged_batch_count_for(1002), 0);

        // Stays inside [1,2)x[0,1]: (1.2,0.5) → (1.35,0.5).
        drive_stroke(&mut state, [1.2, 0.5], [0.01, 0.0], 15);
        state.end_stroke();

        let in_1002 = state.staged_batch_count_for(1002);
        assert!(
            in_1002 > 0,
            "stroke at UV (1.2,0.5) must stage into tile 1002"
        );
        assert_eq!(
            state.staged_batch_count_for(1001),
            0,
            "tile 1001 must stay untouched by a tile-1002 stroke"
        );
        assert_eq!(state.staged_batch_count(), in_1002);
        assert_eq!(state.active_tile(), 1002);
        assert!(state.tile_target(1002).is_some());
        assert!(state.tile_target(1003).is_none());
        // Every tile's queue drains (and the active-tile accessor serves
        // the routed tile).
        state.process_pending().expect("drain succeeds");
        assert!(std::ptr::eq(
            state.paint_target(),
            state.tile_target(1002).expect("tile 1002 exists"),
        ));
    }

    #[test]
    fn tile_targets_are_created_lazily_and_reused() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        state.set_mesh(&two_tile_strip());
        assert_eq!(state.tile_count(), 1, "only tile 1001 exists up front");

        state.begin_stroke(egui::pos2(1.2, 0.5));
        state.end_stroke();
        assert_eq!(state.tile_count(), 2, "first route to 1002 creates it");

        state.begin_stroke(egui::pos2(1.7, 0.3));
        state.end_stroke();
        assert_eq!(state.tile_count(), 2, "re-routing to 1002 reuses it");

        state.begin_stroke(egui::pos2(0.5, 0.5));
        state.end_stroke();
        assert_eq!(state.tile_count(), 2, "1001 already existed");
        assert_eq!(state.active_tile(), 1001);
    }

    #[test]
    fn single_tile_meshes_route_everything_to_1001() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // No mesh: even an out-of-[0,1] UV stays in 1001.
        let mut bare = PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        bare.begin_stroke(egui::pos2(1.5, 0.5));
        bare.end_stroke();
        assert_eq!(bare.tile_count(), 1);
        assert_eq!(bare.active_tile(), 1001);

        // Single-tile mesh: the UV view's u == 1.0 edge (tile_of_uv gives
        // 1002) must not spawn a blank tile.
        let mut single = PaintState::new(device, queue).expect("paint state builds");
        single.set_mesh(&single_island_quad());
        drive_stroke(&mut single, [0.9, 0.5], [0.01, 0.0], 15);
        single.end_stroke();
        assert_eq!(single.tile_count(), 1);
        assert_eq!(single.active_tile(), 1001);
        assert!(single.staged_batch_count() > 0);
        assert_eq!(
            single.staged_batch_count_for(1001),
            single.staged_batch_count(),
            "every batch lands in tile 1001"
        );
    }

    #[test]
    fn mirrors_route_by_their_own_tile_without_moving_active() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // two_island_quad's second island lives at UV ~10–11, which
        // tile_of_uv clamps to tile 1100: the mesh is multi-tile, and the
        // seam mirrors route there while the pointer stays in 1001.
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        state.set_mesh(&two_island_quad());
        drive_stroke(&mut state, NEAR_SEAM_UV, [0.02, 0.02], 16);
        state.end_stroke();
        assert!(state.staged_batch_count_for(1001) > 0);
        assert!(
            state.staged_batch_count_for(1100) > 0,
            "mirrors must land in their own tile"
        );
        assert_eq!(
            state.staged_batch_count(),
            state.staged_batch_count_for(1001) + state.staged_batch_count_for(1100)
        );
        assert_eq!(
            state.active_tile(),
            1001,
            "mirrors never move the active tile"
        );
        state.process_pending().expect("drain succeeds");
    }

    // --- Paint paths (§2 line 26's P1 row) ---

    #[test]
    fn fill_runs_cover_maximal_spans() {
        // Row 0 full, row 1 two spans, row 2 empty.
        let mask = vec![
            true, true, true, true, //
            true, false, false, true, //
            false, false, false, false,
        ];
        assert_eq!(
            fill_runs(&mask, 4, 3),
            vec![
                FillRun { y: 0, x0: 0, x1: 3 },
                FillRun { y: 1, x0: 0, x1: 0 },
                FillRun { y: 1, x0: 3, x1: 3 },
            ]
        );
        assert!(fill_runs(&[], 4, 4).is_empty());
        // A torn (short) rasterizer output degrades to its present prefix,
        // never a panic.
        assert_eq!(
            fill_runs(&[true], 4, 4),
            vec![FillRun { y: 0, x0: 0, x1: 0 }]
        );
    }

    #[test]
    fn texel_center_uv_inverts_local_texel() {
        for tile in [FIRST_TILE, 1002, 1011] {
            for (x, y) in [(0, 0), (17, 42), (511, 511)] {
                let uv = texel_center_uv(tile, x, y, 512.0);
                let back = local_texel(uv, tile, 512.0);
                assert!(
                    (back[0] - (x as f32 + 0.5)).abs() < 1e-3
                        && (back[1] - (y as f32 + 0.5)).abs() < 1e-3,
                    "tile {tile} texel ({x},{y}) must round-trip"
                );
            }
        }
    }

    /// The replay's event stream from path points (16 vertical steps, full
    /// style — the same UVs `drive_stroke` feeds).
    fn straight_path(start: [f32; 2], step: [f32; 2], steps: usize) -> PaintPath {
        let mut path = PaintPath::new();
        path.push_point(start, 1.0, 1.0, 1.0);
        for i in 1..=steps {
            path.push_point(
                [start[0] + step[0] * i as f32, start[1] + step[1] * i as f32],
                1.0,
                1.0,
                1.0,
            );
        }
        path
    }

    #[test]
    fn replayed_path_stages_exactly_like_the_manual_stroke() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let (start, step) = ([0.2, 0.3], [0.0, 0.02]);

        let mut manual =
            PaintState::new(device.clone(), queue.clone()).expect("paint state builds");
        drive_stroke(&mut manual, start, step, 15);
        manual.end_stroke();

        let mut replayed = PaintState::new(device, queue).expect("paint state builds");
        // Park a non-paint tool: the replay must restore it afterwards.
        let mut smudge_tool = BrushTool::Smudge(umber_brush::SmudgeParams::default());
        replayed.sync_tool(&mut smudge_tool);
        let path = straight_path(start, step, 15);
        assert!(replayed.replay_path(&path, &PathOp::Paint));
        assert!(!replayed.is_stroking(), "replay ends its stroke");
        assert_eq!(
            replayed.staged_batch_count(),
            manual.staged_batch_count(),
            "a full-style replay is the manual event stream"
        );
        assert_eq!(
            replayed.staged_dab_count(),
            manual.staged_dab_count(),
            "dab for dab, through the same conditioner"
        );
        assert_eq!(
            replayed.tool, smudge_tool,
            "the session tool survives the replay"
        );
    }

    #[test]
    fn replay_refuses_live_strokes_and_short_paths() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        let mut short = PaintPath::new();
        short.push_point([0.5, 0.5], 1.0, 1.0, 1.0);
        assert!(!state.replay_path(&short, &PathOp::Paint));
        assert!(!state.replay_filled_path(&short, &PathOp::Paint));
        assert!(!state.replay_filled_path(&PaintPath::new(), &PathOp::Paint));
        // A replay never splits a live stroke.
        let path = straight_path([0.2, 0.3], [0.0, 0.02], 15);
        state.begin_stroke(egui::pos2(0.5, 0.5));
        assert!(!state.replay_path(&path, &PathOp::Paint));
        assert!(!state.replay_filled_path(&path, &PathOp::Paint));
        state.end_stroke();
        assert!(state.replay_path(&path, &PathOp::Paint));
    }

    #[test]
    fn retouch_replay_routes_to_the_cpu_mirror() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        let path = straight_path([0.2, 0.5], [0.01, 0.0], 15);
        let batches = state.staged_batch_count();
        let op = PathOp::Smudge(umber_brush::SmudgeParams {
            strength: 1.0,
            smear: 0.5,
        });
        assert!(state.replay_path(&path, &op));
        assert_eq!(
            state.staged_batch_count(),
            batches,
            "smudge dabs never stage GPU splats"
        );
        assert!(
            state.can_undo_stroke(),
            "the replayed smudge is one undo entry"
        );
        // The eraser on a blank target is the exact identity (no writes,
        // no history) — while still replaying through its own consumer.
        let mut blank =
            PaintState::new(state.device.clone(), state.queue.clone()).expect("paint state builds");
        assert!(blank.replay_path(&path, &PathOp::Erase(umber_brush::EraseParams::default())));
        assert_eq!(blank.staged_batch_count(), 0);
        assert!(
            !blank.can_undo_stroke(),
            "an identity erase records nothing"
        );
    }
}
