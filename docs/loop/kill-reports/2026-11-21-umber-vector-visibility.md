# K8d — Editable vector-import display and visibility

- Date: 2026-11-21.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-21-umber-vector-visibility.md` (`eb9dff7cd0e5c18d5e8268097b951a031afb9e2a`).
- Claim: `ea820787c000682cd09847c2128aa290e49c9b89`.
- Implementation: `4f1858aa763c9b83a8d2e0bd994607050fbb5748`.
- Kill lock: `razul-vector-visibility`.
- Locked surface: `crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`, `crates/umber-app/src/vector_panel.rs`, the K8d brief/report, queue, coordination, STATE, and ROADMAP.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8d brief.
- Reviewed pre-commit diff SHA-256: `0178a8fcee531168cafcd5f87f9f1e453fb3d7cdc7c7aad8c87488820cf8e6a0`.

## Built

- `svg.rs`: adds scoped `VectorImportFrame` state for inherited paint, effective CTM, effective display pruning, and inherited visibility. Root state seeds the first frame; nested groups compose display with `parent.display && display` and visibility with the child-overridable inheritance rule.
- `display:none` suppresses direct rendering without becoming an unknown-subtree `None` frame. Descendants still validate display/visibility, unsupported effects, transforms, and inherited paint, but never build geometry or consume shape/contour/anchor budgets. A child cannot re-enable rendering below a pruned ancestor.
- `visibility` accepts `visible`, `hidden`, `collapse`, empty, and `inherit`. Hidden/collapse suppress the current shape; a visible child can override a hidden group. Invalid values remain typed `SvgError::Unsupported` errors.
- Hidden leaves always evaluate transforms and validate inherited paint outside the rendered geometry branch. Their path data is not parsed. Visible zero-geometry shapes retain the previous behavior of skipping paint parsing.
- Non-rendering basic shapes follow the same path as hidden paths. Mixed documents return only visible shapes in original order; all-hidden documents retain the typed `no supported vector geometry in vector import` refusal.
- `vector_svg.rs`: adds focused display, visibility, pruned-descendant validation, hidden-leaf validation, and hidden-budget tests. Old blanket display/visibility refusal cases are removed; opacity remains the pre-geometry refusal witness.
- `vector_panel.rs`: adds a visible-only import regression proving two visible layers install in one undo group and redo restores both. Production panel code is unchanged.
- Module docs now distinguish the raster path from editable vector import; export, serialization, model, and raster production paths are unchanged.

## Verification

- Initial RED: three core display/visibility tests and the panel test failed against the old typed refusals. Initial GREEN: focused core `vector_svg` 48/48 and panel 1/1 passed.
- R1 review `deleg_af72927a`: REQUEST_CHANGES with two blockers. The reviewer reproduced pruned-descendant effect bypass and hidden-leaf transform/paint bypass through the public import API.
- Both R1 findings were reproduced RED locally: `display_pruning_still_validates_descendants` failed on accepted pruned opacity, and `hidden_leaves_validate_transform_and_paint_without_changing_zero_geometry` failed on accepted hidden invalid transform.
- R1 GREEN: both new regressions pass; complete `vector_svg` suite is 50/50.
- `rustfmt --edition 2021 --check` on all touched Rust: exit 0.
- `python3 scripts/check-layers.py`: GREEN, 16 crates, exit 0.
- `python3 scripts/scorecard.py --check`: README current, exit 0.
- `git diff --check`: clean.
- Strict `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0.
- Fixed full battery `proc_f0ba87cb4e7a`: process exit 0 and `full_core_app_exit=0`; core 343/343, app 321 lib + 892 bin, and integration/doc suites totaling 2, 1, 3, 17, 11, 10, 2, 1, 0, and 0 tests all passed with zero failures. Both R1 regression tests and all original K8d tests passed.
- R2 review `deleg_263daeff`: APPROVED, zero findings, on the exact pre-commit diff SHA-256 above. The reviewer reran the complete core+app battery, strict clippy, rustfmt, layers, scorecard, and diff-check, all exit 0.
- Supplemental ad-hoc probes used the already-built core test binary for the two R1 behaviors, printed actual PASS tallies, and self-deleted with zero shell strays. They are not a substitute for the canonical battery above.

## Judgment calls

- Display pruning and unknown-subtree skipping are separate mechanisms. A pruned root/group still traverses for semantic validation; this is stricter than simply dropping descendants and prevents invalid SVG from hiding behind `display:none`.
- Hidden-leaf paint validation is deliberately outside the rendered branch, while visible zero-geometry shapes keep their established skip-paint behavior. This avoids both the R1 validation bypass and an unrelated behavior change.
- Hidden SVG elements are filtered at import, not converted into hidden Umber layers. No hidden state is persisted or re-exported.
- All-hidden input remains a typed refusal instead of producing an empty document. The panel inherits this behavior without a special case.
- Implementation and status documents land separately so the report and queue cite the exact reviewed implementation hash.

## Unverified

- No interactive pointer-driven GUI import smoke was run. Public import API behavior and the panel's grouped document history are covered by core/app tests.
- No claim is made for opacity, CSS style, clip/mask/filter/markers, vector-effect, paint-order, events, references, text layout, clipping geometry, or hidden-element roundtrip preservation. Those remain out of scope.

## Deferred

- Secondary-contour editor affordances and further SVG presentation attributes remain separately scoped work.
- Version bumps, tags, releases, public PRs, schema/project-version changes, new dependencies, and hidden-layer UI remain outside this kill.

## Process note

The first canonical battery passed before R1, but the adversarial review found two real semantic gaps the original tests missed. Both were reproduced RED, fixed, rerun to GREEN, and covered by a fresh full battery before R2 APPROVED. That is the intended review-hardening loop, not a contradiction in the receipts.
