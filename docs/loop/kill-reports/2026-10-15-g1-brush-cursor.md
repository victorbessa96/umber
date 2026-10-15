# Kill report — g1-brush-cursor (GUI plan G1)

Date: 2026-10-15. Author: dragon. Lock: `g1-brush-cursor` over
`crates/umber-app/src/brush_cursor.rs` (new), `viewport.rs`, `main.rs`
(one `mod` line), `crates/umber-paint/src/paint_state.rs` (two read
accessors), this report. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g1-brush-cursor`. Sources: Painter C1, Blender C4.1, photocraft #1
(`docs/research/gui-research-*-2026-10-15.md`).

## Built

- `brush_cursor.rs` (new): `CursorShape::{Ring { radius_px, core_px },
  Crosshair}` and pure functions:
  - `texels_spanned`: one probe's UV distance × texels-per-UV; rejects
    zero and NaN.
  - `combine_spans`: the right and left probes together. Their mean when
    they agree; the smaller when one is more than `SEAM_RATIO` = 4× the
    other (the larger crossed a seam); the survivor when one missed.
  - `screen_radius`: radius_texels × PROBE_PX / span, clamped to 4096 px.
  - `shape`: a crosshair under 3 px or when the probe failed; the core
    ring at `hardness × radius` only when it is more than 2 px from both
    the rim and the centre.
  - `paint`: two-pass dark-under-light strokes plus a centre dot, theme
    independent.
  - 4 unit tests.
- `viewport.rs::paint_brush_cursor`: drawn last in `ui()`, clipped to
  the viewport. It shows only when a mesh and a paint session exist, no
  eyedropper or ID-mask is armed, the projection paints in the viewport,
  the pointer hovers the mesh, and no orbit drag is in progress. It then
  measures the on-screen radius with probes 8 px right AND left through
  the real `pick_uv`, and sets `CursorIcon::None` while the ring shows.
  - Test `brush_cursor_probe_measures_the_quad`: on the head-on quad
    fixture, the probe measures a finite radius, and halving the camera
    distance grows the ring by more than 1.5×.
- `paint_state.rs`: `brush_radius_texels()` (the live-stroke
  `BRUSH_RADIUS_TEXELS`) and `brush_hardness()`.

## Judgment calls

1. **The radius is measured, not derived.** Casting a second ray 8 px
   away folds camera distance, perspective, surface tilt and the UV
   layout's local texel density into one ratio. An analytic UV Jacobian
   would need per-triangle derivatives and still miss the tilt.
2. **A painted ring, not an OS bitmap cursor.** photocraft's bitmap
   cursor needs platform fallbacks (Wayland ≠ 1×, web, oversize tips).
   It is the follow-up row. The painted ring trails the pointer by one
   frame.
3. **Seams are detected relatively, not by an absolute density cap.**
   The first version rejected spans above 512 texels/px, and its own test
   showed why that is wrong: a distant mesh with a 4K map legitimately
   reaches that. Comparing the two probe sides needs no constant tied to
   texture size or zoom.
4. **The probe costs three ray casts per hovered frame** over the
   whole mesh (`ray_intersect` is a linear scan). On a 100k-triangle
   mesh that is measurable. Under the perf ruling it is recorded, not
   blocking. A BVH for picking is the fix if it shows in the
   input-to-photon p95; follow-up row `wave-pick-bvh`.

## Unverified

- No kittest for `CursorIcon::None` over the viewport. The viewport needs
  a GPU context, which the headless harness does not build. The shape
  math and the probe are tested; the draw call is not.
- Live feel on hardware (ring lag, readability on bright paint):
  manual.

## Deferred

- The OS-bitmap cursor path (photocraft #1 second half).
- Ring size reflecting pen pressure (Painter shows the pressure-scaled
  dab while drawing): needs the per-event radius scale exposed.
