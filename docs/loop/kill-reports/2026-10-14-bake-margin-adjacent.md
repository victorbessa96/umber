# Kill report — bake-margin-adjacent (2026-10-14)

- **Author:** subagent (code claw), dispatched by the dragon.
- **Lock:** `bake-margin-adjacent` — `crates/umber-bake/src/margin.rs`,
  `crates/umber-bake/src/lib.rs`,
  `docs/loop/kill-reports/2026-10-14-bake-margin-adjacent.md`.
- **ROADMAP row:** wave 8b `wave-bake-margin-adjacent`.
- **Design source:** Blender manual, Cycles baking, Margin Type —
  "Adjacent Faces: fill margin using pixels from adjacent faces across UV
  seams" vs "Extend: extend border pixels outwards"
  (https://docs.blender.org/manual/en/latest/render/cycles/baking.html).
  Seam topology: `umber_mesh::seam::{SeamGraph, SeamEdge}`.

## Built

- `crates/umber-bake/src/margin.rs` (new): `MarginType { Extend,
  AdjacentFaces }`, `MarginError { EmptyTarget, SizeMismatch,
  MeshSeamMismatch }`, `adjacent_faces_margin(map, width, height, mesh,
  seams, margin_px) -> Result<Vec<u8>, MarginError>`. Pure CPU, no panics
  (checked indexing throughout). For each seam edge, both directed sides
  (A→B, B→A): an uncovered texel centre `p` within the edge's
  perpendicular strip (`s ∈ [0,1]`, outward distance `d ∈ (0, margin_px]`)
  maps to `q = D0 + s·dD + d·(|dD|/|dS|)·n_in_D` and copies the covered
  texel at `floor(q)` from the input map. 11 tests with exact byte values.
- `crates/umber-bake/src/lib.rs`: `pub mod margin;`.

## Judgment calls

- **Nearest sampling**, not bilinear: byte-exact copies, no blending with
  the destination island's own uncovered border. Bilinear is a follow-up.
- **Pixel space under the bake convention** (`u = (x+0.5)/w`,
  `v = 1-(y+0.5)/h`), so `margin_px` is in texels as in Blender.
- **Inside/outside from the triangle UV centroid**: `n_out` points away
  from the source triangle's UV centroid, `n_in` toward the destination's.
  Zero-UV-area triangles (centroid on the edge line) and zero-length UV
  edges skip the edge rather than guess a side.
- **Depth rescaled by `|dD|/|dS|`** (matches `umber_mesh::seam_mirror`)
  for islands with different texel density along the same 3D edge; the
  brief's unscaled `d` is the equal-length case (the test fixture).
- **Perpendicular strip only**: the wedges past an edge's endpoints are not
  filled (they map to a third face via the vertex fan). Left for Extend.
- **Closest seam wins** on overlapping strips (smallest `d`; ties → first
  in `seams.edges` order, A side first). Single pass reading the original
  map only — no chaining through freshly filled texels.
- **Composition**: unreached texels come back byte-identical (`a == 0`), so
  the pipeline is AdjacentFaces first, then `dilation::dilate_map` (Extend).
  Documented in the module docs.
- **Mismatch detection** is shallow: tri-count equality plus every seam
  triangle resolving to three UVs. A graph from a different mesh with the
  same triangle count is not detected.
- umber-bake has no crate-root `deny(unwrap)` lint; the module follows the
  never-crash discipline anyway (no unwrap/expect/panic outside tests).

## Unverified

- Only an axis-aligned, equal-length seam is pinned by exact tests.
  Rotated/diagonal seams and unequal edge lengths (the `ratio` path)
  follow from the formula but have no dedicated test.
- Seam orientation reversal (the B edge stored in the opposite direction
  to A in UV) is handled by the `uv_a[k] ↔ uv_b[k]` endpoint correspondence;
  not separately tested.
- No real-mesh / large-map run; cost is O(edges × strip area), no
  profiling done.

## Deferred

- Wiring into the bake pipeline, CLI flags and UI (Margin Type selector)
  — touches files outside this lock.
- Bilinear sampling; endpoint-wedge fill via vertex-fan adjacency.

## Gates run

- `flock -w 1200 /tmp/umber-cargo.lock cargo fmt --all` — clean.
- `flock -w 1200 /tmp/umber-cargo.lock cargo test -p umber-bake` — 90
  passed, 0 failed (11 new in `margin::tests`).
- `flock -w 1200 /tmp/umber-cargo.lock cargo clippy -p umber-bake
  --all-targets -- -D warnings` — clean.

## Process note

`cargo fmt --all` runs workspace-wide; the tree at the time also had
another agent's in-flight `.github/workflows/ci.yml` and
`scripts/ci-classify.py` (not Rust, untouched by fmt).
