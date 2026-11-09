# Kill report — 2026-11-09 — wave 1.2: ONE mesh-wide malformed-triangle policy

Authorship: claw claude-opus-5.5 (subagent sa-0-12ff6783, 18 api calls, 494 s, no
cargo run by the claw) + dragon review and battery (Razul). Kill lock
`razul-malformed-policy-w12` over `crates/umber-mesh/src/uv_gbuffer.rs`,
`crates/umber-mesh/src/lib.rs`, `crates/umber-mesh/src/fill.rs`,
`crates/umber-app/src/projection.rs` (released in the same step as the commit).
Implements design brief `docs/loop/briefs/2026-11-09-malformed-triangle-policy-design.md`,
kill brief `...-kill-brief.md`, queue row 314, wave plan
`docs/loop/waves/2026-11-09-wave-plan.md` wave 1 item 1.2.

## Built

- `uv_gbuffer.rs`: new `pub fn triangle_positions_finite(mesh, tri) -> bool`
  (moved from `fill`, on the existing `triangle_positions` + a `pub(crate)
  pos_finite`). `for_each_uv_geometry` now composes the caller's filter as
  `triangle_positions_finite(mesh, tri) && filter(tri)` — the position rule
  runs FIRST, so a triangle with a non-finite position or an out-of-range
  position index is never offered, on every path, structurally.
- `lib.rs`: re-exports `triangle_positions_finite` next to
  `for_each_uv_geometry` — one formula, one function, both paths.
- `fill.rs`: private `triangle_positions_finite`/`pos_finite` deleted;
  `rasterize_uv_geometry` passes a filter that only records the current
  triangle and returns `true`. `uv_coverage` now routes through
  `for_each_uv_geometry(.., |_| true, ..)` instead of the crate-private
  `for_each_covered_texel_filtered` directly, so it inherits the composed
  rule (same triangles, same order, same texel set).
- `projection.rs`: the raster's walk keeps `|_| true`; the enforcement is in
  the walk. Module docs rewritten to name the LANDED policy: a
  malformed-position triangle covers nothing and no longer blanks its
  texels; the old "non-finite or missing-position winner blanks" case is
  removed from the doc list.
- Doc-comment sync in the same kill (doc-sync rule): the `uv_gbuffer.rs`
  module header and the `for_each_uv_geometry` contract bullet that said
  "malformed positions are still visited" now say they are never offered.
- New pins: `walk_never_offers_non_finite_position_triangles` (NaN/+inf/-inf,
  plus the finite control winning), `walk_never_offers_out_of_range_position_indices`
  (missing vertex, index 999), `a_malformed_position_triangle_alone_yields_nothing`
  (uv_gbuffer); `projector_keeps_finite_winner_under_malformed_triangle`
  (NaN/inf/missing x cull on/off, raster equals the no-bad-triangle raster,
  and asserts the OLD per-texel reference blanked the texel for NaN/missing —
  the planned byte change), `projector_malformed_triangle_alone_stays_transparent`,
  `fill_coverage_equals_projector_claimed_texels_on_hostile_corpus`
  (6 UV shapes incl. degenerate + the W1.1 needle x 5 position cases,
  each triangle alone and as one mesh; projector paints nothing outside
  the fill mask) (projection).
- Rewritten pins (old asymmetry reversed on purpose): uv_gbuffer
  `malformed_positions_are_visited_and_only_the_filter_skips` -> the two
  `walk_never_offers_*` tests; projection `a_nan_position_winner_blanks_its_texels_...`
  and `a_missing_position_winner_blanks_its_texels_...` ->
  `projector_keeps_finite_winner_under_malformed_triangle`; fill
  `the_fill_filter_skips_a_nan_triangle_the_unfiltered_walk_visits` ->
  `the_walk_never_offers_a_nan_triangle_filtered_or_not`.

## Judgment calls

- The claw switched `uv_coverage` from the crate-private
  `for_each_covered_texel_filtered` to `for_each_uv_geometry` — one call
  site beyond the brief — because once fill's own filter went away the
  direct call would have dropped the position rule. Same texel set by
  construction; pinned by the unchanged `uv_coverage_equals_rasterized_coverage`.
- The `unwrap_or([[f32::NAN; 3]; 3])` fallback in the walk is kept as a
  defensive no-panic path with a comment that it is unreachable since W1.2
  (the composed filter only admits triangles whose positions exist).
- `pos_finite` is `pub(crate)` so the rewritten fill test can still call
  `triangle_positions_finite`/`pos_finite` through the crate.
- In the projector test the inf case checks only the new bytes (the old
  reference's inf-winner texel value was not hand-derived); NaN and missing
  assert the old texel was CLEAR and the new is RED.

## Byte-change statement (the honest cost, per the design brief)

- `rasterize_uv_geometry` / `uv_coverage`: byte-identical (they already
  filtered; the walk now enforces the same rule).
- Projector raster: changes ONLY on meshes containing a triangle with a
  non-finite position or out-of-range position index whose UV footprint is
  valid and was a texel winner: those texels now keep the earlier finite
  triangle's value (or stay transparent with no finite competitor) instead
  of being blanked. Well-formed meshes: byte-identical. Pinned against the
  retained `per_texel_reference`.
- GPU bake path: untouched (UV-space G-buffer, never reads positions). Export
  padding (consumes `uv_coverage`): unchanged.

## Unverified

- Battery results: IN FLIGHT at report-write time (dragon battery
  proc_a2eb6c49e749 under the cargo flock: fmt, clippy `-D warnings` on
  umber-mesh + umber-app `--all-targets`, `cargo test -p umber-mesh`,
  `cargo test -p umber-app`, check-layers, scorecard --check). The claw ran
  no cargo; nothing below the battery line is the claw's word. RESULTS
  PATCHED IN BELOW BEFORE COMMIT.
- The hostile-corpus pin assumes at least one malformed triangle with a
  valid UV footprint exists in the corpus (UV shape 0 + NaN position); a
  guard assert fails loudly if not.

## Deferred

- Wave 2.1 R-A2 v3 depth/occlusion culling (the next row; route decision is
  that kill's first act).
- The GPU bake shader's position handling, if any is ever needed: out of
  scope, named.

## Process note

- Lock taken before the claw's first edit; claw touched only the 4 locked
  files (`git status` at review: exactly those 4 modified, nothing else).
- Queue row renumbered 313 -> 314 after the project-watch cron's row
  collided (max-row read immediately before writing, per the rule).
- Battery log: `~/.hermes/cache/scratch/w12-battery.log`.

## Battery results (patched in before commit)

Dragon battery, proc_a2eb6c49e749 under `/tmp/umber-cargo.lock`, log
`~/.hermes/cache/scratch/w12-battery.log`, all exits 0:

- `cargo fmt --check -p umber-mesh -p umber-app`: exit 0.
- `cargo clippy -p umber-mesh --all-targets -- -D warnings`: exit 0.
- `cargo clippy -p umber-app --all-targets -- -D warnings`: exit 0.
- `cargo test -p umber-mesh` + `cargo test -p umber-app`: 1306 passed, 0
  failed (re-run summary TOTAL_PASSED=1306); the new pins listed green,
  including `fill_coverage_equals_projector_claimed_texels_on_hostile_corpus`,
  `projector_keeps_finite_winner_under_malformed_triangle`,
  `projector_malformed_triangle_alone_stays_transparent`.
- `scripts/check-layers.py`: GREEN (16 crates), exit 0.
- `scripts/scorecard.py --check`: current, exit 0.

Independent verification (not the claw's word): the dragon reviewed the full
diff of all four files before the battery; the byte change is pinned against
the retained `per_texel_reference` (NaN/missing winner: old CLEAR, new RED),
and fill-side identity is pinned by the unchanged
`uv_coverage_equals_rasterized_coverage` and `delegated_geometry_raster_is_bit_identical_to_the_pre_v2d_walk`.
