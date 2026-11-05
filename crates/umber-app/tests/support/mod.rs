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
//! - **The fonts gate** ([`fonts_gate`], run by [`settle`]): the Umber
//!   fonts (Inter, JetBrains Mono) are installed on the harness context and
//!   one frame stepped so the atlas is realized before any font-bearing
//!   snapshot compares.
//!
//! Not every helper is used by every test file (each file is its own
//! crate), so the module allows dead code.

#![allow(dead_code)]

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, HarnessBuilder};
use std::sync::atomic::{AtomicBool, Ordering};

/// One frame of simulated time.
pub const STEP_DT: f32 = 1.0 / 60.0;
/// The harness viewport, in points.
pub const SIZE: [f32; 2] = [480.0, 640.0];
/// Frames [`settle`] allows before calling the UI unsettled.
pub const MAX_SETTLE_FRAMES: usize = 120;

/// The Umber font keys the gate checks (first in their families).
pub const UMBER_FONT_KEYS: [&str; 2] = [umber_gui::fonts::INTER_KEY, umber_gui::fonts::MONO_KEY];

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

/// Whether the canonical Umber TTFs are on disk under
/// [`umber_gui::fonts::fonts_dir`]. Fresh CI runners have no `assets/fonts/`
/// (the `font-assets` follow-up pins them); there the gate keeps egui's
/// stock fonts and snapshot tests skip their pixel comparison.
pub fn umber_fonts_present() -> bool {
    let dir = umber_gui::fonts::fonts_dir();
    let inter_leaf = std::path::Path::new(umber_gui::fonts::INTER_PATH)
        .file_name()
        .expect("INTER_PATH ends in a file name");
    let mono_leaf = std::path::Path::new(umber_gui::fonts::MONO_PATH)
        .file_name()
        .expect("MONO_PATH ends in a file name");
    dir.join(inter_leaf).is_file() && dir.join(mono_leaf).is_file()
}

static FONTS_REALIZED: AtomicBool = AtomicBool::new(false);

/// True once a [`fonts_gate`] run has installed the Umber fonts. Snapshot
/// comparisons are meaningful only then (the baselines are font-dependent).
pub fn fonts_realized() -> bool {
    FONTS_REALIZED.load(Ordering::Relaxed)
}

/// `harness.snapshot(name)` when the Umber fonts were realized; otherwise a
/// loud skip — the baselines are font-dependent and the canonical TTFs are
/// the open `font-assets` follow-up.
pub fn snapshot<'a, S>(harness: &mut Harness<'a, S>, name: &str) {
    if fonts_realized() {
        harness.snapshot(name);
    } else {
        eprintln!("snapshot '{name}' skipped: Umber fonts absent (font-assets follow-up)");
    }
}

/// The fonts gate (gui-plan `g0-kittest-harness`): installs the Umber font
/// definitions on the harness context unless they are already there, steps
/// one frame so the atlas pass runs, then asserts every key in `keys` is
/// registered in the definitions and first in its family. Font-bearing
/// snapshots must not compare before this. When the canonical TTFs are
/// absent (fresh CI runner, `font-assets` follow-up open) the gate keeps
/// egui's stock fonts and reports itself unrealized instead of panicking.
pub fn fonts_gate<S>(harness: &mut Harness<'_, S>, keys: &[&str]) {
    let installed = harness
        .ctx
        .fonts(|f| f.definitions().font_data.contains_key(UMBER_FONT_KEYS[0]));
    if !installed {
        if !umber_fonts_present() {
            eprintln!(
                "kittest fonts gate: Umber TTFs absent, stock fonts kept (font-assets follow-up)"
            );
            return;
        }
        harness.ctx.set_fonts(umber_gui::umber_fonts());
        harness.step();
    }
    FONTS_REALIZED.store(true, Ordering::Relaxed);
    let ready = harness.ctx.fonts(|f| {
        let defs = f.definitions();
        keys.iter().all(|k| {
            defs.font_data.contains_key(*k)
                && defs
                    .families
                    .values()
                    .any(|v| v.first().is_some_and(|n| n == k))
        })
    });
    assert!(
        ready,
        "fonts gate: {keys:?} not registered first in their families"
    );
}

/// Steps until the UI is settled (kittest's `run_ok`), and fails the test
/// with the frame budget if it never settles. Runs the [`fonts_gate`] first
/// so every settled harness has the Umber fonts realized.
pub fn settle<S>(harness: &mut Harness<'_, S>) {
    fonts_gate(harness, &UMBER_FONT_KEYS);
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
