# Kill report: umber-vector-compound-fill (K8a, 2026-11-20)

- Authorship: dragon implementation and local battery, two bounded independent CLI reviews.
- Kill lock: `razul-vector-compound-fill`.
- Brief: `6bd70d8f15f8c9ad18773d3d9cd9435de00a581e` (`docs(vector): K8a compound fill brief`).
- Implementation: `6f35557f66149809814d0f61a6f1b9eabde00716` (`feat(vector): K8a compound fill and subpath import`).
- Prior pushed truth: K7f report `e6d7a5cdeeeacbd4782da65dabf389b7d34b84de` plus its independent review handoff `7b8326fcfa1290d35d37881ab6f25a85e30523bc`.
- Locked code/tests: core `vector.rs`, `svg.rs`, `vector_svg.rs`, `journal.rs`, `project.rs`, `layers.rs`; app `vector_panel.rs`, `fill_layer.rs`, `vector_edit.rs`; generated README scorecard. Bookkeeping surfaces are this brief/report, queue/STATE/ROADMAP, and `.dragon/COORDINATION-next-wave.md`.
- Design sections implemented: Target and Verification in `docs/loop/briefs/2026-11-20-umber-vector-compound-fill.md`.

## Built

- `vector.rs`: additive `VectorSubpath { closed, anchors }` and `VectorShape.subpaths`, serialized as an ordinary array with default + skip-if-empty. The container is `Box<Vec<VectorSubpath>>` to keep `LayerCommand` below clippy's large-enum threshold; old JSON remains keyless and byte-compatible. `is_empty`, NaN-stable cache identity, and `total_anchors()` include every contour.
- `vector.rs`: rasterization flattens primary and secondary contours independently under shared centerline and stroke-polygon budgets. Every closed centerline enters one `rasterize_polys_rule` call, preserving nonzero/evenodd holes across contours. Every contour's stroke uses its own closure joins/caps and fresh dash phase. Primary-empty secondaries render; open contours never subtract fill. Contour, anchor, centerline, stroke-point, and edge-row budgets are aggregate.
- `svg.rs`: import always combines moveto-delimited contours into one shape per path/basic-shape element, filled or stroked. Strict `import_svg` still means exactly one path element, now including one compound path. Builder line/cubic/arc charging uses the aggregate anchor total before mutation; document-level contour and anchor budgets remain fail-closed.
- `vector_svg.rs`: export emits every contour in one deterministic `<path>` with shared paint/rule/stroke, including primary-empty compounds. Aggregate anchor/non-finite validation and compound export→import→export byte stability are pinned.
- App: one compound import produces one layer in one undo group and restores all geometry on redo. Primary canvas edits preserve secondaries; append/insert charge aggregate anchors. Secondary-only edits invalidate cached Fill planes. Compound journal replay restores exact `VectorParams`.
- README scorecard is generated at the 3,200+ test bucket.

## Judgment calls

- The existing primary `closed + anchors` remains the editable contour. Canvas gestures do not expose secondary-contour editing in this kill, but every primary edit carries secondaries unchanged through SetVector, undo, redo, and journal replay.
- Import semantics are always one layer per SVG path element, not split-filled versus split-stroked modes. Existing K7b-created projects remain valid; only new import grouping changes.
- `Box<Vec<VectorSubpath>>` is an enum-size optimization, not a schema change. Serde still emits a normal JSON array and omits it when empty. Deref is explicit at reference-iteration sites.
- The shared stroke-point counter wraps the existing standalone builder so established cap/join/dash behavior and tests remain unchanged. Aggregate refusal now occurs before the edge-row fallback can hide a compound stroke-point flood.

## Review beam

Round 1: `opencode/beam-free` read-only CLI review of the exact 56,263-byte diff returned **SHIP**, with three residual coverage notes (`vector-compound-review-beam.log`): editor primary-only anchor guards, a dash fixture whose equal contour lengths could not detect phase continuation, and per-contour rather than aggregate stroke-point charging.

Disposition: all three were fixed and regression-pinned. The editor test was itself corrected after an initial false-pass through document mismatch; the final setup isolates the aggregate guard. The dash fixture now uses lengths 0.375 and 1.0. The stroke fixture uses calibrated 20+20 join contours, each individually legal and jointly over-cap.

Round 2: `opencode/beam-free` reviewed the exact updated 62,356-byte diff and returned **SHIP**, explicitly verifying all three dispositions and rechecking the original acceptance scope (`vector-compound-review-r2-beam.log`). No edits or test execution were claimed by either reviewer.

## Verification

- Independent design/coverage oracles: `ARC_ORACLE_PASS 9/9` for the preceding arc base and `COMPOUND_FILL_ORACLE_PASS 8/8` for donut, winding, evenodd, open-contour, and primary-empty behavior.
- Compound-focused Rust tests: 16 passed / 0 failed, including exact 8x8 grids for both fill rules and both windings, open-contour non-subtraction, secondary strokes, dash reset, aggregate contour/anchor/centerline/stroke budgets, cache identity, SVG byte stability, and builder aggregate charging.
- App-focused tests: compound panel undo/redo, primary-edit preservation, secondary cache invalidation, and isolated aggregate editor-cap refusal all passed with real document/cache types.
- Full core library before the final battery: 321 passed / 0 failed; final core library is 324 passed / 0 failed.
- Final battery log: `/home/bessa/.hermes/cache/scratch/vector-compound-final-battery-r2.log`.
- rustfmt, check-layers.py, scorecard.py --check: all exit 0; `git diff --check` clean.
- `cargo clippy -p umber-core -p umber-app --all-targets --locked -- -D warnings`: exit 0.
- Full locked core/app battery under `/tmp/umber-cargo.lock` with `UMBER_ASSETS_DIR=/home/bessa/Projects/umber`: 13 suites, 1,581 passed / 0 failed / 0 ignored. Core lib 324, app lib 321, app bin 889; only the two doc-test harnesses contain zero tests. `BATTERY_DONE fmt=0 layers=0 scorecard=0 clippy=0 tests=0`.
- No schema/project-version change, new dependency, public PR, release/tag, centralhost contact, or L-AI/cb4b2026 work.

## Unverified

No windowed import smoke, GPU/golden claim, or release CI run is claimed. The independent reviews were static read-only reviews; the local battery and focused tests are the execution evidence. Safe-push's affected-crate battery and `git ls-remote` are landing checks, not substitutes for this receipt.

## Deferred

Secondary-contour canvas editing and compound-specific panel affordances, boolean path operations, CSS/effects/unit expansion, schema-level arc preservation, and release work remain outside this kill. Next live vector work is further scoped SVG hardening or a separately designed secondary-contour editing wave.

## Process note

The additive field required 45 exhaustive-literal migrations plus functional-update preservation. A mechanical pass initially touched one `-> VectorShape` accessor; the app compile caught it and the false insertion was removed before the battery. Clippy then caught the `LayerCommand` size regression, leading to the boxed container. The first final battery was green in tests but failed scorecard and a test-only wrapper dead-code lint; both were corrected and the complete battery was rerun to a real all-zero sentinel.
