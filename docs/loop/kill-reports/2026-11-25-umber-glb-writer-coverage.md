# Late GLB writer failure coverage

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-glb-writer-coverage.md` (`ec5aaa2f6ad0`).
- Claim: `eacba075ecbc`, queue row 626.
- Implementation: `c3565fe7c440bc7b9e3894dda6c0c3e6c5e18f1f`.
- Reviewed implementation-delta SHA-256: `b9438b22b07f15d72c0ad06220ce7c2d9330472aa358274aa3fd4ff3d6f61066`.
- Kill lock: `razul-glb-writer-coverage`.

## Built

Two acceptance gaps closed, with no production behavior changes:

- Document-level `[0, 1, 2, 0]` reaches divisibility validation, unlike `[0, 1]`, which exits through empty-triangle handling.
- Fresh nonexistent-directory writer assertions cover empty, non-multiple `[0, 1, 2, 0]`, out-of-range indices, short normals, and short UVs with `ExportError::Write`, `!dir.exists()`, and no `.glb` file.

The existing pre-existing-directory failure atomicity, over-cap, huge-name, and actual write-failure coverage is unchanged.

## Review and dispositions

- Independent review `deleg_15967586`: **SHIP, zero findings**.

## Verification

- `cargo test -p umber-export glb::tests` → **4 passed / 0 failed**, including the new divisibility and fresh-directory writer assertions.
- `cargo clippy -p umber-export --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- This is focused acceptance evidence, not a workspace-green claim.

## Unverified

- Full workspace tests are not rerun for this test-only delta; the GLB filter, clippy, and static gates ran.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Final origin check and completion report follow this coverage kill.
