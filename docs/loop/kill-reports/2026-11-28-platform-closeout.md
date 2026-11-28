# Platform lens closeout

- Lock: `razul-platform-closeout` plus `razul-full-audit-2026-11-26`.
- Base: `667712f17f2de2dfa00f47d618a18924bd108466`.
- Fresh bounded lens: `deleg_dad198c4`, replacing the pruned original platform report.
- Outcome: all three current LOW findings are addressed; no HIGH/MEDIUM finding and no regressed prior row was found.

## Findings and fixes

1. **Reusable AppImage workflow was absent from packaging triggers.** Added `.github/workflows/appimage-build.yml` to both `on.push.paths` and `on.pull_request.paths` in `.github/workflows/packaging.yml`. Changes to the actual build procedure now trigger the development packaging proof instead of only a later tag build.
2. **Caller-side tool URLs were dead duplicated config.** Removed `APPIMAGETOOL_URL` and `LINUXDEPLOY_URL` from `packaging.yml`. Their only live definitions remain job-scoped in `appimage-build.yml`; caller env does not propagate into a reusable workflow's jobs.
3. **Action-pinning posture was implicit and inconsistent.** `CONTRIBUTING.md` now records the reviewed 2026-11-28 policy: full-SHA action pins in the reusable AppImage workflow; accepted major-version tags in cargo-dist-generated `release.yml` and lightweight CI/packaging orchestration; credential persistence disabled where applicable; local release-workflow hardening must be reapplied after cargo-dist regeneration.

## Verification

A real local closeout battery passed **17/17** on the edited working tree:

- Exact push/PR trigger and env-ownership assertions, plus policy-text presence.
- `flatpak/test_gen_cargo_sources.py`: **5/5 passed**.
- Real `flatpak/gen-cargo-sources.py --check` and `check-runtime-coupling.py`: clean.
- `scripts/test_scorecard.py`: **15/15 passed**.
- `scripts/test_safe_push.py`: **18/18 passed**.
- `bash -n scripts/release-ci.sh`: clean.
- Real scorecard, parity, and layer gates: current/green.
- `git diff --check`: clean.
- PyYAML parsed `ci.yml`, `release.yml`, `packaging.yml`, and `appimage-build.yml` with job maps present.

## Live startup regression and repair

- Push run `46136880608` at `d2efaf91` ended in `startup_failure` with zero jobs. The reusable AppImage job required `contents: write`; the read-only development packaging caller could not elevate that scope.
- Fix `63d25bf0`: the reusable workflow now inherits caller token scope. `release.yml` supplies `contents: write` for the release attach; development packaging can remain read-only with `attach-release=false`.
- Push run `46137587286` at `63d25bf0` completed **SUCCESS**. Lint packaging and the reusable AppImage job passed, including tool identity, Vulkan-absence, and payload-integrity checks; release attach was correctly skipped, and development Flatpak was skipped by its dispatch-only design.
- Focused ad-hoc permission verifier: **6/6 PASS** for caller token scope, release-only attach gating, caller inputs, YAML shapes, live job creation, and diff-check. Temporary script deleted; this is not a full-suite claim.

## Honest boundaries

- Focused ad-hoc verifier after the workflow edit: **7/7 PASS**, covering trigger ownership, reusable env ownership, preserved PR/dispatch gates, policy text, Flatpak ownership/drift, and diff-check. Temporary script deleted; this is not a full-suite claim.
- Independent re-review `deleg_40997d79`: **SHIP — zero remaining findings**.
- Live packaging CI `46137587286` at code commit `63d25bf0`: **SUCCESS**. Later commits in this closeout are documentation-only.
- No tag was cut. Tag-native AppImage attach and Flatpak packaging remain structurally verified until the next release gate.
- The linuxdeploy rolling-download policy remains the documented accepted risk; appimagetool remains SHA256- and exact-version-gated.
