# Kill report — 2026-11-07 pwys stroke buffer: the camera may move mid-stroke (row 272)

- **Authorship:** claw (Claude Code opus, headless, session `90b5e6db-7206-4145-9c58-257022cc7517`, $2.79); Razul (dragon) reviewed, corrected, gated, committed.
- **Kill-lock:** `claw-pwys-buffer` — `crates/umber-app/src/viewport.rs` (one file; the paint crate, the GPU crate and the i18n catalogs stayed out of scope).
- **Implements:** `docs/specs/pwys-stroke-buffer-design.md` (option A — resolve-at-start with a frozen camera) + `docs/loop/briefs/2026-11-07-pwys-stroke-buffer-kill-brief.md`. This closes requirements §1's named follow-up ("the camera can't move mid-stroke today").

## Built

- **The snapshot:** a new `stroke_camera: Option<OrbitCamera>` field on the viewport (the camera is already plain `Copy` data, so the snapshot is a copy — no GPU-crate change).
  - Latched by `latch_stroke_camera(stroking, camera)`: the FIRST event that leaves a stroke open stores the exact camera that event's pick used, and a latched snapshot is never overwritten during the stroke.
  - Cleared on the same frame the stroke ends (the `ends_viewport_stroke` → `end_stroke` branch), and by the latch whenever no stroke is open.
- **The frozen resolve:** `pick` became a thin wrapper over `pick_with(rect, mesh, pos, camera)`; the stroke's events go through `stroke_event(...)`, which resolves against `stroke_raycast_camera()` (the snapshot when there is one, the live camera otherwise) and then feeds the hit to the paint session exactly as before (`begin_screen_point_pressured` on the opening event, `extend_screen_point_pressured` after). The eyedropper, the ID-mask select, the brush cursor, the straight-line tool and the orbit pivot keep picking against the LIVE camera.
- **The guard, split by INPUT SOURCE:** `handle_input` became `handle_pointer_nav` (the drag chords, the RMB cancel, the wheel zoom — still skipped while a stroke is live, because the artist's pointer belongs to the stroke) and `handle_camera_keys` (the existing P projection toggle and F frame keys — these now RUN during a stroke). With the stroke's raycast frozen, moving the live camera mid-stroke no longer disturbs the stroke in flight.
- **Four pins** in viewport.rs's test module: the frozen rule after a rotation, the snapshot's independence from the live camera, the mid-stroke readback (the dabs visible at the frozen UVs), and the miss-skip under a rotated camera.

## The dragon's review pass (three test-side corrections, no production change)

1. **The control's hand-derived constant.** The first battery run failed pin 1 on the CONTROL assertion — the live camera's pick expected `0.208905`, the f32 ray math lands at `0.20875306` (1.5e-4 away). The pin itself (the stroke IS the start camera's pick, asserted with `assert_eq!` against the code's own pick) was never in doubt. Fix: the control carries a 1e-3 tolerance, and a NEW assertion now proves the control is not vacuous — the live camera's pick must differ from the frozen UV by more than 0.05.
2. **The GPU pin's device.** The readback pin died with `Paint(MissingDeviceFeature)`: the claw's device helper asked for a PLAIN device (the bakes panel's pattern), but the paint state needs `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`. Fix: the helper requests that feature and SKIPS the test when the adapter lacks it (the perf_soak helper's pattern, which is feature-gated and therefore not reachable from here).
3. **The pin's exact-texel literal → the SPAN.** With the device fixed, the readback pin showed the painted span at row 512 running 289..318 — the dabs ARE at the frozen UVs, but the final point's own dab is three texels short of the frozen UV's 321, because the dab placement along a stroke is spacing-driven and the last point's dab is not stamped while the stroke is still open. The literal `texel(321) != untouched` was a wrong claim about the dab placement, not about the camera. Fix: the pin asserts the SPAN — contiguous, containing the start-camera texel 300, reaching at least 309 (the frozen UV's texels minus the brush radius), and never reaching the rotated camera's neighborhood (245 ± the brush radius) — which is the actual contract.

## Judgment calls

- **The snapshot lives in the VIEWPORT, not in the paint session.** The design said "the stroke stores it", but the app owns both the camera and the stroke lifecycle, and keeping the paint crate's API untouched (`begin/extend_screen_point` still take resolved hits) preserves every paint-side invariant (per-event staging, the undo capture, the seam/symmetry accounting, the retouch mirror) by construction. The rejected alternative — the paint session holding the camera — would have touched the paint crate's public API for no behavioral gain.
- **A miss does not latch.** The snapshot is latched by the first event that OPENS a stroke; a drag that opened on a miss starts nothing, so the next event still picks against the live camera (no snapshot until a stroke really exists).
- **Shift suppresses the camera keys during a stroke.** Shift belongs to the stencil chords (Shift+S rotation snap, Shift+click clone), so `handle_camera_keys` refuses Shift-modified presses while a stroke is live.
- **The pointer navigation stays blocked during a stroke.** Only the non-pointer camera path was opened; no keyboard binding was invented.

## Unverified

- **No interactive live-app check** this wave: the camera-keys-during-a-stroke path needs a real pointer/keyboard session, and this wave had none. The pins cover the geometry and the preview; the input feel is the user's to confirm.
- **The readback pin needs a GPU adapter** carrying `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`: it runs on this machine (Intel HD 530) and SKIPS on a runner without that adapter.

## Gates (dragon, 2026-11-07)

- rustfmt --edition 2021 --check on viewport.rs: CLEAN.
- `python3 scripts/check-layers.py`: GREEN (16 crates). `scorecard.py --check`: current.
- clippy `-p umber-app --all-targets -- -D warnings`: **0 errors**, exit 0.
- `cargo test -p umber-app --all-targets`: **1,056 passed / 0 failed** across 10 suites; all four pins green (`stroke_resolves_against_the_start_camera_after_a_rotation`, `the_stroke_snapshot_is_independent_of_the_live_camera`, `a_start_camera_hit_still_paints_after_the_live_camera_turns_away`, `the_staged_dabs_read_back_mid_stroke_at_the_start_camera_uvs`).

## Process note

- The claw could not run cargo (bash gated; the workspace is untrusted for claude) — the expected wall, named in the brief. Its first two pins failed in the dragon's battery for the reasons above; both were test-side, and the production shape was kept.
