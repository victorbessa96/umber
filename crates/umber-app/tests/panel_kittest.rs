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
