# K8h — Bounded inline SVG style declarations

- Date: 2026-11-22.
- Author: Razul (dragon).
- Brief: `docs/loop/briefs/2026-11-22-umber-vector-inline-style.md` (`3d86ad23c1224b4c6db71aea7430ea2c530ab068`).
- Claim: `910b0f75873b6ef4bdfbfb808f1eb9464a5ba825`.
- Implementation: `6e2e5b40a45ec3f97d5381bdb8759aa1c5aade34`.
- Kill lock: `razul-vector-inline-style`.
- Locked surface: `crates/umber-core/src/svg.rs`, `vector_svg.rs`, the K8h brief/report, queue, coordination, STATE, and ROADMAP.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8h brief.
- Final pre-commit diff SHA-256: `a2d0d3cb059867328bd3f04391fc05dd225e8adea07bf472fde6759d45122fdf`.

## Built

- `expand_inline_style` parses bounded inline `style` declarations for editable vector import. Whitelisted properties: `fill`, `fill-opacity`, `fill-rule`, `stroke`, `stroke-opacity`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`, `stroke-dasharray`, `stroke-dashoffset`, `display`, `visibility`, and `vector-effect`.
- Inline declarations override presentation attrs; later declarations replace earlier ones. Property names ASCII-lowercase. Empty style/declarations are ignored. Missing name/colon/value and unknown properties are typed refusals.
- A trailing ASCII-case-insensitive `!important` suffix is stripped whether or not whitespace precedes `!`; trailing whitespace is trimmed and an empty remainder is refused. Spaced and compact forms both work, and importance does not reorder declarations within one inline style.
- Synthesized `Attr` preserves `value_offset = style.value_offset`. Expansion runs at root before geometry, group before existing validation, and shape before display/visibility. Group errors use the label `group`.
- Root/group vector-effect remains refused; hidden leaves still validate. Canonical export emits presentation attrs, not source style text. Raster style handling, model/schema, app UI, and export production are unchanged.
- Tests cover all 14 whitelisted properties, root/group/path precedence, an unmasked group `fill:none` child, last-wins, important-first/normal-later, compact and spaced mixed-case `!important`, property case folding, empty style/declarations, hidden display/visibility, non-scaling-stroke, canonical export without `style=`, malformed declarations, unsupported opacity/transform/CSS variables, and hidden invalid style validation.

## Verification

- Initial RED: both K8h tests failed on the old root/path style refusal. Fixture corrections: close the styled path for fill export and use uppercase canonical hex. Group error label corrected from `g` to `group`.
- R1 `deleg_d7f69ee0` found the compact-`!important` blocker and missing fill-rule/stroke-opacity coverage. Regressions were added before the fix: compact mixed-case paint and `display:none!important`, plus root inline `fill-rule:evenodd` and `stroke-opacity:0.25` with exact EvenOdd/alpha assertions. RED reproduced invalid paint and three-versus-two shapes. The suffix parser was fixed; focused K8h 2/2 and vector_svg 54/54 went green.
- R2 `deleg_1356bb2a` confirmed the runtime blocker fixed and found two coverage gaps plus a stale `build_path` comment. A group child without its own fill now inherits `fill:none` over `fill="#ff0000"` with exact transparent-fill/stroke assertions; an important-first/normal-later duplicate asserts the later normal declaration wins; the comment was corrected. Focused K8h 2/2 and vector_svg 54/54 remained green.
- R3 `deleg_f3c9b25f` returned one nit after delayed delivery: the `build_path` comment generalized vector inline-style expansion to the raster caller. The verdict was recovered from the completed transcript. The comment now names vector-import callers and unchanged raster handling; `R3_NIT_STATIC_FIX=PASS` and focused tests remained 2/2 green.
- R4 `deleg_c14a1b14` returned APPROVED with zero findings after fresh core tests, clippy, and static gates.
- `rustfmt --edition 2021 --check` on touched Rust: exit 0.
- `python3 scripts/check-layers.py`: GREEN, 16 crates, exit 0.
- `python3 scripts/scorecard.py --check`: README current, exit 0.
- `git diff --check`: clean.
- Strict `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`: exit 0 after the R1 fix.
- Canonical post-fix battery `proc_a01c6478ec8c`: process exit 0 and `full_core_app_exit=0`; core 351/351, app 321 lib + 892 bin, and integration/doc suites totaling 2, 1, 3, 17, 11, 10, 2, 1, 0, and 0 tests all passed with zero failures. Both K8h tests passed. R2/R3 changes after that battery were tests/comments only; production behavior is unchanged.
- Supplemental self-deleting ad-hoc probes used the already-built binary for both K8h regressions and left zero strays. They are not a substitute for the canonical battery.

## Judgment calls

- The parser is deliberately not a CSS engine. Only properties Umber already implements are expanded; everything else is refused rather than dropped.
- Source style text is not preserved on export. Canonical presentation attributes carry the parsed paint/display/visibility/vector-effect state instead.
- Raster style handling remains separate and unchanged; vector-import expansion must not be described as a shared raster behavior.
- R2/R3 test and comment changes did not require another full app battery because production code was unchanged from `proc_a01c6478ec8c`; R4 independently reran core 351/351 and clippy.
- Implementation and status documents land separately so the report and queue cite the exact implementation hash.

## Unverified

- No interactive pointer-driven GUI import smoke was run. Public import/export behavior, precedence, hidden pruning, and non-scaling-stroke are covered by core tests and app integration already present in the battery.
- No claim is made for stylesheets, selectors, classes, CSS transforms, currentColor, variables, source-style export preservation, raster style, or release actions.

## Deferred

- Future lanes may add other supported CSS properties or `<style>` stylesheets as separate designs; neither is part of K8h.
- Release tags and public PRs remain outside this kill.

## Process note

R3/R4 completion messages were delayed, so their final verdicts were recovered from the completed delegation transcripts rather than assumed. The final R4 verdict is APPROVED with zero findings.
