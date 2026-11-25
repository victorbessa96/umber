# Theme matrix: 4 brightness x 7 accents

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-theme-matrix.md` (`4d626fafe718`).
- Claim: `7e003000af7b`, queue row 612.
- Implementation: `4cf46062068fd61d89fedb54a5b2fb58c1c537ec`.
- Reviewed implementation-delta SHA-256: `71d548d147c47469a5ead9909e4a08b96ef23f699abe93b3aa8bcb3e35ccf9b6`.
- Kill lock: `razul-theme-matrix`.

## Built

- Added `Theme::Dim` and `Tokens::DIM`, so brightness is exactly Dark / Dim / Light / High Contrast. Dim has its own canvas palette.
- Added `Accent`: Ember / Amber / Azure / Verdant / Violet / Crimson / Cyan, with explicit colors for every brightness level, 28 combinations total.
- Added `Theme::tokens_with_accent`, `themed_matrix_style`, `apply_matrix`, and `active_accent`. Existing `apply_theme` and `themed_style` default to Ember, preserving old palettes and wrappers.
- Live token overrides still win. `tokens()` follows both active values; `accent()` and `focus_ring_stroke()` follow the selected accent.
- Preferences persists `accent=...` with Ember as the default, exposes both selectors, and re-applies the matrix when either preference changes through `applied_accent`.
- Quick Setup adds Dim. Both catalogs carry Dim, Accent, and all seven accent labels.
- Exported `Accent` from `umber_gui` so app-side preferences and selectors compile.

## Judgment calls

- The initial Dim faint token failed the raised-surface floor at 3.9438155:1. It was changed to `#BCC2CC`, measured at 4.5919187:1 on raised and 5.8258931:1 on panel. The matrix test now covers all 28 accent contrasts at ≥ 3:1, and existing text contrast checks cover Dim.
- Matrix tests do not mutate the process-global active theme or accent. They use `tokens_with_accent` and `themed_matrix_style` directly.

## Review and dispositions

- Independent review `deleg_61ecb62c`: **SHIP, zero findings**.

## Verification

- `cargo test -p umber-gui theme` → **18 passed / 0 failed**, including all 28 combinations and Dim canvas/name/contrast coverage.
- `cargo test -p umber-app preferences` → **25 passed / 0 failed**, including Dim and every accent round-trip plus old-file Ember defaults.
- `cargo test -p umber-app first_run` → **3 passed / 0 failed**, including Quick Setup Dim mapping.
- `cargo test -p umber-app` → **1,281 passed / 0 failed** across all suites.
- `cargo clippy -p umber-gui -p umber-app --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- Fresh self-deleting `hermes-verify-theme-matrix-*.sh` probe: **46 targeted tests passed / 0 failed** (18 theme + 25 Preferences + 3 Quick Setup), strict gui+app clippy clean, and static parity/layers/scorecard/diff gates green. The script was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Unverified

- Full workspace tests are not rerun for this seven-file UI slice; only the focused theme/preferences/first-run tests, full app tests, clippy, and static gates ran.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- The 8K COW A/B measurement follows this kill.
