# K8f — Leaf non-scaling-stroke import

- Date: 2026-11-21.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-21-umber-vector-non-scaling-stroke.md` (`7718320e24d2327e8bfedebda90afeb687253201`).
- Claim: `9435dde48949dceef8a09cdc3cec889bcb328fad`.
- Implementation: `9071c3df89fdd320c94b15365546322cb22323bc`.
- Kill lock: `razul-vector-non-scaling-stroke`.
- Locked surface: `crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`, the K8f brief/report, queue, coordination, STATE, and ROADMAP.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8f brief.
- Reviewed pre-commit diff SHA-256: `b967ba543bd1f6cc82ca78319f3a4fc2b98989197b1e228e7422d0e7b5fb26d7`.

## Built

- Editable vector import accepts leaf `vector-effect="non-scaling-stroke"` under SVG 2 §8.13 and §13.6. Root and group `vector-effect` remain typed refusals; no inheritance or group/root form was added.
- `parse_vector_paint` validates the local value at function entry, before the no-stroke early return. `none`, empty, and `inherit` retain ordinary behavior; `non-scaling-stroke` selects host-viewport units; every other value is a typed refusal. Hidden leaves retain this validation.
- Non-scaling width, dash lengths, and dash offset use `1 / min(viewport_width, viewport_height)`. Ordinary strokes keep `effective_ctm.max_scale()` times that viewport scale. Geometry continues to use the full effective CTM.
- `effective_vector_paint` merges only the current graphics element's non-inherited `vector-effect` into inherited paint at both rendered and hidden call sites. It is not in `VECTOR_INHERITED_PAINT_ATTRS`; group/root forms remain refused.
- The old blanket path refusal fixture now uses `non-scaling-size`, which remains unsupported.
- Named limitation: the imported `Stroke` stores the resulting UV width/dashes, not the source `vector-effect` flag. `export_svg` does not emit or preserve `vector-effect`; no model/schema field or export roundtrip is claimed. Raster, app, model, serialization, and export production are unchanged.

## Verification

- RED before implementation: `non_scaling_stroke_uses_host_viewport_units` and `non_scaling_stroke_scope_values_and_shear_are_pinned` both failed on `path vector-effect in vector import`.
- First implementation review exposed local attribute stripping by `inherited_vector_paint`; `effective_vector_paint` now restores only the current leaf's attribute. A second failure exposed no-stroke validation order; value parsing moved before the early return.
- Focused K8f tests: 2/2 green.
- Complete `vector_svg` suite: 52/52 green.
- `rustfmt --edition 2021 --check` on touched Rust: exit 0.
- `python3 scripts/check-layers.py`: GREEN, 16 crates, exit 0.
- `python3 scripts/scorecard.py --check`: README current, exit 0.
- `git diff --check`: clean.
- Strict `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0.
- Canonical full battery `proc_2f14d805e2f3`: process exit 0 and `full_core_app_exit=0`; core 348/348, app 321 lib + 892 bin, and integration/doc suites totaling 2, 1, 3, 17, 11, 10, 2, 1, 0, and 0 tests all passed with zero failures. Both K8f tests passed.
- Independent review `deleg_c0427ff2`: APPROVED, zero findings, on the exact pre-commit diff SHA-256 above. The reviewer reran focused tests 2/2 plus fmt/layers/scorecard/diff checks; supplied full battery and clippy results were not rerun.
- Supplemental ad-hoc probes used the already-built core test binary for both K8f behaviors, printed actual PASS tallies, and self-deleted with zero shell strays. They are not a substitute for the canonical battery.

## Judgment calls

- Host-viewport scaling intentionally excludes viewBox zoom as well as element transforms, matching SVG 2's furthest-ancestor viewport rule. The ordinary path remains effective-CTM scaled.
- `vector-effect` is merged locally after inherited paint composition because it is not an inherited property. This preserves K8d's hidden-leaf validation without introducing a group/root inheritance path.
- Validation happens even when no stroke is present, preventing invalid values from hiding on unstroked or hidden leaves.
- Export non-preservation is explicit rather than implied. A follow-up model-preservation lane must add a default-false field, cache identity, native JSON compatibility, and SVG export emission before claiming roundtrip fidelity.
- Implementation and status documents land separately so the report and queue cite the exact implementation hash.

## Unverified

- No interactive pointer-driven GUI import smoke was run. Public import API geometry, width/dash values, scope errors, and full app integration are covered by the tests.
- No claim is made for `non-scaling-size`, `non-rotation`, `fixed-position`, marker behavior, CSS style, opacity, clip/mask/filter, `paint-order`, or vector-effect roundtrip preservation. Those remain out of scope.

## Deferred

- K8g can preserve non-scaling-stroke in `Stroke`, cache identity, native JSON, and SVG export as an additive false-default field; that work is not part of K8f.
- Version bumps, tags, releases, public PRs, schema/project-version changes, new dependencies, and root/group vector-effect behavior remain outside this kill.

## Process note

The result JSON initially flushed after the process had exited; the final receipt was read from `proc_2f14d805e2f3.json` and every suite tally was extracted before landing. No missing result file was treated as a passing test.
