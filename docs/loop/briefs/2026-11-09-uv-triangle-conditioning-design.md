# Design brief — W1.1: the UV triangle conditioning rule (one formula, CPU and GPU)

Date: 2026-11-09 (wave 1, kill 1.1). Parent: the R-A2 v2d kill
(`docs/loop/kill-reports/2026-11-09-projection-one-pass.md`), which
found this bug with a hostile-mesh fuzz.

**This is a CORRECTNESS kill in the crate's central primitive.** One
rule, both sides (the CPU authority and the GPU bake shader that mirrors
it), and it fixes the decal projector, the fill, the bakes, the export
padding and the GPU at once.

---

## 1. The bug

`UvTri::bary` (crates/umber-mesh/src/uv_gbuffer.rs) computes the three
barycentric weights from the edge functions and tests:

```rust
(b[0] >= -UV_BARY_EPS && b[1] >= -UV_BARY_EPS && b[2] >= -UV_BARY_EPS).then_some(b)
```

The test is the LOWER bound only. A barycentric weight may therefore
exceed 1 without failing the test. And on an ill-conditioned triangle
the weights are not a decomposition at all.

**The measured case** (the exact fixture the fuzz found):

```
uvs   = [[0.1614995, 0.16149949], [0.35829717, 0.3582972], [0.57094914, 0.57094914]]
area  = -1.4901161e-8        (|area| > UV_DET_EPS = 1e-10: a VALID footprint)
ratio = |area| / longest_edge² = 8.888e-8

texel 15, 0   uv (0.96875, 0.96875)   bary [0.0, 0.0, 2.0]
texel  9, 6   uv (0.59375, 0.59375)   bary [0.0, 0.0, 1.0]
```

Texel (15, 0) is FAR outside the triangle's UV footprint (its bounds are
`[0.1614995, 0.16149949] .. [0.57094914, 0.57094914]`), yet the point
lookup CLAIMED it: the weights reconstruct to `2 * uv2 = (1.1419,
1.1419)`, not to the texel centre. The decal projector PAINTED it.

The GPU bake shader (`crates/umber-gpu/src/bake_shaders.rs`, the
`UV_GBUFFER_SHADER`) quotes the same formula and the same lower-bound
test, so it has the same blind spot.

## 2. Why the obvious fixes do not work (measured — do not re-derive)

Three candidate criteria were measured against f64 ground truth (the
same formula at f64 precision) over the hostile corpus:

| candidate | result | verdict |
| --- | --- | --- |
| per-point upper bound (`b_i <= 1 + UV_BARY_EPS`) | garbage 272→216, **lost claims 7→16**, lookup-vs-walk set diff 31→**54** | **REJECTED** — makes it worse |
| per-point reconstruction tolerance (`\|sum(b_i u_i) - p\|`) | not separable: legitimate claims of a thin triangle carry up to **3.5e-1** reconstruction error | **REJECTED** |
| **per-triangle conditioning** | garbage 272→**0**, lookup-vs-walk set diff 31→**0** | **ACCEPTED** |

The per-point routes fail for a structural reason: on an ill-conditioned
triangle the weights are garbage for SOME points and merely imprecise
for others, so no per-point tolerance separates "wrong" from "imprecise".

The per-triangle route works because the conditioning is a property of
the TRIANGLE, not of the query point: if `|area| / longest_edge²` is
tiny, f32 cannot represent the weights reliably for ANY point.

## 3. The rule

**A triangle whose normalized area is below `UV_COND_EPS` is not a valid
footprint: it covers nothing.**

```
norm_area = |area| / longest_edge²
valid iff  norm_area >= UV_COND_EPS
```

- `UV_COND_EPS = 1e-6` (the measured threshold; see §4).
- The measure is the triangle's normalized area — scale-free, so a
  needle is a needle whether the mesh's UVs span 0..1 or 0..1000.
- `longest_edge` is the longest of the triangle's three UV edges.

**The rule is a VALIDITY rule, not a tolerance:** a triangle below the
threshold is treated exactly like a degenerate one today — it is not a
footprint, it covers nothing, and it is skipped by every consumer.

### Where the rule lives

**ONE place:** `UvTri::of` / `UvTri::from_uvs` in
`crates/umber-mesh/src/uv_gbuffer.rs`. Every CPU consumer goes through
`UvTri::of` (the G-buffer walk, the point lookup, the fill, the export
padding), so one change covers them all — and a triangle that fails the
rule is skipped BEFORE any of them sees it.

**The GPU mirrors it through the existing channel.** The bake shader
receives a per-triangle record whose `uv2.z` is the signed area, and
`crates/umber-bake/src/gbuffer.rs` already uploads an ALL-ZERO record
(area = 0) for every triangle `UvTri::of` rejects — the shader's
`abs(area) < UV_DET_EPS` check then skips it. So the CPU-side validity
rule propagates to the GPU with NO shader change, PROVIDED the upload
path uses `UvTri::of` (it does — `gbuffer.rs:107`).

**This is the property to verify, not assume:** after the change, a
needle mesh must produce IDENTICAL coverage on the CPU and the GPU.
That is a NEW parity golden (there is no needle in the parity corpus
today — `overlapped_quad` is well-conditioned).

### What the rule does NOT change

- The `UV_DET_EPS` area test stays (it catches the exactly-zero-area
  case; the conditioning rule catches the thin-but-nonzero case).
- `UV_BARY_EPS` and the lower-bound test stay (the edge-INCLUSIVE
  semantics: a texel centre exactly on an edge is inside).
- The overlap rule (last in index order wins) stays.
- The degenerate-normal rule in the projector (a malformed triangle
  never silently kills paint) stays — that is wave 1.2's separate
  question, not this kill's.

## 4. The measured threshold (why 1e-6)

Probes under `~/.hermes/cache/scratch/probe-raster-cost/` (the probe
sources are in that directory; re-run them if you want the raw numbers).

**Coverage cost** — the rule's only cost is the degenerate needles:

| needle family (eps) | norm_area | f64 claims | lost by the rule |
| --- | --- | --- | --- |
| 1e-1 … 1e-6 | 1.25e-1 … 1.28e-6 | 13-28 texels | **0** |
| 1e-7 | 1.164e-7 | 13 texels | 13 (the whole family) |

**Production impact** — triangles the rule rejects at each threshold:

| threshold | grids rejected | random triangles rejected |
| --- | --- | --- |
| 1e-5 | 0 / 10 912 | 0 / 4 263 |
| **1e-6** | **0 / 10 912** | **0 / 4 263** |
| 1e-4 | 0 / 10 912 | 2 / 4 319 |
| 1e-3 | 0 / 10 912 | 4 / 4 277 |
| 1e-2 | 0 / 10 912 | 81 / 4 295 |

At **1e-6** the rule touches NO production-shaped triangle (a UV grid
or a random triangle in [0,1]²) while it kills every degenerate needle
in the corpus.

**Why not lower (1e-7)?** The first garbage claims appear at
norm_area ≈ 3.7e-7; a threshold of 1e-6 sits above that with margin
and still rejects nothing in the production range.
**Why not higher (1e-5)?** 1e-5 also rejects nothing, but 1e-6 is
tighter (fewer triangles dropped) at the same measured cost, so 1e-6 is
the choice.

**The property the rule buys** (the one the projector needed): with it,
the point lookup's texel set and the windowed walk's texel set agree
EXACTLY (set diff 31 → 0 over the corpus). Before it, a triangle
claimed texels through one path and not the other.

## 5. The pins (the kill's verification contract)

### `crates/umber-mesh/src/uv_gbuffer.rs`

1. **A needle covers nothing.** The fuzz's exact fixture: `UvTri::of`
   returns `None` for it, and both lookup paths claim no texel for the
   mesh.
2. **A well-conditioned triangle is untouched.** A UV grid's triangles
   are all valid (assert the count of rejected triangles is 0 over a
   grid mesh), and its coverage is byte-identical to today's.
3. **The threshold boundary is pinned.** A triangle just above 1e-6 is
   valid; one just below is not. Pin BOTH sides, with the numbers, so a
   future threshold change has to face them.
4. **The rule is scale-free.** The same needle shape at two UV scales
   (e.g. ×1 and ×1000) is rejected by the same rule.
5. **The two lookup paths agree.** On a hostile corpus (the fuzz's
   generator: slivers, needles, huge triangles, NaN UVs, out-of-range
   positions), the point lookup's texel set EQUALS the windowed walk's.
   This is the pin that carries the kill's real value.
6. The existing pins keep passing (the `UV_DET_EPS` boundary pin, the
   winding pins, the overlap pin, the malformed-triangle pins).

### `crates/umber-bake/src/gbuffer.rs` (the CPU/GPU parity)

7. **A needle mesh: CPU and GPU agree on EMPTY coverage.** The new
   parity golden (the corpus has no needle today). If the GPU still
   claims a texel the CPU drops, the upload path does not use
   `UvTri::of` — that is the failure this pin exists to catch.
8. The existing parity goldens keep passing (they need a GPU; the
   `device()` helper returns `None` and the test SKIPS without one —
   note that in the report if it skipped).

### The crate docs

9. The `uv_gbuffer` module docs and the `UvTri` docs name the new rule,
   the new constant, and WHY (the measured needle), so the next reader
   does not "simplify" the test back to a lower bound.
10. `crates/umber-gpu/src/bake_shaders.rs`'s shader doc quotes the CPU
    rule (the doc there already says "identical to the CPU side" — keep
    that true).

## 6. Non-goals

- **The malformed-triangle POLICY asymmetry** (the fill skips
  non-finite-position triangles; the projector keeps today's winner
  semantics) — that is wave 1.2, a separate kill. This kill does not
  touch the fill's filter or the projector's malformed-winner handling.
- **The projector's bytes.** This kill changes the mesh crate's
  validity rule; the projector's output changes only where the old
  bytes were wrong (the out-of-footprint garbage). The row-298 pins
  that describe that difference stay as they are — they document the
  OLD path's bug, and they keep documenting it after the fix (the
  projector's per-texel reference implementation is unchanged, so the
  difference it pins is unchanged).
- No new dependency, no persistence, no UI, no i18n.
- No change to the GPU shader's FORMULA — only to what the CPU sends it
  (via the existing all-zero-record channel).
- No change to `UV_DET_EPS` or `UV_BARY_EPS`.
