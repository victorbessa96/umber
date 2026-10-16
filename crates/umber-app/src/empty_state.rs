//! The home card: the viewport's empty state while no mesh is loaded
//! (GUI roadmap Phase 0b, docs/specs/gui-audit-and-roadmap.md Part 3;
//! upgraded to a real egui panel by docs/specs/gui-plan-2026-10-15.md
//! kill `g5-home-screen`). It keeps the drop-zone hero's title, hint and
//! format line and adds New / Open buttons (with their live shortcuts),
//! up to [`HOME_RECENT_MAX`] recent projects (hover ×, right-click
//! Remove / Clear all), a "Recover last session" row when the autosave
//! journal holds a snapshot, and double-click-empty = Open.
//!
//! The card never acts itself: it returns a [`HomeCardAction`] and the
//! shell dispatches it (the file dialogs, the confirm gates and the
//! recovery load all live in the shell). The one exception is the recent
//! list's Remove / Clear, which edit [`RecentProjects`] in place (the
//! shell owns it and lends it through [`HomeCardInput`]) and persist it
//! through `RecentProjects::save`, the same file the File ▸ Recent menu
//! writes.
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
//! - **Widgets over the viewport's own response.** The viewport allocates
//!   its whole rect first (orbit, double-click); the card's widgets are
//!   added after it, so they sit on top in egui's hit test and a click on
//!   a button or row never reaches the viewport. A double-click on the
//!   card's empty space (or anywhere outside it) does reach it, and
//!   [`resolve_action`] turns that into [`HomeCardAction::OpenProject`].
//! - **Six of the ten.** `RecentProjects` keeps [`crate::recent::MAX_RECENT`]
//!   (10) for the File menu; the card shows the first [`HOME_RECENT_MAX`]
//!   (6, the plan row's number) and stats only those paths each frame.
//! - **Missing paths gray out, but stay removable.** A row whose path is
//!   gone draws in the faint tone, ignores a click, and says why on hover
//!   (the File menu's `menu.recent-missing` string); its × and context
//!   menu still work, so a dead entry can be cleaned up from here.
//! - **The card grows with its content.** The drop-zone math
//!   ([`zone_rect`]) is the card's minimum; [`card_rect`] grows it to the
//!   estimated content height ([`content_height`]), capped at the
//!   viewport's inset, and a vertical scroll area covers a viewport too
//!   small for the whole card.
//! - **No glyph.** A "drop" icon would come from the lucide atlas, which
//!   isn't rasterized yet (`umber_gui::icons`' `icon-raster` follow-up).
//!   The card is type-only until then; × is a text glyph.
//! - **The drag highlight fires on any hovered file.** egui reports
//!   hovered files with platform-dependent detail (some platforms give no
//!   path until the drop). The accent wash means "a drop is coming", not
//!   "this is a mesh".

use std::path::{Path, PathBuf};

use egui::{
    pos2, Align, Align2, Button, Color32, FontId, Layout, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, WidgetType,
};
use umber_gui::theme;

use crate::actions::shortcut_label;
use crate::i18n::tr;
use crate::recent::{display_name, RecentProjects};
use crate::recovery::RecoveryMeta;
use crate::toolbar::ShellAction;

/// Every mesh extension the loader reads (`umber_mesh::detect_format`).
pub const MESH_EXTENSIONS: [&str; 8] = ["obj", "gltf", "glb", "fbx", "usd", "usda", "usdc", "usdz"];

/// How many recent projects the card lists (the plan row's "up to 6").
pub const HOME_RECENT_MAX: usize = 6;

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
/// Padding inside the card, points.
const CARD_PAD: f32 = 16.0;
/// Gap between the card's sections, points.
const SECTION_GAP: f32 = 12.0;
/// One New / Open button, points.
const BUTTON_SIZE: Vec2 = Vec2::new(150.0, 28.0);
/// Gap between the two buttons, points.
const BUTTON_GAP: f32 = 8.0;
/// One recent row (and the Recover row), points.
const ROW_H: f32 = 24.0;
/// Corner radius of a hovered row.
const ROW_ROUNDING: f32 = 4.0;
/// Text inset inside a row, points.
const ROW_INSET: f32 = 8.0;
/// Line heights the content estimate uses: heading, UI and label text.
const HEADING_LINE: f32 = 22.0;
const UI_LINE: f32 = 18.0;
const LABEL_LINE: f32 = 16.0;

/// What a home-card click asks the shell to do. The card never acts on
/// these itself (see the module docs).
//
// `dead_code`: the binary's shell reads these once main.rs dispatches
// `Viewport::take_home_action` (the integration step of g5-home-screen);
// until then the payload is constructed but never read.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeCardAction {
    /// The New project button (`ShellAction::NewProject`).
    NewProject,
    /// The Open… button, or a double-click on empty space
    /// (`ShellAction::OpenProject`).
    OpenProject,
    /// A recent row: open this project dir.
    OpenRecent(PathBuf),
    /// The Recover last session row: open the autosave snapshot.
    RecoverSession,
}

/// The shell state the card reads: the recent list (Remove / Clear edit
/// it) and the pending autosave snapshot, if any.
//
// `dead_code`: constructed by the shell once main.rs passes it to
// `Viewport::ui_with_home` (the integration step of g5-home-screen).
#[allow(dead_code)]
pub struct HomeCardInput<'a> {
    /// The shell's recent projects.
    pub recent: &'a mut RecentProjects,
    /// The snapshot `recovery::pending` found (`None` hides the row).
    pub recovery: Option<&'a RecoveryMeta>,
}

/// An edit the card made to the recent list this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RecentEdit {
    /// × or context Remove on one row.
    Remove(PathBuf),
    /// Context Clear all.
    Clear,
}

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

/// The card's estimated height for `rows` recent rows and the Recover row
/// (`recover`), with `item_gap` (egui's vertical item spacing) between
/// widgets. Title, hint, buttons, [recent heading + rows], [recover],
/// formats line, plus the padding.
fn content_height(rows: usize, recover: bool, item_gap: f32) -> f32 {
    let mut h = 2.0 * CARD_PAD + HEADING_LINE + UI_LINE + SECTION_GAP + BUTTON_SIZE.y;
    let mut items = 3;
    if rows > 0 {
        h += SECTION_GAP + LABEL_LINE + rows as f32 * ROW_H;
        items += 1 + rows;
    }
    if recover {
        h += SECTION_GAP + ROW_H;
        items += 1;
    }
    h += SECTION_GAP + LABEL_LINE;
    items += 1;
    h + items as f32 * item_gap
}

/// The card inside the viewport `rect`: the drop zone ([`zone_rect`]),
/// grown to `content_h` when the content needs it, never past the
/// viewport's inset. Centered.
fn card_rect(rect: Rect, content_h: f32) -> Rect {
    let zone = zone_rect(rect);
    let max_h = rect.height() * (1.0 - 2.0 * ZONE_MARGIN);
    let h = content_h.max(zone.height()).min(max_h);
    Rect::from_center_size(rect.center(), Vec2::new(zone.width(), h))
}

/// The rows the card lists: the first [`HOME_RECENT_MAX`] recents, most
/// recent first, each with whether it still exists per `exists` (only
/// the listed paths are checked).
fn home_rows(recent: &RecentProjects, exists: impl Fn(&Path) -> bool) -> Vec<(PathBuf, bool)> {
    recent
        .paths()
        .iter()
        .take(HOME_RECENT_MAX)
        .map(|p| (p.clone(), exists(p)))
        .collect()
}

/// A row's two labels: the project's name and its dimmed parent dir
/// (empty for a root).
fn row_labels(path: &Path) -> (String, String) {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    (display_name(path), parent)
}

/// A row's (name, parent) colors: primary + muted while the path exists,
/// both faint once it is gone (the graying decision).
fn row_colors(exists: bool) -> (Color32, Color32) {
    if exists {
        (theme::text(), theme::muted())
    } else {
        (theme::faint(), theme::faint())
    }
}

/// What a click on a recent row does: open it, unless its path is gone.
fn row_click_action(path: &Path, exists: bool, clicked: bool) -> Option<HomeCardAction> {
    (clicked && exists).then(|| HomeCardAction::OpenRecent(path.to_path_buf()))
}

/// The frame's action: the card's own click wins; otherwise a
/// double-click on the empty viewport (`double_clicked_empty`) opens.
pub fn resolve_action(
    card: Option<HomeCardAction>,
    double_clicked_empty: bool,
) -> Option<HomeCardAction> {
    card.or_else(|| double_clicked_empty.then_some(HomeCardAction::OpenProject))
}

/// Applies a Remove / Clear to the list; returns whether it changed.
fn apply_edit(recent: &mut RecentProjects, edit: &RecentEdit) -> bool {
    match edit {
        RecentEdit::Remove(path) => recent.remove(path),
        RecentEdit::Clear => {
            let had = !recent.paths().is_empty();
            recent.clear();
            had
        }
    }
}

/// A New / Open button with its bound shortcut (derived from the key
/// table, `actions::shortcut_label`; an unbound action shows none).
fn home_button(label: String, action: ShellAction, is_mac: bool) -> Button<'static> {
    let button = Button::new(label);
    match shortcut_label(action, is_mac) {
        Some(shortcut) => button.shortcut_text(shortcut),
        None => button,
    }
}

/// Draws the home card into the viewport's `rect` and returns the click,
/// if any. `files_hovering` lights the drop wash and switches the title
/// to "release". `input` is the shell's state; `None` draws the card
/// without recents or the Recover row (New / Open still show).
pub fn home_card(
    ui: &mut Ui,
    rect: Rect,
    files_hovering: bool,
    input: Option<HomeCardInput<'_>>,
) -> Option<HomeCardAction> {
    let (recent, recovery) = match input {
        Some(input) => (Some(input.recent), input.recovery),
        None => (None, None),
    };
    let rows = recent
        .as_deref()
        .map(|r| home_rows(r, Path::exists))
        .unwrap_or_default();
    let item_gap = ui.spacing().item_spacing.y;
    let card = card_rect(
        rect,
        content_height(rows.len(), recovery.is_some(), item_gap),
    );

    // The drop zone: accent wash + outline while a file hovers.
    let painter = ui.painter_at(rect);
    let outline = if files_hovering {
        let [r, g, b, _] = theme::accent().to_array();
        painter.rect_filled(
            card,
            ZONE_ROUNDING,
            Color32::from_rgba_unmultiplied(r, g, b, HOVER_WASH_ALPHA),
        );
        theme::accent()
    } else {
        theme::faint()
    };
    painter.rect_stroke(
        card,
        ZONE_ROUNDING,
        Stroke::new(ZONE_STROKE, outline),
        StrokeKind::Inside,
    );

    let is_mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    let inner = card.shrink(CARD_PAD);
    let mut action = None;
    let mut edit = None;
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt("umber-home-card")
            .max_rect(inner)
            .layout(Layout::top_down(Align::Center)),
    );
    egui::ScrollArea::vertical()
        .id_salt("umber-home-card-scroll")
        .auto_shrink([false, true])
        .max_height(inner.height())
        .show(&mut child, |ui| {
            let title = if files_hovering {
                tr("shell.hero-release")
            } else {
                tr("shell.hero-title")
            };
            ui.label(
                RichText::new(title)
                    .size(theme::TEXT_HEADING)
                    .color(theme::text()),
            );
            ui.label(
                RichText::new(tr("shell.hero-hint"))
                    .size(theme::TEXT_UI)
                    .color(theme::muted()),
            );
            ui.add_space(SECTION_GAP);

            // New / Open, side by side and centered.
            let pair = Vec2::new(2.0 * BUTTON_SIZE.x + BUTTON_GAP, BUTTON_SIZE.y);
            let (pair, _) = ui.allocate_exact_size(pair, Sense::hover());
            let new_rect = Rect::from_min_size(pair.min, BUTTON_SIZE);
            let open_rect = Rect::from_min_size(
                pair.min + Vec2::new(BUTTON_SIZE.x + BUTTON_GAP, 0.0),
                BUTTON_SIZE,
            );
            let new = home_button(tr("home.new-project"), ShellAction::NewProject, is_mac);
            if ui
                .place(new_rect, new)
                .on_hover_text(tr("shell.tip-new"))
                .clicked()
            {
                action = Some(HomeCardAction::NewProject);
            }
            let open = home_button(tr("home.open-project"), ShellAction::OpenProject, is_mac);
            if ui
                .place(open_rect, open)
                .on_hover_text(tr("home.open-tip"))
                .clicked()
            {
                action = Some(HomeCardAction::OpenProject);
            }

            let row_w = ui.available_width();
            if !rows.is_empty() {
                ui.add_space(SECTION_GAP);
                ui.label(
                    RichText::new(tr("home.recent"))
                        .size(theme::TEXT_LABEL)
                        .color(theme::muted()),
                );
            }
            for (path, exists) in &rows {
                let exists = *exists;
                let (row, response) =
                    ui.allocate_exact_size(Vec2::new(row_w, ROW_H), Sense::click());
                let (name, parent) = row_labels(path);
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, exists, &name));
                let hovered = ui.rect_contains_pointer(row);
                if hovered && exists {
                    ui.painter().rect_filled(row, ROW_ROUNDING, theme::hover());
                }
                let remove_rect =
                    Rect::from_min_size(pos2(row.right() - ROW_H, row.top()), Vec2::splat(ROW_H));
                let text = ui.painter_at(row.with_max_x(remove_rect.left()));
                let (name_color, parent_color) = row_colors(exists);
                let name_rect = text.text(
                    pos2(row.left() + ROW_INSET, row.center().y),
                    Align2::LEFT_CENTER,
                    &name,
                    FontId::proportional(theme::TEXT_UI),
                    name_color,
                );
                text.text(
                    pos2(name_rect.right() + ROW_INSET, row.center().y),
                    Align2::LEFT_CENTER,
                    &parent,
                    FontId::proportional(theme::TEXT_LABEL),
                    parent_color,
                );
                if let Some(open) = row_click_action(path, exists, response.clicked()) {
                    action = Some(open);
                }
                let response = if exists {
                    response.on_hover_text(path.display().to_string())
                } else {
                    response.on_hover_text(tr("menu.recent-missing"))
                };
                response.context_menu(|ui| {
                    if ui.button(tr("home.recent-remove")).clicked() {
                        edit = Some(RecentEdit::Remove(path.clone()));
                        ui.close();
                    }
                    if ui.button(tr("home.recent-clear")).clicked() {
                        edit = Some(RecentEdit::Clear);
                        ui.close();
                    }
                });
                if hovered
                    && ui
                        .place(remove_rect, Button::new("×").frame(false))
                        .on_hover_text(tr("home.recent-remove"))
                        .clicked()
                {
                    edit = Some(RecentEdit::Remove(path.clone()));
                }
            }

            if let Some(meta) = recovery {
                ui.add_space(SECTION_GAP);
                let label = RichText::new(tr("home.recover"))
                    .size(theme::TEXT_UI)
                    .color(theme::accent());
                let tip = match meta.source_name() {
                    Some(name) => format!("{}\n{name}", tr("home.recover-tip")),
                    None => tr("home.recover-tip"),
                };
                if ui
                    .add(Button::new(label).min_size(Vec2::new(row_w, ROW_H)))
                    .on_hover_text(tip)
                    .clicked()
                {
                    action = Some(HomeCardAction::RecoverSession);
                }
            }

            ui.add_space(SECTION_GAP);
            ui.label(
                RichText::new(format!("{}  {}", tr("shell.hero-formats"), formats_line()))
                    .font(FontId::monospace(theme::TEXT_LABEL))
                    .color(theme::faint()),
            );
        });

    if let (Some(edit), Some(recent)) = (edit, recent) {
        if apply_edit(recent, &edit) {
            if let Some(path) = crate::recent::recent_path() {
                if let Err(e) = recent.save(&path) {
                    log::warn!("recent projects not saved: {e}");
                }
            }
        }
    }
    action
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

    #[test]
    fn the_card_grows_with_its_content_but_never_past_the_inset() {
        let large = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1600.0, 900.0));
        // Bare card: the drop zone is the minimum.
        assert_eq!(card_rect(large, 100.0), zone_rect(large));
        // Six rows + Recover: taller than the zone, same width, centered.
        let tall = content_height(HOME_RECENT_MAX, true, 4.0);
        assert!(tall > ZONE_MAX.y, "{tall}");
        let card = card_rect(large, tall);
        assert_eq!(card.width(), ZONE_MAX.x);
        assert_eq!(card.height(), tall);
        assert_eq!(card.center(), large.center());
        // A short viewport caps at its inset (the scroll area takes over).
        let short = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1600.0, 300.0));
        assert_eq!(card_rect(short, tall).height(), 240.0);
    }

    #[test]
    fn content_height_counts_rows_and_the_recover_row() {
        let bare = content_height(0, false, 0.0);
        assert_eq!(
            content_height(1, false, 0.0) - bare,
            SECTION_GAP + LABEL_LINE + ROW_H,
            "the first row brings the heading"
        );
        assert_eq!(
            content_height(3, false, 0.0) - content_height(2, false, 0.0),
            ROW_H
        );
        assert_eq!(content_height(0, true, 0.0) - bare, SECTION_GAP + ROW_H);
        assert_eq!(
            content_height(0, false, 4.0) - bare,
            4.0 * 4.0,
            "title, hint, buttons, formats"
        );
    }

    fn recents(n: usize) -> RecentProjects {
        let mut r = RecentProjects::default();
        for i in 0..n {
            r.note(Path::new(&format!("/work/p{i}")));
        }
        r
    }

    #[test]
    fn the_card_lists_at_most_six_most_recent_first() {
        let r = recents(crate::recent::MAX_RECENT);
        assert_eq!(r.paths().len(), 10, "the menu keeps ten");
        let rows = home_rows(&r, |_| true);
        assert_eq!(rows.len(), HOME_RECENT_MAX);
        assert_eq!(rows[0].0, PathBuf::from("/work/p9"), "most recent first");
        assert_eq!(rows[5].0, PathBuf::from("/work/p4"));
        // Fewer than six: all of them.
        assert_eq!(home_rows(&recents(2), |_| true).len(), 2);
        assert!(home_rows(&RecentProjects::default(), |_| true).is_empty());
    }

    #[test]
    fn only_the_listed_rows_are_checked_and_missing_ones_flag() {
        let r = recents(10);
        let checked = std::cell::Cell::new(0);
        let rows = home_rows(&r, |p| {
            checked.set(checked.get() + 1);
            p != Path::new("/work/p8")
        });
        assert_eq!(checked.get(), HOME_RECENT_MAX, "no stat past the sixth");
        assert_eq!(rows[0], (PathBuf::from("/work/p9"), true));
        assert_eq!(rows[1], (PathBuf::from("/work/p8"), false));
    }

    #[test]
    fn missing_rows_gray_out_and_ignore_clicks() {
        assert_eq!(row_colors(true), (theme::text(), theme::muted()));
        assert_eq!(row_colors(false), (theme::faint(), theme::faint()));
        assert_ne!(row_colors(true).0, row_colors(false).0);
        let p = Path::new("/work/sword");
        assert_eq!(
            row_click_action(p, true, true),
            Some(HomeCardAction::OpenRecent(PathBuf::from("/work/sword")))
        );
        assert_eq!(row_click_action(p, false, true), None, "gone: no open");
        assert_eq!(row_click_action(p, true, false), None, "no click");
    }

    #[test]
    fn row_labels_split_name_and_parent() {
        assert_eq!(
            row_labels(Path::new("/work/props/sword")),
            ("sword".to_owned(), "/work/props".to_owned())
        );
        assert_eq!(row_labels(Path::new("/")), ("/".to_owned(), String::new()));
        assert_eq!(
            row_labels(Path::new("sword")),
            ("sword".to_owned(), String::new()),
            "a bare name has no parent to dim"
        );
    }

    #[test]
    fn a_card_click_wins_over_double_click_open() {
        assert_eq!(resolve_action(None, false), None);
        assert_eq!(
            resolve_action(None, true),
            Some(HomeCardAction::OpenProject),
            "double-click on empty space opens"
        );
        assert_eq!(
            resolve_action(Some(HomeCardAction::RecoverSession), true),
            Some(HomeCardAction::RecoverSession)
        );
        assert_eq!(
            resolve_action(Some(HomeCardAction::NewProject), false),
            Some(HomeCardAction::NewProject)
        );
    }

    #[test]
    fn remove_and_clear_edit_the_recent_list() {
        let mut r = recents(3);
        assert!(apply_edit(
            &mut r,
            &RecentEdit::Remove(PathBuf::from("/work/p1"))
        ));
        assert_eq!(
            r.paths(),
            [PathBuf::from("/work/p2"), PathBuf::from("/work/p0")]
        );
        assert!(
            !apply_edit(&mut r, &RecentEdit::Remove(PathBuf::from("/work/p1"))),
            "already gone"
        );
        assert!(apply_edit(&mut r, &RecentEdit::Clear));
        assert!(r.paths().is_empty());
        assert!(!apply_edit(&mut r, &RecentEdit::Clear), "nothing to clear");
    }

    #[test]
    fn the_buttons_show_their_bound_shortcuts_only() {
        // Open is bound (Ctrl/Cmd+O); New has no row in the key table.
        let open = shortcut_label(ShellAction::OpenProject, false);
        assert!(open.as_deref().is_some_and(|s| s.contains('O')), "{open:?}");
        assert_eq!(shortcut_label(ShellAction::NewProject, false), None);
    }
}
