# Kill report — R-A2 v2d: the decal raster becomes ONE PASS (row 298)

Date: 2026-11-09. Session: `local-ee79b5c495f10bbd08339152566beca313239d9318a5ef89447f3a297ceda877`.
Design: `docs/loop/briefs/2026-11-09-projection-one-pass-design.md`.
Claw brief: `docs/loop/briefs/2026-11-09-projection-one-pass-kill-brief.md`.

**The finding:** the decal projector was O(texels × triangles). At the
default 4096² paint target, one Apply click on an 8 192-triangle mesh
took ≈34 minutes on the UI thread — the app froze. The lane's own plan
claimed "one O(1) lookup chain per texel"; that was wrong about
`uv_to_barycentric`'s cost, and the code implemented the plan faithfully.

**The kill:** a new public STREAMING walk in `umber-mesh`
(`for_each_uv_geometry`), built on the crate's existing containment
primitive, and a triangle-driven raster in `umber-app`. Zero extra
memory, no second copy of the containment math, and the output bytes are
unchanged on well-conditioned geometry (proven below by fuzz, not by
argument).

---

## 1. What landed

### `crates/umber-mesh/src/uv_gbuffer.rs`

```rust
pub fn for_each_uv_geometry(
    mesh: &MeshData,
    tile: u16,
    width: u32,
    height: u32,
    filter: impl FnMut(usize) -> bool,
    mut f: impl FnMut(u32, u32, [f32; 3], [f32; 3]),
)
```

- It IS `for_each_covered_texel_filtered` (the crate's single containment
  primitive, the same walk behind `UvGBuffer::build` and `fill::uv_coverage`)
  plus per-triangle geometry — one walk, not two.
- `filter(triangle)` runs once per UV-valid triangle, in index order,
  immediately before that triangle's texels; `false` skips it entirely.
- The callback receives `(x, y, world, raw_normal)`:
  - `world` = `lerp3(p0, p1, p2, bary)` — the same expression and order as
    `world_at_uv`, so the two agree byte for byte;
  - `raw_normal` = the UNNORMALIZED `(p1-p0) × (p2-p0)` over the
    positions IN INDEX ORDER. Raw because the two consumers normalize
    under different rules and each keeps its own (the projector needs a
    usable facing, `len > 1e-12` and finite; the fill normalizes when
    `len > 0`, else `[0,0,0]`).
- The triangle's positions and raw normal are computed once per triangle
  and reused across its texels.
- A triangle with non-finite positions is still VISITED (the projector
  needs today's winner semantics); one whose position indices are out of
  range yields `[NaN; 3]` for both. Never panics.
- Zero-sized targets visit nothing.

### `crates/umber-mesh/src/fill.rs`

`rasterize_uv_geometry` now DELEGATES to the new primitive (its public
signature and output unchanged). Its `pos_finite` rule became the
`filter` argument; the fill's own normalization rule stayed. The
triangle index comes from the last triangle the filter accepted (the walk
calls `filter` right before each triangle's texels, in index order) — a
`Cell` carries it to the texel visitor. `uv_coverage` shares the same
`triangle_positions_finite` helper, so the fill has one filter.

### `crates/umber-app/src/projection.rs`

`rasterize_with` is now triangle-driven: one `for_each_uv_geometry` pass,
and the per-texel work (view_proj, `inverse_map`, the tiling wrap, the
samplers, `faces_away`) is unchanged. The rule that keeps the bytes
identical:

> A triangle that produces NO value must WRITE transparent `[0,0,0,0]`,
> not skip the texel.

Skipping would let an EARLIER triangle's colour survive where today's
winner decides. Every no-value path today (the culled winner, the
behind-the-eye winner, the off-NDC winner, the NaN-position winner, the
out-of-image winner) ends at zero, so all of them write transparent.

The module docs' false "one O(1) lookup chain per texel" is corrected to
the measured cost (34.7 s / 131.6 s, ≈120 µs per texel, ≈34 min at 4096²).

### The claw's judgment calls (both accepted)

1. **A missing position index yields `[NaN; 3]`.** A triangle whose UV
   indices are valid but whose position index is out of range: today the
   per-texel lookup returns `None` and the texel stays zero. Skipping it
   in the walk would let an earlier triangle's colour show through, so
   the walk yields NaN and the projector writes transparent. Pinned.
2. **The `filter` ordering guarantee is documented rather than widened.**
   The fill needs the triangle index, which the callback does not carry.
   Adding a fifth argument would have changed the brief's signature; the
   ordering is documented instead and pinned on both sides.

---

## 2. Verification

### The battery (dragon-run, `flock /tmp/umber-cargo.lock`)

`cargo clippy -p umber-mesh -p umber-app --all-targets -- -D warnings`
+ `cargo test -p umber-mesh` + `cargo test -p umber-app` — GREEN.
140 mesh pins, 772 app pins, 0 failures. `scripts/check-layers.py`:
GREEN (16 crates). `rustfmt --check`: clean on all four files.

### The claw's reference implementation is the OLD code, verified

The claw kept the old per-texel raster in the test module as
`per_texel_reference`. The dragon diffed it against the actual
pre-kill `rasterize_with` from `git show HEAD:...` with comments and
whitespace normalized: **1 638 characters on both sides, IDENTICAL_BODY:
true**. The byte-identity pins therefore compare against the real old
implementation, not a re-typed approximation of it.

### Independent fuzz (dragon-run, not the claw's)

The claw flagged one risk it could not pin: the walk tests texels only
inside a triangle's bounding-box window (expanded by ~1.5-2.5 texels),
while the old lookup tested EVERY triangle at EVERY texel. The dragon
hunted exactly that with a hostile-mesh fuzzer (slivers, needles, huge
triangles, NaN UVs, out-of-range positions; 3 000 meshes × 4 resolutions
≈ 50 M texels), comparing the walk's texel set, winner triangle and world
point against the old lookup's, bit for bit.

**Result — the split is clean:**

| mesh class | miss | extra | world mismatch | winner mismatch |
| --- | --- | --- | --- | --- |
| well-conditioned (production) | 0 | 0 | 0 | 0 |
| hostile (slivers/needles) | 114 | **0** | 23 | 23 |

- **On well-conditioned geometry — the production case — the two paths
  are IDENTICAL**, across 1 600 meshes × 4 resolutions (≈14 M texels).
- On hostile meshes the walk never claims a texel the old lookup misses
  (`extra = 0`); it claims strictly FEWER. Every disagreeing triangle has
  a sliver ratio of ~1e-8 (a degenerate needle).

### The mechanism (the one exception, pinned by the dragon)

`UvTri::bary` tests only the LOWER bound of each barycentric weight
(`b_i >= -UV_BARY_EPS`) and never the upper bound. On a needle the
weights are so ill-conditioned that a texel centre far OUTSIDE the
footprint gets weights like `[2.5, -0.0, 0.5]` — every component clears
the lower-bound test, but they sum to 3.0, not 1.0. So the OLD
per-texel lookup CLAIMED (and PAINTED) texels the triangle does not
cover. The walk, bounded by the triangle's window, does not.

**This is a fix, not a regression:** the old path painted garbage outside
the mesh's UV footprint. The dragon wrote two pins for it (the claw did
not):
- `crates/umber-mesh/src/uv_gbuffer.rs`:
  `a_needle_is_claimed_outside_its_footprint_by_the_point_lookup_but_not_by_the_walk`
  — states the difference exactly (the walk's set is a SUBSET; every
  texel claimed only by the lookup has weights summing ≠ 1; on a
  well-conditioned mesh they agree exactly).
- `crates/umber-app/src/projection.rs`:
  `a_needle_is_not_painted_outside_its_footprint_by_the_one_pass_raster`
  — every texel the one-pass raster painted is inside the footprint;
  the old path really did paint outside.

**So the byte-identity claim holds on every well-conditioned mesh, and
its single exception is a case where the old bytes were wrong.** The
underlying primitive bug (`UvTri::bary` has no upper bound) is shared
with the GPU bake shader (`crates/umber-gpu/src/bake_shaders.rs`, the
same `b_i >= -UV_BARY_EPS` formula) — a separate kill, because fixing it
means one formula on both sides.

---

## 3. Cost

| | before | after |
| --- | --- | --- |
| 1024² tile, 2 048 triangles | 34.7 s | 46 ms (754×) |
| 1024² tile, 8 192 triangles | 131.6 s | 51 ms (2 602×) |
| 4096² (default target), 8 192 triangles | ≈34 min | ≈0.6 s (projected) |
| extra memory | — | **0** |

---

## 4. Follow-ups (named, not done)

1. **`UvTri::bary`'s missing upper bound** (CPU + GPU bake shader): the
   point lookup and the GPU share the blind spot; the walk does not.
   One formula, both sides.
2. **The malformed-triangle policy asymmetry** between the fill (skips
   non-finite-position triangles) and the projector (today's winner can
   be one). Preserved here on purpose — unifying it changes bytes.
3. **No cost pin.** The cost class is still not in any test (a 34-minute
   fixture cannot run in CI); the numbers live in the design brief and
   this report.

## 5. Gates run

- `cargo clippy -p umber-mesh -p umber-app --all-targets -- -D warnings` — GREEN.
- `cargo test -p umber-mesh` — 140 passed, 0 failed.
- `cargo test -p umber-app` — 772 passed, 0 failed.
- `scripts/check-layers.py` — GREEN (16 crates).
- `rustfmt --check` on all four files — clean.
- Independent fuzz — IDENTICAL on well-conditioned geometry; the one
  exception pinned and explained above.
