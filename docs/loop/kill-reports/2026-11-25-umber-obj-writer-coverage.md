# Late OBJ writer failure coverage

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-obj-writer-coverage.md` (`93f27b70cbbd`).
- Claim: `2e3b86be4605`, queue row 623.
- Implementation: `adcf99d7da0e678bbf87e382d29d8e661f087ca9`.
- Reviewed implementation-delta SHA-256: `7d5e00e80f4fdb44dc1bc176202acb6df4fdd5ff3873ffea8aaccd0743f2f331`.
- Kill lock: `razul-obj-writer-coverage`.

## Built

Two acceptance gaps closed, with no production behavior changes:

- Every ragged case now calls `write_obj` with a previously nonexistent output directory and asserts `ExportError::Write` plus no directory creation: empty indices, non-multiple indices, out-of-range indices, short normals, and short UVs.
- `create_dir_all` failure is exercised by using a regular file as the output directory, asserting typed `ExportError::Write`.

The existing over-cap writer, huge-name no-directory, and actual `std::fs::write` failure coverage is unchanged.

## Review and dispositions

- Initial review `deleg_39e5824c` stalled after its initial reads and was stopped without a verdict.
- Replacement independent review `deleg_c1270888`: **SHIP, zero findings**.

## Verification

- `cargo test -p umber-export obj::tests` → **9 passed / 0 failed**, including the new writer-level ragged and directory-creation failure assertions.
- `cargo clippy -p umber-export --all-targets -- -D warnings` → clean.
- `cargo fmt --all -- --check`, `check-layers.py`, `scorecard.py --check`, `parity.py --check`, and `git diff --check` all pass.
- This is focused acceptance evidence, not a workspace-green claim.

## Unverified

- Full workspace tests are not rerun for this test-only delta; the OBJ filter, clippy, and static gates ran.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Final origin check and completion report follow this coverage kill.
