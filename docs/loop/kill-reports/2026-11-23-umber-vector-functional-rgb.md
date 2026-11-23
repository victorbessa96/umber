# K8i — CSS2 functional RGB editable-vector import

- Date: 2026-11-23.
- Author: Razul (dragon), session `20261123_093546_365ddd`.
- Brief: `docs/loop/briefs/2026-11-23-umber-vector-functional-rgb.md` (`7af220fb633122fd7414036106fa126e068809fa`).
- Implementation: `0e54303887d398500f392d443844aaefbb8b36d8`.
- Kill lock: `razul-vector-functional-rgb`.
- Locked surface: `crates/umber-core/src/svg.rs`, `crates/umber-core/src/vector_svg.rs`, the K8i brief/report, queue, coordination, STATE, and ROADMAP.
- Design sections implemented: Target, Concurrency and safety, Verification, and Non-goals in the K8i brief.
- Final pre-commit source-diff SHA-256: `cc5dfcea2fea11e5ab8933f784d71ff35a9fdbab1ee4ef47607b3be16769eea4`; committed content was compared equal after excluding only git's `index` metadata lines.

## Built

- `parse_functional_rgb` accepts ASCII-case-insensitive CSS2 `rgb(...)` with exactly three comma-separated integer or percentage components. Optional signs and percentage decimals work; mixed forms, integer decimals, exponents, CSS4 space/slash forms, malformed arity/commas/values, and other color functions remain typed `SvgError::Unsupported` failures.
- Integer components clip to 0..=255 and percentages to 0..=100. Percentages convert to the nearest byte, matching `with_alpha`'s rounding convention. Values that overflow `f64` still clip to the appropriate endpoint; only NaN is rejected after the lexical gate.
- The existing `parse_svg_color` entry point now handles this syntax for fill and stroke, so root/group/leaf inheritance, inline styles, opacity composition, hidden-leaf validation, basic shapes, and public import APIs share the behavior. Existing `none`, `#RGB`, and `#RRGGBB` handling is unchanged.
- Export remains canonical uppercase `#RRGGBB`, with export→import→export byte stability. Raster coverage, app, model, schema, dependencies, and release state are untouched.
- Five focused tests cover accepted forms and gamut clipping (including 400-digit values), root/group/leaf inline style and inheritance, fill/stroke opacity, canonical export, basic shapes, malformed fill/stroke, and parser-independent raster coverage.

## Review and dispositions

- Live canary: OpenCode `deepseek-v4.1-flash-free` returned `CANARY-OK`.
- R1, independent OpenCode/DeepSeek read-only review (`umber-functional-rgb-review-r1.log`): **SHIP**, with one LOW and one NIT plus test-honesty gaps. It independently differential-fuzzed about 57,000 near-valid strings and found no mismatch except the overflow case.
  - LOW: 400-digit valid CSS2 numbers overflowed to infinity and were refused instead of clipped. Fixed by rejecting only NaN after lexical validation and letting `clamp` handle ±infinity. Reverting to `!is_finite` reproduced RED 0 passed/1 failed; restoring the fix gave focused 5/5.
  - NIT: the function doc omitted the deliberate syntax boundary. Added the complete exclusion list.
  - Coverage gaps: root/group inline RGB, fill opacity, non-path paint, malformed stroke, and explicit CSS4 slash refusal were added. The raster test was renamed to state its parser-independent evidentiary role.
- R2, final bounded OpenCode/DeepSeek delta review (`umber-functional-rgb-review-r2.log`): **SHIP**, zero remaining findings. Residual non-coverage is limited to the deliberately unsupported color forms and pre-existing `raw.trim()` / inline-style `split_whitespace()` Unicode-NBSP leniencies, which this kill does not alter.
- R2 log SHA-256: `f0171f07eebe9de6e1fcadf98d16f50229bae4be64496d383323732898211c3f`.

## Verification

- Pristine RED: `flock -w 900 /tmp/umber-cargo.lock nice -n 19 env -u UMBER_FONTS_DIR CARGO_BUILD_JOBS=3 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 /home/bessa/.cargo/bin/cargo test -p umber-core functional_rgb_ -- --nocapture` → 2 passed/2 failed; both acceptance tests failed on the old typed `rgb()` refusal, while the refusal matrix and raster control passed.
- Initial patched focused run: 4 passed/0 failed. Final post-hardening focused run: 5 passed/0 failed, 351 filtered out.
- R1 overflow regression witness: the fully-qualified acceptance test failed 0 passed/1 failed with `!is_finite` temporarily restored, then passed after restoring `value.is_nan()`. One earlier mistyped `--exact` filter selected 0 tests and was discarded as non-evidence.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-core/src/svg.rs crates/umber-core/src/vector_svg.rs` → exit 0 at final implementation HEAD.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- `flock -w 900 /tmp/umber-cargo.lock nice -n 19 env -u UMBER_FONTS_DIR CARGO_BUILD_JOBS=3 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 /home/bessa/.cargo/bin/cargo clippy -p umber-core --all-targets -- -D warnings` → exit 0, zero warnings, rerun at committed implementation `0e543038`.
- `flock -w 900 /tmp/umber-cargo.lock nice -n 19 env -u UMBER_FONTS_DIR CARGO_BUILD_JOBS=3 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 /home/bessa/.cargo/bin/cargo test -p umber-core` → `356 passed; 0 failed`, plus doc-tests `0 passed; 0 failed`, rerun at committed implementation `0e543038`.
- Push through `scripts/safe-push.py` and independent `git ls-remote origin master` verification remain the origin gate for the status commit.

## Judgment calls

- This is a deliberately bounded CSS2 numeric-syntax addition, not a CSS engine. Mixed integer/percentage triples, exponent spellings, CSS4 space/slash forms, `rgba`, named colors, `currentColor`, and every other functional notation fail loudly rather than being approximated.
- Percentage-to-byte rounding is nearest, not truncation; 30%→77 and 50%→128 distinguish that choice. Out-of-range values follow CSS2 clipping rather than refusal, including numerically overflowing but lexically valid values.
- Source `rgb()` spelling is not preserved. Canonical hex export is the stable interchange contract.
- The raster test is intentionally a control: raster treats any non-`none` paint as coverage and does not call the new parser. It is not claimed as editable-import coverage.

## Unverified

- No interactive GUI import smoke, Windows run, full workspace test, or fresh CI dispatch was run for this core-only parser kill. The complete changed-crate suite and strict changed-crate clippy ran locally at committed HEAD; safe-push supplies its affected-crate gate.
- Pre-existing Unicode/NBSP whitespace leniency outside the CSS2 tokenizer remains unchanged and is not claimed fixed. Unsupported color forms named above remain outside this kill.

## Deferred

- Stylesheets/selectors/classes, CSS4 color syntax, named colors, `currentColor`, alpha functions, non-RGB color spaces, and paint servers require separate designs. No second kill or release action is included.

## Process note

Brief `7af220fb` landed before any source/test edit. The full kill lock was registered before the first RED test edit and remained held through implementation, review, and this report. Implementation and status documents land separately so the report and queue cite the exact implementation hash.
