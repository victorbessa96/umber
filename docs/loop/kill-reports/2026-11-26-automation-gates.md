# Mandatory automation-gate hardening

- Lock: `razul-automation-gates`.
- Brief: `384187d43229` (`docs(tooling): automation gate hardening brief`).
- Outcome: all three tools-audit defects are fixed and covered by direct regressions.

## Findings and fixes

1. **Parity was absent from the mandatory local path.** `safe-push.py` now runs `python3 scripts/parity.py --check` after layers, against the exact pushed commit's worktree. `wave-battery.sh` runs the same gate after layers and fails RED on stale output.
2. **Assets received a false-green scope.** `assets/` now touches `umber-core`, `umber-cli`, `umber-gui`, and `umber-app`, then preserves reverse-dependency closure and sorted cargo arguments. In the scratch dependency graph this includes `umber-paint` through app.
3. **Scorecard accepted missing required markers.** `validate_markers` requires exactly one of every `values()` marker and rejects unknown, duplicate, malformed, and whitespace-variant tokens before generation or `--check`. README now has one managed marker per value, including `crates-layout` and `tests-exact`.

## RED/GREEN evidence

- Before the fix, `test_safe_push.py` failed the new parity case (the push incorrectly succeeded) and the asset-scope case (only `umber-app` was gated): **15/17 passed**.
- After the fix, the same harness passes **17/17**, including a failing-parity refusal and the complete asset-dependent crate set.
- `test_scorecard.py` initially failed because `validate_markers` did not exist. It now passes **15/15** marker cases, including digit/uppercase/underscore names, unclosed tokens, and space-less duplicates.
- Real README validation exposed and repaired both a duplicate `crates` marker and a missing `tests-exact` marker; `python3 scripts/scorecard.py --check` is now current.

## Verification

- `python3 scripts/test_safe_push.py`: **17/17 passed**.
- `python3 scripts/test_scorecard.py`: **15/15 passed**.
- Real `python3 scripts/scorecard.py --check`: current.
- Real `python3 scripts/parity.py --check`: current.
- `bash -n scripts/wave-battery.sh`, `python3 -m py_compile` on touched Python files, and `git diff --check`: clean.
- Independent review `deleg_c9e80394` found two marker gaps; fix re-reviews `deleg_dfb7ba18` and `deleg_6ca8db1a` closed the remaining whitespace/token variants and returned **SHIP**. The layout count remains managed through `crates-layout`.
- Commit/push/current-tip CI: local commit next; origin and CI are not yet claimed.

## Non-goals

No release, upstream PR, centralhost work, new dependencies, or unrelated source edits.
