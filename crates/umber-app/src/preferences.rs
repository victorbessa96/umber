//! The Preferences window (GUI plan G3 `g3-preferences-window`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: Painter F2 (general
//! preferences: keyboard helper, camera speeds and inverts), Blender
//! F9-F17 (UI scale separate from DPI, tooltips on/off, Reduce Motion,
//! navigation preset), photocraft #10 (each setting syncs onto the egui
//! style ONLY when it differs, since a theme change rebuilds the style; a
//! search box filters the rows)).
//!
//! Edit ▸ Preferences (Ctrl+,) opens a window with searchable sections:
//!
//! - **Interface:** UI scale, tooltips on/off, Reduce Motion, the
//!   status-bar key hints;
//! - **Navigation:** the preset (Painter / Umber / Blender), replacing the
//!   `UMBER_NAV` env var as the normal way to choose; the env still wins
//!   if set, as an override for scripted launches.
//!
//! Persistence is a plain `key=value` file, `preferences`, in the
//! per-user state dir beside `locale` and the GPU startup files (no serde
//! format dependency for a handful of scalars, the gpu_startup precedent).
//! Unknown keys are ignored and bad values fall back to defaults, so an
//! old or hand-edited file never stops the app from starting.

use std::path::{Path, PathBuf};

use crate::navigation::NavPreset;

/// The persisted file's name in the state dir.
pub const PREFS_FILE: &str = "preferences";
/// UI scale bounds (egui zoom factor).
pub const SCALE_MIN: f32 = 0.75;
pub const SCALE_MAX: f32 = 2.0;

/// The user's preferences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preferences {
    /// UI scale (egui zoom factor; 1.0 = the OS's own scale).
    pub ui_scale: f32,
    /// Show tooltips.
    pub tooltips: bool,
    /// Reduce Motion: no animations (egui `animation_time` = 0).
    pub reduce_motion: bool,
    /// The status bar's mouse / key hints.
    pub key_hints: bool,
    /// The viewport navigation preset.
    pub nav: NavPreset,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            tooltips: true,
            reduce_motion: false,
            key_hints: true,
            nav: NavPreset::default(),
        }
    }
}

fn nav_name(n: NavPreset) -> &'static str {
    match n {
        NavPreset::Painter => "painter",
        NavPreset::Umber => "umber",
        NavPreset::Blender => "blender",
    }
}

impl Preferences {
    /// The `key=value` text (stable key order).
    pub fn serialize(&self) -> String {
        format!(
            "ui_scale={}\ntooltips={}\nreduce_motion={}\nkey_hints={}\nnav={}\n",
            self.ui_scale,
            self.tooltips,
            self.reduce_motion,
            self.key_hints,
            nav_name(self.nav)
        )
    }

    /// Parses `text` leniently: unknown keys and junk lines are ignored,
    /// a bad value keeps that field's default, the scale is clamped.
    pub fn parse(text: &str) -> Self {
        let mut p = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            let bool_of = |v: &str| match v {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            };
            match k.trim() {
                "ui_scale" => {
                    if let Ok(s) = v.parse::<f32>() {
                        if s.is_finite() {
                            p.ui_scale = s.clamp(SCALE_MIN, SCALE_MAX);
                        }
                    }
                }
                "tooltips" => p.tooltips = bool_of(v).unwrap_or(p.tooltips),
                "reduce_motion" => p.reduce_motion = bool_of(v).unwrap_or(p.reduce_motion),
                "key_hints" => p.key_hints = bool_of(v).unwrap_or(p.key_hints),
                "nav" => p.nav = NavPreset::from_name(v).unwrap_or(p.nav),
                _ => {}
            }
        }
        p
    }

    /// Loads from `path`; defaults when the file is absent or unreadable.
    pub fn load(path: Option<&Path>) -> Self {
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// Writes to `path` (creating the dir). Errors log; the session keeps
    /// the values either way.
    pub fn save(&self, path: Option<&Path>) {
        let Some(path) = path else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, self.serialize()) {
            log::warn!("preferences: could not save {}: {e}", path.display());
        }
    }

    /// The navigation preset in force: `UMBER_NAV` (a scripted-launch
    /// override) wins over the saved choice.
    pub fn effective_nav(&self, env: Option<&str>) -> NavPreset {
        env.and_then(NavPreset::from_name).unwrap_or(self.nav)
    }
}

/// The preferences file path (the state dir `gpu_startup` uses).
pub fn prefs_path() -> Option<PathBuf> {
    umber_brush::preset_library::user_preset_dir()
        .and_then(|p| p.parent().map(|d| d.join(PREFS_FILE)))
}

/// What applying a preference set to egui would change: `None` per field
/// when egui already holds that value, so the per-frame sync writes the
/// style only when something differs (photocraft #10: a style write
/// rebuilds every widget's visuals).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleDelta {
    pub zoom: Option<f32>,
    pub tooltip_delay: Option<f32>,
    pub animation_time: Option<f32>,
}

impl StyleDelta {
    pub fn is_empty(&self) -> bool {
        self.zoom.is_none() && self.tooltip_delay.is_none() && self.animation_time.is_none()
    }
}

/// The delta from the current egui values to `prefs`. `base_tooltip` /
/// `base_animation` are the theme's own values (restored when a pref is
/// turned back on).
pub fn style_delta(
    prefs: &Preferences,
    zoom: f32,
    tooltip_delay: f32,
    animation_time: f32,
    base_tooltip: f32,
    base_animation: f32,
) -> StyleDelta {
    let want_tooltip = if prefs.tooltips {
        base_tooltip
    } else {
        f32::INFINITY
    };
    let want_anim = if prefs.reduce_motion {
        0.0
    } else {
        base_animation
    };
    let differs = |a: f32, b: f32| !(a == b || (a - b).abs() < 1e-6);
    StyleDelta {
        zoom: differs(zoom, prefs.ui_scale).then_some(prefs.ui_scale),
        tooltip_delay: differs(tooltip_delay, want_tooltip).then_some(want_tooltip),
        animation_time: differs(animation_time, want_anim).then_some(want_anim),
    }
}

/// Whether a row labelled `label` shows under the search `query`
/// (case-insensitive substring; an empty query shows everything).
pub fn row_matches(query: &str, label: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || label.to_lowercase().contains(&q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let p = Preferences {
            ui_scale: 1.25,
            tooltips: false,
            reduce_motion: true,
            key_hints: false,
            nav: NavPreset::Blender,
        };
        assert_eq!(Preferences::parse(&p.serialize()), p);
        assert_eq!(Preferences::parse(""), Preferences::default());
    }

    #[test]
    fn parse_is_lenient_and_clamps() {
        let p = Preferences::parse(
            "junk\nfuture_key=1\nui_scale=9\ntooltips=maybe\nnav=houdini\nreduce_motion=yes\n",
        );
        assert_eq!(p.ui_scale, SCALE_MAX, "clamped");
        assert!(p.tooltips, "bad value keeps the default");
        assert_eq!(
            p.nav,
            NavPreset::default(),
            "unknown preset keeps the default"
        );
        assert!(p.reduce_motion);
        assert_eq!(Preferences::parse("ui_scale=NaN").ui_scale, 1.0);
        assert_eq!(Preferences::parse("ui_scale=0.1").ui_scale, SCALE_MIN);
    }

    #[test]
    fn env_overrides_the_saved_nav() {
        let p = Preferences {
            nav: NavPreset::Umber,
            ..Preferences::default()
        };
        assert_eq!(p.effective_nav(None), NavPreset::Umber);
        assert_eq!(p.effective_nav(Some("blender")), NavPreset::Blender);
        assert_eq!(p.effective_nav(Some("bogus")), NavPreset::Umber);
    }

    #[test]
    fn style_delta_is_empty_when_egui_already_matches() {
        let p = Preferences::default();
        let d = style_delta(&p, 1.0, 0.35, 0.12, 0.35, 0.12);
        assert!(d.is_empty(), "{d:?}");
        // Tooltips off -> infinite delay; Reduce Motion -> 0; scale moves.
        let off = Preferences {
            tooltips: false,
            reduce_motion: true,
            ui_scale: 1.5,
            ..p
        };
        let d = style_delta(&off, 1.0, 0.35, 0.12, 0.35, 0.12);
        assert_eq!(d.zoom, Some(1.5));
        assert_eq!(d.tooltip_delay, Some(f32::INFINITY));
        assert_eq!(d.animation_time, Some(0.0));
        // Once applied, the next frame's delta is empty again.
        let d2 = style_delta(&off, 1.5, f32::INFINITY, 0.0, 0.35, 0.12);
        assert!(d2.is_empty(), "{d2:?}");
    }

    #[test]
    fn search_filters_rows_case_insensitively() {
        assert!(row_matches("", "UI scale"));
        assert!(row_matches("scale", "UI scale"));
        assert!(row_matches("SCALE", "ui scale"));
        assert!(!row_matches("motion", "UI scale"));
    }

    #[test]
    fn save_then_load_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!("umber-prefs-test-{}", std::process::id()));
        let path = dir.join(PREFS_FILE);
        let p = Preferences {
            ui_scale: 1.5,
            nav: NavPreset::Umber,
            ..Preferences::default()
        };
        p.save(Some(&path));
        assert_eq!(Preferences::load(Some(&path)), p);
        assert_eq!(
            Preferences::load(Some(&dir.join("missing"))),
            Preferences::default()
        );
        assert_eq!(Preferences::load(None), Preferences::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
