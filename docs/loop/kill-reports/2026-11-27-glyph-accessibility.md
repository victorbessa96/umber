# Glyph-button accessible names

- Lock: `razul-glyph-accessibility`.
- Brief: `4d5548c6f6b0` (`docs(gui): glyph accessibility brief`).
- Outcome: every remaining bare glyph `small_button` now has a meaningful localized accessible name.

## Implementation

- Layer duplicate, smart-shelf refresh, saved-search delete, asset refresh, and post-effect remove use the mandatory-tooltip `glyph_button` helper.
- Layer up/down gained distinct `layers.move-up` / `layers.move-down` strings in en-US and pt-BR; sharing `history.op-reorder` made the actions indistinguishable.
- Post-effect move arrows carry localized labels with their actual enabled flags.
- Recent-file remove and mask-toggle also carry localized labels after review found them outside the original call shape.
- The recursive census now extracts balanced multi-line calls across `small_button`, `Button::new`, `selectable_label`, and `ui.button`, inspects second arguments and UPPER_SNAKE constants, and has positive controls for each shape. Set visibility and bake ID-source remove are labeled too.

## Verification

- Pre-fix tree oracle: **7 bare calls** across the three app modules; current tree: **0**.
- Existing kittests initially caught three symbol-coupled queries (layer raise, saved-search delete, duplicate); all now click by accessible action name and pass.
- `cargo test -p umber-gui -- --test-threads=1`: **157 passed, 0 failed**, including positive controls for every detector shape.
- `cargo test -p umber-app -- --test-threads=1`: **926 bin tests plus every lib/integration target passed, 0 failed**, including the new post-effect enabled-state and reorder kittest.
- `cargo clippy -p umber-gui -p umber-app --all-targets -- -D warnings`: clean.
- Rustfmt clean.
- Independent review found two missed glyph paths and census coverage gaps; all were fixed. Final review `deleg_e23ab3e9`: **SHIP**. An independent Python port of the census flagged 11 pre-fix sites and zero current offenders.
- Scorecard/layer/parity gates, commit/push/current-tip CI: not yet claimed.

## Non-goals

No visual redesign, release/tag/upstream PR, or centralhost work.
