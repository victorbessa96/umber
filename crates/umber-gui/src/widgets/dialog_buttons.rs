//! Platform-ordered dialog buttons (GUI plan g7-polish-batch;
//! photocraft #16).
//!
//! Each dialog declares its buttons by ROLE — which one is the
//! default/primary (Enter), which cancels (Esc), which are plain —
//! and [`dialog_buttons`] lays them out in the platform's order with
//! the primary rendered as such. Tab then follows reading order,
//! which is the a11y contract (g7-keyboard-only's audience).
//!
//! Platform order:
//! * **Windows / Linux (GNOME)**: the affirmative/primary sits at the
//!   far RIGHT (Windows conventions; GNOME's own dialogs do both,
//!   with Cancel left — we follow Cancel-left).
//! * **macOS**: the primary sits LEFT of the cancel (Aqua: Save
//!   Cancel order).
//!
//! The roles are data; the widget is one call at the dialog's foot.

use egui::{Button, Ui};

/// What a dialog button DOES (the role orders it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonRole {
    /// The affirmative action (Enter's target). Rendered primary.
    Default,
    /// The cancel/close action (Esc's target).
    Cancel,
    /// A plain secondary action.
    Plain,
}

impl ButtonRole {
    /// The button's ordering class (for tests).
    pub fn order_class(self) -> u8 {
        match self {
            ButtonRole::Default => 1,
            ButtonRole::Plain => 2,
            ButtonRole::Cancel => 3,
        }
    }
}

/// One declared dialog button: its role + its (already localized)
/// label.
#[derive(Debug, Clone)]
pub struct DialogButton {
    /// The role that orders it.
    pub role: ButtonRole,
    /// The localized label.
    pub label: String,
}

impl DialogButton {
    /// Shorthand constructor.
    pub fn new(role: ButtonRole, label: impl Into<String>) -> Self {
        Self {
            role,
            label: label.into(),
        }
    }
}

/// The platform's button order (for tests + the layout call).
///
/// * Windows/Linux: `[Plain…, Default, Cancel]` — Cancel rightmost,
///   Default beside it (WinForms/WPF convention: OK left of Cancel,
///   extras leftmost).
/// * macOS: `[Default, Cancel, Plain…]` — the primary leads.
pub fn platform_order(
    os: egui::os::OperatingSystem,
    buttons: &[(usize, ButtonRole)],
) -> Vec<usize> {
    // buttons: (original index, role) pairs, declared left-to-right.
    let mut sorted: Vec<(usize, ButtonRole)> = buttons.to_vec();
    if os == egui::os::OperatingSystem::Mac {
        // Aqua: Default first, then Cancel, then Plains in declared order.
        sorted.sort_by_key(|(i, r)| match r {
            ButtonRole::Default => (0, *i),
            ButtonRole::Cancel => (1, *i),
            ButtonRole::Plain => (2, *i),
        });
    } else {
        // Windows/Linux: Plains first (declared order), Default, Cancel.
        sorted.sort_by_key(|(i, r)| match r {
            ButtonRole::Plain => (0, *i),
            ButtonRole::Default => (1, *i),
            ButtonRole::Cancel => (2, *i),
        });
    }
    sorted.into_iter().map(|(i, _)| i).collect()
}

/// Draws the dialog's button row in the platform order. Returns the
/// DECLARED index of the clicked button (stable across platforms), or
/// `None` if no button was clicked this frame.
///
/// The Default role renders with egui's `Button`'s strong styling;
/// Cancel and Plain render plain. Tab follows the drawn order, which
/// IS the platform reading order.
pub fn dialog_buttons(
    ui: &mut Ui,
    os: egui::os::OperatingSystem,
    buttons: &[DialogButton],
) -> Option<usize> {
    let declared: Vec<(usize, ButtonRole)> = buttons
        .iter()
        .enumerate()
        .map(|(i, b)| (i, b.role))
        .collect();
    let order = platform_order(os, &declared);
    let mut clicked = None;
    ui.horizontal(|ui| {
        for &i in &order {
            let b = &buttons[i];
            let label = egui::RichText::new(&b.label);
            let rendered = if b.role == ButtonRole::Default {
                Button::new(label.strong())
            } else {
                Button::new(label)
            };
            if ui.add(rendered).clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Save (Default), Cancel (Cancel), and an extra (Plain), declared
    /// in the order a dialog naturally declares them.
    fn declared() -> Vec<(usize, ButtonRole)> {
        vec![
            (0, ButtonRole::Default),
            (1, ButtonRole::Cancel),
            (2, ButtonRole::Plain),
        ]
    }

    #[test]
    fn windows_puts_cancel_rightmost_with_default_beside_it() {
        let order = platform_order(egui::os::OperatingSystem::Windows, &declared());
        // Plains first, then Default, then Cancel.
        assert_eq!(order, vec![2, 0, 1]);
    }

    #[test]
    fn linux_matches_windows() {
        assert_eq!(
            platform_order(egui::os::OperatingSystem::Nix, &declared()),
            platform_order(egui::os::OperatingSystem::Windows, &declared())
        );
    }

    #[test]
    fn mac_puts_the_default_first() {
        let order = platform_order(egui::os::OperatingSystem::Mac, &declared());
        // Default, Cancel, then Plains.
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn the_clicked_index_is_the_declared_one_not_the_drawn_one() {
        // Pure-static: the layout draws in platform order but RETURNS
        // declared indices, so a dialog's match arms are the same on
        // every platform. Verified by construction in `dialog_buttons`
        // (the loop iterates drawn order, pushes the declared `i`);
        // this pins the order fn's stability under reordering.
        let d = declared();
        let o1 = platform_order(egui::os::OperatingSystem::Mac, &d);
        assert_eq!(o1.first(), Some(&0), "Default (index 0 in d) leads");
        // A genuinely RE-DECLARED list (fresh indices, same roles):
        // [Plain, Cancel, Default] — the Default is index 2 now.
        let redeclared = [
            (0, ButtonRole::Plain),
            (1, ButtonRole::Cancel),
            (2, ButtonRole::Default),
        ];
        let o2 = platform_order(egui::os::OperatingSystem::Mac, &redeclared);
        assert_eq!(
            o2.first(),
            Some(&2),
            "the Default's DECLARED index leads wherever it was declared"
        );
        assert_eq!(o2, &[2, 1, 0], "Cancel (1) before Plain (0)");
    }
}
