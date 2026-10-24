//! The set-switch glue's headless test (the W-C4 L1 kill, C2-9's
//! named gap — `docs/loop/briefs/2026-10-22-wc4-l1-set-switch-glue.md`).
//!
//! The texture-sets panel's two non-UI cores live in
//! `umber_app::set_switch` since the L1 extraction; this test pins
//! their contracts without the UI shell — the point the C2-9 row
//! named: a headless variant needs the glue reachable and proven.
//! The desktop smoke (the `docs/loop/desktop-checks.md` page 3
//! Result line) stays the end-to-end instrument; this is the
//! headless half.
//!
//! The five pins (the brief's list, verbatim):
//!
//! 1. Live stencil (active + image), a switch to a set with a
//!    different quick-mask size → `staged_for == Some((tile, w, h))`
//!    matching the NEW set's active tile's quick-mask dims
//!    (Amendment A: the gate follows the tile, not the label). The
//!    dims are READ at runtime (`quick_mask_for_tile`'s own
//!    dimensions) — the pin is the MATCH, not a hardcoded number.
//! 2. Same-set call → `false`, `staged_for` unchanged.
//! 3. Inactive stencil → `true` (the set still switches), no
//!    re-stage attempted (`staged_for` unchanged).
//! 4. Imageless stencil → same as 3.
//! 5. `reconcile_pending_resize`: a pending naming a different set
//!    clears; one naming the active set survives.

use umber_app::paint_state::PaintState;
use umber_app::set_switch::{reconcile_pending_resize, switch_active_set, PendingResize};
use umber_app::stencil::StencilState;
use umber_core::stencil::StencilImage;

/// Small and fast: the pins read dims from the state, never from the
/// constant — only a working paint target is needed.
const SIZE: u32 = 512;
/// The second set's size — different from the first, so the re-stage
/// has a size change to prove itself with.
const OTHER: u32 = 256;

fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("skipping set_switch_glue: no wgpu adapter available");
        return None;
    };
    if !adapter
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        eprintln!(
            "skipping set_switch_glue: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES"
        );
        return None;
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        ..Default::default()
    }))
    .ok()
}

/// A live stencil: active, with a tiny 2×2 all-passing coverage image
/// (alpha 255 ≥ 128 → every texel passes).
fn live_stencil() -> StencilState {
    let image = StencilImage::from_alpha(
        2,
        2,
        &[
            255, 255, 255, 255, //
            255, 255, 255, 255, //
            255, 255, 255, 255, //
            255, 255, 255, 255,
        ],
    )
    .expect("2x2 coverage builds");
    StencilState {
        active: true,
        image: Some(image),
        ..StencilState::default()
    }
}

#[test]
fn set_switch_glue_pins_the_panel_contracts() {
    let Some((device, queue)) = try_request_device() else {
        return;
    };
    let mut paint =
        PaintState::with_target_size(device, queue, SIZE).expect("paint builds at SIZE");

    // Two sets at different sizes: the re-stage must land the new
    // set's own quick-mask dims, not the old set's.
    paint.ensure_set("alpha");
    paint.ensure_set("beta");
    paint.resize_set("beta", OTHER, OTHER);
    paint.set_active_set("alpha");

    // ---- Pin 1: live stencil, switch → the gate follows the tile. ----
    let mut stencil = live_stencil();
    assert!(switch_active_set(&mut paint, Some(&mut stencil), "beta"));
    assert_eq!(paint.active_set(), "beta");
    let tile = paint.active_tile();
    let (mw, mh) = paint.quick_mask_for_tile(tile).dimensions();
    assert_eq!(
        stencil.staged_for,
        Some((tile, mw, mh)),
        "Amendment A: the staged record must match the new set's quick-mask dims"
    );

    // ---- Pin 2: same-set call → false, the record untouched. ----
    let before = stencil.staged_for;
    assert!(!switch_active_set(&mut paint, Some(&mut stencil), "beta"));
    assert_eq!(
        stencil.staged_for, before,
        "a no-op switch must not re-stage"
    );

    // ---- Pin 3: inactive stencil → the set switches, no re-stage. ----
    stencil.active = false;
    assert!(switch_active_set(&mut paint, Some(&mut stencil), "alpha"));
    assert_eq!(paint.active_set(), "alpha");
    assert_eq!(
        stencil.staged_for, before,
        "an inactive stencil holds no gate worth re-staging"
    );

    // ---- Pin 4: imageless stencil → same as 3. ----
    stencil.active = true;
    stencil.image = None;
    assert!(switch_active_set(&mut paint, Some(&mut stencil), "beta"));
    assert_eq!(paint.active_set(), "beta");
    assert_eq!(
        stencil.staged_for, before,
        "an imageless stencil holds no gate worth re-staging"
    );

    // ---- Pin 5: the stale-pending clear. ----
    let mut pending = Some(PendingResize {
        set: "alpha".to_string(),
        from: (SIZE, SIZE),
        to: OTHER,
    });
    reconcile_pending_resize(&mut pending, "beta");
    assert!(
        pending.is_none(),
        "a pending armed for a different set dies with the switch"
    );
    let mut pending = Some(PendingResize {
        set: "beta".to_string(),
        from: (OTHER, OTHER),
        to: SIZE,
    });
    reconcile_pending_resize(&mut pending, "beta");
    assert!(
        pending.is_some(),
        "a pending armed for the active set survives"
    );
}
