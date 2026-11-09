# Kill brief — 2026-11-09 — wave 1.2 kill: unify the malformed-triangle policy in the walk

Implements `docs/loop/briefs/2026-11-09-malformed-triangle-policy-design.md`.
Lock: `razul-malformed-policy-w12`. Authorship: claw writes, dragon reviews + batteries.

## Surface (exactly these files)

- `crates/umber-mesh/src/uv_gbuffer.rs`
- `crates/umber-mesh/src/lib.rs`
- `crates/umber-mesh/src/fill.rs`
- `crates/umber-app/src/projection.rs`

Do NOT touch any other file. No WGSL, no GPU crate, no docs outside the
four files' own comments/tests (the design brief is committed separately
by the dragon).

## The change

1. `uv_gbuffer.rs`: add `pub fn triangle_positions_finite(mesh: &MeshData, tri: usize) -> bool`
   (moved from `fill.rs:623-643`, including the `pos_finite` helper).
   In `for_each_uv_geometry` (line 471), compose the caller's filter:
   pass `|tri| triangle_positions_finite(mesh, tri) && filter(tri)` down
   to `for_each_covered_texel_filtered`. Update the module/function docs
   (the bullet at lines 464-469) to name the LANDED behavior: triangles
   with non-finite positions or out-of-range position indices are never
   offered, on every path.
2. `lib.rs`: add `triangle_positions_finite` to the
   `pub use uv_gbuffer::{...}` re-export (line ~51).
3. `fill.rs`: delete the private `triangle_positions_finite` and
   `pos_finite` (lines 619-643); both call sites (`rasterize_uv_geometry`
   line 568, `uv_coverage` line 613) pass `|_| true`. Sync the doc
   comment at 535-542.
4. `projection.rs`: line 705 stays `|_| true` (enforcement is now in the
   walk). Sync the module docs (lines 29-31, 696-699) to name the landed
   policy: a malformed triangle covers nothing; it no longer blanks
   texels.

## Pins (add; rewrite the old asymmetry pins)

New tests (in the owning module's `#[cfg(test)]`):

- `walk_never_offers_non_finite_position_triangles` (uv_gbuffer): quad
  + NaN-position overlapping triangle, filter `|_| true`, assert the
  malformed triangle is never offered (record offered indices by
  wrapping: the public walk composes, so test through
  `for_each_uv_geometry` with a filter that records).
- `walk_never_offers_out_of_range_position_indices` (uv_gbuffer): index
  999 variant.
- `projector_keeps_finite_winner_under_malformed_triangle` (projection):
  shaded colour of the finite triangle at those texels, not CLEAR.
- `projector_malformed_triangle_alone_stays_transparent` (projection).
- `fill_coverage_equals_projector_claimed_texels_on_hostile_corpus`
  (projection or uv_gbuffer, using `fill::uv_coverage`): corpus with NaN,
  inf, out-of-range indices, degenerate UV, needles.

Rewrite (they pin the OLD winner semantics and must now assert the
unified policy): grep projection.rs and uv_gbuffer.rs tests for the
row-298 pins about the unfiltered walk visiting malformed triangles /
the malformed winner blanking the texel. List every rewritten test by
name in the kill report.

## Rules

- Do NOT run cargo build/test/clippy — the dragon runs the battery under
  the cargo flock. Write code + tests only.
- No new dependencies. No public API removal (additive `pub fn` only).
- `uv_coverage` and `rasterize_uv_geometry` outputs must remain
  byte-identical — they already filtered; prove it with the existing
  pins staying green unmodified.
- rustfmt-clean (the dragon checks).
- Report back: files changed, tests added/rewritten by name, any
  judgment call (e.g. the `unwrap_or([[NAN;3];3])` fallback at
  uv_gbuffer.rs:486 — keep as defensive or simplify), and anything you
  could not verify without cargo.
