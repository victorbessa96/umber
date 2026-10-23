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
//!
//! g1-axis-gizmo adds the axis gizmo's snapshot (its pure projection
//! and `paint`, the same call the viewport makes) and a behaviour test
//! that drives the landed click chain (`hit` → `click_target` →
//! `snap_angles`) exactly as `viewport.rs::ui` does.
//!
//! The gizmo lives in `umber-app`'s library surface (`axis_gizmo`)
//! since the supplement wave put it there for this harness.

use egui_kittest::Harness;

/// The catalog must be installed before any `tr()` call renders.
fn ensure_i18n() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(umber_app::i18n::init);
}

#[test]
fn axis_gizmo_snapshot() {
    // The gizmo under the default camera (yaw 0.6, pitch 0.4): six
    // tips, positive solid + lettered, negative hollow, back-to-front.
    // Pure functions + `paint` only — the exact call `viewport.rs`
    // makes with a mesh loaded. Re-derivable from nothing but the
    // camera constants; no mesh, no GPU.
    let tips = umber_app::axis_gizmo::project_axes(
        umber_gpu::OrbitCamera::default().yaw,
        umber_gpu::OrbitCamera::default().pitch,
        umber_app::axis_gizmo::RADIUS,
    );
    let centre = umber_app::axis_gizmo::centre(egui::Rect::from_min_size(
        egui::pos2(0.0, 0.0),
        egui::vec2(200.0, 200.0),
    ));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(200.0, 200.0))
        .build_ui(|ui| {
            umber_app::axis_gizmo::paint(ui.painter(), centre, &tips, None);
        });
    harness.run_ok();
    harness.snapshot("axis_gizmo");
}

#[test]
fn axis_gizmo_click_chain_snap_the_view() {
    // The click chain the viewport runs, driven headless: a pointer on
    // the +X tip resolves to Axis::PosX, and the snap target is the
    // view down +X (eye on the +X side). Clicking the axis the camera
    // already looks along flips to the opposite side (Blender C2.1).
    use umber_app::axis_gizmo::{click_target, hit, snap_angles, Axis};

    let tips = umber_app::axis_gizmo::project_axes(0.6, 0.4, umber_app::axis_gizmo::RADIUS);
    // The +X tip's offset IS the click offset for that tip.
    let x_offset = tips
        .iter()
        .find(|t| t.axis == Axis::PosX)
        .expect("+X tip present")
        .offset;
    assert_eq!(hit(&tips, x_offset), Some(Axis::PosX));

    // Not aligned with anything: the target is the clicked axis.
    let target = click_target(Axis::PosX, 0.6, 0.4);
    assert_eq!(target, Axis::PosX);
    let (yaw, pitch) = snap_angles(target, 0.6);
    let (_, _, back) = umber_app::axis_gizmo::camera_basis(yaw, pitch);
    assert!(back.dot(Axis::PosX.dir()) > 0.999, "eye must sit on +X");

    // Already looking down +X: the same click flips to -X.
    let (yaw, pitch) = snap_angles(Axis::PosX, 0.6);
    assert_eq!(click_target(Axis::PosX, yaw, pitch), Axis::NegX);
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
    let mut doc = umber_app::document::Document::default();
    let mut harness = Harness::builder().build_ui(|ui| {
        umber_app::psd_import::show_section(ui, &mut state, &mut doc);
    });
    harness.run_ok();
    harness.snapshot("psd_import_section");
}

// A2 snapshot-breadth (the W-C3a plan's lane A2): the History panel the
// module doc has claimed since v1 — an empty session, then after two
// journalled layer ops — plus the toolbar (placeholder glyphs through
// the headless-proven `IconAtlas::empty()`, the main.rs test
// precedent) and the rich tooltip. The four PNG baselines cannot be
// produced host-free: they are generated on the build host with
// UPDATE_SNAPSHOTS=1 BEFORE the wave battery (the close script's
// pre-battery step), then compared from then on.

#[test]
fn history_panel_empty_snapshot() {
    ensure_i18n();
    let mut doc = umber_app::document::Document::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(240.0, 320.0))
        .build_ui(|ui| {
            umber_app::history_panel::history_ui(ui, &mut doc, None);
        });
    harness.run_ok();
    harness.snapshot("history_panel_empty");
}

#[test]
fn history_panel_ops_snapshot() {
    ensure_i18n();
    use umber_core::layers::{LayerCommand, LayerKind};
    let mut doc = umber_app::document::Document::default();
    doc.run(LayerCommand::Add {
        name: "ops-a".into(),
        kind: LayerKind::Paint,
        id: None,
    });
    doc.run(LayerCommand::Add {
        name: "ops-b".into(),
        kind: LayerKind::Fill,
        id: None,
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(240.0, 320.0))
        .build_ui(|ui| {
            umber_app::history_panel::history_ui(ui, &mut doc, None);
        });
    harness.run_ok();
    harness.snapshot("history_panel_ops");
}

#[test]
fn toolbar_snapshot() {
    ensure_i18n();
    use umber_app::toolbar::{ToolKind, ToolbarView, ViewToggles};
    let view = ToolbarView {
        tool: ToolKind::Paint,
        can_undo: true,
        can_redo: false,
        view: ViewToggles {
            wireframe: false,
            grid: true,
            view_mode: umber_gpu::ViewMode::default(),
        },
        paint_available: false,
        mask_editing: false,
    };
    let atlas = umber_gui::IconAtlas::empty();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 48.0))
        .build_ui(|ui| {
            umber_app::toolbar::show(ui, &view, &atlas);
        });
    harness.run_ok();
    harness.snapshot("toolbar");
}

#[test]
fn rich_tooltip_snapshot() {
    ensure_i18n();
    use umber_app::rich_tooltip::TooltipParts;
    let parts = TooltipParts {
        title: "Snapshot tooltip".into(),
        description: Some("The rich tooltip's snapshot fixture.".into()),
        shortcut: Some("Ctrl+Shift+T".into()),
        unavailable: None,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(260.0, 90.0))
        .build_ui(|ui| {
            umber_app::rich_tooltip::draw(ui, &parts);
        });
    harness.run_ok();
    harness.snapshot("rich_tooltip");
}
