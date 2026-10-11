//! umber-app — the desktop application shell.
//!
//! Wave 1: egui/eframe boot + dockable panel shell (egui_dock) + app state
//! wiring. The GPU pass lands the viewport widget: `UmberApp::new` captures
//! eframe's `wgpu_render_state` (available because `NativeOptions.renderer`
//! is forced to `Renderer::Wgpu` below) and builds a `GpuContext` from it —
//! no `wgpu::Instance`/`Adapter` is ever constructed in this crate.
//!
//! API note: egui 0.36 replaced menu::bar with MenuBar/MenuButton
//! containers, TopBottomPanel with the unified Panel API, and moved
//! NativeOptions to eframe; this file was written against the vendored
//! source, not remembered APIs. Threading: the UI thread owns everything
//! except Bakes-panel bakes, which run on a `std::thread` worker
//! (`bakes_panel::BakeJobHandle`, polled each frame below); the
//! paint-thread + ring-buffer architecture (docs/specs/architecture.md)
//! lands with the paint engine.

mod bake_sources;
mod bakes_panel;
mod brush_panel;
mod display_panel;
mod document;
mod env;
mod export_dialog;
mod fill_layer;
mod graph_canvas;
mod graph_panel;
mod history_panel;
mod i18n;
#[cfg(test)]
mod input_photon;
mod mesh_map_import;
mod paint_state;
mod path_tool;
#[cfg(test)]
mod perf_soak;
mod plugins;
mod postfx_panel;
mod retouch;
mod size_presets;
mod tile_selection;
mod uv_view;
mod viewport;

use bakes_panel::{BakesContext, BakesPanel};
use brush_panel::BrushPanel;
use egui::containers::menu::{MenuBar, MenuButton};
use egui::{CentralPanel, Id, Ui, WidgetText};
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use export_dialog::{ExportContext, ExportDialog};
use graph_panel::GraphPanel;
use i18n::tr;
use std::path::{Path, PathBuf};
use umber_gpu::GpuContext;
use uv_view::UvView;
use viewport::Viewport;

/// Every panel the shell can dock.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Panel {
    Viewport,
    UvView,
    LayerStack,
    Properties,
    Assets,
    History,
    TextureSets,
    Bakes,
    Export,
    Graph,
    Display,
}

/// Implements the egui_dock tab interface. Built fresh each frame, borrowing
/// the pieces of `UmberApp` it needs — `egui_dock::TabViewer` has no access
/// to the app otherwise, so this is how the viewport panel reaches the GPU
/// context and the camera/mesh state that outlive a single frame.
struct PanelViewer<'a> {
    viewport: &'a mut Viewport,
    uv_view: &'a mut UvView,
    bakes: &'a mut BakesPanel,
    export: &'a mut ExportDialog,
    graph: &'a mut GraphPanel,
    mesh: Option<&'a umber_mesh::MeshData>,
    mesh_path: Option<&'a std::path::Path>,
    gpu: &'a GpuContext,
    paint: Option<&'a mut paint_state::PaintState>,
    doc: &'a mut document::Document,
    brush: &'a mut BrushPanel,
    mesh_import: &'a mut mesh_map_import::MeshMapImportState,
    /// The project's asset store (`None` before the first Save/Open —
    /// the mesh-map import Apply path waits for it; see `mesh_map_import`).
    store: Option<&'a umber_core::AssetStore>,
    polygon_fill: &'a mut fill_layer::PolygonFillTool,
    path_tool: &'a mut path_tool::PaintPathTool,
    display: &'a mut umber_color::DisplaySettings,
    /// The display LUT's sync state (the Display panel marks it dirty).
    display_lut_state: &'a mut display_panel::DisplayLutState,
    /// The device display LUT both GPU views sample, already narrowed
    /// by the EFFECTIVE HDR mode (`HdrMode::select_lut` — `None` once
    /// HDR is live: the SDR table's transfer function must not apply to
    /// an HDR surface). `Some` unconditionally today, since the
    /// EFFECTIVE mode is always `Sdr` in this eframe version (see
    /// `umber_gpu::hdr`'s module boundary note) — the type stays
    /// `Option` so the bypass needs no signature change once it isn't.
    display_lut: Option<&'a umber_gpu::DisplayLut>,
    /// The post-effects chain (the Display panel's postfx section).
    postfx: &'a mut umber_gpu::postfx::PostChain,
    /// The postfx section's session state (add pick, drag in flight).
    postfx_panel: &'a mut postfx_panel::PostFxPanel,
    /// The requested HDR surface mode (§5 P1 row 85); the Display
    /// panel's HDR row edits this.
    hdr_mode: &'a mut umber_gpu::HdrMode,
}

impl TabViewer for PanelViewer<'_> {
    type Tab = Panel;

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        match tab {
            Panel::Viewport => tr("panel.viewport").into(),
            Panel::UvView => tr("panel.uv-view").into(),
            Panel::LayerStack => tr("panel.layers").into(),
            Panel::Properties => tr("panel.properties").into(),
            Panel::Assets => tr("panel.assets").into(),
            Panel::History => tr("panel.history").into(),
            Panel::TextureSets => tr("panel.texture-sets").into(),
            Panel::Bakes => tr("panel.bakes").into(),
            Panel::Export => tr("panel.export").into(),
            Panel::Graph => tr("panel.graph").into(),
            Panel::Display => tr("panel.display").into(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            Panel::Viewport => {
                self.viewport
                    .ui(ui, self.gpu, self.paint.as_deref_mut(), self.display_lut)
            }
            Panel::UvView => self.uv_view.ui(
                ui,
                self.mesh,
                self.gpu,
                self.paint.as_deref_mut(),
                self.display_lut,
                Some(&mut *self.polygon_fill),
                Some(&mut *self.path_tool),
            ),
            Panel::LayerStack => document::layers_ui(ui, self.doc),
            Panel::Properties => {
                self.brush.show(ui, self.paint.as_deref_mut());
                // The paint session adopts the panel's tool and symmetry
                // (and hands back an alt-click-resolved clone offset and a
                // UV-view mirror-manipulator drag).
                if let Some(paint) = self.paint.as_deref_mut() {
                    paint.sync_tool(&mut self.brush.tool);
                    paint.sync_symmetry(&mut self.brush.symmetry);
                    paint.sync_line_constraint(self.brush.line_constraint);
                    paint.sync_projection(self.brush.projection);
                    // ... plus the active preset's wash mode (build-up
                    // when no preset is selected).
                    let wash = self
                        .brush
                        .active_preset()
                        .map_or(umber_brush::WashMode::BuildUp, |preset| {
                            preset.params.wash_mode
                        });
                    paint.sync_wash_mode(wash);
                }
                // One collapsible row per fill layer (projection picker +
                // v1 source editor) plus the polygon-fill tool rows; edits
                // run through Document as undoable, journaled SetFill ops.
                fill_layer::fill_section_ui(ui, self.doc, self.mesh, &mut *self.polygon_fill);
                // The paint-path tool rows (editable polyline → replayed
                // stroke / closed-path fill through the paint session).
                path_tool::path_tool_rows(ui, self.paint.as_deref_mut(), &mut *self.path_tool);
            }
            Panel::Assets => {
                ui.label(tr("shell.assets-placeholder"));
                ui.separator();
                self.mesh_import.show(
                    ui,
                    self.doc,
                    self.store,
                    mesh_map_import::read_pending_from_disk,
                );
            }
            Panel::History => {
                history_panel::history_ui(ui, self.doc);
                // Retouch (smudge/clone) strokes keep their own per-stroke
                // history in v1 — the layer journal above can't hold texel
                // snapshots yet.
                if let Some(paint) = self.paint.as_deref_mut() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(tr("history.strokes"));
                        let undo = egui::Button::new(tr("history.undo-stroke"));
                        if ui.add_enabled(paint.can_undo_stroke(), undo).clicked() {
                            paint.undo_stroke();
                        }
                        let redo = egui::Button::new(tr("history.redo-stroke"));
                        if ui.add_enabled(paint.can_redo_stroke(), redo).clicked() {
                            paint.redo_stroke();
                        }
                    });
                }
            }
            Panel::TextureSets => {
                ui.label(tr("shell.texture-sets-placeholder"));
            }
            Panel::Bakes => {
                let ctx = BakesContext {
                    gpu: Some(self.gpu),
                    mesh: self.mesh,
                    mesh_path: self.mesh_path,
                };
                self.bakes.show(ui, ctx);
            }
            Panel::Export => {
                let ctx = ExportContext {
                    gpu: Some(self.gpu),
                    mesh: self.mesh,
                    mesh_path: self.mesh_path,
                    // Shared reborrow (not `take()`): the Export panel
                    // reads the live target for the painted bridge while
                    // the center views keep staging strokes into it.
                    paint: self.paint.as_deref(),
                    doc: Some(&*self.doc),
                    graph: Some(&*self.graph),
                };
                self.export.show(ui, ctx);
            }
            Panel::Graph => {
                // The anchor picker binds within the active texture set:
                // refresh the panel's layer list from the document every
                // frame (v1 holds one set's stack, so the list IS the set).
                let layers: Vec<(u64, String)> = self
                    .doc
                    .layers()
                    .iter()
                    .map(|l| (l.id, l.name.clone()))
                    .collect();
                self.graph.set_anchor_layers(layers);
                self.graph.show(ui);
            }
            Panel::Display => {
                // Scrolls: the postfx section grows one group per effect.
                egui::ScrollArea::vertical().show(ui, |ui| {
                    display_panel::show(ui, self.display, self.display_lut_state);
                    ui.separator();
                    display_panel::hdr_row(ui, self.hdr_mode, &self.gpu.hdr_probe);
                    ui.separator();
                    postfx_panel::show(ui, self.postfx, self.postfx_panel);
                });
            }
        }
    }
}

/// Shared application state (grows per wave).
#[derive(Default)]
pub struct AppState {
    pub mesh: Option<umber_mesh::MeshData>,
    pub mesh_path: Option<PathBuf>,
    pub viewport: Viewport,
    pub uv_view: UvView,
    /// The paint session; `None` when the device lacks the storage-texture
    /// feature (constructed in `UmberApp::new`, falls back to view-only).
    pub paint: Option<paint_state::PaintState>,
    /// The open document: layers + undo journal.
    pub doc: document::Document,
    /// The polygon-fill lasso tool: the armed click-path capture feeding
    /// pixel masks into fill layers (session state, never serialized).
    pub polygon_fill: fill_layer::PolygonFillTool,
    /// The fill layers' evaluation caches + the panel's dirty queue.
    pub fill_states: fill_layer::FillLayerStates,
    /// The paint-path tool: the editable polyline replayed as
    /// paint/erase/smudge strokes (session state, never serialized).
    pub path_tool: path_tool::PaintPathTool,
    /// The bakes panel: mesh-map bake settings + last-bake status.
    pub bakes: BakesPanel,
    /// The export dialog: preset choice, output dir + last-export status.
    pub export: ExportDialog,
    /// The node-graph panel: procedural graph + cached eval (wave-5).
    pub graph: GraphPanel,
    /// The viewer chain (Display panel; saved in the project). Reaches
    /// the viewport + UV view through the GPU display LUT.
    pub display: umber_color::DisplaySettings,
    /// Whether the device display LUT matches `display` (starts dirty;
    /// the frame loop rebuilds + uploads).
    pub display_lut: display_panel::DisplayLutState,
    /// The post-effects chain (§7:84; Display panel's postfx section).
    /// Session state in v1: not yet saved in the project, and not yet
    /// composited into the viewport (see `postfx_panel`).
    pub postfx: umber_gpu::postfx::PostChain,
    /// The postfx section's UI state (never serialized).
    pub postfx_panel: postfx_panel::PostFxPanel,
    /// Mesh-map import by naming convention (§7 P1 row): the folder-scan
    /// routing + unmatched report + session texture set (channels ensured
    /// on apply). Owns the session set so imported user channels survive
    /// until the next save (see `mesh_map_import`).
    pub mesh_import: mesh_map_import::MeshMapImportState,
    /// The project directory of the last Save/Open (`None` for a
    /// never-saved document — the import Apply path and the autosave
    /// journal both wait for it).
    pub project_dir: Option<PathBuf>,
}

/// The eframe app.
pub struct UmberApp {
    pub state: AppState,
    dock: DockState<Panel>,
    gpu: GpuContext,
    /// The device display LUT (created at startup on eframe's device;
    /// rewritten in place when `state.display_lut` is dirty). Handed to
    /// the viewport's mesh pass and the UV view's paint display.
    display_lut: umber_gpu::DisplayLut,
    brush_panel: BrushPanel,
    /// Perf HUD toggle (View menu). `perf` feature only.
    #[cfg(feature = "perf")]
    show_perf_hud: bool,
    /// Previous frame's `egui::InputState::time`, for the HUD's frame-ms
    /// line (`perf` feature only).
    #[cfg(feature = "perf")]
    last_frame_time: Option<f64>,
    /// Cumulative `dabs_composited` at the previous frame, for the HUD's
    /// dab-throughput delta (`perf` feature only).
    #[cfg(feature = "perf")]
    last_dabs_composited: u64,
    /// Wireframe overlay toggle (View menu, W). Synced into the
    /// viewport each frame; off by default (off renders byte-identical
    /// to the mesh-only frame).
    show_wireframe: bool,
    /// Ground-grid toggle (View menu, G). Same sync + off contract.
    show_grid: bool,
    /// Shading mode (View menu rows, V cycles lit ↔ unlit). Synced
    /// into the viewport each frame; Lit by default (today's render).
    view_mode: umber_gpu::ViewMode,
    /// The requested HDR surface mode (§5 P1 row 85). Persisted as a
    /// user preference (`display_panel::hdr_pref_path`), not a project
    /// field — see `umber_gpu::hdr`'s module boundary note. The EFFECTIVE
    /// mode a frame actually draws with is `hdr_mode.effective(&gpu.hdr_probe)`,
    /// computed fresh each frame rather than stored, since `gpu.hdr_probe`
    /// never changes within a session.
    hdr_mode: umber_gpu::HdrMode,
}

impl UmberApp {
    /// Captures eframe's `wgpu_render_state` (see module docs) and builds
    /// the one `GpuContext` the app uses for its lifetime.
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("umber requires eframe's wgpu backend"))?;
        let gpu = GpuContext::new(
            render_state.adapter.clone(),
            render_state.device.clone(),
            render_state.queue.clone(),
            render_state.target_format,
            // Depth: eframe threads NativeOptions::depth_buffer into the egui
            // renderer's depth_stencil_format (wgpu_integration.rs:232 ->
            // egui_wgpu::depth_format_from_bits), which attaches depth to the
            // shared render pass our callback draws into. Value comes from
            // umber-gpu so this crate never names a wgpu type directly
            // (review #1 + architecture rule).
            umber_gpu::renderer::depth_format(),
        );
        // The display LUT, identity until the first frame's rebuild
        // (`AppState::display_lut` starts dirty) uploads the chain.
        let display_lut = umber_gpu::DisplayLut::new(&gpu, &umber_gpu::identity_lut_bytes());

        // Layout: [left column | center (3D + 2D UV tabs)] with history docked right.
        let mut dock = DockState::new(vec![Panel::Viewport, Panel::UvView]);
        {
            let tree = dock.main_surface_mut();
            let [left, _] = tree.split_left(
                NodeIndex::root(),
                0.25,
                vec![
                    Panel::LayerStack,
                    Panel::TextureSets,
                    Panel::Bakes,
                    Panel::Export,
                    Panel::Graph,
                    Panel::Display,
                ],
            );
            let [_top, _bottom] =
                tree.split_below(left, 0.4, vec![Panel::Properties, Panel::Assets]);
            tree.split_right(NodeIndex::root(), 0.2, vec![Panel::History]);
        }

        // Paint session on the eframe-owned device/queue (cloned for the
        // thread). Feature-missing devices degrade to view-only with a
        // logged warning, never a hard startup failure.
        let paint = match paint_state::PaintState::new(
            render_state.device.clone(),
            render_state.queue.clone(),
        ) {
            Ok(session) => Some(session),
            Err(err) => {
                log::warn!("paint disabled: {err:#}");
                None
            }
        };

        let mut graph = GraphPanel::new();
        graph.load_plugins(&plugins::plugin_paths());

        // The HDR request (§5 P1 row 85): the persisted preference, or
        // Sdr if there is none yet — a missing/unrecognized file is not
        // an error (same contract as the locale preference).
        let hdr_mode = display_panel::hdr_pref_path()
            .and_then(|path| display_panel::read_hdr_pref(&path))
            .unwrap_or_default();

        Ok(Self {
            state: AppState {
                paint,
                graph,
                ..AppState::default()
            },
            dock,
            gpu,
            display_lut,
            brush_panel: BrushPanel::new(),
            #[cfg(feature = "perf")]
            show_perf_hud: false,
            #[cfg(feature = "perf")]
            last_frame_time: None,
            #[cfg(feature = "perf")]
            last_dabs_composited: 0,
            show_wireframe: false,
            show_grid: false,
            view_mode: umber_gpu::ViewMode::default(),
            hdr_mode,
        })
    }
}

/// App-level actions (kept out of the App impl to keep that block small).
impl UmberApp {
    /// Loads the committed studio default if present (procedural
    /// otherwise) — called once at startup, after construction, so a
    /// missing asset degrades to the fallback instead of failing.
    fn load_default_env(&mut self) {
        if let Some(env) = env::load_default_environment(&self.gpu) {
            self.state.viewport.set_environment(&self.gpu, Some(env));
        }
    }

    /// File > Load Environment…: file dialog → decode → convolve →
    /// viewport swap. Failures log; the current environment is kept.
    fn load_environment(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(tr("shell.filter-environment"), &["png", "exr"])
            .pick_file()
        else {
            return;
        };
        match env::load_environment_file(&self.gpu, &path) {
            Ok(ibl) => {
                self.state.viewport.set_environment(&self.gpu, Some(ibl));
                log::info!("loaded environment: {}", path.display());
            }
            Err(err) => log::error!("environment load failed: {err:#}"),
        }
    }
    /// Exports the current paint target as an sRGB PNG via a save dialog.
    ///
    /// Readback is blocking (one full-target copy); acceptable for a
    /// user-triggered export at 512².
    fn export_paint_png(&mut self) {
        let Some(paint) = self.state.paint.as_ref() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("painted_map.png")
            .add_filter("PNG", &["png"])
            .save_file()
        else {
            return;
        };
        let target = paint.paint_target();
        match target.read_back_rgba8(&self.gpu.device, &self.gpu.queue) {
            Ok(bytes) => {
                let (w, h) = target.dimensions();
                match umber_export::png::write_png(
                    &path,
                    w,
                    h,
                    &bytes,
                    umber_export::png::Transfer::Srgb,
                ) {
                    Ok(()) => log::info!("exported painted map: {}", path.display()),
                    Err(e) => log::error!("png write failed: {e:#}"),
                }
            }
            Err(e) => log::error!("readback failed: {e:#}"),
        }
    }

    /// The texture-set name for project save: the loaded mesh's set
    /// name, else the fallback default.
    fn project_texture_set_name(&self) -> String {
        umber_mesh::texture_set_name(
            self.state
                .mesh_path
                .as_deref()
                .unwrap_or(Path::new("TextureSet")),
            self.state
                .mesh
                .as_ref()
                .unwrap_or(&umber_mesh::MeshData::default()),
        )
    }

    /// Saves the document as a `.umber` project directory via a save
    /// dialog: one texture set (named from the loaded mesh), the
    /// layer stack + project settings through `umber_core::project`,
    /// plus the graph panel's graph as a `.mtlx` string (the additive
    /// wave-5 carry — omitted when the panel's graph is empty, so
    /// graph-less projects save exactly the old shape).
    ///
    /// The saved texture set is the mesh-map import session set (its name
    /// retargeted to the mesh's set name, channels kept): user channels an
    /// earlier `<Set>_<suffix>` import created survive the save instead of
    /// being reset to the five defaults.
    fn save_project(&mut self) {
        let set_name = self.project_texture_set_name();
        let Some(dir) = rfd::FileDialog::new()
            .set_file_name(format!("{set_name}.umber"))
            .save_file()
        else {
            return;
        };
        // Keep the import session set (and its user channels) as the saved
        // set: a rename keeps channels (see `retarget_set`).
        self.state.mesh_import.retarget_set(&set_name);
        let texture_set = self.state.mesh_import.session_set.clone();
        let graphs_mtlx = if self.state.graph.graph().nodes.is_empty() {
            Vec::new()
        } else {
            vec![self.state.graph.to_mtlx()]
        };
        let model = umber_core::project::ProjectModel::new(
            vec![texture_set],
            vec![umber_core::project::TextureSetLayers {
                texture_set: set_name.clone(),
                stack: self.state.doc.stack.clone(),
            }],
            umber_core::project::ProjectSettings {
                active_texture_set: Some(set_name.clone()),
                display: self.state.display,
            },
        )
        .with_graphs_mtlx(graphs_mtlx)
        // The journal high-water mark: every entry recorded so far is in
        // this saved state, so none may replay on top of it.
        .with_journal_seq(self.state.doc.journal_seq());
        match umber_core::project::save_to_dir(&model, &dir) {
            Ok(()) => {
                log::info!("project saved: {}", dir.display());
                // The import Apply path (and future drops) now have a
                // content-hash store to write into.
                self.state.project_dir = Some(dir.clone());
                // From here on the document journals into `dir`, its seq
                // continuing past the stamped mark (a Save As clears the
                // old directory's journal — `dir` now holds those edits).
                if let Err(e) = self
                    .state
                    .doc
                    .attach_journal(&dir, &set_name, model.journal_seq)
                {
                    log::warn!("autosave journal unavailable: {e}");
                }
            }
            Err(e) => log::error!("project save failed: {e}"),
        }
    }

    /// Loads a `.umber` project directory (recovering any autosave
    /// journal a crash left — see `umber_core::journal`): restores the
    /// active texture set's layer stack into the document (undo history
    /// starts fresh; the journal is session-scoped by design), attaches
    /// the project's autosave journal, and the first carried
    /// `.mtlx` graph into the graph panel (a parse failure keeps the
    /// current panel graph and logs — the panel's status line says so).
    fn open_project(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        match umber_core::project::load_from_dir(&dir) {
            Ok(model) => {
                let active = model
                    .settings
                    .active_texture_set
                    .or_else(|| model.texture_sets.first().map(|ts| ts.name.clone()));
                let Some(name) = active else {
                    log::error!("project has no texture sets: {}", dir.display());
                    return;
                };
                let Some(entry) = model.layers.iter().find(|l| l.texture_set == name) else {
                    log::error!("texture set {name:?} has no layers entry");
                    return;
                };
                self.state.doc.load_stack(entry.stack.clone());
                // The import session set restores the saved set wholesale
                // (channels included — user channels from earlier imports
                // survive the open); unknown-name fallback is the default.
                if let Some(saved) = model.texture_sets.iter().find(|ts| ts.name == name) {
                    self.state.mesh_import.set_session_set(saved.clone());
                } else {
                    self.state
                        .mesh_import
                        .set_session_set(umber_core::TextureSet::new_default(&name));
                }
                self.state.project_dir = Some(dir.clone());
                // `load_from_dir` already replayed + truncated any journal
                // a crash left behind; resume journaling from its mark.
                if let Err(e) = self
                    .state
                    .doc
                    .attach_journal(&dir, &name, model.journal_seq)
                {
                    log::warn!("autosave journal unavailable: {e}");
                }
                // Hand-edited files can carry out-of-range values; the
                // sliders' ranges are the contract.
                self.state.display = model.settings.display.sanitized();
                self.state.display_lut.mark_display_lut_dirty();
                if let Some(doc) = model.graphs_mtlx.first() {
                    if self.state.graph.load_mtlx(doc) {
                        log::info!("project graph restored into the Graph panel");
                    } else {
                        log::error!(
                            "project graph failed to parse; panel graph unchanged: {}",
                            self.state.graph.status()
                        );
                    }
                }
                log::info!(
                    "project loaded: {} ({} layers in {name:?})",
                    dir.display(),
                    self.state.doc.stack.layers.len()
                );
            }
            Err(e) => log::error!("project load failed: {e}"),
        }
    }
}

impl eframe::App for UmberApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        profiling::scope!("frame");
        // Async bake (wave 5): drain a finished bake job here, not only in
        // the Bakes tab's `show` (egui_dock skips hidden tabs), and keep
        // frames coming while one runs — egui otherwise repaints only on
        // input, so the result would sit until the mouse moved. (The click
        // frame itself starts the job after this poll; the tab's spinner
        // requests the next frame, and this keeps it going from there.)
        if self.state.bakes.poll_job() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Mesh-map import drops (§7 P1 row): drain here, not only in the
        // Assets tab's `show` (egui_dock skips hidden tabs — same pattern
        // as the async bake poll above), so a folder dropped while Assets
        // is hidden still routes.
        self.state.mesh_import.poll_drops(ui.ctx());
        // Overlay keybinds (Wave-4 item 7): W toggles the wireframe, G
        // the ground grid. View-mode keybind (§5's P0 row): V cycles
        // lit ↔ unlit (from Solo/preview it returns to Lit).
        // Quick-mask keybinds (§2's P1 row): X enters/
        // leaves mask mode, I inverts the mask while in mask mode (outside
        // it, I does nothing). E selects the eraser (§2's P0 row). Skipped
        // while a text edit has focus (brush preset names, save-as dialog)
        // so typing never flips the viewport, the mask, or the tool.
        if !ui.ctx().text_edit_focused() {
            let (wire_key, grid_key, view_key, mask_key, invert_key, eraser_key) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::W),
                    i.key_pressed(egui::Key::G),
                    i.key_pressed(egui::Key::V),
                    i.key_pressed(egui::Key::X),
                    i.key_pressed(egui::Key::I),
                    i.key_pressed(egui::Key::E),
                )
            });
            if wire_key {
                self.show_wireframe = !self.show_wireframe;
            }
            if grid_key {
                self.show_grid = !self.show_grid;
            }
            if view_key {
                self.view_mode = self.view_mode.cycle_v();
            }
            if eraser_key {
                self.brush_panel.select_eraser();
            }
            if let Some(paint) = self.state.paint.as_mut() {
                if mask_key {
                    paint.toggle_mask_mode();
                }
                if invert_key && paint.quick_mask().is_editing() {
                    paint.invert_mask();
                }
            }
        }
        self.state.viewport.set_show_wireframe(self.show_wireframe);
        self.state.viewport.set_show_grid(self.show_grid);
        self.state
            .viewport
            .set_show_cage(self.state.bakes.show_cage());
        self.state.viewport.set_view_mode(self.view_mode);
        // Top bar: MenuBar container (egui 0.36 API).
        MenuBar::new().ui(ui, |ui| {
            MenuButton::new(tr("menu.file")).ui(ui, |ui| {
                if ui.button(tr("menu.open-mesh")).clicked() {
                    if let Some(path) = rfd_pick_mesh() {
                        match umber_mesh::load(&path) {
                            Ok(mesh) => {
                                let n = mesh.vertex_count();
                                match self.state.viewport.load_mesh(&self.gpu, &mesh) {
                                    Ok(()) => {
                                        // The import session set follows the
                                        // mesh's set name (channels kept).
                                        let set_name = umber_mesh::texture_set_name(&path, &mesh);
                                        self.state.mesh_import.retarget_set(&set_name);
                                        self.state.mesh = Some(mesh);
                                        self.state.mesh_path = Some(path);
                                        log::info!("loaded mesh: {n} vertices");
                                    }
                                    Err(e) => log::error!("mesh upload failed: {e:#}"),
                                }
                            }
                            Err(e) => log::error!("mesh load failed: {e}"),
                        }
                    }
                }
                if ui.button(tr("menu.open-project")).clicked() {
                    self.open_project();
                }
                if ui.button(tr("menu.save-project")).clicked() {
                    self.save_project();
                }
                let export_enabled = self.state.paint.is_some();
                if ui
                    .add_enabled(export_enabled, egui::Button::new(tr("menu.export-png")))
                    .clicked()
                {
                    self.export_paint_png();
                }
                if ui.button(tr("menu.load-environment")).clicked() {
                    self.load_environment();
                }
            });
            MenuButton::new(tr("menu.view")).ui(ui, |ui| {
                #[cfg(feature = "perf")]
                ui.checkbox(&mut self.show_perf_hud, tr("menu.perf-hud"));
                #[cfg(not(feature = "perf"))]
                ui.label(tr("menu.perf-hud-unavailable"));
                ui.checkbox(&mut self.show_wireframe, tr("menu.wireframe"));
                ui.checkbox(&mut self.show_grid, tr("menu.grid"));
                ui.separator();
                viewport::view_mode_menu(ui, &mut self.view_mode);
            });
            MenuButton::new(tr("menu.help")).ui(ui, |ui| {
                if ui.button(tr("menu.about")).clicked() {
                    ui.label(tr("menu.about-text"));
                }
            });
        });

        // The import Apply path needs the project's content-hash store;
        // it exists from the first Save/Open on (see `project_dir`).
        let store = self
            .state
            .project_dir
            .as_ref()
            .map(|dir| umber_core::AssetStore::new(dir));
        // The LUT-chain adaptation (§5 P1 row 85, `umber_gpu::hdr`): HDR
        // bypasses the SDR display LUT the same way a data view-mode
        // already does (`ViewMode::select_lut`) — composed here, once
        // per frame, rather than threaded through the render callbacks.
        let effective_lut = self
            .hdr_mode
            .effective(&self.gpu.hdr_probe)
            .select_lut(Some(&self.display_lut));
        CentralPanel::default().show(ui, |ui| {
            let mut viewer = PanelViewer {
                viewport: &mut self.state.viewport,
                uv_view: &mut self.state.uv_view,
                bakes: &mut self.state.bakes,
                export: &mut self.state.export,
                graph: &mut self.state.graph,
                mesh: self.state.mesh.as_ref(),
                mesh_path: self.state.mesh_path.as_deref(),
                gpu: &self.gpu,
                paint: self.state.paint.as_mut(),
                doc: &mut self.state.doc,
                brush: &mut self.brush_panel,
                polygon_fill: &mut self.state.polygon_fill,
                path_tool: &mut self.state.path_tool,
                display: &mut self.state.display,
                display_lut_state: &mut self.state.display_lut,
                display_lut: effective_lut,
                postfx: &mut self.state.postfx,
                postfx_panel: &mut self.state.postfx_panel,
                mesh_import: &mut self.state.mesh_import,
                store: store.as_ref(),
                hdr_mode: &mut self.hdr_mode,
            };
            DockArea::new(&mut self.dock)
                .style(Style::from_egui(ui.style()))
                .show_inside(ui, &mut viewer);
        });

        // Display LUT rebuild, after the panels (the Display panel's
        // edits this frame included): the upload is staged on the queue,
        // so it lands before egui submits this frame — the callbacks the
        // views just built (same texture, unchanged bind group) already
        // draw with the new table. One more frame refreshes the panel's
        // status line, which read "Rebuilding" while the LUT was dirty.
        if let Some(bytes) = self.state.display_lut.take_rebuild(&self.state.display) {
            self.display_lut.upload(&self.gpu, &bytes);
            ui.ctx().request_repaint();
        }

        // Drain the paint channel once per frame after all panels ran.
        if let Some(paint) = self.state.paint.as_mut() {
            if let Err(err) = paint.process_pending() {
                log::warn!("paint processing failed: {err:#}");
            }
        }

        // Fill-layer refresh: adopt every doc-side fill change, then
        // re-evaluate and stage the tiles of any layer the adoption pass
        // marked (panel edits, undo/redo and journal replays all land in
        // Layer::fill before this point in the frame — one adoption link
        // covers every producer of fill-param changes).
        if let Some(paint) = self.state.paint.as_mut() {
            let layers: Vec<_> = self
                .state
                .doc
                .layers()
                .iter()
                .filter(|l| l.kind == umber_core::LayerKind::Fill)
                .map(|l| {
                    (
                        l.id,
                        l.fill.clone().unwrap_or_default(),
                        l.blend_mode,
                        l.opacity,
                    )
                })
                .collect();
            for (id, params, blend, opacity) in &layers {
                self.state.fill_states.ensure(*id, params, *blend, *opacity);
            }
            if !self.state.fill_states.dirty.is_empty() {
                if let Some(mesh) = self.state.mesh.clone() {
                    let dirty: Vec<u64> = self.state.fill_states.dirty.drain(..).collect();
                    let (tiles, symmetry) = (paint.tiles_present(), *paint.symmetry());
                    for layer in dirty {
                        // A layer deleted between queueing and the mesh's
                        // arrival must not ghost-publish.
                        if !self
                            .state
                            .doc
                            .layers()
                            .iter()
                            .any(|l| l.id == layer && l.kind == umber_core::LayerKind::Fill)
                        {
                            continue;
                        }
                        let Some(state) = self.state.fill_states.map.get_mut(&layer) else {
                            continue;
                        };
                        let staged = fill_layer::refresh_fill_layer(
                            state, &mesh, paint, &tiles, 1024, &symmetry,
                        );
                        if staged.is_empty() {
                            log::warn!("fill refresh for layer {layer} staged no tiles");
                        }
                    }
                }
                // No mesh loaded: LEAVE the queue — a fill layer first seen
                // without a mesh stages its tiles on the mesh's arrival
                // (ensure's dedup keeps the queue from growing meanwhile).
            }
        }

        // Perf HUD (read-only overlay, `perf` feature only): frame ms from
        // egui's input-time delta, dab throughput from FrameStats deltas,
        // paint-target bytes, undo depth. No budget enforcement — display
        // only, per the instrumentation design.
        #[cfg(feature = "perf")]
        {
            let now = ui.input(|i| i.time);
            let frame_ms = self
                .last_frame_time
                .map(|t| (now - t) * 1000.0)
                .unwrap_or(0.0);
            self.last_frame_time = Some(now);
            if self.show_perf_hud {
                let dabs_total = self
                    .state
                    .paint
                    .as_ref()
                    .map(|p| p.last_stats().dabs_composited)
                    .unwrap_or(0);
                let dabs_delta = dabs_total.saturating_sub(self.last_dabs_composited);
                self.last_dabs_composited = dabs_total;
                let target_bytes = self
                    .state
                    .paint
                    .as_ref()
                    .map(|p| p.paint_target().byte_len())
                    .unwrap_or(0);
                let undo_depth = self.state.doc.history.len();
                egui::Window::new("Perf HUD").show(ui.ctx(), |ui| {
                    ui.label(format!("frame: {frame_ms:.2} ms"));
                    ui.label(format!(
                        "dabs this frame: {dabs_delta} (total {dabs_total})"
                    ));
                    ui.label(format!("paint target: {target_bytes} bytes"));
                    ui.label(format!("undo depth: {undo_depth}"));
                });
            }
        }
    }
}

/// Minimal file picker (rfd; wrapped so the call site stays clean if we
/// swap pickers later).
fn rfd_pick_mesh() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            tr("shell.filter-meshes"),
            &["obj", "gltf", "glb", "fbx", "usda", "usd"],
        )
        .pick_file()
}

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("umber — starting (wave-2: paint core)");
    // UI strings: catalogs + locale on this (the UI) thread, before any
    // panel renders (docs/specs/i18n-design.md).
    i18n::init();

    // Paint-compositor requirement (umber-gpu paint claw reviewer checklist
    // item #1): read_write storage textures need the adapter-specific format
    // features requested on the device, or the compute splat pass panics at
    // first real stroke. eframe owns the device, so the requirement must be
    // injected here — through the device descriptor closure.
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(ref mut create_new) = wgpu_options.wgpu_setup {
        create_new.device_descriptor =
            std::sync::Arc::new(|_adapter| eframe::egui_wgpu::wgpu::DeviceDescriptor {
                required_features:
                    eframe::egui_wgpu::wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                ..Default::default()
            });
    }

    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Umber"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        // Gives the egui renderer's render pass a depth attachment matching
        // umber-gpu's pipeline (review #1). Value sourced from umber-gpu so
        // this crate stays free of direct wgpu types.
        depth_buffer: umber_gpu::DEPTH_FORMAT_BITS,
        ..Default::default()
    };
    eframe::run_native(
        "Umber",
        native,
        Box::new(|cc| match UmberApp::new(cc) {
            Ok(mut app) => {
                app.load_default_env();
                Ok(Box::new(app) as Box<dyn eframe::App>)
            }
            Err(err) => Err(format!("{err:#}").into()),
        }),
    )
}
