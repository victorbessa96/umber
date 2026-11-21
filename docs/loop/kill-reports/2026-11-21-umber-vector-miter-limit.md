# K8b — SVG stroke miter limits

- Date: 2026-11-21.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-21-umber-vector-miter-limit.md` (`069fbdb5`).
- Implementation: `22bdd3e4be7f8fbc493d3d1e6014a0958c599ea1`.
- Kill lock: `vector-miter-limit`.
- Locked surface: `crates/umber-core/src/vector.rs`, `vector_svg.rs`, `svg.rs`, `layers.rs`, `project.rs`, `crates/umber-app/src/vector_panel.rs`, the K8b brief, and `docs/loop/queue.md`.
- Design sections implemented: normative ratio, default/storage/validation, geometry, SVG scope, verification.

## Built

- `vector.rs`: additive `miter_limit: f64` on `Stroke`, default 4, cache identity, legacy JSON decode/default omission, and deterministic JSON. Miter geometry now computes SVG's normative interior-angle ratio as `2 / |u + v|`. Inclusive comparison uses a documented 32-epsilon relative operation bound; normalized supporting-line directions reject only an exactly zero denominator and retain finite-result guards. Invalid non-finite or sub-1 limits produce typed errors.
- `vector_svg.rs`: root/group/path `stroke-miterlimit` parsing, scoped inheritance, raw-attribute validation before composition, empty/`inherit` preservation, export only when non-default, and strict single-shape plus compound-path coverage. Invalid values cannot be masked by a valid descendant.
- `svg.rs`: native importer explicit-default handling and Round/Bevel/Miter non-default roundtrips.
- `layers.rs` / `project.rs`: additive construction/default wiring.
- `vector_panel.rs`: valid miter-limit construction preserves the existing panel behavior; no UI slider was added.

## Verification

- RED/GREEN: independent interior-angle tests at 20° and 160°, both windings; exact-5 and exact-50 rotations with next-down rejection; near-reversal at `pi - 8 epsilon` with finite limit `2e15`; small/large-scale intersections. Each review reproduction failed before its fix and passed afterward.
- Public raster tests cover both solid and dashed continuous-turn strokes and reject next-down limits.
- `cargo fmt --all --check`: clean.
- `python3 scripts/check-layers.py`: GREEN, 16 crates.
- `python3 scripts/scorecard.py --check`: README current.
- `cargo test -p umber-core`: 338 passed, 0 failed.
- `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0.
- Final `cargo test -p umber-app` tracked as `proc_84135ba77885`: exit 0; 321 lib tests, 889 bin tests, and every integration/doc suite passed with zero failures.
- Fresh executable `hermes-verify-*` ad-hoc run: exact-50, near-reversal, both exact-five production tests, and strict clippy passed; script removed. This supplements rather than replaces the suite receipts above.
- Independent R1 (`deleg_ed63f0b6`): REQUEST CHANGES, five findings; all fixed.
- Independent R2 (`deleg_e8162b8e`): REQUEST CHANGES, two numerical findings; exact-50 tolerance and near-reversal denominator fixed with RED witnesses.
- Independent R3 (`deleg_dcffb07a`): SHIP / APPROVE, no material defect remains; frozen diff matched the working diff.

## Judgment calls

- Replaced the old centerline-distance heuristic only at the normative ratio calculation; default right-angle behavior remains pinned. No false claim of a default-geometry regression is carried.
- The 32-epsilon comparison tolerance is a relative floating-point operation bound, not an exact-5 fixture tuning constant. Exact-50 plus next-down coverage guards the general case.
- Finite high miter limits are honored for arbitrarily small nonzero angles. Non-finite intersection results still fall back safely.
- Compound filled holes remain handled by K8a's combined fill model; K8b changes join construction only and does not reopen compound editing.
- No new UI control was added; this kill closes the stored/imported SVG contract without expanding panel scope.

## Unverified

- No interactive release-GUI smoke was run. Geometry is exercised through public raster APIs, native roundtrips, and unit/integration suites.
- Third-party `resvg` remains the independent importer reference lane; native import/export behavior is tested directly.
- No claim is made for group transforms, CSS style, opacity, visibility, or effects beyond their existing typed-refusal behavior.

## Deferred

- Secondary-contour editor affordances and further SVG features remain separate scoped work.
- Version bumps, tags, releases, and public PR work are outside this kill.

## Process note

Two app-suite foreground attempts hit the 600-second compile timeout during a separate Learning-AI workspace build. The final run was restarted as a tracked background process and its recorded exit and every suite tally were read from the process result. No timeout was counted as a passing test. The lock remained held through the implementation and status-doc commits.
