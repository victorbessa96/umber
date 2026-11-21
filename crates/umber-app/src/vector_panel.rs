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
    Imported(Vec<u64>),
    ExportFailed(String),
    ImportFailed(String),
}

/// Search labels in draw order. Stroke-only rows stay listed while hidden,
/// matching the Fill section's static-vocabulary convention.
const ROW_LABEL_KEYS: &[&str] = &[
    "vector.section",
    "vector.import-svg",
    "vector.closed",
    "vector.fill-rule",
    "vector.fill",
    "vector.stroke",
    "vector.stroke-width",
    "vector.stroke-color",
    "vector.stroke-cap",
    "vector.stroke-join",
    "vector.stroke-miter-limit",
    "vector.stroke-dash",
    "vector.stroke-dash-offset",
    "vector.export-svg",
];

pub fn row_labels() -> Vec<String> {
    ROW_LABEL_KEYS.iter().copied().map(tr).collect()
}

/// One collapsible property section per vector layer. A fresh `None` layer
/// edits the same visible default as the canvas; the first real change creates
/// its params through one undoable command.
pub fn vector_section_ui(ui: &mut egui::Ui, doc: &mut Document) -> Option<VectorPanelOutcome> {
    vector_section_ui_with_import(ui, doc, import_one)
}

fn vector_section_ui_with_import<F>(
    ui: &mut egui::Ui,
    doc: &mut Document,
    import: F,
) -> Option<VectorPanelOutcome>
where
    F: FnOnce(&mut Document) -> Option<VectorPanelOutcome>,
{
    let mut outcome = None;
    ui.add_space(8.0);
    ui.separator();
    ui.strong(tr("vector.section"));
    ui.add_space(4.0);
    if ui.button(tr("vector.import-svg")).clicked() {
        outcome = import(doc);
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
        return outcome;
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
                    ui.label(egui::RichText::new(tr("vector.fill-rule")).color(umber_gui::muted()));
                    let mut rule = edited.shape.fill_rule;
                    egui::ComboBox::from_id_salt(format!("vector-fill-rule-{id}"))
                        .selected_text(match rule {
                            umber_core::svg::FillRule::NonZero => tr("vector.fill-rule-nonzero"),
                            umber_core::svg::FillRule::EvenOdd => tr("vector.fill-rule-evenodd"),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut rule,
                                umber_core::svg::FillRule::NonZero,
                                tr("vector.fill-rule-nonzero"),
                            );
                            ui.selectable_value(
                                &mut rule,
                                umber_core::svg::FillRule::EvenOdd,
                                tr("vector.fill-rule-evenodd"),
                            );
                        });
                    if rule != edited.shape.fill_rule {
                        edited.shape.fill_rule = rule;
                        changed = true;
                    }
                });
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
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr("vector.stroke-cap")).color(umber_gui::muted()),
                        );
                        let mut cap = stroke.cap;
                        egui::ComboBox::from_id_salt(format!("vector-stroke-cap-{id}"))
                            .selected_text(match cap {
                                umber_core::vector::StrokeCap::Butt => tr("vector.stroke-cap-butt"),
                                umber_core::vector::StrokeCap::Round => {
                                    tr("vector.stroke-cap-round")
                                }
                                umber_core::vector::StrokeCap::Square => {
                                    tr("vector.stroke-cap-square")
                                }
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut cap,
                                    umber_core::vector::StrokeCap::Butt,
                                    tr("vector.stroke-cap-butt"),
                                );
                                ui.selectable_value(
                                    &mut cap,
                                    umber_core::vector::StrokeCap::Round,
                                    tr("vector.stroke-cap-round"),
                                );
                                ui.selectable_value(
                                    &mut cap,
                                    umber_core::vector::StrokeCap::Square,
                                    tr("vector.stroke-cap-square"),
                                );
                            });
                        if cap != stroke.cap {
                            stroke.cap = cap;
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr("vector.stroke-join")).color(umber_gui::muted()),
                        );
                        let mut join = stroke.join;
                        egui::ComboBox::from_id_salt(format!("vector-stroke-join-{id}"))
                            .selected_text(match join {
                                umber_core::vector::StrokeJoin::Miter => {
                                    tr("vector.stroke-join-miter")
                                }
                                umber_core::vector::StrokeJoin::Round => {
                                    tr("vector.stroke-join-round")
                                }
                                umber_core::vector::StrokeJoin::Bevel => {
                                    tr("vector.stroke-join-bevel")
                                }
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut join,
                                    umber_core::vector::StrokeJoin::Miter,
                                    tr("vector.stroke-join-miter"),
                                );
                                ui.selectable_value(
                                    &mut join,
                                    umber_core::vector::StrokeJoin::Round,
                                    tr("vector.stroke-join-round"),
                                );
                                ui.selectable_value(
                                    &mut join,
                                    umber_core::vector::StrokeJoin::Bevel,
                                    tr("vector.stroke-join-bevel"),
                                );
                            });
                        if join != stroke.join {
                            stroke.join = join;
                            changed = true;
                        }
                    });
                    if shows_miter_limit(stroke.join) {
                        let mut limit = stroke.miter_limit;
                        let field_changed = field_row(
                            ui,
                            &tr("vector.stroke-miter-limit"),
                            umber_gui::ValueField::new(&mut limit)
                                .context_menu(crate::i18n::prop_menu_labels())
                                .range(1.0..=1000.0),
                        );
                        if field_changed && set_miter_limit(stroke, limit) {
                            changed = true;
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr("vector.stroke-dash")).color(umber_gui::muted()),
                        );
                        let text_id = egui::Id::new(("vector-dash-pattern", id));
                        let current = format_dash_pattern(&stroke.dash);
                        let mut text = ui.memory_mut(|memory| {
                            memory
                                .data
                                .get_temp::<String>(text_id)
                                .unwrap_or_else(|| current.clone())
                        });
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut text)
                                .desired_width(140.0)
                                .hint_text("solid"),
                        );
                        if response.changed() {
                            ui.memory_mut(|memory| memory.data.insert_temp(text_id, text.clone()));
                            if let Ok(dash) = parse_dash_pattern(&text, stroke.dash.offset) {
                                if dash != stroke.dash {
                                    stroke.dash = dash;
                                    changed = true;
                                }
                            }
                        }
                        if !response.has_focus() {
                            ui.memory_mut(|memory| {
                                memory
                                    .data
                                    .insert_temp(text_id, format_dash_pattern(&stroke.dash))
                            });
                        }
                    });
                    if field_row(
                        ui,
                        &tr("vector.stroke-dash-offset"),
                        umber_gui::ValueField::new(&mut stroke.dash.offset)
                            .context_menu(crate::i18n::prop_menu_labels())
                            .range(-16.0..=16.0),
                    ) {
                        changed = true;
                    }
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
    let result = import_with_io(
        doc,
        || {
            std::fs::metadata(&path)
                .map(|metadata| metadata.len())
                .map_err(|error| error.to_string())
        },
        || read_file_capped(&path).map_err(|error| error.to_string()),
    )
    .map_err(|error| import_error(&path, error));
    match result {
        Ok(ids) => Some(VectorPanelOutcome::Imported(ids)),
        Err(error) => Some(VectorPanelOutcome::ImportFailed(error)),
    }
}

/// Reads at most one byte past the import budget, even for FIFOs or files whose
/// metadata reports zero. The caller validates the resulting length.
fn read_capped<R: std::io::Read>(reader: R) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    let mut bytes = Vec::new();
    reader
        .take((umber_core::svg::MAX_SVG_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_file_capped(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    read_capped(std::fs::File::open(path)?)
}

fn import_error(path: &std::path::Path, error: String) -> String {
    format!("{}: {error}", path.display())
}

/// Testable import seam: metadata is always evaluated before the read closure,
/// and parsing always completes before the document is touched.
fn import_with_io<M, R>(doc: &mut Document, metadata: M, read: R) -> Result<Vec<u64>, String>
where
    M: FnOnce() -> Result<u64, String>,
    R: FnOnce() -> Result<Vec<u8>, String>,
{
    let bytes = read_svg_capped_with(metadata, read)?;
    import_bytes(doc, &bytes)
}

fn import_bytes(doc: &mut Document, bytes: &[u8]) -> Result<Vec<u64>, String> {
    let params =
        umber_core::vector_svg::import_svg_many(bytes).map_err(|error| error.to_string())?;
    Ok(install_imported_params(doc, params))
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

fn read_svg_capped_with<M, R>(metadata: M, read: R) -> Result<Vec<u8>, String>
where
    M: FnOnce() -> Result<u64, String>,
    R: FnOnce() -> Result<Vec<u8>, String>,
{
    let declared = metadata()?;
    let declared =
        usize::try_from(declared).map_err(|_| "SVG size does not fit this platform".to_owned())?;
    validate_svg_len(declared)?;
    let bytes = read()?;
    // A file can grow between metadata and read. Check the actual bytes too.
    validate_svg_len(bytes.len())?;
    Ok(bytes)
}

/// Adds one vector layer per parsed path in a single history group, so undo
/// removes the whole import instead of leaving empty layers behind.
fn install_imported_params(doc: &mut Document, params: Vec<VectorParams>) -> Vec<u64> {
    let mut ids = Vec::with_capacity(params.len());
    doc.run_grouped(crate::history_panel::HistoryKind::AddVector, |doc| {
        for params in params {
            doc.run(LayerCommand::add(
                tr("vector.imported-default"),
                LayerKind::Vector,
            ));
            let id = doc
                .layers()
                .last()
                .map(|layer| layer.id)
                .expect("add appends");
            doc.run(LayerCommand::set_vector(id, params));
            ids.push(id);
        }
    });
    ids
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
                miter_limit: 4.0,
                width: DEFAULT_STROKE_WIDTH,
                cap: umber_core::vector::StrokeCap::Round,

                join: umber_core::vector::StrokeJoin::Round,
                dash: umber_core::vector::StrokeDash::solid(),
                color: [0, 0, 0, 255],
            });
        }
        (false, _) => shape.stroke = None,
        (true, Some(_)) => {}
    }
}

/// The miter limit only shapes Miter joins; Round and Bevel keep the stored
/// value for SVG roundtrips but hide the irrelevant row.
fn shows_miter_limit(join: umber_core::vector::StrokeJoin) -> bool {
    matches!(join, umber_core::vector::StrokeJoin::Miter)
}

/// The exact edit mediator the Miter-limit row drives. Returns false on a
/// no-op so the panel does not mint a history entry for an unchanged value.
fn set_miter_limit(stroke: &mut Stroke, value: f32) -> bool {
    if stroke.miter_limit == value {
        false
    } else {
        stroke.miter_limit = value;
        true
    }
}

fn parse_dash_pattern(
    text: &str,
    offset: f32,
) -> Result<umber_core::vector::StrokeDash, umber_core::vector::VectorError> {
    let text = text.trim();
    if text.is_empty() || text == "solid" {
        return Ok(umber_core::vector::StrokeDash::solid());
    }
    let lengths = text
        .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .parse::<f32>()
                .map_err(|_| umber_core::vector::VectorError::InvalidDash)
        })
        .collect::<Result<Vec<_>, _>>()?;
    umber_core::vector::StrokeDash::from_lengths(&lengths, offset)
}

fn format_dash_pattern(dash: &umber_core::vector::StrokeDash) -> String {
    dash.lengths()
        .iter()
        .map(|length| length.to_string())
        .collect::<Vec<_>>()
        .join(",")
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
        let production = drawn.split("#[cfg(test)]").next().unwrap_or(&drawn);
        let section_at = production.find("\"vector.section\"").expect("section draw");
        let import_at = production
            .find("\"vector.import-svg\"")
            .expect("import draw");
        let closed_at = production.find("\"vector.closed\"").expect("closed draw");
        assert!(section_at < import_at && import_at < closed_at);
        assert_eq!(
            labels,
            [
                "Vector",
                "Import SVG",
                "Closed path",
                "Fill rule",
                "Fill",
                "Stroke",
                "Stroke width",
                "Stroke color",
                "Stroke cap",
                "Stroke join",
                "Miter limit",
                "Dash pattern",
                "Dash offset",
                "Export SVG",
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type DocSnapshot = (Vec<(u64, String, Option<VectorParams>)>, bool, usize, usize);

    fn doc_snapshot(doc: &Document) -> DocSnapshot {
        (
            doc.layers()
                .iter()
                .map(|layer| (layer.id, layer.name.clone(), layer.vector.clone()))
                .collect(),
            doc.dirty,
            doc.history.len(),
            doc.session_history.len(),
        )
    }

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
    fn dash_editor_parses_patterns_and_preserves_offset() {
        let dash = parse_dash_pattern("0.25, 0.125", 0.0625).expect("valid dash");
        assert_eq!(dash.lengths(), &[0.25, 0.125]);
        assert_eq!(dash.offset, 0.0625);
        assert_eq!(format_dash_pattern(&dash), "0.25,0.125");
        assert!(parse_dash_pattern("", 0.0)
            .expect("empty is solid")
            .is_solid());
        assert!(parse_dash_pattern("solid", 0.0).expect("solid").is_solid());
        assert!(parse_dash_pattern("0 0", 0.0).is_err());
        assert!(parse_dash_pattern("not-a-number", 0.0).is_err());
    }

    #[test]
    fn miter_limit_row_visibility_matches_the_selected_join() {
        use umber_core::vector::StrokeJoin::*;
        assert!(shows_miter_limit(Miter));
        assert!(!shows_miter_limit(Round));
        assert!(!shows_miter_limit(Bevel));
        let src = include_str!("vector_panel.rs");
        let production = src.split("#[cfg(test)]").next().unwrap_or(src);
        let guard = production
            .find("shows_miter_limit(stroke.join)")
            .expect("production row is guarded by the join helper");
        let row = production[guard..]
            .find("\"vector.stroke-miter-limit\"")
            .expect("production row key")
            + guard;
        assert!(guard < row, "the row must sit behind the Miter-only guard");
    }

    #[test]
    fn miter_limit_edit_is_one_undoable_set_vector_and_preserves_the_stroke() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("Path", LayerKind::Vector));
        let base = doc.history.len();
        let mut edited = doc.layers()[0].vector.clone().unwrap_or_default();
        set_stroke_enabled(&mut edited.shape, true);
        {
            let stroke = edited.shape.stroke.as_mut().expect("stroke enabled");
            stroke.join = umber_core::vector::StrokeJoin::Miter;
            assert!(set_miter_limit(stroke, 8.0));
            assert!(
                !set_miter_limit(stroke, 8.0),
                "no-op edit reports no change"
            );
        }
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
                miter_limit: 8.0,
                width: DEFAULT_STROKE_WIDTH,
                cap: umber_core::vector::StrokeCap::Round,
                join: umber_core::vector::StrokeJoin::Miter,
                dash: umber_core::vector::StrokeDash::solid(),
                color: [0, 0, 0, 255],
            })
        );
        assert!(doc.undo());
        assert!(doc.layers()[0].vector.is_none());
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
                miter_limit: 4.0,
                width: DEFAULT_STROKE_WIDTH,
                cap: umber_core::vector::StrokeCap::Round,

                join: umber_core::vector::StrokeJoin::Round,
                dash: umber_core::vector::StrokeDash::solid(),
                color: [0, 0, 0, 255],
            })
        );
        assert!(doc.undo());
        assert!(doc.layers()[0].vector.is_none());
    }

    #[test]
    fn over_cap_metadata_refuses_before_the_read_closure() {
        use std::cell::Cell;

        let read_called = Cell::new(false);
        let result = read_svg_capped_with(
            || Ok((umber_core::svg::MAX_SVG_INPUT_BYTES + 1) as u64),
            || {
                read_called.set(true);
                Ok(Vec::new())
            },
        );
        assert!(result.is_err());
        assert!(!read_called.get(), "read ran after over-cap metadata");
        assert!(validate_svg_len(umber_core::svg::MAX_SVG_INPUT_BYTES).is_ok());
    }

    #[test]
    fn capped_stream_read_stops_one_byte_past_the_budget() {
        use std::io::Read;

        let bytes =
            read_capped(std::io::repeat(0).take((umber_core::svg::MAX_SVG_INPUT_BYTES + 2) as u64))
                .expect("capped read");
        assert_eq!(bytes.len(), umber_core::svg::MAX_SVG_INPUT_BYTES + 1);
        assert!(validate_svg_len(bytes.len()).is_err());

        let error = import_error(
            std::path::Path::new("picked.svg"),
            "SVG exceeds the budget".to_owned(),
        );
        assert_eq!(error, "picked.svg: SVG exceeds the budget");
        assert_eq!(error.matches("picked.svg").count(), 1);
    }

    #[test]
    fn read_and_parse_failures_leave_the_document_unchanged() {
        let mut doc = Document::default();
        doc.run(LayerCommand::add("Existing", LayerKind::Vector));
        let before = doc_snapshot(&doc);

        let read_failure = import_with_io(&mut doc, || Ok(3), || Err("read failed".to_owned()));
        assert!(read_failure.is_err());
        assert_eq!(doc_snapshot(&doc), before);

        let parse_failure = import_with_io(&mut doc, || Ok(3), || Ok(b"not svg".to_vec()));
        assert!(parse_failure.is_err());
        assert_eq!(doc_snapshot(&doc), before);

        let grouped = br#"<svg width="1" height="1"><g opacity="0.5"><path d="M0 0"/></g></svg>"#;
        let declared = grouped.len() as u64;
        let group_failure =
            import_with_io(&mut doc, move || Ok(declared), move || Ok(grouped.to_vec()));
        assert!(group_failure.is_err());
        assert_eq!(doc_snapshot(&doc), before);

        let transformed =
            br#"<svg width="1" height="1"><g transform="bogus(1)"><path d="M0 0"/></g></svg>"#;
        let declared = transformed.len() as u64;
        let transform_failure = import_with_io(
            &mut doc,
            move || Ok(declared),
            move || Ok(transformed.to_vec()),
        );
        assert!(transform_failure.is_err());
        assert_eq!(doc_snapshot(&doc), before);

        let curved = br#"<svg width="1" height="1"><circle r="1"/><path d="M0 0 A1e308 1e308 0 0 1 1 1"/></svg>"#;
        let declared = curved.len() as u64;
        let curved_failure =
            import_with_io(&mut doc, move || Ok(declared), move || Ok(curved.to_vec()));
        assert!(curved_failure.is_err());
        assert_eq!(doc_snapshot(&doc), before);
    }

    #[test]
    fn import_with_io_success_adds_one_grouped_layer() {
        let mut doc = Document::default();
        let svg = br#"<svg width="1" height="1"><path d="M0 0 L1 1 Z"/></svg>"#;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("import succeeds");
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(ids, vec![doc.layers()[0].id]);
        assert_eq!(doc.history.len(), 2);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.dirty);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
    }

    #[test]
    fn multi_path_import_adds_all_layers_in_one_undo_group() {
        let mut doc = Document::default();
        let svg =
            br#"<svg width="1" height="1"><path d="M0 0 L1 0 L1 1 Z"/><path d="M0 0 L1 1"/></svg>"#;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("multi-path import succeeds");
        assert_eq!(doc.layers().len(), 2);
        assert_eq!(ids.len(), 2);
        assert_eq!(
            ids,
            doc.layers()
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>()
        );
        assert!(doc.layers().iter().all(|layer| layer.vector.is_some()));
        assert_eq!(doc.history.len(), 4);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
        assert!(doc.redo());
        assert_eq!(doc.layers().len(), 2);
    }

    #[test]
    fn compound_subpaths_become_one_layer_in_one_undo_group() {
        let mut doc = Document::default();
        let svg = br##"<svg width="1" height="1"><path fill="none" stroke="#000" d="M0 0 L1 0 M0 1 L1 1"/></svg>"##;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("compound import succeeds");
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(ids.len(), 1);
        assert_eq!(
            doc.layers()[0]
                .vector
                .as_ref()
                .unwrap()
                .shape
                .subpaths
                .len(),
            1
        );
        assert_eq!(doc.history.len(), 2);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
        assert!(doc.redo());
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(
            doc.layers()[0]
                .vector
                .as_ref()
                .unwrap()
                .shape
                .subpaths
                .len(),
            1
        );
    }

    #[test]
    fn grouped_svg_import_adds_layers_in_one_undo_group() {
        let mut doc = Document::default();
        let svg = br##"<svg width="1" height="1" fill="#123456"><g><path d="M0 0 L1 0 L1 1 Z"/><g fill="none" stroke="#000"><path d="M0 1 L1 0"/></g></g></svg>"##;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("grouped import succeeds");
        assert_eq!(doc.layers().len(), 2);
        assert_eq!(ids.len(), 2);
        assert_eq!(doc.history.len(), 4);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
        assert!(doc.redo());
        assert_eq!(doc.layers().len(), 2);
    }

    #[test]
    fn transformed_group_import_adds_layers_in_one_undo_group() {
        let mut doc = Document::default();
        let svg = br##"<svg width="4" height="4"><g transform="translate(1 0)"><path fill="#123456" d="M0 0 L1 0 L1 1 Z"/><path transform="scale(2)" fill="none" stroke="#000" d="M0 1 L1 0"/></g></svg>"##;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("transformed import succeeds");
        assert_eq!(doc.layers().len(), 2);
        assert_eq!(ids.len(), 2);
        assert_eq!(doc.history.len(), 4);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
        assert!(doc.redo());
        assert_eq!(doc.layers().len(), 2);
    }

    #[test]
    fn basic_shapes_import_adds_layers_in_one_undo_group() {
        let mut doc = Document::default();
        let svg = br##"<svg width="8" height="8"><rect width="2" height="2"/><polygon points="0 0 2 0 1 2"/><polyline points="0,0 1,0 1,1" fill="none" stroke="#000"/><line x2="1" y2="1" stroke="#000"/></svg>"##;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("basic-shape import succeeds");
        assert_eq!(doc.layers().len(), 4);
        assert_eq!(ids.len(), 4);
        assert_eq!(doc.history.len(), 8);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
        assert!(doc.redo());
        assert_eq!(doc.layers().len(), 4);
    }

    #[test]
    fn arcs_and_curved_basic_shapes_share_one_grouped_undo_and_redo() {
        let mut doc = Document::default();
        let svg = br#"<svg width="4" height="4"><path d="M1 0 A1 1 0 1 1 0 1"/><rect width="3" height="2" rx="0.5"/><circle cx="1" cy="1" r="1"/><ellipse cx="2" cy="1" rx="1" ry="0.5"/></svg>"#;
        let declared = svg.len() as u64;
        let ids = import_with_io(&mut doc, move || Ok(declared), move || Ok(svg.to_vec()))
            .expect("curved import succeeds");
        assert_eq!(doc.layers().len(), 4);
        assert_eq!(ids.len(), 4);
        assert_eq!(doc.history.len(), 8);
        assert_eq!(doc.session_history.len(), 1);
        let expected = doc
            .layers()
            .iter()
            .map(|layer| layer.vector.clone().expect("installed vector"))
            .collect::<Vec<_>>();
        assert!(expected[0].shape.anchors[0].out_handle != expected[0].shape.anchors[0].pos);
        assert!(expected[1..].iter().all(|params| params.shape.closed));

        assert!(doc.undo());
        assert!(doc.layers().is_empty());
        assert!(doc.redo());
        let restored = doc
            .layers()
            .iter()
            .map(|layer| layer.vector.clone().expect("restored vector"))
            .collect::<Vec<_>>();
        assert_eq!(restored, expected);
        assert_eq!(doc.session_history.len(), 1);
    }

    #[test]
    fn empty_panel_preserves_import_failure_outcome() {
        use egui_kittest::kittest::Queryable;
        use std::cell::RefCell;
        use std::rc::Rc;

        crate::i18n::install(crate::i18n::I18n::builtin());
        let doc = Rc::new(RefCell::new(Document::default()));
        let outcomes = Rc::new(RefCell::new(Vec::new()));
        let doc_ui = doc.clone();
        let outcomes_ui = outcomes.clone();
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            if let Some(outcome) =
                vector_section_ui_with_import(ui, &mut doc_ui.borrow_mut(), |_doc| {
                    Some(VectorPanelOutcome::ImportFailed("boom".to_owned()))
                })
            {
                outcomes_ui.borrow_mut().push(outcome);
            }
        });
        harness.run();
        harness.get_by_label("Import SVG").click();
        harness.run();
        assert!(outcomes
            .borrow()
            .iter()
            .any(|outcome| matches!(outcome, VectorPanelOutcome::ImportFailed(_))));
        assert!(doc.borrow().layers().is_empty());
        assert!(!doc.borrow().dirty);
    }

    #[test]
    fn installed_import_is_one_history_group_and_undo_removes_the_layer() {
        let mut doc = Document::default();
        let params = VectorParams {
            shape: umber_core::vector::VectorShape {
                subpaths: Vec::new().into(),
                closed: true,
                fill_rule: umber_core::svg::FillRule::NonZero,
                fill: [1, 2, 3, 255],
                stroke: None,
                anchors: vec![
                    umber_core::vector::Anchor::corner([0.0, 0.0]),
                    umber_core::vector::Anchor::corner([1.0, 1.0]),
                ],
            },
        };
        let ids = install_imported_params(&mut doc, vec![params.clone()]);
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(ids, vec![doc.layers()[0].id]);
        assert_eq!(doc.layers()[0].vector, Some(params));
        // Two raw commands, but `run_grouped` records one undoable history row
        // and one undo crosses both.
        assert_eq!(doc.history.len(), 2);
        assert_eq!(doc.session_history.len(), 1);
        assert!(doc.undo());
        assert!(doc.layers().is_empty());
    }
}
