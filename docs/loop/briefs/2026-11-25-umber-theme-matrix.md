# Design brief: theme matrix (2026-11-25)

Planned kill lock: `razul-theme-matrix`.
Base: EyeDepth status tip `bcce1321f28bf0a8ee9e8ba5b998da950f31625c`, clean and origin-equal.

## What exists

`Theme` has Dark, Light, and High Contrast with one `Tokens` palette each. `apply_theme` and `themed_style` install a palette, while `UMBER_THEME_FILE` overrides named tokens without changing the base theme. Preferences persists one optional theme; Quick Setup maps those same choices. No fixed brightness/accent matrix exists.

## Target

- Extend `Theme` with `Dim`, so brightness is exactly Dark / Dim / Light / High Contrast. Add `Tokens::DIM` and Dim canvas colors. Preserve existing names and Follow OS resolution.
- Add `Accent` with exactly Ember / Amber / Azure / Verdant / Violet / Crimson / Cyan. Each accent has a persisted name and an explicit color for each of the four brightness levels, 28 combinations total.
- Add `Theme::tokens_with_accent`, `themed_matrix_style`, and `apply_matrix`. Existing `apply_theme` and `themed_style` default to Ember and preserve the current palettes byte-for-byte. `tokens()` follows both active values; `accent()` and `focus_ring_stroke()` follow the selected accent.
- Add `active_accent()` alongside `active_theme()`; `apply_matrix` updates both. Existing wrappers keep Ember as the compatibility default.
- Persist `accent=...` in Preferences with Ember as the default. Preferences shows a second Accent combo beside the four brightness choices. Quick Setup adds Dim while keeping its current theme mapping.
- Add localized labels for Dim, Accent, and all seven accents in both catalogs.

## Failure handling

Do not replace the token system, remove live overrides, change Follow OS, or make Quick Setup invent an accent. Unknown persisted names fall back to Ember. Do not mutate global active-theme state in the matrix tests; test combinations through `tokens_with_accent` and `themed_matrix_style` instead.

## Verification and acceptance

- Matrix test enumerates all 28 combinations and checks accent contrast ≥ 3:1 on its panel. Existing text/semantic contrast tests continue to pass.
- Dim and all accents round-trip by name. Preferences round-trips Dim and every accent, with old files defaulting to Ember.
- Quick Setup maps Dim. Existing Dark/Light/High Contrast and default-Ember behavior remains pinned.
- `cargo test -p umber-gui theme`, `cargo test -p umber-app preferences`, and `cargo test -p umber-app first_run` cover the model and persistence. Strict clippy on `umber-gui` and `umber-app`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No new color-management transform, no per-panel theme model, no ninth accent or fifth brightness, and no change to COW/undo. The 8K COW A/B measurement follows this kill.
