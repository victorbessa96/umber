# Kill report: umber-vector-arcs (K7f, 2026-11-20)

- Authorship: dragon implementation and local battery, bounded independent CLI review.
- Kill lock: `razul-vector-arcs`.
- Brief: `f95b0ab8238f33248517d7c88f68b6e0e66dadd2` (`docs(vector): K7f arc import brief`).
- Implementation: `2555d2d81022f6a53c6a3de3b58143e2dd96aa18` (`feat(vector): K7f arc and curved shape import`).
- Prior pushed truth: K7e report `b72b9a1c1aa2a446b97d4cb7c270f609ce5607be`.
- Locked code/tests: `crates/umber-core/src/{svg.rs,vector_svg.rs}` and `crates/umber-app/src/vector_panel.rs`. The remaining locked surfaces were this brief/report, queue/STATE/ROADMAP, and `.dragon/COORDINATION-next-wave.md`.
- Design sections implemented: Target and Verification in `docs/loop/briefs/2026-11-20-umber-vector-arcs.md`.

## Built

- `svg.rs`: extracted one SVG 1.1 endpoint-to-center conversion shared by the raster segment fan and editable vector arcs. Negative radii use absolute values, zero radii become lines, identical endpoints emit nothing, small radii grow uniformly, and rotation/flags choose the correct center and signed sweep. Derivative-derived cubics split at at most 90° and preserve exact SVG endpoints.
- `svg.rs`: `VectorImportBuilder::sink_arc_to` precomputes and charges all generated anchors before changing handles or anchors. Non-finite conversion/UV results are typed refusals; raster non-finite geometry keeps its established no-contribution behavior.
- `svg.rs`: rounded rects follow rx/ry inheritance, negative-auto, half-side clamping, and zero-radius square behavior. Straight edges and four corners use the same sink; zero-length edges add no duplicate anchors. Positive circles/ellipses become four closed quarter cubics; zero/negative geometry emits no layer.
- `vector_svg.rs`: ten new tests cover independently derived quarter/half/large arcs, absolute/relative/negative-radius input, every flag combination, rotation, radius correction, degenerates and non-finite input, exact rounded-rect handles and closure, radius rules, circle/ellipse closure, CTM/scoped paint, exact per-shape and existing shape/total budgets, and cubic export→import→export.
- `vector_panel.rs`: one new grouped undo/redo test installs an arc, rounded rect, circle, and ellipse in one history group and compares every restored `VectorParams`. The existing failure-atomicity test now also covers a valid circle followed by an overflowing arc.

## Judgment calls

- Arcs are editable cubic approximations, not flattened polylines and not a schema-level arc type. Ninety-degree segments use the standard `4/3*tan(delta/4)` derivative coefficient. Export deliberately emits portable `C` commands rather than trying to reconstruct `A` commands.
- An exact quarter turn can evaluate one ULP above `π/2` after `atan2` and sweep normalization. The segment ceiling subtracts only ratio-scaled floating-point roundoff, preventing a spurious pair of 45° cubics without suppressing a genuinely larger arc. This was caught by the independent rotation fixture and corrected before the battery.
- Full-round rects and circles use different natural start anchors (top versus right); the test compares the closed ring after rotation instead of demanding a different start point. Trigonometric tests use a documented 2e-6 component tolerance only for quarter-turn residue; centers, counts, directions, and non-zero handles remain pinned.
- Existing raster non-finite skip behavior stays unchanged. Vector import fails typed because it must not install non-finite editable state.

## Review beam

`opencode/beam-free` with the read-only `plan` agent reviewed the exact 34,983-byte post-brief diff and returned **SHIP, no findings**. Log: `/home/bessa/.hermes/cache/scratch/vector-arcs-review-beam.log` (`REVIEW_EXIT=0`). The reviewer independently read the conversion, sink, shape dispatcher, tests, and grouped panel path.

Disposition: no HIGH/MEDIUM/LOW findings required fixes. Its residual coverage note says raster-side all-flag/rotation/correction combinations are not independently re-pinned by this diff. That is recorded under Unverified rather than hidden; the conversion is shared, vector fixtures independently cover those branches, and the complete existing raster SVG suite passed. A separate read-only reviewer handoff (`.dragon/COORDINATION-arc-independent-review.md`) also confirmed the ULP correction and oracle geometry; it remained foreign and uncommitted.

## Verification

- Focused importer suite: 36 passed / 0 failed.
- Focused panel selection: 16 passed / 0 failed. Log: `/home/bessa/.hermes/cache/scratch/vector-arcs-focused-app.log`.
- Final battery: `/home/bessa/.hermes/cache/scratch/vector-arcs-final-battery-r1.log`, pinned to the implementation working bytes and brief parent `f95b0ab8`.
- rustfmt, check-layers.py, scorecard.py --check: all exit 0; `git diff --check` clean.
- `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0.
- Full core/app battery under `/tmp/umber-cargo.lock` with `UMBER_ASSETS_DIR=/home/bessa/Projects/umber`: 13 suites, 1,563 passed / 0 failed / 0 ignored. Core lib 309, app lib 321, app bin 886; the only zero-test targets were the two doc-test harnesses. `BATTERY_DONE fmt=0 layers=0 scorecard=0 clippy=0 tests=0`.
- No schema/project-version change, public PR, release/tag, centralhost contact, or L-AI/cb4b2026 work.

## Unverified

No new independent raster-only matrix was added beyond the shared conversion tests and full existing SVG suite. No windowed import smoke, GPU/golden claim, or release CI run is claimed. Safe-push's own affected-crate battery and `git ls-remote` are the landing checks, not substitutes for this local receipt.

## Deferred

Filled compound subpaths, CSS/effects/unit expansion, schema-level arc preservation, UI redesign, and release work remain outside this kill. Next live vector work is compound-fill modeling or another separately scoped SVG hardening slice.

## Process note

The first app foreground compile reached the tool's 600 s cap without a tally; the warm build was resumed once under the same flock and completed with a real 16-test receipt. No overlapping cargo process was started. Only the three implementation files entered the implementation commit; the pre-existing coordination edit and independent reviewer note were not swept into it.
