//! Pipeline wiring: filter → lazy mouse → line constraint → spacing.
//!
//! [`StrokeConditioner`] chains the conditioning stages for one
//! stroke: each [`StrokeEvent`](crate::StrokeEvent) is one-euro filtered
//! (position plus a separate pressure instance), pulled through
//! [`LazyMouse`](crate::LazyMouse), projected through the
//! [`LineConstraint`](crate::LineConstraint) (off by default —
//! byte-identical to no constraint), then expanded into
//! [`DabPlan`](crate::DabPlan)s by [`SpacingAccumulator`](crate::SpacingAccumulator).
//! Dab-to-UV projection and GPU compositing are later slices; this stage only
//! decides *where* dabs land and how strong they are.
//!
//! Deterministic: the same event sequence through equally constructed
//! conditioners yields identical dab plans.
//!
//! Stroke methods ([`StrokeMethod`], `stroke_method.rs`) are a field here,
//! not a wrapper type: Airbrush needs the conditioned cursor + pressure and
//! the event clock, Drag Dot needs the last conditioned cursor, and Dash
//! needs the spaced dabs — all of which only exist inside the private
//! `advance` step. A wrapper would have to re-expose that internal state
//! or re-derive it. Spacing always runs and the anchor always follows the
//! *spaced* dabs, so the method only post-filters; [`StrokeMethod::Space`]
//! returns the spacing output untouched (bit-identical to the pre-method
//! pipeline).

use thiserror::Error;

use crate::{
    line_constraint::{LineConstraint, LineConstraintMode},
    one_euro::OneEuroError,
    spacing::DabPlan,
    stroke_method::{MethodState, StrokeMethod, StrokeMethodError},
    LazyMouse, OneEuroFilter, OneEuroParams, OneEuroScalar, SpacingAccumulator, SpacingError,
    StrokeEvent,
};

/// Construction errors for [`StrokeConditioner`].
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum ConditionerError {
    /// A one-euro parameter set was invalid.
    #[error(transparent)]
    Filter(#[from] OneEuroError),
    /// The dab density was invalid.
    #[error(transparent)]
    Spacing(#[from] SpacingError),
    /// The stroke-method parameters were invalid.
    #[error(transparent)]
    Method(#[from] StrokeMethodError),
}

/// One stroke's conditioning pipeline.
///
/// Created per stroke (or [`reset`](Self::reset) between strokes so no
/// residual partial dab leaks across strokes). The first event passed to
/// [`push`](Self::push) seeds the pipeline and emits no dabs; every later
/// event emits the dabs for the smoothed segment since the previous event.
#[derive(Debug, Clone)]
pub struct StrokeConditioner {
    pos_filter: OneEuroFilter,
    pressure_filter: OneEuroScalar,
    lazy: LazyMouse,
    line: LineConstraint,
    spacing: SpacingAccumulator,
    cursor: [f32; 2],
    anchor: [f32; 2],
    anchor_pressure: f32,
    live: bool,
    /// Exact mode: the one-euro filters and the lazy-mouse leash are
    /// bypassed, so the cursor IS the event position. For input that is
    /// already intended geometry (agent / scripted strokes, path replays):
    /// a 1 Hz one-euro over synthetic 8 ms events moves the cursor ~10% of
    /// the way per event, so a 2-point stroke never travels one dab
    /// spacing and paints NOTHING. Spacing and the line constraint still
    /// run — dab density is the brush's, not the input's.
    exact: bool,
    /// How spaced dabs become emitted dabs (default: Space = pass-through).
    method: StrokeMethod,
    /// Per-stroke state of `method`.
    method_state: MethodState,
}

impl StrokeConditioner {
    /// Build a pipeline: `pos_params` for positions, `pressure_params` for
    /// the separate pressure channel, `lazy_radius_px` for the pull-cursor
    /// leash (`0.0` disables it), `dabs_per_radius` for dab density.
    ///
    /// # Errors
    ///
    /// Returns [`ConditionerError`] when a parameter set is invalid. The
    /// lazy radius is sanitized, never rejected (see
    /// [`LazyMouse::new`](crate::LazyMouse::new)).
    pub fn new(
        pos_params: OneEuroParams,
        pressure_params: OneEuroParams,
        lazy_radius_px: f32,
        dabs_per_radius: f32,
    ) -> Result<Self, ConditionerError> {
        Ok(Self {
            pos_filter: OneEuroFilter::try_new(pos_params)?,
            pressure_filter: OneEuroScalar::try_new(pressure_params)?,
            lazy: LazyMouse::new(lazy_radius_px),
            line: LineConstraint::default(),
            spacing: SpacingAccumulator::new(dabs_per_radius)?,
            cursor: [0.0, 0.0],
            anchor: [0.0, 0.0],
            anchor_pressure: 0.0,
            exact: false,
            live: false,
            method: StrokeMethod::Space,
            method_state: MethodState::default(),
        })
    }

    /// The live spacing density (dabs per radius of travel). The wash-mode
    /// normalizer reads this as the stroke's mean overlap count, so the
    /// linearization always matches the density that actually spaced the
    /// stroke's dabs — even though that density is fixed at construction
    /// today (preset spacing sync is a later slice).
    #[must_use]
    pub fn dabs_per_radius(&self) -> f32 {
        self.spacing.dabs_per_radius
    }

    /// Drop all per-stroke state (filters, cursor, line anchor, banked
    /// partial dab).
    pub fn reset(&mut self) {
        self.pos_filter.reset();
        self.pressure_filter.reset();
        self.line.reset();
        self.spacing.reset();
        self.cursor = [0.0, 0.0];
        self.anchor = [0.0, 0.0];
        self.anchor_pressure = 0.0;
        self.live = false;
        self.method_state.reset();
    }

    /// Feed one stylus event; returns the dabs for the smoothed segment
    /// since the previous event (empty for the seeding event).
    ///
    /// `radius_px` is the brush's current radius in the same units as the
    /// event positions; degenerate values emit no dabs but still advance
    /// the smoother state. Pressure outside `0..=1` is clamped.
    pub fn push(&mut self, event: &StrokeEvent, radius_px: f32) -> Vec<DabPlan> {
        if self.exact {
            self.cursor = event.pos;
            let pressure = saturate01(event.pressure);
            return self.advance(pressure, radius_px, event.time_ns);
        }
        let pos = self.pos_filter.filter(event.pos, event.time_ns);
        let pressure = self
            .pressure_filter
            .filter(saturate01(event.pressure), event.time_ns);
        self.cursor = self.lazy.apply(pos, self.cursor);
        self.advance(pressure, radius_px, event.time_ns)
    }

    /// End the stroke's method: Drag Dot's single dab at the last
    /// conditioned cursor (emitted once); empty for every other method.
    /// Does not reset — call [`Self::reset`] after, as at any stroke end.
    pub fn finish(&mut self) -> Vec<DabPlan> {
        self.method_state.finish(&self.method)
    }

    /// The shared tail of [`Self::push`]: line constraint, seeding, then
    /// spacing from the anchor to the (filtered or exact) cursor, then the
    /// stroke method over the spaced dabs.
    fn advance(&mut self, pressure: f32, radius_px: f32, time_ns: u64) -> Vec<DabPlan> {
        // Straight-line constraint / angle snap (§2 line 23): the smoothed
        // cursor projects onto the anchored line before spacing sees it.
        // Off (the default) is the identity, so spacing input is unchanged.
        self.cursor = self.line.apply(self.cursor);
        if !self.live {
            self.anchor = self.cursor;
            self.anchor_pressure = pressure;
            self.live = true;
            self.method_state
                .seed(&self.method, self.cursor, pressure, time_ns);
            return Vec::new();
        }
        let dabs = self.spacing.plan_dabs(
            self.anchor,
            self.cursor,
            radius_px,
            [self.anchor_pressure, pressure],
        );
        if let Some(last) = dabs.last() {
            self.anchor = last.pos;
            self.anchor_pressure = last.alpha_multiplier;
        }
        self.method_state
            .apply(&self.method, dabs, self.cursor, pressure, time_ns)
    }

    /// Switch the stroke method. Call between strokes; the method's
    /// per-stroke state is dropped (filters and spacing are untouched).
    ///
    /// # Errors
    ///
    /// [`ConditionerError::Method`] when the parameters are invalid; the
    /// current method is kept.
    pub fn set_method(&mut self, method: StrokeMethod) -> Result<(), ConditionerError> {
        method.validate()?;
        self.method = method;
        self.method_state.reset();
        Ok(())
    }

    /// The current stroke method.
    #[must_use]
    pub fn method(&self) -> StrokeMethod {
        self.method
    }

    /// Whether the pipeline has been seeded by at least one event.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Exact mode on/off (see the `exact` field): bypasses smoothing for
    /// already-intended geometry. Call between strokes; resets the
    /// pipeline so no filter state carries across the switch.
    pub fn set_exact(&mut self, exact: bool) {
        if self.exact != exact {
            self.exact = exact;
            self.reset();
        }
    }

    /// Whether exact mode is on.
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.exact
    }

    /// Switch the straight-line / angle-snap mode. Takes effect on the
    /// next event (the paint session only calls this between strokes, so
    /// one stroke never splits across two modes); the anchor is kept.
    pub fn set_line_constraint(&mut self, mode: LineConstraintMode) {
        self.line.set_mode(mode);
    }

    /// The current straight-line / angle-snap mode.
    #[must_use]
    pub fn line_constraint(&self) -> LineConstraintMode {
        self.line.mode()
    }

    /// The constraint's stroke anchor (the first stamp), if set.
    #[must_use]
    pub fn line_anchor(&self) -> Option<[f32; 2]> {
        self.line.anchor()
    }
}

/// Finite values clamp to `0..=1`; non-finite values become `0.0`.
fn saturate01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn event(x: f32, pressure: f32, time_ns: u64) -> StrokeEvent {
        StrokeEvent {
            pos: [x, 0.0],
            pressure,
            tilt: None,
            time_ns,
            in_proximity: true,
            contact: true,
        }
    }

    fn conditioner() -> StrokeConditioner {
        StrokeConditioner::new(OneEuroParams::default(), OneEuroParams::default(), 0.0, 2.0)
            .expect("valid params")
    }

    /// The finding this mode exists for: a 2-event stroke 50 px long under
    /// the default 1 Hz one-euro at 8 ms steps barely moves the filtered
    /// cursor, so it emits NO dabs at a 10 px radius / 2 dabs-per-radius
    /// (5 px spacing). Exact mode emits the full 10.
    #[test]
    fn exact_mode_paints_a_short_stroke_the_filter_swallows() {
        let mut smoothed = conditioner();
        smoothed.push(&event(0.0, 1.0, 0), 10.0);
        let swallowed = smoothed.push(&event(50.0, 1.0, 8_000_000), 10.0);
        assert!(
            swallowed.len() < 2,
            "the filter should swallow most of a 2-event stroke (got {})",
            swallowed.len()
        );

        let mut exact = conditioner();
        exact.set_exact(true);
        assert!(exact.is_exact());
        assert!(exact.push(&event(0.0, 1.0, 0), 10.0).is_empty(), "seeding");
        let dabs = exact.push(&event(50.0, 1.0, 8_000_000), 10.0);
        // 50 px / (10 px / 2 dabs-per-radius) = 10 dabs, the last ON the end.
        assert_eq!(dabs.len(), 10);
        let last = dabs.last().expect("dabs");
        assert!((last.pos[0] - 50.0).abs() < EPS && last.pos[1].abs() < EPS);
        assert!((dabs[0].pos[0] - 5.0).abs() < EPS);
    }

    /// Exact mode passes pressure through unfiltered (clamped only), and
    /// the switch resets so no filter state leaks across it.
    #[test]
    fn exact_mode_is_unfiltered_and_the_switch_resets() {
        let mut c = conditioner();
        c.set_exact(true);
        c.push(&event(0.0, 0.2, 0), 10.0);
        let dabs = c.push(&event(10.0, 0.8, 8_000_000), 10.0);
        let last = dabs.last().expect("dabs");
        // Pressure interpolates linearly 0.2 -> 0.8 along the segment;
        // the last dab sits on the end point.
        assert!(
            (last.alpha_multiplier - 0.8).abs() < EPS,
            "{}",
            last.alpha_multiplier
        );
        // Out-of-range pressure is clamped, never passed through raw.
        let more = c.push(&event(20.0, 7.0, 16_000_000), 10.0);
        assert!(more.iter().all(|d| d.alpha_multiplier <= 1.0));

        assert!(c.is_live());
        c.set_exact(false);
        assert!(!c.is_live(), "switching modes resets the pipeline");
        c.set_exact(false);
        assert!(!c.is_exact());
    }

    #[test]
    fn straight_stroke_emits_dabs_after_seeding() {
        let mut c = conditioner();
        let mut total = 0;
        for i in 0..20 {
            let dabs = c.push(&event(i as f32 * 5.0, 1.0, i as u64 * 8_000_000), 10.0);
            if i == 0 {
                assert!(dabs.is_empty(), "seeding event emits nothing");
            }
            total += dabs.len();
        }
        assert!(total > 0, "a 95 px stroke must emit dabs");
    }

    #[test]
    fn identical_event_sequences_are_identical() {
        let seq: Vec<StrokeEvent> = (0..30)
            .map(|i| {
                event(
                    i as f32 * 3.0 + (i % 2) as f32,
                    0.4 + i as f32 * 0.01,
                    i as u64 * 4_000_000,
                )
            })
            .collect();
        let mut a = conditioner();
        let mut b = conditioner();
        for ev in &seq {
            let da = a.push(ev, 8.0);
            let db = b.push(ev, 8.0);
            assert_eq!(da.len(), db.len());
            for (x, y) in da.iter().zip(db.iter()) {
                assert!((x.pos[0] - y.pos[0]).abs() <= EPS);
                assert!((x.pos[1] - y.pos[1]).abs() <= EPS);
                assert!((x.alpha_multiplier - y.alpha_multiplier).abs() <= EPS);
            }
        }
    }

    #[test]
    fn reset_starts_a_clean_stroke() {
        let mut c = conditioner();
        for i in 0..10 {
            c.push(&event(i as f32 * 5.0, 1.0, i as u64 * 8_000_000), 10.0);
        }
        assert!(c.is_live());
        c.reset();
        assert!(!c.is_live());
        let dabs = c.push(&event(0.0, 1.0, 0), 10.0);
        assert!(dabs.is_empty(), "post-reset event re-seeds");
    }

    #[test]
    fn line_constraint_off_is_byte_identical() {
        // Off (default vs. explicitly set) adds no arithmetic: every dab
        // of the same 2D event sequence must compare bitwise-equal, and
        // the filter must not even seed its anchor.
        let seq: Vec<StrokeEvent> = (0..30)
            .map(|i| StrokeEvent {
                pos: [i as f32 * 3.0 + (i % 2) as f32, (i % 3) as f32 * 1.5],
                pressure: 0.4 + i as f32 * 0.01,
                tilt: None,
                time_ns: i as u64 * 4_000_000,
                in_proximity: true,
                contact: true,
            })
            .collect();
        let mut a = conditioner();
        let mut b = conditioner();
        b.set_line_constraint(LineConstraintMode::Off);
        for ev in &seq {
            assert_eq!(a.push(ev, 8.0), b.push(ev, 8.0));
        }
        assert_eq!(a.line_anchor(), None, "off must never anchor");
        assert_eq!(b.line_anchor(), None, "off must never anchor");
    }

    #[test]
    fn line_anchor_resets_with_the_stroke() {
        let mut c = conditioner();
        c.set_line_constraint(LineConstraintMode::AngleSnap { snap_deg: 15.0 });
        for i in 0..10 {
            c.push(&event(i as f32 * 5.0, 1.0, i as u64 * 8_000_000), 10.0);
        }
        assert!(
            c.line_anchor().is_some(),
            "a stroke must anchor the constraint"
        );
        c.reset();
        assert_eq!(c.line_anchor(), None, "stroke end must drop the anchor");
        assert_eq!(
            c.line_constraint(),
            LineConstraintMode::AngleSnap { snap_deg: 15.0 },
            "reset keeps the mode, only the anchor goes"
        );
    }

    #[test]
    fn angle_snap_pins_dabs_to_the_snapped_ray() {
        // A gentle diagonal drift (~6°) under a 90° snap: every dab must
        // sit on the anchor's horizontal ray, not on the drifting raw
        // path. The drift stays far below 45° even through the filter's
        // startup transient (a first-order lag only ever shrinks the
        // slope ratio toward the raw 1:10), so the snapped ray is always
        // 0° — never ±90°.
        let mut c = conditioner();
        c.set_line_constraint(LineConstraintMode::AngleSnap { snap_deg: 90.0 });
        let mut dabs = Vec::new();
        for i in 0..20 {
            let ev = StrokeEvent {
                pos: [i as f32 * 20.0, i as f32 * 2.0],
                pressure: 1.0,
                tilt: None,
                time_ns: i as u64 * 8_000_000,
                in_proximity: true,
                contact: true,
            };
            dabs.extend(c.push(&ev, 10.0));
        }
        assert!(!dabs.is_empty(), "a 380 px stroke must emit dabs");
        let anchor = c.line_anchor().expect("stroke must be anchored");
        for d in &dabs {
            assert!(
                (d.pos[1] - anchor[1]).abs() <= 1e-3,
                "dab y {} must hold the snapped ray y {}",
                d.pos[1],
                anchor[1]
            );
        }
        // The raw path drifts to y = 38; the snap must have held the ray.
        let last = dabs.last().expect("dabs");
        assert!(
            (last.pos[1] - 38.0).abs() > 10.0,
            "the snap must correct the drift, got y {}",
            last.pos[1]
        );
    }

    fn exact_conditioner(method: StrokeMethod) -> StrokeConditioner {
        let mut c = conditioner();
        c.set_exact(true);
        c.set_method(method).expect("valid method");
        c
    }

    fn event_at(pos: [f32; 2], pressure: f32, time_ns: u64) -> StrokeEvent {
        StrokeEvent {
            pos,
            pressure,
            tilt: None,
            time_ns,
            in_proximity: true,
            contact: true,
        }
    }

    /// The pre-stroke-method pipeline, composed by hand from the stages
    /// exactly as `push`/`advance` did before methods existed.
    struct Reference {
        pos_filter: OneEuroFilter,
        pressure_filter: OneEuroScalar,
        lazy: LazyMouse,
        line: LineConstraint,
        spacing: SpacingAccumulator,
        cursor: [f32; 2],
        anchor: [f32; 2],
        anchor_pressure: f32,
        live: bool,
    }

    impl Reference {
        fn new(lazy_radius_px: f32) -> Self {
            Self {
                pos_filter: OneEuroFilter::try_new(OneEuroParams::default()).expect("valid"),
                pressure_filter: OneEuroScalar::try_new(OneEuroParams::default()).expect("valid"),
                lazy: LazyMouse::new(lazy_radius_px),
                line: LineConstraint::default(),
                spacing: SpacingAccumulator::new(2.0).expect("valid"),
                cursor: [0.0, 0.0],
                anchor: [0.0, 0.0],
                anchor_pressure: 0.0,
                live: false,
            }
        }

        fn push(&mut self, event: &StrokeEvent, radius_px: f32) -> Vec<DabPlan> {
            let pos = self.pos_filter.filter(event.pos, event.time_ns);
            let pressure = self
                .pressure_filter
                .filter(saturate01(event.pressure), event.time_ns);
            self.cursor = self.lazy.apply(pos, self.cursor);
            self.cursor = self.line.apply(self.cursor);
            if !self.live {
                self.anchor = self.cursor;
                self.anchor_pressure = pressure;
                self.live = true;
                return Vec::new();
            }
            let dabs = self.spacing.plan_dabs(
                self.anchor,
                self.cursor,
                radius_px,
                [self.anchor_pressure, pressure],
            );
            if let Some(last) = dabs.last() {
                self.anchor = last.pos;
                self.anchor_pressure = last.alpha_multiplier;
            }
            dabs
        }
    }

    /// Space (default AND explicitly set) is bit-identical to the
    /// pre-method pipeline over a wiggly 2D stroke with varying pressure,
    /// a lazy leash, and a dwell.
    #[test]
    fn space_method_is_bit_identical_to_the_old_pipeline() {
        let seq: Vec<StrokeEvent> = (0..60)
            .map(|i| {
                let x = if (20..30).contains(&i) {
                    60.0
                } else {
                    i as f32 * 3.0
                };
                event_at(
                    [x + (i % 2) as f32, (i % 5) as f32 * 1.7],
                    0.3 + (i % 7) as f32 * 0.1,
                    i as u64 * 4_000_000,
                )
            })
            .collect();
        let mut reference = Reference::new(2.0);
        let mut default_space =
            StrokeConditioner::new(OneEuroParams::default(), OneEuroParams::default(), 2.0, 2.0)
                .expect("valid");
        let mut set_space = default_space.clone();
        set_space.set_method(StrokeMethod::Space).expect("valid");
        assert_eq!(default_space.method(), StrokeMethod::Space);
        let mut total = 0;
        for ev in &seq {
            let want = reference.push(ev, 6.0);
            let got_a = default_space.push(ev, 6.0);
            let got_b = set_space.push(ev, 6.0);
            total += want.len();
            // Bitwise: compare raw f32 bits, not approximate values.
            let bits = |d: &[DabPlan]| -> Vec<[u32; 3]> {
                d.iter()
                    .map(|p| {
                        [
                            p.pos[0].to_bits(),
                            p.pos[1].to_bits(),
                            p.alpha_multiplier.to_bits(),
                        ]
                    })
                    .collect()
            };
            assert_eq!(bits(&got_a), bits(&want));
            assert_eq!(bits(&got_b), bits(&want));
        }
        assert!(
            total > 10,
            "fixture must emit dabs to compare (got {total})"
        );
        assert!(
            default_space.finish().is_empty(),
            "Space emits nothing on finish"
        );
    }

    /// 10 Hz held still for 0.35 s (7 events, 50 ms apart, after the seed)
    /// banks 0.5 per event: dabs fire on events 2, 4, 6 — 3 in total, all
    /// at the dwell point with the event's pressure.
    #[test]
    fn airbrush_held_still_sprays_at_rate() {
        let mut c = exact_conditioner(StrokeMethod::Airbrush { rate_hz: 10.0 });
        assert!(c.push(&event_at([5.0, 7.0], 0.6, 0), 10.0).is_empty());
        let mut per_event = Vec::new();
        let mut all = Vec::new();
        for i in 1..=7u64 {
            let d = c.push(&event_at([5.0, 7.0], 0.6, i * 50_000_000), 10.0);
            per_event.push(d.len());
            all.extend(d);
        }
        assert_eq!(per_event, vec![0, 1, 0, 1, 0, 1, 0]);
        assert_eq!(all.len(), 3);
        for d in &all {
            assert_eq!(d.pos, [5.0, 7.0]);
            assert_eq!(d.alpha_multiplier, 0.6);
        }
        assert!(c.finish().is_empty());
    }

    /// While moving, Airbrush is spaced exactly like Space (a spacing dab
    /// drains the spray bank), so motion is never double-density.
    #[test]
    fn airbrush_moving_matches_space() {
        let mut air = exact_conditioner(StrokeMethod::Airbrush { rate_hz: 10.0 });
        let mut space = exact_conditioner(StrokeMethod::Space);
        for i in 0..=10u64 {
            let ev = event_at([i as f32 * 10.0, 0.0], 1.0, i * 50_000_000);
            assert_eq!(air.push(&ev, 10.0), space.push(&ev, 10.0));
        }
    }

    /// Dash ratio 0.5 length 20 over a straight 100 px stroke at a 5 px
    /// step: spaced dabs at 5, 10, …, 100; phase = x; kept iff
    /// x mod 20 < 10. Pinned the same whether the stroke arrives as one
    /// segment or ten (phase carries across events).
    #[test]
    fn dash_keeps_only_the_on_intervals() {
        let want: Vec<f32> = vec![5.0, 20.0, 25.0, 40.0, 45.0, 60.0, 65.0, 80.0, 85.0, 100.0];
        let method = StrokeMethod::Dash {
            ratio: 0.5,
            length_px: 20.0,
        };
        for events in [1u64, 10] {
            let mut c = exact_conditioner(method);
            assert!(c.push(&event_at([0.0, 0.0], 1.0, 0), 10.0).is_empty());
            let mut got = Vec::new();
            for i in 1..=events {
                let x = 100.0 * i as f32 / events as f32;
                got.extend(c.push(&event_at([x, 0.0], 1.0, i * 8_000_000), 10.0));
            }
            // Positions are spacing's own `from + segment * t` floats (one
            // segment gives 60.000004), so compare within EPS; the selection
            // itself (which dabs survive) is exact via the count.
            let xs: Vec<f32> = got.iter().map(|d| d.pos[0]).collect();
            assert_eq!(xs.len(), want.len(), "{events} event(s): {xs:?}");
            for (g, w) in xs.iter().zip(want.iter()) {
                assert!((g - w).abs() <= EPS, "{events} event(s): {xs:?}");
            }
            assert!(got.iter().all(|d| d.pos[1] == 0.0));
        }
    }

    /// Drag Dot: nothing during the stroke; `finish` emits exactly one dab
    /// at the last conditioned position with its pressure, once.
    #[test]
    fn drag_dot_emits_one_dab_at_the_end() {
        let mut c = exact_conditioner(StrokeMethod::DragDot);
        for i in 0..=10u64 {
            let ev = event_at(
                [i as f32 * 9.0, i as f32 * 2.0],
                0.1 * i as f32,
                i * 8_000_000,
            );
            assert!(c.push(&ev, 10.0).is_empty(), "event {i}");
        }
        let dot = c.finish();
        assert_eq!(dot.len(), 1);
        assert_eq!(dot[0].pos, [90.0, 20.0]);
        assert_eq!(dot[0].alpha_multiplier, 1.0);
        assert!(c.finish().is_empty(), "the dot is emitted once");
        // A click with no motion still leaves its dot.
        c.reset();
        c.push(&event_at([3.0, 4.0], 0.5, 0), 10.0);
        assert_eq!(
            c.finish(),
            vec![DabPlan {
                pos: [3.0, 4.0],
                alpha_multiplier: 0.5
            }]
        );
        // Reset before finish drops the pending dot.
        c.push(&event_at([3.0, 4.0], 0.5, 0), 10.0);
        c.reset();
        assert!(c.finish().is_empty());
    }

    #[test]
    fn set_method_rejects_invalid_and_keeps_the_current() {
        let mut c = conditioner();
        c.set_method(StrokeMethod::DragDot).expect("valid");
        let err = c.set_method(StrokeMethod::Airbrush { rate_hz: 0.0 });
        assert!(matches!(
            err,
            Err(ConditionerError::Method(StrokeMethodError::InvalidRate(_)))
        ));
        assert_eq!(c.method(), StrokeMethod::DragDot);
    }

    #[test]
    fn density_getter_reports_construction_density() {
        // The wash normalizer reads the overlap count through this: it
        // must be the density the spacing actually uses.
        assert!((conditioner().dabs_per_radius() - 2.0).abs() <= EPS);
    }

    #[test]
    fn invalid_params_rejected() {
        let bad_density =
            StrokeConditioner::new(OneEuroParams::default(), OneEuroParams::default(), 0.0, 0.0);
        assert!(bad_density.is_err());
        let bad_filter = StrokeConditioner::new(
            OneEuroParams {
                min_cutoff: -1.0,
                beta: 0.0,
                d_cutoff: 1.0,
            },
            OneEuroParams::default(),
            0.0,
            2.0,
        );
        assert!(bad_filter.is_err());
    }
}
