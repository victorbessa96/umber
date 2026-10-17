//! umber-brush — the brush engine.
//!
//! Wave 2 scope (docs/research/05): a libmypaint-documented-semantics state
//! machine (inputs→curves→dab params, residual partial-dab spacing,
//! dabs-per-radius), one-euro + lazy-mouse stroke conditioning, and the
//! dab batcher feeding umber-gpu compute dispatches.
//!
//! Wave 1 scope: the input-event vocabulary every layer agrees on, so the
//! stylus crate and the paint scheduler can compile against it.

use serde::{Deserialize, Serialize};

/// A normalized stylus event, backend-agnostic.
/// Produced by the `stylus` crate; consumed by the stroke conditioner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeEvent {
    /// Screen-space position in logical pixels.
    pub pos: [f32; 2],
    /// Pressure, normalized 0..=1.
    pub pressure: f32,
    /// Tilt in radians, if the device reports it.
    pub tilt: Option<[f32; 2]>,
    /// Monotonic timestamp (nanoseconds since stroke start).
    pub time_ns: u64,
    /// True while the pen is in range (in-proximity).
    pub in_proximity: bool,
    /// True while the pen tip contact is down.
    pub contact: bool,
}

pub mod cavity;
pub mod dab_adapter;
pub mod falloff;
pub mod id_mask;
pub mod lazy_mouse;
pub mod line_constraint;
pub mod one_euro;
pub mod path;
pub mod preset;
pub mod preset_library;
pub mod quick_mask;
pub mod retouch;
pub mod spacing;
pub mod stroke_method;
pub mod symmetry;
pub mod wiring;

pub use cavity::{CavityMask, CavityMaskError, CavityTarget};
pub use dab_adapter::{
    opaque_linearize, wash_scale, BrushParams, DabAdapter, Jitter, StampAngle, WashMode,
};
pub use falloff::FalloffCurve;
pub use id_mask::{select_id, SelectIdError};
pub use lazy_mouse::LazyMouse;
pub use line_constraint::{
    constrain_point, sanitize_snap_deg, LineConstraint, LineConstraintMode, ANGLE_SNAP_DEFAULT_DEG,
    ANGLE_SNAP_MAX_DEG, ANGLE_SNAP_MIN_DEG,
};
pub use one_euro::{OneEuroError, OneEuroFilter, OneEuroScalar};
pub use path::{
    sanitize_size, PaintPath, PathDab, PathOp, PathPoint, PATH_EVENT_STEP_NS, PATH_MAX_POINTS,
    PATH_MAX_SIZE, PATH_MIN_SIZE,
};
pub use preset_library::{Library, LibraryEntry};
pub use quick_mask::{MaskPaint, MaskView, QuickMask};
pub use retouch::{
    BrushTool, CloneParams, EraseChannels, EraseParams, InpaintParams, RgbaBuffer, SmudgeParams,
};
pub use spacing::{DabPlan, SpacingAccumulator, SpacingError};
pub use stroke_method::{MethodState, StrokeMethod, StrokeMethodError, AIRBRUSH_MAX_DABS_PER_PUSH};
pub use symmetry::{MirrorAxis, MirrorSettings, RadialSettings, SymTransform, SymmetrySettings};
pub use wiring::{ConditionerError, StrokeConditioner};

/// One-euro filter parameters (Casiez CHI'12 — docs/research/05 §4).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OneEuroParams {
    pub min_cutoff: f32,
    pub beta: f32,
    pub d_cutoff: f32,
}

impl Default for OneEuroParams {
    fn default() -> Self {
        // Starting points; tuned during Wave 2 against real tablets.
        Self {
            min_cutoff: 1.0,
            beta: 0.007,
            d_cutoff: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_event_vocabulary_compiles() {
        let e = StrokeEvent {
            pos: [100.0, 200.0],
            pressure: 0.5,
            tilt: Some([0.1, 0.2]),
            time_ns: 16_000_000,
            in_proximity: true,
            contact: true,
        };
        assert!(e.contact);
        assert_eq!(OneEuroParams::default().beta, 0.007);
    }
}
