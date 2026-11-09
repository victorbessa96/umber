# Design brief — R-A2 v2d: the decal raster is ONE PASS, not one scan per texel

Date: 2026-11-09 (row 298). Parent: `docs/specs/projection-fill-plan.md`
(v1, rows 277-281), v2a (row 287, tiling), v2b (row 289, follow camera),
v2c (row 297, backface culling).

**This is a PERFORMANCE kill, not a feature.** The output must be
byte-identical to today's. The finding is that today's output is
computable in ~0.6 s and is instead computed in ~34 minutes.

## 1. The defect, measured

`rasterize_with` (`crates/umber-app/src/projection.rs`) is target-texel
driven: for every texel of the target tile it asks the mesh "which
triangle covers this texel's UV?" through
`umber_mesh::world_and_face_normal_at_uv` → `uv_to_barycentric`, which
scans **every triangle in the mesh** (`uv_gbuffer.rs`: "O(triangles);
per-texel callers use `UvGBuffer::build`, which culls by bounding box").

So the raster costs **O(texels × triangles)**.

Measured on this machine (a probe under `~/.hermes/cache/scratch/`, a
flat UV grid mesh, release build):

| triangles | tile | current per-texel path | one-pass `UvGBuffer::build` | speedup |
| --- | --- | --- | --- | --- |
| 2 048 | 1024² | 34.7 s | 46 ms | 754× |
| 8 192 | 1024² | 131.6 s | 51 ms | 2 602× |
| 8 192 | 256² | 7.8 s | 3.0 ms | — |
| 8 192 | 512² | 32.2 s | 9.8 ms | — |

The per-texel cost is constant per texel (≈120 µs at 8 192 triangles), so
it scales linearly with the tile: **at the default 4096² paint target
(`DEFAULT_TARGET_SIZE`), one Apply click on an 8 192-triangle mesh takes
≈34 minutes of wall clock** — and the call runs on the UI thread
(`projection_section` → `Viewport::apply_projection` →
`PaintState::apply_projection`), so the app FREEZES for the whole time.

Every pin in the lane passes because every fixture is a 2-triangle quad
on a 16×16 tile: the cost class was never in the test. (The lane's own
plan claimed "one O(1) lookup chain per texel" — the plan was wrong
about `uv_to_barycentric`'s cost, and the code faithfully implemented
the plan.)

This defect is PRE-EXISTING (v1's `world_at_uv` per texel was the same
scan); v2a/v2b/v2c inherited it. It is not a v2c regression.

## 2. The fix: one triangle-driven pass

`umber-mesh` already owns the primitive that makes this cheap:
`for_each_covered_texel_filtered` (`uv_gbuffer.rs`) walks the mesh's
triangles ONCE and visits each texel a triangle's UV footprint covers.
`UvGBuffer::build` is that walk with the hits collected into a buffer.

The raster becomes triangle-driven:

```
for each triangle T of the mesh (index order):
    for each texel X of T's UV footprint:
        world  = barycentric point of T at X
        normal = geometric face normal of T
        project world through the camera; sample the image; write X
```

The output buffer keeps its current shape (one RGBA8 texel per target
texel). **The write order changes, the bytes do not** — see §3.

Memory: **zero extra**. The current code allocates the same output
buffer; a per-texel geometry buffer would cost 400 MiB at 4096² (or
512 MiB for an `Option<UvHit>` g-buffer) — that is why the walk is
STREAMING, not buffered.

## 3. Why the bytes are identical (the rule that makes this safe)

Today's winner rule: for each texel X, the covering triangle is the
**LAST triangle in index order** whose UV footprint contains X's centre
(`uv_to_barycentric` keeps the last hit it finds). A triangle-driven
walk visits the triangles in the same index order, so the LAST triangle
that writes X is the same triangle.

Therefore the fix needs one rule the per-texel code got for free:

> **A triangle that does not produce a texel value must WRITE
> transparent `[0, 0, 0, 0]` to that texel, not skip it.**

Skipping would leave an EARLIER triangle's colour in the buffer, where
today the winner's own value (or nothing) is what survives. Every path
that ends in "no value" today leaves the texel at zero:
the culled winner (back-facing, when culling is on), the behind-the-eye
winner, the off-NDC winner, the NaN-position winner, and the
out-of-image winner (the sampler's CLEAR). All of them write transparent
in the triangle-driven version.

The reverse also holds: a texel covered by NO triangle is never written
by either version, and stays transparent.

**Parity is a property to PIN, not to argue.** The pins below compare
the new raster against the current one byte for byte, on meshes built to
exercise every branch (see §5).

## 4. Where the code lives

One new public primitive in `umber-mesh`, one rewrite in `umber-app`:

- **`crates/umber-mesh/src/uv_gbuffer.rs`** — a PUBLIC streaming walk
  that yields each covered texel with its geometry:

  ```rust
  pub fn for_each_uv_geometry(
      mesh: &MeshData,
      tile: u16,
      width: u32,
      height: u32,
      filter: impl FnMut(usize) -> bool,          // per TRIANGLE index
      f: impl FnMut(u32, u32, [f32; 3], [f32; 3]), // x, y, world, RAW normal
  )
  ```

  - It is the EXISTING `for_each_covered_texel_filtered` walk (the crate's
    single containment primitive) plus the per-triangle geometry, so the
    two never drift.
  - `filter` is the per-triangle skip hook (the shape
    `for_each_covered_texel_filtered` already has). The projector passes
    `|_| true`; the FILL passes its own rule (see below).
  - The normal is yielded **RAW** (unnormalized cross product) so each
    consumer keeps its OWN normalization rule:
    - the projector: `len > 1e-12 && finite` → a usable facing, else
      "no facing" → KEEP the texel (today's `FACE_NORMAL_EPS` rule);
    - the fill: `len > 0.0` → normalized, else `[0, 0, 0]` (today's
      `rasterize_uv_geometry` rule).
    Two rules, one walk — no second copy of the geometry math.

- **`crates/umber-mesh/src/fill.rs`** — `rasterize_uv_geometry` DELEGATES
  to the new primitive (with its current `pos_finite` filter), so the
  mesh crate has ONE walk. Its output must stay byte-identical (its
  existing pins cover it).

- **`crates/umber-app/src/projection.rs`** — `rasterize_with` becomes
  triangle-driven through the new primitive.

## 5. The pins (the kill's verification contract)

Mesh crate:
- The new walk yields the SAME texel set as `UvGBuffer::build` and the
  same winner per texel (last in index order) — pinned against
  `uv_to_barycentric` on a mesh with overlapping UV islands.
- A triangle with non-finite POSITIONS is still VISITED (the projector
  needs today's winner semantics); the fill's `pos_finite` filter still
  skips it. Pinned both ways: the primitive's filter is honoured, and
  the unfiltered walk still yields the malformed triangle's texels.
- `rasterize_uv_geometry`'s output is byte-identical to today's (its
  existing pins + a new one against the delegated path).

App crate — the byte-identity contract, against the CURRENT raster
(the current code must be kept in the test file as the reference
implementation, the way `v1_reference` already is):
- A multi-triangle mesh: the new raster equals the old one, byte for
  byte, with culling OFF and ON.
- A mesh with OVERLAPPING UV islands: equal bytes (the winner rule).
- A mesh with a NaN-position triangle: equal bytes (today the NaN
  winner blanks the texel; the new one must too).
- A back-facing winner with culling ON: the texel is TRANSPARENT (not
  an earlier triangle's colour) — the §3 rule, pinned.
- Tiling ON and follow-camera ON compose (the v2a/v2c pins keep their
  meaning).
- The degenerate-normal rule keeps its meaning (a malformed triangle
  does not silently kill paint).

## 6. Non-goals

- **NO change to the output bytes.** This kill changes the COST, not the
  result. Any byte change is a different kill (and a much bigger
  conversation).
- The malformed-triangle POLICY asymmetry between the fill (skips
  non-finite-position triangles) and the projector (today's winner can be
  a non-finite-position triangle) is PRESERVED, not unified — unifying it
  would change bytes. Named here as a follow-up: one mesh-wide policy
  for malformed triangles.
- No depth/occlusion culling (still deferred), no seam/symmetry
  expansion of the projection (still a product question), no decal
  channel, no bake family (plan §7).
- No change to the projection's per-texel WORK (image sampling, the
  transform, the tiling wrap, the backface test) — only how the mesh
  geometry for a texel is obtained.
- No persistence, no new UI, no new dependency.
