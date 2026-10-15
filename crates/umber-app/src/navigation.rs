//! Viewport navigation presets (GUI plan G1 `g1-nav-presets`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: Painter C7, Blender
//! C1.1-C1.8 in `docs/research/gui-research-*-2026-10-15.md`).
//!
//! Umber's users come from Substance Painter, whose navigation is the
//! Maya / "industry" scheme: Alt+LMB orbit, Alt+MMB pan, Alt+RMB zoom
//! (Painter C7, Blender's Industry Compatible keymap C1.7). Umber's
//! original scheme (the [`NavPreset::Umber`] row below) orbited on a bare
//! LMB drag, which a Painter user reads as "my paint stroke turned the
//! camera". The default is therefore [`NavPreset::Painter`].
//!
//! | Gesture | Painter (default) | Umber (classic) | Blender |
//! |---|---|---|---|
//! | orbit | Alt+LMB | LMB | MMB |
//! | pan | Alt+MMB, Ctrl+Alt+LMB | Shift+LMB, MMB | Shift+MMB |
//! | zoom drag | Alt+RMB | — | Ctrl+MMB |
//! | wheel | zoom | zoom | zoom |
//!
//! **Paint stays Ctrl/Cmd+LMB under every preset** (Umber's paint chord,
//! `viewport.rs::ui`). [`NavPreset::camera_op`] never claims it, and a
//! test pins that: navigation must not steal a stroke.
//!
//! The preset is chosen with `UMBER_NAV=painter|umber|blender` until the
//! G3 preferences window exists. An unknown value falls back to the
//! default.

use egui::{Modifiers, PointerButton};

/// A navigation scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavPreset {
    /// Substance Painter / Maya: Alt + mouse. The default.
    #[default]
    Painter,
    /// Umber's original scheme: bare LMB orbits.
    Umber,
    /// Blender's default: MMB orbits.
    Blender,
}

/// What a drag does to the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraOp {
    /// Orbit around the target.
    Orbit,
    /// Pan the target in the view plane.
    Pan,
    /// Dolly toward / away from the target.
    Zoom,
}

impl NavPreset {
    /// Every preset, in menu order.
    pub const ALL: [NavPreset; 3] = [NavPreset::Painter, NavPreset::Umber, NavPreset::Blender];

    /// The preset named `name` (`painter` / `umber` / `blender`,
    /// case-insensitive), or `None`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "painter" | "maya" | "industry" => Some(NavPreset::Painter),
            "umber" | "classic" => Some(NavPreset::Umber),
            "blender" => Some(NavPreset::Blender),
            _ => None,
        }
    }

    /// The preset from `UMBER_NAV`, else the default.
    pub fn from_env() -> Self {
        std::env::var("UMBER_NAV")
            .ok()
            .and_then(|v| Self::from_name(&v))
            .unwrap_or_default()
    }

    /// The camera operation a drag with `button` under `mods` performs,
    /// or `None` (the drag is not navigation: a paint stroke, a tool
    /// click, or unbound).
    pub fn camera_op(self, button: PointerButton, mods: Modifiers) -> Option<CameraOp> {
        let cmd = mods.ctrl || mods.command;
        match self {
            NavPreset::Painter => {
                if !mods.alt {
                    return None;
                }
                match button {
                    // Ctrl+Alt+LMB is Painter's alternate pan.
                    PointerButton::Primary if cmd => Some(CameraOp::Pan),
                    PointerButton::Primary => Some(CameraOp::Orbit),
                    PointerButton::Middle => Some(CameraOp::Pan),
                    PointerButton::Secondary => Some(CameraOp::Zoom),
                    _ => None,
                }
            }
            NavPreset::Umber => match button {
                PointerButton::Primary if cmd => None, // the paint chord
                PointerButton::Primary if mods.shift => Some(CameraOp::Pan),
                PointerButton::Primary => Some(CameraOp::Orbit),
                PointerButton::Middle => Some(CameraOp::Pan),
                _ => None,
            },
            NavPreset::Blender => match button {
                PointerButton::Middle if mods.shift => Some(CameraOp::Pan),
                PointerButton::Middle if cmd => Some(CameraOp::Zoom),
                PointerButton::Middle => Some(CameraOp::Orbit),
                _ => None,
            },
        }
    }

    /// The status-bar hints for navigating, as `(input, action)` i18n key
    /// pairs: the orbit, pan and zoom chords of this preset.
    pub fn hints(self) -> &'static [(&'static str, &'static str)] {
        match self {
            NavPreset::Painter => &[
                ("hint.alt-lmb", "hint.orbit"),
                ("hint.alt-mmb", "hint.pan"),
                ("hint.alt-rmb", "hint.zoom"),
            ],
            NavPreset::Umber => &[
                ("hint.lmb-drag", "hint.orbit"),
                ("hint.shift-lmb", "hint.pan"),
                ("hint.wheel", "hint.zoom"),
            ],
            NavPreset::Blender => &[
                ("hint.mmb", "hint.orbit"),
                ("hint.shift-mmb", "hint.pan"),
                ("hint.wheel", "hint.zoom"),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(alt: bool, ctrl: bool, shift: bool) -> Modifiers {
        Modifiers {
            alt,
            ctrl,
            command: ctrl,
            shift,
            ..Modifiers::NONE
        }
    }

    const BUTTONS: [PointerButton; 3] = [
        PointerButton::Primary,
        PointerButton::Middle,
        PointerButton::Secondary,
    ];

    #[test]
    fn painter_is_the_default_and_alt_drives_it() {
        let p = NavPreset::default();
        assert_eq!(p, NavPreset::Painter);
        let alt = m(true, false, false);
        assert_eq!(
            p.camera_op(PointerButton::Primary, alt),
            Some(CameraOp::Orbit)
        );
        assert_eq!(p.camera_op(PointerButton::Middle, alt), Some(CameraOp::Pan));
        assert_eq!(
            p.camera_op(PointerButton::Secondary, alt),
            Some(CameraOp::Zoom)
        );
        assert_eq!(
            p.camera_op(PointerButton::Primary, m(true, true, false)),
            Some(CameraOp::Pan)
        );
        // Without Alt, Painter's mouse buttons never move the camera.
        for b in BUTTONS {
            assert_eq!(p.camera_op(b, Modifiers::NONE), None, "{b:?}");
        }
    }

    #[test]
    fn no_preset_steals_the_paint_chord() {
        // Ctrl/Cmd+LMB WITHOUT Alt is the paint stroke under every preset.
        for preset in NavPreset::ALL {
            for shift in [false, true] {
                assert_eq!(
                    preset.camera_op(PointerButton::Primary, m(false, true, shift)),
                    None,
                    "{preset:?} shift={shift}"
                );
            }
        }
    }

    #[test]
    fn umber_preset_is_the_classic_scheme() {
        let p = NavPreset::Umber;
        assert_eq!(
            p.camera_op(PointerButton::Primary, Modifiers::NONE),
            Some(CameraOp::Orbit)
        );
        assert_eq!(
            p.camera_op(PointerButton::Primary, m(false, false, true)),
            Some(CameraOp::Pan)
        );
        assert_eq!(
            p.camera_op(PointerButton::Middle, Modifiers::NONE),
            Some(CameraOp::Pan)
        );
        assert_eq!(p.camera_op(PointerButton::Secondary, Modifiers::NONE), None);
    }

    #[test]
    fn blender_preset_orbits_on_mmb() {
        let p = NavPreset::Blender;
        assert_eq!(
            p.camera_op(PointerButton::Middle, Modifiers::NONE),
            Some(CameraOp::Orbit)
        );
        assert_eq!(
            p.camera_op(PointerButton::Middle, m(false, false, true)),
            Some(CameraOp::Pan)
        );
        assert_eq!(
            p.camera_op(PointerButton::Middle, m(false, true, false)),
            Some(CameraOp::Zoom)
        );
        assert_eq!(p.camera_op(PointerButton::Primary, Modifiers::NONE), None);
    }

    #[test]
    fn names_parse_case_insensitively() {
        assert_eq!(NavPreset::from_name(" Painter "), Some(NavPreset::Painter));
        assert_eq!(NavPreset::from_name("MAYA"), Some(NavPreset::Painter));
        assert_eq!(NavPreset::from_name("classic"), Some(NavPreset::Umber));
        assert_eq!(NavPreset::from_name("blender"), Some(NavPreset::Blender));
        assert_eq!(NavPreset::from_name("houdini"), None);
    }

    #[test]
    fn every_nav_hint_key_is_in_both_catalogs() {
        let en = include_str!("../../../assets/i18n/en-US.ftl");
        let pt = include_str!("../../../assets/i18n/pt-BR.ftl");
        for preset in NavPreset::ALL {
            for (input, action) in preset.hints() {
                for k in [input, action] {
                    let attr = format!(".{} =", k.trim_start_matches("hint."));
                    assert!(en.contains(&attr), "en-US missing {k}");
                    assert!(pt.contains(&attr), "pt-BR missing {k}");
                }
            }
        }
    }
}
