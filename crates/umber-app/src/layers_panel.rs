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
//!   is the single paintability truth (armed for materials by Kill A, for
//!   fills and folders by the per-layer-pool stage-3 kill), and the row
//!   renders its verdict as the unpaintable hint. The gate is WIRED into
//!   stroke begin in `PaintState::begin_stroke_styled` (stage 3's
//!   selection seam), so this helper plus its test plus the row badge are
//!   the belt-and-braces layer, and the viewport call sites need no change
//!   (they address the session, which gates).
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
            ui.label(
                egui::RichText::new(tr("layers.material-channel-opacity"))
                    .color(umber_gui::muted()),
            );
            ui.add(
                umber_gui::ValueField::new(&mut next_opacity)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
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

pub mod gestures {
    //! Layer-stack gestures (GUI plan row `g2-layer-gestures`,
    //! `docs/specs/gui-plan-2026-10-15.md`; sources P:B1, B:B7, B:E10).
    //!
    //! * Mask thumbnail: Alt+click solos the mask in the viewport (a toggle —
    //!   Alt+click again un-solos), Shift+click toggles the mask on/off
    //!   ([`LayerCommand::SetMaskEnabled`]), a plain click paints the mask (the
    //!   existing mask-edit target).
    //! * Ctrl/Cmd+D duplicates the selection (the C2-8 `duplicate_layer`, which
    //!   the shell follows with `PaintState::copy_owner_planes` per pair);
    //!   Ctrl/Cmd+G groups it under a new folder.
    //! * Rows multi-select: plain click replaces, Ctrl/Cmd+click toggles,
    //!   Shift+click selects the range from the anchor, Ctrl+Shift+click adds
    //!   the range.
    //! * Reorder by drag: a drag handle per row carries a [`LayerDrag`]
    //!   payload, the [`DropTracker`] paints the drop bar between rows and
    //!   yields the drop on release.
    //! * Delete: the row's ✕ button arms a confirm window; the Delete key
    //!   removes the selection at once (the File ▸ New confirm-window shape).
    //!
    //! Everything here is a SEAM: the panel's rows (`document::layers_ui`) and
    //! the shell (`main.rs`) sit outside this kill's lock. The shell owns one
    //! [`GestureState`], feeds it the gestures the row helpers return plus
    //! [`poll_layer_keys`] while the Layers panel is hovered, and applies the
    //! returned [`GestureOutcome`] to the paint session. The viewport's mask
    //! solo render reads [`MaskSolo::layer`]; no render path for it exists yet.
    //!
    //! # Judgment calls
    //!
    //! * The plan row puts Alt/Shift on the MASK thumbnail (Painter's
    //!   gestures); Shift on a ROW is the multi-select range, so the two never
    //!   collide. "Toggle" is the mask's enabled flag, as in Painter.
    //! * Group: folder membership is not modeled (`umber_core::layers` module
    //!   docs), so "group" is the stack shape a folder will own once it is:
    //!   the selected layers gathered contiguous (relative order kept) and a
    //!   new folder directly above them. Every step is an ordinary undoable
    //!   command.
    //! * No new catalog keys: the strings reuse `history.op-remove`,
    //!   `history.op-reorder`, `shell.new-confirm-cancel`, `layers.kind-folder`
    //!   and `layers.mask-edit`; the confirm body lists the layer names (user
    //!   data, not UI text). The catalogs stay free for the sibling kills.

    // The rows (`document::layers_ui`) and the shell (`main.rs`) call these
    // (g2-gesture-integration); a few accessors (`LayerSelection::len`,
    // `is_empty`, the pure helpers) are still reached only from tests.
    #![allow(dead_code)]

    use std::collections::BTreeSet;

    use egui::{Key, Modifiers};
    use umber_core::layers::{LayerCommand, LayerKind};

    use crate::document::{duplicate_layer, Document};
    use crate::i18n::tr;

    /// How a row click combines with the current selection.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ClickMode {
        /// Plain click: the row alone.
        Replace,
        /// Ctrl/Cmd+click: add or drop the row.
        Toggle,
        /// Shift+click: the range from the anchor to the row.
        Range,
        /// Ctrl/Cmd+Shift+click: add that range to the selection.
        AddRange,
    }

    /// What a mask-thumbnail click asks for.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MaskClick {
        /// Alt+click: solo (or un-solo) the mask in the viewport.
        Solo,
        /// Shift+click: flip the mask's enabled flag.
        ToggleEnabled,
        /// Plain click: paint the mask.
        Edit,
    }

    /// The panel's keyboard chords.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LayerKey {
        /// Ctrl/Cmd+D.
        Duplicate,
        /// Ctrl/Cmd+G.
        Group,
        /// Delete (no Ctrl/Cmd, no Alt).
        Delete,
    }

    /// Where a delete request came from (the confirm gate's input).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeleteSource {
        /// The row's ✕ button: confirm first.
        RowButton,
        /// The Delete key: no confirm (the key is deliberate, and undo
        /// restores it).
        DeleteKey,
    }

    /// One gesture from the rows or the keyboard, applied by
    /// [`GestureState::apply`].
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum LayerGesture {
        /// A row's name was clicked.
        Select {
            /// The row's layer.
            id: u64,
            /// How it combines with the selection.
            mode: ClickMode,
        },
        /// A mask thumbnail was clicked.
        Mask {
            /// The row's layer.
            id: u64,
            /// What the click asks for.
            click: MaskClick,
        },
        /// Duplicate the selection.
        Duplicate,
        /// Group the selection under a new folder.
        Group,
        /// Remove these layers (through the confirm gate per `source`).
        Delete {
            /// The layers to remove.
            ids: Vec<u64>,
            /// Who asked.
            source: DeleteSource,
        },
        /// A dragged row was dropped at `slot` (a gap in display order,
        /// top-first: 0 = above the first row, `len` = below the last).
        Drop {
            /// The dragged layer.
            id: u64,
            /// The gap it lands in.
            slot: usize,
        },
    }

    /// What the shell applies after a gesture: the paint session is not
    /// the panel's to touch.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct GestureOutcome {
        /// The new primary layer (`PaintState::set_active_target`).
        pub select: Option<u64>,
        /// `(source, copy)` pairs: copy the planes
        /// (`PaintState::copy_owner_planes`).
        pub duplicated: Vec<(u64, u64)>,
        /// Paint this layer's mask (`set_active_target` +
        /// `set_mask_edit_layer`, the mask section's order).
        pub mask_edit: Option<u64>,
        /// Layers removed this gesture (prune thumbnails / planes).
        pub removed: Vec<u64>,
        /// The mask solo changed (re-read [`MaskSolo::layer`]).
        pub solo_changed: bool,
    }

    /// The drag-and-drop payload a row's handle carries.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LayerDrag(pub u64);

    /// A row click's mode from the held modifiers.
    pub fn click_mode(mods: Modifiers) -> ClickMode {
        let cmd = mods.command || mods.ctrl;
        match (cmd, mods.shift) {
            (false, false) => ClickMode::Replace,
            (true, false) => ClickMode::Toggle,
            (false, true) => ClickMode::Range,
            (true, true) => ClickMode::AddRange,
        }
    }

    /// A mask-thumbnail click's meaning from the held modifiers. Alt wins
    /// over Shift (solo is the read-only gesture; a stray Shift must not
    /// also flip the mask).
    pub fn mask_click(mods: Modifiers) -> MaskClick {
        if mods.alt {
            MaskClick::Solo
        } else if mods.shift {
            MaskClick::ToggleEnabled
        } else {
            MaskClick::Edit
        }
    }

    /// The panel chord for `key` + `mods`, if any. Ctrl/Cmd+D and
    /// Ctrl/Cmd+G need Ctrl/Cmd and no Alt (Shift is not part of either);
    /// Delete needs neither Ctrl/Cmd nor Alt. Bare D/G stay free (bare G
    /// is the shell's grid toggle).
    pub fn layer_key_for(key: Key, mods: Modifiers) -> Option<LayerKey> {
        let cmd = mods.command || mods.ctrl;
        match key {
            Key::D if cmd && !mods.alt && !mods.shift => Some(LayerKey::Duplicate),
            Key::G if cmd && !mods.alt && !mods.shift => Some(LayerKey::Group),
            Key::Delete if !cmd && !mods.alt => Some(LayerKey::Delete),
            _ => None,
        }
    }

    /// The confirm gate's decision: the ✕ button confirms, the key does
    /// not.
    pub fn delete_needs_confirm(source: DeleteSource) -> bool {
        matches!(source, DeleteSource::RowButton)
    }

    /// Consumes this frame's first panel chord. Returns `None` while a text
    /// field has focus (renaming a layer must not delete it). The shell
    /// calls this only while the Layers panel is hovered/focused, so the
    /// graph canvas keeps its own Delete.
    pub fn poll_layer_keys(ctx: &egui::Context) -> Option<LayerKey> {
        // A focused TEXT field only: a clicked row label can hold focus
        // too, and it must not swallow Delete.
        if ctx.text_edit_focused() {
            return None;
        }
        ctx.input_mut(|input| {
            let (at, chord) =
                input
                    .events
                    .iter()
                    .enumerate()
                    .find_map(|(at, event)| match event {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => layer_key_for(*key, *modifiers).map(|k| (at, k)),
                        _ => None,
                    })?;
            // Consumed by removal: later readers (`key_pressed`, the
            // shell's own poll) never see the chord.
            input.events.remove(at);
            Some(chord)
        })
    }

    /// The multi-selection: a set plus the range anchor and the primary
    /// (the layer strokes paint, the one the properties row shows).
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct LayerSelection {
        ids: BTreeSet<u64>,
        anchor: Option<u64>,
        primary: Option<u64>,
    }

    impl LayerSelection {
        /// Whether `id` is selected.
        pub fn contains(&self, id: u64) -> bool {
            self.ids.contains(&id)
        }

        /// The selected ids, ascending.
        pub fn ids(&self) -> Vec<u64> {
            self.ids.iter().copied().collect()
        }

        /// The primary layer, if any.
        pub fn primary(&self) -> Option<u64> {
            self.primary
        }

        /// Number of selected layers.
        pub fn len(&self) -> usize {
            self.ids.len()
        }

        /// Whether nothing is selected.
        pub fn is_empty(&self) -> bool {
            self.ids.is_empty()
        }

        /// Selects `id` alone (add buttons, duplicate, group results).
        pub fn set_single(&mut self, id: u64) {
            self.ids = BTreeSet::from([id]);
            self.anchor = Some(id);
            self.primary = Some(id);
        }

        /// Selects exactly `ids` with `primary` primary (and anchor).
        pub fn set_many(&mut self, ids: &[u64], primary: Option<u64>) {
            self.ids = ids.iter().copied().collect();
            self.primary = primary.filter(|p| self.ids.contains(p));
            self.anchor = self.primary;
        }

        /// Applies a row click. `order` is the display order (top-first);
        /// a range needs the anchor in it, else the click replaces.
        pub fn click(&mut self, id: u64, mode: ClickMode, order: &[u64]) {
            match mode {
                ClickMode::Replace => self.set_single(id),
                ClickMode::Toggle => {
                    if self.ids.remove(&id) {
                        if self.primary == Some(id) {
                            self.primary = order.iter().copied().find(|x| self.ids.contains(x));
                        }
                    } else {
                        self.ids.insert(id);
                        self.primary = Some(id);
                    }
                    self.anchor = Some(id);
                }
                ClickMode::Range | ClickMode::AddRange => match span(order, self.anchor, id) {
                    // The anchor stays put: successive Shift+clicks pivot
                    // around it (the file-manager convention).
                    Some(span) => {
                        if mode == ClickMode::Range {
                            self.ids.clear();
                        }
                        self.ids.extend(span);
                        self.primary = Some(id);
                    }
                    None => self.set_single(id),
                },
            }
        }

        /// Drops ids no longer in `order` (deleted, undone, other set).
        pub fn retain_existing(&mut self, order: &[u64]) {
            self.ids.retain(|id| order.contains(id));
            if self.anchor.is_some_and(|a| !self.ids.contains(&a)) {
                self.anchor = None;
            }
            if self.primary.is_some_and(|p| !self.ids.contains(&p)) {
                self.primary = order.iter().copied().find(|x| self.ids.contains(x));
            }
        }
    }

    /// The display-order rows from `anchor` to `id` inclusive (`None` when
    /// either is not in `order`).
    fn span(order: &[u64], anchor: Option<u64>, id: u64) -> Option<Vec<u64>> {
        let a = order.iter().position(|x| Some(*x) == anchor)?;
        let b = order.iter().position(|x| *x == id)?;
        Some(order[a.min(b)..=a.max(b)].to_vec())
    }

    /// Which layer's mask the viewport shows solo (`None` = normal view).
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct MaskSolo(Option<u64>);

    impl MaskSolo {
        /// The soloed layer, if any.
        pub fn layer(self) -> Option<u64> {
            self.0
        }

        /// Alt+click: solo `id`, or un-solo when it already is.
        pub fn toggle(&mut self, id: u64) {
            self.0 = if self.0 == Some(id) { None } else { Some(id) };
        }

        /// Clears the solo when its layer is gone.
        pub fn retain_existing(&mut self, order: &[u64]) {
            if self.0.is_some_and(|id| !order.contains(&id)) {
                self.0 = None;
            }
        }
    }

    /// The display order (top-first) of `doc`'s active stack.
    pub fn display_order(doc: &Document) -> Vec<u64> {
        doc.layers().iter().rev().map(|l| l.id).collect()
    }

    /// The insertion gap for a pointer at `y` over rows spanning
    /// `spans` (`(top, bottom)` per row, top-first): the number of rows
    /// whose midpoint lies above `y`.
    pub fn drop_slot(y: f32, spans: &[(f32, f32)]) -> usize {
        spans
            .iter()
            .filter(|(top, bottom)| (top + bottom) * 0.5 < y)
            .count()
    }

    /// The stack reorder `(from, to)` for dropping display row `from` into
    /// gap `slot` of a `len`-row list (display = stack reversed). `None`
    /// for a drop that changes nothing (onto its own gaps) or out of range.
    pub fn drop_to_reorder(len: usize, from: usize, slot: usize) -> Option<(usize, usize)> {
        if from >= len || slot > len || slot == from || slot == from + 1 {
            return None;
        }
        let landed = if slot > from { slot - 1 } else { slot };
        Some((len - 1 - from, len - 1 - landed))
    }

    /// The reorders that gather `selected` (stack indices) into one
    /// contiguous block ending at the highest selected index, relative
    /// order kept. Apply in sequence (each is a `LayerStack::reorder`).
    pub fn contiguous_moves(selected: &[usize]) -> Vec<(usize, usize)> {
        let mut sorted: Vec<usize> = selected.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let Some(&top) = sorted.last() else {
            return Vec::new();
        };
        let k = sorted.len();
        // From the second-highest down: removing a lower layer shifts the
        // placed block down one, and inserting it just below re-lifts it.
        (0..k - 1)
            .rev()
            .filter_map(|i| {
                let to = top - (k - 1 - i);
                (sorted[i] != to).then_some((sorted[i], to))
            })
            .collect()
    }

    /// Everything the gestures keep between frames. The shell owns one.
    #[derive(Debug, Clone, Default)]
    pub struct GestureState {
        /// The multi-selection.
        pub selection: LayerSelection,
        /// The viewport mask solo.
        pub mask_solo: MaskSolo,
        /// Layers waiting on the ✕ confirm (`None` = no window).
        pub pending_delete: Option<Vec<u64>>,
        /// The paint target the empty selection was last seeded from
        /// (the F11 fix): a reseed happens only when the TARGET moved,
        /// so a deliberate deselect sticks.
        seed_target: Option<crate::paint_state::PlaneOwner>,
    }

    /// The confirm window's answer.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ConfirmChoice {
        /// Remove the pending layers.
        Remove,
        /// Close without removing.
        Cancel,
    }

    impl GestureState {
        /// Whether an EMPTY selection should be re-seeded from `target`
        /// (the F11 fix): only when the target moved since the last
        /// seed, so a deliberate deselect (selection emptied, target
        /// unchanged) STICKS instead of re-selecting next frame.
        /// Records the seed when it answers true.
        pub fn should_reseed_from(&mut self, target: crate::paint_state::PlaneOwner) -> bool {
            // The memory records the target the EMPTY selection was
            // last seeded from — written ONLY at seed time. A
            // non-empty frame (the user owns the selection) never
            // touches it, so the deselect-sticks contract holds even
            // when the target wandered while the user had a selection.
            if self.selection.is_empty() && self.seed_target != Some(target) {
                self.seed_target = Some(target);
                true
            } else {
                false
            }
        }

        /// Maps a polled chord to its gesture (Delete takes the selection).
        pub fn key_gesture(&self, key: LayerKey) -> LayerGesture {
            match key {
                LayerKey::Duplicate => LayerGesture::Duplicate,
                LayerKey::Group => LayerGesture::Group,
                LayerKey::Delete => LayerGesture::Delete {
                    ids: self.selection.ids(),
                    source: DeleteSource::DeleteKey,
                },
            }
        }

        /// Applies one gesture to `doc` (every edit an undoable command)
        /// and returns what the shell must mirror on the paint session.
        pub fn apply(&mut self, doc: &mut Document, gesture: LayerGesture) -> GestureOutcome {
            let mut out = GestureOutcome::default();
            let order = display_order(doc);
            self.selection.retain_existing(&order);
            match gesture {
                LayerGesture::Select { id, mode } => {
                    if order.contains(&id) {
                        self.selection.click(id, mode, &order);
                        out.select = self.selection.primary();
                    }
                }
                LayerGesture::Mask { id, click } => {
                    let Some(mask) = doc.stack.layer(id).and_then(|l| l.mask.clone()) else {
                        return out;
                    };
                    match click {
                        MaskClick::Solo => {
                            self.mask_solo.toggle(id);
                            out.solo_changed = true;
                        }
                        MaskClick::ToggleEnabled => {
                            doc.run(LayerCommand::set_mask_enabled(id, !mask.enabled));
                        }
                        MaskClick::Edit => {
                            self.selection.set_single(id);
                            out.select = Some(id);
                            out.mask_edit = Some(id);
                        }
                    }
                }
                LayerGesture::Duplicate => {
                    out.duplicated = self.duplicate_selected(doc);
                    let copies: Vec<u64> = out.duplicated.iter().map(|(_, c)| *c).collect();
                    let primary = self.selection.primary().and_then(|p| {
                        out.duplicated
                            .iter()
                            .find(|(s, _)| *s == p)
                            .map(|(_, c)| *c)
                    });
                    if !copies.is_empty() {
                        self.selection
                            .set_many(&copies, primary.or_else(|| copies.first().copied()));
                        out.select = self.selection.primary();
                    }
                }
                LayerGesture::Group => {
                    if let Some(folder) = self.group_selected(doc) {
                        self.selection.set_single(folder);
                        out.select = Some(folder);
                    }
                }
                LayerGesture::Delete { ids, source } => {
                    let ids: Vec<u64> = ids.into_iter().filter(|id| order.contains(id)).collect();
                    if ids.is_empty() {
                        return out;
                    }
                    if delete_needs_confirm(source) {
                        self.pending_delete = Some(ids);
                    } else {
                        out.removed = self.remove(doc, &ids);
                        out.select = self.selection.primary();
                    }
                }
                LayerGesture::Drop { id, slot } => {
                    let Some(from) = order.iter().position(|x| *x == id) else {
                        return out;
                    };
                    if let Some((a, b)) = drop_to_reorder(order.len(), from, slot) {
                        doc.run(LayerCommand::reorder(a, b));
                    }
                }
            }
            out
        }

        /// Resolves the ✕ confirm window: `Remove` removes the pending
        /// layers (returned), `Cancel` drops the request. A no-op when
        /// nothing is pending.
        pub fn resolve_pending(&mut self, doc: &mut Document, choice: ConfirmChoice) -> Vec<u64> {
            let Some(ids) = self.pending_delete.take() else {
                return Vec::new();
            };
            match choice {
                ConfirmChoice::Remove => self.remove(doc, &ids),
                ConfirmChoice::Cancel => Vec::new(),
            }
        }

        /// Draws the ✕ confirm window while a delete is pending and
        /// resolves it. Returns the layers removed this frame.
        pub fn confirm_ui(&mut self, ctx: &egui::Context, doc: &mut Document) -> Vec<u64> {
            let Some(ids) = self.pending_delete.clone() else {
                return Vec::new();
            };
            let names: Vec<String> = ids
                .iter()
                .filter_map(|id| doc.stack.layer(*id).map(|l| l.name.clone()))
                .collect();
            let choice = egui::Window::new(tr("history.op-remove"))
                .id(egui::Id::new("layers-delete-confirm"))
                .collapsible(false)
                .resizable(false)
                .min_width(240.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(names.join(", "));
                    ui.separator();
                    // Platform-ordered dialog buttons (g7-polish-batch, the
                    // polish-widgets shell half; photocraft #16): the
                    // Remove role is the affirmative (Enter's target,
                    // rendered strong) and Cancel cancels (Esc's target)
                    // — order follows the platform convention
                    // (`platform_order`: Windows/Linux Cancel rightmost,
                    // macOS the primary first).
                    let choice = umber_gui::dialog_buttons(
                        ui,
                        ui.ctx().os(),
                        &[
                            umber_gui::DialogButton::new(
                                umber_gui::ButtonRole::Default,
                                tr("history.op-remove"),
                            ),
                            umber_gui::DialogButton::new(
                                umber_gui::ButtonRole::Cancel,
                                tr("shell.new-confirm-cancel"),
                            ),
                        ],
                    )
                    .map(|clicked| match clicked {
                        0 => ConfirmChoice::Remove,
                        _ => ConfirmChoice::Cancel,
                    });
                    choice
                })
                .and_then(|response| response.inner)
                .flatten();
            match choice {
                Some(choice) => self.resolve_pending(doc, choice),
                None => Vec::new(),
            }
        }

        /// Removes `ids` (one undoable command each) and prunes the
        /// selection and the solo.
        fn remove(&mut self, doc: &mut Document, ids: &[u64]) -> Vec<u64> {
            let mut removed = Vec::new();
            for id in ids {
                if doc.stack.layer(*id).is_some() {
                    doc.run(LayerCommand::remove(*id));
                    removed.push(*id);
                }
            }
            let order = display_order(doc);
            self.selection.retain_existing(&order);
            self.mask_solo.retain_existing(&order);
            removed
        }

        /// Duplicates every selected layer, highest stack index first (a
        /// copy lands directly above its source, so lower sources keep
        /// their indices). Returns `(source, copy)` pairs.
        fn duplicate_selected(&self, doc: &mut Document) -> Vec<(u64, u64)> {
            let mut ids = self.selection.ids();
            ids.sort_by_key(|id| std::cmp::Reverse(doc.stack.index_of(*id)));
            ids.into_iter()
                .filter_map(|id| duplicate_layer(doc, id).map(|copy| (id, copy)))
                .collect()
        }

        /// Gathers the selection contiguous and adds a folder directly
        /// above it. Returns the folder's id.
        fn group_selected(&self, doc: &mut Document) -> Option<u64> {
            let indices: Vec<usize> = self
                .selection
                .ids()
                .into_iter()
                .filter_map(|id| doc.stack.index_of(id))
                .collect();
            let top = *indices.iter().max()?;
            for (from, to) in contiguous_moves(&indices) {
                doc.run(LayerCommand::reorder(from, to));
            }
            let n = doc.layers().len() + 1;
            let name = format!("{} {n}", tr("layers.kind-folder"));
            doc.run(LayerCommand::add(
                name,
                LayerKind::Folder { passthrough: false },
            ));
            let folder = doc.layers().last()?.id;
            let last = doc.layers().len() - 1;
            if last != top + 1 {
                doc.run(LayerCommand::reorder(last, top + 1));
            }
            Some(folder)
        }
    }

    /// The row-name click as a gesture (call with the name label's
    /// response).
    pub fn row_click_gesture(response: &egui::Response, id: u64) -> Option<LayerGesture> {
        response.clicked().then(|| LayerGesture::Select {
            id,
            mode: click_mode(response.ctx.input(|i| i.modifiers)),
        })
    }

    /// The mask thumbnail slot: the mask texture (or an outlined slot),
    /// struck through while the mask is disabled. Click → the
    /// modifier-mapped [`LayerGesture::Mask`].
    pub fn mask_thumb(
        ui: &mut egui::Ui,
        id: u64,
        tex: Option<&egui::TextureHandle>,
        enabled: bool,
        soloed: bool,
    ) -> Option<LayerGesture> {
        let slot = egui::vec2(28.0, 28.0);
        let (rect, response) = ui.allocate_exact_size(slot, egui::Sense::click());
        let painter = ui.painter();
        if let Some(tex) = tex {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(tex.id(), rect, uv, egui::Color32::WHITE);
        }
        let outline = if soloed {
            ui.visuals().selection.stroke
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(70))
        };
        painter.rect_stroke(rect, 3.0, outline, egui::StrokeKind::Inside);
        if !enabled {
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(1.5, ui.visuals().error_fg_color),
            );
        }
        // g7-accesskit-sweep (the painter-drawn half): the slot is one
        // click target — report it as `large_image` (a painter-drawn
        // texture cell; the English-only widget-layer label ruling).
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Image, true, tr("layers.mask-edit"))
        });
        let response = response.on_hover_text(tr("layers.mask-edit"));
        response.clicked().then(|| LayerGesture::Mask {
            id,
            click: mask_click(response.ctx.input(|i| i.modifiers)),
        })
    }

    /// The drag handle: dragging it carries a [`LayerDrag`].
    pub fn drag_handle(ui: &mut egui::Ui, id: u64) -> egui::Response {
        let response = ui
            .add(egui::Label::new("↕").sense(egui::Sense::drag()))
            .on_hover_text(tr("history.op-reorder"));
        // g7-accesskit-sweep (the painter-drawn half): the glyph-only
        // handle names its action (English-only at the widget layer).
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr("history.op-reorder"))
        });
        response.dnd_set_drag_payload(LayerDrag(id));
        response
    }

    /// Collects the rows' rects during the draw, then paints the drop bar
    /// and yields the drop.
    #[derive(Debug, Default)]
    pub struct DropTracker {
        spans: Vec<(f32, f32)>,
        x: Option<egui::Rangef>,
    }

    impl DropTracker {
        /// Records one row (call in display order, top-first).
        pub fn push(&mut self, rect: egui::Rect) {
            self.spans.push((rect.top(), rect.bottom()));
            self.x = Some(match self.x {
                Some(x) => egui::Rangef::new(x.min.min(rect.left()), x.max.max(rect.right())),
                None => rect.x_range(),
            });
        }

        /// While a [`LayerDrag`] is in flight over the rows, paints the bar
        /// at the gap under the pointer; on release, returns the drop.
        pub fn finish(&self, ui: &egui::Ui) -> Option<LayerGesture> {
            let ctx = ui.ctx();
            let drag = egui::DragAndDrop::payload::<LayerDrag>(ctx)?;
            let pos = ctx.pointer_hover_pos()?;
            let x = self.x?;
            let (first, last) = (self.spans.first()?, self.spans.last()?);
            let pad = 8.0;
            if !x.contains(pos.x) || !egui::Rangef::new(first.0 - pad, last.1 + pad).contains(pos.y)
            {
                return None;
            }
            let slot = drop_slot(pos.y, &self.spans);
            let y = match slot {
                0 => first.0,
                s if s >= self.spans.len() => last.1,
                s => (self.spans[s - 1].1 + self.spans[s].0) * 0.5,
            };
            let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
            ui.painter().hline(x, y, stroke);
            if ctx.input(|i| i.pointer.any_released()) {
                egui::DragAndDrop::clear_payload(ctx);
                return Some(LayerGesture::Drop { id: drag.0, slot });
            }
            None
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn cmd() -> Modifiers {
            Modifiers::COMMAND | Modifiers::CTRL
        }

        fn names(doc: &Document) -> Vec<String> {
            doc.layers().iter().map(|l| l.name.clone()).collect()
        }

        /// A, B, C, D, E bottom-to-top; returns their ids in that order.
        fn five(doc: &mut Document) -> Vec<u64> {
            for name in ["A", "B", "C", "D", "E"] {
                doc.run(LayerCommand::add(name, LayerKind::Paint));
            }
            doc.layers().iter().map(|l| l.id).collect()
        }

        #[test]
        fn click_modes_follow_the_modifiers() {
            assert_eq!(click_mode(Modifiers::NONE), ClickMode::Replace);
            assert_eq!(click_mode(cmd()), ClickMode::Toggle);
            assert_eq!(click_mode(Modifiers::CTRL), ClickMode::Toggle);
            assert_eq!(click_mode(Modifiers::SHIFT), ClickMode::Range);
            assert_eq!(click_mode(cmd() | Modifiers::SHIFT), ClickMode::AddRange);
        }

        #[test]
        fn mask_clicks_map_alt_solo_shift_toggle_plain_edit() {
            assert_eq!(mask_click(Modifiers::ALT), MaskClick::Solo);
            assert_eq!(mask_click(Modifiers::SHIFT), MaskClick::ToggleEnabled);
            assert_eq!(mask_click(Modifiers::NONE), MaskClick::Edit);
            assert_eq!(
                mask_click(Modifiers::ALT | Modifiers::SHIFT),
                MaskClick::Solo,
                "Alt wins: a solo never also flips the mask"
            );
        }

        #[test]
        fn layer_keys_map_only_their_chords() {
            assert_eq!(layer_key_for(Key::D, cmd()), Some(LayerKey::Duplicate));
            assert_eq!(layer_key_for(Key::G, cmd()), Some(LayerKey::Group));
            assert_eq!(
                layer_key_for(Key::Delete, Modifiers::NONE),
                Some(LayerKey::Delete)
            );
            assert_eq!(
                layer_key_for(Key::Delete, Modifiers::SHIFT),
                Some(LayerKey::Delete)
            );
            // Bare letters stay the shell's (bare G = grid), and extra
            // modifiers are other chords.
            assert_eq!(layer_key_for(Key::D, Modifiers::NONE), None);
            assert_eq!(layer_key_for(Key::G, Modifiers::NONE), None);
            assert_eq!(layer_key_for(Key::G, Modifiers::SHIFT), None);
            assert_eq!(layer_key_for(Key::G, cmd() | Modifiers::SHIFT), None);
            assert_eq!(layer_key_for(Key::D, cmd() | Modifiers::ALT), None);
            assert_eq!(layer_key_for(Key::Delete, cmd()), None);
            assert_eq!(layer_key_for(Key::Delete, Modifiers::ALT), None);
            assert_eq!(layer_key_for(Key::Backspace, Modifiers::NONE), None);
        }

        #[test]
        fn confirm_gate_confirms_the_button_not_the_key() {
            assert!(delete_needs_confirm(DeleteSource::RowButton));
            assert!(!delete_needs_confirm(DeleteSource::DeleteKey));
        }

        #[test]
        fn selection_replace_toggle_range_add_range() {
            let order = [50, 40, 30, 20, 10];
            let mut sel = LayerSelection::default();
            sel.click(40, ClickMode::Replace, &order);
            assert_eq!((sel.ids(), sel.primary()), (vec![40], Some(40)));
            sel.click(20, ClickMode::Range, &order);
            assert_eq!(sel.ids(), vec![20, 30, 40], "anchor 40 to 20");
            assert_eq!(sel.primary(), Some(20));
            // The anchor stays at 40: a second Shift+click re-pivots.
            sel.click(50, ClickMode::Range, &order);
            assert_eq!(sel.ids(), vec![40, 50], "range replaces");
            sel.click(10, ClickMode::Toggle, &order);
            assert_eq!(sel.ids(), vec![10, 40, 50]);
            assert_eq!(sel.primary(), Some(10));
            // Toggling the primary off falls back to the topmost left.
            sel.click(10, ClickMode::Toggle, &order);
            assert_eq!((sel.ids(), sel.primary()), (vec![40, 50], Some(50)));
            // Anchor is now 10 (the last toggle): AddRange 10..30 adds.
            sel.click(30, ClickMode::AddRange, &order);
            assert_eq!(sel.ids(), vec![10, 20, 30, 40, 50]);
            // A range with no usable anchor replaces.
            let mut fresh = LayerSelection::default();
            fresh.click(30, ClickMode::Range, &order);
            assert_eq!(fresh.ids(), vec![30]);
        }

        #[test]
        fn selection_and_solo_prune_vanished_layers() {
            let mut sel = LayerSelection::default();
            sel.set_many(&[1, 2, 3], Some(2));
            sel.retain_existing(&[3, 1]);
            assert_eq!(sel.ids(), vec![1, 3]);
            assert_eq!(sel.primary(), Some(3), "first survivor in display order");
            let mut solo = MaskSolo::default();
            solo.toggle(4);
            assert_eq!(solo.layer(), Some(4));
            solo.toggle(5);
            assert_eq!(solo.layer(), Some(5), "Alt+click elsewhere moves the solo");
            solo.toggle(5);
            assert_eq!(solo.layer(), None, "Alt+click again un-solos");
            solo.toggle(4);
            solo.retain_existing(&[1, 2]);
            assert_eq!(solo.layer(), None);
        }

        #[test]
        fn drop_slot_counts_rows_above_the_pointer() {
            let spans = [(0.0, 20.0), (20.0, 40.0), (40.0, 60.0)];
            assert_eq!(drop_slot(-5.0, &spans), 0);
            assert_eq!(drop_slot(9.0, &spans), 0);
            assert_eq!(drop_slot(11.0, &spans), 1);
            assert_eq!(drop_slot(35.0, &spans), 2);
            assert_eq!(drop_slot(99.0, &spans), 3);
        }

        #[test]
        fn drop_to_reorder_converts_display_gaps_to_stack_moves() {
            // Stack a0 b1 c2 → display [c, b, a].
            assert_eq!(drop_to_reorder(3, 0, 3), Some((2, 0)), "top to bottom");
            assert_eq!(drop_to_reorder(3, 2, 0), Some((0, 2)), "bottom to top");
            assert_eq!(drop_to_reorder(3, 0, 2), Some((2, 1)), "one step down");
            assert_eq!(drop_to_reorder(3, 1, 1), None, "own upper gap");
            assert_eq!(drop_to_reorder(3, 1, 2), None, "own lower gap");
            assert_eq!(drop_to_reorder(3, 3, 0), None, "unknown row");
            assert_eq!(drop_to_reorder(3, 0, 4), None, "gap out of range");
        }

        #[test]
        fn contiguous_moves_gather_below_the_top_selected() {
            let apply = |selected: &[usize]| {
                let mut v: Vec<char> = "abcdefg".chars().collect();
                for (from, to) in contiguous_moves(selected) {
                    let x = v.remove(from);
                    v.insert(to, x);
                }
                v.into_iter().collect::<String>()
            };
            assert_eq!(apply(&[1, 3, 5]), "acebdfg");
            assert_eq!(apply(&[0, 6]), "bcdefag");
            assert_eq!(apply(&[2, 3, 4]), "abcdefg", "already contiguous");
            assert!(contiguous_moves(&[2, 3, 4]).is_empty());
            assert!(contiguous_moves(&[]).is_empty());
            assert_eq!(apply(&[4]), "abcdefg");
        }

        #[test]
        fn group_gathers_the_selection_under_a_new_folder_and_undoes() {
            let mut doc = Document::default();
            let ids = five(&mut doc);
            let mut state = GestureState::default();
            state.selection.set_many(&[ids[1], ids[3]], Some(ids[3]));
            let before = doc.history.len();
            let out = state.apply(&mut doc, LayerGesture::Group);
            let folder = out.select.expect("the folder is selected");
            let got = names(&doc);
            assert_eq!(&got[..4], ["A", "C", "B", "D"]);
            assert_eq!(got[5], "E", "the unselected top stays on top");
            assert_eq!(doc.layers()[4].id, folder, "folder directly above D");
            assert!(matches!(
                doc.layers()[4].kind,
                LayerKind::Folder { passthrough: false }
            ));
            assert_eq!(state.selection.ids(), vec![folder]);
            let steps = doc.history.len() - before;
            assert_eq!(steps, 3, "one gather move, the add, the lift");
            for _ in 0..steps {
                assert!(doc.undo());
            }
            assert_eq!(names(&doc), ["A", "B", "C", "D", "E"]);
            // Nothing selected: no folder.
            let mut empty = GestureState::default();
            assert_eq!(empty.apply(&mut doc, LayerGesture::Group).select, None);
        }

        #[test]
        fn duplicate_copies_each_selected_layer_and_selects_the_copies() {
            let mut doc = Document::default();
            let ids = five(&mut doc);
            let mut state = GestureState::default();
            state.selection.set_many(&[ids[0], ids[2]], Some(ids[0]));
            let out = state.apply(&mut doc, LayerGesture::Duplicate);
            assert_eq!(
                names(&doc),
                ["A", "A copy", "B", "C", "C copy", "D", "E"],
                "each copy directly above its source"
            );
            assert_eq!(out.duplicated.len(), 2);
            let copy_of_a = out
                .duplicated
                .iter()
                .find(|(s, _)| *s == ids[0])
                .map(|(_, c)| *c);
            assert_eq!(out.select, copy_of_a, "the primary's copy is primary");
            assert_eq!(state.selection.len(), 2);
        }

        #[test]
        fn delete_key_removes_now_and_the_button_waits_for_confirm() {
            let mut doc = Document::default();
            let ids = five(&mut doc);
            let mut state = GestureState::default();
            state.selection.set_many(&[ids[0], ids[1]], Some(ids[1]));
            let key = state.key_gesture(LayerKey::Delete);
            let out = state.apply(&mut doc, key);
            assert_eq!(out.removed, vec![ids[0], ids[1]]);
            assert_eq!(names(&doc), ["C", "D", "E"]);
            assert!(state.selection.is_empty());
            assert!(state.pending_delete.is_none(), "the key never confirms");

            let button = LayerGesture::Delete {
                ids: vec![ids[4]],
                source: DeleteSource::RowButton,
            };
            let out = state.apply(&mut doc, button.clone());
            assert!(out.removed.is_empty(), "the button only arms");
            assert_eq!(names(&doc), ["C", "D", "E"]);
            assert_eq!(state.pending_delete, Some(vec![ids[4]]));
            assert!(state
                .resolve_pending(&mut doc, ConfirmChoice::Cancel)
                .is_empty());
            assert_eq!(names(&doc), ["C", "D", "E"], "cancel keeps the layer");
            assert!(state.pending_delete.is_none());
            state.apply(&mut doc, button);
            assert_eq!(
                state.resolve_pending(&mut doc, ConfirmChoice::Remove),
                vec![ids[4]]
            );
            assert_eq!(names(&doc), ["C", "D"]);
            assert!(doc.undo(), "removal is undoable");
            assert_eq!(names(&doc), ["C", "D", "E"]);
        }

        #[test]
        fn mask_gestures_solo_toggle_and_edit() {
            let mut doc = Document::default();
            let ids = five(&mut doc);
            doc.run(LayerCommand::attach_mask(ids[2], "M"));
            let mut state = GestureState::default();
            let mask = |click: MaskClick| LayerGesture::Mask { id: ids[2], click };
            let out = state.apply(&mut doc, mask(MaskClick::Solo));
            assert!(out.solo_changed);
            assert_eq!(state.mask_solo.layer(), Some(ids[2]));
            let before = doc.history.len();
            state.apply(&mut doc, mask(MaskClick::ToggleEnabled));
            let enabled = |doc: &Document| doc.stack.layer(ids[2]).unwrap().mask.clone().unwrap();
            assert!(!enabled(&doc).enabled, "Shift+click disables");
            assert_eq!(doc.history.len(), before + 1, "one undoable command");
            state.apply(&mut doc, mask(MaskClick::ToggleEnabled));
            assert!(enabled(&doc).enabled, "and re-enables");
            let out = state.apply(&mut doc, mask(MaskClick::Edit));
            assert_eq!((out.select, out.mask_edit), (Some(ids[2]), Some(ids[2])));
            // A layer without a mask ignores every mask gesture.
            let out = state.apply(
                &mut doc,
                LayerGesture::Mask {
                    id: ids[0],
                    click: MaskClick::Solo,
                },
            );
            assert_eq!(out, GestureOutcome::default());
            assert_eq!(state.mask_solo.layer(), Some(ids[2]));
        }

        #[test]
        fn drop_and_select_gestures_drive_the_stack() {
            let mut doc = Document::default();
            let ids = five(&mut doc);
            let mut state = GestureState::default();
            // Display [E, D, C, B, A]: drag E (row 0) below A (gap 5).
            state.apply(
                &mut doc,
                LayerGesture::Drop {
                    id: ids[4],
                    slot: 5,
                },
            );
            assert_eq!(names(&doc), ["E", "A", "B", "C", "D"]);
            let before = doc.history.len();
            state.apply(
                &mut doc,
                LayerGesture::Drop {
                    id: ids[1],
                    slot: 3,
                },
            );
            assert_eq!(doc.history.len(), before, "a no-op drop runs nothing");
            let out = state.apply(
                &mut doc,
                LayerGesture::Select {
                    id: ids[2],
                    mode: ClickMode::Replace,
                },
            );
            assert_eq!(out.select, Some(ids[2]));
            let out = state.apply(
                &mut doc,
                LayerGesture::Select {
                    id: 999,
                    mode: ClickMode::Replace,
                },
            );
            assert_eq!(out.select, None, "unknown rows select nothing");
            assert_eq!(state.selection.ids(), vec![ids[2]]);
        }

        #[test]
        fn poll_consumes_the_chord_and_yields_to_text_fields() {
            let ctx = egui::Context::default();
            let press = |key, modifiers| {
                let mut input = egui::RawInput::default();
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                });
                input
            };
            let mut got = None;
            let mut left = true;
            let mut out = ctx.run_ui(press(Key::D, cmd()), |ui| {
                got = poll_layer_keys(ui.ctx());
                left = ui.input(|i| i.key_pressed(Key::D));
            });
            out.textures_delta.clear();
            assert_eq!(got, Some(LayerKey::Duplicate));
            assert!(!left, "the chord is consumed");
            let mut out = ctx.run_ui(press(Key::D, Modifiers::NONE), |ui| {
                got = poll_layer_keys(ui.ctx());
            });
            out.textures_delta.clear();
            assert_eq!(got, None, "bare D is not a panel chord");
            // A focused text field (renaming) keeps Delete.
            let mut name = String::from("layer");
            let id = egui::Id::new("rename");
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.add(egui::TextEdit::singleline(&mut name).id(id));
                ui.memory_mut(|m| m.request_focus(id));
            });
            out.textures_delta.clear();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.add(egui::TextEdit::singleline(&mut name).id(id));
            });
            out.textures_delta.clear();
            assert!(ctx.text_edit_focused());
            let mut out = ctx.run_ui(press(Key::Delete, Modifiers::NONE), |ui| {
                ui.add(egui::TextEdit::singleline(&mut name).id(id));
                got = poll_layer_keys(ui.ctx());
            });
            out.textures_delta.clear();
            assert_eq!(got, None, "typing never deletes layers");
        }

        #[test]
        fn an_empty_selection_reseeds_only_when_the_target_moves() {
            use crate::paint_state::PlaneOwner;
            let mut g = GestureState::default();
            let a = PlaneOwner::Layer(0);
            let b = PlaneOwner::Layer(1);
            // Fresh: seed from the current target.
            assert!(g.should_reseed_from(a));
            g.selection.set_single(0);
            // A non-empty selection is the user's; no reseed, and the
            // memory tracks the CURRENT target.
            assert!(!g.should_reseed_from(a));
            assert!(!g.should_reseed_from(b));
            // The user deselects everything: the seed must NOT re-fire
            // while the target is unchanged — the deselect sticks.
            g.selection.set_many(&[], None);
            assert!(!g.should_reseed_from(a), "same target = deselect sticks");
            // A target switch invalidates the empty selection: reseed.
            assert!(g.should_reseed_from(b), "target moved = reseed");
        }
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
        // Stage 3 (per-layer-pool §2.3): fills and folders refuse too — a
        // fill's plane is eval-owned, a folder holds no pixels.
        let fill = stack.add_layer("F", LayerKind::Fill);
        let folder = stack.add_layer("D", LayerKind::Folder { passthrough: false });
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
            !paint_gate_allows(&stack, fill),
            "fills are eval-owned: strokes would be destroyed by refresh"
        );
        assert!(
            matches!(
                stack.check_paintable(fill),
                Err(umber_core::InstanceError::PaintOnFill { layer }) if layer == fill
            ),
            "the fill rejection is the typed stage-3 error, naming the layer"
        );
        assert!(!paint_gate_allows(&stack, folder), "folders hold no pixels");
        assert!(
            matches!(
                stack.check_paintable(folder),
                Err(umber_core::InstanceError::PaintOnFolder { layer }) if layer == folder
            ),
            "the folder rejection is the typed stage-3 error, naming the layer"
        );
        assert!(
            !paint_gate_allows(&stack, 999),
            "unknown ids cannot be painted"
        );
    }
}
