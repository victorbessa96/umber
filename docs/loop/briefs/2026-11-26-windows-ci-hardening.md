# Kill brief: Windows CI path identity and WARP polling budget

- Status: implementation brief, 2026-11-26.
- Lock: `razul-windows-ci-hardening`.
- Trigger: current-tip CI run `45760656534` at `42ddbedcd3e4b036665404fcd573f90b7d3903b8`. Ubuntu is fully green, including the repaired GPU suite and E2E; Windows job `139709643775` has two failures.

## Evidence

1. `assets_panel::tests::mesh_dirs_preserve_bundled_exe_and_extra_order_without_duplicates` fails because `mesh_dirs_with` stores the original DOS short-form `C:\Users\RUNNER~1\...` while the test's canonical path is the equivalent `\\?\C:\Users\runneradmin\...`. The function already canonicalizes for duplicate comparison but stores `extra.clone()`, contradicting its own "first-wins canonical dedup" contract.
2. `bakes_panel::tests::occluder_scope_combo_drives_request_and_resolution` exceeds `wait_for_bake`'s 600 × 10 ms polling budget on WARP. The same test file already records a 600-second allowance for WARP bake tests. The six-second budget is a test defect, not evidence of a production regression.

## Target

- Canonicalize each extra mesh directory before insertion, preserving the existing fallback to the original path if canonicalization fails (for example, a not-yet-existing directory). Keep first-wins ordering and duplicate suppression.
- Raise only the occluder kittest's polling allowance to 60,000 × 10 ms (600 seconds), documenting that it matches the established WARP allowance. Do not alter bake workers, output, timing behavior in production, or any other test timeout.
- Keep the existing two tests as the RED evidence; no new test is needed to prove the path bug because the current Windows assertion already covers it.

## Concurrency and compatibility

One lock covers the two app files and close-out docs. No schema, dependency, i18n, public API, or release change. Non-Windows canonicalization is unchanged. Extras that cannot be canonicalized keep their previous representation.

## Verification

- Run the exact assets and occluder acceptance filters locally, then the full `umber-app` suite and workspace clippy.
- Run rustfmt, check-layers, scorecard, parity, and an independent review.
- Commit and push through `safe-push.py`, dispatch CI on the final tip, and require both Windows and Ubuntu success before closure.

## Non-goals

No changes to the GPU parity repair, release packaging, upstream PRs, centralhost, or unrelated WARP-slow test lists.
