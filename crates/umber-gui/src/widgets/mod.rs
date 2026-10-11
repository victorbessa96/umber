//! The four foundational widgets (docs/specs/gui-audit-and-roadmap.md §3
//! Phase 0a).

pub mod param_group;
pub mod swatch_grid;
pub mod tool_row;
pub mod value_field;

pub use param_group::{ParamGroup, ParamGroupUi};
pub use swatch_grid::{sv_hue_placeholder, RecentColors, SwatchGrid};
pub use tool_row::{Tool, ToolRow};
pub use value_field::ValueField;
