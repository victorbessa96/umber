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
