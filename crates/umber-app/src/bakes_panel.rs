//! The Bakes panel: headless-driven mesh-map baking from the egui shell.
//!
//! Wave 3 scope: a Bake button over the real bake engine
//! (`umber-bake`'s mesh-fed entry points) plus the `umber-mesh`
//! `TextureSetName_map` output convention and the `umber-export` PNG
//! writer — the same composition `umber-cli`'s `bake-all` uses, surfaced
//! in the dock beside the layer stack.
//!
//! Async bake (wave 5): the click no longer blocks the frame. `bake_now`
//! snapshots everything the bake reads into an owned [`BakeRequest`]
//! (cloned `wgpu` device + queue — Arc-backed handles, `Send + Sync` on
//! native — a clone of the mesh, the scalar settings, and each job's
//! output path resolved up front) and moves it onto a `std::thread`
//! worker ([`BakeJobHandle`]). The worker runs [`run_bake`] — the old
//! synchronous driver, unchanged in body — and sends the one result back
//! over an `mpsc` channel. The panel drains it with [`BakesPanel::poll_job`]
//! each frame (`main.rs` polls even while the Bakes tab is hidden). Nothing
//! stays main-thread-only: wgpu's `Queue::submit`/`Device::poll` are
//! internally synchronized, so the worker's submissions and readback waits
//! interleave safely with eframe's frame submissions on the same queue.
//! While a job is in flight the Bake button is disabled ([`BakesPanel::can_bake`])
//! and `bake_now` is a no-op — one bake at a time.
//!
//! V1 has no progress events or cancellation (the job reports once, at
//! the end), and quitting mid-bake abandons the detached worker (a PNG
//! being written at exit can be left truncated). The Export dialog still
//! bakes synchronously through `bake_sources` — async export is the
//! follow-up slice. See `LANDING_NOTES_BAKES_PANEL.md`.
//!
//! GPU access follows the `viewport`/`paint_state` pattern: this module
//! never names a `wgpu` type, taking `umber_gpu::WgpuDevice`/`WgpuQueue`
//! (re-exported aliases) or `&umber_gpu::GpuContext` instead.
//!
//! The AO bake itself lives in [`crate::bake_sources`], shared with the
//! Export dialog (`export_dialog.rs`) so the two panels drive one GPU
//! call instead of two copies of the same params.
//!
//! UDIM (wave-5 slice 6): a tile selector ([`TileSelection`]) over the
//! mesh's present tiles ([`umber_mesh::present_tiles`]) picks which tiles
//! a Bake covers ([`plan_bake`]). A single-tile (all-`[0, 1]`) mesh keeps
//! the whole-mesh bakes and `<set>_<suffix>.png` names, byte-identical
//! to before. A multi-tile mesh bakes AO per selected tile
//! (`umber_bake::ao::bake_ao_tile`) to `<set>_<suffix>_<tile>.png` (the
//! export driver's `_$udim` placement; `parse_mesh_map` does not parse
//! the tile-suffixed stems). V1 has no per-tile entry point for
//! curvature/thickness/position: their whole-mesh bakes rasterize only
//! `[0, 1]` — tile 1001's window — so they bake for tile 1001 only and
//! are reported skipped for every other selected tile, never written
//! under the wrong tile's name. The per-tile × per-map checklist matrix
//! and the remaining per-tile bakers are wave-6 work.
//!
//! Bake mode (§6:57–59): skew correction, auto-rebake, and the bake log.
//! - Skew (§6:57): the `bake_skew` checkbox bakes the paintable skew
//!   deviation channel (`umber_bake::skew`, emitted per
//!   `umber_gpu::bake_shaders::SKEW_BAKE_SHADER`'s contract) under
//!   `<set>_skew.png`; the threshold slider drives the edge-protection
//!   guard the worker applies to the AO arm, and the clamped texel count
//!   lands in the bake log's warnings column. V1 skew files are FLAT
//!   (zero measured deviation — the GPU emission wiring is the named
//!   follow-up); the paint-combine, guard, and log columns are fully live
//!   and headless-tested.
//! - Auto-rebake (§6:58): every frame [`BakesPanel::sync_param_tracking`]
//!   diffs the sliders against the last applied snapshot and marks
//!   [`umber_bake::skew::AutoRebake`] + the [`umber_core::AffectedRegion`]
//!   tile set; 500 ms after the last change [`BakesPanel::poll_autorebake`]
//!   fires ONLY the affected (map ∩ checked, tile ∩ selected, set ∩
//!   enabled) jobs through the same async queue as a manual Bake.
//! - Bake log + cage (§6:59): [`BakeLog`] lists one entry per baked map
//!   (timestamp, set, map, tile, duration, skew-clamped warnings); clicking
//!   a row jumps to (highlights) the responsible settings row. The cage
//!   toggle is panel state synced to the viewport's cage wireframe overlay
//!   by the app shell, mirroring the wireframe/grid toggles.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fluent::FluentValue;
use umber_mesh::FIRST_TILE;

use crate::i18n::{tr, tr_args};
use crate::tile_selection::{MeshTilesCache, TileSelection};

/// Bake resolutions offered by the panel (square targets): the shared
/// 512..8K presets ([`crate::size_presets::SIZE_PRESETS`]). The pre-8K list
/// was 128..2048; the sub-512 preview sizes left with the switch to the
/// shared list (the CLI's `--size` still takes any size).
pub const SUPPORTED_RESOLUTIONS: &[u32] = &crate::size_presets::SIZE_PRESETS;
/// The default bake resolution (square texels).
pub const DEFAULT_RESOLUTION: u32 = crate::size_presets::DEFAULT_SIZE;
/// The default hemisphere ray count for the raycast bakers (AO, thickness).
pub const DEFAULT_RAYS: u32 = 16;
/// Minimum hemisphere rays the slider allows.
pub const MIN_RAYS: u32 = 4;
/// Maximum hemisphere rays the slider allows.
pub const MAX_RAYS: u32 = 64;
/// The default UV-padding dilation width in texels.
pub const DEFAULT_DILATION_ITERATIONS: u32 = 16;
/// Maximum dilation iterations the slider allows (`0` = no post-pass).
pub const MAX_DILATION_ITERATIONS: u32 = 64;
/// Fallback texture-set name when the mesh came from nowhere on disk
/// (mirrors `umber_mesh::texture_set_name`'s own fallback).
pub const FALLBACK_TEXTURE_SET: &str = "TextureSet";
/// The skew map's file suffix (`<set>_skew.png`): kept as a local const
/// (not `umber_mesh::MeshMapKind` — that enum feeds the viewport preview
/// ids 1:1, so it cannot grow a tenth kind without a shader + test
/// ripple; see the module docs). Must equal
/// `umber_bake::BakeMap::Skew::file_stem_suffix` (pinned by test).
pub const SKEW_SUFFIX: &str = crate::bake_sources::SKEW_CHANNEL_NAME;
/// The default skew edge-protection threshold in degrees (see
/// `umber_bake::skew::DEFAULT_SKEW_THRESHOLD_DEG`).
pub const DEFAULT_SKEW_THRESHOLD_DEG: f32 = umber_bake::skew::DEFAULT_SKEW_THRESHOLD_DEG;
/// Minimum/maximum skew thresholds the slider allows.
pub const MIN_SKEW_THRESHOLD_DEG: f32 = umber_bake::skew::MIN_SKEW_THRESHOLD_DEG;
/// Maximum skew threshold (90° = protection effectively off).
pub const MAX_SKEW_THRESHOLD_DEG: f32 = umber_bake::skew::MAX_SKEW_THRESHOLD_DEG;

/// Which mesh map the panel can bake (the four `umber-bake` entry points
/// the panel drives; a subset of [`umber_mesh::MeshMapKind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeSelection {
    /// Ambient occlusion (`umber_bake::ao::bake_ao_mesh`).
    Ao,
    /// Signed screen-space curvature (`umber_bake::curvature`).
    Curvature,
    /// Local thickness (`umber_bake::thickness`).
    Thickness,
    /// World-space position (`umber_bake::position`, re-encoded to RGBA8).
    Position,
}

impl BakeSelection {
    /// Every selectable map, in bake order.
    pub const ALL: [Self; 4] = [Self::Ao, Self::Curvature, Self::Thickness, Self::Position];

    /// The checkbox / record label.
    pub fn label(self) -> String {
        match self {
            Self::Ao => tr("bakes.map-ao"),
            Self::Curvature => tr("bakes.map-curvature"),
            Self::Thickness => tr("bakes.map-thickness"),
            Self::Position => tr("bakes.map-position"),
        }
    }

    /// The [`umber_mesh::MeshMapKind`] used for output naming.
    pub fn mesh_map_kind(self) -> umber_mesh::MeshMapKind {
        match self {
            Self::Ao => umber_mesh::MeshMapKind::AmbientOcclusion,
            Self::Curvature => umber_mesh::MeshMapKind::Curvature,
            Self::Thickness => umber_mesh::MeshMapKind::Thickness,
            Self::Position => umber_mesh::MeshMapKind::Position,
        }
    }
}

/// One finished map write (what the status line summarizes).
#[derive(Debug, Clone)]
pub struct BakeRecord {
    /// Which map was written (`None` = the skew deviation channel —
    /// the skew map is not a [`BakeSelection`]: it is a correction mask,
    /// not a color bake, and it must not join `BakeSelection::ALL` or the
    /// pre-skew plan tests change meaning).
    pub selection: Option<BakeSelection>,
    /// The texture set this write belongs to (the per-set checklist).
    pub set: String,
    /// The UDIM tile this write belongs to.
    pub tile: u16,
    /// Where it was written.
    pub path: PathBuf,
    /// Wall time for bake + dilate + PNG encode of this map.
    pub elapsed_ms: u128,
    /// Texels the edge-protection guard clamped (the bake log's warnings
    /// column; 0 for unguarded maps and for skew maps themselves).
    pub skew_clamped: usize,
}

/// Which mesh one bake job rasterizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeSource {
    /// The whole mesh through the map's original entry point (rasterizes
    /// UV `[0, 1]`, i.e. tile 1001).
    WholeMesh,
    /// The job's tile window (`umber_bake::ao::bake_ao_tile`; AO only in v1).
    Tile,
}

/// One map bake for one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BakeJob {
    /// Which map.
    pub selection: BakeSelection,
    /// Which UDIM tile the output belongs to.
    pub tile: u16,
    /// How it is baked.
    pub source: BakeSource,
}

/// What a Bake click runs: the jobs in order, whether outputs carry the
/// tile in their names, and the (tile, map) pairs v1 cannot bake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakePlan {
    /// The bakes to run, tile-major then map order.
    pub jobs: Vec<BakeJob>,
    /// Multi-tile mesh: outputs are named `<set>_<suffix>_<tile>.png`.
    pub tiled: bool,
    /// Selected (tile, map) pairs with no v1 per-tile baker.
    pub skipped: Vec<(u16, BakeSelection)>,
}

/// Plans a Bake over the checked `maps` (pure logic). `present` is the
/// mesh's present tiles; `tiles` the selected list
/// ([`TileSelection::resolve`]).
///
/// - Single-tile mesh (`present` is `[1001]`, or empty): every map bakes
///   whole-mesh, untiled names — the pre-UDIM behavior exactly
///   (`tiles` is necessarily `[1001]` or empty there and is ignored).
/// - Otherwise, per selected tile: AO bakes over the tile's window; the
///   other maps bake whole-mesh for tile 1001 (their `[0, 1]` raster IS
///   tile 1001) and are skipped for any other tile.
pub fn plan_bake(maps: &[BakeSelection], present: &[u16], tiles: &[u16]) -> BakePlan {
    let single = present.is_empty() || present == [FIRST_TILE];
    if single {
        return BakePlan {
            jobs: maps
                .iter()
                .map(|selection| BakeJob {
                    selection: *selection,
                    tile: FIRST_TILE,
                    source: BakeSource::WholeMesh,
                })
                .collect(),
            tiled: false,
            skipped: Vec::new(),
        };
    }
    let mut jobs = Vec::new();
    let mut skipped = Vec::new();
    for tile in tiles {
        for selection in maps {
            let source = match selection {
                BakeSelection::Ao => BakeSource::Tile,
                _ if *tile == FIRST_TILE => BakeSource::WholeMesh,
                _ => {
                    skipped.push((*tile, *selection));
                    continue;
                }
            };
            jobs.push(BakeJob {
                selection: *selection,
                tile: *tile,
                source,
            });
        }
    }
    BakePlan {
        jobs,
        tiled: true,
        skipped,
    }
}

/// The status-line note for [`BakePlan::skipped`] (`None` when nothing
/// was skipped).
pub fn skipped_note(skipped: &[(u16, BakeSelection)]) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let parts: Vec<String> = skipped
        .iter()
        .map(|(tile, selection)| format!("{} {tile}", selection.label()))
        .collect();
    let args = [("pairs", FluentValue::from(parts.join(", ")))];
    Some(tr_args("bakes.skipped-note", args))
}

/// A settings row the bake log can jump to (§6:59): clicking a log entry
/// highlights the row responsible for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeSettingId {
    /// Output resolution.
    Resolution,
    /// Hemisphere ray count.
    Rays,
    /// UV-padding dilation width.
    Dilation,
    /// Skew edge-protection threshold.
    SkewThreshold,
    /// Cage visualization toggle.
    Cage,
}

/// One bake-log entry (§6:59): timestamp, map, set, duration, warnings
/// (the skew-clamped texel counts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeLogEntry {
    /// Monotonic id (click identity for jump-to-setting).
    pub id: u64,
    /// Unix seconds when the bake finished.
    pub timestamp_secs: u64,
    /// The texture set baked.
    pub set: String,
    /// Which map (`None` = the skew deviation channel).
    pub selection: Option<BakeSelection>,
    /// The UDIM tile baked.
    pub tile: u16,
    /// Wall time for bake + dilate + PNG encode of this map.
    pub duration_ms: u128,
    /// Texels the edge-protection guard clamped.
    pub skew_clamped: usize,
}

impl BakeLogEntry {
    /// The map label shown in the log row (the checkbox label, so the log
    /// and the checklist can never disagree about names).
    pub fn map_label(&self) -> String {
        match self.selection {
            Some(selection) => selection.label(),
            None => tr("bakes.map-skew"),
        }
    }

    /// The one-line log row (the dragon's format probe target):
    /// `[<time>] <set> <map> (tile <tile>): <ms> ms[ — <n> skew-clamped]`.
    pub fn summary(&self) -> String {
        let warnings = if self.skew_clamped == 0 {
            String::new()
        } else {
            let args = [("count", FluentValue::from(self.skew_clamped))];
            tr_args("bakes.log-warnings", args)
        };
        let args = [
            ("time", FluentValue::from(self.timestamp_secs)),
            ("set", FluentValue::from(self.set.clone())),
            ("map", FluentValue::from(self.map_label())),
            ("tile", FluentValue::from(self.tile)),
            ("ms", FluentValue::from(self.duration_ms.to_string())),
            ("warnings", FluentValue::from(warnings)),
        ];
        tr_args("bakes.log-entry", args)
    }

    /// The settings row this entry jumps to: skew-clamped entries point
    /// at the threshold that clamped them; skew-map entries at the same
    /// threshold; AO/thickness (the raycast bakers) at the ray count;
    /// position (whose only panel lever is the target size) at resolution;
    /// curvature (whose strength is not a panel param) at dilation, the
    /// post-pass every map shares.
    pub fn setting_hint(&self) -> BakeSettingId {
        if self.skew_clamped > 0 || self.selection.is_none() {
            BakeSettingId::SkewThreshold
        } else if matches!(
            self.selection,
            Some(BakeSelection::Ao) | Some(BakeSelection::Thickness)
        ) {
            BakeSettingId::Rays
        } else if matches!(self.selection, Some(BakeSelection::Position)) {
            BakeSettingId::Resolution
        } else {
            BakeSettingId::Dilation
        }
    }
}

/// The per-bake entry list (§6:59): one [`BakeLogEntry`] per written map,
/// oldest first.
#[derive(Debug, Clone, Default)]
pub struct BakeLog {
    entries: Vec<BakeLogEntry>,
    next_id: u64,
}

impl BakeLog {
    /// Empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one entry; returns its id (the jump-to-setting handle).
    pub fn push(
        &mut self,
        timestamp_secs: u64,
        set: String,
        selection: Option<BakeSelection>,
        tile: u16,
        duration_ms: u128,
        skew_clamped: usize,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(BakeLogEntry {
            id,
            timestamp_secs,
            set,
            selection,
            tile,
            duration_ms,
            skew_clamped,
        });
        id
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> &[BakeLogEntry] {
        &self.entries
    }

    /// Entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no bake has been logged yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Finds an entry by id (the log row click target).
    pub fn entry(&self, id: u64) -> Option<&BakeLogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// One per-set checklist row (§6:59): the texture-set name plus whether it
/// bakes. Synced from the mesh each frame (like the tile selector);
/// manual Bakes start from the mesh's single set until the first sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCheck {
    /// Texture-set name (material name, or the file-stem fallback).
    pub name: String,
    /// Whether this set bakes (manual + auto-rebake alike).
    pub enabled: bool,
}

/// What one auto-rebake firing covers (§6:58): the affected maps (plus
/// whether the dirty skew tiles ride along), tiles, and sets — the
/// per-map/per-set checklist intersected with the dirty sets. The caller
/// hands it to the same async queue as a manual Bake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoRebakeFire {
    /// Affected + still-checked maps, in bake order.
    pub maps: Vec<BakeSelection>,
    /// Whether the skew tiles bake too (dirty + checked).
    pub skew: bool,
    /// Affected + still-selected tiles.
    pub tiles: Vec<u16>,
    /// Enabled sets.
    pub sets: Vec<String>,
}

/// The [`umber_bake::BakeMap`] behind a panel checkbox (the auto-rebake
/// dirty set speaks `BakeMap`; the checklist speaks `BakeSelection`).
pub fn bake_map_of(selection: BakeSelection) -> umber_bake::BakeMap {
    match selection {
        BakeSelection::Ao => umber_bake::BakeMap::AmbientOcclusion,
        BakeSelection::Curvature => umber_bake::BakeMap::Curvature,
        BakeSelection::Thickness => umber_bake::BakeMap::Thickness,
        BakeSelection::Position => umber_bake::BakeMap::Position,
    }
}

/// The status-line note for skew tiles v1 cannot bake (`None` when every
/// selected tile is 1001 / the skew channel is off).
pub fn skipped_skew_note(skipped: &[u16]) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let tiles: Vec<String> = skipped.iter().map(|t| t.to_string()).collect();
    let args = [("tiles", FluentValue::from(tiles.join(", ")))];
    Some(tr_args("bakes.skipped-skew-note", args))
}

/// What [`BakesPanel::show`] needs from the app each frame.
///
/// `gpu`/`mesh` are `Option` so the panel can gate itself: `None` mesh
/// means "No mesh loaded", `None` gpu means no device — either disables
/// the Bake button (the panel never panics on missing context).
pub struct BakesContext<'a> {
    /// The app's GPU context (carries device + queue).
    pub gpu: Option<&'a umber_gpu::GpuContext>,
    /// The loaded mesh to bake against.
    pub mesh: Option<&'a umber_mesh::MeshData>,
    /// The mesh's source path (for the texture-set name); `None` falls
    /// back to [`FALLBACK_TEXTURE_SET`].
    pub mesh_path: Option<&'a Path>,
}

/// What a bake worker sends back: every written map, or the first failure.
type BakeOutcome = anyhow::Result<Vec<BakeRecord>>;

/// The bake dispatch bundle (mesh + plan + the skew/sets fan-out) — keeps
/// [`BakesPanel::bake_jobs`] under clippy's 7-argument floor.
struct BakeDispatch<'a> {
    mesh: &'a umber_mesh::MeshData,
    plan: BakePlan,
    skew_tiles: Vec<u16>,
    skew_skipped: Vec<u16>,
    sets: Vec<String>,
}

/// Everything one Bake reads, owned so it can move onto the worker thread
/// (the `Send + 'static` bound on [`BakeJobHandle::spawn`] pins that at
/// compile time — a non-`Send` GPU handle here would fail to build).
struct BakeRequest {
    device: umber_gpu::WgpuDevice,
    queue: umber_gpu::WgpuQueue,
    mesh: umber_mesh::MeshData,
    /// Square bake resolution in texels.
    size: u32,
    rays: u32,
    dilation_iterations: u32,
    /// Skew edge-protection threshold in degrees (the AO guard).
    skew_threshold_deg: f32,
    /// The planned jobs, each with its output path (resolved on the main
    /// thread from the panel's output dir + texture-set name) and the set
    /// it belongs to (the per-set checklist fan-out).
    jobs: Vec<(BakeJob, PathBuf, String)>,
    /// The skew-map tiles to emit, each with its output path and set
    /// (the whole-mesh path only rasterizes `[0, 1]`, so skew — like
    /// curvature — bakes for tile 1001 only; see
    /// [`BakesPanel::skew_tiles`]).
    skew_jobs: Vec<(u16, PathBuf, String)>,
}

/// One in-flight bake on a worker thread: the receiving end of its
/// one-shot result channel, plus what the status line needs afterward.
pub struct BakeJobHandle {
    receiver: mpsc::Receiver<BakeOutcome>,
    /// The plan's skipped (tile, map) pairs, appended to the done status.
    skipped: Vec<(u16, BakeSelection)>,
    /// The skew tiles v1 cannot bake (whole-mesh path = tile 1001 only),
    /// appended to the done status after [`skipped_note`].
    skipped_skew: Vec<u16>,
}

impl BakeJobHandle {
    /// Runs `work` on a fresh named thread; its result arrives through
    /// [`Self::try_finish`]. The thread is detached (its `JoinHandle` is
    /// dropped): completion is observed through the channel alone.
    fn spawn<F>(
        skipped: Vec<(u16, BakeSelection)>,
        skipped_skew: Vec<u16>,
        work: F,
    ) -> std::io::Result<Self>
    where
        F: FnOnce() -> BakeOutcome + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("umber-bake".into())
            .spawn(move || {
                // The panel may be gone (app quit); a dead receiver is fine.
                let _ = sender.send(work());
            })?;
        Ok(Self {
            receiver,
            skipped,
            skipped_skew,
        })
    }

    /// The job's result once the worker has finished, `None` while it is
    /// still running. A worker that died without sending (a panic — e.g.
    /// wgpu's uncaptured-error handler fires on the submitting thread)
    /// reports as a failure, never as "still baking" forever.
    fn try_finish(&self) -> Option<BakeOutcome> {
        match self.receiver.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
                "bake worker exited without a result (it panicked; see the log)"
            ))),
        }
    }
}

/// The Bakes panel state: bake settings + the last bake's status.
///
/// The egui drawing itself (`show`) is untested by construction; every
/// other method is pure logic with no GPU or UI dependency.
pub struct BakesPanel {
    resolution: u32,
    bake_ao: bool,
    bake_curvature: bool,
    bake_thickness: bool,
    bake_position: bool,
    /// Whether the skew deviation channel bakes (default off, like the
    /// bent-normal toggle: a correction mask, opt-in per bake).
    bake_skew: bool,
    rays: u32,
    dilation_iterations: u32,
    /// Skew edge-protection threshold in degrees (the guard + the log's
    /// warnings column).
    skew_threshold_deg: f32,
    /// Auto-rebake on parameter change (§6:58; default off — background
    /// bakes are opt-in).
    auto_rebake: bool,
    /// The debounce dirty set (§6:58).
    auto_state: umber_bake::skew::AutoRebake,
    /// The dirty tiles (§6:58's WHERE; the WHAT is `auto_state`).
    affected_tiles: umber_core::AffectedRegion,
    /// The slider snapshot `sync_param_tracking` last consumed (param
    /// changes are diffed per frame, so slider drags mark exactly once
    /// per changed value without touching the setters' purity).
    applied_resolution: u32,
    applied_rays: u32,
    applied_dilation: u32,
    applied_skew_threshold_deg: f32,
    /// The per-set checklist (§6:59), synced from the mesh each frame.
    sets: Vec<SetCheck>,
    /// Whether the cage wireframe overlay shows (synced to the viewport
    /// by the app shell, like wireframe/grid).
    show_cage: bool,
    /// The row the last log click jumped to (highlighted in `show`).
    highlighted_setting: Option<BakeSettingId>,
    /// The per-bake entry list (§6:59).
    log: BakeLog,
    output_dir: PathBuf,
    status: String,
    last_records: Vec<BakeRecord>,
    /// Which present tiles a Bake covers (default: all present).
    tiles: TileSelection,
    /// The loaded mesh's present tiles, cached across frames.
    mesh_tiles: MeshTilesCache,
    /// The in-flight bake, if any (at most one at a time).
    job: Option<BakeJobHandle>,
}

impl BakesPanel {
    /// Creates the panel writing to `output_dir` with default settings
    /// (512², all four maps, 16 rays, 16 dilation iterations, every
    /// present tile).
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            resolution: DEFAULT_RESOLUTION,
            bake_ao: true,
            bake_curvature: true,
            bake_thickness: true,
            bake_position: true,
            bake_skew: false,
            rays: DEFAULT_RAYS,
            dilation_iterations: DEFAULT_DILATION_ITERATIONS,
            skew_threshold_deg: DEFAULT_SKEW_THRESHOLD_DEG,
            auto_rebake: false,
            auto_state: umber_bake::skew::AutoRebake::new(),
            affected_tiles: umber_core::AffectedRegion::new(),
            applied_resolution: DEFAULT_RESOLUTION,
            applied_rays: DEFAULT_RAYS,
            applied_dilation: DEFAULT_DILATION_ITERATIONS,
            applied_skew_threshold_deg: DEFAULT_SKEW_THRESHOLD_DEG,
            sets: Vec::new(),
            show_cage: false,
            highlighted_setting: None,
            log: BakeLog::new(),
            output_dir,
            status: tr("bakes.no-bake-yet"),
            last_records: Vec::new(),
            tiles: TileSelection::new(),
            mesh_tiles: MeshTilesCache::default(),
            job: None,
        }
    }

    /// The tile selector's state (test hook: the UI's checkboxes drive
    /// the same `sync`/`set_selected` calls).
    #[cfg(test)]
    pub(crate) fn tiles_mut(&mut self) -> &mut TileSelection {
        &mut self.tiles
    }

    /// What a Bake click would run over a mesh whose present tiles are
    /// `present`: the checked maps × the selected tiles ([`plan_bake`]).
    pub fn bake_plan(&self, present: &[u16]) -> BakePlan {
        plan_bake(&self.selected_maps(), present, &self.tiles.resolve(present))
    }

    /// The tile-suffixed output path a multi-tile bake writes:
    /// `<dir>/<set>_<suffix>_<tile>.png` (the export driver's `_$udim`
    /// placement before the extension).
    pub fn tiled_output_path_for(
        &self,
        texture_set: &str,
        selection: BakeSelection,
        tile: u16,
    ) -> PathBuf {
        self.output_dir.join(format!(
            "{texture_set}_{}_{tile}.png",
            selection.mesh_map_kind().suffix()
        ))
    }

    /// The output directory baked PNGs are written to.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Retargets the output directory.
    pub fn set_output_dir(&mut self, dir: PathBuf) {
        self.output_dir = dir;
    }

    /// The square bake resolution in texels.
    pub fn resolution(&self) -> u32 {
        self.resolution
    }

    /// Whether `resolution` is one of [`SUPPORTED_RESOLUTIONS`].
    pub fn is_supported_resolution(resolution: u32) -> bool {
        SUPPORTED_RESOLUTIONS.contains(&resolution)
    }

    /// Sets the bake resolution; returns `false` (keeping the old value)
    /// for anything outside [`SUPPORTED_RESOLUTIONS`].
    pub fn set_resolution(&mut self, resolution: u32) -> bool {
        if Self::is_supported_resolution(resolution) {
            self.resolution = resolution;
            true
        } else {
            false
        }
    }

    /// The hemisphere ray count for the raycast bakers.
    pub fn rays(&self) -> u32 {
        self.rays
    }

    /// Sets the ray count, clamping to `MIN_RAYS..=MAX_RAYS`.
    pub fn set_rays(&mut self, rays: u32) {
        self.rays = rays.clamp(MIN_RAYS, MAX_RAYS);
    }

    /// The dilation post-pass width in texels (`0` = skip).
    pub fn dilation_iterations(&self) -> u32 {
        self.dilation_iterations
    }

    /// Sets the dilation width, clamping to `0..=MAX_DILATION_ITERATIONS`.
    pub fn set_dilation_iterations(&mut self, iterations: u32) {
        self.dilation_iterations = iterations.min(MAX_DILATION_ITERATIONS);
    }

    /// Enables or disables one map's checkbox.
    pub fn set_map_enabled(&mut self, selection: BakeSelection, enabled: bool) {
        match selection {
            BakeSelection::Ao => self.bake_ao = enabled,
            BakeSelection::Curvature => self.bake_curvature = enabled,
            BakeSelection::Thickness => self.bake_thickness = enabled,
            BakeSelection::Position => self.bake_position = enabled,
        }
    }

    /// Whether one map's checkbox is on.
    pub fn is_map_enabled(&self, selection: BakeSelection) -> bool {
        match selection {
            BakeSelection::Ao => self.bake_ao,
            BakeSelection::Curvature => self.bake_curvature,
            BakeSelection::Thickness => self.bake_thickness,
            BakeSelection::Position => self.bake_position,
        }
    }

    /// The currently-checked maps, in bake order.
    pub fn selected_maps(&self) -> Vec<BakeSelection> {
        BakeSelection::ALL
            .into_iter()
            .filter(|s| self.is_map_enabled(*s))
            .collect()
    }

    /// Whether at least one map is checked (the skew channel counts: a
    /// skew-only bake writes just the correction mask).
    pub fn any_selected(&self) -> bool {
        self.bake_ao
            || self.bake_curvature
            || self.bake_thickness
            || self.bake_position
            || self.bake_skew
    }

    /// Whether the skew deviation channel bakes.
    pub fn skew_enabled(&self) -> bool {
        self.bake_skew
    }

    /// Enables or disables the skew channel's checkbox.
    pub fn set_skew_enabled(&mut self, enabled: bool) {
        self.bake_skew = enabled;
    }

    /// The skew edge-protection threshold in degrees.
    pub fn skew_threshold_deg(&self) -> f32 {
        self.skew_threshold_deg
    }

    /// Sets the skew threshold, clamping to
    /// `MIN_SKEW_THRESHOLD_DEG..=MAX_SKEW_THRESHOLD_DEG`.
    pub fn set_skew_threshold_deg(&mut self, threshold_deg: f32) {
        self.skew_threshold_deg =
            threshold_deg.clamp(MIN_SKEW_THRESHOLD_DEG, MAX_SKEW_THRESHOLD_DEG);
    }

    /// Whether auto-rebake fires on parameter change (§6:58).
    pub fn auto_rebake(&self) -> bool {
        self.auto_rebake
    }

    /// Enables or disables auto-rebake.
    pub fn set_auto_rebake(&mut self, enabled: bool) {
        self.auto_rebake = enabled;
    }

    /// Whether the cage wireframe overlay shows (the app shell syncs the
    /// viewport from this each frame).
    pub fn show_cage(&self) -> bool {
        self.show_cage
    }

    /// Shows or hides the cage overlay.
    pub fn set_show_cage(&mut self, on: bool) {
        self.show_cage = on;
    }

    /// The row the last log click jumped to, if any (highlighted in `show`).
    pub fn highlighted_setting(&self) -> Option<BakeSettingId> {
        self.highlighted_setting
    }

    /// The bake log (§6:59).
    pub fn bake_log(&self) -> &BakeLog {
        &self.log
    }

    /// The skew-map tiles a bake covers: `(baked, skipped)`. The
    /// whole-mesh path only rasterizes `[0, 1]` (tile 1001), so — like
    /// curvature/thickness/position in [`plan_bake`] — skew bakes for tile
    /// 1001 only and is reported skipped for every other selected tile.
    /// A skew file is never written under the wrong tile's name.
    pub fn skew_tiles(&self, present: &[u16], selected: &[u16]) -> (Vec<u16>, Vec<u16>) {
        if !self.bake_skew {
            return (Vec::new(), Vec::new());
        }
        let single = present.is_empty() || present == [FIRST_TILE];
        if single {
            return (vec![FIRST_TILE], Vec::new());
        }
        let mut baked = Vec::new();
        let mut skipped = Vec::new();
        for tile in selected {
            if *tile == FIRST_TILE {
                baked.push(*tile);
            } else {
                skipped.push(*tile);
            }
        }
        (baked, skipped)
    }

    /// The skew channel's output path: `<dir>/<set>_skew.png` (the
    /// `TextureSetName_map` convention with [`SKEW_SUFFIX`]).
    pub fn skew_output_path_for(&self, texture_set: &str) -> PathBuf {
        self.output_dir
            .join(format!("{texture_set}_{SKEW_SUFFIX}.png"))
    }

    /// The tile-suffixed skew output path: `<dir>/<set>_skew_<tile>.png`.
    pub fn skew_tiled_output_path_for(&self, texture_set: &str, tile: u16) -> PathBuf {
        self.output_dir
            .join(format!("{texture_set}_{SKEW_SUFFIX}_{tile}.png"))
    }

    /// The per-set checklist names for `mesh`: one row per material, or
    /// the single texture-set name when the mesh carries no materials
    /// (mirrors `umber_mesh::texture_set_name`'s fallback chain).
    pub fn set_names_for(mesh: &umber_mesh::MeshData, mesh_path: Option<&Path>) -> Vec<String> {
        if mesh.material_names.is_empty() {
            let fallback = PathBuf::from(FALLBACK_TEXTURE_SET);
            vec![umber_mesh::texture_set_name(
                mesh_path.unwrap_or(&fallback),
                mesh,
            )]
        } else {
            let mut names = mesh.material_names.clone();
            names.sort();
            names.dedup();
            names
        }
    }

    /// Syncs the per-set checklist from the mesh (new sets arrive enabled
    /// — Painter checks all sets by default; unchecked-away sets stay
    /// unchecked while present; vanished sets drop).
    pub fn sync_sets(&mut self, names: &[String]) {
        let mut next = Vec::with_capacity(names.len());
        for name in names {
            let enabled = self
                .sets
                .iter()
                .find(|s| &s.name == name)
                .is_some_and(|s| s.enabled);
            next.push(SetCheck {
                name: name.clone(),
                enabled,
            });
        }
        self.sets = next;
    }

    /// The checklist rows (test hook for the UI's checkboxes).
    pub fn sets(&self) -> &[SetCheck] {
        &self.sets
    }

    /// Enables or disables one set's checkbox; `false` for an unknown name.
    pub fn set_set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        match self.sets.iter_mut().find(|s| s.name == name) {
            Some(row) => {
                row.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// The enabled sets' names (what manual + auto bakes cover).
    pub fn enabled_sets(&self) -> Vec<String> {
        self.sets
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.name.clone())
            .collect()
    }

    /// Checks every set and every map (the Bake-all button: one click
    /// from "nothing selected" to a full bake).
    pub fn bake_all_sets(&mut self) {
        for row in &mut self.sets {
            row.enabled = true;
        }
        self.bake_ao = true;
        self.bake_curvature = true;
        self.bake_thickness = true;
        self.bake_position = true;
        self.bake_skew = true;
    }

    /// Diffs the sliders against the last consumed snapshot and marks the
    /// dirty maps + tiles (§6:58). Called once per frame from `show` (and
    /// directly by tests with manual `Instant`s). Slider drags re-arm the
    /// 500 ms debounce on every change, so a drag fires once, after it ends.
    pub fn sync_param_tracking(&mut self, now: Instant, selected_tiles: &[u16]) {
        let mut changed = Vec::new();
        if self.resolution != self.applied_resolution {
            changed.push(umber_bake::skew::BakeParam::Resolution);
            self.applied_resolution = self.resolution;
        }
        if self.rays != self.applied_rays {
            changed.push(umber_bake::skew::BakeParam::RayLength);
            self.applied_rays = self.rays;
        }
        if self.dilation_iterations != self.applied_dilation {
            changed.push(umber_bake::skew::BakeParam::Dilation);
            self.applied_dilation = self.dilation_iterations;
        }
        if (self.skew_threshold_deg - self.applied_skew_threshold_deg).abs() > f32::EPSILON {
            changed.push(umber_bake::skew::BakeParam::SkewThreshold);
            self.applied_skew_threshold_deg = self.skew_threshold_deg;
        }
        for param in changed {
            self.auto_state.mark_param_change(param, now);
            self.affected_tiles.mark_tiles(selected_tiles);
        }
    }

    /// The pending auto-rebake (§6:58): the affected maps ∩ checked maps
    /// (plus the skew tiles when the skew map is both dirty and checked),
    /// over the affected tiles ∩ selected tiles, for the enabled sets.
    /// `None` unless auto-rebake is on, the 500 ms window has elapsed, and
    /// something affected is still checked — the existing async queue
    /// absorbs the returned fire (one bake at a time: `None` while a job
    /// is in flight). Firing takes (clears) both dirty sets.
    pub fn poll_autorebake(
        &mut self,
        now: Instant,
        selected_tiles: &[u16],
    ) -> Option<AutoRebakeFire> {
        if !self.auto_rebake || self.is_baking() || !self.auto_state.rebake_due(now) {
            return None;
        }
        let dirty = self.auto_state.take_dirty()?;
        let affected_tiles = self.affected_tiles.take();
        let tiles: Vec<u16> = selected_tiles
            .iter()
            .copied()
            .filter(|t| affected_tiles.contains(t))
            .collect();
        let mut maps = Vec::new();
        for selection in BakeSelection::ALL {
            if self.is_map_enabled(selection) && dirty.contains(&bake_map_of(selection)) {
                maps.push(selection);
            }
        }
        let skew = self.bake_skew && dirty.contains(&umber_bake::BakeMap::Skew);
        let sets = self.enabled_sets();
        if (maps.is_empty() && !skew) || tiles.is_empty() || sets.is_empty() {
            return None;
        }
        Some(AutoRebakeFire {
            maps,
            skew,
            tiles,
            sets,
        })
    }

    /// Clicks a log row: highlights the responsible settings row (§6:59's
    /// jump-to-setting). Returns `false` for an unknown entry id.
    pub fn jump_to_setting(&mut self, entry_id: u64) -> bool {
        match self.log.entry(entry_id) {
            Some(entry) => {
                self.highlighted_setting = Some(entry.setting_hint());
                true
            }
            None => false,
        }
    }

    /// Prefixes the highlighted settings row with the jump marker (the
    /// `▶` glyph is hardcoded, like the history panel's `↶`/`↷`).
    pub fn row_label(&self, id: BakeSettingId, text: String) -> String {
        if self.highlighted_setting == Some(id) {
            format!("▶ {text}")
        } else {
            text
        }
    }

    /// Unix seconds now (bake-log timestamps; `0` when the clock is
    /// unavailable — never a panic).
    pub fn now_unix_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Whether the Bake button is enabled: mesh + GPU present, at least
    /// one map checked, and no bake already in flight.
    pub fn can_bake(&self, mesh_loaded: bool, gpu_ready: bool) -> bool {
        mesh_loaded && gpu_ready && self.any_selected() && !self.is_baking()
    }

    /// Whether a bake job is running on its worker thread.
    pub fn is_baking(&self) -> bool {
        self.job.is_some()
    }

    /// Drains a finished bake job into `status`/`last_records` (call once
    /// per frame). Returns whether a job is still in flight afterward —
    /// the caller keeps repainting while it is.
    pub fn poll_job(&mut self) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let Some(outcome) = job.try_finish() else {
            return true;
        };
        let (skipped, skipped_skew) = self
            .job
            .take()
            .map(|job| (job.skipped, job.skipped_skew))
            .unwrap_or_default();
        self.apply_outcome(outcome, &skipped, &skipped_skew);
        false
    }

    /// The output path for one map under the panel's output dir, via the
    /// [`umber_mesh::format_mesh_map`] convention
    /// (`<dir>/<set>_<suffix>.png`).
    pub fn output_path_for(&self, texture_set: &str, selection: BakeSelection) -> PathBuf {
        umber_mesh::format_mesh_map(
            &self.output_dir,
            texture_set,
            selection.mesh_map_kind(),
            "png",
        )
    }

    /// The last bake's one-line summary (outputs + timings).
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The last bake's per-map records.
    pub fn last_records(&self) -> &[BakeRecord] {
        &self.last_records
    }

    /// Draws the panel: resolution combo, per-map checkboxes, rays and
    /// dilation sliders, output-dir row, the Bake button (starts a
    /// background job), and the status line.
    ///
    /// Gated: with no mesh (or no GPU) a reason line replaces the bake
    /// controls' effect — the Bake button is disabled either way, and
    /// while a bake is in flight.
    pub fn show(&mut self, ui: &mut egui::Ui, ctx: BakesContext<'_>) {
        self.poll_job();
        let mesh_loaded = ctx.mesh.is_some();
        let gpu_ready = ctx.gpu.is_some();
        if !mesh_loaded {
            ui.label(tr("bakes.no-mesh"));
        } else if !gpu_ready {
            ui.label(tr("bakes.no-gpu"));
        }

        // Resolution row (jump-highlightable via the marker prefix).
        if self.highlighted_setting == Some(BakeSettingId::Resolution) {
            ui.horizontal(|ui| {
                ui.label("▶");
                crate::size_presets::size_combo(ui, &tr("bakes.resolution"), &mut self.resolution);
            });
        } else {
            crate::size_presets::size_combo(ui, &tr("bakes.resolution"), &mut self.resolution);
        }

        // Per-map checklist (§6:59) + the paintable skew channel (§6:57).
        ui.checkbox(&mut self.bake_ao, BakeSelection::Ao.label());
        ui.checkbox(&mut self.bake_curvature, BakeSelection::Curvature.label());
        ui.checkbox(&mut self.bake_thickness, BakeSelection::Thickness.label());
        ui.checkbox(&mut self.bake_position, BakeSelection::Position.label());
        ui.checkbox(&mut self.bake_skew, tr("bakes.map-skew"));

        // UDIM tile selector: which present tiles the Bake covers (a
        // single-tile mesh shows `Tile 1001` with nothing to choose).
        if let Some(mesh) = ctx.mesh {
            self.tiles.sync(self.mesh_tiles.get(mesh));
            self.sync_sets(&Self::set_names_for(mesh, ctx.mesh_path));
        }
        self.tiles.show(ui);

        // Per-set checklist (§6:59): one checkbox per texture set.
        if !self.sets.is_empty() {
            ui.label(tr("bakes.sets"));
            for row in &mut self.sets {
                ui.checkbox(&mut row.enabled, &row.name);
            }
        }

        let rays_label = self.row_label(BakeSettingId::Rays, tr("bakes.rays"));
        ui.add(egui::Slider::new(&mut self.rays, MIN_RAYS..=MAX_RAYS).text(rays_label));
        let dilation_label = self.row_label(BakeSettingId::Dilation, tr("bakes.dilation"));
        ui.add(
            egui::Slider::new(&mut self.dilation_iterations, 0..=MAX_DILATION_ITERATIONS)
                .text(dilation_label),
        );
        let skew_label = self.row_label(BakeSettingId::SkewThreshold, tr("bakes.skew-threshold"));
        ui.add(
            egui::Slider::new(
                &mut self.skew_threshold_deg,
                MIN_SKEW_THRESHOLD_DEG..=MAX_SKEW_THRESHOLD_DEG,
            )
            .text(skew_label),
        );

        ui.checkbox(&mut self.auto_rebake, tr("bakes.auto-rebake"));
        let cage_label = self.row_label(BakeSettingId::Cage, tr("bakes.show-cage"));
        ui.checkbox(&mut self.show_cage, cage_label);

        ui.horizontal(|ui| {
            let dir = self.output_dir.display().to_string();
            ui.label(tr_args("common.out-dir", [("dir", FluentValue::from(dir))]));
            if ui.button(tr("button.choose")).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.output_dir = dir;
                }
            }
        });

        if !self.any_selected() {
            ui.label(tr("bakes.select-map"));
        }
        if self.tiles.nothing_selected() {
            ui.label(tr("bakes.select-tile"));
        }
        let sets_ok = self.sets.is_empty() || !self.enabled_sets().is_empty();
        if !sets_ok {
            ui.label(tr("bakes.select-set"));
        }
        let enabled =
            self.can_bake(mesh_loaded, gpu_ready) && !self.tiles.nothing_selected() && sets_ok;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(enabled, egui::Button::new(tr("button.bake")))
                .clicked()
            {
                if let (Some(gpu), Some(mesh)) = (ctx.gpu, ctx.mesh) {
                    self.bake_now(&gpu.device, &gpu.queue, mesh, ctx.mesh_path);
                }
            }
            if ui
                .add_enabled(enabled, egui::Button::new(tr("bakes.bake-all")))
                .clicked()
            {
                self.bake_all_sets();
                if let (Some(gpu), Some(mesh)) = (ctx.gpu, ctx.mesh) {
                    self.bake_now(&gpu.device, &gpu.queue, mesh, ctx.mesh_path);
                }
            }
        });

        // Param tracking + auto-rebake (§6:58): diff the sliders, then fire
        // through the same async queue when the debounce elapsed. Runs
        // after the controls above so this frame's checkbox state applies.
        if let Some(mesh) = ctx.mesh {
            let present = self.mesh_tiles.get(mesh).to_vec();
            let selected = self.tiles.resolve(&present);
            self.sync_param_tracking(Instant::now(), &selected);
            if let (Some(gpu), true) = (ctx.gpu, self.auto_rebake) {
                let now = Instant::now();
                if let Some(fire) = self.poll_autorebake(now, &selected) {
                    self.bake_filtered(&gpu.device, &gpu.queue, mesh, ctx.mesh_path, &fire);
                }
            }
        }

        // Bake log (§6:59): one row per baked map; a click jumps to the
        // responsible settings row (highlighted above).
        ui.collapsing(tr("bakes.log"), |ui| {
            if self.log.is_empty() {
                ui.label(tr("bakes.log-empty"));
            } else {
                let rows: Vec<(u64, String)> = self
                    .log
                    .entries()
                    .iter()
                    .map(|e| (e.id, e.summary()))
                    .collect();
                for (id, summary) in rows {
                    if ui.selectable_label(false, &summary).clicked() {
                        self.jump_to_setting(id);
                    }
                }
            }
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if self.is_baking() {
                ui.spinner();
            }
            ui.label(self.status.clone());
        });
    }

    /// Starts every checked bake over the selected tiles as a background
    /// job ([`BakeJobHandle`]); [`Self::poll_job`] records the outcome in
    /// `status`/`last_records` when it lands (never propagates: the panel
    /// reports failures as a status line, not a crash). A no-op while a
    /// bake is already in flight.
    fn bake_now(
        &mut self,
        device: &umber_gpu::WgpuDevice,
        queue: &umber_gpu::WgpuQueue,
        mesh: &umber_mesh::MeshData,
        mesh_path: Option<&Path>,
    ) {
        if self.is_baking() {
            return;
        }
        let present = self.mesh_tiles.get(mesh).to_vec();
        let selected = self.tiles.resolve(&present);
        let plan = plan_bake(&self.selected_maps(), &present, &selected);
        let (skew_tiles, skew_skipped) = self.skew_tiles(&present, &selected);
        let sets = self.sets_for_bake(mesh, mesh_path);
        self.bake_jobs(
            device,
            queue,
            &BakeDispatch {
                mesh,
                plan,
                skew_tiles,
                skew_skipped,
                sets,
            },
        );
    }

    /// The sets one bake writes: the enabled checklist sets, or the
    /// mesh's single set when the checklist never synced (the pre-§6
    /// single-set path, byte-identical to before).
    fn sets_for_bake(&self, mesh: &umber_mesh::MeshData, mesh_path: Option<&Path>) -> Vec<String> {
        if self.sets.is_empty() {
            return Self::set_names_for(mesh, mesh_path);
        }
        self.enabled_sets()
    }

    /// Starts a filtered bake (the auto-rebake fire path): only `fire`'s
    /// maps over its tiles for its sets. A no-op while a bake is already
    /// in flight (the pending dirty set was already taken — the next
    /// frame's tracking re-marks what is still stale).
    fn bake_filtered(
        &mut self,
        device: &umber_gpu::WgpuDevice,
        queue: &umber_gpu::WgpuQueue,
        mesh: &umber_mesh::MeshData,
        mesh_path: Option<&Path>,
        fire: &AutoRebakeFire,
    ) {
        if self.is_baking() {
            return;
        }
        let present = self.mesh_tiles.get(mesh).to_vec();
        let plan = plan_bake(&fire.maps, &present, &fire.tiles);
        let (skew_tiles, skew_skipped) = if fire.skew {
            self.skew_tiles(&present, &fire.tiles)
        } else {
            (Vec::new(), Vec::new())
        };
        // The auto fire resolves its own sets (already checklist-filtered);
        // fall back to the mesh's single set when the checklist is empty.
        let fallback = PathBuf::from(FALLBACK_TEXTURE_SET);
        let single_set = umber_mesh::texture_set_name(mesh_path.unwrap_or(&fallback), mesh);
        let sets = if fire.sets.is_empty() {
            vec![single_set]
        } else {
            fire.sets.clone()
        };
        // Naming follows the plan (tiled iff the MESH is multi-tile —
        // plan_bake derives it from `present`, not from the fire's tile
        // subset — so a single-tile auto fire keeps the tiled names).
        self.bake_jobs(
            device,
            queue,
            &BakeDispatch {
                mesh,
                plan,
                skew_tiles,
                skew_skipped,
                sets,
            },
        );
    }

    /// Resolves output paths per set and spawns the worker: one job per
    /// (set × planned job) plus one skew file per (set × skew tile). The
    /// per-set fan-out writes the shared bake under each enabled set's
    /// name (per-material masked bakes are the named follow-up — v1
    /// establishes the checklist + naming, not the masking).
    fn bake_jobs(
        &mut self,
        device: &umber_gpu::WgpuDevice,
        queue: &umber_gpu::WgpuQueue,
        dispatch: &BakeDispatch<'_>,
    ) {
        let (mesh, plan, skew_tiles, skew_skipped, sets) = (
            dispatch.mesh,
            &dispatch.plan,
            &dispatch.skew_tiles,
            &dispatch.skew_skipped,
            &dispatch.sets,
        );
        let mut jobs = Vec::new();
        let mut skew_jobs = Vec::new();
        for set in &sets {
            for job in &plan.jobs {
                let path = if plan.tiled {
                    self.tiled_output_path_for(set, job.selection, job.tile)
                } else {
                    self.output_path_for(set, job.selection)
                };
                jobs.push((*job, path, set.clone()));
            }
            for tile in &skew_tiles {
                let path = if plan.tiled {
                    self.skew_tiled_output_path_for(set, *tile)
                } else {
                    self.skew_output_path_for(set)
                };
                skew_jobs.push((*tile, path, set.clone()));
            }
        }
        let count = jobs.len() + skew_jobs.len();
        let skipped = plan.skipped;
        let request = BakeRequest {
            device: device.clone(),
            queue: queue.clone(),
            mesh: mesh.clone(),
            size: self.resolution,
            rays: self.rays,
            dilation_iterations: self.dilation_iterations,
            skew_threshold_deg: self.skew_threshold_deg,
            jobs,
            skew_jobs,
        };
        self.start_job(count, skipped, skew_skipped.clone(), move || {
            run_bake(request)
        });
    }

    /// Spawns `work` as the panel's bake job and shows the in-flight
    /// status; returns `false` (and does nothing) when a job is already
    /// running. `bake_now`'s only path onto a worker — the plumbing tests
    /// drive it directly with GPU-free closures.
    fn start_job<F>(
        &mut self,
        job_count: usize,
        skipped: Vec<(u16, BakeSelection)>,
        skipped_skew: Vec<u16>,
        work: F,
    ) -> bool
    where
        F: FnOnce() -> BakeOutcome + Send + 'static,
    {
        if self.is_baking() {
            return false;
        }
        match BakeJobHandle::spawn(skipped, skipped_skew, work) {
            Ok(job) => {
                self.status = tr_args("status.baking", [("count", FluentValue::from(job_count))]);
                self.job = Some(job);
                true
            }
            Err(err) => {
                log::error!("bake worker failed to start: {err}");
                let args = [("error", FluentValue::from(err.to_string()))];
                self.status = tr_args("bakes.worker-failed", args);
                false
            }
        }
    }

    /// Records a finished job's outcome in `status`/`last_records` — the
    /// same status lines the synchronous Bake wrote — plus one bake-log
    /// entry per written map (§6:59).
    fn apply_outcome(
        &mut self,
        outcome: BakeOutcome,
        skipped: &[(u16, BakeSelection)],
        skipped_skew: &[u16],
    ) {
        match outcome {
            Ok(records) => {
                let total: u128 = records.iter().map(|r| r.elapsed_ms).sum();
                let parts: Vec<String> = records
                    .iter()
                    .map(|r| {
                        let name = r
                            .path
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| r.path.display().to_string());
                        format!("{name} ({} ms)", r.elapsed_ms)
                    })
                    .collect();
                let args = [
                    ("count", FluentValue::from(records.len())),
                    ("ms", FluentValue::from(total.to_string())),
                    ("files", FluentValue::from(parts.join(", "))),
                ];
                let mut status = tr_args("bakes.done", args);
                if let Some(note) = skipped_note(skipped) {
                    status.push_str(&format!(" — {note}"));
                }
                if let Some(note) = skipped_skew_note(skipped_skew) {
                    status.push_str(&format!(" — {note}"));
                }
                self.status = status;
                let now = Self::now_unix_secs();
                for record in &records {
                    self.log.push(
                        now,
                        record.set.clone(),
                        record.selection,
                        record.tile,
                        record.elapsed_ms,
                        record.skew_clamped,
                    );
                }
                self.last_records = records;
            }
            Err(err) => {
                log::error!("bake failed: {err:#}");
                let args = [("error", FluentValue::from(format!("{err:#}")))];
                self.status = tr_args("bakes.failed", args);
            }
        }
    }
}

/// The bake driver (runs on the job's worker thread): one mesh-fed bake
/// per planned job ([`plan_bake`]), each followed by the dilation
/// post-pass, written as PNG to the job's pre-resolved path (the mesh-map
/// convention, tile-suffixed when the plan is tiled). `Result`-typed so
/// failures carry context instead of unwrapping; the first failure ends
/// the bake (maps already written stay on disk, as before).
///
/// Skew (§6:57) rides along: skew jobs emit first (v1: the flat CPU-model
/// map — zero measured deviation — per the module docs; the GPU
/// `SKEW_BAKE_SHADER` wiring replaces this emission without touching the
/// guard), and the AO arm — the skew-guarded baker — runs the
/// edge-protection guard over its dilated bytes under this run's skew map
/// for the same tile (no skew bytes for the tile = uncovered guard = no
/// clamp, never a misaligned guard). The clamped count lands in the
/// record (the bake log's warnings column).
fn run_bake(request: BakeRequest) -> BakeOutcome {
    use anyhow::Context as _;

    let BakeRequest {
        device,
        queue,
        mesh,
        size,
        rays,
        dilation_iterations,
        skew_threshold_deg,
        jobs,
        skew_jobs,
    } = request;
    let (device, queue, mesh) = (&device, &queue, &mesh);
    let dilate = umber_bake::DilateParams::new(dilation_iterations);
    let mut records = Vec::new();
    let mut skew_by_tile: HashMap<u16, Vec<u8>> = HashMap::new();

    for (tile, path, set) in skew_jobs {
        let started = Instant::now();
        let texels = (size as usize) * (size as usize);
        // V1 flat emission (see this function's doc comment).
        let baked = umber_bake::skew::skew_map_from_deviations(
            &vec![0.0f32; texels],
            &vec![true; texels],
            size,
            size,
        );
        let combined = crate::bake_sources::combine_skew_with_paint(&baked, None);
        umber_export::png::write_png(
            &path,
            size,
            size,
            &combined,
            umber_export::png::Transfer::Srgb,
        )
        .with_context(|| format!("writing {}", path.display()))?;
        skew_by_tile.insert(tile, combined);
        records.push(BakeRecord {
            selection: None,
            set,
            tile,
            path,
            elapsed_ms: started.elapsed().as_millis(),
            skew_clamped: 0,
        });
    }

    for (job, path, set) in jobs {
        let selection = job.selection;
        let started = Instant::now();
        // `plan_bake` routes only AO to `BakeSource::Tile`; every
        // other map's job is a whole-mesh bake.
        let mut skew_clamped = 0usize;
        let bytes: Vec<u8> = match selection {
            BakeSelection::Ao => {
                // Shared with the Export dialog (`bake_sources::bake_ao`)
                // — this panel dilates afterward; the export path uses
                // the raw bake directly (see `bake_sources` docs).
                let raw = match job.source {
                    BakeSource::WholeMesh => {
                        crate::bake_sources::bake_ao(device, queue, mesh, size, rays)?
                    }
                    BakeSource::Tile => crate::bake_sources::bake_ao_tile(
                        device, queue, mesh, size, rays, job.tile,
                    )?,
                };
                let mut dilated =
                    umber_bake::dilation::dilate_map(device, queue, &raw, size, size, &dilate)
                        .context("ao dilation")?;
                // Edge protection (§6:57): the guard in the bake pass,
                // over the DILATED bytes (the guard sees what ships).
                if let Some(skew) = skew_by_tile.get(&job.tile) {
                    skew_clamped = crate::bake_sources::apply_skew_edge_protection(
                        &mut dilated,
                        skew,
                        skew_threshold_deg,
                    );
                }
                dilated
            }
            BakeSelection::Curvature => {
                let raw = umber_bake::curvature::bake_curvature_mesh(
                    device,
                    queue,
                    mesh,
                    size,
                    size,
                    &umber_bake::CurvatureParams::default(),
                )
                .context("curvature bake")?;
                umber_bake::dilation::dilate_map(device, queue, &raw, size, size, &dilate)
                    .context("curvature dilation")?
            }
            BakeSelection::Thickness => {
                let params = umber_bake::ThicknessParams {
                    rays,
                    ..umber_bake::ThicknessParams::default()
                };
                let raw = umber_bake::thickness::bake_thickness_mesh(
                    device, queue, mesh, size, size, &params,
                )
                .context("thickness bake")?;
                umber_bake::dilation::dilate_map(device, queue, &raw, size, size, &dilate)
                    .context("thickness dilation")?
            }
            BakeSelection::Position => {
                let params = umber_bake::position::PositionMapParams {
                    width: size,
                    height: size,
                };
                let f32map = umber_bake::position::bake_position_map(device, queue, mesh, &params)
                    .context("position bake")?;
                let raw = encode_position_rgba8(&f32map);
                umber_bake::dilation::dilate_map(device, queue, &raw, size, size, &dilate)
                    .context("position dilation")?
            }
        };
        umber_export::png::write_png(&path, size, size, &bytes, umber_export::png::Transfer::Srgb)
            .with_context(|| format!("writing {}", path.display()))?;
        records.push(BakeRecord {
            selection: Some(selection),
            set,
            tile: job.tile,
            path,
            elapsed_ms: started.elapsed().as_millis(),
            skew_clamped,
        });
    }
    Ok(records)
}

impl Default for BakesPanel {
    fn default() -> Self {
        Self::new(PathBuf::from("bakes"))
    }
}

/// Re-encodes an f32 position map (rgba32f: xyz = world position,
/// w = coverage) to RGBA8: each axis normalized over the covered texels'
/// own bounds to 0..255, alpha carrying coverage. Same normalization
/// `umber-cli`'s `bake-all` uses, so panel and CLI outputs match.
///
/// Uncovered texels (and fully-uncovered maps) encode as zero bytes —
/// never NaN, never a divide-by-zero.
fn encode_position_rgba8(position_f32: &[f32]) -> Vec<u8> {
    let texels = position_f32.len() / 4;
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for t in 0..texels {
        if position_f32.get(t * 4 + 3).copied().unwrap_or(0.0) > 0.5 {
            for axis in 0..3 {
                let v = position_f32[t * 4 + axis];
                min[axis] = min[axis].min(v);
                max[axis] = max[axis].max(v);
            }
        }
    }
    let span = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let mut out = vec![0u8; texels * 4];
    for t in 0..texels {
        if position_f32[t * 4 + 3] > 0.5 {
            for axis in 0..3 {
                let v = if span[axis] > f32::EPSILON {
                    (position_f32[t * 4 + axis] - min[axis]) / span[axis]
                } else {
                    0.5
                };
                out[t * 4 + axis] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            out[t * 4 + 3] = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> BakesPanel {
        BakesPanel::new(PathBuf::from("/tmp/umber-bakes-test"))
    }

    #[test]
    fn defaults_match_documented_constants() {
        let p = panel();
        assert_eq!(p.resolution(), DEFAULT_RESOLUTION);
        assert_eq!(p.resolution(), 512);
        assert_eq!(p.rays(), DEFAULT_RAYS);
        assert_eq!(p.rays(), 16);
        assert_eq!(p.dilation_iterations(), DEFAULT_DILATION_ITERATIONS);
        assert_eq!(p.dilation_iterations(), 16);
        assert_eq!(p.selected_maps(), BakeSelection::ALL.to_vec());
        assert!(p.any_selected());
        assert_eq!(p.status(), "No bake yet.");
        assert!(p.last_records().is_empty());
    }

    #[test]
    fn supported_resolutions_accepted_others_rejected() {
        // The shared 512..8K presets — 4096 and 8192 were rejected before
        // the 8K slice (the list stopped at 2048).
        for accepted in [512, 1024, 2048, 4096, 8192] {
            assert!(BakesPanel::is_supported_resolution(accepted));
            let mut p = panel();
            assert!(p.set_resolution(accepted));
            assert_eq!(p.resolution(), accepted);
        }
        for rejected in [0, 100, 128, 256, 300, 513, 8193, u32::MAX] {
            assert!(!BakesPanel::is_supported_resolution(rejected));
            let mut p = panel();
            assert!(!p.set_resolution(rejected));
            assert_eq!(p.resolution(), DEFAULT_RESOLUTION);
        }
    }

    #[test]
    fn selection_toggling_drives_any_selected() {
        let mut p = panel();
        for selection in BakeSelection::ALL {
            assert!(p.is_map_enabled(selection));
        }
        for selection in BakeSelection::ALL {
            p.set_map_enabled(selection, false);
        }
        assert!(!p.any_selected());
        assert!(p.selected_maps().is_empty());
        p.set_map_enabled(BakeSelection::Curvature, true);
        assert!(p.any_selected());
        assert_eq!(p.selected_maps(), vec![BakeSelection::Curvature]);
    }

    #[test]
    fn can_bake_gates_on_mesh_gpu_and_selection() {
        let mut p = panel();
        assert!(p.can_bake(true, true));
        assert!(!p.can_bake(false, true));
        assert!(!p.can_bake(true, false));
        assert!(!p.can_bake(false, false));
        for selection in BakeSelection::ALL {
            p.set_map_enabled(selection, false);
        }
        assert!(!p.can_bake(true, true));
    }

    #[test]
    fn rays_and_dilation_setters_clamp() {
        let mut p = panel();
        p.set_rays(3);
        assert_eq!(p.rays(), MIN_RAYS);
        p.set_rays(65);
        assert_eq!(p.rays(), MAX_RAYS);
        p.set_rays(32);
        assert_eq!(p.rays(), 32);

        p.set_dilation_iterations(u32::MAX);
        assert_eq!(p.dilation_iterations(), MAX_DILATION_ITERATIONS);
        p.set_dilation_iterations(0);
        assert_eq!(p.dilation_iterations(), 0);
    }

    #[test]
    fn output_paths_follow_mesh_map_convention() {
        let p = panel();
        let dir = Path::new("/tmp/umber-bakes-test");
        for (selection, suffix) in [
            (BakeSelection::Ao, "ambient_occlusion"),
            (BakeSelection::Curvature, "curvature"),
            (BakeSelection::Thickness, "thickness"),
            (BakeSelection::Position, "position"),
        ] {
            let path = p.output_path_for("Sword", selection);
            assert_eq!(
                path,
                dir.join(format!("Sword_{suffix}.png")),
                "{selection:?}"
            );
            // The mesh-map parser recovers the same set + kind.
            let parsed =
                umber_mesh::parse_mesh_map(&path).expect("panel output parses as a mesh map");
            assert_eq!(parsed.texture_set, "Sword");
            assert_eq!(parsed.kind, selection.mesh_map_kind());
        }
    }

    #[test]
    fn selection_labels_and_kinds_cover_all_maps() {
        for selection in BakeSelection::ALL {
            assert!(!selection.label().is_empty());
            // Suffix is non-empty so the formatted filename always has
            // a `<set>_<suffix>.png` shape.
            assert!(!selection.mesh_map_kind().suffix().is_empty());
        }
    }

    #[test]
    fn position_encode_normalizes_covered_texels_over_own_bounds() {
        // 2x2 map: x spans 0..10, y/z constant (degenerate span), one
        // texel uncovered.
        let f32map: Vec<f32> = vec![
            0.0, 5.0, -3.0, 1.0, // covered: x=min
            10.0, 5.0, -3.0, 1.0, // covered: x=max
            99.0, 99.0, 99.0, 0.0, // uncovered: ignored for bounds
            5.0, 5.0, -3.0, 1.0, // covered: x=mid
        ];
        let out = encode_position_rgba8(&f32map);
        assert_eq!(out.len(), 16);
        // x: 0 -> 0, 10 -> 255, 5 -> 128 (rounded).
        assert_eq!(out[0], 0);
        assert_eq!(out[4], 255);
        assert_eq!(out[12], 128);
        // Degenerate y/z spans map to mid-gray.
        assert_eq!(out[1], 128);
        assert_eq!(out[2], 128);
        // Covered alpha is opaque, uncovered texel is all zeros.
        assert_eq!(out[3], 255);
        assert_eq!(&out[8..12], &[0, 0, 0, 0]);
        assert_eq!(out[15], 255);
    }

    /// The udim.rs two-tile fixture: triangle 0 -> 1001, triangle 1 -> 1002.
    fn two_tile_mesh() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![[0.0; 3]; 4],
            normals: vec![[0.0; 3]; 4],
            uvs: vec![[0.5, 0.5], [0.7, 0.5], [0.5, 0.7], [1.5, 0.5]],
            indices: vec![0, 1, 2, 3, 1, 2],
            material_names: vec!["m".into()],
        }
    }

    fn job(selection: BakeSelection, tile: u16, source: BakeSource) -> BakeJob {
        BakeJob {
            selection,
            tile,
            source,
        }
    }

    #[test]
    fn fresh_panel_selects_every_present_tile() {
        let present = umber_mesh::present_tiles(&two_tile_mesh());
        assert_eq!(present, vec![1001, 1002]);
        let mut p = panel();
        // `show` syncs the selector from the mesh each frame.
        p.tiles_mut().sync(&present);
        assert_eq!(
            p.tiles_mut().selected(),
            &present
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<u16>>()
        );
        let plan = p.bake_plan(&present);
        let tiles: Vec<u16> = plan.jobs.iter().map(|j| j.tile).collect();
        // AO covers both tiles; curvature/thickness/position bake whole-
        // mesh for 1001 and are skipped (named) for 1002.
        assert_eq!(tiles, vec![1001, 1001, 1001, 1001, 1002]);
        assert!(plan.tiled);
    }

    #[test]
    fn deselecting_a_tile_shrinks_the_bake_list() {
        let present = [1001, 1002];
        let mut p = panel();
        p.tiles_mut().sync(&present);
        p.tiles_mut().set_selected(1001, false);
        let plan = p.bake_plan(&present);
        // Only tile 1002 remains: AO per tile; the three whole-mesh maps
        // have no 1002 baker in v1 and are reported, not mis-written.
        assert_eq!(
            plan.jobs,
            vec![job(BakeSelection::Ao, 1002, BakeSource::Tile)]
        );
        assert_eq!(
            plan.skipped,
            vec![
                (1002, BakeSelection::Curvature),
                (1002, BakeSelection::Thickness),
                (1002, BakeSelection::Position),
            ]
        );
        assert!(plan.tiled);
    }

    #[test]
    fn multi_tile_plan_is_tile_major_with_ao_per_tile() {
        let maps = [BakeSelection::Ao, BakeSelection::Curvature];
        let plan = plan_bake(&maps, &[1001, 1002], &[1001, 1002]);
        assert_eq!(
            plan.jobs,
            vec![
                // Tile 1001 AO goes through the tile window too (multi-
                // tile mesh: the whole-mesh AO would also see 1002's
                // triangles clipped at the [0, 1] edge).
                job(BakeSelection::Ao, 1001, BakeSource::Tile),
                job(BakeSelection::Curvature, 1001, BakeSource::WholeMesh),
                job(BakeSelection::Ao, 1002, BakeSource::Tile),
            ]
        );
        assert_eq!(plan.skipped, vec![(1002, BakeSelection::Curvature)]);
        // A mesh living only in 1002 is "multi-tile" for naming: its
        // outputs must never carry the untiled (= 1001) name.
        let lone = plan_bake(&maps, &[1002], &[1002]);
        assert!(lone.tiled);
        assert_eq!(
            lone.jobs,
            vec![job(BakeSelection::Ao, 1002, BakeSource::Tile)]
        );
        // Nothing selected: nothing baked.
        assert!(plan_bake(&maps, &[1001, 1002], &[]).jobs.is_empty());
    }

    #[test]
    fn single_tile_mesh_plan_is_the_pre_udim_bake() {
        // Regression: an all-[0, 1] mesh bakes every checked map whole-
        // mesh with untiled names — exactly the pre-selector behavior.
        let single = umber_mesh::MeshData {
            positions: vec![[0.0; 3]; 3],
            normals: vec![[0.0; 3]; 3],
            uvs: vec![[0.1, 0.1], [0.9, 0.1], [0.1, 0.9]],
            indices: vec![0, 1, 2],
            material_names: vec!["m".into()],
        };
        let present = umber_mesh::present_tiles(&single);
        assert_eq!(present, vec![1001]);
        let mut p = panel();
        p.tiles_mut().sync(&present);
        // The selector renders one tile, and it can't be unchecked away.
        assert_eq!(p.tiles_mut().known(), &[1001]);
        p.tiles_mut().set_selected(1001, false);
        assert!(!p.tiles_mut().nothing_selected());
        assert_eq!(p.tiles_mut().tiles(), vec![1001]);
        let plan = p.bake_plan(&present);
        let expected: Vec<BakeJob> = BakeSelection::ALL
            .into_iter()
            .map(|s| job(s, 1001, BakeSource::WholeMesh))
            .collect();
        assert_eq!(plan.jobs, expected);
        assert!(!plan.tiled);
        assert!(plan.skipped.is_empty());
        // An empty mesh (no tiles) takes the same path: the bake core
        // reports the empty mesh, as before.
        assert_eq!(p.bake_plan(&[]).jobs, expected);
    }

    #[test]
    fn tiled_output_paths_suffix_the_tile() {
        let p = panel();
        assert_eq!(
            p.tiled_output_path_for("Sword", BakeSelection::Ao, 1002),
            Path::new("/tmp/umber-bakes-test").join("Sword_ambient_occlusion_1002.png")
        );
        assert_eq!(
            p.tiled_output_path_for("Sword", BakeSelection::Curvature, 1001),
            Path::new("/tmp/umber-bakes-test").join("Sword_curvature_1001.png")
        );
    }

    #[test]
    fn skipped_note_names_each_tile_and_map() {
        assert_eq!(skipped_note(&[]), None);
        assert_eq!(
            skipped_note(&[(1002, BakeSelection::Curvature), (1011, BakeSelection::Position)])
                .as_deref(),
            Some("skipped (v1 bakes only AO per tile; other maps tile 1001 only): Curvature 1002, Position 1011")
        );
    }

    #[test]
    fn position_encode_all_uncovered_is_all_zeros() {
        let f32map = vec![1.0f32, 2.0, 3.0, 0.0];
        assert_eq!(encode_position_rgba8(&f32map), vec![0, 0, 0, 0]);
        assert!(encode_position_rgba8(&[]).is_empty());
    }

    // --- Async bake job plumbing (wave 5) -------------------------------

    /// Drives the real per-frame `poll_job` until the job lands; fails
    /// after 5 s so a hung (or never-spawned) worker fails the test
    /// instead of hanging it.
    fn await_job(p: &mut BakesPanel) {
        await_job_within(p, 5);
    }

    /// [`await_job`] with a custom deadline — the GPU-gated tests allow a
    /// cold shader compile on a software adapter.
    fn await_job_within(p: &mut BakesPanel, secs: u64) {
        let deadline = Instant::now() + std::time::Duration::from_secs(secs);
        while p.poll_job() {
            assert!(Instant::now() < deadline, "bake job never finished");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// A job body that parks on a gate until the test releases it, then
    /// yields `outcome` — makes the "still baking" window deterministic.
    fn gated(outcome: BakeOutcome) -> (mpsc::Sender<()>, Box<dyn FnOnce() -> BakeOutcome + Send>) {
        let (release, gate) = mpsc::channel::<()>();
        let work = move || {
            let _ = gate.recv();
            outcome
        };
        (release, Box::new(work))
    }

    fn record(name: &str, elapsed_ms: u128) -> BakeRecord {
        BakeRecord {
            selection: Some(BakeSelection::Ao),
            set: "Sword".to_string(),
            tile: FIRST_TILE,
            path: Path::new("/tmp/umber-bakes-test").join(name),
            elapsed_ms,
            skew_clamped: 0,
        }
    }

    #[test]
    fn bake_job_lands_through_poll_only_after_the_worker_finishes() {
        // THE CAN-FAIL CORE: the result reaches `status`/`last_records`
        // only through `poll_job` after the worker sends — a bake that
        // ran inline (or a poll that never drains) fails here.
        let mut p = panel();
        let (release, work) = gated(Ok(vec![record("Sword_ambient_occlusion.png", 7)]));
        assert!(p.start_job(1, vec![(1002, BakeSelection::Curvature)], Vec::new(), work));
        assert!(p.is_baking());
        // Worker parked on the gate: polling reports in-flight, and the
        // panel shows the baking status with no records yet.
        assert!(p.poll_job());
        assert!(p.is_baking());
        assert_eq!(p.status(), "Baking 1 map…");
        assert!(p.last_records().is_empty());

        release.send(()).expect("worker is waiting on the gate");
        await_job(&mut p);
        assert!(!p.is_baking());
        assert_eq!(p.last_records().len(), 1);
        assert_eq!(
            p.status(),
            "Baked 1 map in 7 ms: Sword_ambient_occlusion.png (7 ms) — skipped (v1 bakes \
             only AO per tile; other maps tile 1001 only): Curvature 1002"
        );
        // Drained: further polls are idle and keep the result.
        assert!(!p.poll_job());
        assert_eq!(p.last_records().len(), 1);
    }

    #[test]
    fn in_flight_job_disables_bake_and_refuses_a_second_start() {
        let mut p = panel();
        assert!(p.can_bake(true, true));
        let (release, work) = gated(Ok(vec![record("first.png", 1)]));
        assert!(p.start_job(1, Vec::new(), Vec::new(), work));
        // The button's enabled logic: off while baking, whatever else holds.
        assert!(!p.can_bake(true, true));
        // A second click is a no-op: the in-flight job stays the one that
        // lands (the second body would report "second.png").
        let (_second_release, second) = gated(Ok(vec![record("second.png", 2)]));
        assert!(!p.start_job(1, Vec::new(), Vec::new(), second));
        assert_eq!(p.status(), "Baking 1 map…");

        release.send(()).expect("worker is waiting on the gate");
        await_job(&mut p);
        assert_eq!(p.last_records()[0].path.file_name().unwrap(), "first.png");
        // Done: the button re-enables.
        assert!(p.can_bake(true, true));
    }

    #[test]
    fn failed_job_reaches_the_status_line_and_keeps_old_records() {
        let mut p = panel();
        let (release, work) = gated(Ok(vec![record("old.png", 3)]));
        p.start_job(1, Vec::new(), Vec::new(), work);
        release.send(()).unwrap();
        await_job(&mut p);

        let (release, work) = gated(Err(anyhow::anyhow!("empty mesh").context("ao bake")));
        assert!(p.start_job(4, Vec::new(), Vec::new(), work));
        assert_eq!(p.status(), "Baking 4 maps…");
        release.send(()).unwrap();
        await_job(&mut p);
        assert!(!p.is_baking());
        // `{err:#}` keeps the whole context chain, as the sync path did.
        assert_eq!(p.status(), "Bake failed: ao bake: empty mesh");
        // A failure doesn't wipe the last successful bake's records.
        assert_eq!(p.last_records()[0].path.file_name().unwrap(), "old.png");
        assert!(p.can_bake(true, true));
    }

    #[test]
    fn panicked_worker_reports_failure_instead_of_baking_forever() {
        // A worker that dies without sending drops the channel: the poll
        // must turn `Disconnected` into a failure and re-enable the button
        // (treating it like `Empty` would show "Baking…" forever).
        let mut p = panel();
        assert!(p.start_job(1, Vec::new(), Vec::new(), || panic!(
            "simulated wgpu validation panic"
        )));
        await_job(&mut p);
        assert!(!p.is_baking());
        assert!(
            p.status()
                .starts_with("Bake failed: bake worker exited without a result"),
            "{}",
            p.status()
        );
        assert!(p.can_bake(true, true));
    }

    /// A plain device (graceful skip where no wgpu adapter exists — the
    /// `umber-bake` GPU tests' pattern; the bakers need no extra features).
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return None;
        };
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    #[test]
    fn bake_now_runs_on_a_worker_and_writes_the_map() {
        // GPU-gated end to end: the real `bake_now` hands a cloned device,
        // queue, and mesh to the worker; the map only lands via `poll_job`.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let quad = umber_mesh::MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
        };
        let dir = std::env::temp_dir().join(format!("umber-bake-job-{}", std::process::id()));
        let mut p = BakesPanel::new(dir.clone());
        // The smallest preset (sub-512 sizes left the list with 8K).
        assert!(p.set_resolution(512));
        p.set_rays(MIN_RAYS);
        for selection in [
            BakeSelection::Curvature,
            BakeSelection::Thickness,
            BakeSelection::Position,
        ] {
            p.set_map_enabled(selection, false);
        }
        p.bake_now(&device, &queue, &quad, Some(Path::new("Quad.obj")));
        // Only `poll_job` clears the job, so this holds even if the
        // worker already finished — a synchronous bake_now fails here.
        assert!(p.is_baking());
        assert!(p.last_records().is_empty());
        await_job_within(&mut p, 60);
        assert!(p.status().starts_with("Baked 1 map"), "{}", p.status());
        let written = &p.last_records()[0].path;
        assert_eq!(written, &dir.join("Quad_ambient_occlusion.png"));
        assert!(written.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bake_now_error_from_the_worker_reaches_the_status_line() {
        // GPU-gated: an empty mesh fails inside the worker (the bake core
        // rejects it); the error crosses the channel into the status line.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut p = panel();
        p.bake_now(&device, &queue, &umber_mesh::MeshData::default(), None);
        assert!(p.is_baking());
        await_job_within(&mut p, 60);
        assert!(
            p.status().starts_with("Bake failed: ao bake"),
            "{}",
            p.status()
        );
        assert!(p.last_records().is_empty());
        assert!(p.can_bake(true, true));
    }

    // --- Bake mode (§6:57–59) -------------------------------------------

    #[test]
    fn skew_channel_defaults_off_and_threshold_clamps() {
        let mut p = panel();
        assert!(!p.skew_enabled());
        assert_eq!(p.skew_threshold_deg(), DEFAULT_SKEW_THRESHOLD_DEG);
        assert_eq!(p.skew_threshold_deg(), 15.0);
        // The local suffix pins the bake-engine convention.
        assert_eq!(SKEW_SUFFIX, umber_bake::BakeMap::Skew.file_stem_suffix());
        assert_eq!(SKEW_SUFFIX, "skew");
        assert!(!crate::bake_sources::SKEW_CHANNEL_NAME.is_empty());

        p.set_skew_threshold_deg(-5.0);
        assert_eq!(p.skew_threshold_deg(), MIN_SKEW_THRESHOLD_DEG);
        p.set_skew_threshold_deg(500.0);
        assert_eq!(p.skew_threshold_deg(), MAX_SKEW_THRESHOLD_DEG);
        p.set_skew_threshold_deg(20.0);
        assert_eq!(p.skew_threshold_deg(), 20.0);

        p.set_skew_enabled(true);
        assert!(p.skew_enabled());
        assert!(p.any_selected());
        // Skew-only selection still bakes (the mask alone is a valid bake).
        for selection in BakeSelection::ALL {
            p.set_map_enabled(selection, false);
        }
        assert!(p.any_selected());
        assert!(p.can_bake(true, true));
        p.set_skew_enabled(false);
        assert!(!p.any_selected());

        // Skew output paths follow the TextureSetName_map convention.
        let dir = Path::new("/tmp/umber-bakes-test");
        assert_eq!(p.skew_output_path_for("Sword"), dir.join("Sword_skew.png"));
        assert_eq!(
            p.skew_tiled_output_path_for("Sword", 1002),
            dir.join("Sword_skew_1002.png")
        );
    }

    #[test]
    fn skew_tiles_route_1001_only() {
        let mut p = panel();
        // Off: no skew jobs, no skips.
        assert_eq!(p.skew_tiles(&[1001], &[1001]), (Vec::new(), Vec::new()));
        p.set_skew_enabled(true);
        // Single-tile mesh: the pre-UDIM whole-mesh path covers 1001.
        assert_eq!(p.skew_tiles(&[1001], &[1001]), (vec![1001], Vec::new()));
        assert_eq!(p.skew_tiles(&[], &[]), (vec![1001], Vec::new()));
        // Multi-tile mesh: 1001 bakes whole-mesh, the rest are reported
        // skipped — never written under the wrong tile's name.
        assert_eq!(
            p.skew_tiles(&[1001, 1002], &[1001, 1002]),
            (vec![1001], vec![1002])
        );
        assert_eq!(
            p.skew_tiles(&[1001, 1002], &[1002]),
            (Vec::new(), vec![1002])
        );
    }

    #[test]
    fn skipped_skew_note_names_tiles() {
        assert_eq!(skipped_skew_note(&[]), None);
        assert_eq!(
            skipped_skew_note(&[1002, 1011]).as_deref(),
            Some("skew bakes tile 1001 only, skipped tiles: 1002, 1011")
        );
    }

    #[test]
    fn bake_map_of_covers_the_checklist() {
        // The auto-rebake dirty set (BakeMap) meets the checklist
        // (BakeSelection) 1:1 for the four color maps; skew is panel
        // state, not a BakeSelection (see BakeRecord's docs).
        for (selection, map) in [
            (BakeSelection::Ao, umber_bake::BakeMap::AmbientOcclusion),
            (BakeSelection::Curvature, umber_bake::BakeMap::Curvature),
            (BakeSelection::Thickness, umber_bake::BakeMap::Thickness),
            (BakeSelection::Position, umber_bake::BakeMap::Position),
        ] {
            assert_eq!(bake_map_of(selection), map, "{selection:?}");
        }
    }

    #[test]
    fn sets_checklist_syncs_filters_and_bakes_all() {
        // Material names become the checklist rows (sorted, deduped).
        let mesh = umber_mesh::MeshData {
            material_names: vec!["B".into(), "A".into(), "A".into()],
            ..umber_mesh::MeshData::default()
        };
        assert_eq!(
            BakesPanel::set_names_for(&mesh, None),
            vec!["A".to_string(), "B".to_string()]
        );
        // No materials: the single texture-set fallback (today's path).
        let bare = umber_mesh::MeshData::default();
        assert_eq!(
            BakesPanel::set_names_for(&bare, Some(Path::new("/m/Sword.obj"))),
            vec!["Sword".to_string()]
        );

        let mut p = panel();
        assert!(p.sets().is_empty());
        assert!(p.enabled_sets().is_empty());
        p.sync_sets(&["A".to_string(), "B".to_string()]);
        // New sets arrive enabled (Painter checks all sets by default).
        assert_eq!(p.enabled_sets(), vec!["A".to_string(), "B".to_string()]);
        assert!(p.set_set_enabled("B", false));
        assert_eq!(p.enabled_sets(), vec!["A".to_string()]);
        assert!(!p.set_set_enabled("Z", true));
        // Re-sync keeps the unchecked-away set unchecked, drops vanished
        // sets, enables brand-new ones.
        p.sync_sets(&["B".to_string(), "C".to_string()]);
        assert_eq!(
            p.sets(),
            &[
                SetCheck {
                    name: "B".to_string(),
                    enabled: false,
                },
                SetCheck {
                    name: "C".to_string(),
                    enabled: true,
                },
            ]
        );
        // Bake-all re-checks everything: sets, maps, and skew.
        for selection in BakeSelection::ALL {
            p.set_map_enabled(selection, false);
        }
        p.set_skew_enabled(false);
        p.bake_all_sets();
        assert_eq!(p.enabled_sets(), vec!["B".to_string(), "C".to_string()]);
        assert_eq!(p.selected_maps(), BakeSelection::ALL.to_vec());
        assert!(p.skew_enabled());
    }

    #[test]
    fn log_entry_format_and_jump_to_setting() {
        // THE FORMAT PROBE: timestamp, map, set, duration, warnings —
        // exactly this string (the dragon checks it statically).
        let mut log = BakeLog::new();
        assert!(log.is_empty());
        let clean = log.push(1, "Sword".into(), Some(BakeSelection::Ao), 1001, 7, 0);
        let clamped = log.push(2, "Sword".into(), Some(BakeSelection::Ao), 1001, 9, 3);
        let skew = log.push(2, "Blade".into(), None, 1001, 4, 0);
        assert_eq!(log.len(), 3);
        assert_eq!(
            log.entry(clean).expect("just pushed").summary(),
            "[1] Sword Ambient occlusion (tile 1001): 7 ms"
        );
        assert_eq!(
            log.entry(clamped).expect("just pushed").summary(),
            "[2] Sword Ambient occlusion (tile 1001): 9 ms — 3 skew-clamped texels"
        );
        assert_eq!(
            log.entry(skew).expect("just pushed").summary(),
            "[2] Blade Skew (correction mask) (tile 1001): 4 ms"
        );

        // Jump-to-setting: the log row click highlights the responsible
        // row (clamped → threshold, clean AO → rays, skew → threshold,
        // position → resolution, curvature → dilation).
        let mut p = panel();
        assert!(!p.jump_to_setting(9999), "unknown id jumps nowhere");
        let t = BakesPanel::now_unix_secs();
        let id_ao = p
            .log
            .push(t, "S".into(), Some(BakeSelection::Ao), 1001, 1, 0);
        let id_clamped = p
            .log
            .push(t, "S".into(), Some(BakeSelection::Ao), 1001, 1, 5);
        let id_skew = p.log.push(t, "S".into(), None, 1001, 1, 0);
        let id_pos = p
            .log
            .push(t, "S".into(), Some(BakeSelection::Position), 1001, 1, 0);
        let id_curv = p
            .log
            .push(t, "S".into(), Some(BakeSelection::Curvature), 1001, 1, 0);
        assert!(p.jump_to_setting(id_ao));
        assert_eq!(p.highlighted_setting(), Some(BakeSettingId::Rays));
        assert!(p.jump_to_setting(id_clamped));
        assert_eq!(p.highlighted_setting(), Some(BakeSettingId::SkewThreshold));
        assert!(p.jump_to_setting(id_skew));
        assert_eq!(p.highlighted_setting(), Some(BakeSettingId::SkewThreshold));
        assert!(p.jump_to_setting(id_pos));
        assert_eq!(p.highlighted_setting(), Some(BakeSettingId::Resolution));
        assert!(p.jump_to_setting(id_curv));
        assert_eq!(p.highlighted_setting(), Some(BakeSettingId::Dilation));
        // The highlighted row carries the jump marker.
        assert_eq!(
            p.row_label(BakeSettingId::Dilation, "Dilation".to_string()),
            "▶ Dilation"
        );
        assert_eq!(p.row_label(BakeSettingId::Rays, "Rays".to_string()), "Rays");
    }

    #[test]
    fn param_tracking_marks_dirty_and_autorebake_fires_affected_only() {
        use std::time::Duration;

        let present = [1001, 1002];
        // A ray-count change dirties the raycast maps {AO, thickness} —
        // and nothing else (the affected-map set at panel level).
        let mut p = panel();
        p.set_auto_rebake(true);
        p.tiles_mut().sync(&present);
        p.sync_sets(&["A".to_string(), "B".to_string()]);
        p.set_map_enabled(BakeSelection::Thickness, false);
        let start = Instant::now();
        let selected = p.tiles.resolve(&present);
        p.set_rays(32);
        p.sync_param_tracking(start, &selected);
        // Inside the 500 ms window: pending but not due.
        assert!(p
            .poll_autorebake(start + Duration::from_millis(499), &selected)
            .is_none());
        // Due: ONLY the affected + still-checked maps fire (AO — thickness
        // is unchecked, curvature/position were never dirtied).
        let fire = p
            .poll_autorebake(start + Duration::from_millis(500), &selected)
            .expect("debounced fire is due");
        assert_eq!(fire.maps, vec![BakeSelection::Ao]);
        assert!(!fire.skew);
        assert_eq!(fire.tiles, vec![1001, 1002]);
        assert_eq!(fire.sets, vec!["A".to_string(), "B".to_string()]);
        // Firing takes the dirty sets: nothing pending afterward.
        assert!(p
            .poll_autorebake(start + Duration::from_secs(60), &selected)
            .is_none());

        // A skew-threshold change fires the skew tiles when checked (and
        // only the affected sets).
        let mut p = panel();
        p.set_auto_rebake(true);
        p.set_skew_enabled(true);
        p.tiles_mut().sync(&present);
        p.sync_sets(&["A".to_string(), "B".to_string()]);
        p.set_set_enabled("B", false);
        let start = Instant::now();
        let selected = p.tiles.resolve(&present);
        p.set_skew_threshold_deg(25.0);
        p.sync_param_tracking(start, &selected);
        let fire = p
            .poll_autorebake(start + Duration::from_millis(500), &selected)
            .expect("skew fire is due");
        assert!(fire.skew);
        assert!(fire.maps.contains(&BakeSelection::Ao));
        assert!(!fire.maps.contains(&BakeSelection::Thickness));
        assert_eq!(fire.sets, vec!["A".to_string()]);

        // Disabled auto-rebake never fires, however stale the params.
        let mut p = panel();
        p.tiles_mut().sync(&present);
        let selected = p.tiles.resolve(&present);
        p.set_rays(32);
        p.sync_param_tracking(start, &selected);
        assert!(p
            .poll_autorebake(start + Duration::from_secs(60), &selected)
            .is_none());
    }

    #[test]
    fn landed_bake_feeds_the_log() {
        // A finished job appends one log entry per record (map, set,
        // duration, warnings) — the log survives beside the status line.
        let mut p = panel();
        let (release, work) = gated(Ok(vec![
            BakeRecord {
                selection: Some(BakeSelection::Ao),
                set: "Sword".to_string(),
                tile: 1001,
                path: Path::new("/tmp/umber-bakes-test").join("Sword_ambient_occlusion.png"),
                elapsed_ms: 7,
                skew_clamped: 2,
            },
            BakeRecord {
                selection: None,
                set: "Sword".to_string(),
                tile: 1001,
                path: Path::new("/tmp/umber-bakes-test").join("Sword_skew.png"),
                elapsed_ms: 3,
                skew_clamped: 0,
            },
        ]));
        assert!(p.start_job(2, Vec::new(), Vec::new(), work));
        release.send(()).unwrap();
        await_job(&mut p);
        assert_eq!(p.bake_log().len(), 2);
        let entries = p.bake_log().entries();
        assert_eq!(entries[0].selection, Some(BakeSelection::Ao));
        assert_eq!(entries[0].skew_clamped, 2);
        assert_eq!(entries[0].setting_hint(), BakeSettingId::SkewThreshold);
        assert_eq!(entries[1].selection, None);
        assert!(
            entries[1]
                .summary()
                .contains("Sword Skew (correction mask) (tile 1001): 3 ms"),
            "{}",
            entries[1].summary()
        );
    }
}
