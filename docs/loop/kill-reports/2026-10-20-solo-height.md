# Kill report — solo-height (W-C3b kill 3)

Date: 2026-10-20. Session: local-4d2e8a71 (dragon). Host-free under
the 2026-10-20 code steer (FREEZE-4, centralhost dark); battery at
HOST_BACK via scripts/w-c3b-close.sh, which runs AFTER w-c3a-close.sh.

## Claim

Lock `solo-height` (4 paths: umber-gpu/shaders.rs, view_mode.rs,
renderer.rs, this report). All three code files verified OUTSIDE every
parked manifest before the first edit.

## What this closes

- Requirements row 79's Solo/Preview boundary: "Solo/Preview's height
  arm is untouched (still mid-gray — see `view_mode.rs`'s doc)".
- The `view_mode` module doc's named "small, separate follow-up
  (same texture, different consumer)" — the 2026-10-12 height-normal
  kill deliberately left it out to keep its surface to the lit gate.

## Built

- **shaders.rs** — `solo_proxy`'s HEIGHT arm now reads the bound
  height tile: the SAME addressing chain as `height_perturbed_normal`
  (`paint_tile_uv` → `textureDimensions` → clamped texel →
  `textureLoad(...).r`) behind the SAME flag gate
  (`PAINT_HAS_HEIGHT_TEXTURE`). The mid-gray stand-in stays for a mesh
  with no height channel — the fallback texture is never sampled
  while the flag is clear (renderer.rs:220's contract). The mesh-map
  `preview_proxy`'s PREVIEW_HEIGHT delegation inherits the real data
  through this arm.
- **view_mode.rs** — the module doc's partial-exception paragraph now
  states the implemented state (Solo/Preview consume binding 12 like
  Lit) and cites both kill reports.
- **renderer.rs** — the GPU golden
  `solo_height_reads_the_bound_height_tile`, mirroring the
  `height_stroke_perturbs_the_lit_normal` precedent's shape
  (textured_quad_mesh, the custom camera, pixel (1,1) at byte offset
  20, ±1 tolerance): unbound → the mid-gray stand-in (~byte 128);
  bound u-ramp → the texel's own value (row 1, col 1 holds 85/255 →
  ~byte 85). Skip-graceful without a wgpu adapter.

## Judgment calls

1. **The same addressing, not a new scheme.** Solo and Lit must never
   disagree about which texel a pixel reads — one addressing chain,
   both consumers. Alternative rejected: a solo-local sampler path.
2. **Mid-gray preserved for unbound meshes.** The flag gate keeps the
   fallback texture unsampled — binding 12 is layout-static in the
   shared group 0, so the SHADER arm alone reaches both the solo and
   preview paths with zero renderer changes.
3. **Grayscale by replication** (`.r` into RGB): height is scalar;
   the display-LUT split (`applies_display_lut` false for Height)
   already routes data solos through the identity LUT.
4. **A dedicated golden, not a borrow.** The existing solo tests
   cover BaseColor; the height arm gets its own pin so a future
   addressing change cannot silently regress Solo while Lit's golden
   stays green.

## Unverified (until the battery at HOST_BACK)

- The compile + the golden on the host's real GPU (skip-graceful
  elsewhere; rustfmt clean parse ×3, first pass, no canonicalization).
- Byte 128 for the stand-in: 0.5 linear into `Rgba8Unorm` rounds
  GPU-dependently at the half — the ±1 tolerance carries it (the
  precedent's convention).

## Deferred

- The mesh-map preview's real-BAKE handoff (bake textures have no
  bindings in v1 — PREVIEW_HEIGHT still delegates to the solo proxy;
  this kill changes what that delegation SHOWS, not the handoff lane).
- `SoloChannel::texture_binding`'s group-2 reservation: unchanged.

## Process note

All three files fmt-clean on the first pass. The `height_ramp_target`
fixture's texel values (steps `[0, 85, 170, 255]`) give the golden's
expected bytes by inspection — the ramp is also the lit-golden's
fixture, so one texture serves both consumers' pins.
