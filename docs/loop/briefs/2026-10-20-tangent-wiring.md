# Brief — the tangent wiring (row 79's named follow-up, the consumer half)

Written 2026-10-20 21:0x by local-4d2e8a71 (the W-C3b session) so the
kill executes from this file instead of re-deriving a day of scoping.
The foundation module is LANDED and parked:
`crates/umber-mesh/src/tangents.rs` (kill report
`docs/loop/kill-reports/2026-10-20-tangent-frame.md` — the weld-group
design and its reasoning live there; read it first).

## Timing (the load-bearing decision)

This kill runs POST-RECOVERY, as a normal wave kill on CLEAN files —
NOT as a fifth parked row. It consumes `shaders.rs` + `renderer.rs`,
which the W-C3b recovery's solo-height row commits; wiring BEFORE that
commit means re-pinning the solo-height manifest (legal but pointless
risk), wiring AFTER means a fresh manifest on clean files with its own
battery. Order: w-c3a-close.sh → w-c3b-close.sh → THIS KILL (with the
W-C4 lanes interleaved per their own plan).

## The current implementation (what gets replaced)

`shaders.rs`, `height_perturbed_normal(n, world_pos, uv)` (search
`fn height_perturbed_normal`, ~:245): the Mikkelsen derivative
surface gradient — `dpos_dx/dpos_dy = dpdx/dpdy(world_pos)`,
`r1 = cross(dpos_dy, n)`, `r2 = cross(n, dpos_dx)`,
`det = dot(dpos_dx, r1)`,
`surf_grad = sign(det) * (dhdx * r1 + dhdy * r2)`,
`normalize(abs(det) * n - surf_grad)`. The `dhdu/dhdv` come from
three texel loads (`h0/hu/hv`) scaled by `dims_f × height_strength`
then chained through `duv/dscreen` via `dpdx/dpdy(tile_uv)` — read the
whole fn before editing; this summary is a map, not a transcription.

The 2026-10-12 heightnormal kill chose this deliberately ("no tangent
attribute required" — its report). The wiring kill REVERSES that
choice per requirements row 79's named follow-up ("true per-vertex
tangents"): the derivative frame is correct per-facet, not across a
curved mesh. The reversal lands as a closing amendment to that kill's
report's judgment, with the curved-surface golden below as evidence.

## The wiring steps (anchors from the 2026-10-20 scoping)

1. **Compute at upload** — `renderer.rs`, `MeshBuffers::upload` (the
   mesh upload path; `textured_quad_mesh`'s buffers in tests flow
   through it): call
   `umber_mesh::tangents::vertex_tangent_frames(&mesh.positions,
   &mesh.normals, &mesh.uvs, &mesh.indices)` — one frame per vertex
   slot; pack `tangent.xyz + handedness` as `[f32; 4]` (the standard
   compact attribute; `B = handedness · cross(N, T)` in the shader).
2. **Vertex data + layout** — `renderer.rs` declares the mesh vertex
   layout at :827 and :914 (`array_stride: size_of::<Vertex>()` —
   BOTH sites share the mesh pass's buffers; extend both). The
   wireframe overlay at :999 uses `WireVertex` (separate — untouched).
   A new `VertexBufferLayout` entry (or extend `Vertex`) + a new
   buffer binding in `MeshBuffers` + write it at upload.
3. **The WGSL** — `shaders.rs`, `MESH_SHADER`: vertex stage gains the
   attribute, interpolates `tangent` (+handedness) to the fragment;
   `height_perturbed_normal` switches to the TBN gradient: with
   `T̂ = normalize(tangent - n·dot(n, tangent))` (re-orthogonalize
   against the interpolated normal — interpolation degrades), 
   `B = handedness · cross(n, T̂)`, perturbed
   `= normalize(n - (dhdu·T̂ + dhdv·B))` with the SAME `dhdu/dhdv`
   texel loads the fn already computes (the UV-space gradient; the
   `duv/dscreen` chain-rule block is DELETED — the TBN frame replaces
   it). Derive the exact algebra before writing; the CPU helper must
   mirror it bit-for-bit.
4. **The CPU golden helper** — `renderer.rs`
   `expected_height_perturbed_byte` (~:4004) mirrors the CURRENT
   derivative formula via the test-local `mikkelsen_perturbed_normal`.
   Both switch to the TBN formula TOGETHER. KEY EXPECTATION: on the
   FLAT quad fixtures (`textured_quad_mesh`, identity-ish UVs,
   T=+X/B=+Y), the two formulas agree analytically — the existing
   goldens' expected BYTES should come out IDENTICAL; re-derive to
   CONFIRM, and do not weaken them if they do.
5. **The NEW golden that proves the point** — a CURVED-surface fixture
   (a cylinder segment: curved in one axis, UVs u=angle, v=height)
   where the derivative frame and the TBN frame give DIFFERENT
   perturbed normals; pin the TBN bytes (hand-derived + the python
   port). Without this golden the wiring is a no-op-shaped change
   that its own fixtures cannot distinguish.
6. **Unaffected surfaces** — the solo-height golden
   (`solo_height_reads_the_bound_height_tile`) never enters the
   gradient fn (solo reads the tile raw); the flag-off regression
   gates the same way (mid-gray → no sampling); `preview_proxy`'s
   PREVIEW_HEIGHT delegates to the solo arm — unaffected.

## Verification ladder

rustfmt; the TBN algebra's python port (the golden helper and the
WGSL must agree with the port); the battery's GPU goldens (the flat
fixtures unchanged + the new curved golden); a read-back render
comparison if the goldens surprise.

## Non-goals

- No tangent work in the bake passes (`bake_shaders.rs`'s own
  `orthonormal_basis` is bake-local and correct for its purpose).
- No MeshData change (frames are upload-time only — MeshData literals
  in parked files must not move).
- No re-pinning of the solo-height manifest (the timing decision
  above makes it unnecessary).
