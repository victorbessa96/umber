# Kill brief: native release graph and packaging truth

- Lock: `razul-release-graph`.
- Trigger: platform audit PLAT-01/03/04/05/06/07/08 found release-upload races, missing write permissions/dependencies/triggers, an ineffective AppImage tool pin, invalid `.DirIcon`, wrong runtime recipe, and stale platform claims.
- Scope: release/packaging workflows, release gate, AppImage/Flatpak docs, SECURITY, CONTRIBUTING, CI comments, and the kill report. Flatpak source/lock ownership is a separate kill.

## Required fixes

1. Move tag-native AppImage and Flatpak publication into `release.yml` after `host` creates the release. Both jobs inherit the existing `contents: write` permission, and native build/upload failure fails the release verdict. Remove the independent tag race from `packaging.yml`.
2. Add `Cargo.lock` to packaging drift triggers, make heavy jobs depend on `lint-packaging`, and include `packaging.yml` in `release-ci.sh` verification with exact-run matching.
3. Invoke the versioned appimagetool explicitly for the pack step instead of linuxdeploy's embedded tool. Declare linuxdeploy/runtime rolling honestly; do not claim a fully pinned toolchain. Stage PNG + `.DirIcon`, assert them, and fail on appimagetool `::error::` diagnostics.
4. Update the Flatpak human recipe to 26.08 including the rust-stable extension. Correct SECURITY release availability and contributor/CI trigger/platform prose to actual policy.

## Constraints

- No workflow dispatch, tag, release mutation, upstream PR, centralhost, or Flatpak source/lock redesign in this kill.
- Preserve existing artifact checks, Vulkan-loader exclusion, package metadata, and desktop/Flatpak build behavior.

## Verification

- Parse both workflows and inspect job dependencies/permissions with a structural Python check.
- Shell-parse release scripts, run packaging static checks, run `git diff --check`, and obtain independent review before commit. Actual CI evidence is required after push; no release is claimed locally.
