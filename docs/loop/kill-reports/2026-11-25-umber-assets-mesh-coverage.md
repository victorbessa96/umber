# Late Assets mesh acceptance coverage

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-assets-mesh-coverage.md` (`dd1510b3fb1f`).
- Claim: `b66db4b017ff`, queue row 620.
- Implementation: `35f12d3256c199819fca7dfce058f4c74fa6058f`.
- Reviewed implementation-delta SHA-256: `3b59b849c11feb92112ccb096e3abcf667dcb30bf9b5e41e43ff0b6fa5911b51`.
- Kill lock: `razul-assets-mesh-coverage`.

## Built

Two acceptance assertions tightened, with no production behavior changes:

- Mesh no-match is isolated by position: exactly one no-match label below the Meshes header, so other categories cannot satisfy the assertion.
- Directory order is exact: bundled → exe → extra, with built-in aliases included in the extras input and canonical duplicate suppression preserved.

The late R1 review `deleg_915dd909`'s first two findings were already fixed: empty results stay cached until `scan_epoch` changes, and the lister covers missing directories.

## Review and dispositions

- Independent review `deleg_aa4e5245`: **SHIP, zero findings**.

## Verification

- `cargo test -p umber-app mesh` → **32 passed / 0 failed**, including the tightened no-match and exact directory-order assertions, the empty-cache regression, and the missing-directory lister coverage.
- `cargo clippy -p umber-app --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- Fresh self-deleting `hermes-verify-assets-mesh-*.sh` probe: **33 tests passed / 0 failed** (1 lib + 32 bin), clippy and static gates green. The script was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Unverified

- Full workspace tests are not rerun for this test-only delta; the mesh filter, clippy, and static gates ran.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Final origin check and completion report follow this coverage kill.
