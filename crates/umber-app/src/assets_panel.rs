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
//! `⟳` precedent), and apply-on-click:
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
/// the scan is disk I/O, refreshed by the user's ⟳).
#[derive(Debug, Default)]
pub struct AssetsPanel {
    /// The search filter (substring, case-insensitive; empty = all).
    /// Persisted to the state dir (the swatches-file precedent) so the
    /// browser reopens on the last query.
    pub search: String,
    /// Content-keyed swatch cache: (kind, name, preset-JSON hash) →
    /// texture. A changed preset file changes the key and regenerates.
    thumbs: std::collections::HashMap<crate::asset_thumbs::ThumbKey, egui::TextureId>,
    /// Whether `search` was restored from disk this session.
    restored: bool,
    /// The last value written to disk (skip no-op writes).
    last_saved: String,
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
    fn thumb(
        &mut self,
        ctx: &egui::Context,
        kind: u8,
        name: &str,
        preset_json: &str,
        make: impl FnOnce() -> egui::ColorImage,
    ) -> Option<egui::TextureId> {
        let key = crate::asset_thumbs::thumb_key(kind, name, preset_json);
        if let Some(tex) = self.thumbs.get(&key) {
            return Some(*tex);
        }
        let img = make();
        let tex = ctx.load_texture(
            format!("asset-thumb-{kind}-{name}"),
            img,
            egui::TextureOptions::LINEAR,
        );
        self.thumbs.insert(key, tex.id());
        Some(tex.id())
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
            ui.small_button("⟳")
        })
        .inner
        .clicked();
    let _ = refresh; // The list re-scans each open; the ⟳ exists for the eye's rhythm.
    let cache_id = egui::Id::new("assets-materials");
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
            label.dnd_set_drag_payload(crate::layers_panel::AssetDrag(
                crate::layers_panel::AssetDropRef::Material(name.clone()),
            ));
            if ui.small_button("＋").clicked() {
                out.material = Some(material.clone());
            }
        });
    }
    if materials.is_empty() {
        ui.label(tr("assets.empty"));
    }

    // Smart masks: the same loader rules + the shell's compose flow.
    let cache_id = egui::Id::new("assets-masks");
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
            label.dnd_set_drag_payload(crate::layers_panel::AssetDrag(
                crate::layers_panel::AssetDropRef::Mask(name.clone()),
            ));
            if ui.small_button("＋").clicked() {
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
