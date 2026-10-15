//! umber-gui — the presentation layer: theme tokens, fonts, the icon
//! atlas, and the foundational widgets.
//!
//! Phase 0a of docs/specs/gui-audit-and-roadmap.md (Part 3). The design
//! language is decided in Part 2:
//! - [`theme`]: the Umber palette (§2.1) → [`umber_style`], plus the
//!   density scale (§2.5) and the 120ms motion constant (§2.4).
//! - [`fonts`]: Inter + JetBrains Mono (§2.2).
//! - [`icons`]: the lucide atlas model + `icon` / `icon_button` (§2.3).
//! - [`widgets`]: `param_group`, `value_field`, `tool_row`, `swatch_grid`,
//!   and the `notices` toast stack.
//!
//! The app wiring (apply the style + fonts at boot, migrate the Brush
//! panel) is a separate step. This crate depends on egui alone. It never
//! touches engine state, and panels pass their values in by `&mut`.

#![warn(missing_docs)]

pub mod fonts;
pub mod icons;
pub mod theme;
pub mod widgets;

pub use fonts::{install_fonts, umber_fonts};
#[cfg(feature = "embedded-icons")]
pub use icons::load_embedded_atlas;
pub use icons::{
    assemble_baked_atlas, atlas_dir, atlas_file_name, icon, icon_button, load_baked_atlas,
    load_baked_atlas_from, manifest_key, texture_name, AtlasBuilder, AtlasLoadError, BakedFamily,
    BakedManifest, IconAtlas, IconFamily, ATLAS_DIR_ENV, ATLAS_DIR_PATH, ATLAS_VERSION,
    LUCIDE_VERSION, MANIFEST_FILE,
};
pub use theme::apply;
pub use theme::{
    accent, accent_hover, hover, muted, panel_bg, umber_style, umber_style_scaled, Density,
    MOTION_SECS,
};
pub use widgets::{
    Notice, NoticeLevel, Notices, ParamGroup, RecentColors, SwatchGrid, Tool, ToolRow, ValueField,
};
