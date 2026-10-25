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

mod actions;
mod assets_panel;
mod axis_gizmo;
mod bake_sources;
mod bakes_panel;
mod brush_cursor;
mod brush_panel;
mod crash;
mod display_keys;
mod display_panel;
mod document;
mod empty_state;
mod env;
mod export_dialog;
mod fill_layer;
mod first_run;
mod gpu_startup;
mod graph_canvas;
mod graph_panel;
mod history_panel;
mod i18n;
#[cfg(test)]
mod input_photon;
mod layers_panel;
mod logfile;
mod navigation;
mod palette;
mod preferences;
mod recent;
mod recovery;
mod rich_tooltip;
mod shell_menu;
mod workspaces;
use umber_paint::material_layer;
mod mesh_map_import;
use umber_paint::paint_state;
mod path_tool;
#[cfg(test)]
mod perf_soak;
mod plugins;
mod postfx_panel;
mod psd_import;
use umber_paint::persistence::{
    dormancy_for, load_paint_tiles_reported, paint_manifest_path, paint_save_version,
    parse_manifest_header, save_paint_tiles, LayerDormancy, OpenSeedRow,
};
use umber_paint::snapshot::build_composite_snapshot;
mod size_presets;
mod spp_import;
mod status_bar;
mod stylus_input;
mod thumbnails;
mod tile_selection;
mod toolbar;
mod uv_view;
mod viewport;

// The lib now owns the stencil session module (the set-switch L1
// extraction): the bin re-exports it at the crate root so every
// existing `crate::stencil::` / `stencil::` path in the shell keeps
// resolving unchanged, and the glue module likewise.
use set_switch::PendingResize;
pub use umber_app::set_switch;
pub use umber_app::stencil;

use bakes_panel::{BakesContext, BakesPanel};
use brush_panel::BrushPanel;
use egui::containers::menu::{MenuBar, MenuButton};
use egui::{CentralPanel, Id, Ui, WidgetText};
use egui_dock::{DockArea, DockState, NodePath, Style, TabViewer};
use export_dialog::{ExportContext, ExportDialog};
use graph_panel::GraphPanel;
use i18n::tr;
use std::path::{Path, PathBuf};
use umber_gpu::GpuContext;
use umber_gui::widgets::tab_overflow;
use uv_view::UvView;
use viewport::Viewport;

/// A panel's localized title: the dock tab and Window ▸ Panels read it
/// from here, so the two never name a panel differently.
fn panel_title(panel: &Panel) -> String {
    tr(match panel {
        Panel::Viewport => "panel.viewport",
        Panel::UvView => "panel.uv-view",
        Panel::LayerStack => "panel.layers",
        Panel::Properties => "panel.properties",
        Panel::Assets => "panel.assets",
        Panel::History => "panel.history",
        Panel::TextureSets => "panel.texture-sets",
        Panel::Bakes => "panel.bakes",
        Panel::Export => "panel.export",
        Panel::Graph => "panel.graph",
        Panel::Display => "panel.display",
    })
}

// --- Tab-strip overflow (GUI plan g5-tab-overflow, the dock wiring) ---
//
// The pure half lives in `umber_gui::widgets::tab_overflow`; this is the
// shell side (docs/specs/g5-tab-overflow-design.md §1, §4):
// - `TabViewer::title` returns a PRE-LAID galley, elided at
//   `TAB_TITLE_MAX`: egui_dock 0.21.1 lays a string/LayoutJob title out
//   at an infinite width (`leaf.rs` `tab_title`), but passes a
//   `WidgetText::Galley` through untouched. The full title rides the
//   tab's hover text (`on_tab_button`) whenever the galley was cut.
// - After each `DockArea::show`, every leaf whose ACTIVE tab changed
//   since the last pass gets its `LeafNode::scroll` set so that tab is
//   visible (`tab_overflow::scroll_into_view`). Only a change moves the
//   strip, so a user's own wheel/drag scroll is never fought.
//
// The width arithmetic below restates egui_dock 0.21.1's `tab_bar` /
// `tab_title` layout (its button sizes are `pub(crate)`); the pinned
// tests in `tab_overflow_dock_tests` fail loudly if a bump changes it.

/// egui_dock 0.21.1's tab-bar button size (`Style::TAB_CLOSE_BUTTON_SIZE`,
/// `TAB_CLOSE_ALL_BUTTON_SIZE`, `TAB_COLLAPSE_BUTTON_SIZE`: all 24 pt and
/// crate-private there).
const DOCK_BUTTON_SIZE: f32 = 24.0;
/// egui_dock 0.21.1's horizontal title padding inside a tab (`x_spacing`
/// in `tab_title`, applied on both sides).
const DOCK_TAB_X_SPACING: f32 = 8.0;

/// A tab title as the dock draws it: one row, cut with "…" past
/// `TAB_TITLE_MAX`, pre-laid so egui_dock cannot re-wrap it.
fn tab_title_text(ctx: &egui::Context, title: &str, font: egui::FontId) -> WidgetText {
    WidgetText::Galley(tab_overflow::elided_title(
        ctx,
        title,
        font,
        tab_overflow::TAB_TITLE_MAX,
    ))
}

/// Each tab's laid-out width from its title galley widths, the way
/// egui_dock 0.21.1 allocates them: `max(minimum_width, text + 2·8 +
/// close)`, plus the inter-tab `spacing`. The spacing is counted at the
/// END of every tab but the last, so `sum(widths[..k])` is exactly tab
/// `k`'s left edge and the sum is the strip's full width — the contract
/// `tab_overflow::scroll_into_view` reads.
fn dock_tab_widths(text: &[f32], spacing: f32, minimum: Option<f32>, close: f32) -> Vec<f32> {
    let n = text.len();
    text.iter()
        .enumerate()
        .map(|(i, &w)| {
            let tab = minimum
                .unwrap_or(0.0)
                .max(w + 2.0 * DOCK_TAB_X_SPACING + close);
            if i + 1 < n {
                tab + spacing
            } else {
                tab
            }
        })
        .collect()
}

/// The visible width of a leaf's tab strip: the leaf's width less the
/// tab bar's horizontal inner margin and the two buttons the shell's
/// `DockArea` shows at the bar's end (close-all and collapse; no add
/// button) — egui_dock 0.21.1 `tab_bar`'s `available_width`.
fn dock_bar_width(leaf_width: f32, margin_x: f32) -> f32 {
    (leaf_width - margin_x - 2.0 * DOCK_BUTTON_SIZE).max(0.0)
}

/// Scrolls every leaf whose active tab changed since the last call so
/// that tab is visible; `seen` remembers each leaf's last active panel.
/// A leaf not laid out yet (its rect is still `Rect::NOTHING`) is left
/// unrecorded, so it is handled once it has a width. Returns whether
/// any strip moved (the caller repaints, since the dock already drew).
fn scroll_active_tabs_into_view(
    dock: &mut DockState<Panel>,
    seen: &mut std::collections::HashMap<NodePath, Panel>,
    ctx: &egui::Context,
    font: &egui::FontId,
    style: &Style,
) -> bool {
    let close = DOCK_BUTTON_SIZE.min(style.tab_bar.height);
    let margin_x = style.tab_bar.inner_margin.sum().x;
    let mut live = std::collections::HashSet::new();
    let mut moved = false;
    for (path, leaf) in dock.iter_leaves_mut() {
        live.insert(path);
        let Some(active) = leaf.tabs.get(leaf.active.0) else {
            continue;
        };
        if seen.get(&path) == Some(active) {
            continue;
        }
        let leaf_width = leaf.rect.width();
        if !leaf_width.is_finite() || leaf_width <= 0.0 {
            continue;
        }
        seen.insert(path, active.clone());
        let text: Vec<f32> = leaf
            .tabs
            .iter()
            .map(|panel| {
                tab_overflow::elided_title(
                    ctx,
                    &panel_title(panel),
                    font.clone(),
                    tab_overflow::TAB_TITLE_MAX,
                )
                .size()
                .x
            })
            .collect();
        let widths = dock_tab_widths(&text, style.tab.spacing, style.tab.minimum_width, close);
        let bar = dock_bar_width(leaf_width, margin_x);
        let scroll = tab_overflow::scroll_into_view(&widths, leaf.active.0, bar, leaf.scroll);
        if scroll != leaf.scroll {
            leaf.scroll = scroll;
            moved = true;
        }
    }
    seen.retain(|path, _| live.contains(path));
    moved
}

/// Every panel the shell can dock. Serializable: the workspace layouts
/// persist across launches (`workspaces::save_layouts`).
#[derive(Clone, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
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
    /// The UI context, for laying out the elided tab titles: `title()`
    /// gets no `Ui` (g5-tab-overflow).
    ctx: egui::Context,
    /// The font the tab titles are laid out in (the Button text style,
    /// egui_dock's own choice for a plain-string title).
    tab_font: egui::FontId,
    viewport: &'a mut Viewport,
    uv_view: &'a mut UvView,
    /// The home card's recent-projects source (the empty-viewport card,
    /// g5-home-screen). None keeps the card at its no-input shape.
    home_recent: Option<&'a mut recent::RecentProjects>,
    /// The autosave snapshot a previous run left (the card's Recover row;
    /// None hides the row).
    home_recovery: Option<&'a recovery::RecoveryMeta>,
    /// The home card's actions this frame (drained by the app after the
    /// dock: the viewer cannot dispatch on the app itself).
    home_actions: Vec<crate::empty_state::HomeCardAction>,
    bakes: &'a mut BakesPanel,
    /// The Assets panel's session state (R-C1's library browser).
    assets: &'a mut assets_panel::AssetsPanel,
    export: &'a mut ExportDialog,
    graph: &'a mut GraphPanel,
    mesh: Option<&'a umber_mesh::MeshData>,
    mesh_path: Option<&'a std::path::Path>,
    gpu: &'a GpuContext,
    paint: Option<&'a mut paint_state::PaintState>,
    doc: &'a mut document::Document,
    brush: &'a mut BrushPanel,
    /// The Properties context rail's state (the Properties panel draws
    /// it; deciding apparatus itself is `properties_kinds`).
    properties_rail: &'a mut umber_gui::ContextRail,
    /// The icon atlas (the rail's kinds draw from the Panel family).
    icons: &'a umber_gui::IconAtlas,
    mesh_import: &'a mut mesh_map_import::MeshMapImportState,
    /// Inactive sets' channel lists (C2-9 stage 4).
    parked_texture_sets: &'a mut std::collections::BTreeMap<String, umber_core::TextureSet>,
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
    /// The layer thumbnails (g2-layer-thumbnails).
    thumbnails: &'a mut thumbnails::Thumbnails,
    /// The Layers panel's gesture state (g2-layer-gestures): selection,
    /// mask solo, the pending ✕ confirm.
    layer_gestures: &'a mut layers_panel::gestures::GestureState,
    /// A texture-set resize waiting for its confirm.
    pending_resize: &'a mut Option<PendingResize>,
    /// Fill layers blocked by the plane budget: layer id → badge line.
    fill_badges: &'a std::collections::HashMap<u64, String>,
    /// Import-created layers with refused publishes: layer id → line.
    import_badges: &'a std::collections::HashMap<u64, String>,
    /// Dormant fill/material layers (§6.3): layer ids.
    dormant: &'a std::collections::HashSet<u64>,
    /// The session stencil (the Display panel's stencil section edits it).
    stencil: &'a mut stencil::StencilState,
}

impl TabViewer for PanelViewer<'_> {
    type Tab = Panel;

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab_title_text(&self.ctx, &panel_title(tab), self.tab_font.clone())
    }

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        // A cut title keeps its full text in the hover (g5-tab-overflow);
        // the layout is memoized, so re-laying it here is a cache hit.
        let title = panel_title(tab);
        let galley = tab_overflow::elided_title(
            &self.ctx,
            &title,
            self.tab_font.clone(),
            tab_overflow::TAB_TITLE_MAX,
        );
        if galley.elided {
            response.clone().on_hover_text(title);
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            Panel::Viewport => {
                // The contextual strip (GUI plan g6-contextual-toolbar):
                // the active tool's key params over the viewport, same
                // values the Brush panel edits. The strip edits the
                // panel's own state (no second copy); size/hardness ride
                // StripEdit back into the paint session through the same
                // setters the [` / `] keys use.
                {
                    let size = self.paint.as_deref().map_or(1.0, |p| p.brush_size());
                    let preset = self
                        .brush
                        .active
                        .and_then(|i| self.brush.library.entries.get_mut(i))
                        .map(|e| &mut e.preset);
                    let mut view = toolbar::ContextualView {
                        size,
                        preset,
                        mirror: &mut self.brush.symmetry.mirror,
                        paint_available: self.paint.is_some(),
                    };
                    for edit in toolbar::contextual_strip(ui, &mut view) {
                        match edit {
                            toolbar::StripEdit::Size(v) => {
                                if let Some(paint) = self.paint.as_deref_mut() {
                                    paint.set_brush_size(v);
                                }
                            }
                            toolbar::StripEdit::Preset => {
                                self.brush.dirty = true;
                            }
                            toolbar::StripEdit::LazyToggle => {
                                self.brush.dirty = true;
                            }
                            toolbar::StripEdit::SymmetryToggle => {
                                // sync_symmetry hands the panel's settings
                                // to the session at the next begin_stroke
                                // (the end-of-frame flush does the sync).
                            }
                        }
                    }
                }
                // The eyedropper arm syncs from the strip's tool state each
                // frame (Pick highlights; the brush keeps its last real
                // tool — Pick never strokes, the viewport suppresses paint
                // while armed). A sample becomes the brush color through
                // the paint session's existing color seam.
                let pick_armed = self.brush.strip == toolbar::ToolKind::Pick;
                self.viewport.set_pick_armed(pick_armed);
                // The picker samples what the artist sees: the session's
                // viewer chain (the same DisplaySettings the Display panel
                // edits) rides along every frame, identity by default.
                self.viewport.set_display_settings(self.display);
                // The ID-mask arm (C1 worklist #11): same strip-only shape.
                // A click bakes the mesh's region IDs at the paint target's
                // size and selects the clicked region through the paint
                // session (which arms the quick-mask gate); failures log
                // and leave the mask alone.
                let idmask_armed = self.brush.strip == toolbar::ToolKind::IdMask;
                self.viewport.set_idmask_armed(idmask_armed);
                // The W-C4 L3 kill: pre-warm the ID bake cache at tool
                // entry — the first click's full-frame bake (the
                // 16M-texel hitch) happens HERE, on the armed frame,
                // so the click selects without the entry-latency
                // hitch. Opportunistic: no stroke in flight, a
                // missing mesh, and a bake error all no-op cleanly.
                if idmask_armed {
                    if let (Some(p), Some(mesh)) = (self.paint.as_deref_mut(), self.mesh.as_ref()) {
                        let _ = p.prewarm_id_bake_cache(mesh);
                    }
                }
                let home = self.home_recent.as_deref_mut().zip(self.home_recovery).map(
                    |(recent, recovery)| crate::empty_state::HomeCardInput {
                        recent,
                        recovery: Some(recovery),
                    },
                );
                let click = self.viewport.ui_with_home(
                    ui,
                    self.gpu,
                    self.paint.as_deref_mut(),
                    self.stylus_pressure,
                    self.display_lut,
                    self.postfx,
                    home,
                    Some(&mut *self.stencil),
                );
                // The card's click (or the double-click) is collected and
                // the app drains it after the dock: the viewer cannot
                // reach the app's dispatch/open methods from here.
                if let Some(action) = self.viewport.take_home_action() {
                    self.home_actions.push(action);
                }
                if let Some(click) = click {
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
                // Thumbnails first (they read the paint session), then the
                // rows draw from the finished map. Rebuild-budgeted.
                let mut thumbs = std::collections::HashMap::new();
                if let Some(paint) = self.paint.as_deref_mut() {
                    let live: Vec<u64> = self.doc.layers().iter().map(|l| l.id).collect();
                    self.thumbnails.prune(&live);
                    let mut budget = thumbnails::MAX_REBUILDS_PER_FRAME;
                    for &id in &live {
                        if let Some(t) = self.thumbnails.get(ui.ctx(), paint, id, &mut budget) {
                            thumbs.insert(id, t);
                        }
                    }
                    // Budget spent = possibly more stale thumbnails: come
                    // back next frame. A budget left over means everything
                    // is current, so an idle panel never spins frames.
                    if budget == 0 {
                        ui.ctx().request_repaint();
                    }
                }
                // The panel chords (Ctrl/Cmd+D, Ctrl/Cmd+G, Delete) only
                // while the pointer is over this tab's body: egui_dock draws
                // only the visible tab, and the hover keeps the graph
                // canvas's own Delete (and every other panel's keys) intact.
                // Polled before the rows so the chord is consumed first.
                // A session that never clicked a row still has a paint
                // target (auto-resolved, or restored on open): seed the
                // selection with it, so Delete / Ctrl+D act on the layer
                // the rows highlight.
                // Seed only when the paint TARGET moved since the last
                // seed (a target switch invalidates an empty selection);
                // a deliberate deselect (selection emptied, target
                // unchanged) must STICK, so no per-frame reseed.
                if self.layer_gestures.should_reseed_from(active) {
                    sync_gesture_selection(&mut *self.layer_gestures, active);
                }
                let layer_key = if ui.rect_contains_pointer(ui.max_rect()) {
                    layers_panel::gestures::poll_layer_keys(ui.ctx())
                } else {
                    None
                };
                let mut outcome = document::layers_ui(
                    ui,
                    self.doc,
                    mats,
                    active,
                    *self.base_visible,
                    &badges,
                    &thumbs,
                    &*self.layer_gestures,
                );
                if let Some(key) = layer_key {
                    outcome.gestures.push(self.layer_gestures.key_gesture(key));
                }
                // The rows' gestures + the key, through the ONE gesture
                // state; each outcome mirrors onto the paint session.
                for gesture in std::mem::take(&mut outcome.gestures) {
                    let applied = self.layer_gestures.apply(self.doc, gesture);
                    if applied.solo_changed {
                        // No render path reads the solo yet (the gestures
                        // module docs); the row outline is the only
                        // consumer, so a repaint is the whole refresh.
                        ui.ctx().request_repaint();
                    }
                    // The R-B1 flatten (the gesture outcome's heavy half,
                    // owned by the shell): the selection's isolated bake
                    // becomes ONE new paint layer's own planes; the
                    // sources remove through the journal. Fully undoable —
                    // the sources' planes tombstone (§5.4), the journal's
                    // reverts restore both halves. No confirm gate: undo
                    // IS the safety net (the named follow-up may add one).
                    if !applied.flatten.is_empty() {
                        let ids = applied.flatten.clone();
                        if let Some(paint) = self.paint.as_deref_mut() {
                            let snapshot = paint.synthesized_snapshot();
                            if let Some(baked) = paint.bake_layers(&snapshot, &ids) {
                                let n = self.doc.layers().len() + 1;
                                let name = crate::i18n::tr_args(
                                    "layers.default-paint-name",
                                    [("n", fluent::FluentValue::from(n))],
                                );
                                self.doc.run(umber_core::layers::LayerCommand::add(
                                    name,
                                    umber_core::layers::LayerKind::Paint,
                                ));
                                let Some(new_id) = self.doc.layers().last().map(|l| l.id) else {
                                    continue;
                                };
                                for plane in &baked {
                                    let _ = paint.publish_plane(
                                        paint_state::PlaneOwner::Layer(new_id),
                                        plane.tile,
                                        &plane.channel,
                                        plane.width,
                                        plane.height,
                                        plane.data.clone(),
                                    );
                                }
                                for id in &ids {
                                    self.doc.run(umber_core::layers::LayerCommand::remove(*id));
                                }
                                // The baked layer becomes the selection,
                                // the paint target, and — if a source was
                                // being mask-edited — the mask edit closes.
                                self.layer_gestures.selection.set_single(new_id);
                                paint.set_active_target(paint_state::PlaneOwner::Layer(new_id));
                                if paint.mask_edit_layer().is_some_and(|m| ids.contains(&m)) {
                                    paint.set_mask_edit_layer(None);
                                }
                            } else {
                                // The engine refused (a stale selection, a
                                // one-id floor): log it, the same surface
                                // the duplicate-refusal uses.
                                log::warn!(
                                    "flatten: the bake refused for {ids:?} — the selection \
                                     must hold two or more live rows"
                                );
                            }
                        }
                    }
                    if let Some(paint) = self.paint.as_deref_mut() {
                        apply_layer_session_ops(paint, &layer_session_ops(&applied));
                    }
                }
                // Smart material generators (C2-10): each fill's mesh map is
                // the Bakes panel's newest bake of it, snapshotted now.
                if let Some((material, ids)) = &outcome.smart_applied {
                    let missing =
                        document::bind_smart_generators(self.doc, material, ids, |name| {
                            let path = self.bakes.latest_map(name)?;
                            match umber_export::png::read_png_rgba8(path) {
                                Ok(image) => Some(image),
                                Err(e) => {
                                    log::warn!("smart material: {}: {e}", path.display());
                                    None
                                }
                            }
                        });
                    if !missing.is_empty() {
                        log::info!(
                            "smart material '{}': bake {} in the Bakes panel, then apply again to bind them",
                            material.name,
                            missing.join(", ")
                        );
                    }
                }
                // Smart mask (C2-10): the attached mask's pixels are the
                // preset composed over the newest bakes, on every tile of
                // the active set at the set's size. The bakes' sRGB PNGs
                // are linearized first (vector maps are already linear).
                if let Some((layer, mask)) = &outcome.smart_mask_applied {
                    if let Some(paint) = self.paint.as_deref_mut() {
                        compose_mask_flow(paint, self.doc, self.bakes, *layer, mask);
                    }
                }
                // Duplicate with content (C2-8): the document made the copy's
                // settings; its pixels come from the source's planes.
                if let Some((source, copy)) = outcome.duplicated {
                    if let Some(paint) = self.paint.as_deref_mut() {
                        if let Err(refusal) = paint.copy_owner_planes(
                            paint_state::PlaneOwner::Layer(source),
                            paint_state::PlaneOwner::Layer(copy),
                        ) {
                            log::warn!("duplicate: plane copy refused: {refusal:?}");
                        }
                    }
                }
                if let Some(target) = outcome.select {
                    if let Some(paint) = self.paint.as_deref_mut() {
                        paint.set_active_target(target);
                    }
                    // The add buttons, the shelf and the Base row select
                    // outside the gestures: the multi-selection follows.
                    sync_gesture_selection(&mut *self.layer_gestures, target);
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
                    // ... and the preset's dab params, stroke method and
                    // size (brush-wiring, 2026-10-15). Before this, the
                    // session painted BrushParams::default() forever: the
                    // preset editor's alpha / hardness / gamma / falloff /
                    // color never reached a stroke.
                    // Live size / hardness edits from the viewport (`[` / `]`,
                    // Ctrl+RMB) flow into the preset first, so the sync
                    // below does not snap them back.
                    if !paint.is_stroking() {
                        self.brush
                            .adopt_live_size(paint.brush_size(), paint.brush_hardness());
                    }
                    let preset_changed = self.brush.take_preset_changed();
                    if let Some(preset) = self.brush.active_preset() {
                        // An eyedropper pick overrides the preset's color
                        // until the preset itself changes.
                        paint.sync_brush_params(&preset.params, !preset_changed);
                        paint.sync_stroke_method(preset.stroke_method);
                        if preset_changed {
                            paint.set_brush_size(preset.size);
                        }
                    }
                }
                // g2-properties-context (the shell half): the rail
                // decides which contexts apply to the target, the bodies
                // below are per kind. The sync above runs EVERY frame —
                // the viewport strokes while another context's body
                // shows, so its session sync may never gate on the
                // rail's kind.
                let (crumb_set_name, mask_focus) = match self.paint.as_deref() {
                    Some(paint) => {
                        let set = paint.active_set().to_string();
                        let mask = paint.mask_edit_layer();
                        ((!set.is_empty()).then_some(set), mask)
                    }
                    None => (None, None),
                };
                let mask_name = mask_focus.and_then(|id| {
                    self.doc
                        .layers()
                        .iter()
                        .find(|l| l.id == id)
                        .map(|l| l.name.clone())
                });
                // Owned snapshot: the rail call needs `&mut self.doc` and
                // friends in its bodies — the borrow cannot stay held.
                let target = self
                    .layer_gestures
                    .selection
                    .primary()
                    .and_then(|id| self.doc.layers().iter().find(|l| l.id == id))
                    .map(|l| (l.id, l.kind.clone(), l.mask.is_some(), l.name.clone()));
                let kinds = properties_kinds(
                    target.as_ref().map(|(_, kind, _, _)| kind.clone()),
                    target.as_ref().is_some_and(|(_, _, has_mask, _)| *has_mask),
                    mask_focus.is_some(),
                );
                // The rail keeps its own active kind (radio); a fallback
                // switch reads back through `rail.active()` next body.
                let _ = self.properties_rail.set_applicable(&kinds);
                let set_name = crumb_set_name.unwrap_or_else(|| tr("ctx-rail.texture-set"));
                let mut crumb = vec![set_name];
                if let Some((_, _, _, name)) = &target {
                    crumb.push(name.clone());
                }
                if let Some(name) = mask_name {
                    crumb.push(name);
                }
                let tips_raw = [
                    tr("ctx-rail.brush"),
                    tr("ctx-rail.layer"),
                    tr("ctx-rail.fill"),
                    tr("ctx-rail.mask"),
                    tr("ctx-rail.texture-set"),
                ];
                let hint = tr("ctx-rail.search-hint");
                let rail_ctx = umber_gui::ContextRailCtx {
                    atlas: self.icons,
                    tooltips: [
                        &tips_raw[0],
                        &tips_raw[1],
                        &tips_raw[2],
                        &tips_raw[3],
                        &tips_raw[4],
                    ],
                    breadcrumb: &crumb,
                    search_hint: &hint,
                };
                // The body reads the kind the rail held BEFORE this
                // frame's clicks: the closure's captures are fixed at
                // construction, and `rail_ui` runs inside the widget
                // after them (a same-frame re-read is a shared-borrow
                // conflict). A click switches the body next frame — the
                // rail's own highlight still lands this frame.
                let kind = self.properties_rail.active();
                let body_kind = kind;
                // The search's vocabulary: the same kind's row labels,
                // re-registered every frame (an unchanged list keeps the
                // current hit; a switch restarts at the new list's first).
                self.properties_rail
                    .set_labels(properties_row_labels(body_kind));
                let target_id = target.as_ref().map(|(id, _, _, _)| *id);
                let (_, ()) = self
                    .properties_rail
                    .ui(ui, &rail_ctx, |ui, _| match body_kind {
                        umber_gui::ContextKind::Brush => {
                            self.brush.show(ui, self.paint.as_deref_mut());
                        }
                        umber_gui::ContextKind::Layer => {
                            if let Some(id) = target_id {
                                document::layer_properties_row(ui, self.doc, id);
                            }
                        }
                        umber_gui::ContextKind::Fill => {
                            // One collapsible row per fill layer (projection
                            // picker + v1 source editor) plus the
                            // polygon-fill tool rows; edits run through
                            // Document as undoable, journaled SetFill ops.
                            fill_layer::fill_section_ui(
                                ui,
                                self.doc,
                                self.mesh,
                                &mut *self.polygon_fill,
                                &self
                                    .mesh_import
                                    .session_set
                                    .channels
                                    .iter()
                                    .map(|c| c.name.clone())
                                    .collect::<Vec<String>>(),
                            );
                            // The paint-path tool rows (editable polyline →
                            // replayed stroke / closed-path fill through the
                            // paint session).
                            path_tool::path_tool_rows(
                                ui,
                                self.paint.as_deref_mut(),
                                &mut *self.path_tool,
                            );
                        }
                        umber_gui::ContextKind::Mask => {
                            if let Some(paint) = self.paint.as_deref_mut() {
                                document::layer_mask_ui(ui, self.doc, paint);
                            }
                        }
                        umber_gui::ContextKind::TextureSet => {
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
                                Some(&mut *self.stencil),
                                self.mesh_import.session_set.name.as_str(),
                                &channels,
                                self.pending_resize,
                            );
                            // The document follows the paint session's
                            // active set (C2-9): the Layers panel, undo and
                            // History then act on that set's own stack.
                            if let Some(paint) = self.paint.as_deref() {
                                let active = paint.active_set().to_string();
                                // (The first activation adopts the current
                                // stack: see `Document::activate_set`.)
                                if !active.is_empty() && self.doc.active_set() != active {
                                    self.doc.activate_set(&active);
                                    // Its channel list follows (C2-9
                                    // stage 4).
                                    self.mesh_import
                                        .switch_set(&active, self.parked_texture_sets);
                                }
                            }
                        }
                    });
                // (The outcome's flags are consumed states: switched is
                // the rail's own next-frame active, the search flags are
                // informational — the row-label registry is the named
                // follow-up, see the module docs.)
            }
            Panel::Assets => {
                // R-C1's library browser: the searchable face over the
                // three existing libraries (the brief's discoverability
                // layer). The import sections stay below.
                let selected = self.layer_gestures.selection.primary();
                let outcome = assets_panel::library_ui(ui, self.assets, self.brush, selected);
                if let Some(name) = outcome.brush {
                    if !self.brush.select_preset_by_name(&name) {
                        log::warn!("assets: brush '{name}' left the shelf (a stale row)");
                    }
                }
                if let Some(material) = &outcome.material {
                    // The shelf's own three-step: apply, bind the
                    // generators to the newest bakes, select the top.
                    let ids = document::apply_smart_material(self.doc, material);
                    let missing =
                        document::bind_smart_generators(self.doc, material, &ids, |name| {
                            let path = self.bakes.latest_map(name)?;
                            match umber_export::png::read_png_rgba8(path) {
                                Ok(image) => Some(image),
                                Err(e) => {
                                    log::warn!("smart material: {}: {e}", path.display());
                                    None
                                }
                            }
                        });
                    if !missing.is_empty() {
                        log::info!(
                            "smart material '{}': bake {} in the Bakes panel, then apply again to bind them",
                            material.name,
                            missing.join(", ")
                        );
                    }
                    if let Some(top) = ids.last() {
                        let target = paint_state::PlaneOwner::Layer(*top);
                        if let Some(paint) = self.paint.as_deref_mut() {
                            paint.set_active_target(target);
                        }
                        sync_gesture_selection(&mut *self.layer_gestures, target);
                    }
                }
                if let Some((target, mask)) = &outcome.mask {
                    // The Layers panel's own mask path: the journal binds
                    // the mask to the layer by name, THEN the shared compose
                    // flow publishes the pixels (the extraction both arms
                    // call).
                    self.doc.run(umber_core::layers::LayerCommand::attach_mask(
                        *target,
                        mask.name.clone(),
                    ));
                    if let Some(paint) = self.paint.as_deref_mut() {
                        compose_mask_flow(paint, self.doc, self.bakes, *target, mask);
                    }
                }
                ui.separator();
                self.mesh_import.show(
                    ui,
                    self.doc,
                    self.store,
                    mesh_map_import::read_pending_from_disk,
                );
                ui.separator();
                psd_import::show_section(ui, self.psd_import, self.doc);
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
                    Some(&mut *self.stencil),
                    self.mesh_import.session_set.name.as_str(),
                    &channels,
                    self.pending_resize,
                );
                // The document follows the paint session's active set
                // (C2-9): the Layers panel, undo and History then act on
                // that set's own stack.
                if let Some(paint) = self.paint.as_deref() {
                    let active = paint.active_set().to_string();
                    // (The first activation adopts the current stack: see
                    // `Document::activate_set`.)
                    if !active.is_empty() && self.doc.active_set() != active {
                        self.doc.activate_set(&active);
                        // Its channel list follows (C2-9 stage 4).
                        self.mesh_import
                            .switch_set(&active, self.parked_texture_sets);
                    }
                }
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
                    baked_normal: self.bakes.baked_normal(),
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
                    // The stencil section: loads, toggles, Resets,
                    // re-stages the label bytes, and hands the RAW gate
                    // to umber-paint's hook (`stencil`).
                    stencil::panel_section(ui, self.stencil, self.paint.as_deref_mut());
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
    /// Layer thumbnails, content-keyed (g2-layer-thumbnails).
    pub thumbnails: thumbnails::Thumbnails,
    /// The Layers panel's gesture state (g2-layer-gestures): the
    /// multi-selection, the mask solo and the pending ✕ confirm. Session
    /// state keyed by layer id, so it dies with its document.
    pub layer_gestures: layers_panel::gestures::GestureState,
    /// A texture-set resize armed by the size combo (confirm first).
    pub pending_resize: Option<PendingResize>,
    /// Dormant fill/material layers (§6.3 [B2, M11]): their pre-v2 result
    /// lives in the Base paint, so they seed CLEAN and never re-evaluate
    /// until an explicit edit. Survives save/reopen via `# dormant <id>`
    /// manifest lines while untouched.
    pub dormant: std::collections::HashSet<u64>,
    /// The last frame's composite snapshot (the retire census + `Full`
    /// diff baseline, §4.2/§5.4).
    pub prev_snapshot: Option<paint_state::CompositeSnapshot>,
    /// The Assets panel's library-browser state (R-C1: the search
    /// field; the lists cache in egui memory, the smart shelf's own
    /// precedent).
    pub assets_panel: assets_panel::AssetsPanel,
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
    /// g7-polish-batch: the held-key temp-tool state (hold E = erase
    /// while held, restore on release; Space = pan hold the viewport
    /// will read). Fed by [`toolbar::poll_temp_keys`] every frame.
    pub temp_tools: toolbar::TempTools,
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
    /// The session stencil (stencil-engine design step 2): image,
    /// transform, gating flag, overlay opacity and the staged gate.
    /// Session state, never serialized.
    pub stencil: stencil::StencilState,
    /// Mesh-map import by naming convention (§7 P1 row): the folder-scan
    /// routing + unmatched report + session texture set (channels ensured
    /// on apply). Owns the session set so imported user channels survive
    /// until the next save (see `mesh_map_import`).
    pub mesh_import: mesh_map_import::MeshMapImportState,
    /// Every inactive texture set's channel list (C2-9 stage 4): the
    /// import session holds the active set; these wait for their switch.
    pub parked_texture_sets: std::collections::BTreeMap<String, umber_core::TextureSet>,
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
        // A resize armed against the old document must not fire on the new.
        self.pending_resize = None;
        self.mesh_import = mesh_map_import::MeshMapImportState::default();
        self.parked_texture_sets.clear();
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
        // Layer ids restart per document: a selection or solo must not
        // alias document B's layer that reuses an id.
        self.layer_gestures = layers_panel::gestures::GestureState::default();
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
    // C2-32: an ordinary colour PNG (e.g. `Body_baseColor.png`) stores
    // sRGB-encoded bytes; the planes are LINEAR (the picker and the
    // eyedropper linearize on the way in, export re-curves baseColor and
    // emissive), so publishing the bytes as-is curves them twice on
    // export (washed out) and renders too light in the viewport. Color
    // channels linearize BEFORE the premultiply — data channels (AO,
    // roughness, ...) are unit-range values, not encoded colour, and
    // pass through unchanged. `curvature_png_to_linear` is the shared
    // sRGB→linear curve (RGB only, alpha/coverage untouched).
    let mut rgba = rgba;
    if crate::mesh_map_import::channel_kind_for(&applied.channel) == umber_core::ChannelKind::Color
    {
        umber_paint::paint_state::curvature_png_to_linear(&mut rgba);
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

/// The W-GUI-3 PSD binding: drains one queued [`psd_import::PsdPublish`]
/// — publishes the flattened import into the layer JUST created for it
/// (`apply_session_set`'s `doc.run(LayerCommand::add(..))`, the same
/// user step — undo removes layer + pixels together). The flat's OWN
/// w/h are passed (never recomputed — `publish_plane` drops on
/// mismatch, which the size gate here turns into the row badge so the
/// layer is left empty loudly, exactly like the mesh-map drain).
///
/// The curve: `publish_psd_pixels` linearizes (Photoshop gamma-space →
/// the linear planes, the C2-32 rule) and premultiplies, so the drain
/// carries no extra curve. Badges reuse the mesh-map import row keys —
/// the layer row treats both imports the same.
fn publish_psd_plane(
    paint: &mut paint_state::PaintState,
    badges: &mut std::collections::HashMap<u64, String>,
    applied: &psd_import::PsdPublish,
) {
    use paint_state::{PaintRefusal, PlaneOwner};
    let id = applied.layer_id;
    let expected =
        paint.tile_channel_size(psd_import::PSD_PUBLISH_TILE, psd_import::PSD_TARGET_CHANNEL);
    if (applied.flat.width, applied.flat.height) != expected {
        let (ew, eh) = expected;
        log::warn!(
            "psd import: layer {id} flat is {}x{}, want {ew}x{eh}; leaving the layer empty",
            applied.flat.width,
            applied.flat.height
        );
        badges.insert(id, crate::i18n::tr("import.size-mismatch"));
        return;
    }
    let rgba = psd_import::publish_psd_pixels(&applied.flat);
    match paint.publish_plane(
        PlaneOwner::Layer(id),
        psd_import::PSD_PUBLISH_TILE,
        psd_import::PSD_TARGET_CHANNEL,
        applied.flat.width,
        applied.flat.height,
        rgba,
    ) {
        Ok(()) => {
            badges.remove(&id);
        }
        Err(PaintRefusal::PlaneBudget { bytes }) => {
            log::warn!("psd import: layer {id} plane budget refused ({bytes} bytes)");
            badges.insert(
                id,
                crate::i18n::tr_args(
                    "import.budget-refused",
                    [("bytes", fluent::FluentValue::from(bytes as i64))],
                ),
            );
        }
        Err(other) => {
            log::warn!("psd import: layer {id} publish refused ({other:?})");
            badges.insert(id, crate::i18n::tr("import.decode-failed"));
        }
    }
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

    /// The IO's OWN set's tiles (C2-9): with several sets, a material
    /// layer evaluates only onto its set's tiles, never another set's
    /// (there its planes would be invisible and still cost the budget).
    /// A single-set session's tiles are all of them, as before.
    fn tiles(&self) -> Vec<u16> {
        self.paint.tiles_of_set(&self.set)
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

/// A smart mask's anchor stages, read once per tile: `(layer name,
/// channel)` → the layer plane's `(w, h, rgba8)`, `None` when absent.
type AnchorReads = std::collections::HashMap<(String, String), Option<(u32, u32, Vec<u8>)>>;

/// The eframe app.
pub struct UmberApp {
    pub state: AppState,
    /// The toast stack (GUI plan G0 integration): user-facing failures
    /// that used to reach only the log (mesh / environment / project /
    /// export) post here too, so a failed action is never silent.
    notices: umber_gui::Notices,
    /// The command palette (GUI plan G3).
    palette: palette::Palette,
    /// Preferences (GUI plan G3): the values, whether the window is open,
    /// and its search query.
    prefs: preferences::Preferences,
    /// File ▸ Recent (GUI plan g5-menu-completion), persisted beside the
    /// locale preference.
    recent: recent::RecentProjects,
    prefs_open: bool,
    prefs_query: String,
    /// The Preferences ▸ Keymap page's capture / search / paste state
    /// (GUI plan g3-keymap-editor).
    keymap_editor: preferences::KeymapEditor,
    /// The Quick Setup window (g5-first-run): opens on the very first
    /// launch (no preferences file yet) and from Help.
    quick_setup: first_run::QuickSetup,
    /// Which landed Curvature bakes the paint session already loaded (the
    /// cavity mask's input).
    curvature_feed: CurvatureFeed,
    /// The style was installed on both OS-theme branches (once, first
    /// frame: `umber_gui::theme::apply`).
    style_installed: bool,
    dock: DockState<Panel>,
    /// Each dock leaf's active panel at the last scroll-into-view pass
    /// (g5-tab-overflow): a change scrolls that leaf's strip once.
    tab_active_seen: std::collections::HashMap<NodePath, Panel>,
    gpu: GpuContext,
    /// File > New with unsaved work arms this; the confirm window
    /// (drawn in `ui` beside the Perf HUD) offers Save / Discard /
    /// Cancel. `None` = no confirmation pending. `Some(true)` = a
    /// discard was confirmed; `Some(false)` = the user asked to save
    /// first (the save dialog runs, then the reset).
    pending_new: Option<bool>,
    /// The quit guard (C2-1): the window's close was requested with
    /// unsaved edits. While set, the close is cancelled each frame and the
    /// Save / Discard / Cancel window shows.
    pending_quit: bool,
    /// The user chose to quit (Discard, or a successful Save): the next
    /// close request goes through.
    quit_confirmed: bool,
    /// The pixel autosave's timer (C2-3).
    autosave: recovery::AutosaveClock,
    /// A snapshot the previous run left, offered once at launch (C2-3).
    pending_recovery: Option<recovery::RecoveryMeta>,
    /// File ▸ Revert is asking to discard the unsaved edits.
    pending_revert: bool,
    /// Help ▸ About is open.
    about_open: bool,
    /// Help ▸ Keyboard shortcuts is open (G5 menu completion).
    shortcuts_open: bool,
    /// Hide UI / maximize (GUI plan g5-focus-modes).
    focus: shell_menu::FocusState,
    /// Named layouts (GUI plan g5-workspaces).
    workspaces: workspaces::Workspaces,
    /// A bare Tab arrived with nothing focused: `raw_input_hook` took it
    /// out of egui's input (where it would move keyboard focus) and the
    /// frame turns it into Hide UI.
    tab_hide_pending: bool,
    /// The device display LUT (created at startup on eframe's device;
    /// rewritten in place when `state.display_lut` is dirty). Handed to
    /// the viewport's mesh pass and the UV view's paint display.
    display_lut: umber_gpu::DisplayLut,
    brush_panel: BrushPanel,
    /// The Properties context rail (g2-properties-context, shell half):
    /// the active kind, the applicable per-target set, the property-row
    /// search. Session state (menu switching survives across panels).
    properties_rail: umber_gui::ContextRail,
    /// Whether the axis gizmo draws (View menu; GUI plan G1
    /// `g1-axis-gizmo`). Shell state synced into the viewport each
    /// frame, like the view toggles. On by default.
    show_gizmo: bool,
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

        // Layout: the last session's workspaces (`<state>/layouts.json`),
        // else the boot shape (`shell_menu::default_dock`, which Window ▸
        // Reset Layout rebuilds).
        let (workspaces, dock) = workspaces::load_layouts(&layouts_path())
            .map(workspaces::Workspaces::restore)
            .unwrap_or_else(|| {
                (
                    workspaces::Workspaces::default(),
                    shell_menu::default_dock(),
                )
            });

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
            tab_active_seen: std::collections::HashMap::new(),
            gpu,
            display_lut,
            brush_panel: BrushPanel::new(),
            properties_rail: umber_gui::ContextRail::new(
                "properties-rail",
                umber_gui::ContextKind::Brush,
            ),
            show_gizmo: true,
            #[cfg(feature = "perf")]
            show_perf_hud: false,
            notices: pending_crash_notices(),
            palette: palette::Palette::default(),
            prefs: {
                // The saved keymap overrides become the effective table
                // before the first frame polls a key (g3-keymap-editor).
                let prefs = preferences::Preferences::load(preferences::prefs_path().as_deref());
                actions::set_overrides(&prefs.keymap_overrides);
                prefs
            },
            recent: recent::recent_path()
                .map(|p| recent::RecentProjects::load(&p))
                .unwrap_or_default(),
            prefs_open: false,
            prefs_query: String::new(),
            keymap_editor: preferences::KeymapEditor::default(),
            quick_setup: {
                let mut s = first_run::QuickSetup::default();
                if first_run::should_show() {
                    s.open();
                }
                s
            },
            curvature_feed: CurvatureFeed::default(),
            style_installed: false,
            #[cfg(feature = "perf")]
            last_frame_time: None,
            #[cfg(feature = "perf")]
            last_dabs_composited: 0,
            view: toolbar::ViewToggles::default(),
            icons,
            hdr_mode,
            pending_new: None,
            pending_quit: false,
            quit_confirmed: false,
            autosave: recovery::AutosaveClock::default(),
            pending_recovery: recovery::pending(&gpu_startup::state_dir()),
            pending_revert: false,
            about_open: false,
            shortcuts_open: false,
            focus: shell_menu::FocusState::default(),
            workspaces,
            tab_hide_pending: false,
        })
    }
}

/// App-level actions (kept out of the App impl to keep that block small).
impl UmberApp {
    /// The one place a [`toolbar::ShellAction`] takes effect: the toolbar
    /// buttons and the shell shortcuts both land here, so a button and its
    /// key cannot drift (see `toolbar`'s module docs for the key table and
    /// the routing calls).
    /// The Preferences window (when open) and the per-frame style sync.
    /// The sync writes egui only when a value differs (`style_delta`), so
    /// an unchanged preference set costs nothing per frame.
    fn show_preferences(&mut self, ctx: &egui::Context) {
        let before = self.prefs.clone();
        if self.prefs_open {
            let mut open = true;
            egui::Window::new(tr("prefs.title"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(380.0)
                .show(ctx, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.prefs_query)
                            .hint_text(tr("prefs.search")),
                    );
                    ui.separator();
                    let q = self.prefs_query.clone();
                    let p = &mut self.prefs;
                    let interface = tr("prefs.section-interface");
                    let scale = tr("prefs.ui-scale");
                    let auto_scale = tr("prefs.ui-scale-auto");
                    let hit_zone = tr("prefs.splitter-hit-zone");
                    let tips = tr("prefs.tooltips");
                    let motion = tr("prefs.reduce-motion");
                    let hints = tr("prefs.key-hints");
                    let navigation = tr("prefs.section-navigation");
                    let nav = tr("prefs.nav-preset");
                    let any =
                        |labels: &[&str]| labels.iter().any(|l| preferences::row_matches(&q, l));
                    if any(&[
                        &interface,
                        &scale,
                        &auto_scale,
                        &hit_zone,
                        &tips,
                        &motion,
                        &hints,
                    ]) {
                        ui.strong(&interface);
                        if preferences::row_matches(&q, &scale)
                            || preferences::row_matches(&q, &auto_scale)
                            || preferences::row_matches(&q, &interface)
                        {
                            ui.horizontal(|ui| {
                                ui.label(&scale);
                                // g7-density-scale: Auto = native scale; the
                                // manual value survives a toggle back.
                                ui.checkbox(&mut p.ui_scale_auto, auto_scale.as_str());
                                let field = umber_gui::ValueField::new(&mut p.ui_scale)
                                    .context_menu(crate::i18n::prop_menu_labels())
                                    .range(preferences::SCALE_MIN..=preferences::SCALE_MAX)
                                    .step(0.05);
                                let response = ui.add(field);
                                // Writing a number IS leaving Auto (the
                                // pinned contract: the re-pinned
                                // `style_delta_is_empty...` test encodes
                                // it); the written value becomes the zoom
                                // on the next frame's delta.
                                if response.changed() {
                                    p.ui_scale_auto = false;
                                }
                            });
                        }
                        // g7-density-scale: the dock separators' hit zone
                        // (the dock style applies it where it is built).
                        if preferences::row_matches(&q, &hit_zone)
                            || preferences::row_matches(&q, &interface)
                        {
                            let mut tablet =
                                p.splitter_hit_zone == preferences::HitZoneName::Tablet;
                            if ui.checkbox(&mut tablet, hit_zone.as_str()).changed() {
                                p.splitter_hit_zone = if tablet {
                                    preferences::HitZoneName::Tablet
                                } else {
                                    preferences::HitZoneName::Standard
                                };
                            }
                        }
                        for (label, value) in [
                            (&tips, &mut p.tooltips),
                            (&motion, &mut p.reduce_motion),
                            (&hints, &mut p.key_hints),
                        ] {
                            if preferences::row_matches(&q, label)
                                || preferences::row_matches(&q, &interface)
                            {
                                ui.checkbox(value, label.as_str());
                            }
                        }
                        ui.add_space(6.0);
                    }
                    if any(&[&navigation, &nav]) {
                        ui.strong(&navigation);
                        ui.horizontal(|ui| {
                            ui.label(&nav);
                            for (preset, key) in [
                                (navigation::NavPreset::Painter, "prefs.nav-painter"),
                                (navigation::NavPreset::Umber, "prefs.nav-umber"),
                                (navigation::NavPreset::Blender, "prefs.nav-blender"),
                            ] {
                                ui.selectable_value(&mut p.nav, preset, tr(key));
                            }
                        });
                        if std::env::var("UMBER_NAV").is_ok() {
                            ui.label(
                                egui::RichText::new(tr("prefs.nav-env-override"))
                                    .small()
                                    .color(umber_gui::muted()),
                            );
                        }
                        ui.add_space(6.0);
                    }
                    // Keymap (g3-keymap-editor): the page lives in
                    // `preferences::keymap_section`.
                    let is_mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
                    preferences::keymap_section(
                        ui,
                        &mut self.keymap_editor,
                        &mut p.keymap_overrides,
                        &q,
                        is_mac,
                    );
                });
            self.prefs_open = open;
            if !open {
                self.keymap_editor.cancel_capture();
            }
        }
        if self.prefs.keymap_overrides != before.keymap_overrides {
            actions::set_overrides(&self.prefs.keymap_overrides);
        }
        if self.prefs != before {
            self.prefs.save(preferences::prefs_path().as_deref());
            let env = std::env::var("UMBER_NAV").ok();
            self.state
                .viewport
                .set_nav_preset(self.prefs.effective_nav(env.as_deref()));
        }
        // The style sync: only what differs is written (photocraft #10).
        let style = ctx.global_style();
        let delta = preferences::style_delta(
            &self.prefs,
            ctx.zoom_factor(),
            style.interaction.tooltip_delay,
            style.animation_time,
            umber_gui::theme::TOOLTIP_DELAY_SECS,
            umber_gui::MOTION_SECS,
        );
        if delta.is_empty() {
            return;
        }
        if let Some(zoom) = delta.zoom {
            ctx.set_zoom_factor(zoom);
        }
        if delta.tooltip_delay.is_some() || delta.animation_time.is_some() {
            ctx.all_styles_mut(|s| {
                if let Some(t) = delta.tooltip_delay {
                    s.interaction.tooltip_delay = t;
                }
                if let Some(a) = delta.animation_time {
                    s.animation_time = a;
                }
            });
        }
    }

    /// Posts a sticky error toast: the translated `key` headline and the
    /// underlying error's text (errors stay until dismissed — a failed
    /// save must not vanish while the user looks away).
    fn notify_error(&mut self, key: &str, detail: &str) {
        let text = format!("{}\n{detail}", tr(key));
        self.notices
            .post_sticky(umber_gui::NoticeLevel::Error, text);
    }

    /// The registry's view of the shell this frame (`actions::availability`).
    fn shell_state(&self) -> actions::ShellState {
        actions::ShellState {
            can_undo: self.state.doc.history.can_undo(),
            can_redo: self.state.doc.history.can_redo(),
            paint_available: self.state.paint.is_some(),
            mask_editing: self
                .state
                .paint
                .as_ref()
                .is_some_and(|p| p.quick_mask().is_editing()),
        }
    }

    fn dispatch(&mut self, action: toolbar::ShellAction) {
        use toolbar::ShellAction;
        // The registry's one enabled-check (GUI plan G0): a shortcut on an
        // action that cannot run says why instead of doing nothing
        // (photocraft's disabled-reason rule). Menu rows and toolbar
        // buttons are already greyed, so only keys reach this arm.
        if let Err(reason) = actions::availability(action, self.shell_state()) {
            self.notices.post(umber_gui::NoticeLevel::Info, tr(reason));
            return;
        }
        if self.view.apply(action) {
            return;
        }
        match action {
            ShellAction::NewProject => self.new_project(),
            ShellAction::OpenMesh => self.open_mesh(),
            ShellAction::OpenProject => self.open_project(),
            ShellAction::SaveProject => self.save_project(),
            ShellAction::SaveProjectAs => self.save_project_as(),
            // One timeline (C2-7): Ctrl+Z steps whichever journal holds the
            // newest applied edit, a layer op or a stroke.
            ShellAction::Undo => {
                let _ = history_panel::unified_undo(&mut self.state.doc, self.state.paint.as_mut());
            }
            ShellAction::Redo => {
                let _ = history_panel::unified_redo(&mut self.state.doc, self.state.paint.as_mut());
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
            // `[` / `]` (Painter's size steps): one notch of the shared
            // multiplicative step, so ten presses span ~2.6x.
            ShellAction::CommandPalette => self.palette.open(),
            ShellAction::Preferences => self.prefs_open = true,
            ShellAction::ToggleHideUi => {
                self.focus.toggle(
                    shell_menu::FocusMode::HideUi,
                    &mut self.dock,
                    Panel::Viewport,
                );
            }
            ShellAction::ViewMaterial
            | ShellAction::NextChannel
            | ShellAction::PrevChannel
            | ShellAction::NextMeshMap
            | ShellAction::PrevMeshMap => {
                let mode = self.view.view_mode;
                self.view.view_mode = match action {
                    ShellAction::NextChannel => display_keys::cycle_channel(mode, true),
                    ShellAction::PrevChannel => display_keys::cycle_channel(mode, false),
                    ShellAction::NextMeshMap => display_keys::cycle_mesh_map(mode, true),
                    ShellAction::PrevMeshMap => display_keys::cycle_mesh_map(mode, false),
                    _ => umber_gpu::ViewMode::Lit,
                };
                // The soloed name, so the user sees what the viewport shows.
                self.notices.post(
                    umber_gui::NoticeLevel::Info,
                    display_keys::mode_name(self.view.view_mode, tr),
                );
            }
            ShellAction::NextWorkspace | ShellAction::PrevWorkspace => {
                let to = self
                    .workspaces
                    .active()
                    .cycle(action == ShellAction::NextWorkspace);
                self.switch_workspace(to);
            }
            ShellAction::ToggleMaximize => {
                let keep = shell_menu::maximize_target(&mut self.dock);
                self.focus
                    .toggle(shell_menu::FocusMode::Maximized, &mut self.dock, keep);
            }
            ShellAction::BrushSmaller | ShellAction::BrushLarger => {
                if let Some(paint) = self.state.paint.as_mut() {
                    let grow = action == ShellAction::BrushLarger;
                    let next = brush_cursor::step_size(paint.brush_size(), grow);
                    paint.set_brush_size(next);
                }
            }
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
                    Err(e) => {
                        log::error!("mesh upload failed: {e:#}");
                        self.notify_error("notice.mesh-upload-failed", &format!("{e:#}"));
                    }
                }
            }
            Err(e) => {
                log::error!("mesh load failed: {e}");
                self.notify_error("notice.mesh-load-failed", &e.to_string());
            }
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
        // Startup is also where the navigation preset is read
        // (`UMBER_NAV`, until the G3 preferences window owns it).
        let env = std::env::var("UMBER_NAV").ok();
        self.state
            .viewport
            .set_nav_preset(self.prefs.effective_nav(env.as_deref()));
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
            Err(err) => {
                log::error!("environment load failed: {err:#}");
                self.notify_error("notice.environment-failed", &format!("{err:#}"));
            }
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
                    Err(e) => {
                        log::error!("png write failed: {e:#}");
                        self.notify_error("notice.export-failed", &format!("{e:#}"));
                    }
                }
            }
            Err(e) => {
                log::error!("readback failed: {e:#}");
                self.notify_error("notice.export-failed", &format!("{e:#}"));
            }
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

/// The Properties rail's per-target applicable kinds (g2-properties-context,
/// the shell dispatch's decision half — pure, so the kittest pins it):
/// - every target with a selected layer shows Layer + TextureSet; a fill
///   layer adds Fill,
/// - a mask target (the layer owns a mask, or the paint session is
///   editing one) adds Mask,
/// - the Brush body applies whenever the panel exists (the strokes'
///   session sync runs per frame regardless of the rail's kind).
/// - hidden kinds are ABSENT from the rail (the widget's
///   `set_applicable` contract), never greyed.
///
/// No selection (or a stale id, e.g. after a set switch): Brush +
/// TextureSet only.
fn properties_kinds(
    target: Option<umber_core::LayerKind>,
    has_mask: bool,
    mask_focus: bool,
) -> Vec<umber_gui::ContextKind> {
    use umber_gui::ContextKind;
    let mut kinds = vec![ContextKind::Brush];
    if target.is_some() {
        kinds.push(ContextKind::Layer);
    }
    if target == Some(umber_core::LayerKind::Fill) {
        kinds.push(ContextKind::Fill);
    }
    if has_mask || mask_focus {
        kinds.push(ContextKind::Mask);
    }
    kinds.push(ContextKind::TextureSet);
    kinds
}

/// The row labels the Properties rail's search runs over for `kind`: the
/// label fn sitting beside the body the shell draws for that kind
/// (`rail-row-labels`, the W-GUI-3 shell-halves follow-up). Pure, so a
/// unit test pins the kind → list pairing; the arm registers the result
/// before `rail.ui` every frame (`ContextRail::set_labels` keeps the
/// current hit while the list is unchanged). Fill is the fill section's
/// rows then the path tool's, the order the Fill body draws them.
fn properties_row_labels(kind: umber_gui::ContextKind) -> Vec<String> {
    use umber_gui::ContextKind;
    match kind {
        ContextKind::Brush => brush_panel::row_labels(),
        ContextKind::Layer => document::layer_properties_row_labels(),
        ContextKind::Fill => {
            let mut labels = fill_layer::row_labels();
            labels.extend(path_tool::row_labels());
            labels
        }
        ContextKind::Mask => document::layer_mask_row_labels(),
        ContextKind::TextureSet => texture_sets_row_labels(),
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
    stencil: Option<&mut stencil::StencilState>,
    session_set: &str,
    channels: &[String],
    pending: &mut Option<PendingResize>,
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
    // The set-switch glue (W-C4 L1): the switch + Amendment A point 4's
    // guarded re-stage now live in the lib fn — the guard inside, so
    // the headless test exercises it (see `set_switch`'s module docs).
    set_switch::switch_active_set(paint, stencil, &active);
    let (width, height) = paint.size_of_set(&active);
    let mut size = width;
    ui.horizontal(|ui| {
        ui.label(tr("texturesets.size"));
        size_presets::size_combo(ui, "", &mut size);
    });
    // A resize clears the set's canvas and the stroke undo history (Painter
    // B4 warns that a resolution change triggers long recomputation). The
    // combo pick only ARMS it; the change runs on an explicit confirm, so
    // one stray click never wipes a painted set (GUI plan rule 8:
    // destructive = confirm). The confirm also arms when the set has no
    // paint yet: the rule is the same everywhere, and the click is cheap.
    if (size, size) != (width, height) {
        *pending = Some(PendingResize {
            set: active.clone(),
            from: (width, height),
            to: size,
        });
    }
    // The stale-pending clear (the W-C4 L1 extraction): a pending armed
    // for a different set dies with the switch — the glue fn owns the
    // rule now.
    set_switch::reconcile_pending_resize(pending, &active);
    if let Some(p) = pending.clone() {
        {
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new(crate::i18n::tr_args(
                        "texturesets.resize-confirm",
                        [
                            ("set", fluent::FluentValue::from(p.set.as_str())),
                            ("from", fluent::FluentValue::from(p.from.0.to_string())),
                            ("to", fluent::FluentValue::from(p.to.to_string())),
                        ],
                    ))
                    .color(umber_gui::theme::palette::WARN),
                );
                ui.horizontal(|ui| {
                    if ui.button(tr("texturesets.resize-apply")).clicked() {
                        paint.resize_set(&p.set, p.to, p.to);
                        *pending = None;
                    }
                    if ui.button(tr("texturesets.resize-cancel")).clicked() {
                        *pending = None;
                    }
                });
            });
        }
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

/// The catalog keys of [`texture_sets_panel_ui`]'s labelled rows, in draw
/// order (the set picker, the size combo, the per-channel heading): the
/// Properties rail's search vocabulary for the TextureSet context
/// (`rail-row-labels`; the inclusion rule is stated on
/// `brush_panel::ROW_LABEL_KEYS`). The per-channel rows are labelled by
/// channel name (data, not a catalog key), and the resize confirm, the
/// hints and the no-paint line are skipped. Static: the per-channel
/// heading stays listed when no channel row is drawn (the Brush list's
/// v1 boundary).
const TEXTURE_SET_LABEL_KEYS: &[&str] = &[
    "texturesets.set",
    "texturesets.size",
    "texturesets.per-channel",
];

/// The TextureSet body's row labels, localized. Pure: no `Ui`, no paint
/// session.
fn texture_sets_row_labels() -> Vec<String> {
    TEXTURE_SET_LABEL_KEYS.iter().copied().map(tr).collect()
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

    /// The pixel autosave (C2-3): while the document has unsaved edits,
    /// every `recovery::AUTOSAVE_SECS` (never mid-stroke, never twice for
    /// the same edits) write a full snapshot into the state dir's
    /// recovery folder. The user's project dir, recents and journal are
    /// untouched (`write_project_files`).
    fn autosave_tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        // Every edit adds a history row (layer op or stroke), so the row
        // count is the edit generation.
        let generation = self.state.doc.session_history.len() as u64;
        let stroking = self.state.paint.as_ref().is_some_and(|p| p.is_stroking());
        if !self
            .autosave
            .due(now, generation, self.state.doc.is_dirty(), stroking)
        {
            return;
        }
        self.autosave.snapshotted(now, generation);
        let state = gpu_startup::state_dir();
        let dir = recovery::snapshot_dir(&state);
        match self.write_project_files(&dir) {
            Ok(_) => {
                let meta = recovery::RecoveryMeta {
                    source_project: self.state.project_dir.clone(),
                    mesh: self.state.mesh_path.clone(),
                    written_unix: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs()),
                };
                if let Err(e) = recovery::write_meta(&state, &meta) {
                    log::warn!("autosave meta not written: {e}");
                }
                log::info!("autosave snapshot: {}", dir.display());
            }
            Err(e) => log::warn!("autosave snapshot failed: {e}"),
        }
    }

    /// The launch-time offer (C2-3): a snapshot a previous run left (a
    /// crash, a kill) asks Recover / Discard once.
    fn recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(meta) = self.pending_recovery.clone() else {
            return;
        };
        let mut choice = None;
        egui::Window::new(tr("recovery.title"))
            .id(egui::Id::new("recovery-prompt"))
            .collapsible(false)
            .resizable(false)
            .min_width(320.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(tr("recovery.body"));
                if let Some(p) = &meta.source_project {
                    ui.weak(p.display().to_string());
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(tr("recovery.recover")).clicked() {
                        choice = Some(true);
                    }
                    if ui.button(tr("shell.new-confirm-discard")).clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                self.pending_recovery = None;
                self.recover(&meta);
            }
            Some(false) => {
                self.pending_recovery = None;
                recovery::discard(&gpu_startup::state_dir());
            }
            None => {}
        }
    }

    /// Opens the snapshot as the document: the mesh first (so the paint
    /// lands on it), then the snapshot project. The session then forgets
    /// the snapshot's directory: the project dir becomes the ORIGINAL
    /// project (when there was one) or none, so Ctrl+S never writes into
    /// the state dir, and the recovered work stays dirty until saved.
    fn recover(&mut self, meta: &recovery::RecoveryMeta) {
        if let Some(mesh) = meta.mesh.clone().filter(|p| p.is_file()) {
            self.load_mesh_path(mesh);
        }
        let snapshot = recovery::snapshot_dir(&gpu_startup::state_dir());
        self.open_project_at(snapshot.clone());
        if self.state.project_dir.as_deref() != Some(snapshot.as_path()) {
            // The open failed (logged by open_project_at): keep the
            // snapshot on disk for another try.
            return;
        }
        self.state.doc.detach_journal();
        self.state.project_dir = meta.source_project.clone().filter(|p| p.is_dir());
        if let Some(dir) = self.state.project_dir.clone() {
            let set = self.project_texture_set_name();
            let seq = self.state.doc.journal_seq();
            if let Err(e) = self.state.doc.attach_journal(&dir, &set, seq) {
                log::warn!("autosave journal unavailable: {e}");
            }
        }
        // The recovered state is not on disk anywhere the user chose.
        self.state.doc.dirty = true;
        self.recent.remove(&snapshot);
        self.save_recent();
    }

    /// Closes the window for real: marks the quit confirmed (so the next
    /// close request is not intercepted again) and asks the viewport to
    /// close.
    fn quit_now(&mut self, ctx: &egui::Context) {
        self.pending_quit = false;
        self.quit_confirmed = true;
        // A deliberate quit (Discard, or after a save): no crash to recover.
        recovery::discard(&gpu_startup::state_dir());
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
    /// Save (C2-4, Ctrl+S): re-saves SILENTLY to the open project's
    /// directory once it has one (the atomic manifest commit makes the
    /// in-place overwrite crash-safe); a never-saved document asks for a
    /// folder, which is Save As.
    fn save_project(&mut self) {
        match save_target(self.state.project_dir.as_deref()) {
            SaveTarget::InPlace(dir) => self.save_project_to(dir),
            SaveTarget::Ask => self.save_project_as(),
        }
    }

    /// Save As (Ctrl+Shift+S): always asks for a new directory.
    fn save_project_as(&mut self) {
        let set_name = self.project_texture_set_name();
        let Some(dir) = rfd::FileDialog::new()
            .set_file_name(format!("{set_name}.umber"))
            .save_file()
        else {
            return;
        };
        self.save_project_to(dir);
    }

    /// Writes the project (metadata + the paint sidecar) to `dir` and
    /// returns the stamped journal mark, WITHOUT touching the session's
    /// project dir, recents or journal: the shared core of a user save and
    /// an autosave snapshot (C2-3).
    fn write_project_files(&mut self, dir: &std::path::Path) -> Result<u64, String> {
        let set_name = self.project_texture_set_name();
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
        // Every set's ids (C2-9): the sidecar's orphan filter must keep the
        // parked sets' planes, or a multi-set save would prune them.
        let live: std::collections::HashSet<u64> = self
            .state
            .doc
            .all_sets()
            .iter()
            .flat_map(|(_, stack)| stack.layers.iter().map(|layer| layer.id))
            .chain(self.state.doc.stack.layers.iter().map(|layer| layer.id))
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
        // Every set (C2-9): the active one is the import session's set, the
        // parked ones their own saved channel lists (stage 4). A single-set
        // document writes exactly what it did before.
        let mut texture_sets = vec![texture_set];
        let mut layer_sets = vec![umber_core::project::TextureSetLayers {
            texture_set: set_name.clone(),
            stack: self.state.doc.stack.clone(),
        }];
        for (name, stack) in self.state.doc.all_sets() {
            if name == set_name || name == self.state.doc.active_set() {
                continue;
            }
            texture_sets.push(
                self.state
                    .parked_texture_sets
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| umber_core::TextureSet::new_default(name)),
            );
            layer_sets.push(umber_core::project::TextureSetLayers {
                texture_set: name.to_string(),
                stack: stack.clone(),
            });
        }
        let mut model = umber_core::project::ProjectModel::new(
            texture_sets,
            layer_sets,
            umber_core::project::ProjectSettings {
                active_texture_set: Some(set_name.clone()),
                display: self.state.display.clone(),
                postfx: postfx_panel::carry_from_chain(&self.state.postfx),
                symmetry: symmetry_carry_from(&self.brush_panel.symmetry),
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
        match umber_core::project::save_to_dir(&model, dir) {
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
                    let tiles = save_paint_tiles(paint, dir, &live, &dormant);
                    log::info!("paint saved: {tiles} tile(s)");
                }
                Ok(model.journal_seq)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// The save itself, to `dir`: write, then make `dir` the session's
    /// project (recents, journal) and drop any recovery snapshot (C2-3).
    fn save_project_to(&mut self, dir: PathBuf) {
        let set_name = self.project_texture_set_name();
        match self.write_project_files(&dir) {
            Ok(journal_seq) => {
                // The import Apply path (and future drops) now have a
                // content-hash store to write into.
                self.state.project_dir = Some(dir.clone());
                self.note_recent(&dir);
                // From here on the document journals into `dir`, its seq
                // continuing past the stamped mark (a Save As clears the
                // old directory's journal — `dir` now holds those edits).
                if let Err(e) = self.state.doc.attach_journal(&dir, &set_name, journal_seq) {
                    log::warn!("autosave journal unavailable: {e}");
                }
                // On disk now: the crash snapshot is redundant.
                recovery::discard(&gpu_startup::state_dir());
            }
            Err(e) => {
                log::error!("project save failed: {e}");
                self.notify_error("notice.save-failed", &e.to_string());
            }
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
    /// File ▸ Recent: the submenu of recent project dirs, most recent
    /// first (missing ones greyed with the reason), plus Clear. Returns
    /// the dir to open. With an empty list the entry draws disabled.
    fn recent_menu(&mut self, ui: &mut Ui) -> Option<PathBuf> {
        if self.recent.paths().is_empty() {
            ui.add_enabled(false, egui::Button::new(tr("menu.recent")))
                .on_disabled_hover_text(tr("menu.recent-empty"));
            return None;
        }
        let mut open = None;
        let mut clear = false;
        ui.menu_button(tr("menu.recent"), |ui| {
            for (path, exists) in self.recent.existing(std::path::Path::exists) {
                let row = ui
                    .add_enabled(exists, egui::Button::new(recent::display_name(path)))
                    .on_hover_text(path.display().to_string())
                    .on_disabled_hover_text(tr("menu.recent-missing"));
                if row.clicked() {
                    open = Some(path.to_path_buf());
                }
            }
            ui.separator();
            if ui.button(tr("menu.recent-clear")).clicked() {
                clear = true;
            }
        });
        if clear {
            self.recent.clear();
            self.save_recent();
            ui.close();
        }
        open
    }

    /// Switches the dock to workspace `to`, leaving any focus mode first so
    /// the outgoing workspace stores its real layout.
    fn switch_workspace(&mut self, to: workspaces::Workspace) {
        self.focus.restore(&mut self.dock);
        if self.workspaces.switch(to, &mut self.dock) {
            self.save_layouts();
        }
    }

    /// Persists every workspace's layout (exit + workspace switch). Saved
    /// out of any focus mode, so the one-tab stand-in is never what
    /// reopens. A failure only logs.
    fn save_layouts(&mut self) {
        let saved = self.workspaces.snapshot(self.focus.real_layout(&self.dock));
        if let Err(e) = workspaces::save_layouts(&layouts_path(), &saved) {
            log::warn!("layouts not saved: {e}");
        }
    }

    fn open_project(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        self.open_project_at(dir);
    }

    /// Records `dir` as the most recent project and persists the list (a
    /// write failure only logs: the list is a convenience).
    fn note_recent(&mut self, dir: &std::path::Path) {
        if self.recent.note(dir) {
            self.save_recent();
        }
    }

    fn save_recent(&self) {
        if let Some(path) = recent::recent_path() {
            if let Err(e) = self.recent.save(&path) {
                log::warn!("recent projects not saved: {e}");
            }
        }
    }

    /// [`Self::open_project`]'s body for a known dir (the dialog, or a
    /// File ▸ Recent row).
    fn open_project_at(&mut self, dir: PathBuf) {
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
                // Every set's stack (C2-9): `name` live, the rest parked.
                let sets: Vec<(String, umber_core::LayerStack)> = model
                    .layers
                    .iter()
                    .map(|l| (l.texture_set.clone(), l.stack.clone()))
                    .collect();
                // Each parked set's channel list (C2-9 stage 4).
                self.state.parked_texture_sets = model
                    .texture_sets
                    .iter()
                    .filter(|ts| ts.name != name)
                    .map(|ts| (ts.name.clone(), ts.clone()))
                    .collect();
                if sets.len() > 1 {
                    self.state.doc.load_sets(sets, &name);
                } else {
                    self.state.doc.load_stack(entry.stack.clone());
                }
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
                self.note_recent(&dir);
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
                // The carried symmetry (the mirror + radial settings
                // adopt into the brush panel; the paint session syncs
                // on the next stroke through its own adoption path).
                self.brush_panel.symmetry = symmetry_from_carry(&model.settings.symmetry);
                // The painted pixels (or a clear to transparent when the
                // project predates the sidecar — never the previous
                // document's paint). The session lifecycle (§6.3 [M10])
                // clears every layer-owned plane BEFORE the sidecar load:
                // layer ids restart at 0 in every LayerStack, so pixel
                // state keyed by id must not outlive its document.
                // Every loaded set's ids (C2-9): planes of parked sets load too.
                let live: std::collections::HashSet<u64> = self
                    .state
                    .doc
                    .all_sets()
                    .iter()
                    .flat_map(|(_, stack)| stack.layers.iter().map(|layer| layer.id))
                    .chain(self.state.doc.stack.layers.iter().map(|layer| layer.id))
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
            Err(e) => {
                log::error!("project load failed: {e}");
                self.notify_error("notice.open-failed", &e.to_string());
                // A dir that exists but no longer loads (not a project any
                // more, corrupt) leaves File ▸ Recent: it would fail again.
                if self.recent.remove(&dir) {
                    self.save_recent();
                }
            }
        }
    }
}

/// Turns the Bakes panel's record list into "newly landed Curvature maps"
/// exactly once each. A record is new when its `(path, modified time)`
/// was not seen before, so a re-bake to the same path (newer mtime) is
/// loaded again, and an unchanged list loads nothing.
#[derive(Debug, Default)]
struct CurvatureFeed {
    seen: std::collections::BTreeMap<std::path::PathBuf, std::time::SystemTime>,
}

impl CurvatureFeed {
    fn take_new(&mut self, records: &[bakes_panel::BakeRecord]) -> Vec<(u16, std::path::PathBuf)> {
        let mut out = Vec::new();
        for r in records {
            if r.selection != Some(bakes_panel::BakeSelection::Curvature) {
                continue;
            }
            let Ok(mtime) = std::fs::metadata(&r.path).and_then(|m| m.modified()) else {
                continue;
            };
            if self.seen.get(&r.path) != Some(&mtime) {
                self.seen.insert(r.path.clone(), mtime);
                out.push((r.tile, r.path.clone()));
            }
        }
        out
    }
}

/// The workspace layouts file (GUI plan g5-workspaces): the state dir.
fn layouts_path() -> PathBuf {
    gpu_startup::state_dir().join("layouts.json")
}

impl eframe::App for UmberApp {
    /// Last chance to persist the dock: the workspace layouts.
    fn on_exit(&mut self) {
        self.save_layouts();
    }

    /// Bare Tab with nothing focused is Hide UI (Painter's Tab), not
    /// egui's "focus the first widget". Taken out of the input here, before
    /// egui's focus logic runs on it; Tab inside a focused widget still
    /// moves focus as usual.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Preferences ▸ Keymap capturing a binding owns the keyboard: the
        // press is taken before egui and the shortcut poll see it, so it
        // never fires the action it is about to be bound away from.
        if self.prefs_open && self.keymap_editor.intercept(&mut raw_input.events) {
            ctx.request_repaint();
            return;
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        // Only while bare Tab is still Hide UI (a keymap override may have
        // moved it).
        let tab = actions::resolve(egui::Key::Tab, egui::Modifiers::NONE);
        if tab != Some(toolbar::ShellAction::ToggleHideUi) {
            return;
        }
        let before = raw_input.events.len();
        raw_input.events.retain(|e| {
            !matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.is_none()
            )
        });
        if raw_input.events.len() != before {
            self.tab_hide_pending = true;
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        profiling::scope!("frame");
        // Crash-safe GPU startup: the 3rd frame confirms the backend.
        gpu_startup::note_frame();
        // The Umber style on BOTH OS-theme branches, once (GUI plan G0
        // g0-style: an OS light/dark flip can no longer swap egui's stock
        // palette in). Chrome panels still apply their own density.
        if !self.style_installed {
            umber_gui::theme::apply(ui.ctx(), umber_gui::Density::Comfortable, 1.0);
            self.style_installed = true;
        }
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
        // The quit guard (C2-1): a close with unsaved edits is cancelled and
        // asks first, the File ▸ New confirm shape. Every close path
        // (the window's ✕, Alt+F4, the OS) arrives as `close_requested`.
        self.autosave_tick(ui.ctx());
        self.recovery_prompt(ui.ctx());
        let close_requested = ui.ctx().input(|i| i.viewport().close_requested());
        match quit_decision(
            close_requested,
            self.state.doc.is_dirty(),
            self.quit_confirmed,
        ) {
            QuitDecision::Allow => {
                if close_requested && !self.state.doc.is_dirty() {
                    // A clean close: nothing unsaved, nothing to recover.
                    recovery::discard(&gpu_startup::state_dir());
                }
            }
            QuitDecision::AskFirst => {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_quit = true;
            }
        }
        if self.pending_quit {
            let mut choice = None;
            egui::Window::new(tr("shell.new-confirm-title"))
                .id(egui::Id::new("quit-guard"))
                .collapsible(false)
                .resizable(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label(tr("shell.new-confirm-body"));
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(tr("shell.new-confirm-save")).clicked() {
                            choice = Some(QuitChoice::Save);
                        }
                        if ui.button(tr("shell.new-confirm-discard")).clicked() {
                            choice = Some(QuitChoice::Discard);
                        }
                        if ui.button(tr("shell.new-confirm-cancel")).clicked() {
                            choice = Some(QuitChoice::Cancel);
                        }
                    });
                });
            match choice {
                Some(QuitChoice::Save) => {
                    // Ctrl+S semantics: in place if the project has a
                    // directory, else the dialog. Quit only if it saved.
                    self.save_project();
                    if !self.state.doc.is_dirty() {
                        self.quit_now(ui.ctx());
                    }
                }
                Some(QuitChoice::Discard) => self.quit_now(ui.ctx()),
                Some(QuitChoice::Cancel) => self.pending_quit = false,
                None => {}
            }
        }
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
        // File ▸ Revert: reopen the saved project, discarding the edits —
        // through the same confirm-window shape File ▸ New uses.
        if self.pending_revert {
            let mut close = false;
            egui::Window::new(tr("menu.revert-confirm-title"))
                .collapsible(false)
                .resizable(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label(tr("menu.revert-confirm-body"));
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(tr("menu.revert")).clicked() {
                            if let Some(dir) = self.state.project_dir.clone() {
                                self.open_project_at(dir);
                            }
                            close = true;
                        }
                        if ui.button(tr("shell.new-confirm-cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.pending_revert = false;
            }
        }
        if self.shortcuts_open {
            // The same rows `docs/user/keyboard.md` is generated from
            // (`actions::shortcut_rows`), in this platform's spelling.
            let is_mac = cfg!(target_os = "macos");
            let mut open = true;
            egui::Window::new(tr("menu.shortcuts"))
                .open(&mut open)
                .collapsible(false)
                .default_width(520.0)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(420.0)
                        .show(ui, |ui| {
                            egui::Grid::new("shortcuts-grid")
                                .striped(true)
                                .num_columns(3)
                                .show(ui, |ui| {
                                    for (key, text, scope) in actions::shortcut_rows(is_mac) {
                                        ui.monospace(key);
                                        ui.label(text);
                                        ui.weak(scope);
                                        ui.end_row();
                                    }
                                });
                        });
                });
            self.shortcuts_open = open;
        }
        if self.about_open {
            let info = self.gpu.adapter.get_info();
            let about = shell_menu::AboutInfo::new(info.name, format!("{:?}", info.backend));
            let mut open = true;
            let mut close = false;
            egui::Window::new(tr("menu.about"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    for line in about.lines() {
                        ui.monospace(line);
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(tr("menu.about-copy")).clicked() {
                            ui.ctx().copy_text(about.lines().join("\n"));
                        }
                        if ui.button(tr("menu.about-close")).clicked() {
                            close = true;
                        }
                    });
                });
            self.about_open = open && !close;
        }
        if self.state.bakes.poll_job() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        // The cavity mask's input (brush-wiring cavity half): each landed
        // Curvature bake loads into the paint session for its tile, once
        // per landing (`take_landed_curvature` drains). The PNG is sRGB-
        // encoded on disk; `curvature_png_to_linear` restores the bake's
        // linear `g`. A failed read or a size mismatch logs and leaves the
        // mask inert on that tile (the panel shows the bake hint).
        // `last_records()` is the panel's public record list (bakes_panel.rs
        // is another session's lock right now); `curvature_feed` keeps the
        // once-per-landing rule on this side, keyed by path + file mtime.
        let landed = self
            .curvature_feed
            .take_new(self.state.bakes.last_records());
        for (tile, path) in landed {
            let Some(paint) = self.state.paint.as_mut() else {
                break;
            };
            match umber_export::png::read_png_rgba8(&path) {
                Ok((w, h, mut rgba)) => {
                    umber_paint::paint_state::curvature_png_to_linear(&mut rgba);
                    if !paint.set_curvature_map(tile, w, h, rgba) {
                        log::warn!(
                            "cavity: curvature map {} ({w}x{h}) does not match tile {tile}'s paint size",
                            path.display()
                        );
                    }
                }
                Err(e) => log::warn!("cavity: cannot read {}: {e}", path.display()),
            }
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
        // g7-polish-batch: the held-key poll runs BEFORE the registry so
        // it sees the raw E / Space down-up pair (`poll_shortcuts`
        // consumes the events the hold resolved; this poll only peeks,
        // so nothing else loses an event). A returned tool rides the
        // same SelectTool dispatch the E registry binding uses.
        let mut actions = toolbar::poll_temp_keys(
            ui.ctx(),
            &mut self.state.temp_tools,
            toolbar::ToolKind::of(&self.brush_panel.tool),
        );
        // Shell shortcuts (`toolbar`'s key table): the overlay keys (W
        // wireframe, G grid — Wave-4 item 7), V lit ↔ unlit (§5's P0 row),
        // X / I quick mask (§2's P1 row), E eraser (§2's P0 row), plus the
        // Ctrl chords for undo/redo/save/open. Empty while a text edit has
        // focus (brush preset names, save-as dialog), so typing never
        // flips the viewport, the mask, or the tool. Dispatched with the
        // toolbar's clicks below, through the same `dispatch`.
        actions.extend(toolbar::poll_shortcuts(ui.ctx()));
        // Keys only a keymap override binds (the main poll pre-filters to
        // the default keys).
        actions.extend(crate::actions::poll_override_keys(
            ui.ctx(),
            &toolbar::SHORTCUT_KEYS,
        ));
        if std::mem::take(&mut self.tab_hide_pending) {
            actions.push(toolbar::ShellAction::ToggleHideUi);
        }
        let hide_ui = self.focus.mode() == shell_menu::FocusMode::HideUi;
        // Top bar: MenuBar container (egui 0.36 API).
        MenuBar::new().ui(ui, |ui| {
            // Every action row prints its registry shortcut (GUI plan G0:
            // the key shown is the key that fires) and greys out from the
            // registry's one enabled-check.
            let shell_state = self.shell_state();
            let is_mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
            let action_item = |ui: &mut Ui, label: String, action: toolbar::ShellAction| {
                let mut button = egui::Button::new(label);
                if let Some(shortcut) = actions::shortcut_label(action, is_mac) {
                    button = button.shortcut_text(shortcut);
                }
                let state = actions::availability(action, shell_state);
                let response = ui.add_enabled(state.is_ok(), button);
                let response = match state {
                    Err(reason) => response.on_disabled_hover_text(tr(reason)),
                    Ok(()) => response,
                };
                response.clicked()
            };
            MenuButton::new(tr("menu.file")).ui(ui, |ui| {
                if action_item(ui, tr("shell.new"), toolbar::ShellAction::NewProject) {
                    actions.push(toolbar::ShellAction::NewProject);
                }
                if action_item(ui, tr("menu.open-mesh"), toolbar::ShellAction::OpenMesh) {
                    actions.push(toolbar::ShellAction::OpenMesh);
                }
                if action_item(
                    ui,
                    tr("menu.open-project"),
                    toolbar::ShellAction::OpenProject,
                ) {
                    actions.push(toolbar::ShellAction::OpenProject);
                }
                if let Some(dir) = self.recent_menu(ui) {
                    ui.close();
                    self.open_project_at(dir);
                }
                if action_item(
                    ui,
                    tr("menu.save-project"),
                    toolbar::ShellAction::SaveProject,
                ) {
                    actions.push(toolbar::ShellAction::SaveProject);
                }
                if action_item(
                    ui,
                    tr("button.save-as"),
                    toolbar::ShellAction::SaveProjectAs,
                ) {
                    actions.push(toolbar::ShellAction::SaveProjectAs);
                }
                let revert = shell_menu::revert_availability(
                    self.state.project_dir.is_some(),
                    self.state.doc.is_dirty(),
                );
                let revert_row =
                    ui.add_enabled(revert.is_ok(), egui::Button::new(tr("menu.revert")));
                let revert_row = match revert {
                    Err(reason) => revert_row.on_disabled_hover_text(tr(reason)),
                    Ok(()) => revert_row,
                };
                if revert_row.clicked() {
                    self.pending_revert = true;
                    ui.close();
                }
                // Close = New: an empty session, through the same unsaved-
                // work confirm (never a silent discard).
                if ui.button(tr("menu.close-project")).clicked() {
                    ui.close();
                    actions.push(toolbar::ShellAction::NewProject);
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
            MenuButton::new(tr("menu.edit")).ui(ui, |ui| {
                if action_item(ui, tr("shell.undo"), toolbar::ShellAction::Undo) {
                    actions.push(toolbar::ShellAction::Undo);
                }
                if action_item(ui, tr("shell.redo"), toolbar::ShellAction::Redo) {
                    actions.push(toolbar::ShellAction::Redo);
                }
                ui.separator();
                if action_item(ui, tr("prefs.title"), toolbar::ShellAction::Preferences) {
                    actions.push(toolbar::ShellAction::Preferences);
                }
            });
            MenuButton::new(tr("menu.view")).ui(ui, |ui| {
                #[cfg(feature = "perf")]
                ui.checkbox(&mut self.show_perf_hud, tr("menu.perf-hud"));
                #[cfg(not(feature = "perf"))]
                ui.label(tr("menu.perf-hud-unavailable"));
                ui.checkbox(&mut self.view.wireframe, tr("menu.wireframe"));
                ui.checkbox(&mut self.view.grid, tr("menu.grid"));
                ui.checkbox(&mut self.show_gizmo, tr("menu.gizmo"));
                ui.separator();
                viewport::view_mode_menu(ui, &mut self.view.view_mode);
            });
            MenuButton::new(tr("menu.window")).ui(ui, |ui| {
                if action_item(ui, tr("shell.hide-ui"), toolbar::ShellAction::ToggleHideUi) {
                    actions.push(toolbar::ShellAction::ToggleHideUi);
                }
                if action_item(
                    ui,
                    tr("shell.maximize"),
                    toolbar::ShellAction::ToggleMaximize,
                ) {
                    actions.push(toolbar::ShellAction::ToggleMaximize);
                }
                ui.separator();
                // Layout edits act on the real layout: leave a focus mode
                // first, or the edit would land on the one-tab stand-in.
                ui.menu_button(tr("menu.panels"), |ui| {
                    for panel in &shell_menu::ALL_PANELS {
                        let mut open = shell_menu::is_open(&self.dock, panel)
                            && self.focus.mode() == shell_menu::FocusMode::Normal;
                        if ui.checkbox(&mut open, panel_title(panel)).changed() {
                            self.focus.restore(&mut self.dock);
                            if shell_menu::is_open(&self.dock, panel) != open {
                                shell_menu::toggle_panel(&mut self.dock, panel);
                            }
                        }
                    }
                });
                if ui.button(tr("menu.reset-layout")).clicked() {
                    self.focus.restore(&mut self.dock);
                    self.dock = self.workspaces.reset_active();
                    ui.close();
                }
            });
            MenuButton::new(tr("menu.help")).ui(ui, |ui| {
                if ui.button(tr("menu.shortcuts")).clicked() {
                    self.shortcuts_open = true;
                    ui.close();
                }
                // Quick Setup re-opens first-run choices (g5-first-run).
                if ui.button(tr("setup.title")).clicked() {
                    self.quick_setup.open();
                    ui.close();
                }
                if ui.button(tr("menu.show-log")).clicked() {
                    ui.close();
                    let path = logfile::log_path(&gpu_startup::state_dir());
                    if let Err(e) = logfile::open_in_system(&path) {
                        self.notices.post(
                            umber_gui::NoticeLevel::Error,
                            crate::i18n::tr_args(
                                "menu.show-log-failed",
                                [
                                    (
                                        "path",
                                        fluent::FluentValue::from(path.display().to_string()),
                                    ),
                                    ("error", fluent::FluentValue::from(e.to_string())),
                                ],
                            ),
                        );
                    }
                }
                if ui.button(tr("menu.about")).clicked() {
                    self.about_open = true;
                    ui.close();
                }
            });
            // The workspace tabs after the menus (Blender's top row).
            ui.separator();
            for workspace in workspaces::Workspace::ALL {
                let active = self.workspaces.active() == workspace;
                if ui
                    .selectable_label(active, tr(workspace.label_key()))
                    .clicked()
                    && !active
                {
                    self.switch_workspace(workspace);
                }
            }
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
        if !hide_ui {
            egui::Panel::top("umber-toolbar").show(ui, |ui| {
                actions.extend(toolbar::show(ui, &toolbar_view, icons));
            });
        }
        for action in actions {
            self.dispatch(action);
        }
        // The Overlays popover's edit (g6-overlays-popover) drains into
        // the shell's own view state BEFORE the sync below, so the
        // popover's toggles become the source of truth the sync
        // propagates — not a value the sync overwrites next frame.
        if let Some(edit) = self.state.viewport.take_overlay_edit() {
            self.view.wireframe = edit.wireframe;
            self.view.grid = edit.grid;
        }
        self.state.viewport.set_show_wireframe(self.view.wireframe);
        self.state.viewport.set_show_grid(self.view.grid);
        self.state.viewport.set_show_gizmo(self.show_gizmo);
        self.state
            .viewport
            .set_show_cage(self.state.bakes.show_cage());
        self.state.viewport.set_view_mode(self.view.view_mode);
        self.state
            .viewport
            .set_front_face_falloff_deg(self.brush_panel.front_face_falloff);
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
            hints: if self.prefs.key_hints {
                status_bar::HintContext::of(
                    self.state.paint.is_some(),
                    self.brush_panel.strip,
                    ui.input(|i| i.modifiers),
                )
            } else {
                status_bar::HintContext::Hidden
            },
            nav: self.state.viewport.nav_preset(),
        };
        if hide_ui {
            // The visible way back (P:A4): a corner button over the
            // viewport, beside the menu bar that also stays.
            egui::Area::new(Id::new("umber-show-ui"))
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 28.0))
                .show(ui.ctx(), |ui| {
                    let label = match actions::shortcut_label(
                        toolbar::ShellAction::ToggleHideUi,
                        ui.ctx().os() == egui::os::OperatingSystem::Mac,
                    ) {
                        Some(key) => format!("{} ({key})", tr("shell.show-ui")),
                        None => tr("shell.show-ui"),
                    };
                    if ui.button(label).clicked() {
                        self.dispatch(toolbar::ShellAction::ToggleHideUi);
                    }
                });
        } else {
            egui::Panel::bottom("umber-status-bar").show(ui, |ui| {
                status_bar::show(ui, &status);
            });
        }

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
        // The home card's inputs (g5-home-screen), split out of the
        // closure: `self.recent` mutably here (the card's Remove/Clear),
        // `pending_recovery` immutably — computed against `state.mesh`
        // so a mesh loaded this frame already skips the card.
        let home_recent: Option<&mut recent::RecentProjects> = if self.state.mesh.is_none() {
            Some(&mut self.recent)
        } else {
            None
        };
        let home_recovery = if self.state.mesh.is_none() {
            self.pending_recovery.as_ref()
        } else {
            None
        };
        let home_actions: Vec<crate::empty_state::HomeCardAction> = CentralPanel::default()
            .show(ui, |ui| {
                let mut dock_style = Style::from_egui(ui.style());
                // g7-density-scale: the separators' grab width follows the
                // hit-zone preference (4 pt standard, 8 pt tablet). The
                // value only changes on a preference change, so this
                // per-frame write is exactly the apply-on-change pattern
                // the Preferences window's ruling pins.
                dock_style.separator.extra_interact_width = self
                    .prefs
                    .splitter_hit_zone
                    .hit_zone()
                    .extra_interact_width();
                let tab_font = egui::TextStyle::Button.resolve(ui.style());
                let mut viewer = PanelViewer {
                    ctx: ui.ctx().clone(),
                    tab_font: tab_font.clone(),
                    assets: &mut self.state.assets_panel,
                    viewport: &mut self.state.viewport,
                    uv_view: &mut self.state.uv_view,
                    home_recent,
                    home_recovery,
                    home_actions: Vec::new(),
                    bakes: &mut self.state.bakes,
                    export: &mut self.state.export,
                    graph: &mut self.state.graph,
                    mesh: self.state.mesh.as_ref(),
                    mesh_path: self.state.mesh_path.as_deref(),
                    gpu: &self.gpu,
                    paint: self.state.paint.as_mut(),
                    doc: &mut self.state.doc,
                    brush: &mut self.brush_panel,
                    properties_rail: &mut self.properties_rail,
                    icons: &self.icons,
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
                    parked_texture_sets: &mut self.state.parked_texture_sets,
                    psd_import: &mut self.state.psd_import,
                    spp_import: &mut self.state.spp_import,
                    base_visible: &mut self.state.base_visible,
                    thumbnails: &mut self.state.thumbnails,
                    layer_gestures: &mut self.state.layer_gestures,
                    pending_resize: &mut self.state.pending_resize,
                    fill_badges: &self.state.fill_badges,
                    import_badges: &self.state.import_badges,
                    dormant: &self.state.dormant,
                    stencil: &mut self.state.stencil,
                    store: store.as_ref(),
                    hdr_mode: &mut self.hdr_mode,
                    stylus_pressure,
                };
                DockArea::new(&mut self.dock)
                    .style(dock_style.clone())
                    .show_inside(ui, &mut viewer);
                // The selected tab stays visible (g5-tab-overflow): a leaf
                // whose active tab changed this frame (a click, a panel
                // toggled in, a restored layout) scrolls it into the strip.
                if scroll_active_tabs_into_view(
                    &mut self.dock,
                    &mut self.tab_active_seen,
                    ui.ctx(),
                    &tab_font,
                    &dock_style,
                ) {
                    ui.ctx().request_repaint();
                }

                let actions: Vec<crate::empty_state::HomeCardAction> =
                    viewer.home_actions.drain(..).collect();
                actions
            })
            .inner;
        // The status bar drew before the dock: one more frame when the
        // hover moved, so its readout catches up (converges — the next
        // frame records the same hover).
        // The home card's actions (g5-home-screen), drained AFTER the
        // dock closure: the same dispatch the File menu rows use, so the
        // card and the menu cannot drift. Recents open through the menu's
        // own opener + note; Recover clears the pending meta first (the
        // launch prompt's own order, else the offer re-fires).
        for action in home_actions {
            match action {
                crate::empty_state::HomeCardAction::NewProject => {
                    self.dispatch(toolbar::ShellAction::NewProject)
                }
                crate::empty_state::HomeCardAction::OpenProject => {
                    self.dispatch(toolbar::ShellAction::OpenProject)
                }
                crate::empty_state::HomeCardAction::OpenRecent(path) => {
                    self.open_project_at(path.clone());
                    self.note_recent(&path);
                }
                crate::empty_state::HomeCardAction::RecoverSession => {
                    if let Some(meta) = self.pending_recovery.take() {
                        self.recover(&meta);
                    }
                }
            }
        }

        if self.state.uv_view.hover_uv() != hover_before {
            ui.ctx().request_repaint();
        }

        // The toast stack, drawn after the dock (GUI plan G0): everything
        // under it has laid out, and `show` schedules its own repaints.
        self.notices.show(ui.ctx());
        // The Layers panel's ✕ confirm (g2-layer-gestures), with the other
        // shell windows: drawn after the dock so it sits over the panel.
        let removed = self
            .state
            .layer_gestures
            .confirm_ui(ui.ctx(), &mut self.state.doc);
        if !removed.is_empty() {
            if let Some(paint) = self.state.paint.as_mut() {
                let outcome = layers_panel::gestures::GestureOutcome {
                    removed,
                    ..Default::default()
                };
                apply_layer_session_ops(paint, &layer_session_ops(&outcome));
            }
        }
        self.show_preferences(ui.ctx());
        // Quick Setup (g5-first-run): the window the first launch shows
        // (and Help re-opens). The choices apply through the same
        // setters the Preferences window uses — locale + prefs already
        // persisted by the picker/apply, theme via the context, nav +
        // keymap through their own sync paths.
        if self.quick_setup.is_open() {
            let before = self.prefs.clone();
            if let Some(done) = self
                .quick_setup
                .show(ui.ctx(), &self.state.stylus, &self.prefs)
            {
                self.prefs.nav = done.nav;
                // The keymap choice applies BEFORE the save (the save
                // persists it): Blender installs its table, Umber RESETS
                // any earlier override table.
                let keymap_overrides = if done.keymap_blender {
                    actions::KeymapPreset::Blender
                        .overrides()
                        .expect("Blender carries overrides")
                } else {
                    actions::KeymapPreset::Umber
                        .overrides()
                        .expect("Umber carries overrides")
                };
                self.prefs.keymap_overrides = keymap_overrides;
                actions::set_overrides(&self.prefs.keymap_overrides);
                self.prefs.save(preferences::prefs_path().as_deref());
                if let Some(theme) = done.theme.egui_theme() {
                    ui.ctx().set_theme(theme);
                }
                // A nav change adopts immediately (the same call the
                // Preferences window's combo makes).
                if self.prefs.nav != before.nav {
                    let env = std::env::var("UMBER_NAV").ok();
                    self.state
                        .viewport
                        .set_nav_preset(self.prefs.effective_nav(env.as_deref()));
                }
            }
        }
        // The command palette overlay (Ctrl+Shift+P / F3), drawn last so it
        // sits over everything; a chosen action runs through the same
        // dispatch as every key and menu row.
        let is_mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
        let picked = self.palette.show(
            ui.ctx(),
            tr,
            i18n::tr_en,
            |a| actions::shortcut_label(a, is_mac),
            &tr("palette.placeholder"),
        );
        if let Some(action) = picked {
            self.dispatch(action);
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
                    // The ACTIVE set's tiles only (C2-9): the doc's live
                    // stack is that set's, and another set's tiles never
                    // composite these fills (the material drain's rule).
                    let active_set = paint.active_set().to_string();
                    let (tiles, symmetry) = (paint.tiles_of_set(&active_set), *paint.symmetry());
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
            let mut snapshot = build_composite_snapshot(
                &self.state.doc.stack,
                &self.state.material_states,
                self.state.base_visible,
                displayed,
            );
            // Per-set rows (C2-9): with more than one set, each tile
            // composites its own set's stack, and the inactive sets keep
            // their planes (the retire census reads the union).
            let sets = self.state.doc.all_sets();
            if sets.len() > 1 {
                for (name, stack) in sets {
                    let rows = build_composite_snapshot(
                        stack,
                        &self.state.material_states,
                        self.state.base_visible,
                        Vec::new(),
                    )
                    .layers;
                    snapshot.set_layers.insert(name.to_string(), rows);
                }
            }
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
            // § psd-publish (W-GUI-3): the same drain for flattened PSD
            // imports — layer creation already happened in the Assets
            // panel's apply; the flat pixels publish here into the new
            // layer's plane (or the row badge refuses loudly).
            let psd_imports = std::mem::take(&mut self.state.psd_import.to_publish);
            for applied in &psd_imports {
                publish_psd_plane(paint, &mut self.state.import_badges, applied);
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

/// The notice stack a launch starts with: one sticky notice per crash
/// report the previous runs left (C2-2), each acknowledged once offered
/// (renamed `.seen`, kept on disk), so a report is shown exactly once.
fn pending_crash_notices() -> umber_gui::Notices {
    let mut notices = umber_gui::Notices::new();
    let state = gpu_startup::state_dir();
    for report in crash::pending(&state) {
        // Untranslated until the catalogs take a crash key.
        notices.post_sticky(
            umber_gui::NoticeLevel::Error,
            format!(
                "Umber crashed last time. The report is at\n{}\n(it holds no document data; attach it to an issue).",
                report.display()
            ),
        );
        if let Err(e) = crash::acknowledge(&report) {
            log::warn!("crash report {} not acknowledged: {e}", report.display());
        }
    }
    notices
}

/// What the quit guard does with this frame's close request (C2-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitDecision {
    /// Let it close (nothing pending, nothing unsaved, or already confirmed).
    Allow,
    /// Cancel the close and ask Save / Discard / Cancel.
    AskFirst,
}

/// The guard's rule: only a close request over unsaved edits that the user
/// has not already confirmed asks first. Pure, so it is unit-tested.
fn quit_decision(close_requested: bool, dirty: bool, confirmed: bool) -> QuitDecision {
    if close_requested && dirty && !confirmed {
        QuitDecision::AskFirst
    } else {
        QuitDecision::Allow
    }
}

/// The quit window's three answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitChoice {
    Save,
    Discard,
    Cancel,
}

/// Where a plain Save writes (C2-4): in place when the document already
/// lives in a directory, through the dialog when it never did.
#[derive(Debug, PartialEq, Eq)]
enum SaveTarget {
    /// Overwrite the open project.
    InPlace(PathBuf),
    /// Never saved: Save As.
    Ask,
}

fn save_target(project_dir: Option<&std::path::Path>) -> SaveTarget {
    project_dir.map_or(SaveTarget::Ask, |dir| {
        SaveTarget::InPlace(dir.to_path_buf())
    })
}

fn main() -> eframe::Result<()> {
    // The crash reporter first (C2-2): every later panic, startup included,
    // leaves a report under `<state>/crashes/` for the next launch to offer.
    crash::install(gpu_startup::state_dir());
    // Logs tee to `<state>/logs/umber.log` (Help ▸ Show log); stderr stays
    // the sink of record when the file cannot open.
    let log_tee = logfile::Tee {
        primary: std::io::stderr(),
        secondary: logfile::open_session_log(&gpu_startup::state_dir()),
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(log_tee)))
        .init();
    log::info!("umber — starting (wave-2: paint core)");
    // UI strings: catalogs + locale on this (the UI) thread, before any
    // panel renders (docs/specs/i18n-design.md).
    i18n::init();

    // Paint-compositor requirement (umber-gpu paint claw reviewer checklist
    // item #1): read_write storage textures need the adapter-specific format
    // features requested on the device, or the compute splat pass panics at
    // first real stroke. eframe owns the device, so the requirement must be
    // injected here — through the device descriptor closure.
    //
    // Crash-safe GPU startup (gpu_startup.rs): decide the backend from the
    // previous launch's marker (or WGPU_BACKEND / --safe-gpu), and write the
    // marker before eframe touches the driver.
    let safe_gpu = std::env::args().skip(1).any(|a| a == "--safe-gpu");
    let gpu_decision = gpu_startup::prepare(safe_gpu);
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(ref mut create_new) = wgpu_options.wgpu_setup {
        if let Some(backends) = gpu_decision.wgpu_backends() {
            create_new.instance_descriptor.backends = backends;
        }
        create_new.device_descriptor = std::sync::Arc::new(|adapter| {
            // egui-wgpu calls this right before request_device: the marker
            // names the adapter a driver crash would have died on.
            gpu_startup::note_adapter(&adapter.get_info());
            eframe::egui_wgpu::wgpu::DeviceDescriptor {
                required_features:
                    eframe::egui_wgpu::wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                ..Default::default()
            }
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
    let result = eframe::run_native(
        "Umber",
        native,
        Box::new(|cc| {
            // The device exists: a death from here is not a driver crash.
            gpu_startup::note_device();
            match UmberApp::new(cc) {
                Ok(mut app) => {
                    app.load_default_env();
                    Ok(Box::new(app) as Box<dyn eframe::App>)
                }
                Err(err) => Err(format!("{err:#}").into()),
            }
        }),
    );
    // An Err (e.g. no device) keeps an unconfirmed marker so the next
    // launch steps the backend down; a clean exit clears it.
    gpu_startup::finish(result.is_ok());
    result
}

/// One paint-session step a layer gesture's outcome asks for
/// (g2-layer-gestures). The pure half of the shell's mirror: the gesture
/// state edits the document, these touch the paint session.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LayerSessionOp {
    /// Copy every plane of `source` into its duplicate `copy`.
    CopyPlanes { source: u64, copy: u64 },
    /// Leave mask editing when the edited layer is among these removed
    /// layers (their planes stay: undo restores the layer).
    EndMaskEditIfRemoved(Vec<u64>),
    /// Select the layer for strokes (explicit).
    SetTarget(u64),
    /// Paint this layer's mask (after its `SetTarget`, the mask section's
    /// [m5] order: the target first, then the mask edit).
    EditMask(u64),
}

/// The paint-session steps for one gesture outcome, in apply order:
/// plane copies first (the copy must hold pixels before it is the
/// target), then the removed-layer mask check, then the target, then the
/// mask edit. The target before the mask edit matters when the target
/// MOVED to the mask's layer from another (`set_active_target` keeps a
/// mask edit only on the target's OWN layer — a foreign edit is cleared,
/// which is exactly what a select-then-paint gesture wants). A same-layer
/// target is a no-op for the edit either way. A solo change has no
/// session step (no render path reads it yet).
/// The smart-mask compose flow (C2-10), extracted for BOTH panel arms:
/// the Layers panel's mask section and the Assets browser's mask rows
/// (R-C1) run the SAME path — the mask composed over the newest bakes
/// on every tile of the active set at the set's size, the anchors read
/// the named layers' own planes, the bakes' sRGB PNGs linearized first
/// (vector maps are already linear).
fn compose_mask_flow(
    paint: &mut paint_state::PaintState,
    doc: &document::Document,
    bakes: &BakesPanel,
    layer: u64,
    mask: &umber_core::smart::SmartMask,
) {
    let active_set = paint.active_set().to_string();
    let tiles: Vec<u16> = paint
        .tiles_present()
        .into_iter()
        .filter(|t| paint.set_of_tile(*t) == active_set)
        .collect();
    // Anchor stages resolve by layer NAME in the active set's stack
    // (the preset's template-local names).
    let by_name: std::collections::HashMap<String, u64> = doc
        .layers()
        .iter()
        .map(|l| (l.name.clone(), l.id))
        .collect();
    for tile in tiles {
        let (w, h) = paint.tile_size(tile);
        // Anchors read the named layer's OWN plane (premultiplied
        // linear bytes); read up front so the compose closures borrow
        // nothing mutably.
        let mut anchor_reads: AnchorReads = std::collections::HashMap::new();
        for stage in &mask.ops {
            if let umber_core::smart::MaskSource::Anchor { layer, channel } = &stage.source {
                let key = (layer.clone(), channel.clone());
                if anchor_reads.contains_key(&key) {
                    continue;
                }
                let read = by_name.get(layer).and_then(|id| {
                    paint
                        .read_plane(paint_state::PlaneOwner::Layer(*id), tile, channel)
                        .map(|b| (b.width, b.height, b.data))
                });
                anchor_reads.insert(key, read);
            }
        }
        let composed = document::compose_smart_mask(
            mask,
            w,
            h,
            |name| {
                let path = bakes.latest_map(name)?;
                let (mw, mh, mut rgba) = umber_export::png::read_png_rgba8(path).ok()?;
                let linear = matches!(
                    name.to_ascii_lowercase().as_str(),
                    "normal"
                        | "normal_base"
                        | "world_normal"
                        | "world_space_normal"
                        | "bent_normal"
                        | "height"
                );
                if !linear {
                    paint_state::curvature_png_to_linear(&mut rgba);
                }
                Some((mw, mh, rgba))
            },
            |layer, channel| {
                anchor_reads
                    .get(&(layer.to_string(), channel.to_string()))
                    .cloned()
                    .flatten()
            },
        );
        match composed {
            Ok((plane, missing)) => {
                if !missing.is_empty() {
                    log::info!(
                        "smart mask '{}': {} missing at {w}x{h} (unbaked maps read mid-grey, missing layers open)",
                        mask.name,
                        missing.join(", ")
                    );
                }
                if !paint.publish_layer_mask(layer, tile, plane) {
                    log::warn!("smart mask: tile {tile} refused the plane");
                }
            }
            Err(e) => log::warn!("smart mask '{}': {e}", mask.name),
        }
    }
}

/// The symmetry carry conversions (the symmetry-persistence kill,
/// 2026-10-24): free fns, not From impls — the orphan rule bars the
/// app from impling a trait between two foreign crates' types, and
/// umber-core stays GPU-free (it owns the serde MIRROR; the app owns
/// the mapping — the `postfx_panel::carry_from_chain` precedent).
fn symmetry_carry_from(
    s: &umber_brush::symmetry::SymmetrySettings,
) -> umber_core::project::ProjectSymmetry {
    use umber_brush::symmetry::MirrorAxis;
    umber_core::project::ProjectSymmetry {
        mirror: umber_core::project::ProjectMirror {
            enabled: s.mirror.enabled,
            axis: match s.mirror.axis {
                MirrorAxis::X => "x".to_owned(),
                MirrorAxis::Y => "y".to_owned(),
            },
            offset: s.mirror.offset,
        },
        radial: umber_core::project::ProjectRadial {
            enabled: s.radial.enabled,
            count: s.radial.count,
            flip_u: s.radial.flip_u,
            flip_v: s.radial.flip_v,
        },
    }
}

fn symmetry_from_carry(
    c: &umber_core::project::ProjectSymmetry,
) -> umber_brush::symmetry::SymmetrySettings {
    use umber_brush::symmetry::{MirrorAxis, MirrorSettings, RadialSettings, SymmetrySettings};
    SymmetrySettings {
        mirror: MirrorSettings {
            enabled: c.mirror.enabled,
            axis: if c.mirror.axis.eq_ignore_ascii_case("y") {
                MirrorAxis::Y
            } else {
                MirrorAxis::X
            },
            offset: c.mirror.offset,
        },
        radial: RadialSettings {
            enabled: c.radial.enabled,
            count: c.radial.count,
            flip_u: c.radial.flip_u,
            flip_v: c.radial.flip_v,
        },
    }
}

fn layer_session_ops(outcome: &layers_panel::gestures::GestureOutcome) -> Vec<LayerSessionOp> {
    let mut ops: Vec<LayerSessionOp> = outcome
        .duplicated
        .iter()
        .map(|&(source, copy)| LayerSessionOp::CopyPlanes { source, copy })
        .collect();
    if !outcome.removed.is_empty() {
        ops.push(LayerSessionOp::EndMaskEditIfRemoved(
            outcome.removed.clone(),
        ));
    }
    if let Some(id) = outcome.mask_edit.or(outcome.select) {
        ops.push(LayerSessionOp::SetTarget(id));
    }
    if let Some(id) = outcome.mask_edit {
        ops.push(LayerSessionOp::EditMask(id));
    }
    ops
}

/// Runs [`layer_session_ops`]' steps on the paint session.
fn apply_layer_session_ops(paint: &mut paint_state::PaintState, ops: &[LayerSessionOp]) {
    for op in ops {
        match op {
            LayerSessionOp::CopyPlanes { source, copy } => {
                if let Err(refusal) = paint.copy_owner_planes(
                    paint_state::PlaneOwner::Layer(*source),
                    paint_state::PlaneOwner::Layer(*copy),
                ) {
                    log::warn!("duplicate: plane copy refused: {refusal:?}");
                }
            }
            LayerSessionOp::EndMaskEditIfRemoved(removed) => {
                if paint
                    .mask_edit_layer()
                    .is_some_and(|id| removed.contains(&id))
                {
                    paint.set_mask_edit_layer(None);
                }
            }
            LayerSessionOp::SetTarget(id) => {
                paint.set_active_target(paint_state::PlaneOwner::Layer(*id));
            }
            LayerSessionOp::EditMask(id) => paint.set_mask_edit_layer(Some(*id)),
        }
    }
}

/// Follows a selection made outside the gestures (an add button, the
/// smart shelf, the Base row) with the multi-selection: a layer becomes
/// the single selection, Base clears it.
fn sync_gesture_selection(
    gestures: &mut layers_panel::gestures::GestureState,
    target: paint_state::PlaneOwner,
) {
    match target {
        paint_state::PlaneOwner::Layer(id) => gestures.selection.set_single(id),
        paint_state::PlaneOwner::Base => gestures.selection.set_many(&[], None),
    }
}

/// g2-properties-context (the shell half): the decision fn pinned above,
/// the drawn rail pinned here.
#[cfg(test)]
mod properties_rail_tests {
    use super::properties_kinds;
    use umber_gui::ContextKind;

    #[test]
    fn a_fill_target_puts_fill_on_the_rail_a_paint_target_does_not() {
        // The brief's pinned decision: a fill layer shows Fill; Layer +
        // TextureSet show for every layer target; a paint layer has no
        // Fill; no selection leaves Brush + TextureSet.
        let fill = properties_kinds(Some(umber_core::LayerKind::Fill), false, false);
        assert_eq!(
            fill,
            vec![
                ContextKind::Brush,
                ContextKind::Layer,
                ContextKind::Fill,
                ContextKind::TextureSet
            ]
        );
        let paint = properties_kinds(Some(umber_core::LayerKind::Paint), false, false);
        assert!(
            !paint.contains(&ContextKind::Fill),
            "a paint layer has no fill context"
        );
        let none = properties_kinds(None, false, false);
        assert_eq!(none, vec![ContextKind::Brush, ContextKind::TextureSet]);
        let masked = properties_kinds(Some(umber_core::LayerKind::Paint), true, false);
        assert!(
            masked.contains(&ContextKind::Mask),
            "a layer mask shows Mask"
        );
        let focus = properties_kinds(None, false, true);
        assert!(focus.contains(&ContextKind::Mask), "a mask edit shows Mask");
    }

    /// The plan row's kittest pin, drawn headless: a fill target's rail
    /// actually SHOWS the fill tab, a paint target's does not. The
    /// queries are what a screen reader sees (the tooltip label rides
    /// the widget_info). English tooltips in-fixture: the widget holds
    /// no strings; the shell's localized ones are the live path's.
    #[test]
    fn a_fill_layer_shows_the_fill_tab_on_the_drawn_rail() {
        use egui_kittest::kittest::Queryable;
        use egui_kittest::Harness;

        /// Draws the rail once (the harness's lifetime keeps the
        /// fixture's borrows, so the assertions run INSIDE) and checks
        /// which tabs are present / absent by label.
        fn assert_rail_tabs(kinds: &[ContextKind], present: &[&str], absent: &[&str]) {
            let mut rail = umber_gui::ContextRail::new("kit-rail", ContextKind::Brush);
            let _ = rail.set_applicable(kinds);
            let atlas = umber_gui::IconAtlas::empty();
            const TIPS: [&str; ContextKind::COUNT] =
                ["Brush", "Layer", "Fill", "Mask", "Texture set"];
            let crumb = vec!["Set".to_owned(), "Sword".to_owned()];
            let ctx = umber_gui::ContextRailCtx {
                atlas: &atlas,
                tooltips: TIPS,
                breadcrumb: &crumb,
                search_hint: "Search",
            };
            let mut harness = Harness::<()>::builder()
                .with_size(egui::vec2(320.0, 240.0))
                .build_ui(|ui| {
                    let (_, ()) = rail.ui(ui, &ctx, |ui, _| {
                        ui.label("body");
                    });
                });
            assert!(harness.run_ok().is_some(), "the rail frame laid out");
            for tab in present {
                assert!(
                    harness.query_by_label(tab).is_some(),
                    "expected tab {tab:?} on the drawn rail"
                );
            }
            for tab in absent {
                assert!(
                    harness.query_by_label(tab).is_none(),
                    "tab {tab:?} must be absent from the drawn rail"
                );
            }
        }

        assert_rail_tabs(
            &properties_kinds(Some(umber_core::LayerKind::Fill), false, false),
            &["Fill", "Layer", "Brush", "Texture set"],
            &[],
        );
        assert_rail_tabs(
            &properties_kinds(Some(umber_core::LayerKind::Paint), false, false),
            &["Layer", "Brush", "Texture set"],
            &["Fill"],
        );
    }
}

/// The rail's row-label registry (`rail-row-labels`): each kind's search
/// vocabulary is its own body's label list, and the Properties arm
/// registers it before drawing.
#[cfg(test)]
mod properties_row_label_tests {
    use super::*;
    use umber_gui::{ContextKind, ContextRail};

    #[test]
    fn the_texture_set_row_labels_are_the_rows_the_panel_draws() {
        let src = include_str!("main.rs");
        let start = src
            .find("fn texture_sets_panel_ui(")
            .expect("the texture-set body");
        let end = start + src[start..].find("\n}").expect("its closing brace");
        crate::document::assert_row_label_keys(
            TEXTURE_SET_LABEL_KEYS,
            &texture_sets_row_labels(),
            &src[start..end],
        );
        assert_eq!(
            texture_sets_row_labels(),
            ["Texture set", "Paint target size", "Per-channel sizes"]
        );
    }

    #[test]
    fn each_kind_registers_its_own_bodys_labels() {
        assert_eq!(
            properties_row_labels(ContextKind::Brush),
            brush_panel::row_labels()
        );
        assert_eq!(
            properties_row_labels(ContextKind::Layer),
            document::layer_properties_row_labels()
        );
        let mut fill = fill_layer::row_labels();
        fill.extend(path_tool::row_labels());
        assert_eq!(properties_row_labels(ContextKind::Fill), fill);
        assert_eq!(
            properties_row_labels(ContextKind::Mask),
            document::layer_mask_row_labels()
        );
        assert_eq!(
            properties_row_labels(ContextKind::TextureSet),
            texture_sets_row_labels()
        );
        // Pairwise distinct, so two swapped arms cannot pass the pins
        // above; the Fill union (two modules' lists) stays unique too.
        let lists: Vec<Vec<String>> = ContextKind::ALL
            .into_iter()
            .map(properties_row_labels)
            .collect();
        for (i, a) in lists.iter().enumerate() {
            for b in &lists[i + 1..] {
                assert_ne!(a, b);
            }
        }
        let unique: std::collections::HashSet<&String> = fill.iter().collect();
        assert_eq!(unique.len(), fill.len(), "{fill:?}");
        assert_eq!(fill.len(), 20);
    }

    #[test]
    fn the_registered_labels_drive_the_rails_search() {
        let mut rail = ContextRail::new("row-labels", ContextKind::Brush);
        rail.set_labels(properties_row_labels(ContextKind::Brush));
        rail.set_query("jitter");
        // Size / Angle / Flow jitter; the position jitter reads "Scatter".
        assert_eq!(rail.matches(), [34, 35, 37]);
        assert_eq!(rail.next_hit(), Some(35));
        // Re-registering the same list (every frame) keeps the hit.
        rail.set_labels(properties_row_labels(ContextKind::Brush));
        assert_eq!(rail.current_hit(), Some(35));
        // A context switch registers a different list: the search
        // restarts at that list's first hit.
        assert!(rail.select(ContextKind::Layer));
        rail.set_labels(properties_row_labels(ContextKind::Layer));
        rail.set_query("blend");
        assert_eq!(rail.current_hit(), Some(1));
    }

    #[test]
    fn the_properties_arm_registers_the_labels_before_drawing_the_body() {
        // Source order in the arm: the kind is read, its labels are
        // registered, then the rail draws (the first occurrences sit in
        // the arm; this test's own copies come later in the file).
        let src = include_str!("main.rs");
        let kind = src.find("let body_kind = kind;").expect("the kind read");
        let register = src
            .find("set_labels(properties_row_labels(body_kind))")
            .expect("the registration");
        let draw = src.find(".ui(ui, &rail_ctx,").expect("the rail draw");
        assert!(
            kind < register && register < draw,
            "{kind} {register} {draw}"
        );
    }
}

/// The PSD publish binding's drain test: a matching flat publishes,
/// a size-mismatched flat refuses loudly (the kill report's pin).
#[cfg(test)]
mod psd_publish_tests {
    /// Requests a paint-capable device (graceful skip where no wgpu
    /// adapter exists — the same pattern as `paint_state`'s and
    /// `bake_sources`' GPU tests). Returns `None` when skipped.
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

    use super::{psd_import, publish_psd_plane};
    use umber_paint::paint_state;

    #[test]
    fn the_psd_drain_publishes_a_matching_flat_and_refuses_a_mismatch_loudly() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let size = 4u32;
        let mut paint = paint_state::PaintState::with_target_size(device, queue, size)
            .expect("paint state builds");
        let flat = psd_import::PsdImport {
            set_name: "Sword".into(),
            width: size,
            height: size,
            pixels: [188, 188, 188, 255].repeat(4 * 4),
            report: psd_import::PsdImportReport::default(),
        };
        let mut badges = std::collections::HashMap::<u64, String>::new();
        publish_psd_plane(
            &mut paint,
            &mut badges,
            &psd_import::PsdPublish {
                layer_id: 7,
                flat: flat.clone(),
            },
        );
        assert!(
            badges.is_empty(),
            "a target-sized flat publishes with no badge"
        );
        let plane = paint
            .read_plane(
                paint_state::PlaneOwner::Layer(7),
                psd_import::PSD_PUBLISH_TILE,
                psd_import::PSD_TARGET_CHANNEL,
            )
            .expect("the publish materializes the layer's plane");
        assert_eq!((plane.width, plane.height), (size, size));
        assert_eq!(
            plane.data,
            psd_import::publish_psd_pixels(&flat),
            "the plane carries the curve's linearized premultiplied rows (the drain must not skip the curve)"
        );
        // A flat bigger than the target: the drain's gate badges instead
        // of trusting `publish_plane`'s silent drop; the layer stays
        // empty (the plane read above is untouched).
        let big = psd_import::PsdImport {
            set_name: "Scabbard".into(),
            width: 8,
            height: 8,
            pixels: [64, 64, 64, 255].repeat(8 * 8),
            report: psd_import::PsdImportReport::default(),
        };
        publish_psd_plane(
            &mut paint,
            &mut badges,
            &psd_import::PsdPublish {
                layer_id: 9,
                flat: big,
            },
        );
        assert_eq!(
            badges.get(&9).map(String::as_str),
            Some(crate::i18n::tr("import.size-mismatch").as_str()),
            "the mismatch is the row badge, not a silent drop"
        );
        assert!(
            paint
                .read_plane(
                    paint_state::PlaneOwner::Layer(9),
                    psd_import::PSD_PUBLISH_TILE,
                    psd_import::PSD_TARGET_CHANNEL
                )
                .is_none(),
            "the refused flat leaves the layer with no plane"
        );
    }
}

/// g2-gesture-integration: the gesture outcome → paint-session mapping
/// (headless; the `PaintState` half needs a GPU session — the kill
/// report names it unverified).
#[cfg(test)]
mod layer_gesture_shell_tests {
    use super::{layer_session_ops, sync_gesture_selection, LayerSessionOp};
    use crate::layers_panel::gestures::{GestureOutcome, GestureState};
    use crate::paint_state::PlaneOwner;

    #[test]
    fn an_empty_outcome_touches_nothing() {
        assert!(layer_session_ops(&GestureOutcome::default()).is_empty());
        // A solo change alone has no session step either.
        let solo = GestureOutcome {
            solo_changed: true,
            ..Default::default()
        };
        assert!(layer_session_ops(&solo).is_empty());
    }

    #[test]
    fn a_duplicate_copies_planes_before_selecting_the_copy() {
        let outcome = GestureOutcome {
            select: Some(11),
            duplicated: vec![(1, 10), (2, 11)],
            ..Default::default()
        };
        assert_eq!(
            layer_session_ops(&outcome),
            vec![
                LayerSessionOp::CopyPlanes {
                    source: 1,
                    copy: 10
                },
                LayerSessionOp::CopyPlanes {
                    source: 2,
                    copy: 11
                },
                LayerSessionOp::SetTarget(11),
            ]
        );
    }

    #[test]
    fn a_mask_edit_targets_the_layer_first_then_its_mask() {
        // [m5]: `set_active_target` clears a foreign mask edit, so the
        // target must land BEFORE the mask edit or the edit is wiped.
        let outcome = GestureOutcome {
            select: Some(4),
            mask_edit: Some(4),
            ..Default::default()
        };
        assert_eq!(
            layer_session_ops(&outcome),
            vec![LayerSessionOp::SetTarget(4), LayerSessionOp::EditMask(4)]
        );
    }

    #[test]
    fn a_removal_ends_its_mask_edit_and_retargets() {
        let outcome = GestureOutcome {
            select: Some(2),
            removed: vec![5, 6],
            ..Default::default()
        };
        assert_eq!(
            layer_session_ops(&outcome),
            vec![
                LayerSessionOp::EndMaskEditIfRemoved(vec![5, 6]),
                LayerSessionOp::SetTarget(2),
            ]
        );
        // Everything removed: no target step (the per-frame revalidation
        // resolves the vanished id to a neighbour or Base).
        let all = GestureOutcome {
            removed: vec![5],
            ..Default::default()
        };
        assert_eq!(
            layer_session_ops(&all),
            vec![LayerSessionOp::EndMaskEditIfRemoved(vec![5])]
        );
    }

    #[test]
    fn outside_selections_follow_into_the_gesture_selection() {
        let mut state = GestureState::default();
        state.selection.set_many(&[1, 2, 3], Some(2));
        sync_gesture_selection(&mut state, PlaneOwner::Layer(7));
        assert_eq!(state.selection.ids(), vec![7]);
        assert_eq!(state.selection.primary(), Some(7));
        sync_gesture_selection(&mut state, PlaneOwner::Base);
        assert!(state.selection.is_empty());
        assert_eq!(state.selection.primary(), None);
    }
}

/// File > New tests: the headless reset half (`AppState::reset_for_new` —
/// every field pinned to its exact boot-fresh value, not a range check).
/// The GPU half (the paint-session rebuild inside `new_project`) needs an
/// `eframe::CreationContext`, so it is unverified headless — see the kill
/// report.
#[cfg(test)]
mod new_project_tests {
    #[test]
    fn two_sets_round_trip_through_the_project_model() {
        // C2-9 stage 3b's file contract: the save writes one TextureSetLayers
        // per set (what `write_project_files` assembles from `all_sets`),
        // and the open's `load_sets` restores both, ids intact and unique.
        use crate::document::Document;
        use umber_core::layers::{LayerCommand, LayerKind};
        let mut doc = Document::default();
        doc.activate_set("Body");
        doc.run(LayerCommand::add("body", LayerKind::Paint));
        doc.activate_set("Hilt");
        doc.run(LayerCommand::add("hilt", LayerKind::Paint));
        let sets: Vec<(String, umber_core::LayerStack)> = doc
            .all_sets()
            .into_iter()
            .map(|(n, st)| (n.to_string(), st.clone()))
            .collect();
        let model = umber_core::project::ProjectModel::new(
            sets.iter()
                .map(|(n, _)| umber_core::TextureSet::new_default(n))
                .collect(),
            sets.iter()
                .map(|(n, st)| umber_core::project::TextureSetLayers {
                    texture_set: n.clone(),
                    stack: st.clone(),
                })
                .collect(),
            umber_core::project::ProjectSettings {
                active_texture_set: Some("Hilt".into()),
                ..Default::default()
            },
        );
        let dir = std::env::temp_dir().join(format!("umber-two-sets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        umber_core::project::save_to_dir(&model, &dir).expect("saves");
        let back = umber_core::project::load_from_dir(&dir).expect("opens");
        let mut reopened = Document::default();
        reopened.load_sets(
            back.layers
                .iter()
                .map(|l| (l.texture_set.clone(), l.stack.clone()))
                .collect(),
            back.settings.active_texture_set.as_deref().unwrap_or(""),
        );
        assert_eq!(reopened.active_set(), "Hilt");
        assert_eq!(reopened.layers()[0].name, "hilt");
        reopened.activate_set("Body");
        assert_eq!(reopened.layers()[0].name, "body");
        let ids: Vec<u64> = reopened
            .all_sets()
            .iter()
            .flat_map(|(_, st)| st.layers.iter().map(|l| l.id))
            .collect();
        let unique: std::collections::BTreeSet<u64> = ids.iter().copied().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "ids unique across reopened sets: {ids:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_quit_guard_asks_only_for_unsaved_unconfirmed_closes() {
        use super::{quit_decision, QuitDecision::*};
        assert_eq!(
            quit_decision(true, true, false),
            AskFirst,
            "dirty close asks"
        );
        assert_eq!(
            quit_decision(true, false, false),
            Allow,
            "a clean close just closes"
        );
        assert_eq!(
            quit_decision(true, true, true),
            Allow,
            "Discard / Save confirmed it"
        );
        assert_eq!(
            quit_decision(false, true, false),
            Allow,
            "no close, nothing to guard"
        );
    }

    #[test]
    fn save_is_silent_once_the_project_has_a_directory() {
        use super::{save_target, SaveTarget};
        assert_eq!(
            save_target(None),
            SaveTarget::Ask,
            "never saved: the dialog"
        );
        let dir = std::path::Path::new("/projects/sword.umber");
        assert_eq!(
            save_target(Some(dir)),
            SaveTarget::InPlace(dir.to_path_buf())
        );
    }

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
        state.layer_gestures.selection.set_single(3);
        state.layer_gestures.mask_solo.toggle(3);
        state.layer_gestures.pending_delete = Some(vec![3]);
        state.pending_resize = Some(PendingResize {
            set: "Old".to_string(),
            from: (512, 512),
            to: 4096,
        });

        state.reset_for_new();

        assert_eq!(
            state.pending_resize, None,
            "an armed resize must not fire on the new document"
        );

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
        assert!(
            state.layer_gestures.selection.is_empty()
                && state.layer_gestures.mask_solo.layer().is_none()
                && state.layer_gestures.pending_delete.is_none(),
            "the layer gestures die with their document (ids restart)"
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
    use umber_paint::persistence::{format_paint_commit, parse_paint_commit};
    use umber_paint::snapshot::resolve_instance_source;

    #[test]
    fn a_material_evaluates_onto_its_own_sets_tiles_only() {
        // C2-9: the material IO lists the IO's set's tiles, so a parked
        // set's tiles are never written by the active set's materials.
        use material_layer::MaterialTileIo as _;
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            return;
        };
        let Ok((device, queue)) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                ..Default::default()
            }))
        else {
            return;
        };
        let mut paint = paint_state::PaintState::with_target_size(device, queue, 64)
            .expect("paint state builds");
        paint.assign_tile_to_set(1001, "Body");
        paint.assign_tile_to_set(1002, "Hilt");
        paint.set_mesh(&paint_state::two_tile_strip());
        // Tile 1002 materializes on its first stroke (the paint_state
        // routing tests' shape).
        paint.begin_stroke(egui::pos2(1.2, 0.5));
        paint.end_stroke();
        assert_eq!(paint.tiles_present(), vec![1001, 1002]);
        let io = |paint: &mut paint_state::PaintState, set: &str| -> Vec<u16> {
            PaintMaterialIo {
                paint,
                set: set.to_string(),
                live_layers: Vec::new(),
                snapshot: paint_state::CompositeSnapshot::default(),
            }
            .tiles()
        };
        assert_eq!(io(&mut paint, "Body"), vec![1001]);
        assert_eq!(io(&mut paint, "Hilt"), vec![1002]);
    }

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

/// g5-tab-overflow dock wiring: the title galley the `TabViewer` returns,
/// egui_dock 0.21.1's tab-width arithmetic, and the post-show
/// scroll-into-view pass on a real `DockState` — all headless (fonts
/// only, no GPU, no rendered dock).
#[cfg(test)]
mod tab_overflow_dock_tests {
    use super::*;

    /// A context with fonts loaded (the first pass builds the atlas);
    /// the first frame's texture delta is cleared so its Drop bomb
    /// cannot fire (epaint 0.36 `textures.rs`).
    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        ctx
    }

    /// Galleys built after the first pass queue a font-texture delta;
    /// clear it before the test ends (same disarm as the widget's tests).
    fn disarm(ctx: &egui::Context) {
        ctx.tex_manager().write().take_delta().clear();
    }

    #[test]
    fn the_tab_title_is_a_pre_laid_elided_galley() {
        let ctx = ctx();
        let font = egui::TextStyle::Button.resolve(&ctx.global_style());
        let long = "Conjuntos de texturas e muito mais texto que nunca cabe";
        let WidgetText::Galley(cut) = tab_title_text(&ctx, long, font.clone()) else {
            panic!("the title must be a Galley: egui_dock re-wraps any other shape");
        };
        let WidgetText::Galley(short) = tab_title_text(&ctx, "Layers", font) else {
            panic!("the title must be a Galley");
        };
        disarm(&ctx);
        // What is DRAWN is the row's text; `Galley::text()` is the input.
        let drawn = cut.rows[0].text();
        assert!(cut.elided && drawn.ends_with('…'), "{drawn:?}");
        assert_eq!(cut.text(), long, "the full title survives for the hover");
        assert!(cut.size().x <= tab_overflow::TAB_TITLE_MAX + 0.5);
        assert!(!short.elided);
        assert_eq!(short.rows[0].text(), "Layers");
    }

    #[test]
    fn tab_widths_follow_egui_dock_layout() {
        // text + 2·8 padding + close, spacing after all but the last.
        assert_eq!(
            dock_tab_widths(&[50.0, 60.0, 10.0], 2.0, None, 24.0),
            [92.0, 102.0, 50.0]
        );
        // `minimum_width` is a floor, never a cap.
        assert_eq!(
            dock_tab_widths(&[10.0, 100.0], 0.0, Some(80.0), 0.0),
            [80.0, 116.0]
        );
        assert!(dock_tab_widths(&[], 2.0, None, 24.0).is_empty());
        // The bar loses the margin and the close-all + collapse buttons.
        assert_eq!(dock_bar_width(300.0, 4.0), 248.0);
        assert_eq!(dock_bar_width(30.0, 0.0), 0.0);
    }

    /// Every panel in one leaf: eleven tabs, far wider than 300 pt.
    fn crowded_dock(width: f32) -> DockState<Panel> {
        let mut dock = DockState::new(vec![
            Panel::Viewport,
            Panel::UvView,
            Panel::LayerStack,
            Panel::Properties,
            Panel::Assets,
            Panel::History,
            Panel::TextureSets,
            Panel::Bakes,
            Panel::Export,
            Panel::Graph,
            Panel::Display,
        ]);
        if width > 0.0 {
            let (_, leaf) = dock.iter_leaves_mut().next().expect("one leaf");
            leaf.rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 200.0));
        }
        dock
    }

    #[test]
    fn a_newly_active_tab_is_scrolled_into_the_strip_once() {
        let ctx = ctx();
        let font = egui::TextStyle::Button.resolve(&ctx.global_style());
        let style = Style::from_egui(&ctx.global_style());
        let mut seen = std::collections::HashMap::new();
        let mut dock = crowded_dock(300.0);

        // First pass: tab 0 is active and already visible — recorded,
        // nothing moves.
        assert!(!scroll_active_tabs_into_view(
            &mut dock, &mut seen, &ctx, &font, &style
        ));
        assert_eq!(seen.len(), 1);

        // Activate the LAST tab (what a click or a panel toggle does):
        // the strip scrolls to egui_dock's own clamp end, -(total - bar).
        {
            let (_, leaf) = dock.iter_leaves_mut().next().unwrap();
            leaf.set_active_tab(10).unwrap();
        }
        assert!(scroll_active_tabs_into_view(
            &mut dock, &mut seen, &ctx, &font, &style
        ));
        let (_, leaf) = dock.iter_leaves_mut().next().unwrap();
        let close = DOCK_BUTTON_SIZE.min(style.tab_bar.height);
        let text: Vec<f32> = leaf
            .tabs
            .iter()
            .map(|p| {
                tab_overflow::elided_title(
                    &ctx,
                    &panel_title(p),
                    font.clone(),
                    tab_overflow::TAB_TITLE_MAX,
                )
                .size()
                .x
            })
            .collect();
        let widths = dock_tab_widths(&text, style.tab.spacing, style.tab.minimum_width, close);
        let total: f32 = widths.iter().sum();
        let bar = dock_bar_width(300.0, style.tab_bar.inner_margin.sum().x);
        assert!(total > bar, "the fixture must overflow: {total} vs {bar}");
        assert!(
            (leaf.scroll - (bar - total)).abs() < 0.01,
            "{}",
            leaf.scroll
        );

        // The user scrolls back by hand: the SAME active tab never
        // fights it.
        leaf.scroll = 0.0;
        assert!(!scroll_active_tabs_into_view(
            &mut dock, &mut seen, &ctx, &font, &style
        ));
        let (_, leaf) = dock.iter_leaves_mut().next().unwrap();
        assert_eq!(leaf.scroll, 0.0);
        disarm(&ctx);
    }

    #[test]
    fn a_leaf_not_laid_out_yet_is_retried_later() {
        // Before the first `DockArea::show` the leaf rect is NOTHING: the
        // pass must not record it, or the first real frame would skip it.
        let ctx = ctx();
        let font = egui::TextStyle::Button.resolve(&ctx.global_style());
        let style = Style::from_egui(&ctx.global_style());
        let mut seen = std::collections::HashMap::new();
        let mut dock = crowded_dock(0.0);
        {
            let (_, leaf) = dock.iter_leaves_mut().next().unwrap();
            leaf.set_active_tab(10).unwrap();
        }
        assert!(!scroll_active_tabs_into_view(
            &mut dock, &mut seen, &ctx, &font, &style
        ));
        assert!(seen.is_empty());
        {
            let (_, leaf) = dock.iter_leaves_mut().next().unwrap();
            leaf.rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0));
        }
        assert!(scroll_active_tabs_into_view(
            &mut dock, &mut seen, &ctx, &font, &style
        ));
        let (_, leaf) = dock.iter_leaves_mut().next().unwrap();
        assert!(leaf.scroll < 0.0);
        disarm(&ctx);
    }
}
