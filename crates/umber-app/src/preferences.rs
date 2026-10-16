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
//!
//! # Keymap (kill `g3-keymap-editor`, `docs/specs/gui-plan-2026-10-15.md`)
//!
//! The **Keymap** section edits [`Preferences::keymap_overrides`], an
//! [`actions::Overrides`] delta over the default `actions::BINDINGS`
//! table (the override model is documented in `actions.rs`). It persists
//! as one `keymap=Action:spec,spec;Action:spec` line (empty = none), and
//! `main.rs` installs it with `actions::set_overrides` at startup and on
//! every change, so menus, tooltips, status hints, the Help ▸ Keyboard
//! window and the shortcut poll all follow it.
//!
//! The page ([`keymap_section`]) is a 3-column grid (action, binding,
//! alternate) filtered by [`keymap_row_matches`] (action label, or key
//! tokens: "ctrl d" is a reverse lookup). Clicking a binding arms
//! capture: `main.rs`'s `raw_input_hook` hands the raw events to
//! [`KeymapEditor::intercept`] BEFORE egui and the shortcut poll see
//! them, so the press being captured never fires its current action.
//! [`apply_capture`] then assigns (refusing a binding another action
//! owns), cancels (Esc) or clears (Backspace: an overridden action goes
//! back to its defaults). A preset combo, "Copy changed bindings" (the
//! `keymap=` line of the delta) and a paste box ([`parse_pasted`])
//! complete the page.

use std::path::{Path, PathBuf};

use fluent::FluentValue;

use crate::actions::{self, Binding, Capture, KeymapPreset, Overrides};
use crate::i18n::{tr, tr_args};
use crate::navigation::NavPreset;
use crate::toolbar::ShellAction;

/// The persisted file's name in the state dir.
pub const PREFS_FILE: &str = "preferences";
/// UI scale bounds (egui zoom factor).
pub const SCALE_MIN: f32 = 0.75;
pub const SCALE_MAX: f32 = 2.0;

/// The user's preferences.
#[derive(Debug, Clone, PartialEq)]
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
    /// The user's keymap delta over the default bindings (the `keymap=`
    /// line; see the module docs).
    pub keymap_overrides: Overrides,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            tooltips: true,
            reduce_motion: false,
            key_hints: true,
            nav: NavPreset::default(),
            keymap_overrides: Overrides::default(),
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
            "ui_scale={}\ntooltips={}\nreduce_motion={}\nkey_hints={}\nnav={}\nkeymap={}\n",
            self.ui_scale,
            self.tooltips,
            self.reduce_motion,
            self.key_hints,
            nav_name(self.nav),
            self.keymap_overrides.serialize()
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
                "keymap" => p.keymap_overrides = Overrides::parse(v),
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

// ---------------------------------------------------------------------
// The Keymap page (kill g3-keymap-editor; see the module docs).
// ---------------------------------------------------------------------

/// The pieces of `binding` a key search matches: the modifier words
/// (`ctrl`, `shift`), the key's name and, for punctuation, its cap.
fn binding_tokens(binding: Binding) -> Vec<String> {
    let mut tokens: Vec<String> = actions::binding_spec(binding)
        .split('+')
        .map(str::to_owned)
        .collect();
    let cap = actions::key_cap_label(bind_bare(binding), false).to_lowercase();
    if !tokens.contains(&cap) {
        tokens.push(cap);
    }
    tokens
}

/// `binding`'s key with no modifiers (its key cap alone).
fn bind_bare(binding: Binding) -> Binding {
    Binding {
        key: binding.key,
        chord: actions::Chord::Bare,
    }
}

/// Whether a keymap row shows under `query`: an empty query shows all;
/// otherwise the row's `label` or `english` label contains it, OR every
/// whitespace / `+`-separated token of the query names a part of one of
/// `bindings` (the reverse lookup: "ctrl d", "shift+c", "["). `cmd` and
/// `command` mean `ctrl`.
pub fn keymap_row_matches(query: &str, label: &str, english: &str, bindings: &[Binding]) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() || label.to_lowercase().contains(&q) || english.to_lowercase().contains(&q) {
        return true;
    }
    let wanted: Vec<&str> = q
        .split(|c: char| c.is_whitespace() || c == '+')
        .filter(|t| !t.is_empty())
        .map(|t| match t {
            "cmd" | "command" | "⌘" => "ctrl",
            "⇧" => "shift",
            t => t,
        })
        .collect();
    !wanted.is_empty()
        && bindings.iter().any(|b| {
            let tokens = binding_tokens(*b);
            wanted.iter().all(|w| tokens.iter().any(|t| t == w))
        })
}

/// What applying one captured press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// Assigned, cleared or cancelled: capture ends.
    Done,
    /// Refused: the binding belongs to this other action. Capture ends.
    Conflict(ShellAction),
    /// A shape no chord expresses: still capturing.
    Waiting,
}

/// Applies `capture` to `action`'s binding `slot` (0 = primary, 1 =
/// alternate) in `overrides`:
///
/// - `Assign`: refused with [`CaptureOutcome::Conflict`] when another
///   action owns the binding in the effective table (defaults +
///   overrides); otherwise the slot takes it (an empty slot past the
///   list's end appends);
/// - `Clear`: an overridden action goes back to its defaults; a default
///   one loses that slot (an override saying so);
/// - `Cancel`: nothing changes.
pub fn apply_capture(
    overrides: &mut Overrides,
    action: ShellAction,
    slot: usize,
    capture: Capture,
) -> CaptureOutcome {
    match capture {
        Capture::Ignore => CaptureOutcome::Waiting,
        Capture::Cancel => CaptureOutcome::Done,
        Capture::Clear => {
            if overrides.get(action).is_some() {
                overrides.reset(action);
            } else {
                let mut list = overrides.bindings(action);
                if slot < list.len() {
                    list.remove(slot);
                    overrides.set(action, list);
                }
            }
            CaptureOutcome::Done
        }
        Capture::Assign(binding) => {
            let table = actions::table_for(overrides);
            if let Some(other) = actions::conflict_in(&table, action, binding) {
                return CaptureOutcome::Conflict(other);
            }
            let mut list = overrides.bindings(action);
            if slot < list.len() {
                list[slot] = binding;
            } else {
                list.push(binding);
            }
            overrides.set(action, list);
            CaptureOutcome::Done
        }
    }
}

/// The clipboard text of "Copy changed bindings": the persisted line of
/// the delta (`keymap=` + [`Overrides::serialize`]).
pub fn export_line(overrides: &Overrides) -> String {
    format!("keymap={}", overrides.serialize())
}

/// Why a pasted keymap was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteError {
    /// Text that names no valid override.
    Invalid,
    /// The result would bind one key to two actions.
    Conflict(Binding, ShellAction, ShellAction),
}

/// Parses a pasted `keymap=` line (the prefix is optional) into the
/// overrides it describes. An empty value is valid (= the defaults).
pub fn parse_pasted(text: &str) -> Result<Overrides, PasteError> {
    let text = text.trim();
    let value = text.strip_prefix("keymap=").unwrap_or(text).trim();
    let overrides = Overrides::parse(value);
    if overrides.is_empty() && !value.is_empty() {
        return Err(PasteError::Invalid);
    }
    if let Some((b, first, second)) = actions::first_conflict(&overrides) {
        return Err(PasteError::Conflict(b, first, second));
    }
    Ok(overrides)
}

/// The Keymap page's transient state (never persisted).
#[derive(Debug, Clone, Default)]
pub struct KeymapEditor {
    /// The page's own search box.
    pub query: String,
    /// The slot capturing a binding: `(action, slot)`.
    capturing: Option<(ShellAction, usize)>,
    /// A press captured by [`KeymapEditor::intercept`], applied when the
    /// page next draws.
    pending: Option<Capture>,
    /// The last refused assignment: `(row action, owning action)`.
    conflict: Option<(ShellAction, ShellAction)>,
    /// The paste box's text and the last paste's result.
    paste: String,
    paste_error: Option<PasteError>,
}

impl KeymapEditor {
    /// Stops capturing (the window closed).
    pub fn cancel_capture(&mut self) {
        self.capturing = None;
        self.pending = None;
    }

    /// Called from `raw_input_hook`, before egui and the shortcut poll
    /// see the frame's events. While capturing, takes every key, text and
    /// clipboard event out of `events` (so the press never fires its
    /// current action or types into a field) and keeps the first press a
    /// chord can express as the pending capture. Returns whether it is
    /// capturing.
    pub fn intercept(&mut self, events: &mut Vec<egui::Event>) -> bool {
        if self.capturing.is_none() {
            return false;
        }
        let mut pending = self.pending;
        events.retain(|event| match event {
            egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } => {
                if *pressed && !*repeat && pending.is_none() {
                    let capture = actions::capture_shape(*key, *modifiers);
                    if capture != Capture::Ignore {
                        pending = Some(capture);
                    }
                }
                false
            }
            // egui-winit turns Ctrl+C / X / V into these and never sends
            // the key: such a binding could not fire, so it is not
            // captured (and not passed on either).
            egui::Event::Text(_) | egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => {
                false
            }
            _ => true,
        });
        self.pending = pending;
        true
    }
}

/// The localized label of `action`: the palette / menu key when it has
/// one, else the keyboard reference's English words, else its name.
pub fn action_label(action: ShellAction) -> String {
    crate::palette::CATALOG
        .iter()
        .find(|e| e.action == action)
        .map_or_else(
            || match actions::reference_label(action) {
                "" => actions::action_name(action),
                text => text.to_owned(),
            },
            |e| tr(e.label),
        )
}

fn preset_label(preset: KeymapPreset) -> String {
    match preset {
        KeymapPreset::Umber => tr("prefs.keymap-preset-umber"),
        KeymapPreset::Blender => tr("prefs.keymap-preset-blender"),
        KeymapPreset::Custom => tr("prefs.keymap-preset-custom"),
    }
}

/// The Keymap section of the Preferences window. `window_query` is the
/// window's own search: the section shows when it is empty, matches the
/// section title, or matches a row. Edits land in `overrides`; the
/// caller saves and installs them when they changed.
pub fn keymap_section(
    ui: &mut egui::Ui,
    editor: &mut KeymapEditor,
    overrides: &mut Overrides,
    window_query: &str,
    is_mac: bool,
) {
    // A press the raw-input hook captured since the last draw
    // (applied first, so this frame's rows show the result).
    if let (Some(capture), Some((action, slot))) = (editor.pending.take(), editor.capturing) {
        match apply_capture(overrides, action, slot, capture) {
            CaptureOutcome::Waiting => {}
            CaptureOutcome::Done => {
                editor.capturing = None;
                editor.conflict = None;
            }
            CaptureOutcome::Conflict(other) => {
                editor.capturing = None;
                editor.conflict = Some((action, other));
            }
        }
    }

    let title = tr("prefs.section-keymap");
    let rows: Vec<(ShellAction, String, Vec<Binding>)> = actions::ALL_ACTIONS
        .iter()
        .map(|a| (*a, action_label(*a), overrides.bindings(*a)))
        .collect();
    let in_window = |a: ShellAction, label: &str, b: &[Binding]| {
        row_matches(window_query, &title)
            || keymap_row_matches(window_query, label, actions::reference_label(a), b)
    };
    if !rows.iter().any(|(a, l, b)| in_window(*a, l, b)) {
        return;
    }

    ui.strong(&title);
    ui.horizontal(|ui| {
        ui.label(tr("prefs.keymap-preset"));
        let current = KeymapPreset::of(overrides);
        egui::ComboBox::from_id_salt("prefs_keymap_preset")
            .selected_text(preset_label(current))
            .show_ui(ui, |ui| {
                for preset in [KeymapPreset::Umber, KeymapPreset::Blender] {
                    if ui
                        .selectable_label(current == preset, preset_label(preset))
                        .clicked()
                    {
                        if let Some(o) = preset.overrides() {
                            *overrides = o;
                            editor.cancel_capture();
                            editor.conflict = None;
                        }
                    }
                }
            });
    });
    ui.add(egui::TextEdit::singleline(&mut editor.query).hint_text(tr("prefs.keymap-search")));

    let warn = ui.visuals().warn_fg_color;
    egui::ScrollArea::vertical()
        .id_salt("prefs_keymap_scroll")
        .max_height(320.0)
        .show(ui, |ui| {
            egui::Grid::new("prefs_keymap_grid")
                .striped(true)
                .num_columns(3)
                .show(ui, |ui| {
                    ui.weak(tr("prefs.keymap-col-action"));
                    ui.weak(tr("prefs.keymap-col-binding"));
                    ui.weak(tr("prefs.keymap-col-alt"));
                    ui.end_row();
                    for (action, label, bindings) in &rows {
                        let english = actions::reference_label(*action);
                        if !in_window(*action, label, bindings)
                            || !keymap_row_matches(&editor.query, label, english, bindings)
                        {
                            continue;
                        }
                        ui.label(label);
                        for slot in 0..actions::MAX_BINDINGS {
                            let capturing = editor.capturing == Some((*action, slot));
                            let text = if capturing {
                                tr("prefs.keymap-press-keys")
                            } else {
                                bindings.get(slot).map_or_else(
                                    || tr("prefs.keymap-unbound"),
                                    |b| actions::key_cap_label(*b, is_mac),
                                )
                            };
                            let button = egui::Button::new(text)
                                .selected(capturing)
                                .min_size(egui::vec2(110.0, 0.0));
                            if ui
                                .add(button)
                                .on_hover_text(tr("prefs.keymap-button-tip"))
                                .clicked()
                            {
                                editor.capturing = Some((*action, slot));
                                editor.pending = None;
                                editor.conflict = None;
                            }
                        }
                        ui.end_row();
                        if let Some((row, other)) = editor.conflict {
                            if row == *action {
                                ui.label("");
                                let args = [("action", FluentValue::from(action_label(other)))];
                                ui.colored_label(warn, tr_args("prefs.keymap-conflict", args));
                                ui.label("");
                                ui.end_row();
                            }
                        }
                    }
                });
        });

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !overrides.is_empty(),
                egui::Button::new(tr("prefs.keymap-copy")),
            )
            .clicked()
        {
            ui.ctx().copy_text(export_line(overrides));
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut editor.paste)
                .hint_text(tr("prefs.keymap-paste-hint"))
                .desired_width(220.0),
        );
        if ui.button(tr("prefs.keymap-paste")).clicked() {
            match parse_pasted(&editor.paste) {
                Ok(o) => {
                    *overrides = o;
                    editor.paste.clear();
                    editor.paste_error = None;
                    editor.cancel_capture();
                    editor.conflict = None;
                }
                Err(e) => editor.paste_error = Some(e),
            }
        }
    });
    match editor.paste_error {
        Some(PasteError::Invalid) => {
            ui.colored_label(warn, tr("prefs.keymap-paste-invalid"));
        }
        Some(PasteError::Conflict(b, first, second)) => {
            let args = [
                ("key", FluentValue::from(actions::key_cap_label(b, is_mac))),
                ("first", FluentValue::from(action_label(first))),
                ("second", FluentValue::from(action_label(second))),
            ];
            ui.colored_label(warn, tr_args("prefs.keymap-paste-conflict", args));
        }
        None => {}
    }
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
            keymap_overrides: actions::blender_overrides(),
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

    // --- Keymap page (g3-keymap-editor) ---

    use crate::actions::Chord;
    use egui::{Key, Modifiers};

    fn b(key: Key, chord: Chord) -> Binding {
        Binding { key, chord }
    }

    #[test]
    fn keymap_line_round_trips_and_is_lenient() {
        let mut o = Overrides::default();
        o.set(
            ShellAction::Undo,
            vec![b(Key::U, Chord::Cmd), b(Key::F9, Chord::Bare)],
        );
        o.set(ShellAction::ToggleGrid, Vec::new());
        let p = Preferences {
            keymap_overrides: o.clone(),
            ..Preferences::default()
        };
        let text = p.serialize();
        assert!(
            text.contains("\nkeymap=Undo:ctrl+u,f9;ToggleGrid:\n"),
            "{text}"
        );
        assert_eq!(Preferences::parse(&text), p);
        assert_eq!(
            Preferences::parse(&text).serialize(),
            text,
            "exact round trip"
        );
        // Empty / absent line = no overrides; an old file still loads.
        assert!(Preferences::parse("keymap=\n").keymap_overrides.is_empty());
        assert!(Preferences::parse("ui_scale=1.5\n")
            .keymap_overrides
            .is_empty());
        assert!(Preferences::default().serialize().ends_with("keymap=\n"));
        // Junk in the line never stops the rest of the file.
        let junk = Preferences::parse("keymap=Nope:zz;Undo:ctrl+u\nui_scale=1.5\n");
        assert_eq!(junk.ui_scale, 1.5);
        assert_eq!(
            junk.keymap_overrides.get(ShellAction::Undo),
            Some(&[b(Key::U, Chord::Cmd)][..])
        );
    }

    #[test]
    fn capture_assigns_refuses_conflicts_and_clears() {
        let mut o = Overrides::default();
        // Ctrl+Z belongs to Undo: refused, nothing assigned.
        let grid = ShellAction::ToggleGrid;
        let out = apply_capture(&mut o, grid, 0, Capture::Assign(b(Key::Z, Chord::Cmd)));
        assert_eq!(out, CaptureOutcome::Conflict(ShellAction::Undo));
        assert!(o.is_empty());
        // A free key: the primary slot takes it.
        let f5 = b(Key::F5, Chord::Bare);
        assert_eq!(
            apply_capture(&mut o, grid, 0, Capture::Assign(f5)),
            CaptureOutcome::Done
        );
        assert_eq!(o.get(grid), Some(&[f5][..]));
        // The alternate slot appends.
        let f6 = b(Key::F6, Chord::Bare);
        apply_capture(&mut o, grid, 1, Capture::Assign(f6));
        assert_eq!(o.bindings(grid), vec![f5, f6]);
        // Now F5 conflicts for anyone else, and is fine for the owner.
        assert_eq!(
            apply_capture(&mut o, ShellAction::Undo, 1, Capture::Assign(f5)),
            CaptureOutcome::Conflict(grid)
        );
        assert_eq!(
            apply_capture(&mut o, grid, 0, Capture::Assign(f5)),
            CaptureOutcome::Done
        );
        // Esc and unexpressible shapes change nothing.
        let before = o.clone();
        assert_eq!(
            apply_capture(&mut o, grid, 0, Capture::Cancel),
            CaptureOutcome::Done
        );
        assert_eq!(
            apply_capture(&mut o, grid, 0, Capture::Ignore),
            CaptureOutcome::Waiting
        );
        assert_eq!(o, before);
        // Backspace on an override: back to the defaults (G).
        apply_capture(&mut o, grid, 0, Capture::Clear);
        assert_eq!(o.get(grid), None);
        assert_eq!(o.bindings(grid), vec![b(Key::G, Chord::Bare)]);
        // Backspace on a default: that slot is unbound (an override).
        apply_capture(&mut o, grid, 0, Capture::Clear);
        assert_eq!(o.get(grid), Some(&[] as &[Binding]));
        // The palette's alternate (F3) clears alone, keeping Ctrl+Shift+P.
        let palette = ShellAction::CommandPalette;
        apply_capture(&mut o, palette, 1, Capture::Clear);
        assert_eq!(o.bindings(palette), vec![b(Key::P, Chord::CmdShift)]);
    }

    #[test]
    fn intercept_takes_keys_only_while_capturing() {
        let key = |key: Key, modifiers: Modifiers, pressed: bool| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        let mut ed = KeymapEditor::default();
        let mut events = vec![key(Key::W, Modifiers::NONE, true)];
        assert!(!ed.intercept(&mut events));
        assert_eq!(events.len(), 1, "idle: nothing taken");
        ed.capturing = Some((ShellAction::ToggleGrid, 0));
        let mut events = vec![
            egui::Event::ModifiersChanged(Modifiers::ALT),
            key(Key::W, Modifiers::ALT, true), // no chord: skipped
            key(Key::K, Modifiers::SHIFT, true),
            egui::Event::Text("K".into()),
            key(Key::K, Modifiers::SHIFT, false),
            key(Key::J, Modifiers::NONE, true), // after the first: ignored
        ];
        assert!(ed.intercept(&mut events));
        assert_eq!(ed.pending, Some(Capture::Assign(b(Key::K, Chord::Shift))));
        // Only the non-key event survives: the press never reaches the poll.
        assert_eq!(events, vec![egui::Event::ModifiersChanged(Modifiers::ALT)]);
        ed.cancel_capture();
        assert_eq!((ed.capturing, ed.pending), (None, None));
    }

    #[test]
    fn keymap_search_matches_labels_and_keys() {
        let ctrl_s = [b(Key::S, Chord::Cmd)];
        assert!(keymap_row_matches("", "Save project", "Save", &ctrl_s));
        assert!(keymap_row_matches(
            "save",
            "Salvar projeto",
            "Save project",
            &ctrl_s
        ));
        assert!(keymap_row_matches("SALVAR", "Salvar projeto", "", &ctrl_s));
        // Reverse lookup by key: every token must name a part.
        assert!(keymap_row_matches("ctrl s", "Salvar", "", &ctrl_s));
        assert!(keymap_row_matches("Ctrl+S", "Salvar", "", &ctrl_s));
        assert!(keymap_row_matches("cmd s", "Salvar", "", &ctrl_s));
        assert!(!keymap_row_matches("ctrl d", "Salvar", "", &ctrl_s));
        assert!(!keymap_row_matches("shift s", "Salvar", "", &ctrl_s));
        // Punctuation by its cap; any of the row's bindings counts.
        let brush = [b(Key::OpenBracket, Chord::Bare)];
        assert!(keymap_row_matches("[", "Brush smaller", "", &brush));
        let palette = [b(Key::P, Chord::CmdShift), b(Key::F3, Chord::Bare)];
        assert!(keymap_row_matches("f3", "Paleta", "", &palette));
        assert!(keymap_row_matches("ctrl shift p", "Paleta", "", &palette));
        assert!(!keymap_row_matches("f3", "Unbound", "", &[]));
    }

    #[test]
    fn export_is_the_delta_and_paste_reverses_it() {
        assert_eq!(export_line(&Overrides::default()), "keymap=");
        let blender = actions::blender_overrides();
        let line = export_line(&blender);
        assert!(line.starts_with("keymap="));
        // Only the changed rows: three Blender swaps.
        assert_eq!(line.matches(';').count(), 2, "{line}");
        assert_eq!(parse_pasted(&line), Ok(blender.clone()));
        assert_eq!(
            parse_pasted(line.trim_start_matches("keymap=")),
            Ok(blender)
        );
        assert_eq!(parse_pasted("  "), Ok(Overrides::default()));
        assert_eq!(parse_pasted("hello world"), Err(PasteError::Invalid));
        // A paste that would double-bind W is refused whole.
        assert_eq!(
            parse_pasted("keymap=ToggleGrid:w"),
            Err(PasteError::Conflict(
                b(Key::W, Chord::Bare),
                ShellAction::ToggleWireframe,
                ShellAction::ToggleGrid
            ))
        );
    }

    #[test]
    fn every_keymap_key_is_in_both_catalogs() {
        let en = include_str!("../../../assets/i18n/en-US.ftl");
        let pt = include_str!("../../../assets/i18n/pt-BR.ftl");
        for key in [
            "section-keymap",
            "keymap-preset",
            "keymap-preset-umber",
            "keymap-preset-blender",
            "keymap-preset-custom",
            "keymap-search",
            "keymap-col-action",
            "keymap-col-binding",
            "keymap-col-alt",
            "keymap-unbound",
            "keymap-press-keys",
            "keymap-button-tip",
            "keymap-conflict",
            "keymap-copy",
            "keymap-paste",
            "keymap-paste-hint",
            "keymap-paste-invalid",
            "keymap-paste-conflict",
        ] {
            assert!(en.contains(&format!("    .{key} =")), "en-US {key}");
            assert!(pt.contains(&format!("    .{key} =")), "pt-BR {key}");
        }
    }
}
