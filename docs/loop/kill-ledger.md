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
| b10-display-chain-eyedropper | 6 | opencode/deepseek | master (lock held: b10-eyedropper-adoption) | claw done + ADOPTED 10-20 by local-10e6ed18 (holder exit confirmed, static gates re-derived, the two test pins python-ported [137,188,229,173]->[158,185,204,173] + identity passthrough confirmed; manifest-pinned) | host battery + the close script's row 7 |
| channel-export-resample | 6 | opencode/deepseek | master (lock held) | claw DONE + report; dragon census pass 1 (fit fn verified, the hand-derived bilinear bytes re-derived independently); PARKED — freeze 4 | host battery + commit |
| idmask-tile-cache | 6 | opencode/deepseek | master (lock held) | claw DONE + report; census pass 1 (CachedIdBake + rebase_uv_for_tile + refusal-first docs); PARKED | host battery + commit |
| picker-sliders-recents | 6 | opencode/deepseek | master (lock held) | claw DONE + report (8 tests incl. kittest + AccessKit roles); PARKED | host battery + commit |
| release-checklist-truth | 6 | dragon | LANDED `c45b32f` | six stale claims corrected, every one tree-verified; doc-only | rides the W-C3a push |
| winink-pen-flags | 6 | opencode/deepseek | master (lock held) | claw DONE + report (7 tests; the winink delta compiles on Windows CI only); PARKED | host battery (stylus, default features) + commit |
| graph-history (wave-procedural-history k1) | 9 | dragon + nemotron cross-review | master (lock held) | authored, cross-reviewed (BLOCKER: AddNode redo id — found + fixed pre-battery), fmt-clean, 13 tests; PARKED | host battery (umber-app) + commit; kill 2 = the unified timeline |
| next-wave plan | 6 | claude (planner) | docs/loop/next-wave-plan-draft.md | draft landed; the fleet is NINE-coded (A2 + A3 coded 10-20 host-free under the code steer; the eyedropper adopted) — the plan's lanes are all built or parked | host battery + the close script's ten rows |
| a2-snapshot-breadth | 6 | dragon | master (lock held) | CODED 10-20: four kittests (History empty — the harness doc's v1 claim made true — + History after two journalled Adds, toolbar via IconAtlas::empty, rich tooltip fixture); baselines host-generated in the close script's step 4a (exit-5 vision-check gate); fmt-clean, verify-kill clean; PARKED | host battery + row 8 |
| a3-picker-app-sites | 6 | dragon | master (lock held) | CODED 10-20: the three colour rows -> preferences::picker_button (G1's popup over session-shared recents, egui IdTypeMap get_temp/insert_temp); the swatches codec (plain #rrggbb, garbage skipped, cap-16) + 4 codec tests; both catalogs' shared picker.* group (python pair-checked); graph-history's manifest re-pinned for the additive keys; the stock color_button keeps its kittest consumers (the delegate conversion = named follow-up); fmt-clean, verify-kill clean; PARKED | host battery + row 9 |
| w-c4-research-plan | 6 | dragon | LANDED 4bb8ca8 | the §5 tail mapped into lanes L1-L7 (set-switch glue, dab density, IdMask pre-warm, the winink hook, masked bakes, the unified timeline, G1's delegate) with exact seams + lock status + dispatch order; the §5 item 2 API-UNVERIFIED clause CLOSED by two vendored artifacts (eframe 0.36.2 RWH surface + windows 0.62.2 subclass surface, line-cited from the vendored registry bytes); the plan stays DRAFT until W-C4 opens at HOST_BACK | rides the recovery push |

## Waiting (blocked / scheduled)

| Item | Blocked on |
|---|---|
| the doc/tooling tail's push (everything after origin `aac1c4a` — 17 commits through the dcommit pre-commit-hook a178913; count ticked in the 2026-10-20 M1 pass-2 sweep) | centralhost HOST_BACK — freeze-4; order + scopes: `.dragon/runbooks/host-back-recovery.md` (the recovery session's ONE push after the parked fleet's batteries) |
| the parked fleet's batteries + commits (A1/P1/G1/S1 + graph-history + snapshot-dir; the eyedropper stays its owner's) | centralhost HOST_BACK — freeze-4; per-kill battery scopes: `.dragon/runbooks/host-back-recovery.md` fleet table (needs Bessa's power-cycle) |
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


## Landed 2026-10-20 — meta-lane M3 (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| dragon-commit-wrapper (M3) | 0cc7030 | dcommit: fail-closed commit gate (.dragon/dcommit.py) — staged set must sit inside declared --lock paths (19:3x interlock) + --amend needs ledger proof HEAD belongs to the calling session (14:5x interlock); 36-leg scratch battery ALL GREEN; dogfood-committed via itself (first ledger row); this row appended worktree-side (file carries sibling's uncommitted hunks — single-owner commit rule, rides the next ledger commit) |


## Landed 2026-10-20 — meta-lane M4 (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| coordination STATE header (M4) | 9c8b30f + 86fe4a4 | auto-maintained block at the top of .dragon/COORDINATION-next-wave.md (file stays untracked by convention): host/envelope, live locks, parked fleet, open lanes, partner-gated, recent events, watchers; sessions rewrite the block on every claim/land, body append-only; kills the tail-probe miss class (the 4d69808 incident). Claim-pairing census; drift-hardening correctly NOT open (dc4b355 landed — body prose was stale, tree wins). Integrity: pre-M4 body byte-identical contiguous substring at :1824, asserting check (86fe4a4 records the mis-designed first check + the toothless &&-guard miss). Report committed via dcommit (M3 follow-up adopted, dogfooded twice); this row worktree-side per the single-owner rule |

## Landed 2026-10-20 — meta-lane M1 (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| stale-row sweep (M1, first standing pass) | 8cd3bf4 | 10 ticks (queue rows 2/16/23, the header's FROZEN line, the push-log row, the Scratch rewrite; STATE.md status line + Next + Blocked/waiting incl. the missing FREEZE-4 line); HEADLINE: the stencil+perf wave WAS PUSHED 10-19 15:54 (origin aac1c4a — CI 39306460278 green both platforms, hardened nightlies green ×2 = the drift gate's live proof; job lists cited per M2); held-current markers named in the report; the recurring-lane protocol encoded there; Waiting-row blocked-on corrected to HOST_BACK worktree-side; this row rides the next ledger commit per the single-owner rule |

## Landed 2026-10-20 — meta-lane M5 (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| queue-row runbooks (M5) | `ea83093` | hunt next_step rows point at runbook files, never embed policy prose: `.dragon/runbooks/host-back-recovery.md` (the recovery order + the fleet's per-kill battery scopes, extracted from STATE.md Next + these Waiting rows) + `docs/loop/desktop-checks.md` (the W-C3a D1 desktop-check pack — the C2-1/C2-3/C2-9 smokes instrumented, labels grep-verified, fixture verified live, Result lines open); STATE.md Next + the queue rows converted to pointers; row 15 ticked with committed-tree evidence (the --script arm = protocol form per the harness-script judgment 1; M1's flag-literal grep named); C2-12 kept true on the committed tree; this row appended worktree-side per the single-owner rule |

## Landed 2026-10-20 — the W-C3a close tooling (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| w-c3a-close-script (M3 recovery-runbook-adoption tail) | `17f56a8` | scripts/w-c3a-close.sh: the one-command recovery — claim interlock `^W-C3A RECOVERY` anchored at line start (unit-tested 4/4 on the note's own bytes), six 16-hex manifests pinned from the kill reports (29 pins, 0 drift at pin time), the tree-state sentinel verdict-reuse (HEAD + porcelain + dirty BYTES), commit rows through dcommit --path-limited (ledger-attributed; the eyedropper's viewport.rs/main.rs cannot ride — scratch-repo-proven), ONE push + explicit run-id ci-watch, the packaging re-dispatch only; both stencil-wave-close defects pre-fixed at design time; dry-run ×2 byte-identical; this row worktree-side per the single-owner rule |

## Landed 2026-10-20 — the commit-gate enforcement (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| dcommit-pre-commit-hook (M3 tail) | `a178913` | scripts/hooks/pre-commit: bare `git commit` on an installed clone refuses with the dcommit usage + the interlock rationale unless DCOMMIT_INVOCATION=1 is stamped (dcommit.py's one-line env stamp on its commit subprocess — py_compile clean, every refusal leg re-proven); `git commit --no-verify` the deliberate human override (the partner stays sovereign, an agent using it is visible); installed on the shared tree (.git/hooks/pre-commit, cmp-identical, per-clone scope); scratch battery 7/7 incl. both dcommit modes passing THROUGH the hook and both gate refusals still firing; the lane's own commit ran the hook live; this row worktree-side per the single-owner rule |

## Landed 2026-10-20 — the umber-wave-ops skill split (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| umber-wave-ops skill split | (profile skills dir — no repo commit) | SKILL.md 105,564 → 53,022 bytes (v1.6.3 → 1.7.0): Cargo discipline + Kill locks + Kill cycle extracted VERBATIM to references/{cargo-discipline,kill-locks,kill-cycle}.md (byte-identity assert-proven), the every-session rules + pointers left inline, the two census/gating lessons landed in Build-host outage (the patch the 100k cap refused that morning); the reference load path proven live via skill_view; 47k headroom — future skill lessons can land again; the topical-curation follow-up named (the 1.6.3 Kill-locks section had accreted kittest/egui lessons — extracted byte-faithfully, header-noted); this row worktree-side per the single-owner rule |

## Landed 2026-10-20 — meta-lane M5b (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| dragon-queue-row runbooks (M5b) | `6d05bf2` | the steer's original blob incident closed on the DRAGON side: two policy homes in .dragon/runbooks/ (hunt-umber-local-continuation.md — the envelope, lane menu, claim protocol; hunt-centralhost-freeze-watch.md — the probe, watcher rules, closing gate; untracked by convention, siblings of host-back-recovery.md) and both hunt rows (d0c76c4d + 1971c1fd) shrunk to pointer+state, readback-verified (runbook pointer in both goals; policy text gone from the rows). Lane forked clean from the main session's M5 after the 10:59 lock-catch (zero edits leaked; umber-internal = ea83093, dragon-queue = this). Standing rule: new recurring umber-territory hunts get a runbook at creation; queue rows never embed policy prose. Census + non-coverage (cb4b2026 cross-territory follow-up) in the report; this row appended worktree-side per the single-owner rule |


## Landed 2026-10-20 — meta-lane M1 pass 2 (freeze-4 host-free continuation)

| Kill | Commit | Notes |
|---|---|---|
| stale-row sweep pass 2 (M1) | `92ebf3d` | five ticks, each re-derived live before the edit: the doc/tooling tail count 12/13→17 (git rev-list aac1c4a..HEAD, endpoint a178913) at STATE.md's status line + queue.md's Scratch line + this ledger's Waiting row (worktree-side per the single-owner rule); the queue.md commit-gate row gains the ENFORCEMENT fact (scripts/hooks/pre-commit committed a178913 + installed, bare commits refuse, DCOMMIT_INVOCATION=1, the --no-verify partner override deliberate); B6's "CI verdict pending" cell stale since 10-12 → Gate 0 PASS run 38255484123 per the queue's own GATE 0 section (pass 1 missed the row — the recurring lane catching its own misses). Held current, verified live: desktop-checks Result lines ×4 empty, design-doc DRAFTs legitimate, the In-flight fleet rows PARKED, the Waiting blocked-ons all live. Claim-collision record + the 13:2x watcher-race side-record in the report; this row appended worktree-side per the single-owner rule |
