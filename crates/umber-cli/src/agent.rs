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
//! Control protocol v0 (project + paint READS + the save seam — no
//! interactive paint driving): `project_open` holds a `.umber` project
//! beside the mesh (the mesh stays whatever `load` set — projects carry
//! no geometry), `project_state` censuses it (`null` when none is open),
//! `project_save` writes it back (`painted: 0`: the CLI holds no paint
//! tiles, so the save is `project.json` + layers only), `set_active`
//! chooses the working set, and `undo`/`redo` drive the model's per-set
//! undo journals. When a project is open, `bake` names its outputs after
//! the active set; `export` still names from the mesh (its naming lives
//! in main.rs's `export_mesh`, out of this surface — unifying it is
//! deferred). The session attaches the project's autosave journal on
//! open like the app does, so undo/redo record the edit they perform
//! and saves stamp the live high-water mark.
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
    BlendMode, ChannelKind, Journal, JournalOp, LayerCommand, LayerKind, LayerStack, ProjectModel,
    TextureSet, TileSet, UndoStack,
};

use crate::batch::{default_rays, default_size, BakeTransferStep, InspectStep};
use crate::{
    bake_context, bake_mesh_maps, export_mesh, mesh_summary, select_tiles, BakeFlags, BakeMap,
    MeshSummary, UDIM_GRID,
};

/// One session step. The batch's `inspect` / `bake_transfer` reuse the
/// recipe structs. `bake` / `export` drop the recipe's `mesh` field
/// because they act on the held mesh. The `project_*` / `set_active` /
/// `undo` / `redo` steps are the control-protocol v0 surface: they act
/// on the held project, never on the mesh.
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
    Undo(NoFields),
    Redo(NoFields),
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

/// The paint sidecar's size model as the census reads it: a structural
/// mirror of the app's `target_sizes.txt` size/channel arms (main.rs's
/// `parse_paint_set_sizes` + `parse_paint_target_size`). It lives here
/// because umber-cli must not depend on the app crate (that would pull
/// egui into the headless binary). Tile assignments are intentionally
/// not carried (the census needs sizes only — carrying them would be
/// dead state). Drift guard: the `#`-comment skip, the `=` split, the
/// `WxH` body and the reserved `tile-`/`channel/` prefixes must match
/// the app's parser line for line.
#[derive(Debug, Default)]
struct SidecarSizes {
    sizes: BTreeMap<String, (u32, u32)>,
    channels: BTreeMap<(String, String), (u32, u32)>,
}

/// The `target_size.txt` body shape (`"{width}x{height}"`): only
/// positive dims parse (a hand-edited file degrades to the model size —
/// never a panic, never a zero-size target).
fn parse_sidecar_size(text: &str) -> Option<(u32, u32)> {
    let (width, height) = text.trim().split_once('x')?;
    let width: u32 = width.trim().parse().ok()?;
    let height: u32 = height.trim().parse().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

/// One `channel/<set>/<channel>=<WxH>` override line into `into`.
/// The reserved-separator rule is the app's: a set or channel name
/// holding `/` cannot round-trip (the first `/` splits set from
/// channel), and the sidecar PNG layout already nests channels as
/// `<channel>/tile-<N>.png`, so `/` in a channel name never arrives.
fn parse_channel_override(
    into: &mut BTreeMap<(String, String), (u32, u32)>,
    rest: &str,
    right: &str,
) {
    let Some((set, channel)) = rest.split_once('/') else {
        return;
    };
    if set.is_empty() || channel.is_empty() {
        return;
    }
    if let Some(size) = parse_sidecar_size(right) {
        into.insert((set.to_string(), channel.to_string()), size);
    }
}

/// Parses a `target_sizes.txt` body the way the app's loader does:
/// `#` comments and malformed lines are skipped (never a panic), the
/// `tile-<N>` prefix is reserved for assignments (skipped here — sizes
/// only), and later lines win (BTreeMap insert order).
fn parse_sidecar_sizes(text: &str) -> SidecarSizes {
    let mut out = SidecarSizes::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let (left, right) = (left.trim(), right.trim());
        if left
            .strip_prefix("tile-")
            .is_some_and(|n| n.parse::<u16>().is_ok())
        {
            continue;
        }
        if let Some(rest) = left.strip_prefix("channel/") {
            parse_channel_override(&mut out.channels, rest, right);
            continue;
        }
        if left.is_empty() || right.is_empty() {
            continue;
        }
        if let Some(size) = parse_sidecar_size(right) {
            out.sizes.insert(left.to_string(), size);
        }
    }
    out
}

/// Reads the opened project's `paint/target_sizes.txt` when present.
/// A missing or unreadable sidecar is an empty mapping (no per-set
/// information — sizes fall back to the model), never an error.
fn read_sidecar_sizes(dir: &Path) -> SidecarSizes {
    match std::fs::read_to_string(dir.join("paint").join("target_sizes.txt")) {
        Ok(text) => parse_sidecar_sizes(&text),
        Err(_) => SidecarSizes::default(),
    }
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

/// One set's census: the model channels sized through the sidecar
/// mapping (override, else set size, else the model resolution), the
/// present tiles (an empty tile map means the implied 1001 — the
/// `TileSet` conversion's rule), and the stack summary (no layers
/// entry reads as an empty stack — lenient here; the ACTIVE set's
/// entry is required at open by [`resolve_active`]).
fn texture_set_census(
    set: &TextureSet,
    stack: Option<&LayerStack>,
    sizes: &SidecarSizes,
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
        Some(stack) => stack
            .layers
            .iter()
            .map(|layer| LayerCensus {
                id: layer.id,
                name: layer.name.clone(),
                kind: layer.kind.clone(),
                opacity: layer.opacity,
                visible: layer.visible,
                blend: layer.blend_mode,
            })
            .collect(),
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
            AgentStep::Undo(NoFields {}) => self.project_undo(),
            AgentStep::Redo(NoFields {}) => self.project_redo(),
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
        let session = ProjectSession {
            model,
            dir: s.dir,
            active,
            histories: BTreeMap::new(),
            journal,
        };
        let census = session.census();
        self.project = Some(session);
        fields(census)
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
        umber_core::save_to_dir(&held.model, &s.dir).map_err(|e| anyhow::anyhow!("{e}"))?;
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
            "painted": 0,
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
