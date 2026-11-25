# Late material-picker acceptance coverage

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-material-picker-coverage.md` (`e08bdada24f4`).
- Claim: `50d0d5302fa2`, queue row 617.
- Implementation: `a40cdb7f042598006942e00376310e9e8f0a2081`.
- Reviewed implementation-delta SHA-256: `17a6060d856c2cd8e971313799a108fd470bb7ba3d25b816d62427d33d29cb89`.
- Kill lock: `razul-material-picker-coverage`.

## Built

Three tests added to the existing `material_pick_tests` module, with no production behavior changes:

- Active mode preserves a selected fill's non-default `Planar` projection, triangle mask, and target channel while replacing only its sampled Solid.
- Active mode returns `Ignored` with no selection and with a non-fill selection, leaving layer count and history unchanged.
- All-channel mode excludes a declared-but-unmaterialized channel. A `resize_channel` size override records the declaration without allocating a plane; `planes_present()` is byte-for-byte unchanged and only baseColor + custom fills are created.

This closes the late R1 review `deleg_e9845b8b`'s third finding. Its first two findings were already fixed in the landed implementation.

## Review and dispositions

- R1 `deleg_d21b9d3e`: **REQUEST_CHANGES**, one valid finding. The active-preservation fixture used default `baseColor`, so a reset to the default would pass. Fixed by using non-default `custom`; baseColor sampling remains independent through `paint.active_channel()`. Picker filter still 8/8.
- R2 `deleg_d7f9eeea`: **SHIP, zero findings**.

## Verification

- `cargo test -p umber-app material_pick` → **8 passed / 0 failed**, including all three new acceptance legs and the existing five tests.
- `cargo clippy -p umber-app --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- Fresh self-deleting `hermes-verify-picker-coverage-*.sh` probe: **8 targeted tests passed / 0 failed**, clippy and static gates green. The script was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Unverified

- Full workspace tests are not rerun for this test-only delta; the picker filter, clippy, and static gates ran.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Final origin check and completion report follow this coverage kill.
