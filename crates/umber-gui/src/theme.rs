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
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x8E, 0x92, 0x98);
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

/// The UI theme (UI/UX pass U4, photocraft's `Tokens`-per-theme
/// pattern): every theme is one [`Tokens`] set over the same token names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Theme {
    /// Umber Dark: the warm-neutral dark chrome ([`palette`]).
    #[default]
    Dark,
    /// Umber Light: the same ember accent, deepened for light surfaces.
    Light,
    /// High Contrast: black chrome, white text, a bright accent; every
    /// text token clears WCAG AAA (7:1) on the panel.
    HighContrast,
}

impl Theme {
    /// Every theme, in menu order.
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::HighContrast];

    /// The persisted name (`theme=` in the preferences file).
    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
        }
    }

    /// The theme a persisted name names, if any.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name.trim())
    }

    /// The theme's tokens.
    pub fn tokens(self) -> Tokens {
        match self {
            Self::Dark => Tokens::DARK,
            Self::Light => Tokens::LIGHT,
            Self::HighContrast => Tokens::HIGH_CONTRAST,
        }
    }

    /// Whether the theme's surfaces are dark (egui's `dark_mode`).
    pub fn is_dark(self) -> bool {
        !matches!(self, Self::Light)
    }
}

/// One theme's color tokens: the same names as [`palette::TOKENS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// App background.
    pub bg: Color32,
    /// Panel fill.
    pub panel: Color32,
    /// Raised surfaces.
    pub raised: Color32,
    /// Hovered surfaces.
    pub hover: Color32,
    /// Primary text.
    pub text: Color32,
    /// Muted text.
    pub text_muted: Color32,
    /// Faint text.
    pub text_faint: Color32,
    /// The accent.
    pub accent: Color32,
    /// Semantic success.
    pub success: Color32,
    /// Semantic warning.
    pub warn: Color32,
    /// Semantic error.
    pub error: Color32,
    /// Viewport chrome.
    pub viewport: Color32,
}

impl Tokens {
    /// Umber Dark: exactly [`palette`] (the pinned roadmap values).
    pub const DARK: Self = Self {
        bg: palette::BG,
        panel: palette::PANEL,
        raised: palette::RAISED,
        hover: palette::HOVER,
        text: palette::TEXT,
        text_muted: palette::TEXT_MUTED,
        text_faint: palette::TEXT_FAINT,
        accent: palette::ACCENT,
        success: palette::SUCCESS,
        warn: palette::WARN,
        error: palette::ERROR,
        viewport: palette::VIEWPORT,
    };

    /// Umber Light: warm off-white surfaces, near-black text, the ember
    /// accent deepened so it holds 3:1 against the panel (WCAG non-text).
    /// The viewport stays a mid-dark neutral: texture work is judged on
    /// a neutral backdrop, not a white one (Substance / Blender keep the
    /// 3D view dark under a light UI too).
    pub const LIGHT: Self = Self {
        bg: Color32::from_rgb(0xE4, 0xE2, 0xDE),
        panel: Color32::from_rgb(0xF2, 0xF0, 0xEC),
        raised: Color32::from_rgb(0xE6, 0xE3, 0xDD),
        hover: Color32::from_rgb(0xD8, 0xD4, 0xCC),
        text: Color32::from_rgb(0x1C, 0x1D, 0x21),
        text_muted: Color32::from_rgb(0x50, 0x53, 0x5B),
        text_faint: Color32::from_rgb(0x60, 0x61, 0x67),
        accent: Color32::from_rgb(0xC4, 0x55, 0x00),
        success: Color32::from_rgb(0x1E, 0x7A, 0x45),
        warn: Color32::from_rgb(0x8F, 0x5A, 0x00),
        error: Color32::from_rgb(0xB8, 0x2E, 0x26),
        viewport: Color32::from_rgb(0x3A, 0x3C, 0x40),
    };

    /// High Contrast: pure black chrome, white text, and every text
    /// token at 7:1 or better on the panel.
    pub const HIGH_CONTRAST: Self = Self {
        bg: Color32::from_rgb(0x00, 0x00, 0x00),
        panel: Color32::from_rgb(0x00, 0x00, 0x00),
        raised: Color32::from_rgb(0x1A, 0x1A, 0x1A),
        hover: Color32::from_rgb(0x33, 0x33, 0x33),
        text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        text_muted: Color32::from_rgb(0xE0, 0xE0, 0xE0),
        text_faint: Color32::from_rgb(0xB8, 0xB8, 0xB8),
        accent: Color32::from_rgb(0xFF, 0xB0, 0x40),
        success: Color32::from_rgb(0x5C, 0xF0, 0x96),
        warn: Color32::from_rgb(0xFF, 0xD2, 0x4D),
        error: Color32::from_rgb(0xFF, 0x80, 0x78),
        viewport: Color32::from_rgb(0x00, 0x00, 0x00),
    };

    /// `name`'s color (the [`palette::TOKENS`] names).
    pub fn get(&self, name: &str) -> Option<Color32> {
        Some(match name {
            "bg" => self.bg,
            "panel" => self.panel,
            "raised" => self.raised,
            "hover" => self.hover,
            "text" => self.text,
            "text_muted" => self.text_muted,
            "text_faint" => self.text_faint,
            "accent" => self.accent,
            "success" => self.success,
            "warn" => self.warn,
            "error" => self.error,
            "viewport" => self.viewport,
            _ => return None,
        })
    }
}

/// The active theme, process-wide: the no-argument helpers ([`accent`],
/// [`text`] …) are called from painters with no `Context` in reach, so
/// the theme they read lives here; [`apply_theme`] sets it.
static ACTIVE_THEME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The active theme ([`apply_theme`] sets it; Dark until then).
pub fn active_theme() -> Theme {
    match ACTIVE_THEME.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Theme::Light,
        2 => Theme::HighContrast,
        _ => Theme::Dark,
    }
}

fn set_active_theme(theme: Theme) {
    let v = match theme {
        Theme::Dark => 0,
        Theme::Light => 1,
        Theme::HighContrast => 2,
    };
    ACTIVE_THEME.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// The active theme's tokens.
pub fn tokens() -> Tokens {
    active_theme().tokens()
}

/// The 2D canvas colours (the UV view's padding, the UV square, the
/// UDIM tile fill, labels and grid): content backdrops, so they stay a
/// neutral dark under Light (as the 3D view does) but follow the theme's
/// viewport token. Dark keeps the original UV-view constants exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasColors {
    /// Outside the 0..1 square.
    pub pad: Color32,
    /// Inside the 0..1 square.
    pub square: Color32,
    /// A present but inactive UDIM tile.
    pub tile_dim: Color32,
    /// Tile numbers and the empty-canvas hint.
    pub label: Color32,
    /// The UDIM grid lines.
    pub grid: Color32,
}

impl Theme {
    /// This theme's 2D canvas colours.
    pub fn canvas(self) -> CanvasColors {
        match self {
            Self::Dark => CanvasColors {
                pad: Color32::from_rgb(28, 28, 30),
                square: Color32::from_rgb(38, 38, 42),
                tile_dim: Color32::from_rgb(32, 32, 36),
                label: Color32::from_rgb(0x8C, 0x94, 0x9E),
                grid: Color32::from_rgb(55, 58, 64),
            },
            Self::Light => CanvasColors {
                pad: Color32::from_rgb(0x3A, 0x3C, 0x40),
                square: Color32::from_rgb(0x48, 0x4A, 0x4F),
                tile_dim: Color32::from_rgb(0x41, 0x43, 0x48),
                label: Color32::from_rgb(0xB4, 0xBA, 0xC4),
                grid: Color32::from_rgb(0x62, 0x65, 0x6C),
            },
            Self::HighContrast => CanvasColors {
                pad: Color32::from_rgb(0, 0, 0),
                square: Color32::from_rgb(0x14, 0x14, 0x14),
                tile_dim: Color32::from_rgb(0x0A, 0x0A, 0x0A),
                label: Color32::from_rgb(0xD0, 0xD0, 0xD0),
                grid: Color32::from_rgb(0x80, 0x80, 0x80),
            },
        }
    }
}

/// The active theme's 2D canvas colours.
pub fn canvas() -> CanvasColors {
    active_theme().canvas()
}

/// WCAG 2.x relative luminance of an sRGB color.
pub fn relative_luminance(c: Color32) -> f32 {
    let lin = |u: u8| {
        let v = u as f32 / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG 2.x contrast ratio between two colors (1..=21).
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The house motion curve duration (§2.4: 120ms ease-out). Pass to
/// `animate_value_with_time` / `animate_bool_with_time`.
pub const MOTION_SECS: f32 = 0.12;

/// The motion duration to animate with: the style's `animation_time`
/// (the house [`MOTION_SECS`], or 0 under Reduce Motion, which the app
/// writes there). Widgets that pass a fixed duration to
/// `animate_value_with_time` bypass Reduce Motion; read it here instead.
pub fn motion_secs(ui: &egui::Ui) -> f32 {
    ui.style().animation_time
}

/// Seconds the pointer rests before a tooltip shows (GUI plan G0,
/// photocraft `theme.rs`: 0.35 s). egui's default is 0, so tooltips flash
/// while the pointer merely crosses a toolbar.
pub const TOOLTIP_DELAY_SECS: f32 = 0.35;

/// Maximum tooltip width, px: long `tr()` strings wrap instead of spanning
/// the screen.
pub const TOOLTIP_WIDTH: f32 = 280.0;

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

/// g7-density-scale (B:F10): the extra grab width added to dock
/// separators (`egui_dock::SeparatorStyle::extra_interact_width`).
/// The default matches egui_dock's own (4 pt); a tablet/pen session
/// doubles it so a 1 pt splitter is grabbable with a stylus tip.
pub const SPLITTER_HIT_ZONE: f32 = 4.0;
/// The tablet variant: 8 pt of extra grab width.
pub const SPLITTER_HIT_ZONE_TABLET: f32 = 8.0;

/// g7-density-scale: how grabbable the dock separators are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HitZone {
    /// egui_dock's default (4 pt extra).
    #[default]
    Standard,
    /// Doubled for stylus work.
    Tablet,
}

impl HitZone {
    /// The `extra_interact_width` this mode resolves to.
    pub fn extra_interact_width(self) -> f32 {
        match self {
            HitZone::Standard => SPLITTER_HIT_ZONE,
            HitZone::Tablet => SPLITTER_HIT_ZONE_TABLET,
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
    tokens().accent
}

/// The accent on hover: deepened, same hue.
pub fn accent_hover() -> Color32 {
    deepen(accent(), ACCENT_HOVER_FACTOR)
}

/// Semantic success (the active theme's).
pub fn success() -> Color32 {
    tokens().success
}

/// Semantic warning (the active theme's).
pub fn warn() -> Color32 {
    tokens().warn
}

/// Semantic error (the active theme's).
pub fn error() -> Color32 {
    tokens().error
}

/// The panel fill.
pub fn panel_bg() -> Color32 {
    tokens().panel
}

/// The app background.
pub fn app_bg() -> Color32 {
    tokens().bg
}

/// Raised-surface fill.
pub fn raised() -> Color32 {
    tokens().raised
}

/// Hovered-surface fill.
pub fn hover() -> Color32 {
    tokens().hover
}

/// Primary text.
pub fn text() -> Color32 {
    tokens().text
}

/// Muted text (labels, units).
pub fn muted() -> Color32 {
    tokens().text_muted
}

/// Faint text (disabled, placeholders).
pub fn faint() -> Color32 {
    tokens().text_faint
}

/// The viewport chrome fill.
pub fn viewport_bg() -> Color32 {
    tokens().viewport
}

/// F6 (2026-11-10 round-3 audit): the shell chrome's style helper —
/// the Umber style at `density`, but at the CONTEXT's current text scale
/// (read off the Body font) instead of a hard 1.0, so the text-scale
/// preference reaches the toolbar, strip and status bar.
pub fn chrome_style(ctx: &egui::Context, ui_style: &egui::Style, density: Density) -> egui::Style {
    let scale = ui_style.text_styles[&egui::TextStyle::Body].size / TEXT_UI;
    umber_style_scaled_with(ctx, density, scale)
}

/// The Umber style at the default text scale.
pub fn umber_style(density: Density) -> egui::Style {
    umber_style_scaled(density, 1.0)
}

/// The Umber style with a user text scale (clamped to ±20%).
pub fn umber_style_scaled(density: Density, text_scale: f32) -> egui::Style {
    style_with_visuals(umber_visuals_compiled(), density, text_scale)
}

/// The style for `theme` (no live overrides): the per-theme builder the
/// tests and [`apply_theme`] share.
pub fn themed_style(theme: Theme, density: Density, text_scale: f32) -> egui::Style {
    style_with_visuals(visuals_for(theme, None), density, text_scale)
}

/// [`umber_style_scaled`] with the live [`TokenOverrides`] on `ctx`
/// applied over the compiled palette.
pub fn umber_style_scaled_with(
    ctx: &egui::Context,
    density: Density,
    text_scale: f32,
) -> egui::Style {
    style_with_visuals(umber_visuals(ctx), density, text_scale)
}

fn style_with_visuals(visuals: egui::Visuals, density: Density, text_scale: f32) -> egui::Style {
    let scale = clamp_text_scale(text_scale);
    // Struct-update form (not field reassignment): clippy's
    // `field_reassign_with_default` fails the post-Default mutation pattern.
    let mut style = egui::Style {
        visuals,
        animation_time: MOTION_SECS,
        ..Default::default()
    };
    style.interaction.tooltip_delay = TOOLTIP_DELAY_SECS;
    style.spacing.tooltip_width = TOOLTIP_WIDTH;
    // Thin overlay scroll bars, but with an opaque handle: egui's thin
    // preset fades the handle to near-invisible while idle (photocraft
    // `theme.rs` sets the same 1.0), and a scroll bar you cannot see is a
    // panel you do not know scrolls.
    style.spacing.scroll = egui::style::ScrollStyle {
        interact_handle_opacity: 1.0,
        ..egui::style::ScrollStyle::thin()
    };

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

/// Installs the Umber style on the whole context (GUI plan G0,
/// `g0-style-both-branches`).
///
/// egui keeps one style per OS theme (`Theme::Dark` / `Theme::Light`)
/// and switches between them when the OS appearance flips. Setting only
/// one branch (or calling `set_style`, which writes only the current
/// branch) lets an OS light-mode flip swap egui's stock light palette in
/// over the Umber chrome mid-session. This writes the SAME style to both
/// branches, and keeps `ThemePreference::System` so appearance events
/// still arrive (photocraft `theme.rs`, PC:#9).
/// Debug-only live-reload token overrides (GUI plan g0-style): a
/// `UMBER_THEME_FILE` of `name = #rrggbb` lines, keyed by the lowercase
/// [`palette::TOKENS`] names. Applied over the compiled palette by
/// [`apply`], so a debug session can recolor without recompiling.
pub type TokenOverrides = std::collections::HashMap<String, Color32>;

fn overrides_id() -> egui::Id {
    egui::Id::new("umber-theme-overrides")
}

/// Parses `name = #rrggbb` lines. Unknown token names and bad hex are
/// skipped — a reload must never take the theme down. `#` starts a
/// comment only at the start of a line.
pub fn parse_token_overrides(text: &str) -> TokenOverrides {
    let mut map = TokenOverrides::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, hex)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        if !palette::TOKENS.iter().any(|(n, _)| *n == name) {
            continue;
        }
        if let Ok(c) = Color32::from_hex(hex.trim()) {
            map.insert(name, c);
        }
    }
    map
}

/// Stores `overrides` on `ctx` (persistent), replacing any previous set.
pub fn set_token_overrides(ctx: &egui::Context, overrides: TokenOverrides) {
    ctx.data_mut(|d| d.insert_persisted(overrides_id(), overrides));
}

/// The active overrides on `ctx`, if any.
pub fn token_overrides(ctx: &egui::Context) -> TokenOverrides {
    ctx.data_mut(|d| d.get_persisted::<TokenOverrides>(overrides_id()))
        .unwrap_or_default()
}

/// `name`'s color in `theme`: the live override when present, else the
/// theme's token.
fn token(theme: Theme, ov: Option<&TokenOverrides>, name: &str) -> Color32 {
    if let Some(ov) = ov {
        if let Some(c) = ov.get(name) {
            return *c;
        }
    }
    theme.tokens().get(name).unwrap_or(Color32::BLACK)
}

/// Applies the themed style to both OS themes (dark and light) at the
/// given density and text scale; stores any active token overrides so
/// they re-apply on every subsequent `apply`.
pub fn apply(ctx: &egui::Context, density: Density, text_scale: f32) {
    let style = std::sync::Arc::new(umber_style_scaled_with(ctx, density, text_scale));
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

/// Makes `theme` the active theme (the token helpers read it) and
/// installs its style on both OS-theme branches, as [`apply`] does.
pub fn apply_theme(ctx: &egui::Context, theme: Theme, density: Density, text_scale: f32) {
    set_active_theme(theme);
    apply(ctx, density, text_scale);
}

/// The keyboard-focus ring width. egui 0.36 paints the `active` widget
/// visuals for the widget holding keyboard focus, so the focus ring token
/// IS `visuals.widgets.active.bg_stroke` — a 2px accent ring (hover/press
/// share the state, which reads as the same emphasis).
pub const FOCUS_RING_WIDTH: f32 = 2.0;

/// The Umber visuals with the live [`TokenOverrides`] on `ctx`.
fn umber_visuals(ctx: &egui::Context) -> egui::Visuals {
    visuals_for(active_theme(), Some(&token_overrides(ctx)))
}

fn umber_visuals_compiled() -> egui::Visuals {
    visuals_for(active_theme(), None)
}

/// The Umber visuals for `theme`: egui's stock dark or light as the base
/// (per [`Theme::is_dark`]), every color re-authored from the theme's
/// tokens. `ov` (the live token overrides, debug-only) recolors them.
fn visuals_for(theme: Theme, ov: Option<&TokenOverrides>) -> egui::Visuals {
    let c_bg = token(theme, ov, "bg");
    let c_panel = token(theme, ov, "panel");
    let c_raised = token(theme, ov, "raised");
    let c_hover = token(theme, ov, "hover");
    let c_text = token(theme, ov, "text");
    let c_accent = token(theme, ov, "accent");
    let c_warn = token(theme, ov, "warn");
    let c_error = token(theme, ov, "error");
    let dark = theme.is_dark();
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.dark_mode = dark;
    v.override_text_color = None;
    v.panel_fill = c_panel;
    v.window_fill = c_panel;
    v.window_stroke = Stroke::new(1.0, c_raised);
    v.extreme_bg_color = c_bg;
    v.faint_bg_color = c_raised;
    v.code_bg_color = c_bg;
    v.hyperlink_color = c_accent;
    v.warn_fg_color = c_warn;
    v.error_fg_color = c_error;
    v.selection.bg_fill = c_accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, c_accent);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);

    let radius = CornerRadius::same(3);
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = c_panel;
    w.noninteractive.weak_bg_fill = c_panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, c_raised);
    w.noninteractive.fg_stroke = Stroke::new(1.0, c_text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = c_raised;
    w.inactive.weak_bg_fill = c_raised;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, c_text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = c_hover;
    w.hovered.weak_bg_fill = c_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, c_hover);
    w.hovered.fg_stroke = Stroke::new(1.5, c_text);
    w.hovered.corner_radius = radius;

    w.active.bg_fill = c_hover;
    w.active.weak_bg_fill = c_hover;
    w.active.bg_stroke = Stroke::new(FOCUS_RING_WIDTH, c_accent);
    w.active.fg_stroke = Stroke::new(2.0, c_text);
    w.active.corner_radius = radius;

    w.open.bg_fill = c_raised;
    w.open.weak_bg_fill = c_raised;
    w.open.bg_stroke = Stroke::new(1.0, c_hover);
    w.open.fg_stroke = Stroke::new(1.0, c_text);
    w.open.corner_radius = radius;

    if theme == Theme::HighContrast {
        // Disabled labels and placeholders stay at 7:1 or better on
        // black (egui's defaults halve the text: about 5:1 here).
        v.disabled_alpha = 0.75;
        v.weak_text_color = Some(token(theme, ov, "text_faint"));
        // Every interactive edge drawn: no fill-only affordances.
        let edge = Stroke::new(1.0, token(theme, ov, "text_faint"));
        w.inactive.bg_stroke = edge;
        w.noninteractive.bg_stroke = edge;
        v.window_stroke = edge;
    }

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
    fn token_overrides_parse_skips_junk() {
        let ov = parse_token_overrides(
            "# a comment\naccent = #00FF00\nbogus = #123456\npanel=#ABCDEF\nno-equals-line\nbad = nothex\n",
        );
        assert_eq!(ov.len(), 2);
        assert_eq!(ov["accent"], Color32::from_rgb(0x00, 0xFF, 0x00));
        assert_eq!(ov["panel"], Color32::from_rgb(0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn token_overrides_recolor_the_visuals() {
        let ctx = egui::Context::default();
        let mut ov = TokenOverrides::new();
        ov.insert("panel".to_owned(), Color32::from_rgb(1, 2, 3));
        set_token_overrides(&ctx, ov);
        let v = umber_visuals(&ctx);
        assert_eq!(v.panel_fill, Color32::from_rgb(1, 2, 3));
        assert_eq!(v.window_fill, Color32::from_rgb(1, 2, 3));
        // Untouched tokens stay compiled.
        assert_eq!(v.extreme_bg_color, palette::BG);
        // And the compiled path is unchanged without overrides.
        assert_eq!(umber_visuals_compiled().panel_fill, palette::PANEL);
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
            ("text_faint", "#8E9298"),
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
    fn focus_ring_token_is_the_active_widget_ring() {
        // egui 0.36 renders the focused widget with the `active` visuals,
        // so the focus ring is pinned here as a theme token.
        let s = umber_style(Density::Comfortable);
        assert_eq!(
            s.visuals.widgets.active.bg_stroke,
            Stroke::new(FOCUS_RING_WIDTH, palette::ACCENT)
        );
    }

    #[test]
    fn apply_writes_both_theme_branches() {
        let ctx = egui::Context::default();
        apply(&ctx, Density::Comfortable, 1.0);
        let dark = ctx.style_of(egui::Theme::Dark);
        let light = ctx.style_of(egui::Theme::Light);
        // An OS appearance flip must not change the chrome.
        assert_eq!(dark.visuals.panel_fill, palette::PANEL);
        assert_eq!(light.visuals.panel_fill, palette::PANEL);
        assert_eq!(*dark, *light);
        ctx.set_theme(egui::Theme::Light);
        assert_eq!(ctx.global_style().visuals.panel_fill, palette::PANEL);
    }

    #[test]
    fn style_sets_tooltip_and_scroll_tokens() {
        let s = umber_style(Density::Comfortable);
        assert_eq!(s.interaction.tooltip_delay, TOOLTIP_DELAY_SECS);
        assert_eq!(s.spacing.tooltip_width, TOOLTIP_WIDTH);
        assert!(s.spacing.scroll.floating, "thin = floating bars");
        assert_eq!(s.spacing.scroll.interact_handle_opacity, 1.0);
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

    /// U4: Dark IS the pinned palette; the names round-trip; every
    /// theme's text tokens clear the WCAG AA text floor (4.5:1) on the
    /// panel, including faint (raised from the 3:1 non-text floor by the
    /// 2026-11-10 round-3 audit F5); the accent and semantic colors hold
    /// 3:1; High Contrast holds AAA 7:1 on every text token. egui's
    /// weak_text_color is measured too, not assumed.
    #[test]
    fn themes_round_trip_and_clear_their_contrast_floors() {
        for (name, color) in palette::TOKENS {
            assert_eq!(Tokens::DARK.get(name), Some(color), "{name}");
        }
        for t in Theme::ALL {
            assert_eq!(Theme::from_name(t.name()), Some(t));
            let k = t.tokens();
            for (name, _) in palette::TOKENS {
                assert!(k.get(name).is_some(), "{t:?} carries {name}");
            }
            let on = |c: Color32| contrast_ratio(c, k.panel);
            let on_raised = |c: Color32| contrast_ratio(c, k.raised);
            assert!(on(k.text) >= 4.5, "{t:?} text {}", on(k.text));
            assert!(on(k.text_muted) >= 4.5, "{t:?} muted {}", on(k.text_muted));
            assert!(
                on(k.text_faint) >= 4.5 && on_raised(k.text_faint) >= 4.5,
                "{t:?} faint {} / raised {}",
                on(k.text_faint),
                on_raised(k.text_faint)
            );
            assert!(on(k.accent) >= 3.0, "{t:?} accent {}", on(k.accent));
            for sem in [k.warn, k.error, k.success] {
                assert!(on(sem) >= 3.0, "{t:?} semantic {}", on(sem));
            }
            // egui's weak (disabled/placeholder) color is measured, not
            // assumed: it must clear the text floor too (F5).
            let v = visuals_for(t, None);
            let weak = v.weak_text_color.unwrap_or(v.text_color());
            assert!(
                contrast_ratio(weak, k.panel) >= 4.5,
                "{t:?} weak {}",
                contrast_ratio(weak, k.panel)
            );
        }
        let hc = Tokens::HIGH_CONTRAST;
        for c in [hc.text, hc.text_muted, hc.text_faint, hc.accent] {
            assert!(contrast_ratio(c, hc.panel) >= 7.0);
        }
        assert_eq!(Theme::from_name("bogus"), None);
        assert!((contrast_ratio(Color32::WHITE, Color32::BLACK) - 21.0).abs() < 0.01);
    }

    /// U4 follow-up: canvas labels read on their own square in every
    /// theme (4.5:1), grid lines show (1.5:1 is enough for a 0.5 px
    /// guide on dark), and Dark is the original UV-view constants.
    #[test]
    fn canvas_colors_read_in_every_theme() {
        let d = Theme::Dark.canvas();
        assert_eq!(d.square, Color32::from_rgb(38, 38, 42));
        assert_eq!(d.pad, Color32::from_rgb(28, 28, 30));
        for t in [Theme::Dark, Theme::Light, Theme::HighContrast] {
            let c = t.canvas();
            assert!(contrast_ratio(c.label, c.square) >= 4.5, "{t:?} label");
        }
        // Grid lines are 0.5 px guides, not text: 1.5:1 where the canvas
        // is light, still visible (>= 1.0) on the Dark original constants.
        for t in [Theme::Light, Theme::HighContrast] {
            let c = t.canvas();
            assert!(contrast_ratio(c.grid, c.square) >= 1.5, "{t:?} grid");
        }
        assert!(contrast_ratio(d.grid, d.square) >= 1.0, "Dark grid");
        let hc = themed_style(Theme::HighContrast, Density::Comfortable, 1.0);
        // egui fades disabled text with `disable` (a premultiplied
        // gamma-multiply): over black the shown colour is the rgb.
        let disabled = hc.visuals.disable(Tokens::HIGH_CONTRAST.text);
        let on_black = Color32::from_rgb(disabled.r(), disabled.g(), disabled.b());
        assert!(contrast_ratio(on_black, Color32::BLACK) >= 7.0);
    }

    /// U4: each theme's visuals carry its own tokens and egui base.
    #[test]
    fn themed_styles_carry_their_tokens() {
        for t in Theme::ALL {
            let s = themed_style(t, Density::Comfortable, 1.0);
            let k = t.tokens();
            assert_eq!(s.visuals.panel_fill, k.panel, "{t:?}");
            assert_eq!(s.visuals.dark_mode, t.is_dark(), "{t:?}");
            assert_eq!(s.visuals.widgets.active.bg_stroke.color, k.accent);
            assert_eq!(s.visuals.widgets.inactive.fg_stroke.color, k.text);
        }
        // Dark is today's style exactly. Compared on the visuals (the
        // part the theme owns): a whole `egui::Style` never equals
        // itself, it carries a NaN (identical Debug text, unequal).
        assert_eq!(
            themed_style(Theme::Dark, Density::Comfortable, 1.0).visuals,
            umber_style(Density::Comfortable).visuals
        );
    }

    /// U4: `motion_secs` follows the style, so Reduce Motion's 0 reaches
    /// every widget that asks for it.
    #[test]
    fn motion_secs_follows_the_style_animation_time() {
        let ctx = egui::Context::default();
        let mut seen = Vec::new();
        for anim in [MOTION_SECS, 0.0] {
            ctx.all_styles_mut(|s| s.animation_time = anim);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| seen.push(motion_secs(ui)));
            out.textures_delta.clear();
        }
        assert_eq!(seen, vec![MOTION_SECS, 0.0]);
    }

    #[test]
    fn hit_zones_resolve_to_grab_widths() {
        // g7-density-scale: the defaults match egui_dock's own; the
        // tablet mode doubles the grab width.
        assert_eq!(HitZone::default(), HitZone::Standard);
        assert_eq!(HitZone::Standard.extra_interact_width(), 4.0);
        assert_eq!(HitZone::Tablet.extra_interact_width(), 8.0);
        assert_eq!(SPLITTER_HIT_ZONE, 4.0);
        assert_eq!(SPLITTER_HIT_ZONE_TABLET, 8.0);
    }
}
