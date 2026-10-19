# The stencil engine (wave-stencil) — design (R-A1 + g6-stencil-overlay)

Status: LANDED + CLOSED 2026-10-19 (see the Closure tail). Owner:
dragon. Written 2026-10-17 against HEAD `7f3d529` (every claim
cites file:line in that tree).
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

## Amendment A (2026-10-18 — the paint hook: the wiring bullet corrected)

The "The paint-path wiring" bullet above is WRONG about the
mechanism, and this amendment records the verified truth (the
stencil-app kill's cross-reviewed headline, commit 5e730b4; every
anchor below re-verified against the tree):

1. The shell CANNOT hand composed bytes to
   `write_mask_region`: every link in that chain is private to
   umber-paint — `stroke_mask_gate` is armed only by the quick mask
   or the cavity (paint_state.rs:1714), `sync_mask_tile` →
   `publish_mask` → `gate_bytes` → `gate_plane` are all private
   (:5218, :5240, :5197, :5171), and the only byte-taking pub,
   `publish_layer_mask` (:2243), is the LAYER-mask lane. Even with a
   lane, two breaks remain: with the quick mask off, nothing arms
   `stroke_mask_gate`, and every resync or incremental stamp
   (`mask_event`, :5074) would overwrite shell-written bytes with
   `gate_plane`'s quick-mask-only product.
2. The composition lives INSIDE `gate_plane`, the way the cavity
   plane folds in (:5183-5191): a pub
   `PaintState::set_stencil_gate(Option<(u32, u32, Vec<u8>)>)`,
   following the `set_cavity_mask` precedent (:5103) — refused
   mid-stroke (one stroke never changes its gate partway), a
   same-value re-set is a no-op, and adoption clears `mask_synced`
   (the `rebuild_cavity_coverage` precedent, :5165 — the next gated
   dab re-uploads).
3. The bytes are the stencil's RAW R8 coverage (0 = blocked,
   255 = passes) at the tile's quick-mask plane size — NOT
   pre-composed with the quick mask. `gate_plane` folds them in as
   f32 (byte / 255.0) after the cavity block, consistent with its
   existing quick × cavity product. The shell-side `compose_gate`
   min stays the STAGED LABEL's contract only (the app report's
   judgment call 7).
4. Dimension match is the per-set scope: `gate_plane` composes only
   for the tile whose quick-mask dims equal the gate's (w, h) — the
   gate stages at the ACTIVE tile's plane (judgment call 8);
   other-sized tiles pass through stencil-ungated until the shell
   re-stages on active-tile change. Malformed bytes (zero side,
   len != w*h) are refused AT the setter, fail-closed (judgment
   call 12).
5. `stroke_mask_gate` arms with `|| stencil_gates()` at :1714.
6. Cost class unchanged: staged once per stencil change on the UI
   thread, never per frame (judgment call 17); the hook inherits
   the change-driven contract.

## Amendment B (2026-10-18 — soft edges: Open question 3 answered)

v1 ships binary (alpha >= 0.5 → 255 else 0). Soft edges are a
RASTERIZE-ONLY change: `rasterize_gate` carries the bilinear
coverage byte (0-255) instead of the threshold. The gate then
composes correctly at BOTH sites, and the divergence between them is
the contract, named here so the soft-edges kill does not re-litigate
it: the SHELL-side staged-label compose is min (the app report's
judgment call 6, pinned by test 3's soft case — min over bitwise &
is what keeps partial coverage honest there), while `gate_plane`'s
internal fold of the three planes is PRODUCT (the quick × cavity
precedent :5171-5179) — identical for binary v1 bytes, and for soft
values the hook consumes RAW stencil bytes so the product fold is
the one in force at paint time. Both choices keep their existing
tests green; neither site changes when soft edges land.


## Closure (2026-10-19 — landed + closed)

Every stage landed in the design's own sequencing, all four steps
plus the amendments recorded in-flight:

- **Step 1** (umber-core transform + rasterize): the wave's
  stencil-core kill (`docs/loop/kill-reports/2026-10-18-stencil-core.md`),
  port-verified + battery-green in the stencil-wave close (`0955ac8`).
- **Step 2** (umber-app state + the seam): stencil-app
  (`2026-10-18-stencil-app.md`), then the paint hook (`3a8b4cb`,
  `2026-10-19-stencil-paint-hook.md`), the panel wiring (`6c95e26`,
  `2026-10-19-stencil-app-wiring.md`), the set-switch re-stage
  (`dcc5e89`, `2026-10-19-stencil-tile-restage.md`).
- **Step 3** (the viewport overlay + chords + the popover row): the
  wave's stencil-viewport kill (`2026-10-18-stencil-viewport.md`),
  then the shell wiring + the N key (`2bb8c60`,
  `2026-10-19-stencil-viewport-shell.md`) and the cross-tile
  observer (`45ad4e4`, `2026-10-19-stencil-tile-observer.md`).
- **Step 4** (this docs tick): the gui-plan g6 row, the ROADMAP
  wave-stencil row, and this status line
  (`2026-10-19-stencil-docs-tick.md`).
- **Amendment A** corrected the original wiring bullet (the
  composition lives INSIDE `gate_plane`; the shell stages RAW
  bytes through `set_stencil_gate`). **Amendment B** answered the
  soft-edges question (rasterize-only; `min` at the label, product
  at the fold — should soft edges ever land, both sites keep their
  own semantics by contract).
- **Open question 1** (image source): the file dialog, v1 as
  built. **Open question 2** (per-set stencils): still a named
  follow-up — v1 is one global stencil. **Open question 3** (soft
  edges): answered by Amendment B; binary v1 shipped.
- The **mid-stroke window** (one stroke never changes its gate
  partway) is the snapshot contract, BY DESIGN; the observer closes
  the cross-tile gap on the first non-stroking frame after the
  stroke ends.
