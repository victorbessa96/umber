# Kill brief — W1.1: the UV triangle conditioning rule (wave 1, kill 1)

You are a claw in the Umber workspace. The tree lives at
**`/mnt/hdd/data/Projects/umber`** (the old path `/home/bessa/Projects/umber`
is a symlink to it).

**Read first:** `docs/loop/briefs/2026-11-09-uv-triangle-conditioning-design.md`
(binding — it carries the measured numbers and the REJECTED alternatives,
so you do not re-derive them).

## The change in one paragraph

Add a conditioning rule to the crate's UV-triangle validity test: a
triangle whose **normalized area** `|area| / longest_edge²` is below the
new constant `UV_COND_EPS = 1e-6` is NOT a valid footprint — it covers
nothing, on the CPU and the GPU alike. One place: `UvTri::of` /
`UvTri::from_uvs` in `crates/umber-mesh/src/uv_gbuffer.rs`. The GPU
inherits the rule through the EXISTING channel: `crates/umber-bake/src/gbuffer.rs`
uploads an all-zero record (area = 0) for every triangle `UvTri::of`
rejects, and the shader's `abs(area) < UV_DET_EPS` check then skips it.
Verify that channel actually uses `UvTri::of` (it does at gbuffer.rs:107)
and state it in your report.

## Your surface (these files and NOTHING else)

- `crates/umber-mesh/src/uv_gbuffer.rs` — the rule + the constant + the
  doc comments (module doc and `UvTri` docs must name the rule and WHY,
  with the measured needle, so nobody simplifies the test back).
- `crates/umber-bake/src/gbuffer.rs` — ONLY the parity golden (a needle
  fixture; CPU and GPU must agree on empty coverage). Do not change the
  upload path's logic — just verify it uses `UvTri::of`.
- `crates/umber-gpu/src/bake_shaders.rs` — the shader DOC comment only,
  if needed to keep "identical to the CPU side" true. Do NOT change the
  shader formula.

**Do NOT touch** any other file: not `fill.rs`, not `projection.rs`, not
`polygon_fill.rs`, not the `.ftl` catalogs, not `Cargo.toml`, not the
queue, not the ROADMAP.

## The contract

1. `UV_COND_EPS = 1e-6`, a new `pub const` next to `UV_DET_EPS` /
   `UV_BARY_EPS`, with a doc comment stating the normalized-area
   definition and the measured justification (1e-6 rejects 0/10 912 grid
   and 0/4 263 random triangles while killing every degenerate needle).
2. The rule goes in `UvTri::from_uvs` (the single construction point),
   alongside the existing finite-UV and `UV_DET_EPS` checks. It needs
   the longest UV edge: compute it from the three UVs. Keep the existing
   `area.abs() < UV_DET_EPS` check.
3. The rule is scale-free by construction (normalized area), and a pin
   must prove it: the same needle shape at UV scale 1 and at scale 1000
   is rejected by the same rule.
4. `UV_DET_EPS`, `UV_BARY_EPS`, the lower-bound inside test, and the
   overlap rule are ALL unchanged.

## The pins you must write

In `crates/umber-mesh/src/uv_gbuffer.rs`:

1. **A needle covers nothing** — the fuzz's exact fixture
   (`uvs = [[0.1614995, 0.16149949], [0.35829717, 0.3582972], [0.57094914, 0.57094914]]`):
   `UvTri::of` returns `None`, and neither the point lookup nor the
   walk claims any texel for the mesh.
2. **A well-conditioned mesh is untouched** — a UV grid's triangles are
   all valid (assert rejected count == 0), and its coverage is
   byte-identical to before.
3. **The threshold boundary is pinned on both sides** — a triangle just
   above 1e-6 is valid, one just below is not, with the numbers in the
   assertion message.
4. **Scale-free** — the needle at ×1 and ×1000 both rejected.
5. **The two lookup paths agree** — on a hostile corpus (slivers,
   needles, huge triangles, NaN UVs, out-of-range positions), the point
   lookup's texel set EQUALS the windowed walk's texel set. Build the
   corpus with a small deterministic PRNG in the test (see the probe
   under `~/.hermes/cache/scratch/probe-raster-cost/` for the generator
   shape — you may reimplement it in the test, do not import it).

In `crates/umber-bake/src/gbuffer.rs`:

6. **A needle mesh: CPU and GPU agree on EMPTY coverage** — add the
   needle to the parity corpus. If the GPU still claims a texel the CPU
   drops, the upload path does not use `UvTri::of`; that is the failure
   this pin exists to catch.

Look at the existing tests in both files and FOLLOW their fixture style
(`quad()`, `overlapped_quad()`, `assert_parity(...)`).

## Hard rules

- **NO cargo.** No build, test, clippy, fmt. The dragon runs the battery.
- **NEVER commit, push, stash, checkout, reset or clean.** Leave changes
  in the working tree.
- **Do not reformat unrelated code.** Match formatting by hand.
- **No new dependencies.**
- If the design brief conflicts with what the code actually says, the
  CODE wins for the mechanics; report the deviation in your final message.

## Final message

Report: files changed, the exact public names added (the constant and
where the rule lives), proof that the bake upload path uses `UvTri::of`
(file:line), the pins you wrote, and any deviation from the brief.
