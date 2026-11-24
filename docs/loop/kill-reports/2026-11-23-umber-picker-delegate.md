# L7 — Stock `color_button` delegate conversion

- Date: 2026-11-23.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-23-umber-picker-delegate.md` (`1247189065b8`).
- Claim: `b391649b8a56`, queue row 580.
- Implementation: `eb2237a917e6697f990ffe8cb1da09e57f517029`.
- Final pre-commit source-diff SHA-256: `7b8f47dc1e9c57930fa1db52be71b5dc6df7dd0e3ff31db6fcf600bfbcdfa5dc`.
- Kill lock: `razul-picker-delegate`.
- Locked surface: `crates/umber-gui/src/widgets/color_picker.rs`, the brief/report, next-steps census, queue, coordination, STATE, ROADMAP, and the 2026-11-09 wave plan.

## Built

- Replaced the stock button's duplicated 40x18 swatch/popup body with one call to `color_button_with`, using `PickerLabels::default()` and a fresh per-call empty `RecentColors`.
- Deleted 72 duplicated lines. Public signature, linear-RGB writeback, English labels, popup state, previous-colour handling, outside-click/Escape close behavior, AccessKit role, and focus ring all route through the existing shared implementation.
- Added `color_button_stock_shows_default_sliders_and_no_recents_grid`, proving the stock popup exposes R/G/B/H/S/V slider rows and no `colour palette` recents grid. Its name deliberately matches the brief's required `color_button` test filter.
- Updated module and function docs to describe delegation and the per-call recents lifetime. App call sites, persistence, translations, dependencies, color math, and release state are untouched.

## Review and dispositions

- R1 `deleg_32a8ff3e`: **REQUEST_CHANGES**, one P2 test-honesty finding and zero production findings. The first regression name, `stock_button_shows_default_sliders_and_no_recents_grid`, did not match the brief's required `cargo test -p umber-gui color_button` filter, so that command skipped it.
- Disposition: renamed it `color_button_stock_shows_default_sliders_and_no_recents_grid`. The exact brief command then executed both stock acceptance tests: 2 passed, 0 failed, 149 filtered out. The reviewer independently confirmed the delegate's default labels, stack-local recents lifetime, linear writeback, state/close behavior, AccessKit/focus behavior, and docs. It noted six small default-label `String` allocations per stock call as the brief-prescribed allocation delta, not a correctness blocker.
- R2 `deleg_2c79a7b1`: **SHIP**, zero findings. It confirmed the renamed regression is selected by the required filter alongside the original stock test, matching the supplied 2/2 result, and found the final diff scoped to the delegate, docs, and regression.

## Verification

- Clean-base full `umber-gui` suite: 150 passed, 0 failed, plus 0 doc-tests.
- Clean-base focused stock test: 1 passed, 0 failed, 149 filtered out.
- Post-change focused brief command: 2 passed, 0 failed, 149 filtered out; both the original stock-button test and the new no-recents-grid regression ran.
- Full post-change `umber-gui` suite: 151 passed, 0 failed, plus 0 doc-tests.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-gui/src/widgets/color_picker.rs` → exit 0 after formatting the new assertion. The first ad-hoc attempt stopped on that formatting diff before running tests; it was corrected rather than reported as a pass.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- Strict `cargo clippy -p umber-gui --all-targets -- -D warnings` → exit 0, zero warnings.
- Fileless and temporary-script ad-hoc probes reran the exact focused command after the R1 rename: 2/2, `ADHOC-VERIFY: PASS`, scratch script deleted and verified absent. This is focused ad-hoc evidence in addition to the changed-crate suite, not a workspace-green claim.

## Judgment calls

- The stock delegate receives a fresh empty recents list every call. It cannot show the grid while open; any close-time insertion is dropped on return. No recents persistence or cross-call leakage is introduced.
- Default labels are constructed per stock call to preserve the existing public signature and app-free `umber-gui` boundary. The six small `String` allocations are accepted for this compatibility path.
- This is a behavior-preserving refactor. The added test pins a user-visible invariant rather than source shape, so no artificial RED was required.

## Unverified

- No interactive app-driven picker smoke was run; the actual egui popup is exercised through kittest at the widget entry point. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- The refreshed complete remaining-work map is `docs/loop/next-steps-2026-11-23.md`: further bounded SVG hardening, control-channel smoke tail, row-156 candidates requiring verify-open scoping, P2 design tracks, rendering/Windows gates, and human release gates. No second kill rides this wave.
