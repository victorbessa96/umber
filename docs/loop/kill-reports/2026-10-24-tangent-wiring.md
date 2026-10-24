# Kill report — the tangent wiring (row 79's consumer half, the briefed runway's first dev kill)

Executed 2026-10-24 00:5x–02:3x by local-7de7d3ed (the W-C4
recovery-window session), from
`docs/loop/briefs/2026-10-20-tangent-wiring.md` verbatim (the brief's
anchors held — re-derived at dispatch as the standing rule demands).
Foundation: `d7eea57`'s `vertex_tangent_frames` (the mikkTSpace
weld-group module, kill report `2026-10-20-tangent-frame.md`).

## What landed

- `Vertex` grows `tangent: [f32; 4]` (location 3; the stride assert
  moves 32 → 48): the packed frame `[t.x, t.y, t.z, handedness]`.
- `MeshBuffers::upload` computes
  `umber_mesh::tangents::vertex_tangent_frames` on the RESOLVED
  normals/uvs (degenerate UVs get the module's fallback, never a
  panic) and packs each slot's frame into its vertex.
- `MESH_SHADER` interpolates the frame (VertexInput/Output location
  3, `vs_main` passes it); `height_perturbed_normal` switches from
  the dpdx/dpdy derivative form to the TBN form: `T̂ =
  normalize(t_raw − n·dot(n, t_raw))` (re-orthogonalize —
  interpolation degrades), `B = handedness·cross(n, T̂)`,
  `normalize(n − (dhdu·T̂ + dhdv·B))`. The duv/dscreen chain-rule block
  is deleted; the fn's v1-boundary doc block carries the closing
  amendment to the 2026-10-12 judgment (the reversal this kill is).
- The CPU golden mirror switches with it: the flat family's helper
  becomes `tbn_perturbed_normal` (the fixture IS its own frame:
  T = +X, handedness +1, B = +Y) and the calls pass UV-space
  derivatives `(1,0)/(0,1)` — same texel-load quantities the shader
  consumes, one space, not two.
- The NEW golden `height_curved_surface_uses_the_vertex_tangent_frame`
  (the kill's point): a cylinder-segment sliver (θ 0..π/4, y −1..1,
  analytic normals) where the derivative frame and the TBN frame
  genuinely diverge. Expected bytes derived at runtime through the REAL
  `vertex_tangent_frames` (never hand-copied) + the barycentric blend;
  the divergence assert pins that the old Mikkelsen bytes differ by
  more than the tolerance.

## The brief's one expectation, corrected live

The brief asserted the flat goldens' bytes come out IDENTICAL. The
derivation disproved the magnitude half before any edit: on the flat
quad the two forms agree in DIRECTION on both ramps, but the
derivative form's screen-space chain folds in the fixture's
UV-to-world scale (du/dworld = 0.5) while the TBN form consumes the
UV-space gradient directly — the tilt STRENGTH differs. The goldens
were re-derived from the TBN algebra (not weakened: tolerance ≤1
unchanged, the values recomputed exactly) and passed first strike.

## The claw lane + the review catches

The curved golden was delegated to the opencode claw (the standing
claw rule). Its work was strong — the probe discipline (my requested
pixel (row 1, col 3) was OUTSIDE the sliver, w0 = −0.479; it moved to
(row 3, col 3), weights (0.0214, 0.8536, 0.125)), the clamped-ramp
subtlety (the sliver's interpolated u = 0.854 lands in the ramp's top
texel, so the +u difference clamps to 0 → it bound the +v ramp), and
the derived divergence proof: TBN bytes [76, 73, 77] vs Mikkelsen
[110, 109, 119] at the probe pixel.

Three defects survived to review, all caught in-session before commit:
1. E0428 — the helper written twice (identical bodies); one removed.
2. The sliver's index winding projected CW in NDC-xy against the mesh
   pass's `cull_mode: Back` + CCW front — the whole triangle culled,
   the probe pixel read the clear black. Fix: indices [0, 2, 1]; every
   derived value recomputes at runtime, so the one line carries it.
3. The interpolated normal's y slot carried `w1·0.707` where every
   vertex normal's y is 0 — the mirror was fed a normal that never
   existed on the sliver. Fix: y = 0.

The claw's derived TBN bytes were CORRECT throughout — defects 2 and 3
were mirror-side, and the GPU's own render [76, 73, 77] matched the
claw's derivation the moment the culling let the pixel exist.

## Evidence ladder

- `cargo check -p umber-gpu --features gpu --all-targets` clean.
- The flat family through the real adapter: 3/0 first strike after the
  re-derivation (height_stroke_perturbs_the_lit_normal, the flag-off
  regression, solo_height).
- The curved golden after the review fixes: 4/0 family total
  (`test result: ok. 4 passed; 0 failed`), the divergence assert live.
- The battery (Gate 1, the improved one carrying the feature-on check
  leg this kill motivated) on umber-gpu + umber-mesh.
- Gate 2: the commit's CI run (the pinned runner).

## Non-goals held

No bake-pass changes (`bake_shaders.rs`'s orthonormal_basis untouched);
no MeshData change; no solo-height manifest re-pin (the timing
decision: this kill ran post-close on clean files, exactly as the
brief ordered).
