# Kill report — cavity-mask (paint by curvature, no mask layer)

Date: 2026-10-15. Author: dragon. Lock: `cavity-mask` over
`crates/umber-brush/src/cavity.rs` (new), `crates/umber-brush/src/quick_mask.rs`
(reserved, not touched), this report. Module registration
(`pub mod cavity;` + re-exports in `lib.rs`) was handed to the concurrent
stroke-methods kill, which holds `lib.rs`. Source: ROADMAP wave 8b row
`wave-brush-cavity-mask`; Blender's texture-paint cavity mask.

## Built

- `CavityMask { target: Cavities | Ridges, threshold, width, invert }` with
  `validate`, `gate(curvature_byte) -> f32` and
  `coverage(rgba, w, h) -> Vec<f32>` (a coverage plane in the quick mask's
  format: `1.0` = paint allowed).
- The decode follows the curvature bake's own encoding
  (`CURVATURE_BAKE_SHADER`: `g = (curv + 1) * 0.5`; MeshLab sign, convex =
  negative): the gate reads the baked map directly, no re-bake.
- 6 tests: cavities pass concave and block flat/convex; Ridges mirrors
  Cavities exactly on mirrored bytes across all 256 values, and the two
  never both paint one texel; invert is the complement; `coverage` reads R,
  gates uncovered (`a == 0`) texels to 0, rejects a short buffer; bad
  parameters refuse before the buffer is read; serde round trip.
  Verified standalone (the module compiled alone with its three deps): 6/6.

## Judgment calls

1. **Read the baked curvature map, not per-vertex cavity.** Blender's
   cavity mask is vertex-based; Umber already bakes texel-resolution
   curvature, which is finer and needs no new pass.
2. **A coverage plane, not a new gate path.** The output has the quick
   mask's format, so wiring it is "multiply into the gate", using the GPU
   mask upload the quick mask already has (`PaintState::publish_mask`). No
   new shader binding.
3. **Uncovered texels gate 0** (blocking is the safe failure: a mask must
   never leak paint).

## Deferred

- The paint-session wiring: compose `coverage` into the per-tile mask the
  shader samples, re-upload when the curvature map or the params change, and
  arm the gate per stroke. This touches `umber-paint/src/paint_state.rs`
  (mask sync), the bake → paint hand-off (which curvature map is live), and
  the brush panel row. That is the follow-up kill.
- Persisting the params in `.umberbrush`.
