# 8K COW vs dirty-tile undo A/B: DEFER

- Date: 2026-11-25.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-25-umber-cow-ab.md` (`e9f14c288b07`).
- Claim: `3d0bdc150c22`, queue row 614.
- Implementation: measurement-only; no production COW implementation.
- Kill lock: `razul-cow-ab`.

## Decision

**DEFER COW.** The pre-registered adoption rule required at least 1.5× smaller history bytes at no worse than 1.25× median latency. Full-one clears that rule, but both overlapping cases are only 1.333× smaller, so the meaningful history case does not meet it. Latency is better in every case, but it does not rescue the memory threshold.

## Actual 8192² measurements

The temporary test ran `cargo test -p umber-paint cow_ab_probe -- --nocapture` and asserted final-byte equality between the real `record_stroke` baseline and the sparse 64×64 COW prototype. Three runs per case supplied median elapsed microseconds.

| Case | Baseline bytes | COW bytes | Memory ratio (baseline / COW) | Baseline median µs | COW median µs | Latency ratio |
|---|---:|---:|---:|---:|---:|---:|
| full_one | 536,870,912 | 268,435,456 | 2.000000 | 3,041,139 | 1,931,616 | 1.574401 |
| full_three | 1,073,741,824 | 805,306,368 | 1.333333 | 9,769,175 | 5,942,526 | 1.643943 |
| small_three | 65,536 | 49,152 | 1.333333 | 1,065,739 | 384 | 2,775.361979 |

The first full-three run exposed a prototype bug: blocks were overwritten per block instead of retained per stroke. It was corrected by keying COW blocks by stroke index and replaying them in order; the values above are from the corrected, final-byte-equality run. The probe was then removed with `git restore --source=HEAD -- crates/umber-paint/src/retouch.rs`.

## Review and dispositions

- Independent review `deleg_106ff052`: **SHIP, zero findings**.
- Fresh self-deleting `hermes-verify-cow-matrix-final-*.sh` probe: **47 targeted tests passed / 0 failed** (COW A/B + theme 18 + Preferences 25 + first_run 3), with final-byte equality, clippy, and static gates green. The script restored `retouch.rs` and was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Verification

- The actual probe passed with final-byte equality for all three cases.
- `git status --short` after probe removal was empty, proving no COW code remained.
- This is measurement evidence, not a workspace-green claim.

## Unverified

- Full workspace tests are not rerun for this measurement-only change; the probe itself, clippy, and static gates are the named checks.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- COW is deferred by measurement, not by assumption. No production rewrite follows from this result.
- Final origin check and completion report follow this kill.
