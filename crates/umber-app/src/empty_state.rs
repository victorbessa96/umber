//! The drop-zone hero: the viewport's empty state while no mesh is
//! loaded (GUI roadmap Phase 0b, docs/specs/gui-audit-and-roadmap.md
//! Part 3). It shows "drop a mesh here", the other ways in, and the
//! formats the loader reads, set in the theme's type scale and palette.
//!
//! # Judgment calls
//!
//! - **The format list is the loader's.** [`MESH_EXTENSIONS`] is every
//!   extension `umber_mesh::detect_format` accepts, pinned by a test. The
//!   File menu's picker filter (`rfd_pick_mesh`) and the drop router both
//!   read it, so the hero cannot advertise a format the loader refuses.
//!   The picker previously lacked `usdc` and `usdz`.
//! - **Mesh drops load the mesh; everything else goes to the mesh-map
//!   import.** Before this module, every drop went to the mesh-map import
//!   (`MeshMapImportState::poll_drops`), so a dropped `.obj` was listed
//!   there as an unmatched map. Now [`is_mesh_file`] routes mesh files to
//!   the mesh load (`UmberApp::load_mesh_path`) and the import skips them.
//!   When several meshes drop at once, the first one loads (one mesh per
//!   session in v1) and the rest are logged. A mesh dropped while a mesh
//!   is loaded replaces it, the same as File > Open Mesh.
//! - **Painter-only, inside the viewport's rect.** The hero draws over the
//!   empty viewport's background and takes no input, so it can't steal
//!   the viewport's orbit drag. There is no "Open Mesh" button inside it:
//!   that would need a return channel out of `Viewport::ui`, and the
//!   toolbar button sits right above it.
//! - **No glyph.** A "drop" icon would come from the lucide atlas, which
//!   isn't rasterized yet (`umber_gui::icons`' `icon-raster` follow-up).
//!   The hero is type-only until then.
//! - **The drag highlight fires on any hovered file.** egui reports
//!   hovered files with platform-dependent detail (some platforms give no
//!   path until the drop). The accent outline means "a drop is coming",
//!   not "this is a mesh".

use std::path::Path;

use egui::{Align2, Color32, FontId, Rect, Stroke, StrokeKind, Ui, Vec2};
use umber_gui::theme;

use crate::i18n::tr;

/// Every mesh extension the loader reads (`umber_mesh::detect_format`).
pub const MESH_EXTENSIONS: [&str; 8] = ["obj", "gltf", "glb", "fbx", "usd", "usda", "usdc", "usdz"];

/// The drop zone's largest size, points (it shrinks with the viewport).
const ZONE_MAX: Vec2 = Vec2::new(520.0, 240.0);
/// The zone's margin inside the viewport, as a fraction of each side.
const ZONE_MARGIN: f32 = 0.1;
/// Corner radius of the zone outline.
const ZONE_ROUNDING: f32 = 8.0;
/// Outline width, points.
const ZONE_STROKE: f32 = 1.5;
/// Alpha of the accent wash while a file hovers.
const HOVER_WASH_ALPHA: u8 = 24;
/// Vertical gap between the hero's three lines, points.
const LINE_GAP: f32 = 26.0;

/// Whether `path` is a mesh the loader reads (case-insensitive extension).
pub fn is_mesh_file(path: &Path) -> bool {
    umber_mesh::detect_format(path).is_ok()
}

/// The hero's format line: `.obj  ·  .gltf  ·  …` in [`MESH_EXTENSIONS`]
/// order. Extensions aren't translated.
pub fn formats_line() -> String {
    MESH_EXTENSIONS
        .iter()
        .map(|ext| format!(".{ext}"))
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// The drop zone inside the viewport `rect`: inset by [`ZONE_MARGIN`],
/// capped at [`ZONE_MAX`], centered.
fn zone_rect(rect: Rect) -> Rect {
    let inset = rect.size() * (1.0 - 2.0 * ZONE_MARGIN);
    Rect::from_center_size(rect.center(), inset.min(ZONE_MAX))
}

/// Paints the hero into the viewport's `rect`. `files_hovering`
/// switches the outline to the accent and the title to "release".
pub fn paint_hero(ui: &Ui, rect: Rect, files_hovering: bool) {
    let zone = zone_rect(rect);
    let painter = ui.painter_at(rect);
    let outline = if files_hovering {
        let [r, g, b, _] = theme::accent().to_array();
        painter.rect_filled(
            zone,
            ZONE_ROUNDING,
            Color32::from_rgba_unmultiplied(r, g, b, HOVER_WASH_ALPHA),
        );
        theme::accent()
    } else {
        theme::faint()
    };
    painter.rect_stroke(
        zone,
        ZONE_ROUNDING,
        Stroke::new(ZONE_STROKE, outline),
        StrokeKind::Inside,
    );

    let center = zone.center();
    let title = if files_hovering {
        tr("shell.hero-release")
    } else {
        tr("shell.hero-title")
    };
    painter.text(
        center - Vec2::new(0.0, LINE_GAP),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(theme::TEXT_HEADING),
        theme::text(),
    );
    painter.text(
        center,
        Align2::CENTER_CENTER,
        tr("shell.hero-hint"),
        FontId::proportional(theme::TEXT_UI),
        theme::muted(),
    );
    painter.text(
        center + Vec2::new(0.0, LINE_GAP),
        Align2::CENTER_CENTER,
        format!("{}  {}", tr("shell.hero-formats"), formats_line()),
        FontId::monospace(theme::TEXT_LABEL),
        theme::faint(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_extension_is_one_the_loader_reads() {
        for ext in MESH_EXTENSIONS {
            let path = format!("model.{ext}");
            assert!(is_mesh_file(Path::new(&path)), "{ext}");
        }
    }

    #[test]
    fn mesh_routing_is_case_insensitive_and_rejects_maps_and_folders() {
        assert!(is_mesh_file(Path::new("/tmp/Robot.FBX")));
        assert!(is_mesh_file(Path::new("scene.Usdz")));
        assert!(!is_mesh_file(Path::new("Robot_BaseColor.png")));
        assert!(!is_mesh_file(Path::new("Robot_Normal.exr")));
        assert!(!is_mesh_file(Path::new("/tmp/maps")));
    }

    #[test]
    fn formats_line_lists_every_extension_in_order() {
        assert_eq!(
            formats_line(),
            ".obj  ·  .gltf  ·  .glb  ·  .fbx  ·  .usd  ·  .usda  ·  .usdc  ·  .usdz"
        );
    }

    #[test]
    fn zone_insets_then_caps() {
        // Small viewport: 10% margin each side.
        let small = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(400.0, 200.0));
        let z = zone_rect(small);
        assert_eq!(z.size(), Vec2::new(320.0, 160.0));
        assert_eq!(z.center(), egui::pos2(200.0, 100.0));
        // Large viewport: capped at ZONE_MAX, still centered.
        let large = Rect::from_min_size(egui::pos2(100.0, 50.0), Vec2::new(1600.0, 900.0));
        let z = zone_rect(large);
        assert_eq!(z.size(), Vec2::new(520.0, 240.0));
        assert_eq!(z.center(), egui::pos2(900.0, 500.0));
    }
}
