//! The Display panel: edits the viewer chain (requirements.md §9 —
//! display transform) — view transform, exposure, display gamma — as
//! [`umber_color::DisplaySettings`] on `AppState::display`, persisted in
//! the `.umber` project (`ProjectSettings::display`).
//!
//! The chain reaches the pixels through the GPU display LUT
//! (`LANDING_NOTES_DISPLAY_PANEL.md`): [`umber_color::build_display_lut`]
//! tabulates it into 256 RGBA8 entries, the app uploads them into its
//! `umber_gpu::DisplayLut`, and both GPU views sample that LUT as their
//! last step — the 3D viewport's mesh pass and the UV view's paint-
//! target display (`umber_gpu::texture_display`). What the panel does:
//!
//! - the settings rows write the data (it saves/loads with the project)
//!   and, on any change, mark the LUT dirty
//!   ([`DisplayLutState::mark_display_lut_dirty`]); the frame loop
//!   rebuilds and uploads it ([`DisplayLutState::take_rebuild`]);
//! - the preview strip runs the pure [`apply_display_chain`] over a
//!   synthetic linear ramp ([`PREVIEW_RAMP`]) and paints the results as
//!   egui swatches — the transform's effect on a known ramp;
//! - the status line names the active chain and the live consumers.
//!
//! v1 limits: the ground grid and wireframe overlays are not
//! transformed (only the two named consumers), and the LUT's input
//! clamps to 0..1. The panel also carries the UI-language row (the
//! locale picker of `docs/specs/i18n-design.md`) — the app's one
//! settings surface today. Real OCIO configs stay gated behind
//! `umber-color`'s `ocio` feature.
//!
//! [`hdr_row`] adds the HDR output row (requirements.md §5, P1 row 85):
//! a single checkbox behind which the whole feature sits in v1 — see
//! `umber_gpu::hdr`'s module doc for the honest eframe boundary this
//! panel reports (disabled + a visible reason, not a hardware verdict)
//! and for why the request persists as a user preference
//! ([`hdr_pref_path`]), the same pattern as the locale preference
//! (`crate::i18n::locale_pref_path`), rather than in `.umber` project.
//!
//! GUI Phase 1 chrome, kill 5 (docs/specs/gui-audit-and-roadmap.md
//! Phase 1, "Display panel"): the panel's sections render as
//! [`umber_gui::ParamGroup`] collapsibles and the exposure/gamma sliders
//! are [`umber_gui::ValueField`]s (drag-scrub, wheel-step, tabular
//! numerals) over the same ranges and backing state as the stock sliders
//! they replaced. Chrome only: no state struct, default, range, LUT math,
//! preview ramp, HDR persistence, or locale path changed, and every test
//! below is untouched.
//!
//! Deliberately NOT migrated (v1 boundaries of this pass):
//! - Group titles: the transform group is the one new content key
//!   (`display.transform`, both catalogs); the preview group reuses
//!   `display.preview` verbatim as its header (trailing colon and all —
//!   renaming it would churn the pt-BR rendering for no gain); language
//!   reuses `settings.language`; HDR adds `display-hdr.title` (`HDR`,
//!   identical in both locales — an untranslatable acronym). The old
//!   `ComboBox::from_label` texts moved into the row labels / headers,
//!   so no visible string was lost.
//! - The preview strip stays custom-painted: the swatch colors are LUT
//!   data, not chrome, so scope item (3) leaves them alone.
//! - No other hand-rolled colors exist in this file (no
//!   `RichText::color`, no custom strokes), so scope item (3) otherwise
//!   changes nothing; the groups themselves paint from `umber-gui`'s
//!   theme tokens internally.
//! - No `umber_style`/fonts call at panel level (the `brush_panel.rs`
//!   rationale): `main.rs` applies no Umber style today, and a
//!   panel-local style would diverge from the still-unmigrated panels.
//!   [`umber_gui::ValueField`] only needs `FontId::monospace`, which
//!   works on stock fonts.
//! - Exposure wheel-steps 0.1 EV per notch (`.step(0.1)` — its range is
//!   10 EV wide); gamma keeps the field default step (0.01, matching the
//!   status line's two decimals). Both keep two decimals and no unit
//!   suffix — the row labels already carry "(EV)", and gamma is
//!   unitless. Stored values keep full precision; snapping only applies
//!   while scrubbing/stepping, same as before.
//! - [`umber_gui::ParamGroup`]'s header count covers top-level
//!   [`umber_gui::widgets::ParamGroupUi::row`] calls only: the transform
//!   group counts its view/exposure/gamma rows (the reset button draws
//!   on `group.ui` verbatim, uncounted); the preview, language, and HDR
//!   groups draw entirely on `group.ui` (custom canvas / shared-combo /
//!   checkbox bodies), so their headers show no count — the same wart
//!   class `brush_panel.rs` documents.

use crate::i18n;
use egui::{Color32, Ui};
use fluent::FluentValue;
use std::path::{Path, PathBuf};
#[cfg(not(feature = "ocio"))]
use umber_color::apply_display_chain;
use umber_color::{build_display_lut, DisplaySettings, DisplayTransform, DISPLAY_LUT_BYTES};
use umber_gui::{ParamGroup, ValueField};

/// The GPU display LUT's CPU-side sync state: whether the table on the
/// device matches the current settings. The panel's write path marks it
/// dirty; the frame loop calls [`Self::take_rebuild`] once per frame and
/// uploads what it returns. Starts dirty, so the first frame builds the
/// LUT from whatever settings the app holds.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayLutState {
    dirty: bool,
    /// The settings the device LUT was last built from.
    built: Option<DisplaySettings>,
}

impl Default for DisplayLutState {
    fn default() -> Self {
        Self {
            dirty: true,
            built: None,
        }
    }
}

impl DisplayLutState {
    /// The settings changed: the next [`Self::take_rebuild`] rebuilds.
    pub fn mark_display_lut_dirty(&mut self) {
        self.dirty = true;
    }

    /// The table to upload, if the device LUT is stale: dirty, or built
    /// from other settings than `settings` (a safety net — a write path
    /// that forgot to mark still converges in one frame). Clears the
    /// dirty flag and records `settings` as built.
    #[cfg(not(feature = "ocio"))]
    pub fn take_rebuild(&mut self, settings: &DisplaySettings) -> Option<[u8; DISPLAY_LUT_BYTES]> {
        if !self.dirty && self.built.as_ref() == Some(settings) {
            return None;
        }
        self.dirty = false;
        self.built = Some(settings.clone());
        Some(build_display_lut(settings))
    }

    /// The ocio-feature rebuild: the adopted processor threads through
    /// (the same staleness rule — the settings comparison includes the
    /// OCIO selection, so a selection change rebuilds even when the
    /// cached processor was already live).
    #[cfg(feature = "ocio")]
    pub fn take_rebuild(
        &mut self,
        settings: &DisplaySettings,
        ocio_processor: Option<&umber_color::ocio_rs::CPUProcessor>,
    ) -> Option<[u8; DISPLAY_LUT_BYTES]> {
        if !self.dirty && self.built.as_ref() == Some(settings) {
            return None;
        }
        self.dirty = false;
        self.built = Some(settings.clone());
        Some(build_display_lut(settings, ocio_processor))
    }

    /// Whether the device LUT holds `settings` (what the status line
    /// reports as live).
    pub fn is_live(&self, settings: &DisplaySettings) -> bool {
        !self.dirty && self.built.as_ref() == Some(settings)
    }
}

/// Number of preview swatches.
pub const PREVIEW_STOPS: usize = 8;

/// The preview's synthetic linear ramp: one swatch per EV stop,
/// `0.2 · 2^(k−7)` for k = 0..8 — seven stops of shadow up to 0.2
/// (just above 18% gray). Topping out at 0.2 keeps even +2 EV (×4 =
/// 0.8) below the sRGB/Rec.709 clamp at every swatch, so the strip
/// stays readable while the exposure slider pushes it.
pub const PREVIEW_RAMP: [f32; PREVIEW_STOPS] = [
    0.2 / 128.0,
    0.2 / 64.0,
    0.2 / 32.0,
    0.2 / 16.0,
    0.2 / 8.0,
    0.2 / 4.0,
    0.2 / 2.0,
    0.2,
];

/// Swatch strip height (points).
const SWATCH_HEIGHT: f32 = 28.0;

/// The OCIO section's session state (§9 "in panel", the `ocio`
/// feature): the held config + the cached scene-linear → display
/// processor for the CURRENT selection. Rebuilt when the selection
/// changes; never serialized (the selection itself lives in
/// `DisplaySettings::ocio` and saves with the project — this is the
/// materialized handle for it, rebuilt on load).
#[cfg(feature = "ocio")]
#[derive(Default)]
pub struct OcioSectionState {
    /// The live built-in config handle (`None` until a selection
    /// materializes one — and permanently in stub mode).
    config: Option<umber_color::ocio::OcioBuiltinConfig>,
    /// What `processor` was built for (config name + display space);
    /// `None` = no processor yet.
    built_for: Option<(String, String)>,
    /// The cached CPU processor for the current selection.
    processor: Option<umber_color::ocio_rs::CPUProcessor>,
    /// The last selection error, shown under the section (a failed
    /// config creation is a visible status, not a silent fallback).
    pub error: Option<String>,
}

#[cfg(feature = "ocio")]
impl OcioSectionState {
    /// The processor for the selection in `settings.ocio`, building it
    /// when the selection changed since the last call. `None` when the
    /// selection is `None` (the fixed path) or the config cannot
    /// materialize (stub mode / a rejected name — `error` says why).
    pub fn processor_for(
        &mut self,
        settings: &DisplaySettings,
    ) -> Option<&umber_color::ocio_rs::CPUProcessor> {
        let selection = settings.ocio.as_ref()?;
        let wanted = (selection.config.clone(), selection.display_space.clone());
        if self.built_for.as_ref() == Some(&wanted) {
            return self.processor.as_ref();
        }
        self.error = None;
        if self.config.is_none() {
            match umber_color::ocio::OcioBuiltinConfig::create(&selection.config) {
                Ok(config) => self.config = Some(config),
                Err(err) => {
                    self.error = Some(err.to_string());
                    self.processor = None;
                    self.built_for = None;
                    return None;
                }
            }
        }
        let config = self.config.as_ref().expect("just created");
        match config.display_processor(&selection.display_space) {
            Ok(cpu) => {
                self.processor = Some(cpu);
                self.built_for = Some(wanted);
                self.processor.as_ref()
            }
            Err(err) => {
                self.error = Some(err.to_string());
                self.processor = None;
                self.built_for = None;
                None
            }
        }
    }

    /// Whether OCIO is available AT ALL in this build (stub mode: the
    /// registry enumerates nothing — the section's disabled arm).
    fn available() -> bool {
        !umber_color::ocio::builtin_configs().is_empty()
    }
}

/// The OCIO section (the `ocio` feature): the config + display-space
/// pickers that adopt an ACES 2.0 built-in into the viewer chain.
/// Any change marks the LUT dirty (the chain's view stage is now the
/// config's processor — the rebuild path threads it).
#[cfg(feature = "ocio")]
fn ocio_section(
    ui: &mut Ui,
    settings: &mut DisplaySettings,
    ocio_state: &mut OcioSectionState,
    lut: &mut DisplayLutState,
) -> bool {
    let mut changed = false;
    let title = i18n::tr("display-ocio.title");
    ParamGroup::new("display-ocio", title.as_str()).show(ui, |group| {
        if !OcioSectionState::available() {
            // Stub mode (the honest arm): every control disabled, the
            // visible reason readable without hunting for a hover.
            group.ui.add_enabled(
                false,
                egui::Label::new(i18n::tr("display-ocio.unavailable")),
            );
            return;
        }
        // The config picker: the registry's pairs (enum name is the
        // selection's stored value; the UI name is what shows).
        let configs = umber_color::ocio::builtin_configs();
        let current_config = settings
            .ocio
            .as_ref()
            .map(|s| s.config.clone())
            .unwrap_or_default();
        let mut picked_config = current_config.clone();
        group.row(&i18n::tr("display-ocio.config"), |ui| {
            egui::ComboBox::from_id_salt("ocio_config_combo")
                .selected_text(
                    configs
                        .iter()
                        .find(|(n, _)| *n == current_config)
                        .map(|(_, ui)| ui.clone())
                        .unwrap_or_else(|| i18n::tr("display-ocio.none")),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut picked_config,
                        String::new(),
                        i18n::tr("display-ocio.none"),
                    );
                    for (name, ui_name) in &configs {
                        ui.selectable_value(&mut picked_config, name.clone(), ui_name.clone());
                    }
                });
        });
        // The display-space picker: enabled once a config is picked;
        // the spaces come from the HELD config (already created for
        // the current selection — a first pick lists after adoption).
        let spaces = ocio_state
            .config
            .as_ref()
            .map(|c| c.display_spaces())
            .unwrap_or_default();
        let current_space = settings
            .ocio
            .as_ref()
            .map(|s| s.display_space.clone())
            .unwrap_or_default();
        let mut picked_space = current_space.clone();
        group.row(&i18n::tr("display-ocio.space"), |ui| {
            egui::ComboBox::from_id_salt("ocio_space_combo")
                .selected_text(if current_space.is_empty() {
                    i18n::tr("display-ocio.space-pick")
                } else {
                    current_space.clone()
                })
                .show_ui(ui, |ui| {
                    for space in &spaces {
                        ui.selectable_value(&mut picked_space, space.clone(), space.clone());
                    }
                });
        });
        if picked_config != current_config {
            // A config switch: adopt it with the default space (the
            // first sRGB display space the config names — probed CG:
            // "sRGB - Display" at index 0; a config without one keeps
            // the space empty and the chain stays fixed until picked).
            if picked_config.is_empty() {
                settings.ocio = None;
            } else {
                let space = spaces
                    .iter()
                    .find(|s| s.contains("sRGB"))
                    .cloned()
                    .unwrap_or_default();
                settings.ocio = Some(umber_color::OcioDisplaySelection {
                    config: picked_config.clone(),
                    display_space: space,
                });
            }
            ocio_state.config = None;
            ocio_state.built_for = None;
            ocio_state.processor = None;
            ocio_state.error = None;
            changed = true;
        } else if picked_space != current_space {
            // A space switch on the same config.
            if let Some(selection) = settings.ocio.as_mut() {
                selection.display_space = picked_space.clone();
                changed = true;
            }
        }
        if let Some(err) = ocio_state.error.clone() {
            group
                .ui
                .label(egui::RichText::new(err).color(egui::Color32::from_rgb(200, 90, 90)));
        }
    });
    if changed {
        lut.mark_display_lut_dirty();
    }
    changed
}

/// The chain applied to each gray ramp stop (display-encoded 0..=1,
/// or raw values under the Raw view). With the ocio feature and an
/// adopted selection, the OCIO processor is the view stage (threaded
/// from the section state).
#[cfg(not(feature = "ocio"))]
fn preview_values(s: &DisplaySettings) -> [[f32; 3]; PREVIEW_STOPS] {
    PREVIEW_RAMP.map(|l| apply_display_chain([l, l, l], s))
}

#[cfg(feature = "ocio")]
fn preview_values(
    s: &DisplaySettings,
    ocio_processor: Option<&umber_color::ocio_rs::CPUProcessor>,
) -> [[f32; 3]; PREVIEW_STOPS] {
    PREVIEW_RAMP.map(|l| umber_color::apply_display_chain_with([l, l, l], s, ocio_processor))
}

/// Display-encoded 0..=1 → egui color. `Color32` is already sRGB-
/// encoded display bytes, so the chain output is quantized directly —
/// never through `egui::Rgba` (linear), which would encode twice.
fn to_color32(v: [f32; 3]) -> Color32 {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(byte(v[0]), byte(v[1]), byte(v[2]))
}

/// The preview strip's swatch colors for `s` (test-side helper; the
/// panel path threads the OCIO processor through `preview_values`
/// directly when the feature is on).
/// The status line: the active chain, and where it is live — the two
/// LUT consumers once the device table matches (`live`), else that the
/// LUT is being rebuilt (the frame loop uploads it this frame).
fn chain_label(s: &DisplaySettings, live: bool) -> String {
    let chain = if s.is_identity() {
        i18n::tr("display.chain-identity")
    } else {
        let args = [
            ("exposure", FluentValue::from(format!("{:+.2}", s.exposure))),
            ("view", FluentValue::from(s.view.label())),
            ("gamma", FluentValue::from(format!("{:.2}", s.gamma))),
        ];
        i18n::tr_args("display.chain", args)
    };
    let status = if live {
        i18n::tr("display.live")
    } else {
        i18n::tr("display.rebuilding")
    };
    let args = [
        ("chain", FluentValue::from(chain)),
        ("status", FluentValue::from(status)),
    ];
    i18n::tr_args("display.active-chain", args)
}

/// Draws the panel and edits `settings` in place; any change marks `lut`
/// dirty (the frame loop rebuilds it). Returns whether any value changed
/// this frame.
#[cfg(not(feature = "ocio"))]
pub fn show(ui: &mut Ui, settings: &mut DisplaySettings, lut: &mut DisplayLutState) -> bool {
    let changed = transform_section(ui, settings, lut);
    preview_section(ui, settings, lut, preview_values);
    ui.add_space(4.0);
    language_row(ui);
    changed
}

/// The ocio-feature entry: the OCIO section threads the section state
/// in, and the preview strip runs the adopted processor (rebuilt for
/// whatever selection the section left this frame).
#[cfg(feature = "ocio")]
pub fn show(
    ui: &mut Ui,
    settings: &mut DisplaySettings,
    lut: &mut DisplayLutState,
    ocio_state: &mut OcioSectionState,
) -> bool {
    let mut changed = transform_section(ui, settings, lut);
    changed |= ocio_section(ui, settings, ocio_state, lut);
    let processor = ocio_state.processor_for(settings);
    preview_section(ui, settings, lut, |s| preview_values(s, processor));
    ui.add_space(4.0);
    language_row(ui);
    changed
}

/// The transform group: view / exposure / gamma / reset. Marks the LUT
/// dirty on any change.
fn transform_section(
    ui: &mut Ui,
    settings: &mut DisplaySettings,
    lut: &mut DisplayLutState,
) -> bool {
    let mut changed = false;

    let transform_title = i18n::tr("display.transform");
    ParamGroup::new("display-transform", transform_title.as_str()).show(ui, |group| {
        group.row(&i18n::tr("display.view"), |ui| {
            egui::ComboBox::from_id_salt("display_view_combo")
                .selected_text(settings.view.label())
                .show_ui(ui, |ui| {
                    for candidate in DisplayTransform::ALL {
                        changed |= ui
                            .selectable_value(&mut settings.view, candidate, candidate.label())
                            .changed();
                    }
                });
        });
        changed |= group
            .row(&i18n::tr("display.exposure"), |ui| {
                ui.add(
                    ValueField::new(&mut settings.exposure)
                        .range(DisplaySettings::EXPOSURE_RANGE)
                        .step(0.1),
                )
            })
            .changed();
        changed |= group
            .row(&i18n::tr("display.gamma"), |ui| {
                ui.add(ValueField::new(&mut settings.gamma).range(DisplaySettings::GAMMA_RANGE))
            })
            .changed();
        if group
            .ui
            .add_enabled(
                !settings.is_identity(),
                egui::Button::new(i18n::tr("button.reset")),
            )
            .clicked()
        {
            *settings = DisplaySettings::default();
            changed = true;
        }
    });
    if changed {
        lut.mark_display_lut_dirty();
    }
    changed
}

/// The preview strip + the status line: the chain (through `values`,
/// which threads the OCIO processor when adopted) over the synthetic
/// ramp, painted as swatches.
fn preview_section(
    ui: &mut Ui,
    settings: &DisplaySettings,
    lut: &DisplayLutState,
    values: impl Fn(&DisplaySettings) -> [[f32; 3]; PREVIEW_STOPS],
) {
    ui.add_space(4.0);
    let preview_title = i18n::tr("display.preview");
    ParamGroup::new("display-preview", preview_title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        let width = ui.available_width().max(PREVIEW_STOPS as f32);
        let size = egui::vec2(width, SWATCH_HEIGHT);
        let (strip, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(strip);
        let step = strip.width() / PREVIEW_STOPS as f32;
        for (i, color) in values(settings).map(to_color32).into_iter().enumerate() {
            let cell = egui::Rect::from_min_size(
                egui::pos2(strip.left() + i as f32 * step, strip.top()),
                egui::vec2(step, strip.height()),
            );
            painter.rect_filled(cell, 0.0, color);
        }
        ui.label(chain_label(settings, lut.is_live(settings)));
    });
}

/// The picker's label for a locale tag: the language's own name for the
/// built-in catalogs, the bare tag for discovered ones.
fn locale_label(tag: &str) -> String {
    match tag {
        "en-US" => "English (US)".to_owned(),
        "pt-BR" => "Português (Brasil)".to_owned(),
        other => other.to_owned(),
    }
}

/// The UI-language settings row (an app preference, not a project
/// setting: [`crate::i18n::set_locale_persisted`] writes the per-user
/// locale file). Takes effect from the next frame.
fn language_row(ui: &mut Ui) {
    let title = i18n::tr("settings.language");
    ParamGroup::new("display-language", title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        let (current, locales) = i18n::with(|i| (i.locale().to_owned(), i.locales()));
        let mut picked = current.clone();
        egui::ComboBox::from_id_salt("display_language_combo")
            .selected_text(locale_label(&current))
            .show_ui(ui, |ui| {
                for tag in locales {
                    let label = locale_label(&tag);
                    ui.selectable_value(&mut picked, tag, label);
                }
            });
        if picked != current {
            i18n::set_locale_persisted(&picked);
        }
    });
}

/// The HDR output row (§5 P1 row 85, `umber_gpu::hdr`): a checkbox
/// bound to the REQUEST (`hdr_mode`), disabled whenever `probe` offers
/// no HDR space. The disabled reason is a visible label, not a
/// hover-only tooltip — a disabled control's reason should be readable
/// without hunting for it. Persists the request immediately on change
/// (best-effort — a failed write logs and keeps the session's choice,
/// the same contract [`crate::i18n::set_locale_persisted`] uses).
pub fn hdr_row(ui: &mut Ui, hdr_mode: &mut umber_gpu::HdrMode, probe: &umber_gpu::HdrProbe) {
    let title = i18n::tr("display-hdr.title");
    ParamGroup::new("display-hdr", title.as_str()).show(ui, |group| {
        let ui: &mut egui::Ui = &mut *group.ui;
        let mut requested_on = hdr_mode.space().is_some();
        let available = probe.is_available();
        let response = ui.add_enabled(
            available,
            egui::Checkbox::new(&mut requested_on, i18n::tr("display-hdr.toggle")),
        );
        if available && response.changed() {
            *hdr_mode = if requested_on {
                probe
                    .best()
                    .map_or(umber_gpu::HdrMode::Sdr, umber_gpu::HdrMode::Hdr)
            } else {
                umber_gpu::HdrMode::Sdr
            };
            if let Some(path) = hdr_pref_path() {
                if let Err(err) = write_hdr_pref(&path, *hdr_mode) {
                    log::warn!("could not save HDR preference to {}: {err}", path.display());
                }
            }
        }
        if let Some(reason) = probe.disabled_reason() {
            // Literal keys at each arm (not a variable) so the i18n
            // completeness scan sees both call sites.
            let text = match reason {
                umber_gpu::DisabledReason::NotExposed => i18n::tr("display-hdr.reason-not-exposed"),
                umber_gpu::DisabledReason::SdrOnly => i18n::tr("display-hdr.reason-sdr-only"),
            };
            ui.label(text);
        }
    });
}

/// The HDR preference file: a sibling of the locale/brushes/plugins
/// per-user files (see [`crate::i18n::locale_pref_path`]). `None` when
/// no home is discoverable.
pub fn hdr_pref_path() -> Option<PathBuf> {
    hdr_pref_path_from(umber_brush::preset_library::user_preset_dir().as_deref())
}

/// Pure half of [`hdr_pref_path`].
pub fn hdr_pref_path_from(user_brush_dir: Option<&Path>) -> Option<PathBuf> {
    user_brush_dir
        .and_then(Path::parent)
        .map(|base| base.join("hdr-mode"))
}

/// The persisted HDR request, `None` if the file is missing, blank, or
/// holds a token this build does not recognize
/// ([`umber_gpu::HdrMode::from_token`]) — the caller's `unwrap_or_default`
/// then lands on `Sdr` rather than guessing.
pub fn read_hdr_pref(path: &Path) -> Option<umber_gpu::HdrMode> {
    let text = std::fs::read_to_string(path).ok()?;
    umber_gpu::HdrMode::from_token(text.trim())
}

/// Persists `mode`'s token (creating the parent dir).
pub fn write_hdr_pref(path: &Path, mode: umber_gpu::HdrMode) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", mode.token()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chain values for a settings snapshot, per feature arm (the
    /// ocio arm threads the fixed path — no adopted selection in tests).
    #[cfg(not(feature = "ocio"))]
    fn test_chain_values(s: &DisplaySettings) -> [[f32; 3]; PREVIEW_STOPS] {
        preview_values(s)
    }

    #[cfg(feature = "ocio")]
    fn test_chain_values(s: &DisplaySettings) -> [[f32; 3]; PREVIEW_STOPS] {
        preview_values(s, None)
    }

    /// A DisplaySettings literal with the per-feature ocio field filled
    /// (tests construct via this — the literals stay feature-blind).
    fn test_settings(view: DisplayTransform, exposure: f32, gamma: f32) -> DisplaySettings {
        DisplaySettings {
            view,
            exposure,
            gamma,
            #[cfg(feature = "ocio")]
            ocio: None,
        }
    }

    #[test]
    fn ramp_is_one_stop_per_swatch_below_the_clamp() {
        for pair in PREVIEW_RAMP.windows(2) {
            assert_eq!(pair[1], pair[0] * 2.0, "one EV per swatch");
        }
        // Nonzero, and +2 EV (×4) stays unclamped at every swatch.
        for &l in &PREVIEW_RAMP {
            assert!(l > 0.0 && l * 4.0 < 1.0, "stop {l}");
        }
    }

    #[test]
    fn exposure_is_strictly_monotonic_at_every_stop() {
        // The can-fail ordering: +2 EV > +1 EV > 0 EV at every swatch,
        // for every view and with or without display gamma.
        for view in DisplayTransform::ALL {
            for gamma in [1.0, 2.2, 0.5] {
                let at = |exposure: f32| test_chain_values(&test_settings(view, exposure, gamma));
                let (e0, e1, e2) = (at(0.0), at(1.0), at(2.0));
                for k in 0..PREVIEW_STOPS {
                    for c in 0..3 {
                        assert!(
                            e2[k][c] > e1[k][c] && e1[k][c] > e0[k][c],
                            "{view:?} gamma {gamma} stop {k}: \
                             +2 {} / +1 {} / 0 {}",
                            e2[k][c],
                            e1[k][c],
                            e0[k][c]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn identity_swatches_are_the_raw_ramp_bytes() {
        // Raw / 0 / 1: the chain is the identity, so each swatch is the
        // linear value quantized directly (0.2·255 = 51 at the top).
        let colors = test_chain_values(&DisplaySettings::default()).map(to_color32);
        for (color, l) in colors.iter().zip(PREVIEW_RAMP) {
            let b = (l * 255.0).round() as u8;
            assert_eq!(*color, Color32::from_rgb(b, b, b));
        }
        assert_eq!(colors[PREVIEW_STOPS - 1], Color32::from_rgb(51, 51, 51));
    }

    #[test]
    fn srgb_view_brightens_the_preview() {
        let raw = test_chain_values(&DisplaySettings::default()).map(to_color32);
        let srgb =
            test_chain_values(&test_settings(DisplayTransform::Srgb, 0.0, 1.0)).map(to_color32);
        for (r, s) in raw.iter().zip(srgb.iter()) {
            assert!(s.r() > r.r(), "sRGB {s:?} vs raw {r:?}");
        }
    }

    #[test]
    fn to_color32_clamps_and_survives_nan() {
        assert_eq!(
            to_color32([-1.0, 2.0, f32::NAN]),
            Color32::from_rgb(0, 255, 0)
        );
    }

    #[test]
    fn locale_labels_name_the_builtin_languages() {
        // Every built-in catalog gets a native name; discovered ones show
        // their tag.
        for tag in i18n::I18n::builtin().locales() {
            assert_ne!(locale_label(&tag), tag, "{tag} has no native name");
        }
        assert_eq!(locale_label("de-DE"), "de-DE");
    }

    #[test]
    fn chain_label_names_the_chain_and_the_live_consumers() {
        let identity = chain_label(&DisplaySettings::default(), true);
        assert!(identity.contains("identity"), "{identity}");
        assert!(
            identity.contains("Live in the 3D viewport (mesh pass) and the UV view"),
            "{identity}"
        );
        assert!(!identity.contains("Preview only"), "{identity}");
        let s = test_settings(DisplayTransform::Rec709, 1.5, 2.2);
        let label = chain_label(&s, true);
        assert!(
            label.contains("exposure +1.50 EV → Rec.709 → gamma 2.20"),
            "{label}"
        );
        let pending = chain_label(&s, false);
        assert!(
            pending.contains("Rebuilding the GPU display LUT"),
            "{pending}"
        );
        assert!(!pending.contains("Live in"), "{pending}");
    }

    #[test]
    fn lut_dirty_rebuild_cycle_drives_the_status() {
        // The design's test 4: settings change → rebuild → the status
        // reflects it, on the data path the frame loop runs.
        let mut settings = DisplaySettings::default();
        let mut lut = DisplayLutState::default();

        // Startup is dirty: the first frame builds the identity table —
        // the exact bytes the GPU side's identity fallback holds.
        assert!(!lut.is_live(&settings));
        let first = lut
            .take_rebuild(
                &settings,
                #[cfg(feature = "ocio")]
                None,
            )
            .expect("startup build");
        assert_eq!(first, umber_gpu::identity_lut_bytes());
        assert!(lut.is_live(&settings));
        assert!(chain_label(&settings, lut.is_live(&settings)).contains("Live in"));
        // Clean + unchanged: no upload this frame.
        assert_eq!(
            lut.take_rebuild(
                &settings,
                #[cfg(feature = "ocio")]
                None
            ),
            None
        );

        // The panel's write path: +1 EV, marked dirty → not live yet.
        settings.exposure = 1.0;
        lut.mark_display_lut_dirty();
        assert!(!lut.is_live(&settings));
        assert!(chain_label(&settings, lut.is_live(&settings)).contains("Rebuilding"));
        // The frame loop rebuilds: the chain's table (entry 100 → 200
        // under Raw +1 EV), and the status flips back to live.
        let rebuilt = lut
            .take_rebuild(
                &settings,
                #[cfg(feature = "ocio")]
                None,
            )
            .expect("dirty → rebuild");
        assert_eq!(
            rebuilt,
            build_display_lut(
                &settings,
                #[cfg(feature = "ocio")]
                None
            )
        );
        assert_eq!(&rebuilt[400..404], &[200, 200, 200, 255]);
        assert!(lut.is_live(&settings));
        assert_eq!(
            lut.take_rebuild(
                &settings,
                #[cfg(feature = "ocio")]
                None
            ),
            None
        );

        // A write path that forgot to mark (e.g. a project load) is
        // still caught: the built settings no longer match.
        settings.view = DisplayTransform::Srgb;
        assert!(!lut.is_live(&settings));
        assert_eq!(
            lut.take_rebuild(
                &settings,
                #[cfg(feature = "ocio")]
                None
            ),
            Some(build_display_lut(
                &settings,
                #[cfg(feature = "ocio")]
                None
            ))
        );
        assert!(lut.is_live(&settings));
    }

    #[test]
    fn color_and_gpu_agree_on_the_lut_shape() {
        // The crate boundary: umber-color builds the bytes, umber-gpu
        // uploads them — same length, same identity table.
        assert_eq!(DISPLAY_LUT_BYTES, umber_gpu::DISPLAY_LUT_BYTES);
        assert_eq!(
            build_display_lut(
                &DisplaySettings::default(),
                #[cfg(feature = "ocio")]
                None,
            ),
            umber_gpu::identity_lut_bytes()
        );
    }

    fn fresh_hdr_tmpdir() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "umber-app-hdr-pref-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("test tmpdir must be creatable");
        dir
    }

    #[test]
    fn hdr_pref_path_sits_beside_the_brushes_dir() {
        let dir = fresh_hdr_tmpdir();
        let path = hdr_pref_path_from(Some(dir.join("brushes").as_path())).unwrap();
        assert_eq!(path, dir.join("hdr-mode"));
        assert!(hdr_pref_path_from(None).is_none());
    }

    #[test]
    fn hdr_pref_round_trips_and_rejects_unknown_tokens() {
        let dir = fresh_hdr_tmpdir();
        let path = dir.join("hdr-mode");
        assert_eq!(read_hdr_pref(&path), None, "no file yet");

        write_hdr_pref(&path, umber_gpu::HdrMode::Hdr(umber_gpu::HdrSpace::Pq))
            .expect("write must succeed");
        assert_eq!(
            read_hdr_pref(&path),
            Some(umber_gpu::HdrMode::Hdr(umber_gpu::HdrSpace::Pq))
        );

        write_hdr_pref(&path, umber_gpu::HdrMode::Sdr).expect("write must succeed");
        assert_eq!(read_hdr_pref(&path), Some(umber_gpu::HdrMode::Sdr));

        // A hand-edited or future-version token: `None`, not a guess —
        // the caller's `unwrap_or_default` then lands on `Sdr`.
        std::fs::write(&path, "hdr11\n").unwrap();
        assert_eq!(read_hdr_pref(&path), None);
    }
}
