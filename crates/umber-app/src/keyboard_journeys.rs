//! wave-a11y-first-pass: the G7 keyboard-only journeys, as kittests
//! (docs/loop/briefs/2026-11-11-a11y-first-pass.md: the kittests define
//! done). Every journey is driven by key presses only (Tab, Shift+Tab,
//! Enter, Space, Esc, number keys); the pointer is never used.
//!
//! Journeys:
//! - **Import**: File menu open → its number key fires Open Mesh; the dock
//!   tab strip is Tab-reachable, each tab names its panel, Enter selects.
//! - **Paint**: the toolbar's tool strip is Tab-reachable slot by slot,
//!   each slot names its OWN tool, Enter picks it, and the focused slot
//!   paints the theme focus ring.
//! - **Export**: the Export panel's preset group header, both combos, the
//!   toggles and the Export button are reached in order, all named; Space
//!   opens a combo; Esc closes the combo without changing the selection;
//!   Shift+Tab walks back.
//! - **Esc**: Esc with a popup open is the popup's, never the window's.
//!
//! On the BIN target (export_dialog and the dock viewer are bin-private):
//! `cargo test -p umber-app --bin umber keyboard_journeys`.

use egui::accesskit::Role;
use egui::Key;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use crate::i18n::tr;
use crate::toolbar::{self, ShellAction, ToolKind, ToolbarView};

fn ensure_i18n() {
    crate::i18n::install(crate::i18n::I18n::builtin());
}

/// The accessible name of the node holding keyboard focus, with its role.
fn focused<S>(h: &Harness<'_, S>) -> Option<(Role, String)> {
    h.query_all_by(|n| n.is_focused()).next().map(|n| {
        (
            n.accesskit_node().role(),
            n.accesskit_node().label().unwrap_or_default(),
        )
    })
}

/// Presses `key` and settles (two frames: egui moves focus at the end of
/// the press frame, and the newly focused widget reports it a frame on).
fn press<S>(h: &mut Harness<'_, S>, key: Key) {
    h.key_press(key);
    h.run();
}

fn shift_tab<S>(h: &mut Harness<'_, S>) {
    h.key_press_modifiers(egui::Modifiers::SHIFT, Key::Tab);
    h.run();
}

/// Tabs forward until the focused node is `want` (role + name), at most
/// `budget` presses; returns the names visited on the way, in order.
fn tab_to<S>(h: &mut Harness<'_, S>, want: (Role, &str), budget: usize) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..budget {
        press(h, Key::Tab);
        let now = focused(h);
        if let Some((role, label)) = &now {
            seen.push(label.clone());
            if *role == want.0 && label == want.1 {
                return seen;
            }
        }
    }
    panic!("Tab never reached {want:?} in {budget} presses; visited {seen:?}");
}

// ---------------------------------------------------------------------------
// Paint: the toolbar's tool strip.

struct Bar {
    view: ToolbarView,
    fired: Vec<ShellAction>,
    atlas: umber_gui::IconAtlas,
}

fn toolbar_harness() -> Harness<'static, Bar> {
    ensure_i18n();
    Harness::builder()
        .with_size(egui::vec2(1400.0, 60.0))
        .build_ui_state(
            |ui, s: &mut Bar| {
                let fired = toolbar::show(ui, &s.view, &s.atlas);
                s.fired.extend(fired);
            },
            Bar {
                view: ToolbarView {
                    tool: ToolKind::Paint,
                    can_undo: false,
                    can_redo: false,
                    view: Default::default(),
                    paint_available: true,
                    mask_editing: false,
                },
                fired: Vec::new(),
                atlas: umber_gui::IconAtlas::empty(),
            },
        )
}

#[test]
fn paint_journey_tab_reaches_each_tool_by_its_own_name_and_enter_picks_it() {
    let mut h = toolbar_harness();
    h.run();
    let paint = ToolKind::Paint.label();
    let smudge = ToolKind::Smudge.label();
    // From nothing focused, Tab walks the toolbar left to right: New,
    // the file icons, then the tool strip, Paint first.
    let seen = tab_to(&mut h, (Role::Button, &paint), 12);
    assert_eq!(
        seen.first().map(String::as_str),
        Some(tr("shell.new").as_str())
    );
    // The next stop is the NEXT tool, named as itself (the row used to
    // relabel slot 1 with the active tool's name).
    press(&mut h, Key::Tab);
    assert_eq!(focused(&h), Some((Role::Button, smudge.clone())));
    // Each slot is a toggle: Paint is the active one, Smudge is not.
    let toggled = |h: &Harness<'_, Bar>, name: &str| {
        h.get_by_role_and_label(Role::Button, name)
            .accesskit_node()
            .toggled()
    };
    use egui::accesskit::Toggled;
    assert_eq!(toggled(&h, &paint), Some(Toggled::True));
    assert_eq!(toggled(&h, &smudge), Some(Toggled::False));
    // Enter on the focused slot picks the tool (no pointer).
    press(&mut h, Key::Enter);
    assert_eq!(
        h.state().fired,
        vec![ShellAction::SelectTool(ToolKind::Smudge)]
    );
    // Shift+Tab walks back to Paint.
    shift_tab(&mut h);
    assert_eq!(focused(&h), Some((Role::Button, paint)));
}

#[test]
fn paint_journey_the_focused_icon_slot_paints_the_theme_focus_ring() {
    let mut h = toolbar_harness();
    h.run();
    let paint = ToolKind::Paint.label();
    let rings = |h: &Harness<'_, Bar>| {
        let ring = umber_gui::theme::focus_ring_stroke();
        h.output()
            .shapes
            .iter()
            .filter(|c| {
                matches!(&c.shape, egui::Shape::Rect(r) if r.stroke == ring
                    && r.fill == egui::Color32::TRANSPARENT)
            })
            .count()
    };
    assert_eq!(rings(&h), 0, "nothing focused, no ring");
    tab_to(&mut h, (Role::Button, &paint), 12);
    assert_eq!(rings(&h), 1, "exactly the focused slot rings");
}

// ---------------------------------------------------------------------------
// Import: the File menu's number keys + the dock tab strip.

#[test]
fn import_journey_file_menu_number_key_fires_open_mesh() {
    // The shell's File menu collects New, Open Mesh, Open Project…; the
    // digit pick is keyboard-only end to end: open the menu from the
    // keyboard (Tab to the menu button, Enter), then press 2.
    ensure_i18n();
    let fired = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = fired.clone();
    let mut h = Harness::new_ui(move |ui| {
        let mut actions = Vec::new();
        egui::containers::menu::MenuBar::new().ui(ui, |ui| {
            egui::containers::menu::MenuButton::new(tr("menu.file")).ui(ui, |ui| {
                let mut items = Vec::new();
                for (key, a) in [
                    ("shell.new", ShellAction::NewProject),
                    ("menu.open-mesh", ShellAction::OpenMesh),
                    ("menu.open-project", ShellAction::OpenProject),
                ] {
                    if ui.button(tr(key)).clicked() {
                        actions.push(a);
                    }
                    items.push(a);
                }
                toolbar::menu_digit_pick(ui, &items, &mut actions);
            });
        });
        sink.borrow_mut().extend(actions);
    });
    h.run();
    tab_to(&mut h, (Role::Button, &tr("menu.file")), 3);
    press(&mut h, Key::Enter);
    assert!(
        h.query_by_label(&tr("menu.open-mesh")).is_some(),
        "Enter on the focused menu button opened the menu"
    );
    press(&mut h, Key::Num2);
    assert_eq!(*fired.borrow(), vec![ShellAction::OpenMesh]);
    assert!(
        h.query_by_label(&tr("menu.open-mesh")).is_none(),
        "the pick closed the menu"
    );
}

/// A minimal dock over the shell's real `on_tab_button` naming.
struct DockViewer;

impl egui_dock::TabViewer for DockViewer {
    type Tab = crate::Panel;
    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(tab)
    }
    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        crate::panel_title(tab).into()
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        ui.label(crate::panel_title(tab));
    }
    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        crate::dock_tab_a11y(&response.ctx, &crate::panel_title(tab), response);
    }
}

#[test]
fn import_journey_dock_tabs_are_named_tab_stops_and_enter_selects() {
    use crate::Panel;
    ensure_i18n();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 400.0))
        .build_ui_state(
            |ui, dock: &mut egui_dock::DockState<Panel>| {
                egui_dock::DockArea::new(dock)
                    .show_close_buttons(false)
                    .show_add_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut DockViewer);
            },
            egui_dock::DockState::new(vec![Panel::LayerStack, Panel::History, Panel::Export]),
        );
    h.run();
    // Every tab is a named Tab node.
    for p in [Panel::LayerStack, Panel::History, Panel::Export] {
        let _ = h.get_by_role_and_label(Role::Tab, &crate::panel_title(&p));
    }
    // Keyboard only: Tab to the Export tab, Enter selects it.
    let export = crate::panel_title(&Panel::Export);
    tab_to(&mut h, (Role::Tab, &export), 6);
    press(&mut h, Key::Enter);
    let (_, leaf) = h.state().iter_leaves().next().expect("one leaf");
    assert_eq!(leaf.tabs[leaf.active.0], Panel::Export);
}

// ---------------------------------------------------------------------------
// Export: the Export panel, keyboard only.

fn export_harness() -> Harness<'static, crate::export_dialog::ExportDialog> {
    use crate::export_dialog::{ExportContext, ExportDialog};
    ensure_i18n();
    Harness::builder()
        .with_size(egui::vec2(480.0, 640.0))
        .build_ui_state(
            |ui, d: &mut ExportDialog| {
                d.show(
                    ui,
                    ExportContext {
                        gpu: None,
                        mesh: None,
                        mesh_path: None,
                        paint: None,
                        doc: None,
                        graph: None,
                        baked_normal: None,
                    },
                );
            },
            ExportDialog::new(std::path::PathBuf::from("/tmp/umber-a11y-export")),
        )
}

#[test]
fn export_journey_tab_order_is_header_combos_toggles_and_every_stop_is_named() {
    let mut h = export_harness();
    h.run();
    let preset = tr("export.preset");
    // The walk from nothing focused, in reading order. Every stop must
    // carry an accessible name (a named role, never "unknown").
    let mut stops: Vec<(Role, String)> = Vec::new();
    for _ in 0..8 {
        press(&mut h, Key::Tab);
        let f = focused(&h).expect("Tab always lands on something here");
        assert!(
            !f.1.is_empty(),
            "an unnamed Tab stop: {f:?} after {stops:?}"
        );
        if stops.first() == Some(&f) {
            break; // wrapped around
        }
        stops.push(f);
    }
    let expected = [
        (Role::Button, preset.clone()), // the group header
        (Role::ComboBox, preset.clone()),
        (Role::ComboBox, tr("export.size")),
        (Role::CheckBox, tr("export.materialx")),
        (Role::CheckBox, tr("export.usda")),
        (Role::Button, tr("button.choose")),
        (Role::Button, tr("tiles.label")), // the tiles group header
    ];
    assert_eq!(
        &stops[..expected.len()],
        &expected[..],
        "full walk {stops:?}"
    );
    // Shift+Tab walks the same order backwards, from a known stop.
    tab_to(&mut h, (Role::Button, &tr("button.choose")), 10);
    shift_tab(&mut h);
    assert_eq!(
        focused(&h),
        Some(expected[4].clone()),
        "Choose back to USDA"
    );
    shift_tab(&mut h);
    assert_eq!(
        focused(&h),
        Some(expected[3].clone()),
        "USDA back to MaterialX"
    );
}

#[test]
fn export_journey_space_toggles_and_esc_closes_a_combo_without_a_pick() {
    let mut h = export_harness();
    h.run();
    let before = h.state().preset();
    let mtlx = h.state().materialx();
    // Space on a checkbox flips it.
    tab_to(&mut h, (Role::CheckBox, &tr("export.materialx")), 8);
    press(&mut h, Key::Space);
    assert_eq!(h.state().materialx(), !mtlx);
    // Back to the preset combo; Space opens it.
    shift_tab(&mut h);
    shift_tab(&mut h);
    assert_eq!(focused(&h), Some((Role::ComboBox, tr("export.preset"))));
    press(&mut h, Key::Space);
    assert!(egui::Popup::is_any_open(&h.ctx), "Space opened the combo");
    // Esc closes the popup and leaves the selection alone.
    press(&mut h, Key::Escape);
    assert!(!egui::Popup::is_any_open(&h.ctx), "Esc closed the combo");
    assert_eq!(h.state().preset(), before, "Esc picked nothing");
}

#[test]
fn export_journey_the_group_header_toggles_from_the_keyboard() {
    let mut h = export_harness();
    h.run();
    let preset = tr("export.preset");
    let materialx = tr("export.materialx");
    assert!(h.query_by_label(&materialx).is_some(), "open by default");
    tab_to(&mut h, (Role::Button, &preset), 2);
    use egui::accesskit::Toggled;
    let header_state = |h: &Harness<'_, _>| {
        h.get_by_role_and_label(Role::Button, &preset)
            .accesskit_node()
            .toggled()
    };
    assert_eq!(header_state(&h), Some(Toggled::True), "reports open");
    press(&mut h, Key::Enter);
    assert!(
        h.query_by_label(&materialx).is_none(),
        "Enter folded the group"
    );
    assert_eq!(header_state(&h), Some(Toggled::False), "reports folded");
}

// ---------------------------------------------------------------------------
// Esc: the topmost thing only.

#[test]
fn esc_journey_an_open_popup_owns_esc_and_the_window_behind_it_stays() {
    // A Preferences-like window holding a combo: Esc with the combo open
    // closes the combo only; the next Esc closes the window.
    ensure_i18n();
    struct S {
        open: bool,
        pick: u8,
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(480.0, 400.0))
        .build_ui_state(
            |ui, s: &mut S| {
                if crate::actions::esc_closes_overlays(ui.ctx(), false) {
                    s.open = false;
                }
                if s.open {
                    egui::Window::new("Prefs").show(ui.ctx(), |ui| {
                        egui::ComboBox::from_label("Theme")
                            .selected_text(format!("{}", s.pick))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut s.pick, 0, "0");
                                ui.selectable_value(&mut s.pick, 1, "1");
                            });
                    });
                }
            },
            S {
                open: true,
                pick: 0,
            },
        );
    h.run();
    tab_to(&mut h, (Role::ComboBox, "Theme"), 4);
    press(&mut h, Key::Space);
    assert!(egui::Popup::is_any_open(&h.ctx), "the combo is open");
    press(&mut h, Key::Escape);
    assert!(!egui::Popup::is_any_open(&h.ctx), "Esc closed the combo");
    assert!(h.state().open, "…and NOT the window behind it");
    press(&mut h, Key::Escape);
    assert!(!h.state().open, "the next Esc closes the window");
}

#[test]
fn esc_journey_the_colour_picker_owns_esc_inside_a_window() {
    ensure_i18n();
    struct S {
        open: bool,
        colour: [f32; 3],
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(480.0, 480.0))
        .build_ui_state(
            |ui, s: &mut S| {
                if crate::actions::esc_closes_overlays(ui.ctx(), false) {
                    s.open = false;
                }
                if s.open {
                    egui::Window::new("Fill").show(ui.ctx(), |ui| {
                        umber_gui::widgets::color_picker::color_button(ui, &mut s.colour);
                    });
                }
            },
            S {
                open: true,
                colour: [0.5, 0.2, 0.1],
            },
        );
    h.run();
    tab_to(&mut h, (Role::Button, "colour"), 4);
    press(&mut h, Key::Enter);
    let _ = h.get_by_role_and_label(Role::Slider, "hue"); // the picker is up
    press(&mut h, Key::Escape);
    assert!(
        h.query_by_role_and_label(Role::Slider, "hue").is_none(),
        "Esc closed the picker"
    );
    assert!(h.state().open, "…and NOT the window it sits in");
    press(&mut h, Key::Escape);
    assert!(!h.state().open, "the next Esc closes the window");
}
