//! Offscreen panel snapshot tests (queue row 15 — the absorbed
//! photocraft harness pattern, on egui's own kittest).
//!
//! The panels render into a headless Harness; `snapshot` compares the
//! PNG against `tests/snapshots/<name>.png` (UPDATE_SNAPSHOTS=true
//! regenerates; CI compares). v1 pins the three simplest panels whose
//! state is constructible headless: the Display panel (settings +
//! the LUT state), the History panel (an empty session), and the
//! PSD import section (the no-files arm). The GPU-hungry panels
//! (Viewport, Bakes) stay out until their state is headless-shaped.

use egui_kittest::Harness;

/// The catalog must be installed before any `tr()` call renders.
fn ensure_i18n() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(umber_app::i18n::init);
}

#[test]
fn display_panel_snapshot() {
    ensure_i18n();
    let mut settings = umber_color::DisplaySettings::default();
    let mut lut = umber_app::display_panel::DisplayLutState::default();
    let mut harness = Harness::builder().build_ui(|ui| {
        umber_app::display_panel::show(ui, &mut settings, &mut lut);
    });
    harness.run_ok();
    harness.snapshot("display_panel");
}

#[test]
fn psd_import_section_snapshot() {
    ensure_i18n();
    let mut state = umber_app::psd_import::PsdImportState::default();
    let mut harness = Harness::builder().build_ui(|ui| {
        umber_app::psd_import::show_section(ui, &mut state);
    });
    harness.run_ok();
    harness.snapshot("psd_import_section");
}
