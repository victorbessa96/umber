# Bounded SVG `rgba()` paint support

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-svg-rgba.md` (`80c6b8f9a1fb`).
- Claim: `507443e02e21`, queue row 594.
- Implementation: `8baf5e2d4109dfc21d5b181ca5d48214f2de7543`.
- Final pre-commit source-diff SHA-256: `340d3b4b56d3165ed64ebe771e3cfcf36ad86c89606dc35a658fb423f597c9ad`.
- Kill lock: `razul-svg-rgba`.

## Built

- Added CSS2 `rgba(r,g,b,a)` parsing, case-insensitive, with exactly four comma-separated components.
- The first three components reuse the existing `rgb()` rules: integer or percentage forms, nearest-byte percentage conversion, gamut clipping, and no mixed forms or exponent syntax.
- The fourth component accepts a finite number in `0..1` or a percentage in `0%..100%`, clipped to the valid alpha range and converted through the existing `with_alpha` rounding path.
- `ParsedPaint` carries RGB plus alpha, so fill/stroke compose multiplicatively with `fill-opacity` and `stroke-opacity`. Default attribute opacity is 1.0, preserving the function's own alpha.
- Canonical export remains uppercase hex plus the appropriate opacity attribute, never `rgba()` output. No schema or model change.
- Malformed components, mixed forms, non-finite alpha, malformed percentages, trailing junk, and CSS4 space/slash syntax are refused with the existing paint error, never accepted as opaque.

## Review and dispositions

- R1 `deleg_54e02a61`: **REQUEST_CHANGES**, one valid finding. Non-finite RGB overflow was clamped into gamut instead of refused. `parse_functional_rgba` now rejects `!value.is_finite()` for RGB only, while `parse_functional_rgb` preserves its existing clipping contract. New 400-digit integer/percentage positive and negative overflow cases are refused, and the existing functional-RGB clipping tests still pass.
- R2 `deleg_feabc962`: **SHIP**, zero findings. It independently confirmed every non-finite `rgba()` RGB component is rejected before gamut clipping, all positive/negative integer/percentage overflow variants are covered, and `parse_functional_rgb` remains unchanged with its existing clipping tests.

## Verification

- RED: `rgba_accepts_css2_forms_and_composes_opacity` failed with `Unsupported { what: "fill paint rgba(255,0,128,0.5)" }`. The malformed-input test already passed and did not count as acceptance.
- GREEN: exact `rgba_` tests **2/2**: integer/percentage alpha forms, `fill-opacity` composition, stroke composition, case-insensitive function name, zero alpha, and canonical export→import→export.
- Strict `cargo clippy -p umber-core --all-targets -- -D warnings` → exit 0.
- Full `cargo test -p umber-core` with `UMBER_ASSETS_DIR=/mnt/hdd/data/Projects/umber`: **358 passed, 0 failed** with zero doc-test failures.
- `~/.cargo/bin/rustfmt --edition 2021 --check crates/umber-core/src/svg.rs crates/umber-core/src/vector_svg.rs` → exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean.
- A fresh OS-safe `hermes-verify-svg-rgba-*.sh` probe reran the exact acceptance tests after the R1 fix: **2/2**, `ADHOC-VERIFY: PASS`, script deleted and verified absent. This is focused ad-hoc evidence in addition to the core suite, not a workspace-green claim.

## Judgment calls

- Alpha is composed multiplicatively with presentation opacity rather than replacing it. `rgba()` alpha × `fill-opacity`/`stroke-opacity` preserves the existing attribute contract and the canonical hex+opacity export shape.
- Zero alpha follows the existing transparent-paint behavior: no stroke and no painted fill effect. It never falls back to opaque.
- The slice stays bounded to `rgba()`: no CSS4 syntax, named colors, `currentColor`, stylesheets, paint servers, root/group `vector-effect`, or source-style preservation.

## Unverified

- No dedicated interactive application-window SVG import smoke was run. The real parser, paint composition, canonical round-trip, and full core suite carry the evidence. No Windows run, full workspace test, or fresh CI dispatch was performed. Per-push CI is disabled by design; safe-push supplies the affected-crate gate.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Next independent kill: the control-channel smoke tail.
- CSS4 syntax, named colors, `currentColor`, stylesheets/selectors/classes, paint servers, root/group `vector-effect`, and source-style preservation remain separate follow-ups.
