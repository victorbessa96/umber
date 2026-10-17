# The stencil engine (wave-stencil) — design (R-A1 + g6-stencil-overlay)

Status: DRAFT for implementation. Owner: dragon. Written 2026-10-17
against HEAD `7f3d529` (every claim cites file:line in that tree).
This is the engine g6-stencil-overlay (gui-plan:143) is blocked on:
a paint-through screen mask with transform chords — the logo/decal
workflow (ROADMAP:146).

## What the user sees (the row's contract)

An image overlay in the viewport you move/scale/rotate and paint
THROUGH: strokes land only where the stencil image is opaque. The
overlay itself is a viewport-drawn quad (the overlays family,
display_panel.rs:176's OverlaySettings), the gizmo chords are
S+LMB rotate (Shift snaps 90°), S+RMB scale, S+MMB move, a Reset
button, N ignores (paint passes through the mask entirely), and an
opacity control (the OVERLAY's drawn opacity, separate from the
mask's coverage).

## The constraint set (grounded in the paint pass as it exists)

1. **Dab's padding is exhausted** (paint.rs:60-88 — the falloff
   Custom design already established this). A per-dab stencil
   pointer CANNOT ride the dab struct.
2. **The gate precedent exists and is exactly right**: the quick-mask
   texture (binding 3, paint.rs:655+, write_mask_region's
   `(zero = blocked)` R8 texture, target-sized) multiplies a dab's
   per-texel coverage when `mask_gate = 1.0` (paint.rs:72-74). A
   stencil is a second, INDEPENDENT gate with the same shape.
3. **The sampling precedent exists**: FALLOFF_STAMP samples an
   image through the dab path (binding 4, shaders.rs:538-558) with
   a manual bilinear textureLoad — no sampler needed.
4. **The transform must be per-STROKE, not per-dab**: the stencil
   is one screen-space quad while a stroke emits many dabs. The
   mask the paint pass needs is the STENCIL IMAGE RASTERIZED AT
   TARGET RESOLUTION in its transformed position — a per-texel gate
   array, exactly the quick-mask's shape.

## The design: CPU-rasterize the transformed stencil into a
write_mask_region-shaped gate

**No new GPU binding. No shader change at all.**

- **The stencil state** (session-owned, umber-app): `StencilState {
  image: Option<StencilImage>, transform: StencilTransform, active:
  bool, overlay_opacity: f32 }`. `StencilImage` is an R8 coverage
  array + width/height (loaded from a PNG through the existing
  image path; alpha >= 0.5 = 255 else 0 — a binary gate, no partial
  coverage in v1). `StencilTransform { center: [f32;2] (screen
  px), scale: f32, rotation: f32 (radians) }`.
- **The gate rasterization** (a pure fn, `stencil_rasterize`):
  inverse-transform each TARGET texel's center through the
  transform (screen->stencil-image space), bilinear-sample the
  coverage at that point, threshold, write the byte. Empty when
  the stencil is inactive/N-ignored. This is CPU work ONCE PER
  TRANSFORM CHANGE, not per stroke — the same cost class as the
  quick-mask upload.
- **The paint-path wiring**: after each transform change (and on
  stencil activation), the shell rasterizes the gate at the
  current target's resolution and uploads through the EXISTING
  `write_mask_region(0, 0, w, h, values)` — the quick-mask's own
  lane, serialized on the encoder before the next splat. The
  dabs' `mask_gate` is ALREADY set by the quick-mask path; the
  stencil composes by ANDing into the same bytes (the quick-mask
  and the stencil both write the gate texture — the shell owns
  the composition order: stencil gate AND quick-mask gate =
  the byte the texture holds).
  - **Why not a second gate field**: one gate texture with
    composed bytes keeps the shader unchanged and the semantics
    obvious (a texel is paintable iff the composed gate says so).
    Two gates would need a new binding + shader arm for zero new
    behavior.
- **The overlay draw** (viewport.rs): the transformed quad drawn
    at `overlay_opacity` using the existing overlay painter paths
    (the same layer the wireframe/grid draw at), textured with
    the stencil image. Pure drawing — the gate is what the paint
    pass reads, the overlay is what the eye sees.
- **The chords** (g6's spec): S+LMB rotate (Shift = 90° snap —
  the angle-snap precedent from g1-straight-line-preview's
  Ctrl+Shift 5° snap), S+RMB scale, S+MMB move, Reset
  (transform back to fit-center, scale 1, rotation 0), N toggles
  `active` off (the gate rasterizes empty; the overlay hides).
- **The pure halves** (testable headless): `StencilTransform`
  (its inverse-mapping math), `stencil_rasterize` (the gate
  bytes for a given image+transform+target size), the
  chord->transform mapping (the g1-nav-presents' event->op
  precedent), and the fit-to-viewport Reset computation.

## What lands in which crate (the blast radius)

- `umber-core`: the transform + rasterize fns (pure, no GPU, no
  egui — the engine's math core, testable everywhere).
- `umber-app`: `stencil.rs` (new — the session state + the image
  load + the shell wiring), `viewport.rs` (the overlay quad draw
  + the chord input block), `display_panel.rs` (the Overlays
  popover's stencil row: enable, opacity, Reset — i18n keys in
  BOTH catalogs).
- `umber-gpu`: NOTHING. The whole point of the shape.

## The test plan

1. Pure (umber-core): the transform's inverse round-trips
   (forward then inverse = identity within 1e-6 for interior
   points); rasterize on a known 4x4 image + identity transform
   = the image's own bytes; a 2x scale centers-correctly; a
   90° rotation permutes as pinned; an out-of-coverage texel =
   0. A Python port + mutation rung per the ladder.
2. Shell (umber-app, pure): the chord mapping (S+LMB drag delta
   -> rotation; Shift snaps; S+RMB -> scale with a min clamp;
   S+MMB -> translate); N toggles active; Reset's fit math.
3. Gate wiring: rasterize->write_mask_region byte equality (the
   same values the upload carries); the AND-composition with a
   quick-mask region (stencil 255 AND quick 0 = 0).
4. Kittest: the Overlays popover row renders and toggles;
   the overlay quad draws when active.

## Open questions (the panel's call, none blocking the engine)

1. **The image source**: file-dialog PNG only in v1, or also
   the clipboard? (The mesh_map_import.rs dialog precedent is
   the cheap path.)
2. **Sub-stencil (per-set)**: v1 is one global stencil; per-set
   stENCIL state is the natural follow-up once the overlay's
   per-set UI exists (the texture_sets panel's shape).
3. **Soft edges**: alpha >= 0.5 binary now; the gate COULD carry
   the coverage value (0-255 = partial paint) — the shader
   already multiplies by the byte, so soft stencils are a
   rasterize-only change later. Recommended: binary v1, soft as
   the named follow-up.

## Sequencing

1. `umber-core` transform+rasterize (the engine) — own kill,
   battery umber-core.
2. `umber-app` stencil.rs + the gate wiring — battery umber-app
   + umber-core.
3. The viewport overlay + chords + popover row — battery
   umber-app (+ the kittest).
4. Docs tick (gui-plan g6 row, ROADMAP wave-stencil, queue).
