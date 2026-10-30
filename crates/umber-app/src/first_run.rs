//! First-run Quick Setup (GUI plan G5 `g5-first-run`, B:A9.2): a small
//! window the very first launch shows — language, theme, navigation
//! preset, keymap preset, and a tablet check — re-openable from
//! Help ▸ Quick setup.
//!
//! # When it shows
//!
//! [`should_show`] is true when the preferences file does not exist
//! (the user has never chosen anything — `preferences::prefs_path` is
//! the persisted-choices home). The window writes the file on Finish
//! (through [`Setup`]'s apply), so it shows exactly once unless the
//! file is deleted.
//!
//! # The choices
//!
//! - **Language** switches the thread locale AND persists it
//!   ([`i18n::set_locale_persisted`]) — the same path the Preferences
//!   window's picker uses.
//! - **Theme** picks Light / Dark / Follow-OS, applied through
//!   `umber_gui::theme::apply` on the next frame (the shell reads
//!   [`Setup::theme`] at apply time).
//! - **Navigation preset** writes `Preferences::nav` (Painter default,
//!   Blender the alternative) and saves.
//! - **Keymap preset** writes the keymap overrides line (Umber default
//!   / Blender-like) through the same `Overrides` machinery the
//!   keymap editor uses, and installs them.
//! - **Tablet check** is informational: `StylusPoller::hovering` /
//!   pressure, so a pen user confirms their tablet is alive before
//!   the first stroke.

use egui::{Align, ComboBox, Window};
use umber_gui::theme;

use crate::i18n::tr;
use crate::navigation::NavPreset;
use crate::preferences::{prefs_path, Preferences};
use crate::stylus_input::StylusPoller;

/// The theme choice offered at first run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetupTheme {
    /// The OS appearance (egui's default behavior).
    #[default]
    FollowOs,
    /// Force the light palette.
    Light,
    /// Force the dark palette.
    Dark,
}

impl SetupTheme {
    /// Every variant in combo order.
    pub const ALL: [Self; 3] = [Self::FollowOs, Self::Light, Self::Dark];

    /// The egui theme this choice maps to (`None` = follow the OS).
    pub fn egui_theme(self) -> Option<egui::ThemePreference> {
        match self {
            Self::FollowOs => None,
            Self::Light => Some(egui::ThemePreference::Light),
            Self::Dark => Some(egui::ThemePreference::Dark),
        }
    }
}

/// Whether Quick Setup should show: no preferences file exists yet.
pub fn should_show() -> bool {
    prefs_path().is_none_or(|p| !p.exists())
}

/// The Quick Setup state: the in-flight choices before Finish.
#[derive(Debug, Default)]
pub struct QuickSetup {
    /// The chosen theme.
    pub theme: SetupTheme,
    /// The chosen navigation preset (Painter default).
    pub nav: NavPreset,
    /// The chosen keymap preset (Umber default).
    pub keymap_blender: bool,
    open: bool,
}

impl QuickSetup {
    /// Opens the window (Help ▸ Quick setup; also the first-launch path).
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Whether the window is open (the shell gates its draw on this).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Closes the window without applying choices (Esc's path).
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Draws the Quick Setup window. `i18n` is the locale picker's own
    /// state (the shell's copy — the picker switches the thread
    /// locale, so the window's OWN labels re-localize the frame after
    /// the switch). `stylus` feeds the tablet check. Returns the
    /// FINISHED choices when the user clicks Finish (the shell applies
    /// them and the window closes).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        stylus: &StylusPoller,
        current_prefs: &Preferences,
    ) -> Option<FinishedSetup> {
        let mut finished = None;
        let mut open = self.open;
        // The buttons close through a flag: `.open(&mut open)` holds the
        // mutable borrow while the window body runs, so the nested
        // closures cannot write `open` directly.
        let mut close = false;
        Window::new(tr("setup.title"))
            .id(egui::Id::new("umber-quick-setup"))
            .collapsible(false)
            .resizable(false)
            .min_width(320.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("setup.intro"));
                ui.add_space(8.0);

                // Language: the same picker the Preferences window
                // uses, over the thread's own i18n state.
                ui.label(tr("setup.language"));
                let (current, locales) =
                    crate::i18n::with(|i| (i.locale().to_string(), i.locales()));
                let mut picked = current.clone();
                ComboBox::from_id_salt("setup-language")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for loc in &locales {
                            ui.selectable_value(&mut picked, loc.clone(), loc.clone());
                        }
                    });
                // The switch applies immediately so the window's own
                // labels re-localize (the picker's own behavior).
                if picked != crate::i18n::with(|i| i.locale().to_string()) {
                    crate::i18n::set_locale_persisted(&picked);
                }
                ui.add_space(8.0);

                // Theme.
                ui.label(tr("setup.theme"));
                let theme_label = |t: SetupTheme| match t {
                    SetupTheme::FollowOs => tr("setup.theme-follow-os"),
                    SetupTheme::Light => tr("setup.theme-light"),
                    SetupTheme::Dark => tr("setup.theme-dark"),
                };
                ComboBox::from_id_salt("setup-theme")
                    .selected_text(theme_label(self.theme))
                    .show_ui(ui, |ui| {
                        for t in SetupTheme::ALL {
                            ui.selectable_value(&mut self.theme, t, theme_label(t));
                        }
                    });
                ui.add_space(8.0);

                // Navigation preset: the Preferences window's own rows
                // (the same keys, so the two surfaces cannot drift).
                ui.label(tr("setup.navigation"));
                let mut nav = self.nav;
                let nav_label = |n: NavPreset| match n {
                    NavPreset::Painter => tr("prefs.nav-painter"),
                    NavPreset::Umber => tr("prefs.nav-umber"),
                    NavPreset::Blender => tr("prefs.nav-blender"),
                };
                ComboBox::from_id_salt("setup-nav")
                    .selected_text(nav_label(nav))
                    .show_ui(ui, |ui| {
                        for preset in NavPreset::ALL {
                            ui.selectable_value(&mut nav, preset, nav_label(preset));
                        }
                    });
                self.nav = nav;
                ui.add_space(8.0);

                // Keymap preset.
                ui.label(tr("setup.keymap"));
                ComboBox::from_id_salt("setup-keymap")
                    .selected_text(if self.keymap_blender {
                        tr("prefs.keymap-preset-blender")
                    } else {
                        tr("prefs.keymap-preset-umber")
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.keymap_blender,
                            false,
                            tr("prefs.keymap-preset-umber"),
                        );
                        ui.selectable_value(
                            &mut self.keymap_blender,
                            true,
                            tr("prefs.keymap-preset-blender"),
                        );
                    });
                ui.add_space(8.0);

                // Tablet check: informational.
                ui.label(tr("setup.tablet"));
                let pen = stylus.hovering();
                let pressure = stylus.pressure_for_frame();
                if pen {
                    ui.label(
                        egui::RichText::new(tr("setup.tablet-live")).color(theme::palette::SUCCESS),
                    );
                    if let Some(p) = pressure {
                        ui.label(format!("{} {:.0}%", tr("setup.tablet-pressure"), p * 100.0));
                    }
                } else {
                    ui.weak(tr("setup.tablet-none"));
                }
                ui.add_space(12.0);

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(tr("setup.finish")).clicked() {
                        finished = Some(FinishedSetup {
                            theme: self.theme,
                            nav: self.nav,
                            keymap_blender: self.keymap_blender,
                        });
                        close = true;
                    }
                    if ui.button(tr("setup.skip")).clicked() {
                        // Skip: keep the defaults, but still write the
                        // prefs file so the window does not return.
                        finished = Some(FinishedSetup {
                            theme: SetupTheme::FollowOs,
                            nav: current_prefs.nav,
                            keymap_blender: false,
                        });
                        close = true;
                    }
                });
            });
        self.open = open && !close;
        finished
    }
}

/// The choices the user finished with.
#[derive(Debug, Clone, Copy)]
pub struct FinishedSetup {
    /// The theme to apply.
    pub theme: SetupTheme,
    /// The navigation preset to write into the preferences.
    pub nav: NavPreset,
    /// Whether the Blender-like keymap preset should be installed.
    pub keymap_blender: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_show_is_true_only_without_a_prefs_file() {
        // The contract reads `prefs_path()` — the FILE itself (it
        // returns the prefs path, not its directory), so this test
        // never writes anything: a host WITH a saved prefs file must
        // read no-first-run, a fresh host (no file) must read
        // first-run, and an unresolvable state dir reads as fresh.
        if let Some(file) = prefs_path() {
            if file.exists() {
                assert!(!should_show(), "a prefs file present = no first-run");
            } else {
                assert!(should_show(), "no prefs file = first-run");
            }
        } else {
            assert!(should_show(), "no resolvable state dir = first-run");
        }
    }

    #[test]
    fn theme_choices_map_to_egui_preferences() {
        assert_eq!(SetupTheme::FollowOs.egui_theme(), None);
        assert_eq!(
            SetupTheme::Light.egui_theme(),
            Some(egui::ThemePreference::Light)
        );
        assert_eq!(
            SetupTheme::Dark.egui_theme(),
            Some(egui::ThemePreference::Dark)
        );
    }

    #[test]
    fn the_setup_window_round_trips_open_state() {
        let mut setup = QuickSetup::default();
        assert!(!setup.is_open());
        setup.open();
        assert!(setup.is_open());
    }
}
