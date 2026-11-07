//! The Assets panel's library browser (R-C1, the SPP census's bigger
//! P0 moat — the brief `docs/loop/briefs/2026-10-24-rc1-assets-panel.md`).
//!
//! The census's diagnosis: "capability exists, discoverability does
//! not" — the loaders, the dirs, and the apply paths are the engines'
//! own pub surfaces (`document::list_smart_materials`,
//! `document::list_smart_masks`, `document::smart_dirs`,
//! `umber_brush::preset_library::Library`,
//! `document::apply_smart_material`, the smart-mask compose flow).
//! This module is the DISCOVERABILITY layer: one searchable browser
//! over the three existing libraries, apply-on-click through the
//! existing engines. No new loaders, no new formats, no filesystem
//! writes (the brief's non-goals).
//!
//! The v1 slice's shape: a search field (substring,
//! case-insensitive) + the collapsible categories (Brushes, Smart
//! materials, Masks, and — K1 — Environment maps as the fourth,
//! DEFAULT CLOSED so the resting panel shape doesn't move), each
//! listing its library, the rows filtered by the search (a search
//! that empties a non-empty list shows `assets.no-matches`; an empty
//! library still shows `assets.empty`), the name + source-dir shown
//! (the starter-vs-user distinction the census names; the tooltip
//! carries the full path), a reload button per category (the smart
//! shelf's own `↻` precedent), and apply-on-click:
//!
//! - a brush row: [`BrushPanel::select_preset_by_name`] (the new
//!   seam) loads the preset into the brush session;
//! - a smart-material row: [`document::apply_smart_material`] adds
//!   the material layer stack (the existing shelf flow);
//! - a mask row: the outcome carries `(target, SmartMask)` and the
//!   shell's EXISTING compose flow (the same one the Layers panel's
//!   smart-mask section emits through `smart_mask_applied`) does
//!   the per-tile compose with the anchor reads + bake sources.
//!
//! The named follow-ups (the brief): drag-to-layer-stack, the
//! content-keyed thumbnails, custom library
//! locations, the filter/mesh/env categories, the import-dialog
//! integration. (Saved searches shipped as K3; the first-run
//! empty-state wiring as K4.)

use crate::document;
use crate::i18n::tr;
use egui::Ui;

/// The panel's session state: the search field + the cached lists'
/// dirty flag. The lists themselves cache in egui memory per
/// frame-group (the smart shelf's own `smart-shelf-cache` precedent:
/// the scan is disk I/O, refreshed by the user's ↻).
#[derive(Default)]
pub struct AssetsPanel {
    /// The search filter (substring, case-insensitive; empty = all).
    /// Persisted to the state dir (the swatches-file precedent) so the
    /// browser reopens on the last query.
    pub search: String,
    /// Content-keyed swatch cache: (kind, name, preset-JSON hash) →
    /// texture. A changed preset file changes the key and regenerates.
    /// The HANDLE is cached, not just its id: dropping a `TextureHandle`
    /// frees the texture, so an id-only cache pointed at freed textures
    /// from the first frame (UI/UX pass U3 caught it).
    thumbs: std::collections::HashMap<crate::asset_thumbs::ThumbKey, egui::TextureHandle>,
    /// Whether `search` was restored from disk this session.
    restored: bool,
    /// The last value written to disk (skip no-op writes).
    last_saved: String,
    /// The "add folder" text field.
    new_dir: String,
    /// Whether the Import… dialog window is open (the three import
    /// sections moved into it per the R-C1 brief).
    pub import_open: bool,
    /// Bumped whenever the extra library folders change — the material /
    /// mask list caches key on it, so a new folder rescans them.
    scan_epoch: u64,
    /// The brush shelf's catalog tab (U3 catalogs): `None` = All.
    pub shelf_catalog: Option<String>,
    /// K3: the saved searches, persisted to the state dir's
    /// `assets_saved_searches.json` (the single-query file above stays
    /// the last-used query, untouched).
    pub saved: Vec<SavedSearch>,
    /// Whether `saved` was restored from disk this session.
    saved_restored: bool,
    /// The last value written to disk (skip no-op writes).
    saved_last: Vec<SavedSearch>,
    /// The "save current" name field.
    new_search_name: String,
    /// K4: categories that already logged the empty-without-starter
    /// warn this session (once each, not every frame).
    starter_warns: std::collections::HashSet<&'static str>,
}

impl std::fmt::Debug for AssetsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetsPanel")
            .field("search", &self.search)
            .field("thumbs", &self.thumbs.len())
            .field("import_open", &self.import_open)
            .field("scan_epoch", &self.scan_epoch)
            .finish_non_exhaustive()
    }
}

/// Restores the persisted search once per session (the `restored` flag).
fn restore_search(state: &mut AssetsPanel, path: Option<&std::path::Path>) {
    if state.restored {
        return;
    }
    state.restored = true;
    state.search = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.trim_end().replace('\n', " "))
        .unwrap_or_default();
    state.last_saved = state.search.clone();
}

/// Writes the search when it changed since the last save.
fn persist_search(state: &mut AssetsPanel, path: Option<&std::path::Path>) {
    if !state.restored || state.search == state.last_saved {
        return;
    }
    if let Some(p) = path {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(p, &state.search) {
            log::warn!("assets: could not save search {}: {e}", p.display());
        }
    }
    state.last_saved = state.search.clone();
}

/// K3: one saved search — a name plus the query it restores.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedSearch {
    pub name: String,
    pub query: String,
}

/// Loads the saved searches from the state-dir JSON file. A missing,
/// unreadable or invalid file reads as empty (the `load_extra_dirs`
/// precedent — never a panic, never a broken panel); a parse failure
/// logs a warn so a corrupted file is diagnosable.
pub fn load_saved_searches(path: Option<&std::path::Path>) -> Vec<SavedSearch> {
    let Some(path) = path else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        log::warn!(
            "assets: could not parse saved searches {}: {e}",
            path.display()
        );
        Vec::new()
    })
}

/// Writes the saved searches as JSON. Errors log; the session keeps the
/// list either way (the `save_extra_dirs` precedent).
pub fn save_saved_searches(list: &[SavedSearch], path: Option<&std::path::Path>) {
    let Some(path) = path else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(list).unwrap_or_else(|_| "[]".to_owned());
    if let Err(e) = std::fs::write(path, text) {
        log::warn!("assets: could not save searches {}: {e}", path.display());
    }
}

/// Restores the saved searches once per session (the `restored` flag,
/// the single-query file's own pattern).
fn restore_saved(state: &mut AssetsPanel, path: Option<&std::path::Path>) {
    if state.saved_restored {
        return;
    }
    state.saved_restored = true;
    state.saved = load_saved_searches(path);
    state.saved_last = state.saved.clone();
}

/// Writes the saved searches when they changed since the last save.
fn persist_saved(state: &mut AssetsPanel, path: Option<&std::path::Path>) {
    if !state.saved_restored || state.saved == state.saved_last {
        return;
    }
    save_saved_searches(&state.saved, path);
    state.saved_last = state.saved.clone();
}

/// K4: a category's bundled starter dir (repo `assets/<name>`), resolved
/// through the bundled-data lookup — exe-relative in packaged builds.
fn starter_dir(rel: &str) -> std::path::PathBuf {
    umber_core::bundled::data_path(rel)
}

/// K4: whether the starter dir was found on disk — the manifest join,
/// or the CWD-relative one the brush library also scans.
fn starter_found(rel: &str) -> bool {
    std::path::Path::new(rel).is_dir() || starter_dir(rel).is_dir()
}

/// K4's pure predicate (the testable core): an empty list warns only
/// when the bundled starter dir was not found either — a fresh state
/// dir scans the starter dirs with zero user state, so both missing
/// means the bundled library itself is gone.
pub fn empty_without_starter_warns(list_len: usize, starter_found: bool) -> bool {
    list_len == 0 && !starter_found
}

/// K4: logs the first-run warn once per category per session — an
/// empty category AND no starter dir on disk (the env loader's
/// skip-bad-file warn precedent: diagnosable, not silent).
fn warn_empty_without_starter(
    warned: &mut std::collections::HashSet<&'static str>,
    category: &'static str,
    list_len: usize,
    starter_rel: &str,
) {
    if empty_without_starter_warns(list_len, starter_found(starter_rel)) && warned.insert(category)
    {
        log::warn!(
            "assets: {category} library is empty and no starter dir was found ({})",
            starter_dir(starter_rel).display()
        );
    }
}

impl AssetsPanel {
    /// The row's swatch texture, generating + caching on first sight.
    pub(crate) fn thumb(
        &mut self,
        ctx: &egui::Context,
        kind: u8,
        name: &str,
        preset_json: &str,
        make: impl FnOnce() -> egui::ColorImage,
    ) -> Option<egui::TextureId> {
        let key = crate::asset_thumbs::thumb_key(kind, name, preset_json);
        if let Some(tex) = self.thumbs.get(&key) {
            return Some(tex.id());
        }
        let img = make();
        let tex = ctx.load_texture(
            format!("asset-thumb-{kind}-{name}"),
            img,
            egui::TextureOptions::LINEAR,
        );
        let id = tex.id();
        self.thumbs.insert(key, tex);
        Some(id)
    }
}

/// ThumbKey kind bytes for the [`AssetsPanel::thumb`] cache.
pub const THUMB_KIND_BRUSH: u8 = 0;
pub const THUMB_KIND_MATERIAL: u8 = 1;
pub const THUMB_KIND_MASK: u8 = 2;
/// K1: an env-map row's downsampled equirect swatch.
pub const THUMB_KIND_ENV: u8 = 3;
/// K2: a brush row's `<stem>.stamp.png` sidecar swatch.
pub const THUMB_KIND_STAMP: u8 = 4;

/// A file's content-ish key for the [`crate::asset_thumbs::ThumbKey`]
/// cache: path + length + mtime, so a re-exported env map or stamp tip
/// regenerates its swatch without hashing whole files (EXRs can be huge).
fn file_content_key(path: &std::path::Path) -> String {
    let (len, mtime) = std::fs::metadata(path)
        .ok()
        .map(|m| {
            (
                m.len(),
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            )
        })
        .unwrap_or((0, 0));
    format!("file|{}|{}|{}", path.display(), len, mtime)
}

/// What the shell must apply after a library row's click.
#[derive(Debug, Default)]
pub struct LibraryOutcome {
    /// A brush preset to load by name ([`BrushPanel::select_preset_by_name`]).
    pub brush: Option<String>,
    /// A smart material to apply ([`document::apply_smart_material`]).
    pub material: Option<umber_core::smart::SmartMaterial>,
    /// A smart mask to compose onto `target` (the shell's existing
    /// `smart_mask_applied` flow — the SAME compose path the Layers
    /// panel's mask section uses).
    pub mask: Option<(u64, umber_core::smart::SmartMask)>,
    /// K1: an environment map file to load — the shell reuses the exact
    /// File▸Load path (`env::load_environment_file` →
    /// `viewport.set_environment`).
    pub env: Option<std::path::PathBuf>,
    /// A smart filter file to apply to the ACTIVE layer as one undo step
    /// (the shell's `apply_filter_file` → `filter_apply` plane-op stack).
    pub filter: Option<std::path::PathBuf>,
}

/// The brush shelf's swatch edge, points (UI/UX pass U3).
pub const SHELF_SWATCH: f32 = 40.0;

/// The catalog id a preset files under (its trimmed `catalog`);
/// uncategorised presets share the `"Other"` catalog.
fn catalog_of(preset: &umber_brush::preset::BrushPreset) -> String {
    let c = preset.catalog.trim();
    if c.is_empty() {
        "Other".to_owned()
    } else {
        c.to_owned()
    }
}

/// A catalog's display label: the shipped catalogs are localized
/// (`assets.catalog-basic` …); a user's own catalog shows as written.
pub fn catalog_label(id: &str) -> String {
    let key = format!("assets.catalog-{}", id.to_lowercase());
    let label = tr(&key);
    if label == key {
        id.to_owned()
    } else {
        label
    }
}

/// The library's catalogs in display order (first appearance in the
/// name-sorted library, so it is stable), each with its preset indices.
pub fn brush_catalogs(brush: &crate::brush_panel::BrushPanel) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, entry) in brush.library.entries.iter().enumerate() {
        let name = catalog_of(&entry.preset);
        match out.iter_mut().find(|(c, _)| *c == name) {
            Some((_, v)) => v.push(i),
            None => out.push((name, vec![i])),
        }
    }
    out
}

/// The brush asset shelf (UI/UX pass U3, Blender 4.3's asset shelf in
/// paint modes): a row of catalog tabs (All + one per catalog) over one
/// horizontal, scrollable strip of preset swatches along the 3D
/// viewport's bottom edge. A click selects the preset (the caller routes
/// it through `BrushPanel::select_preset_by_name`, the same seam the
/// Assets browser rows use); the active preset is framed in the accent;
/// the name rides the caption and the hover.
pub fn brush_shelf_ui(
    ui: &mut Ui,
    state: &mut AssetsPanel,
    brush: &crate::brush_panel::BrushPanel,
) -> Option<String> {
    let catalogs = brush_catalogs(brush);
    // A catalog that vanished (library rescan) falls back to All.
    if let Some(c) = &state.shelf_catalog {
        if !catalogs.iter().any(|(n, _)| n == c) {
            state.shelf_catalog = None;
        }
    }
    let mut picked = None;
    // One tab row (All + a tab per catalog) above the swatch strip, the
    // layout Blender 4.3's shelf header uses; a stacked column of tabs
    // would make the strip taller than its swatches.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let tab = |ui: &mut Ui, sel: bool, label: &str| {
            ui.add(egui::Button::selectable(
                sel,
                egui::RichText::new(label).size(umber_gui::theme::TEXT_STATUS),
            ))
            .clicked()
        };
        if tab(ui, state.shelf_catalog.is_none(), &tr("assets.catalog-all")) {
            state.shelf_catalog = None;
        }
        for (name, _) in &catalogs {
            if tab(
                ui,
                state.shelf_catalog.as_deref() == Some(name),
                &catalog_label(name),
            ) {
                state.shelf_catalog = Some(name.clone());
            }
        }
    });
    let shown: Vec<usize> = match &state.shelf_catalog {
        None => (0..brush.library.entries.len()).collect(),
        Some(c) => catalogs
            .iter()
            .find(|(n, _)| n == c)
            .map(|(_, v)| v.clone())
            .unwrap_or_default(),
    };
    let out = egui::ScrollArea::horizontal()
        .id_salt("umber-brush-shelf")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| picked = brush_swatches(ui, state, brush, &shown));
        });
    // UI/UX audit A2-5: a fade at each edge with more swatches beyond it,
    // so a clipped last card reads as "scroll for more", not as cut off.
    let (left, right) = shelf_overflow(
        out.state.offset.x,
        out.content_size.x,
        out.inner_rect.width(),
    );
    let bg = umber_gui::theme::panel_bg();
    let r = out.inner_rect;
    for (edge, show) in [(r.left(), left), (r.right() - SHELF_FADE, right)] {
        if !show {
            continue;
        }
        let fade = egui::Rect::from_min_size(
            egui::pos2(edge, r.top()),
            egui::vec2(SHELF_FADE, r.height()),
        );
        let (a, b) = if edge == r.left() {
            (bg, bg.gamma_multiply(0.0))
        } else {
            (bg.gamma_multiply(0.0), bg)
        };
        let mut mesh = egui::Mesh::default();
        let i = mesh.vertices.len() as u32;
        for (p, c) in [
            (fade.left_top(), a),
            (fade.right_top(), b),
            (fade.right_bottom(), b),
            (fade.left_bottom(), a),
        ] {
            mesh.colored_vertex(p, c);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
        ui.painter().add(egui::Shape::mesh(mesh));
    }
    picked
}

/// Width of the shelf's overflow fades, points.
pub const SHELF_FADE: f32 = 24.0;

/// Whether the shelf has swatches hidden past its left / right edge: the
/// scroll offset, the content width and the visible width, all in points.
/// Half a point of slack absorbs float rounding at the ends.
pub fn shelf_overflow(offset: f32, content: f32, visible: f32) -> (bool, bool) {
    (offset > 0.5, offset + visible < content - 0.5)
}

/// Swatches per row in the brush popup.
pub const POPUP_COLUMNS: usize = 6;

#[cfg(test)]
mod shelf_overflow_tests {
    use super::shelf_overflow;

    #[test]
    fn fades_mark_only_the_edges_with_hidden_swatches() {
        assert_eq!(shelf_overflow(0.0, 300.0, 400.0), (false, false), "fits");
        assert_eq!(shelf_overflow(0.0, 600.0, 400.0), (false, true), "start");
        assert_eq!(shelf_overflow(100.0, 600.0, 400.0), (true, true), "middle");
        assert_eq!(shelf_overflow(200.0, 600.0, 400.0), (true, false), "end");
    }
}

/// The brush popup's body (Shift+Space): the same swatches as the shelf,
/// one titled section per catalog, each in rows of [`POPUP_COLUMNS`], so
/// every preset shows without scrolling however large the library grows.
pub fn brush_grid_ui(
    ui: &mut Ui,
    state: &mut AssetsPanel,
    brush: &crate::brush_panel::BrushPanel,
) -> Option<String> {
    let mut picked = None;
    for (name, indices) in brush_catalogs(brush) {
        ui.label(
            egui::RichText::new(catalog_label(&name))
                .size(umber_gui::theme::TEXT_STATUS)
                .color(umber_gui::theme::muted()),
        );
        for row in indices.chunks(POPUP_COLUMNS) {
            if let Some(n) = ui
                .horizontal(|ui| brush_swatches(ui, state, brush, row))
                .inner
            {
                picked = Some(n);
            }
        }
    }
    picked
}

/// One swatch button + caption per library preset; the clicked one's name.
/// The swatch card colour for the active theme (its raised surface, as
/// sRGB 0..1), plus the cache-key salt so a theme switch re-renders.
fn swatch_card() -> ([f32; 3], &'static str) {
    let c = umber_gui::theme::raised();
    (
        [
            c.r() as f32 / 255.0,
            c.g() as f32 / 255.0,
            c.b() as f32 / 255.0,
        ],
        umber_gui::theme::active_theme().name(),
    )
}

fn brush_swatches(
    ui: &mut Ui,
    state: &mut AssetsPanel,
    brush: &crate::brush_panel::BrushPanel,
    indices: &[usize],
) -> Option<String> {
    let mut picked = None;
    for &i in indices {
        let Some(entry) = brush.library.entries.get(i) else {
            continue;
        };
        let (card, salt) = swatch_card();
        let json = format!("{salt}{}", entry.preset.to_json().unwrap_or_default());
        let Some(tex) = state.thumb(
            ui.ctx(),
            THUMB_KIND_BRUSH,
            &entry.preset.name,
            &json,
            || {
                crate::asset_thumbs::brush_preview_on(
                    &entry.preset,
                    crate::asset_thumbs::THUMB_PX * 2,
                    card,
                )
            },
        ) else {
            continue;
        };
        let active = brush.active == Some(i);
        let image = egui::Image::new(egui::load::SizedTexture::new(
            tex,
            egui::vec2(SHELF_SWATCH, SHELF_SWATCH),
        ))
        .corner_radius(4.0);
        let button = egui::Button::image(image)
            .selected(active)
            .stroke(if active {
                egui::Stroke::new(2.0, umber_gui::theme::accent())
            } else {
                egui::Stroke::NONE
            });
        let response = ui
            .vertical(|ui| {
                ui.set_width(SHELF_SWATCH + 8.0);
                let r = ui.add(button);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&entry.preset.name)
                            .size(umber_gui::theme::TEXT_STATUS)
                            .color(if active {
                                umber_gui::theme::accent()
                            } else {
                                umber_gui::theme::muted()
                            }),
                    )
                    .truncate(),
                );
                r
            })
            .inner
            .on_hover_text(&entry.preset.name);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, &entry.preset.name)
        });
        if response.clicked() && !active {
            picked = Some(entry.preset.name.clone());
        }
    }
    picked
}

/// Draws the library section (search + saved searches + the categories +
/// rows). The caller (the Assets tab's arm) keeps the import sections
/// below. `search_path` is the single-query file (restored once, written
/// on change); `saved_path` is the K3 saved-searches JSON beside it.
pub fn library_ui(
    ui: &mut Ui,
    state: &mut AssetsPanel,
    brush: &crate::brush_panel::BrushPanel,
    selected_layer: Option<u64>,
    search_path: Option<&std::path::Path>,
    saved_path: Option<&std::path::Path>,
) -> LibraryOutcome {
    let mut out = LibraryOutcome::default();
    restore_search(state, search_path);
    ui.horizontal(|ui| {
        ui.label(tr("assets.search"));
        ui.text_edit_singleline(&mut state.search);
    });
    let needle = state.search.to_lowercase();
    let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);

    // K3 — Saved searches: named queries persisted in the state dir's
    // `assets_saved_searches.json`, next to the single-query file above
    // (which stays the last-used query, read-compatible). Collapsible,
    // DEFAULT CLOSED so the resting panel shape doesn't move.
    restore_saved(state, saved_path);
    egui::CollapsingHeader::new(tr("assets.saved-searches"))
        .id_salt("assets-saved-searches")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_search_name)
                        .hint_text(tr("assets.saved-name-hint"))
                        .desired_width(140.0),
                );
                if ui
                    .button(tr("assets.save-current"))
                    .on_hover_text(tr("assets.save-current-tip"))
                    .clicked()
                {
                    let entered = state.new_search_name.trim().to_owned();
                    // No name entered: the query itself names the search.
                    let name = if entered.is_empty() {
                        state.search.trim().to_owned()
                    } else {
                        entered
                    };
                    if !name.is_empty() {
                        let query = state.search.clone();
                        match state.saved.iter_mut().find(|s| s.name == name) {
                            Some(s) => s.query = query,
                            None => state.saved.push(SavedSearch { name, query }),
                        }
                        state.new_search_name.clear();
                    }
                }
            });
            let mut remove: Option<String> = None;
            let mut apply: Option<String> = None;
            for s in &state.saved {
                ui.horizontal(|ui| {
                    if ui
                        .small_button("✕")
                        .on_hover_text(tr("assets.saved-delete-tip"))
                        .clicked()
                    {
                        remove = Some(s.name.clone());
                    }
                    let row = ui
                        .add(egui::Label::new(&s.name).sense(egui::Sense::click()))
                        .on_hover_text(format!("{}: {}", tr("assets.saved-apply-tip"), s.query));
                    row.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, s.name.as_str())
                    });
                    if row.clicked() {
                        apply = Some(s.query.clone());
                    }
                    ui.label(
                        egui::RichText::new(&s.query)
                            .size(umber_gui::theme::TEXT_STATUS)
                            .color(umber_gui::theme::muted()),
                    );
                });
            }
            if let Some(q) = apply {
                state.search = q;
            }
            if let Some(n) = remove {
                state.saved.retain(|s| s.name != n);
            }
        });

    // Extra library folders (persisted in the state dir; the built-in
    // dirs stay first and win same-name dedup). The list caches per
    // scan epoch; adding/removing a folder bumps the epoch, which also
    // rescans the material/mask lists below.
    let dirs_path = document::extra_dirs_path();
    let extra_id = egui::Id::new(("assets-extra-dirs", state.scan_epoch));
    let mut extras: Vec<std::path::PathBuf> = ui.ctx().data(|d| {
        d.get_temp::<Option<Vec<std::path::PathBuf>>>(extra_id)
            .flatten()
            .unwrap_or_default()
    });
    if extras.is_empty() {
        extras = document::load_extra_dirs(dirs_path.as_deref());
        ui.ctx()
            .data_mut(|d| d.insert_temp(extra_id, Some(extras.clone())));
    }
    let mut dir_changed = false;
    egui::CollapsingHeader::new(tr("assets.extra-dirs"))
        .id_salt("assets-extra-dirs-header")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut state.new_dir);
                if ui.small_button("+").clicked() {
                    let p = std::path::PathBuf::from(state.new_dir.trim());
                    if p.is_dir() && !extras.contains(&p) {
                        extras.push(p);
                        dir_changed = true;
                        state.new_dir.clear();
                    }
                }
            });
            let mut remove: Option<std::path::PathBuf> = None;
            for d in &extras {
                ui.horizontal(|ui| {
                    if ui.small_button("✕").clicked() {
                        remove = Some(d.clone());
                    }
                    ui.label(d.display().to_string())
                        .on_hover_text(d.display().to_string());
                });
            }
            if let Some(d) = remove {
                extras.retain(|e| *e != d);
                dir_changed = true;
            }
        });
    if dir_changed {
        document::save_extra_dirs(&extras, dirs_path.as_deref());
        state.scan_epoch += 1;
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new(("assets-extra-dirs", state.scan_epoch)),
                Some(extras.clone()),
            )
        });
    }

    // Brushes: the brush panel's own loaded library (the shelf's
    // source of truth — the Assets browser never re-scans what the
    // brush session already holds).
    let brushes = egui::CollapsingHeader::new(tr("assets.brushes"))
        .id_salt("assets-brushes")
        .default_open(true);
    brushes.show(ui, |ui| {
        let mut shown = 0usize;
        for entry in &brush.library.entries {
            if !matches(&entry.preset.name) {
                continue;
            }
            shown += 1;
            ui.horizontal(|ui| {
                let (card, salt) = swatch_card();
                // K2: a stamped preset's `<stem>.stamp.png` sidecar is
                // its honest swatch, keyed by the file's own stamp so a
                // re-exported tip regenerates; without a sidecar the
                // procedural stroke swatch (U3) is used.
                let stamp = entry.source_path.with_extension("stamp.png");
                let tex = if stamp.is_file() {
                    let key = file_content_key(&stamp);
                    let p = stamp.clone();
                    state.thumb(ui.ctx(), THUMB_KIND_STAMP, &entry.preset.name, &key, || {
                        crate::asset_thumbs::png_file_preview(&p, crate::asset_thumbs::THUMB_PX)
                            .unwrap_or_else(|| {
                                crate::asset_thumbs::brush_preview_on(
                                    &entry.preset,
                                    crate::asset_thumbs::THUMB_PX,
                                    card,
                                )
                            })
                    })
                } else {
                    let json = format!("{salt}{}", entry.preset.to_json().unwrap_or_default());
                    state.thumb(
                        ui.ctx(),
                        THUMB_KIND_BRUSH,
                        &entry.preset.name,
                        &json,
                        || {
                            crate::asset_thumbs::brush_preview_on(
                                &entry.preset,
                                crate::asset_thumbs::THUMB_PX,
                                card,
                            )
                        },
                    )
                };
                if let Some(tex) = tex {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(
                        tex,
                        egui::vec2(24.0, 24.0),
                    )));
                }
                ui.label(&entry.preset.name)
                    .on_hover_text(entry.source_path.display().to_string());
                if ui.small_button("▸").clicked() {
                    out.brush = Some(entry.preset.name.clone());
                }
            });
        }
        // K4: an empty brush list with no starter dir on disk logs a
        // warn once (the first-run diagnostic half).
        warn_empty_without_starter(
            &mut state.starter_warns,
            "brushes",
            brush.library.entries.len(),
            "assets/brushes",
        );
        if brush.library.entries.is_empty() {
            ui.label(tr("assets.empty"));
        } else if shown == 0 {
            ui.label(tr("assets.no-matches"));
        }
    });

    // Smart materials: the hardened loader + the existing apply.
    let refresh = ui
        .horizontal(|ui| {
            ui.strong(tr("assets.materials"));
            ui.small_button("↻").on_hover_text(tr("assets.refresh"))
        })
        .inner
        .clicked();
    let _ = refresh; // The list re-scans each open; the ↻ exists for the eye's rhythm.
    let cache_id = egui::Id::new(("assets-materials", state.scan_epoch));
    let materials: Vec<(String, umber_core::smart::SmartMaterial)> = ui.ctx().data(|d| {
        d.get_temp::<Option<Vec<(String, umber_core::smart::SmartMaterial)>>>(cache_id)
            .flatten()
            .unwrap_or_default()
    });
    let materials = if materials.is_empty() {
        let scanned = document::list_smart_materials(&document::smart_dirs());
        ui.ctx()
            .data_mut(|d| d.insert_temp(cache_id, Some(scanned.clone())));
        scanned
    } else {
        materials
    };
    let mut shown = 0usize;
    for (name, material) in &materials {
        if !matches(name) {
            continue;
        }
        shown += 1;
        ui.horizontal(|ui| {
            let json = serde_json::to_string(material).unwrap_or_default();
            if let Some(tex) = state.thumb(ui.ctx(), THUMB_KIND_MATERIAL, name, &json, || {
                crate::asset_thumbs::material_preview(material, crate::asset_thumbs::THUMB_PX)
            }) {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    tex,
                    egui::vec2(24.0, 24.0),
                )));
            }
            let label = ui
                .add(egui::Label::new(name).sense(egui::Sense::drag()))
                .on_hover_text(tr("assets.drag-to-layers"));
            // g7-accesskit-sweep: the drag-only label still names itself.
            label.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.as_str())
            });
            label.dnd_set_drag_payload(crate::layers_panel::AssetDrag(
                crate::layers_panel::AssetDropRef::Material(name.clone()),
            ));
            if ui.small_button("+").clicked() {
                out.material = Some(material.clone());
            }
        });
    }
    warn_empty_without_starter(
        &mut state.starter_warns,
        "materials",
        materials.len(),
        "assets/smart",
    );
    if materials.is_empty() {
        ui.label(tr("assets.empty"));
    } else if shown == 0 {
        ui.label(tr("assets.no-matches"));
    }

    // Smart masks: the same loader rules + the shell's compose flow.
    let cache_id = egui::Id::new(("assets-masks", state.scan_epoch));
    let masks: Vec<(String, umber_core::smart::SmartMask)> = ui.ctx().data(|d| {
        d.get_temp::<Option<Vec<(String, umber_core::smart::SmartMask)>>>(cache_id)
            .flatten()
            .unwrap_or_default()
    });
    let masks = if masks.is_empty() {
        let scanned = document::list_smart_masks(&document::smart_dirs());
        ui.ctx()
            .data_mut(|d| d.insert_temp(cache_id, Some(scanned.clone())));
        scanned
    } else {
        masks
    };
    let mut shown = 0usize;
    for (name, mask) in &masks {
        if !matches(name) {
            continue;
        }
        shown += 1;
        ui.horizontal(|ui| {
            let json = serde_json::to_string(mask).unwrap_or_default();
            if let Some(tex) = state.thumb(ui.ctx(), THUMB_KIND_MASK, name, &json, || {
                crate::asset_thumbs::mask_preview(mask, crate::asset_thumbs::THUMB_PX)
                    .unwrap_or_else(|| egui::ColorImage::filled([1, 1], egui::Color32::TRANSPARENT))
            }) {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    tex,
                    egui::vec2(24.0, 24.0),
                )));
            }
            let label = ui
                .add(egui::Label::new(name).sense(egui::Sense::drag()))
                .on_hover_text(tr("assets.drag-to-layers"));
            label.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.as_str())
            });
            label.dnd_set_drag_payload(crate::layers_panel::AssetDrag(
                crate::layers_panel::AssetDropRef::Mask(name.clone()),
            ));
            if ui.small_button("+").clicked() {
                if let Some(target) = selected_layer {
                    out.mask = Some((target, mask.clone()));
                }
            }
        });
    }
    warn_empty_without_starter(
        &mut state.starter_warns,
        "masks",
        masks.len(),
        "assets/smart",
    );
    if masks.is_empty() {
        ui.label(tr("assets.empty"));
    } else if shown == 0 {
        ui.label(tr("assets.no-matches"));
    }

    // K1 — Environment maps: the fourth category, DEFAULT CLOSED so the
    // resting panel (and the snapshot pins) don't move; the disk scan
    // runs only while open, cached per scan epoch like the lists above.
    // A row click yields `LibraryOutcome.env` and the shell applies it
    // through the exact File▸Load Environment… path.
    egui::CollapsingHeader::new(tr("assets.env-maps"))
        .id_salt("assets-env-maps")
        .default_open(false)
        .show(ui, |ui| {
            let cache_id = egui::Id::new(("assets-env-maps", state.scan_epoch));
            let envs: Vec<(String, std::path::PathBuf)> = ui.ctx().data(|d| {
                d.get_temp::<Option<Vec<(String, std::path::PathBuf)>>>(cache_id)
                    .flatten()
                    .unwrap_or_default()
            });
            let envs = if envs.is_empty() {
                let scanned = crate::env::list_env_maps(&crate::env::env_dirs());
                ui.ctx()
                    .data_mut(|d| d.insert_temp(cache_id, Some(scanned.clone())));
                scanned
            } else {
                envs
            };
            let mut shown = 0usize;
            for (name, path) in &envs {
                if !matches(name) {
                    continue;
                }
                shown += 1;
                ui.horizontal(|ui| {
                    let key = file_content_key(path);
                    if let Some(tex) = state.thumb(ui.ctx(), THUMB_KIND_ENV, name, &key, || {
                        crate::asset_thumbs::env_preview(path, crate::asset_thumbs::THUMB_PX)
                            .unwrap_or_else(|| {
                                egui::ColorImage::filled(
                                    [crate::asset_thumbs::THUMB_PX as usize; 2],
                                    egui::Color32::TRANSPARENT,
                                )
                            })
                    }) {
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(
                            tex,
                            egui::vec2(24.0, 24.0),
                        )));
                    }
                    let row = ui
                        .add(egui::Label::new(name).sense(egui::Sense::click()))
                        .on_hover_text(path.display().to_string());
                    row.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.as_str())
                    });
                    if row.clicked() {
                        out.env = Some(path.clone());
                    }
                });
            }
            warn_empty_without_starter(
                &mut state.starter_warns,
                "env-maps",
                envs.len(),
                "assets/env",
            );
            if envs.is_empty() {
                ui.label(tr("assets.empty"));
            } else if shown == 0 {
                ui.label(tr("assets.no-matches"));
            }
        });

    // Filters: the fifth category (the landed SmartFilter's
    // `.umberfilter` files), DEFAULT CLOSED so the resting panel (and the
    // snapshot pins) don't move; the disk scan runs only while open,
    // cached per scan epoch like the lists above. A row click yields
    // `LibraryOutcome.filter` and the shell applies it to the active
    // layer as ONE undo step.
    egui::CollapsingHeader::new(tr("assets.filters"))
        .id_salt("assets-filters")
        .default_open(false)
        .show(ui, |ui| {
            let cache_id = egui::Id::new(("assets-filters", state.scan_epoch));
            let filters: Vec<(String, std::path::PathBuf)> = ui.ctx().data(|d| {
                d.get_temp::<Option<Vec<(String, std::path::PathBuf)>>>(cache_id)
                    .flatten()
                    .unwrap_or_default()
            });
            let filters = if filters.is_empty() {
                let scanned = document::list_smart_filters(&document::smart_dirs());
                ui.ctx()
                    .data_mut(|d| d.insert_temp(cache_id, Some(scanned.clone())));
                scanned
            } else {
                filters
            };
            let mut shown = 0usize;
            for (name, path) in &filters {
                if !matches(name) {
                    continue;
                }
                shown += 1;
                ui.horizontal(|ui| {
                    let row = ui
                        .add(egui::Label::new(name).sense(egui::Sense::click()))
                        .on_hover_text(format!(
                            "{}\n{}",
                            path.display(),
                            tr("assets.filter-apply-tip")
                        ));
                    row.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.as_str())
                    });
                    if row.clicked() {
                        out.filter = Some(path.clone());
                    }
                });
            }
            warn_empty_without_starter(
                &mut state.starter_warns,
                "filters",
                filters.len(),
                "assets/smart",
            );
            if filters.is_empty() {
                ui.label(tr("assets.empty"));
            } else if shown == 0 {
                ui.label(tr("assets.no-matches"));
            }
        });
    persist_saved(state, saved_path);
    persist_search(state, search_path);
    out
}

#[cfg(test)]
mod kittest {
    //! The browser kittest (the R-C1 named follow-up), bin-side: the
    //! panel's state is headless-constructible, so the harness drives
    //! `library_ui` directly. Lives here (not tests/) because
    //! assets_panel is a bin module — moving it to the lib is the
    //! larger dual-crate unification, named in the saved-search report.

    use super::*;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    struct AssetsHarness {
        panel: AssetsPanel,
        brush: crate::brush_panel::BrushPanel,
        /// The first non-empty outcome (clicks land on one frame; the
        /// next frame's outcome is empty again).
        hit: Option<LibraryOutcome>,
        search_path: Option<std::path::PathBuf>,
        saved_path: Option<std::path::PathBuf>,
    }

    fn harness() -> Harness<'static, AssetsHarness> {
        harness_with(crate::brush_panel::BrushPanel::new())
    }

    fn harness_with(brush: crate::brush_panel::BrushPanel) -> Harness<'static, AssetsHarness> {
        harness_full(brush, None, None, [480.0, 640.0])
    }

    fn harness_full(
        brush: crate::brush_panel::BrushPanel,
        search_path: Option<std::path::PathBuf>,
        saved_path: Option<std::path::PathBuf>,
        size: [f32; 2],
    ) -> Harness<'static, AssetsHarness> {
        crate::i18n::init();
        Harness::<AssetsHarness>::builder()
            .with_step_dt(1.0 / 60.0)
            .with_size(size)
            .build_ui_state(
                |ui, s: &mut AssetsHarness| {
                    let o = library_ui(
                        ui,
                        &mut s.panel,
                        &s.brush,
                        None,
                        s.search_path.as_deref(),
                        s.saved_path.as_deref(),
                    );
                    if o.material.is_some()
                        || o.mask.is_some()
                        || o.brush.is_some()
                        || o.env.is_some()
                        || o.filter.is_some()
                    {
                        s.hit = Some(o);
                    }
                },
                AssetsHarness {
                    panel: AssetsPanel::default(),
                    brush,
                    hit: None,
                    search_path,
                    saved_path,
                },
            )
    }

    /// U3: the shelf draws one swatch per library preset, a click hands
    /// back that preset's name, and the active one is the selected one.
    #[test]
    fn the_brush_shelf_picks_presets_by_swatch() {
        struct Shelf {
            panel: AssetsPanel,
            brush: crate::brush_panel::BrushPanel,
            picked: Option<String>,
        }
        crate::i18n::init();
        let mut h = Harness::<Shelf>::builder()
            .with_size([900.0, 80.0])
            .build_ui_state(
                |ui, s: &mut Shelf| {
                    if let Some(name) = brush_shelf_ui(ui, &mut s.panel, &s.brush) {
                        s.brush.select_preset_by_name(&name);
                        s.picked = Some(name);
                    }
                },
                Shelf {
                    panel: AssetsPanel::default(),
                    brush: {
                        // The repo's bundled presets (the test exe's dir
                        // has none; the brush_panel tests' own seam).
                        let mut b = crate::brush_panel::BrushPanel::new();
                        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../assets/brushes");
                        b.library = umber_brush::Library::load_from_dirs(&[dir]);
                        b.active = None;
                        b
                    },
                    picked: None,
                },
            );
        h.run();
        let names: Vec<String> = h
            .state()
            .brush
            .library
            .entries
            .iter()
            .map(|e| e.preset.name.clone())
            .collect();
        assert!(names.len() >= 10, "the bundled library: {names:?}");
        let buttons = |h: &Harness<'_, Shelf>, name: &str| {
            h.query_all_by_role_and_label(egui::accesskit::Role::Button, name)
                .count()
        };
        for name in &names {
            assert_eq!(buttons(&h, name), 1, "one swatch button for {name}");
        }
        assert_eq!(
            h.state().panel.thumbs.len(),
            names.len(),
            "one cached swatch each"
        );
        let target = names.last().unwrap().clone();
        // The shelf scrolls horizontally: with the wave-2 library (18
        // brushes) the last swatch sits past the 900 px harness, where a
        // click lands on nothing. Scroll it into view first, as a user would.
        h.get_by_role_and_label(egui::accesskit::Role::Button, &target)
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::Button, &target)
            .click();
        h.run();
        assert_eq!(h.state().picked.as_deref(), Some(target.as_str()));
        let active = h.state().brush.active.map(|i| names[i].clone());
        assert_eq!(
            active.as_deref(),
            Some(target.as_str()),
            "the pick became active"
        );
    }

    fn seeded_brushes() -> crate::brush_panel::BrushPanel {
        let mut brush = crate::brush_panel::BrushPanel::new();
        brush.library = umber_brush::Library::load_from_dirs(&[std::path::Path::new(env!(
            "CARGO_MANIFEST_DIR"
        ))
        .join("../../assets/brushes")]);
        brush.active = None;
        brush
    }

    /// The popup groups by catalog: each catalog's swatches start a new
    /// row and wrap at POPUP_COLUMNS, so the row count is the sum of
    /// ceil(len / columns) over the catalogs.
    #[test]
    fn the_brush_popup_grid_rows_follow_the_catalogs() {
        struct Grid {
            panel: AssetsPanel,
            brush: crate::brush_panel::BrushPanel,
        }
        crate::i18n::init();
        let brush = seeded_brushes();
        let catalogs = brush_catalogs(&brush);
        assert!(
            catalogs.len() > 1,
            "the starter library files several catalogs"
        );
        let names: Vec<String> = brush
            .library
            .entries
            .iter()
            .map(|e| e.preset.name.clone())
            .collect();
        let mut h = Harness::<Grid>::builder()
            .with_size([900.0, 900.0])
            .build_ui_state(
                |ui, s: &mut Grid| {
                    let _ = brush_grid_ui(ui, &mut s.panel, &s.brush);
                },
                Grid {
                    panel: AssetsPanel::default(),
                    brush,
                },
            );
        h.run();
        let mut rows: Vec<i32> = Vec::new();
        for name in &names {
            let node = h.get_by_role_and_label(egui::accesskit::Role::Button, name);
            let y = node.rect().min.y.round() as i32;
            if !rows.contains(&y) {
                rows.push(y);
            }
        }
        let expected: usize = catalogs
            .iter()
            .map(|(_, v)| v.len().div_ceil(POPUP_COLUMNS))
            .sum();
        assert_eq!(rows.len(), expected);
        for (id, _) in &catalogs {
            assert!(
                h.query_by_label(&catalog_label(id)).is_some(),
                "header {id}"
            );
        }
    }

    /// The shelf's catalog tabs filter the strip; All shows everything.
    #[test]
    fn the_shelf_catalog_tabs_filter_the_swatches() {
        struct Shelf {
            panel: AssetsPanel,
            brush: crate::brush_panel::BrushPanel,
        }
        crate::i18n::init();
        let brush = seeded_brushes();
        let catalogs = brush_catalogs(&brush);
        let (erase_id, erase_ix) = catalogs
            .iter()
            .find(|(c, _)| c == "Erase")
            .cloned()
            .expect("an Erase catalog ships");
        let erase_names: Vec<String> = erase_ix
            .iter()
            .map(|&i| brush.library.entries[i].preset.name.clone())
            .collect();
        let all = brush.library.entries.len();
        let mut h = Harness::<Shelf>::builder()
            .with_size([1600.0, 140.0])
            .build_ui_state(
                |ui, s: &mut Shelf| {
                    let _ = brush_shelf_ui(ui, &mut s.panel, &s.brush);
                },
                Shelf {
                    panel: AssetsPanel::default(),
                    brush,
                },
            );
        h.run();
        let swatches = |h: &Harness<'_, Shelf>| {
            h.state()
                .brush
                .library
                .entries
                .iter()
                .filter(|e| {
                    h.query_by_role_and_label(egui::accesskit::Role::Button, &e.preset.name)
                        .is_some()
                })
                .count()
        };
        assert_eq!(swatches(&h), all, "All shows every preset");
        h.get_by_label(&catalog_label(&erase_id)).click();
        h.run();
        assert_eq!(h.state().panel.shelf_catalog.as_deref(), Some("Erase"));
        assert_eq!(swatches(&h), erase_names.len());
        for n in &erase_names {
            assert!(h
                .query_by_role_and_label(egui::accesskit::Role::Button, n)
                .is_some());
        }
        h.get_by_label(&tr("assets.catalog-all")).click();
        h.run();
        assert_eq!(swatches(&h), all);
    }

    #[test]
    fn catalog_labels_localize_shipped_ids_and_keep_user_ones() {
        crate::i18n::init();
        assert_eq!(catalog_label("Basic"), tr("assets.catalog-basic"));
        assert_ne!(tr("assets.catalog-basic"), "assets.catalog-basic");
        assert_eq!(catalog_label("My Inks"), "My Inks");
    }

    #[test]
    fn browser_lists_bundled_presets_and_applies_on_plus() {
        let mut h = harness();
        assert!(h.run_ok().is_some(), "the browser settles");
        // The bundled library (assets/smart) includes the original pair
        // of materials and the edge-wear mask.
        assert!(
            h.query_by_label("Worn metal").is_some(),
            "material row (display name)"
        );
        assert!(
            h.query_by_label("Painted wood").is_some(),
            "material row (display name)"
        );
        assert!(h.query_by_label("Edge wear").is_some(), "mask row");
        // The first + button is the first material row. Materials render
        // before masks in the scan's own order; the expected name comes
        // from that scan, so growing the starter library (K7-K9 added ten
        // materials ahead of "Painted wood") never breaks this pin.
        let first = document::list_smart_materials(&document::smart_dirs())
            .into_iter()
            .next()
            .map(|(_, m)| m.name)
            .expect("the bundled library has materials");
        let plus = h.get_all_by_label("+").next().expect("row buttons");
        plus.click();
        assert!(h.run_ok().is_some());
        let out = h
            .state_mut()
            .hit
            .take()
            .expect("the + click produced an outcome");
        assert_eq!(
            out.material.as_ref().map(|m| m.name.as_str()),
            Some(first.as_str()),
            "the first material row's + applies that material"
        );
    }

    #[test]
    fn search_filters_the_library_rows() {
        let mut h = harness();
        assert!(h.run_ok().is_some());
        h.state_mut().panel.search = "worn".into();
        assert!(h.run_ok().is_some());
        assert!(h.query_by_label("Worn metal").is_some());
        assert!(h.query_by_label("Painted wood").is_none(), "filtered out");
        assert!(
            h.query_by_label("Edge wear").is_none(),
            "mask filtered out too"
        );
    }

    /// K1: the fourth category draws its header but is DEFAULT CLOSED
    /// (no env rows in the resting panel); opening it lists the bundled
    /// studio maps, and clicking a row yields its path in the outcome.
    #[test]
    fn the_env_maps_category_is_closed_by_default_and_applies_on_click() {
        // Tall: the wave-2 library (31 smart assets) pushes the fourth
        // category below a 640 px panel, where a click lands on nothing
        // and the opened rows draw off-screen (egui culls them).
        let mut h = harness_full(
            crate::brush_panel::BrushPanel::new(),
            None,
            None,
            [480.0, 2400.0],
        );
        assert!(h.run_ok().is_some());
        let header = tr("assets.env-maps");
        assert!(h.query_by_label(&header).is_some(), "the header draws");
        assert!(
            h.query_by_label("studio-neutral").is_none(),
            "closed by default: no env rows in the resting panel"
        );
        h.get_by_label(&header).click();
        assert!(h.run_ok().is_some());
        assert!(
            h.query_by_label("studio-neutral").is_some(),
            "opened: the bundled studio maps list"
        );
        h.get_by_label("studio-neutral").click();
        assert!(h.run_ok().is_some());
        let out = h
            .state_mut()
            .hit
            .take()
            .expect("the row click produced an outcome");
        let path = out.env.expect("the env outcome carries the path");
        assert!(
            path.ends_with("studio-neutral.png"),
            "the clicked row's file: {}",
            path.display()
        );
    }

    /// The fifth category draws its header but is DEFAULT CLOSED (the
    /// resting panel — and the snapshot pins — don't move); opened, it
    /// lists the scanned `.umberfilter` files, and clicking a row yields
    /// its PATH in the outcome's `filter` (the shell applies it to the
    /// active layer). The scan dir here is the test exe's own `smart/`
    /// sidecar (a `.umberfilter` touches only the filters lister, so no
    /// other test's scan sees it).
    #[test]
    fn the_filters_category_is_closed_by_default_and_yields_the_path_on_click() {
        let exe_smart = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("smart");
        std::fs::create_dir_all(&exe_smart).unwrap();
        let filter_path = exe_smart.join("kittest-crunch.umberfilter");
        std::fs::write(
            &filter_path,
            r#"{"name":"Kittest crunch","version":1,"ops":[{"type":"posterize","levels":3}]}"#,
        )
        .unwrap();
        let mut h = harness_full(seeded_brushes(), None, None, [480.0, 1600.0]);
        assert!(h.run_ok().is_some());
        let header = tr("assets.filters");
        assert!(h.query_by_label(&header).is_some(), "the header draws");
        assert!(
            h.query_by_label("Kittest crunch").is_none(),
            "closed by default: no filter rows in the resting panel"
        );
        h.get_by_label(&header).click();
        assert!(h.run_ok().is_some());
        assert!(
            h.query_by_label("Kittest crunch").is_some(),
            "opened: the scanned .umberfilter lists"
        );
        h.get_by_label("Kittest crunch").click();
        assert!(h.run_ok().is_some());
        let out = h
            .state_mut()
            .hit
            .take()
            .expect("the row click produced an outcome");
        let path = out.filter.expect("the filter outcome carries the path");
        assert_eq!(path, filter_path, "the clicked row's file");
        let _ = std::fs::remove_file(&filter_path);
    }

    /// K2: a search that empties a non-empty list shows the distinct
    /// no-matches label (once per emptied category); an empty library
    /// would still show assets.empty, which stays absent here.
    #[test]
    fn search_with_no_matches_shows_no_matches_not_empty() {
        let mut h = harness_with(seeded_brushes());
        assert!(h.run_ok().is_some());
        assert!(
            h.query_by_label(&tr("assets.no-matches")).is_none(),
            "empty search: the lists are full"
        );
        h.state_mut().panel.search = "zzz-no-such-asset".into();
        assert!(h.run_ok().is_some());
        let nm = tr("assets.no-matches");
        // Brushes + materials + masks are all non-empty and fully
        // filtered out → one no-matches label each; the env category is
        // closed, so it draws none.
        let count = h.query_all_by_label(&nm).count();
        assert_eq!(
            count, 3,
            "one no-matches per non-empty emptied list: {count}"
        );
        assert!(
            h.query_by_label(&tr("assets.empty")).is_none(),
            "no empty-library label while the lists hold rows"
        );
        assert!(h.query_by_label("Worn metal").is_none());
    }

    /// K3: saved searches save-current / apply / delete through the UI
    /// and persist to the state-dir JSON file (the single-query file is
    /// a separate, untouched path).
    #[test]
    fn saved_searches_save_apply_delete_and_persist() {
        let dir = std::env::temp_dir().join(format!("umber-saved-searches-{}", std::process::id()));
        let saved_path = dir.join("assets_saved_searches.json");
        let _ = std::fs::remove_file(&saved_path);
        let mut h = harness_full(
            seeded_brushes(),
            None,
            Some(saved_path.clone()),
            [480.0, 640.0],
        );
        assert!(h.run_ok().is_some());
        // DEFAULT CLOSED: no save button in the resting panel.
        assert!(h.query_by_label(&tr("assets.save-current")).is_none());
        h.get_by_label(&tr("assets.saved-searches")).click();
        assert!(h.run_ok().is_some());
        // Type a query, name it, save current.
        h.state_mut().panel.search = "worn".into();
        h.state_mut().panel.new_search_name = "Wear".into();
        h.get_by_label(&tr("assets.save-current")).click();
        assert!(h.run_ok().is_some());
        assert_eq!(
            h.state().panel.saved,
            vec![SavedSearch {
                name: "Wear".into(),
                query: "worn".into()
            }]
        );
        assert!(saved_path.exists(), "persisted to the state-dir file");
        let on_disk: Vec<SavedSearch> =
            serde_json::from_str(&std::fs::read_to_string(&saved_path).unwrap()).unwrap();
        assert_eq!(on_disk, h.state().panel.saved, "the file holds the row");
        // The row draws; clearing the search then applying restores it.
        assert!(h.query_by_label("Wear").is_some(), "the saved row");
        h.state_mut().panel.search.clear();
        assert!(h.run_ok().is_some());
        h.get_by_label("Wear").click();
        assert!(h.run_ok().is_some());
        assert_eq!(h.state().panel.search, "worn", "apply restores the query");
        // Delete removes the row and persists the empty list.
        h.get_by_label("✕").click();
        assert!(h.run_ok().is_some());
        assert!(h.state().panel.saved.is_empty());
        let on_disk: Vec<SavedSearch> =
            serde_json::from_str(&std::fs::read_to_string(&saved_path).unwrap()).unwrap();
        assert!(on_disk.is_empty(), "the delete persisted");
        // A fresh panel restores the (empty) list from the same file.
        let mut p = AssetsPanel::default();
        restore_saved(&mut p, Some(&saved_path));
        assert!(p.saved.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// K4: a fresh-state panel (zero user state — the first-run shape)
    /// lists the bundled starter rows in every category: brushes, smart
    /// materials, masks, and — opened — env maps.
    #[test]
    fn a_fresh_state_panel_lists_the_bundled_starter_rows() {
        // Tall window: the env-maps header sits on-screen so its click
        // lands (at 640px with a full brush list it is below the fold).
        let mut h = harness_full(seeded_brushes(), None, None, [480.0, 1600.0]);
        assert!(h.run_ok().is_some());
        let brush_names: Vec<String> = h
            .state()
            .brush
            .library
            .entries
            .iter()
            .map(|e| e.preset.name.clone())
            .collect();
        assert!(
            brush_names.len() >= 10,
            "the bundled brush library: {brush_names:?}"
        );
        for name in &brush_names {
            assert!(h.query_by_label(name).is_some(), "brush row {name}");
        }
        assert!(h.query_by_label("Worn metal").is_some(), "starter material");
        assert!(
            h.query_by_label("Painted wood").is_some(),
            "starter material"
        );
        assert!(h.query_by_label("Edge wear").is_some(), "starter mask");
        // Env maps: closed by default; opened, the bundled studio maps list.
        assert!(h.query_by_label("studio-neutral").is_none());
        h.get_by_label(&tr("assets.env-maps")).click();
        assert!(h.run_ok().is_some());
        assert!(
            h.query_by_label("studio-neutral").is_some(),
            "starter env map"
        );
    }

    /// K2: a brush row with a `<stem>.stamp.png` sidecar keys its swatch
    /// by the stamp file (THUMB_KIND_STAMP). The bundled shelf ships
    /// sidecars for some starter presets (the content lane), so the pin
    /// is: a stamp key exists exactly for the presets that HAVE a
    /// sidecar, and those rows never also key the procedural swatch.
    #[test]
    fn brush_rows_key_stamp_sidecar_swatches_through_the_thumb_cache() {
        crate::i18n::init();
        let dir = std::env::temp_dir().join(format!("umber-stamp-swatch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::copy(
            repo.join("assets/brushes/hard-round.umberbrush"),
            dir.join("hard-round.umberbrush"),
        )
        .unwrap();
        // A real PNG as the tip (the committed studio map stands in).
        std::fs::copy(
            repo.join("assets/env/studio-neutral.png"),
            dir.join("hard-round.stamp.png"),
        )
        .unwrap();
        let mut brush = crate::brush_panel::BrushPanel::new();
        brush.library = umber_brush::Library::load_from_dirs(std::slice::from_ref(&dir));
        brush.active = None;
        let name = brush.library.entries[0].preset.name.clone();
        let mut h = harness_with(brush);
        assert!(h.run_ok().is_some());
        let keys: Vec<_> = h.state().panel.thumbs.keys().cloned().collect();
        assert!(
            keys.iter()
                .any(|k| k.kind == THUMB_KIND_STAMP && k.name == name),
            "the stamp sidecar keyed its own swatch: {keys:?}"
        );
        assert!(
            !keys
                .iter()
                .any(|k| k.kind == THUMB_KIND_BRUSH && k.name == name),
            "the procedural key is not used when the sidecar exists"
        );
        let mut h2 = harness_with(seeded_brushes());
        assert!(h2.run_ok().is_some());
        let with_sidecar: Vec<String> = h2
            .state()
            .brush
            .library
            .entries
            .iter()
            .filter(|e| e.source_path.with_extension("stamp.png").is_file())
            .map(|e| e.preset.name.clone())
            .collect();
        let keys: Vec<_> = h2.state().panel.thumbs.keys().cloned().collect();
        for name in &with_sidecar {
            assert!(
                keys.iter()
                    .any(|k| k.kind == THUMB_KIND_STAMP && &k.name == name),
                "sidecar preset {name} got its stamp-keyed swatch"
            );
            assert!(
                !keys
                    .iter()
                    .any(|k| k.kind == THUMB_KIND_BRUSH && &k.name == name),
                "the procedural key is not used when the sidecar exists: {name}"
            );
        }
        assert!(
            keys.iter()
                .all(|k| k.kind != THUMB_KIND_STAMP || with_sidecar.contains(&k.name)),
            "no phantom stamp keys without a sidecar file: {keys:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The K1/K2/K3/K4 key pins: the new keys resolve — and actually
    /// translate — in BOTH shipped catalogs; the empty-library label is
    /// unchanged by K2.
    #[test]
    fn the_new_assets_keys_resolve_in_both_shipped_catalogs() {
        crate::i18n::init();
        let mut pt = crate::i18n::I18n::builtin();
        assert_eq!(pt.set_locale("pt-BR"), "pt-BR");
        for key in [
            "assets.env-maps",
            "assets.filters",
            "assets.filter-apply-tip",
            "assets.no-matches",
            "assets.saved-searches",
            "assets.save-current",
            "assets.save-current-tip",
            "assets.saved-name-hint",
            "assets.saved-apply-tip",
            "assets.saved-delete-tip",
            "home.browse-assets",
            "home.browse-assets-tip",
        ] {
            let en = crate::i18n::tr_en(key);
            assert_ne!(en, key, "en-US defines {key}");
            let pt_s = pt.lookup(key, None);
            assert_ne!(pt_s, key, "pt-BR defines {key}");
            assert_ne!(pt_s, en, "pt-BR translates {key}");
        }
        assert_eq!(crate::i18n::tr_en("assets.empty"), "Nothing here yet.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_persists_and_restores_through_a_file() {
        let dir = std::env::temp_dir().join(format!("umber-assets-search-{}", std::process::id()));
        let path = dir.join("assets_search");
        let _ = std::fs::remove_file(&path);
        let mut a = AssetsPanel::default();
        restore_search(&mut a, Some(&path));
        assert_eq!(a.search, "", "no file yet = empty");
        a.search = "metal".into();
        persist_search(&mut a, Some(&path));
        assert!(path.exists(), "the change was written");
        let mut b = AssetsPanel::default();
        restore_search(&mut b, Some(&path));
        assert_eq!(b.search, "metal", "the next session reopens on it");
        // No change → no rewrite (mtime stable).
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        persist_search(&mut b, Some(&path));
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), mtime);
        // Newlines never survive into the single-line field.
        std::fs::write(&path, "a\nb\n").unwrap();
        let mut c = AssetsPanel::default();
        restore_search(&mut c, Some(&path));
        assert_eq!(c.search, "a b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_searches_persist_and_restore_through_a_file() {
        let dir = std::env::temp_dir().join(format!("umber-assets-saved-{}", std::process::id()));
        let path = dir.join("assets_saved_searches.json");
        let _ = std::fs::remove_file(&path);
        let mut a = AssetsPanel::default();
        restore_saved(&mut a, Some(&path));
        assert!(a.saved.is_empty(), "no file yet = empty");
        a.saved = vec![
            SavedSearch {
                name: "metals".into(),
                query: "metal".into(),
            },
            SavedSearch {
                name: "wear".into(),
                query: "worn".into(),
            },
        ];
        persist_saved(&mut a, Some(&path));
        assert!(path.exists(), "the change was written");
        let mut b = AssetsPanel::default();
        restore_saved(&mut b, Some(&path));
        assert_eq!(b.saved, a.saved, "the next session reopens on them");
        // No change → no rewrite (mtime stable).
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        persist_saved(&mut b, Some(&path));
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), mtime);
        // A corrupt file reads as empty, never a panic.
        std::fs::write(&path, "{not json").unwrap();
        let mut c = AssetsPanel::default();
        restore_saved(&mut c, Some(&path));
        assert!(c.saved.is_empty());
        // None path: loads and saves are both no-ops.
        let mut d = AssetsPanel::default();
        restore_saved(&mut d, None);
        d.saved.push(SavedSearch {
            name: "x".into(),
            query: String::new(),
        });
        persist_saved(&mut d, None);
        assert_eq!(d.saved.len(), 1, "the session keeps the list");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_category_warns_only_when_the_starter_dir_is_missing_too() {
        assert!(empty_without_starter_warns(0, false), "both missing: warn");
        assert!(
            !empty_without_starter_warns(0, true),
            "starter found: the list may be legitimately empty"
        );
        assert!(!empty_without_starter_warns(4, false), "non-empty: silent");
        assert!(!empty_without_starter_warns(4, true));
    }
}
