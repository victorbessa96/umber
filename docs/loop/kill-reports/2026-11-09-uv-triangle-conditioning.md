# Kill report — W1.1: the UV triangle conditioning rule (wave 1, kill 1)

Date: 2026-11-09. Lock: `razul-uv-conditioning-w1-307` (4 files). Claw:
claude/opus on `docs/loop/briefs/2026-11-09-uv-triangle-conditioning-kill-brief.md`;
dragon rewrite of the app-side pin.

## The change

`UvTri::from_uvs` (crates/umber-mesh/src/uv_gbuffer.rs) now rejects a
triangle whose **normalized area** `|area| / longest_edge²` is below the
new `pub const UV_COND_EPS: f32 = 1e-6`. A triangle below it is not a
valid footprint: it covers nothing, on the CPU and the GPU alike.

- **One place, whole crate:** every CPU consumer goes through
  `UvTri::of`, so the rule covers the G-buffer walk, the point lookup,
  the geometry walk, the fill and the export padding at once.
- **The GPU inherits it through the existing channel, no shader
  change:** `crates/umber-bake/src/gbuffer.rs:108` maps every triangle
  through `UvTri::of`; a rejected one uploads as
  `GpuGbTri::zeroed()` (area 0), which the shader's
  `abs(area) < UV_DET_EPS` skip drops. Pinned by
  `a_needle_uploads_as_a_zero_record` (no GPU needed) and the GPU parity
  golden `a_needle_claims_nothing_on_the_cpu_or_the_gpu`.
- `UV_DET_EPS`, `UV_BARY_EPS`, the lower-bound inside test and the
  overlap rule are unchanged.

## Why 1e-6 (measured, in the design brief §4)

- garbage claims on the hostile corpus: 272 → **0**;
- point lookup vs windowed walk texel-set diff: 31 → **0** (the two
  paths now agree exactly);
- production impact at 1e-6: **0 / 10 912** grid triangles and
  **0 / 4 263** random triangles rejected; the only coverage lost is the
  degenerate needles themselves;
- the first garbage claims appear near norm_area 3.7e-7, so 1e-6 sits
  above them with margin. Rejected alternatives (do not re-derive): the
  per-point upper bound made it worse (lost 7→16, diff 31→54); a
  per-point reconstruction tolerance is not separable (legitimate thin
  claims carry up to 3.5e-1 error).

## Pins (all green in the battery)

umber-mesh: `a_needle_covers_nothing`,
`a_well_conditioned_grid_is_untouched` (512-triangle grid, 0 rejected,
G-buffer bit-identical to a pre-W1.1 reimplementation),
`the_conditioning_threshold_is_1e_6_inclusive` (boundary pinned both
sides on an exact fixture), `the_conditioning_rule_is_scale_free`
(needle rejected at UV scale 1 and 1000), plus the hostile-corpus
agreement pin.
umber-bake: `a_needle_uploads_as_a_zero_record` + the GPU
parity golden (skips without a GPU device, as before).
umber-app: the row-298 needle pin REWRITTEN — pre-W1.1 it documented the
one divergence (old path painted texel (15,0) with weights [0,0,2]);
now both rasters route through `UvTri::of`, both reject the needle, and
the pin states both come out empty and identical.

## Battery (local, `flock /tmp/umber-cargo.lock`, nice 19, JOBS=3)

`cargo fmt --check` clean; `cargo clippy --workspace --all-targets
-D warnings` clean; `cargo test -p umber-mesh -p umber-bake
-p umber-gpu -p umber-app` all green (incl. 304 umber-mesh, 772
umber-app); `check-layers.py` GREEN (16 crates); `scorecard.py --check`
current. Log: `~/.hermes/cache/scratch/w11-battery2.log`, exit 0.

## Non-goals honored

No new dependency; no WGSL change; no change to the fill's
malformed-triangle filter or the projector's winner semantics (that
asymmetry is wave 1.2, deliberately untouched); no UI/i18n/persistence.
