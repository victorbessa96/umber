//! The set-switch glue (the W-C4 L1 extraction, `docs/loop/briefs/
//! 2026-10-22-wc4-l1-set-switch-glue.md`): the two non-UI cores of the
//! texture-sets panel, lifted to a lib-reachable module so a headless
//! variant — and the headless test beside `gate0_journey.rs` — can
//! exercise them without the UI shell (C2-9's named gap).
//!
//! Extraction boundaries (`texture_sets_panel_ui` in the shell stays
//! the only caller of both fns in-tree; its behavior is
//! bit-identical):
//!
//! - [`switch_active_set`]: the set switch plus Amendment A point 4's
//!   guarded re-stage — the stencil gate stages at the ACTIVE tile's
//!   quick-mask plane size, so a paint-side set switch must re-stage
//!   (the new set can carry a differently-sized quick mask, and the
//!   new tile would otherwise paint stencil-ungated). The guard lives
//!   INSIDE the fn (a live stencil: active + image-bearing) so the
//!   headless test exercises it, not a UI-side condition the test
//!   cannot reach.
//! - [`reconcile_pending_resize`]: the stale-pending clear — a
//!   `PendingResize` armed for a set that is no longer active is
//!   dead the moment the switch lands. The ARMING stays in the panel
//!   (it reads the size combo's pick — UI); the reconcile is the
//!   non-UI half.
//!
//! [`restage_stencil`] moved here from `viewport.rs` unchanged in
//! name and signature: it was `pub(crate)` in a bin-only module, so
//! no lib-side caller could ever reach it — the move is the point of
//! the kill. Its contract (the test's asserts):
//!
//! - It stages the gate from `paint.active_tile()`'s quick mask
//!   (`target = mask.dimensions()`, bytes via
//!   [`crate::stencil::quick_mask_gate_bytes`]), hooks it into paint
//!   through `PaintState::set_stencil_gate` (which refuses mid-stroke
//!   and no-ops on a same value), and records
//!   `stencil.staged_for = Some((tile, w, h))` only on an ADOPTED
//!   hand-off (`true` return) — `None` on an adopted clear.
//! - The record follows the HOOK, not the label: recording on
//!   anything but `true` would make the observer read the label's
//!   dims as a match and never retry, leaving a stale gate in place
//!   forever.

use crate::stencil::StencilState;
use umber_paint::paint_state::PaintState;

/// A set resize ARMED but not yet confirmed (the texture-sets panel's
/// state): the combo pick only arms; the destructive change (it
/// clears the set's canvas and the stroke undo history) waits for the
/// explicit confirm, so one stray click never wipes a painted set
/// (GUI plan rule 8: destructive = confirm).
///
/// The fields are public so the headless test (and a headless
/// variant) can construct the state directly; the panel remains the
/// only in-tree constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingResize {
    /// The set the resize was armed for.
    pub set: String,
    /// The set's current size.
    pub from: (u32, u32),
    /// The picked new size (square).
    pub to: u32,
}

/// The stencil re-stage (moved from `viewport.rs` unchanged; see the
/// module docs for the record-after-hook contract).
pub fn restage_stencil(stencil: &mut StencilState, paint: Option<&mut PaintState>) {
    if let Some(p) = paint {
        let tile = p.active_tile();
        let mask = p.quick_mask_for_tile(tile);
        let (w, h) = mask.dimensions();
        let target = Some((w, h));
        let quick = crate::stencil::quick_mask_gate_bytes(mask, tile);
        stencil.stage_gate(target, quick.as_deref());
        // The observer's record follows the HOOK, not the label: only an
        // ADOPTED hand-off proves paint's gate matches this tile + these
        // dims. `set_stencil_gate` refuses mid-stroke (and no-ops on a
        // same value), and the label above still stages on a refusal — so
        // recording on anything but `true` would make the observer read
        // the label's dims as a match and never retry, leaving the stale
        // gate in place forever. An adopted CLEAR (a deactivation hands
        // off `None`) records `None`: there are no bytes staged to claim.
        let gate = stencil.hook_gate(target);
        let has_gate = gate.is_some();
        if p.set_stencil_gate(gate) {
            stencil.staged_for = if has_gate { Some((tile, w, h)) } else { None };
        }
    } else {
        stencil.stage_gate(None, None);
    }
}

/// Switch the paint's active texture set, re-staging the stencil gate
/// when the stencil is live (Amendment A point 4: the gate stages at
/// the ACTIVE tile's quick-mask plane size — see the module docs).
///
/// Returns `false` without touching anything when `new_set` is
/// already the active set (the panel's early-out becomes the fn's);
/// `true` on an actual switch. The re-stage is guarded to a live
/// stencil (`active && image.is_some()`): an inactive or imageless
/// stencil holds no gate worth re-staging, and re-staging it would
/// only clear paint's gate for nothing.
pub fn switch_active_set(
    paint: &mut PaintState,
    stencil: Option<&mut StencilState>,
    new_set: &str,
) -> bool {
    if new_set == paint.active_set() {
        return false;
    }
    paint.set_active_set(new_set);
    if let Some(s) = stencil {
        if s.active && s.image.is_some() {
            restage_stencil(s, Some(paint));
        }
    }
    true
}

/// Reconcile an armed [`PendingResize`] against the active set: a
/// pending armed for a different set is stale the moment the switch
/// lands — the confirm dialog belongs to the set it was armed for,
/// and the panel would otherwise render a confirm group for a set
/// the user is no longer looking at.
pub fn reconcile_pending_resize(pending: &mut Option<PendingResize>, active: &str) {
    if let Some(p) = &*pending {
        if p.set != active {
            *pending = None;
        }
    }
}
