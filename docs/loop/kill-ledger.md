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
| (none — the tree is clean at `3f1f307`; the runway's first dispatch opens the next fleet) | | | | | |

## Waiting (blocked / scheduled)

| Item | Blocked on |
|---|---|
| packaging re-dispatch = the flatpak chain's verdict gate — CONSUMED (10-24, M1 pass 4) | answered: the five-wall chain went GREEN — verdict run 40040567126 on `3f1f307`, Umber.flatpak (12,635,392 bytes) uploaded as the artifact; the row stays as the audit trail |
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

## Landed 2026-10-23 — the segv hunt (post-close first kill)

| Kill | Commit | Notes |
|---|---|---|
| ci-ubuntu-image-pin | `c697bab` | the pushed tips' ubuntu-leg SIGSEGV root-caused to the RUNNER IMAGE ROTATION: ubuntu-latest moved 24.04→26.04 on 2026-10-21 (mesa 25.2.8→26.0.8 lavapipe, noble→resolute repos), zero code change, and the crash site WANDERS (brush_panel in 39903281308 vs bakes_panel in 39908194347 — the broken-runtime shape, no code-side crash to name); the full local lavapipe suite passes 635/635, 0 ignored, on the tower's own 25.2.8-family stack (attempt 3's detached-unit repro: supervisor umber-segv-repro + inner umber-segv-tryN with 8G/2G shields at launch, nice 19 -j1, after attempts 1+2 died mid-compile to oomd on the default-j recipe); fix = the check matrix's linux leg pinned ubuntu-24.04 (the green runs' exact image), gates/changes jobs left on latest (green on 26.04), windows untouched, perf-nightly's drift-gate red named as plausibly the same rotation (that hunt's opening hypothesis, its call pin vs re-baseline); the pinned-image CI verdict rides the W-C3b close's push (Unverified until then, named in the report); this row appended worktree-side per the single-owner rule |

## Landed 2026-10-23 — the W-C3a fleet (LOCAL close, eleven rows pushed e610ed7..96edd1e)

| Kill | Commit | Notes |
|---|---|---|
| channel-export-resample (A1) | `8f85074` | both painted readbacks fit to the export size through material_layer::resample_image_rgba8; fit_channel_to_export the pure decision; the painted_at_size decline re-pinned as the new contract, named in the report; battery-green this recovery |
| idmask-tile-cache (P1) | `7ade11c` | the multi-tile bake (filter_mesh_for_tile per present tile, the click UV rebased by the tile offset) + the single-entry content-keyed CachedIdBake; the refusal-first contract untouched on error paths; battery-green this recovery |
| picker-sliders-recents (G1) | `21ada34` | HSV/RGB slider rows + the recents/swatches popup; 8 tests incl. kittest + AccessKit roles; the battery's sRGB-endpoint ULP red fixed production-side mid-close (linear_to_srgb boundary-exact top, f32-port verified, manifest+report re-pinned) |
| winink-pen-flags (S1) | `8d7bf8a` | the platform-free PenFlagTracker (Eraser on INVERTED|ERASER per the XI2 precedent, barrel edge detection, no repeat on steady flags); the Windows-only winink delta's sole compile gate = Windows CI |
| snapshot-dir (C1) | `db63487` | paint_snapshot takes exactly one of out | out_dir (typed refusal before GPU work); the harness-script judgment's deferral closed |
| graph-history (wave-procedural-history k1) | `2a84fb8` | per-node procedural history, journal-integrated; the cross-review BLOCKER (AddNode redo id) found + fixed pre-battery; 13 tests; the unified timeline = kill 2, named |
| b10-display-chain-eyedropper (ADOPTED) | `8e33c8f` | the display-chain-aware pick adopted per the full rules (holder exit confirmed, fresh lock, static gates re-derived, the two test pins python-ported); its kittest GPU-pick fixture = the named follow-up |
| a2-snapshot-breadth | `42cbc7c` | four kittests (History empty + ops, toolbar via IconAtlas::empty, rich tooltip) + four baselines generated under the flock, the three existing baselines byte-verified, vision-checked pre-battery |
| a3-picker-app-sites | `5b38c96` | the brush-preset + fill-layer colour rows on the shared preferences::picker_button; the swatches codec (garbage skipped, cap-16); both catalogs' picker.* group python-pair-checked |
| wave docs + plan-of-record + ledger | `5781600` | the close tooling commit: scripts/w-c3a-close.sh + its report + docs/loop/next-wave-plan-draft.md + the ledger's accumulated rows |
| the local port | `96edd1e` | scripts/w-c3a-local-close.sh + its report — the host seams replaced by local steps under the slow-throttle envelope; the close ran through it after nine fires, two reboots, five oomd scope kills |

Fleet-level: battery GREEN 1,304 tests / 6 crates locally under the slow envelope (-j3, test-threads 2/1, nice 19, one cargo under the flock, the 8G/2G scope shield — fire 8's proven recipe; the one-stack rule + serialized heavy jobs are the day's standing lesson after two reboots). CI 39908194347 on the pushed tip: windows + change classifier green; the ubuntu leg red = the runner-image SIGSEGV (root-caused + pinned `c697bab`, the sibling hunt section above — its verdict rides the W-C3b push); the layering gate red = the stale scorecard (the fleet crossed the README's 2,200-test floor; regenerated in the 10-23 docs pass). The flatpak dispatch 39908210580 red at the runner's bwrap/rofiles layer — pinned the same day (below). The docs-tail push aac1c4a..e610ed7 landed first, 02:00, on Bessa's explicit order. The tail also carried the earlier landed riders (release-checklist-truth `c45b32f`, the W-C4 research plan `4bb8ca8`, the tangent-TBN static prep `e50bbe9`) — their In-flight rows retired at this docs pass.

## Landed 2026-10-23 — the post-close reds (the W-C3b push riders)

| Kill | Commit | Notes |
|---|---|---|
| flatpak-runner-pin | `f62d112` | the packaging workflow's Flatpak job pinned ubuntu-latest → ubuntu-24.04: the 10-19 dispatch (24.04) got PAST the bwrap/rofiles sandbox and failed on content (the vendor-checksums bug, fixed `4d69808`); the 10-23 dispatch (26.04, zero workflow/manifest delta between them) dies at sandbox setup before the command runs — the same 10-21 ubuntu-latest rotation as the ci SIGSEGV; the W-C3b push's post-push re-dispatch on the pinned tip is this kill's verdict gate; report docs/loop/kill-reports/2026-10-23-flatpak-runner-pin.md |
| scorecard regen (the layering gate's red) | `f4eff95` | the W-C3a fleet crossed the README's test floor (2,244 exact at the post-close tree) — scripts/scorecard.py regenerated the span 2,100+ → 2,200+; the W-C3b local port re-verifies --check post-rows so a battery fix-forward can never push a stale count again; check-layers.py re-derived GREEN on the clean 96edd1e tip (the red was the scorecard step alone) |

## Landed 2026-10-23 — the W-C3b sweep through the local port (origin `089b1a4`..`07bcd2b`, the wave's Gate 2 green)

| Kill | Commit | Notes |
|---|---|---|
| usdz-sublayer-walk | `d36cfb1` | load_usdz walks the sublayer stack — root precedence, authored order, depth-first, resolved inside the package; missing/unreadable sublayer skipped per USD layer-registry tolerance; cycle-guarded + depth-bounded (16); escaping paths rejected lexically; found-but-malformed Mesh = hard error; 8 pinned tests + the python port 12/12 |
| blend-replace-opacity | `b5a60b1` | Replace bypasses layer opacity AND source-alpha grading at every coverage gate (a present texel, any alpha > 0, overwrites in full; only fully-transparent is a no-op); composite_channel returns unmixed for Replace; the panel's channel-opacity greys for Replace rows; the red-risk sweep over every surviving Replace pin; the python port 4/4 |
| solo-height | `230d730` | solo_proxy's HEIGHT arm samples binding 12 through the same addressing chain + PAINT_HAS_HEIGHT_TEXTURE gate as height_perturbed_normal (solo and Lit can never disagree which texel a pixel reads); the mid-gray stand-in stays only for a mesh with no height channel; zero renderer changes; the GPU golden skip-graceful |
| tangent-frame | `d7eea57` | per-vertex mikkTSpace tangent frames — corners weld by bit-exact (position, normal, uv) tuples (the UV-seam rule), per-triangle Lengyel UV-derivative accumulation, Gram-Schmidt against the group normal, handedness-encoded B; untrusted-input-safe (skips, % 3 remainder, zero-area contributes nothing, least-dominant-axis fallback); 7 unit tests + the python port 7/7; the renderer wiring = the briefed next kill |
| swatches unique_dir race | `f9a4f7a` | the helper's line!() inside its own body was caller-invariant — all four swatches tests shared one tempdir; uniqueness per call (atomic counter + pid); proven 634/1 red → 635/635 at full concurrency |
| gpu golden import fix | `07bcd2b` | the sweep's SoloChannel enum unimported in the gpu-feature golden module (E0433, run 40017411324's --features gpu leg — the close battery runs default features only); verified locally 212 passed/0 failed at the exact CI shape; fmt clean |
| the local port + close tooling | `82975df` + `089b1a4` | scripts/w-c3b-local-close.sh + the host original w-c3b-close.sh: the W-C3B claim interlock, the W-C3A-must-be-CLOSED gate, four 16-hex manifests full-verified, the tree-state sentinel, dcommit --path-limited rows, ONE push + workflow-filtered run-id ci-watch, the post-rows scorecard gate, the post-push packaging re-dispatch |

Fleet-level: battery GREEN through the local port (layers + fmt + clippy 4 crates + suites: umber-mesh 145, umber-core 126, umber-gpu 151, umber-app 850 = 1,272 tests; the two mid-close fix-forwards — tangents.rs iterator-enumerate + compositor.rs's Replace-alpha exception — solo-verified 113/113 + 151/151 before the green fire; log /tmp/w-c3b-local-battery-r1.log). The close's push carried ALL prior unpushed rows forward (c697bab the ci runner pin, f62d112 the flatpak runner pin, f4eff95 the W-C3a docs pass, 82975df the port, f9a4f7a the swatches fix) — tip `089b1a4`, CI 40017411324: the pinned ubuntu-24.04 leg ran clean (the segv pin held) but the gpu-feature leg red E0433 → the wave's real Gate 2 = run 40020097585 on the fix tip `07bcd2b`, GREEN.

## Landed 2026-10-23 — the flatpak five-wall chain (the packaging lane GREEN end to end, verdict run 40040567126)

| Kill | Commit | Notes |
|---|---|---|
| flatpak-runner-pin | `f62d112` | the Flatpak job pinned ubuntu-latest→ubuntu-24.04: the 10-23 dispatch (39908210580 on `96edd1e`) died at bwrap/rofiles sandbox setup on the 10-21 ubuntu-26.04 rotation before any command ran (the same rotation class as the ci pin `c697bab`); the 10-19 dispatch on 24.04 had passed the sandbox and failed on content — environment-caused; verdict gate = the chain's green close |
| wall 1 — vendor parser | `01f69a6` | vendor-checksums.py parses the semver SHAPE not a dash position (cranelift-assembler-x64-0.137.0-rc.1 died unparseable in run 40017442313 — the close's own re-dispatch); trailing-semver regex with optional prerelease/build segments; the exact CI-failing dir name + 5 historical shapes parse identically |
| wall 2 — runtime 26.08 | `e3ae21d` | the flatpak runtime bumped 24.08→26.08 (manifest + .in template): the deliberate wasmtime 50.0.0-rc.1 pin pulls cranelift 0.137.0-rc.1 whose rust-version demands rustc 1.97, and 24.08's rust-stable ships 1.89 (red dispatch 40023752815 on `01f69a6` named the exact packages); 26.08 carries the 1.97-era toolchain |
| wall 3 — install trio | `e76be0c` | the flatpak job installs the 26.08 platform/SDK trio — the install step was version-coupled to the manifest's runtime (red dispatch 40025359694 on `e3ae21d`: 'org.freedesktop.Sdk/x86_64/26.08 not installed'); install refs move with the runtime-version, named in the step title |
| wall 4 — rofiles hardening (attribution corrected) | `3824b41` + `334af5b` | the compose died E: file-read-error reading the hicolor icon through rofiles-fuse (red dispatch 40026754260 on `e76be0c`); fix --disable-rofiles-fuse (flatpak-builder's own documented workaround, builder-main.c:103); FALSIFIED as the compose fix by the no-rofiles verdict run 40035240398 on `334af5b` (identical death) — the flag STAYS as orthogonal runner hardening, its compose-fix claim withdrawn, the report superseded in place; dcommit index-mode lesson: `3824b41` was docs-only, the bytes rode `334af5b` |
| wall 5 — svg pixbuf loader | `3f1f307` | the compose's real mechanism, proven with the runner's exact loader state: asc-image loads the hicolor SVG through gdk_pixbuf_new_from_stream_at_scale (asc-image.c:457-473); the svg loader ships in librsvg2-common, recommends-only, never installed by --no-install-recommends; reproduced on the tower by regenerating the loaders cache without libpixbufloader-svg.so; fix = apt-install librsvg2-common in the job; VERDICT run 40040567126 on `3f1f307`: SUCCESS, all three jobs green — the compose completes, Umber.flatpak (12,635,392 bytes) uploads as 'Umber-flatpak-bundle' — the lane's first real bundle |

Chain-level: five red dispatches (40017442313 → 40023752815 → 40025359694 → 40026754260 → 40035240398) each named its wall from the run object; ci GREEN at every chain tip (40020097585, 40023735402, 40025342767, 40026739440, 40034973269, 40035173125, 40040549515) — the c697bab ubuntu-24.04 pin's verdict green, the segv gone. The Waiting row's verdict gate is consumed by 40040567126 (ticked above). The scorecard --check gate held through every chain push.
