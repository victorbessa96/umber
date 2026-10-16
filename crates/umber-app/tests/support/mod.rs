//! The shared kittest support module (docs/specs/gui-plan-2026-10-15.md
//! kill `g0-kittest-harness`, PC:#19). Every kittest file pulls it in with
//! `mod support;` and builds its harness through [`harness`], so all of
//! them share one setup:
//!
//! - **i18n once** ([`ensure_i18n`]): the catalogs are installed before
//!   the first `tr()` renders, or every label reads as its raw key.
//! - **A fixed step** ([`STEP_DT`], 1/60 s): animations and the
//!   tooltip/notice timers advance the same amount per frame on every
//!   host, so a test that steps N frames means the same thing everywhere.
//! - **A fixed size** ([`SIZE`]): layout, and therefore every snapshot,
//!   does not depend on a default that could change upstream.
//! - **Settled frames** ([`settle`]): run until egui stops requesting
//!   repaints, bounded, so a widget that animates forever fails loudly
//!   instead of hanging.
//! - **AccessKit helpers** ([`click_label`], [`has_label`]): tests find
//!   widgets the way a screen reader does, by role and label, never by
//!   pixel position.
//!
//! Not every helper is used by every test file (each file is its own
//! crate), so the module allows dead code.

#![allow(dead_code)]

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, HarnessBuilder};

/// One frame of simulated time.
pub const STEP_DT: f32 = 1.0 / 60.0;
/// The harness viewport, in points.
pub const SIZE: [f32; 2] = [480.0, 640.0];
/// Frames [`settle`] allows before calling the UI unsettled.
pub const MAX_SETTLE_FRAMES: usize = 120;

/// Installs the i18n catalogs once per test binary.
pub fn ensure_i18n() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(umber_app::i18n::init);
}

/// The house builder: i18n installed, [`STEP_DT`] and [`SIZE`] applied.
/// Generic over the harness state (`()` for a plain UI closure).
pub fn builder<S>() -> HarnessBuilder<S> {
    ensure_i18n();
    Harness::<S>::builder()
        .with_step_dt(STEP_DT)
        .with_size(SIZE)
}

/// A harness over a UI closure, through [`builder`].
pub fn harness<'a>(app: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    builder::<()>().build_ui(app)
}

/// A harness over a UI closure with test-owned state.
pub fn harness_state<'a, S>(
    app: impl FnMut(&mut egui::Ui, &mut S) + 'a,
    state: S,
) -> Harness<'a, S> {
    builder::<S>().build_ui_state(app, state)
}

/// Steps until the UI is settled (kittest's `run_ok`), and fails the test
/// with the frame budget if it never settles.
pub fn settle<S>(harness: &mut Harness<'_, S>) {
    assert!(
        harness.run_ok().is_some(),
        "the UI did not settle within egui_kittest's step budget"
    );
}

/// Clicks the widget labelled `label` (exact match), then settles.
pub fn press_enter<S>(harness: &mut Harness<'_, S>) {
    harness.key_press(egui::Key::Enter);
    harness.step();
}

/// Clicks the first widget whose AccessKit label matches exactly.
pub fn click_label<S>(harness: &mut Harness<'_, S>, label: &str) {
    harness.get_by_label(label).click();
    settle(harness);
}

/// Whether a widget labelled exactly `label` exists right now.
pub fn has_label<S>(harness: &Harness<'_, S>, label: &str) -> bool {
    harness.query_by_label(label).is_some()
}
