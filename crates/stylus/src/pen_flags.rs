//! Pen-flag mapping: Windows Ink `penFlags` → contact/barrel events.
//!
//! Platform-free and ungated on purpose: the eraser end and the barrel
//! button are BITS in `POINTER_PEN_INFO::penFlags`, not window messages,
//! and the Windows-only decode that fetches that struct (`winink.rs`)
//! cannot be compiled — let alone run — off a Windows host. Putting the
//! mapping here means `cargo test -p stylus` exercises it on Linux.
//!
//! # The flag words this decodes (vendored, verified)
//!
//! `docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs`
//! carries the three bits: `PEN_FLAG_BARREL = 1` (`:5477`),
//! `PEN_FLAG_ERASER = 4` (`:5478`), `PEN_FLAG_INVERTED = 2` (`:5479`);
//! `POINTER_PEN_INFO::penFlags: u32`
//! (`docs/claw-artifacts/stylus/win32_pointer/mod.rs:305`) is the word
//! they live in. The mirrors below repeat the values so this module
//! compiles (and tests) with no `windows` dependency at all.
//!
//! # Judgment calls and v1 boundaries
//!
//! * **Eraser INSTEAD of Contact.** A flagged sample maps to
//!   [`TabletEvent::Eraser`] and NEVER to [`TabletEvent::Contact`] — the
//!   vocabulary's own rule (`lib.rs:38-45`) and the XI2 precedent
//!   (`xi2.rs`'s `contact_or_eraser`). The app starts the eraser hold on
//!   `Eraser { active: true }` and restores the saved tool at
//!   `Eraser { active: false }`; emitting both variants for one sample
//!   would make it clear its own hold (`umber-app`'s `stylus_input.rs`).
//! * **Either eraser bit selects the eraser end.** `PEN_FLAG_INVERTED`
//!   (the pen is physically flipped) and `PEN_FLAG_ERASER` (a dedicated
//!   eraser tip/tail) both mean "the eraser end is in use"; digitizers
//!   report one or the other. `PEN_FLAG_BARREL` alone must NOT flip the
//!   tip (pinned).
//! * **`contact` is stateless.** The variant is decided by this sample's
//!   flags alone; the remembered word belongs to the barrel detector and
//!   the two halves never read each other (pinned). A pen still reported
//!   inverted on the release edge yields `Eraser { active: false }` for
//!   free; a plain-tip release yields `Contact { down: false }`.
//! * **Barrel edges only.** `barrel_edge` yields `Button { button: 0,
//!   down }` exactly when the barrel bit differs from the previous
//!   call's; a steady barrel reports nothing. `button: 0` is the crate's
//!   first button slot — the frozen vocabulary's `u8` carries no Windows
//!   button number because Windows Ink reports this button only as a
//!   flag.
//! * **A fresh tracker's memory is "no flags" (0)**, so a first sample
//!   with the barrel already held reports ONE press edge. There is no
//!   earlier observation to diff against, and reporting the state once
//!   is more honest than hiding a held barrel until the user released
//!   and re-pressed; `Button` is a state report, not a click.
//! * **Tilt is NOT here.** `PEN_MASK_TILT_X`/`_Y` gate tilt in
//!   `winink.rs`'s `map_pen_message`; this module is tip/barrel flags.
//!
//! # Deferred
//!
//! * The live `WndProc` hook that drives this tracker is a named later
//!   kill. `winink.rs`'s `decode` keeps its single-event
//!   `Option<TabletEvent>` contract, so it consumes
//!   [`PenFlagTracker::contact`] only: a barrel edge needs a second event
//!   slot on the same message, which the hook (owning the queue) has and
//!   `decode` does not. The detector's consumers today are its tests
//!   below.

use crate::TabletEvent;

/// `PEN_FLAG_BARREL` — the barrel (side) button is held.
/// Vendored value `1u32`
/// (`docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs:5477`).
pub const PEN_FLAG_BARREL: u32 = 1;

/// `PEN_FLAG_ERASER` — the tip in use is an eraser.
/// Vendored value `4u32`
/// (`docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs:5478`).
pub const PEN_FLAG_ERASER: u32 = 4;

/// `PEN_FLAG_INVERTED` — the pen is physically inverted (eraser end
/// toward the tablet). Vendored value `2u32`
/// (`docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs:5479`).
pub const PEN_FLAG_INVERTED: u32 = 2;

/// The bits that mean "the eraser end is in use": either the pen is
/// inverted ([`PEN_FLAG_INVERTED`]) or the tip itself is an eraser
/// ([`PEN_FLAG_ERASER`]). See the module docs for why either one is
/// enough.
const ERASER_FLAGS: u32 = PEN_FLAG_INVERTED | PEN_FLAG_ERASER;

/// Per-pointer pen-flag memory plus the flag → event mapping.
///
/// Holds the last `penFlags` word it observed so [`Self::barrel_edge`]
/// can report transitions; [`Self::contact`] needs no memory (see the
/// module docs).
#[derive(Debug, Default)]
pub struct PenFlagTracker {
    /// The `penFlags` word of the previous barrel observation (`0` = none
    /// seen yet).
    last_flags: u32,
}

impl PenFlagTracker {
    /// A tracker that has observed no flags yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a `penFlags` word means the eraser end is in use
    /// (inverted pen or eraser tip).
    #[must_use]
    pub fn is_eraser(pen_flags: u32) -> bool {
        pen_flags & ERASER_FLAGS != 0
    }

    /// The contact event for one tip sample: [`TabletEvent::Eraser`]
    /// (`active = down`) when the sample is flagged for the eraser end,
    /// else [`TabletEvent::Contact`] — never both, and never a third
    /// variant (the module docs carry the reasoning).
    #[must_use]
    pub fn contact(&self, down: bool, pen_flags: u32, time_ns: u64) -> TabletEvent {
        if Self::is_eraser(pen_flags) {
            TabletEvent::Eraser {
                active: down,
                time_ns,
            }
        } else {
            TabletEvent::Contact { down, time_ns }
        }
    }

    /// The barrel edge for one sample: `Some(Button { button: 0, down })`
    /// exactly when the [`PEN_FLAG_BARREL`] bit differs from the previous
    /// call's, else `None`. The remembered word is replaced on every call,
    /// so steady flags report nothing (see the module docs).
    pub fn barrel_edge(&mut self, pen_flags: u32, time_ns: u64) -> Option<TabletEvent> {
        let barrel = pen_flags & PEN_FLAG_BARREL != 0;
        let was = self.last_flags & PEN_FLAG_BARREL != 0;
        self.last_flags = pen_flags;
        (barrel != was).then_some(TabletEvent::Button {
            button: 0,
            down: barrel,
            time_ns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eraser_flags_emit_eraser_instead_of_contact() {
        let tracker = PenFlagTracker::new();
        // Both bits set: a flipped pen whose tip is an eraser. An asserted
        // Eraser is also the pin that this is NOT a Contact (the app would
        // clear its eraser hold on a Contact instead of starting one).
        assert_eq!(
            tracker.contact(true, PEN_FLAG_INVERTED | PEN_FLAG_ERASER, 21),
            TabletEvent::Eraser {
                active: true,
                time_ns: 21
            }
        );
        // Each bit alone selects the eraser end (hardware sets one).
        assert_eq!(
            tracker.contact(true, PEN_FLAG_INVERTED, 22),
            TabletEvent::Eraser {
                active: true,
                time_ns: 22
            }
        );
        assert_eq!(
            tracker.contact(true, PEN_FLAG_ERASER, 23),
            TabletEvent::Eraser {
                active: true,
                time_ns: 23
            }
        );
    }

    #[test]
    fn eraser_release_clears_the_hold_and_a_plain_tip_flips_back() {
        let tracker = PenFlagTracker::new();
        assert_eq!(
            tracker.contact(true, PEN_FLAG_INVERTED | PEN_FLAG_ERASER, 31),
            TabletEvent::Eraser {
                active: true,
                time_ns: 31
            }
        );
        // Lifting the eraser end: Windows still reports the pen inverted,
        // so the release edge is `Eraser { active: false }` — which is
        // what releases the app's hold.
        assert_eq!(
            tracker.contact(false, PEN_FLAG_INVERTED | PEN_FLAG_ERASER, 32),
            TabletEvent::Eraser {
                active: false,
                time_ns: 32
            }
        );
        // Flipped back to the tip: a plain Contact, whose down arm also
        // clears a standing hold.
        assert_eq!(
            tracker.contact(true, 0, 33),
            TabletEvent::Contact {
                down: true,
                time_ns: 33
            }
        );
        assert_eq!(
            tracker.contact(false, 0, 34),
            TabletEvent::Contact {
                down: false,
                time_ns: 34
            }
        );
    }

    #[test]
    fn plain_tip_flags_emit_contact() {
        let tracker = PenFlagTracker::new();
        assert_eq!(
            tracker.contact(true, 0, 41),
            TabletEvent::Contact {
                down: true,
                time_ns: 41
            }
        );
        assert_eq!(
            tracker.contact(false, 0, 42),
            TabletEvent::Contact {
                down: false,
                time_ns: 42
            }
        );
        // A held barrel must NOT turn the tip into an eraser; only the
        // eraser bits do that (module docs).
        assert_eq!(
            tracker.contact(true, PEN_FLAG_BARREL, 43),
            TabletEvent::Contact {
                down: true,
                time_ns: 43
            }
        );
    }

    #[test]
    fn barrel_press_and_release_are_one_edge_each() {
        let mut tracker = PenFlagTracker::new();
        assert_eq!(tracker.barrel_edge(0, 51), None);
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_BARREL, 52),
            Some(TabletEvent::Button {
                button: 0,
                down: true,
                time_ns: 52
            })
        );
        assert_eq!(tracker.barrel_edge(PEN_FLAG_BARREL, 53), None);
        assert_eq!(
            tracker.barrel_edge(0, 54),
            Some(TabletEvent::Button {
                button: 0,
                down: false,
                time_ns: 54
            })
        );
        assert_eq!(tracker.barrel_edge(0, 55), None);
    }

    #[test]
    fn steady_barrel_flags_never_report_twice() {
        let mut tracker = PenFlagTracker::new();
        // The first sample already has the barrel held: one press edge,
        // because a fresh tracker's memory is "no flags" (module docs).
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_BARREL, 61),
            Some(TabletEvent::Button {
                button: 0,
                down: true,
                time_ns: 61
            })
        );
        // The barrel keeps holding while the OTHER bits churn (the pen is
        // flipped mid-drag): no second Button.
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_BARREL | PEN_FLAG_INVERTED, 62),
            None
        );
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_BARREL | PEN_FLAG_INVERTED, 63),
            None
        );
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_BARREL | PEN_FLAG_ERASER, 64),
            None
        );
        // The release still reports exactly once.
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_INVERTED, 65),
            Some(TabletEvent::Button {
                button: 0,
                down: false,
                time_ns: 65
            })
        );
        assert_eq!(tracker.barrel_edge(PEN_FLAG_INVERTED, 66), None);
    }

    #[test]
    fn contact_mapping_does_not_disturb_the_barrel_memory() {
        let mut tracker = PenFlagTracker::new();
        // One sample seen by both halves: eraser tip AND barrel held.
        assert_eq!(
            tracker.contact(true, PEN_FLAG_INVERTED | PEN_FLAG_BARREL, 71),
            TabletEvent::Eraser {
                active: true,
                time_ns: 71
            }
        );
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_INVERTED | PEN_FLAG_BARREL, 71),
            Some(TabletEvent::Button {
                button: 0,
                down: true,
                time_ns: 71
            })
        );
        // Still held next sample: no repeat, even with `contact` run in
        // between (the two halves share the tracker, not the memory).
        assert_eq!(
            tracker.barrel_edge(PEN_FLAG_INVERTED | PEN_FLAG_BARREL, 72),
            None
        );
    }
}
