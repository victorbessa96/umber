//! Behavior kittests for panels whose state is headless-constructible
//! (GUI plan G0 `g0-kittest-harness`: the first consumer of the shared
//! `support` module). Snapshot comparisons live in `panel_snapshot.rs`;
//! this file pins what clicking DOES.

mod support;

use egui_kittest::kittest::{NodeT, Queryable};
use umber_app::display_panel::{self, DisplayLutState};
use umber_color::DisplaySettings;

/// The settings + LUT state the Display panel edits.
struct Display {
    settings: DisplaySettings,
    lut: DisplayLutState,
}

fn display_harness(settings: DisplaySettings) -> egui_kittest::Harness<'static, Display> {
    support::harness_state(
        |ui, s: &mut Display| {
            display_panel::show(ui, &mut s.settings, &mut s.lut);
        },
        Display {
            settings,
            lut: DisplayLutState::default(),
        },
    )
}

#[test]
fn display_reset_restores_the_default_transform() {
    let reset = umber_app::i18n::tr("button.reset");
    let edited = DisplaySettings {
        exposure: 1.5,
        ..DisplaySettings::default()
    };
    assert!(!edited.is_identity(), "the fixture must be an edit");
    let mut harness = display_harness(edited);
    support::settle(&mut harness);
    assert!(support::has_label(&harness, &reset));
    support::click_label(&mut harness, &reset);
    assert_eq!(harness.state().settings, DisplaySettings::default());
}

#[test]
fn display_reset_is_disabled_at_the_default() {
    // The `add_enabled(!is_identity)` gate, read through AccessKit: at the
    // identity the Reset button reports disabled, after an edit it does
    // not. (Clicking it at the default could not fail as a test: a reset
    // of the default is the default either way.)
    let reset = umber_app::i18n::tr("button.reset");
    let mut harness = display_harness(DisplaySettings::default());
    support::settle(&mut harness);
    assert!(harness.get_by_label(&reset).accesskit_node().is_disabled());
    harness.state_mut().settings.gamma = 2.6;
    support::settle(&mut harness);
    assert!(!harness.get_by_label(&reset).accesskit_node().is_disabled());
}

#[test]
fn the_command_palette_picks_and_notes_a_recent() {
    // The overlay: typing filters the catalog, Enter picks the selected
    // row, and the picked action lands in the recents list.
    use umber_app::palette::Palette;
    use umber_app::toolbar::ShellAction;
    struct S {
        palette: Palette,
        picked: Option<ShellAction>,
    }
    let mut harness = support::harness_state(
        |ui, s: &mut S| {
            ui.allocate_space(egui::vec2(480.0, 640.0));
            if let Some(action) = s.palette.show(
                ui.ctx(),
                umber_app::i18n::tr,
                umber_app::i18n::tr,
                |a| umber_app::actions::shortcut_label(a, false),
                "search",
            ) {
                s.picked = Some(action);
            }
        },
        {
            let mut palette = Palette::default();
            palette.open();
            S {
                palette,
                picked: None,
            }
        },
    );
    support::settle(&mut harness);
    // Type a query that matches few actions, then Enter.
    harness.state_mut().palette.set_query("wireframe");
    support::settle(&mut harness);
    let before = harness.state().palette.recents().to_vec();
    support::press_enter(&mut harness);
    support::settle(&mut harness);
    let picked = harness.state().picked.expect("Enter picked an action");
    let after = harness.state().palette.recents().to_vec();
    assert!(
        !harness.state().palette.is_open(),
        "the overlay closed after picking {picked:?}"
    );
    assert_ne!(before, after, "the pick was noted as a recent");
}

#[test]
fn the_history_buttons_undo_and_redo_layer_edits() {
    // One document, two adds: the panel's Undo button empties the stack's
    // redo side, Redo fills it again.
    use umber_app::document::Document;
    use umber_core::layers::{LayerCommand, LayerKind};
    let mut harness = support::harness_state(
        |ui, doc: &mut Document| {
            umber_app::history_panel::history_ui(ui, doc, None);
        },
        {
            let mut doc = Document::default();
            doc.run(LayerCommand::add("one", LayerKind::Paint));
            doc.run(LayerCommand::add("two", LayerKind::Paint));
            doc
        },
    );
    support::settle(&mut harness);
    assert_eq!(harness.state().layers().len(), 2);
    support::click_label(&mut harness, &umber_app::i18n::tr("history.undo"));
    assert_eq!(harness.state().layers().len(), 1, "one undo");
    support::click_label(&mut harness, &umber_app::i18n::tr("history.redo"));
    assert_eq!(harness.state().layers().len(), 2, "redo restores it");
}

#[test]
fn the_harness_installs_i18n_before_the_first_frame() {
    // Labels render translated, never as raw `display.*` keys.
    let mut harness = display_harness(DisplaySettings::default());
    support::settle(&mut harness);
    assert!(support::has_label(
        &harness,
        &umber_app::i18n::tr("button.reset")
    ));
    assert!(!support::has_label(&harness, "button.reset"));
}

// --- g7-accesskit-sweep, the painter-drawn umber-app half -----------------

#[test]
fn the_mask_thumb_reports_a_role_and_label() {
    // The slot is painter-drawn (`allocate_exact_size` + strokes): before
    // the sweep it was invisible to a screen reader. The query IS the
    // assertion — kittest panics when the node is missing.
    use egui::accesskit::Role;
    use umber_app::layers_panel::gestures::LayerGesture;
    let label = umber_app::i18n::tr("layers.mask-edit");
    let mut harness = support::harness_state(
        |ui, s: &mut Option<LayerGesture>| {
            if let Some(g) = umber_app::layers_panel::gestures::mask_thumb(ui, 7, None, true, false)
            {
                *s = Some(g);
            }
        },
        None,
    );
    support::settle(&mut harness);
    // The query IS the assertion (kittest panics on miss); egui_kittest
    // 0.36.2's Node carries no is_disabled — presence + role + label is
    // the pinned contract, same as the gui half's tests.
    let _node = harness.get_by_role_and_label(Role::Image, &label);
}

#[test]
fn the_mask_thumb_click_yields_the_mask_gesture() {
    use umber_app::layers_panel::gestures::{LayerGesture, MaskClick};
    let label = umber_app::i18n::tr("layers.mask-edit");
    let mut harness = support::harness_state(
        |ui, s: &mut Option<LayerGesture>| {
            if let Some(g) = umber_app::layers_panel::gestures::mask_thumb(ui, 7, None, true, false)
            {
                *s = Some(g);
            }
        },
        None,
    );
    support::settle(&mut harness);
    support::click_label(&mut harness, &label);
    assert_eq!(
        harness.state().clone(),
        Some(LayerGesture::Mask {
            id: 7,
            click: MaskClick::Edit
        }),
        "a plain click paints the mask"
    );
}

#[test]
fn the_drag_handle_reports_a_role_and_label() {
    use egui::accesskit::Role;
    let label = umber_app::i18n::tr("history.op-reorder");
    let mut harness = support::harness(|ui| {
        let _r = umber_app::layers_panel::gestures::drag_handle(ui, 3);
    });
    support::settle(&mut harness);
    let _node = harness.get_by_role_and_label(Role::Button, &label);
    // Missing node => get panics: reaching here the reader found the row.
}

#[test]
fn the_row_eye_toggles_visibility_and_reports_itself() {
    // The eye row goes through the real `layers_ui` draw path, so the
    // widget_info call site is exercised, not a fixture. (SelectableLabel
    // maps to accesskit's Button role — egui response.rs's fill.)
    use egui::accesskit::Role;
    use umber_app::layers_panel::gestures::GestureState;
    use umber_app::{document, paint_state};
    let mut harness = support::harness_state(
        |ui, s: &mut document::Document| {
            let defs = std::collections::BTreeMap::<String, String>::new();
            let badges = std::collections::HashMap::<u64, String>::new();
            let mut states = umber_app::material_layer::MaterialLayerStates::default();
            let thumbs = std::collections::HashMap::<u64, egui::TextureHandle>::new();
            let gestures = GestureState::default();
            let mats = umber_app::layers_panel::MaterialPanelContext {
                defs: &defs,
                badges: &badges,
                status: None,
                states: &mut states,
            };
            let _ = document::layers_ui(
                ui,
                s,
                mats,
                paint_state::PlaneOwner::Base,
                true,
                &document::LayerRowBadges::default(),
                &thumbs,
                &gestures,
            );
        },
        {
            use umber_core::layers::{LayerCommand, LayerKind};
            let mut doc = document::Document::default();
            doc.run(LayerCommand::add("one", LayerKind::Paint));
            doc
        },
    );
    support::settle(&mut harness);
    // Two eyes carry the label: the Base plane's pseudo-row (document.rs's
    // selectable ownerless row) and the layer's own row. The LAYER's eye is
    // the one under test — the last match in draw order (base pseudo-row
    // draws first). get_all_ is kittest 0.4.0's multi-match family.
    let eyes: Vec<_> = harness
        .query_all_by_role_and_label(Role::Button, "visibility")
        .collect();
    assert!(
        eyes.len() >= 2,
        "expected the base pseudo-row eye AND the layer eye, found {}",
        eyes.len()
    );
    // Exactly ONE of the eyes hides the LAYER (the other toggles the
    // Base pseudo-row's visibility, not the layer's own flag). Node
    // handles borrow the harness, so every click is a fully inline
    // one-liner — no held node, no held iterator.
    fn click_eye_then_layer_hidden(
        h: &mut egui_kittest::Harness<'_, document::Document>,
        i: usize,
    ) -> bool {
        h.query_all_by_role_and_label(Role::Button, "visibility")
            .nth(i)
            .expect("the eye count is stable across queries")
            .click();
        support::settle(h);
        !h.state().layers().first().expect("the layer").visible
    }
    for i in 0..eyes.len() {
        if click_eye_then_layer_hidden(&mut harness, i) {
            // One eye hid the layer — the contract holds.
            return;
        }
    }
    panic!("no eye hid the layer");
}

/// The layers_ui draw closure the breadth tests share (the row-eye
/// test's shape): one empty-materials context, Base visible, badges
/// and thumbs empty — `active` is the parameter under test.
fn layers_ui_closure(
    active: umber_app::paint_state::PlaneOwner,
) -> impl FnMut(&mut egui::Ui, &mut umber_app::document::Document) {
    use umber_app::layers_panel::gestures::GestureState;
    move |ui, s: &mut umber_app::document::Document| {
        let defs = std::collections::BTreeMap::<String, String>::new();
        let badges = std::collections::HashMap::<u64, String>::new();
        let mut states = umber_app::material_layer::MaterialLayerStates::default();
        let thumbs = std::collections::HashMap::<u64, egui::TextureHandle>::new();
        let gestures = GestureState::default();
        let mats = umber_app::layers_panel::MaterialPanelContext {
            defs: &defs,
            badges: &badges,
            status: None,
            states: &mut states,
        };
        let _ = umber_app::document::layers_ui(
            ui,
            s,
            mats,
            active,
            true,
            &umber_app::document::LayerRowBadges::default(),
            &thumbs,
            &gestures,
        );
    }
}

/// The ⧉ row button duplicates the layer: a same-kind "copy" above the
/// source, settings carried (C2-8's duplicate; the undoable-command
/// path is the History panel's own test).
#[test]
fn the_duplicate_button_copies_the_layer_row() {
    use umber_core::layers::{LayerCommand, LayerKind};
    let mut harness = support::harness_state(
        layers_ui_closure(umber_app::paint_state::PlaneOwner::Base),
        {
            let mut doc = umber_app::document::Document::default();
            doc.run(LayerCommand::add("one", LayerKind::Paint));
            doc
        },
    );
    support::settle(&mut harness);
    assert_eq!(harness.state().layers().len(), 1);
    // The glyph button's accessible label is its text (the duplicate
    // row deliberately carries no tooltip: its catalog key waits on
    // the stencil wave's i18n lock — see document.rs's row comment).
    harness.get_by_label("⧉").click();
    support::settle(&mut harness);
    let doc = harness.state();
    assert_eq!(doc.layers().len(), 2, "the duplicate landed");
    let copy = doc
        .layers()
        .iter()
        .find(|l| l.name == umber_app::document::duplicate_name("one"))
        .expect("the copy carries the ' copy' name");
    assert_eq!(copy.kind, LayerKind::Paint, "same kind as the source");
    assert!(
        (copy.opacity - 1.0).abs() < f32::EPSILON,
        "settings carried at the default"
    );
    assert_eq!(doc.layers().len(), 2);
}

/// The selected layer's properties rows render under ITS row only:
/// with the layer active they draw (opacity, blend, rename); with Base
/// active they do not — the selection gate is the behavior under test.
#[test]
fn the_properties_rows_render_for_the_selected_layer_only() {
    use umber_core::layers::{LayerCommand, LayerKind};
    let mut selected = support::harness_state(
        layers_ui_closure(umber_app::paint_state::PlaneOwner::Layer(0)),
        {
            let mut doc = umber_app::document::Document::default();
            doc.run(LayerCommand::add("one", LayerKind::Paint));
            doc
        },
    );
    support::settle(&mut selected);
    for key in [
        "history.op-set-opacity",
        "history.op-set-blend",
        "history.op-rename",
    ] {
        assert!(
            support::has_label(&selected, &umber_app::i18n::tr(key)),
            "the {key} row renders for the selected layer"
        );
    }
    let mut base = support::harness_state(
        layers_ui_closure(umber_app::paint_state::PlaneOwner::Base),
        {
            let mut doc = umber_app::document::Document::default();
            doc.run(LayerCommand::add("one", LayerKind::Paint));
            doc
        },
    );
    support::settle(&mut base);
    for key in [
        "history.op-set-opacity",
        "history.op-set-blend",
        "history.op-rename",
    ] {
        assert!(
            !support::has_label(&base, &umber_app::i18n::tr(key)),
            "the {key} row stays hidden while Base is the active plane"
        );
    }
}

/// The blend combo (the channel-picker idiom): open it, pick Multiply,
/// the selected layer's mode becomes Multiply through ONE undoable
/// command — the History panel's undo restores Normal exactly.
#[test]
fn the_blend_combo_sets_the_selected_layers_mode_and_undo_restores() {
    use umber_core::layers::{LayerCommand, LayerKind};
    // Layers + History in ONE draw: the combo lives in the layers
    // properties rows, the ↶ Undo button in the history panel, and the
    // undo must reach the SAME document through the panel a user would
    // click. (The battery's first run caught the layers-only closure:
    // no history node, so the undo query panicked at support's
    // get_by_label.)
    let mut harness = support::harness_state(
        {
            use umber_app::layers_panel::gestures::GestureState;
            move |ui, s: &mut umber_app::document::Document| {
                let defs = std::collections::BTreeMap::<String, String>::new();
                let badges = std::collections::HashMap::<u64, String>::new();
                let mut states = umber_app::material_layer::MaterialLayerStates::default();
                let thumbs = std::collections::HashMap::<u64, egui::TextureHandle>::new();
                let gestures = GestureState::default();
                let mats = umber_app::layers_panel::MaterialPanelContext {
                    defs: &defs,
                    badges: &badges,
                    status: None,
                    states: &mut states,
                };
                let _ = umber_app::document::layers_ui(
                    ui,
                    s,
                    mats,
                    umber_app::paint_state::PlaneOwner::Layer(0),
                    true,
                    &umber_app::document::LayerRowBadges::default(),
                    &thumbs,
                    &gestures,
                );
                umber_app::history_panel::history_ui(ui, s, None);
            }
        },
        {
            let mut doc = umber_app::document::Document::default();
            doc.run(LayerCommand::add("one", LayerKind::Paint));
            doc
        },
    );
    support::settle(&mut harness);
    assert_eq!(
        harness.state().layers()[0].blend_mode,
        umber_core::BlendMode::Normal,
        "the default mode is Normal"
    );
    // The blend combo is the only ComboBox in the layers draw. The
    // options carry egui's Debug labels ("Multiply" — the mode set's
    // own spelling, no i18n involvement).
    harness.get_by_role(egui::accesskit::Role::ComboBox).click();
    support::settle(&mut harness);
    harness.get_by_label("Multiply").click();
    support::settle(&mut harness);
    assert_eq!(
        harness.state().layers()[0].blend_mode,
        umber_core::BlendMode::Multiply,
        "the combo picked Multiply"
    );
    support::click_label(&mut harness, &umber_app::i18n::tr("history.undo"));
    support::settle(&mut harness);
    assert_eq!(
        harness.state().layers()[0].blend_mode,
        umber_core::BlendMode::Normal,
        "one undo restores the mode (one command per change)"
    );
}
