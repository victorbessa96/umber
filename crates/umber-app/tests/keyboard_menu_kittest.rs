//! g7-keyboard-only kittests: menu number keys and the Esc-overlay
//! predicate (docs/specs/gui-plan-2026-10-15.md, G7 row).
//!
//! The shell itself is bin-only, so these drive the two lib-exposed
//! pieces directly: `toolbar::menu_digit_pick` inside a real MenuButton,
//! and `actions::esc_closes_overlays` against a harness context.

use std::cell::RefCell;
use std::rc::Rc;

use egui::Key;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use umber_app::actions;
use umber_app::toolbar::{self, ShellAction};

/// A minimal menubar with one menu whose items are collected the way the
/// shell collects them (`items` sink + `menu_digit_pick` at the end).
fn menu_harness(fired: Rc<RefCell<Vec<ShellAction>>>) -> Harness<'static> {
    Harness::new_ui(move |ui| {
        let mut actions = Vec::new();
        egui::containers::menu::MenuBar::new().ui(ui, |ui| {
            egui::containers::menu::MenuButton::new("File").ui(ui, |ui| {
                let mut items = Vec::new();
                for a in [
                    ShellAction::NewProject,
                    ShellAction::OpenProject,
                    ShellAction::SaveProject,
                ] {
                    if ui.button(format!("{a:?}")).clicked() {
                        actions.push(a);
                    }
                    items.push(a);
                }
                toolbar::menu_digit_pick(ui, &items, &mut actions);
            });
        });
        fired.borrow_mut().extend(actions);
    })
}

#[test]
fn menu_number_key_picks_the_nth_item() {
    let fired = Rc::new(RefCell::new(Vec::new()));
    let mut h = menu_harness(fired.clone());
    h.run();
    h.get_by_label("File").click();
    h.run();
    // Digit2 picks the second collected item (OpenProject) and closes.
    h.key_press(Key::Num2);
    h.run();
    assert_eq!(*fired.borrow(), vec![ShellAction::OpenProject]);
}

#[test]
fn menu_number_key_beyond_items_fires_nothing() {
    let fired = Rc::new(RefCell::new(Vec::new()));
    let mut h = menu_harness(fired.clone());
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.key_press(Key::Num9);
    h.run();
    assert!(fired.borrow().is_empty());
}

#[test]
fn esc_closes_overlays_unless_a_text_field_holds_the_keyboard() {
    use std::cell::Cell;
    let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let want_focus = Rc::new(Cell::new(false));
    let seen2 = seen.clone();
    let want2 = want_focus.clone();
    let mut h = Harness::new_ui(move |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let r = ui.text_edit_singleline(&mut String::new());
            if want2.get() {
                r.request_focus();
            }
            seen2
                .borrow_mut()
                .push(actions::esc_closes_overlays(ui.ctx(), false));
        });
    });
    h.run();
    // Nothing focused: Esc may close overlays (key_press queues down+up,
    // step() runs one frame each — check the frames after the injection).
    let before = seen.borrow().len();
    h.key_press(Key::Escape);
    h.step();
    assert!(
        seen.borrow()[before..].contains(&true),
        "Esc with no text focus must close overlays, got {:?}",
        &seen.borrow()[before..]
    );
    // Focus the text edit: Esc belongs to the text field now.
    want_focus.set(true);
    h.run();
    let before = seen.borrow().len();
    h.key_press(Key::Escape);
    h.step();
    assert!(
        !seen.borrow()[before..].contains(&true),
        "Esc with a text field focused must NOT close overlays, got {:?}",
        &seen.borrow()[before..]
    );
    // Palette open: the palette consumes Esc and closes itself.
    let seen3: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let seen4 = seen3.clone();
    let mut h2 = Harness::new_ui(move |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            seen4
                .borrow_mut()
                .push(actions::esc_closes_overlays(ui.ctx(), true));
        });
    });
    h2.run();
    let before = seen3.borrow().len();
    h2.key_press(Key::Escape);
    h2.step();
    assert!(
        !seen3.borrow()[before..].contains(&true),
        "Esc with the palette open must be left to the palette, got {:?}",
        &seen3.borrow()[before..]
    );
}
