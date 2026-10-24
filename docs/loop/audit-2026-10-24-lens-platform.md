# Umber audit — lens F: platform, distribution, release, security (2026-10-24)

Author: dragon, solo. Every claim cited against the live tree.

## The workflow matrix (.github/workflows/)

| Workflow | Trigger | Platforms | State |
|---|---|---|---|
| ci.yml | push + PRs | Linux + Windows (the platforms job gates the Windows leg on platform-sensitive paths — 908135e; docs-only runs the light path) | green as of the 10-24 tip runs (all four W-C4 kills' CIs green) |
| perf-nightly.yml | schedule (08:1x-class) | Linux on the PINNED 24.04 image (ec7ccb8) | **GREEN 10-24** (run 40075780768, 19m20s; the 10-23 red was the rotating-image class the pin killed) |
| packaging.yml | tags + releases | Linux | the flatpak chain landed with the honest appstream-compose revert (d5c2a63/4f0c482): the 26.08 platform/SDK trio installs, --disable-rofiles-fuse, librsvg2-common; compose OFF |
| release.yml | tags | — | awaits the first tag |

## The packaging state

Flatpak: the wall-4 arc closed (the platform trio, the fuse
disable, librsvg, the compose revert). AppImage + the shell+PS
path proven. What a user can download today: the CI's build
artifacts + the published images per release; no release tag has
been cut yet.

## The release critical path (0.1.0)

From docs/specs/release-checklist-0.1.0.md:

- **MET**: the perf re-baseline (the nightly green on the pinned
  image); the two-gate wave discipline; the scorecard --check.
- **MECHANICALLY OPEN**: the SPEC v1.1 ratification packet's
  contents; the `version = "0.1.0-alpha.1"` workspace bump; the
  pre-release dry-run tag (checklist gate 9: "no tag has" been
  cut — today's tree releases nothing without the bump + tag).
- **PARTNER-GATED**: the SPEC ratification itself, the release
  decisions (C2-16, 22, 23-25, 30), the checklist walkthrough,
  the upstream bug drafts (docs/upstream/ wait on his go-ahead).

**The honest critical path to a tag**: bump the workspace
version, cut `v0.1.0-alpha.1`, walk the checklist in a desktop
session (the four unfired smokes), ratify the SPEC. Two of the
four are his calls; two are a day of mechanical work.

## The security posture

SECURITY.md: 12 rows, zero PENDING (re-read this audit). The
WASM consent gate (blake3 before load, plugins.rs:22); the
importer memory caps; the spp crate's deny(unwrap/expect/panic);
the fuzz corpora (mesh panic hunt 24/24 + the mutated-valid
replays); the crash-reporter local-first doctrine (no submission
by design). Dependency pins: wgpu/eframe/openusd 0.8/ufbx pinned
in Cargo.toml (the advisory surface is named there; no live
advisory check in a read-only pass).

## The .ftl LOCK STATE (the named kill's gate)

**LIFTED.** The freeze-3 recovery landed (the W-C3a/C3b closes +
FREEZE-4); post-recovery commits add i18n keys again — 2a84fb8
(the procedural history panel, 10-23) added 12 keys to each
catalog. The L5(b) OccluderScope panel row is UNGATED on the
catalog side; only its own implementation remains.

## The i18n + a11y release surface

- en-US.ftl (953 lines, the source of truth) + pt-BR.ftl (972):
  the Fluent attribute scheme, the completeness test enforcing
  key parity, per-key fallback. More catalogs = content work
  (census F5).
- The a11y state: the AccessKit sweep landed (g7, d9fa931);
  keyboard-only journeys (g7's open row) + the desktop smokes
  unfired = the trust surface before release.

## The named gaps

1. No tag ever cut — the release lane waits on the version bump
   + his ratifications.
2. The four desktop smokes unfired (lens B's finding).
3. Keyboard-only journeys (g7's open half).
4. The plugin distribution story (C4).

LENS-F-DONE
