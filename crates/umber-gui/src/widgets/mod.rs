//! The four foundational widgets (docs/specs/gui-audit-and-roadmap.md §3
//! Phase 0a), plus the notice stack (docs/specs/gui-plan-2026-10-15.md
//! g0-notices), the colour ramp (g4-gradient-ramp), the curve editor
//! (g4-curve-widget) and the colour picker (g4-color-picker).

pub mod color_picker;
pub mod curve;
pub mod notices;
pub mod param_group;
pub mod ramp;
pub mod swatch_grid;
pub mod tab_overflow;
pub mod tool_row;
pub mod value_field;

pub use color_picker::{ColorPicker, ColorState};
pub use curve::{Curve, CurveEdit, CurveError};
pub use notices::{Notice, NoticeLevel, Notices};
pub use param_group::{ParamGroup, ParamGroupUi};
pub use ramp::{Interpolation, Ramp, RampEdit, Stop};
pub use swatch_grid::{sv_hue_placeholder, RecentColors, SwatchGrid};
pub use tool_row::{Tool, ToolRow};
pub use value_field::ValueField;
