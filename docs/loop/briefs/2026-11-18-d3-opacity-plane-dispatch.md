# Design brief: D3 opacity plane-path narrowing + dispatch fix (2026-11-18)

Kill: `razul-d3-opacity-plane-dispatch`. Resolves next-steps B2 and the D3 MEDs in
`docs/loop/reviews/2026-11-17-d2-d5-cross-review.md` §D3 by taking the review's
"narrow the public contract" option, with a documented reason.

## Finding (worse than the review noted)

`bake_opacity_mesh` (crates/umber-bake/src/opacity.rs) bakes the LOW mesh's position
map, binds `position_tex`/`normal_tex` at bindings 3/4, but dispatches
`entry_point: "cs_main"` — the PLANE entry, which hardcodes a plane at the origin
(`bake_shaders.rs:2139-2142`) and ignores the position textures entirely. The
mesh-fed entry `cs_main_from_position` (which reads bindings 3/4/6 and is what the
kittests were written against) is never dispatched. So every opacity bake currently
runs a hardcoded-origin-plane raycast with full coverage, and the position pass it
just ran is dead work.

## Decision: narrow the contract to the mesh-fed route

Opacity is only ever baked from the LOW mesh's own UV layout (the app's
`bake_opacity_tile` filters the mesh per tile and calls `bake_opacity_mesh`; nothing
dispatches a plane entry). The plane entry cannot serve arbitrary `PlaneDesc` anyway
(it hardcodes the origin plane), so it is dead, misleading scaffolding — same class
as the D4 `cs_main_from_position` zero-stub removed 2026-11-18. Contract after this
kill: **opacity is mesh-fed only** (`cs_main_from_position`); there is no plane path.

## Scope

1. `crates/umber-bake/src/opacity.rs`:
   - dispatch `entry_point: Some("cs_main_from_position")` in `bake_opacity_mesh`;
     update the bind-group comment.
   - replace `opacity_mesh_fed_matches_plane` with
     `opacity_mesh_fed_double_bake_is_byte_identical` (same body; the plane path it
     nominally compared against no longer exists — documented reason above);
   - add a source-level regression guard `opacity_pipeline_dispatches_mesh_fed_entry`:
     `include_str!("opacity.rs")` contains `entry_point: Some("cs_main_from_position")`
     and `OPACITY_BAKE_SHADER` no longer contains the plane entry.
   - module doc: drop the plane-entry paragraph.
2. `crates/umber-gpu/src/bake_shaders.rs`: delete the plane `cs_main` from
   `OPACITY_BAKE_SHADER` (the hardcoded `plane_origin` block) and its doc paragraph;
   keep bindings/uniform layout unchanged (32-byte `OpacityParams`).
3. `docs/loop/briefs/2026-11-16-d3-opacity-baker.md`: add a 2026-11-18 resolution note
   narrowing the plane contract.
4. New kill report + queue row.

## Verification note

The opacity GPU kittests (`opacity_*_matches_*`) skip on this host under the
2026-11-18 capability gate (SKL/GL adapters); they run in CI's lavapipe step
(`UMBER_RUN_GPU_TESTS=1`). Local battery: clippy (incl. `--features gpu` compile),
CPU tests, rustfmt, layers, scorecard, safe-push matrix.

## Acceptance

- clippy `-p umber-bake -p umber-gpu --features umber-bake/gpu,umber-gpu/gpu --all-targets -D warnings` green; rustfmt clean.
- `cargo test -p umber-bake -p umber-gpu` green (GPU tests skip with named reason locally).
- safe-push green, ls-remote verified, lock released, queue row appended.
