//! Stylus poller: the `stylus`-crate backends' pressure/tilt source ABOVE
//! the egui `Event::Touch` fallback.
//!
//! Once per frame — before the viewport's paint arm reads — the app calls
//! [`StylusPoller::poll_frame`], which drains the system tablet stream
//! ([`stylus::open_system_stream`]: Linux XI2 when the `xi2` feature is
//! compiled in, `None` everywhere else) into per-frame state. The
//! viewport's paint arm then prefers [`StylusPoller::pressure_for_frame`]
//! over the Touch-derived force, else 1.0 (unchanged).
//!
//! # Judgment calls and v1 boundaries
//!
//! * **Fresher-than-frame wins, not freshest-event.** `poll_frame`
//!   stamps drained Motions with the frame index; `pressure_for_frame`
//!   returns `Some` only for Motions drained THIS frame. A stale (last
//!   frame's) pressure must not override this frame's Touch value — the
//!   pen may be gone while a finger paints.
//! * **Stylus wins entirely when fresh.** A fresh stylus pressure of 0.0
//!   or NaN resolves to 1.0 by the same positivity rule the Touch arm
//!   uses (`None`/0.0→1.0) — it does NOT fall through to the Touch
//!   value. Two live sources for one pen is not a case v1 arbitrates.
//! * **Tilt is carried, not consumed.** The poller stores the latest
//!   tilt (`tilt()`), but no brush path reads tilt yet —
//!   `begin/extend_screen_point_pressured` take pressure only. Tilt-driven
//!   dab shape is the follow-up; the C1 audit's "tilt-varying stroke"
//!   gate stays open behind this field.
//! * **Proximity only logs in v1** (`hovering()` toggles; a debug log per
//!   transition). The future paint cursor reads it; nothing visual does.
//! * **Eraser holds the tool for the stroke's duration.**
//!   [`StylusPoller::apply_tool_override`] switches the panel to Erase
//!   while `eraser_active` and restores the saved strip when it clears.
//!   The restore fires only if the strip still shows the forced Erase —
//!   a manual tool change mid-hold is the user's, not ours to yank back.
//!   Pen-tip `Contact { down: true }` clears the eraser hold (the pen was
//!   flipped back); backends that cannot tell the ends apart emit plain
//!   `Contact` and never set the hold.
//! * **Test seam:** the poller drains a [`stylus::StylusStream`], and
//!   `with_stream` injects any backend — the unit tests drive a
//!   [`VecStream`] of synthetic `SourcedEvent`s with no X connection.

use std::collections::VecDeque;

use stylus::{InputSource, SourcedEvent, StylusStream, TabletEvent};

use crate::brush_panel::BrushPanel;
use crate::toolbar::ToolKind;

/// Resolves one frame's paint pressure: fresh stylus pressure first,
/// then the Touch-derived force, else 1.0. Non-positive or non-finite
/// values fold to 1.0 — the viewport's long-standing `None`/0.0→1.0 rule
/// (a mouse, or a platform that cannot measure force, paints full).
pub fn resolve_pressure(stylus: Option<f32>, touch: Option<f32>) -> f32 {
    match stylus.or(touch) {
        Some(f) if f.is_finite() && f > 0.0 => f,
        _ => 1.0,
    }
}

/// A [`StylusStream`] over a fixed script: the unit-test double (no X
/// connection — synthetic `SourcedEvent`s in, drained out). The bin
/// target never constructs it — the test module at the file's tail is
/// its only consumer.
#[allow(dead_code)]
pub struct VecStream {
    queue: VecDeque<SourcedEvent>,
}

impl VecStream {
    /// Queues `events` for the next drain.
    #[allow(dead_code)]
    pub fn new(events: Vec<SourcedEvent>) -> Self {
        Self {
            queue: events.into(),
        }
    }

    /// Appends one event to the drain queue.
    #[allow(dead_code)]
    pub fn push(&mut self, event: SourcedEvent) {
        self.queue.push_back(event);
    }
}

impl StylusStream for VecStream {
    fn drain(&mut self) -> Vec<SourcedEvent> {
        self.queue.drain(..).collect()
    }
}

/// Per-frame tablet state drained from the system stream. Owns the
/// stream (or none — headless, or no backend compiled in) and the frame
/// index that makes [`Self::pressure_for_frame`] fresher-than-frame.
pub struct StylusPoller {
    stream: Option<Box<dyn StylusStream>>,
    frame: u64,
    last_pressure: Option<f32>,
    pressure_frame: u64,
    last_tilt: Option<[f32; 2]>,
    hovering: bool,
    eraser_active: bool,
    last_source: Option<InputSource>,
    saved_tool: Option<ToolKind>,
}

impl StylusPoller {
    /// Opens the system stream ([`stylus::open_system_stream`]). Logs
    /// and degrades to the Touch fallback when there is none — which is
    /// every headless/CI run, by design.
    pub fn new() -> Self {
        Self {
            stream: stylus::open_system_stream(),
            frame: 0,
            last_pressure: None,
            pressure_frame: 0,
            last_tilt: None,
            hovering: false,
            eraser_active: false,
            last_source: None,
            saved_tool: None,
        }
    }

    /// Injects a stream (backends, test doubles). The frame clock starts
    /// at zero either way.
    pub fn with_stream(stream: impl StylusStream + 'static) -> Self {
        Self {
            stream: Some(Box::new(stream)),
            frame: 0,
            last_pressure: None,
            pressure_frame: 0,
            last_tilt: None,
            hovering: false,
            eraser_active: false,
            last_source: None,
            saved_tool: None,
        }
    }

    /// Drains the stream once (call once per frame, before the paint
    /// arm). With no stream this only advances the frame clock, so
    /// `pressure_for_frame` stays `None` and Touch rules.
    pub fn poll_frame(&mut self) {
        self.frame += 1;
        let events = self.stream.as_mut().map_or_else(Vec::new, |s| s.drain());
        for dated in events {
            self.apply(dated);
        }
    }

    /// Folds one drained event into the frame state.
    fn apply(&mut self, dated: SourcedEvent) {
        let source = dated.source;
        self.last_source = Some(source);
        match dated.event {
            TabletEvent::Motion { pressure, tilt, .. } => {
                self.last_pressure = Some(pressure);
                self.pressure_frame = self.frame;
                self.last_tilt = tilt;
            }
            TabletEvent::ProximityIn { .. } => {
                self.hovering = true;
                log::debug!("stylus: {source} proximity in (hover)");
            }
            TabletEvent::ProximityOut { .. } => {
                self.hovering = false;
                log::debug!("stylus: {source} proximity out");
            }
            TabletEvent::Contact { down, .. } => {
                // Pen tip back in use: whatever eraser hold stood ends.
                if down {
                    self.eraser_active = false;
                }
            }
            TabletEvent::Eraser { active, .. } => {
                self.eraser_active = active;
                log::debug!("stylus: {source} eraser active={active}");
            }
            TabletEvent::Button { .. } => {}
        }
    }

    /// This frame's stylus pressure, or `None` when no Motion drained
    /// this frame (the viewport then uses the Touch value, else 1.0).
    pub fn pressure_for_frame(&self) -> Option<f32> {
        if self.pressure_frame == self.frame && self.frame > 0 {
            self.last_pressure
        } else {
            None
        }
    }

    /// The latest tilt sample (degrees, backend convention), if any
    /// Motion ever carried one. Carried for the follow-up (module docs).
    pub fn tilt(&self) -> Option<[f32; 2]> {
        self.last_tilt
    }

    /// Whether the pen is in proximity (hovering). V1 only logs it.
    pub fn hovering(&self) -> bool {
        self.hovering
    }

    /// Whether the eraser end currently holds the tool.
    pub fn eraser_active(&self) -> bool {
        self.eraser_active
    }

    /// The backend that produced the most recent event, if any (the
    /// status bar's `pen:` readout).
    pub fn pen_source(&self) -> Option<InputSource> {
        self.last_source
    }

    /// Holds the Erase tool while the eraser end is active, restoring
    /// the saved strip when it clears (see the module docs). Call once
    /// per frame after [`Self::poll_frame`], before the paint arm.
    pub fn apply_tool_override(&mut self, brush: &mut BrushPanel) {
        if self.eraser_active {
            if brush.strip != ToolKind::Erase {
                if self.saved_tool.is_none() {
                    self.saved_tool = Some(brush.strip);
                }
                brush.select_tool(ToolKind::Erase);
            }
        } else if let Some(saved) = self.saved_tool.take() {
            if brush.strip == ToolKind::Erase {
                brush.select_tool(saved);
            }
        }
    }
}

impl Default for StylusPoller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motion(source: InputSource, pressure: f32) -> SourcedEvent {
        SourcedEvent {
            source,
            event: TabletEvent::Motion {
                pos: [100.0, 200.0],
                pressure,
                tilt: None,
                time_ns: 7,
            },
        }
    }

    fn eraser(source: InputSource, active: bool) -> SourcedEvent {
        SourcedEvent {
            source,
            event: TabletEvent::Eraser { active, time_ns: 9 },
        }
    }

    #[test]
    fn stylus_pressure_overrides_touch_mid_stroke() {
        // The C1 receipt: stylus 0.3 beats Touch 1.0 for the frame it
        // arrived in.
        let mut poller =
            StylusPoller::with_stream(VecStream::new(vec![motion(InputSource::X11Xinput2, 0.3)]));
        poller.poll_frame();
        assert_eq!(poller.pressure_for_frame(), Some(0.3));
        assert_eq!(
            resolve_pressure(poller.pressure_for_frame(), Some(1.0)),
            0.3
        );
    }

    #[test]
    fn stale_stylus_pressure_does_not_override_touch() {
        // A Motion drained last frame must not win this frame: the pen
        // may be gone while a finger paints at 0.7.
        let mut poller =
            StylusPoller::with_stream(VecStream::new(vec![motion(InputSource::X11Xinput2, 0.3)]));
        poller.poll_frame();
        assert_eq!(poller.pressure_for_frame(), Some(0.3));
        poller.poll_frame();
        assert_eq!(poller.pressure_for_frame(), None);
        assert_eq!(
            resolve_pressure(poller.pressure_for_frame(), Some(0.7)),
            0.7
        );
    }

    #[test]
    fn pressure_resolve_folds_to_full() {
        // The viewport's None/0.0→1.0 rule, through the shared resolver:
        // no source, dead sources, and a fresh-but-empty stylus sample
        // all paint full.
        assert_eq!(resolve_pressure(None, None), 1.0);
        assert_eq!(resolve_pressure(None, Some(0.0)), 1.0);
        assert_eq!(resolve_pressure(Some(0.0), Some(0.7)), 1.0);
        assert_eq!(resolve_pressure(Some(f32::NAN), Some(0.5)), 1.0);
        assert_eq!(resolve_pressure(Some(0.5), Some(1.0)), 0.5);
        assert_eq!(resolve_pressure(None, Some(0.5)), 0.5);
    }

    #[test]
    fn eraser_end_holds_and_releases_the_tool() {
        let mut poller =
            StylusPoller::with_stream(VecStream::new(vec![eraser(InputSource::X11Xinput2, true)]));
        let mut brush = BrushPanel::with_dirs(Vec::new());
        assert_eq!(brush.strip, ToolKind::Paint);
        poller.poll_frame();
        assert!(poller.eraser_active());
        poller.apply_tool_override(&mut brush);
        assert_eq!(brush.strip, ToolKind::Erase);
        // Holding across frames keeps it (no re-save, no flicker).
        poller.poll_frame();
        poller.apply_tool_override(&mut brush);
        assert_eq!(brush.strip, ToolKind::Erase);
        // Lifting the eraser restores the saved strip.
        let mut lifting =
            StylusPoller::with_stream(VecStream::new(vec![eraser(InputSource::X11Xinput2, false)]));
        // Hand the hold over: the lift event clears whatever hold stood.
        lifting.eraser_active = true;
        lifting.saved_tool = Some(ToolKind::Paint);
        lifting.poll_frame();
        assert!(!lifting.eraser_active());
        lifting.apply_tool_override(&mut brush);
        assert_eq!(brush.strip, ToolKind::Paint);
    }

    #[test]
    fn pen_tip_contact_clears_the_eraser_hold() {
        // Flipping the pen back: tip contact ends the eraser hold, and
        // the next override pass restores the saved tool.
        let mut poller = StylusPoller::with_stream(VecStream::new(vec![
            eraser(InputSource::X11Xinput2, true),
            SourcedEvent {
                source: InputSource::X11Xinput2,
                event: TabletEvent::Contact {
                    down: true,
                    time_ns: 10,
                },
            },
        ]));
        poller.poll_frame();
        assert!(!poller.eraser_active());
        // Lifting the tip alone changes nothing.
        let mut lifting = StylusPoller::with_stream(VecStream::new(vec![SourcedEvent {
            source: InputSource::X11Xinput2,
            event: TabletEvent::Contact {
                down: false,
                time_ns: 11,
            },
        }]));
        lifting.eraser_active = true;
        lifting.poll_frame();
        assert!(lifting.eraser_active());
    }

    #[test]
    fn proximity_toggles_hover_and_records_the_source() {
        let mut poller = StylusPoller::with_stream(VecStream::new(vec![
            SourcedEvent {
                source: InputSource::X11Xinput2,
                event: TabletEvent::ProximityIn { time_ns: 1 },
            },
            motion(InputSource::X11Xinput2, 0.5),
        ]));
        assert!(!poller.hovering());
        assert_eq!(poller.pen_source(), None);
        poller.poll_frame();
        assert!(poller.hovering());
        assert_eq!(poller.pen_source(), Some(InputSource::X11Xinput2));
        assert_eq!(poller.tilt(), None);
        let mut out = StylusPoller::with_stream(VecStream::new(vec![SourcedEvent {
            source: InputSource::X11Xinput2,
            event: TabletEvent::ProximityOut { time_ns: 2 },
        }]));
        out.poll_frame();
        assert!(!out.hovering());
    }

    #[test]
    fn tilt_samples_are_carried() {
        let mut poller = StylusPoller::with_stream(VecStream::new(vec![SourcedEvent {
            source: InputSource::X11Xinput2,
            event: TabletEvent::Motion {
                pos: [0.0, 0.0],
                pressure: 0.8,
                tilt: Some([10.0, -5.0]),
                time_ns: 3,
            },
        }]));
        poller.poll_frame();
        assert_eq!(poller.tilt(), Some([10.0, -5.0]));
    }

    #[test]
    fn empty_stream_leaves_touch_in_charge() {
        let mut poller = StylusPoller::with_stream(VecStream::new(Vec::new()));
        poller.poll_frame();
        assert_eq!(poller.pressure_for_frame(), None);
        assert_eq!(resolve_pressure(poller.pressure_for_frame(), None), 1.0);
    }
}
