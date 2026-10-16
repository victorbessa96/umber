//! The brush properties panel: live editing of the active [`BrushPreset`].
//!
//! Wave-4 item 3 (docs/specs/brush-presets-design.md, "The properties
//! panel data model"): the panel edits a [`BrushPreset`] live — engine
//! params bind directly to sliders, the two pressure curves bind to a
//! 4-point drag-the-control-points editor. Changes write back to the
//! preset file on explicit save only.
//!
//! GUI Phase 2 item 2 (docs/specs/gui-audit-and-roadmap.md §2, the
//! flagship-widget pass) repaints that editor: a custom-painted grid
//! backdrop, an accent-colored display spline through the 4 points,
//! raised/hover/drag handle states, click-to-insert, right-click-to-
//! delete, and a 120ms animated glide on a discrete position change.
//! The spline is cosmetic only — [`ControlCurve::evaluate`] stays the
//! piecewise-linear v1 brush-presets-design.md calls for ("linear
//! interpolation first... Substance's curves are also piecewise-linear
//! under the hood"), so the drawn curve and the value the engine
//! actually samples can disagree by a little between control points;
//! the cursor readout always shows `evaluate`'s real value, never the
//! spline's. [`ControlCurve`] itself is unchanged by this pass: it is
//! still fixed at exactly 4 points, so "insert"/"delete" are reinterpreted
//! for that frozen shape — see [`insert_point`] and [`delete_point`].
//!
//! [`BrushPanel::new`] assumes the working directory is the repo root in
//! dev builds (that is how the app runs today): it loads
//! `assets/brushes` as an extra search dir alongside the packaged path.
//! The packaged path override happens in a later slice.
//!
//! GUI Phase 0a/1 chrome (docs/specs/gui-audit-and-roadmap.md):
//! - The panel's sections render as [`umber_gui::ParamGroup`] collapsibles
//!   and every numeric slider is an [`umber_gui::ValueField`] (drag-scrub,
//!   wheel-step, tabular numerals) over the same range and backing
//!   state as the stock slider it replaced. Chrome only: no state struct,
//!   default, range, or save path changed, and the 18 tests below are
//!   untouched.
//!
//! Deliberately NOT migrated (v1 boundaries of this pass):
//! - The curve editor, its "Pressure → …" section headers, and the preset
//!   selector / save rows stay as-is (roadmap Phase 2 scope).
//! - The clone-offset [`egui::DragValue`]s are not sliders, so they stay.
//! - The preset editor's hardcoded English labels ("Alpha", "Min cutoff",
//!   …) stay hardcoded: converting them to `tr()` would change pt-BR
//!   rendering, which is a separate i18n kill. The `ftl` catalogs already
//!   carry matching keys; wiring them is deferred, not forgotten.
//! - No `umber_style`/fonts call at boot or panel level: `main.rs` applies
//!   no Umber style today (only a dock `Style::from_egui`), `assets/fonts`
//!   is not in the tree yet so `install_fonts` would panic (see
//!   `umber_gui::fonts`' `font-assets` follow-up), and a panel-local style
//!   would diverge from the other eleven panels before the migration wave
//!   reaches them. [`umber_gui::ValueField`] only needs
//!   `FontId::monospace`, which works on stock fonts.
//! - The kill brief named size/flow/opacity/jitter/angle params, but this
//!   panel's actual sliders are alpha, hardness, pressure-gamma, the
//!   stabilizer trio, the lazy-mouse pair, spacing density, the smudge
//!   pair, mirror offset, radial count, and angle snap. Every numeric
//!   slider present was migrated; nothing matching the brief's names
//!   exists in this file to migrate.
//! - Wheel-step sizes, drag speeds, decimals, and the "px"/"°" unit
//!   suffixes are display-chrome choices (literal symbols per
//!   [`umber_gui::ValueField`]'s own docs, not translatable words, so no
//!   `tr()` keys). The stored values keep full precision; snapping only
//!   applies while scrubbing/stepping, same as before.
//! - [`umber_gui::ParamGroup`]'s header count covers top-level
//!   [`umber_gui::ParamGroupUi::row`] calls; rows inside `add_enabled_ui`
//!   blocks (mirror/radial bodies) use manual label + field rows and do
//!   not count, so a fully-disabled section header can read "0".

use std::path::PathBuf;

use crate::i18n::tr;
use umber_gui::widgets::ParamGroupUi;
use umber_gui::{ParamGroup, ValueField};

use umber_brush::preset::{BrushPreset, ControlCurve};
use umber_brush::preset_library::{user_preset_dir, Library};
use umber_brush::symmetry::{RADIAL_COUNT_MAX, RADIAL_COUNT_MIN};
use umber_brush::{
    BrushTool, CloneParams, EraseParams, LineConstraintMode, MaskPaint, MirrorAxis, SmudgeParams,
    SymmetrySettings, WashMode, ANGLE_SNAP_DEFAULT_DEG, ANGLE_SNAP_MAX_DEG, ANGLE_SNAP_MIN_DEG,
};

/// `.abr` import (ROADMAP wave-abr). A submodule, so it ships with the
/// panel that uses it and needs no line in `main.rs`.
pub(crate) mod abr_import;

use crate::paint_state::PaintProjection;
use crate::paint_state::PaintState;

/// Size of the curve-editor plot (logical pixels).
const CURVE_PLOT_SIZE: egui::Vec2 = egui::Vec2::new(150.0, 100.0);
/// Radius of the draggable control-point handles (logical pixels).
const CURVE_HANDLE_RADIUS: f32 = 5.0;
/// Side length of each control point's (invisible) hit-box: bigger than
/// the drawn radius so a precise click is not required to grab a point.
const CURVE_HANDLE_HIT_SIZE: f32 = 14.0;
/// Minimum x gap between adjacent control points: the strict-monotonicity
/// invariant of [`ControlCurve`] in curve-editor units.
const CURVE_MIN_GAP: f32 = 0.01;
/// Minor-gridline divisions, each direction (§2: "4x4 minor").
const CURVE_GRID_DIVS: u32 = 4;
/// Samples drawn per segment for the cosmetic display spline
/// ([`spline_polyline`]).
const CURVE_SPLINE_SAMPLES: usize = 16;
/// Font size (logical pixels) of the cursor in/out readout.
const CURVE_LABEL_SIZE: f32 = 11.0;

/// The house motion-curve duration (docs/specs/gui-audit-and-roadmap.md
/// §2.4: "120ms ease-out as the house curve"). This mirrors the
/// in-flight `umber_gui::theme::MOTION_SECS` value exactly; it is
/// duplicated here rather than depended on because `umber-app` does not
/// wire the `umber-gui` crate yet (that crate's own Phase-0a landing is
/// separate, still uncommitted work) — this upgrade is scoped to stay
/// in-file, per the task that drove it, and collapses into the shared
/// constant once that wiring lands.
///
/// Only the DURATION matches the house curve here, not necessarily its
/// shape: the task names `animate_value_with_time` as the mechanism to
/// use, and that call interpolates linearly over this duration rather
/// than easing — an unverified detail (no cargo access to check egui's
/// rustdoc against this exact version), but worth flagging since the
/// roadmap's curve is specifically an ease-OUT. Shaping it would mean
/// not using `animate_value_with_time` for the interpolation itself
/// (e.g. driving it from a manually-tracked start time instead), which
/// this pass does not do.
const CURVE_MOTION_SECS: f32 = 0.12;

/// The curve-editor palette: local consts carrying the same hex values
/// as docs/specs/gui-audit-and-roadmap.md §2.1 (and the in-flight
/// `umber_gui::theme::palette`) — see [`CURVE_MOTION_SECS`] for why this
/// duplicates rather than imports.
mod curve_palette {
    use egui::Color32;

    /// Plot background: the darkest neutral, so the plot reads recessed
    /// against its parent panel.
    pub const BG: Color32 = Color32::from_rgb(0x16, 0x18, 0x1D);
    /// Minor gridlines (§2: "4x4 minor, the muted color").
    pub const GRID_MINOR: Color32 = Color32::from_rgb(0x26, 0x2A, 0x33);
    /// The outer frame: one step brighter than the minor grid.
    pub const GRID_BORDER: Color32 = Color32::from_rgb(0x33, 0x38, 0x42);
    /// The display spline: the ember-orange accent.
    pub const SPLINE: Color32 = Color32::from_rgb(0xFF, 0x7A, 0x1A);
    /// Idle control-point fill: the panel tone — a notch above [`BG`],
    /// distinct from both the grid lines and the hover state below.
    pub const POINT_IDLE: Color32 = Color32::from_rgb(0x1E, 0x21, 0x28);
    /// Hovered control-point fill: literally "the raised color" per the
    /// task (`RAISED`, same hex as [`GRID_MINOR`] — the handle's size and
    /// bezel ring keep it legible against the thin gridlines).
    pub const POINT_HOVER: Color32 = Color32::from_rgb(0x26, 0x2A, 0x33);
    /// Dragged/active control-point fill: "the accent", per the task.
    pub const POINT_DRAG: Color32 = Color32::from_rgb(0xFF, 0x7A, 0x1A);
    /// Control-point bezel ring: muted text, bright enough to read
    /// against every fill state above (including the drag state's own
    /// accent-colored fill).
    pub const POINT_STROKE: Color32 = Color32::from_rgb(0x9C, 0xA0, 0xAB);
    /// The cursor in/out readout text.
    pub const LABEL: Color32 = Color32::from_rgb(0x9C, 0xA0, 0xAB);
}

/// The properties panel state: the loaded shelf plus the edit session.
pub struct BrushPanel {
    /// The loaded preset shelf (sorted by name; the loader guarantees it).
    pub library: Library,
    /// Index into [`Library::entries`] of the preset being edited.
    pub active: Option<usize>,
    /// True once the active preset was edited since load/save.
    pub dirty: bool,
    /// Directories the library was loaded from (kept so Save-As can
    /// reload the same search path and the new entry appears).
    dirs: Vec<PathBuf>,
    /// Last save/save-as/reset failure, shown inline (never a panic).
    save_error: Option<String>,
    /// The last `.abr` import's report lines (brushes made, what did not
    /// carry over).
    abr_report: Vec<String>,
    /// The `.stamp.png` last loaded into the session for the active
    /// preset, so the tip is decoded once per selection, not per frame.
    loaded_stamp: Option<PathBuf>,
    /// Buffer for the Save-As name field.
    save_as_name: String,
    /// The active tool (paint / smudge / clone / eraser) and its params.
    /// Panel state, not preset state: the `.umberbrush` format is unchanged.
    /// The paint session adopts it via `PaintState::sync_tool`.
    pub tool: BrushTool,
    /// The tool the strip shows (C1 worklist #10, extended by #11). The
    /// eyedropper and the ID-mask select are strip-only: while `strip` is
    /// Pick or IdMask, [`Self::tool`] keeps the last real brush (its params
    /// stay editable) and the viewport suppresses painting
    /// (`Viewport::set_pick_armed` / `set_idmask_armed`) — neither strokes.
    pub strip: crate::toolbar::ToolKind,
    /// The last-edited params of each retouch tool, kept while another
    /// tool is selected so switching back restores them.
    smudge: SmudgeParams,
    clone: CloneParams,
    eraser: EraseParams,
    /// Mirror + radial symmetry. Panel (session) state like `tool`: not
    /// in the preset nor the project file in v1. The paint session adopts
    /// it via `PaintState::sync_symmetry`.
    pub symmetry: SymmetrySettings,
    /// Straight-line constraint + angle snap (§2 line 23). Panel (session)
    /// state like `symmetry`: not in the preset nor the project file in
    /// v1. The paint session adopts it via
    /// `PaintState::sync_line_constraint`.
    pub line_constraint: LineConstraintMode,
    /// Paint-what-you-see projection (§2's P1 row). Panel (session) state
    /// like `line_constraint`: not in the preset nor the project file in
    /// v1. The paint session adopts it via `PaintState::sync_projection`.
    pub projection: PaintProjection,
    /// Front-face falloff start angle in degrees, or `None` = off (the
    /// default — strokes unchanged). Panel state like `projection`; the
    /// shell syncs it to the viewport each frame
    /// (`Viewport::set_front_face_falloff_deg`). Applies to viewport
    /// (Screen) strokes only — a UV-view stroke has no view angle.
    pub front_face_falloff: Option<f32>,
    /// The active preset was picked or edited since the paint session last
    /// adopted it (`take_preset_changed`): the session then takes the
    /// preset's color and size too, not just its dab params (an
    /// eyedropper pick survives until the preset itself changes).
    preset_changed: bool,
}

impl BrushPanel {
    /// Load the dev search path: the user dir (when discoverable) plus
    /// the repo's `assets/brushes`. Assumes the working directory is the
    /// repo root (true for how the app runs today); nonexistent dirs skip
    /// silently per [`Library::load_from_dirs`].
    pub fn new() -> Self {
        let mut dirs = Vec::new();
        if let Some(user) = user_preset_dir() {
            dirs.push(user);
        }
        // The packaged install dir (`current_exe` parent + "brushes") is
        // covered by `Library::load_default_dirs`; in dev the exe lives
        // under target/, so name the repo dir explicitly instead.
        dirs.push(PathBuf::from("assets/brushes"));
        Self::with_dirs(dirs)
    }

    /// Load the shelf from an explicit priority-ordered search path
    /// (earlier dirs win). No preset is selected initially.
    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        let library = Library::load_from_dirs(&dirs);
        Self {
            library,
            active: None,
            dirty: false,
            dirs,
            save_error: None,
            abr_report: Vec::new(),
            loaded_stamp: None,
            strip: crate::toolbar::ToolKind::Paint,
            save_as_name: String::new(),
            tool: BrushTool::Paint,
            smudge: SmudgeParams::default(),
            clone: CloneParams::default(),
            eraser: EraseParams::default(),
            symmetry: SymmetrySettings::default(),
            line_constraint: LineConstraintMode::default(),
            projection: PaintProjection::default(),
            front_face_falloff: None,
            preset_changed: true,
        }
    }

    /// The panel body: tool rows then symmetry then quick mask then preset
    /// selector then live editor then save row. The tool rows come first:
    /// they work with no preset selected. The quick-mask rows edit the
    /// paint session's mask directly, so they show only when `paint` exists.
    pub fn show(&mut self, ui: &mut egui::Ui, paint: Option<&mut PaintState>) {
        // The chrome sections above the preset selector are ParamGroups
        // (their headers carry the separation, so no separators between
        // them); the preset selector / editor / save rows below keep
        // their existing separators — Phase 2 scope.
        self.tool_rows(ui);
        symmetry_rows(ui, &mut self.symmetry);
        line_constraint_rows(ui, &mut self.line_constraint);
        projection_rows(ui, &mut self.projection, &mut self.front_face_falloff);
        if let Some(paint) = paint {
            self.load_stamp_for_active(paint);
            channel_rows(ui, paint);
            quick_mask_rows(ui, paint);
            cavity_rows(ui, paint);
            stamp_image_rows(ui, paint);
        }
        ui.separator();
        self.preset_selector(ui);
        ui.add_space(8.0);
        ui.separator();
        let Some(idx) = self.active else {
            ui.centered_and_justified(|ui| {
                ui.label("No preset selected");
            });
            return;
        };
        if idx >= self.library.entries.len() {
            // The shelf reloaded out from under the selection (Save-As
            // restores `active` by name, but stay defensive anyway).
            self.active = None;
            self.dirty = false;
            return;
        }
        self.preset_editor(ui, idx);
        ui.add_space(8.0);
        ui.separator();
        self.save_row(ui, idx);
        if let Some(err) = self.save_error.clone() {
            ui.label(egui::RichText::new(err).color(ui.visuals().error_fg_color));
        }
    }

    /// A stamped preset carries its tip beside it as `<stem>.stamp.png`
    /// (what `.abr` import writes). Loads it into the session when the
    /// active preset changes to one that has it; a preset without one
    /// leaves the session's stamp alone (a hand-picked stamp stays).
    fn load_stamp_for_active(&mut self, paint: &mut PaintState) {
        let tip = self
            .active
            .and_then(|i| self.library.entries.get(i))
            .filter(|e| e.preset.params.stamped)
            .map(|e| e.source_path.with_extension("stamp.png"))
            .filter(|p| p.is_file());
        if tip == self.loaded_stamp {
            return;
        }
        if let Some(path) = &tip {
            match umber_export::png::read_png_rgba8(path) {
                Ok((w, h, rgba)) => {
                    if let Err(e) = paint.set_stamp(w, h, &stamp_coverage(&rgba)) {
                        log::warn!("stamp {} refused: {e}", path.display());
                    }
                }
                Err(e) => log::warn!("stamp {} unreadable: {e}", path.display()),
            }
        }
        self.loaded_stamp = tip;
    }

    /// The tool selector (paint / smudge / clone / eraser) plus the active
    /// retouch tool's param rows.
    fn tool_rows(&mut self, ui: &mut egui::Ui) {
        // Bank the active retouch params so a tool switch keeps them.
        match self.tool {
            BrushTool::Paint => {}
            BrushTool::Smudge(params) => self.smudge = params,
            BrushTool::Clone(params) => self.clone = params,
            BrushTool::Erase(params) => self.eraser = params,
        }
        let title = tr("brush.tool");
        ParamGroup::new("brush-tool", title.as_str()).show(ui, |group| {
            tool_selector_row(group, &mut self.tool, self.smudge, self.clone, self.eraser);
            match &mut self.tool {
                BrushTool::Paint => {}
                BrushTool::Smudge(params) => {
                    group.row(&tr("brush.smudge-strength"), |ui| {
                        ui.add(
                            ValueField::new(&mut params.strength)
                                .context_menu(crate::i18n::prop_menu_labels())
                                .range(0.0..=1.0),
                        )
                    });
                    group.row(&tr("brush.smudge-smear"), |ui| {
                        ui.add(
                            ValueField::new(&mut params.smear)
                                .context_menu(crate::i18n::prop_menu_labels())
                                .range(0.0..=1.0),
                        )
                    });
                }
                BrushTool::Clone(params) => {
                    group.ui.horizontal_wrapped(|ui| {
                        ui.label(tr("brush.clone-offset"));
                        ui.add(egui::DragValue::new(&mut params.source_offset.0).speed(0.001));
                        ui.add(egui::DragValue::new(&mut params.source_offset.1).speed(0.001));
                    });
                    group
                        .ui
                        .checkbox(&mut params.follow, tr("brush.clone-follow"));
                    group.ui.weak(tr("brush.clone-hint"));
                }
                BrushTool::Erase(params) => {
                    group.ui.horizontal_wrapped(|ui| {
                        ui.label(tr("brush.erase-channels"));
                        ui.checkbox(&mut params.channels.r, "R");
                        ui.checkbox(&mut params.channels.g, "G");
                        ui.checkbox(&mut params.channels.b, "B");
                        ui.checkbox(&mut params.channels.a, "A");
                    });
                    group.ui.weak(tr("brush.erase-hint"));
                }
            }
        });
    }

    /// The preset being edited, if one is selected (the paint session
    /// reads its wash mode through this).
    /// The live size and hardness flowing BACK from the paint session
    /// (`[` / `]`, Ctrl+RMB drag): written into the active preset so a Save
    /// keeps them, and marked dirty. A no-op when they already match, so
    /// the per-frame call costs nothing at rest.
    pub fn adopt_live_size(&mut self, size: f32, hardness: f32) {
        let Some(idx) = self.active else {
            return;
        };
        let Some(entry) = self.library.entries.get_mut(idx) else {
            return;
        };
        let p = &mut entry.preset;
        if (p.size - size).abs() > 1e-6 || (p.params.hardness - hardness).abs() > 1e-6 {
            p.size = size;
            p.params.hardness = hardness;
            self.dirty = true;
        }
    }

    /// Whether the active preset was picked or edited since the last call
    /// (and clears the flag): the paint session's cue to adopt the
    /// preset's color and size, not only its dab params.
    pub fn take_preset_changed(&mut self) -> bool {
        std::mem::take(&mut self.preset_changed)
    }

    pub fn active_preset(&self) -> Option<&BrushPreset> {
        self.active
            .and_then(|i| self.library.entries.get(i))
            .map(|entry| &entry.preset)
    }

    /// The toolbar strip and the E shortcut (`ShellAction::SelectTool`):
    /// banks the current retouch params, then selects `kind` with its
    /// banked params restored. A no-op when `kind` is already selected.
    /// (Generalizes the former `select_eraser`, which E called.)
    pub fn select_tool(&mut self, kind: crate::toolbar::ToolKind) {
        use crate::toolbar::ToolKind;
        // The strip always follows the selection (Pick and IdMask included:
        // strip states, not brush states).
        self.strip = kind;
        if ToolKind::of(&self.tool) == kind || matches!(kind, ToolKind::Pick | ToolKind::IdMask) {
            // Pick / IdMask: the strip shows the tool, the brush stays the
            // last real one (the viewport suppresses painting while either
            // is the strip tool).
            // Same-kind: nothing to bank or switch.
            return;
        }
        // Bank first: an edit made this frame hasn't been banked by
        // `tool_rows` yet when a shortcut fires before the panel draws.
        match self.tool {
            BrushTool::Paint => {}
            BrushTool::Smudge(params) => self.smudge = params,
            BrushTool::Clone(params) => self.clone = params,
            BrushTool::Erase(params) => self.eraser = params,
        }
        self.tool = match kind {
            ToolKind::Paint => BrushTool::Paint,
            ToolKind::Smudge => BrushTool::Smudge(self.smudge),
            ToolKind::Clone => BrushTool::Clone(self.clone),
            ToolKind::Erase => BrushTool::Erase(self.eraser),
            // Unreachable: the Pick/IdMask early-return above exits before
            // the match. Kept (not `_`) so a future ToolKind still fails the
            // exhaustiveness check here.
            ToolKind::Pick | ToolKind::IdMask => return,
        };
    }

    /// Preset combo + broken-file badge + overridden count.
    fn preset_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Preset");
            let current = self
                .active
                .and_then(|i| self.library.entries.get(i))
                .map_or_else(|| "None".to_owned(), |e| e.preset.name.clone());
            let mut picked = self.active;
            egui::ComboBox::from_id_salt("brush_preset_combo")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (i, entry) in self.library.entries.iter().enumerate() {
                        ui.selectable_value(&mut picked, Some(i), &entry.preset.name);
                    }
                });
            if picked != self.active {
                self.active = picked;
                self.preset_changed = true;
                self.dirty = false;
                self.save_error = None;
                // Prefill Save-As with the selected name for "duplicate
                // then rename" flows.
                self.save_as_name = self
                    .active
                    .and_then(|i| self.library.entries.get(i))
                    .map_or_else(String::new, |e| e.preset.name.clone());
            }
        });
        ui.horizontal_wrapped(|ui| {
            if !self.library.errors.is_empty() {
                let n = self.library.errors.len();
                ui.collapsing(format!("{n} broken"), |ui| {
                    for (path, err) in &self.library.errors {
                        ui.label(format!("{}: {err}", path.display()));
                    }
                });
            }
            if self.library.overridden > 0 {
                ui.weak(format!("{} overridden", self.library.overridden));
            }
        });
    }

    /// Live editor for every field of the active preset. Every change
    /// writes through to the preset struct and sets `dirty`.
    fn preset_editor(&mut self, ui: &mut egui::Ui, idx: usize) {
        let preset = &mut self.library.entries[idx].preset;
        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            changed |= ui
                .add(egui::TextEdit::singleline(&mut preset.name))
                .changed();
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Color");
            let mut rgb = [
                preset.params.color[0],
                preset.params.color[1],
                preset.params.color[2],
            ];
            if umber_gui::widgets::color_picker::color_button(ui, &mut rgb).changed() {
                preset.params.color[0] = rgb[0];
                preset.params.color[1] = rgb[1];
                preset.params.color[2] = rgb[2];
                changed = true;
            }
        });

        changed |= value_row(
            ui,
            "Alpha",
            ValueField::new(&mut preset.params.alpha)
                .context_menu(crate::i18n::prop_menu_labels())
                .range(0.0..=1.0),
        )
        .changed();
        changed |= value_row(
            ui,
            "Hardness",
            ValueField::new(&mut preset.params.hardness)
                .context_menu(crate::i18n::prop_menu_labels())
                .range(0.0..=1.0),
        )
        .changed();
        // Gamma > 1 favors light pressure; the 0.05 floor avoids
        // div-by-zero downstream.
        changed |= value_row(
            ui,
            "Pressure gamma",
            ValueField::new(&mut preset.params.pressure_gamma)
                .context_menu(crate::i18n::prop_menu_labels())
                .range(0.05..=4.0)
                .step(0.05),
        )
        .changed();

        // Opacity accumulation: build-up lets overlapping stamps pile
        // past the stroke opacity; wash normalizes the overlap to it
        // (Krita wash mode via MyPaint's opaque_linearize).
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("brush.wash-mode"));
            changed |= ui
                .selectable_value(
                    &mut preset.params.wash_mode,
                    WashMode::Wash,
                    tr("brush.wash"),
                )
                .changed();
            changed |= ui
                .selectable_value(
                    &mut preset.params.wash_mode,
                    WashMode::BuildUp,
                    tr("brush.build-up"),
                )
                .changed();
        });

        // Bitmap stamp + angle + jitter (C2-11): the preset says THAT it
        // is stamped and how stamps turn and vary; the stamp image itself
        // is session state (the Stamp image row).
        changed |= stamp_rows(ui, &mut preset.params);

        // Falloff curve (absorbed from Blender's brush falloff presets):
        // the dab's radial profile, applied past the hardness core.
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("brush.falloff"));
            let current = preset.params.falloff;
            egui::ComboBox::from_id_salt("brush_falloff_combo")
                .selected_text(tr(falloff_key(current)))
                .show_ui(ui, |ui| {
                    for curve in umber_brush::FalloffCurve::ALL {
                        changed |= ui
                            .selectable_value(
                                &mut preset.params.falloff,
                                curve,
                                tr(falloff_key(curve)),
                            )
                            .changed();
                    }
                });
        });

        changed |= value_row(
            ui,
            &tr("brush.size"),
            ValueField::new(&mut preset.size)
                .context_menu(crate::i18n::prop_menu_labels())
                .range(0.05..=8.0)
                .step(0.01),
        )
        .changed();
        changed |= stroke_method_rows(ui, &mut preset.stroke_method);

        ui.add_space(4.0);
        ParamGroup::new("brush-stabilizer", "Stabilizer (one-euro)").show(ui, |group| {
            changed |= group
                .row("Min cutoff", |ui| {
                    ui.add(
                        ValueField::new(&mut preset.one_euro.min_cutoff)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(0.1..=10.0)
                            .step(0.1),
                    )
                })
                .changed();
            changed |= group
                .row("Beta", |ui| {
                    ui.add(
                        ValueField::new(&mut preset.one_euro.beta)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(0.0..=0.1)
                            .step(0.005)
                            .decimals(3),
                    )
                })
                .changed();
            changed |= group
                .row("d cutoff", |ui| {
                    ui.add(
                        ValueField::new(&mut preset.one_euro.d_cutoff)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(0.1..=10.0)
                            .step(0.1),
                    )
                })
                .changed();
        });

        ui.add_space(4.0);
        ParamGroup::new("brush-lazy-mouse", "Lazy mouse").show(ui, |group| {
            changed |= group
                .row("Radius px", |ui| {
                    ui.add(
                        ValueField::new(&mut preset.lazy_mouse.radius_px)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(0.0..=64.0)
                            .step(0.5)
                            .decimals(1)
                            .unit("px"),
                    )
                })
                .changed();
            changed |= group
                .row("Strength", |ui| {
                    ui.add(
                        ValueField::new(&mut preset.lazy_mouse.strength)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(0.0..=1.0),
                    )
                })
                .changed();
        });

        ui.add_space(4.0);
        ParamGroup::new("brush-spacing", "Spacing").show(ui, |group| {
            changed |= group
                .row("Dabs per radius", |ui| {
                    ui.add(
                        ValueField::new(&mut preset.spacing.dabs_per_radius)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(1.0..=64.0)
                            .step(0.5)
                            .decimals(1),
                    )
                })
                .changed();
        });

        ui.add_space(4.0);
        ui.strong("Pressure → alpha");
        changed |= curve_editor(ui, &mut preset.alpha_curve);
        ui.add_space(4.0);
        ui.strong("Pressure → radius");
        changed |= curve_editor(ui, &mut preset.radius_curve);

        if changed {
            self.dirty = true;
            self.preset_changed = true;
        }
    }

    /// Save (to the entry's source file) / Save-As (to the user dir) /
    /// Reset (reload from the source file).
    fn save_row(&mut self, ui: &mut egui::Ui, _idx: usize) {
        ui.horizontal_wrapped(|ui| {
            let save = ui.add_enabled(self.dirty, egui::Button::new(tr("button.save")));
            if save.clicked() {
                self.save_active();
            }
            let reset = ui.add_enabled(self.dirty, egui::Button::new(tr("button.reset")));
            if reset.clicked() {
                self.reset_active();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Save as");
            ui.add(
                egui::TextEdit::singleline(&mut self.save_as_name)
                    .hint_text("New preset name")
                    .desired_width(140.0),
            );
            if ui.button(tr("button.save-as")).clicked() {
                self.save_as();
            }
        });
        // Labels are English until the catalogs' lock frees (queue).
        ui.horizontal_wrapped(|ui| {
            if ui.button("Import .abr…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Photoshop brushes", &["abr"])
                    .pick_file()
                {
                    self.import_abr(&path);
                }
            }
        });
        for line in &self.abr_report {
            ui.weak(line);
        }
    }

    /// Imports every brush in an `.abr` into the user preset dir as
    /// `.umberbrush` files (plus `.stamp.png` tips), then reloads the shelf
    /// and selects the first imported brush. A stamped preset's tip loads
    /// into the session when it is selected (`load_stamp_for_active`).
    fn import_abr(&mut self, path: &std::path::Path) {
        self.abr_report.clear();
        let import = match abr_import::import_abr_file(path) {
            Ok(import) => import,
            Err(err) => {
                self.save_error = Some(format!("Import failed: {err}"));
                return;
            }
        };
        let Some(user_dir) = user_preset_dir() else {
            self.save_error = Some("Import failed: no user preset dir".to_owned());
            return;
        };
        let written = match abr_import::write_brushes(&import, &user_dir) {
            Ok(written) => written,
            Err(err) => {
                self.save_error = Some(format!("Import failed: {err}"));
                return;
            }
        };
        // Per-brush "not carried over" lines surface in the panel's report
        // too (the report on the pack, the field per brush).
        let per_brush: Vec<String> = import
            .brushes
            .iter()
            .filter(|b| !b.dropped.is_empty())
            .map(|b| {
                format!(
                    "{}: not carried over: {}",
                    b.preset.name,
                    b.dropped.join(", ")
                )
            })
            .collect();
        let mut report = import.report.clone();
        report.extend(per_brush);
        self.abr_report = report;
        self.abr_report.insert(
            0,
            format!(
                "{}: {} brush(es) imported",
                path.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                written.len()
            ),
        );
        if !self.dirs.contains(&user_dir) {
            let mut dirs = Vec::with_capacity(self.dirs.len() + 1);
            dirs.push(user_dir);
            dirs.extend(self.dirs.iter().cloned());
            self.dirs = dirs;
        }
        self.library = Library::load_from_dirs(&self.dirs);
        let first = written.first().cloned();
        self.active =
            first.and_then(|p| self.library.entries.iter().position(|e| e.source_path == p));
        self.preset_changed = true;
        self.dirty = false;
        self.save_error = None;
    }

    /// Serialize the active preset back to its source file. IO/serialize
    /// failures surface inline via `save_error`; never a panic. Clears
    /// `dirty` on success.
    fn save_active(&mut self) {
        let Some(idx) = self.active else {
            return;
        };
        if idx >= self.library.entries.len() {
            return;
        }
        let entry = &self.library.entries[idx];
        let json = match entry.preset.to_json() {
            Ok(json) => json,
            Err(err) => {
                self.save_error = Some(format!("Save failed: {err}"));
                return;
            }
        };
        match std::fs::write(&entry.source_path, json) {
            Ok(()) => {
                self.dirty = false;
                self.save_error = None;
            }
            Err(err) => {
                self.save_error = Some(format!("Save failed: {err}"));
            }
        }
    }

    /// Write the active preset under a new name into the user library
    /// dir (created when missing), then reload the shelf so the new
    /// entry appears and select it by name lookup.
    fn save_as(&mut self) {
        let name = self.save_as_name.trim().to_owned();
        if name.is_empty() {
            self.save_error = Some("Save As needs a name".to_owned());
            return;
        }
        let Some(idx) = self.active else {
            self.save_error = Some("Save As needs an active preset".to_owned());
            return;
        };
        if idx >= self.library.entries.len() {
            return;
        }
        let Some(user_dir) = user_preset_dir() else {
            self.save_error = Some("Save As failed: no user preset dir".to_owned());
            return;
        };
        if let Err(err) = std::fs::create_dir_all(&user_dir) {
            self.save_error = Some(format!("Save As failed: {err}"));
            return;
        }
        let mut preset = self.library.entries[idx].preset.clone();
        preset.name = name.clone();
        let json = match preset.to_json() {
            Ok(json) => json,
            Err(err) => {
                self.save_error = Some(format!("Save As failed: {err}"));
                return;
            }
        };
        let path = user_dir.join(format!("{}.umberbrush", sanitize_file_stem(&name)));
        if let Err(err) = std::fs::write(&path, json) {
            self.save_error = Some(format!("Save As failed: {err}"));
            return;
        }
        // Reload the same search path (plus the user dir, in case this
        // panel was built from dirs that omit it) so the new entry
        // appears, then select it by name.
        if !self.dirs.contains(&user_dir) {
            let mut dirs = Vec::with_capacity(self.dirs.len() + 1);
            dirs.push(user_dir);
            dirs.extend(self.dirs.iter().cloned());
            self.dirs = dirs;
        }
        self.library = Library::load_from_dirs(&self.dirs);
        self.active = self
            .library
            .entries
            .iter()
            .position(|e| e.preset.name == name);
        self.dirty = false;
        self.save_error = None;
    }

    /// Discard edits by reloading the active preset from its source file.
    fn reset_active(&mut self) {
        let Some(idx) = self.active else {
            return;
        };
        if idx >= self.library.entries.len() {
            return;
        }
        let path = self.library.entries[idx].source_path.clone();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                self.save_error = Some(format!("Reset failed: {err}"));
                return;
            }
        };
        match BrushPreset::from_json(&text).and_then(|preset| {
            preset.validate()?;
            Ok(preset)
        }) {
            Ok(preset) => {
                self.library.entries[idx].preset = preset;
                self.dirty = false;
                self.save_error = None;
            }
            Err(err) => {
                self.save_error = Some(format!("Reset failed: {err}"));
            }
        }
    }
}

impl Default for BrushPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// The tool selector radio row (paint / smudge / clone / eraser), split
/// out of [`BrushPanel::tool_rows`] so that method stays a chrome wrapper:
/// the banked params arrive by value and the picked tool writes back
/// through `tool`. Behavior is the original code verbatim.
fn tool_selector_row(
    group: &mut ParamGroupUi<'_>,
    tool: &mut BrushTool,
    smudge: SmudgeParams,
    clone: CloneParams,
    eraser: EraseParams,
) {
    group.ui.horizontal_wrapped(|ui| {
        let mut picked = *tool;
        ui.selectable_value(&mut picked, BrushTool::Paint, tr("brush.tool-paint"));
        let smudge_on = matches!(picked, BrushTool::Smudge(_));
        if ui
            .selectable_label(smudge_on, tr("brush.tool-smudge"))
            .clicked()
        {
            picked = BrushTool::Smudge(smudge);
        }
        let clone_on = matches!(picked, BrushTool::Clone(_));
        if ui
            .selectable_label(clone_on, tr("brush.tool-clone"))
            .clicked()
        {
            picked = BrushTool::Clone(clone);
        }
        let eraser_on = matches!(picked, BrushTool::Erase(_));
        if ui
            .selectable_label(eraser_on, tr("brush.tool-eraser"))
            .clicked()
        {
            picked = BrushTool::Erase(eraser);
        }
        *tool = picked;
    });
}

/// The quick-mask section (§2's P1 row, `umber_brush::quick_mask`): mask
/// mode (also X), gating, what mask strokes paint, Invert (also I in mask
/// mode) and Clear. It edits the paint session's mask in place. The mask
/// is session state, kept out of the panel, the preset, the project file
/// and the undo history.
fn quick_mask_rows(ui: &mut egui::Ui, paint: &mut PaintState) {
    let title = tr("brush.quick-mask");
    ParamGroup::new("brush-quick-mask", title.as_str()).show(ui, |group| {
        quick_mask_body(group, paint);
    });
}

/// The Cavity Mask section (Blender's texture-paint Cavity Mask panel,
/// `tool_settings/index.html`): paint only where the baked curvature says
/// crevice (or ridge). It edits the paint session directly, like the quick
/// mask: it depends on the mesh's curvature bake, not on the brush preset.
/// The channels the picker offers, in order: the default texture set's
/// OpenPBR channels (`TextureSet::new_default`, the set every new project
/// starts from), then any extra channel the paint session already holds
/// on tile 1001 (an imported user channel), deduplicated.
pub(crate) fn pickable_channels(present: &[String]) -> Vec<String> {
    let mut out: Vec<String> = umber_core::TextureSet::new_default("")
        .channels
        .into_iter()
        .map(|c| c.name)
        .collect();
    for name in present {
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    out
}

/// The catalog key of a channel's display name; `None` for a user channel
/// (shown by its own name).
pub(crate) fn channel_label_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "baseColor" => "viewmode.basecolor",
        "roughness" => "viewmode.roughness",
        "metallic" => "viewmode.metallic",
        "normal" => "viewmode.normal",
        "height" => "viewmode.height",
        "emissive" => "viewmode.emissive",
        "ambientOcclusion" | "ao" => "viewmode.ao",
        _ => return None,
    })
}

fn channel_label(name: &str) -> String {
    channel_label_key(name).map_or_else(|| name.to_owned(), tr)
}

/// The paint-channel picker (C2-5, `docs/loop/audit-2026-10-15-c2.md`):
/// which channel the next stroke paints. The paint session snapshots it
/// at stroke begin, so a change mid-stroke takes effect on the next one.
fn channel_rows(ui: &mut egui::Ui, paint: &mut PaintState) {
    // The View menu's existing "Channel" key: both catalogs sit under a
    // sibling session's lock right now, so this kill adds no new strings.
    let title = tr("viewmode.channel");
    ParamGroup::new("brush-channel", title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        let active = paint.active_channel().to_owned();
        let options = pickable_channels(&paint.channels_present(umber_mesh::FIRST_TILE));
        egui::ComboBox::from_id_salt("brush_channel_combo")
            .selected_text(channel_label(&active))
            .show_ui(ui, |ui| {
                for name in &options {
                    if ui
                        .selectable_label(*name == active, channel_label(name))
                        .clicked()
                        && *name != active
                    {
                        paint.set_active_channel(name);
                    }
                }
            });
    });
}

/// The preset's stamp switch, angle mode and the jitter quartet (C2-11).
/// Returns whether anything changed.
fn stamp_rows(ui: &mut egui::Ui, params: &mut umber_brush::BrushParams) -> bool {
    use umber_brush::StampAngle as A;
    let mut changed = ui
        .checkbox(&mut params.stamped, tr("brush.stamped"))
        .changed();
    if params.stamped {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("brush.stamp-angle"));
            let key = |a: &A| match a {
                A::None => "brush.angle-none",
                A::Fixed { .. } => "brush.angle-fixed",
                A::FollowStroke { .. } => "brush.angle-follow",
                A::Random => "brush.angle-random",
            };
            egui::ComboBox::from_id_salt("brush_stamp_angle")
                .selected_text(tr(key(&params.stamp_angle)))
                .show_ui(ui, |ui| {
                    for mode in [
                        A::None,
                        A::Fixed { degrees: 0.0 },
                        A::FollowStroke {
                            offset_degrees: 0.0,
                        },
                        A::Random,
                    ] {
                        let same = std::mem::discriminant(&mode)
                            == std::mem::discriminant(&params.stamp_angle);
                        if ui.selectable_label(same, tr(key(&mode))).clicked() && !same {
                            params.stamp_angle = mode;
                            changed = true;
                        }
                    }
                });
        });
        match &mut params.stamp_angle {
            A::Fixed { degrees } => {
                changed |= value_row(
                    ui,
                    &tr("brush.angle-degrees"),
                    ValueField::new(degrees)
                        .context_menu(crate::i18n::prop_menu_labels())
                        .range(-180.0..=180.0)
                        .unit("°"),
                )
                .changed();
            }
            A::FollowStroke { offset_degrees } => {
                changed |= value_row(
                    ui,
                    &tr("brush.angle-offset"),
                    ValueField::new(offset_degrees)
                        .context_menu(crate::i18n::prop_menu_labels())
                        .range(-180.0..=180.0)
                        .unit("°"),
                )
                .changed();
            }
            A::None | A::Random => {}
        }
    }
    let j = &mut params.jitter;
    for (key, value, max, suffix) in [
        ("brush.jitter-size", &mut j.size, 1.0, ""),
        ("brush.jitter-angle", &mut j.angle_degrees, 180.0, "°"),
        ("brush.jitter-position", &mut j.position, 2.0, ""),
        ("brush.jitter-flow", &mut j.flow, 1.0, ""),
    ] {
        changed |= value_row(
            ui,
            &tr(key),
            ValueField::new(value)
                .context_menu(crate::i18n::prop_menu_labels())
                .range(0.0..=max)
                .default_value(0.0)
                .unit(suffix),
        )
        .changed();
    }
    changed
}

/// The session's stamp image (C2-11): load a PNG (its luminance becomes
/// the R8 coverage, alpha-weighted) or clear back to the full square.
fn stamp_image_rows(ui: &mut egui::Ui, paint: &mut PaintState) {
    let title = tr("brush.stamp-image");
    ParamGroup::new("brush-stamp", title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        let (w, h) = paint.stamp_size();
        ui.horizontal_wrapped(|ui| {
            if (w, h) == (1, 1) {
                ui.weak(tr("brush.stamp-none"));
            } else {
                ui.weak(format!("{w} × {h}"));
            }
            if ui.button(tr("button.choose")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .pick_file()
                {
                    match umber_export::png::read_png_rgba8(&path) {
                        Ok((w, h, rgba)) => {
                            if let Err(e) = paint.set_stamp(w, h, &stamp_coverage(&rgba)) {
                                log::warn!("stamp {} refused: {e}", path.display());
                            }
                        }
                        Err(e) => log::warn!("stamp {} unreadable: {e}", path.display()),
                    }
                }
            }
            if (w, h) != (1, 1) && ui.button(tr("button.reset")).clicked() {
                paint.clear_stamp();
            }
        });
    });
}

/// An RGBA8 image as stamp coverage: Rec. 709 luma times alpha, so a white
/// shape on transparency and a white shape on black both stamp the shape.
pub(crate) fn stamp_coverage(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .map(|px| {
            let luma =
                0.2126 * f32::from(px[0]) + 0.7152 * f32::from(px[1]) + 0.0722 * f32::from(px[2]);
            (luma * f32::from(px[3]) / 255.0).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

fn cavity_rows(ui: &mut egui::Ui, paint: &mut PaintState) {
    use umber_brush::{CavityMask, CavityTarget};
    let title = tr("brush.cavity");
    ParamGroup::new("brush-cavity", title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        let current = paint.cavity_mask();
        let mut on = current.is_some();
        let mut mask: CavityMask = current.unwrap_or_default();
        let mut changed = ui.checkbox(&mut on, tr("brush.cavity-enable")).changed();
        if on {
            ui.horizontal_wrapped(|ui| {
                changed |= ui
                    .selectable_value(
                        &mut mask.target,
                        CavityTarget::Cavities,
                        tr("brush.cavity-cavities"),
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut mask.target,
                        CavityTarget::Ridges,
                        tr("brush.cavity-ridges"),
                    )
                    .changed();
            });
            changed |= value_row(
                ui,
                &tr("brush.cavity-threshold"),
                ValueField::new(&mut mask.threshold)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
            )
            .changed();
            changed |= value_row(
                ui,
                &tr("brush.cavity-width"),
                ValueField::new(&mut mask.width)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.01..=1.0),
            )
            .changed();
            changed |= ui
                .checkbox(&mut mask.invert, tr("brush.cavity-invert"))
                .changed();
            if !paint.has_curvature_map(paint.active_tile()) {
                ui.label(
                    egui::RichText::new(tr("brush.cavity-no-map"))
                        .small()
                        .color(umber_gui::muted()),
                );
            }
        }
        if changed {
            paint.set_cavity_mask(on.then_some(mask));
        }
    });
}

/// The stroke-method rows (Blender's Stroke Method, `stroke.html`): a
/// combo, then only the chosen method's own parameters. Returns whether
/// anything changed. Every value is kept inside the method's own valid
/// domain, so the conditioner never refuses what the panel wrote.
fn stroke_method_rows(ui: &mut egui::Ui, method: &mut umber_brush::StrokeMethod) -> bool {
    use umber_brush::StrokeMethod as M;
    let mut changed = false;
    let kind_key = |m: &M| match m {
        M::Space => "brush.method-space",
        M::Airbrush { .. } => "brush.method-airbrush",
        M::Dash { .. } => "brush.method-dash",
        M::DragDot => "brush.method-drag-dot",
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("brush.method"));
        egui::ComboBox::from_id_salt("brush_method_combo")
            .selected_text(tr(kind_key(method)))
            .show_ui(ui, |ui| {
                for candidate in [
                    M::Space,
                    M::Airbrush { rate_hz: 10.0 },
                    M::Dash {
                        ratio: 0.5,
                        length_px: 20.0,
                    },
                    M::DragDot,
                ] {
                    let same = std::mem::discriminant(method) == std::mem::discriminant(&candidate);
                    if ui
                        .selectable_label(same, tr(kind_key(&candidate)))
                        .clicked()
                        && !same
                    {
                        *method = candidate;
                        changed = true;
                    }
                }
            });
    });
    match method {
        M::Airbrush { rate_hz } => {
            changed |= value_row(
                ui,
                &tr("brush.method-rate"),
                ValueField::new(rate_hz)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(1.0..=60.0)
                    .unit("Hz"),
            )
            .changed();
        }
        M::Dash { ratio, length_px } => {
            changed |= value_row(
                ui,
                &tr("brush.method-dash-ratio"),
                ValueField::new(ratio)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.05..=1.0),
            )
            .changed();
            changed |= value_row(
                ui,
                &tr("brush.method-dash-length"),
                ValueField::new(length_px)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(1.0..=512.0)
                    .unit("px"),
            )
            .changed();
        }
        M::Space | M::DragDot => {}
    }
    changed
}

/// The quick-mask body: the group-level widgets (kept verbatim, only the
/// `ui` they draw on changed).
fn quick_mask_body(group: &mut ParamGroupUi<'_>, paint: &mut PaintState) {
    let ui: &mut egui::Ui = &mut *group.ui;
    let mask = paint.quick_mask();
    let (mut editing, mut active) = (mask.is_editing(), mask.is_active());
    let (current, inverted) = (mask.paint(), mask.is_inverted());
    if ui
        .checkbox(&mut editing, tr("brush.quick-mask-mode"))
        .changed()
    {
        paint.set_mask_mode(editing);
    }
    if ui
        .checkbox(&mut active, tr("brush.quick-mask-active"))
        .changed()
    {
        paint.set_mask_active(active);
    }
    let mut picked = current;
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("brush.quick-mask-paint"));
        ui.selectable_value(&mut picked, MaskPaint::Allow, tr("brush.quick-mask-allow"));
        ui.selectable_value(&mut picked, MaskPaint::Block, tr("brush.quick-mask-block"));
    });
    if picked != current {
        paint.set_mask_paint(picked);
    }
    ui.horizontal(|ui| {
        if ui.button(tr("brush.quick-mask-invert")).clicked() {
            paint.invert_mask();
        }
        if ui.button(tr("brush.quick-mask-clear")).clicked() {
            paint.clear_mask();
        }
        if inverted {
            ui.weak(tr("brush.quick-mask-inverted"));
        }
    });
    ui.weak(tr("brush.quick-mask-hint"));
}

/// A muted-label + [`ValueField`] row for bodies that draw on a plain
/// `&mut Ui` (inside `add_enabled_ui` blocks, or the ungrouped preset
/// rows), where [`ParamGroupUi::row`] is not reachable. Same grammar,
/// uncounted — see the module docs. Returns the field's [`egui::Response`]
/// so callers keep their `.changed()` dirty-tracking verbatim.
fn value_row<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    field: ValueField<'_, N>,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(umber_gui::muted()));
        ui.add(field)
    })
    .inner
}

/// The symmetry section: the mirror toggle with its axis + offset rows,
/// the radial toggle with its count + flip rows. Applies to every tool
/// (paint, smudge, clone, eraser).
fn symmetry_rows(ui: &mut egui::Ui, symmetry: &mut SymmetrySettings) {
    let title = tr("brush.symmetry");
    ParamGroup::new("brush-symmetry", title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        ui.checkbox(&mut symmetry.mirror.enabled, tr("brush.symmetry-mirror"));
        ui.add_enabled_ui(symmetry.mirror.enabled, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("brush.symmetry-axis"));
                ui.selectable_value(
                    &mut symmetry.mirror.axis,
                    MirrorAxis::X,
                    tr("brush.symmetry-axis-x"),
                );
                ui.selectable_value(
                    &mut symmetry.mirror.axis,
                    MirrorAxis::Y,
                    tr("brush.symmetry-axis-y"),
                );
            });
            value_row(
                ui,
                &tr("brush.symmetry-offset"),
                ValueField::new(&mut symmetry.mirror.offset)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
            );
            ui.weak(tr("brush.symmetry-hint"));
        });
        let radial = &mut symmetry.radial;
        ui.checkbox(&mut radial.enabled, tr("brush.symmetry-radial"));
        ui.add_enabled_ui(radial.enabled, |ui| {
            value_row(
                ui,
                &tr("brush.symmetry-count"),
                ValueField::new(&mut radial.count)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(RADIAL_COUNT_MIN..=RADIAL_COUNT_MAX),
            );
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut radial.flip_u, tr("brush.symmetry-flip-u"));
                ui.checkbox(&mut radial.flip_v, tr("brush.symmetry-flip-v"));
            });
        });
    });
}

/// The straight-line constraint section (§2 line 23,
/// `umber_brush::line_constraint`): off, the Shift-like anchored line, or
/// the anchored line with angle snap (configurable step, 15° default).
/// Applies to every tool (paint, smudge, clone, eraser).
fn line_constraint_rows(ui: &mut egui::Ui, mode: &mut LineConstraintMode) {
    let title = tr("brush.line-constraint");
    ParamGroup::new("brush-line-constraint", title.as_str()).show(ui, |group| {
        group.ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut *mode,
                LineConstraintMode::Off,
                tr("brush.line-constraint-off"),
            );
            ui.selectable_value(
                &mut *mode,
                LineConstraintMode::Line,
                tr("brush.line-constraint-line"),
            );
            // AngleSnap carries the step, so it gets a manual radio that arms
            // the default step; the field below edits it in place.
            let snap_on = matches!(mode, LineConstraintMode::AngleSnap { .. });
            if ui
                .selectable_label(snap_on, tr("brush.line-constraint-snap"))
                .clicked()
            {
                *mode = LineConstraintMode::AngleSnap {
                    snap_deg: ANGLE_SNAP_DEFAULT_DEG,
                };
            }
        });
        if let LineConstraintMode::AngleSnap { snap_deg } = mode {
            // Any finite positive step is usable downstream
            // (`sanitize_snap_deg`), so the wheel step is half a degree.
            group.row(&tr("brush.line-constraint-angle"), |ui| {
                ui.add(
                    ValueField::new(snap_deg)
                        .context_menu(crate::i18n::prop_menu_labels())
                        .range(ANGLE_SNAP_MIN_DEG..=ANGLE_SNAP_MAX_DEG)
                        .step(0.5)
                        .decimals(1)
                        .unit("°"),
                )
            });
        }
        group.ui.weak(tr("brush.line-constraint-hint"));
    });
}

/// The paint-what-you-see projection section (§2's P1 row): 3D
/// (Screen — viewport strokes raycast to the mesh) or UV-only (the 3D
/// viewport orbits). Applies to every tool (paint, smudge, clone, eraser).
fn projection_rows(
    ui: &mut egui::Ui,
    mode: &mut PaintProjection,
    front_face_falloff: &mut Option<f32>,
) {
    let title = tr("brush.projection");
    ParamGroup::new("brush-projection", title.as_str()).show(ui, |group| {
        group.ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut *mode,
                PaintProjection::Screen,
                tr("brush.projection-screen"),
            );
            ui.selectable_value(&mut *mode, PaintProjection::Uv, tr("brush.projection-uv"));
        });
        group.ui.weak(tr("brush.projection-hint"));
        // Front-face falloff (absorbed from Blender's Normal Falloff):
        // viewport strokes fade on faces seen at a grazing angle. Off by
        // default; the angle is where the fade starts. 80 degrees is near
        // Blender's default — on the first enable it seeds there.
        group.ui.horizontal_wrapped(|ui| {
            let mut on = front_face_falloff.is_some();
            if ui
                .checkbox(&mut on, tr("brush.front-face-falloff"))
                .changed()
            {
                *front_face_falloff = on.then_some(80.0);
            }
            if let Some(deg) = front_face_falloff.as_mut() {
                ui.add(
                    ValueField::new(deg)
                        .context_menu(crate::i18n::prop_menu_labels())
                        .range(0.0..=89.0)
                        .step(1.0),
                );
                ui.label("°");
            }
        });
    });
}

/// Filename stem for Save-As: lowercase, spaces to dashes, everything
/// else outside `[a-z0-9-_.]` dropped. Falls back to `"preset"` so the
/// join always yields a usable file name.
fn sanitize_file_stem(name: &str) -> String {
    let stem: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if stem.is_empty() {
        "preset".to_owned()
    } else {
        stem
    }
}

/// Clamp a dragged control point into its legal window: x stays strictly
/// between the neighbors ([`CURVE_MIN_GAP`] each side, the
/// [`ControlCurve`] strict-monotonicity invariant), endpoints additionally
/// stay in `[0, 1]`; y clamps to `0..=1`.
///
/// This is the exact function the curve-editor painter calls on every
/// drag, factored out so the clamp math is unit-testable headless.
/// Degenerate windows (neighbors closer than twice the gap — only
/// possible for hand-loaded files, since every edit goes through this
/// clamp) pin the point in place rather than panicking in `f32::clamp`.
pub(crate) fn clamp_control_point(
    points: &[(f32, f32); 4],
    idx: usize,
    new_x: f32,
    new_y: f32,
) -> (f32, f32) {
    let y = new_y.clamp(0.0, 1.0);
    let (lo, hi) = match idx {
        0 => (0.0, points[1].0 - CURVE_MIN_GAP),
        3 => (points[2].0 + CURVE_MIN_GAP, 1.0),
        _ => (
            points[idx - 1].0 + CURVE_MIN_GAP,
            points[idx + 1].0 - CURVE_MIN_GAP,
        ),
    };
    let x = if lo <= hi && new_x.is_finite() {
        new_x.clamp(lo, hi)
    } else {
        points[idx].0
    };
    (x, y)
}

/// Which of the two interior control points (index 1 or 2) a click at
/// `click_x` should move, chosen by x-bracketing rather than raw
/// distance: left of `points[1]` always targets `points[1]` (segment 0),
/// right of `points[2]` always targets `points[2]` (segment 2), and a
/// click between them targets whichever is nearer in x. Never returns 0
/// or 3 — the endpoints are not insert targets.
pub(crate) fn nearest_interior_index(points: &[(f32, f32); 4], click_x: f32) -> usize {
    if click_x <= points[1].0 {
        1
    } else if click_x >= points[2].0 {
        2
    } else if click_x - points[1].0 <= points[2].0 - click_x {
        1
    } else {
        2
    }
}

/// "Insert on click": [`ControlCurve`] is fixed at exactly four points
/// (`[(f32, f32); 4]`, enforced by `try_new` and `Deserialize` — the
/// model is unchanged by this pass), so there is no fifth slot to insert
/// into. Instead, a click relocates the nearest free interior point
/// ([`nearest_interior_index`]) to the clicked position, clamped into
/// its legal x-monotonic window by [`clamp_control_point`]. Factored out
/// so it is unit-testable headless, same as `clamp_control_point`.
pub(crate) fn insert_point(points: [(f32, f32); 4], click_x: f32, click_y: f32) -> [(f32, f32); 4] {
    let idx = nearest_interior_index(&points, click_x);
    let mut out = points;
    out[idx] = clamp_control_point(&points, idx, click_x, click_y);
    out
}

/// "Delete on right-click, never the endpoints": the endpoints (indices
/// 0 and 3) always exist by construction, so a right-click on either is
/// a no-op (`None`). For an interior point (index 1 or 2), deletion means
/// collinearizing it — snapping its y onto the straight chord between
/// its immediate neighbors, at its current x — which exactly cancels its
/// contribution to [`ControlCurve::evaluate`]'s piecewise-linear
/// sampling: the closest equivalent to removal the frozen four-point
/// model allows. x is left untouched, so the result is always still
/// strictly increasing in x.
pub(crate) fn delete_point(points: [(f32, f32); 4], idx: usize) -> Option<[(f32, f32); 4]> {
    let (lo, hi) = match idx {
        1 => (points[0], points[2]),
        2 => (points[1], points[3]),
        _ => return None,
    };
    let x = points[idx].0;
    let t = (x - lo.0) / (hi.0 - lo.0);
    let y = lo.1 + (hi.1 - lo.1) * t;
    let mut out = points;
    out[idx] = (x, y);
    Some(out)
}

/// Overwrite `curve` with `preset`, reporting whether anything actually
/// changed — so clicking a preset button that already matches the
/// current curve does not dirty the panel.
pub(crate) fn apply_preset(curve: &mut ControlCurve, preset: ControlCurve) -> bool {
    if *curve == preset {
        false
    } else {
        *curve = preset;
        true
    }
}

/// Smoothstep, `3t^2 - 2t^3`: the "Smooth" preset's shape.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Ease-out cubic, `1 - (1-t)^3`: the "Ease" preset's shape — a fast
/// initial rise that levels off, the same ease-OUT direction as the
/// house motion curve ([`CURVE_MOTION_SECS`]). Ease-IN (a slow start) was
/// the other reasonable reading of "Ease"; this upgrade picks ease-out
/// for that consistency and does not build the other.
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// The "Linear" preset: [`ControlCurve::identity`].
pub(crate) fn preset_linear() -> ControlCurve {
    ControlCurve::identity()
}

/// The "Smooth" preset: a smoothstep S-curve sampled at the identity
/// curve's interior x values (1/3, 2/3).
pub(crate) fn preset_smooth() -> ControlCurve {
    let (t1, t2) = (1.0 / 3.0, 2.0 / 3.0);
    ControlCurve::try_new(&[
        (0.0, 0.0),
        (t1, smoothstep(t1)),
        (t2, smoothstep(t2)),
        (1.0, 1.0),
    ])
    .expect("fixed, strictly-increasing x values always construct")
}

/// The "Ease" preset: an ease-out cubic sampled at the same x values.
pub(crate) fn preset_ease() -> ControlCurve {
    let (t1, t2) = (1.0 / 3.0, 2.0 / 3.0);
    ControlCurve::try_new(&[
        (0.0, 0.0),
        (t1, ease_out_cubic(t1)),
        (t2, ease_out_cubic(t2)),
        (1.0, 1.0),
    ])
    .expect("fixed, strictly-increasing x values always construct")
}

/// Fritsch-Carlson monotone-cubic tangents at the 4 control points, used
/// only to draw the smooth display spline ([`spline_polyline`]) — NOT
/// [`ControlCurve::evaluate`]'s interpolation (see the module docs: that
/// stays piecewise-linear; the spline is cosmetic chrome and the two can
/// disagree slightly between control points). The limiter (the
/// `alpha^2 + beta^2 > 9` rescale) keeps the drawn curve from
/// overshooting past its neighbors' y range on a steep preset.
fn hermite_tangents(points: &[(f32, f32); 4]) -> [f32; 4] {
    let mut d = [0.0f32; 3];
    for i in 0..3 {
        let (x0, y0) = points[i];
        let (x1, y1) = points[i + 1];
        d[i] = (y1 - y0) / (x1 - x0);
    }
    let mut m = [d[0], 0.0, 0.0, d[2]];
    for i in 1..3 {
        let (dm, dp) = (d[i - 1], d[i]);
        m[i] = if dm == 0.0 || dp == 0.0 || dm.signum() != dp.signum() {
            0.0
        } else {
            (dm + dp) / 2.0
        };
    }
    for i in 0..3 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let alpha = m[i] / d[i];
        let beta = m[i + 1] / d[i];
        let s = alpha * alpha + beta * beta;
        if s > 9.0 {
            let tau = 3.0 / s.sqrt();
            m[i] = tau * alpha * d[i];
            m[i + 1] = tau * beta * d[i];
        }
    }
    m
}

/// Sample the cosmetic display spline (a Hermite cubic per segment, via
/// [`hermite_tangents`]) into a curve-space (`[0,1]^2`) polyline:
/// `samples_per_segment` steps per segment, sharing boundary points so
/// the result has no duplicate vertices.
fn spline_polyline(points: &[(f32, f32); 4], samples_per_segment: usize) -> Vec<(f32, f32)> {
    let m = hermite_tangents(points);
    let mut out = Vec::with_capacity(samples_per_segment * 3 + 1);
    for seg in 0..3 {
        let (x0, y0) = points[seg];
        let (x1, y1) = points[seg + 1];
        let dx = x1 - x0;
        let start = if seg == 0 { 0 } else { 1 };
        for step in start..=samples_per_segment {
            let t = step as f32 / samples_per_segment as f32;
            let (t2, t3) = (t * t, t * t * t);
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            let y = h00 * y0 + h10 * dx * m[seg] + h01 * y1 + h11 * dx * m[seg + 1];
            out.push((x0 + dx * t, y));
        }
    }
    out
}

/// The grid backdrop: [`CURVE_GRID_DIVS`]x[`CURVE_GRID_DIVS`] minor cells
/// plus the outer frame (§2: "4x4 minor, the muted color").
fn paint_curve_grid(painter: &egui::Painter, rect: egui::Rect) {
    let stroke = egui::Stroke::new(1.0, curve_palette::GRID_MINOR);
    for i in 1..CURVE_GRID_DIVS {
        let t = i as f32 / CURVE_GRID_DIVS as f32;
        let x = rect.left() + t * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            stroke,
        );
        let y = rect.top() + t * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            stroke,
        );
    }
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, curve_palette::GRID_BORDER),
        egui::StrokeKind::Inside,
    );
}

/// The 4-point curve editor: a custom-painted 150x100 plot (grid
/// backdrop, accent display spline, raised/hover/drag handles),
/// drag-to-edit, click-to-insert, right-click-to-delete, and a 120ms
/// animated glide on any non-drag position change (insert, delete, or a
/// preset button). `ControlCurve` stays fixed at 4 points — see
/// [`insert_point`] and [`delete_point`] for what "insert"/"delete" mean
/// against that frozen shape — so the handles are always exactly those
/// four, never created or destroyed; only their positions move. Returns
/// true when the curve changed this frame.
fn curve_editor(ui: &mut egui::Ui, curve: &mut ControlCurve) -> bool {
    let mut changed = false;

    // `click()` only (no drag): the per-point handles below are
    // allocated after this, on top, so they win hit-testing wherever
    // they overlap the canvas (the same ordering the original editor
    // relied on for its drag handles) and a click landing on a handle
    // does not also register as a canvas-level insert.
    let (rect, canvas_response) = ui.allocate_exact_size(CURVE_PLOT_SIZE, egui::Sense::click());
    let base_id = canvas_response.id;

    let to_screen = |p: (f32, f32)| {
        egui::pos2(
            rect.left() + p.0 * rect.width(),
            rect.bottom() - p.1 * rect.height(),
        )
    };
    let from_screen_x = |sx: f32| ((sx - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let from_screen_y = |sy: f32| ((rect.bottom() - sy) / rect.height()).clamp(0.0, 1.0);

    if canvas_response.clicked() {
        if let Some(pos) = canvas_response.interact_pointer_pos() {
            let click = (from_screen_x(pos.x), from_screen_y(pos.y));
            let new_points = insert_point(curve.points, click.0, click.1);
            if new_points != curve.points {
                curve.points = new_points;
                changed = true;
            }
        }
    }

    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, curve_palette::BG);
    paint_curve_grid(&painter, rect);

    // Pass 1: interact with every handle first (drag, delete), updating
    // `curve.points`, and resolve each point's DISPLAYED position. A
    // dragged point shows its raw (un-animated) position — it must
    // track the pointer exactly, with no lag — every other point shows
    // the animated one, so a discrete jump (insert, delete, a preset
    // button) glides into place over `CURVE_MOTION_SECS` instead of
    // snapping. The `animate_value_with_time` call still runs for the
    // dragged point too (with `animation_time` 0), only to keep its
    // internal last-known position resynced every frame; otherwise the
    // moment the drag releases, the next frame would see a sudden jump
    // from a stale pre-drag position and glide "back" to where the
    // point already visually is.
    let mut displayed = curve.points;
    let mut dragged_idx = None;
    let mut point_state = [(false, false); 4]; // (dragged, hovered), for fill color below
    for i in 0..4 {
        let point_id = base_id.with(("curve-point", i));
        let center = to_screen(curve.points[i]);
        let grab = egui::Rect::from_center_size(center, egui::Vec2::splat(CURVE_HANDLE_HIT_SIZE));
        let response = ui.interact(grab, point_id, egui::Sense::click_and_drag());

        if response.dragged() {
            dragged_idx = Some(i);
            // Absolute pointer position, not accumulated `drag_delta`:
            // `click_and_drag` sense (needed for `secondary_clicked`
            // below) only starts reporting `dragged()` once the pointer
            // clears a small click-vs-drag threshold, so summing deltas
            // from that point on would drop the first few pixels and
            // leave the handle trailing the cursor for the rest of the
            // drag.
            if let Some(pos) = response.interact_pointer_pos() {
                let (x, y) = clamp_control_point(
                    &curve.points,
                    i,
                    from_screen_x(pos.x),
                    from_screen_y(pos.y),
                );
                if (x, y) != curve.points[i] {
                    curve.points[i] = (x, y);
                    changed = true;
                }
            }
        } else if response.secondary_clicked() {
            if let Some(new_points) = delete_point(curve.points, i) {
                if new_points != curve.points {
                    curve.points = new_points;
                    changed = true;
                }
            }
        }
        point_state[i] = (response.dragged(), response.hovered());

        let anim_time = if response.dragged() {
            0.0
        } else {
            CURVE_MOTION_SECS
        };
        let ctx = ui.ctx();
        let ax = ctx.animate_value_with_time(point_id.with("x"), curve.points[i].0, anim_time);
        let ay = ctx.animate_value_with_time(point_id.with("y"), curve.points[i].1, anim_time);
        displayed[i] = if response.dragged() {
            curve.points[i]
        } else {
            (ax, ay)
        };
    }

    // The spline is drawn through the DISPLAYED (animated) positions, not
    // the raw model: otherwise the line would snap to the new points
    // instantly while the handles glide in behind it, visibly off the
    // curve for 120ms. Guard against two edits landing inside the same
    // animation window and leaving the displayed x's not strictly
    // increasing (e.g. two inserts within 120ms of each other) — that
    // would divide by a near-zero segment width in `hermite_tangents` —
    // by falling back to the raw (always-valid) points for that frame.
    let spline_source = if displayed[0].0 < displayed[1].0
        && displayed[1].0 < displayed[2].0
        && displayed[2].0 < displayed[3].0
    {
        displayed
    } else {
        curve.points
    };
    let spline: Vec<egui::Pos2> = spline_polyline(&spline_source, CURVE_SPLINE_SAMPLES)
        .into_iter()
        .map(to_screen)
        .collect();
    painter.add(egui::Shape::line(
        spline,
        egui::Stroke::new(1.75, curve_palette::SPLINE),
    ));

    // Pass 2: paint the handles at their displayed positions, on top of
    // the spline.
    for i in 0..4 {
        let (dragged, hovered) = point_state[i];
        let anim_center = to_screen(displayed[i]);
        let fill = if dragged {
            curve_palette::POINT_DRAG
        } else if hovered {
            curve_palette::POINT_HOVER
        } else {
            curve_palette::POINT_IDLE
        };
        painter.circle_filled(anim_center, CURVE_HANDLE_RADIUS, fill);
        painter.circle_stroke(
            anim_center,
            CURVE_HANDLE_RADIUS,
            egui::Stroke::new(1.5, curve_palette::POINT_STROKE),
        );
    }

    // The in/out readout: the dragged point's exact x while dragging,
    // else whatever x the pointer hovers over the plot at. "out" is
    // always `ControlCurve::evaluate` — the real engine value — never
    // the cosmetic spline above (see `hermite_tangents`'s doc: the two
    // can differ slightly between control points).
    let cursor_x = if let Some(i) = dragged_idx {
        Some(curve.points[i].0)
    } else {
        canvas_response.hover_pos().map(|pos| from_screen_x(pos.x))
    };
    if let Some(in_x) = cursor_x {
        let out_y = curve.evaluate(in_x);
        // Flip the label to whichever side has room, so it stays inside
        // the 150x100 plot instead of running off an edge.
        let (anchor, dx, dy) = match (in_x > 0.5, out_y > 0.5) {
            (true, true) => (egui::Align2::RIGHT_TOP, -8.0, 8.0),
            (true, false) => (egui::Align2::RIGHT_BOTTOM, -8.0, -8.0),
            (false, true) => (egui::Align2::LEFT_TOP, 8.0, 8.0),
            (false, false) => (egui::Align2::LEFT_BOTTOM, 8.0, -8.0),
        };
        let label_pos = to_screen((in_x, out_y)) + egui::vec2(dx, dy);
        painter.text(
            label_pos,
            anchor,
            format!("in {in_x:.2}  out {out_y:.2}"),
            egui::FontId::monospace(CURVE_LABEL_SIZE),
            curve_palette::LABEL,
        );
    }

    // The presets row. The button labels are i18n'd (en-US + pt-BR); the
    // tabular-numerals ask in §2 wants JetBrains Mono for value fields,
    // which is not wired into `umber-app` yet (the font loader is part
    // of `umber-gui`'s own in-flight, uncommitted landing — see
    // `CURVE_MOTION_SECS`) — `FontId::monospace` above is egui's bundled
    // monospace font, used as the interim tabular-numerals stand-in.
    ui.horizontal_wrapped(|ui| {
        ui.weak(tr("brush.curve-presets"));
        if ui.button(tr("brush.curve-preset-linear")).clicked() {
            changed |= apply_preset(curve, preset_linear());
        }
        if ui.button(tr("brush.curve-preset-smooth")).clicked() {
            changed |= apply_preset(curve, preset_smooth());
        }
        if ui.button(tr("brush.curve-preset-ease")).clicked() {
            changed |= apply_preset(curve, preset_ease());
        }
    });

    changed
}

/// The catalog key naming a falloff curve (`brush.falloff-<curve>`).
/// Exhaustive on purpose: a new curve fails to compile here until it has
/// a label in both catalogs.
fn falloff_key(curve: umber_brush::FalloffCurve) -> &'static str {
    use umber_brush::FalloffCurve as F;
    match curve {
        F::Smooth => "brush.falloff-smooth",
        F::Smoother => "brush.falloff-smoother",
        F::Sphere => "brush.falloff-sphere",
        F::Root => "brush.falloff-root",
        F::Sharp => "brush.falloff-sharp",
        F::Sharper => "brush.falloff-sharper",
        F::Linear => "brush.falloff-linear",
        F::InverseSquare => "brush.falloff-inverse-square",
        F::Constant => "brush.falloff-constant",
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn stamp_coverage_is_luma_times_alpha() {
        let rgba = [
            255, 255, 255, 255, // white opaque -> 255
            255, 255, 255, 0, // white transparent -> 0
            0, 0, 0, 255, // black opaque -> 0
            255, 255, 255, 128, // white half -> 128
        ];
        assert_eq!(stamp_coverage(&rgba), vec![255, 0, 0, 128]);
    }

    #[test]
    fn the_stamp_rows_write_the_preset_params() {
        // kittest: the Stamped checkbox, then a jitter field, reach the
        // params the session syncs from.
        use egui_kittest::kittest::Queryable;
        crate::i18n::install(crate::i18n::I18n::builtin());
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, p: &mut umber_brush::BrushParams| {
                stamp_rows(ui, p);
            },
            umber_brush::BrushParams::default(),
        );
        harness.run();
        assert!(!harness.state().stamped);
        harness.get_by_label(&tr("brush.stamped")).click();
        harness.run();
        assert!(harness.state().stamped, "the checkbox stamps the preset");
        // Once stamped, the angle combo shows (it was hidden before).
        assert!(harness.query_by_label(&tr("brush.stamp-angle")).is_some());
    }

    /// kittest (C2-28): the Channel combo drives the paint session's
    /// active channel. Open the combo, pick "Roughness", and the session
    /// strokes into roughness (GPU-gated: `PaintState` needs a device).
    #[test]
    fn the_channel_combo_retargets_the_paint_session() {
        use egui_kittest::kittest::Queryable;
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return;
        };
        // PaintState needs the adapter-specific format features (the
        // `paint_state` GPU tests' device).
        let features = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        if !adapter.features().contains(features) {
            eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
            return;
        }
        let Ok((device, queue)) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_features: features,
                ..Default::default()
            }))
        else {
            return;
        };
        crate::i18n::install(crate::i18n::I18n::builtin());
        let paint = PaintState::with_target_size(device, queue, 64).expect("paint state builds");
        assert_eq!(
            paint.active_channel(),
            crate::paint_state::BASE_COLOR_CHANNEL
        );
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, paint: &mut PaintState| channel_rows(ui, paint),
            paint,
        );
        harness.run();
        // The combo shows the active channel; clicking it opens the list.
        // egui exposes the selected text as the combo's VALUE, not its
        // label: find it by role.
        harness.get_by_role(egui::accesskit::Role::ComboBox).click();
        harness.run();
        harness.get_by_label(&channel_label("roughness")).click();
        harness.run();
        assert_eq!(harness.state().active_channel(), "roughness");
    }

    use super::*;

    #[test]
    fn channel_picker_lists_the_default_set_then_session_extras() {
        let options = pickable_channels(&[]);
        assert_eq!(options[0], crate::paint_state::BASE_COLOR_CHANNEL);
        for want in ["roughness", "metallic", "normal", "height"] {
            assert!(options.iter().any(|c| c == want), "{want} offered");
        }
        // A user channel the session already holds joins at the end, once.
        let with_user = pickable_channels(&["wear".into(), "height".into()]);
        assert_eq!(with_user.last().map(String::as_str), Some("wear"));
        assert_eq!(with_user.iter().filter(|c| *c == "height").count(), 1);
        // Labels: known channels translate, user channels show as named.
        assert_eq!(channel_label_key("roughness"), Some("viewmode.roughness"));
        assert_eq!(channel_label_key("wear"), None);
        let i18n = crate::i18n::I18n::builtin();
        for name in pickable_channels(&[]) {
            if let Some(key) = channel_label_key(&name) {
                for locale in ["en-US", "pt-BR"] {
                    assert!(i18n.has_key(locale, key), "{locale} {key}");
                }
            }
        }
        for key in ["viewmode.channel"] {
            for locale in ["en-US", "pt-BR"] {
                assert!(i18n.has_key(locale, key), "{locale} {key}");
            }
        }
    }

    #[test]
    fn front_face_falloff_defaults_off() {
        // Off by default: the viewport receives None and every stroke keeps
        // its pressure unchanged.
        assert_eq!(BrushPanel::new().front_face_falloff, None);
    }

    #[test]
    fn every_falloff_curve_has_a_distinct_catalog_key() {
        let keys: std::collections::BTreeSet<&str> = umber_brush::FalloffCurve::ALL
            .iter()
            .map(|c| falloff_key(*c))
            .collect();
        assert_eq!(keys.len(), umber_brush::FalloffCurve::ALL.len());
        crate::i18n::init();
        for key in keys {
            assert_ne!(
                crate::i18n::tr(key).as_str(),
                key,
                "{key} has no en-US entry"
            );
        }
    }
    use std::sync::atomic::{AtomicU64, Ordering};
    use umber_brush::preset::{LazyMouseConfig, SpacingConfig};
    use umber_brush::preset_library::user_preset_dir_from;
    use umber_brush::{BrushParams, OneEuroParams};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_tmpdir(tag: &str) -> PathBuf {
        let id = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "umber-app-panel-test-{}-{}-{}",
            std::process::id(),
            id,
            tag
        ));
        std::fs::create_dir_all(&dir).expect("test tmpdir must be creatable");
        dir
    }

    fn test_preset(name: &str, alpha: f32) -> BrushPreset {
        BrushPreset {
            name: name.to_owned(),
            params: BrushParams {
                color: [0.8, 0.2, 0.1, 1.0],
                alpha,
                hardness: 0.25,
                pressure_gamma: 1.0,
                wash_mode: umber_brush::WashMode::BuildUp,
                falloff: Default::default(),
                stamped: false,
                stamp_angle: Default::default(),
                jitter: Default::default(),
            },
            one_euro: OneEuroParams {
                min_cutoff: 1.2,
                beta: 0.02,
                d_cutoff: 1.0,
            },
            lazy_mouse: LazyMouseConfig::default(),
            spacing: SpacingConfig::default(),
            alpha_curve: ControlCurve::identity(),
            radius_curve: ControlCurve::identity(),
            stroke_method: Default::default(),
            size: 1.0,
        }
    }

    fn write_preset(dir: &std::path::Path, file_name: &str, preset: &BrushPreset) -> PathBuf {
        let path = dir.join(file_name);
        std::fs::write(&path, preset.to_json().expect("test preset must serialize"))
            .expect("test preset must be writable");
        path
    }

    #[test]
    fn with_dirs_loads_fixtures_with_no_selection() {
        let dir = fresh_tmpdir("with-dirs");
        write_preset(&dir, "b_soft.umberbrush", &test_preset("Soft Round", 0.7));
        write_preset(&dir, "a_hard.umberbrush", &test_preset("Hard Round", 0.95));

        let panel = BrushPanel::with_dirs(vec![dir.clone()]);

        assert_eq!(panel.library.entries.len(), 2);
        assert!(panel.library.errors.is_empty());
        assert_eq!(panel.active, None);
        assert!(!panel.dirty);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wash_mode_toggle_persists_through_save_and_reload() {
        let dir = fresh_tmpdir("wash-persist");
        write_preset(&dir, "round.umberbrush", &test_preset("Round", 0.5));

        let mut panel = BrushPanel::with_dirs(vec![dir.clone()]);
        assert_eq!(panel.library.entries.len(), 1);
        panel.active = Some(0);
        assert_eq!(
            panel.active_preset().map(|p| p.params.wash_mode),
            Some(umber_brush::WashMode::BuildUp),
            "legacy preset file loads as build-up"
        );

        // What the Wash/Build-up toggle row does: write through to the
        // active preset and dirty the panel.
        panel.library.entries[0].preset.params.wash_mode = umber_brush::WashMode::Wash;
        panel.dirty = true;
        panel.save_active();
        assert!(!panel.dirty, "successful save clears dirty");

        let fresh = Library::load_from_dirs(std::slice::from_ref(&dir));
        assert_eq!(fresh.entries.len(), 1);
        let back = &fresh.entries[0].preset;
        assert_eq!(
            back.params.wash_mode,
            umber_brush::WashMode::Wash,
            "the toggled mode must survive save + reload"
        );
        back.validate().expect("wash preset must validate");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clamp_pins_interior_points_between_neighbors() {
        let points = [(0.0, 0.0), (0.33, 0.4), (0.66, 0.7), (1.0, 1.0)];
        // NOTE: bounds are written as the same f32 arithmetic the clamp
        // performs (`0.66 - 0.01`, not the decimal `0.65`: the two differ
        // by one ulp), so these assert_eq!s are bitwise-exact.
        // Dragging point 1 left past point 0 pins to neighbor.x - gap...
        // (here the left neighbor is x=0.0, so the lower bound 0.01 wins).
        assert_eq!(
            clamp_control_point(&points, 1, -1.0, 0.5),
            (0.0 + 0.01, 0.5)
        );
        // ... and dragging right past point 2 pins to 0.66 - 0.01 exactly.
        assert_eq!(
            clamp_control_point(&points, 1, 10.0, 0.5),
            (0.66 - 0.01, 0.5)
        );
        // Over-dragging past the right neighbor pins to neighbor.x - 0.01
        // exactly: 1.05 is 0.05 past point 3 (x=1.0), window hi is 0.99.
        assert_eq!(
            clamp_control_point(&points, 2, 1.0 + 0.05, 0.5),
            (1.0 - 0.01, 0.5)
        );
        assert_eq!(
            clamp_control_point(&points, 2, 0.33 + 0.05, 0.5),
            (0.33 + 0.05, 0.5),
            "0.38 lies inside the (0.34, 0.99) window, so it passes through"
        );
        assert_eq!(
            clamp_control_point(&points, 2, 0.33 - 0.05, 0.5),
            (0.33 + 0.01, 0.5),
            "0.28 is below the window, so it pins to 0.33 + 0.01"
        );
    }

    #[test]
    fn clamp_pins_endpoints_to_unit_x_and_all_y() {
        let points = [(0.0, 0.0), (0.33, 0.4), (0.66, 0.7), (1.0, 1.0)];
        assert_eq!(clamp_control_point(&points, 0, -0.5, 0.5), (0.0, 0.5));
        assert_eq!(
            clamp_control_point(&points, 0, 0.9, 0.5),
            (0.33 - 0.01, 0.5)
        );
        assert_eq!(clamp_control_point(&points, 3, 1.5, 0.5), (1.0, 0.5));
        assert_eq!(
            clamp_control_point(&points, 3, 0.1, 0.5),
            (0.66 + 0.01, 0.5)
        );
        assert_eq!(clamp_control_point(&points, 1, 0.5, -2.0), (0.5, 0.0));
        assert_eq!(clamp_control_point(&points, 1, 0.5, 2.0), (0.5, 1.0));
    }

    #[test]
    fn user_dir_computation_honors_xdg() {
        assert_eq!(
            user_preset_dir_from("/home/ava", Some("/run/user/1000/data")),
            PathBuf::from("/run/user/1000/data/umber/brushes"),
        );
        assert_eq!(
            user_preset_dir_from("/home/ava", None),
            PathBuf::from("/home/ava/.local/share/umber/brushes"),
        );
        assert_eq!(
            user_preset_dir_from("/home/ava", Some("")),
            PathBuf::from("/home/ava/.local/share/umber/brushes"),
            "empty XDG falls back to the home default",
        );
    }

    #[test]
    fn save_round_trips_through_the_real_model() {
        let dir = fresh_tmpdir("save-roundtrip");
        write_preset(&dir, "round.umberbrush", &test_preset("Round", 0.5));

        let mut panel = BrushPanel::with_dirs(vec![dir.clone()]);
        assert_eq!(panel.library.entries.len(), 1);
        panel.active = Some(0);
        panel.library.entries[0].preset.params.alpha = 0.25;
        panel.dirty = true;
        panel.save_active();

        assert!(!panel.dirty, "successful save clears dirty");
        assert!(panel.save_error.is_none());

        let fresh = Library::load_from_dirs(std::slice::from_ref(&dir));
        assert_eq!(fresh.entries.len(), 1);
        let back = BrushPreset::from_json(
            &std::fs::read_to_string(&fresh.entries[0].source_path).expect("readable"),
        )
        .expect("parseable");
        assert!(
            (back.params.alpha - 0.25).abs() < f32::EPSILON,
            "on-disk alpha must reflect the edit, got {}",
            back.params.alpha
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    const EPS: f32 = 1e-6;

    #[test]
    fn nearest_interior_index_brackets_by_segment() {
        let points = [(0.0, 0.0), (0.3, 0.3), (0.7, 0.7), (1.0, 1.0)];
        // Segment 0 (left of p1) always targets p1, including the exact
        // boundary and points left of the plot entirely.
        assert_eq!(nearest_interior_index(&points, -0.5), 1);
        assert_eq!(nearest_interior_index(&points, 0.0), 1);
        assert_eq!(nearest_interior_index(&points, 0.3), 1);
        // Segment 2 (right of p2) always targets p2, same at both ends.
        assert_eq!(nearest_interior_index(&points, 0.7), 2);
        assert_eq!(nearest_interior_index(&points, 1.0), 2);
        assert_eq!(nearest_interior_index(&points, 1.5), 2);
        // Segment 1 (between p1 and p2) picks whichever is nearer: the
        // midpoint 0.5 is equidistant (0.2 each way) and ties to p1 (the
        // `<=` branch), 0.45 is nearer p1, 0.55 is nearer p2.
        assert_eq!(nearest_interior_index(&points, 0.5), 1);
        assert_eq!(nearest_interior_index(&points, 0.45), 1);
        assert_eq!(nearest_interior_index(&points, 0.55), 2);
    }

    #[test]
    fn insert_point_moves_only_the_targeted_index_and_stays_valid() {
        let points = [(0.0, 0.0), (0.3, 0.3), (0.7, 0.7), (1.0, 1.0)];
        let out = insert_point(points, 0.1, 0.9);
        // 0.1 is in segment 0, so only index 1 moves; the clamp pins y
        // (0.9 is inside 0..=1, so it passes through unchanged) and x
        // (0.1 is inside (0+gap, 0.7-gap), so it also passes through).
        assert_eq!(out, [(0.0, 0.0), (0.1, 0.9), (0.7, 0.7), (1.0, 1.0)]);
        assert!(
            ControlCurve::try_new(&out).is_ok(),
            "insert must never break the strictly-increasing-x invariant"
        );

        // A click far outside the legal window still produces a valid
        // curve: clamp_control_point pins it to the nearest legal edge
        // rather than violating monotonicity.
        let out = insert_point(points, 5.0, -5.0);
        assert!(ControlCurve::try_new(&out).is_ok());
        assert_eq!(out[0], points[0], "untouched endpoints stay put");
        assert_eq!(out[3], points[3], "untouched endpoints stay put");
    }

    #[test]
    fn insert_point_in_the_middle_segment_picks_the_nearer_point() {
        let points = [(0.0, 0.0), (0.2, 0.2), (0.8, 0.8), (1.0, 1.0)];
        // 0.3 is nearer p1 (0.2) than p2 (0.8): only index 1 moves.
        let out = insert_point(points, 0.3, 0.6);
        assert_eq!(out[1], (0.3, 0.6));
        assert_eq!(out[2], points[2]);

        // 0.75 is nearer p2 (0.8) than p1 (0.2): only index 2 moves.
        let out = insert_point(points, 0.75, 0.4);
        assert_eq!(out[1], points[1]);
        assert_eq!(out[2], (0.75, 0.4));
    }

    #[test]
    fn delete_point_rejects_the_endpoints() {
        let points = [(0.0, 0.0), (0.3, 0.4), (0.7, 0.6), (1.0, 1.0)];
        assert_eq!(delete_point(points, 0), None, "never delete the endpoints");
        assert_eq!(delete_point(points, 3), None, "never delete the endpoints");
    }

    #[test]
    fn delete_point_collinearizes_the_interior_point_in_place() {
        let points = [(0.0, 0.0), (0.25, 0.9), (0.75, 0.1), (1.0, 1.0)];

        let out = delete_point(points, 1).expect("interior index deletes");
        // x is untouched; y snaps onto the chord from p0 to p2:
        // t = (0.25-0.0)/(0.75-0.0) = 1/3, y = 0 + (0.1-0)/3.
        assert_eq!(out[1].0, points[1].0, "x never moves on delete");
        assert!((out[1].1 - (0.1 / 3.0)).abs() <= EPS);
        assert_eq!(out[0], points[0]);
        assert_eq!(out[2], points[2]);
        assert_eq!(out[3], points[3]);
        assert!(ControlCurve::try_new(&out).is_ok());

        let out = delete_point(points, 2).expect("interior index deletes");
        // t = (0.75-0.25)/(1.0-0.25) = 2/3, y = 0.9 + (1.0-0.9)*2/3.
        assert_eq!(out[2].0, points[2].0);
        assert!((out[2].1 - (0.9 + 0.1 * 2.0 / 3.0)).abs() <= EPS);
        assert!(ControlCurve::try_new(&out).is_ok());
    }

    #[test]
    fn apply_preset_reports_no_change_when_already_matching() {
        let mut curve = preset_linear();
        assert!(!apply_preset(&mut curve, preset_linear()));
        assert_eq!(curve, preset_linear());
    }

    #[test]
    fn apply_preset_overwrites_and_reports_changed() {
        let mut curve = preset_linear();
        assert!(apply_preset(&mut curve, preset_smooth()));
        assert_eq!(curve, preset_smooth());
        // Applying the same preset again is a no-op the second time.
        assert!(!apply_preset(&mut curve, preset_smooth()));
    }

    #[test]
    fn preset_linear_is_the_identity_curve() {
        assert_eq!(preset_linear(), ControlCurve::identity());
    }

    #[test]
    fn preset_smooth_matches_smoothstep_at_the_identity_x_values() {
        let curve = preset_smooth();
        assert_eq!(curve.points[0], (0.0, 0.0));
        assert_eq!(curve.points[3], (1.0, 1.0));
        // smoothstep(1/3) = 3*(1/3)^2 - 2*(1/3)^3 = 1/3 - 2/27 = 7/27.
        assert!((curve.points[1].0 - 1.0 / 3.0).abs() <= EPS);
        assert!((curve.points[1].1 - 7.0 / 27.0).abs() <= EPS);
        // smoothstep(2/3) = 3*(2/3)^2 - 2*(2/3)^3 = 4/3 - 16/27 = 20/27.
        assert!((curve.points[2].0 - 2.0 / 3.0).abs() <= EPS);
        assert!((curve.points[2].1 - 20.0 / 27.0).abs() <= EPS);
    }

    #[test]
    fn preset_ease_matches_ease_out_cubic_at_the_identity_x_values() {
        let curve = preset_ease();
        assert_eq!(curve.points[0], (0.0, 0.0));
        assert_eq!(curve.points[3], (1.0, 1.0));
        // ease_out_cubic(1/3) = 1 - (2/3)^3 = 1 - 8/27 = 19/27.
        assert!((curve.points[1].1 - 19.0 / 27.0).abs() <= EPS);
        // ease_out_cubic(2/3) = 1 - (1/3)^3 = 1 - 1/27 = 26/27.
        assert!((curve.points[2].1 - 26.0 / 27.0).abs() <= EPS);
        // "Ease" rises faster than "Smooth" does early on (the chosen
        // ease-OUT direction): a fast initial rise that levels off.
        assert!(curve.points[1].1 > preset_smooth().points[1].1);
    }

    #[test]
    fn spline_polyline_does_not_overshoot_monotonic_data() {
        // A steep middle segment: the kind of preset most likely to make
        // a naive (non-monotone) cubic overshoot past 0 or 1.
        let curve = ControlCurve::try_new(&[(0.0, 0.0), (0.2, 0.05), (0.8, 0.95), (1.0, 1.0)])
            .expect("valid test curve");
        let poly = spline_polyline(&curve.points, 20);
        let mut prev_y = f32::NEG_INFINITY;
        for (x, y) in poly {
            assert!(
                (-1e-4..=1.0 + 1e-4).contains(&y),
                "monotone-cubic sample out of [0,1] bounds: y={y} at x={x}"
            );
            assert!(
                y >= prev_y - 1e-4,
                "monotone-cubic sample not monotonic: {prev_y} -> {y} at x={x}"
            );
            prev_y = y;
        }
    }

    #[test]
    fn spline_polyline_passes_through_every_control_point() {
        let curve = ControlCurve::try_new(&[(0.0, 0.1), (0.3, 0.5), (0.6, 0.2), (1.0, 0.9)])
            .expect("valid test curve");
        let poly = spline_polyline(&curve.points, 10);
        for &(px, py) in &curve.points {
            assert!(
                poly.iter()
                    .any(|&(x, y)| (x - px).abs() <= EPS && (y - py).abs() <= 1e-4),
                "control point ({px}, {py}) must land exactly on the sampled spline"
            );
        }
    }

    #[test]
    fn select_tool_banks_and_restores_retouch_params() {
        use crate::toolbar::ToolKind;
        let mut panel = BrushPanel::with_dirs(Vec::new());
        assert_eq!(panel.tool, BrushTool::Paint);

        // Select smudge, edit it in place (as the panel's fields do), then
        // switch away before any `tool_rows` draw banks the edit.
        panel.select_tool(ToolKind::Smudge);
        let BrushTool::Smudge(params) = &mut panel.tool else {
            panic!("smudge selected: {:?}", panel.tool);
        };
        params.strength = 0.25;
        panel.select_tool(ToolKind::Erase);
        assert!(matches!(panel.tool, BrushTool::Erase(_)));

        // Back to smudge: the edit survived the round trip.
        panel.select_tool(ToolKind::Smudge);
        let BrushTool::Smudge(params) = panel.tool else {
            panic!("smudge reselected: {:?}", panel.tool);
        };
        assert_eq!(params.strength, 0.25);

        // Reselecting the active kind keeps its live params.
        panel.select_tool(ToolKind::Smudge);
        assert_eq!(panel.tool, BrushTool::Smudge(params));
        panel.select_tool(ToolKind::Paint);
        assert_eq!(panel.tool, BrushTool::Paint);
    }
}
