# K7f independent arc review handoff (2026-11-20)

Reviewer: this interactive session, read-only on `razul-vector-arcs` surfaces.
No code edits made by reviewer. The arc owner keeps implementation and landing.

Independent checks:
- Initial Python oracle: `ARC_ORACLE_PASS 8/8`, plus closed-circle chain 4/4. After the ULP correction and exact-quarter fixture, final result: `ARC_ORACLE_PASS 9/9`.
- Initial focused Rust run at the intermediate working bytes: `cargo test -p umber-core vector_svg::tests::` = 31 passed / 5 failed / 36 total. These were TEST EXPECTATION defects plus one stale refusal, not evidence against the production arc math.
- Final landed state: focused Rust suite 36 passed / 0 failed; all dispositions below were consumed before `e6d7a5cd` was pushed.

Required dispositions before battery green:

1. `arc_large_arc_and_sweep_flags_select_the_derived_center_and_direction` (`vector_svg.rs:838`): actual small reverse first out is `[0.44771525, -3.3817688e-17]`, expected `[0.44771525, 0.0]`. The value matches the oracle. Use component-wise tolerance or canonicalize near-zero; do not require exact `0.0` after trig.
2. `arc_rotation_and_radius_correction_match_independent_center_geometry` (`:868`): initial diagnosis was incomplete. The two-cubic output was a one-ULP overshoot at an exact quarter, not a semicircle. The correct fix is a ratio-scaled ULP tolerance before the segment ceiling, preserving the original two-anchor quarter expectation. See the reviewer correction below.
3. `rounded_rect_imports_exact_lines_cubic_corners_and_closure` (`:944`): actual `4.227211e-18` vs exact `0.0`; use tolerance/canonical near-zero for trigonometric handles.
4. `rounded_rect_radius_rules_clamp_inherit_and_preserve_square_behavior` (`:962`): full-round rect starts at TOP `[2,0]`; circle starts at RIGHT `[4,2]`. Same ring, different start anchor. Compare a rotation of the anchor ring (circle order `[3,0,1,2]`) or canonicalize ring start, not vectors in the original order.
5. `unsupported_documents_and_anchor_floods_are_typed_refusals` (`:1435`): the old `M0 0 A1 1 0 0 0 1 1` case is now supported and must be removed from the unsupported list, just as the curved basic-shape refusals were removed.

Independent confirmation:
- All four large/sweep first controls match the oracle, including small reverse `(1-k, ~0)`.
- Radius correction from `(2,0)` to `(0,2)` uses two cubics and midpoint `(2,2)`; that part of the test is correct.
- Rounded-rect ring geometry, circle four-cubic closure, CTM paint/stroke scaling, exact arc-anchor budget, and grouped panel undo logic all look correct on read.

## Reviewer correction (same reviewer, after the ULP fix)

My earlier "semicircle replacement pin" is WITHDRAWN. Ideal geometry is an exact quarter; the two 45° cubics came from a one-ULP overshoot in `ceil(abs(delta) / (pi/2))`. The subagent's production fix is the correct one: subtract only a ratio-scaled floating-point tolerance before the ceiling. That preserves exact quarter segmentation and the original two-anchor expectation. Do not replace it with my earlier three-anchor pin. The oracle is being corrected with the same tolerance and rerun.

CONSUMED 2026-11-20: focused suite green, bounded independent review SHIP, safe-push green, and origin/master verified at `e6d7a5cdeeeacbd4782da65dabf389b7d34b84de`. This file is retained as review evidence, not a live coordination claim.
