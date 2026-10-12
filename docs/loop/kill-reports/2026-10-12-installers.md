# Kill report — umber-app installers proven (worklist #9)

Date: 2026-10-12
Author: dragon (the opencode claw scouted the config surface then died on the
sandbox's external_directory wall; the dragon took the kill)

## What was proven

`cargo dist plan` (cargo-dist 0.32.0, freshly installed and version-locked)
accepts `dist-workspace.toml` and produces a COMPLETE, coherent plan:

- `umber-app` 0.1.0: shell installer (`umber-app-installer.sh`), powershell
  installer (`umber-app-installer.ps1`), `sha256.sum`, both platform archives
  (linux-gnu `.tar.xz` carrying `[bin] umber` + CHANGELOG/LICENSE/README,
  windows-msvc `.zip` carrying `[bin] umber.exe`), per-artifact `.sha256`
  checksum files.
- `umber-cli` 0.1.0: the same shape (`umber-cli-*`).
- Both targets from the config (x86_64-unknown-linux-gnu,
  x86_64-pc-windows-msvc) resolve; the installers = shell + powershell per
  the config.

The plan is the machine-checked proof that the dist config CAN produce every
claimed installer type — the C1 audit's "config exists, installers unproven"
split mark had exactly this question open.

## What was produced locally

`cargo dist build --target x86_64-unknown-linux-gnu` — DIST_BUILD_EXIT=0,
8m21s in the new `dist` profile (LTO thin, codegen-units 1, inherits
release). Real artifacts under `target/distrib/`:

- `umber-app-x86_64-unknown-linux-gnu.tar.xz` (11.3 MB; binary + README +
  CHANGELOG + LICENSE) + `.sha256` — VERIFIED (`sha256sum -c`: OK)
- `umber-cli-x86_64-unknown-linux-gnu.tar.xz` (3.0 MB) + `.sha256` — VERIFIED
- `source.tar.gz` (4.3 MB) + `.sha256` — VERIFIED
- `umber-app-installer.sh` / `umber-cli-installer.sh` (49 KB each) —
  `bash -n` SYNTAX OK on both; content references the real artifact names
  (9 umber-app mentions)
- `sha256.sum` (the umbrella checksum manifest)

## The config fix this kill surfaced

cargo-dist 0.32 defaults to the `dist` build profile; dist-workspace.toml
never defined one and Cargo.toml had no `[profile.dist]` — the first real
build died on `error: profile 'dist' is not defined`. Fixed in the root
Cargo.toml: `[profile.dist] inherits = "release"` (the config's intent —
release-grade artifacts). Verified: TOML parses, cargo metadata resolves,
dist plan accepts (ad-hoc probe, 3/3 PASS).

## Honest unproven boundary

- The windows-msvc `.zip` + powershell installer can only be PRODUCED on
  CI's windows runner (no cross toolchain locally; the build's WARN lines
  say exactly this). The plan proves the config; CI produces the zip on
  tag.
- The installers' own curl-of-release fetches require a PUBLISHED GitHub
  release to smoke-test end-to-end — that is release.yml's job on the
  first version tag; the local scripts' syntax + artifact references are
  what can be proven pre-tag.
- MSI + Flatpak + AppImage: dist-workspace.toml configures shell +
  powershell installers ONLY. The requirements row's "MSI + Flatpak +
  AppImage" list was config-ambitious; the honest statement is
  shell/powershell per the config as shipped. Flatpak/AppImage would be
  NEW config work, not unproven existing config.

## Config fixes

None needed — 0.32.0 accepted the config as-is (the brief suspected schema
drift; there is none).

## Report

- opencode claw: session ses_eceaecc00ffeWsUEyHU9dFRH9O, 9 steps, 18 tool
  uses, died on the external_directory permission wall (scratch-dir
  download auto-rejected). Its config-surface map (release.yml anatomy,
  artifact naming, the 18e8c46 dist commit history) fed this kill.
- The scratch-dir wall is the THIRD sandbox-permission class today
  (claude×2 on kill-locks.sh, opencode on scratch) — the dispatch briefs
  must either pre-stage downloads inside the repo or state the wall as
  expected. Recorded in the delegation skill at the next pass.
