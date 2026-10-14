//! `agent` — the AI interface hook (docs/specs/ai-hook-design.md): an
//! interactive JSONL session over the batch step vocabulary, for an
//! external agent (an AI assistant, a pipeline robot, a script) driving
//! umber without the GUI.
//!
//! Each stdin line is one step, written in the recipe's shape
//! (`{ "<step>": { fields } }`). The no-field steps also accept a bare
//! `"<step>"`, `{ "<step>": null }` or `{ "<step>": {} }`. Blank lines
//! are skipped. Each step gets exactly one stdout line in reply:
//! `{"ok": true, "step": "<name>", "index": i, ...result fields}` or
//! `{"ok": false, "step": "<name>", "index": i, "error": "..."}`, where
//! `i` counts the non-blank lines and `step` is `null` when the line
//! names no step. A failing step never ends the session. `quit` replies
//! and then exits 0; stdin EOF also exits 0, so a crashed harness never
//! leaves an orphaned process. Stdout carries only these JSON lines.
//! Logs and diagnostics go to stderr.
//!
//! The session holds one mesh (`load`). `state`, `bake` and `export`
//! work on that mesh. `inspect` and `bake_transfer` are the batch steps
//! unchanged and stay stateless.
//!
//! Control protocol v0 (project + the save seam): `project_open` holds a `.umber` project
//! beside the mesh (the mesh stays whatever `load` set — projects carry
//! no geometry), `project_state` censuses it (`null` when none is open),
//! `project_save` writes it back (with the paint sidecar when the session
//! painted or opened paint — `painted` counts the staged planes), `set_active`
//! chooses the working set, and `undo`/`redo` drive the model's per-set
//! undo journals. When a project is open, `bake` names its outputs after
//! the active set; `export` still names from the mesh (its naming lives
//! in main.rs's `export_mesh`, out of this surface — unifying it is
//! deferred). The session attaches the project's autosave journal on
//! open like the app does, so undo/redo record the edit they perform
//! and saves stamp the live high-water mark.
//!
//! Edit steps (queue row 14's follow-up): `layer_add`, `layer_remove`,
//! `layer_set_opacity`, `layer_reorder` and `layer_set_material` drive the
//! active set's stack through the same per-set `histories` + attached
//! autosave journal the undo/redo arms use (one `Document::run` chokepoint
//! each — see `Session::apply_edit`), so every edit is undoable and every
//! save stamps it. Unknown layer ids and a missing project are typed
//! errors; a failed step never ends the session.
//!
//! Paint steps (docs/specs/agent-paint-ops-design.md): `paint_stroke`,
//! `paint_read`, `paint_undo` and `paint_redo` drive the SAME headless
//! engine the GUI paints through (`umber_paint`), and `project_save` /
//! `project_open` write and read the paint sidecar through the SAME
//! persistence (`umber_paint::persistence`) — no second implementation.
//! The paint half (a GPU device + a `PaintState`) is created lazily on
//! the first paint step, or at open when the project has a `paint/`
//! sidecar; a session that never paints requests no device. Stroke undo
//! is the engine's own history, separate from the model's `undo` (the
//! app's two-stack shape). v1 boundaries: no fill/material evaluation
//! (their rows composite nothing here), no mask strokes, no pressure.
//!
//! Judgment calls (v1 boundaries): there is no `layer_rename` —
//! [`LayerCommand`] carries no rename op and the journal has no name-only
//! entry (the app itself never renames: names are fixed at add time), so a
//! wire rename could be neither undone nor recovered; it waits on a core
//! `Rename` command plus a journal entry. `layer_set_material` with absent
//! `channels` stores the sparse form (empty list): the definition wins at
//! eval time (declared-but-unlisted channels read as
//! [`MaterialChannel::named`] defaults), so the sparse form and the app's
//! eagerly-materialized list evaluate identically. `layer_add` with
//! `opacity` records two undo entries (add, then set-opacity): one wire
//! step but two undos. Replies carry the affected row plus `journal_seq` /
//! `can_undo` / `can_redo` (the undo reply's shape), never a full census.
//!
//! `bake` runs asynchronously on a session-owned thread, and the client
//! polls it with `bake_status`. The app's async bake worker (7bf6140)
//! lives in umber-app, which umber-cli does not depend on and should not
//! (that would pull egui into the headless binary). So this module has
//! its own minimal job: a `JoinHandle` plus a shared status. The thread
//! runs the same bake functions the batch `bake` step uses
//! ([`bake_context`], [`bake_mesh_maps`]). The panel's worker stays on
//! the app side.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use umber_core::{
    BlendMode, ChannelKind, Journal, JournalOp, Layer, LayerCommand, LayerKind, LayerStack,
    MaterialChannel, MaterialParams, ProjectModel, TextureSet, TileSet, UndoStack,
};

use umber_paint::paint_state::{CompositeSnapshot, PaintState, PlaneOwner};
use umber_paint::persistence::{
    list_paint_tile_names, load_paint_tiles_reported, paint_save_version, paint_set_sizes_path,
    paint_sidecar_dir, parse_paint_set_sizes, save_paint_tiles, PaintSetSizes,
};
use umber_paint::snapshot::build_composite_snapshot;

use crate::batch::{default_rays, default_size, BakeTransferStep, InspectStep};
use crate::{
    bake_context, bake_mesh_maps, export_mesh, mesh_summary, select_tiles, BakeFlags, BakeMap,
    MeshSummary, UDIM_GRID,
};

/// One session step. The batch's `inspect` / `bake_transfer` reuse the
/// recipe structs. `bake` / `export` drop the recipe's `mesh` field
/// because they act on the held mesh. The `project_*` / `set_active` /
/// `undo` / `redo` steps are the control-protocol v0 surface: they act
/// on the held project, never on the mesh. The `layer_*` steps are the
/// edit family (row-14 follow-up): they act on the active set's stack.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentStep {
    Load(LoadStep),
    State(NoFields),
    Inspect(InspectStep),
    Bake(AgentBakeStep),
    BakeStatus(NoFields),
    BakeTransfer(BakeTransferStep),
    Export(AgentExportStep),
    ProjectOpen(ProjectOpenStep),
    ProjectState(NoFields),
    ProjectSave(ProjectSaveStep),
    SetActive(SetActiveStep),
    LayerAdd(LayerAddStep),
    LayerRemove(LayerRemoveStep),
    LayerSetOpacity(LayerSetOpacityStep),
    LayerReorder(LayerReorderStep),
    LayerRename(LayerRenameStep),
    LayerSetMaterial(LayerSetMaterialStep),
    Undo(NoFields),
    Redo(NoFields),
    PaintStroke(PaintStrokeStep),
    PaintRead(PaintReadStep),
    PaintUndo(NoFields),
    PaintRedo(NoFields),
    Quit(NoFields),
}

/// The body of a step that takes no fields (`{}`; `null` and the bare
/// name are normalized to it by [`normalize_step`]).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoFields {}

/// `load` — load a mesh and hold it for the session.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadStep {
    mesh: PathBuf,
}

/// `bake` — the batch `bake` run on the held mesh. `maps` empty or
/// absent bakes every map. `tiles` empty or absent bakes the whole mesh
/// with the batch's untiled names. Otherwise each requested tile is
/// baked from its own triangles and named `<set>_<map>_<tile>.png`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentBakeStep {
    #[serde(default)]
    maps: Vec<BakeMap>,
    out_dir: PathBuf,
    #[serde(default = "default_size")]
    size: u32,
    #[serde(default = "default_rays")]
    rays: u32,
    #[serde(default)]
    dilate: u32,
    #[serde(default)]
    tiles: Vec<u16>,
}

/// `export` — the batch `export` run on the held mesh. `tiles` empty or
/// absent exports every present tile.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentExportStep {
    preset: String,
    out_dir: PathBuf,
    #[serde(default = "default_size")]
    size: u32,
    #[serde(default = "default_rays")]
    rays: u32,
    #[serde(default)]
    tiles: Vec<u16>,
    /// Also write `<textureSet>.usda` (the `--usda` flag's session form).
    #[serde(default)]
    usda: bool,
}

/// `project_open` — open a `.umber` project directory and hold it for
/// the session. The mesh is untouched (projects carry no geometry).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectOpenStep {
    dir: PathBuf,
}

/// `project_save` — save the held project to a directory (a new
/// directory is a Save As; the session follows it).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSaveStep {
    dir: PathBuf,
}

/// `set_active` — choose the session's working texture set (what
/// project-scoped steps and `bake` target).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetActiveStep {
    set: String,
}

/// `paint_stroke` — one stroke through the shared paint engine
/// (docs/specs/agent-paint-ops-design.md). `layer` absent paints Base.
/// `points` are UV (≥ 1; a point's integer part routes it to its UDIM
/// tile). `color` is straight RGBA in `0..=1`; `radius_scale` multiplies
/// the session brush's radius (the engine's per-stroke scale — v1 has no
/// absolute-radius seam).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaintStrokeStep {
    #[serde(default)]
    layer: Option<u64>,
    points: Vec<[f32; 2]>,
    color: [f32; 4],
    #[serde(default = "default_radius_scale")]
    radius_scale: f32,
}

/// The `paint_stroke` radius scale when absent: the session brush as-is.
fn default_radius_scale() -> f32 {
    1.0
}

/// `paint_read` — sample texels from one plane (`layer` present; Base is
/// `layer` absent with `composite: false`) or the stack composite
/// (`composite: true`, the default when `layer` is absent). Sampled texels
/// only (1..=[`PAINT_READ_MAX_TEXELS`]): a 4K plane is 64 MiB, which no
/// JSONL line should carry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaintReadStep {
    #[serde(default)]
    layer: Option<u64>,
    #[serde(default)]
    composite: Option<bool>,
    #[serde(default = "default_paint_tile")]
    tile: u16,
    #[serde(default = "default_paint_channel")]
    channel: String,
    texels: Vec<[u32; 2]>,
}

/// The `paint_read` tile when absent: the first UDIM tile.
fn default_paint_tile() -> u16 {
    umber_mesh::udim::FIRST_TILE
}

/// The `paint_read` channel when absent: Base Color.
fn default_paint_channel() -> String {
    umber_paint::paint_state::BASE_COLOR_CHANNEL.to_string()
}

/// The `paint_read` sample cap (see [`PaintReadStep`]).
const PAINT_READ_MAX_TEXELS: usize = 64;

/// The layer kinds a wire `layer_add` may create: paint, fill or material.
/// Folders and instances exist in [`LayerKind`] but have no wire
/// constructor (a folder needs members the flat stack cannot name yet; an
/// instance needs a source the step does not take) — v1 boundary.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LayerAddKind {
    Paint,
    Fill,
    Material,
}

/// `layer_add` — push a layer onto the top of the active set's stack.
/// `opacity` (absent = the stack default 1.0) records a second undo entry
/// when present (see the module docs).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerAddStep {
    name: String,
    kind: LayerAddKind,
    #[serde(default)]
    opacity: Option<f32>,
}

/// `layer_remove` — remove one layer by id.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerRemoveStep {
    id: u64,
}

/// `layer_set_opacity` — set one layer's opacity (clamped `0..=1`;
/// non-finite input is a typed error, never stored).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerSetOpacityStep {
    id: u64,
    opacity: f32,
}

/// `layer_rename` — renames one layer (one undoable `Rename` op; the
/// name-only journal entry rides the core's `JournalOp::Rename`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerRenameStep {
    id: u64,
    name: String,
}

/// `layer_reorder` — move one layer to stack position `to` (index 0 =
/// bottom; past-the-end clamps onto the top — the [`LayerStack::reorder`]
/// rule, with the landed index in the reply).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerReorderStep {
    id: u64,
    to: usize,
}

/// One channel entry of a wire `layer_set_material`: the per-layer
/// controls for one channel. Absent fields read as the
/// [`MaterialChannel::named`] defaults (Normal / 1.0 / enabled); opacity
/// clamps `0..=1` with non-finite mapping to `0.0` (the app panel's
/// `upsert_channel` rule).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialChannelStep {
    name: String,
    #[serde(default)]
    blend: BlendMode,
    #[serde(default = "default_material_channel_opacity")]
    opacity: f32,
    #[serde(default = "default_material_channel_enabled")]
    enabled: bool,
}

/// `layer_set_material` — re-point one layer at `material_id` with a full
/// channel list. `channels` absent stores the sparse form (see the module
/// docs); present replaces the whole list, exactly like the app panel's
/// per-widget `SetMaterial` ops. An unknown definition id still points
/// (the app badges it `MissingDefinition` — never a refused edit).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerSetMaterialStep {
    id: u64,
    material_id: String,
    #[serde(default)]
    channels: Option<Vec<MaterialChannelStep>>,
}

/// The [`MaterialChannel::named`] opacity default for a wire channel entry
/// that omits it.
fn default_material_channel_opacity() -> f32 {
    1.0
}

/// The [`MaterialChannel::named`] enabled default for a wire channel entry
/// that omits it (serde's bool default is `false` — wrong here).
fn default_material_channel_enabled() -> bool {
    true
}

/// The async bake's state, as `bake_status` reports it
/// (`{"status": "pending" | "running" | "done" | "failed", ...}`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum BakeStatus {
    /// The thread is spawned but has not started baking yet.
    Pending,
    Running,
    Done {
        texture_set: String,
        written: Vec<PathBuf>,
    },
    Failed {
        error: String,
    },
}

impl BakeStatus {
    fn is_finished(&self) -> bool {
        matches!(self, BakeStatus::Done { .. } | BakeStatus::Failed { .. })
    }
}

/// The session's bake job: the worker thread and the status it reports.
struct BakeJob {
    status: Arc<Mutex<BakeStatus>>,
    /// Taken (joined) once the thread has finished.
    handle: Option<JoinHandle<()>>,
}

/// Locks the status. A panicking bake thread never holds the lock while
/// baking, but a poisoned lock still carries the last status written.
fn lock(status: &Mutex<BakeStatus>) -> MutexGuard<'_, BakeStatus> {
    status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl BakeJob {
    /// The current status. Once the thread has finished it is joined
    /// here. A thread that ended without writing a final status
    /// (a panic) is reported as `failed`, never left `running`.
    fn poll(&mut self) -> BakeStatus {
        if self.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            let joined = self.handle.take().expect("checked above").join();
            let mut status = lock(&self.status);
            if !status.is_finished() {
                *status = BakeStatus::Failed {
                    error: match joined {
                        Err(_) => "the bake thread panicked".to_string(),
                        Ok(()) => "the bake thread ended without a result".to_string(),
                    },
                };
            }
        }
        lock(&self.status).clone()
    }
}

/// The held mesh: its path (which names the texture set and the
/// `$mesh` token) and its summary.
struct Loaded {
    path: PathBuf,
    mesh: Arc<umber_mesh::MeshData>,
    summary: MeshSummary,
}

/// Undo depth per texture set (entries). Mirrors the app `Document`'s
/// cap (document.rs): one journal per set, so switching the active set
/// never replays one stack's commands against another's (the stale-
/// journal corruption `Document::load_stack` guards against).
const HISTORY_MAX_ENTRIES: usize = 256;

/// The held project: the document model plus the session state the app
/// keeps beside it (the open directory, the working set, the per-set
/// undo journals, the attached autosave journal).
struct ProjectSession {
    model: ProjectModel,
    dir: PathBuf,
    active: String,
    histories: BTreeMap<String, UndoStack<LayerCommand>>,
    journal: Option<Journal>,
    /// The paint half (agent paint ops): created on the first paint step,
    /// or at `project_open` when the project has a `paint/` sidecar. A
    /// session that never paints requests no GPU device.
    paint: Option<PaintState>,
    /// The last snapshot the paint half flushed against: the shell's
    /// `prev_snapshot`. The flush does NOT diff snapshots itself — the
    /// caller marks changed owners `Full` (`CompositeSnapshot::full_owners`)
    /// and retires removed layers, exactly as the app's frame does, or a
    /// wire `layer_set_opacity` would leave a stale composite cache.
    prev_snapshot: Option<CompositeSnapshot>,
}

/// A session's state between steps.
#[derive(Default)]
struct Session {
    loaded: Option<Loaded>,
    bake: Option<BakeJob>,
    project: Option<ProjectSession>,
}

/// One channel in the project census: the model's channel plus its
/// paint size (the sidecar override, else the set size, else the model
/// resolution — the app's absent-means-inherit rule).
#[derive(Debug, Serialize)]
struct ChannelCensus {
    name: String,
    kind: ChannelKind,
    width: u32,
    height: u32,
}

/// One layer in the project census: the stack summary (names, kinds,
/// opacity, blend — plus the id and visibility a driver needs to name
/// a layer back). Mask presence is deliberately not carried (v1
/// boundary: the census is a stack summary, not a mask inventory).
#[derive(Debug, Serialize)]
struct LayerCensus {
    id: u64,
    name: String,
    kind: LayerKind,
    opacity: f32,
    visible: bool,
    blend: BlendMode,
}

/// One texture set in the project census.
#[derive(Debug, Serialize)]
struct TextureSetCensus {
    name: String,
    resolution: u32,
    channels: Vec<ChannelCensus>,
    tiles: Vec<u16>,
    layers: Vec<LayerCensus>,
}

/// The `project_open` reply body. The layer detail is shared with
/// `project_state` (one builder), so the two can never disagree.
#[derive(Debug, Serialize)]
struct ProjectCensus {
    version: u32,
    active_set: String,
    journal_seq: u64,
    texture_sets: Vec<TextureSetCensus>,
}

/// The `project_state` reply body when a project is open: where it
/// lives plus the census.
#[derive(Debug, Serialize)]
struct ProjectStateBody {
    dir: PathBuf,
    version: u32,
    active_set: String,
    journal_seq: u64,
    texture_sets: Vec<TextureSetCensus>,
}

/// What every edit reply shares: the live journal mark plus the active
/// set's undo capabilities (the undo/redo reply's shape).
struct EditReport {
    journal_seq: u64,
    can_undo: bool,
    can_redo: bool,
}

/// The active set's stack, shared, or the honest error. The edit steps
/// pre/post-read through this; [`Session::apply_edit`] takes the `&mut`
/// half for the push.
fn active_stack(held: &ProjectSession) -> Result<&LayerStack> {
    held.model
        .layers
        .iter()
        .find(|e| e.texture_set == held.active)
        .map(|e| &e.stack)
        .ok_or_else(|| {
            let active = held.active.clone();
            anyhow::anyhow!("texture set {active:?} has no layers entry")
        })
}

/// Reads the opened project's `paint/target_sizes.txt` through the
/// shared persistence codec (`umber_paint::persistence` — the parser the
/// app's open path uses, so the census can never drift from what the GUI
/// restores). A missing, unreadable or empty sidecar is an empty mapping
/// (sizes fall back to the model), never an error.
fn read_sidecar_sizes(dir: &Path) -> PaintSetSizes {
    std::fs::read_to_string(paint_set_sizes_path(dir))
        .ok()
        .and_then(|text| parse_paint_set_sizes(&text))
        .unwrap_or_default()
}

/// A paint-capable headless device: the paint compositor's read-write
/// storage textures need `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, which
/// the bake device does not request (the app injects the same feature
/// into eframe's device descriptor). An adapter without it is a typed
/// error, never a panic at first splat.
fn paint_context() -> Result<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| anyhow::anyhow!("no suitable GPU adapter: {e}"))?;
    let needed = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    if !adapter.features().contains(needed) {
        let name = adapter.get_info().name;
        return Err(anyhow::anyhow!(
            "adapter {name:?} lacks read-write storage textures (paint needs them)"
        ));
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: needed,
        ..Default::default()
    }))?;
    Ok((device, queue))
}

/// The working set a freshly opened project starts on: the saved
/// active set when it names a set the project actually holds (mirrors
/// the app's `open_project` fallback to the first set), resolved
/// against the layers so a dangling saved name fails here instead of
/// censusing a set with no stack.
fn resolve_active(model: &ProjectModel) -> Result<String> {
    let first = model
        .texture_sets
        .first()
        .ok_or_else(|| anyhow::anyhow!("project has no texture sets"))?;
    let name = match &model.settings.active_texture_set {
        Some(name) => {
            if !model.texture_sets.iter().any(|ts| ts.name == *name) {
                return Err(anyhow::anyhow!(
                    "project's active texture set {name:?} is unknown"
                ));
            }
            name.clone()
        }
        None => first.name.clone(),
    };
    if !model.layers.iter().any(|e| e.texture_set == name) {
        return Err(anyhow::anyhow!("texture set {name:?} has no layers entry"));
    }
    Ok(name)
}

/// One layer's census row. Shared by [`texture_set_census`] and the edit
/// replies, so the two can never disagree.
fn layer_census_row(layer: &Layer) -> LayerCensus {
    LayerCensus {
        id: layer.id,
        name: layer.name.clone(),
        kind: layer.kind.clone(),
        opacity: layer.opacity,
        visible: layer.visible,
        blend: layer.blend_mode,
    }
}

/// One set's census: the model channels sized through the sidecar
/// mapping (override, else set size, else the model resolution), the
/// present tiles (an empty tile map means the implied 1001 — the
/// `TileSet` conversion's rule), and the stack summary (no layers
/// entry reads as an empty stack — lenient here; the ACTIVE set's
/// entry is required at open by [`resolve_active`]).
fn texture_set_census(
    set: &TextureSet,
    stack: Option<&LayerStack>,
    sizes: &PaintSetSizes,
) -> TextureSetCensus {
    let mut channels = Vec::with_capacity(set.channels.len());
    for channel in &set.channels {
        let mut size = (set.resolution, set.resolution);
        if let Some(mapped) = sizes.sizes.get(&set.name) {
            size = *mapped;
        }
        if let Some(over) = sizes
            .channels
            .get(&(set.name.clone(), channel.name.clone()))
        {
            size = *over;
        }
        channels.push(ChannelCensus {
            name: channel.name.clone(),
            kind: channel.kind,
            width: size.0,
            height: size.1,
        });
    }
    let layers: Vec<LayerCensus> = match stack {
        Some(stack) => stack.layers.iter().map(layer_census_row).collect(),
        None => Vec::new(),
    };
    TextureSetCensus {
        name: set.name.clone(),
        resolution: set.resolution,
        channels,
        tiles: TileSet::from(set).tiles_present(),
        layers,
    }
}

impl ProjectSession {
    /// The active set's live layer ids (the persistence `live` filter:
    /// orphan planes never stage, masks prune against it).
    fn live_ids(&self) -> std::collections::HashSet<u64> {
        self.model
            .layers
            .iter()
            .find(|e| e.texture_set == self.active)
            .map(|e| e.stack.layers.iter().map(|layer| layer.id).collect())
            .unwrap_or_default()
    }

    /// The paint half, created on first use: a paint-capable device (see
    /// [`paint_context`]) and a session on the active set, sized to the
    /// set's `resolution` (the sidecar's own sizes win on a later load —
    /// `load_paint_tiles_reported` resizes from `target_sizes.txt`).
    fn paint_mut(&mut self) -> Result<&mut PaintState> {
        if self.paint.is_none() {
            let size = self
                .model
                .texture_sets
                .iter()
                .find(|ts| ts.name == self.active)
                .map(|ts| ts.resolution)
                .unwrap_or(umber_paint::paint_state::DEFAULT_TARGET_SIZE);
            let (device, queue) = paint_context()?;
            let mut paint = PaintState::with_target_size(device, queue, size)
                .map_err(|e| anyhow::anyhow!("paint session: {e}"))?;
            paint.set_active_set(&self.active);
            self.paint = Some(paint);
        }
        self.paint
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("internal: paint session missing after creation"))
    }

    /// The composite rows for the active set (the shell's builder,
    /// `umber_paint::snapshot`): no material states (the CLI evaluates no
    /// materials, so their rows contribute nothing), Base visible, nothing
    /// displayed (every composite happens on read, flush-first).
    fn snapshot(&self) -> Result<CompositeSnapshot> {
        let stack = self
            .model
            .layers
            .iter()
            .find(|e| e.texture_set == self.active)
            .map(|e| &e.stack)
            .ok_or_else(|| {
                let active = &self.active;
                anyhow::anyhow!("texture set {active:?} has no layers entry")
            })?;
        let states = umber_paint::material_layer::MaterialLayerStates::default();
        Ok(build_composite_snapshot(stack, &states, true, Vec::new()))
    }

    /// The end-of-step composite (the app's end-of-update flush, one
    /// step = one frame): builds the current snapshot, marks every owner
    /// whose row changed since the last flush `Full` and retires removed
    /// layers (the app's diff — `full_owners` / `removed`), then flushes.
    /// No-op when the session never painted (nothing to composite).
    fn flush_paint(&mut self) -> Result<()> {
        let snapshot = self.snapshot()?;
        let Some(paint) = self.paint.as_mut() else {
            return Ok(());
        };
        if let Some(prev) = self.prev_snapshot.as_ref() {
            for owner in CompositeSnapshot::full_owners(prev, &snapshot) {
                paint.mark_owner_full(owner);
            }
            paint.note_removed_layers(&CompositeSnapshot::removed(prev, &snapshot));
        }
        paint.flush_composite(&snapshot);
        self.prev_snapshot = Some(snapshot);
        Ok(())
    }

    /// The census sequence number: the saved mark advanced by any
    /// journaled edits since (mirrors `Document::journal_seq` — the
    /// attached journal's last seq, never below the stamped mark).
    fn journal_seq(&self) -> u64 {
        let live = self.journal.as_ref().map(Journal::last_seq).unwrap_or(0);
        self.model.journal_seq.max(live)
    }

    /// The full census every project reply shares.
    fn census(&self) -> ProjectCensus {
        let sizes = read_sidecar_sizes(&self.dir);
        let mut texture_sets = Vec::with_capacity(self.model.texture_sets.len());
        for set in &self.model.texture_sets {
            let stack = self
                .model
                .layers
                .iter()
                .find(|e| e.texture_set == set.name)
                .map(|e| &e.stack);
            texture_sets.push(texture_set_census(set, stack, &sizes));
        }
        ProjectCensus {
            version: self.model.version,
            active_set: self.active.clone(),
            journal_seq: self.journal_seq(),
            texture_sets,
        }
    }
}

/// A step's outcome: the result fields to merge into the reply.
type Fields = Map<String, Value>;

fn fields(value: impl Serialize) -> Result<Fields> {
    match serde_json::to_value(value)? {
        Value::Object(map) => Ok(map),
        other => Err(anyhow::anyhow!("internal: non-object result {other}")),
    }
}

/// `{"summary": ...}`, the reply to `load`, `state` and `inspect`.
fn summary_fields(summary: &MeshSummary) -> Result<Fields> {
    let mut map = Fields::new();
    map.insert("summary".into(), serde_json::to_value(summary)?);
    Ok(map)
}

/// The step name a line's JSON carries: the bare string, or the single
/// key of a one-key object.
fn step_name(value: &Value) -> Option<String> {
    match value {
        Value::String(name) => Some(name.clone()),
        Value::Object(obj) if obj.len() == 1 => obj.keys().next().cloned(),
        _ => None,
    }
}

/// Converts the shorthand forms of a no-field step to serde's struct
/// form: `"quit"` and `{"quit": null}` become `{"quit": {}}`. Any other
/// value is returned unchanged.
fn normalize_step(value: Value) -> Value {
    match value {
        Value::String(name) => {
            let mut obj = Map::new();
            obj.insert(name, Value::Object(Map::new()));
            Value::Object(obj)
        }
        Value::Object(mut obj) if obj.len() == 1 => {
            if let Some(body) = obj.values_mut().next() {
                if body.is_null() {
                    *body = Value::Object(Map::new());
                }
            }
            Value::Object(obj)
        }
        other => other,
    }
}

impl Session {
    /// The held mesh, or the honest error.
    fn loaded(&self) -> Result<&Loaded> {
        self.loaded
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no mesh loaded (send a `load` step first)"))
    }

    /// The held project, mutably, or the honest error.
    fn project_mut(&mut self) -> Result<&mut ProjectSession> {
        self.project
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no project open (send a `project_open` step first)"))
    }

    /// The held project, shared, or the honest error (the pre/post-read
    /// half of the edit steps — [`Session::project_mut`] is the push half).
    fn project(&self) -> Result<&ProjectSession> {
        self.project
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no project open (send a `project_open` step first)"))
    }

    /// Runs one step and returns its result fields. `Quit` is handled by
    /// the caller ([`Session::reply`]).
    fn run(&mut self, step: AgentStep) -> Result<Fields> {
        match step {
            AgentStep::Load(s) => {
                let mesh = umber_mesh::load(&s.mesh)?;
                let summary = mesh_summary(&s.mesh, &mesh);
                let reply = summary_fields(&summary)?;
                self.loaded = Some(Loaded {
                    path: s.mesh,
                    mesh: Arc::new(mesh),
                    summary,
                });
                Ok(reply)
            }
            AgentStep::State(NoFields {}) => summary_fields(&self.loaded()?.summary),
            AgentStep::Inspect(s) => {
                let mesh = umber_mesh::load(&s.mesh)?;
                summary_fields(&mesh_summary(&s.mesh, &mesh))
            }
            AgentStep::Bake(s) => self.start_bake(s),
            AgentStep::BakeStatus(NoFields {}) => {
                let job = self
                    .bake
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("no bake started (send `bake` first)"))?;
                fields(job.poll())
            }
            AgentStep::BakeTransfer(s) => {
                let (texture_set, written) = s.run()?;
                fields(serde_json::json!({ "texture_set": texture_set, "written": written }))
            }
            AgentStep::Export(s) => {
                for (j, tile) in s.tiles.iter().enumerate() {
                    if !UDIM_GRID.contains(tile) {
                        return Err(anyhow::anyhow!(
                            "tiles[{j}]: {tile} is outside the UDIM grid 1001..=1100"
                        ));
                    }
                }
                let loaded = self.loaded()?;
                let flags = BakeFlags {
                    size: s.size,
                    rays: s.rays,
                    dilate: 0,
                };
                let mut skipped = Vec::new();
                let ask = crate::ExportAsk {
                    preset_name: &s.preset,
                    flags: &flags,
                    requested_tiles: &s.tiles,
                    usda: s.usda,
                };
                let (written, tiles) = export_mesh(
                    &loaded.path,
                    &loaded.mesh,
                    &s.out_dir,
                    &ask,
                    &mut |name: &str| skipped.push(name.to_string()),
                )?;
                fields(serde_json::json!({
                    "written": written,
                    "tiles": tiles,
                    "skipped": skipped,
                }))
            }
            AgentStep::ProjectOpen(s) => self.project_open(s),
            AgentStep::ProjectState(NoFields {}) => self.project_state(),
            AgentStep::ProjectSave(s) => self.project_save(s),
            AgentStep::SetActive(s) => self.set_active(s),
            AgentStep::LayerAdd(s) => self.layer_add(s),
            AgentStep::LayerRemove(s) => self.layer_remove(s),
            AgentStep::LayerSetOpacity(s) => self.layer_set_opacity(s),
            AgentStep::LayerReorder(s) => self.layer_reorder(s),
            AgentStep::LayerRename(s) => self.layer_rename(s),
            AgentStep::LayerSetMaterial(s) => self.layer_set_material(s),
            AgentStep::Undo(NoFields {}) => self.project_undo(),
            AgentStep::Redo(NoFields {}) => self.project_redo(),
            AgentStep::PaintStroke(s) => self.paint_stroke(s),
            AgentStep::PaintRead(s) => self.paint_read(s),
            AgentStep::PaintUndo(NoFields {}) => self.paint_history(true),
            AgentStep::PaintRedo(NoFields {}) => self.paint_history(false),
            AgentStep::Quit(NoFields {}) => unreachable!("quit is answered by Session::reply"),
        }
    }

    /// `project_open`: loads the project (the typed failure verbatim —
    /// a bad directory is an error, never a partial session), resolves
    /// the working set, attaches the autosave journal the way the app's
    /// `open_project` does (a read-only directory degrades to no journal
    /// with a stderr note — the journal is an optimization, never a
    /// gate), and replaces any previously held project wholesale (with
    /// its undo journals — ids collide across documents, the
    /// `Document::load_stack` rule).
    fn project_open(&mut self, s: ProjectOpenStep) -> Result<Fields> {
        let model = umber_core::load_from_dir(&s.dir).map_err(|e| anyhow::anyhow!("{e}"))?;
        let active = resolve_active(&model)?;
        let mark = model.journal_seq;
        let journal = match Journal::open(&s.dir, mark) {
            Ok(journal) => Some(journal),
            Err(e) => {
                eprintln!("agent: autosave journal unavailable: {e}");
                None
            }
        };
        let mut session = ProjectSession {
            model,
            dir: s.dir,
            active,
            histories: BTreeMap::new(),
            journal,
            paint: None,
            prev_snapshot: None,
        };
        // A project with a `paint/` sidecar restores its pixels now (the
        // app's `open_project` sequence), so `paint_read` after open sees
        // what was saved. No sidecar -> no GPU device is requested.
        let mut painted = 0usize;
        // Trigger on real pixels, not on the directory: a sizes-only
        // `paint/` (target_sizes.txt, no PNGs) restores nothing, so it
        // must not cost a GPU device (the census reads the sizes itself).
        let has_paint = list_paint_tile_names(&paint_sidecar_dir(&session.dir))
            .map(|names| !names.is_empty())
            .unwrap_or(false);
        if has_paint {
            let live = session.live_ids();
            let dir = session.dir.clone();
            let paint = session.paint_mut()?;
            paint.clear_layer_planes();
            let report = load_paint_tiles_reported(paint, &dir, &live);
            paint.prune_layer_masks(&live);
            painted = report.restored;
            // Seed the diff baseline with the opened stack (the app's
            // first frame after open).
            session.flush_paint()?;
        }
        let census = session.census();
        self.project = Some(session);
        let mut reply = fields(census)?;
        reply.insert("painted".to_string(), Value::from(painted));
        Ok(reply)
    }

    /// `project_state`: the session's project census. An empty session
    /// is a state, not an error (`project: null` — the MCP convention).
    fn project_state(&self) -> Result<Fields> {
        match self.project.as_ref() {
            Some(held) => {
                let census = held.census();
                let body = ProjectStateBody {
                    dir: held.dir.clone(),
                    version: census.version,
                    active_set: census.active_set,
                    journal_seq: census.journal_seq,
                    texture_sets: census.texture_sets,
                };
                fields(serde_json::json!({ "project": body }))
            }
            None => fields(serde_json::json!({ "project": null })),
        }
    }

    /// `project_save`: stamps the live journal mark and the working set
    /// into the model (the app's `save_project` rule — every entry
    /// recorded so far is in this saved state), writes it, then follows
    /// a Save As: the session now lives at the new directory with a
    /// freshly attached journal, and the old directory's journal is
    /// truncated (the `Document::attach_journal` rule — its entries are
    /// in the state just saved elsewhere). `painted: 0`: the CLI holds
    /// no paint tiles, so the save is `project.json` + layers only (an
    /// existing sidecar under the target directory is left intact; a
    /// fresh directory simply has none).
    fn project_save(&mut self, s: ProjectSaveStep) -> Result<Fields> {
        let held = self.project_mut()?;
        let live = held.journal.as_ref().map(Journal::last_seq).unwrap_or(0);
        held.model.journal_seq = held.model.journal_seq.max(live);
        held.model.settings.active_texture_set = Some(held.active.clone());
        // Paint first decides the format version (§6.3: v2 iff a live
        // layer owns a plane) — the app's `save_project` order.
        let live_ids = held.live_ids();
        if let Some(paint) = held.paint.as_ref() {
            held.model.version = paint_save_version(paint, &live_ids);
        }
        umber_core::save_to_dir(&held.model, &s.dir).map_err(|e| anyhow::anyhow!("{e}"))?;
        // The sidecar rides after the metadata commit (the directory now
        // exists); the paint commit is atomic on its own. Dormancy is the
        // GUI's (the CLI evaluates no fills/materials), so no layer is
        // listed dormant: a reopen in the app re-derives from the header.
        let painted = match held.paint.as_mut() {
            Some(paint) => {
                paint.prune_layer_masks(&live_ids);
                save_paint_tiles(paint, &s.dir, &live_ids, &[])
            }
            None => 0,
        };
        let journal_seq = held.model.journal_seq;
        let old_dir = held.dir.clone();
        if s.dir != old_dir {
            if let Err(e) = umber_core::journal::truncate_journal(&old_dir) {
                eprintln!("agent: previous autosave journal not cleared: {e}");
            }
        }
        held.journal = match Journal::open(&s.dir, journal_seq) {
            Ok(journal) => Some(journal),
            Err(e) => {
                eprintln!("agent: autosave journal unavailable: {e}");
                None
            }
        };
        held.dir = s.dir.clone();
        fields(serde_json::json!({
            "dir": s.dir,
            "painted": painted,
            "journal_seq": journal_seq,
        }))
    }

    /// `set_active`: chooses the working set. Unknown names are a typed
    /// error carrying the project's set list (nothing is switched).
    fn set_active(&mut self, s: SetActiveStep) -> Result<Fields> {
        let held = self.project_mut()?;
        if !held.model.texture_sets.iter().any(|ts| ts.name == s.set) {
            let known: Vec<&str> = held
                .model
                .texture_sets
                .iter()
                .map(|ts| ts.name.as_str())
                .collect();
            return Err(anyhow::anyhow!(
                "unknown texture set {:?} (project has: {})",
                s.set,
                known.join(", ")
            ));
        }
        held.active = s.set.clone();
        fields(serde_json::json!({ "active_set": s.set }))
    }

    /// Runs one model edit through the active set's undo journal and the
    /// attached autosave journal — the session's `Document::run`
    /// chokepoint (see the app's `document.rs`): push through the per-set
    /// history, then journal the applied op (read after the push); a
    /// failed append is a stderr note, never fatal. Callers pre-validate
    /// (unknown ids, non-finite input) so a pushed command always lands on
    /// a live layer — a no-op push would still occupy undo depth while
    /// journaling nothing. Field borrows split (`model` / `histories` /
    /// `journal`) so no `&mut self` method runs under a borrow (E0502).
    fn apply_edit(&mut self, command: LayerCommand) -> Result<EditReport> {
        let held = self.project_mut()?;
        let active = held.active.clone();
        let stack = held
            .model
            .layers
            .iter_mut()
            .find(|e| e.texture_set == active)
            .map(|e| &mut e.stack)
            .ok_or_else(|| anyhow::anyhow!("texture set {active:?} has no layers entry"))?;
        let history = held
            .histories
            .entry(active.clone())
            .or_insert_with(|| UndoStack::new(HISTORY_MAX_ENTRIES));
        history.push(command, stack);
        let op = history
            .peek_undo()
            .and_then(|c| JournalOp::applied(c, stack));
        if let (Some(journal), Some(op)) = (held.journal.as_mut(), op) {
            if let Err(e) = journal.record(&active, op) {
                eprintln!("agent: autosave journal append failed: {e}");
            }
        }
        // Inline `ProjectSession::journal_seq` (not the method): the
        // method borrows all of `held` while `history` is still alive.
        let live = held.journal.as_ref().map(Journal::last_seq).unwrap_or(0);
        let journal_seq = held.model.journal_seq.max(live);
        let (can_undo, can_redo) = (history.can_undo(), history.can_redo());
        Ok(EditReport {
            journal_seq,
            can_undo,
            can_redo,
        })
    }

    /// `layer_add`: pushes an add through [`Session::apply_edit`] (the new
    /// id is the stack's next counter — captured before the push, verified
    /// after), then an optional opacity as its own undo entry. The reply
    /// carries the new row plus the post-add count (the census delta).
    fn layer_add(&mut self, s: LayerAddStep) -> Result<Fields> {
        if let Some(value) = s.opacity {
            if !value.is_finite() {
                return Err(anyhow::anyhow!(
                    "opacity {value} is not finite (expected 0..=1)"
                ));
            }
        }
        let kind = match s.kind {
            LayerAddKind::Paint => LayerKind::Paint,
            LayerAddKind::Fill => LayerKind::Fill,
            LayerAddKind::Material => LayerKind::Material,
        };
        let id = active_stack(self.project()?)?.next_layer_id();
        let mut report = self.apply_edit(LayerCommand::add(s.name, kind))?;
        if let Some(value) = s.opacity {
            report = self.apply_edit(LayerCommand::set_opacity(id, value))?;
        }
        let stack = active_stack(self.project()?)?;
        let row = stack
            .layer(id)
            .map(layer_census_row)
            .ok_or_else(|| anyhow::anyhow!("internal: added layer {id} missing"))?;
        let layers = stack.layers.len();
        fields(serde_json::json!({
            "id": id,
            "layer": row,
            "layers": layers,
            "journal_seq": report.journal_seq,
            "can_undo": report.can_undo,
            "can_redo": report.can_redo,
        }))
    }

    /// `layer_remove`: the remove path. The reply names the removed row
    /// (id, name, former index) plus the post-remove count.
    fn layer_remove(&mut self, s: LayerRemoveStep) -> Result<Fields> {
        let id = s.id;
        let (name, index) = {
            let stack = active_stack(self.project()?)?;
            let index = stack
                .index_of(id)
                .ok_or_else(|| anyhow::anyhow!("unknown layer {id}"))?;
            (stack.layers[index].name.clone(), index)
        };
        let report = self.apply_edit(LayerCommand::remove(id))?;
        let layers = active_stack(self.project()?)?.layers.len();
        fields(serde_json::json!({
            "id": id,
            "name": name,
            "index": index,
            "layers": layers,
            "journal_seq": report.journal_seq,
            "can_undo": report.can_undo,
            "can_redo": report.can_redo,
        }))
    }

    /// `layer_set_opacity`: clamped `0..=1` through the command path (the
    /// stack clamps; the reply carries what it actually holds). Non-finite
    /// input is a typed error — the stack would silently ignore it, which
    /// would journal nothing while occupying undo depth.
    fn layer_set_opacity(&mut self, s: LayerSetOpacityStep) -> Result<Fields> {
        let (id, value) = (s.id, s.opacity);
        if !value.is_finite() {
            return Err(anyhow::anyhow!(
                "opacity {value} is not finite (expected 0..=1)"
            ));
        }
        if active_stack(self.project()?)?.layer(id).is_none() {
            return Err(anyhow::anyhow!("unknown layer {id}"));
        }
        let report = self.apply_edit(LayerCommand::set_opacity(id, value))?;
        let stored = active_stack(self.project()?)?
            .layer(id)
            .map(|layer| layer.opacity)
            .ok_or_else(|| anyhow::anyhow!("internal: layer {id} missing after set_opacity"))?;
        fields(serde_json::json!({
            "id": id,
            "opacity": stored,
            "journal_seq": report.journal_seq,
            "can_undo": report.can_undo,
            "can_redo": report.can_redo,
        }))
    }

    /// `layer_reorder`: resolves the id to its current index, then runs
    /// the index-based reorder command (index 0 = bottom). The reply
    /// carries where the layer actually landed (past-the-end clamps).
    fn layer_reorder(&mut self, s: LayerReorderStep) -> Result<Fields> {
        let (id, to) = (s.id, s.to);
        let from = active_stack(self.project()?)?
            .index_of(id)
            .ok_or_else(|| anyhow::anyhow!("unknown layer {id}"))?;
        let report = self.apply_edit(LayerCommand::reorder(from, to))?;
        let landed = active_stack(self.project()?)?
            .index_of(id)
            .ok_or_else(|| anyhow::anyhow!("internal: layer {id} missing after reorder"))?;
        fields(serde_json::json!({
            "id": id,
            "from": from,
            "to": landed,
            "journal_seq": report.journal_seq,
            "can_undo": report.can_undo,
            "can_redo": report.can_redo,
        }))
    }

    /// `layer_rename`: renames one layer (one undoable `Rename` op).
    /// An empty name is rejected (the stack has no unnamed layers);
    /// the reply carries the stored name verbatim.
    fn layer_rename(&mut self, s: LayerRenameStep) -> Result<Fields> {
        let (id, name) = (s.id, s.name);
        if name.trim().is_empty() {
            anyhow::bail!("layer_rename: name must not be empty");
        }
        let report = self.apply_edit(LayerCommand::rename(id, name))?;
        let stored = active_stack(self.project()?)?
            .layer(id)
            .map(|l| l.name.clone())
            .ok_or_else(|| anyhow::anyhow!("internal: layer {id} missing after rename"))?;
        fields(serde_json::json!({
            "id": id,
            "name": stored,
            "journal_seq": report.journal_seq,
            "can_undo": report.can_undo,
            "can_redo": report.can_redo,
        }))
    }

    /// `layer_set_material`: re-points one layer (one undoable
    /// `SetMaterial` op). An absent list stores the sparse form; a present
    /// one replaces the whole list (see the module docs).
    fn layer_set_material(&mut self, s: LayerSetMaterialStep) -> Result<Fields> {
        let id = s.id;
        if active_stack(self.project()?)?.layer(id).is_none() {
            return Err(anyhow::anyhow!("unknown layer {id}"));
        }
        let channels = s
            .channels
            .unwrap_or_default()
            .into_iter()
            .map(|c| MaterialChannel {
                name: c.name,
                blend: c.blend,
                opacity: if c.opacity.is_finite() {
                    c.opacity.clamp(0.0, 1.0)
                } else {
                    0.0
                },
                enabled: c.enabled,
            })
            .collect::<Vec<_>>();
        let report = self.apply_edit(LayerCommand::set_material(
            id,
            MaterialParams {
                material_id: s.material_id,
                channels,
                params: Vec::new(),
            },
        ))?;
        let stored = active_stack(self.project()?)?
            .layer(id)
            .and_then(|layer| layer.material.clone())
            .ok_or_else(|| anyhow::anyhow!("internal: layer {id} missing after set_material"))?;
        fields(serde_json::json!({
            "id": id,
            "material_id": stored.material_id,
            "channels": stored.channels,
            "journal_seq": report.journal_seq,
            "can_undo": report.can_undo,
            "can_redo": report.can_redo,
        }))
    }

    /// `undo`: reverts the active set's most recent journaled edit
    /// (model-level — the CLI holds the model). An empty journal
    /// reports `undone: false`, never an error (absence is state). The
    /// reverting edit is journaled first, exactly like the app's
    /// `Document::undo` (revert consumes the command's captured state,
    /// so the op must be read before it). Field borrows split
    /// (`model` / `histories` / `journal`) so no `&mut self` method
    /// runs under a borrow (E0502).
    fn project_undo(&mut self) -> Result<Fields> {
        let held = self.project_mut()?;
        let active = held.active.clone();
        let stack = held
            .model
            .layers
            .iter_mut()
            .find(|e| e.texture_set == active)
            .map(|e| &mut e.stack);
        let Some(stack) = stack else {
            return Err(anyhow::anyhow!(
                "texture set {active:?} has no layers entry"
            ));
        };
        let history = held
            .histories
            .entry(active.clone())
            .or_insert_with(|| UndoStack::new(HISTORY_MAX_ENTRIES));
        let op = match history.peek_undo() {
            Some(command) => JournalOp::reverting(command, stack),
            None => None,
        };
        let undone = history.undo(stack);
        if undone {
            if let (Some(journal), Some(op)) = (held.journal.as_mut(), op) {
                if let Err(e) = journal.record(&active, op) {
                    eprintln!("agent: autosave journal append failed: {e}");
                }
            }
        }
        // Inline `ProjectSession::journal_seq` (not the method): the
        // method borrows all of `held` while `history` is still alive.
        let live = held.journal.as_ref().map(Journal::last_seq).unwrap_or(0);
        let journal_seq = held.model.journal_seq.max(live);
        let (can_undo, can_redo) = (history.can_undo(), history.can_redo());
        fields(serde_json::json!({
            "undone": undone,
            "journal_seq": journal_seq,
            "can_undo": can_undo,
            "can_redo": can_redo,
        }))
    }

    /// `paint_stroke`: one stroke through the shared engine on the active
    /// set (docs/specs/agent-paint-ops-design.md). The target is set
    /// EXPLICITLY with the stack's verdict, so a Fill/Material/Folder/
    /// Instance layer refuses typed (never silently paints Base); the
    /// stroke runs, the queue drains, and the end-of-step flush
    /// composites against the session's snapshot.
    fn paint_stroke(&mut self, s: PaintStrokeStep) -> Result<Fields> {
        if s.points.is_empty() {
            return Err(anyhow::anyhow!(
                "paint_stroke: points must hold at least one [u, v]"
            ));
        }
        for (j, p) in s.points.iter().enumerate() {
            if !p[0].is_finite() || !p[1].is_finite() {
                return Err(anyhow::anyhow!("points[{j}]: {p:?} is not finite"));
            }
        }
        if s.color
            .iter()
            .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
        {
            let color = s.color;
            return Err(anyhow::anyhow!(
                "color {color:?}: each channel must be in 0..=1"
            ));
        }
        if !s.radius_scale.is_finite() || s.radius_scale <= 0.0 {
            let scale = s.radius_scale;
            return Err(anyhow::anyhow!(
                "radius_scale {scale} must be finite and > 0"
            ));
        }
        let held = self.project_mut()?;
        let (target, verdict, index) = {
            let stack = active_stack(held)?;
            match s.layer {
                None => (PlaneOwner::Base, Ok(()), 0),
                Some(id) => {
                    if stack.layer(id).is_none() {
                        return Err(anyhow::anyhow!("no layer {id} in the active set"));
                    }
                    let index = stack.index_of(id).unwrap_or(0);
                    (PlaneOwner::Layer(id), stack.check_paintable(id), index)
                }
            }
        };
        // The first flush seeds the diff baseline before the stroke (so
        // this stroke's dirt is the only thing the end-of-step flush
        // composites on a fresh session).
        if held.paint.is_none() {
            held.paint_mut()?;
            held.flush_paint()?;
        }
        let paint = held.paint_mut()?;
        paint.sync_active_target(target, verdict, index);
        paint.set_active_target(target);
        // Straight color in, premultiplied brush color out (the brush
        // param is premultiplied — `BrushParams::color`).
        let [r, g, b, a] = s.color;
        paint.set_brush_color([r * a, g * a, b * a, a]);
        let first = s.points[0];
        paint.begin_stroke_styled(emath::pos2(first[0], first[1]), 1.0, s.radius_scale);
        if let Some(refusal) = paint.stroke_refusal() {
            return Err(anyhow::anyhow!("{refusal}"));
        }
        for p in &s.points[1..] {
            paint.extend_stroke_styled(emath::pos2(p[0], p[1]), 1.0, s.radius_scale);
        }
        paint.end_stroke();
        paint
            .process_pending()
            .map_err(|e| anyhow::anyhow!("paint drain failed: {e:#}"))?;
        let (can_undo, can_redo) = (paint.can_undo_stroke(), paint.can_redo_stroke());
        held.flush_paint()?;
        let tiles: std::collections::BTreeSet<u16> = s
            .points
            .iter()
            .map(|p| umber_mesh::tile_of_uv(*p))
            .collect();
        fields(serde_json::json!({
            "layer": s.layer,
            "tiles": tiles,
            "can_undo_stroke": can_undo,
            "can_redo_stroke": can_redo,
        }))
    }

    /// `paint_read`: samples texels from one plane or from the composite
    /// (see [`PaintReadStep`]). Out-of-range texels are a typed error
    /// naming the plane's size; a plane nothing ever wrote reads as
    /// absent (`exists: false`, no texels) — never invented zeros.
    fn paint_read(&mut self, s: PaintReadStep) -> Result<Fields> {
        if s.texels.is_empty() || s.texels.len() > PAINT_READ_MAX_TEXELS {
            let n = s.texels.len();
            return Err(anyhow::anyhow!(
                "paint_read: texels must hold 1..={PAINT_READ_MAX_TEXELS} [x, y] pairs (got {n})"
            ));
        }
        let composite = s.composite.unwrap_or(s.layer.is_none());
        if composite && s.layer.is_some() {
            return Err(anyhow::anyhow!(
                "paint_read: `composite: true` reads the whole stack — drop `layer`"
            ));
        }
        let held = self.project_mut()?;
        if let Some(id) = s.layer {
            if active_stack(held)?.layer(id).is_none() {
                return Err(anyhow::anyhow!("no layer {id} in the active set"));
            }
        }
        held.paint_mut()?;
        if composite {
            // The read composites the stack as it stands NOW: a wire
            // layer edit since the last step (opacity, order, removal)
            // diffs into `Full` dirt here, never a stale cache.
            held.flush_paint()?;
        }
        let paint = held.paint_mut()?;
        let image = if composite {
            paint.read_composite(s.tile, &s.channel)
        } else {
            let owner = s.layer.map_or(PlaneOwner::Base, PlaneOwner::Layer);
            paint.read_plane(owner, s.tile, &s.channel)
        };
        let Some(image) = image else {
            return fields(serde_json::json!({
                "exists": false,
                "tile": s.tile,
                "channel": s.channel,
            }));
        };
        let mut texels = Vec::with_capacity(s.texels.len());
        for (j, [x, y]) in s.texels.iter().copied().enumerate() {
            if x >= image.width || y >= image.height {
                let (w, h) = (image.width, image.height);
                return Err(anyhow::anyhow!(
                    "texels[{j}]: ({x}, {y}) is outside the {w}x{h} plane"
                ));
            }
            let i = (y as usize * image.width as usize + x as usize) * 4;
            let px = image
                .data
                .get(i..i + 4)
                .ok_or_else(|| anyhow::anyhow!("internal: short plane readback"))?;
            texels.push([px[0], px[1], px[2], px[3]]);
        }
        fields(serde_json::json!({
            "exists": true,
            "composite": composite,
            "tile": s.tile,
            "channel": s.channel,
            "width": image.width,
            "height": image.height,
            "texels": texels,
        }))
    }

    /// `paint_undo` / `paint_redo` (`undo = true` / `false`): the paint
    /// session's stroke history — separate from the model's `undo`/`redo`
    /// (layer edits), exactly as in the app (two stacks; see the design
    /// doc). A session that never painted has nothing to undo: `false`,
    /// and no GPU device is requested.
    fn paint_history(&mut self, undo: bool) -> Result<Fields> {
        let key = if undo { "undone" } else { "redone" };
        let held = self.project_mut()?;
        let Some(paint) = held.paint.as_mut() else {
            return fields(serde_json::json!({
                (key): false,
                "can_undo_stroke": false,
                "can_redo_stroke": false,
            }));
        };
        let moved = if undo {
            paint.undo_stroke()
        } else {
            paint.redo_stroke()
        };
        paint
            .process_pending()
            .map_err(|e| anyhow::anyhow!("paint drain failed: {e:#}"))?;
        let (can_undo, can_redo) = (paint.can_undo_stroke(), paint.can_redo_stroke());
        held.flush_paint()?;
        fields(serde_json::json!({
            (key): moved,
            "can_undo_stroke": can_undo,
            "can_redo_stroke": can_redo,
        }))
    }

    /// `redo`: re-applies the active set's most recently undone edit.
    /// Mirrors [`Session::project_undo`] (empty reads as `redone:
    /// false`; the applied edit is journaled after the re-apply, like
    /// the app's `Document::redo`).
    fn project_redo(&mut self) -> Result<Fields> {
        let held = self.project_mut()?;
        let active = held.active.clone();
        let stack = held
            .model
            .layers
            .iter_mut()
            .find(|e| e.texture_set == active)
            .map(|e| &mut e.stack);
        let Some(stack) = stack else {
            return Err(anyhow::anyhow!(
                "texture set {active:?} has no layers entry"
            ));
        };
        let history = held
            .histories
            .entry(active.clone())
            .or_insert_with(|| UndoStack::new(HISTORY_MAX_ENTRIES));
        let redone = history.redo(stack);
        if redone {
            let op = match history.peek_undo() {
                Some(command) => JournalOp::applied(command, stack),
                None => None,
            };
            if let (Some(journal), Some(op)) = (held.journal.as_mut(), op) {
                if let Err(e) = journal.record(&active, op) {
                    eprintln!("agent: autosave journal append failed: {e}");
                }
            }
        }
        // Inline `ProjectSession::journal_seq` (see `project_undo`).
        let live = held.journal.as_ref().map(Journal::last_seq).unwrap_or(0);
        let journal_seq = held.model.journal_seq.max(live);
        let (can_undo, can_redo) = (history.can_undo(), history.can_redo());
        fields(serde_json::json!({
            "redone": redone,
            "journal_seq": journal_seq,
            "can_undo": can_undo,
            "can_redo": can_redo,
        }))
    }

    /// `bake`: validates the request synchronously (held mesh, no bake in
    /// flight, tiles), then starts the bake thread and replies
    /// `{"status": "started"}`.
    fn start_bake(&mut self, s: AgentBakeStep) -> Result<Fields> {
        if let Some(job) = &mut self.bake {
            if !job.poll().is_finished() {
                return Err(anyhow::anyhow!(
                    "a bake is already in flight (poll `bake_status` until it is done or failed)"
                ));
            }
        }
        let loaded = self.loaded()?;
        let present = &loaded.summary.tiles;
        let tiles = if s.tiles.is_empty() {
            Vec::new()
        } else {
            let tiles = select_tiles(&s.tiles, present)?;
            // Export's whole-mesh rule: a mesh living only in 1001 bakes
            // whole, with the untiled names (its triangles may be tagged
            // 1012 by a UV on the far edge, so filtering would drop them).
            if *present == [umber_mesh::FIRST_TILE] {
                Vec::new()
            } else {
                tiles
            }
        };
        let maps = if s.maps.is_empty() {
            BakeMap::ALL.to_vec()
        } else {
            s.maps
        };
        let flags = BakeFlags {
            size: s.size,
            rays: s.rays,
            dilate: s.dilate,
        };
        // v0 (`set_active`'s contract): an open project's working set
        // names the bake; otherwise the mesh's own set, as before.
        // `export` keeps the mesh's set: its naming lives inside
        // main.rs's `export_mesh` (out of this surface), so unifying it
        // is deferred — see the module docs.
        let set = self
            .project
            .as_ref()
            .map(|held| held.active.clone())
            .unwrap_or_else(|| loaded.summary.texture_set.clone());
        let mesh = Arc::clone(&loaded.mesh);
        let out_dir = s.out_dir;

        let status = Arc::new(Mutex::new(BakeStatus::Pending));
        let worker_status = Arc::clone(&status);
        let handle = std::thread::Builder::new()
            .name("umber-agent-bake".into())
            .spawn(move || {
                *lock(&worker_status) = BakeStatus::Running;
                let result = bake_held(&mesh, &set, &out_dir, &flags, &maps, &tiles);
                *lock(&worker_status) = match result {
                    Ok(written) => BakeStatus::Done {
                        texture_set: set,
                        written,
                    },
                    Err(e) => BakeStatus::Failed {
                        error: format!("{e:#}"),
                    },
                };
            })?;
        self.bake = Some(BakeJob {
            status,
            handle: Some(handle),
        });
        fields(serde_json::json!({ "status": "started" }))
    }

    /// The reply line for input line `line` (step `index`), and whether
    /// the session should end.
    fn reply(&mut self, index: usize, line: &str) -> (Value, bool) {
        let (name, step) = parse_line(line);
        let (outcome, quit) = match step {
            Ok(AgentStep::Quit(NoFields {})) => {
                let in_flight = self
                    .bake
                    .as_mut()
                    .is_some_and(|job| !job.poll().is_finished());
                let mut map = Fields::new();
                // An unfinished bake is abandoned at exit; say so.
                map.insert("bake_in_flight".into(), Value::Bool(in_flight));
                (Ok(map), true)
            }
            Ok(step) => (self.run(step), false),
            Err(e) => (Err(e), false),
        };
        (envelope(index, name, outcome), quit)
    }
}

/// Parses one input line: the step name it carries (for the reply, even
/// when the step itself is invalid) and the typed step.
fn parse_line(line: &str) -> (Option<String>, Result<AgentStep>) {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(e) => return (None, Err(anyhow::anyhow!("not a JSON step: {e}"))),
    };
    let name = step_name(&value);
    let step = serde_json::from_value(normalize_step(value)).map_err(anyhow::Error::from);
    (name, step)
}

/// The reply line: `ok` / `step` / `index`, then the result fields or
/// the `error`.
fn envelope(index: usize, step: Option<String>, outcome: Result<Fields>) -> Value {
    let mut map = Fields::new();
    map.insert("ok".into(), Value::Bool(outcome.is_ok()));
    map.insert("step".into(), step.map_or(Value::Null, Value::String));
    map.insert("index".into(), Value::from(index));
    match outcome {
        Ok(fields) => {
            for (key, value) in fields {
                map.insert(key, value);
            }
        }
        Err(e) => {
            map.insert("error".into(), Value::String(format!("{e:#}")));
        }
    }
    Value::Object(map)
}

/// Bakes the held mesh: the whole mesh when `tiles` is empty (the batch
/// `bake` behavior), otherwise each tile's own triangles.
fn bake_held(
    mesh: &umber_mesh::MeshData,
    set: &str,
    out_dir: &Path,
    flags: &BakeFlags,
    maps: &[BakeMap],
    tiles: &[u16],
) -> Result<Vec<PathBuf>> {
    let ctx = bake_context()?;
    if tiles.is_empty() {
        return bake_mesh_maps(&ctx, mesh, set, out_dir, flags, maps, None);
    }
    let mut written = Vec::new();
    for &tile in tiles {
        let tile_mesh = umber_bake::ao::filter_mesh_for_tile(mesh, tile);
        written.extend(bake_mesh_maps(
            &ctx,
            &tile_mesh,
            set,
            out_dir,
            flags,
            maps,
            Some(tile),
        )?);
    }
    Ok(written)
}

/// Runs the session over `input` until `quit` or EOF, writing one reply
/// line per step to `out` (flushed per line, because the harness is
/// waiting on it).
fn run_session(input: impl BufRead, out: &mut impl Write) -> Result<()> {
    let mut session = Session::default();
    let mut index = 0;
    for line in input.lines() {
        let line = match line {
            Ok(line) => line,
            // Invalid UTF-8: the line is consumed; answer and go on.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                let reply = envelope(index, None, Err(anyhow::anyhow!("not a JSON step: {e}")));
                index += 1;
                if !write_line(out, &reply)? {
                    return Ok(());
                }
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        if line.trim().is_empty() {
            continue;
        }
        let (reply, quit) = session.reply(index, &line);
        index += 1;
        if !write_line(out, &reply)? || quit {
            return Ok(());
        }
    }
    // EOF: the harness is gone (or done). A clean exit either way.
    Ok(())
}

/// Writes and flushes one reply line. Returns `Ok(false)` when the
/// reader has gone away (a broken pipe), which ends the session cleanly
/// just as EOF does.
fn write_line(out: &mut impl Write, reply: &Value) -> Result<bool> {
    match writeln!(out, "{reply}").and_then(|()| out.flush()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// `agent` — the JSONL session on stdin/stdout.
pub(crate) fn agent_cmd(args: &[String]) -> Result<()> {
    if let Some(extra) = args.first() {
        return Err(anyhow::anyhow!(
            "unexpected argument: {extra}\nusage: umber-cli agent (steps as JSON lines on stdin)"
        ));
    }
    // Stdout is locked per write, not for the session. A stray println
    // elsewhere then shows up as a non-JSON line (which the tests catch)
    // instead of deadlocking on the lock.
    run_session(std::io::stdin().lock(), &mut std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::{Channel, ProjectSettings, TextureSetLayers};

    /// Runs `input` through a session and parses every reply line (each
    /// must be JSON: the stdout discipline).
    fn session(input: &str) -> Vec<Value> {
        let mut out = Vec::new();
        run_session(input.as_bytes(), &mut out).expect("session runs");
        String::from_utf8(out)
            .expect("utf-8")
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
            .collect()
    }

    #[test]
    fn no_field_steps_accept_every_shorthand() {
        let replies = session("\"state\"\n{\"state\": null}\n{\"state\": {}}\n");
        assert_eq!(replies.len(), 3);
        for (i, r) in replies.iter().enumerate() {
            // Each parsed as `state` (no mesh yet: the honest error).
            assert_eq!(r["step"], "state", "{r}");
            assert_eq!(r["index"], i, "{r}");
            assert_eq!(r["ok"], false, "{r}");
            assert!(
                r["error"].as_str().unwrap().contains("no mesh loaded"),
                "{r}"
            );
        }
    }

    #[test]
    fn bad_lines_get_error_envelopes_and_the_loop_goes_on() {
        let replies = session(
            "{\"bakee\": {}}\n\nnot json\n{\"state\": {\"x\": 1}}\n\"bake_status\"\n\"quit\"\n\"state\"\n",
        );
        // The blank line is skipped; nothing after quit runs.
        assert_eq!(replies.len(), 5, "{replies:?}");
        assert_eq!(replies[0]["step"], "bakee");
        assert_eq!(replies[0]["ok"], false);
        assert!(replies[0]["error"]
            .as_str()
            .unwrap()
            .contains("unknown variant"));
        assert_eq!(replies[1]["step"], Value::Null);
        assert_eq!(replies[1]["index"], 1);
        assert!(replies[1]["error"]
            .as_str()
            .unwrap()
            .contains("not a JSON step"));
        assert_eq!(replies[2]["step"], "state");
        assert!(replies[2]["error"]
            .as_str()
            .unwrap()
            .contains("unknown field"));
        assert_eq!(replies[3]["step"], "bake_status");
        assert!(replies[3]["error"]
            .as_str()
            .unwrap()
            .contains("no bake started"));
        assert_eq!(replies[4]["step"], "quit");
        assert_eq!(replies[4]["ok"], true);
        assert_eq!(replies[4]["bake_in_flight"], false);
    }

    #[test]
    fn bake_and_export_need_a_loaded_mesh() {
        let replies = session(
            "{\"bake\": {\"out_dir\": \"o\"}}\n{\"export\": {\"preset\": \"gltf\", \"out_dir\": \"o\"}}\n",
        );
        for r in &replies {
            assert_eq!(r["ok"], false, "{r}");
            assert!(
                r["error"].as_str().unwrap().contains("no mesh loaded"),
                "{r}"
            );
        }
    }

    #[test]
    fn bake_status_serializes_tagged() {
        let done = BakeStatus::Done {
            texture_set: "quad".into(),
            written: vec![PathBuf::from("o/quad_ambient_occlusion.png")],
        };
        let v = serde_json::to_value(&done).unwrap();
        assert_eq!(v["status"], "done");
        assert_eq!(v["texture_set"], "quad");
        assert_eq!(
            serde_json::to_value(BakeStatus::Pending).unwrap()["status"],
            "pending"
        );
    }

    /// Two sets, three layers, exact opacities/blends/visibility: the
    /// census pins every value (0.5 and 1.0 are exact in binary, so the
    /// JSON round trip is bit-exact, not approximate).
    fn fixture_project() -> ProjectModel {
        let body = TextureSet::new_default("Body");
        let helmet = TextureSet {
            name: "Helmet".to_string(),
            resolution: 4096,
            channels: vec![Channel {
                name: "baseColor".into(),
                kind: ChannelKind::Color,
            }],
            tiles: BTreeMap::new(),
        };
        let mut stack = LayerStack::new();
        let base = stack.add_layer("Base", LayerKind::Paint);
        stack.set_opacity(base, 0.5);
        stack.set_blend_mode(base, BlendMode::Multiply);
        let top = stack.add_layer("Top", LayerKind::Fill);
        stack.set_visible(top, false);
        let mut helmet_stack = LayerStack::new();
        helmet_stack.add_layer("Metal", LayerKind::Paint);
        ProjectModel::new(
            vec![body, helmet],
            vec![
                TextureSetLayers {
                    texture_set: "Body".into(),
                    stack,
                },
                TextureSetLayers {
                    texture_set: "Helmet".into(),
                    stack: helmet_stack,
                },
            ],
            ProjectSettings {
                active_texture_set: Some("Body".into()),
                ..ProjectSettings::default()
            },
        )
    }

    /// A temp-dir project built the way umber-core's round-trip tests
    /// do (save_to_dir), plus the paint sidecar's size model: a set
    /// size, a per-channel override, and a tile assignment (the
    /// reserved prefix the census must skip, not choke on).
    fn write_fixture_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "umber-cli-agent-project-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        umber_core::save_to_dir(&fixture_project(), &dir).expect("fixture saves");
        std::fs::create_dir_all(dir.join("paint")).expect("paint dir");
        std::fs::write(
            dir.join("paint").join("target_sizes.txt"),
            "# Umber paint-set sizes v1\nBody=1024x1024\nchannel/Body/baseColor=512x512\ntile-1001=Body\n",
        )
        .expect("sidecar");
        dir
    }

    fn step_line(step: &Value) -> String {
        format!("{step}\n")
    }

    #[test]
    fn project_open_reads_back_the_saved_census_exactly() {
        let dir = write_fixture_project("open");
        let open = step_line(&serde_json::json!({ "project_open": { "dir": dir } }));
        let replies = session(&format!("{open}\"project_state\"\n"));
        assert_eq!(replies.len(), 2, "{replies:?}");
        let r = &replies[0];
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["step"], "project_open", "{r}");
        assert_eq!(r["index"], 0, "{r}");
        assert_eq!(r["version"], 1, "{r}");
        assert_eq!(r["active_set"], "Body", "{r}");
        assert_eq!(r["journal_seq"], 0, "{r}");
        let sets = r["texture_sets"].as_array().expect("sets array");
        assert_eq!(sets.len(), 2, "{r}");
        let body = &sets[0];
        assert_eq!(body["name"], "Body", "{r}");
        assert_eq!(body["resolution"], 2048, "{r}");
        assert_eq!(body["tiles"], serde_json::json!([1001]), "{r}");
        let channels = body["channels"].as_array().expect("channels array");
        assert_eq!(channels.len(), 5, "{r}");
        assert_eq!(channels[0]["name"], "baseColor", "{r}");
        assert_eq!(channels[0]["kind"], "Color", "{r}");
        assert_eq!(channels[0]["width"], 512, "{r}");
        assert_eq!(channels[0]["height"], 512, "{r}");
        assert_eq!(channels[1]["name"], "roughness", "{r}");
        assert_eq!(channels[1]["width"], 1024, "{r}");
        assert_eq!(channels[1]["height"], 1024, "{r}");
        let layers = body["layers"].as_array().expect("layers array");
        assert_eq!(layers.len(), 2, "{r}");
        assert_eq!(layers[0]["id"], 0, "{r}");
        assert_eq!(layers[0]["name"], "Base", "{r}");
        assert_eq!(layers[0]["kind"], "Paint", "{r}");
        assert_eq!(layers[0]["opacity"].as_f64().unwrap(), 0.5, "{r}");
        assert_eq!(layers[0]["visible"], true, "{r}");
        assert_eq!(layers[0]["blend"], "Multiply", "{r}");
        assert_eq!(layers[1]["name"], "Top", "{r}");
        assert_eq!(layers[1]["kind"], "Fill", "{r}");
        assert_eq!(layers[1]["visible"], false, "{r}");
        assert_eq!(layers[1]["blend"], "Normal", "{r}");
        let helmet = &sets[1];
        assert_eq!(helmet["name"], "Helmet", "{r}");
        assert_eq!(helmet["resolution"], 4096, "{r}");
        assert_eq!(helmet["channels"][0]["width"], 4096, "{r}");
        assert_eq!(helmet["channels"][0]["height"], 4096, "{r}");
        assert_eq!(helmet["layers"].as_array().unwrap().len(), 1, "{r}");
        // `project_state` carries the same sets: one census builder, so
        // the two can never disagree.
        let state = &replies[1];
        assert_eq!(state["ok"], true, "{state}");
        assert_eq!(
            state["project"]["texture_sets"], r["texture_sets"],
            "{state}"
        );
        assert_eq!(state["project"]["active_set"], "Body", "{state}");
        assert_eq!(
            state["project"]["dir"],
            serde_json::to_value(&dir).unwrap(),
            "{state}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_state_with_no_project_open_is_null_not_an_error() {
        let replies = session("\"project_state\"\n");
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["ok"], true, "{}", replies[0]);
        assert_eq!(replies[0]["project"], Value::Null, "{}", replies[0]);
    }

    #[test]
    fn project_steps_reject_unknown_fields_and_names() {
        let dir = write_fixture_project("errors");
        let open = step_line(&serde_json::json!({ "project_open": { "dir": dir } }));
        let bad_field =
            step_line(&serde_json::json!({ "project_open": { "dir": dir, "bogus": 1 } }));
        let bad_set = step_line(&serde_json::json!({ "set_active": { "set": "Nope" } }));
        let bad_state = step_line(&serde_json::json!({ "project_state": { "x": 1 } }));
        let missing =
            step_line(&serde_json::json!({ "project_open": { "dir": dir.join("missing.umber") } }));
        let input = format!("{bad_field}{open}{bad_set}{bad_state}{missing}");
        let replies = session(&input);
        assert_eq!(replies.len(), 5, "{replies:?}");
        assert_eq!(replies[0]["ok"], false, "{}", replies[0]);
        assert!(
            replies[0]["error"]
                .as_str()
                .unwrap()
                .contains("unknown field"),
            "{}",
            replies[0]
        );
        assert_eq!(replies[1]["ok"], true, "{}", replies[1]);
        assert_eq!(replies[2]["ok"], false, "{}", replies[2]);
        assert!(
            replies[2]["error"]
                .as_str()
                .unwrap()
                .contains("unknown texture set"),
            "{}",
            replies[2]
        );
        assert!(
            replies[2]["error"].as_str().unwrap().contains("\"Nope\""),
            "{}",
            replies[2]
        );
        assert!(
            replies[3]["error"]
                .as_str()
                .unwrap()
                .contains("unknown field"),
            "{}",
            replies[3]
        );
        // The typed load failure verbatim: the I/O prefix is ours, the
        // OS message rides along.
        assert!(
            replies[4]["error"].as_str().unwrap().contains("I/O error"),
            "{}",
            replies[4]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_active_switches_and_save_round_trips() {
        let dir = write_fixture_project("save");
        let save_dir = std::env::temp_dir().join(format!(
            "umber-cli-agent-project-saved-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&save_dir);
        let open = step_line(&serde_json::json!({ "project_open": { "dir": dir } }));
        let switch = step_line(&serde_json::json!({ "set_active": { "set": "Helmet" } }));
        let save = step_line(&serde_json::json!({ "project_save": { "dir": save_dir } }));
        let replies = session(&format!("{open}{switch}\"project_state\"\n{save}"));
        assert_eq!(replies.len(), 4, "{replies:?}");
        assert_eq!(replies[1]["active_set"], "Helmet", "{}", replies[1]);
        assert_eq!(
            replies[2]["project"]["active_set"], "Helmet",
            "{}",
            replies[2]
        );
        let saved = &replies[3];
        assert_eq!(saved["ok"], true, "{saved}");
        assert_eq!(saved["painted"], 0, "{saved}");
        assert_eq!(saved["journal_seq"], 0, "{saved}");
        // The save is project.json + layers only: no paint sidecar is
        // written (the CLI holds no paint tiles).
        assert!(save_dir.join("project.json").is_file());
        assert!(save_dir
            .join("layers")
            .join("Body")
            .join("0.json")
            .is_file());
        assert!(save_dir
            .join("layers")
            .join("Helmet")
            .join("0.json")
            .is_file());
        assert!(!save_dir.join("paint").exists());
        // The working set stamps into the saved settings: reopening
        // resolves Helmet, and the layers round-trip exactly. The
        // channel SIZES do not: a Save As target has no paint sidecar
        // (the save is project.json + layers only), so they fall back
        // to the model resolution — pinned here, not papered over.
        let text = std::fs::read_to_string(save_dir.join("project.json")).expect("read back");
        let file: Value = serde_json::from_str(&text).expect("project.json parses");
        assert_eq!(file["settings"]["active_texture_set"], "Helmet");
        let reopen = step_line(&serde_json::json!({ "project_open": { "dir": save_dir } }));
        let again = session(&reopen);
        assert_eq!(again[0]["active_set"], "Helmet", "{:?}", again[0]);
        assert_eq!(
            again[0]["texture_sets"][0]["layers"], replies[0]["texture_sets"][0]["layers"],
            "{:?}",
            again[0]
        );
        assert_eq!(
            again[0]["texture_sets"][1]["layers"], replies[0]["texture_sets"][1]["layers"],
            "{:?}",
            again[0]
        );
        assert_eq!(again[0]["texture_sets"][0]["channels"][0]["width"], 2048);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&save_dir);
    }

    /// The Body stack's layer count through a shared session borrow
    /// (a free fn, not a closure: the test interleaves it with
    /// `reply(&mut)` calls, and a closure would hold the shared borrow
    /// across them).
    fn body_layers_len(session: &Session) -> usize {
        session
            .project
            .as_ref()
            .expect("held")
            .model
            .layers
            .iter()
            .find(|e| e.texture_set == "Body")
            .expect("Body entry")
            .stack
            .layers
            .len()
    }

    // The field assignment builds test state incrementally (the
    // session starts default, the project lands mid-test); the
    // struct-literal form would need the full Session surface.
    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn undo_and_redo_drive_the_held_model() {
        let mut session = Session::default();
        session.project = Some(ProjectSession {
            model: fixture_project(),
            dir: PathBuf::from("/none"),
            active: "Body".to_string(),
            histories: BTreeMap::new(),
            journal: None,
            paint: None,
            prev_snapshot: None,
        });
        // A journaled edit, the way a future edit step records one:
        // through the undo journal, against the active set's stack.
        {
            let held = session.project.as_mut().expect("just stored");
            let stack = held
                .model
                .layers
                .iter_mut()
                .find(|e| e.texture_set == "Body")
                .map(|e| &mut e.stack)
                .expect("Body entry");
            held.histories
                .entry("Body".to_string())
                .or_insert_with(|| UndoStack::new(HISTORY_MAX_ENTRIES))
                .push(LayerCommand::add("Sketched", LayerKind::Paint), stack);
        }
        assert_eq!(body_layers_len(&session), 3);
        let (undone, quit) = session.reply(0, "\"undo\"");
        assert!(!quit);
        assert_eq!(undone["ok"], true, "{undone}");
        assert_eq!(undone["undone"], true, "{undone}");
        assert_eq!(undone["can_undo"], false, "{undone}");
        assert_eq!(undone["can_redo"], true, "{undone}");
        assert_eq!(undone["journal_seq"], 0, "{undone}");
        assert_eq!(body_layers_len(&session), 2);
        let (redone, _) = session.reply(1, "{\"redo\": {}}");
        assert_eq!(redone["ok"], true, "{redone}");
        assert_eq!(redone["redone"], true, "{redone}");
        assert_eq!(redone["can_undo"], true, "{redone}");
        assert_eq!(redone["can_redo"], false, "{redone}");
        assert_eq!(body_layers_len(&session), 3);
        let top = session
            .project
            .as_ref()
            .expect("held")
            .model
            .layers
            .iter()
            .find(|e| e.texture_set == "Body")
            .expect("Body entry")
            .stack
            .layers
            .last()
            .expect("redone layer")
            .name
            .clone();
        assert_eq!(top, "Sketched");
        // Undoing past the bottom reports false, never an error.
        let (again, _) = session.reply(2, "\"undo\"");
        assert_eq!(again["undone"], true, "{again}");
        let (empty, _) = session.reply(3, "\"undo\"");
        assert_eq!(empty["ok"], true, "{empty}");
        assert_eq!(empty["undone"], false, "{empty}");
        assert_eq!(empty["can_undo"], false, "{empty}");
        assert_eq!(empty["can_redo"], true, "{empty}");
    }

    #[test]
    fn project_steps_need_an_open_project() {
        let replies = session(
            "\"undo\"\n\"redo\"\n{\"project_save\": {\"dir\": \"o\"}}\n{\"set_active\": {\"set\": \"Body\"}}\n",
        );
        assert_eq!(replies.len(), 4, "{replies:?}");
        for r in &replies {
            assert_eq!(r["ok"], false, "{r}");
            assert!(
                r["error"].as_str().unwrap().contains("no project open"),
                "{r}"
            );
        }
    }
}
