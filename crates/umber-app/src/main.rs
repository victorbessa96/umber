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
//!
//! Paint-pixel persistence (CORE-LOOP CRITICAL 4, the 2026-10-12 strategy
//! review): `save_project` also writes every materialized plane (Base AND
//! every live layer-owned one — per-layer-pool stage 2,
//! docs/specs/per-layer-pool-design.md §6) as a byte-exact PNG under
//! `<project>/paint/` (Base keeps the `<project>/paint/tile-<N>.png` and
//! `<channel>/` layout; layer planes nest under
//! `paint/layers/<id>/<channel>/`), and `open_project` publishes them back
//! through the per-plane publish path (`PaintState::publish_plane`).
//! Judgment calls (v1 boundaries):
//!
//! * Sidecar files, not `project.json`: the project schema is owned by
//!   `umber-core` (another kill's surface) and this crate has no serde
//!   dependency, so a `paint` section with `AssetRef`s cannot be added
//!   here. The `AssetStore` sits BEHIND the sidecar instead: every saved
//!   artifact is also stored by content hash under `<project>/assets/`
//!   (the same store the mesh-map import uses — unchanged tiles dedupe),
//!   and `paint/manifest.txt` records name → hash (provenance). The
//!   sidecar files stay authoritative for the load path, so the layout
//!   `load_paint_tiles` reads — and every v1/v2 project — is unchanged.
//! * Crash-safe commit (the audit's paint-loss vector): a save (1) sweeps
//!   or rolls forward any earlier interrupted save, (2) stages every
//!   artifact as `paint/<name>.tmp.<pid>` plus its store blob, both
//!   fsynced, (3) writes `paint/manifest.pending` by tmp+fsync+rename —
//!   the ONE commit point — then (4) renames each temp over its sidecar,
//!   prunes unlisted tiles, and renames the manifest to `manifest.txt`.
//!   Open runs the same recovery first: a pending manifest rolls forward
//!   (idempotent; the content-addressed blob re-materializes any sidecar
//!   whose temp is gone), and with none pending every staged temp is an
//!   uncommitted orphan and is deleted. Reopen is therefore the old paint
//!   or the new paint, never a mix — provided the roll-forward can run
//!   (v1 boundary: a post-commit crash on a project that is now
//!   unwritable, or whose `assets/` blob went missing, logs a warning and
//!   opens the sidecars as they lie; the next writable open finishes the
//!   commit). Per-file renames alone could not
//!   promise that (a crash between two renames is torn); the manifest
//!   rename can. Judgment calls: a staging I/O failure aborts the whole
//!   save (old paint intact) where the pre-commit code skipped the one
//!   tile; a plane whose GPU readback fails re-stages its previous file so
//!   the commit does not prune it — but ONLY for an in-place save (the
//!   re-stage reads the target dir, and a save-as must never re-stage bytes
//!   from a dir the plane was not committed to: there it is a loud
//!   per-plane failure instead); a clean plane (same dir + same blake3 as
//!   its recorded commit) re-stages its on-disk bytes with no readback at
//!   all (§6.2); the store is the roll-forward source,
//!   so its blob is fsynced through a mirrored shard path (pinned by a
//!   test against the real store). Power-loss durability rests on
//!   `sync_all` + the unix directory fsync; only process-crash states are
//!   test-pinned. v1 boundaries: no store GC (superseded paint blobs
//!   accumulate under `assets/` until a store-wide GC lands); each saved
//!   tile is written twice (sidecar + blob) — hard-links were not worth
//!   the cross-platform surface; a hand-clobbered sidecar is not checked
//!   against `manifest.txt` on open (the sidecar wins).
//! * Every present tile is written; the verified boundary is tile 1001
//!   (the same tile the viewport bind shows).
//! * `Transfer::Linear`: the export dialog's `Srgb` moves bytes through
//!   the IEC curve, which would break the save→load byte comparison.
//! * `read_png_rgba8` flips rows into its equirect GPU row order, so the
//!   load path flips them back (`flip_rows_vertical`) — without it the
//!   round trip is a vertical mirror, and the byte-compare test pins this.
//! * Open resizes the session to the saved tile dims first (a resize drops
//!   contents and clears the session-scoped stroke-undo history, so no
//!   stale rect can undo against the restored pixels). A project with no
//!   `paint/` sidecar clears the session to transparent instead of keeping
//!   the previous document's pixels (the layer stack is replaced wholesale
//!   on open; the paint must not bleed across documents).
//! * Journal coexistence: the autosave journal records layer-metadata ops
//!   only (`umber_core::journal::JournalOp` has no pixel op) and the stroke
//!   undo history is RAM-only, so save/open and the journal share nothing
//!   but the `journal_seq` stamp — paint pixels ride beside both. The paint
//!   commit is atomic on its own, but a crash between the `project.json`
//!   commit and the paint commit can still leave new metadata with old
//!   paint; one atomic commit spanning both is deferred.
//! * Save prunes `tile-*.png` files for tiles the session no longer holds,
//!   so a re-save into a project dir never resurrects a dropped tile.
//! * The per-set paint sizes ride in `<project>/paint/target_sizes.txt`
//!   (v2: one `<set>=<width>x<height>` line per sized set plus one
//!   `tile-<N>=<set>` assignment line per saved tile, plus one
//!   `channel/<set>/<channel>=<width>x<height>` line per recorded
//!   per-channel override — absent means that channel inherits its set's
//!   size, so every pre-channel file loads all-inherit; `#` comments and
//!   blank lines skipped). The v1 `<project>/paint/target_size.txt`
//!   global is still written (the active set's size, for old readers) and
//!   still read: a v1 project (tiles + global, no mapping) opens with its
//!   global applied to every set (the migration — see `load_paint_tiles`).
//!   With pixels, the PNG dims lead per tile: a tile whose PNG disagrees
//!   with its set's size is SKIPPED loudly (warn log + the load report's
//!   skip list), never silently resampled or torn. Any positive dims
//!   round-trip: the UI offers squares only, but clamping a stored size
//!   down would drop pixels silently. Sidecar files (not `project.json`)
//!   for the same schema-ownership reason as the pixels above.
//! * Save writes sizes for sets holding saved tiles plus the active set
//!   (a size choice with no paint yet survives the round trip); sets the
//!   session only ever read from an old mapping (no tiles, never
//!   activated) are not rewritten — activating a set to edit it always
//!   re-registers it first. Channel overrides ride for exactly those same
//!   sets (an override on an unwritten set is dropped from the file, the
//!   set-size rule applied to channels).

mod bake_sources;
mod bakes_panel;
mod brush_panel;
mod display_panel;
mod document;
mod empty_state;
mod env;
mod export_dialog;
mod fill_layer;
mod graph_canvas;
mod graph_panel;
mod history_panel;
mod i18n;
#[cfg(test)]
mod input_photon;
mod layers_panel;
mod material_layer;
mod mesh_map_import;
mod paint_state;
mod path_tool;
#[cfg(test)]
mod perf_soak;
mod plugins;
mod postfx_panel;
mod psd_import;
mod retouch;
mod size_presets;
mod spp_import;
mod status_bar;
mod stylus_input;
mod tile_selection;
mod toolbar;
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
    /// The PSD import state (the Assets tab's PSD section).
    psd_import: &'a mut psd_import::PsdImportState,
    spp_import: &'a mut spp_import::SppImportState,
    /// The project's asset store (`None` before the first Save/Open —
    /// the mesh-map import Apply path waits for it; see `mesh_map_import`).
    store: Option<&'a umber_core::AssetStore>,
    polygon_fill: &'a mut fill_layer::PolygonFillTool,
    path_tool: &'a mut path_tool::PaintPathTool,
    /// Embedded material definitions (the session copy of
    /// `ProjectModel::material_defs_mtlx` — the frame refresh reads them,
    /// the Layers panel lists them, the Graph panel extends them).
    material_defs: &'a mut std::collections::BTreeMap<String, String>,
    /// The last failed-refresh message per material layer (the row badge source).
    material_badges: &'a std::collections::HashMap<u64, String>,
    /// The last material drain's one-line summary (`None` = quiet).
    material_status: Option<&'a str>,
    /// The material live-eval states (the explicit-refresh button marks dirty here).
    material_states: &'a mut material_layer::MaterialLayerStates,
    display: &'a mut umber_color::DisplaySettings,
    /// The display LUT's sync state (the Display panel marks it dirty).
    display_lut_state: &'a mut display_panel::DisplayLutState,
    /// The OCIO section's state (the `ocio` feature; the no-feature
    /// build has no field and the call site compiles without it).
    #[cfg(feature = "ocio")]
    ocio_state: &'a mut display_panel::OcioSectionState,
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
    /// This frame's stylus pressure (XI2), or `None` when no stylus
    /// Motion drained this frame — the viewport prefers it over Touch.
    stylus_pressure: Option<f32>,
    /// The Base pseudo-row's composite visibility (stage 4's toggle):
    /// the Layers panel edits this; the end-of-update flush reads it.
    base_visible: &'a mut bool,
    /// Fill layers blocked by the plane budget: layer id → badge line.
    fill_badges: &'a std::collections::HashMap<u64, String>,
    /// Import-created layers with refused publishes: layer id → line.
    import_badges: &'a std::collections::HashMap<u64, String>,
    /// Dormant fill/material layers (§6.3): layer ids.
    dormant: &'a std::collections::HashSet<u64>,
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
                // The eyedropper arm syncs from the strip's tool state each
                // frame (Pick highlights; the brush keeps its last real
                // tool — Pick never strokes, the viewport suppresses paint
                // while armed). A sample becomes the brush color through
                // the paint session's existing color seam.
                let pick_armed = self.brush.strip == toolbar::ToolKind::Pick;
                self.viewport.set_pick_armed(pick_armed);
                // The ID-mask arm (C1 worklist #11): same strip-only shape.
                // A click bakes the mesh's region IDs at the paint target's
                // size and selects the clicked region through the paint
                // session (which arms the quick-mask gate); failures log
                // and leave the mask alone.
                let idmask_armed = self.brush.strip == toolbar::ToolKind::IdMask;
                self.viewport.set_idmask_armed(idmask_armed);
                if let Some(click) = self.viewport.ui(
                    ui,
                    self.gpu,
                    self.paint.as_deref_mut(),
                    self.stylus_pressure,
                    self.display_lut,
                    self.postfx,
                ) {
                    match click {
                        viewport::ViewportClick::Pick(sample) => {
                            // The sample is sRGBA bytes; the brush wants LINEAR
                            // [f32; 4]. Convert here (the same curve the viewport
                            // applied when reading).
                            let [r, g, b, a] = sample.0;
                            let lin = |v: u8| {
                                let s = v as f32 / 255.0;
                                if s <= 0.040_45 {
                                    s / 12.92
                                } else {
                                    ((s + 0.055) / 1.055).powf(2.4)
                                }
                            };
                            let color = [lin(r), lin(g), lin(b), a as f32 / 255.0];
                            if let Some(paint) = self.paint.as_deref_mut() {
                                paint.set_brush_color(color);
                            }
                        }
                        viewport::ViewportClick::IdMask(uv) => {
                            if let (Some(paint), Some(mesh)) =
                                (self.paint.as_mut(), self.mesh.as_ref())
                            {
                                match paint.bake_and_select_id(mesh, uv) {
                                    Ok(id) => log::info!(
                                        "id-mask: selected region [{}, {}, {}]",
                                        id[0],
                                        id[1],
                                        id[2]
                                    ),
                                    Err(err) => {
                                        log::warn!("id-mask: selection failed: {err:#}")
                                    }
                                }
                            }
                        }
                    }
                }
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
            Panel::LayerStack => {
                let mats = layers_panel::MaterialPanelContext {
                    defs: &*self.material_defs,
                    badges: self.material_badges,
                    status: self.material_status,
                    states: &mut *self.material_states,
                };
                // Per-layer pool stage 3 (§2.2) + stage 4 (§4–§6): the panel
                // returns the selection intent; the shell applies it to the
                // paint session OUTSIDE the material-context borrow above.
                let active = self
                    .paint
                    .as_ref()
                    .map(|paint| paint.active_target())
                    .unwrap_or(paint_state::PlaneOwner::Base);
                let badges = document::LayerRowBadges {
                    fill_budget: (*self.fill_badges).clone(),
                    import_refusals: (*self.import_badges).clone(),
                    dormant: (*self.dormant).clone(),
                };
                let outcome =
                    document::layers_ui(ui, self.doc, mats, active, *self.base_visible, &badges);
                if let Some(target) = outcome.select {
                    if let Some(paint) = self.paint.as_deref_mut() {
                        paint.set_active_target(target);
                    }
                }
                if let Some(visible) = outcome.base_visible {
                    *self.base_visible = visible;
                }
                // Stage-4 status lines (the status bar's snapshot is fixed
                // — `status_bar.rs` stays outside the lock — so the
                // LayerStack panel carries them): the plane-budget line and
                // the last stroke refusal. The paint borrow ends first
                // (the panel borrows end here).
                let plane_line = self.paint.as_ref().map(|paint| {
                    (
                        paint.plane_bytes(),
                        paint.is_over_budget(),
                        paint.stroke_refusal().clone(),
                    )
                });
                if let Some((bytes, over, refusal)) = plane_line {
                    if over {
                        ui.separator();
                        ui.label(crate::i18n::tr_args(
                            "layers.budget-line",
                            [
                                ("used", fluent::FluentValue::from(bytes as i64)),
                                (
                                    "cap",
                                    fluent::FluentValue::from(
                                        paint_state::PLANE_BUDGET_BYTES as i64,
                                    ),
                                ),
                            ],
                        ));
                    }
                    if let Some(refusal) = refusal {
                        ui.separator();
                        ui.label(format!(
                            "{}: {refusal}",
                            crate::i18n::tr("layers.stroke-refused")
                        ));
                    }
                }
                // Layer masks need the paint session too (the edit
                // selection is session state there); without one the
                // metadata rows have nothing to route strokes to.
                if let Some(paint) = self.paint.as_deref_mut() {
                    document::layer_mask_ui(ui, self.doc, paint);
                }
            }
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
                ui.separator();
                psd_import::show_section(ui, self.psd_import);
                ui.separator();
                spp_import::show_section(ui, self.spp_import);
            }
            Panel::History => {
                history_panel::history_ui(ui, self.doc, self.paint.as_deref_mut());
                // Single-step stroke buttons below the session log: the
                // log's stroke rows above jump whole depths on the same
                // journal; these step one entry at a time.
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
                let channels: Vec<String> = self
                    .mesh_import
                    .session_set
                    .channels
                    .iter()
                    .map(|channel| channel.name.clone())
                    .collect();
                texture_sets_panel_ui(
                    ui,
                    self.paint.as_deref_mut(),
                    self.mesh_import.session_set.name.as_str(),
                    &channels,
                );
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
                self.graph.show_material_section(ui, self.material_defs);
            }
            Panel::Display => {
                // Scrolls: the postfx section grows one group per effect.
                egui::ScrollArea::vertical().show(ui, |ui| {
                    display_panel::show(
                        ui,
                        self.display,
                        self.display_lut_state,
                        #[cfg(feature = "ocio")]
                        self.ocio_state,
                    );
                    ui.separator();
                    display_panel::hdr_row(ui, self.hdr_mode, &self.gpu.hdr_probe);
                    ui.separator();
                    // The view mode decides whether an active chain is
                    // live or bypassed (data modes) — the status line.
                    let mode = self.viewport.view_mode();
                    postfx_panel::show(ui, self.postfx, self.postfx_panel, mode);
                });
            }
        }
    }
}

/// The last-observed material sizing: the set's size plus every set
/// channel's size (the re-dirty trigger for channel-override resizes —
/// `MaterialLayerState` tracks the set size but not per-channel
/// overrides, so this snapshot is what a resize compares against).
type MaterialSizesSnapshot = ((u32, u32), Vec<(String, (u32, u32))>);

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
    /// The dynamic-material live eval states beside the document (the
    /// `fill_states` precedent): one state per live material layer plus the
    /// dirty queue the frame's refresh drain consumes (Kill B).
    pub material_states: material_layer::MaterialLayerStates,
    /// Embedded material definitions (`material_id` → definition `.mtlx`):
    /// the session copy of `ProjectModel::material_defs_mtlx` — restored on
    /// open, committed on save, extended by the Graph panel's "Save as
    /// material". A copy store (the design's non-goal 3): later
    /// source-graph edits never propagate into saved definitions.
    pub material_defs: std::collections::BTreeMap<String, String>,
    /// The last failed-refresh message per material layer (the Layers
    /// panel's error-badge source; pruned against live layers every drain).
    pub material_badges: std::collections::HashMap<u64, String>,
    /// The last material drain's one-line summary for the Layers panel
    /// (`None` = nothing to report — the status line stays quiet).
    pub material_status: Option<String>,
    /// The channel-size census the last material adoption saw: the active
    /// set's size plus every set channel's size. `MaterialLayerState` tracks
    /// the set size but not per-channel overrides, so a channel-override
    /// resize would otherwise never re-dirty (the design's "size changed
    /// invalidates the caches").
    pub material_sizes: Option<MaterialSizesSnapshot>,
    /// The Base pseudo-row's composite visibility (stage 4's toggle):
    /// session state, never in the layer JSON, never journaled. Rides
    /// every composite snapshot; shown whenever the stack is non-empty
    /// or Base holds planes.
    pub base_visible: bool,
    /// Dormant fill/material layers (§6.3 [B2, M11]): their pre-v2 result
    /// lives in the Base paint, so they seed CLEAN and never re-evaluate
    /// until an explicit edit. Survives save/reopen via `# dormant <id>`
    /// manifest lines while untouched.
    pub dormant: std::collections::HashSet<u64>,
    /// The last frame's composite snapshot (the retire census + `Full`
    /// diff baseline, §4.2/§5.4).
    pub prev_snapshot: Option<paint_state::CompositeSnapshot>,
    /// Fill layers blocked by the plane budget: layer id → badge line
    /// (the Layers rows render these — fills have no `material_badges`
    /// slot).
    pub fill_badges: std::collections::HashMap<u64, String>,
    /// Import-created layers whose plane publish was refused: layer id →
    /// badge line (the layer still exists, empty — §5.6).
    pub import_badges: std::collections::HashMap<u64, String>,
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
    /// The OCIO section's session state (the `ocio` feature): the held
    /// config + the adopted processor. Never serialized — the selection
    /// saves in `display` (DisplaySettings::ocio) and this rebuilds on
    /// load. Absent in no-ocio builds (a zero-sized field compiles the
    /// same call sites with the feature off).
    #[cfg(feature = "ocio")]
    pub ocio_state: display_panel::OcioSectionState,
    /// The post-effects chain (§7:84; Display panel's postfx section).
    /// Live in the viewport whenever an effect is enabled (see
    /// `umber_gpu::postfx_viewport`); saved in the project through the
    /// `postfx` settings carry (only the panel's add-pick + drag state
    /// stays session-only — see `postfx_panel::PostFxPanel`).
    pub postfx: umber_gpu::postfx::PostChain,
    /// The postfx section's UI state (never serialized).
    pub postfx_panel: postfx_panel::PostFxPanel,
    /// Mesh-map import by naming convention (§7 P1 row): the folder-scan
    /// routing + unmatched report + session texture set (channels ensured
    /// on apply). Owns the session set so imported user channels survive
    /// until the next save (see `mesh_map_import`).
    pub mesh_import: mesh_map_import::MeshMapImportState,
    /// PSD flatten-import (requirements §7 row 101, the designed-deferred
    /// clause now built): drop-scan + import state mirroring the mesh-map
    /// shape (composite-first; the honest rasterizer fallback reports
    /// every deviation). The panel section lives in the Assets tab.
    pub psd_import: psd_import::PsdImportState,
    /// The .spp import state (ruling 1: import-only; the reader crate's
    /// report is the artifact this presents).
    pub spp_import: spp_import::SppImportState,
    /// The project directory of the last Save/Open (`None` for a
    /// never-saved document — the import Apply path and the autosave
    /// journal both wait for it).
    pub project_dir: Option<PathBuf>,
    /// The tablet poller (Linux XI2 when the `xi2` feature is compiled
    /// in, Touch fallback otherwise): drained once per frame before the
    /// viewport's paint arm reads, so stylus pressure wins over Touch.
    /// Opens its stream on construction; headless/CI degrades to `None`
    /// (no stream) by design — see `stylus_input`.
    pub stylus: stylus_input::StylusPoller,
}

impl AppState {
    /// Resets the document-scoped state for File > New (the headless half
    /// of [`UmberApp::new_project`], which adds the paint-session rebuild
    /// that needs the GPU): a fresh document (fresh stack + fresh undo
    /// journal + detached autosave journal — the same outcome as
    /// [`document::Document::load_stack`] minus the load), defaulted
    /// import session states, identity display settings with a dirty LUT
    /// flag (the frame loop rebuilds + uploads), the identity postfx
    /// chain ([`umber_gpu::postfx::PostChain::new`]), fresh (empty) material
    /// states/definitions/badges/status/size census (document-scoped — a new
    /// document carries no materials), and no project directory (unsaved).
    ///
    /// Deliberately NOT reset: the loaded mesh + viewport (mesh buffers
    /// have no unload path; clearing the handle alone would leave a ghost
    /// on screen), the viewport environment (a studio asset, not document
    /// state — [`Viewport::set_environment`](viewport::Viewport::set_environment)
    /// is never called here), the graph panel (its plugin paths load at
    /// construction), and the fill/path/bake/export panel states
    /// (session-scoped caches keyed by layer id sit unused once their
    /// layers are gone — the same shape as deleting a layer today). The
    /// dirty-document confirm is the named follow-up: the repo has no
    /// modal pattern (the only `egui::Window` is the Perf HUD), and a
    /// half-modal would be worse than none.
    pub fn reset_for_new(&mut self) {
        self.doc = document::Document::default();
        self.mesh_import = mesh_map_import::MeshMapImportState::default();
        self.psd_import = psd_import::PsdImportState::default();
        self.spp_import = spp_import::SppImportState::default();
        self.display = umber_color::DisplaySettings::default();
        self.display_lut.mark_display_lut_dirty();
        self.postfx = umber_gpu::postfx::PostChain::new();
        self.project_dir = None;
        self.material_states = material_layer::MaterialLayerStates::default();
        self.material_defs = std::collections::BTreeMap::new();
        self.material_badges = std::collections::HashMap::new();
        self.material_status = None;
        self.material_sizes = None;
        // §6.3 [M10]: fill states are session state keyed by layer id —
        // without this reset document B's Fill 3 inherits document A's
        // Fill 3 state (§8 T30).
        self.fill_states = fill_layer::FillLayerStates::default();
        // Stage-4 session pixel state: Base shows again, dormancy and
        // badges die with their document, and the snapshot baseline
        // restarts (the next frame diffs against nothing).
        self.base_visible = true;
        self.dormant.clear();
        self.prev_snapshot = None;
        self.fill_badges.clear();
        self.import_badges.clear();
        // §6.3's session lifecycle: layer ids restart at 0 per document,
        // so layer-owned pixel state must not outlive its document. (The
        // File > New path rebuilds the paint session wholesale right
        // after; this covers every other reset caller.)
        if let Some(paint) = self.paint.as_mut() {
            paint.clear_layer_planes();
        }
    }
}

/// Builds the per-frame composite snapshot (§4.2) from the document
/// stack plus the Base row's visibility: bottom-to-top rows (index 0 is
/// the stack bottom, matching `LayerStack` and the compositor), Base
/// first, and the displayed `(tile, channel)`s (the active tile's
/// baseColor + height — plus the UV canvas's bound channel when the
/// caller passes it).
///
/// Material rows expand per contributing channel (§5.2): the shell reads
/// each layer's [`material_layer::MaterialLayerStates`] plans and forms
/// `layer.opacity × channel.opacity` here, so opacity edits propagate
/// without a re-eval. The layer's own `blend_mode` stays inert on
/// materials; folders carry no planes (no entry); instances emit a row
/// over their SOURCE's planes (stage 5, see the Instance arm).
fn build_composite_snapshot(
    stack: &umber_core::layers::LayerStack,
    states: &material_layer::MaterialLayerStates,
    base_visible: bool,
    displayed: Vec<(u16, String)>,
) -> paint_state::CompositeSnapshot {
    use paint_state::{PlaneOwner, SnapshotLayer, SnapshotLayerKind, SnapshotMaterialChannel};
    let mut layers = vec![SnapshotLayer {
        owner: PlaneOwner::Base,
        layer_id: None,
        kind: SnapshotLayerKind::Paint,
        blend: umber_core::BlendMode::Normal,
        opacity: 1.0,
        visible: base_visible,
        mask_enabled: false,
        material_channels: None,
    }];
    for layer in &stack.layers {
        let owner = PlaneOwner::Layer(layer.id);
        let mask_enabled = layer.mask.as_ref().is_some_and(|mask| mask.gates());
        match layer.kind {
            umber_core::LayerKind::Material => {
                let channels: Vec<SnapshotMaterialChannel> = states
                    .state(layer.id)
                    .map(|state| {
                        state
                            .composite_channels()
                            .iter()
                            .map(|entry| SnapshotMaterialChannel {
                                channel: entry.channel.clone(),
                                blend: entry.blend,
                                opacity: layer.opacity * entry.opacity,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                layers.push(SnapshotLayer {
                    owner,
                    layer_id: Some(layer.id),
                    kind: SnapshotLayerKind::Material,
                    blend: layer.blend_mode,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    mask_enabled,
                    material_channels: Some(channels),
                });
            }
            umber_core::LayerKind::Fill => {
                layers.push(SnapshotLayer {
                    owner,
                    layer_id: Some(layer.id),
                    kind: SnapshotLayerKind::Fill,
                    blend: layer.blend_mode,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    mask_enabled,
                    material_channels: None,
                });
            }
            umber_core::LayerKind::Paint => {
                layers.push(SnapshotLayer {
                    owner,
                    layer_id: Some(layer.id),
                    kind: SnapshotLayerKind::Paint,
                    blend: layer.blend_mode,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    mask_enabled,
                    material_channels: None,
                });
            }
            umber_core::LayerKind::Instance { .. } => {
                // Stage 5 (§5.3): an instance renders its SOURCE's planes
                // (resolved through instance chains) with its OWN
                // visibility, opacity and mask. A Paint/Fill source takes
                // the instance's own blend; a Material source keeps its
                // per-channel blends with the instance's opacity folded in
                // (the material row's `layer.opacity × entry.opacity`
                // shape). A dangling or Folder source contributes nothing.
                let Some(source) = resolve_instance_source(stack, layer.id) else {
                    continue;
                };
                let source_owner = PlaneOwner::Layer(source.id);
                match source.kind {
                    umber_core::LayerKind::Material => {
                        let channels: Vec<SnapshotMaterialChannel> = states
                            .state(source.id)
                            .map(|state| {
                                state
                                    .composite_channels()
                                    .iter()
                                    .map(|entry| SnapshotMaterialChannel {
                                        channel: entry.channel.clone(),
                                        blend: entry.blend,
                                        opacity: layer.opacity * entry.opacity,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        layers.push(SnapshotLayer {
                            owner: source_owner,
                            layer_id: Some(layer.id),
                            kind: SnapshotLayerKind::Material,
                            blend: layer.blend_mode,
                            opacity: layer.opacity,
                            visible: layer.visible,
                            mask_enabled,
                            material_channels: Some(channels),
                        });
                    }
                    umber_core::LayerKind::Paint | umber_core::LayerKind::Fill => {
                        let kind = if source.kind == umber_core::LayerKind::Fill {
                            SnapshotLayerKind::Fill
                        } else {
                            SnapshotLayerKind::Paint
                        };
                        layers.push(SnapshotLayer {
                            owner: source_owner,
                            layer_id: Some(layer.id),
                            kind,
                            blend: layer.blend_mode,
                            opacity: layer.opacity,
                            visible: layer.visible,
                            mask_enabled,
                            material_channels: None,
                        });
                    }
                    umber_core::LayerKind::Folder { .. }
                    | umber_core::LayerKind::Instance { .. } => {}
                }
            }
            umber_core::LayerKind::Folder { .. } => {
                // Folders carry no pixels (compositor judgment 4): no
                // entry (a missing plane is the transparent no-op).
            }
        }
    }
    paint_state::CompositeSnapshot {
        layers,
        base_visible,
        displayed,
    }
}

/// Follows `id`'s instance chain to the first non-instance layer (§5.3:
/// instances render their source transitively). `None` when the chain
/// dangles or loops — `set_instance_source` refuses cycles at attach
/// time, but a corrupt load must not hang the frame, so the walk is
/// bounded by the stack's length.
fn resolve_instance_source(
    stack: &umber_core::layers::LayerStack,
    id: u64,
) -> Option<&umber_core::layers::Layer> {
    let mut current = stack.layer(id)?;
    for _ in 0..=stack.layers.len() {
        match current.kind {
            umber_core::LayerKind::Instance { source_id } => {
                current = stack.layer(source_id)?;
            }
            _ => return Some(current),
        }
    }
    None
}

/// One fill/material layer's open-seeding verdict (§6.3 [B2, M11]):
/// dormant (provenance says its result is baked into Base), clean (its
/// planes hold the result), or dirty (nothing holds a result — evaluate
/// when a mesh is present).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerDormancy {
    /// Seeded CLEAN + badged: a legacy flat plane already contains the
    /// output (v1 header, no planes, non-empty Base), or a previous save
    /// listed it still-dormant (v2 header + `# dormant <id>`). Never
    /// re-evaluates until an explicit edit.
    Dormant,
    /// Seeded CLEAN, unbadged: its planes loaded (or it never needed
    /// any).
    Clean,
    /// Seeded DIRTY: planeless with nothing baked anywhere (v1 + empty
    /// Base, or v2 + not listed) — evaluates normally on mesh arrival.
    Dirty,
}

/// The dormancy decision (§6.3, pure — §8 T18/T31 pin it headless):
/// provenance (the paint manifest's header), not absence. "Planeless"
/// alone cannot tell legacy output baked into Base from never-evaluated.
fn dormancy_for(
    has_planes: bool,
    header_is_v2: bool,
    listed_dormant: bool,
    base_nonempty: bool,
) -> LayerDormancy {
    if has_planes {
        LayerDormancy::Clean
    } else if header_is_v2 {
        if listed_dormant {
            LayerDormancy::Dormant
        } else {
            // A v2 planeless fill/material never evaluated (a fill first
            // seen without a mesh, or a material whose every eval failed):
            // its result lives nowhere, so it is dirty, never dormant.
            LayerDormancy::Dirty
        }
    } else if base_nonempty {
        // A legacy flat plane already contains every fill's and
        // material's destructive output (Multiply and compounded publishes
        // are not invertible): re-evaluating over it would double-count.
        LayerDormancy::Dormant
    } else {
        // A v1 project with an empty Base baked nothing: the layer must
        // evaluate (nothing to double-count).
        LayerDormancy::Dirty
    }
}

/// Publishes one applied mesh-map import into its layer's own plane
/// (§5.6): decodes the PNG (the source path when the scan saw the
/// filesystem, else the stored asset bytes through a temp file),
/// converts straight → premultiplied, and publishes
/// `(Layer(layer_id), tile, channel)`.
///
/// A size mismatch against the plane's `tile_channel_size` is refused
/// loudly (warn log + row badge — the layer still exists, exactly like
/// the sidecar load's loud skip); a plane-budget refusal badges the
/// same way; an undecodable file badges too (imports store raw bytes —
/// only PNG decodes). Import never moves an existing selection.
fn publish_import_plane(
    paint: &mut paint_state::PaintState,
    store: Option<&umber_core::AssetStore>,
    badges: &mut std::collections::HashMap<u64, String>,
    applied: &mesh_map_import::ImportApplied,
) {
    use paint_state::{PaintRefusal, PlaneOwner};
    let id = applied.layer_id;
    let refuse = |badges: &mut std::collections::HashMap<u64, String>, key: &str| {
        badges.insert(id, crate::i18n::tr(key));
    };
    // The PNG bytes: the source path decodes directly; bytes-only
    // fixtures (drops carrying bytes, tests) round-trip through a temp
    // file (`read_png_rgba8` is path-based — `umber-app` carries no
    // direct PNG decoder, and adding one is a dependency change outside
    // this kill's surface).
    let decoded = match applied.source_path.as_ref() {
        Some(path) => umber_export::png::read_png_rgba8(path).map_err(|e| e.to_string()),
        None => (|| -> Result<(u32, u32, std::vec::Vec<u8>), String> {
            let store = store.ok_or_else(|| "no project store".to_string())?;
            let bytes = store.load(&applied.asset).map_err(|e| e.to_string())?;
            static IMPORT_TMP_SEQ: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let seq = IMPORT_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let tmp =
                std::env::temp_dir().join(format!("umber-import-{}-{seq}.png", std::process::id()));
            std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
            let decoded = umber_export::png::read_png_rgba8(&tmp).map_err(|e| e.to_string())?;
            std::fs::remove_file(&tmp).ok();
            Ok(decoded)
        })(),
    };
    let Ok((width, height, rgba)) = decoded else {
        log::warn!("import: layer {id} PNG undecodable; leaving the layer empty");
        refuse(badges, "import.decode-failed");
        return;
    };
    // The reader yields GPU row order (output row 0 = the file's last
    // stored row): the plane's row order, no flip (unlike the sidecar
    // load, whose files store GPU order and flip back).
    let expected = paint.tile_channel_size(applied.tile, &applied.channel);
    if (width, height) != expected {
        let (ew, eh) = expected;
        log::warn!(
            "import: layer {id} is {width}x{height}, want {ew}x{eh}; leaving the layer empty"
        );
        badges.insert(id, crate::i18n::tr("import.size-mismatch"));
        return;
    }
    let plane = material_layer::straight_to_premultiplied(&rgba);
    match paint.publish_plane(
        PlaneOwner::Layer(id),
        applied.tile,
        &applied.channel,
        width,
        height,
        plane,
    ) {
        Ok(()) => {
            badges.remove(&id);
        }
        Err(PaintRefusal::PlaneBudget { bytes }) => {
            log::warn!("import: layer {id} plane budget refused ({bytes} bytes)");
            badges.insert(
                id,
                crate::i18n::tr_args(
                    "import.budget-refused",
                    [("bytes", fluent::FluentValue::from(bytes as i64))],
                ),
            );
        }
        Err(other) => {
            log::warn!("import: layer {id} publish refused ({other:?})");
            refuse(badges, "import.decode-failed");
        }
    }
}

/// Reads the paint manifest's header lines (§6.1 [B2, M11]): whether the
/// commit is v2 (any stage-4+ save), and which layer ids are still
/// dormant. An absent or unreadable manifest counts as legacy (v1, no
/// dormant ids) — erring toward dormancy, the byte-identical side.
fn parse_manifest_header(text: &str) -> (bool, Vec<u64>) {
    let mut is_v2 = false;
    let mut dormant = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "# Umber paint commit v2" {
            is_v2 = true;
        } else if let Some(id) = line.strip_prefix("# dormant ") {
            if let Ok(id) = id.trim().parse::<u64>() {
                dormant.push(id);
            }
        }
    }
    (is_v2, dormant)
}

/// Adapts the live paint session to the headless material seam
/// ([`material_layer::MaterialTileIo`], Kill A): sizes/tiles stay on the
/// session's channel-aware paths (non-side-effecting reads), publishes
/// land in the refreshed layer's OWN plane (premultiplied — never the
/// flattened target, so refreshes replace instead of compounding),
/// anchors resolve to the stack composited THROUGH the anchored layer
/// (stage 5's downward partial flatten, §5.3 — the CPU prefix over
/// `snapshot`, the same rows the frame's flush folds, so an anchor never
/// sees layers above it nor the material's own output), and mesh maps
/// are the Graph panel's flat placeholders
/// at the set size. The bake bridge exposes no live in-memory bakes yet,
/// so the placeholders mirror `graph_panel`'s private stand-ins (same
/// bytes: opaque white AO, tangent-space-up Normal); unknown map names
/// read as absent, so the eval fails loud — never a silent default.
struct PaintMaterialIo<'a> {
    paint: &'a mut paint_state::PaintState,
    set: String,
    live_layers: Vec<u64>,
    /// The stack's composite rows (built once per refresh pass from the
    /// document — the anchor prefix folds exactly what the flush will).
    snapshot: paint_state::CompositeSnapshot,
}

impl material_layer::MaterialTileIo for PaintMaterialIo<'_> {
    fn set_size(&self) -> (u32, u32) {
        self.paint.size_of_set(&self.set)
    }

    fn channel_size(&self, channel: &str) -> (u32, u32) {
        self.paint.size_of_channel(&self.set, channel)
    }

    fn tiles(&self) -> Vec<u16> {
        self.paint.tiles_present()
    }

    fn publish_channel(
        &mut self,
        layer: u64,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<(), paint_state::PaintRefusal> {
        self.paint.publish_plane(
            paint_state::PlaneOwner::Layer(layer),
            tile,
            channel,
            width,
            height,
            rgba,
        )
    }

    fn anchor_image(
        &mut self,
        tile: u16,
        layer: u64,
        channel: &str,
    ) -> Option<(u32, u32, Vec<u8>)> {
        // A deleted source layer is an eval error (never a silent default);
        // a live one resolves to the prefix composite through it (§5.3).
        if !self.live_layers.contains(&layer) {
            return None;
        }
        self.paint
            .read_anchor_prefix(
                &self.snapshot,
                tile,
                channel,
                paint_state::PlaneOwner::Layer(layer),
            )
            .map(|buffer| (buffer.width, buffer.height, buffer.data))
    }

    fn mesh_map_image(&mut self, name: &str) -> Option<(u32, u32, Vec<u8>)> {
        let (width, height) = self.set_size();
        let texels = width as usize * height as usize;
        match name {
            "AO" => Some((width, height, vec![255u8; texels * 4])),
            "Normal" => {
                let mut bytes = Vec::with_capacity(texels * 4);
                for _ in 0..texels {
                    bytes.extend_from_slice(&[128, 128, 255, 255]);
                }
                Some((width, height, bytes))
            }
            _ => None,
        }
    }
}

/// The eframe app.
pub struct UmberApp {
    pub state: AppState,
    dock: DockState<Panel>,
    gpu: GpuContext,
    /// File > New with unsaved work arms this; the confirm window
    /// (drawn in `ui` beside the Perf HUD) offers Save / Discard /
    /// Cancel. `None` = no confirmation pending. `Some(true)` = a
    /// discard was confirmed; `Some(false)` = the user asked to save
    /// first (the save dialog runs, then the reset).
    pending_new: Option<bool>,
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
    /// The view toggles: wireframe (View menu / toolbar / W), ground grid
    /// (G), shading mode (V cycles lit ↔ unlit). Synced into the viewport
    /// each frame; off/off/Lit by default (off renders byte-identical to
    /// the mesh-only frame, Lit is today's render).
    view: toolbar::ViewToggles,
    /// The icon atlas the toolbar's tool strip draws from. Empty until
    /// the lucide raster step lands (see `toolbar`'s module docs), so the
    /// strip shows placeholder glyphs.
    icons: umber_gui::IconAtlas,
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

        // The baked atlas (§11): real pixels from the icon-raster step. The
        // 2x family loads when the screen is hidpi (native scale ≥ 1.75,
        // egui's convention — eframe publishes pixels_per_point into the
        // input BEFORE app creation, wgpu_integration.rs:246-252). Any
        // load failure keeps the empty atlas: every lookup misses cleanly
        // and callers keep their text placeholders (icons.rs' contract) —
        // a missing atlas is never a startup error. Only the Tool family
        // has consumers today (the toolbar's ToolRow); the Panel family's
        // consumers arrive with the panel-icon wave, and its loader call
        // lands with them.
        let hidpi = cc.egui_ctx.input(|input| input.pixels_per_point) >= 1.75;
        let icons =
            umber_gui::icons::load_baked_atlas(&cc.egui_ctx, umber_gui::IconFamily::Tool, hidpi)
                .unwrap_or_else(|err| {
                    log::warn!("icon atlas unavailable ({err}); falling back to empty");
                    umber_gui::IconAtlas::empty()
                });

        Ok(Self {
            state: AppState {
                paint,
                graph,
                // The Base row shows until the user hides it (stage 4's
                // toggle — `Default` would hide it; see `reset_for_new`).
                base_visible: true,
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
            view: toolbar::ViewToggles::default(),
            icons,
            hdr_mode,
            pending_new: None,
        })
    }
}

/// App-level actions (kept out of the App impl to keep that block small).
impl UmberApp {
    /// The one place a [`toolbar::ShellAction`] takes effect: the toolbar
    /// buttons and the shell shortcuts both land here, so a button and its
    /// key cannot drift (see `toolbar`'s module docs for the key table and
    /// the routing calls).
    fn dispatch(&mut self, action: toolbar::ShellAction) {
        use toolbar::ShellAction;
        if self.view.apply(action) {
            return;
        }
        match action {
            ShellAction::NewProject => self.new_project(),
            ShellAction::OpenMesh => self.open_mesh(),
            ShellAction::OpenProject => self.open_project(),
            ShellAction::SaveProject => self.save_project(),
            ShellAction::Undo => {
                let _ = self.state.doc.undo();
            }
            ShellAction::Redo => {
                let _ = self.state.doc.redo();
            }
            // Quick mask (§2's P1 row): needs a paint session; I inverts
            // only while the mask is being edited (outside it, nothing).
            ShellAction::ToggleMaskMode => {
                if let Some(paint) = self.state.paint.as_mut() {
                    paint.toggle_mask_mode();
                }
            }
            ShellAction::InvertMask => {
                if let Some(paint) = self.state.paint.as_mut() {
                    if paint.quick_mask().is_editing() {
                        paint.invert_mask();
                    }
                }
            }
            ShellAction::SelectTool(kind) => self.brush_panel.select_tool(kind),
            // Applied by `ViewToggles::apply` above.
            ShellAction::ToggleWireframe | ShellAction::ToggleGrid | ShellAction::CycleViewMode => {
            }
        }
    }

    /// File > Open Mesh / the toolbar button: picker → [`Self::load_mesh_path`].
    fn open_mesh(&mut self) {
        if let Some(path) = rfd_pick_mesh() {
            self.load_mesh_path(path);
        }
    }

    /// Loads `path` as the session mesh (the picker and a mesh drop on
    /// the window both land here). Failures log; the current mesh stays.
    fn load_mesh_path(&mut self, path: PathBuf) {
        match umber_mesh::load(&path) {
            Ok(mesh) => {
                let n = mesh.vertex_count();
                match self.state.viewport.load_mesh(&self.gpu, &mesh) {
                    Ok(()) => {
                        // The import session set follows the mesh's set
                        // name (channels kept); the paint session's active
                        // set follows it too (new tiles join the mesh's
                        // set — existing tiles keep their sets).
                        let set_name = umber_mesh::texture_set_name(&path, &mesh);
                        self.state.mesh_import.retarget_set(&set_name);
                        if let Some(paint) = self.state.paint.as_mut() {
                            paint.set_active_set(&set_name);
                        }
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

    /// Mesh files dropped on the window this frame load as the mesh (the
    /// empty-state hero's promise; `empty_state` has the routing call).
    /// One mesh per session in v1: the first loads, the rest are logged.
    fn load_dropped_mesh(&mut self, ctx: &egui::Context) {
        let meshes: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| empty_state::is_mesh_file(p))
                .collect()
        });
        let Some((first, rest)) = meshes.split_first() else {
            return;
        };
        for skipped in rest {
            log::warn!(
                "one mesh per session: ignored dropped {}",
                skipped.display()
            );
        }
        self.load_mesh_path(first.clone());
    }

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
    /// user-triggered export at the session target size.
    fn export_paint_png(&mut self) {
        // Flush-first composite read (§1.1 [B3]): the export sees the
        // composite (cache-else-Base), never a stale cache.
        let tile = self.state.paint.as_ref().map(|paint| paint.active_tile());
        if let Some(paint) = self.state.paint.as_mut() {
            if let Some(tile) = tile {
                let _ = paint.read_composite(tile, paint_state::BASE_COLOR_CHANNEL);
            }
        }
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
}

/// The paint sidecar directory inside a `.umber` project: one
/// byte-exact `tile-<N>.png` per persisted paint tile (see the module
/// docs for why a sidecar, not `project.json`).
fn paint_sidecar_dir(project_dir: &Path) -> PathBuf {
    project_dir.join("paint")
}

/// The sidecar PNG for one baseColor paint tile — the legacy FLAT
/// `paint/tile-<N>.png` shape, byte-identical to the v2 layout and
/// untouched by the per-channel tile pool
/// (docs/specs/per-channel-tile-pool-design.md): every v2 project and
/// every one of today's single-channel sessions keeps this exact path.
/// [`paint_channel_tile_path`] is the channel-aware sibling for every
/// OTHER channel, which nests under its own `paint/<channel>/` directory
/// (baseColor deliberately does NOT move there — a smaller, zero-churn
/// diff than the design doc's sketch; see the kill report).
fn paint_tile_path(project_dir: &Path, tile: u16) -> PathBuf {
    paint_sidecar_dir(project_dir).join(format!("tile-{tile}.png"))
}

/// [`paint_tile_path`]'s channel-aware sibling: `paint/tile-<N>.png` for
/// the reserved baseColor channel (identical to `paint_tile_path`),
/// `paint/<channel>/tile-<N>.png` for every other channel.
fn paint_channel_tile_path(project_dir: &Path, channel: &str, tile: u16) -> PathBuf {
    if channel == paint_state::BASE_COLOR_CHANNEL {
        paint_tile_path(project_dir, tile)
    } else {
        paint_sidecar_dir(project_dir)
            .join(channel)
            .join(format!("tile-{tile}.png"))
    }
}

/// The manifest `name`/sidecar-listing form of `(channel, tile)`: the
/// legacy flat `tile-<N>.png` for baseColor (byte-identical to the v2
/// commit shape — see [`paint_tile_path`]'s docs), else
/// `<channel>/tile-<N>.png`. NOT a 6th manifest column: `parse_paint_commit`
/// destructures an exact 5-field array (`main.rs`'s `PaintCommitEntry`
/// codec), so a literal new column would refuse every v2 manifest AND
/// every new manifest a v2 binary tries to roll forward. Encoding the
/// channel in the `name` field itself keeps the codec's arity exactly 5
/// and is what [`parse_paint_sidecar_name`] parses back.
fn paint_sidecar_name(channel: &str, tile: u16) -> String {
    if channel == paint_state::BASE_COLOR_CHANNEL {
        format!("tile-{tile}.png")
    } else {
        format!("{channel}/tile-{tile}.png")
    }
}

/// Parses a manifest `name`/sidecar listing back to its `(owner,
/// channel, tile)` (per-layer-pool design §6.1's parse rule — the trust
/// boundary): a bare `tile-<N>.png` is the reserved baseColor channel's
/// Base plane; `<channel>/tile-<N>.png` is that channel's Base plane;
/// EXACTLY `layers/<u64>/<channel>/tile-<N>.png` is a layer-owned plane.
/// `None` for anything else — including `..`, a backslash, an empty
/// channel, a non-numeric id, and 3 components (`layers/7/tile-<N>.png`
/// rejects). Depth disambiguates a texture-set channel literally named
/// `layers`: `layers/tile-<N>.png` is that channel's Base plane. `None`
/// also for a channel component that is empty, `..`, or holds a backslash
/// (texture-set channel names are arbitrary user/import-derived strings,
/// and the manifest line is space-delimited — this is the trust boundary,
/// not `is_paint_sidecar_name`'s whitelist alone).
fn parse_paint_sidecar_name(name: &str) -> Option<(paint_state::PlaneOwner, String, u16)> {
    let base = paint_state::PlaneOwner::Base;
    let base_color = paint_state::BASE_COLOR_CHANNEL.to_string();
    let parts: Vec<&str> = name.split('/').collect();
    match parts.as_slice() {
        [tile] => {
            let tile = parse_paint_tile_name(tile)?;
            Some((base, base_color, tile))
        }
        [channel, tile] => {
            if channel.is_empty() || *channel == ".." || channel.contains('\\') {
                return None;
            }
            let tile = parse_paint_tile_name(tile)?;
            Some((base, channel.to_string(), tile))
        }
        ["layers", id, channel, tile] => {
            if channel.is_empty() || *channel == ".." || channel.contains('\\') {
                return None;
            }
            let id: u64 = id.parse().ok()?;
            let tile = parse_paint_tile_name(tile)?;
            Some((
                paint_state::PlaneOwner::Layer(id),
                channel.to_string(),
                tile,
            ))
        }
        _ => None,
    }
}

/// The manifest `name`/sidecar-listing form of one plane: Base keeps the
/// legacy flat/channel-qualified shape byte-for-byte
/// ([`paint_sidecar_name`]); a layer-owned plane nests under
/// `layers/<id>/<channel>/` (design §6.1). NOT a 6th manifest column (see
/// [`paint_sidecar_name`'s docs] — `parse_paint_commit`'s exact 5-field
/// destructure is unchanged).
fn paint_plane_sidecar_name(owner: paint_state::PlaneOwner, channel: &str, tile: u16) -> String {
    match owner {
        paint_state::PlaneOwner::Base => paint_sidecar_name(channel, tile),
        paint_state::PlaneOwner::Layer(id) => format!("layers/{id}/{channel}/tile-{tile}.png"),
    }
}

/// The sidecar file holding one plane's PNG: Base keeps the legacy paths
/// byte-for-byte ([`paint_channel_tile_path`]); a layer-owned plane lives
/// under `paint/layers/<id>/<channel>/`. Never under `layers/<set>/` (the
/// layer-JSON tree — `prune_stale_layer_files` deletes every entry there
/// that is not a live `<id>.json`, directories included).
fn paint_plane_path(
    project_dir: &Path,
    owner: paint_state::PlaneOwner,
    channel: &str,
    tile: u16,
) -> PathBuf {
    match owner {
        paint_state::PlaneOwner::Base => paint_channel_tile_path(project_dir, channel, tile),
        paint_state::PlaneOwner::Layer(_) => {
            paint_sidecar_dir(project_dir).join(paint_plane_sidecar_name(owner, channel, tile))
        }
    }
}

/// One-line tag for a plane owner in save/load logs (format strings can't
/// hold field access, so the tag is built here, not at the log site).
fn plane_owner_tag(owner: paint_state::PlaneOwner) -> String {
    match owner {
        paint_state::PlaneOwner::Base => "base".to_string(),
        paint_state::PlaneOwner::Layer(id) => format!("layer {id}"),
    }
}

/// The sidecar file holding the session's paint-target size
/// (`<project>/paint/target_size.txt`, body `"{width}x{height}"`): written
/// on every save — even with zero tiles, so a 4K choice with no paint yet
/// survives the round trip — and read on open only when the sidecar holds
/// no tiles (with tiles, the PNG dims lead: they must fit the pixels they
/// carry). A sidecar file, not `project.json`: the project schema is owned
/// by `umber-core` (another kill's surface) and this crate has no serde
/// dependency. Judgment call: any positive dims round-trip (the combo only
/// offers square presets, but the load path honors what it stored —
/// clamping a hand-grown size back down would drop pixels silently).
fn paint_target_size_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("target_size.txt")
}

/// `"{width}x{height}"` (the `target_size.txt` body).
fn format_paint_target_size(width: u32, height: u32) -> String {
    format!("{width}x{height}")
}

/// Parses [`format_paint_target_size`]'s output; `None` for anything else
/// (a hand-edited file degrades to the session's live size — never a panic
/// and never a zero-size target).
fn parse_paint_target_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once('x')?;
    let (width, height): (u32, u32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// Parses a `tile-<N>.png` sidecar file name back to its tile number;
/// `None` for anything else (the temp files the atomic writers leave on
/// a crash, a foreign file dropped into the folder, …).
fn parse_paint_tile_name(name: &str) -> Option<u16> {
    let rest = name.strip_prefix("tile-")?.strip_suffix(".png")?;
    rest.parse::<u16>().ok()
}

/// The sidecar PNG for one layer-mask plane: `paint/mask-<layer>-tile-<N>.png`
/// (flat, not nested: mask planes are single-channel `u8` stored as RGBA8
/// PNGs through the same writer the paint tiles use, and the flat `mask-`
/// prefix keeps [`parse_paint_tile_name`] — which requires the `tile-`
/// prefix — from ever colliding, while [`collect_paint_tiles`] skips them
/// for the same reason: masks restore through their own pass, never as
/// paint).
fn paint_mask_name(layer: u64, tile: u16) -> String {
    format!("mask-{layer}-tile-{tile}.png")
}

/// Parses [`paint_mask_name`]'s output back to `(layer, tile)`; `None`
/// for anything else (paint tiles, size files, temps, foreign files).
fn parse_paint_mask_name(name: &str) -> Option<(u64, u16)> {
    let rest = name.strip_prefix("mask-")?.strip_suffix(".png")?;
    let (layer, tile) = rest.split_once("-tile-")?;
    Some((layer.parse::<u64>().ok()?, tile.parse::<u16>().ok()?))
}

/// Expands one grayscale mask plane to RGBA8 (gray in RGB, opaque alpha)
/// for the Linear-transfer PNG writer the paint tiles use — the mask
/// sidecar stays decodable by the same reader.
fn mask_to_rgba(values: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for &v in values {
        out.extend_from_slice(&[v, v, v, 255]);
    }
    out
}

/// Takes the red channel back out of a decoded mask PNG (the inverse of
/// [`mask_to_rgba`] — writer and reader agree the plane rides in R; G and
/// B are the same gray by construction, alpha is the constant 255).
fn rgba_to_mask(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).map(|px| px[0]).collect()
}

/// The sidecar file holding the PER-SET paint-target sizes
/// (`<project>/paint/target_sizes.txt`): one `<set>=<width>x<height>`
/// line per sized set plus one `tile-<N>=<set>` assignment line per saved
/// tile (see the module docs). Written on every save alongside the legacy
/// global below.
fn paint_set_sizes_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("target_sizes.txt")
}

/// The parsed `target_sizes.txt`: set sizes, per-channel overrides, plus
/// tile assignments (all may be empty — an empty mapping means "no per-set
/// information", and the load path falls back to the legacy global).
#[derive(Debug, Default, PartialEq, Eq)]
struct PaintSetSizes {
    /// Set name → `(width, height)`, sorted (BTreeMap wire order keeps
    /// saves byte-deterministic).
    sizes: std::collections::BTreeMap<String, (u32, u32)>,
    /// `(set, channel)` → `(width, height)` overrides, sorted. Absent =
    /// inherit the set's size (see `paint_state`'s channel-sizing docs).
    channels: std::collections::BTreeMap<(String, String), (u32, u32)>,
    /// Tile id → set name, sorted.
    tiles: std::collections::BTreeMap<u16, String>,
}

/// Formats [`PaintSetSizes`] for `target_sizes.txt`: size lines first
/// (sorted by set name), then channel overrides (sorted by set, then
/// channel), then tile assignments (sorted by tile). A `#` header names
/// the version so a future format can branch on it. The header stays the
/// `v1` line on purpose: channel lines are ADDITIVE (no file without
/// them changes by a byte), and the parser never branched on the header
/// (it skips `#` lines), so old readers keep their exact behavior.
fn format_paint_set_sizes(mapping: &PaintSetSizes) -> String {
    let mut out = String::from("# Umber paint-set sizes v1\n");
    for (name, (width, height)) in &mapping.sizes {
        out.push_str(&format!("{name}={width}x{height}\n"));
    }
    for ((set, channel), (width, height)) in &mapping.channels {
        out.push_str(&format!("channel/{set}/{channel}={width}x{height}\n"));
    }
    for (tile, set) in &mapping.tiles {
        out.push_str(&format!("tile-{tile}={set}\n"));
    }
    out
}

/// Parses [`format_paint_set_sizes`]'s output; `None` (no mapping — the
/// load path falls back to the legacy global) when NOTHING parsed, even
/// if the file exists but holds only comments/garbage. Malformed lines
/// are skipped (never a panic, never a zero-size target). The `tile-<N>`
/// prefix is RESERVED for assignments: a set literally named `tile-1002`
/// cannot be sized through this file (its line parses as an assignment) —
/// set names are mesh/material-derived, so this never triggers in
/// practice, and the load path never strands a tile over it (an
/// assignment to an unknown name registers that name at the default size).
/// The `channel/` prefix is likewise RESERVED for per-channel overrides:
/// a line `channel/<set>/<channel>=<WxH>` records that channel's size in
/// that set (absent = inherit the set's size, so every pre-channel file
/// loads all-inherit). A set or channel name holding `/` cannot round-trip
/// (the first `/` splits set from channel — the same reserved-separator
/// class as the `tile-` prefix above); the sidecar PNG layout already
/// nests channels as `<channel>/tile-<N>.png`, so `/` in a channel name
/// never reaches this file in practice.
fn parse_paint_set_sizes(text: &str) -> Option<PaintSetSizes> {
    let mut mapping = PaintSetSizes::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let (left, right) = (left.trim(), right.trim());
        if let Some(tile) = left
            .strip_prefix("tile-")
            .and_then(|n| n.parse::<u16>().ok())
        {
            if !right.is_empty() {
                mapping.tiles.insert(tile, right.to_string());
            }
            continue;
        }
        if let Some(rest) = left.strip_prefix("channel/") {
            if let Some((set, channel)) = rest.split_once('/') {
                if !set.is_empty() && !channel.is_empty() {
                    if let Some((width, height)) = parse_paint_target_size(right) {
                        mapping
                            .channels
                            .insert((set.to_string(), channel.to_string()), (width, height));
                    }
                }
            }
            continue;
        }
        if left.is_empty() || right.is_empty() {
            continue;
        }
        if let Some((width, height)) = parse_paint_target_size(right) {
            mapping.sizes.insert(left.to_string(), (width, height));
        }
    }
    (!mapping.sizes.is_empty() || !mapping.channels.is_empty() || !mapping.tiles.is_empty())
        .then_some(mapping)
}

/// One tile the open path refused to restore: its PNG's dims disagree
/// with its set's size (see `load_paint_tiles_reported`). Loud by
/// construction — the report is logged and returned, never swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PaintTileSkip {
    /// The refused tile.
    tile: u16,
    /// The refused channel (`baseColor` for the flat tiles — one tile can
    /// skip its channels independently, so the channel rides the record).
    channel: String,
    /// The set's size the tile was expected at (the CHANNEL's size when
    /// overridden — see `PaintState::tile_channel_size`).
    expected: (u32, u32),
    /// The PNG's actual dims.
    found: (u32, u32),
}

/// What `load_paint_tiles_reported` restored and what it refused.
#[derive(Debug, Default, PartialEq, Eq)]
struct PaintLoadReport {
    /// Tiles uploaded byte-identically.
    restored: usize,
    /// Layer-mask planes uploaded byte-identically.
    masks_restored: usize,
    /// Tiles skipped for size mismatch (loud — logged per tile).
    skipped: Vec<PaintTileSkip>,
    /// Whether any loaded Base plane holds a non-zero byte (§6.3): the
    /// dormancy decision's "Base is non-empty" arm. Scanned once at open
    /// (a fully-transparent Base baked nothing — its fills must evaluate,
    /// not doze).
    base_nonempty: bool,
}

/// One Texture Sets channel row's combo state: inherit the set's size
/// (the default — no override recorded), a square `SIZE_PRESETS` pick, or
/// a stored non-square override the combo only DISPLAYS (the UI writes
/// squares; the load path honors what it stored — the set combo's rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelSizeChoice {
    Inherit,
    Preset(u32),
    Custom(u32, u32),
}

impl ChannelSizeChoice {
    /// The combo's initial state for a recorded override (`None` =
    /// inherit): a square size becomes its preset row, a non-square its
    /// display-only custom.
    fn from_override(current: Option<(u32, u32)>) -> Self {
        match current {
            None => Self::Inherit,
            Some((size, other)) if size == other => Self::Preset(size),
            Some((width, height)) => Self::Custom(width, height),
        }
    }

    /// The combo's selected text for this state.
    fn label(self) -> String {
        match self {
            Self::Inherit => tr("texturesets.inherit"),
            Self::Preset(size) => size_presets::preset_label(size),
            Self::Custom(width, height) => format!("{width} × {height}"),
        }
    }
}

/// The Texture Sets panel's per-set paint-target size section (CORE-LOOP
/// CRITICAL 2, the 2026-10-12 strategy review): one selectable row per
/// known set (the session set is always listed — mesh load retargets it),
/// and the shared `SIZE_PRESETS` combo (512..8K — the same list the Bakes
/// panel and the Export dialog offer) editing the SELECTED set's size
/// through [`paint_state::PaintState::resize_set`]. Selecting a set also
/// activates it (new tiles join it; the mask rows read it). The resize is
/// INTENTIONAL-clear (that set's canvas, the session stroke-undo history
/// and that set's quick mask all reset — see `resize_set`): the hint
/// string names it per set, so a size change is never a silent undo loss.
/// The choice persists per set (`target_sizes.txt` — see
/// [`save_paint_tiles`]).
///
/// Single-set sessions (everything today: one mesh, one session set) show
/// one row and behave exactly as the v1 global combo did. No paint session
/// (no GPU target) shows the unavailable string instead of the rows.
///
/// Per-channel sizes (requirements §2's sizing row, the deferred half)
/// ride the same panel: one row per document channel under the set's
/// combo, each the shared `SIZE_PRESETS` combo plus the `inherit` default.
/// A channel WITHOUT an override reads the set's size — inherit stays the
/// default row state, and baseColor has no row (it IS the set's size —
/// the set combo above edits it). Picking a size records the override and
/// rebuilds that channel's targets through
/// [`paint_state::PaintState::resize_channel`]; picking inherit drops the
/// override and rebuilds at the set's size. The same intentional-clear
/// contract as the set resize (that channel's canvas plus the session
/// stroke-undo history) — the hint string below names it.
fn texture_sets_panel_ui(
    ui: &mut Ui,
    paint: Option<&mut paint_state::PaintState>,
    session_set: &str,
    channels: &[String],
) {
    let Some(paint) = paint else {
        ui.label(tr("texturesets.no-paint"));
        return;
    };
    paint.ensure_set(session_set);
    // Selection first (owned, so the borrow ends before the size combo
    // and the activation mutate the session below).
    let mut active = paint.active_set().to_string();
    let sets = paint.set_names();
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("texturesets.set"));
        for name in &sets {
            let (width, height) = paint.size_of_set(name);
            let tiles = paint.tiles_of_set(name).len();
            ui.selectable_value(
                &mut active,
                name.clone(),
                format!("{name} ({width}×{height}, {tiles})"),
            );
        }
    });
    if active != paint.active_set() {
        paint.set_active_set(&active);
    }
    let (width, height) = paint.size_of_set(&active);
    let mut size = width;
    ui.horizontal(|ui| {
        ui.label(tr("texturesets.size"));
        size_presets::size_combo(ui, "", &mut size);
    });
    if (size, size) != (width, height) {
        paint.resize_set(&active, size, size);
    }
    ui.label(tr("texturesets.resize-clears"));
    // Per-channel rows for the SELECTED set: the document's channels plus
    // any materialized-but-unlisted one (an override outliving its import
    // still edits — dropping it from the panel would strand it).
    let mut rows: Vec<String> = channels.to_vec();
    for tile in paint.tiles_of_set(&active) {
        for channel in paint.channels_present(tile) {
            if !rows.contains(&channel) {
                rows.push(channel);
            }
        }
    }
    rows.sort();
    rows.retain(|channel| channel.as_str() != paint_state::BASE_COLOR_CHANNEL);
    if rows.is_empty() {
        return;
    }
    ui.label(tr("texturesets.per-channel"));
    for channel in &rows {
        let current = paint.channel_override(&active, channel);
        let initial = ChannelSizeChoice::from_override(current);
        let mut picked = initial;
        ui.horizontal(|ui| {
            ui.label(channel.as_str());
            egui::ComboBox::from_id_salt(format!("channel-size:{active}/{channel}"))
                .selected_text(picked.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut picked,
                        ChannelSizeChoice::Inherit,
                        tr("texturesets.inherit"),
                    );
                    for size in size_presets::SIZE_PRESETS {
                        ui.selectable_value(
                            &mut picked,
                            ChannelSizeChoice::Preset(size),
                            size_presets::preset_label(size),
                        );
                    }
                });
        });
        if picked != initial {
            match picked {
                ChannelSizeChoice::Inherit => paint.clear_channel_size(&active, channel),
                ChannelSizeChoice::Preset(size) => {
                    paint.resize_channel(&active, channel, size, size);
                }
                // Display-only (never offered as a row): a changed pick is
                // always inherit or a square preset.
                ChannelSizeChoice::Custom(..) => {}
            }
        }
    }
    ui.label(tr("texturesets.channel-resize-clears"));
}

/// Vertical row flip over tightly-packed RGBA8 bytes: output row 0 is
/// input row `height - 1`. Compensates `read_png_rgba8`'s equirect GPU
/// row-order flip on the load path (and is its own inverse, which the
/// round-trip test pins). A length mismatch returns the input unchanged
/// rather than panicking — the callers' bytes always match; this guards
/// only against a future caller passing a sliced region.
fn flip_rows_vertical(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = width as usize * 4;
    let rows = height as usize;
    if rgba.len() != stride * rows {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; rgba.len()];
    for row in 0..rows {
        let src = row * stride;
        let dst = (rows - 1 - row) * stride;
        out[dst..dst + stride].copy_from_slice(&rgba[src..src + stride]);
    }
    out
}

/// The pending paint commit (`<project>/paint/manifest.pending`): written
/// (tmp+rename) only once every artifact of a save is staged. Its rename
/// is the save's ONE commit point (see the module docs' commit protocol).
fn paint_commit_pending_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("manifest.pending")
}

/// The applied commit, kept as the provenance record
/// (`<project>/paint/manifest.txt`: each sidecar name → its content hash
/// in the project's `AssetStore`). The load path never reads it — the
/// sidecar files stay authoritative, so v1/v2 readers are unaffected.
fn paint_manifest_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("manifest.txt")
}

/// Whether `name` is a file a paint commit may write under `paint/`: a
/// flat or channel-qualified `tile-<N>.png`, a layer plane
/// `layers/<id>/<channel>/tile-<N>.png` (see
/// [`parse_paint_sidecar_name`]), a layer-mask `mask-<layer>-tile-<N>.png`
/// (see [`parse_paint_mask_name`]), or one of the two size files. Recovery
/// refuses a manifest naming anything else (a hand-edited `../x` never
/// escapes the folder).
fn is_paint_sidecar_name(name: &str) -> bool {
    parse_paint_sidecar_name(name).is_some()
        || parse_paint_mask_name(name).is_some()
        || name == "target_size.txt"
        || name == "target_sizes.txt"
}

/// The staged temp name for `name`: `<name>.tmp.<pid>` (the shape
/// `parse_paint_tile_name` already ignores).
fn paint_temp_name(name: &str) -> String {
    format!("{name}.tmp.{}", std::process::id())
}

/// Whether `name` is a staged temp (`<anything>.tmp.<digits>`) — what an
/// uncommitted save leaves behind, swept on the next open/save.
fn is_paint_temp_name(name: &str) -> bool {
    let Some((_, pid)) = name.rsplit_once(".tmp.") else {
        return false;
    };
    !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
}

/// One staged artifact of a paint commit (one manifest line).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PaintCommitEntry {
    /// Final sidecar name under `paint/` (`tile-1001.png`, …).
    name: String,
    /// Staged temp name under `paint/` (`<name>.tmp.<pid>`).
    temp: String,
    /// The content-hash blob in the project's `AssetStore` — dedupe plus
    /// provenance, and the roll-forward source once the temp is gone.
    asset: umber_core::AssetRef,
}

/// Formats a commit manifest: the `# Umber paint commit v2` provenance
/// header (§6.1 [B2, M11] — every save from stage 4 on, regardless of
/// `project.json`'s version), one `# dormant <id>` line per still-dormant
/// layer (§6.3 — so dormancy survives a save/reopen the user never
/// touched), then one `<name> <hash> <ext> <size> <temp>` line per entry
/// (names never hold spaces — `is_paint_sidecar_name` admits none).
fn format_paint_commit(entries: &[PaintCommitEntry], dormant: &[u64]) -> String {
    let mut out = String::from("# Umber paint commit v2\n");
    let mut dormant = dormant.to_vec();
    dormant.sort_unstable();
    dormant.dedup();
    for id in dormant {
        out.push_str(&format!("# dormant {id}\n"));
    }
    for entry in entries {
        let (name, temp, asset) = (&entry.name, &entry.temp, &entry.asset);
        let (hash, ext, size) = (&asset.hash, &asset.ext, asset.size);
        out.push_str(&format!("{name} {hash} {ext} {size} {temp}\n"));
    }
    out
}

/// Parses [`format_paint_commit`]'s output. `None` when ANY line is
/// malformed: the manifest is renamed into place whole, so a bad line
/// means tampering, and applying half of it would be the torn state the
/// protocol exists to prevent.
fn parse_paint_commit(text: &str) -> Option<Vec<PaintCommitEntry>> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(' ').collect();
        let [name, hash, ext, size, temp] = fields.as_slice() else {
            return None;
        };
        let asset = umber_core::AssetRef {
            hash: hash.to_string(),
            ext: ext.to_string(),
            size: size.parse().ok()?,
        };
        let temp_ok = temp
            .strip_prefix(*name)
            .is_some_and(|rest| rest.starts_with(".tmp.") && is_paint_temp_name(temp));
        if !is_paint_sidecar_name(name) || !temp_ok || asset.validate().is_err() {
            return None;
        }
        entries.push(PaintCommitEntry {
            name: name.to_string(),
            temp: temp.to_string(),
            asset,
        });
    }
    Some(entries)
}

/// The on-disk blob path of a stored asset. Mirrors `AssetStore`'s
/// private shard layout (`assets/<hash[..2]>/<hash>.<ext>`), needed only
/// to fsync the blob before the commit; the
/// `asset_blob_path_matches_the_store_layout` test pins it against a real
/// store, so a layout change fails a test instead of durability.
fn paint_asset_blob_path(project_dir: &Path, asset: &umber_core::AssetRef) -> PathBuf {
    let (hash, ext) = (&asset.hash, &asset.ext);
    project_dir
        .join("assets")
        .join(&hash[..2])
        .join(format!("{hash}.{ext}"))
}

/// Flushes a file to stable storage (write access: Windows'
/// `FlushFileBuffers` refuses a read-only handle).
fn sync_file(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .sync_all()
}

/// Flushes a directory's entries (the renames) to stable storage. Unix
/// only: Windows cannot open a directory as a file, and NTFS journals
/// renames itself.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Finishes staging the temp already written at `paint/<temp>`: fsyncs
/// it, stores its bytes in the project's `AssetStore` (deduped by
/// content hash) and fsyncs the blob — both are durable before any
/// manifest can name them.
fn finish_paint_stage(
    project_dir: &Path,
    store: &umber_core::AssetStore,
    name: &str,
    temp: String,
    bytes: &[u8],
) -> std::io::Result<PaintCommitEntry> {
    sync_file(&paint_sidecar_dir(project_dir).join(&temp))?;
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let asset = store.store(bytes, ext).map_err(std::io::Error::other)?;
    sync_file(&paint_asset_blob_path(project_dir, &asset))?;
    Ok(PaintCommitEntry {
        name: name.to_string(),
        temp,
        asset,
    })
}

/// Stages `bytes` as the next commit's `name` (temp + store blob).
/// Creates `name`'s parent directory first — a no-op for a flat name
/// (the sidecar directory already exists), the channel subdirectory's
/// first write otherwise (`paint/<channel>/` nests one level under
/// `paint/`, created lazily like the tile pool it mirrors).
fn stage_paint_bytes(
    project_dir: &Path,
    store: &umber_core::AssetStore,
    name: &str,
    bytes: &[u8],
) -> std::io::Result<PaintCommitEntry> {
    let temp = paint_temp_name(name);
    let path = paint_sidecar_dir(project_dir).join(&temp);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes)?;
    finish_paint_stage(project_dir, store, name, temp, bytes)
}

/// Stages one tile readback as a Linear-transfer PNG temp (the same
/// `write_png` call the sidecar always used — byte-exact), then stores
/// the encoded bytes. Creates `name`'s parent directory first, like
/// [`stage_paint_bytes`].
fn stage_paint_png(
    project_dir: &Path,
    store: &umber_core::AssetStore,
    name: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> std::io::Result<PaintCommitEntry> {
    let temp = paint_temp_name(name);
    let path = paint_sidecar_dir(project_dir).join(&temp);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    umber_export::png::write_png(
        &path,
        width,
        height,
        rgba,
        umber_export::png::Transfer::Linear,
    )
    .map_err(std::io::Error::other)?;
    let bytes = std::fs::read(&path)?;
    finish_paint_stage(project_dir, store, name, temp, &bytes)
}

/// Writes the commit manifest: temp, fsync, then the rename to
/// `manifest.pending` — THE commit point. Before it returns, an
/// interrupted save leaves the old sidecar intact; after, the next open
/// rolls the new one forward.
fn commit_paint_manifest(
    project_dir: &Path,
    entries: &[PaintCommitEntry],
    dormant: &[u64],
) -> std::io::Result<()> {
    let dir = paint_sidecar_dir(project_dir);
    let temp = dir.join(paint_temp_name("manifest.pending"));
    std::fs::write(&temp, format_paint_commit(entries, dormant))?;
    sync_file(&temp)?;
    std::fs::rename(&temp, paint_commit_pending_path(project_dir))?;
    // Past the commit point an error must not read as "not committed".
    if let Err(e) = sync_dir(&dir) {
        log::warn!("paint save: committed, directory fsync failed: {e:#}");
    }
    Ok(())
}

/// Every paint artifact currently on disk under `dir`, as the SAME
/// relative name the manifest uses: a flat `tile-<N>.png` at the top level
/// (the reserved baseColor channel), `<channel>/tile-<N>.png` one level
/// into a channel subdirectory, or `layers/<id>/<channel>/tile-<N>.png`
/// two levels into the layer tree (design §6.1 — exactly as deep as the
/// per-layer layout ever nests). Layer-mask `mask-<layer>-tile-<N>.png`
/// files are listed too (flat, top level only): the prune step must drop a
/// detached layer's mask just like a dropped tile, or re-saves resurrect
/// dead masks.
/// Used by the prune step so a dropped tile, a dropped channel's tile, or
/// a dropped layer's planes are pruned regardless of which directory they
/// live in. Anything the parse rule rejects (a 3-component `layers/…`
/// path, foreign junk) is never listed and never pruned — the prune only
/// removes what the manifest's trust boundary admits.
fn list_paint_tile_names(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if !path.is_dir() {
            if parse_paint_tile_name(&name).is_some() || parse_paint_mask_name(&name).is_some() {
                names.push(name);
            }
            continue;
        }
        if name == "layers" {
            for id_entry in std::fs::read_dir(&path)?.flatten() {
                let id_name = id_entry.file_name().to_string_lossy().into_owned();
                let id_path = id_entry.path();
                if !id_path.is_dir() {
                    continue;
                }
                for channel_entry in std::fs::read_dir(&id_path)?.flatten() {
                    let channel_name = channel_entry.file_name().to_string_lossy().into_owned();
                    let channel_path = channel_entry.path();
                    if !channel_path.is_dir() {
                        continue;
                    }
                    for inner in std::fs::read_dir(&channel_path)?.flatten() {
                        let inner_name = inner.file_name().to_string_lossy().into_owned();
                        if parse_paint_tile_name(&inner_name).is_some() {
                            names.push(format!("layers/{id_name}/{channel_name}/{inner_name}"));
                        }
                    }
                }
            }
            continue;
        }
        for inner in std::fs::read_dir(&path)?.flatten() {
            let inner_name = inner.file_name().to_string_lossy().into_owned();
            if parse_paint_tile_name(&inner_name).is_some() {
                names.push(format!("{name}/{inner_name}"));
            }
        }
    }
    Ok(names)
}

/// Removes the now-empty directories under `paint/layers/` bottom-up
/// (`<channel>/`, then `<id>/` — design §6.1 [m10]), so a deleted layer
/// leaves no empty tree behind. Only directories under `paint/layers/`
/// are candidates; a directory that still holds any file (including
/// parse-rejected junk the prune never lists) survives.
fn prune_empty_layer_dirs(project_dir: &Path) {
    let layers = paint_sidecar_dir(project_dir).join("layers");
    let Ok(ids) = std::fs::read_dir(&layers) else {
        return;
    };
    for id in ids.flatten() {
        let id_path = id.path();
        if !id_path.is_dir() {
            continue;
        }
        if let Ok(channels) = std::fs::read_dir(&id_path) {
            for channel in channels.flatten() {
                let channel_path = channel.path();
                let empty = channel_path.is_dir()
                    && std::fs::read_dir(&channel_path)
                        .is_ok_and(|mut entries| entries.next().is_none());
                if empty {
                    if let Err(e) = std::fs::remove_dir(&channel_path) {
                        log::warn!(
                            "paint save: empty layer dir {} not removed: {e}",
                            channel_path.display()
                        );
                    }
                }
            }
        }
        let empty = std::fs::read_dir(&id_path).is_ok_and(|mut entries| entries.next().is_none());
        if empty {
            if let Err(e) = std::fs::remove_dir(&id_path) {
                log::warn!(
                    "paint save: empty layer dir {} not removed: {e}",
                    id_path.display()
                );
            }
        }
    }
}

/// Applies a pending commit (idempotent — safe to re-run after a crash at
/// any step): renames each staged temp over its sidecar name; an entry
/// whose temp is already gone is checked against its store blob and
/// re-materialized from it if the sidecar differs; tile PNGs the manifest
/// does not list are pruned (a re-save never resurrects a dropped tile);
/// finally `manifest.pending` becomes `manifest.txt`. Returns whether a
/// commit was pending.
///
/// `max_steps` is the fault-injection seam: `None` runs to completion
/// (every production caller); `Some(k)` fails the (k+1)-th mutating step
/// with an injected error, leaving the disk exactly as a crash there
/// would — including a half-written temp when the step was a write.
fn apply_paint_commit(project_dir: &Path, max_steps: Option<usize>) -> std::io::Result<bool> {
    let dir = paint_sidecar_dir(project_dir);
    let pending = paint_commit_pending_path(project_dir);
    let text = match std::fs::read_to_string(&pending) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let entries = parse_paint_commit(&text).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed paint manifest")
    })?;
    let store = umber_core::AssetStore::new(project_dir);
    let mut steps = 0usize;
    let mut step = || {
        if max_steps.is_some_and(|max| steps >= max) {
            return Err(std::io::Error::other("injected fault"));
        }
        steps += 1;
        Ok(())
    };
    for entry in &entries {
        let temp = dir.join(&entry.temp);
        let target = dir.join(&entry.name);
        if temp.exists() {
            step()?;
            std::fs::rename(&temp, &target)?;
            continue;
        }
        // The temp is gone: this rename already ran, or the temp was lost.
        // Either way the content-addressed blob is the committed truth.
        let blob = store.load(&entry.asset).map_err(std::io::Error::other)?;
        if std::fs::read(&target).is_ok_and(|bytes| bytes == blob) {
            continue;
        }
        // Re-materialize through a temp the manifest never names: a crash
        // mid-write leaves a torn `.restore` orphan (swept later), never a
        // torn temp the next recovery would rename into place.
        let restore = dir.join(paint_temp_name(&format!("{}.restore", entry.name)));
        if let Err(fault) = step() {
            let _ = std::fs::write(&restore, &blob[..blob.len() / 2]);
            return Err(fault);
        }
        std::fs::write(&restore, &blob)?;
        sync_file(&restore)?;
        std::fs::rename(&restore, &target)?;
    }
    let listed: std::collections::HashSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    for name in list_paint_tile_names(&dir)? {
        if !listed.contains(name.as_str()) {
            step()?;
            std::fs::remove_file(dir.join(&name))?;
        }
    }
    // A deleted layer's planes prune as files above; their now-empty
    // directories go here (bottom-up, `paint/layers/` only).
    prune_empty_layer_dirs(project_dir);
    sync_dir(&dir)?;
    step()?;
    std::fs::rename(&pending, paint_manifest_path(project_dir))?;
    sync_dir(&dir)?;
    Ok(true)
}

/// Crash recovery, run before anything reads or stages `paint/`: rolls a
/// pending commit forward (an interrupted save's NEW state), then sweeps
/// staged temps no commit names (an interrupted save's uncommitted
/// leftovers — the OLD state stands). A failed roll-forward keeps the
/// temps (they are its source) and logs; the next open retries.
fn recover_paint_sidecar(project_dir: &Path) {
    match apply_paint_commit(project_dir, None) {
        Ok(true) => log::info!("paint: rolled an interrupted save forward"),
        Ok(false) => {}
        Err(e) => {
            log::warn!("paint: interrupted save not rolled forward yet: {e:#}");
            return;
        }
    }
    sweep_paint_temps(&paint_sidecar_dir(project_dir));
}

/// Removes every staged temp under `dir`, recursing fully (channel
/// subdirectories AND the `layers/` tree) so a crash mid-save on any plane
/// leaves no orphan `<name>.tmp.<pid>` behind either.
fn sweep_paint_temps(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sweep_paint_temps(&path);
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_paint_temp_name(&name) {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("paint: orphan temp {} not removed: {e}", path.display());
            }
        }
    }
}

/// What `commit_paint_tiles` staged: `staged` counts manifest-listed
/// plane PNGs (a clean-skipped plane re-stages its on-disk bytes, so it
/// counts); `read_back` counts GPU readbacks — the §8 T20 observable (an
/// in-place save right after open reads back nothing).
#[derive(Debug, Default, PartialEq, Eq)]
struct PaintSaveReport {
    /// Plane PNGs listed in the committed manifest.
    staged: usize,
    /// Planes read back from the GPU (`read_plane` calls).
    read_back: usize,
}

/// §6.2 condition 1: the save's target project dir equals the plane's
/// committed dir. Compared after `std::fs::canonicalize`; a canonicalize
/// failure on either side counts as different.
fn same_project_dir(target: &Path, committed: &Path) -> bool {
    match (
        std::fs::canonicalize(target),
        std::fs::canonicalize(committed),
    ) {
        (Ok(current), Ok(recorded)) => current == recorded,
        _ => false,
    }
}

/// §6.3's minimal bump [M2]: the save writes version 2 iff it will stage
/// at least one plane whose owner is a `Layer(id)` LIVE in the stack.
/// Orphans and tombstones don't count (`live` is the stack's ids), and a
/// layer-less or headless session stays version 1 (readable by old
/// builds). Computable from the session before either file is written. A
/// loaded model is never re-stamped: the save recomputes the version every
/// time, so re-saving a version-2 project after deleting its only planed
/// layer writes 1 again (§8 T24).
fn paint_save_version(
    paint: &paint_state::PaintState,
    live: &std::collections::HashSet<u64>,
) -> u32 {
    let has_live_layer_plane = paint.planes_present().iter().any(
        |(owner, _, _)| matches!(owner, paint_state::PlaneOwner::Layer(id) if live.contains(id)),
    );
    if has_live_layer_plane {
        umber_core::project::LAYER_PLANES_PROJECT_VERSION
    } else {
        umber_core::project::CURRENT_PROJECT_VERSION
    }
}

/// Writes every materialized plane — Base AND every live layer-owned one —
/// into the project sidecar as a Linear-transfer PNG (byte-exact — the
/// export dialog's `Srgb` would move the bytes through the IEC curve),
/// crash-safely: every artifact is staged (temp + `AssetStore` blob)
/// before ONE manifest rename commits the save (see the module docs).
/// Reads through `PaintState::read_plane` for EVERY owner, Base included
/// (design §1.1 [B3]): the save loop is a PLANE reader, never a composite
/// reader — writing the cache into `paint/tile-<N>.png` would double-count
/// every layer on the next reopen (§8 T21).
///
/// A clean plane may skip its readback ONLY under §6.2's two-part test
/// [B1]: the target dir equals the plane's committed dir (canonicalized)
/// AND the target dir's sidecar file for that plane hashes (blake3) to the
/// recorded commit hash. The check re-reads and re-hashes the file — a
/// file read plus blake3, far cheaper than a GPU readback plus PNG encode.
/// Every other case reads back and encodes: `committed == None`, a
/// different dir (save-as), a missing file, a hash mismatch.
///
/// A failed readback keeps the artifact's previous sidecar (if any) ONLY
/// for an in-place save: re-staging reads the TARGET dir's file, and a
/// save-as must never re-stage bytes from a dir the plane was not committed
/// to (that was r1's silent-drop shape — every clean plane lost from a
/// save-as, Base included). In a save-as a failed readback is a loud
/// per-plane failure (warn log), never a silent omission.
///
/// `live` is the document stack's layer ids: orphan planes (an id no
/// longer in the stack) are never staged — the next save's manifest prunes
/// them (§6.4) — and never count toward the version decision (§6.3).
/// Prunes sidecar files for planes the session no longer holds, then
/// returns what was staged and what was read back (a zero-`staged` commit
/// still commits the size files; an `Err` means the commit point was never
/// reached — the previous paint stays intact).
fn save_paint_tiles(
    paint: &mut paint_state::PaintState,
    project_dir: &Path,
    live: &std::collections::HashSet<u64>,
    dormant: &[u64],
) -> usize {
    match commit_paint_tiles(paint, project_dir, live, dormant) {
        Ok(report) => report.staged,
        Err(e) => {
            log::warn!("paint save: not committed, the previous paint stays intact: {e:#}");
            0
        }
    }
}

/// [`save_paint_tiles`]'s body: stage everything, commit, apply. An `Err`
/// means the commit point was never reached.
fn commit_paint_tiles(
    paint: &mut paint_state::PaintState,
    project_dir: &Path,
    live: &std::collections::HashSet<u64>,
    dormant: &[u64],
) -> std::io::Result<PaintSaveReport> {
    std::fs::create_dir_all(paint_sidecar_dir(project_dir))?;
    // An earlier crash's pending commit lands (or its orphans go) before
    // this save stages anything.
    recover_paint_sidecar(project_dir);
    let store = umber_core::AssetStore::new(project_dir);
    let mut entries = Vec::new();
    // The session sizes ride along even with zero tiles (a size choice
    // with no paint yet must survive the round trip). The legacy global
    // first (the active set's size — old readers apply one size to every
    // tile, and with pixels the PNG dims lead anyway).
    let (width, height) = paint.target_size();
    entries.push(stage_paint_bytes(
        project_dir,
        &store,
        "target_size.txt",
        format_paint_target_size(width, height).as_bytes(),
    )?);
    // The per-set mapping: sizes for every set holding a saved tile
    // plus the active set (a chooser with no paint yet survives), and
    // one assignment line per saved tile. Sorted by construction
    // (BTreeMap), so saves are byte-deterministic.
    let tiles = paint.tiles_present();
    let mut mapping = PaintSetSizes::default();
    for tile in &tiles {
        let set = paint.set_of_tile(*tile).to_string();
        mapping.tiles.insert(*tile, set.clone());
        mapping.sizes.insert(set, paint.tile_size(*tile));
    }
    let active = paint.active_set().to_string();
    if !mapping.sizes.contains_key(&active) {
        mapping
            .sizes
            .insert(active.clone(), paint.size_of_set(&active));
    }
    // The per-channel overrides for exactly those same sets (the set-size
    // rule applied to channels — see the module docs); sorted by
    // construction (BTreeMap), so saves stay byte-deterministic.
    for ((set, channel), (width, height)) in paint.channel_sizes() {
        if mapping.sizes.contains_key(set) {
            mapping
                .channels
                .insert((set.clone(), channel.clone()), (*width, *height));
        }
    }
    entries.push(stage_paint_bytes(
        project_dir,
        &store,
        "target_sizes.txt",
        format_paint_set_sizes(&mapping).as_bytes(),
    )?);
    let mut report = PaintSaveReport::default();
    // The plane census (§1.1): every materialized `(owner, tile, channel)`
    // stages its own file. Collected up front (owned) so no session borrow
    // is held across the `&mut` readback/commit calls below.
    for (owner, tile, channel) in paint.planes_present() {
        if let paint_state::PlaneOwner::Layer(id) = owner {
            if !live.contains(&id) {
                continue;
            }
        }
        let name = paint_plane_sidecar_name(owner, &channel, tile);
        let tag = plane_owner_tag(owner);
        let committed = paint.plane_committed(owner, tile, &channel);
        // §6.2's clean-skip [B1]: same dir AND same bytes on disk.
        if let Some((committed_dir, committed_hash)) = committed.as_ref() {
            if same_project_dir(project_dir, committed_dir) {
                let path = paint_sidecar_dir(project_dir).join(&name);
                match std::fs::read(&path) {
                    Ok(bytes) if blake3::hash(&bytes) == *committed_hash => {
                        let entry = stage_paint_bytes(project_dir, &store, &name, &bytes)?;
                        paint.mark_plane_committed(
                            owner,
                            tile,
                            &channel,
                            project_dir.to_path_buf(),
                            *committed_hash,
                        );
                        entries.push(entry);
                        report.staged += 1;
                        continue;
                    }
                    Ok(_) => {
                        log::warn!(
                            "paint save: {tag}/{channel} tile {tile} changed on disk since commit; reading back"
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "paint save: {tag}/{channel} tile {tile} sidecar unreadable ({e:#}); reading back"
                        );
                    }
                }
            }
        }
        let Some(image) = paint.read_plane(owner, tile, &channel) else {
            // A failed readback keeps the artifact's previous sidecar
            // (if any): it is re-staged as-is so the commit does not
            // prune it — but ONLY for an in-place save (condition 1
            // above). In a save-as there is no committed source to keep
            // from, so this is a loud per-plane failure instead.
            log::warn!("paint save: {tag}/{channel} tile {tile} readback failed");
            let in_place = committed
                .as_ref()
                .is_some_and(|(committed_dir, _)| same_project_dir(project_dir, committed_dir));
            if !in_place {
                log::warn!(
                    "paint save: {tag}/{channel} tile {tile} has no in-place sidecar to keep; skipping"
                );
                continue;
            }
            let path = paint_plane_path(project_dir, owner, &channel, tile);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    entries.push(stage_paint_bytes(project_dir, &store, &name, &bytes)?);
                }
                Err(e) => {
                    log::warn!(
                        "paint save: {tag}/{channel} tile {tile} previous file unreadable ({e:#}); skipping"
                    );
                }
            }
            continue;
        };
        let entry = stage_paint_png(
            project_dir,
            &store,
            &name,
            image.width,
            image.height,
            &image.data,
        )?;
        // The commit hash is over the STAGED png bytes (not the readback:
        // the clean-skip compares against the file). Re-read the staged
        // temp — one extra file read per read-back plane, no GPU cost.
        let staged_bytes = std::fs::read(paint_sidecar_dir(project_dir).join(&entry.temp))?;
        paint.mark_plane_committed(
            owner,
            tile,
            &channel,
            project_dir.to_path_buf(),
            blake3::hash(&staged_bytes),
        );
        entries.push(entry);
        report.staged += 1;
        report.read_back += 1;
    }
    // Layer masks ride as their own flat sub-namespace (`mask-<layer>-
    // tile-<N>.png` — see `paint_mask_name`): one Linear-transfer PNG per
    // materialized plane, gray-expanded through `mask_to_rgba`. Only live
    // layers' planes are staged — the caller prunes orphans against the
    // document first (see `save_project`), so a deleted layer's mask can
    // never be re-committed here (and the apply step prunes whatever the
    // manifest does not list). A plane whose size disagrees with its tile
    // is skipped loudly, like a failed readback (a torn plane must never
    // gate the wrong texels on open).
    for (layer, tile) in paint.layer_mask_tiles() {
        let name = paint_mask_name(layer, tile);
        let Some(plane) = paint.layer_mask(layer, tile) else {
            continue;
        };
        let (width, height) = paint.tile_size(tile);
        if plane.len() != width as usize * height as usize {
            log::warn!(
                "paint save: mask {layer}/tile {tile} is {} bytes, want {}x{}; skipping",
                plane.len(),
                width,
                height
            );
            continue;
        }
        entries.push(stage_paint_png(
            project_dir,
            &store,
            &name,
            width,
            height,
            &mask_to_rgba(plane),
        )?);
        report.staged += 1;
    }
    commit_paint_manifest(project_dir, &entries, dormant)?;
    // Committed: from here an interruption rolls forward on the next open.
    if let Err(e) = apply_paint_commit(project_dir, None) {
        log::warn!("paint save: committed, apply stopped early ({e:#}); next open finishes it");
    }
    Ok(report)
}

/// Restores the paint sidecar into `paint` through the per-plane publish
/// path (`PaintState::publish_plane` — one full-plane write per plane, in
/// the sidecar's sorted order). Decodes each sidecar PNG, flips it back out
/// of the PNG reader's row order, applies the stored sizes first (each
/// resize drops the old contents and the session-scoped stroke-undo history
/// clears with the first one, so no stale rect can undo against the
/// restored pixels), then drains so the pixels have landed before
/// returning.
///
/// Dual read, keyed on the sidecar's CONTENT (§6.3): legacy flat and
/// channel-qualified paths always load into Base; `layers/…` paths load
/// into layer planes. A v1 project therefore lands in Base verbatim, and
/// the first frame is byte-identical (a Base-only `(tile, channel)` reads
/// Base directly). Every loaded plane starts committed to the opened dir
/// (`committed = (opened_dir, blake3(png_bytes))`), so an in-place save
/// right after open re-stages without a readback (§6.2).
///
/// Orphan planes (a `Layer(id)` no longer in the stack — `live` is the
/// loaded stack's ids) are ignored with a log: they never enter the
/// session, never count toward the version decision, and the next save's
/// manifest prunes them (§6.4). A budget refusal on any plane is a loud
/// skip (warn log — §1.3's load rule), never a silent hole.
///
/// Per-set sizes come from `target_sizes.txt` when present (authoritative:
/// each tile must match ITS set's size); without it the v1 shape holds —
/// every tile joins the active set at the first tile's dims, and the
/// legacy `target_size.txt` global only matters with no tiles (applied to
/// every set — the v1→v2 migration). Tiles whose dims disagree with their
/// set's size are skipped LOUDLY (warn log plus the report's skip list —
/// never a silent resample). A project with no sidecar tiles clears the
/// session to transparent instead of keeping the previous document's
/// pixels. Returns how many tiles were restored and what was refused.
/// Decodes one sidecar PNG, flipped back out of the reader's row order.
/// `None` (logged) on a decode failure — the caller skips that artifact.
fn decode_paint_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    match umber_export::png::read_png_rgba8(path) {
        Ok((width, height, bytes)) => {
            Some((width, height, flip_rows_vertical(&bytes, width, height)))
        }
        Err(e) => {
            log::warn!("paint open: {} decode failed: {e:#}", path.display());
            None
        }
    }
}
/// One layer's open-path seeding row: everything the fill/material
/// open-seeding needs, cloned out for the borrow-split.
type OpenSeedRow = (
    u64,
    umber_core::LayerKind,
    Option<umber_mesh::FillParams>,
    Option<umber_core::MaterialParams>,
    umber_core::BlendMode,
    f32,
);

/// One loaded plane sidecar's decoded payload, keyed by its plane
/// identity (owner, channel, tile) — the open-path plane census.
type LoadedPlaneSidecars =
    std::collections::BTreeMap<(paint_state::PlaneOwner, String, u16), (u32, u32, Vec<u8>)>;

/// Every `(owner, channel, tile)` artifact currently on disk under `dir`
/// (design §6.1's dual read, keyed on the sidecar's CONTENT, not on
/// `project.json`): flat `tile-<N>.png` files first (the reserved
/// baseColor channel's Base plane), then one level into every channel
/// subdirectory EXCEPT `layers/` (an explicit channel's Base plane), then
/// the `layers/<id>/<channel>/` tree (layer-owned planes). The flat pass
/// runs first so a (forward-compatible, never written by this crate today)
/// literal `paint/baseColor/` wins over a same-tile flat collision. Every
/// relative name goes through [`parse_paint_sidecar_name`] — the single
/// parse rule for both passes.
/// trust boundary — so a rejected shape never reaches the session.
fn collect_paint_tiles(dir: &Path) -> LoadedPlaneSidecars {
    let mut found = std::collections::BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    let mut channel_dirs = Vec::new();
    let mut layers_dir = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            if name == "layers" {
                layers_dir = Some(path);
            } else {
                channel_dirs.push((name, path));
            }
            continue;
        }
        let Some(tile) = parse_paint_tile_name(&name) else {
            continue;
        };
        if let Some(decoded) = decode_paint_png(&path) {
            found.insert(
                (
                    paint_state::PlaneOwner::Base,
                    paint_state::BASE_COLOR_CHANNEL.to_string(),
                    tile,
                ),
                decoded,
            );
        }
    }
    for (dir_name, path) in channel_dirs {
        let Ok(inner) = std::fs::read_dir(&path) else {
            continue;
        };
        for inner_entry in inner.flatten() {
            let inner_name = inner_entry.file_name().to_string_lossy().into_owned();
            let key = parse_paint_sidecar_name(&format!("{dir_name}/{inner_name}"));
            let Some((owner, channel, tile)) = key else {
                continue;
            };
            if let Some(decoded) = decode_paint_png(&inner_entry.path()) {
                found.insert((owner, channel, tile), decoded);
            }
        }
    }
    let Some(layers) = layers_dir else {
        return found;
    };
    let Ok(ids) = std::fs::read_dir(&layers) else {
        return found;
    };
    for id_entry in ids.flatten() {
        let id_name = id_entry.file_name().to_string_lossy().into_owned();
        let id_path = id_entry.path();
        if !id_path.is_dir() {
            continue;
        }
        let Ok(channels) = std::fs::read_dir(&id_path) else {
            continue;
        };
        for channel_entry in channels.flatten() {
            let channel_name = channel_entry.file_name().to_string_lossy().into_owned();
            let channel_path = channel_entry.path();
            if !channel_path.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&channel_path) else {
                continue;
            };
            for file in files.flatten() {
                let file_name = file.file_name().to_string_lossy().into_owned();
                let rel = format!("layers/{id_name}/{channel_name}/{file_name}");
                let Some((owner, channel, tile)) = parse_paint_sidecar_name(&rel) else {
                    continue;
                };
                if let Some(decoded) = decode_paint_png(&file.path()) {
                    found.insert((owner, channel, tile), decoded);
                }
            }
        }
    }
    found
}

/// Every `(layer, tile)` mask artifact currently on disk under `dir`:
/// flat `mask-<layer>-tile-<N>.png` files only (masks never nest — see
/// [`paint_mask_name`]). Decoded like paint tiles (the PNG reader's row
/// order flipped back); a decode failure skips that plane loudly.
fn collect_paint_masks(dir: &Path) -> std::collections::BTreeMap<(u64, u16), (u32, u32, Vec<u8>)> {
    let mut found = std::collections::BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((layer, tile)) = parse_paint_mask_name(&name) else {
            continue;
        };
        if let Some(decoded) = decode_paint_png(&path) {
            found.insert((layer, tile), decoded);
        }
    }
    found
}

fn load_paint_tiles_reported(
    paint: &mut paint_state::PaintState,
    project_dir: &Path,
    live: &std::collections::HashSet<u64>,
) -> PaintLoadReport {
    // An interrupted save resolves first (rolled forward when it had
    // committed, its orphans swept when not), so the sidecar read below
    // is the old project or the new one — never a torn mix.
    recover_paint_sidecar(project_dir);
    let dir = paint_sidecar_dir(project_dir);
    let found = collect_paint_tiles(&dir);
    let masks = collect_paint_masks(&dir);
    let mapping = std::fs::read_to_string(paint_set_sizes_path(project_dir))
        .ok()
        .and_then(|text| parse_paint_set_sizes(&text));
    let active = paint.active_set().to_string();
    if found.is_empty() && masks.is_empty() {
        // No paint persisted: the newly opened document must not show the
        // previous document's pixels (the layer stack is replaced
        // wholesale on open; the paint follows the same rule).
        match &mapping {
            Some(known) => {
                // Every known set goes to its mapped size (unmapped sets
                // resize to their live size — the resize still drops their
                // stale contents); a missing/unparseable mapping degrades
                // below, never a panic. Assignment-only sets (a tile line
                // with no size line) join the resize at the default size,
                // so no previous document's pixels survive on them either.
                // Channel-only sets (an override line with no size line)
                // join for the same reason — their override applies below.
                let mut names = paint.set_names();
                for name in known
                    .sizes
                    .keys()
                    .chain(known.tiles.values())
                    .chain(known.channels.keys().map(|(set, _)| set))
                {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                }
                for name in &names {
                    let (width, height) = known
                        .sizes
                        .get(name)
                        .copied()
                        .unwrap_or_else(|| paint.size_of_set(name));
                    paint.resize_set(name, width, height);
                }
                // Channel overrides land AFTER the set resizes (a set
                // resize rebuilds inheriting channels at the set's size —
                // the override must win). No paint persisted, so this only
                // records sizes for later tiles and drops stale targets.
                for ((set, channel), (width, height)) in &known.channels {
                    paint.resize_channel(set, channel, *width, *height);
                }
            }
            None => {
                // The v1 shape: the saved global (or the live size when the
                // sidecar stored none) applies to EVERY set — the
                // migration. Either way the pool is dropped: the clear
                // still runs.
                let (width, height) = std::fs::read_to_string(paint_target_size_path(project_dir))
                    .ok()
                    .and_then(|text| parse_paint_target_size(&text))
                    .unwrap_or_else(|| paint.target_size());
                for name in paint.set_names() {
                    paint.resize_set(&name, width, height);
                }
            }
        }
        if let Err(e) = paint.process_pending() {
            log::warn!("paint open: clearing the session failed: {e:#}");
        }
        return PaintLoadReport::default();
    }
    match &mapping {
        Some(known) => {
            // The mapping is authoritative: every known set goes to its
            // mapped size first (unmapped sets resize to their live size,
            // dropping stale pixels), then assignments re-derive from the
            // sidecar so a previous document's sets never steer this one.
            // Channel-only sets join the resize (their override applies
            // below); see the empty-sidecar branch above.
            let mut names = paint.set_names();
            for name in known
                .sizes
                .keys()
                .chain(known.tiles.values())
                .chain(known.channels.keys().map(|(set, _)| set))
            {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            // Assignments first, then the resize: a pre-existing tile's
            // TARGET keeps its old size when the mapping lands (assignment
            // steers future creations only — the field docs), so the set's
            // resize must run AFTER the assignment for the tile's target
            // to actually take the set's size on reopen.
            paint.clear_tile_assignments();
            for (tile, set) in &known.tiles {
                paint.assign_tile_to_set(*tile, set);
            }
            for name in &names {
                let (width, height) = known
                    .sizes
                    .get(name)
                    .copied()
                    .unwrap_or_else(|| paint.size_of_set(name));
                paint.resize_set(name, width, height);
            }
            // Channel overrides land AFTER the set resizes (which rebuild
            // inheriting channels at the set's size — the override must
            // win) and BEFORE the PNG uploads below (which size-check
            // against the channel's size): a reopened override is in place
            // before its pixels land, so a later `ensure_channel` — or the
            // upload's own materialization — builds at the override.
            for ((set, channel), (width, height)) in &known.channels {
                paint.resize_channel(set, channel, *width, *height);
            }
        }
        None => {
            // The v1 shape, byte-identical: every tile joins the active
            // set and the first tile's dims lead (a resize drops the old
            // contents and clears the session-scoped stroke-undo history
            // with them). No mapping means no `target_sizes.txt`, which
            // this crate always writes alongside any channel data — so
            // `found` here holds ONLY Base baseColor entries, and `found`'s
            // `(owner, channel, tile)` key order reduces to tile order
            // (every key shares owner and channel).
            paint.clear_tile_assignments();
            let mut tile_ids: Vec<u16> = found.keys().map(|(_, _, tile)| *tile).collect();
            tile_ids.dedup();
            for tile in &tile_ids {
                paint.assign_tile_to_set(*tile, &active);
            }
            let (first_width, first_height, _) = found
                .values()
                .next()
                .cloned()
                .expect("found is non-empty here");
            paint.resize_set(&active, first_width, first_height);
        }
    }
    if let Err(e) = paint.process_pending() {
        log::warn!("paint open: resize to the saved size failed: {e:#}");
        return PaintLoadReport::default();
    }
    let mut report = PaintLoadReport::default();
    for ((owner, channel, tile), (width, height, bytes)) in found {
        // Orphans (a saved plane whose layer is not in the loaded stack)
        // never enter the session (§6.4): ignored with a log, pruned by
        // the next save's manifest.
        if let paint_state::PlaneOwner::Layer(id) = owner {
            if !live.contains(&id) {
                log::warn!(
                    "paint open: layer {id}/{channel} tile {tile} has no live layer; ignoring"
                );
                continue;
            }
        }
        // §6.3's "Base is non-empty" arm (see `PaintLoadReport`): any
        // non-zero byte in any loaded Base plane.
        if matches!(owner, paint_state::PlaneOwner::Base)
            && !report.base_nonempty
            && bytes.iter().any(|&b| b != 0)
        {
            report.base_nonempty = true;
        }
        // Tiles the mapping never assigned resolve to the active set;
        // recording the resolution keeps the next save's mapping exact.
        let set = paint.set_of_tile(tile).to_string();
        paint.assign_tile_to_set(tile, &set);
        // The CHANNEL's size (its override, else the set's — a channel
        // PNG must match ITS size, never silently resample).
        let expected = paint.tile_channel_size(tile, &channel);
        if (width, height) != expected {
            let (ew, eh) = expected;
            let tag = plane_owner_tag(owner);
            log::warn!(
                "paint open: {tag}/{channel} tile {tile} is {width}x{height}, want {ew}x{eh}; skipping"
            );
            report.skipped.push(PaintTileSkip {
                tile,
                channel: channel.clone(),
                expected,
                found: (width, height),
            });
            continue;
        }
        // The commit hash is over the sidecar PNG's bytes: re-read the
        // file (it decoded a moment ago, so this only fails under a
        // concurrent delete — which then skips the commit mark loudly).
        let sidecar =
            paint_sidecar_dir(project_dir).join(paint_plane_sidecar_name(owner, &channel, tile));
        let png_bytes = std::fs::read(&sidecar);
        match paint.publish_plane(owner, tile, &channel, width, height, bytes) {
            Ok(()) => {
                report.restored += 1;
                match png_bytes {
                    Ok(png) => {
                        paint.mark_plane_committed(
                            owner,
                            tile,
                            &channel,
                            project_dir.to_path_buf(),
                            blake3::hash(&png),
                        );
                    }
                    Err(e) => {
                        let tag = plane_owner_tag(owner);
                        log::warn!(
                            "paint open: {tag}/{channel} tile {tile} sidecar vanished ({e:#}); commit mark skipped"
                        );
                    }
                }
            }
            Err(refusal) => {
                let tag = plane_owner_tag(owner);
                log::warn!(
                    "paint open: {tag}/{channel} tile {tile} publish refused ({refusal:?}); skipping"
                );
            }
        }
    }
    // Layer masks restore through the size-checked publish path (the
    // plane rides in the PNG's red channel — see `rgba_to_mask`); a plane
    // whose dims disagree with its tile's set size is skipped loudly, the
    // same rule as paint tiles. Masks for layers the document no longer
    // holds stay session-side here — the caller prunes them against the
    // loaded stack (see `open_project`), so a previous document's masks
    // never gate the new one.
    for ((layer, tile), (width, height, bytes)) in masks {
        let expected = paint.tile_size(tile);
        if (width, height) != expected {
            let (ew, eh) = expected;
            log::warn!(
                "paint open: mask {layer}/tile {tile} is {width}x{height}, want {ew}x{eh}; skipping"
            );
            continue;
        }
        if paint.publish_layer_mask(layer, tile, rgba_to_mask(&bytes)) {
            report.masks_restored += 1;
        } else {
            log::warn!("paint open: mask {layer}/tile {tile} upload failed; skipping");
        }
    }
    if let Err(e) = paint.process_pending() {
        log::warn!("paint open: upload drain failed: {e:#}");
    }
    report
}

impl UmberApp {
    /// File > New / the toolbar button: resets the document-scoped state
    /// to the boot-fresh shape ([`AppState::reset_for_new`]) and rebuilds
    /// the paint session on the eframe-owned device/queue — the same
    /// default texture set + channel stack the boot builds, so no stroke,
    /// mask, or tile from the previous document survives. The viewport
    /// keeps its environment: the IBL is a studio asset, not document
    /// state (and the loaded mesh stays too — see `reset_for_new`'s
    /// boundary note).
    ///
    /// Unsaved work gates through the confirm window
    /// ([`Self::pending_new`]): Save offers the save dialog first,
    /// Discard resets now, Cancel disarms.
    fn new_project(&mut self) {
        if self.state.doc.is_dirty() && self.pending_new.is_none() {
            // Unsaved work exists: arm the confirm window instead of
            // discarding silently (the never-lose-work doctrine).
            self.pending_new = Some(false);
            log::info!("new project: unsaved edits — confirm dialog armed");
            return;
        }
        self.confirm_new_project();
    }

    /// The File > New reset itself, post-confirmation (or when clean).
    fn confirm_new_project(&mut self) {
        self.pending_new = None;
        self.state.reset_for_new();
        // The boot shape (see `UmberApp::new`): a fresh default session on
        // the same device/queue, degrading to view-only with a logged
        // warning on feature-missing devices, never a failure. The old
        // session (its tiles, stroke history, masks) drops here.
        self.state.paint =
            match paint_state::PaintState::new(self.gpu.device.clone(), self.gpu.queue.clone()) {
                Ok(session) => Some(session),
                Err(err) => {
                    log::warn!("paint disabled: {err:#}");
                    None
                }
            };
        log::info!("new project: document reset (unsaved)");
    }

    /// Saves the document as a `.umber` project directory via a save
    /// dialog: one texture set (named from the loaded mesh), the
    /// layer stack + project settings through `umber_core::project`,
    /// plus the graph panel's graph as a `.mtlx` string (the additive
    /// wave-5 carry — omitted when the panel's graph is empty, so
    /// graph-less projects save exactly the old shape) and the postfx
    /// chain as the `postfx` settings carry (empty chains save the empty
    /// list, which loads back as the identity — the additive rule is
    /// load-compat: pre-postfx files simply lack the key), and the session's
    /// embedded material definitions (unreferenced ids ride along — a
    /// save-as-material entry not yet layered survives the save).
    ///
    /// Paint pixels ride along as the `paint/` sidecar
    /// (`save_paint_tiles`, after the `project.json` commit so the
    /// directory is guaranteed to exist).
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
        // set: a rename keeps channels (see `retarget_set`). The paint
        // session's active set follows it (the saved set is the session
        // set — the mapping's active entry names it).
        self.state.mesh_import.retarget_set(&set_name);
        if let Some(paint) = self.state.paint.as_mut() {
            paint.set_active_set(&set_name);
        }
        // The stack's live layer ids: the version decision's (§6.3) and
        // the sidecar save's (§6.4) shared orphan filter.
        let live: std::collections::HashSet<u64> = self
            .state
            .doc
            .stack
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect();
        // §6.3's minimal bump [M2]: version 2 iff this save stages a live
        // layer plane, else the default 1 (`ProjectModel::new`'s stamp —
        // headless/CLI sessions always write 1, §8 T24).
        let texture_set = self.state.mesh_import.session_set.clone();
        let graphs_mtlx = if self.state.graph.graph().nodes.is_empty() {
            Vec::new()
        } else {
            vec![self.state.graph.to_mtlx()]
        };
        let mut model = umber_core::project::ProjectModel::new(
            vec![texture_set],
            vec![umber_core::project::TextureSetLayers {
                texture_set: set_name.clone(),
                stack: self.state.doc.stack.clone(),
            }],
            umber_core::project::ProjectSettings {
                active_texture_set: Some(set_name.clone()),
                display: self.state.display.clone(),
                postfx: postfx_panel::carry_from_chain(&self.state.postfx),
            },
        )
        .with_graphs_mtlx(graphs_mtlx)
        // Embedded material definitions (the session map — every definition
        // a material layer references is in there, so the saved project
        // evaluates identically on another machine).
        .with_material_defs_mtlx(self.state.material_defs.clone())
        // The journal high-water mark: every entry recorded so far is in
        // this saved state, so none may replay on top of it.
        .with_journal_seq(self.state.doc.journal_seq());
        if let Some(paint) = self.state.paint.as_ref() {
            model.version = paint_save_version(paint, &live);
        }
        match umber_core::project::save_to_dir(&model, &dir) {
            Ok(()) => {
                log::info!("project saved: {}", dir.display());
                // The painted pixels ride along as the `paint/` sidecar,
                // after the commit so the directory exists (the paint
                // commit is atomic on its own; a failed one logs, keeps
                // the metadata save and leaves the old paint intact — the
                // cross-commit window is in the module docs).
                if let Some(paint) = self.state.paint.as_mut() {
                    // Layer-mask planes for deleted layers prune here, not
                    // in the sidecar walk: only live layers' masks commit
                    // (a re-save never resurrects a detached mask — the
                    // apply step prunes whatever the manifest omits). The
                    // same `live` set filters orphan pixel planes in the
                    // sidecar walk itself (§6.4).
                    paint.prune_layer_masks(&live);
                    // Still-dormant layers ride the v2 header's `# dormant`
                    // lines (§6.3): only live fill/material ids, sorted by
                    // the formatter.
                    let dormant: Vec<u64> = self
                        .state
                        .dormant
                        .iter()
                        .filter(|id| {
                            live.contains(*id)
                                && self.state.doc.stack.layer(**id).is_some_and(|layer| {
                                    matches!(
                                        layer.kind,
                                        umber_core::LayerKind::Fill
                                            | umber_core::LayerKind::Material
                                    )
                                })
                        })
                        .copied()
                        .collect();
                    let tiles = save_paint_tiles(paint, &dir, &live, &dormant);
                    log::info!("paint saved: {tiles} tile(s)");
                }
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
    /// starts fresh; the journal is session-scoped by design), the paint
    /// sidecar's pixels into the paint session (`load_paint_tiles` — the
    /// stroke-undo history restarts empty with them), attaches
    /// the project's autosave journal, the carried postfx chain into the
    /// Display panel's postfx section, and the first carried
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
                // The paint session's active set follows it (new tiles join
                // the opened set; the sidecar re-derives the rest).
                if let Some(saved) = model.texture_sets.iter().find(|ts| ts.name == name) {
                    self.state.mesh_import.set_session_set(saved.clone());
                } else {
                    self.state
                        .mesh_import
                        .set_session_set(umber_core::TextureSet::new_default(&name));
                }
                if let Some(paint) = self.state.paint.as_mut() {
                    paint.set_active_set(&name);
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
                // The carried postfx chain (the conversion clamps — the
                // same hand-edited-file rule). No dirty flag: the viewport
                // reads the chain every frame and `PostStack` builds a
                // newly-seen kind on first use (params ride uniforms).
                self.state.postfx = postfx_panel::chain_from_carry(&model.settings.postfx);
                // The painted pixels (or a clear to transparent when the
                // project predates the sidecar — never the previous
                // document's paint). The session lifecycle (§6.3 [M10])
                // clears every layer-owned plane BEFORE the sidecar load:
                // layer ids restart at 0 in every LayerStack, so pixel
                // state keyed by id must not outlive its document.
                let live: std::collections::HashSet<u64> = self
                    .state
                    .doc
                    .stack
                    .layers
                    .iter()
                    .map(|layer| layer.id)
                    .collect();
                // The paint commit's provenance header (§6.1 [B2, M11]):
                // v2 + `# dormant` lines, or legacy (absent/unreadable
                // counts as v1 — erring toward dormancy).
                let (header_v2, header_dormant) =
                    std::fs::read_to_string(paint_manifest_path(&dir))
                        .ok()
                        .map(|text| parse_manifest_header(&text))
                        .unwrap_or((false, Vec::new()));
                let mut base_nonempty = false;
                if let Some(paint) = self.state.paint.as_mut() {
                    paint.clear_layer_planes();
                    let report = load_paint_tiles_reported(paint, &dir, &live);
                    base_nonempty = report.base_nonempty;
                    log::info!("paint restored: {} tile(s)", report.restored);
                    // Masks restore session-side keyed by layer id: drop
                    // whatever the loaded stack no longer holds, so a
                    // previous document's masks never gate the new one.
                    paint.prune_layer_masks(&live);
                }
                // Session pixel state keyed by layer id dies with the
                // document: queued import publishes (another document's
                // ids), Base visibility, dormancy, badges and the snapshot
                // baseline all reset.
                self.state.mesh_import.to_publish.clear();
                self.state.base_visible = true;
                self.state.dormant.clear();
                self.state.prev_snapshot = None;
                self.state.fill_badges.clear();
                self.state.import_badges.clear();
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
                // Embedded material definitions: the session copy the frame
                // refresh and the Layers panel read (a pre-material project
                // simply carries none).
                self.state.material_defs = model.material_defs_mtlx.clone();
                // Project-open seeding is CLEAN (design §2): the paint
                // sidecars already hold the published result, so
                // re-publishing on open would double-count over the legacy
                // Base. `ensure_open` inserts every fill/material state
                // clean; then §6.3's dormancy decides (§8 T18/T31):
                // dormant layers stay clean AND badged (their pre-v2
                // result lives in Base — never re-evaluated until an
                // explicit edit), v2 planeless layers seed DIRTY (their
                // result lives nowhere). Without a paint session there is
                // nothing to size against — states materialize dirty at
                // runtime instead.
                if let Some(paint) = self.state.paint.as_ref() {
                    let size = paint.size_of_set(&name);
                    let planed: std::collections::HashSet<u64> = paint
                        .planes_present()
                        .into_iter()
                        .filter_map(|(owner, _, _)| match owner {
                            paint_state::PlaneOwner::Layer(id) => Some(id),
                            paint_state::PlaneOwner::Base => None,
                        })
                        .collect();
                    // Borrow-split: the layer rows clone out first (the
                    // seeding mutates sibling state below).
                    let rows: Vec<OpenSeedRow> = self
                        .state
                        .doc
                        .stack
                        .layers
                        .iter()
                        .map(|layer| {
                            (
                                layer.id,
                                layer.kind.clone(),
                                layer.fill.clone(),
                                layer.material.clone(),
                                layer.blend_mode,
                                layer.opacity,
                            )
                        })
                        .collect();
                    for (id, kind, fill, material, blend, opacity) in &rows {
                        let has_planes = planed.contains(id);
                        let listed = header_dormant.contains(id);
                        match kind {
                            umber_core::LayerKind::Fill => {
                                if let Some(params) = fill {
                                    self.state
                                        .fill_states
                                        .ensure_open(*id, params, *blend, *opacity);
                                    match dormancy_for(has_planes, header_v2, listed, base_nonempty)
                                    {
                                        LayerDormancy::Dormant => {
                                            self.state.dormant.insert(*id);
                                        }
                                        LayerDormancy::Dirty => {
                                            self.state.fill_states.mark_dirty(*id);
                                        }
                                        LayerDormancy::Clean => {}
                                    }
                                }
                            }
                            umber_core::LayerKind::Material => {
                                if let Some(params) = material {
                                    let def = self
                                        .state
                                        .material_defs
                                        .get(&params.material_id)
                                        .cloned()
                                        .unwrap_or_default();
                                    self.state
                                        .material_states
                                        .ensure_open(*id, params, &def, size);
                                    match dormancy_for(has_planes, header_v2, listed, base_nonempty)
                                    {
                                        LayerDormancy::Dormant => {
                                            self.state.dormant.insert(*id);
                                        }
                                        LayerDormancy::Dirty => {
                                            self.state.material_states.mark_dirty(*id);
                                        }
                                        LayerDormancy::Clean => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    // The size-census baseline: set now, so the first frame's
                    // adoption sees no change and marks nothing dirty.
                    let census: Vec<(String, (u32, u32))> = self
                        .state
                        .mesh_import
                        .session_set
                        .channels
                        .iter()
                        .map(|channel| {
                            (
                                channel.name.clone(),
                                paint.size_of_channel(&name, &channel.name),
                            )
                        })
                        .collect();
                    self.state.material_sizes = Some((size, census));
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
        // The File > New confirm (the dirty-document gate): one small
        // centered window — Save (the save dialog, then the reset),
        // Discard (reset now), Cancel (disarm). Nothing else in the
        // shell is blocked: egui has no true modal in 0.36, and a
        // half-modal that blocks input would fight the dock — the
        // dialog is the honest v1 shape (the Perf HUD's Window
        // precedent).
        if self.pending_new.is_some() {
            use egui::Window;
            Window::new(tr("shell.new-confirm-title"))
                .collapsible(false)
                .resizable(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label(tr("shell.new-confirm-body"));
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(tr("shell.new-confirm-save")).clicked() {
                            // Save-then-reset: the save dialog runs
                            // (rfd, modal at the OS level), then the
                            // document is on-disk current — the reset
                            // proceeds this frame.
                            self.save_project();
                            if !self.state.doc.is_dirty() {
                                self.confirm_new_project();
                            }
                        }
                        if ui.button(tr("shell.new-confirm-discard")).clicked() {
                            self.confirm_new_project();
                        }
                        if ui.button(tr("shell.new-confirm-cancel")).clicked() {
                            self.pending_new = None;
                        }
                    });
                });
        }
        if self.state.bakes.poll_job() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Mesh-map import drops (§7 P1 row): drain here, not only in the
        // Assets tab's `show` (egui_dock skips hidden tabs — same pattern
        // as the async bake poll above), so a folder dropped while Assets
        // is hidden still routes.
        // (Mesh files are filtered out there; `load_dropped_mesh` takes them.)
        self.state.mesh_import.poll_drops(ui.ctx());
        // PSD drops route the same frame (the state's scan filters .psd;
        // mesh files were taken above).
        self.state.psd_import.poll_drops(ui.ctx());
        self.state.spp_import.poll_drops(ui.ctx());
        self.load_dropped_mesh(ui.ctx());
        // Shell shortcuts (`toolbar`'s key table): the overlay keys (W
        // wireframe, G grid — Wave-4 item 7), V lit ↔ unlit (§5's P0 row),
        // X / I quick mask (§2's P1 row), E eraser (§2's P0 row), plus the
        // Ctrl chords for undo/redo/save/open. Empty while a text edit has
        // focus (brush preset names, save-as dialog), so typing never
        // flips the viewport, the mask, or the tool. Dispatched with the
        // toolbar's clicks below, through the same `dispatch`.
        let mut actions = toolbar::poll_shortcuts(ui.ctx());
        // Top bar: MenuBar container (egui 0.36 API).
        MenuBar::new().ui(ui, |ui| {
            MenuButton::new(tr("menu.file")).ui(ui, |ui| {
                if ui.button(tr("shell.new")).clicked() {
                    actions.push(toolbar::ShellAction::NewProject);
                }
                if ui.button(tr("menu.open-mesh")).clicked() {
                    actions.push(toolbar::ShellAction::OpenMesh);
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
                ui.checkbox(&mut self.view.wireframe, tr("menu.wireframe"));
                ui.checkbox(&mut self.view.grid, tr("menu.grid"));
                ui.separator();
                viewport::view_mode_menu(ui, &mut self.view.view_mode);
            });
            MenuButton::new(tr("menu.help")).ui(ui, |ui| {
                if ui.button(tr("menu.about")).clicked() {
                    ui.label(tr("menu.about-text"));
                }
            });
        });

        // Shell chrome (GUI Phase 0b): the toolbar under the menu bar, the
        // status bar at the bottom. Both are egui panels, laid out before
        // the central dock area, which takes what remains (egui_dock has
        // no top/bottom slots of its own).
        let toolbar_view = toolbar::ToolbarView {
            tool: self.brush_panel.strip,
            can_undo: self.state.doc.history.can_undo(),
            can_redo: self.state.doc.history.can_redo(),
            view: self.view,
            paint_available: self.state.paint.is_some(),
            mask_editing: self
                .state
                .paint
                .as_ref()
                .is_some_and(|p| p.quick_mask().is_editing()),
        };
        let icons = &self.icons;
        egui::Panel::top("umber-toolbar").show(ui, |ui| {
            actions.extend(toolbar::show(ui, &toolbar_view, icons));
        });
        for action in actions {
            self.dispatch(action);
        }
        self.state.viewport.set_show_wireframe(self.view.wireframe);
        self.state.viewport.set_show_grid(self.view.grid);
        self.state
            .viewport
            .set_show_cage(self.state.bakes.show_cage());
        self.state.viewport.set_view_mode(self.view.view_mode);
        // The paint session adopts the panel's tool here too, not only in
        // the Properties tab (egui_dock skips hidden tabs — same pattern as
        // the bake poll), so a toolbar/E tool change reaches the session
        // while Assets is the visible tab of that node.
        if let Some(paint) = self.state.paint.as_mut() {
            paint.sync_tool(&mut self.brush_panel.tool);
        }
        // Stroke-commit drain: finished strokes queue their History-window
        // rows on the paint session (which owns the stroke journal), and
        // the rows live on the document — drained here, not only in the
        // History tab's `show` (egui_dock skips hidden tabs — same pattern
        // as the bake poll above), so strokes painted while History is
        // hidden still get rows.
        if let Some(paint) = self.state.paint.as_mut() {
            history_panel::drain_stroke_commits(&mut self.state.doc, paint);
        }

        // Stylus (XI2) poll, once per frame BEFORE the paint arm reads
        // (the dock below): drains the tablet stream, holds the Erase
        // tool while the eraser end is active, and hands this frame's
        // pressure to the viewport via the viewer. Without a backend the
        // poll is a clock tick and the pressure is `None` (Touch rules).
        self.state.stylus.poll_frame();
        self.state.stylus.apply_tool_override(&mut self.brush_panel);
        let stylus_pressure = self.state.stylus.pressure_for_frame();
        let pen_source = self.state.stylus.pen_source();

        // The status bar reads the UV view's hover from the last frame
        // (taken, so a hidden UV tab reads none); see `status_bar`.
        let hover_before = self.state.uv_view.take_hover_uv();
        let tile = self.state.paint.as_ref().map(|p| p.active_tile());
        let status = status_bar::StatusSnapshot {
            cursor_uv: hover_before
                .map(|uv| status_bar::absolute_uv(tile.unwrap_or(umber_mesh::FIRST_TILE), uv)),
            tile,
            frame_dt: ui.input(|i| i.stable_dt),
            pen: pen_source,
        };
        egui::Panel::bottom("umber-status-bar").show(ui, |ui| {
            status_bar::show(ui, &status);
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
        // Per-layer pool stage 3 (§2.3): revalidate the stroke selection
        // once per frame, before panels run (the fill-adoption precedent
        // below re-resolves adopted params the same way). An explicit Base
        // never auto-moves; an auto Base resolves to the topmost paintable
        // layer or Base; a vanished id resolves to its former neighbour or
        // Base; the verdict is fresh for the next `begin_stroke`. The
        // explicit bookkeeping rides `sync_active_target` (same target =
        // keep, moved = auto).
        if let Some(paint) = self.state.paint.as_mut() {
            let sync = document::revalidate_active(
                &self.state.doc.stack,
                paint.active_target(),
                paint.active_explicit(),
                paint.active_index(),
            );
            paint.sync_active_target(sync.target, sync.verdict, sync.index);
        }
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
                material_defs: &mut self.state.material_defs,
                material_badges: &self.state.material_badges,
                material_status: self.state.material_status.as_deref(),
                material_states: &mut self.state.material_states,
                display: &mut self.state.display,
                display_lut_state: &mut self.state.display_lut,
                #[cfg(feature = "ocio")]
                ocio_state: &mut self.state.ocio_state,
                display_lut: effective_lut,
                postfx: &mut self.state.postfx,
                postfx_panel: &mut self.state.postfx_panel,
                mesh_import: &mut self.state.mesh_import,
                psd_import: &mut self.state.psd_import,
                spp_import: &mut self.state.spp_import,
                base_visible: &mut self.state.base_visible,
                fill_badges: &self.state.fill_badges,
                import_badges: &self.state.import_badges,
                dormant: &self.state.dormant,
                store: store.as_ref(),
                hdr_mode: &mut self.hdr_mode,
                stylus_pressure,
            };
            DockArea::new(&mut self.dock)
                .style(Style::from_egui(ui.style()))
                .show_inside(ui, &mut viewer);
        });
        // The status bar drew before the dock: one more frame when the
        // hover moved, so its readout catches up (converges — the next
        // frame records the same hover).
        if self.state.uv_view.hover_uv() != hover_before {
            ui.ctx().request_repaint();
        }

        // Display LUT rebuild, after the panels (the Display panel's
        // edits this frame included): the upload is staged on the queue,
        // so it lands before egui submits this frame — the callbacks the
        // views just built (same texture, unchanged bind group) already
        // draw with the new table. One more frame refreshes the panel's
        // status line, which read "Rebuilding" while the LUT was dirty.
        // The adopted OCIO processor for the CURRENT selection (built if
        // stale — the same call the panel makes; None = fixed path).
        #[cfg(feature = "ocio")]
        let ocio_processor = self.state.ocio_state.processor_for(&self.state.display);
        #[cfg(feature = "ocio")]
        let bytes = self
            .state
            .display_lut
            .take_rebuild(&self.state.display, ocio_processor);
        #[cfg(not(feature = "ocio"))]
        let bytes = self.state.display_lut.take_rebuild(&self.state.display);
        if let Some(bytes) = bytes {
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
        // re-evaluate and publish the tiles of any layer the adoption pass
        // marked (panel edits, undo/redo and journal replays all land in
        // Layer::fill before this point in the frame — one adoption link
        // covers every producer of fill-param changes). Each refresh
        // REPLACES the layer's own planes (re-eval is idempotent, hides
        // un-publish — §5.1); the end-of-update flush composites them the
        // same frame ([M7]).
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
            // Budget-blocked fills retry when bytes fell (a plane freed);
            // param edits requeue through `ensure` above.
            self.state
                .fill_states
                .retry_budget_freed(paint.plane_bytes());
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
                        let staged =
                            fill_layer::refresh_fill_layer(state, &mesh, paint, &tiles, &symmetry);
                        // A refreshed dormant layer wakes: its old Base
                        // contribution stays, and the badge said so
                        // beforehand (§6.3).
                        self.state.dormant.remove(&layer);
                        if staged.is_empty() && state.budget_blocked().is_none() {
                            log::warn!("fill refresh for layer {layer} staged no tiles");
                        }
                        if let Some(bytes) = state.budget_blocked() {
                            self.state.fill_badges.insert(
                                layer,
                                crate::i18n::tr_args(
                                    "layers.budget-blocked",
                                    [("bytes", fluent::FluentValue::from(bytes as i64))],
                                ),
                            );
                        } else {
                            self.state.fill_badges.remove(&layer);
                        }
                    }
                }
                // No mesh loaded: LEAVE the queue — a fill layer first seen
                // without a mesh stages its tiles on the mesh's arrival
                // (ensure's dedup keeps the queue from growing meanwhile).
            }
        }

        // Dynamic-material refresh (Kill B, `docs/specs/dynamic-materials-
        // design.md` §2, stage-4 planes): adopt every doc-side material
        // change, then drain the dirty queue bottom-to-top through the
        // headless driver. Panel edits, undo/redo replays of `SetMaterial`,
        // "Save as material" definition writes and resizes all land in
        // `Layer::material` / `material_defs` before this point in the
        // frame — one adoption pass covers every producer. Each refresh
        // REPLACES the layer's own planes (re-eval is idempotent, hides
        // un-publish — §5.2); the end-of-update flush composites them the
        // same frame ([M7]). Evaluates at the set size, never a literal.
        if let Some(paint) = self.state.paint.as_mut() {
            let set_name = paint.active_set().to_string();
            let set_size = paint.size_of_set(&set_name);
            let set_channels: Vec<String> = self
                .state
                .mesh_import
                .session_set
                .channels
                .iter()
                .map(|channel| channel.name.clone())
                .collect();
            // Stack order is bottom-to-top — the drain's refresh order.
            let layers: Vec<umber_core::layers::Layer> = self.state.doc.stack.layers.clone();
            let live: Vec<u64> = layers.iter().map(|layer| layer.id).collect();
            for layer in layers
                .iter()
                .filter(|l| l.kind == umber_core::layers::LayerKind::Material)
            {
                // A `material: None` Material layer gets no state (nothing to
                // evaluate); the Layers panel badges it statically.
                if let Some(params) = layer.material.as_ref() {
                    let def = self
                        .state
                        .material_defs
                        .get(&params.material_id)
                        .cloned()
                        .unwrap_or_default();
                    self.state
                        .material_states
                        .ensure(layer.id, params, &def, set_size);
                }
            }
            // Channel-override census: `MaterialLayerState` tracks the set
            // size but not per-channel overrides, so an override resize
            // would otherwise never re-dirty (the design's "size changed
            // invalidates the caches"). Mesh-map revisions need no census:
            // the maps are flat placeholders in v1 and never bump.
            let census: Vec<(String, (u32, u32))> = set_channels
                .iter()
                .map(|channel| (channel.clone(), paint.size_of_channel(&set_name, channel)))
                .collect();
            if self.state.material_sizes.as_ref() != Some(&(set_size, census.clone())) {
                self.state.material_sizes = Some((set_size, census));
                for layer in layers
                    .iter()
                    .filter(|l| l.kind == umber_core::layers::LayerKind::Material)
                {
                    self.state.material_states.mark_dirty(layer.id);
                }
            }
            // Budget-blocked materials retry when bytes fell (a plane
            // freed); param edits requeue through `ensure` above.
            self.state
                .material_states
                .retry_budget_freed(paint.plane_bytes());
            if !self.state.material_states.dirty_ids().is_empty() {
                let registry = umber_graph::full_registry();
                // Anchor rows = the flush's rows (§5.3: "the same entry
                // builder the flush uses" — visibility and Base visibility
                // included, so an anchor reads what the stack shows through
                // that layer). `displayed` stays empty: the prefix
                // composites on the CPU and never touches [M8]'s gate.
                let snapshot = build_composite_snapshot(
                    &self.state.doc.stack,
                    &self.state.material_states,
                    self.state.base_visible,
                    Vec::new(),
                );
                let mut io = PaintMaterialIo {
                    paint,
                    set: set_name,
                    live_layers: live.clone(),
                    snapshot,
                };
                // The driver takes `HashMap`; the session keeps the
                // `BTreeMap` the project model sorts by — cloned on dirty
                // frames only (definitions are KB-scale).
                let definitions: std::collections::HashMap<String, String> = self
                    .state
                    .material_defs
                    .iter()
                    .map(|(id, def)| (id.clone(), def.clone()))
                    .collect();
                let reports = material_layer::refresh_dirty_materials(
                    &mut self.state.material_states,
                    &layers,
                    &definitions,
                    &set_channels,
                    &registry,
                    &mut io,
                );
                let mut status: Option<String> = None;
                for report in &reports {
                    let id = report.layer;
                    match &report.result {
                        material_layer::MaterialResult::Failed(err) => {
                            let msg = err.to_string();
                            log::warn!("material layer {id} refresh failed: {msg}");
                            self.state.material_badges.insert(id, msg.clone());
                            status = Some(format!("material layer {id}: {msg}"));
                        }
                        material_layer::MaterialResult::BudgetBlocked { bytes } => {
                            // The existing badge slot names the block (the
                            // fill precedent's row badge is `fill_badges` —
                            // materials reuse this one). The layer stays
                            // dirty and retries per §1.3, never every frame.
                            let line = crate::i18n::tr_args(
                                "layers.budget-blocked",
                                [("bytes", fluent::FluentValue::from(*bytes as i64))],
                            );
                            log::warn!("material layer {id} refresh blocked: {line}");
                            self.state.material_badges.insert(id, line.clone());
                            status = Some(format!("material layer {id}: {line}"));
                        }
                        material_layer::MaterialResult::Refreshed {
                            channels,
                            ignored_undeclared,
                        } => {
                            self.state.material_badges.remove(&id);
                            // A refreshed dormant layer wakes (§6.3).
                            self.state.dormant.remove(&id);
                            // Per-channel outcomes in declaration order — the
                            // doc's status-line contract. Skips (disabled/
                            // uniform) are named once per drain; publishes
                            // stay silent (the happy path is no noise).
                            let skipped: Vec<String> = channels
                                .iter()
                                .filter(|c| {
                                    !matches!(c.status, material_layer::ChannelStatus::Published)
                                })
                                .map(|c| format!("{} ({:?})", c.name, c.status))
                                .collect();
                            if !skipped.is_empty() {
                                let names = skipped.join(", ");
                                log::info!("material layer {id} channels skipped: {names}");
                                status =
                                    Some(format!("material layer {id} channels skipped: {names}"));
                            }
                            if !ignored_undeclared.is_empty() {
                                let names = ignored_undeclared.join(", ");
                                log::warn!(
                                    "material layer {id} ignores undeclared channels: {names}"
                                );
                                status = Some(format!(
                                    "material layer {id} ignores undeclared channels: {names}"
                                ));
                            }
                        }
                        material_layer::MaterialResult::SkippedInvisible => {}
                    }
                }
                self.state.material_status = status;
                let live_set: std::collections::HashSet<u64> = live.into_iter().collect();
                self.state
                    .material_badges
                    .retain(|id, _| live_set.contains(id));
            }
        }

        // Composite-on-read flush (§4.1 [M7]): at the END of the update,
        // AFTER the fill AND material refresh blocks — the producers
        // publish `WriteRegion`s into per-plane queues, and the composite
        // reads plane textures, so queued-but-unprocessed commands would
        // be invisible without the drain-first flush. Strokes show the
        // same frame they splat; fills and materials show the same frame
        // they refresh (never one frame late).
        if let Some(paint) = self.state.paint.as_mut() {
            // The displayed `(tile, channel)`s ([M8]'s coalescing): the
            // active tile's baseColor + height (the viewport's binds and
            // the UV canvas's — which shows the active baseColor too).
            let active = paint.active_tile();
            let displayed = vec![
                (active, paint_state::BASE_COLOR_CHANNEL.to_string()),
                (active, paint_state::HEIGHT_CHANNEL.to_string()),
            ];
            let snapshot = build_composite_snapshot(
                &self.state.doc.stack,
                &self.state.material_states,
                self.state.base_visible,
                displayed,
            );
            // Metadata edits surface through the snapshot diff (order,
            // visibility, opacity, blend, mask attach/detach/enable,
            // channel params, Base visibility): changed owners mark
            // `Full` (§4.2). Removed ids retire (paint) or drop (eval).
            if let Some(prev) = self.state.prev_snapshot.as_ref() {
                for owner in paint_state::CompositeSnapshot::full_owners(prev, &snapshot) {
                    paint.mark_owner_full(owner);
                }
                paint
                    .note_removed_layers(&paint_state::CompositeSnapshot::removed(prev, &snapshot));
            }
            self.state.prev_snapshot = Some(snapshot.clone());
            // §5.6 import-as-producer: every applied import publishes its
            // decoded PNG into the new layer's own plane (or refuses
            // loudly, leaving the layer empty — the row badges it).
            let store = self
                .state
                .project_dir
                .as_ref()
                .map(|dir| umber_core::AssetStore::new(dir));
            let imports = std::mem::take(&mut self.state.mesh_import.to_publish);
            for applied in &imports {
                publish_import_plane(
                    paint,
                    store.as_ref(),
                    &mut self.state.import_badges,
                    applied,
                );
            }
            paint.flush_composite(&snapshot);
            // Tombstone losses surface on the Layers rows (undo revives
            // EMPTY — the `layers.tombstone-lost` line).
            if let Some(lost) = paint.take_tombstone_notice() {
                self.state.import_badges.insert(
                    lost,
                    crate::i18n::tr_args(
                        "layers.tombstone-lost",
                        [("layer", fluent::FluentValue::from(lost as i64))],
                    ),
                );
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
                    .map(|p| p.plane_bytes())
                    .unwrap_or(0);
                let undo_depth = self.state.doc.history.len();
                egui::Window::new("Perf HUD").show(ui.ctx(), |ui| {
                    ui.label(format!("frame: {frame_ms:.2} ms"));
                    ui.label(format!(
                        "dabs this frame: {dabs_delta} (total {dabs_total})"
                    ));
                    ui.label(format!("plane bytes: {target_bytes} bytes"));
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
        .add_filter(tr("shell.filter-meshes"), &empty_state::MESH_EXTENSIONS)
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

/// File > New tests: the headless reset half (`AppState::reset_for_new` —
/// every field pinned to its exact boot-fresh value, not a range check).
/// The GPU half (the paint-session rebuild inside `new_project`) needs an
/// `eframe::CreationContext`, so it is unverified headless — see the kill
/// report.
#[cfg(test)]
mod new_project_tests {
    use super::*;

    #[test]
    fn reset_for_new_restores_the_boot_fresh_shape() {
        let mut state = AppState::default();
        // Dirty every reset field away from its default first, so the
        // asserts below cannot pass vacuously.
        state.doc.run(umber_core::layers::LayerCommand::add(
            "Paint 1".to_owned(),
            umber_core::layers::LayerKind::Paint,
        ));
        assert!(state.doc.history.can_undo());
        state.mesh_import.retarget_set("OldSet");
        state.mesh_import.applied = 7;
        state.psd_import.retarget_set("OldSet");
        state.psd_import.applied = 3;
        state.spp_import.pending.push(("a.spp".to_owned(), None));
        state.spp_import.status = "waiting".to_owned();
        state.display.exposure = 2.0;
        #[cfg(not(feature = "ocio"))]
        let built = state.display_lut.take_rebuild(&state.display);
        #[cfg(feature = "ocio")]
        let built = state.display_lut.take_rebuild(&state.display, None);
        assert!(
            built.is_some(),
            "a dirty LUT must offer a rebuild (else the live assert below is vacuous)"
        );
        assert!(state.display_lut.is_live(&state.display));
        state.postfx.push(umber_gpu::postfx::PostEffect::Vignette {
            strength: 0.5,
            softness: 0.5,
        });
        assert_eq!(state.postfx.effects.len(), 1);
        state.project_dir = Some(PathBuf::from("/tmp/umber-new-project-test"));
        // Stage-4 session pixel state, dirtied away from reset.
        state.base_visible = false;
        state.dormant.insert(3);
        state.fill_badges.insert(3, "blocked".to_string());
        state.import_badges.insert(5, "refused".to_string());

        state.reset_for_new();

        assert!(
            state.doc.stack.layers.is_empty(),
            "a fresh document has no layers"
        );
        assert!(
            !state.doc.history.can_undo(),
            "a fresh document has no undo"
        );
        assert!(
            !state.doc.history.can_redo(),
            "a fresh document has no redo"
        );
        assert_eq!(
            state.doc.journal_seq(),
            0,
            "a fresh document journals nowhere"
        );
        assert_eq!(state.mesh_import.session_set.name, "TextureSet");
        assert_eq!(state.mesh_import.applied, 0);
        assert_eq!(state.psd_import.session_set_name, "TextureSet");
        assert_eq!(state.psd_import.applied, 0);
        assert!(state.spp_import.pending.is_empty());
        assert_eq!(state.spp_import.status, "");
        assert!(
            state.display.is_identity(),
            "display resets to the identity chain (Raw, 0 EV, gamma 1)"
        );
        assert!(
            !state.display_lut.is_live(&state.display),
            "the reset must dirty the LUT so the frame loop rebuilds it"
        );
        assert_eq!(state.postfx, umber_gpu::postfx::PostChain::new());
        assert!(state.project_dir.is_none(), "a new project is unsaved");
        assert!(state.base_visible, "Base shows again on a new document");
        assert!(state.dormant.is_empty(), "dormancy dies with its document");
        assert!(
            state.prev_snapshot.is_none(),
            "the snapshot baseline restarts"
        );
        assert!(
            state.fill_badges.is_empty(),
            "fill badges die with their document"
        );
        assert!(
            state.import_badges.is_empty(),
            "import badges die with their document"
        );
    }

    /// Stage-2 §8 T30 (`reset_for_new` half): document B's Fill 3 must not
    /// inherit document A's Fill 3 state — the fill states reset with the
    /// document. (The paint half — `clear_layer_planes` — is pinned beside
    /// the sidecar tests, where a GPU session exists.)
    #[test]
    fn reset_for_new_clears_fill_states() {
        let mut state = AppState::default();
        // Dirty the fill states away from default first, so the asserts
        // below cannot pass vacuously.
        state.fill_states.dirty.push(3);
        assert!(!state.fill_states.dirty.is_empty());

        state.reset_for_new();

        assert!(
            state.fill_states.map.is_empty(),
            "no fill state survives a new document"
        );
        assert!(
            state.fill_states.dirty.is_empty(),
            "no fill dirty queued survives a new document"
        );
    }
}

/// Per-layer pool stage-4 shell tests (§8 T18/T31 + snapshot): the
/// dormancy decision, the v2 manifest header, and the composite-snapshot
/// builder — all headless (no GPU session needed).
#[cfg(test)]
mod pool_stage4_tests {
    use super::*;

    #[test]
    fn dormancy_keys_on_provenance_not_absence() {
        // §8 T18 [B2]: a v1 project (v1 manifest header) with a Fill
        // layer, a mesh and non-empty Base opens dormant — clean,
        // planeless, never re-evaluated (no double-count).
        assert_eq!(
            dormancy_for(false, false, false, true),
            LayerDormancy::Dormant
        );
        // ... and saving while still dormant writes the v2 header plus
        // `# dormant <id>`, so reopening is dormant again.
        let text = format_paint_commit(&[], &[7]);
        assert_eq!(text, "# Umber paint commit v2\n# dormant 7\n");
        let (is_v2, dormant) = parse_manifest_header(&text);
        assert!(is_v2);
        assert_eq!(dormant, vec![7]);
        assert_eq!(
            dormancy_for(false, true, true, true),
            LayerDormancy::Dormant,
            "the listed id reopens dormant"
        );
        // Loaded planes seed clean (their bytes hold the result).
        assert_eq!(dormancy_for(true, false, false, true), LayerDormancy::Clean);
        assert_eq!(dormancy_for(true, true, false, false), LayerDormancy::Clean);
        // A v1 project with an EMPTY Base baked nothing: dirty, so the
        // layer evaluates on mesh arrival (nothing to double-count).
        assert_eq!(
            dormancy_for(false, false, false, false),
            LayerDormancy::Dirty
        );
    }

    #[test]
    fn v2_planeless_fill_is_not_dormant() {
        // §8 T31 [M11]: a v2 header with a planeless Fill NOT listed
        // dormant (never evaluated, or every eval failed) opens dirty —
        // and its row shows no dormant badge.
        assert_eq!(dormancy_for(false, true, false, true), LayerDormancy::Dirty);
        assert_eq!(
            dormancy_for(false, true, false, false),
            LayerDormancy::Dirty
        );
    }

    #[test]
    fn manifest_header_parses_v2_and_dormant_ids() {
        assert_eq!(
            parse_manifest_header("# Umber paint commit v2\ntile-1001.png h png 1 t\n"),
            (true, vec![])
        );
        assert_eq!(
            parse_manifest_header("# Umber paint commit v2\n# dormant 3\n# dormant 9\n"),
            (true, vec![3, 9])
        );
        // Legacy: v1, absent, unreadable, or malformed dormant lines.
        assert_eq!(
            parse_manifest_header("# Umber paint commit v1\n"),
            (false, vec![])
        );
        assert_eq!(parse_manifest_header(""), (false, vec![]));
        assert_eq!(
            parse_manifest_header("# Umber paint commit v2\n# dormant x\n"),
            (true, vec![])
        );
        // `parse_paint_commit` still skips every `#` line for the artifact
        // table (the header is provenance, never artifacts).
        let text = format_paint_commit(&[], &[3, 9]);
        assert_eq!(parse_paint_commit(&text), Some(Vec::new()));
    }

    #[test]
    fn snapshot_orders_base_first_and_expands_materials() {
        use umber_core::layers::{LayerKind, LayerStack};
        let mut stack = LayerStack::new();
        let paint = stack.add_layer("P", LayerKind::Paint);
        stack.set_opacity(paint, 0.5);
        let fill = stack.add_layer("F", LayerKind::Fill);
        let material = stack.add_layer("M", LayerKind::Material);
        let _folder = stack.add_layer("G", LayerKind::Folder { passthrough: true });
        let instance = stack.add_layer("I", LayerKind::Instance { source_id: paint });
        stack.set_opacity(instance, 0.25);
        // Material plans: one contributing channel (Multiply 0.5).
        let mut states = material_layer::MaterialLayerStates::default();
        let params = umber_core::MaterialParams {
            material_id: "umber.m".into(),
            channels: vec![umber_core::layers::MaterialChannel {
                name: "baseColor".into(),
                blend: umber_core::BlendMode::Multiply,
                opacity: 0.5,
                enabled: true,
            }],
            params: vec![],
        };
        stack.set_material(material, params.clone());
        states.ensure(material, &params, "", (64, 64));
        // The builder reads whatever the states carry: empty before any
        // refresh (a never-evaluated material contributes no entries —
        // exactly like `pixels: None`).
        let snapshot =
            build_composite_snapshot(&stack, &states, true, vec![(1001, "baseColor".into())]);
        let owners: Vec<Option<u64>> = snapshot.layers.iter().map(|layer| layer.layer_id).collect();
        // Base first, then stack order; folders carry no entry (their
        // planes can never exist); the instance (stage 5) emits its own row.
        assert_eq!(
            owners,
            vec![
                None,
                Some(paint),
                Some(fill),
                Some(material),
                Some(instance)
            ]
        );
        // The instance row composites its SOURCE's planes with its OWN
        // opacity (§5.3): owner = the paint source, id = the instance.
        let instance_row = &snapshot.layers[4];
        assert_eq!(instance_row.owner, paint_state::PlaneOwner::Layer(paint));
        assert_eq!(instance_row.opacity, 0.25);
        assert_eq!(instance_row.kind, paint_state::SnapshotLayerKind::Paint);
        assert!(snapshot.base_visible);
        // The paint row carries the layer's own blend/opacity...
        let paint_row = &snapshot.layers[1];
        assert_eq!(paint_row.opacity, 0.5);
        // ... while the material row expands per channel (none yet — the
        // layer never refreshed, so nothing contributes).
        let material_row = &snapshot.layers[3];
        assert!(material_row.material_channels.as_ref().unwrap().is_empty());
        // Hidden Base flips the row's visibility (the flush marks Full).
        let hidden = build_composite_snapshot(&stack, &states, false, vec![]);
        assert!(!hidden.layers[0].visible);
        assert!(!hidden.base_visible);
    }

    #[test]
    fn instance_rows_resolve_chains_and_skip_dangling_sources() {
        // §5.3: an instance of an instance renders the ROOT source's planes;
        // a material source keeps its per-channel blend with the
        // instance's opacity folded in; a dangling source contributes
        // nothing (no row, never a panic).
        use umber_core::layers::{LayerKind, LayerStack};
        let mut stack = LayerStack::new();
        let material = stack.add_layer("M", LayerKind::Material);
        let first = stack.add_layer(
            "I1",
            LayerKind::Instance {
                source_id: material,
            },
        );
        let second = stack.add_layer("I2", LayerKind::Instance { source_id: first });
        stack.set_opacity(second, 0.5);
        let dangling = stack.add_layer("I3", LayerKind::Instance { source_id: 999 });
        let mut states = material_layer::MaterialLayerStates::default();
        let params = umber_core::MaterialParams {
            material_id: "umber.m".into(),
            channels: vec![umber_core::layers::MaterialChannel {
                name: "baseColor".into(),
                blend: umber_core::BlendMode::Multiply,
                opacity: 0.5,
                enabled: true,
            }],
            params: vec![],
        };
        stack.set_material(material, params.clone());
        states.ensure(material, &params, "", (64, 64));
        let snapshot = build_composite_snapshot(&stack, &states, true, vec![]);
        let ids: Vec<Option<u64>> = snapshot.layers.iter().map(|l| l.layer_id).collect();
        assert_eq!(ids, vec![None, Some(material), Some(first), Some(second)]);
        assert!(!ids.contains(&Some(dangling)));
        let row = &snapshot.layers[3];
        assert_eq!(row.owner, paint_state::PlaneOwner::Layer(material));
        assert_eq!(row.kind, paint_state::SnapshotLayerKind::Material);
        assert_eq!(row.opacity, 0.5);
        assert_eq!(
            resolve_instance_source(&stack, second).map(|l| l.id),
            Some(material)
        );
        assert!(resolve_instance_source(&stack, dangling).is_none());
    }
}

/// Paint-sidecar persistence tests (CORE-LOOP CRITICAL 4): the filename
/// codec, the row-flip inverse, and the save→load byte comparison.
#[cfg(test)]
mod paint_persistence_tests {
    use super::*;

    /// Requests a paint-capable device (graceful skip where no wgpu
    /// adapter exists — the same pattern as
    /// `paint_state::tests::try_request_device`; lavapipe covers CI).
    /// Returns `None` when the test must be skipped.
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

    fn unique_temp_dir(label: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "umber-app-paint-{label}-{}-{n}",
            std::process::id()
        ))
    }

    /// The stage-2 orphan filter for Base-only sessions: no live layers,
    /// so only Base planes stage and load (orphan planes never stage).
    fn no_live_layers() -> std::collections::HashSet<u64> {
        std::collections::HashSet::new()
    }

    #[test]
    fn paint_tile_filename_parses_exactly() {
        assert_eq!(parse_paint_tile_name("tile-1001.png"), Some(1001));
        assert_eq!(parse_paint_tile_name("tile-65535.png"), Some(65535));
        assert_eq!(parse_paint_tile_name("project.json"), None);
        assert_eq!(parse_paint_tile_name("tile-1001.png.tmp.123"), None);
        assert_eq!(parse_paint_tile_name("tile-.png"), None);
        assert_eq!(parse_paint_tile_name("tile-abc.png"), None);
        assert_eq!(parse_paint_tile_name("tile-1001.exr"), None);
        assert_eq!(
            paint_tile_path(Path::new("/p"), 1001),
            PathBuf::from("/p/paint/tile-1001.png")
        );
    }

    #[test]
    fn target_size_round_trips_exactly_and_rejects_garbage() {
        // Every offered preset formats and parses back exactly (not a
        // range check — a dropped size would fail); trailing newlines
        // (hand-edited files) parse too.
        for size in crate::size_presets::SIZE_PRESETS {
            let text = format_paint_target_size(size, size);
            assert_eq!(text, format!("{size}x{size}"));
            assert_eq!(parse_paint_target_size(&text), Some((size, size)));
            assert_eq!(
                parse_paint_target_size(&format!("{text}\n")),
                Some((size, size))
            );
        }
        assert_eq!(parse_paint_target_size("4096x4096"), Some((4096, 4096)));
        // Garbage degrades to None (the load path keeps the live size):
        // empty, one-sided, zero-sided, non-numeric, wrong separator.
        for bad in [
            "", "512", "x512", "512x", "0x512", "512x0", "0x0", "ax512", "512xa", "512*512",
        ] {
            assert_eq!(parse_paint_target_size(bad), None, "{bad:?} must reject");
        }
        assert_eq!(
            paint_target_size_path(Path::new("/p")),
            PathBuf::from("/p/paint/target_size.txt")
        );
    }

    #[test]
    fn flip_rows_vertical_is_its_own_inverse_with_exact_bytes() {
        // 2x3 RGBA: every row one distinct value, so the order is pinned
        // exactly (not a range check — a mirror would fail).
        let (width, height) = (2u32, 3u32);
        let mut bytes = Vec::new();
        for row in 0..3u8 {
            bytes.extend_from_slice(&[row, row, row, 255, row, row, row, 255]);
        }
        let flipped = flip_rows_vertical(&bytes, width, height);
        let mut expected = Vec::new();
        for row in [2u8, 1, 0] {
            expected.extend_from_slice(&[row, row, row, 255, row, row, row, 255]);
        }
        assert_eq!(flipped, expected, "row order must reverse exactly");
        assert_eq!(
            flip_rows_vertical(&flipped, width, height),
            bytes,
            "a second flip must restore the input (the load path depends on this)"
        );
    }

    #[test]
    fn save_open_round_trip_is_byte_identical() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("paint builds");
        // A 25-event diagonal stroke (the conditioner needs a run of
        // events before it emits its first dab).
        paint.begin_stroke(egui::pos2(0.2, 0.2));
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            paint.extend_stroke(egui::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        paint.end_stroke();
        paint.process_pending().expect("drain succeeds");
        let before = paint
            .read_tile(umber_mesh::FIRST_TILE)
            .expect("readback succeeds");
        // Can-fail guard against a vacuous pass: a stroke that staged
        // nothing would compare two blank targets equal. Fresh targets
        // are zero-initialized (transparent black), so any painted texel
        // is nonzero somewhere.
        assert!(
            before.data.iter().any(|&b| b != 0),
            "the stroke must leave non-zero texels (else the comparison below is vacuous)"
        );

        let dir = unique_temp_dir("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 1, "a single-tile session stores exactly tile 1001");
        assert!(paint_tile_path(&dir, umber_mesh::FIRST_TILE).exists());
        // Store-routed: the commit applied (no pending manifest, no temps)
        // and the provenance record names a store blob byte-identical to
        // the sidecar PNG the reopen reads.
        assert!(!paint_commit_pending_path(&dir).exists());
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let tile_entry = entries
            .iter()
            .find(|e| e.name == "tile-1001.png")
            .expect("the manifest names the tile");
        assert_eq!(
            umber_core::AssetStore::new(&dir)
                .load(&tile_entry.asset)
                .unwrap(),
            std::fs::read(paint_tile_path(&dir, umber_mesh::FIRST_TILE)).unwrap(),
            "the store blob is the sidecar's exact bytes"
        );

        let mut fresh =
            paint_state::PaintState::new(device, queue).expect("fresh paint state builds");
        let restored = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(restored.restored, 1);
        let after = fresh
            .read_tile(umber_mesh::FIRST_TILE)
            .expect("fresh readback succeeds");
        assert_eq!(after.width, before.width);
        assert_eq!(after.height, before.height);
        assert_eq!(
            after.data, before.data,
            "save→load must restore byte-identical texels"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_sizes_codec_round_trips_exactly_and_rejects_garbage() {
        // Sizes sorted by set name, then assignments sorted by tile — the
        // save is byte-deterministic, so the exact text is pinned (not a
        // contains-check — a reordered save would fail).
        let mut mapping = PaintSetSizes::default();
        mapping.sizes.insert("Sword".to_string(), (4096, 4096));
        mapping.sizes.insert("Shield".to_string(), (2048, 1024));
        mapping.tiles.insert(1002, "Shield".to_string());
        let text = format_paint_set_sizes(&mapping);
        assert_eq!(
            text,
            "# Umber paint-set sizes v1\nShield=2048x1024\nSword=4096x4096\ntile-1002=Shield\n"
        );
        assert_eq!(parse_paint_set_sizes(&text), Some(mapping));
        // A stored non-square survives (the load path honors stored dims —
        // only the UI is square-only).
        assert_eq!(
            parse_paint_set_sizes("Shield=2048x1024\n"),
            Some(PaintSetSizes {
                sizes: std::collections::BTreeMap::from([("Shield".to_string(), (2048, 1024))]),
                channels: std::collections::BTreeMap::new(),
                tiles: std::collections::BTreeMap::new(),
            })
        );
        // Comments, blanks and malformed lines skip; the tile- prefix is
        // reserved for assignments even when the right side is a size.
        let mixed = "# header\n\nSword=512x512\nbogus\n=512x512\nA=\nA=0x512\ntile-1002=Sword\n";
        let parsed = parse_paint_set_sizes(mixed).expect("one size + one assignment parse");
        assert_eq!(parsed.sizes.len(), 1);
        assert_eq!(parsed.sizes.get("Sword"), Some(&(512, 512)));
        assert_eq!(parsed.tiles.get(&1002), Some(&"Sword".to_string()));
        assert!(
            parsed.channels.is_empty(),
            "no channel lines means all-inherit"
        );
        // Nothing parseable at all degrades to None (the load path falls
        // back to the legacy global): empty, comments-only, and every
        // malformed shape.
        for bad in [
            "",
            "# only a comment\n\n",
            "no-equals-here\n",
            "=512x512\n",
            "A=\n",
            "A=0x512\n",
            "tile-abc=X\n",
            "tile-=X\n",
        ] {
            assert_eq!(
                parse_paint_set_sizes(bad),
                None,
                "{bad:?} must yield no mapping"
            );
        }
        assert_eq!(
            paint_set_sizes_path(Path::new("/p")),
            PathBuf::from("/p/paint/target_sizes.txt")
        );
    }

    #[test]
    fn channel_sizes_codec_round_trips_and_old_files_load_all_inherit() {
        // Channel lines sort between sizes and assignments (set, then
        // channel) — the save is byte-deterministic, so the exact text is
        // pinned, and the header stays the v1 line on purpose (additive —
        // see `format_paint_set_sizes`).
        let mut mapping = PaintSetSizes::default();
        mapping.sizes.insert("Sword".to_string(), (4096, 4096));
        mapping
            .channels
            .insert(("Sword".to_string(), "roughness".to_string()), (512, 512));
        mapping
            .channels
            .insert(("Sword".to_string(), "height".to_string()), (1024, 2048));
        mapping.tiles.insert(1001, "Sword".to_string());
        let text = format_paint_set_sizes(&mapping);
        assert_eq!(
            text,
            "# Umber paint-set sizes v1\n\
             Sword=4096x4096\n\
             channel/Sword/height=1024x2048\n\
             channel/Sword/roughness=512x512\n\
             tile-1001=Sword\n"
        );
        assert_eq!(parse_paint_set_sizes(&text), Some(mapping));
        // A channel-only file still parses (never None while something
        // parsed — the load path must see the overrides).
        let only = parse_paint_set_sizes("channel/Body/height=256x256\n")
            .expect("a lone channel line parses");
        assert!(only.sizes.is_empty() && only.tiles.is_empty());
        assert_eq!(
            only.channels
                .get(&("Body".to_string(), "height".to_string())),
            Some(&(256, 256))
        );
        // Malformed channel lines skip without killing the good ones:
        // empty set, empty channel, missing size, zero size, and the
        // double-`=` shape (the second `=` lands in the size field, which
        // rejects it — the codec stays single-`=`).
        let messy = "channel/Body/height=256x256\n\
                     channel//height=128x128\n\
                     channel/Body/=128x128\n\
                     channel/Body/roughness\n\
                     channel/Body/metallic=0x128\n\
                     channel=Body/ao=64x64\n";
        let parsed = parse_paint_set_sizes(messy).expect("the one good line parses");
        assert_eq!(parsed.channels.len(), 1);
        assert_eq!(
            parsed
                .channels
                .get(&("Body".to_string(), "height".to_string())),
            Some(&(256, 256))
        );
        // A pre-channel file (no channel lines — the shape the tile-pool
        // kill writes) loads all-inherit: sizes and tiles parse, channels
        // stay empty.
        let old = "# Umber paint-set sizes v1\nBody=2048x2048\ntile-1001=Body\n";
        let parsed_old = parse_paint_set_sizes(old).expect("a pre-channel file parses");
        assert!(parsed_old.channels.is_empty());
        assert_eq!(parsed_old.sizes.get("Body"), Some(&(2048, 2048)));
    }

    /// Deterministic RGBA8 pattern: texel `(x, y)` channel `c` holds
    /// `(x*31 + y*17 + c*71 + seed) mod 251` (251 is prime, so the values
    /// spread; the seed tells two sets' paint apart).
    fn pattern_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                for c in 0..4u32 {
                    out.push(((x * 31 + y * 17 + c * 71 + u32::from(seed)) % 251) as u8);
                }
            }
        }
        out
    }

    #[test]
    fn two_sets_at_different_sizes_round_trip_byte_identical_per_set() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Set A ("Body") at 2048, set B ("Cloak") at 4096 — the audit's
        // gate: two sets, two sizes, one session.
        let mut paint =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("paint builds");
        paint.set_active_set("Body");
        // Tile 1001 is the constructor's pre-assigned default-set tile
        // (tile_sets maps it to TextureSet at build); switching the ACTIVE
        // set never re-routes recorded tiles (the field docs: assignment
        // steers FUTURE ensure_tile calls). Re-assign it to Body so the
        // 2048 publish lands in a 2048 target.
        paint.assign_tile_to_set(1001, "Body");
        paint.resize_set("Body", 2048, 2048);
        paint.ensure_set("Cloak");
        assert_eq!(paint.size_of_set("Cloak"), (4096, 4096));
        paint.assign_tile_to_set(1002, "Cloak");

        let before_a = pattern_bytes(2048, 2048, 3);
        let before_b = pattern_bytes(4096, 4096, 77);
        // Can-fail guards against a vacuous pass: the patterns are
        // non-blank and distinct, and the (0,0) texels pin the formula
        // exactly (seed 3 → [3, 74, 145, 216]; seed 77 → [77, 148, 219, 39]).
        assert_eq!(&before_a[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&before_b[..4], &[77u8, 148, 219, 39]);
        assert_ne!(before_a[..64], before_b[..64]);
        assert!(before_a.iter().any(|&b| b != 0));

        assert!(paint.publish_fill(1001, 2048, 2048, before_a.clone()));
        assert!(paint.publish_fill(1002, 4096, 4096, before_b.clone()));
        paint.process_pending().expect("uploads land");
        for (tile, bytes, (w, h)) in [
            (&1001u16, &before_a, (2048u32, 2048u32)),
            (&1002u16, &before_b, (4096u32, 4096u32)),
        ] {
            let image = paint.read_tile(*tile).expect("readback succeeds");
            assert_eq!((image.width, image.height), (w, h));
            assert_eq!(&image.data, bytes, "tile {tile} stages its set's bytes");
        }

        let dir = unique_temp_dir("perset-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 2, "both sets' tiles persist");
        assert!(paint_tile_path(&dir, 1001).exists());
        assert!(paint_tile_path(&dir, 1002).exists());
        // The mapping is byte-exact (sorted sizes, then sorted tiles);
        // the legacy global still carries the active ("Body") size.
        let mapping_text = std::fs::read_to_string(paint_set_sizes_path(&dir)).unwrap();
        assert_eq!(
            mapping_text,
            "# Umber paint-set sizes v1\nBody=2048x2048\nCloak=4096x4096\ntile-1001=Body\ntile-1002=Cloak\n"
        );
        assert_eq!(
            std::fs::read_to_string(paint_target_size_path(&dir)).unwrap(),
            "2048x2048"
        );

        // Reopen in a fresh session (default set, 4K): each set's paint
        // restores byte-identical AT ITS size — the load path re-derives
        // sizes and assignments from the mapping, not from the live state.
        let mut fresh = paint_state::PaintState::new(device.clone(), queue.clone())
            .expect("fresh paint state builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(fresh.size_of_set("Body"), (2048, 2048));
        assert_eq!(fresh.size_of_set("Cloak"), (4096, 4096));
        assert_eq!(fresh.set_of_tile(1001), "Body");
        assert_eq!(fresh.set_of_tile(1002), "Cloak");
        let after_a = fresh.read_tile(1001).expect("tile 1001 reads back");
        assert_eq!((after_a.width, after_a.height), (2048, 2048));
        assert_eq!(
            after_a.data, before_a,
            "set A restores byte-identical at 2048"
        );
        let after_b = fresh.read_tile(1002).expect("tile 1002 reads back");
        assert_eq!((after_b.width, after_b.height), (4096, 4096));
        assert_eq!(
            after_b.data, before_b,
            "set B restores byte-identical at 4096"
        );

        // A size-mismatched restore fails LOUDLY: clobber tile 1002's PNG
        // with 512² bytes (its set says 4096²) and reopen — tile 1001
        // still restores, 1002 is refused with its expected/found dims on
        // the record (never silently resampled).
        let wrong = pattern_bytes(512, 512, 9);
        umber_export::png::write_png(
            &paint_tile_path(&dir, 1002),
            512,
            512,
            &wrong,
            umber_export::png::Transfer::Linear,
        )
        .expect("clobber writes");
        let mut mismatch =
            paint_state::PaintState::new(device, queue).expect("mismatch session builds");
        let report = load_paint_tiles_reported(&mut mismatch, &dir, &no_live_layers());
        assert_eq!(report.restored, 1, "only the matching tile restores");
        assert_eq!(
            report.skipped,
            vec![PaintTileSkip {
                tile: 1002,
                channel: paint_state::BASE_COLOR_CHANNEL.to_string(),
                expected: (4096, 4096),
                found: (512, 512),
            }],
            "the mismatch is on the record with exact dims"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_channels_round_trip_byte_identical_per_channel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // 512² keeps the test fast; the sizes must MATCH the target —
        // a 512 publish into a 4096 target would land a 512 region at
        // the origin, not the pattern — so the session is built at 512
        // via with_target_size (tile 1001 pre-exists at that size).
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let before_base = pattern_bytes(512, 512, 3);
        let before_height = pattern_bytes(512, 512, 77);
        // Can-fail guards against a vacuous pass: the patterns are
        // non-blank and distinct, and the (0,0) texels pin the formula
        // exactly (seed 3 → [3, 74, 145, 216]; seed 77 → [77, 148, 219,
        // 39] — the same derivation as the two-set test).
        assert_eq!(&before_base[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&before_height[..4], &[77u8, 148, 219, 39]);
        assert_ne!(before_base, before_height);
        assert!(before_base.iter().any(|&b| b != 0));
        assert!(paint.publish_fill(1001, 512, 512, before_base.clone()));
        assert!(paint.publish_fill_channel(1001, "height", 512, 512, before_height.clone()));
        paint.process_pending().expect("uploads land");
        let staged_base = paint.read_tile(1001).expect("baseColor stages");
        assert_eq!(staged_base.data, before_base);
        let staged_height = paint
            .read_channel_tile(1001, "height")
            .expect("height stages");
        assert_eq!(staged_height.data, before_height);

        let dir = unique_temp_dir("per-channel-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 2, "baseColor + height persist as two artifacts");
        // baseColor keeps the v2 FLAT layout byte-for-byte
        // (paint/tile-1001.png — paint_tile_path, deliberately
        // unchanged by the per-channel pool; see its docs), while any
        // other channel nests one level under paint/<channel>/.
        assert!(paint_tile_path(&dir, 1001).exists());
        assert_eq!(
            paint_channel_tile_path(&dir, paint_state::BASE_COLOR_CHANNEL, 1001),
            paint_tile_path(&dir, 1001),
            "the reserved channel resolves to the flat v2 path"
        );
        let height_path = paint_channel_tile_path(&dir, "height", 1001);
        assert_eq!(
            height_path,
            paint_sidecar_dir(&dir).join("height").join("tile-1001.png")
        );
        assert!(height_path.exists());
        // The manifest names both artifacts — the channel rides in the
        // NAME field (paint_sidecar_name), not a new column — and the
        // commit applied (no pending manifest left behind).
        assert!(!paint_commit_pending_path(&dir).exists());
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert!(
            names.contains(&"tile-1001.png"),
            "baseColor keeps its flat v2 name, got {names:?}"
        );
        assert!(
            names.contains(&"height/tile-1001.png"),
            "height nests under its channel dir, got {names:?}"
        );

        // Reopen in a fresh session (default set, 4K): both channels
        // restore byte-identical AT the saved size.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(fresh.channels_present(1001), vec!["height".to_string()]);
        let after_base = fresh.read_tile(1001).expect("baseColor reads back");
        assert_eq!((after_base.width, after_base.height), (512, 512));
        assert_eq!(
            after_base.data, before_base,
            "baseColor restores byte-identical"
        );
        let after_height = fresh
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((after_height.width, after_height.height), (512, 512));
        assert_eq!(
            after_height.data, before_height,
            "height restores byte-identical"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn channel_size_override_round_trips_and_mismatches_skip_per_channel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // The default set at 512 with height overridden to 256: two sizes,
        // one session (the per-channel sizing gate).
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        paint.resize_channel(paint_state::DEFAULT_TEXTURE_SET, "height", 256, 256);
        let before_base = pattern_bytes(512, 512, 3);
        let before_height = pattern_bytes(256, 256, 77);
        assert_eq!(&before_base[..4], &[3u8, 74, 145, 216]);
        // Seed 77 pins (0,0) to [77, 148, 219, 39] — the same derivation
        // as the two-channel test (dims never enter it).
        assert_eq!(&before_height[..4], &[77u8, 148, 219, 39]);
        assert!(paint.publish_fill(1001, 512, 512, before_base.clone()));
        assert!(paint.publish_fill_channel(1001, "height", 256, 256, before_height.clone()));
        paint.process_pending().expect("uploads land");

        let dir = unique_temp_dir("per-channel-size-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 2, "baseColor + height persist as two artifacts");
        // The mapping pins the override line exactly (sorted between the
        // size and the assignment — the codec test pins the grammar; this
        // pins the save path emitting it).
        let mapping_text = std::fs::read_to_string(paint_set_sizes_path(&dir)).unwrap();
        assert_eq!(
            mapping_text,
            "# Umber paint-set sizes v1\n\
             TextureSet=512x512\n\
             channel/TextureSet/height=256x256\n\
             tile-1001=TextureSet\n"
        );

        // Reopen fresh (4K default): the override restores BEFORE its
        // pixels land, so height materializes at 256 while baseColor
        // restores at 512 — both byte-identical.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(
            fresh.size_of_channel("TextureSet", "height"),
            (256, 256),
            "the override survives the round trip"
        );
        assert_eq!(
            fresh.size_of_channel("TextureSet", "roughness"),
            (512, 512),
            "no line means inherit the set's size"
        );
        let after_base = fresh.read_tile(1001).expect("baseColor reads back");
        assert_eq!((after_base.width, after_base.height), (512, 512));
        assert_eq!(
            after_base.data, before_base,
            "baseColor restores byte-identical"
        );
        let after_height = fresh
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((after_height.width, after_height.height), (256, 256));
        assert_eq!(
            after_height.data, before_height,
            "height restores byte-identical at ITS size"
        );

        // A size-mismatched CHANNEL refuses loudly per channel: clobber
        // height's PNG with 512² bytes (its override says 256²) and
        // reopen — baseColor still restores, height refuses with its
        // channel on the record (never silently resampled).
        let wrong = pattern_bytes(512, 512, 9);
        umber_export::png::write_png(
            &paint_channel_tile_path(&dir, "height", 1001),
            512,
            512,
            &wrong,
            umber_export::png::Transfer::Linear,
        )
        .expect("clobber writes");
        let mut mismatch =
            paint_state::PaintState::new(device, queue).expect("mismatch session builds");
        let report = load_paint_tiles_reported(&mut mismatch, &dir, &no_live_layers());
        assert_eq!(report.restored, 1, "only the matching channel restores");
        assert_eq!(
            report.skipped,
            vec![PaintTileSkip {
                tile: 1001,
                channel: "height".to_string(),
                expected: (256, 256),
                found: (512, 512),
            }],
            "the channel mismatch is on the record with exact dims"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_flat_manifest_loads_all_base_color() {
        // The codec has NO channel column (verified against source):
        // parse_paint_commit destructures an exact 5-field array
        // `<name> <hash> <ext> <size> <temp>` and refuses any other
        // arity — a literal 6th column fails the whole manifest. The
        // channel instead rides in the NAME field
        // (paint_sidecar_name/parse_paint_sidecar_name): a bare
        // `tile-<N>.png` IS the reserved baseColor channel. A v2
        // project (saved before channels existed) therefore names only
        // flat tiles, and the load path restores every one through
        // publish_fill. Hand-written codec pins first:
        let zeros = "0".repeat(64);
        let flat_line = format!("tile-1001.png {zeros} png 123 tile-1001.png.tmp.42");
        let flat = parse_paint_commit(&format!("# Umber paint commit v1\n{flat_line}\n"))
            .expect("a v2-shaped 5-field flat line parses");
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].name, "tile-1001.png");
        // A 6-field line (the "add a channel column" shape the design
        // doc's draft text sketches) refuses the WHOLE manifest —
        // which is why the landed codec encodes the channel in the
        // name instead (see paint_sidecar_name's docs).
        let six_field = format!("tile-1001.png baseColor {zeros} png 123 tile-1001.png.tmp.42");
        assert_eq!(
            parse_paint_commit(&format!("# Umber paint commit v1\n{six_field}\n")),
            None,
            "a 6-field line must refuse"
        );
        // The name codec itself, exact (not contains-checks): flat is
        // baseColor, one level nests, anything else rejects.
        assert_eq!(
            parse_paint_sidecar_name("tile-1001.png"),
            Some((
                paint_state::PlaneOwner::Base,
                paint_state::BASE_COLOR_CHANNEL.to_string(),
                1001
            ))
        );
        assert_eq!(
            parse_paint_sidecar_name("height/tile-1001.png"),
            Some((paint_state::PlaneOwner::Base, "height".to_string(), 1001))
        );
        // A literal paint/baseColor/ subdirectory parses as the
        // reserved channel too — and wins over a same-tile flat file
        // on load (collect_paint_tiles inserts the flat pass first;
        // the channel pass overwrites). Pinned so a future rename of
        // the reserved name fails loudly instead of forking the pool.
        assert_eq!(
            parse_paint_sidecar_name("baseColor/tile-1001.png"),
            Some((
                paint_state::PlaneOwner::Base,
                paint_state::BASE_COLOR_CHANNEL.to_string(),
                1001
            ))
        );
        for bad in [
            "",
            "tile-1001.png/",
            "/tile-1001.png",
            "../tile-1001.png",
            "height\\tile-1001.png",
            "a\\b/tile-1001.png",
            "height/tile-abc.png",
            "height/tile-1001.exr",
            "tile-1001.exr",
        ] {
            assert_eq!(parse_paint_sidecar_name(bad), None, "{bad:?} must reject");
        }

        // End to end: a baseColor-only session saved through the REAL
        // save path produces exactly the v2 shape (flat names only, no
        // channel directory), and reopens all-baseColor with no
        // channel materialized.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let before = pattern_bytes(512, 512, 11);
        assert_eq!(&before[..4], &[11u8, 82, 153, 224]);
        assert!(paint.publish_fill(1001, 512, 512, before.clone()));
        paint.process_pending().expect("upload lands");

        let dir = unique_temp_dir("channel-migration");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]),
            1
        );
        assert!(
            !paint_sidecar_dir(&dir).join("height").exists(),
            "no channel was routed, so no channel directory is written"
        );
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        for entry in &entries {
            if entry.name == "target_size.txt" || entry.name == "target_sizes.txt" {
                continue;
            }
            assert_eq!(
                parse_paint_sidecar_name(&entry.name),
                Some((
                    paint_state::PlaneOwner::Base,
                    paint_state::BASE_COLOR_CHANNEL.to_string(),
                    1001
                )),
                "every tile entry in a channel-less save is flat baseColor"
            );
        }

        let mut fresh = paint_state::PaintState::new(device, queue).expect("fresh session builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 1,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert!(
            fresh.channels_present(1001).is_empty(),
            "the migration must not materialize any channel"
        );
        let after = fresh.read_tile(1001).expect("readback succeeds");
        assert_eq!((after.width, after.height), (512, 512));
        assert_eq!(
            after.data, before,
            "v2 pixels restore byte-identical as baseColor"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v1_project_without_mapping_migrates_its_global_to_every_set() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // A v1-shaped project: 512² tile PNGs + the global, NO mapping.
        // Saved through the real save path, then the mapping deleted (a v1
        // build never wrote one). Note v1 stored no set NAMES — only the
        // global size migrates; tiles join the opening session's active set.
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        paint.set_active_set("Body");
        // 1001 is pre-assigned to the default set at build; the global
        // sidecar records the ACTIVE set's size, so route the tile to
        // Body (512) before saving — else the global reads the default
        // set's 4096 and the v1 round-trip premise breaks. The resize
        // REBUILDS the pre-existing 1001 target at 512 (assignment alone
        // steers future creations only — the field docs).
        paint.assign_tile_to_set(1001, "Body");
        paint.resize_set("Body", 512, 512);
        let before = pattern_bytes(512, 512, 11);
        assert_eq!(&before[..4], &[11u8, 82, 153, 224]);
        assert!(paint.publish_fill(1001, 512, 512, before.clone()));
        paint.process_pending().expect("upload lands");

        let dir = unique_temp_dir("legacy-migration");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]),
            1
        );
        std::fs::remove_file(paint_set_sizes_path(&dir)).expect("a v1 project has no mapping");
        assert_eq!(
            std::fs::read_to_string(paint_target_size_path(&dir)).unwrap(),
            "512x512"
        );

        // A fresh 4K session opens it: the 512 global leads (first tile's
        // dims, exactly the v1 rule), and the pixels restore byte-identical.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 1,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(fresh.target_size(), (512, 512));
        assert_eq!(
            fresh.set_of_tile(1001),
            fresh.active_set(),
            "v1 tiles join the opening session's active set (v1 stored no names)"
        );
        let after = fresh.read_tile(1001).expect("readback succeeds");
        assert_eq!((after.width, after.height), (512, 512));
        assert_eq!(
            after.data, before,
            "v1 pixels restore byte-identical under the migrated size"
        );

        // Tiles-less and mapping-less: the legacy global still applies to
        // every set (a size choice with no paint yet survives the round
        // trip) while the pool clears.
        std::fs::remove_file(paint_tile_path(&dir, 1001)).unwrap();
        let mut empty = paint_state::PaintState::new(device, queue).expect("empty session builds");
        let report = load_paint_tiles_reported(&mut empty, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 0,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture's Base is empty (fully transparent): the
                // dormancy decision's non-empty arm stays off.
                base_nonempty: false,
            }
        );
        assert_eq!(
            empty.target_size(),
            (512, 512),
            "the stored global sizes the session with no paint yet"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The stage-2 orphan filter for sessions WITH live layers.
    fn live_layers(ids: &[u64]) -> std::collections::HashSet<u64> {
        ids.iter().copied().collect()
    }

    /// Stage-2 §8 T15: the §6.1 parse rule, exactly as the doc derives it
    /// (no GPU needed — pure codec).
    #[test]
    fn layer_plane_sidecar_names_follow_the_parse_rule() {
        let base = paint_state::PlaneOwner::Base;
        let base_color = paint_state::BASE_COLOR_CHANNEL.to_string();
        assert_eq!(
            parse_paint_sidecar_name("layers/7/baseColor/tile-1001.png"),
            Some((
                paint_state::PlaneOwner::Layer(7),
                "baseColor".to_string(),
                1001
            ))
        );
        // Depth disambiguates a channel literally named `layers`.
        assert_eq!(
            parse_paint_sidecar_name("layers/tile-1001.png"),
            Some((base, "layers".to_string(), 1001))
        );
        for bad in [
            "layers/7/tile-1001.png",
            "layers/x/baseColor/tile-1001.png",
            "layers/7/../tile-1001.png",
            "layers/7/a\\b/tile-1001.png",
            "layers//baseColor/tile-1001.png",
            "layers/7//tile-1001.png",
            "layers/7/baseColor/tile-1001.png/",
            "layers/7/baseColor/tile-abc.png",
        ] {
            assert_eq!(parse_paint_sidecar_name(bad), None, "{bad:?} must reject");
        }
        // The name builder round-trips through the parser exactly (not a
        // contains-check — a renamed nesting would fail).
        for (owner, channel, tile) in [
            (base, base_color.clone(), 1001),
            (base, "height".to_string(), 1002),
            (
                paint_state::PlaneOwner::Layer(7),
                "baseColor".to_string(),
                1001,
            ),
            (
                paint_state::PlaneOwner::Layer(12),
                "normal".to_string(),
                1002,
            ),
        ] {
            let name = paint_plane_sidecar_name(owner, &channel, tile);
            assert_eq!(
                parse_paint_sidecar_name(&name),
                Some((owner, channel, tile)),
                "{name:?} must parse back to its plane"
            );
        }
        assert_eq!(
            paint_plane_sidecar_name(paint_state::PlaneOwner::Layer(7), "baseColor", 1001),
            "layers/7/baseColor/tile-1001.png"
        );
        assert_eq!(
            paint_plane_path(
                Path::new("/p"),
                paint_state::PlaneOwner::Layer(7),
                "baseColor",
                1001
            ),
            PathBuf::from("/p/paint/layers/7/baseColor/tile-1001.png")
        );
        assert_eq!(
            paint_plane_path(Path::new("/p"), base, "baseColor", 1001),
            paint_tile_path(Path::new("/p"), 1001),
            "Base keeps the legacy flat path byte-for-byte"
        );
        // The manifest gate admits layer names (else the commit would
        // refuse its own entries) and still refuses the 3-component shape.
        assert!(is_paint_sidecar_name("layers/7/baseColor/tile-1001.png"));
        assert!(!is_paint_sidecar_name("layers/7/tile-1001.png"));
    }

    /// Stage-2 §8 T16: a v2 save → load gives byte-identical planes per
    /// owner; orphan planes (no live layer) are ignored with a log.
    #[test]
    fn layer_planes_round_trip_per_owner_with_orphans_ignored() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let before_base = pattern_bytes(512, 512, 3);
        let before_layer = pattern_bytes(512, 512, 77);
        // Can-fail guards against a vacuous pass (the seeds pin (0,0) to
        // [3, 74, 145, 216] and [77, 148, 219, 39]).
        assert_eq!(&before_base[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&before_layer[..4], &[77u8, 148, 219, 39]);
        assert_ne!(before_base, before_layer);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                before_base.clone()
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(7),
                1001,
                "baseColor",
                512,
                512,
                before_layer.clone()
            )
            .is_ok());
        paint.process_pending().expect("uploads land");

        let dir = unique_temp_dir("plane-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let live = live_layers(&[7]);
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live, &[]),
            2,
            "Base + one layer plane persist as two artifacts"
        );
        // The manifest header is v2 from stage 4 on (§6.1 [B2, M11] —
        // every save, regardless of `project.json`'s version).
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        assert_eq!(
            manifest.lines().next(),
            Some("# Umber paint commit v2"),
            "stage 4 writes the v2 header on every save"
        );
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert!(
            names.contains(&"tile-1001.png"),
            "Base keeps its flat v2 name, got {names:?}"
        );
        assert!(
            names.contains(&"layers/7/baseColor/tile-1001.png"),
            "the layer plane nests under layers/<id>/, got {names:?}"
        );
        assert!(paint_sidecar_dir(&dir)
            .join("layers/7/baseColor/tile-1001.png")
            .exists());
        assert!(
            !dir.join("layers").exists(),
            "planes never land under layers/<set>/ (the layer-JSON tree)"
        );

        // Open with the layer live: both planes restore byte-identical.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &live);
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        let after_base = fresh
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base restores");
        assert_eq!(after_base.data, before_base);
        let after_layer = fresh
            .read_plane(paint_state::PlaneOwner::Layer(7), 1001, "baseColor")
            .expect("the layer plane restores");
        assert_eq!(after_layer.data, before_layer);

        // Open with NO live layer: the layer plane is an orphan — ignored
        // (never enters the session), Base still restores.
        let mut orphaned =
            paint_state::PaintState::new(device, queue).expect("orphan session builds");
        let report = load_paint_tiles_reported(&mut orphaned, &dir, &no_live_layers());
        assert_eq!(report.restored, 1, "only Base restores for an orphan plane");
        assert!(
            orphaned
                .read_plane(paint_state::PlaneOwner::Layer(7), 1001, "baseColor")
                .is_none(),
            "the orphan plane never enters the session"
        );
        let orphan_base = orphaned
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base restores");
        assert_eq!(orphan_base.data, before_base);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stage-2 §8 T20 [B1]: save-as into a new dir keeps every plane; an
    /// in-place save right after open reads back NOTHING; a corrupted
    /// on-disk sidecar reads back (hash mismatch) instead of re-staging.
    #[test]
    fn save_as_into_new_dir_keeps_every_plane() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let base_bytes = pattern_bytes(512, 512, 3);
        let l1_bytes = pattern_bytes(512, 512, 77);
        let l2_bytes = pattern_bytes(512, 512, 11);
        assert_ne!(l1_bytes, l2_bytes);
        for (owner, bytes) in [
            (paint_state::PlaneOwner::Base, base_bytes.clone()),
            (paint_state::PlaneOwner::Layer(1), l1_bytes.clone()),
            (paint_state::PlaneOwner::Layer(2), l2_bytes.clone()),
        ] {
            assert!(paint
                .publish_plane(owner, 1001, "baseColor", 512, 512, bytes)
                .is_ok());
        }
        paint.process_pending().expect("uploads land");
        let live = live_layers(&[1, 2]);

        let dir_a = unique_temp_dir("saveas-a");
        std::fs::create_dir_all(&dir_a).unwrap();
        assert_eq!(save_paint_tiles(&mut paint, &dir_a, &live, &[]), 3);

        // Corrupt L2's sidecar on disk, then save in place: L2 reads back
        // (hash mismatch), Base + L1 skip (same dir, same bytes).
        let l2_path =
            paint_plane_path(&dir_a, paint_state::PlaneOwner::Layer(2), "baseColor", 1001);
        umber_export::png::write_png(
            &l2_path,
            512,
            512,
            &pattern_bytes(512, 512, 5),
            umber_export::png::Transfer::Linear,
        )
        .expect("clobber writes");
        let report =
            commit_paint_tiles(&mut paint, &dir_a, &live, &[]).expect("in-place save commits");
        assert_eq!(
            report,
            PaintSaveReport {
                staged: 3,
                read_back: 1
            },
            "only the corrupted plane reads back"
        );
        let (width, height, repaired) =
            decode_paint_png(&l2_path).expect("the re-saved L2 decodes");
        assert_eq!((width, height), (512, 512));
        assert_eq!(
            repaired, l2_bytes,
            "a hash mismatch reads back; the corrupt file never re-stages"
        );

        // In-place save right after that commit reads back NOTHING.
        let report = commit_paint_tiles(&mut paint, &dir_a, &live, &[]).expect("re-save commits");
        assert_eq!(
            report,
            PaintSaveReport {
                staged: 3,
                read_back: 0
            },
            "an in-place save of clean planes stages without a readback"
        );

        // Change L1 only, then save-as into a NEW dir: every plane reads
        // back (a save-as shares no committed dir), and B reopens with
        // every plane byte-identical (the r1 sentinel dropped them).
        let l1_new = pattern_bytes(512, 512, 99);
        assert_ne!(l1_new, l1_bytes);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(1),
                1001,
                "baseColor",
                512,
                512,
                l1_new.clone()
            )
            .is_ok());
        paint.process_pending().expect("the L1 change lands");
        let dir_b = unique_temp_dir("saveas-b");
        std::fs::create_dir_all(&dir_b).unwrap();
        let report = commit_paint_tiles(&mut paint, &dir_b, &live, &[]).expect("save-as commits");
        assert_eq!(
            report,
            PaintSaveReport {
                staged: 3,
                read_back: 3
            },
            "a save-as shares no committed dir, so every plane reads back"
        );
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir_b)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("B's manifest parses");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        for want in [
            "tile-1001.png",
            "layers/1/baseColor/tile-1001.png",
            "layers/2/baseColor/tile-1001.png",
        ] {
            assert!(
                names.contains(&want),
                "B's manifest lists {want}, got {names:?}"
            );
        }

        let mut fresh = paint_state::PaintState::new(device, queue).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir_b, &live);
        assert_eq!(report.restored, 3);
        let reopened_base = fresh
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base reopens");
        assert_eq!(reopened_base.data, base_bytes);
        let reopened_l1 = fresh
            .read_plane(paint_state::PlaneOwner::Layer(1), 1001, "baseColor")
            .expect("L1 reopens");
        assert_eq!(reopened_l1.data, l1_new);
        let reopened_l2 = fresh
            .read_plane(paint_state::PlaneOwner::Layer(2), 1001, "baseColor")
            .expect("L2 reopens");
        assert_eq!(reopened_l2.data, l2_bytes);

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    /// Stage-2 §8 T21 [B3]: the save loop reads Base via `read_plane`,
    /// never the composite — `paint/tile-1001.png` decodes to Base's plane
    /// exactly, and a reopen shows no double-count.
    #[test]
    fn save_writes_base_not_the_cache() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let base_bytes = pattern_bytes(512, 512, 3);
        let layer_bytes = pattern_bytes(512, 512, 77);
        assert_eq!(&base_bytes[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&layer_bytes[..4], &[77u8, 148, 219, 39]);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                base_bytes.clone()
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(1),
                1001,
                "baseColor",
                512,
                512,
                layer_bytes.clone()
            )
            .is_ok());
        paint.process_pending().expect("uploads land");
        let live = live_layers(&[1]);

        let dir = unique_temp_dir("base-not-cache");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(save_paint_tiles(&mut paint, &dir, &live, &[]), 2);

        let (width, height, file_bytes) =
            decode_paint_png(&paint_tile_path(&dir, 1001)).expect("the flat tile decodes");
        assert_eq!((width, height), (512, 512));
        assert_eq!(
            file_bytes, base_bytes,
            "the flat path carries Base's plane, never the composite"
        );
        assert_ne!(
            file_bytes, layer_bytes,
            "the layer's bytes must not leak into the flat path"
        );

        // Reopen: Base is still exactly Base (no layer folded in) and the
        // layer plane is intact — no per-cycle copy.
        let mut fresh = paint_state::PaintState::new(device, queue).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &live);
        assert_eq!(report.restored, 2);
        let reopened_base = fresh
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base reopens");
        assert_eq!(
            reopened_base.data, base_bytes,
            "reopening must not composite the layer into Base"
        );
        let reopened_layer = fresh
            .read_plane(paint_state::PlaneOwner::Layer(1), 1001, "baseColor")
            .expect("the layer reopens");
        assert_eq!(reopened_layer.data, layer_bytes);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stage-2 §8 T17 (+ [m10]): a save → save cycle keeps every
    /// `paint/layers/…` file (the `prune_stale_layer_files` guard — planes
    /// never live under `layers/<set>/`), and deleting a layer then saving
    /// leaves no `paint/layers/<id>/` directory behind, empty or not.
    #[test]
    fn layer_plane_directories_prune_with_their_layer() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        for (owner, seed) in [
            (paint_state::PlaneOwner::Base, 3u8),
            (paint_state::PlaneOwner::Layer(7), 77u8),
            (paint_state::PlaneOwner::Layer(9), 11u8),
        ] {
            assert!(paint
                .publish_plane(
                    owner,
                    1001,
                    "baseColor",
                    512,
                    512,
                    pattern_bytes(512, 512, seed)
                )
                .is_ok());
        }
        paint.process_pending().expect("uploads land");

        let dir = unique_temp_dir("layer-dirs");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live_layers(&[7, 9]), &[]),
            3
        );
        let l7 = paint_sidecar_dir(&dir).join("layers/7/baseColor/tile-1001.png");
        let l9 = paint_sidecar_dir(&dir).join("layers/9/baseColor/tile-1001.png");
        assert!(l7.exists());
        assert!(l9.exists());
        let l9_bytes = std::fs::read(&l9).unwrap();

        // Save → save keeps every paint/layers file byte-identical.
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live_layers(&[7, 9]), &[]),
            3
        );
        assert!(l7.exists());
        assert_eq!(
            std::fs::read(&l9).unwrap(),
            l9_bytes,
            "a re-save keeps the surviving layer's bytes"
        );

        // Delete layer 7 (its planes go orphan), then save: no
        // paint/layers/7 directory survives, and layer 9's does.
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live_layers(&[9]), &[]),
            2,
            "the orphan plane never stages"
        );
        assert!(
            !paint_sidecar_dir(&dir).join("layers/7").exists(),
            "no layers/7 directory survives, empty or not"
        );
        assert_eq!(
            std::fs::read(&l9).unwrap(),
            l9_bytes,
            "every other layer's directory survives"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stage-2 §8 T24 (app half): the minimal bump — version 2 iff a LIVE
    /// layer plane exists; orphans don't count. The other half (a loaded
    /// version-2 project re-saved layer-less writes 1) follows from the
    /// same helper `save_project` calls every save: a loaded model is
    /// never re-stamped. `ProjectModel::new` (every headless/CLI session's
    /// path) stays version 1.
    #[test]
    fn save_version_is_minimal_and_new_model_stamps_1() {
        assert_eq!(umber_core::CURRENT_PROJECT_VERSION, 1);
        assert_eq!(
            umber_core::ProjectModel::new(vec![], vec![], umber_core::ProjectSettings::default())
                .version,
            1,
            "the default write version never moves"
        );

        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device, queue, 512).expect("paint builds");
        // No planes at all (beyond Base): version 1.
        assert_eq!(paint_save_version(&paint, &no_live_layers()), 1);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                pattern_bytes(512, 512, 3)
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(7),
                1001,
                "baseColor",
                512,
                512,
                pattern_bytes(512, 512, 77)
            )
            .is_ok());
        paint.process_pending().expect("uploads land");
        // A layer plane with no live layer is an orphan: still 1.
        assert_eq!(
            paint_save_version(&paint, &no_live_layers()),
            1,
            "orphans don't count toward the version decision"
        );
        assert_eq!(
            paint_save_version(&paint, &live_layers(&[9])),
            1,
            "a different live set leaves layer 7 an orphan"
        );
        assert_eq!(
            paint_save_version(&paint, &live_layers(&[7])),
            2,
            "one live layer plane writes version 2"
        );
    }

    /// Stage-2 §8 T30 (paint half): the session lifecycle clear drops
    /// every layer-owned plane (and nothing else), so the next save
    /// writes version 1. (The `fill_states` half lives beside
    /// `reset_for_new`'s own tests.)
    #[test]
    fn clear_layer_planes_drops_layer_planes_not_base() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device, queue, 512).expect("paint builds");
        let base_bytes = pattern_bytes(512, 512, 3);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                base_bytes.clone()
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(3),
                1001,
                "baseColor",
                512,
                512,
                pattern_bytes(512, 512, 77)
            )
            .is_ok());
        paint.process_pending().expect("uploads land");
        assert!(
            paint
                .read_plane(paint_state::PlaneOwner::Layer(3), 1001, "baseColor")
                .is_some(),
            "the layer plane exists before the clear (else the test is vacuous)"
        );

        paint.clear_layer_planes();

        assert!(
            paint
                .read_plane(paint_state::PlaneOwner::Layer(3), 1001, "baseColor")
                .is_none(),
            "no (Layer(3), …) plane survives the lifecycle clear"
        );
        let base = paint
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base survives the layer clear");
        assert_eq!(base.data, base_bytes);
        assert_eq!(
            paint_save_version(&paint, &no_live_layers()),
            1,
            "a cleared session saves version 1"
        );
    }

    /// The paint-relevant files under `paint/` (sidecar names only — the
    /// provenance manifest and temps are not what a reopen reads).
    fn sidecar_snapshot(project: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(paint_sidecar_dir(project)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_paint_sidecar_name(&name) {
                    out.insert(name, std::fs::read(entry.path()).unwrap());
                }
            }
        }
        out
    }

    fn snapshot_of(files: &[(&str, &[u8])]) -> std::collections::BTreeMap<String, Vec<u8>> {
        files
            .iter()
            .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
            .collect()
    }

    /// Leftovers a reopen must never leave: staged/restore temps and an
    /// unapplied manifest.
    fn commit_leftovers(project: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(paint_sidecar_dir(project))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| is_paint_temp_name(name) || name == "manifest.pending")
            .collect();
        out.sort();
        out
    }

    /// The save sequence `commit_paint_tiles` runs (stage each file →
    /// commit the manifest → apply), driven from bytes so no GPU is
    /// needed, with ONE step budget spanning all three phases: the
    /// `budget`-th step (0-based) "crashes". A crashing stage leaves a
    /// half-written temp; a crashing commit a half-written manifest temp;
    /// apply's own faults come from its `max_steps` seam.
    fn save_files_faulted(
        project: &Path,
        files: &[(&str, &[u8])],
        budget: usize,
    ) -> std::io::Result<()> {
        let dir = paint_sidecar_dir(project);
        std::fs::create_dir_all(&dir)?;
        let store = umber_core::AssetStore::new(project);
        let mut used = 0usize;
        let mut entries = Vec::new();
        for (name, bytes) in files {
            if used == budget {
                std::fs::write(dir.join(paint_temp_name(name)), &bytes[..bytes.len() / 2])?;
                return Err(std::io::Error::other("injected fault"));
            }
            used += 1;
            entries.push(stage_paint_bytes(project, &store, name, bytes)?);
        }
        if used == budget {
            let text = format_paint_commit(&entries, &[]);
            std::fs::write(
                dir.join(paint_temp_name("manifest.pending")),
                &text.as_bytes()[..text.len() / 2],
            )?;
            return Err(std::io::Error::other("injected fault"));
        }
        used += 1;
        commit_paint_manifest(project, &entries, &[])?;
        apply_paint_commit(project, Some(budget - used)).map(|_| ())
    }

    const OLD_FILES: [(&str, &[u8]); 4] = [
        ("tile-1001.png", b"old tile 1001 bytes"),
        ("tile-1002.png", b"old tile 1002 bytes"),
        ("target_size.txt", b"512x512"),
        (
            "target_sizes.txt",
            b"# Umber paint-set sizes v1\nBody=512x512\ntile-1001=Body\ntile-1002=Body\n",
        ),
    ];

    /// The NEW save differs in every way a torn mix could show: tile 1001
    /// repainted, tile 1002 dropped (pruned), tile 1003 added, both size
    /// files changed.
    const NEW_FILES: [(&str, &[u8]); 4] = [
        ("tile-1001.png", b"NEW tile 1001 bytes, repainted"),
        ("tile-1003.png", b"NEW tile 1003 bytes"),
        ("target_size.txt", b"1024x1024"),
        (
            "target_sizes.txt",
            b"# Umber paint-set sizes v1\nBody=1024x1024\ntile-1001=Body\ntile-1003=Body\n",
        ),
    ];

    #[test]
    fn paint_commit_codec_round_trips_and_rejects_tampering() {
        let asset = umber_core::AssetRef {
            hash: "ab".repeat(32),
            ext: "png".to_string(),
            size: 42,
        };
        let entries = vec![PaintCommitEntry {
            name: "tile-1001.png".to_string(),
            temp: "tile-1001.png.tmp.77".to_string(),
            asset: asset.clone(),
        }];
        let text = format_paint_commit(&entries, &[]);
        let hash = "ab".repeat(32);
        let line = format!("tile-1001.png {hash} png 42 tile-1001.png.tmp.77\n");
        assert_eq!(text, format!("# Umber paint commit v2\n{line}"));
        assert_eq!(parse_paint_commit(&text), Some(entries));
        // Any bad line refuses the WHOLE manifest (never half-applied):
        // an escaping name, a temp for another name, a non-temp, a short
        // hash, a non-numeric size, a missing field.
        for bad in [
            format!("../x.png {hash} png 42 ../x.png.tmp.77\n"),
            format!("tile-1001.png {hash} png 42 tile-1002.png.tmp.77\n"),
            format!("tile-1001.png {hash} png 42 tile-1001.png\n"),
            "tile-1001.png abc png 42 tile-1001.png.tmp.77\n".to_string(),
            format!("tile-1001.png {hash} png x tile-1001.png.tmp.77\n"),
            format!("tile-1001.png {hash} png 42\n"),
        ] {
            assert_eq!(parse_paint_commit(&bad), None, "{bad:?} must refuse");
        }
        assert!(is_paint_temp_name("tile-1001.png.tmp.123"));
        assert!(is_paint_temp_name("tile-1001.png.restore.tmp.9"));
        assert!(!is_paint_temp_name("tile-1001.png"));
        assert!(!is_paint_temp_name("tile-1001.png.tmp."));
        assert!(!is_paint_temp_name("manifest.txt"));
    }

    #[test]
    fn asset_blob_path_matches_the_store_layout() {
        // The fsync before the commit targets this mirrored path; pin it
        // against a real store so a layout change fails here, not at a
        // power cut.
        let project = unique_temp_dir("blob-path");
        std::fs::create_dir_all(&project).unwrap();
        let asset = umber_core::AssetStore::new(&project)
            .store(b"blob path probe", "png")
            .unwrap();
        let path = paint_asset_blob_path(&project, &asset);
        assert_eq!(std::fs::read(&path).unwrap(), b"blob path probe");
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn interrupted_paint_save_reopens_old_or_new_never_torn() {
        let old = snapshot_of(&OLD_FILES);
        let new = snapshot_of(&NEW_FILES);
        // The old project in both shapes a save can meet: one an earlier
        // atomic save committed, and a legacy (pre-commit) direct write.
        for legacy_old in [false, true] {
            let mut saw_torn_disk = false;
            let mut budget = 0usize;
            loop {
                let project = unique_temp_dir("torn");
                if legacy_old {
                    for (name, bytes) in OLD_FILES {
                        std::fs::create_dir_all(paint_sidecar_dir(&project)).unwrap();
                        std::fs::write(paint_sidecar_dir(&project).join(name), bytes).unwrap();
                    }
                } else {
                    save_files_faulted(&project, &OLD_FILES, usize::MAX).unwrap();
                }
                assert_eq!(sidecar_snapshot(&project), old);

                let completed = save_files_faulted(&project, &NEW_FILES, budget).is_ok();
                let on_disk = sidecar_snapshot(&project);
                if on_disk != old && on_disk != new {
                    saw_torn_disk = true;
                }
                // "Reopen": the load path's recovery, then what it reads.
                recover_paint_sidecar(&project);
                let reopened = sidecar_snapshot(&project);
                // Exact pin, not just old-or-new: 4 stages + the commit
                // are steps 0..=4, so a crash at budget ≤ 4 predates the
                // commit (OLD); any later crash rolls forward (NEW).
                let committed = budget > NEW_FILES.len();
                let expected = if committed { &new } else { &old };
                assert_eq!(
                    &reopened, expected,
                    "legacy_old={legacy_old} crash at step {budget}: committed={committed}"
                );
                assert_eq!(
                    commit_leftovers(&project),
                    Vec::<String>::new(),
                    "legacy_old={legacy_old} step {budget}: reopen sweeps every temp"
                );
                let _ = std::fs::remove_dir_all(&project);
                if completed {
                    break;
                }
                budget += 1;
            }
            // 4 stages + 1 commit + 4 renames + 1 prune (tile 1002) + 1
            // manifest finalize = 11 steps; budget 11 is the first clean run.
            assert_eq!(budget, 11, "legacy_old={legacy_old}: the step count");
            // Can-fail guard: the disk WAS torn mid-apply at some step (a
            // protocol that only ever exposed old-or-new states would make
            // the recovery assertions above vacuous).
            assert!(
                saw_torn_disk,
                "legacy_old={legacy_old}: no crash tore the disk"
            );
        }
    }

    #[test]
    fn reopen_after_a_post_commit_crash_restores_the_new_paint() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // OLD saved for real; then a NEW tile staged and committed but
        // never applied (the crash lands right after the commit point).
        // The load path's own recovery must roll it forward: without it
        // the reopen reads the OLD tile-1001.png and this test fails.
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let old = pattern_bytes(512, 512, 5);
        let new = pattern_bytes(512, 512, 99);
        assert_ne!(old, new);
        assert!(paint.publish_fill(1001, 512, 512, old.clone()));
        paint.process_pending().expect("upload lands");
        let dir = unique_temp_dir("post-commit-crash");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]),
            1
        );

        let store = umber_core::AssetStore::new(&dir);
        let entry = stage_paint_png(&dir, &store, "tile-1001.png", 512, 512, &new).unwrap();
        commit_paint_manifest(&dir, &[entry], &[]).unwrap();
        assert!(paint_commit_pending_path(&dir).exists());

        let mut fresh = paint_state::PaintState::with_target_size(device, queue, 512)
            .expect("fresh paint state builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(report.restored, 1);
        assert_eq!(
            fresh.read_tile(1001).expect("readback succeeds").data,
            new,
            "the committed NEW paint is what reopens"
        );
        assert!(!paint_commit_pending_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn committed_save_rolls_forward_from_the_store_when_a_temp_is_lost() {
        let project = unique_temp_dir("lost-temp");
        save_files_faulted(&project, &OLD_FILES, usize::MAX).unwrap();
        // Commit NEW, then lose tile 1001's staged temp: the store blob is
        // the roll-forward source. A first recovery attempt crashes mid
        // re-materialization (leaving a half `.restore` temp); the next
        // one must still land NEW byte-exactly and sweep the torn temp.
        let store = umber_core::AssetStore::new(&project);
        let entries: Vec<PaintCommitEntry> = NEW_FILES
            .iter()
            .map(|(name, bytes)| stage_paint_bytes(&project, &store, name, bytes).unwrap())
            .collect();
        commit_paint_manifest(&project, &entries, &[]).unwrap();
        std::fs::remove_file(paint_sidecar_dir(&project).join(&entries[0].temp)).unwrap();
        assert!(apply_paint_commit(&project, Some(0)).is_err());
        let torn_restore = paint_temp_name("tile-1001.png.restore");
        assert_eq!(
            std::fs::read(paint_sidecar_dir(&project).join(&torn_restore)).unwrap(),
            &NEW_FILES[0].1[..NEW_FILES[0].1.len() / 2],
            "the injected crash left a half-written restore temp"
        );
        recover_paint_sidecar(&project);
        assert_eq!(sidecar_snapshot(&project), snapshot_of(&NEW_FILES));
        assert_eq!(commit_leftovers(&project), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn store_routed_save_reopens_byte_identical_and_dedupes() {
        let project = unique_temp_dir("store-routed");
        save_files_faulted(&project, &NEW_FILES, usize::MAX).unwrap();
        recover_paint_sidecar(&project);
        let reopened = sidecar_snapshot(&project);
        assert_eq!(reopened, snapshot_of(&NEW_FILES));
        // Every sidecar has a manifest line whose store blob is its exact
        // bytes (the provenance record is complete and truthful).
        let manifest = std::fs::read_to_string(paint_manifest_path(&project)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        let want: Vec<&str> = NEW_FILES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, want, "one manifest line per sidecar, in save order");
        let store = umber_core::AssetStore::new(&project);
        for entry in &entries {
            let blob = store.load(&entry.asset).unwrap();
            assert_eq!(&blob, &reopened[&entry.name], "{} blob", entry.name);
            assert_eq!(entry.asset.size, blob.len() as u64);
        }
        // Dedupe: re-saving identical content stores no new blob — 4
        // distinct contents, 4 blobs, before and after.
        let count_blobs = || {
            std::fs::read_dir(project.join("assets"))
                .unwrap()
                .flatten()
                .map(|shard| std::fs::read_dir(shard.path()).unwrap().count())
                .sum::<usize>()
        };
        assert_eq!(count_blobs(), 4);
        save_files_faulted(&project, &NEW_FILES, usize::MAX).unwrap();
        assert_eq!(count_blobs(), 4, "identical content dedupes in the store");
        let again = std::fs::read_to_string(paint_manifest_path(&project)).unwrap();
        let again = parse_paint_commit(&again).unwrap();
        assert_eq!(
            again.iter().map(|e| &e.asset).collect::<Vec<_>>(),
            entries.iter().map(|e| &e.asset).collect::<Vec<_>>(),
            "the same bytes hash to the same refs"
        );
        assert_eq!(sidecar_snapshot(&project), snapshot_of(&NEW_FILES));
        let _ = std::fs::remove_dir_all(&project);
    }
}
