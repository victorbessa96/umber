//! The Graph panel: the wave-5 slice-5 app bridge for the procedural
//! node graph (`docs/specs/node-graph-design.md`, "The app bridge").
//!
//! v1 is a node list + param editors (the [`crate::brush_panel`] slider
//! patterns), NOT the noodle canvas — the canvas editor is wave-6, and
//! the design is explicit that the P0 is the engine + interchange. The
//! panel owns a [`umber_graph::Graph`], a fully-seeded
//! [`umber_graph::NodeRegistry`], an [`umber_graph::EvalCache`], and the
//! dirty set the cached-eval API consumes; edits mark nodes dirty, the
//! Evaluate button re-runs only dirty nodes + their downstream, and the
//! output node's image displays in the panel via an egui texture.
//!
//! Failures are honest: eval errors land on the status line, never a
//! panic. Mesh-map externals (`mesh_map:AO`, `mesh_map:Normal`) are flat
//! placeholders in v1 — the bake bridge does not expose live in-memory
//! bakes yet, so `mesh_map_generator` nodes evaluate against documented
//! stand-ins until it does.
//!
//! Wave-6: the noodle canvas ([`crate::graph_canvas`]) is the default
//! interaction surface; the v1 list stays behind a Canvas/List toggle.
//! Both views share the selected-node param editor below them.
//!
//! # Canvas-position persistence (the mtlx carry)
//!
//! [`Node::canvas`] rides the panel's `.mtlx` string as a `vector2` input
//! named [`CANVAS_POS_PARAM`] — chosen because the mtlx layer round-trips
//! Vec2 params exactly (shortest-round-trip `f32` formatting; pinned by
//! umber-graph's `round_trip_preserves_data`), so no sidecar map is
//! needed. [`GraphPanel::to_mtlx`] appends it only for placed nodes (never
//! touching the live graph), and [`GraphPanel::load_graph`] lifts it back
//! out of the params into `Node::canvas`. Files without it — every
//! pre-canvas `.umber` — load with `None`, which the canvas lays out with
//! its deterministic id-hash scatter.
//!
//! # WASM plugins (wave-6 plugins slice 3)
//!
//! [`GraphPanel::load_plugins`] registers plugin nodes AFTER the
//! built-ins (see [`crate::plugins`] for the dirs, the per-file
//! tolerance, and the no-shadowing collision rule). They list in the
//! Add-Node menu beside the built-ins (the registry's `node_defs` is the
//! one listing), dispatch through the same `eval_graph_cached`, and save
//! as custom nodedefs declared after [`umber_graph::mtlx::painter_nodedefs`].
//!
//! # The curves editor (g4 census consumer)
//!
//! A `curves` node's four `Vec2` params `curve_p0..curve_p3` edit as ONE
//! [`umber_gui::CurveEdit`] (fixed count, no label of its own: the node
//! title names the def), drawn at the first curve param's slot in the param
//! loop. The special case keys on `def == "curves"` AND all four params
//! being `Vec2` ([`curve_points`]); no other built-in def carries a
//! `curve_p*` param, and plugins can never register as `curves` (the
//! no-shadowing rule). Anything else, a partial curves node included,
//! keeps the raw rows. The model is rebuilt from the params every frame
//! (the brush_panel falloff-Custom pattern: clamp + `MIN_GAP + 1e-6` nudge,
//! identity fallback). Judgment calls:
//!
//! - **The write-back contract is the eval's, not the widget's.** graph_eval
//!   and umber-graph's `CurvesNode` require only finite coordinates and
//!   strictly increasing x. y (and x) are not range-limited, so the write-back
//!   ([`curve_write_back`]) invents no clamp. Only the point the user moved
//!   takes the editor's value; the others keep their stored value, so a
//!   stored `y = 2.0` survives an edit elsewhere. The point being dragged is
//!   clamped into `[0, 1]²`; the widget cannot represent anything else.
//! - **The identity fallback writes back whole.** When the model refuses
//!   the stored curve (e.g. three points crowded within `MIN_GAP` of
//!   `x = 1`), the editor shows [`CURVE_IDENTITY`], and the first edit
//!   replaces the whole curve with the edited one. A merge there would keep
//!   the crowded points, and the editor would show the identity forever.
//! - **Write-back only on change.** It needs both the widget's `changed()`
//!   and a result different from the stored points. Then four `set_param`
//!   calls follow (the node goes dirty once).
//! - **A fresh `curves` node starts at the identity** ([`default_params`]),
//!   so a node added from the menu evaluates and shows the editor.
//!   Before this it started empty and reported the missing-param error.
//! - **Size 160 px** (brush's in-row editor is 120): here the curve is the
//!   node's whole body, not a row beside a combo.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use fluent::FluentValue;
use umber_gpu::{EvalBackend, GraphGpuEvaluator, TileSpec};
use umber_graph::{
    eval_graph_cached, AnchorRef, EvalCache, EvalContext, Graph, NodeOutput, NodeRegistry,
    ParamValue,
};

use umber_core::undo::UndoStack;

use crate::graph_canvas::CanvasState;
use crate::i18n::{tr, tr_args};
use crate::plugins::{self, PluginLoadReport};

mod history;

use history::GraphCommand;

/// Default raster resolution (square) for a fresh panel.
const DEFAULT_RESOLUTION: (u32, u32) = (512, 512);

/// The reserved param name carrying [`Node::canvas`] through `.mtlx`
/// (see module docs). Never shown in the param editor.
const CANVAS_POS_PARAM: &str = "__canvas_pos";

/// Which interaction surface the panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphView {
    /// The noodle canvas (default).
    Canvas,
    /// The v1 node list (fallback).
    List,
}

/// Sensible starting params for an added node, by `node_def`. Nodes not
/// listed here start with no params — evaluation then reports the honest
/// missing-param error on the status line rather than silently inventing
/// values.
fn default_params(node_def: &str) -> Vec<(String, ParamValue)> {
    match node_def {
        "uniform" => vec![("color".into(), ParamValue::Color([1.0, 1.0, 1.0]))],
        "noise_perlin" | "noise_value" | "noise_worley" => vec![
            ("scale".into(), ParamValue::Float(4.0)),
            ("seed".into(), ParamValue::Int(1)),
        ],
        "image_asset" => vec![("path".into(), ParamValue::Asset(String::new()))],
        "mesh_map_generator" => vec![("map_name".into(), ParamValue::Asset("AO".into()))],
        "anchor" => vec![
            ("layer_id".into(), ParamValue::Int(0)),
            ("channel".into(), ParamValue::Asset("baseColor".into())),
        ],
        "synth" => vec![
            ("example".into(), ParamValue::Asset(String::new())),
            ("map_mode".into(), ParamValue::Int(0)),
            ("tiling_mode".into(), ParamValue::Int(0)),
            ("randomness".into(), ParamValue::Float(0.0)),
            ("seed".into(), ParamValue::Int(1)),
        ],
        // The identity curve, so a node added from the menu evaluates and
        // shows the curve editor (which keys on the four params).
        "curves" => CURVE_PARAMS
            .iter()
            .zip(CURVE_IDENTITY)
            .map(|(name, point)| ((*name).to_string(), ParamValue::Vec2(point)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Builds the panel's registry: the shared [`umber_graph::full_registry`] —
/// the engine seed plus every merged node family plus the dynamic-material
/// nodes. A private delegation (not inlined at the call site) so the
/// sharing contract reads at the one place the registry is built: the
/// Graph panel and the material eval driver evaluate on identical
/// registries, and the panel can open a definition containing
/// `material_output`.
fn full_registry() -> NodeRegistry {
    umber_graph::full_registry()
}

/// The Graph panel state: the editable graph, its eval cache, and the
/// egui session around them.
///
/// The egui drawing itself (`show`) is untested by construction; every
/// other method is pure logic with no UI dependency.
pub struct GraphPanel {
    graph: Graph,
    registry: NodeRegistry,
    cache: EvalCache,
    dirty: HashSet<u64>,
    resolution: (u32, u32),
    last_output: Option<NodeOutput>,
    selected: Option<u64>,
    status: String,
    /// Whether `status` reports a failure (drawn in the error color).
    status_is_error: bool,
    output_node: Option<u64>,
    next_id: u64,
    /// The graph edit journal (wave-procedural-history, kill 1): every
    /// topology + param edit rides a [`GraphCommand`] through
    /// [`Self::run_graph_command`]. Panel-local in this kill — the
    /// Graph panel's own Undo/Redo buttons drive it; the unified
    /// Ctrl+Z timeline + the History panel's rows are the named
    /// follow-up kill.
    history: UndoStack<GraphCommand>,
    /// Add-Node combo selection (a `node_def` name).
    add_def: String,
    /// Edge editor: the input name new edges attach to.
    edge_input: String,
    /// Edge editor: the upstream node new edges come from.
    edge_from: Option<u64>,
    /// Displayed output texture + the key (dims + content hash) it was
    /// built from, so the texture only re-uploads when the bytes change.
    texture: Option<egui::TextureHandle>,
    texture_key: Option<(u32, u32, u64)>,
    /// Canvas or list.
    view: GraphView,
    /// Which backend [`Self::evaluate`] runs: CPU always, GPU when an
    /// evaluator is attached (Auto, the default), or GPU-required. The
    /// CPU path is unchanged and stays the reference for tests.
    eval_backend: EvalBackend,
    /// The device-bound GPU evaluator, attached by the app once eframe's
    /// device is available (the panel itself never owns wgpu objects —
    /// the architecture rule).
    gpu_evaluator: Option<GraphGpuEvaluator>,
    /// The canvas's session state (view, drag, edge selection, menu).
    pub(crate) canvas: CanvasState,
    /// `.mtlx` declarations for the loaded plugin nodes (sorted by name).
    plugin_decls: Vec<umber_graph::mtlx::NodedefDecl>,
    /// The last plugin scan's summary line (kept apart from `status`,
    /// which every evaluate overwrites).
    plugin_status: Option<String>,
    /// Untrusted plugins awaiting consent, `(path, blake3 hex)` (C2-21).
    plugin_pending: Vec<(PathBuf, String)>,
    /// The consent store the scan and the Trust button share.
    plugin_trust: plugins::TrustStore,
    /// The active texture set's layers, for the anchor picker (the app
    /// refreshes this each frame — the picker filters graph inputs to
    /// the set by construction).
    anchor_layers: Vec<(u64, String)>,
    /// Composited layer channels for anchor resolution, keyed
    /// `anchor:{id}:{channel}` (merged into the eval externals on every
    /// [`Self::evaluate`] — the bridge uploads what the compositor baked).
    anchor_externals: HashMap<String, NodeOutput>,
}

impl GraphPanel {
    /// Creates the panel: empty graph, fully-seeded registry, 512x512
    /// resolution, no selection, nothing evaluated yet.
    pub fn new() -> Self {
        let registry = full_registry();
        let add_def = registry
            .node_defs()
            .first()
            .copied()
            .unwrap_or("passthrough")
            .to_string();
        Self {
            graph: Graph::new(),
            registry,
            cache: EvalCache::new(),
            dirty: HashSet::new(),
            resolution: DEFAULT_RESOLUTION,
            last_output: None,
            selected: None,
            status: tr("graph.no-eval-yet"),
            status_is_error: false,
            output_node: None,
            next_id: 1,
            history: UndoStack::new(history::HISTORY_MAX),
            add_def,
            edge_input: String::from("in"),
            edge_from: None,
            texture: None,
            texture_key: None,
            view: GraphView::Canvas,
            eval_backend: EvalBackend::default(),
            gpu_evaluator: None,
            canvas: CanvasState::default(),
            plugin_decls: Vec::new(),
            plugin_status: None,
            plugin_pending: Vec::new(),
            plugin_trust: plugins::TrustStore::default(),
            anchor_layers: Vec::new(),
            anchor_externals: HashMap::new(),
        }
    }

    /// Scans `dirs` for `*.wasm` plugins and registers them after the
    /// built-ins ([`crate::plugins::load_plugins`]): failures are
    /// warnings on the plugin status line, never fatal. Returns the
    /// scan's report.
    pub fn load_plugins(&mut self, dirs: &[PathBuf]) -> PluginLoadReport {
        self.load_plugins_with_trust(dirs, plugins::TrustStore::open(plugins::trust_path()))
    }

    /// [`Self::load_plugins`] against an explicit consent store: tests
    /// pass a temp-file store, so no test ever reads or writes the
    /// user's real `plugins-trusted.txt`.
    pub fn load_plugins_with_trust(
        &mut self,
        dirs: &[PathBuf],
        trust: plugins::TrustStore,
    ) -> PluginLoadReport {
        self.plugin_trust = trust;
        let report = plugins::load_plugins(&mut self.registry, dirs, &self.plugin_trust);
        self.plugin_pending = report.pending.clone();
        self.plugin_decls.extend(
            report
                .loaded
                .iter()
                .map(|name| plugins::plugin_nodedef(name)),
        );
        self.plugin_decls.sort_by(|a, b| a.name.cmp(&b.name));
        self.plugin_status = report.status_line();
        report
    }

    /// Trusts pending plugin `index` (persisting its hash) and loads it.
    /// The file must still hash to what was listed: a swapped file is
    /// refused and stays out. Returns the registered name, or the reason.
    ///
    /// # Errors
    ///
    /// The trust file failing to save, or the load failing.
    pub fn trust_pending_plugin(&mut self, index: usize) -> Result<String, String> {
        let (path, hash) = self
            .plugin_pending
            .get(index)
            .cloned()
            .ok_or_else(|| "no such pending plugin".to_owned())?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.plugin_trust
            .trust(&hash, &name)
            .map_err(|e| format!("cannot save trust: {e}"))?;
        let result = plugins::load_trusted_file(&mut self.registry, &path, &hash);
        self.plugin_pending.remove(index);
        if let Ok(node) = &result {
            self.plugin_decls.push(plugins::plugin_nodedef(node));
            self.plugin_decls.sort_by(|a, b| a.name.cmp(&b.name));
        }
        result
    }

    /// The untrusted plugins awaiting consent.
    pub fn plugin_pending(&self) -> &[(PathBuf, String)] {
        &self.plugin_pending
    }

    /// One row per untrusted plugin: its file, the first 12 hex digits of
    /// its hash (to compare against what the author published), and the
    /// one-click Trust & load. Plugins run sandboxed (no imports, fuel,
    /// a memory cap), so the consent guards against unexpected code
    /// rather than host access; the row says where the file is.
    fn pending_plugins_ui(&mut self, ui: &mut egui::Ui) {
        let mut trust = None;
        for (i, (path, hash)) in self.plugin_pending.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label("⚠");
                ui.monospace(path.display().to_string());
                ui.weak(format!("blake3 {}…", &hash[..12]));
                // Untranslated until the catalogs take the consent keys.
                if ui.button("Trust & load").clicked() {
                    trust = Some(i);
                }
            });
        }
        if let Some(i) = trust {
            self.status = match self.trust_pending_plugin(i) {
                Ok(name) => format!("plugin {name} trusted and loaded"),
                Err(reason) => format!("plugin not loaded: {reason}"),
            };
        }
    }

    /// The last plugin scan's summary (`None` when no `.wasm` was found).
    pub fn plugin_status(&self) -> Option<&str> {
        self.plugin_status.as_deref()
    }

    /// The editable graph.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The seeded registry (all merged node families).
    pub fn registry(&self) -> &NodeRegistry {
        &self.registry
    }

    /// The dirty set (nodes awaiting re-evaluation).
    pub fn dirty_set(&self) -> &HashSet<u64> {
        &self.dirty
    }

    /// The selected node id, if any.
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    /// The output node id (the node Evaluate displays/exports).
    pub fn output_node(&self) -> Option<u64> {
        self.output_node
    }

    /// The last evaluation's status line.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The output node's last evaluated value, if any.
    pub fn last_output(&self) -> Option<&NodeOutput> {
        self.last_output.as_ref()
    }

    /// The shared raster resolution evaluations render at.
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }

    /// Which backend [`Self::evaluate`] runs (default [`EvalBackend::Auto`]:
    /// GPU when an evaluator is attached, else CPU).
    pub fn eval_backend(&self) -> EvalBackend {
        self.eval_backend
    }

    /// Selects the evaluation backend (the §5 GPU row's switch: `Cpu`
    /// keeps the reference path, `Gpu` requires an attached evaluator,
    /// `Auto` uses one when present).
    pub fn set_eval_backend(&mut self, backend: EvalBackend) {
        self.eval_backend = backend;
    }

    /// Attaches a device-bound GPU evaluator (built by the app from
    /// eframe's device/queue). `Auto` evaluations use it from then on;
    /// detaching returns to CPU.
    pub fn attach_gpu_evaluator(&mut self, evaluator: GraphGpuEvaluator) {
        self.gpu_evaluator = Some(evaluator);
    }

    /// Detaches the GPU evaluator, if any (evaluations run on the CPU).
    pub fn detach_gpu_evaluator(&mut self) {
        self.gpu_evaluator = None;
    }

    /// Whether a GPU evaluator is currently attached.
    pub fn has_gpu_evaluator(&self) -> bool {
        self.gpu_evaluator.is_some()
    }

    /// Retargets the raster resolution (marks everything dirty — every
    /// cached image is now the wrong size).
    pub fn set_resolution(&mut self, resolution: (u32, u32)) {
        let (w, h) = (resolution.0.max(1), resolution.1.max(1));
        if (w, h) != self.resolution {
            self.resolution = (w, h);
            self.mark_all_dirty();
        }
    }

    /// Selects a node for param editing (`None` clears).
    pub fn select(&mut self, id: Option<u64>) {
        self.selected = id.filter(|id| self.graph.nodes.iter().any(|n| n.id == *id));
    }

    /// Edits (or adds) one param on a node and marks it dirty. Returns
    /// `false` when the node id is unknown. Journaled
    /// (wave-procedural-history: graph edits were the last
    /// non-undoable edit class).
    pub fn set_param(&mut self, node_id: u64, name: &str, value: ParamValue) -> bool {
        if !self.graph.nodes.iter().any(|n| n.id == node_id) {
            return false;
        }
        self.run_graph_command(GraphCommand::SetParam {
            node: node_id,
            name: name.to_string(),
            new: value,
            old: None,
        });
        true
    }

    /// Appends a node with a fresh unique id and starter params, marks
    /// it dirty, and adopts it as the output node when the graph had
    /// none. Returns the new id. Journaled (wave-procedural-history):
    /// the command captures the allocated id, so a redo restores the
    /// exact same node.
    pub fn add_node(&mut self, node_def: &str) -> u64 {
        self.run_graph_command(GraphCommand::AddNode {
            def: node_def.to_string(),
            pos: None,
            id: None,
            prior_output: None,
        });
        match self.history.peek_undo() {
            Some(GraphCommand::AddNode { id: Some(id), .. }) => *id,
            // `peek_undo` is `Some` exactly after a push and AddNode's
            // apply always captures its id — the `Document::run`
            // invariant comment's precedent.
            _ => unreachable!("AddNode captures its id on apply"),
        }
    }

    /// [`Self::add_node`], placed at a graph-space canvas position (the
    /// canvas creation menu's path). The placement rides the journaled
    /// command, so a redo restores the placed node exactly; moving an
    /// EXISTING node stays unjournaled by the design's own
    /// editor-only-data rule (position dirties nothing).
    pub fn add_node_at(&mut self, node_def: &str, pos: [f32; 2]) -> u64 {
        self.run_graph_command(GraphCommand::AddNode {
            def: node_def.to_string(),
            pos: Some(pos),
            id: None,
            prior_output: None,
        });
        match self.history.peek_undo() {
            Some(GraphCommand::AddNode { id: Some(id), .. }) => *id,
            _ => unreachable!("AddNode captures its id on apply"),
        }
    }

    /// Moves a node on the canvas. Position is editor-only data: nothing
    /// is marked dirty. Returns `false` when the id is unknown.
    pub fn set_node_canvas(&mut self, node_id: u64, pos: [f32; 2]) -> bool {
        match self.graph.nodes.iter_mut().find(|n| n.id == node_id) {
            Some(node) => {
                node.canvas = Some(pos);
                true
            }
            None => false,
        }
    }

    /// The canvas connect: replaces whatever already feeds `to`'s
    /// `input` (an input takes one edge), then wires `from` in.
    /// Self-loops and unknown endpoints return `false` and change
    /// nothing. Journaled (wave-procedural-history): the replaced
    /// edge's source is captured, so an undo rewires the prior feed.
    pub fn connect(&mut self, from: u64, to: u64, input: &str) -> bool {
        let known = |id: u64| self.graph.nodes.iter().any(|n| n.id == id);
        if from == to || !known(from) || !known(to) {
            return false;
        }
        self.run_graph_command(GraphCommand::Connect {
            from,
            to,
            input: input.to_string(),
            replaced_from: None,
        });
        true
    }

    /// Removes the edge feeding `to`'s `input` and marks `to` dirty.
    /// Returns `false` when no such edge exists. Journaled
    /// (wave-procedural-history).
    pub fn remove_edge(&mut self, to: u64, input: &str) -> bool {
        if !self
            .graph
            .edges
            .iter()
            .any(|e| e.to == to && e.input == input)
        {
            return false;
        }
        self.run_graph_command(GraphCommand::RemoveEdge {
            to,
            input: input.to_string(),
            old_from: None,
        });
        true
    }

    /// Removes a node and every edge touching it. Returns `false` when
    /// the id is unknown. Structural edits invalidate the whole cache
    /// (downstream wiring changed), so everything still standing is
    /// marked dirty. Journaled (wave-procedural-history): the node,
    /// its edges and the panel's prior output/selection are captured
    /// for a full restore.
    pub fn remove_node(&mut self, node_id: u64) -> bool {
        if !self.graph.nodes.iter().any(|n| n.id == node_id) {
            return false;
        }
        self.run_graph_command(GraphCommand::RemoveNode {
            id: node_id,
            removed: None,
            prior_output: None,
            prior_selected: None,
        });
        true
    }

    /// Adds an edge (`from` output → `to`'s `input`) and marks the
    /// consumer dirty. Returns `false` when either endpoint is unknown.
    /// Journaled (wave-procedural-history) — the list view's edge
    /// editor reaches this directly, so the raw insert is a command of
    /// its own, distinct from [`Self::connect`]'s replace semantics
    /// (the pre-kill push behavior is preserved exactly).
    pub fn add_edge(&mut self, from: u64, to: u64, input: &str) -> bool {
        let known = |id: u64| self.graph.nodes.iter().any(|n| n.id == id);
        if !known(from) || !known(to) {
            return false;
        }
        self.run_graph_command(GraphCommand::AddEdge {
            from,
            to,
            input: input.to_string(),
        });
        true
    }

    /// Marks a node as the panel's output (displayed + exported).
    /// Returns `false` when the id is unknown. Journaled
    /// (wave-procedural-history): the prior output is captured.
    pub fn set_output_node(&mut self, node_id: u64) -> bool {
        if !self.graph.nodes.iter().any(|n| n.id == node_id) {
            return false;
        }
        self.run_graph_command(GraphCommand::SetOutput {
            id: node_id,
            old: None,
        });
        true
    }

    /// Refreshes the anchor picker's layer list: the active texture set's
    /// `(id, name)` pairs. The picker filters graph inputs to the set by
    /// construction — it can only bind what is listed here.
    pub fn set_anchor_layers(&mut self, layers: Vec<(u64, String)>) {
        self.anchor_layers = layers;
    }

    /// The anchor picker's current layer list.
    pub fn anchor_layers(&self) -> &[(u64, String)] {
        &self.anchor_layers
    }

    /// Binds a node input to a layer channel anchor (§3:42) and marks the
    /// node dirty. Returns `false` when the node id is unknown.
    pub fn bind_anchor(&mut self, node_id: u64, input: &str, layer_id: u64, channel: &str) -> bool {
        self.set_param(
            node_id,
            input,
            ParamValue::Anchor(AnchorRef::new(layer_id, channel)),
        )
    }

    /// Uploads composited layer channels for anchor resolution, keyed
    /// `anchor:{id}:{channel}` (see [`AnchorRef::key`]). Merged into the
    /// eval externals on every [`Self::evaluate`], on both backends.
    pub fn set_anchor_inputs(&mut self, inputs: HashMap<String, NodeOutput>) {
        self.anchor_externals = inputs;
    }

    /// Merges the bridge externals (mesh-map stand-ins + uploaded anchor
    /// channels) for one evaluation.
    fn eval_externals(&self) -> HashMap<String, NodeOutput> {
        let mut external = mesh_map_externals(self.resolution);
        external.extend(self.anchor_externals.clone());
        external
    }

    /// Replaces the graph wholesale (the Open-Project path): cache
    /// cleared, everything marked dirty, output defaulting to the
    /// highest id, selection cleared. Carried canvas positions
    /// ([`CANVAS_POS_PARAM`]) are lifted out of the params into
    /// [`Node::canvas`].
    pub fn load_graph(&mut self, mut graph: Graph) {
        lift_canvas_positions(&mut graph);
        self.canvas.drag = Default::default();
        self.canvas.selected_edge = None;
        self.canvas.menu = None;
        // A loaded graph replaces the state the journal's entries
        // describe — they die with the old graph (an undo across a
        // load would resurrect nodes into a foreign graph).
        self.history.clear();
        self.next_id = graph.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        self.output_node = graph.nodes.iter().map(|n| n.id).max();
        self.graph = graph;
        self.cache = EvalCache::new();
        self.mark_all_dirty();
        self.selected = None;
        self.last_output = None;
        self.texture = None;
        self.texture_key = None;
        self.status = tr("graph.loaded");
        self.status_is_error = false;
    }

    /// Serializes the graph to a `.mtlx` document string (the
    /// project-persistence carry). Placed nodes carry their canvas
    /// position as a trailing [`CANVAS_POS_PARAM`] `vector2` input (see
    /// module docs); unplaced nodes emit exactly as before.
    pub fn to_mtlx(&self) -> String {
        let mut carried = self.graph.clone();
        for node in &mut carried.nodes {
            if let Some(pos) = node.canvas {
                node.params
                    .push((CANVAS_POS_PARAM.to_string(), ParamValue::Vec2(pos)));
            }
        }
        let mut decls = umber_graph::mtlx::painter_nodedefs();
        decls.extend(self.plugin_decls.iter().cloned());
        umber_graph::mtlx::to_mtlx(&carried, &decls)
    }

    /// Parses a `.mtlx` document string into the panel (the
    /// project-restore path). Unknown node types survive as opaque defs
    /// per the mtlx forward-compat contract; the warning count rides
    /// the status line. Returns `false` (keeping the current graph)
    /// when the string does not parse.
    pub fn load_mtlx(&mut self, doc: &str) -> bool {
        match umber_graph::mtlx::from_mtlx(doc) {
            Ok((graph, _, warnings)) => {
                self.load_graph(graph);
                if !warnings.is_empty() {
                    let args = [("count", FluentValue::from(warnings.len()))];
                    self.status = tr_args("graph.loaded-unknown", args);
                }
                true
            }
            Err(err) => {
                let args = [("error", FluentValue::from(err.to_string()))];
                self.status = tr_args("graph.load-failed", args);
                self.status_is_error = true;
                false
            }
        }
    }

    /// Copies the open graph into `defs` under a fresh `material_id` (Kill B,
    /// `docs/specs/dynamic-materials-design.md` §4 — the Graph panel's "Save
    /// as material"). A COPY: the definition is the `.mtlx` string as it
    /// reads now (canvas positions and plugin declarations included), so
    /// later Graph-panel edits to the source graph never propagate (the
    /// design's non-goal 3). Ids mint as `umber.material-<n>` (smallest `n`
    /// unused — never an overwrite). Returns the new id, or `None` when the
    /// graph is empty (nothing to save — an honest no-op, never an empty
    /// definition).
    pub fn save_as_material(&mut self, defs: &mut BTreeMap<String, String>) -> Option<String> {
        if self.graph.nodes.is_empty() {
            return None;
        }
        let doc = self.to_mtlx();
        let mut n = defs.len() + 1;
        loop {
            let id = format!("umber.material-{n}");
            if !defs.contains_key(&id) {
                defs.insert(id.clone(), doc);
                return Some(id);
            }
            n += 1;
        }
    }

    /// The "Save as material" section, drawn under the panel (the button the
    /// Layers panel's `material_id` combo then offers). Writes into the
    /// session's definition map; the status line reports the new id (or the
    /// empty-graph refusal). The egui drawing is untested by construction;
    /// [`Self::save_as_material`] carries the headless tests.
    pub fn show_material_section(
        &mut self,
        ui: &mut egui::Ui,
        defs: &mut BTreeMap<String, String>,
    ) {
        ui.add_space(4.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("graph.save-material")).clicked() {
                match self.save_as_material(defs) {
                    Some(id) => {
                        let args = [("id", FluentValue::from(id.as_str()))];
                        self.status = tr_args("graph.saved-material", args);
                        self.status_is_error = false;
                    }
                    None => {
                        self.status = tr("graph.save-material-empty");
                        self.status_is_error = true;
                    }
                }
            }
        });
    }

    /// Re-runs dirty nodes (+ downstream) through the cache and refreshes
    /// the output display. Errors land on the status line; the previous
    /// output (if any) is kept. Never panics.
    ///
    /// The backend follows [`Self::eval_backend`]: `Cpu` (and `Auto`
    /// without an attached evaluator) runs the CPU cached-eval
    /// ([`Self::evaluate_cpu`], unchanged); `Gpu` (and `Auto` with an
    /// evaluator attached) runs the GPU tile evaluator
    /// ([`Self::evaluate_gpu`]). `Gpu` with no evaluator attached is an
    /// honest status-line error, never a silent CPU run.
    pub fn evaluate(&mut self) {
        let use_gpu = match self.eval_backend {
            EvalBackend::Cpu => false,
            EvalBackend::Gpu => true,
            EvalBackend::Auto => self.gpu_evaluator.is_some(),
        };
        if use_gpu {
            self.evaluate_gpu();
        } else {
            self.evaluate_cpu();
        }
    }

    /// The CPU cached-eval path (the reference implementation — behavior
    /// identical to the pre-GPU `evaluate`).
    fn evaluate_cpu(&mut self) {
        if self.graph.nodes.is_empty() {
            self.status = tr("graph.empty");
            self.status_is_error = false;
            self.last_output = None;
            return;
        }
        let Some(output_id) = self.output_node else {
            self.status = tr("graph.no-output-node");
            self.status_is_error = false;
            self.last_output = None;
            return;
        };
        let ctx = EvalContext {
            resolution: self.resolution,
            external: self.eval_externals(),
        };
        match eval_graph_cached(
            &self.graph,
            &self.registry,
            HashMap::new(),
            &ctx,
            &mut self.cache,
            &self.dirty,
        ) {
            Ok(outputs) => {
                self.dirty.clear();
                self.last_output = outputs.get(&output_id).cloned();
                let args = [
                    ("count", FluentValue::from(outputs.len())),
                    ("width", FluentValue::from(self.resolution.0.to_string())),
                    ("height", FluentValue::from(self.resolution.1.to_string())),
                ];
                self.status = tr_args("graph.evaluated", args);
                self.status_is_error = false;
            }
            Err(err) => {
                let args = [("error", FluentValue::from(err.to_string()))];
                self.status = tr_args("graph.eval-failed", args);
                self.status_is_error = true;
            }
        }
    }

    /// The GPU tile-eval path: the dirty subgraph runs as WGSL compute
    /// dispatches for the panel-resolution tile, through the attached
    /// evaluator. Same status-line contract as [`Self::evaluate_cpu`]
    /// (success clears dirty and refreshes the display; failures keep the
    /// previous output); the cache ends identical to a CPU eval, so the
    /// backends are interchangeable frame to frame.
    fn evaluate_gpu(&mut self) {
        if self.graph.nodes.is_empty() {
            self.status = tr("graph.empty");
            self.status_is_error = false;
            self.last_output = None;
            return;
        }
        let Some(output_id) = self.output_node else {
            self.status = tr("graph.no-output-node");
            self.status_is_error = false;
            self.last_output = None;
            return;
        };
        // Taken (not borrowed): the evaluator runs against the panel's
        // other fields and is restored before the status update.
        let Some(mut evaluator) = self.gpu_evaluator.take() else {
            let args = [("error", FluentValue::from("GPU evaluator not attached"))];
            self.status = tr_args("graph.eval-failed", args);
            self.status_is_error = true;
            return;
        };
        let tile = TileSpec::tile_1001(self.resolution.0.max(1), self.resolution.1.max(1));
        let ctx = EvalContext {
            resolution: self.resolution,
            external: self.eval_externals(),
        };
        let result = evaluator.evaluate(
            &self.graph,
            &self.registry,
            HashMap::new(),
            &ctx,
            &mut self.cache,
            &self.dirty,
            &tile,
        );
        self.gpu_evaluator = Some(evaluator);
        match result {
            Ok(outputs) => {
                self.dirty.clear();
                self.last_output = outputs.get(&output_id).cloned();
                let args = [
                    ("count", FluentValue::from(outputs.len())),
                    ("width", FluentValue::from(self.resolution.0.to_string())),
                    ("height", FluentValue::from(self.resolution.1.to_string())),
                ];
                self.status = tr_args("graph.evaluated", args);
                self.status_is_error = false;
            }
            Err(err) => {
                let args = [("error", FluentValue::from(err.to_string()))];
                self.status = tr_args("graph.eval-failed", args);
                self.status_is_error = true;
            }
        }
    }

    /// Whether the output node currently has exportable bytes (the cheap
    /// probe the Export badge reads each frame — no allocation).
    pub fn has_exportable_output(&self) -> bool {
        self.export_output_dims().is_some()
    }

    /// The dims the export bytes would have (`Image` dims, or the panel
    /// resolution for a `Uniform(Color)` fill); `None` when nothing
    /// exportable is cached.
    pub fn export_output_dims(&self) -> Option<(u32, u32)> {
        match self.export_output()? {
            NodeOutput::Image(buf) => Some((buf.width, buf.height)),
            NodeOutput::Uniform(ParamValue::Color(_)) => Some(self.resolution),
            NodeOutput::Uniform(_) => None,
        }
    }

    /// The output node's image as tightly-packed RGBA8 bytes at the
    /// panel resolution: `Image` outputs clone their bytes;
    /// `Uniform(Color)` fills a buffer; anything else (other uniforms,
    /// unevaluated, empty graph) is `None`.
    pub fn export_base_color(&self) -> Option<Vec<u8>> {
        match self.export_output()? {
            NodeOutput::Image(buf) => {
                if (buf.width, buf.height) == self.resolution {
                    Some(buf.data.clone())
                } else {
                    None
                }
            }
            NodeOutput::Uniform(ParamValue::Color(rgb)) => {
                let px = [
                    float_to_u8(rgb[0]),
                    float_to_u8(rgb[1]),
                    float_to_u8(rgb[2]),
                    255,
                ];
                let (w, h) = self.resolution;
                let mut out = Vec::with_capacity(w as usize * h as usize * 4);
                for _ in 0..(w as usize * h as usize) {
                    out.extend_from_slice(&px);
                }
                Some(out)
            }
            NodeOutput::Uniform(_) => None,
        }
    }

    /// The cached value of the output node, if evaluated.
    fn export_output(&self) -> Option<&NodeOutput> {
        let id = self.output_node?;
        self.cache.get(id)
    }

    /// Marks every node dirty (structural edits, resolution changes).
    fn mark_all_dirty(&mut self) {
        self.dirty = self.graph.nodes.iter().map(|n| n.id).collect();
    }

    /// Draws the panel: the Canvas/List toggle, the chosen view (noodle
    /// canvas or v1 node list), the Add-Node combo, the selected node's
    /// param editors, edge wiring, output marker, Evaluate + status, and
    /// the output image.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, GraphView::Canvas, tr("graph.view-canvas"));
            ui.selectable_value(&mut self.view, GraphView::List, tr("graph.view-list"));
        });
        match self.view {
            GraphView::Canvas => {
                crate::graph_canvas::canvas_ui(ui, self);
                ui.weak(tr("graph.canvas-help"));
            }
            GraphView::List => self.node_list(ui),
        }
        self.add_remove_row(ui);
        self.history_row(ui);
        if let Some(line) = self.plugin_status() {
            ui.weak(line);
        }
        self.pending_plugins_ui(ui);
        self.editor_section(ui);

        ui.add_space(4.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("button.evaluate")).clicked() {
                self.evaluate();
            }
            let node = self
                .output_node
                .map(|id| id.to_string())
                .unwrap_or_else(|| tr("common.none"));
            ui.label(tr_args("graph.output", [("node", FluentValue::from(node))]));
        });
        let status = self.status.clone();
        if self.status_is_error {
            ui.label(egui::RichText::new(status).color(ui.visuals().error_fg_color));
        } else {
            ui.label(status);
        }

        ui.add_space(4.0);
        self.output_image(ui);
    }

    /// The v1 node list (the List view).
    fn node_list(&mut self, ui: &mut egui::Ui) {
        ui.strong(tr("graph.nodes"));
        let mut ids: Vec<u64> = self.graph.nodes.iter().map(|n| n.id).collect();
        ids.sort_unstable();
        egui::ScrollArea::vertical()
            .max_height(140.0)
            .show(ui, |ui| {
                for id in &ids {
                    let def = self
                        .graph
                        .nodes
                        .iter()
                        .find(|n| n.id == *id)
                        .map(|n| n.node_def.clone())
                        .unwrap_or_default();
                    let marker = if self.output_node == Some(*id) {
                        " ●"
                    } else {
                        ""
                    };
                    let label = format!("{id}: {def}{marker}");
                    if ui
                        .selectable_label(self.selected == Some(*id), label)
                        .clicked()
                    {
                        self.selected = Some(*id);
                    }
                }
                if ids.is_empty() {
                    ui.weak(tr("graph.no-nodes"));
                }
            });
    }

    /// The Add-Node combo + Remove-selected row (both views).
    fn add_remove_row(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let defs = self.registry.node_defs();
            egui::ComboBox::from_label(tr("graph.add-node"))
                .selected_text(&self.add_def)
                .show_ui(ui, |ui| {
                    for def in &defs {
                        ui.selectable_value(&mut self.add_def, (*def).to_string(), *def);
                    }
                });
            if ui.button(tr("button.add")).clicked() {
                let id = self.add_node(&self.add_def.clone());
                self.selected = Some(id);
            }
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new(tr("graph.remove-selected")),
                )
                .clicked()
            {
                if let Some(id) = self.selected {
                    self.remove_node(id);
                }
            }
        });
    }

    /// The selected node's editor section — the v1 editor verbatim, shared
    /// by both views (the canvas selects, this edits).
    fn editor_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.separator();
        if let Some(id) = self.selected {
            self.selected_editor(ui, id);
        } else {
            ui.weak(tr("graph.select-node"));
        }
    }

    /// Param editors + edge wiring + output marker for the selected node.
    fn selected_editor(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(pos) = self.graph.nodes.iter().position(|n| n.id == id) else {
            self.selected = None;
            return;
        };
        let def = self.graph.nodes[pos].node_def.clone();
        ui.horizontal_wrapped(|ui| {
            let args = [
                ("id", FluentValue::from(id.to_string())),
                ("def", FluentValue::from(def.as_str())),
            ];
            ui.strong(tr_args("graph.node-title", args));
            if self.output_node != Some(id) && ui.small_button(tr("graph.set-output")).clicked() {
                self.set_output_node(id);
            }
        });
        if def == "synth" {
            ui.weak(tr("synth.help"));
        }

        // Snapshot params so UI edits below can't borrow-conflict with
        // the write-back (E0502): each editor produces an optional
        // replacement routed through set_param (which marks dirty).
        let params = self.graph.nodes[pos].params.clone();
        // A `curves` node's four points draw as ONE curve editor, at the
        // first curve param's slot (see the module docs); every other
        // param keeps its row.
        let curve = curve_points(&def, &params);
        let mut curve_drawn = false;
        for (name, value) in &params {
            if curve.is_some() && CURVE_PARAMS.contains(&name.as_str()) {
                if let (false, Some(stored)) = (curve_drawn, curve.as_ref()) {
                    curve_drawn = true;
                    if let Some(next) = curves_editor(ui, id, stored) {
                        for (point_name, point) in CURVE_PARAMS.iter().zip(next) {
                            self.set_param(id, point_name, ParamValue::Vec2(point));
                        }
                    }
                }
                continue;
            }
            let replacement = param_editor(ui, name, value, self);
            if let Some(new_value) = replacement {
                self.set_param(id, name, new_value);
            }
        }

        ui.add_space(4.0);
        ui.strong(tr("graph.edges"));
        for (from, input) in self
            .graph
            .edges
            .iter()
            .filter(|e| e.to == id)
            .map(|e| (e.from, e.input.clone()))
            .collect::<Vec<_>>()
        {
            ui.horizontal(|ui| {
                let args = [
                    ("input", FluentValue::from(input.as_str())),
                    ("from", FluentValue::from(from.to_string())),
                ];
                ui.label(tr_args("graph.edge", args));
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("graph.edge-input"));
            ui.add(
                egui::TextEdit::singleline(&mut self.edge_input)
                    .hint_text("in")
                    .desired_width(80.0),
            );
            let others: Vec<u64> = self
                .graph
                .nodes
                .iter()
                .map(|n| n.id)
                .filter(|nid| *nid != id)
                .collect();
            if self.edge_from.is_none() {
                self.edge_from = others.first().copied();
            }
            let from_label = self
                .edge_from
                .map(|f| f.to_string())
                .unwrap_or_else(|| tr("common.none"));
            egui::ComboBox::from_label(tr("graph.edge-from"))
                .selected_text(from_label)
                .show_ui(ui, |ui| {
                    for candidate in &others {
                        ui.selectable_value(
                            &mut self.edge_from,
                            Some(*candidate),
                            candidate.to_string(),
                        );
                    }
                });
            if ui.button(tr("graph.add-edge")).clicked() {
                if let Some(from) = self.edge_from {
                    let input = self.edge_input.clone();
                    let input = if input.trim().is_empty() {
                        "in".to_string()
                    } else {
                        input
                    };
                    self.add_edge(from, id, &input);
                }
            }
        });
    }

    /// The output image display: the evaluated `Image` as an egui
    /// texture (re-uploaded only when the bytes change), a swatch line
    /// for `Uniform` values, or a hint when nothing evaluated yet.
    fn output_image(&mut self, ui: &mut egui::Ui) {
        let Some(output) = self.last_output.clone() else {
            ui.weak(tr("graph.no-output"));
            return;
        };
        match output {
            NodeOutput::Image(buf) => {
                let key = (buf.width, buf.height, content_hash(&buf.data));
                if self.texture_key != Some(key) {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [buf.width as usize, buf.height as usize],
                        &buf.data,
                    );
                    self.texture = Some(ui.ctx().load_texture(
                        "graph-output",
                        image,
                        Default::default(),
                    ));
                    self.texture_key = Some(key);
                }
                if let Some(texture) = &self.texture {
                    let max_side = 256.0;
                    let (w, h) = (buf.width as f32, buf.height as f32);
                    let scale = (max_side / w.max(h)).min(1.0);
                    ui.image((texture.id(), egui::vec2(w * scale, h * scale)));
                }
            }
            NodeOutput::Uniform(value) => {
                let args = [("value", FluentValue::from(format!("{value:?}")))];
                ui.label(tr_args("graph.uniform-output", args));
            }
        }
    }
}

impl Default for GraphPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// One param editor row. Returns the replacement value when the user
/// edited it this frame (`None` = untouched). `panel` is threaded
/// through read-only for the `NodeRef` combo's node listing.
fn param_editor(
    ui: &mut egui::Ui,
    name: &str,
    value: &ParamValue,
    panel: &GraphPanel,
) -> Option<ParamValue> {
    match value {
        ParamValue::Float(v) => {
            let mut edit = *v;
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::DragValue::new(&mut edit).speed(0.01));
            });
            (edit != *v).then_some(ParamValue::Float(edit))
        }
        ParamValue::Int(v) => {
            let mut edit = *v;
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::DragValue::new(&mut edit));
            });
            (edit != *v).then_some(ParamValue::Int(edit))
        }
        ParamValue::Vec2(v) => {
            let mut edit = *v;
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::DragValue::new(&mut edit[0]).speed(0.01));
                ui.add(egui::DragValue::new(&mut edit[1]).speed(0.01));
            });
            (edit != *v).then_some(ParamValue::Vec2(edit))
        }
        ParamValue::Vec3(v) => {
            let mut edit = *v;
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::DragValue::new(&mut edit[0]).speed(0.01));
                ui.add(egui::DragValue::new(&mut edit[1]).speed(0.01));
                ui.add(egui::DragValue::new(&mut edit[2]).speed(0.01));
            });
            (edit != *v).then_some(ParamValue::Vec3(edit))
        }
        ParamValue::Color(v) => {
            let mut edit = *v;
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::DragValue::new(&mut edit[0]).speed(0.01));
                ui.add(egui::DragValue::new(&mut edit[1]).speed(0.01));
                ui.add(egui::DragValue::new(&mut edit[2]).speed(0.01));
            });
            (edit != *v).then_some(ParamValue::Color(edit))
        }
        ParamValue::Bool(v) => {
            let mut edit = *v;
            ui.checkbox(&mut edit, name);
            (edit != *v).then_some(ParamValue::Bool(edit))
        }
        ParamValue::Asset(v) => {
            let mut edit = v.clone();
            ui.horizontal(|ui| {
                ui.label(name);
                ui.add(egui::TextEdit::singleline(&mut edit).desired_width(160.0));
            });
            (edit != *v).then_some(ParamValue::Asset(edit))
        }
        ParamValue::NodeRef(v) => {
            let mut edit = *v;
            let others: Vec<u64> = panel.graph.nodes.iter().map(|n| n.id).collect();
            egui::ComboBox::from_label(name)
                .selected_text(edit.to_string())
                .show_ui(ui, |ui| {
                    for candidate in &others {
                        ui.selectable_value(&mut edit, *candidate, candidate.to_string());
                    }
                });
            (edit != *v).then_some(ParamValue::NodeRef(edit))
        }
        // The anchor picker: the layer combo lists ONLY the active texture
        // set's layers (see `set_anchor_layers`), so a graph input binds
        // within its set by construction. Layer and channel edit
        // independently (one write-back per frame).
        ParamValue::Anchor(a) => {
            let mut result = None;
            ui.horizontal(|ui| {
                ui.label(name);
                if panel.anchor_layers.is_empty() {
                    ui.weak(tr("graph.anchor-none"));
                } else {
                    let current_label = panel
                        .anchor_layers
                        .iter()
                        .find(|(id, _)| *id == a.layer_id)
                        .map(|(id, lname)| format!("{id}: {lname}"))
                        .unwrap_or_else(|| a.layer_id.to_string());
                    let mut layer = a.layer_id;
                    egui::ComboBox::from_label(tr("graph.anchor-layer"))
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for (id, lname) in &panel.anchor_layers {
                                ui.selectable_value(&mut layer, *id, format!("{id}: {lname}"));
                            }
                        });
                    if layer != a.layer_id {
                        result = Some(ParamValue::Anchor(AnchorRef::new(layer, a.channel.clone())));
                    }
                }
                ui.label(tr("graph.anchor-channel"));
                let mut channel = a.channel.clone();
                ui.add(egui::TextEdit::singleline(&mut channel).desired_width(90.0));
                if result.is_none() && !channel.trim().is_empty() && channel != a.channel {
                    result = Some(ParamValue::Anchor(AnchorRef::new(a.layer_id, channel)));
                }
            });
            result
        }
    }
}

/// The `curves` node's point params, in order (`curve_p0..curve_p3`, the
/// contract shared by umber-graph's `CurvesNode` and umber-gpu's
/// `GpuOp::Curves`).
const CURVE_PARAMS: [&str; 4] = ["curve_p0", "curve_p1", "curve_p2", "curve_p3"];

/// The identity curve a fresh `curves` node starts with: the points
/// umber-graph's `CurvesNode` docs pin as byte-exact on the input.
const CURVE_IDENTITY: [[f32; 2]; 4] = [
    [0.0, 0.0],
    [1.0 / 3.0, 1.0 / 3.0],
    [2.0 / 3.0, 2.0 / 3.0],
    [1.0, 1.0],
];

/// The four stored points of a `curves` node, or `None` when `def` is not
/// `curves` or any of the four params is missing or not a `Vec2` (those
/// rows then stay raw editors, and eval reports the honest error).
/// First match per name, the lookup both evaluators use.
fn curve_points(def: &str, params: &[(String, ParamValue)]) -> Option<[[f32; 2]; 4]> {
    if def != "curves" {
        return None;
    }
    let mut points = [[0.0; 2]; 4];
    for (point, name) in points.iter_mut().zip(CURVE_PARAMS) {
        match params.iter().find(|(n, _)| n == name).map(|(_, v)| v) {
            Some(ParamValue::Vec2(v)) => *point = *v,
            _ => return None,
        }
    }
    Some(points)
}

/// The `curves` node's editor: [`umber_gui::CurveEdit`] over the four
/// points (fixed count: the node has exactly four), under a per-node id.
/// Returns the four points to write back, only when an edit changed the
/// stored curve.
fn curves_editor(ui: &mut egui::Ui, id: u64, stored: &[[f32; 2]; 4]) -> Option<[[f32; 2]; 4]> {
    let (shown, fallback) = curve_edit_model(stored);
    let mut model = shown.clone();
    let response = ui
        .push_id(("graph_curves_curve", id), |ui| {
            umber_gui::CurveEdit::new(&mut model)
                .size(160.0)
                .fixed_count(true)
                .show(ui)
        })
        .inner;
    if !response.changed() {
        return None;
    }
    curve_write_back(stored, &shown, fallback, &model)
}

/// The editor model for `stored`, rebuilt every frame (the node params
/// are the state), and whether it is the identity fallback. The
/// brush_panel `falloff_edit_model` approach: [`umber_gui::Curve`] is
/// stricter than the eval contract (points inside `[0, 1]²`, x gaps of at
/// least `MIN_GAP`), so the points are shown clamped into `[0, 1]²` and
/// each x nudged to at least `MIN_GAP + 1e-6` past its left neighbour
/// (the f32-ulp margin: `move_point` parks a point exactly `MIN_GAP` from
/// its neighbour, which can fall an ulp short on the next frame's
/// `Curve::new`). A curve still refused (non-finite, or the nudge pushed
/// the last x past 1) shows as [`CURVE_IDENTITY`] with `fallback` set.
fn curve_edit_model(stored: &[[f32; 2]; 4]) -> (umber_gui::Curve, bool) {
    use umber_gui::widgets::curve::MIN_GAP;
    let unit = |v: f32| v.clamp(0.0, 1.0);
    let mut points = stored.map(|[x, y]| (unit(x), unit(y)));
    for i in 1..points.len() {
        points[i].0 = points[i].0.max(points[i - 1].0 + MIN_GAP + 1e-6);
    }
    match umber_gui::Curve::new(points.to_vec()) {
        Ok(curve) => (curve, false),
        Err(_) => {
            let identity = CURVE_IDENTITY.map(|[x, y]| (x, y)).to_vec();
            (umber_gui::Curve::new(identity).unwrap_or_default(), true)
        }
    }
}

/// The eval contract as a gate (graph_eval `GpuOp::Curves` and
/// umber-graph `Curve4::try_new`): every coordinate finite, x strictly
/// increasing. Nothing else: y and x are NOT range-limited there, so
/// they are not here. `None` = refused.
fn sanitize_curve(points: [[f32; 2]; 4]) -> Option<[[f32; 2]; 4]> {
    let finite = points.iter().flatten().all(|c| c.is_finite());
    let increasing = points.windows(2).all(|w| w[0][0] < w[1][0]);
    (finite && increasing).then_some(points)
}

/// What an edit writes back: only the points the editor moved (`edited`
/// differs from `shown`) take the editor's value; the rest keep their
/// STORED value, so the display clamp and nudge never leak into the
/// graph. The merge goes through [`sanitize_curve`]; when it is refused
/// (a stored curve that broke the contract), or the model was the
/// identity fallback (its points are not the stored ones), the whole
/// edited curve is written instead (always valid: the editor keeps
/// `MIN_GAP`). `None` when the result equals `stored`.
fn curve_write_back(
    stored: &[[f32; 2]; 4],
    shown: &umber_gui::Curve,
    fallback: bool,
    edited: &umber_gui::Curve,
) -> Option<[[f32; 2]; 4]> {
    let four = |curve: &umber_gui::Curve| -> Option<[[f32; 2]; 4]> {
        <[(f32, f32); 4]>::try_from(curve.points())
            .ok()
            .map(|p| p.map(|(x, y)| [x, y]))
    };
    let (shown, edited) = (four(shown)?, four(edited)?);
    if edited == shown {
        // No edit (an untouched re-render, or a drag back to the start):
        // even the identity fallback stays unwritten.
        return None;
    }
    let merged = if fallback {
        None
    } else {
        let mut merged = *stored;
        for ((slot, edit), seen) in merged.iter_mut().zip(edited).zip(shown) {
            if edit != seen {
                *slot = edit;
            }
        }
        sanitize_curve(merged)
    };
    let next = merged.or_else(|| sanitize_curve(edited))?;
    (next != *stored).then_some(next)
}

/// Lifts every carried [`CANVAS_POS_PARAM`] out of the node params into
/// [`Node::canvas`] (the load half of the mtlx carry). The param is always
/// removed; only a finite `Vec2` becomes a position — anything else
/// (a foreign-typed or NaN value) leaves the node unplaced (the scatter).
fn lift_canvas_positions(graph: &mut Graph) {
    for node in &mut graph.nodes {
        let mut lifted = None;
        node.params.retain(|(name, value)| {
            if name != CANVAS_POS_PARAM {
                return true;
            }
            if let ParamValue::Vec2(pos) = value {
                if pos.iter().all(|c| c.is_finite()) {
                    lifted = Some(*pos);
                }
            }
            false
        });
        if lifted.is_some() {
            node.canvas = lifted;
        }
    }
}

/// The v1 mesh-map externals: flat stand-ins at the panel resolution
/// (see module docs — the bake bridge does not expose live in-memory
/// bakes yet). AO-flat is opaque white (unoccluded); Normal-flat is
/// tangent-space up, matching `bake_sources::flat_normal_rgba8`.
fn mesh_map_externals(resolution: (u32, u32)) -> HashMap<String, NodeOutput> {
    let (w, h) = (resolution.0.max(1), resolution.1.max(1));
    let texels = w as usize * h as usize;
    let mut externals = HashMap::new();
    if let Ok(ao) = umber_graph::ImageBuffer::filled(w, h, [255, 255, 255, 255]) {
        externals.insert("mesh_map:AO".to_string(), NodeOutput::Image(ao));
    }
    let mut normal = Vec::with_capacity(texels * 4);
    for _ in 0..texels {
        normal.extend_from_slice(&[128, 128, 255, 255]);
    }
    if let Ok(buf) = umber_graph::ImageBuffer::new(w, h, normal) {
        externals.insert("mesh_map:Normal".to_string(), NodeOutput::Image(buf));
    }
    externals
}

/// The engine's float→u8 quantizer, restated (umber-graph keeps the
/// function private; the export path pins the same math).
fn float_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// FNV-1a64 over the bytes (the texture re-upload key — std-only,
/// no new deps).
fn content_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8482_2225;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {

    /// kittest (C2-28 + C2-21): an untrusted plugin shows its Trust & load
    /// row; clicking it registers the node, persists the hash to the
    /// (temp) trust file, and removes the row.
    #[test]
    fn trust_and_load_button_registers_the_pending_plugin() {
        use egui_kittest::kittest::Queryable;
        crate::i18n::install(crate::i18n::I18n::builtin());
        let dir = std::env::temp_dir().join(format!("umber-graph-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        std::fs::copy(
            crate::plugins::dev_plugin_dir().join("plugin_vignette.wasm"),
            dir.join("plugins").join("vignette.wasm"),
        )
        .unwrap();
        let trust_file = dir.join("plugins-trusted.txt");
        let mut panel = GraphPanel::new();
        let report = panel.load_plugins_with_trust(
            &[dir.join("plugins")],
            plugins::TrustStore::open(Some(trust_file.clone())),
        );
        assert_eq!(report.pending.len(), 1, "untrusted: listed, not loaded");
        assert!(panel.registry().get("vignette").is_none());

        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 900.0))
            .build_ui_state(|ui, panel: &mut GraphPanel| panel.show(ui), panel);
        harness.run();
        harness.get_by_label("Trust & load").click();
        harness.run();
        let panel = harness.state();
        assert!(
            panel.registry().get("vignette").is_some(),
            "registered: {}",
            panel.status()
        );
        assert!(panel.plugin_pending().is_empty(), "the row is gone");
        let saved = std::fs::read_to_string(&trust_file).unwrap();
        assert!(saved.contains("vignette.wasm"), "trust persisted: {saved}");
        assert!(harness.query_by_label("Trust & load").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use std::collections::HashMap;

    /// A noise→passthrough graph built through the panel's own pub API.
    fn noise_chain(panel: &mut GraphPanel) -> (u64, u64) {
        let head = panel.add_node("noise_perlin");
        assert!(panel.set_param(head, "scale", ParamValue::Float(4.0)));
        assert!(panel.set_param(head, "seed", ParamValue::Int(5)));
        let tail = panel.add_node("passthrough");
        assert!(panel.add_edge(head, tail, "in"));
        assert!(panel.set_output_node(tail));
        (head, tail)
    }

    #[test]
    fn panel_registry_shares_the_material_nodes() {
        // The delegation contract: the panel builds the shared registry, so
        // a definition containing `material_output` opens AND evaluates
        // here (not just in the headless driver).
        let mut panel = GraphPanel::new();
        assert!(
            panel.registry().node_defs().contains(&"material_output"),
            "panel registry must dispatch material_output"
        );
        let head = panel.add_node("uniform");
        assert!(panel.set_param(head, "color", ParamValue::Color([0.0, 0.0, 1.0])));
        let tail = panel.add_node("material_output");
        assert!(panel.set_param(tail, "channel", ParamValue::Asset("baseColor".into())));
        assert!(panel.add_edge(head, tail, "input"));
        assert!(panel.set_output_node(tail));
        panel.set_resolution((2, 2));
        panel.evaluate();
        assert!(
            !panel.status().starts_with("Eval failed"),
            "material chain must evaluate in the panel, got {:?}",
            panel.status()
        );
        match panel.last_output().expect("output evaluated") {
            NodeOutput::Image(buf) => {
                assert!(buf.data.chunks_exact(4).all(|px| px == [0, 0, 255, 255]));
            }
            other => panic!("expected an Image, got {other:?}"),
        }
    }

    #[test]
    fn panel_eval_matches_direct_engine_eval_byte_exact() {
        // THE CAN-FAIL CORE: the panel's cached eval must equal a direct
        // engine eval of the same graph — any panel-side input mangling
        // (wrong resolution, dropped externals, cache staleness) breaks
        // byte equality here.
        let mut panel = GraphPanel::new();
        panel.set_resolution((8, 8));
        let (_head, tail) = noise_chain(&mut panel);
        panel.evaluate();
        assert!(
            !panel.status().starts_with("Eval failed"),
            "eval must succeed, got {:?}",
            panel.status()
        );
        let shown = panel.last_output().expect("output node evaluated").clone();

        let ctx = EvalContext {
            resolution: (8, 8),
            external: mesh_map_externals((8, 8)),
        };
        let direct = umber_graph::eval_graph(panel.graph(), panel.registry(), HashMap::new(), &ctx)
            .expect("direct engine eval works");
        assert_eq!(
            shown, direct[&tail],
            "panel display must equal the direct engine eval"
        );
        assert!(
            matches!(shown, NodeOutput::Image(_)),
            "noise chain must produce an Image, got {shown:?}"
        );
    }

    #[test]
    fn param_edit_dirties_and_changes_noise_output() {
        let mut panel = GraphPanel::new();
        panel.set_resolution((8, 8));
        let (head, _tail) = noise_chain(&mut panel);
        panel.evaluate();
        let before = panel.last_output().cloned().expect("evaluated");

        // Editing a param marks that node dirty…
        assert!(panel.dirty_set().is_empty(), "eval must clear dirty");
        assert!(panel.set_param(head, "scale", ParamValue::Float(4.0)));
        assert!(
            panel.dirty_set().contains(&head),
            "edited node must be dirty"
        );
        // …and a scale change MUST change noise bytes (same value is a
        // no-op write; a new scale re-renders).
        assert!(panel.set_param(head, "scale", ParamValue::Float(9.0)));
        panel.evaluate();
        let after = panel.last_output().cloned().expect("re-evaluated");
        assert_ne!(
            before, after,
            "noise scale 4.0 → 9.0 must change the output bytes"
        );
    }

    #[test]
    fn export_base_color_fills_uniform_color_exactly_and_empty_is_none() {
        let mut panel = GraphPanel::new();
        assert_eq!(
            panel.export_base_color(),
            None,
            "empty graph exports nothing"
        );

        let id = panel.add_node("uniform");
        assert!(panel.set_param(id, "color", ParamValue::Color([1.0, 0.0, 0.25])));
        assert!(panel.set_output_node(id));
        panel.evaluate();
        assert!(
            !panel.status().starts_with("Eval failed"),
            "uniform fill must evaluate, got {:?}",
            panel.status()
        );
        let bytes = panel
            .export_base_color()
            .expect("uniform color graph exports");
        let (w, h) = panel.resolution();
        assert_eq!(bytes.len(), w as usize * h as usize * 4);
        // [1.0, 0.0, 0.25] → [255, 0, 64, 255] under the pinned quantizer.
        for texel in bytes.chunks_exact(4) {
            assert_eq!(texel, [255, 0, 64, 255]);
        }
    }

    #[test]
    fn project_round_trip_preserves_graph_nodes_and_edges() {
        // graph → mtlx string → project dir → back → PartialEq on the
        // rebuilt graph (the additive mtlx-string carry path).
        let mut panel = GraphPanel::new();
        let (_head, _tail) = noise_chain(&mut panel);
        let doc = panel.to_mtlx();
        assert!(!doc.is_empty());

        let model = umber_core::project::ProjectModel::new(vec![], vec![], Default::default())
            .with_graphs_mtlx(vec![doc]);
        let dir = std::env::temp_dir().join(format!(
            "umber-graph-panel-roundtrip-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        umber_core::project::save_to_dir(&model, &dir).expect("save works");
        let back = umber_core::project::load_from_dir(&dir).expect("old-shape load works");
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.graphs_mtlx.len(), 1);

        let (graph, _, _) =
            umber_graph::mtlx::from_mtlx(&back.graphs_mtlx[0]).expect("mtlx parses");
        assert_eq!(
            graph,
            *panel.graph(),
            "nodes AND edges must survive the project round-trip"
        );
    }

    /// Saves `doc` through the real project dir path and reads it back.
    fn project_round_trip(doc: String, tag: &str) -> String {
        let model = umber_core::project::ProjectModel::new(vec![], vec![], Default::default())
            .with_graphs_mtlx(vec![doc]);
        let dir =
            std::env::temp_dir().join(format!("umber-graph-panel-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        umber_core::project::save_to_dir(&model, &dir).expect("save works");
        let back = umber_core::project::load_from_dir(&dir).expect("load works");
        std::fs::remove_dir_all(&dir).ok();
        back.graphs_mtlx.into_iter().next().expect("one graph")
    }

    #[test]
    fn canvas_positions_survive_project_save_load_exactly() {
        let mut saved = GraphPanel::new();
        let (head, tail) = noise_chain(&mut saved);
        // Awkward floats: negative, fractional, large, tiny.
        assert!(saved.set_node_canvas(head, [-123.456, 0.1]));
        assert!(saved.set_node_canvas(tail, [98765.43, -0.000_123]));
        let placed = saved.add_node_at("mix", [3.0e-7, 1.0e6 + 0.5]);
        let unplaced = saved.add_node("blur");
        assert!(saved.connect(tail, placed, "fg"));

        let doc = project_round_trip(saved.to_mtlx(), "canvas-roundtrip");
        assert!(doc.contains(CANVAS_POS_PARAM), "positions ride the mtlx");

        let mut loaded = GraphPanel::new();
        assert!(loaded.load_mtlx(&doc), "status: {}", loaded.status());
        assert_eq!(
            loaded.graph(),
            saved.graph(),
            "load == saved: params, edges AND canvas positions"
        );
        let canvas_of = |p: &GraphPanel, id: u64| {
            p.graph()
                .nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.canvas)
        };
        assert_eq!(canvas_of(&loaded, head), Some([-123.456, 0.1]));
        assert_eq!(canvas_of(&loaded, tail), Some([98765.43, -0.000_123]));
        assert_eq!(canvas_of(&loaded, placed), Some([3.0e-7, 1.0e6 + 0.5]));
        assert_eq!(canvas_of(&loaded, unplaced), None, "unplaced stays None");
        // The carry param never leaks into the live params (the editor).
        assert!(loaded
            .graph()
            .nodes
            .iter()
            .all(|n| n.params.iter().all(|(name, _)| name != CANVAS_POS_PARAM)));
        // Save → load → save is byte-stable (no duplicate carry params).
        assert_eq!(loaded.to_mtlx(), saved.to_mtlx());
    }

    #[test]
    fn pre_canvas_files_load_unplaced_onto_the_scatter() {
        // An old-format document: no __canvas_pos anywhere.
        let mut old = GraphPanel::new();
        let (head, tail) = noise_chain(&mut old);
        let doc = old.to_mtlx();
        assert!(!doc.contains(CANVAS_POS_PARAM), "unplaced nodes emit as v1");

        let mut loaded = GraphPanel::new();
        assert!(loaded.load_mtlx(&project_round_trip(doc, "canvas-old")));
        for id in [head, tail] {
            let node = loaded
                .graph()
                .nodes
                .iter()
                .find(|n| n.id == id)
                .expect("node survives");
            assert_eq!(node.canvas, None);
            assert_eq!(
                crate::graph_canvas::node_rect(id, node.canvas).min,
                crate::graph_canvas::default_position(id),
                "old files land on the deterministic scatter"
            );
        }
    }

    #[test]
    fn non_vec2_or_nan_carry_is_dropped_not_placed() {
        let doc = r#"<?xml version="1.0" encoding="UTF-8"?>
<materialx version="1.38">
  <node name="N1" type="uniform">
    <input name="__canvas_pos" type="string" value="garbage" />
  </node>
  <node name="N2" type="uniform">
    <input name="__canvas_pos" type="vector2" value="NaN, 4" />
  </node>
</materialx>
"#;
        let mut panel = GraphPanel::new();
        assert!(panel.load_mtlx(doc), "status: {}", panel.status());
        for node in &panel.graph().nodes {
            assert_eq!(node.canvas, None, "node {} must stay unplaced", node.id);
            assert!(node.params.is_empty(), "carry param stripped: {node:?}");
        }
    }

    #[test]
    fn moving_a_node_dirties_nothing_and_connect_rewires_the_input() {
        let mut panel = GraphPanel::new();
        panel.set_resolution((4, 4));
        let a = panel.add_node("uniform");
        let b = panel.add_node("uniform");
        let p = panel.add_node("passthrough");
        assert!(panel.connect(a, p, "in"));
        panel.evaluate();
        assert!(panel.dirty_set().is_empty(), "status: {}", panel.status());

        assert!(panel.set_node_canvas(a, [10.0, 20.0]));
        assert!(panel.dirty_set().is_empty(), "position is editor-only");
        assert!(!panel.set_node_canvas(999, [0.0, 0.0]));

        assert!(panel.connect(b, p, "in"), "re-wiring an input");
        assert!(
            panel.dirty_set().contains(&p),
            "same dirty path as add_edge"
        );
        let feeding: Vec<u64> = panel
            .graph()
            .edges
            .iter()
            .filter(|e| e.to == p && e.input == "in")
            .map(|e| e.from)
            .collect();
        assert_eq!(feeding, [b], "one edge per input: the new one replaces");
        assert!(!panel.connect(p, p, "in"), "no self-loops");
        assert!(!panel.connect(a, 999, "in"));

        panel.evaluate();
        assert!(panel.dirty_set().is_empty(), "status: {}", panel.status());
        assert!(panel.remove_edge(p, "in"));
        assert!(panel.dirty_set().contains(&p));
        assert!(!panel.remove_edge(p, "in"), "already gone");
    }

    #[test]
    fn registry_listing_covers_all_merged_families() {
        // 3 engine seeds + 7 slice-2a + 8 slice-2b + 6 slice-2c + 1 anchor
        // + 1 synth (post-v1) + 1 material_output (the dynmat channel
        // contract, Kill A) = 27.
        let panel = GraphPanel::new();
        let defs = panel.registry().node_defs();
        assert_eq!(defs.len(), 27, "unexpected registry size: {defs:?}");
        for expected in [
            "uniform",
            "image_asset",
            "passthrough",
            "noise_perlin",
            "noise_value",
            "noise_worley",
            "gradient",
            "checkerboard",
            "dots",
            "brick_pattern",
            "blur",
            "sharpen",
            "levels",
            "curves",
            "invert",
            "mix",
            "color_correct",
            "hsv_adjust",
            "flood_fill",
            "edge_detect",
            "histogram_match",
            "direction_warp",
            "triplanar_blend",
            "mesh_map_generator",
            "anchor",
            "synth",
        ] {
            assert!(
                defs.contains(&expected),
                "registry must list {expected:?}, got {defs:?}"
            );
        }
        let mut sorted = defs.clone();
        sorted.sort_unstable();
        assert_eq!(defs, sorted, "listing must be sorted");
    }

    #[test]
    fn anchor_binding_resolves_through_panel_eval() {
        // End-to-end §3:42: a node input bound to a layer anchor evaluates
        // against the bridge-uploaded channel bytes.
        let mut panel = GraphPanel::new();
        panel.set_resolution((2, 2));
        panel.set_anchor_layers(vec![(5, "Base".into())]);
        assert_eq!(
            panel.anchor_layers(),
            &[(5, "Base".to_string())],
            "the picker lists the set's layers"
        );
        let tail = panel.add_node("passthrough");
        assert!(panel.bind_anchor(tail, "in", 5, "baseColor"));
        assert!(!panel.bind_anchor(999, "in", 5, "baseColor"));
        assert!(panel.set_output_node(tail));
        // Unbound (deleted layer / never composited): honest failure, the
        // previous output (none) kept.
        panel.evaluate();
        assert!(
            panel.status().starts_with("Eval failed"),
            "unbound anchor must fail, got {:?}",
            panel.status()
        );
        assert!(panel.last_output().is_none());
        // Bound: the bridge uploads the composited channel; the eval
        // resolves the anchor to exactly those bytes.
        let channel = NodeOutput::Image(
            umber_graph::ImageBuffer::filled(2, 2, [9, 9, 9, 255]).expect("2x2 fills"),
        );
        panel.set_anchor_inputs(HashMap::from([(
            "anchor:5:baseColor".to_string(),
            channel.clone(),
        )]));
        panel.evaluate();
        assert!(
            !panel.status().starts_with("Eval failed"),
            "bound anchor must evaluate, got {:?}",
            panel.status()
        );
        assert_eq!(panel.last_output(), Some(&channel));
    }

    #[test]
    fn structural_edits_keep_output_marker_honest() {
        let mut panel = GraphPanel::new();
        let a = panel.add_node("uniform");
        let b = panel.add_node("passthrough");
        assert_eq!(panel.output_node(), Some(a), "first node adopts output");
        assert!(panel.set_output_node(b));
        assert!(panel.remove_node(b));
        assert_eq!(
            panel.output_node(),
            Some(a),
            "removing the output node falls back to the highest id"
        );
        assert!(!panel.remove_node(999), "unknown remove returns false");
        assert!(!panel.set_output_node(999), "unknown output returns false");
        assert!(!panel.set_param(999, "x", ParamValue::Float(1.0)));
        assert!(!panel.add_edge(a, 999, "in"));
    }

    /// A panel with the repo's example plugins loaded (blur5, infinite,
    /// vignette — git-tracked, so no skip path).
    fn plugin_panel() -> GraphPanel {
        let mut panel = GraphPanel::new();
        let report = panel.load_plugins(&[crate::plugins::dev_plugin_dir()]);
        assert!(report.failed.is_empty(), "examples load: {report:?}");
        panel
    }

    #[test]
    fn panel_registry_lists_and_dispatches_a_wasm_node() {
        let mut panel = plugin_panel();
        let defs = panel.registry().node_defs();
        assert!(defs.contains(&"blur5"), "Add-Node lists blur5: {defs:?}");
        assert!(defs.contains(&"blur"), "built-ins beside it: {defs:?}");
        assert_eq!(panel.plugin_status(), Some("3 plugin(s) loaded, 0 failed"));

        // Small odd raster (catches w/h swaps; keeps the guest cheap).
        panel.set_resolution((33, 17));
        let grad = panel.add_node("gradient");
        panel.set_param(grad, "type", ParamValue::Int(1));
        let wasm = panel.add_node("blur5");
        let native = panel.add_node("blur");
        panel.set_param(native, "radius", ParamValue::Int(2));
        assert!(panel.connect(grad, wasm, "in"));
        assert!(panel.connect(grad, native, "in"));

        assert!(panel.set_output_node(wasm));
        panel.evaluate();
        assert!(
            !panel.status().starts_with("Eval failed"),
            "status: {}",
            panel.status()
        );
        let wasm_out = panel.last_output().cloned().expect("blur5 output");
        assert!(panel.set_output_node(native));
        panel.evaluate();
        let native_out = panel.last_output().cloned().expect("blur output");
        assert!(matches!(wasm_out, NodeOutput::Image(_)));
        assert_eq!(wasm_out, native_out, "panel-dispatched blur5 == blur r2");
    }

    #[test]
    fn wasm_node_saves_as_declared_nodedef_and_round_trips() {
        let mut saved = plugin_panel();
        let grad = saved.add_node("gradient");
        let vig = saved.add_node("vignette");
        saved.set_param(vig, "strength", ParamValue::Float(0.3));
        assert!(saved.connect(grad, vig, "in"));
        let doc = saved.to_mtlx();
        assert!(
            doc.contains("<nodedef name=\"vignette\" node=\"vignette\">"),
            "plugin nodedef declared: {doc}"
        );
        assert!(
            doc.find("<nodedef name=\"vignette\"").unwrap() < doc.find("<node ").unwrap(),
            "declarations precede nodes"
        );

        let (graph, decls, warnings) = umber_graph::mtlx::from_mtlx(&doc).expect("parses");
        assert!(
            warnings.is_empty(),
            "declared type must not warn: {warnings:?}"
        );
        assert!(decls.iter().any(|d| d.name == "vignette"));
        let node = graph.nodes.iter().find(|n| n.id == vig).expect("node kept");
        assert_eq!(node.node_def, "vignette");
        assert!(node
            .params
            .contains(&("strength".to_string(), ParamValue::Float(0.3))));
        assert!(graph
            .edges
            .iter()
            .any(|e| e.from == grad && e.to == vig && e.input == "in"));

        let mut loaded = plugin_panel();
        assert!(loaded.load_mtlx(&doc), "status: {}", loaded.status());
        assert_eq!(loaded.status(), "Graph loaded — press Evaluate.");
        assert_eq!(loaded.to_mtlx(), doc, "save/load/save is byte-stable");
    }

    #[test]
    fn eval_backend_defaults_to_auto_without_an_evaluator() {
        // The pre-GPU behavior is preserved: a fresh panel selects
        // Auto, attaches nothing, and `evaluate` runs the CPU path (the
        // existing CPU tests above prove what that computes — here the
        // routing itself is pinned).
        let panel = GraphPanel::new();
        assert_eq!(panel.eval_backend(), EvalBackend::Auto);
        assert!(!panel.has_gpu_evaluator());
    }

    #[test]
    fn eval_backend_flag_round_trips_and_detaches() {
        let mut panel = GraphPanel::new();
        panel.set_eval_backend(EvalBackend::Cpu);
        assert_eq!(panel.eval_backend(), EvalBackend::Cpu);
        panel.set_eval_backend(EvalBackend::Gpu);
        assert_eq!(panel.eval_backend(), EvalBackend::Gpu);
        // Detaching an already-absent evaluator is a harmless no-op.
        panel.detach_gpu_evaluator();
        assert!(!panel.has_gpu_evaluator());
    }

    #[test]
    fn gpu_backend_without_an_evaluator_is_an_honest_error_not_a_panic() {
        // `Gpu` with nothing attached must NOT silently run the CPU path
        // (a caller asking for GPU must know what it got): the status
        // reports the failure and any previous output is kept.
        let mut panel = GraphPanel::new();
        panel.set_resolution((8, 8));
        let (head, _tail) = noise_chain(&mut panel);
        panel.set_eval_backend(EvalBackend::Cpu);
        panel.evaluate();
        assert!(
            !panel.status().starts_with("Eval failed"),
            "cpu baseline must succeed, got {:?}",
            panel.status()
        );
        let kept = panel.last_output().cloned().expect("cpu evaluated");

        panel.set_eval_backend(EvalBackend::Gpu);
        panel.evaluate();
        assert!(
            panel.status().starts_with("Eval failed"),
            "gpu-without-evaluator must fail loudly, got {:?}",
            panel.status()
        );
        assert_eq!(
            panel.last_output(),
            Some(&kept),
            "previous output survives the failed gpu eval"
        );
        // The failed GPU eval must not clear the dirty set — but the
        // preceding CPU baseline eval legitimately DID (success clears).
        // Re-dirty via a param touch, then assert the failed GPU eval
        // preserves it.
        panel.set_param(head, "scale", ParamValue::Float(5.0));
        assert!(!panel.dirty_set().is_empty(), "param touch re-dirties");
        panel.evaluate();
        assert!(panel.status().starts_with("Eval failed"));
        assert!(
            !panel.dirty_set().is_empty(),
            "failed eval must not clear the dirty set"
        );
    }

    #[test]
    fn auto_without_an_evaluator_matches_the_cpu_reference() {
        // Auto with nothing attached IS the CPU path (byte-identical to
        // an explicit Cpu selection on the same graph).
        let mut auto_panel = GraphPanel::new();
        auto_panel.set_resolution((8, 8));
        let (_head, tail) = noise_chain(&mut auto_panel);
        auto_panel.evaluate();
        let auto_out = auto_panel.last_output().cloned().expect("evaluated");

        let mut cpu_panel = GraphPanel::new();
        cpu_panel.set_resolution((8, 8));
        let (_head2, tail2) = noise_chain(&mut cpu_panel);
        cpu_panel.set_eval_backend(EvalBackend::Cpu);
        cpu_panel.evaluate();
        let cpu_out = cpu_panel.last_output().cloned().expect("evaluated");

        assert_eq!(tail, tail2);
        assert_eq!(auto_out, cpu_out, "auto-without-gpu == explicit cpu");
    }

    #[test]
    fn save_as_material_copies_the_open_graph_under_a_fresh_id() {
        // "Save as material" COPIES the open graph into the project's
        // definitions (later source edits never propagate — non-goal 3),
        // minting `umber.material-<n>` without overwriting.
        let mut panel = GraphPanel::new();
        let head = panel.add_node("uniform");
        assert!(panel.set_param(head, "color", ParamValue::Color([1.0, 0.0, 0.0])));
        let mut defs = BTreeMap::from([("umber.material-1".to_string(), "stale".to_string())]);
        let snapshot = panel.to_mtlx();

        let id = panel
            .save_as_material(&mut defs)
            .expect("a non-empty graph saves");
        assert_eq!(id, "umber.material-2", "occupied ids are skipped");
        assert_eq!(
            defs["umber.material-1"], "stale",
            "an existing entry is never overwritten"
        );
        assert_eq!(
            defs[&id], snapshot,
            "the definition is the open graph's bytes"
        );

        // Later Graph-panel edits to the source graph do not propagate.
        assert!(panel.set_param(head, "color", ParamValue::Color([0.0, 1.0, 0.0])));
        assert_eq!(defs[&id], snapshot, "the copy is frozen at save time");
        assert_ne!(
            panel.to_mtlx(),
            snapshot,
            "the edit really changed the source (else the freeze assert is vacuous)"
        );
    }

    #[test]
    fn save_as_material_on_an_empty_graph_saves_nothing() {
        let mut panel = GraphPanel::new();
        let mut defs = BTreeMap::new();
        assert_eq!(
            panel.save_as_material(&mut defs),
            None,
            "an empty graph has no definition to copy"
        );
        assert!(defs.is_empty(), "the no-op stores nothing");
    }

    /// Four `curve_p*` params in order, as a node's param list.
    fn curve_params(points: [[f32; 2]; 4]) -> Vec<(String, ParamValue)> {
        CURVE_PARAMS
            .iter()
            .zip(points)
            .map(|(name, point)| ((*name).to_string(), ParamValue::Vec2(point)))
            .collect()
    }

    /// (a) A fresh `curves` node carries the identity curve; the model
    /// shows a valid stored curve exactly; moving one point writes back
    /// exactly that point, the other three verbatim.
    #[test]
    fn curves_model_round_trips_and_writes_back_the_moved_point() {
        let mut panel = GraphPanel::new();
        let id = panel.add_node("curves");
        let node = panel.graph.nodes.iter().find(|n| n.id == id).unwrap();
        let fresh = curve_points("curves", &node.params);
        assert_eq!(fresh, Some(CURVE_IDENTITY));

        let stored = [[0.0, 0.0], [0.25, 0.8], [0.6, 0.3], [1.0, 1.0]];
        let (shown, fallback) = curve_edit_model(&stored);
        assert!(!fallback);
        let as_pairs: [(f32, f32); 4] = [(0.0, 0.0), (0.25, 0.8), (0.6, 0.3), (1.0, 1.0)];
        assert_eq!(shown.points(), &as_pairs);
        let mut moved = shown.clone();
        assert!(moved.move_point(1, 0.3, 0.5));
        let written = curve_write_back(&stored, &shown, fallback, &moved);
        let expected = [[0.0, 0.0], [0.3, 0.5], [0.6, 0.3], [1.0, 1.0]];
        assert_eq!(written, Some(expected));

        // Through the panel: the four params land, the node goes dirty.
        panel.dirty.clear();
        for (name, point) in CURVE_PARAMS.iter().zip(expected) {
            assert!(panel.set_param(id, name, ParamValue::Vec2(point)));
        }
        let node = panel.graph.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(curve_points("curves", &node.params), Some(expected));
        assert!(panel.dirty.contains(&id));
    }

    /// (b) The write-back follows the eval contract, not the widget's
    /// range: a stored y outside `[0, 1]` (eval-valid) survives an edit to
    /// another point; the display nudge never reaches the graph; a merge
    /// the contract refuses falls back to the edited curve; an identity
    /// fallback model writes back whole.
    #[test]
    fn curves_write_back_keeps_the_eval_contract() {
        use umber_gui::widgets::curve::MIN_GAP;

        // Out-of-range y: shown clamped, kept on an edit elsewhere.
        let stored = [[0.0, -1.0], [0.3, 0.5], [0.6, 0.5], [1.0, 2.0]];
        let (shown, fallback) = curve_edit_model(&stored);
        assert!(!fallback);
        let clamped: [(f32, f32); 4] = [(0.0, 0.0), (0.3, 0.5), (0.6, 0.5), (1.0, 1.0)];
        assert_eq!(shown.points(), &clamped);
        let mut moved = shown.clone();
        assert!(moved.move_point(1, 0.4, 0.7));
        assert_eq!(
            curve_write_back(&stored, &shown, fallback, &moved),
            Some([[0.0, -1.0], [0.4, 0.7], [0.6, 0.5], [1.0, 2.0]])
        );

        // A gap of exactly MIN_GAP is nudged ~1e-6 for display only; an
        // edit to p0 writes p2's STORED x back.
        let tight_x = 0.5 + MIN_GAP;
        let stored = [[0.0, 0.0], [0.5, 0.2], [tight_x, 0.9], [1.0, 1.0]];
        let (shown, fallback) = curve_edit_model(&stored);
        assert!(!fallback);
        assert!(shown.points()[2].0 > tight_x, "display nudged");
        let mut moved = shown.clone();
        assert!(moved.move_point(0, 0.0, 0.3));
        assert_eq!(
            curve_write_back(&stored, &shown, fallback, &moved),
            Some([[0.0, 0.3], [0.5, 0.2], [tight_x, 0.9], [1.0, 1.0]])
        );

        // A stored non-monotone curve (p1.x == p2.x, eval refuses it):
        // shown nudged, an edit to p3 would keep the tie, so the merge is
        // refused and the whole edited curve lands (valid by the editor).
        let stored = [[0.0, 0.0], [0.3, 0.2], [0.3, 0.6], [1.0, 1.0]];
        assert_eq!(sanitize_curve(stored), None);
        let (shown, fallback) = curve_edit_model(&stored);
        assert!(!fallback);
        let mut moved = shown.clone();
        assert!(moved.move_point(3, 1.0, 0.5));
        let written = curve_write_back(&stored, &shown, fallback, &moved);
        let next = written.expect("the edited curve lands");
        assert_eq!(next[0], [0.0, 0.0]);
        assert_eq!(next[1], [0.3, 0.2]);
        assert_eq!(next[2], [shown.points()[2].0, 0.6], "the nudged x");
        assert_eq!(next[3], [1.0, 0.5]);
        assert!(sanitize_curve(next).is_some());

        // Crowded at x = 1 (eval-valid, but the nudge pushes x past 1):
        // the identity shows, and the first edit replaces the whole curve.
        let stored = [[0.0, 0.0], [0.999, 0.2], [0.9995, 0.3], [1.0, 1.0]];
        assert!(sanitize_curve(stored).is_some());
        let (shown, fallback) = curve_edit_model(&stored);
        assert!(fallback);
        let identity = CURVE_IDENTITY.map(|[x, y]| (x, y));
        assert_eq!(shown.points(), &identity);
        let mut moved = shown.clone();
        assert!(moved.move_point(1, 0.5, 0.5));
        assert_eq!(
            curve_write_back(&stored, &shown, fallback, &moved),
            Some([[0.0, 0.0], [0.5, 0.5], CURVE_IDENTITY[2], [1.0, 1.0]])
        );

        // The gate itself: non-finite and non-increasing are refused,
        // out-of-range y/x are not.
        assert_eq!(
            sanitize_curve([[0.0, f32::NAN], [0.3, 0.5], [0.6, 0.5], [1.0, 1.0]]),
            None
        );
        assert_eq!(
            sanitize_curve([[0.0, 0.0], [0.6, 0.5], [0.3, 0.5], [1.0, 1.0]]),
            None
        );
        let wide = [[-0.5, -3.0], [0.3, 0.5], [0.6, 0.5], [1.5, 4.0]];
        assert_eq!(sanitize_curve(wide), Some(wide));
    }

    /// (c) Only `def == "curves"` with all four `Vec2` params builds the
    /// editor: another def carrying the same params, a partial curves
    /// node, and a def with an ordinary Vec2 param all stay raw rows.
    #[test]
    fn only_a_complete_curves_node_builds_the_editor() {
        let params = curve_params(CURVE_IDENTITY);
        assert_eq!(curve_points("curves", &params), Some(CURVE_IDENTITY));
        assert_eq!(curve_points("levels", &params), None);
        assert_eq!(curve_points("gradient", &params), None);

        let mut partial = params.clone();
        partial[3].1 = ParamValue::Float(1.0);
        assert_eq!(curve_points("curves", &partial), None);
        assert_eq!(curve_points("curves", &params[..3]), None);

        let center = vec![("center".to_string(), ParamValue::Vec2([0.5, 0.5]))];
        assert_eq!(curve_points("gradient", &center), None);
        assert_eq!(curve_points("curves", &center), None);

        // A fresh node of any other def never carries the curve params.
        let mut panel = GraphPanel::new();
        let defs: Vec<String> = panel
            .registry()
            .node_defs()
            .iter()
            .map(|d| (*d).to_string())
            .collect();
        for def in defs.iter().filter(|d| *d != "curves") {
            let id = panel.add_node(def);
            let node = panel.graph.nodes.iter().find(|n| n.id == id).unwrap();
            assert_eq!(curve_points(def, &node.params), None, "{def}");
        }
    }

    /// (d) Re-rendering an untouched model writes nothing, for a valid
    /// curve, a display-nudged one, and the identity fallback. Dragging a
    /// point back onto its stored value writes nothing either.
    #[test]
    fn curves_write_back_fires_only_on_change() {
        use umber_gui::widgets::curve::MIN_GAP;
        for stored in [
            CURVE_IDENTITY,
            [[0.0, -1.0], [0.3, 0.5], [0.6, 0.5], [1.0, 2.0]],
            [[0.0, 0.0], [0.5, 0.2], [0.5 + MIN_GAP, 0.9], [1.0, 1.0]],
            [[0.0, 0.0], [0.999, 0.2], [0.9995, 0.3], [1.0, 1.0]],
        ] {
            let (shown, fallback) = curve_edit_model(&stored);
            let rerender = shown.clone();
            assert_eq!(
                curve_write_back(&stored, &shown, fallback, &rerender),
                None,
                "{stored:?}"
            );
        }
        let stored = [[0.0, 0.0], [0.25, 0.8], [0.6, 0.3], [1.0, 1.0]];
        let (shown, fallback) = curve_edit_model(&stored);
        let mut moved = shown.clone();
        assert!(moved.move_point(1, 0.3, 0.5));
        assert!(moved.move_point(1, 0.25, 0.8));
        assert_eq!(curve_write_back(&stored, &shown, fallback, &moved), None);
    }

    /// kittest: a selected `curves` node draws no raw `curve_p*` rows (the
    /// editor replaces them); the same params on a `levels` node keep their
    /// rows (the positive control: the label query does reach the editor
    /// section).
    #[test]
    fn curves_node_replaces_the_raw_point_rows() {
        use egui_kittest::kittest::Queryable;
        crate::i18n::install(crate::i18n::I18n::builtin());
        let rows = |def: &str| -> bool {
            let mut panel = GraphPanel::new();
            let id = panel.add_node(def);
            for (name, value) in curve_params(CURVE_IDENTITY) {
                assert!(panel.set_param(id, &name, value));
            }
            panel.select(Some(id));
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(900.0, 2000.0))
                .build_ui_state(|ui, panel: &mut GraphPanel| panel.show(ui), panel);
            harness.run();
            let found = harness.query_by_label("curve_p0").is_some();
            drop(harness);
            found
        };
        assert!(rows("levels"), "levels keeps the raw rows");
        assert!(!rows("curves"), "curves draws the editor instead");
    }
}
