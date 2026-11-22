# K8g — Non-scaling-stroke model/export preservation

- Date: 2026-11-21.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-21-umber-vector-non-scaling-stroke-preservation.md` (`4371f5b8c448d90df1a4f51636f5e2072ffffcf2`).
- Claim: `8b5cb81071be5399ed809ec7a82c5ab8348613b4`.
- Implementation: `1f49a533d0bde872591a7fd80f1bd8b3eeeb9e3e`.
- Kill lock: `razul-vector-non-scaling-stroke-preservation`.
- Locked surface: `vector.rs`, `svg.rs`, `vector_svg.rs`, `layers.rs`, `project.rs`, `vector_panel.rs`, the K8g brief/report, queue, coordination, STATE, and ROADMAP.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8g brief.
- Final pre-commit diff SHA-256: `024f50a4d83ed74e0a572941b48152b73846868a00fc75885a19ebbd13930ae4`.

## Built

- `Stroke.non_scaling_stroke: bool` is additive with `#[serde(default, skip_serializing_if = "is_false")]`. Legacy JSON without the field loads false; ordinary strokes omit it.
- `stroke_cache_eq` compares the flag, so a flag-only change invalidates vector cache identity.
- K8f import stores true for leaf `non-scaling-stroke` and false for `none`, empty, and `inherit`. Hidden/no-stroke validation and root/group refusals are unchanged.
- `export_svg` emits `vector-effect="non-scaling-stroke"` only when true. Canonical export→import→export is byte-stable for the supported subset.
- K8f's host-scale fixture now proves fixed/ordinary flags, conditional export, reimport recovery, and canonical re-export. A new vector test proves JSON omission/default, true serialization/decode, and cache identity change.
- All existing `Stroke` literals in `vector.rs`, `vector_svg.rs`, `layers.rs`, `project.rs`, and `vector_panel.rs` explicitly default false. The three app panel constructors are mechanical only; no app UI or behavior changed.
- Export docs now state SVG 1.1 geometry/paint plus optional SVG 2 non-scaling-stroke, with configured cap, join, miter limit, and dash pattern. No project-version bump was needed.

## Verification

- RED before implementation: six assertions failed to compile because `Stroke.non_scaling_stroke` did not exist.
- Common literals were defaulted, then custom-miter and `project.rs` sites were fixed; strict clippy subsequently found three `vector_panel.rs` constructors, which now default false.
- Focused `cargo test -p umber-core non_scaling`: 3/3 green, including the host-scale/roundtrip fixture and JSON/cache test.
- Focused `cargo test -p umber-app vector_panel`: 20/20 green after the app constructor closures.
- `rustfmt --edition 2021 --check` on touched Rust: exit 0.
- `python3 scripts/check-layers.py`: GREEN, 16 crates, exit 0.
- `python3 scripts/scorecard.py --check`: README current, exit 0.
- `git diff --check`: clean.
- Strict `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0 after the app defaults.
- Canonical full battery `proc_464d756fac6c`: process exit 0 and `full_core_app_exit=0`; core 349/349, app 321 lib + 892 bin, and integration/doc suites totaling 2, 1, 3, 17, 11, 10, 2, 1, 0, and 0 tests all passed with zero failures. All three K8g tests passed.
- R1 `deleg_3fa6a0b8` found two minor documentation issues: the constructor inventory omitted the `project.rs` test and queue row 575 said “No app production” despite the three mechanical app defaults. Both reproduced as `R1_STATIC_REPRO=FAIL`, were fixed, and passed as `R1_STATIC_FIX=PASS`.
- R2 `deleg_200cfa81` found two more documentation issues: coordination omitted the full `project.rs`/`vector_panel.rs` paths and retained the app-production exclusion, while export docs unconditionally claimed SVG 1.1. Both reproduced as `R2_STATIC_REPRO=FAIL`, were fixed, and passed as `R2_STATIC_FIX=PASS`; fresh core 3/3 and app 20/20 focused tests also passed.
- R3 `deleg_ad7fa1ee` returned APPROVED with two non-blocking nits: the inventory's “Only core files” wording contradicted the following app sentence, and export docs still said K2's round join. Both were fixed and verified as `R3_NIT_STATIC_FIX=PASS`; focused tests remain 3/3 green.
- Supplemental self-deleting ad-hoc probes used the already-built binary for all three behaviors and left zero strays. They are not a substitute for the canonical battery.

## Judgment calls

- The flag is model/export preservation, not a new independent editor transform state. Native rendering continues to use the existing UV geometry and stroke values.
- False-default JSON omission preserves old native files and avoids a project-version bump. Only true values add a field.
- The lock and brief were expanded for discovered `project.rs` and `vector_panel.rs` constructor sites; these are mechanical compile closures, not UI behavior changes.
- Documentation review iterated until every stale claim was corrected. No code behavior changed after R1; R2/R3 changes were docs/comments only.
- Implementation and status documents land separately so the report and queue cite the exact implementation hash.

## Unverified

- No interactive pointer-driven GUI import smoke was run. Public import/export behavior, cache identity, native JSON, and app panel integration are covered by the tests.
- No claim is made for root/group vector-effect, other vector-effect values, independent editor transforms, CSS style, opacity, clip/mask/filter, markers, or release actions.

## Deferred

- K8h can add bounded inline `style=` declarations for already-supported properties with CSS inline-style precedence; that work is not part of K8g.
- Root/group vector-effect, other values, project-version changes, tags, releases, and public PRs remain outside this kill.

## Process note

The result JSON initially flushed after the process had exited; the final receipt was read from `proc_464d756fac6c.json` and every suite tally was extracted before landing. No missing result file was treated as a passing test.
