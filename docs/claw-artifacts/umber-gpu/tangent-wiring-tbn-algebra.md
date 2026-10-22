# The tangent-wiring TBN algebra — the exact derivation (static prep)

Status: RESEARCH-VERIFIED, host-free (no cargo ran, no bytes of app code
moved). Written 2026-10-22 by local-7e5953 (dragon) as the static-prep
half of the tangent-wiring kill — the consumer is
`docs/loop/briefs/2026-10-20-tangent-wiring.md` (row 79's named
follow-up), whose step 3 says "derive the exact algebra before writing".
Kill report: `docs/loop/kill-reports/2026-10-22-tangent-tbn-static-prep.md`.
Companion port: `scripts/tbn_perturb_port.py` (run it; exit 0 = green).

Everything below is grounded in the live tree: the current formula read
from `crates/umber-gpu/src/shaders.rs` (`height_perturbed_normal`,
~:245), the golden helpers from `crates/umber-gpu/src/renderer.rs`
(:3928 fixture, :3980 CPU mirror, :4004 byte path), the foundation
module `crates/umber-mesh/src/tangents.rs` (parked, READ-ONLY here).

## 1. THE CORRECTION — the brief's step-3 sketch is stretch-broken

The brief sketches the new fragment math as:

    perturbed = normalize(n - (dhdu*T̂ + dhdv*B))     [sketch, WRONG]

with T̂ a UNIT tangent. That formula drops the parametrization
stretch: `dhdu` is the height gradient **per UV unit** (scaled by
`dims × height_strength`), but T̂ is unit **per world unit**. The
current screen-space formula never notices the difference because its
chain rule (`duv/dscreen`, `dpos/dscreen`) carries the UV-to-world
density implicitly — `|det|` is exactly the world-area per screen
pixel. Replace the frame but keep the unstretched gradient and the
perturbation magnitude changes with UV density.

The brief predicted the flat goldens' bytes "come out IDENTICAL" under
the sketch. They do not. On the goldens' OWN fixture
(`textured_quad_mesh`: positions −1..1, uvs 0..1 → 1 UV unit = 2 world
units, su = sv = 2):

| case | mikkelsen (current golden) | TBN corrected | TBN no-stretch (sketch) |
|---|---|---|---|
| flat | [127, 131, 152] | [127, 131, 152] | [127, 131, 152] |
| +u ramp | [95, 100, 122] | [95, 100, 122] | [69, 75, 98] |
| +v ramp | [89, 89, 99] | [89, 89, 99] | [59, 57, 61] |

The sketch's +u normal is (−0.7071, 0, 0.7071) vs the golden's
(−0.4472, 0, 0.8944): diffuse under `to_light = normalize(1,1,1)`
collapses 0.2583 → 0. A wiring session following the sketch verbatim
fails its own flat goldens — and the dangerous fix (weakening the
goldens to match) would silently redefine `height_strength` as
mesh-dependent. The port prints this table; the corrected formula
reproduces every flat golden byte EXACTLY (see §3 for why the match is
exact at f32, not just within tolerance).

## 2. The corrected algebra

Per-vertex attributes (locations 3 and 4): `tangent` = [f32;4]
(xyz = the unit weld-group tangent from `vertex_tangent_frames`,
w = handedness ±1 — the standard compact encoding the foundation
module already produces) and `stretch` = [f32;2] (su, sv — the
extension this artifact recommends, §5).

For an orthogonal UV parametrization (the v1 scope, §4), the
world-space surface gradient of the height field is:

    ∇s h = (dhdu / su) · T̂ + (dhdv / sv) · B̂
    n'   = normalize(n − ∇s h)

with `su = |∂p/∂u|`, `sv = |∂p/∂v|` (world units per UV unit) — the
stretch factors that make the gradient per-world-unit, matching what
the screen-space formula measured. WGSL form (the whole
`duv/dscreen` chain-rule block AND the `dpdx/dpdy` position
derivatives are deleted; the texel loads stay verbatim):

    let t_hat = normalize(tangent.xyz - n * dot(n, tangent.xyz));
    let b_hat = tangent.w * cross(n, t_hat);
    let su = max(stretch.x, 1e-6);
    let sv = max(stretch.y, 1e-6);
    let grad = (dhdu / su) * t_hat + (dhdv / sv) * b_hat;
    return normalize(n - grad);

CPU golden-helper mirror (glam, same op order — the golden helper and
the WGSL switch TOGETHER per the brief's step 4):

    fn tbn_perturbed_normal(n: Vec3, tangent: Vec4, stretch: Vec2,
                            dhdu: f32, dhdv: f32) -> Vec3 {
        let t_raw = tangent.truncate();
        let t_hat = (t_raw - n * n.dot(t_raw)).normalize();
        let b_hat = tangent.w * n.cross(t_hat);
        let su = stretch.x.max(1e-6);
        let sv = stretch.y.max(1e-6);
        let grad = t_hat * (dhdu / su) + b_hat * (dhdv / sv);
        (n - grad).normalize()
    }

Guards: the `1e-6` stretch floor keeps a degenerate parametrization
from producing inf/NaN (the foundation fallback path guarantees
su = sv = 1.0 for zero-contribution groups, so the floor is a
belt-and-suspenders for pathological real meshes). `t̂` re-orthogonalizes
against the INTERPOLATED normal — interpolation degrades the
pre-orthogonalized attribute (the brief's own note). Handedness is ±1
exactly, so `b_hat` never flips by rounding.

Why the flat match is EXACT at f32: on the flat fixture the corrected
gradient vector is the mikkelsen surf_grad scaled by |det| = 0.25 — a
power of two. IEEE f32 multiplication by 2^k is exact, sqrt(4x) = 2·√x
exactly, and correctly-rounded division of equally-scaled operands
reproduces the same quotient bits. The port asserts byte equality, not
closeness. (On general meshes the two formulas differ by sub-ulp
rounding; the goldens' ±1 byte tolerance and the 8-bit quantization
absorb that — the WGSL normalize is rsqrt-based where glam's is
sqrt+div.)

## 3. The derivation (why /su, why the metric, and the non-orthogonal tail)

The displaced surface is `P(u,v) = p(u,v) + h(u,v)·N`. Its exact
normal is `n' = normalize(N − ∇s h)` to first order, where `∇s h` is
the surface gradient of h **in world units**. With the tangent basis
`Tu = ∂p/∂u`, `Tv = ∂p/∂v` and the first fundamental form
`g = [[E,F],[F,G]]` (E = Tu·Tu, F = Tu·Tv, G = Tv·Tv):

    ∇s h = [Tu Tv] · g⁻¹ · [dhdu; dhdv]
         = ((G·dhdu − F·dhdv)/(EG−F²))·Tu + ((E·dhdv − F·dhdu)/(EG−F²))·Tv

For an ORTHOGONAL parametrization (F = 0) this reduces to
`(dhdu/|Tu|)·T̂ + (dhdv/|Tv|)·B̂` — §2's form, with su = |Tu|, sv = |Tv|.
The current screen-space formula computes exactly this per-facet (the
Mikkelsen surface gradient IS the metric-corrected world gradient,
measured in screen units that cancel in `normalize(|det|·n − ∇)`); the
TBN form must carry the metric explicitly because the compact
attribute normalizes the direction away.

v1 scope: adopt the orthogonal form. The residual for F ≠ 0 is bounded
by the parametrization's shear; real importer UVs at the seams that
matter are near-orthogonal, and carrying F would cost one more
attribute component (or packing handedness into a sign and |w| into a
stretch — rejected: it cannot carry sv). Name it in the wiring kill's
report as the known approximation, same class as the foundation
module's own v1 choices.

## 4. Fixture A — the flat goldens (the step-4 proof)

`textured_quad_mesh` through the ported `vertex_tangent_frames`:
T̂ = (1,0,0), handedness +1, **su = sv = 2** (the port asserts — this
is the number the sketch missed). The three golden cases
(`expected_height_perturbed_byte(0,0) / (0.25,0) / (0,−0.25)` on the
mikkelsen side; dhdu/dhdv = (0,0) / (1,0) / (0,1) on the TBN side,
with duv/dscreen = (0.25,0)/(0,−0.25) chaining the shader path):
the §1 table. Corrected TBN == mikkelsen, byte-exact, all three cases.

## 5. Fixture B — the cylinder segment (the curved golden)

Definition (the wiring kill's mesh fixture; the port builds it and
asserts outward CCW winding — 18 slots, 16 facets):

    R = 1, θ ∈ [−π/4, π/4] over 8 segments (Δθ = π/16),
    u = (θ + π/4)/(π/2) ∈ [0,1], v ∈ [0,1] over height 1 (y = v − 0.5),
    p(u,v) = (R·sin θ, v − 0.5, R·cos θ),
    n(θ)  = (sin θ, 0, cos θ)          [analytic smoothed normals],
    indices: per cell i, [b0, b1, t0] + [b1, t1, t0].

Frames from the ported foundation module (with the stretch extension):

- Every column's T̂ is DIRECTION-EXACT against the analytic
  `(cos θ, 0, −sin θ)`, including the half-supported edge columns.
  This is a property, not luck: the chord tangent at θ+δ Gram-Schmidt
  projected against the vertex normal at θ collapses to
  `cos δ·(cos θ, 0, −sin θ)` exactly (expand
  `t_chord + sin δ·n_v`), and interior columns sum two symmetric
  chords. The golden can probe ANY column.
- su = 1.568274 at every column (mean per-face |t|; the chord
  understates the analytic π/2 = 1.5707963 by 0.1606% — O(Δθ²),
  smooths away as tessellation refines). sv = 1. Handedness +1
  everywhere (v runs along +Y, right-handed).

The probe (θ = 0, dhdu = 1.0, dhdv = 0 — the +u ramp convention;
derivative-side numbers under the stated screen model "identity ortho,
1 world unit per pixel, dpdx in-facet"):

| arm | n' | bytes |
|---|---|---|
| derivative frame, LEFT facet | (−0.6177, 0, 0.7864) | [79, 85, 107] |
| derivative frame, RIGHT facet | (−0.4524, 0, 0.8918) | [94, 100, 122] |
| TBN (accumulated frame) | (−0.5376, 0, 0.8432) | **[87, 92, 114]** |
| analytic curved answer | (−0.5370, 0, 0.8436) | [87, 92, 114] |

The defect row 79 names, quantified: the derivative frame's answer
JUMPS across the probe's facet edge (Δx = 0.1653, 15 bytes in channel
0 — one pixel left and right of the θ=0 vertex line disagree), while
the TBN value sits between them and lands byte-identical to the
analytic curved-surface answer. The TBN arm is screen-independent by
construction (no dpdx/dpdy anywhere) — the curved golden's expected
bytes come from §2's formula + this fixture, with no raster model.

**The curved golden pins [87, 92, 114] at the probe.** The GPU-side
test reads back the probe pixel; the CPU expectation is
`tbn_perturbed_normal` with this fixture's accumulated frame values
(not the analytic ones — the GPU interpolates the accumulated
attribute). The port is the derivation source for both.

## 6. Encoding + layout (the wiring kill's step 1-2 map)

- Extend `Vertex` (renderer.rs:51) with `tangent: [f32; 4]` +
  `stretch: [f32; 2]`: 32 → 56 bytes. Both `array_stride:
  size_of::<Vertex>()` sites (:828 mesh, :915 openpbr) read the same
  constant, so the stride updates itself; only the MESH pipeline's
  attribute list grows (`3 => Float32x4, 4 => Float32x2`). The
  openpbr shader reads locations 0-1 only and is untouched; the
  wireframe overlay's `WireVertex` (:1000) is separate and untouched.
  The `size_of::<Vertex>() == 32` const assert moves to 56.
- `MeshBuffers::upload` (renderer.rs:1379) calls
  `vertex_tangent_frames` and fills both fields. The uv-less fallback
  path (zero-filled uvs) gets the foundation's fallback frames
  (su = sv = 1.0) — total, never NaN.
- Foundation module extension (tangents.rs — parked until the
  recovery; the wiring kill edits it): return su/sv per slot —
  per-weld-group MEAN of contributing faces' |t_face| / |b_face|
  (the unweighted accumulation convention, foundation judgment call
  2), fallback su = sv = 1.0 for zero-contribution groups. The port
  implements exactly this; its fixture numbers are the reference.
- Rejected: reconstructing stretch in-shader from dpdx/dpdy (the
  2×2 facet solve) — it reintroduces the derivatives the rewrite
  exists to delete and makes the magnitude per-facet; and the
  no-stretch sketch — §1's table.

## 7. What the wiring kill still owes (beyond this artifact)

1. The implementation per §2/§6 (post-recovery, clean files, its own
   manifest per the solo-height re-pin duty the foundation report
   names).
2. The GPU test: flat goldens unchanged (§4 — the helper switches
   formula, not values), the curved golden per §5 (raster caveats are
   its design call: camera framing, probe pixel choice, texel-center
   alignment for the ramp).
3. The closing amendment to the 2026-10-12 heightnormal report's
   judgment (the brief's named reversal) + this artifact cited as the
   derivation.
4. The non-orthogonal F≠0 residual named in its report (§3).
5. The battery's GPU goldens — everything here is CPU-side verified;
   nothing GPU-side has run (no cargo under the 2026-10-22 envelope).

## Reproducing

    python3 scripts/tbn_perturb_port.py

Validates the foundation module against all 7 umber-mesh unit-test
expectations, then prints §4's and §5's tables and asserts the
step-4 byte agreement. Exit 0 = green. Host-free: pure python, no
dependencies, every arithmetic op rounded through IEEE f32.
