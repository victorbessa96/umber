# FalloffCurve::Custom — the curve-payload design (W-GUI-3 follow-up)

Status: DRAFT for the panel round. Owner: dragon. Written 2026-10-17
against HEAD `efd0cbf` (every claim cites file:line in that tree).
This is the GPU-stable-id question the snapshot-tests kill parked:
how a user-authored falloff curve reaches the WGSL `radial_falloff`
switch without renumbering the stable ids.

## The constraint set (what makes this not a variant add)

1. `Dab`'s padding is EXHAUSTED. The four slots are taken: mask_gate
   (byte 32), falloff id (byte 40), angle (byte 44) —
   `crates/umber-gpu/src/paint.rs:60-88`. There is no fifth padding
   slot to smuggle a curve pointer through. Any payload rides a
   NEW binding, not the dab struct.
2. The id space is 0..=9 (`FALLOFF_SMOOTH..FALLOFF_STAMP`,
   paint.rs:90-111). `FALLOFF_STAMP` (9) already broke the
   "one variant = one id" symmetry: it reads binding 4 (the stamp
   texture) instead of switching an analytic curve
   (shaders.rs:532-541). Custom is the SECOND id that carries a
   side-channel — the precedent is built, not hypothetical.
3. The enum is `Copy + Eq + Hash + serde(lowercase)` with
   `as_u32/from_u32` stability guarantees ("never renumber" —
   falloff.rs:86). `from_u32(unknown) -> None` must keep returning
   None for every id the GPU does not know, and the GPU's unknown-id
   fallback must stay Smooth (shaders.rs:570's contract).

## The design: id 10 + a 256×1 curve texture on a new binding

**FALLOFF_CUSTOM = 10.**

- **The payload**: a 256×1 R32Float (or R16Float) texture, one row,
  sampled at `t` (the normalized dab radius) — the same LUT shape
  the compositor's display path already uses for its LUT textures.
  256 taps is visually lossless for any curve a painter draws with
  ≤16 control points; R32Float keeps the CPU `eval` and the GPU
  sample numerically comparable for the parity tests.
- **The binding**: `@group(0) @binding(5) var falloff_lut:
  texture_2d<f32>` in `PAINT_COMPUTE_SHADER` (the paint pass's group
  0 owns bindings 0-4 only — shaders.rs:532-541; the mesh/display
  shader's own group(0) binding(5) `prefilter1_tex` at
  shaders.rs:86 belongs to a DIFFERENT pipeline with its own
  bind-group layout, so there is no collision — group/binding
  numbers are per-pipeline, not global). A 1×1 white fallback bound
  at pipeline construction, exactly the stamp's pattern
  (paint.rs:669-673): a Custom dab with no LUT set paints a solid
  disc (the fallback texel = 1.0), same graceful degradation as the
  stamp's 1×1 white.
- **The compositor**: `PaintCompositor.falloff_lut: RwLock<(Texture,
  TextureView)>` mirroring `stamp`'s lock shape (paint.rs:683-690);
  `set_falloff_lut(&self, samples: [f32; 256])` uploads through the
  queue (the stamp's `stamp_needs_white` lazy-upload pattern avoids
  the no-queue-at-construction gap — reuse it: the white fallback
  uploads on the first splat's encoder).
- **The WGSL**: `FALLOFF_CUSTOM = 10u`; `radial_falloff`'s switch
  gains `case 10: { return textureLoad(falloff_lut, vec2<i32>(
  clamp(u32(e * 255.0), 0u, 255u), 0), 0).r; }` — where `e` is the
  SAME remapped edge parameter the analytic curves use
  (shaders.rs:570's `e = clamp((t - hardness) / max(1-hardness,
  1e-4), 0, 1)`), so hardness keeps meaning "flat core" for custom
  curves too, and the LUT is indexed by edge-parameter not raw
  radius. Hardness=0 makes them identical.
- **The brush side**: `FalloffCurve::Custom` variant; `as_u32` →
  `gpu::FALLOFF_CUSTOM`; `from_u32(10)` → Some(Custom). `eval` on
  CPU: piecewise-linear over the same 256 samples (the preset's
  ControlCurve.evaluate supplies the sampling function —
  preset.rs:131's clamped piecewise-linear). The variant CANNOT be
  `Copy`-payloaded with the samples inline (Copy + 256 floats = a
  1KB enum — no); it stays `#[derive(Clone, Copy...)]` by keeping
  the curve OUT of the enum: the enum gains a `Custom` unit variant
  and the SAMPLES live in `BrushParams` as a sibling field
  `falloff_custom: ControlCurve` (serde(default) = identity — old
  presets load unchanged, same migration as falloff itself,
  kill-report 2026-10-14-falloff-curves.md:18).
- **The panel row** (brush_panel.rs:592-607's combo): selecting
  Custom swaps the combo for the `CurveEdit` widget
  (umber-gui/src/widgets/curve.rs — the g4 census finding's
  consumer), editing `params.falloff_custom`; the combo and the
  editor share the row.
- **The GPU-side write path**: `stamps_to_dabs`/`stamps_to_dabs_wash`
  already call `.with_falloff(params.falloff.as_u32())`
  (kill-report 2026-10-14:18). Custom additionally needs the LUT
  uploaded WHEN THE PRESET CHANGES — a `set_falloff_lut` call in the
  brush apply path, the same place `set_stamp` lives today.

## Why this shape (the alternatives, killed)

- **Uniform array of 4 ControlCurve points** (push the points, lerp
  in WGSL): saves a binding but puts per-dab data in a per-DRAW
  uniform — the curve is per-PRESET, so it changes between splat
  calls; a uniform write between encodes on a shared compositor
  needs the same RwLock discipline anyway, and a 4-point lerp in
  WGSL duplicates `ControlCurve::evaluate`'s exact arithmetic in a
  second language (the parity-test surface doubles). The LUT
  keeps ONE evaluation implementation (CPU) and the GPU only
  samples — the parity test is "sample the same LUT", which is
  exact, not approximate.
- **Storage buffer of curve points**: same uniform problems, plus
  binding-count cost identical to the texture.
- **Inlining samples in Dab**: impossible (constraint 1).
- **A second compute pipeline with a different bind group layout**:
  pipeline permutation for one curve shape — overkill; the stamp
  already proved the texture-in-one-pipeline shape.

## Migration + back-compat

- `FalloffCurve` is serde-lowercase; a new `custom` arm round-trips
  `.umberbrush` files as `"falloff":"custom"`. Old binaries reading
  a new file: `from_u32(10)` → None → the preset loader's unknown-
  variant path (whatever the serde custom impl already does for
  unknown strings — VERIFY at implementation: if it errors, new
  files break old binaries; if it defaults, fine. The falloff
  field is `#[serde(default)]` so the missing-field case is safe).
- The panel combo's `ALL` list gains Custom LAST (falloff.rs:78's
  array order — ids are positional there; `ALL` order and u32 ids
  must stay in lockstep, one more reason ids are never renumbered).
- GPU ids: 10 is free. The `radial_falloff` switch's fallback stays
  Smooth for unknown ids — a Custom dab on an old binary's GPU
  paints Smooth, visually wrong but harmless; no crash.

## Test plan (the host-gated steps)

1. CPU: `eval(Custom)` = the LUT's piecewise-linear read; parity:
  `for t in 0..=255/255.0: cpu_eval(t) == lut_sample(t)` exact
   (same rounding path).
2. Serde: `"falloff":"custom"` round-trips; a preset WITHOUT
   `falloff_custom` loads as identity; `falloff_custom` without
   `falloff:"custom"` is inert (eval only reads it when the enum
   says Custom).
3. GPU (lavapipe goldens, the `goldens.rs` pattern): a Custom dab
   with a step-function LUT (0 below 0.5, 1 above) paints a ring,
   byte-pinned; the same dab with the white fallback paints the
   full disc.
4. WGSL parity: the shader's `radial_falloff(0.7, 0.0, 10)` ==
   the CPU eval at 0.7 (the parity harness already exists for the
   analytic curves — extend it).

## Open questions for the panel / Bessa

1. **R32Float vs R16Float** for the LUT: R32 keeps CPU/GPU parity
   exact; R16 halves the upload. Parity tests favor R32 — recommend
   R32Float unless memory profiling says otherwise.
2. **Per-preset LUT size**: 256 is the proposal; Sub Painter's own
   falloff editor (if it exists) may pin a different count. The LUT
   SIZE is a texture dimension, changeable without touching ids.
3. **The CurveEdit row's placement**: in the falloff combo row
   (in-place swap) vs. a sub-row below. Recommend in-place: the row
   already conditionally shows method params (the stroke-method
   precedent, brush_panel.rs:608's conditional rows).

## Blast radius (the implementation kill's surface)

- `crates/umber-gpu/src/paint.rs` (id 10 + RwLock LUT + setter),
  `shaders.rs` (binding 5 + switch case), `crates/umber-brush/src/
  falloff.rs` (variant + ALL + ids), `preset.rs` (falloff_custom
  field), `dab_adapter.rs` (the apply path's set_falloff_lut call),
  `brush_panel.rs` (the CurveEdit row) — SIX files, one kill, one
  battery round (umber-gpu + umber-brush + umber-app), GPU goldens
  host-gated. The Level editor's consumer (GpuOp::Levels growing a
  CurveEdit param) is the NEXT kill after this one — shared
  dependency, separate surface (graph_eval.rs).
