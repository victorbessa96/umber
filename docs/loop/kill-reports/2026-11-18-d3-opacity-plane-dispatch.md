# Kill report: D3 opacity plane-path narrowing + mesh-fed dispatch fix (2026-11-18)

Kill lock: `razul-d3-opacity-plane-dispatch`. Brief: `docs/loop/briefs/2026-11-18-d3-opacity-plane-dispatch.md`
(commit `c52f685b`, docs-first) + resolution notes in `docs/loop/briefs/2026-11-16-d3-opacity-baker.md`.
Resolves next-steps B2 and the D3 MEDs of `docs/loop/reviews/2026-11-17-d2-d5-cross-review.md`.

## Finding

`bake_opacity_mesh` baked the LOW position map and bound `position_tex`/`normal_tex`,
but dispatched `entry_point: "cs_main"` — the plane entry with a hardcoded origin
plane that ignored those textures. The mesh-fed `cs_main_from_position` (what the
kittests were written against) was never dispatched. Worse than the review noted.

## Decision (review's option B, documented)

Opacity is only ever baked from the LOW mesh's UV layout (`bake_opacity_tile` filters
the mesh and calls `bake_opacity_mesh`); no caller dispatches a plane entry, and the
hardcoded-plane entry could never serve arbitrary `PlaneDesc`. Contract narrowed:
**opacity is mesh-fed only.** Same class as the D4 zero-stub removal.

## Changes

- `crates/umber-bake/src/opacity.rs`: pipeline now dispatches `cs_main_from_position`;
  module doc updated; `opacity_mesh_fed_matches_plane` →
  `opacity_mesh_fed_double_bake_is_byte_identical`; new CPU-level guard
  `opacity_pipeline_dispatches_mesh_fed_entry` (source-level: dispatch string present,
  `plane_origin` gone from the shader, `cs_main_from_position` present).
- `crates/umber-gpu/src/bake_shaders.rs`: plane `cs_main` (56 lines incl. the
  hardcoded `plane_origin` block) removed from `OPACITY_BAKE_SHADER`; doc header
  updated. Uniform layout unchanged (32-byte `OpacityParams`); bindings 3/4/6 are
  used by the mesh-fed entry.

## Battery (flock /tmp/umber-cargo.lock, CARGO_BUILD_JOBS=3, nice 19)

- rustfmt clean; check-layers GREEN (16); scorecard current.
- clippy `-p umber-bake -p umber-gpu --features umber-bake/gpu,umber-gpu/gpu --all-targets -D warnings`: 0 errors.
- `cargo test -p umber-bake -p umber-gpu`: all suites ok (guard test green). GPU kittests
  skip locally under the 2026-11-18 capability gate and run in CI's lavapipe step
  (`UMBER_RUN_GPU_TESTS=1`) — the dispatch change is verified there.
- safe-push green; ls-remote verified; kill lock released; queue row 539.
