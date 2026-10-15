//! `notices` — the toast/notice stack (docs/specs/gui-plan-2026-10-15.md
//! row g0-notices; docs/research/gui-research-photocraft-2026-10-15.md §5).
//!
//! - A bottom-right `egui::Area` (foreground order) sits above the status
//!   bar and holds one card per notice. Each card has the level's accent
//!   stroke and a × button that dismisses it.
//! - At most [`MAX_VISIBLE`] notices are kept. Posting over the cap drops
//!   the oldest NON-sticky notice first. Only when every notice is sticky
//!   does the oldest sticky one go.
//! - Auto-hide: a non-sticky notice expires [`AUTO_HIDE_SECS`] after
//!   `shown_at`, which is stamped on the first frame it is drawn (a notice
//!   posted while the window is hidden does not burn its time unseen).
//!   While the pointer hovers the stack every timer resets to `now`, so
//!   reading pauses the hide; the countdown restarts when the pointer
//!   leaves.
//! - Repaint: `request_repaint_after(remaining)` for the soonest expiry,
//!   plus one `request_repaint()` on the frame a card expires. Never a
//!   continuous repaint.
//!
//! Text is shown verbatim: umber-gui has no i18n, so callers pass already
//! translated strings (the app's `tr()` output). The timing logic is in
//! pure methods ([`Notices::stamp`], [`Notices::expire`],
//! [`Notices::next_deadline`]) so it is testable without an
//! `egui::Context`.

use std::time::Duration;

use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Order, RichText, Stroke};

use crate::theme;

/// The most notices kept at once.
pub const MAX_VISIBLE: usize = 3;
/// Seconds a non-sticky notice stays up after it was first shown (or last
/// hovered).
pub const AUTO_HIDE_SECS: f64 = 6.0;
/// The stack's `egui::Area` id.
pub const AREA_ID: &str = "umber-notices";
/// Offset from the bottom-right corner; the y clears the status bar.
pub const ANCHOR_OFFSET: [f32; 2] = [-16.0, -36.0];
/// Card width, px.
pub const CARD_WIDTH: f32 = 320.0;

/// Severity of a notice: picks the card's accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoticeLevel {
    /// Neutral information (border stroke, no accent).
    Info,
    /// Something degraded but the action completed.
    Warning,
    /// The action failed.
    Error,
}

impl NoticeLevel {
    /// The card stroke color for this level, from the theme tokens.
    pub fn stroke_color(self) -> Color32 {
        match self {
            NoticeLevel::Info => theme::hover(),
            NoticeLevel::Warning => theme::palette::WARN,
            NoticeLevel::Error => theme::palette::ERROR,
        }
    }
}

/// One posted notice.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// Stable id, returned by [`Notices::post`]; used to dismiss.
    pub id: u64,
    /// Severity.
    pub level: NoticeLevel,
    /// Already-translated text (the caller's `tr()` output).
    pub text: String,
    /// Sticky notices never auto-hide; only × or [`Notices::dismiss`].
    pub sticky: bool,
    /// `egui` time (seconds) the auto-hide timer started: stamped the
    /// first frame the notice is drawn, reset while hovered. `None` until
    /// first drawn.
    pub shown_at: Option<f64>,
}

impl Notice {
    /// Seconds until this notice expires at `now`, or `None` when it does
    /// not auto-hide (sticky) or has not been shown yet. Clamped at 0.
    pub fn remaining(&self, now: f64) -> Option<f64> {
        if self.sticky {
            return None;
        }
        self.shown_at.map(|t| (t + AUTO_HIDE_SECS - now).max(0.0))
    }
}

/// The notice stack. Owned by the app; call [`Notices::show`] once per
/// frame.
#[derive(Debug, Clone, Default)]
pub struct Notices {
    /// Notices in post order (oldest first).
    pub items: Vec<Notice>,
    next_id: u64,
}

impl Notices {
    /// An empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Post an auto-hiding notice. Returns its id.
    pub fn post(&mut self, level: NoticeLevel, text: impl Into<String>) -> u64 {
        self.push(level, text.into(), false)
    }

    /// Post a sticky notice (stays until dismissed). Returns its id.
    pub fn post_sticky(&mut self, level: NoticeLevel, text: impl Into<String>) -> u64 {
        self.push(level, text.into(), true)
    }

    fn push(&mut self, level: NoticeLevel, text: String, sticky: bool) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Notice {
            id,
            level,
            text,
            sticky,
            shown_at: None,
        });
        self.enforce_cap();
        id
    }

    /// Drop notices over [`MAX_VISIBLE`]: the oldest non-sticky first,
    /// then (all sticky) the oldest overall.
    fn enforce_cap(&mut self) {
        while self.items.len() > MAX_VISIBLE {
            let victim = self.items.iter().position(|n| !n.sticky).unwrap_or(0);
            self.items.remove(victim);
        }
    }

    /// Remove the notice with `id`. Returns whether one was removed.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|n| n.id != id);
        self.items.len() != before
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Start the timer of every notice not yet shown (`shown_at = now`).
    pub fn stamp(&mut self, now: f64) {
        for n in &mut self.items {
            n.shown_at.get_or_insert(now);
        }
    }

    /// Advance the timers to `now`. While `hovered`, every shown notice's
    /// timer resets to `now` and nothing expires. Otherwise non-sticky
    /// notices whose time is up are removed. Returns whether any expired.
    pub fn expire(&mut self, now: f64, hovered: bool) -> bool {
        if hovered {
            for n in &mut self.items {
                if n.shown_at.is_some() {
                    n.shown_at = Some(now);
                }
            }
            return false;
        }
        let before = self.items.len();
        self.items
            .retain(|n| n.remaining(now).is_none_or(|r| r > 0.0));
        self.items.len() != before
    }

    /// Seconds until the soonest auto-hide at `now` (the delay to pass to
    /// `request_repaint_after`), or `None` when nothing is counting down.
    pub fn next_deadline(&self, now: f64) -> Option<f64> {
        self.items
            .iter()
            .filter_map(|n| n.remaining(now))
            .min_by(|a, b| a.total_cmp(b))
    }

    /// Draw the stack and run its timers. Call once per frame.
    pub fn show(&mut self, ctx: &egui::Context) {
        if self.items.is_empty() {
            return;
        }
        let area_id = Id::new(AREA_ID);
        let now = ctx.input(|i| i.time);
        // Hover is read against last frame's rect: the stack is drawn
        // after the timers run, and one frame of lag is invisible.
        let hovered = match (
            ctx.memory(|m| m.area_rect(area_id)),
            ctx.input(|i| i.pointer.hover_pos()),
        ) {
            (Some(rect), Some(pos)) => rect.contains(pos),
            _ => false,
        };
        if self.expire(now, hovered) {
            ctx.request_repaint();
        }
        if self.items.is_empty() {
            return;
        }
        // Everything drawn this frame starts (or keeps) its timer.
        self.stamp(now);

        let mut dismissed = Vec::new();
        egui::Area::new(area_id)
            .order(Order::Foreground)
            .anchor(
                Align2::RIGHT_BOTTOM,
                egui::vec2(ANCHOR_OFFSET[0], ANCHOR_OFFSET[1]),
            )
            .show(ctx, |ui| {
                ui.set_width(CARD_WIDTH);
                for n in &self.items {
                    Frame::NONE
                        .fill(theme::raised())
                        .stroke(Stroke::new(1.0, n.level.stroke_color()))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(8))
                        .shadow(ui.visuals().popup_shadow)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&n.text).color(theme::text()))
                                        .wrap(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::TOP),
                                    |ui| {
                                        let x = ui.add(
                                            egui::Button::new(
                                                RichText::new("×").color(theme::muted()),
                                            )
                                            .frame(false),
                                        );
                                        if x.clicked() {
                                            dismissed.push(n.id);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        for id in dismissed {
            self.dismiss(id);
        }

        if let Some(remaining) = self.next_deadline(now) {
            ctx.request_repaint_after(Duration::from_secs_f64(remaining));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(notices: &mut Notices, now: f64) {
        notices.stamp(now);
    }

    #[test]
    fn non_sticky_expires_after_auto_hide() {
        let mut n = Notices::new();
        n.post(NoticeLevel::Info, "saved");
        shown(&mut n, 10.0);
        assert!(!n.expire(10.0 + AUTO_HIDE_SECS - 0.01, false));
        assert_eq!(n.items.len(), 1);
        assert!(n.expire(10.0 + AUTO_HIDE_SECS, false));
        assert!(n.is_empty());
    }

    #[test]
    fn unshown_notice_does_not_expire() {
        let mut n = Notices::new();
        n.post(NoticeLevel::Info, "queued while hidden");
        assert!(!n.expire(1_000.0, false));
        assert_eq!(n.items.len(), 1);
        assert_eq!(n.next_deadline(1_000.0), None);
    }

    #[test]
    fn stamp_keeps_the_first_shown_time() {
        let mut n = Notices::new();
        n.post(NoticeLevel::Info, "a");
        n.stamp(1.0);
        n.stamp(5.0);
        assert_eq!(n.items[0].shown_at, Some(1.0));
    }

    #[test]
    fn sticky_never_expires() {
        let mut n = Notices::new();
        n.post_sticky(NoticeLevel::Error, "export failed");
        shown(&mut n, 0.0);
        assert!(!n.expire(1.0e6, false));
        assert_eq!(n.items.len(), 1);
        assert_eq!(n.next_deadline(1.0e6), None);
    }

    #[test]
    fn hover_resets_every_timer() {
        let mut n = Notices::new();
        n.post(NoticeLevel::Info, "a");
        n.post(NoticeLevel::Warning, "b");
        shown(&mut n, 0.0);
        // Hovered well past the deadline: nothing expires, timers reset.
        assert!(!n.expire(100.0, true));
        assert_eq!(n.items.len(), 2);
        assert!(n.items.iter().all(|x| x.shown_at == Some(100.0)));
        // Pointer left: the full countdown runs from the last hovered frame.
        assert!(!n.expire(100.0 + AUTO_HIDE_SECS - 0.5, false));
        assert!(n.expire(100.0 + AUTO_HIDE_SECS, false));
        assert!(n.is_empty());
    }

    #[test]
    fn hover_does_not_stamp_unshown() {
        let mut n = Notices::new();
        n.post(NoticeLevel::Info, "a");
        n.expire(3.0, true);
        assert_eq!(n.items[0].shown_at, None);
    }

    #[test]
    fn cap_drops_oldest_non_sticky_first() {
        let mut n = Notices::new();
        let s = n.post_sticky(NoticeLevel::Error, "sticky oldest");
        let a = n.post(NoticeLevel::Info, "a");
        let b = n.post(NoticeLevel::Info, "b");
        let c = n.post(NoticeLevel::Info, "c");
        let ids: Vec<u64> = n.items.iter().map(|x| x.id).collect();
        // The sticky survives though it is oldest; `a` (oldest temp) went.
        assert_eq!(ids, vec![s, b, c]);
        assert!(!ids.contains(&a));
        assert_eq!(n.items.len(), MAX_VISIBLE);
    }

    #[test]
    fn cap_all_sticky_drops_oldest() {
        let mut n = Notices::new();
        let ids: Vec<u64> = (0..4)
            .map(|i| n.post_sticky(NoticeLevel::Warning, format!("s{i}")))
            .collect();
        let kept: Vec<u64> = n.items.iter().map(|x| x.id).collect();
        assert_eq!(kept, ids[1..].to_vec());
    }

    #[test]
    fn dismiss_removes_by_id() {
        let mut n = Notices::new();
        let a = n.post(NoticeLevel::Info, "a");
        let b = n.post_sticky(NoticeLevel::Error, "b");
        assert!(n.dismiss(b));
        assert!(!n.dismiss(b));
        assert_eq!(n.items.len(), 1);
        assert_eq!(n.items[0].id, a);
    }

    #[test]
    fn ids_are_unique_and_increasing() {
        let mut n = Notices::new();
        let a = n.post(NoticeLevel::Info, "a");
        n.dismiss(a);
        let b = n.post(NoticeLevel::Info, "b");
        assert!(b > a);
    }

    #[test]
    fn next_deadline_is_the_soonest_remaining() {
        let mut n = Notices::new();
        n.post(NoticeLevel::Info, "early");
        n.stamp(0.0);
        n.post(NoticeLevel::Info, "late");
        n.post_sticky(NoticeLevel::Error, "sticky");
        n.stamp(2.0);
        let d = n.next_deadline(3.0).unwrap();
        assert!((d - (AUTO_HIDE_SECS - 3.0)).abs() < 1e-9);
        // Past due clamps at zero, never negative.
        assert_eq!(n.next_deadline(100.0), Some(0.0));
        assert_eq!(Notices::new().next_deadline(0.0), None);
    }

    #[test]
    fn levels_use_theme_tokens() {
        assert_eq!(NoticeLevel::Warning.stroke_color(), theme::palette::WARN);
        assert_eq!(NoticeLevel::Error.stroke_color(), theme::palette::ERROR);
        assert_eq!(NoticeLevel::Info.stroke_color(), theme::hover());
    }

    #[test]
    fn show_headless_stamps_and_schedules() {
        let ctx = egui::Context::default();
        let mut n = Notices::new();
        n.post(NoticeLevel::Warning, "bake warning");
        // A headless pass leaves the font atlas upload pending; clear it so
        // epaint's unapplied-delta debug assert does not fire on drop.
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| n.show(ui.ctx()));
        out.textures_delta.clear();
        assert_eq!(n.items.len(), 1);
        assert!(n.items[0].shown_at.is_some());
    }
}
