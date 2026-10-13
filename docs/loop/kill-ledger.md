# Umber Kill Ledger — the live state of every hunt

One row per in-flight or recently-landed kill. The queue (docs/loop/queue.md)
tracks worklist items; this ledger tracks EVERYTHING hunting: waves, follow-ups,
one-off kills. Updated at dispatch, at landing, at consumption.

Format: | kill | wave | claw | worktree/branch | status | next action |

## Wave 1 — LANDED (2026-10-12, the first big-wave cycle complete)

| Kill | Commit | Notes |
|---|---|---|
| wave-blends | d861482 | 17 modes, HSV + RNM families; 2 claw pins corrected at gate |
| wave-dist | 43180f4 | Flatpak + AppImage + packaging CI |
| wave-history | 638fb2b | click-to-time-travel, dual-journal depths |
| wave-masks | 770162f | per-layer mask planes + the compositor gate; claw died at report step, dragon wrote it from the diff |
| battery fixes | 1f28048 | merge typo, journal arms, mask-mode test premise, i18n keys |

The wave protocol's first full cycle: 4 claws in worktrees (one DB-lock
relaunch round), zero cargo mid-wave, ONE battery at the end (4 classes
caught: a merge typo, 2 E0004s, a wrong test premise, 6 missing i18n
keys), land-in-order with one hand-rebase + one conflict resolution,
332/332 green, CI per branch.

## In flight

| Kill | Wave | Claw | Branch/Worktree | Status | Next |
|---|---|---|---|---|---|
| perf-baseline (#2) | — | CI nightly | run 38304995746 | queued (runner starved since 10:10Z; 38234644651 cancelled after 10h) | poll; on completion seed baseline.json |
| wave-2 dispatch | 2 | — | — | WAITING on wave-1 CI green | write briefs fresh, stagger launches |

## Landed today (2026-10-12)

| Kill | Commit | Notes |
|---|---|---|
| eyedropper (#10) | 7067cd3 | Pick tool + viewport sampling + set_brush_color |
| undo merge (#5) | 37025d5 | merge-at-push; the byte budget carries regions |
| crash-safe saves (#3) | e4ad8b7 | atomic manifest commit + AssetStore routing |
| reopen resize fix | 7769b2c | assign-then-resize in the mapping branch |
| XI2 stylus (#7) | d2fd7b7 | Linux pen backend + StylusPoller wiring |
| installers (#9) | 9f875b3 | dist build proven, checksums verified, profile.dist fixed |
| height→normal (#12) | 861ca9a | Mikkelsen perturbation + the GPU-test CI leg |
| ID-mask (#11) | e2bed70 | click region → only that ID paints |
| OCIO bundled (#8) | 60b04d0 | the never-run C++ build ran; enum-name constants fixed |
| icon-raster | e737960 | lucide 1.55 atlas, 2 renames caught; successors landed cb4d2bf/0d392db |
| per-channel tile pool | dc5c2c9 | TileTargets + lazy routing + nested save; 3 claws, 4 gates |
| binding seam (c) | be11fe5 | height channel reaches the Lit path; NOT-REACHABLE dead |
| workflow batch | 79bba5d | claw-review findings: land-kill .md fix, paren fix, settings.json |
| kill-worktree.sh | 49eaf96 | per-kill worktrees, flock cargo, lifecycle verified |
| doc-deltas | cb6e168 | --doc-deltas + the leaf-vs-path guard fix |
| wave plan + ROADMAP | 069989b | five waves, 19/19 census cross-check |

## Waiting (blocked / scheduled)

| Item | Blocked on |
|---|---|
| wave-2 briefs | wave-1 landing (anchors shift) |
| .spp import decision | Bessa's ratification (legal scope: import-only, no writing, no spec derivation) |
| perf compare gate | the nightly's artifact |
