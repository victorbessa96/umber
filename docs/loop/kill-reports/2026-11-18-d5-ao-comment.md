# Kill report: D5 MED relaxed-AO expectation (2026-11-18, lock `umber-d5-ao-comment`)

Last open item of the 2026-11-17 D2-D5 cross-review. Resolution: the brief's
DoD was wrong physics, not a relaxed assertion. A scene quad spanning
[-5,5]^2 at distance 0.1 from the plane subtends ~99% of the upper hemisphere,
so occlusion ~ 1 and AO ~ 0 at plane center. The implementation (and the test's
0.0..=0.1 assertion) was correct; the brief's "R=128 (0.5 occlusion)" was not.

Changes: DoD line corrected in `docs/loop/briefs/2026-11-16-d5-ground-plane.md`
with the solid-angle argument; test comment in
`crates/umber-bake/src/ground_plane.rs` rewritten to state the physics and
reference the corrected brief. No assertion weakened; no code changed.

Battery: rustfmt clean, check-layers GREEN, scorecard current,
clippy `-p umber-bake --all-targets -D warnings` 0 errors,
`cargo test -p umber-bake --lib ground_plane` 8/0 ok.
