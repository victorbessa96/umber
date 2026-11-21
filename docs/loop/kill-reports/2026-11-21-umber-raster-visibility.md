# K8e — Raster SVG display and visibility

- Date: 2026-11-21.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-21-umber-raster-visibility.md` (`661f6f14be752114ab25accacca81d20205b0213`).
- Claim: `9784df157b7ddb42da8daf6f61ef7ed882c279d8`.
- Implementation: `c2831099ec106e2e68745d6f5989507d64d689d0`.
- Kill lock: `razul-raster-visibility`.
- Locked surface: `crates/umber-core/src/svg.rs`, the K8e brief/report, queue, coordination, STATE, and ROADMAP.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8e brief.
- Pre-commit diff SHA-256: `cebeed2ea59f5eb3715a986d30ad18e47abb6a1022e986459fdb7e443eb6f891`.

## Built

- Raster `Style` now carries effective `display` and `visible` booleans alongside painted, fill rule, and CTM.
- `apply_raster_presentation` wraps the shared fill/rule/transform validation and computes display with `parent.display && vector_display_renders(attrs)`, visibility with `vector_visibility_renders(attrs, parent.visible, element)`.
- Root style starts rendered and visible. Group frames preserve effective display, so no descendant can re-enable a pruned ancestor. Visibility remains independently inherited and permits child-visible overrides.
- Shapes build raster geometry only when effective display, visibility, and painted are all true. Hidden shapes bypass `build_shape` and `MAX_SHAPES`; hidden transforms still parse and fail normally.
- All-hidden documents produce the normal zero-filled `width * height` coverage plane rather than the editable importer's typed no-geometry refusal.
- Module docs now state that both raster and editable import interpret display/visibility under SVG 1.1 §11.5. K8d's editable import changes only inert Style field initialization; app, model, serialization, and export production paths are untouched.
- Tests cover exact visible pixels, non-none display values, display-none root/group/shape pruning, visibility inheritance and override, blank all-hidden coverage, invalid visibility, hidden invalid transforms, and exact hidden-budget exclusion.

## Verification

- RED before implementation: `display_and_visibility_filter_raster_coverage` painted hidden shapes; `all_hidden_raster_is_blank_and_invalid_values_are_typed` painted a hidden rect; `hidden_raster_shapes_do_not_consume_shape_budget` exceeded `MAX_SHAPES`.
- GREEN after implementation: `cargo test -p umber-core svg::tests` passed 71/71, including all three new tests and the existing editable-import suite.
- `rustfmt --edition 2021 --check crates/umber-core/src/svg.rs`: exit 0.
- `python3 scripts/check-layers.py`: GREEN, 16 crates, exit 0.
- `python3 scripts/scorecard.py --check`: README current, exit 0.
- `git diff --check`: clean.
- Strict `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0.
- Canonical full battery `proc_d111266c31ff`: process exit 0 and `full_core_app_exit=0`; core 346/346, app 321 lib + 892 bin, and integration/doc suites totaling 2, 1, 3, 17, 11, 10, 2, 1, 0, and 0 tests all passed with zero failures. All three K8e tests passed.
- Independent review `deleg_c0a26c8a`: APPROVED, zero findings. It verified conjunctive display pruning, independent visibility inheritance/override, hidden validation without geometry/budget charges, zero-filled all-hidden coverage, and the single-file diff. The reviewer did not independently rerun Cargo; the canonical battery above is the execution receipt.
- Supplemental ad-hoc probes used the already-built core test binary for all three K8e behaviors, printed actual PASS tallies, and self-deleted with zero shell strays. They are not a substitute for the canonical battery.

## Judgment calls

- Display pruning remains separate from unknown/path subtree skipping. Groups retain effective display state for descendants, while hidden shapes skip only geometry and budget consumption; shared transform validation still runs.
- All-hidden raster input returns blank coverage, which is the raster API's natural representation. This deliberately differs from editable import's typed no-geometry refusal.
- K8e changes no hidden-layer persistence and no editable import semantics from K8d. The two import lanes share only the small display/visibility helpers and inert Style initialization.
- Implementation and status documents land separately so the report and queue cite the exact implementation hash.

## Unverified

- No interactive stencil-file GUI smoke was run. Exact raster coverage, parser errors, and shape budgets are covered by the core tests and full app battery.
- No claim is made for opacity, CSS style, clip/mask/filter/markers, vector-effect, paint-order, events, references, text layout, or hidden-element persistence. Those remain out of scope.

## Deferred

- SVG 2 leaf `vector-effect="non-scaling-stroke"` is a separate scoped candidate, not part of K8e.
- Version bumps, tags, releases, public PRs, schema/project-version changes, new dependencies, and hidden-layer UI remain outside this kill.

## Process note

The result JSON initially flushed after the process had exited; the final receipt was read from `proc_d111266c31ff.json` and every suite tally was extracted before landing. No missing result file was treated as a passing test.
