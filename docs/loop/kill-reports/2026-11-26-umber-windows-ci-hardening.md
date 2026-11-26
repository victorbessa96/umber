# Windows CI path identity and WARP budget hardening

- Lock: `razul-windows-ci-hardening`.
- Brief: `4114c5a114d234bee29d9c560944d785f552ce2d`.
- Implementation: `65ffc37d70414f6b37051567257cbaee1b409276`.
- Trigger: current-tip CI run `45760656534` at `42ddbedcd3e4b036665404fcd573f90b7d3903b8`.
- Prior CI state: Ubuntu `139709643770` fully green; Windows `139709643775` failed two acceptance tests.

## Findings and fixes

### Mesh extra-directory identity

`mesh_dirs_with` canonicalized paths only for duplicate comparison, then pushed `extra.clone()`. On Windows the test therefore returned the DOS short-form path while its canonical expectation used the equivalent extended-length path. The function now stores `extra.canonicalize().unwrap_or_else(|_| extra.clone())`, preserving the original path when canonicalization fails. Built-in/exe ordering, first-wins behavior, and duplicate suppression are unchanged.

The existing acceptance test was already the regression: it compares DOS short-form and canonical extended-length spellings. It now passes locally.

### Occluder WARP budget

`wait_for_bake` used 600 × 10 ms, six seconds. Windows WARP exceeded that budget before the worker landed. The helper now uses 60,000 × 10 ms, matching the established 600-second WARP allowance in this test file. No production bake timing or output behavior changed.

## Verification

- Exact assets path-identity regression: **1 passed, 0 failed**.
- Exact occluder combo/request/worker/PNG regression: **1 passed, 0 failed**.
- Full app battery across 11 suites, including both lib and bin targets: **1,284 passed, 0 failed, 0 ignored**. Log: `/home/bessa/.hermes/cache/scratch/umber-windows-ci-hardening-app-tests.log`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- Rustfmt, check-layers, scorecard, parity, and `git diff --check`: clean.
- Independent DeepSeek review at `/home/bessa/.hermes/cache/scratch/umber-windows-ci-hardening-review.log`: **SHIP / NO FINDINGS**. It independently checked canonicalization fallback, first-wins ordering, duplicate semantics, and the test-only timing change. A duplicate review was stopped rather than treated as approval.
- Sibling coordination: a bounded continuation adopted the reboot-orphaned kill, independently verified it, and stood down when this owner resumed. Its full app log is the 1,284-test battery above. This owner re-locked and committed the same reviewed two-file diff; no duplicate implementation was merged.
- Final two-platform CI `45829837272` at `c14d1baf61a263a396cbe09b699152dfcd7e5df8`: **SUCCESS**. Ubuntu `139943504609` passed all steps including GPU and E2E; Windows `139943504498` passed tests, WinInk, and release smoke. Origin/master read-back is the same SHA.

## Non-goals

No changes to the GPU parity repair, bake worker behavior, unrelated WARP timeouts, release packaging, upstream PRs, or centralhost.
