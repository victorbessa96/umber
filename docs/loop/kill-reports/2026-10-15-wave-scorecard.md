# Kill report: wave-scorecard, the README's numbers come from the tree

Date: 2026-10-15. Author: dragon. Locks: `wave-scorecard` (the script,
README, this report) and `wave-scorecard-ci` (`ci.yml`). ROADMAP
`wave-scorecard`. No cargo needed: pure Python, stdlib.

## Finding

README said "16 crates, 1,600+ tests". HEAD has **1,903** `#[test]`
functions, 300 more than claimed: the hand-written number had drifted
for days. The crate count (16) was right.

## Built

- `scripts/scorecard.py`: counts workspace members (root `Cargo.toml`)
  and `#[test]` attributes under `crates/`, then rewrites the marked
  spans `<!-- scorecard:NAME -->…<!-- /scorecard -->` in README.md.
  - `--check` exits 1 on a stale span; `--print` shows the numbers.
  - An unknown marker name is an error, never silently skipped.
  - Tests are shown as a floor (`1,900+`), so adding tests does not
    churn the README every commit; it moves at each hundred.
- README: both crate mentions and the test count are marked.
- CI: a "Scorecard current" step in the cargo-free `gates` job, beside
  the layering check.

## The count is static, and deliberately so

It counts tests in the tree, not tests that ran. A run count depends on
the host, the features and the GPU adapter: GPU tests return early with
no adapter, and Windows now skips four. The static count is what "the
project has N tests" means.

## Verified (no cargo)

On a clean `git worktree` of HEAD: `--print` → crates 16, tests 1,903;
`--check` on the old README → exit 1 naming `1,600+ → 1,900+`;
rewrite; `--check` → exit 0. The working tree (1,912, which includes the
parked kills' 9 new tests) rounds to the same 1,900+, so the committed
README is true at HEAD and stays true when they land.
