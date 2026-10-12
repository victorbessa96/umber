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

`cargo dist build` was NOT run this pass: it compiles the whole workspace in
release mode, and a second claw (heightnormal) is writing renderer/shader
files in the same tree RIGHT NOW. One cargo at a time; the build is the next
step once that kill lands.

## Honest unproven boundary

- The windows-msvc artifacts can only be PRODUCED on CI's windows runner (no
  cross toolchain here); the plan proves the config, CI produces the zip.
- The installers' own curl-of-release fetches require a published GitHub
  release to smoke-test end-to-end — that is release.yml's job on tag, and
  the smoke test runs when the first tag lands.
- What CAN be locally proven and is still owed: `cargo dist build` on
  linux-gnu (real `.tar.xz` + installer.sh), `bash -n` on the installer
  script, checksum verification of the produced archive against its
  `.sha256`.

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
