# Native release graph and packaging truth

- Lock: `razul-release-graph`.
- Brief: `d6fd13c58ddf` (`docs(ci): native release graph brief`).
- Outcome: tag-native AppImage and Flatpak publication now lives in the same release graph as cargo-dist, with native failure able to fail the release.

## Findings and fixes

1. **Release race and permissions.** AppImage and Flatpak jobs moved into `release.yml` after `host` creates the release. The existing top-level `contents: write` permission covers both uploads. `announce` requires `host`, AppImage, and Flatpak success. The independent tag trigger and direct uploads were removed from `packaging.yml`.
2. **Drift gates and dependencies.** `Cargo.lock` is in push and PR packaging triggers, heavy jobs depend on `lint-packaging`, and `release-ci.sh` requires exact-SHA application and packaging runs before allowing a tag. Tag-native Flatpak invokes the generator from the tagged SHA; lint fetches full history for ancestor pins.
3. **AppImage tool path and icon.** Development and release call the same reusable `appimage-build.yml`, eliminating duplicated procedure drift. linuxdeploy bundles dependencies without packing. The versioned appimagetool 1.9.1 binary packs explicitly under `pipefail`, checks its version, and fails on `::error::` diagnostics. PNG and `.DirIcon` are rendered and asserted. linuxdeploy and its type2 runtime are declared rolling instead of falsely presenting a full toolchain pin.
4. **Documentation truth.** The Flatpak recipe now installs 26.08 plus `org.freedesktop.Sdk.Extension.rust-stable`, and states pinned-source lockfile ownership without claiming working-tree drift detection. SECURITY marks the two hardening rows present in `v0.1.0-alpha.1`. CONTRIBUTING documents the required GPU test opt-in and CI comments describe actual release-gate triggers and platform-specific legs.

## Verification

- Parsed all three workflows with PyYAML and asserted permissions, `needs: host`, reusable AppImage callers, native announcement dependencies, lockfile triggers, lint dependencies, full-history lint checkout, and no independent packaging tag trigger.
- Structural checks confirmed the tagged generator invocation, explicit appimagetool call, `.DirIcon`, `pipefail`, error-diagnostic gate, checkout hygiene, and both release uploads; `--output appimage` is absent.
- Flatpak source/lock ownership tests: **5/5 passed**; real `gen-cargo-sources.py --check` passes.
- `bash -n scripts/release-ci.sh`: clean.
- `python3 flatpak/check-runtime-coupling.py`: all three 26.08 references coupled.
- `git diff --check`: clean.
- Independent review `deleg_2da8632a` found six issues, then two residual trigger issues; all were fixed. Fresh final review `deleg_fd741100`: **SHIP**, zero findings, with three generator mutations confirmed to fail the ownership tests.
- Commit/push/current-tip CI: not yet claimed. No workflow was dispatched and no release was mutated.

## Explicit open boundary

PLAT-02 source/lock ownership is fixed in the paired Flatpak kill: source and vendor closure now resolve from the same commit. Actual release CI and native build evidence remain post-push requirements; no tag or release mutation is claimed.

## Non-goals

No workflow dispatch, tag, release mutation, upstream PR, or centralhost work in this kill.
