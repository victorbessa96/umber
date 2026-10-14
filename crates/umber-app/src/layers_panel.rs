//! Dynamic-material Layers-panel rows (Kill B,
//! `docs/specs/dynamic-materials-design.md` §4).
//!
//! The material layer is one row (rendered by [`crate::document::layers_ui`]
//! like every other kind — name, visibility, kind badge, no layer blend
//! combo: the layer's own `blend_mode` is INERT on materials, doc §1.3) with
//! a disclosure triangle expanding to one sub-row per DECLARED channel:
//! channel name + enable checkbox + blend combo + opacity slider. Sub-row
//! edits go through [`Document::run`](crate::document::Document::run) as
//! [`LayerCommand::SetMaterial`](umber_core::layers::LayerCommand::SetMaterial)
//! — undoable, journaled, one op per widget change (the fill-section
//! live-edit convention).
//!
//! The declaration comes from the embedded definition (parsed live from the
//! session's `material_defs_mtlx` copy): undeclared channels are NOT shown,
//! disabled channels render dimmed, and a malformed material renders an
//! honest error badge while every control stays operable (re-point
//! `material_id`, reset channels) — the never-crash doctrine at the UI
//! layer. The badge's ground truth is the frame refresh's report (stored on
//! the app state by the `main.rs` drain); the static fallbacks here cover
//! layers that never refreshed (no paint session, invisible, freshly
//! opened).
//!
//! The egui drawing itself is untested by construction (the `graph_panel`
//! precedent); every helper below it is pure logic with headless tests.
//!
//! # Judgment calls
//!
//! * The `material_id` editor is a combo over the session's known definition
//!   ids, not free text: free text would emit one undo op per keystroke,
//!   while a pick is one clean op. Re-pointing at an unknown id (the
//!   missing-definition badge) still arises — from project files, hand
//!   edits and other machines — and the refresh badges it honestly.
//! * Blend modes render as their `BlendMode::{:?}` identifiers (the Graph
//!   panel's `node_def` precedent — technical identifiers, not prose), so no
//!   `.ftl` key per mode. The combo still filters exactly like the driver:
//!   never `Passthrough`, normal-map modes only on `normal`.
//! * The declaration is re-parsed when an expanded section draws (no cached
//!   copy beside the definition — definition and declaration can never
//!   drift, doc §1.2). Definitions are small graphs; the parse costs
//!   microseconds and runs only for expanded rows.
//! * The stroke gate lives here as [`paint_gate_allows`]: `check_paintable`
//!   (armed for materials by Kill A) is the single paintability truth, and
//!   the row renders its verdict as the unpaintable hint. Wiring the gate
//!   INTO stroke begin waits on a selected-layer seam that does not exist in
//!   v1 — strokes begin in `viewport.rs`/`uv_view.rs` against the
//!   layer-agnostic pool session, both outside this kill's surface — so the
//!   helper plus its test plus the row badge are the delivered gate, and the
//!   viewport call sites are the named follow-up (see the kill report).
//! * The section defaults open (the `fill_section_ui` precedent): the
//!   channel controls sit beside the badge that explains them.

use std::collections::{BTreeMap, HashMap};

use fluent::FluentValue;
use umber_core::layers::{
    BlendMode, Layer, LayerCommand, LayerStack, MaterialChannel, MaterialParams,
};

use crate::document::Document;
use crate::i18n::{tr, tr_args};
use crate::material_layer::{discover_material_outputs, MaterialLayerStates};

/// What the material rows read: the session's embedded definitions, the last
/// refresh's badges, the drain's one-line status, and the live states (for
/// the explicit-refresh button). Built per frame by the shell — `document`
/// never owns any of it.
pub struct MaterialPanelContext<'a> {
    /// The session copy of `ProjectModel::material_defs_mtlx`.
    pub defs: &'a BTreeMap<String, String>,
    /// The last failed-refresh message per layer (row badge source).
    pub badges: &'a HashMap<u64, String>,
    /// The last drain's one-line summary (`None` = quiet).
    pub status: Option<&'a str>,
    /// The live eval states (the explicit-refresh button marks dirty here).
    pub states: &'a mut MaterialLayerStates,
}

/// The stroke gate (doc §1, §5.8): materials (like instances) carry no paint
/// pixels, so a stroke addressing one is rejected before it stages anything.
/// `false` for unknown ids too — what does not exist cannot be painted.
pub fn paint_gate_allows(stack: &LayerStack, layer: u64) -> bool {
    stack.check_paintable(layer).is_ok()
}

/// The blend modes one channel row may pick: every [`BlendMode`] except
/// `Passthrough` (never a pixel op — the driver skips that channel with the
/// badge, it never falls back to Normal), minus the normal-map trio unless
/// the channel IS `normal` (the `is_normal_map` routing contract, doc §1.2).
/// Order is the enum's declaration order (stable, pinned by test).
pub fn channel_blend_options(channel: &str) -> Vec<BlendMode> {
    const ALL: [BlendMode; 32] = [
        BlendMode::Normal,
        BlendMode::Passthrough,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Add,
        BlendMode::Subtract,
        BlendMode::Difference,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::Divide,
        BlendMode::InverseDivide,
        BlendMode::InverseSubtract,
        BlendMode::Exclusion,
        BlendMode::AddSub,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::Disable,
        BlendMode::Replace,
        BlendMode::Tint,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Value,
        BlendMode::NormalCombine,
        BlendMode::NormalDetail,
        BlendMode::NormalInverseDetail,
    ];
    ALL.into_iter()
        .filter(|mode| {
            !matches!(mode, BlendMode::Passthrough)
                && (!mode.is_normal_map() || channel == "normal")
        })
        .collect()
}

/// The definition's declared channel set: every `material_output` node, in
/// node-id order (the driver's [`discover_material_outputs`] — the panel and
/// the driver read one declaration, so the rows and the refresh agree).
///
/// # Errors
///
/// The parse failure as text, or the discovery failure
/// (`DuplicateChannel`, `BadChannelParam`) as text — both render as the
/// row's badge beside the refresh report.
pub fn declared_channels(layer_id: u64, def_mtlx: &str) -> Result<Vec<String>, String> {
    let (graph, _, _) = umber_graph::mtlx::from_mtlx(def_mtlx).map_err(|err| err.to_string())?;
    let outputs = discover_material_outputs(layer_id, &graph).map_err(|err| err.to_string())?;
    Ok(outputs.into_iter().map(|o| o.channel).collect())
}

/// Listed channels the definition no longer declares: ignored by the refresh
/// (the definition wins, doc §1.1) and named once on the status line.
fn ignored_channel_names(params: &MaterialParams, declared: &[String]) -> Vec<String> {
    params
        .channels
        .iter()
        .filter(|c| !declared.iter().any(|d| d == &c.name))
        .map(|c| c.name.clone())
        .collect()
}

/// The definition-wins defaults for a fresh channel list
/// (`Normal` / `1.0` / enabled — [`MaterialChannel::named`]).
fn default_channels_for(declared: &[String]) -> Vec<MaterialChannel> {
    declared
        .iter()
        .map(|name| MaterialChannel::named(name.clone()))
        .collect()
}

/// Inserts or replaces one channel's controls (an edit to a declared-but-
/// unlisted channel upserts a defaults-plus-change entry — unlisted means
/// defaults, so the row always has something to change). Opacity clamps to
/// `0.0..=1.0` with non-finite mapping to `0.0` (the compositor's
/// fail-closed convention — a control must never store a coverage poison).
fn upsert_channel(
    params: &mut MaterialParams,
    name: &str,
    blend: BlendMode,
    opacity: f32,
    enabled: bool,
) {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match params.channels.iter_mut().find(|c| c.name == name) {
        Some(control) => {
            control.blend = blend;
            control.opacity = opacity;
            control.enabled = enabled;
        }
        None => params.channels.push(MaterialChannel {
            name: name.to_string(),
            blend,
            opacity,
            enabled,
        }),
    }
}

/// Re-points `layer_id` at `material_id` (one undoable `SetMaterial` op):
/// channels reset to the new definition's declared defaults (the old list
/// belonged to another declaration) and overrides clear (they named the old
/// graph's nodes). An unknown id still points (the refresh badges it
/// `MissingDefinition` — never a refused edit); an unparseable definition
/// points with empty channels (the badge follows one frame later).
fn point_at_definition(
    doc: &mut Document,
    layer_id: u64,
    material_id: &str,
    defs: &BTreeMap<String, String>,
) {
    let declared = defs
        .get(material_id)
        .and_then(|def| declared_channels(layer_id, def).ok())
        .unwrap_or_default();
    doc.run(LayerCommand::set_material(
        layer_id,
        MaterialParams {
            material_id: material_id.to_string(),
            channels: default_channels_for(&declared),
            params: Vec::new(),
        },
    ));
}

/// Resets one layer's channel list to its definition's declared defaults
/// (one undoable op; a no-op when already default — scrubbing the button
/// must not spam the journal).
fn reset_channels(doc: &mut Document, layer_id: u64, defs: &BTreeMap<String, String>) {
    let Some(params) = doc.stack.layer(layer_id).and_then(|l| l.material.clone()) else {
        return;
    };
    let Some(def) = defs.get(&params.material_id) else {
        return;
    };
    let Ok(declared) = declared_channels(layer_id, def) else {
        return;
    };
    let mut next = params.clone();
    next.channels = default_channels_for(&declared);
    if next != params {
        doc.run(LayerCommand::set_material(layer_id, next));
    }
}

/// Draws one material layer's badge line plus its expandable channel section.
/// Called from [`crate::document::layers_ui`] under the layer's own row —
/// never replacing it. Missing layers return silently (deleted mid-frame).
pub fn material_section(
    ui: &mut egui::Ui,
    doc: &mut Document,
    layer_id: u64,
    ctx: &mut MaterialPanelContext<'_>,
) {
    let Some(layer) = doc.stack.layer(layer_id).cloned() else {
        return;
    };
    // The never-crash badge: the refresh report first, static fallbacks for
    // layers that never refreshed. Every control below stays operable.
    if let Some(badge) = ctx.badges.get(&layer_id) {
        ui.label(egui::RichText::new(badge.as_str()).color(ui.visuals().error_fg_color));
    } else if layer.material.is_none() {
        ui.label(
            egui::RichText::new(tr("layers.material-badge-no-params"))
                .color(ui.visuals().error_fg_color),
        );
    } else if let Some(params) = layer.material.as_ref() {
        if !ctx.defs.contains_key(&params.material_id) {
            let id = params.material_id.clone();
            let args = [("id", FluentValue::from(id.as_str()))];
            ui.label(
                egui::RichText::new(tr_args("layers.material-badge-missing-def", args))
                    .color(ui.visuals().error_fg_color),
            );
        }
    }
    // The stroke-gate truth at the UI layer: no paint affordance is offered
    // on this row, and the hint says why (the gate decides — a future
    // paintable kind reuses this row without touching it).
    if !paint_gate_allows(&doc.stack, layer_id) {
        ui.weak(tr("layers.material-unpaintable"));
    }

    let header = format!("{} (#{layer_id})", tr("layers.material-channels"));
    egui::CollapsingHeader::new(header)
        .default_open(true)
        .show(ui, |ui| {
            material_id_row(ui, doc, &layer, ctx.defs);
            ui.horizontal(|ui| {
                if ui.button(tr("layers.material-reset-channels")).clicked() {
                    reset_channels(doc, layer_id, ctx.defs);
                }
                // The explicit refresh (doc §2): re-publish on demand. Edits
                // to OTHER layers never dirty a material (anchors included),
                // so this is how the user re-resolves them.
                if ui.button(tr("layers.material-refresh")).clicked() {
                    ctx.states.mark_dirty(layer_id);
                }
            });
            channel_rows(ui, doc, &layer, ctx.defs);
        });
}

/// The `material_id` re-point combo: every known definition id, showing the
/// current value even when it names nothing (a typo'd id stays visible and
/// badged rather than silently reverting). One op per pick.
fn material_id_row(
    ui: &mut egui::Ui,
    doc: &mut Document,
    layer: &Layer,
    defs: &BTreeMap<String, String>,
) {
    let current = layer
        .material
        .as_ref()
        .map(|p| p.material_id.clone())
        .unwrap_or_default();
    // `BTreeMap` iterates sorted — the combo lists ids deterministically.
    let ids: Vec<&String> = defs.keys().collect();
    let selected = if current.is_empty() {
        tr("layers.material-id-none")
    } else {
        current.clone()
    };
    let mut picked: Option<String> = None;
    let combo_id = format!("material-id-{}", layer.id);
    ui.horizontal(|ui| {
        ui.label(tr("layers.material-id"));
        egui::ComboBox::from_id_salt(combo_id)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for option in &ids {
                    if ui
                        .selectable_label(*option == &current, option.as_str())
                        .clicked()
                    {
                        picked = Some((*option).clone());
                    }
                }
                if ids.is_empty() {
                    ui.weak(tr("layers.material-no-defs"));
                }
            });
    });
    if let Some(id) = picked {
        if id != current {
            point_at_definition(doc, layer.id, &id, defs);
        }
    }
}

/// One sub-row per DECLARED channel: name-as-enable-checkbox (dimmed when
/// disabled) + filtered blend combo + opacity slider. Each changed widget is
/// one undoable `SetMaterial` op. Undeclared list entries are NOT shown (the
/// ignored line below names them); an unparseable definition shows its error
/// instead of rows (the refresh badges it too — this is the no-paint-
/// session fallback).
fn channel_rows(
    ui: &mut egui::Ui,
    doc: &mut Document,
    layer: &Layer,
    defs: &BTreeMap<String, String>,
) {
    let Some(params) = layer.material.clone() else {
        return;
    };
    let Some(def) = defs.get(&params.material_id) else {
        return;
    };
    let declared = match declared_channels(layer.id, def) {
        Ok(declared) => declared,
        Err(err) => {
            ui.label(egui::RichText::new(err.as_str()).color(ui.visuals().error_fg_color));
            return;
        }
    };
    for name in &declared {
        let (blend, opacity, enabled) = match params.channels.iter().find(|c| c.name == *name) {
            Some(control) => (control.blend, control.opacity, control.enabled),
            None => (BlendMode::Normal, 1.0, true),
        };
        let mut next_enabled = enabled;
        let mut next_blend = blend;
        let mut next_opacity = opacity;
        let combo_id = format!("material-blend-{}-{name}", layer.id);
        ui.horizontal(|ui| {
            let name_text = if next_enabled {
                egui::RichText::new(name.as_str())
            } else {
                egui::RichText::new(name.as_str()).color(egui::Color32::from_gray(140))
            };
            ui.checkbox(&mut next_enabled, name_text);
            egui::ComboBox::from_id_salt(combo_id)
                .selected_text(format!("{next_blend:?}"))
                .show_ui(ui, |ui| {
                    for option in channel_blend_options(name) {
                        ui.selectable_value(&mut next_blend, option, format!("{option:?}"));
                    }
                });
            ui.add(
                egui::Slider::new(&mut next_opacity, 0.0..=1.0)
                    .text(tr("layers.material-channel-opacity")),
            );
        });
        if next_enabled != enabled || next_blend != blend || next_opacity != opacity {
            let mut next = params.clone();
            upsert_channel(&mut next, name, next_blend, next_opacity, next_enabled);
            doc.run(LayerCommand::set_material(layer.id, next));
        }
    }
    let ignored = ignored_channel_names(&params, &declared);
    if !ignored.is_empty() {
        let list = ignored.join(", ");
        let args = [("channels", FluentValue::from(list.as_str()))];
        ui.weak(tr_args("layers.material-ignored", args));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::layers::LayerKind;
    use umber_graph::mtlx::{painter_nodedefs, to_mtlx};
    use umber_graph::{Edge, Graph, Node, ParamValue};

    fn uniform_node(id: u64, color: [f32; 3]) -> Node {
        Node {
            id,
            node_def: "uniform".into(),
            params: vec![("color".into(), ParamValue::Color(color))],
            canvas: None,
        }
    }

    fn output_node(id: u64, channel: &str) -> Node {
        Node {
            id,
            node_def: "material_output".into(),
            params: vec![("channel".into(), ParamValue::Asset(channel.into()))],
            canvas: None,
        }
    }

    /// A two-output definition: `baseColor` solid red (node 2), `roughness`
    /// solid gray (node 9) — ids deliberately unordered, so the declaration
    /// order (node-id sorted) is pinned, not the insertion order.
    fn two_channel_def() -> String {
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(output_node(9, "roughness"));
        graph.add_node(output_node(2, "baseColor"));
        graph.add_edge(Edge {
            from: 1,
            to: 2,
            input: "input".into(),
        });
        graph.add_edge(Edge {
            from: 1,
            to: 9,
            input: "input".into(),
        });
        to_mtlx(&graph, &painter_nodedefs())
    }

    #[test]
    fn declared_channels_follow_node_id_order() {
        assert_eq!(
            declared_channels(7, &two_channel_def()).expect("parses"),
            vec!["baseColor".to_string(), "roughness".to_string()],
            "declaration order is node-id sorted, not insertion order"
        );
    }

    #[test]
    fn declared_channels_rejects_garbage_loudly() {
        assert!(
            declared_channels(7, "not mtlx at all").is_err(),
            "truncated/unparseable definitions must not yield channels"
        );
        let mut graph = Graph::new();
        graph.add_node(uniform_node(1, [1.0, 0.0, 0.0]));
        graph.add_node(output_node(2, "baseColor"));
        graph.add_node(output_node(3, "baseColor"));
        graph.add_edge(Edge {
            from: 1,
            to: 2,
            input: "input".into(),
        });
        graph.add_edge(Edge {
            from: 1,
            to: 3,
            input: "input".into(),
        });
        let err = declared_channels(7, &to_mtlx(&graph, &painter_nodedefs()))
            .expect_err("two outputs on one channel must fail");
        assert!(
            err.contains("twice"),
            "the duplicate-channel error must name itself, got {err:?}"
        );
    }

    #[test]
    fn blend_options_filter_passthrough_and_gate_the_normal_trio() {
        let normal = channel_blend_options("normal");
        assert_eq!(
            normal.len(),
            31,
            "32 modes minus Passthrough (the trio stays on `normal`)"
        );
        assert!(
            !normal.contains(&BlendMode::Passthrough),
            "Passthrough is never offered"
        );
        for trio in [
            BlendMode::NormalCombine,
            BlendMode::NormalDetail,
            BlendMode::NormalInverseDetail,
        ] {
            assert!(
                normal.contains(&trio),
                "{trio:?} is offered on the `normal` channel"
            );
        }
        let roughness = channel_blend_options("roughness");
        assert_eq!(
            roughness.len(),
            28,
            "32 modes minus Passthrough minus the normal-map trio"
        );
        for banned in [
            BlendMode::Passthrough,
            BlendMode::NormalCombine,
            BlendMode::NormalDetail,
            BlendMode::NormalInverseDetail,
        ] {
            assert!(
                !roughness.contains(&banned),
                "{banned:?} is not offered off `normal`"
            );
        }
        assert!(roughness.contains(&BlendMode::Multiply));
        assert!(roughness.contains(&BlendMode::Normal));
    }

    #[test]
    fn upsert_updates_inserts_and_clamps_opacity() {
        let mut params = MaterialParams {
            material_id: "umber.x".into(),
            channels: vec![MaterialChannel {
                name: "baseColor".into(),
                blend: BlendMode::Normal,
                opacity: 1.0,
                enabled: true,
            }],
            params: Vec::new(),
        };
        upsert_channel(&mut params, "baseColor", BlendMode::Multiply, 0.5, false);
        assert_eq!(
            params.channels,
            vec![MaterialChannel {
                name: "baseColor".into(),
                blend: BlendMode::Multiply,
                opacity: 0.5,
                enabled: false,
            }],
            "an existing entry updates in place"
        );
        // A declared-but-unlisted channel upserts defaults-plus-change, and
        // out-of-range opacities clamp (non-finite maps to zero coverage).
        upsert_channel(&mut params, "roughness", BlendMode::Screen, 1.5, true);
        upsert_channel(&mut params, "metallic", BlendMode::Normal, f32::NAN, true);
        upsert_channel(&mut params, "height", BlendMode::Normal, -0.5, true);
        let get = |name: &str| {
            params
                .channels
                .iter()
                .find(|c| c.name == name)
                .expect("upserted")
                .opacity
        };
        assert_eq!(get("roughness"), 1.0);
        assert_eq!(get("metallic"), 0.0);
        assert_eq!(get("height"), 0.0);
    }

    #[test]
    fn ignored_names_pins_the_definition_wins_set() {
        let params = MaterialParams {
            material_id: "umber.x".into(),
            channels: vec![
                MaterialChannel::named("baseColor"),
                MaterialChannel::named("stale-channel"),
            ],
            params: Vec::new(),
        };
        assert_eq!(
            ignored_channel_names(&params, &["baseColor".to_string()]),
            vec!["stale-channel".to_string()],
            "listed-but-undeclared channels are ignored, in list order"
        );
        assert!(ignored_channel_names(
            &params,
            &["baseColor".to_string(), "stale-channel".to_string()]
        )
        .is_empty());
    }

    #[test]
    fn stroke_gate_rejects_materials_instances_and_ghosts() {
        let mut stack = LayerStack::new();
        let paint = stack.add_layer("P", LayerKind::Paint);
        let material = stack.add_layer("M", LayerKind::Material);
        let instance = stack.add_layer("I", LayerKind::Instance { source_id: paint });
        assert!(
            paint_gate_allows(&stack, paint),
            "paint layers stay paintable"
        );
        assert!(
            !paint_gate_allows(&stack, material),
            "a stroke begun on a material layer is rejected (§5.8)"
        );
        assert!(
            matches!(
                stack.check_paintable(material),
                Err(umber_core::InstanceError::PaintOnMaterial { layer })
                if layer == material
            ),
            "the rejection is the typed Kill A error, naming the layer"
        );
        assert!(
            !paint_gate_allows(&stack, instance),
            "instances stay rejected too"
        );
        assert!(
            !paint_gate_allows(&stack, 999),
            "unknown ids cannot be painted"
        );
    }
}
