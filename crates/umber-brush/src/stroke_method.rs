//! Stroke methods: how a held, moving pointer becomes dabs.
//!
//! Source: the Blender manual's Stroke panel
//! (<https://docs.blender.org/manual/en/latest/sculpt_paint/brush/stroke.html>):
//! *Space* (dabs separated by spacing), *Airbrush* (flow continues while
//! the button is held, at a *Rate*), *Drag Dot* (one dab, placed by
//! dragging), and the *Dash Ratio* / *Dash Length* pair (the brush is on
//! for a ratio of each dash cycle).
//!
//! [`StrokeMethod`] is the persisted setting; [`MethodState`] is the
//! per-stroke runtime state the [`StrokeConditioner`](crate::StrokeConditioner)
//! carries. The conditioner always runs spacing (so its anchor and residual
//! evolve exactly as before) and hands the spaced dabs to
//! [`MethodState::apply`], which passes them through ([`StrokeMethod::Space`]),
//! adds time-based dabs (Airbrush), filters them (Dash) or withholds them
//! for [`MethodState::finish`] (Drag Dot).
//!
//! Judgment calls (v1 boundaries):
//! - **Space is a literal pass-through**: `apply` returns the spacing output
//!   untouched and mutates no state, so the default is bit-identical to the
//!   pre-method pipeline (pinned by a reference-pipeline test in `wiring`).
//! - **Airbrush = "time since the last dab"**: the rate accumulator banks
//!   `elapsed_s * rate_hz` per event and emits one dab at the current cursor
//!   per whole unit; any spacing dab in the same push drains the bank. So a
//!   moving stroke is spaced exactly like Space and the spray only adds dabs
//!   while the pointer dwells (Blender sprays on top of motion too; we
//!   chose not to double the density of moving strokes). Emissions per
//!   push are capped at [`AIRBRUSH_MAX_DABS_PER_PUSH`]; the backlog beyond
//!   the cap is dropped, never replayed, so a huge time jump (a stalled
//!   event loop, a resumed laptop) cannot flood the compositor. Time going
//!   backwards counts as zero elapsed.
//! - **Dash length is in pixels of path**, not Blender's "stroke samples":
//!   the brief asks for a size-independent dash, and pixels keep the dash
//!   pattern stable when the radius (and so the dab step) changes. The
//!   phase is arc length along the emitted dab chain, measured from the
//!   stroke's seed point, carried across events like spacing's residual.
//!   A dab is kept iff `phase mod length_px < ratio * length_px`. The phase
//!   is quantized to 1/256 px before the test so dabs that land exactly on
//!   a dash boundary (the common case for integer spacings) do not flicker
//!   on float noise.
//! - **Drag Dot**: during the stroke nothing is emitted; the last cursor
//!   (post smoothing / line constraint) and its pressure are held, and
//!   `finish` emits exactly that one dab, once. A click without motion
//!   still yields a dot (the seed sets the pending dab).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::spacing::DabPlan;

/// Hard cap on airbrush dabs emitted by one event push.
pub const AIRBRUSH_MAX_DABS_PER_PUSH: usize = 64;

/// Dash-phase quantization (1/256 px), see the module docs.
const DASH_PHASE_QUANTUM: f64 = 256.0;

/// Validation errors for [`StrokeMethod`] parameters.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum StrokeMethodError {
    /// Airbrush rate must be finite and > 0.
    #[error("airbrush rate_hz must be finite and > 0, got {0}")]
    InvalidRate(f32),
    /// Dash ratio must be finite and in (0, 1].
    #[error("dash ratio must be in (0, 1], got {0}")]
    InvalidDashRatio(f32),
    /// Dash length must be finite and > 0.
    #[error("dash length_px must be finite and > 0, got {0}")]
    InvalidDashLength(f32),
}

/// How the stroke turns pointer motion into dabs.
///
/// Serde shape: internally tagged by `kind`, snake_case variant names —
/// `{"kind":"space"}`, `{"kind":"airbrush","rate_hz":10.0}`,
/// `{"kind":"dash","ratio":0.5,"length_px":20.0}`, `{"kind":"drag_dot"}`.
/// Internal tagging keeps a flat, self-describing object that reads well
/// inside a brush preset. Deserialization validates (via
/// [`StrokeMethod::validate`]), so an out-of-range file is an error, never
/// a live invalid method.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(try_from = "RawStrokeMethod", into = "RawStrokeMethod")]
pub enum StrokeMethod {
    /// Dabs separated by the brush spacing (today's behavior).
    #[default]
    Space,
    /// Spacing while moving, plus `rate_hz` dabs per second at the cursor
    /// while the pointer dwells.
    Airbrush {
        /// Dabs per second while held still.
        rate_hz: f32,
    },
    /// Spacing, but only the first `ratio` of every `length_px` of path
    /// paints.
    Dash {
        /// On fraction of each cycle, in (0, 1].
        ratio: f32,
        /// Cycle length in pixels of path.
        length_px: f32,
    },
    /// One dab at the final cursor position, emitted on stroke end.
    DragDot,
}

/// The wire shape (unvalidated); `StrokeMethod` converts through it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum RawStrokeMethod {
    Space,
    Airbrush { rate_hz: f32 },
    Dash { ratio: f32, length_px: f32 },
    DragDot,
}

impl TryFrom<RawStrokeMethod> for StrokeMethod {
    type Error = StrokeMethodError;

    fn try_from(raw: RawStrokeMethod) -> Result<Self, Self::Error> {
        let method = match raw {
            RawStrokeMethod::Space => Self::Space,
            RawStrokeMethod::Airbrush { rate_hz } => Self::Airbrush { rate_hz },
            RawStrokeMethod::Dash { ratio, length_px } => Self::Dash { ratio, length_px },
            RawStrokeMethod::DragDot => Self::DragDot,
        };
        method.validate()?;
        Ok(method)
    }
}

impl From<StrokeMethod> for RawStrokeMethod {
    fn from(method: StrokeMethod) -> Self {
        match method {
            StrokeMethod::Space => Self::Space,
            StrokeMethod::Airbrush { rate_hz } => Self::Airbrush { rate_hz },
            StrokeMethod::Dash { ratio, length_px } => Self::Dash { ratio, length_px },
            StrokeMethod::DragDot => Self::DragDot,
        }
    }
}

impl StrokeMethod {
    /// Check the parameters.
    ///
    /// # Errors
    ///
    /// [`StrokeMethodError`] for a non-finite or non-positive airbrush
    /// rate, a dash ratio outside (0, 1], or a non-positive dash length.
    pub fn validate(&self) -> Result<(), StrokeMethodError> {
        match *self {
            Self::Space | Self::DragDot => Ok(()),
            Self::Airbrush { rate_hz } => {
                if !rate_hz.is_finite() || rate_hz <= 0.0 {
                    Err(StrokeMethodError::InvalidRate(rate_hz))
                } else {
                    Ok(())
                }
            }
            Self::Dash { ratio, length_px } => {
                if !ratio.is_finite() || ratio <= 0.0 || ratio > 1.0 {
                    Err(StrokeMethodError::InvalidDashRatio(ratio))
                } else if !length_px.is_finite() || length_px <= 0.0 {
                    Err(StrokeMethodError::InvalidDashLength(length_px))
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Per-stroke runtime state for a [`StrokeMethod`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MethodState {
    /// Timestamp of the previous event (airbrush clock).
    last_time_ns: u64,
    /// Banked airbrush dabs (fractional), in units of dabs.
    airbrush_bank: f64,
    /// Arc length (px) along the dab chain since the seed (dash phase).
    dash_phase: f64,
    /// The previous dab (or the seed point) the dash phase measures from.
    dash_prev: [f32; 2],
    /// Drag Dot's withheld dab.
    pending_dot: Option<DabPlan>,
}

impl MethodState {
    /// Drop all per-stroke state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The stroke's seeding event at `cursor`.
    pub fn seed(&mut self, method: &StrokeMethod, cursor: [f32; 2], pressure: f32, time_ns: u64) {
        match method {
            StrokeMethod::Space => {}
            StrokeMethod::Airbrush { .. } => {
                self.last_time_ns = time_ns;
                self.airbrush_bank = 0.0;
            }
            StrokeMethod::Dash { .. } => {
                self.dash_phase = 0.0;
                self.dash_prev = cursor;
            }
            StrokeMethod::DragDot => {
                self.pending_dot = Some(DabPlan {
                    pos: cursor,
                    alpha_multiplier: pressure,
                });
            }
        }
    }

    /// Transform one push's spaced dabs per the method. `cursor` /
    /// `pressure` are the conditioned cursor and pressure of this event.
    pub fn apply(
        &mut self,
        method: &StrokeMethod,
        spaced: Vec<DabPlan>,
        cursor: [f32; 2],
        pressure: f32,
        time_ns: u64,
    ) -> Vec<DabPlan> {
        match *method {
            StrokeMethod::Space => spaced,
            StrokeMethod::Airbrush { rate_hz } => {
                self.airbrush(spaced, rate_hz, cursor, pressure, time_ns)
            }
            StrokeMethod::Dash { ratio, length_px } => self.dash(spaced, ratio, length_px),
            StrokeMethod::DragDot => {
                self.pending_dot = Some(DabPlan {
                    pos: cursor,
                    alpha_multiplier: pressure,
                });
                Vec::new()
            }
        }
    }

    /// Stroke end: Drag Dot's single dab (once); empty for other methods.
    pub fn finish(&mut self, method: &StrokeMethod) -> Vec<DabPlan> {
        match method {
            StrokeMethod::DragDot => self.pending_dot.take().into_iter().collect(),
            _ => Vec::new(),
        }
    }

    fn airbrush(
        &mut self,
        mut spaced: Vec<DabPlan>,
        rate_hz: f32,
        cursor: [f32; 2],
        pressure: f32,
        time_ns: u64,
    ) -> Vec<DabPlan> {
        let elapsed_ns = time_ns.saturating_sub(self.last_time_ns);
        self.last_time_ns = time_ns;
        if !spaced.is_empty() {
            // A spacing dab just landed: the spray clock restarts.
            self.airbrush_bank = 0.0;
            return spaced;
        }
        if !rate_hz.is_finite() || rate_hz <= 0.0 {
            return spaced;
        }
        // f64 keeps `ns * rate / 1e9` exact for ordinary event cadences
        // (50 ms at 10 Hz banks exactly 0.5).
        self.airbrush_bank += elapsed_ns as f64 * f64::from(rate_hz) / 1e9;
        if !self.airbrush_bank.is_finite() {
            self.airbrush_bank = 0.0;
            return spaced;
        }
        let whole = self.airbrush_bank.floor();
        self.airbrush_bank -= whole;
        // `whole` is finite and >= 0; the cap bounds the loop and drops
        // the backlog beyond it.
        let n = if whole >= AIRBRUSH_MAX_DABS_PER_PUSH as f64 {
            AIRBRUSH_MAX_DABS_PER_PUSH
        } else {
            whole as usize
        };
        spaced.extend((0..n).map(|_| DabPlan {
            pos: cursor,
            alpha_multiplier: pressure,
        }));
        spaced
    }

    fn dash(&mut self, spaced: Vec<DabPlan>, ratio: f32, length_px: f32) -> Vec<DabPlan> {
        let length = f64::from(length_px);
        let on = f64::from(ratio) * length;
        let mut out = Vec::with_capacity(spaced.len());
        for dab in spaced {
            let dx = f64::from(dab.pos[0]) - f64::from(self.dash_prev[0]);
            let dy = f64::from(dab.pos[1]) - f64::from(self.dash_prev[1]);
            let step = (dx * dx + dy * dy).sqrt();
            if step.is_finite() {
                self.dash_phase += step;
            }
            self.dash_prev = dab.pos;
            let phase = (self.dash_phase * DASH_PHASE_QUANTUM).round() / DASH_PHASE_QUANTUM;
            if !length.is_finite() || length <= 0.0 || phase.rem_euclid(length) < on {
                out.push(dab);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_space() {
        assert_eq!(StrokeMethod::default(), StrokeMethod::Space);
    }

    #[test]
    fn validation_rejects_bad_params() {
        use StrokeMethodError as E;
        let bad = [
            (StrokeMethod::Airbrush { rate_hz: 0.0 }, E::InvalidRate(0.0)),
            (
                StrokeMethod::Airbrush { rate_hz: -1.0 },
                E::InvalidRate(-1.0),
            ),
            (
                StrokeMethod::Airbrush {
                    rate_hz: f32::INFINITY,
                },
                E::InvalidRate(f32::INFINITY),
            ),
            (
                StrokeMethod::Dash {
                    ratio: 0.0,
                    length_px: 20.0,
                },
                E::InvalidDashRatio(0.0),
            ),
            (
                StrokeMethod::Dash {
                    ratio: 1.5,
                    length_px: 20.0,
                },
                E::InvalidDashRatio(1.5),
            ),
            (
                StrokeMethod::Dash {
                    ratio: 0.5,
                    length_px: 0.0,
                },
                E::InvalidDashLength(0.0),
            ),
            (
                StrokeMethod::Dash {
                    ratio: 0.5,
                    length_px: -3.0,
                },
                E::InvalidDashLength(-3.0),
            ),
        ];
        for (method, want) in bad {
            assert_eq!(method.validate(), Err(want), "{method:?}");
        }
        assert!(matches!(
            StrokeMethod::Airbrush { rate_hz: f32::NAN }.validate(),
            Err(E::InvalidRate(_))
        ));
        let good = [
            StrokeMethod::Space,
            StrokeMethod::DragDot,
            StrokeMethod::Airbrush { rate_hz: 10.0 },
            StrokeMethod::Dash {
                ratio: 1.0,
                length_px: 20.0,
            },
            StrokeMethod::Dash {
                ratio: 0.01,
                length_px: 0.5,
            },
        ];
        for method in good {
            assert_eq!(method.validate(), Ok(()), "{method:?}");
        }
    }

    #[test]
    fn serde_round_trip_and_wire_shape() {
        let cases = [
            (StrokeMethod::Space, r#"{"kind":"space"}"#),
            (
                StrokeMethod::Airbrush { rate_hz: 10.0 },
                r#"{"kind":"airbrush","rate_hz":10.0}"#,
            ),
            (
                StrokeMethod::Dash {
                    ratio: 0.5,
                    length_px: 20.0,
                },
                r#"{"kind":"dash","ratio":0.5,"length_px":20.0}"#,
            ),
            (StrokeMethod::DragDot, r#"{"kind":"drag_dot"}"#),
        ];
        for (method, json) in cases {
            assert_eq!(serde_json::to_string(&method).expect("serialize"), json);
            let back: StrokeMethod = serde_json::from_str(json).expect("deserialize");
            assert_eq!(back, method);
        }
    }

    #[test]
    fn deserialize_validates() {
        assert!(
            serde_json::from_str::<StrokeMethod>(r#"{"kind":"airbrush","rate_hz":0.0}"#).is_err()
        );
        assert!(serde_json::from_str::<StrokeMethod>(
            r#"{"kind":"dash","ratio":2.0,"length_px":20.0}"#
        )
        .is_err());
        assert!(serde_json::from_str::<StrokeMethod>(r#"{"kind":"spray"}"#).is_err());
    }

    fn dab(x: f32) -> DabPlan {
        DabPlan {
            pos: [x, 0.0],
            alpha_multiplier: 1.0,
        }
    }

    #[test]
    fn space_is_a_pass_through_that_touches_no_state() {
        let mut s = MethodState::default();
        s.seed(&StrokeMethod::Space, [1.0, 2.0], 0.5, 7);
        let spaced = vec![dab(1.0), dab(2.0)];
        let out = s.apply(&StrokeMethod::Space, spaced.clone(), [3.0, 0.0], 1.0, 99);
        assert_eq!(out, spaced);
        assert_eq!(s, MethodState::default());
        assert!(s.finish(&StrokeMethod::Space).is_empty());
    }

    #[test]
    fn airbrush_cap_drops_the_backlog() {
        let m = StrokeMethod::Airbrush { rate_hz: 1000.0 };
        let mut s = MethodState::default();
        s.seed(&m, [0.0, 0.0], 1.0, 0);
        // 10 s at 1 kHz = 10 000 owed; the cap emits 64 and forgets the rest.
        let out = s.apply(&m, Vec::new(), [0.0, 0.0], 1.0, 10_000_000_000);
        assert_eq!(out.len(), AIRBRUSH_MAX_DABS_PER_PUSH);
        // Backlog gone: 0.5 ms more banks 0.5, so nothing fires.
        assert!(s
            .apply(&m, Vec::new(), [0.0, 0.0], 1.0, 10_000_500_000)
            .is_empty());
        // Time going backwards is zero elapsed, not a u64 wrap.
        assert!(s.apply(&m, Vec::new(), [0.0, 0.0], 1.0, 5).is_empty());
    }

    #[test]
    fn dash_ratio_one_keeps_everything() {
        let m = StrokeMethod::Dash {
            ratio: 1.0,
            length_px: 7.0,
        };
        let mut s = MethodState::default();
        s.seed(&m, [0.0, 0.0], 1.0, 0);
        let spaced: Vec<DabPlan> = (1..=20).map(|i| dab(i as f32 * 3.0)).collect();
        assert_eq!(s.apply(&m, spaced.clone(), [60.0, 0.0], 1.0, 0), spaced);
    }
}
