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

## Landed 2026-10-19 — the stencil chain + the claw day (wave 1 of the day PUSHED, origin aac1c4a)

| Kill | Commit | Notes |
|---|---|---|
| flatpak archive-type | 5e7f4dd | tar-gzip on every source; verified LIVE by the 39306564270 dispatch (621 crates extracted past the old wall) |
| stencil paint-hook | 3a8b4cb | set_stencil_gate + the RAW-byte fold in gate_plane (Amendment A) |
| stencil app-wiring | 6c95e26 | hook_gate hands RAW bytes; panel threading; i18n |
| stencil viewport-shell + N | 2bb8c60 | overlay, S chords, N edit, Overlays popover |
| stencil tile-restage | dcc5e89 | CLAW (deepseek): set-switch restage at paint.set_active_set |
| stencil tile-observer | 45ad4e4 | CLAW: staged_for on adopted hand-off only; dims-only per-frame compare |
| fleet-drift verdict + reseed | c53a08d | code exonerated (infra ×0.5); baseline.json reseeded; nightly GREEN vs it |
| queue sweeps (C2-7/C2-11/B-14/B-17, C2-8) | f0ae70d, f2c87d4 | stale-row ticks, grep-verified at landing sites |
| docs audit (secrets verdict: CLEAN) | 907c393 | stale per-layer-pool status; dup + superseded files removed; registries cross-referenced |
| stencil soft-edges | 35ad8bd | CLAW (deepseek): Amendment B — rounded sample, no threshold; mutation rung |
| flatpak strip-components 0 | 4d69808 | the .crate top dir kept; vendor/ shape restored (sibling, reproduced+proven) |
| packaging error-hint | c7b2991 | split_dirname carries the full strip diagnostic (the deferred claude item) |
| stencil-core port annotation | 4df239e | binary-era port flagged byte-intact; soft-shape pointer |

History for 2026-10-13..10-18 lives in CHANGELOG.md (## sections per arc)
and docs/loop/kill-reports/ (per-kill evidence); the ledger's
"recently-landed" scope resumes from the table above.

## In flight

| Kill | Wave | Claw | Branch/Worktree | Status | Next |
|---|---|---|---|---|---|
| b10-display-chain-eyedropper | 6 | opencode/deepseek | master (lock held) | claw mid-coding (viewport.rs, main.rs) | census + battery + commit |
| channel-export-resample | 6 | opencode/deepseek | master (lock held) | claw mid-coding (bake_sources.rs, export_dialog.rs) | census + battery + commit |
| idmask-tile-cache | 6 | — | master (lock held) | claimed, not yet dispatched | brief + dispatch |
| picker-sliders-recents | 6 | — | master (lock held) | claimed, not yet dispatched | brief + dispatch |
| release-checklist-truth | 6 | — | master (lock held) | claimed, not yet dispatched | truth pass vs the tree |
| winink-pen-flags | 6 | — | master (lock held) | claimed, not yet dispatched | brief + dispatch |
| next-wave plan | 6 | claude (planner) | docs/loop/next-wave-plan-draft.md | draft landed | sibling's fan-out review |

## Waiting (blocked / scheduled)

| Item | Blocked on |
|---|---|
| the 7-commit wave push (strip fix, audit, sweeps, soft edges, hint, annotation) | the sibling's close motion (B-10 + fan-out) |
| packaging re-dispatch = the flatpak chain's verdict gate | the wave push (PUSH FIRST — the manifest clones master at build time) |
| .spp import decision | Bessa's ratification (legal scope: import-only, no writing, no spec derivation) |
| centralhost memtest86+ (UECC bank 14) | Bessa's next physical access; boot straight into memtest, skip OS |
| rasdaemon on centralhost | not packaged/enabled on the distro; journal MCE grep is the interim decode |

## Landed 2026-10-12 — wave 1

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

