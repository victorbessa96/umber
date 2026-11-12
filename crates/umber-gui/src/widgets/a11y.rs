//! Keyboard and screen-reader primitives (wave-a11y-first-pass).
//!
//! # Esc ownership
//!
//! The shell closes its app-owned windows (Preferences, About, the
//! confirms) on Esc. A popup drawn INSIDE one of those windows also closes
//! on Esc, and both read the same key press, so one Esc used to close the
//! popup AND the window behind it. Esc must close the topmost thing only.
//!
//! egui's own popups (menus, combo boxes) are visible to the shell through
//! `egui::Popup::is_any_open`. A popup Umber draws itself (an `Area` with
//! its open flag in temp memory, like the colour picker) calls
//! [`claim_escape`] every frame it is open; the shell reads
//! [`escape_claimed`] before acting on Esc. The claim is pass-stamped, so a
//! popup that stops drawing releases it on its own: there is no release
//! call to forget.
//!
//! # Accessible names
//!
//! Some widgets reach the keyboard with no accessible name: a combo box
//! whose visible label is a separate row label (egui only names a combo
//! from its own `from_label` text), or a dock tab drawn by `egui_dock`
//! (which registers the tab title as a bare focusable node). [`name`]
//! writes a label (and optionally a role) onto such a node after it is
//! drawn, without changing how it looks or reacts to the pointer.

use egui::accesskit::Role;
use egui::{Context, Id};

/// The temp-memory slot holding the pass number of the latest claim.
fn slot() -> Id {
    Id::new("umber-gui-esc-claim")
}

/// Marks Esc as owned by a popup that is open this frame.
pub fn claim_escape(ctx: &Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(slot(), pass));
}

/// Whether a popup claimed Esc on the previous pass (or earlier in this
/// one). The shell reads it BEFORE the popups draw, so the previous
/// pass's claim is the one that counts; egui bumps the pass counter at
/// the END of a pass, so a claim made during pass N reads as `N`.
pub fn escape_claimed(ctx: &Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(slot()))
        .is_some_and(|claimed| claimed + 1 >= pass)
}

/// Gives the AccessKit node `id` the accessible name `label` (and `role`,
/// when `Some`). A no-op when AccessKit is off or the node was not drawn
/// this pass.
pub fn name(ctx: &Context, id: Id, role: Option<Role>, label: &str) {
    ctx.accesskit_node_builder(id, |node| {
        if let Some(role) = role {
            node.set_role(role);
        }
        node.set_label(label.to_owned());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pass: optionally claims, and returns what `escape_claimed` read
    /// at the START of the pass (where the shell reads it).
    fn frame(ctx: &Context, claim_now: bool) -> bool {
        let mut seen = false;
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            seen = escape_claimed(ui.ctx());
            if claim_now {
                claim_escape(ui.ctx());
            }
        });
        out.drop_without_applying_deltas();
        seen
    }

    #[test]
    fn an_escape_claim_covers_exactly_the_next_pass() {
        let ctx = Context::default();
        assert!(!frame(&ctx, false), "nothing claimed yet");
        assert!(!frame(&ctx, true), "a claim made this pass reads next pass");
        assert!(frame(&ctx, false), "the pass after the claim defers Esc");
        assert!(
            !frame(&ctx, false),
            "a popup that stopped drawing released Esc"
        );
    }

    #[test]
    fn name_labels_an_unnamed_focusable_node() {
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            let r = ui.allocate_response(egui::vec2(40.0, 20.0), egui::Sense::click());
            name(ui.ctx(), r.id, Some(Role::Tab), "Layers");
        });
        harness.run();
        // The query panics on miss: the call is the assertion.
        let _tab = harness.get_by_role_and_label(Role::Tab, "Layers");
    }
}
