//! The Umber palette and the `egui::Style` authored from it
//! (docs/specs/gui-audit-and-roadmap.md §2.1, §2.4, §2.5).
//!
//! Dark warm-neutral chrome + ONE saturated accent (ember-orange). Hover
//! deepens the accent, never shifts its hue ([`deepen`]).

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};

/// The palette tokens, verbatim from the roadmap §2.1.
pub mod palette {
    use egui::Color32;

    /// App background.
    pub const BG: Color32 = Color32::from_rgb(0x16, 0x18, 0x1D);
    /// Panel fill.
    pub const PANEL: Color32 = Color32::from_rgb(0x1E, 0x21, 0x28);
    /// Raised surfaces (inactive widgets, group headers).
    pub const RAISED: Color32 = Color32::from_rgb(0x26, 0x2A, 0x33);
    /// Hovered surfaces.
    pub const HOVER: Color32 = Color32::from_rgb(0x33, 0x38, 0x42);
    /// Primary text.
    pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE9, 0xEC);
    /// Muted text (labels, units).
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9C, 0xA0, 0xAB);
    /// Faint text (disabled, placeholders).
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6A, 0x6E, 0x78);
    /// The ember-orange accent: selection / active / values.
    pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0x7A, 0x1A);
    /// Semantic success.
    pub const SUCCESS: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x72);
    /// Semantic warning.
    pub const WARN: Color32 = Color32::from_rgb(0xE5, 0xA1, 0x3D);
    /// Semantic error.
    pub const ERROR: Color32 = Color32::from_rgb(0xE5, 0x53, 0x4B);
    /// Viewport chrome — darker than BG so the 3D view dominates.
    pub const VIEWPORT: Color32 = Color32::from_rgb(0x11, 0x13, 0x17);

    /// The token table: (name, color), the census order of §2.1.
    pub const TOKENS: [(&str, Color32); 12] = [
        ("bg", BG),
        ("panel", PANEL),
        ("raised", RAISED),
        ("hover", HOVER),
        ("text", TEXT),
        ("text_muted", TEXT_MUTED),
        ("text_faint", TEXT_FAINT),
        ("accent", ACCENT),
        ("success", SUCCESS),
        ("warn", WARN),
        ("error", ERROR),
        ("viewport", VIEWPORT),
    ];
}

/// The house motion curve duration (§2.4: 120ms ease-out). Pass to
/// `animate_value_with_time` / `animate_bool_with_time`.
pub const MOTION_SECS: f32 = 0.12;

/// How much the accent darkens on hover (RGB multiply, hue-preserving).
pub const ACCENT_HOVER_FACTOR: f32 = 0.85;

/// UI body text size, px (§2.2).
pub const TEXT_UI: f32 = 13.0;
/// Label text size, px (§2.2).
pub const TEXT_LABEL: f32 = 11.0;
/// Status-bar text size, px (§2.2).
pub const TEXT_STATUS: f32 = 10.0;
/// Heading text size, px (group titles above the body size).
pub const TEXT_HEADING: f32 = 15.0;
/// Numeric (JetBrains Mono) text size, px — value fields.
pub const TEXT_NUMERIC: f32 = 12.0;
/// The named text style for the status bar (`TextStyle::Name`).
pub const STATUS_STYLE: &str = "status";

/// Lower bound of the user text scale (§2.2: ±20%).
pub const TEXT_SCALE_MIN: f32 = 0.8;
/// Upper bound of the user text scale (§2.2: ±20%).
pub const TEXT_SCALE_MAX: f32 = 1.2;

/// Layout density (§2.5): compact halves every spacing token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Density {
    /// The default, roomy layout.
    #[default]
    Comfortable,
    /// Dense layout: spacing / 2.
    Compact,
}

impl Density {
    /// The spacing multiplier: 1 for comfortable, 1/2 for compact.
    pub fn spacing_scale(self) -> f32 {
        match self {
            Density::Comfortable => 1.0,
            Density::Compact => 0.5,
        }
    }
}

/// The spacing tokens one density resolves to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpacingTokens {
    /// Gap between consecutive widgets.
    pub item: Vec2,
    /// Padding inside buttons.
    pub button_padding: Vec2,
    /// Indent of nested content (param-group bodies).
    pub indent: f32,
    /// Minimum interactive height (hit target).
    pub interact_height: f32,
}

/// Comfortable item spacing.
const BASE_ITEM: Vec2 = Vec2::new(8.0, 6.0);
/// Comfortable button padding.
const BASE_BUTTON_PADDING: Vec2 = Vec2::new(8.0, 4.0);
/// Comfortable indent.
const BASE_INDENT: f32 = 16.0;
/// Interact height per density. NOT halved: it is a hit target, not a
/// gap — halving 22px to 11px would make every control unclickable.
const INTERACT_HEIGHT_COMFORTABLE: f32 = 22.0;
const INTERACT_HEIGHT_COMPACT: f32 = 18.0;

impl SpacingTokens {
    /// Resolve the tokens for `density` (the gaps scale by
    /// [`Density::spacing_scale`]).
    pub fn for_density(density: Density) -> Self {
        let s = density.spacing_scale();
        SpacingTokens {
            item: BASE_ITEM * s,
            button_padding: BASE_BUTTON_PADDING * s,
            indent: BASE_INDENT * s,
            interact_height: match density {
                Density::Comfortable => INTERACT_HEIGHT_COMFORTABLE,
                Density::Compact => INTERACT_HEIGHT_COMPACT,
            },
        }
    }
}

/// Clamp a user text scale into the ±20% band.
pub fn clamp_text_scale(scale: f32) -> f32 {
    if scale.is_nan() {
        return 1.0;
    }
    scale.clamp(TEXT_SCALE_MIN, TEXT_SCALE_MAX)
}

/// Darken a color by `factor` in RGB (alpha kept). A uniform RGB scale
/// leaves the hue unchanged — the "hover deepens, never shifts hue" rule.
pub fn deepen(color: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(0.0, 1.0);
    let scale = |c: u8| (c as f32 * f).round() as u8;
    Color32::from_rgba_unmultiplied(
        scale(color.r()),
        scale(color.g()),
        scale(color.b()),
        color.a(),
    )
}

/// The accent (selection / active / values).
pub fn accent() -> Color32 {
    palette::ACCENT
}

/// The accent on hover: deepened, same hue.
pub fn accent_hover() -> Color32 {
    deepen(palette::ACCENT, ACCENT_HOVER_FACTOR)
}

/// The panel fill.
pub fn panel_bg() -> Color32 {
    palette::PANEL
}

/// The app background.
pub fn app_bg() -> Color32 {
    palette::BG
}

/// Raised-surface fill.
pub fn raised() -> Color32 {
    palette::RAISED
}

/// Hovered-surface fill.
pub fn hover() -> Color32 {
    palette::HOVER
}

/// Primary text.
pub fn text() -> Color32 {
    palette::TEXT
}

/// Muted text (labels, units).
pub fn muted() -> Color32 {
    palette::TEXT_MUTED
}

/// Faint text (disabled, placeholders).
pub fn faint() -> Color32 {
    palette::TEXT_FAINT
}

/// The viewport chrome fill.
pub fn viewport_bg() -> Color32 {
    palette::VIEWPORT
}

/// The Umber style at the default text scale.
pub fn umber_style(density: Density) -> egui::Style {
    umber_style_scaled(density, 1.0)
}

/// The Umber style with a user text scale (clamped to ±20%).
pub fn umber_style_scaled(density: Density, text_scale: f32) -> egui::Style {
    let scale = clamp_text_scale(text_scale);
    let mut style = egui::Style::default();
    style.visuals = umber_visuals();
    style.animation_time = MOTION_SECS;

    let tokens = SpacingTokens::for_density(density);
    style.spacing.item_spacing = tokens.item;
    style.spacing.button_padding = tokens.button_padding;
    style.spacing.indent = tokens.indent;
    style.spacing.interact_size.y = tokens.interact_height;

    style.text_styles = [
        (TextStyle::Body, FontId::proportional(TEXT_UI * scale)),
        (TextStyle::Button, FontId::proportional(TEXT_UI * scale)),
        (TextStyle::Small, FontId::proportional(TEXT_LABEL * scale)),
        (
            TextStyle::Heading,
            FontId::proportional(TEXT_HEADING * scale),
        ),
        (
            TextStyle::Monospace,
            FontId::monospace(TEXT_NUMERIC * scale),
        ),
        (
            TextStyle::Name(STATUS_STYLE.into()),
            FontId::new(TEXT_STATUS * scale, FontFamily::Proportional),
        ),
    ]
    .into();
    style
}

/// The Umber visuals: stock dark as the base, every color re-authored.
fn umber_visuals() -> egui::Visuals {
    use palette::*;
    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = None;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, RAISED);
    v.extreme_bg_color = BG;
    v.faint_bg_color = RAISED;
    v.code_bg_color = BG;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = ERROR;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);

    let radius = CornerRadius::same(3);
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, RAISED);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = RAISED;
    w.inactive.weak_bg_fill = RAISED;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = HOVER;
    w.hovered.weak_bg_fill = HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, HOVER);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = radius;

    w.active.bg_fill = HOVER;
    w.active.weak_bg_fill = HOVER;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(2.0, TEXT);
    w.active.corner_radius = radius;

    w.open.bg_fill = RAISED;
    w.open.weak_bg_fill = RAISED;
    w.open.bg_stroke = Stroke::new(1.0, HOVER);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = radius;

    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> (u8, u8, u8) {
        let s = s.trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap();
        (byte(0), byte(2), byte(4))
    }

    fn rgb(c: Color32) -> (u8, u8, u8) {
        (c.r(), c.g(), c.b())
    }

    #[test]
    fn palette_hex_values_pinned() {
        // The roadmap §2.1 strings, verbatim.
        let pinned = [
            ("bg", "#16181D"),
            ("panel", "#1E2128"),
            ("raised", "#262A33"),
            ("hover", "#333842"),
            ("text", "#E8E9EC"),
            ("text_muted", "#9CA0AB"),
            ("text_faint", "#6A6E78"),
            ("accent", "#FF7A1A"),
            ("success", "#3FB972"),
            ("warn", "#E5A13D"),
            ("error", "#E5534B"),
            ("viewport", "#111317"),
        ];
        assert_eq!(pinned.len(), palette::TOKENS.len());
        for ((name, color), (pin_name, pin_hex)) in palette::TOKENS.iter().zip(pinned) {
            assert_eq!(*name, pin_name);
            assert_eq!(rgb(*color), hex(pin_hex), "token {name}");
            assert_eq!(color.a(), 255, "token {name} must be opaque");
        }
    }

    #[test]
    fn semantic_helpers_map_to_tokens() {
        assert_eq!(accent(), palette::ACCENT);
        assert_eq!(panel_bg(), palette::PANEL);
        assert_eq!(hover(), palette::HOVER);
        assert_eq!(muted(), palette::TEXT_MUTED);
        assert_eq!(viewport_bg(), palette::VIEWPORT);
    }

    #[test]
    fn accent_hover_deepens_without_hue_shift() {
        let h = accent_hover();
        let a = accent();
        assert!(h.r() < a.r() && h.g() < a.g() && h.b() < a.b());
        // Uniform scale: channel ratios hold (within rounding).
        let ratio = |c: Color32| c.g() as f32 / c.r() as f32;
        assert!((ratio(h) - ratio(a)).abs() < 0.01);
        assert_eq!(h.a(), 255);
    }

    #[test]
    fn compact_is_half_spacing() {
        let c = SpacingTokens::for_density(Density::Comfortable);
        let k = SpacingTokens::for_density(Density::Compact);
        assert_eq!(k.item, c.item / 2.0);
        assert_eq!(k.button_padding, c.button_padding / 2.0);
        assert_eq!(k.indent, c.indent / 2.0);
        assert_eq!(Density::Compact.spacing_scale(), 0.5);
        assert_eq!(Density::Comfortable.spacing_scale(), 1.0);
        // The hit target shrinks, but is not halved.
        assert!(k.interact_height < c.interact_height);
        assert!(k.interact_height > c.interact_height / 2.0);
    }

    #[test]
    fn style_applies_density_tokens() {
        let s = umber_style(Density::Compact);
        let k = SpacingTokens::for_density(Density::Compact);
        assert_eq!(s.spacing.item_spacing, k.item);
        assert_eq!(s.spacing.indent, k.indent);
        assert_eq!(s.visuals.panel_fill, palette::PANEL);
        assert_eq!(s.visuals.selection.stroke.color, palette::ACCENT);
        assert_eq!(s.animation_time, MOTION_SECS);
    }

    #[test]
    fn text_scale_clamped_to_twenty_percent() {
        assert_eq!(clamp_text_scale(0.5), TEXT_SCALE_MIN);
        assert_eq!(clamp_text_scale(2.0), TEXT_SCALE_MAX);
        assert_eq!(clamp_text_scale(1.1), 1.1);
        assert_eq!(clamp_text_scale(f32::NAN), 1.0);
        let s = umber_style_scaled(Density::Comfortable, 5.0);
        assert_eq!(
            s.text_styles[&TextStyle::Body].size,
            TEXT_UI * TEXT_SCALE_MAX
        );
    }
}
