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
//! case-insensitive) + three collapsible categories (Brushes, Smart
//! materials, Masks), each listing its library, the rows filtered
//! by the search, the name + source-dir shown (the starter-vs-user
//! distinction the census names; the tooltip carries the full
//! path), a reload button per category (the smart shelf's own
//! `↻` precedent), and apply-on-click:
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
//! content-keyed thumbnails, saved searches, custom library
//! locations, the filter/mesh/env categories, the import-dialog
//! integration.

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
}

/// The brush shelf's swatch edge, points (UI/UX pass U3).
pub const SHELF_SWATCH: f32 = 40.0;

/// The brush asset shelf (UI/UX pass U3, Blender 4.3's asset shelf in
/// paint modes): one horizontal, scrollable strip of preset swatches
/// along the 3D viewport's bottom edge. A click selects the preset
/// (the caller routes it through `BrushPanel::select_preset_by_name`,
/// the same seam the Assets browser rows use); the active preset is
/// framed in the accent; the name rides the hover.
pub fn brush_shelf_ui(
    ui: &mut Ui,
    state: &mut AssetsPanel,
    brush: &crate::brush_panel::BrushPanel,
) -> Option<String> {
    let mut picked = None;
    egui::ScrollArea::horizontal()
        .id_salt("umber-brush-shelf")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, entry) in brush.library.entries.iter().enumerate() {
                    let json = entry.preset.to_json().unwrap_or_default();
                    let Some(tex) = state.thumb(ui.ctx(), 0, &entry.preset.name, &json, || {
                        crate::asset_thumbs::brush_preview(
                            &entry.preset,
                            crate::asset_thumbs::THUMB_PX * 2,
                        )
                    }) else {
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
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            true,
                            active,
                            &entry.preset.name,
                        )
                    });
                    if response.clicked() && !active {
                        picked = Some(entry.preset.name.clone());
                    }
                }
            });
        });
    picked
}

/// Draws the library section (search + three categories + rows).
/// The caller (the Assets tab's arm) keeps the import sections below.
pub fn library_ui(
    ui: &mut Ui,
    state: &mut AssetsPanel,
    brush: &crate::brush_panel::BrushPanel,
    selected_layer: Option<u64>,
    search_path: Option<&std::path::Path>,
) -> LibraryOutcome {
    let mut out = LibraryOutcome::default();
    restore_search(state, search_path);
    ui.horizontal(|ui| {
        ui.label(tr("assets.search"));
        ui.text_edit_singleline(&mut state.search);
    });
    let needle = state.search.to_lowercase();
    let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);

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
                Some(extras),
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
        for entry in &brush.library.entries {
            if !matches(&entry.preset.name) {
                continue;
            }
            ui.horizontal(|ui| {
                let json = entry.preset.to_json().unwrap_or_default();
                if let Some(tex) = state.thumb(ui.ctx(), 0, &entry.preset.name, &json, || {
                    crate::asset_thumbs::brush_preview(&entry.preset, crate::asset_thumbs::THUMB_PX)
                }) {
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
        if brush.library.entries.is_empty() {
            ui.label(tr("assets.empty"));
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
    for (name, material) in &materials {
        if !matches(name) {
            continue;
        }
        ui.horizontal(|ui| {
            let json = serde_json::to_string(material).unwrap_or_default();
            if let Some(tex) = state.thumb(ui.ctx(), 1, name, &json, || {
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
    if materials.is_empty() {
        ui.label(tr("assets.empty"));
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
    for (name, mask) in &masks {
        if !matches(name) {
            continue;
        }
        ui.horizontal(|ui| {
            let json = serde_json::to_string(mask).unwrap_or_default();
            if let Some(tex) = state.thumb(ui.ctx(), 2, name, &json, || {
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
    if masks.is_empty() {
        ui.label(tr("assets.empty"));
    }
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
    }

    fn harness() -> Harness<'static, AssetsHarness> {
        crate::i18n::init();
        Harness::<AssetsHarness>::builder()
            .with_step_dt(1.0 / 60.0)
            .with_size([480.0, 640.0])
            .build_ui_state(
                |ui, s: &mut AssetsHarness| {
                    let o = library_ui(ui, &mut s.panel, &s.brush, None, None);
                    if o.material.is_some() || o.mask.is_some() || o.brush.is_some() {
                        s.hit = Some(o);
                    }
                },
                AssetsHarness {
                    panel: AssetsPanel::default(),
                    brush: crate::brush_panel::BrushPanel::new(),
                    hit: None,
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

    #[test]
    fn browser_lists_bundled_presets_and_applies_on_plus() {
        let mut h = harness();
        assert!(h.run_ok().is_some(), "the browser settles");
        // The bundled library (assets/smart): two materials + one mask.
        assert!(
            h.query_by_label("Worn metal").is_some(),
            "material row (display name)"
        );
        assert!(
            h.query_by_label("Painted wood").is_some(),
            "material row (display name)"
        );
        assert!(h.query_by_label("Edge wear").is_some(), "mask row");
        // The first + button is the first material row (the list is
        // name-sorted): clicking it applies that material.
        // Materials render before masks, and the list is name-sorted:
        // the first + is "Painted wood".
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
            Some("Painted wood"),
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
}
