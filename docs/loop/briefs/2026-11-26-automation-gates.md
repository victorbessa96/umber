# Kill brief: mandatory automation-gate hardening

- Lock: `razul-automation-gates`.
- Trigger: tools audit findings 7–9 at `ec979743` showed that the local mandatory path does not enforce generated parity, asset changes receive a false-green test scope, and scorecard accepts missing required markers.
- Scope: scripts only, plus direct stdlib/shell regressions and the kill report.

## Required fixes

1. Add `python3 scripts/parity.py --check` to `safe-push.py` and `wave-battery.sh`, with a stale-output negative test in the existing scratch-repo harness.
2. Expand `affected_crates` for `assets/` to every crate with asset-dependent tests: `umber-core`, `umber-cli`, `umber-gui`, and `umber-app`, then preserve reverse-dependency closure and sorted cargo arguments. Add the exact scope regression to `test_safe_push.py`.
3. Require exactly one of each scorecard marker named by `values()` (`crates`, `tests`, `tests-exact`) in both generation and `--check`. Missing and duplicate markers fail; unknown markers remain errors. Add `scripts/test_scorecard.py` covering valid, missing, duplicate, unknown, and stale values without mutating the real README.

## Constraints

- No release, upstream PR, centralhost, or unrelated source edits.
- Keep all existing gate order and ledger/FF rules intact. New gates must read the pushed commit's tree, not the dirty working tree.
- No broad shell refactor or new dependencies.

## Verification

- `python3 scripts/test_safe_push.py`.
- `python3 scripts/test_scorecard.py`.
- Real `python3 scripts/scorecard.py --check`, `python3 scripts/parity.py --check`, `bash -n scripts/wave-battery.sh`, `python3 -m py_compile` on touched Python files, and `git diff --check`.
- Independent bounded review before commit.
