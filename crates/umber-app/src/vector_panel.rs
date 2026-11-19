//! Vector-layer property rows (K4). Every edit goes through K3's
//! `LayerCommand::SetVector`, so fill/stroke changes share canvas editing's
//! undo, journal, adoption, and raster-refresh path.

use umber_core::vector::{Stroke, VectorParams, VectorShape};
use umber_core::{LayerCommand, LayerKind};

use crate::document::Document;
use crate::i18n::tr;

const DEFAULT_STROKE_WIDTH: f32 = 0.01;

/// One user-selected panel action result, handed to the shell's notice rail.
pub enum VectorPanelOutcome {
    ExportSaved(std::path::PathBuf),
    Imported(u64),
    ExportFailed(String),
    ImportFailed(String),
}

/// Search labels in draw order. Stroke-only rows stay listed while hidden,
/// matching the Fill section's static-vocabulary convention.
const ROW_LABEL_KEYS: &[&str] = &[
    "vector.section",
    "vector.closed",
    "vector.fill",
    "vector.stroke",
    "vector.stroke-width",
    "vector.stroke-color",
    "vector.export-svg",
];

pub fn row_labels() -> Vec<String> {
    ROW_LABEL_KEYS.iter().copied().map(tr).collect()
}

/// One collapsible property section per vector layer. A fresh `None` layer
/// edits the same visible default as the canvas; the first real change creates
/// its params through one undoable command.
pub fn vector_section_ui(ui: &mut egui::Ui, doc: &mut Document) -> Option<VectorPanelOutcome> {
    let mut outcome = None;
    ui.add_space(8.0);
    ui.separator();
    ui.strong(tr("vector.section"));
    ui.add_space(4.0);
    if ui.button(tr("vector.import-svg")).clicked() {
        outcome = import_one(doc);
    }
    ui.add_space(4.0);
    let vectors: Vec<(u64, String, Option<VectorParams>)> = doc
        .layers()
        .iter()
        .filter(|layer| layer.kind == LayerKind::Vector)
        .map(|layer| (layer.id, layer.name.clone(), layer.vector.clone()))
        .collect();
    if vectors.is_empty() {
        ui.label(tr("vector.none"));
        return None;
    }
    for (id, name, params) in vectors {
        egui::CollapsingHeader::new(&name)
            .default_open(true)
            .show(ui, |ui| {
                let mut edited = params.unwrap_or_default();
                let mut changed = false;
                if ui
                    .checkbox(&mut edited.shape.closed, tr("vector.closed"))
                    .changed()
                {
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr("vector.fill")).color(umber_gui::muted()));
                    if ui
                        .color_edit_button_srgba_unmultiplied(&mut edited.shape.fill)
                        .changed()
                    {
                        changed = true;
                    }
                });
                let mut stroke_enabled = edited.shape.stroke.is_some();
                if ui
                    .checkbox(&mut stroke_enabled, tr("vector.stroke"))
                    .changed()
                {
                    set_stroke_enabled(&mut edited.shape, stroke_enabled);
                    changed = true;
                }
                if let Some(stroke) = edited.shape.stroke.as_mut() {
                    if field_row(
                        ui,
                        &tr("vector.stroke-width"),
                        umber_gui::ValueField::new(&mut stroke.width)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(0.0001..=1.0),
                    ) {
                        changed = true;
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr("vector.stroke-color"))
                                .color(umber_gui::muted()),
                        );
                        if ui
                            .color_edit_button_srgba_unmultiplied(&mut stroke.color)
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }
                let export_clicked = ui.button(tr("vector.export-svg")).clicked();
                if changed {
                    doc.run(LayerCommand::set_vector(id, edited.clone()));
                }
                if export_clicked {
                    outcome = export_one(&name, &edited);
                }
            });
    }
    outcome
}

/// Opens the native save dialog and writes only the user-selected path.
/// Cancel returns `None`; serialization and filesystem failures stay visible.
fn export_one(name: &str, params: &VectorParams) -> Option<VectorPanelOutcome> {
    let svg = match umber_core::vector_svg::export_svg(&params.shape) {
        Ok(svg) => svg,
        Err(error) => return Some(VectorPanelOutcome::ExportFailed(error.to_string())),
    };
    let path = rfd::FileDialog::new()
        .add_filter("SVG", &["svg"])
        .set_file_name(suggested_svg_name(name))
        .save_file()?;
    match std::fs::write(&path, svg) {
        Ok(()) => Some(VectorPanelOutcome::ExportSaved(path)),
        Err(error) => Some(VectorPanelOutcome::ExportFailed(error.to_string())),
    }
}

/// Opens the native open dialog, refuses oversized files before reading them,
/// parses only after the whole read is in memory, and installs the result as
/// one undoable add + set. Cancel and every failure leave the document intact.
fn import_one(doc: &mut Document) -> Option<VectorPanelOutcome> {
    let path = rfd::FileDialog::new()
        .add_filter("SVG", &["svg"])
        .pick_file()?;
    let bytes = match read_svg_capped(&path) {
        Ok(bytes) => bytes,
        Err(error) => return Some(VectorPanelOutcome::ImportFailed(error)),
    };
    let params = match umber_core::vector_svg::import_svg(&bytes) {
        Ok(params) => params,
        Err(error) => return Some(VectorPanelOutcome::ImportFailed(error.to_string())),
    };
    let id = install_imported_params(doc, params);
    Some(VectorPanelOutcome::Imported(id))
}

fn validate_svg_len(bytes: usize) -> Result<(), String> {
    if bytes > umber_core::svg::MAX_SVG_INPUT_BYTES {
        return Err(format!(
            "SVG exceeds the {}-byte import budget",
            umber_core::svg::MAX_SVG_INPUT_BYTES
        ));
    }
    Ok(())
}

fn read_svg_capped(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_svg_len(metadata.len() as usize)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_svg_len(bytes.len()).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(bytes)
}

/// Adds one vector layer and installs its parsed params in a single history
/// group, so undo removes the imported layer instead of leaving an empty one.
fn install_imported_params(doc: &mut Document, params: VectorParams) -> u64 {
    let mut id = None;
    doc.run_grouped(crate::history_panel::HistoryKind::AddVector, |doc| {
        doc.run(LayerCommand::add(
            tr("vector.imported-default"),
            LayerKind::Vector,
        ));
        id = doc.layers().last().map(|layer| layer.id);
        if let Some(id) = id {
            doc.run(LayerCommand::set_vector(id, params));
        }
    });
    id.expect("add always appends one layer")
}

fn suggested_svg_name(name: &str) -> String {
    let candidate: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let candidate = candidate.trim_matches('_');
    let stem = if candidate.is_empty() {
        "vector"
    } else {
        candidate
    };
    format!("{stem}.svg")
}

/// Enables a stroke with useful defaults, or removes it. Re-enabling an
/// existing stroke never resets its width or color.
fn set_stroke_enabled(shape: &mut VectorShape, enabled: bool) {
    match (enabled, shape.stroke) {
        (true, None) => {
            shape.stroke = Some(Stroke {
                width: DEFAULT_STROKE_WIDTH,
                color: [0, 0, 0, 255],
            });
        }
        (false, _) => shape.stroke = None,
        (true, Some(_)) => {}
    }
}

fn field_row<N: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    field: umber_gui::ValueField<'_, N>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(umber_gui::muted()));
        ui.add(field).changed()
    })
    .inner
}

#[cfg(test)]
mod row_label_tests {
    use super::*;
    use crate::document::{assert_row_label_keys, drawn_source};

    #[test]
    fn the_vector_row_labels_are_the_rows_the_panel_draws() {
        let src = include_str!("vector_panel.rs");
        let drawn = drawn_source(src, "ROW_LABEL_KEYS").expect("list");
        let labels = row_labels();
        assert_row_label_keys(ROW_LABEL_KEYS, &labels, &drawn);
        assert_eq!(
            labels,
            [
                "Vector",
                "Closed path",
                "Fill",
                "Stroke",
                "Stroke width",
                "Stroke color",
                "Export SVG",
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_toggle_installs_defaults_and_preserves_other_geometry() {
        let mut shape = VectorParams::default().shape;
        shape
            .anchors
            .push(umber_core::vector::Anchor::corner([0.5, 0.5]));
        set_stroke_enabled(&mut shape, true);
        let stroke = shape.stroke.expect("enabled");
        assert_eq!(stroke.width, DEFAULT_STROKE_WIDTH);
        assert_eq!(stroke.color, [0, 0, 0, 255]);
        assert_eq!(shape.fill, [255, 255, 255, 255]);
        assert_eq!(shape.anchors.len(), 1);

        set_stroke_enabled(&mut shape, true);
        assert_eq!(shape.stroke, Some(stroke));
        set_stroke_enabled(&mut shape, false);
        assert_eq!(shape.stroke, None);
        assert_eq!(shape.fill, [255, 255, 255, 255]);
        assert_eq!(shape.anchors.len(), 1);
    }

    #[test]
    fn svg_filename_suggestions_are_safe_bounded_and_nonempty() {
        assert_eq!(suggested_svg_name("Body / Arms"), "Body___Arms.svg");
        assert_eq!(suggested_svg_name("..."), "vector.svg");
        assert_eq!(suggested_svg_name(""), "vector.svg");
        assert_eq!(
            suggested_svg_name(&"x".repeat(100)),
            format!("{}.svg", "x".repeat(64))
        );
    }

    #[test]
    fn first_property_edit_creates_params_and_undo_restores_none() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("Path", LayerKind::Vector));
        assert!(doc.layers()[0].vector.is_none());
        let base = doc.history.len();
        let mut edited = doc.layers()[0].vector.clone().unwrap_or_default();
        set_stroke_enabled(&mut edited.shape, true);
        doc.run(LayerCommand::set_vector(0, edited));
        assert_eq!(doc.history.len(), base + 1);
        assert_eq!(
            doc.layers()[0]
                .vector
                .as_ref()
                .expect("created")
                .shape
                .stroke,
            Some(Stroke {
                width: DEFAULT_STROKE_WIDTH,
                color: [0, 0, 0, 255],
            })
        );
        assert!(doc.undo());
        assert!(doc.layers()[0].vector.is_none());
    }

    #[test]
    fn import_budget_refuses_before_any_file_read() {
        assert!(validate_svg_len(umber_core::svg::MAX_SVG_INPUT_BYTES).is_ok());
        assert!(validate_svg_len(umber_core::svg::MAX_SVG_INPUT_BYTES + 1).is_err());
    }

    #[test]
    fn installed_import_is_one_history_group_and_undo_removes_the_layer() {
        let mut doc = Document::default();
        let params = VectorParams {
            shape: umber_core::vector::VectorShape {
                closed: true,
                fill: [1, 2, 3, 255],
                stroke: None,
                anchors: vec![
                    umber_core::vector::Anchor::corner([0.0, 0.0]),
                    umber_core::vector::Anchor::corner([1.0, 1.0]),
                ],
            },
        };
        let id = install_imported_params(&mut doc, params.clone());
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(doc.layers()[0].id, id);
        assert_eq!(doc.layers()[0].vector, Some(params));
        // Two raw commands, but `run_grouped` records one undoable history row
        // and one undo crosses both.
        assert_eq!(doc.history.len(), 2);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
    }
}
