# Lens E-lite — the crate health census (2026-10-24)

Read-only census per `.dragon/briefs/2026-10-24-audit-lens-e-lite.md`. No
cargo, no builds, no tree mutations; this file is the only write.

**Method.** Crate list from `[workspace] members` (Cargo.toml:3-20 — 16
members; `examples/` excluded, Cargo.toml:24). LOC per the brief's command
(`find crates/<name> -name "*.rs" | xargs wc -l | tail -1`), cross-checked
against a batching-proof `cat | wc -l` sum — all 16 matched, so no xargs
truncation. Test count = `grep -rn "#\[test\]" crates/<name>
--include="*.rs" | wc -l` (unit + integration). Roles quoted from each
crate's entry-file module docs. Tree state: HEAD `0ec64bd` (2026-10-24),
`git status --porcelain` = 10 lines, all untracked `?? .dragon/…`
orchestrator files — the census measures committed code.

## 1. Headline table

| Crate | Role (entry-file docs) | LOC | `#[test]` | Density (/kLOC) |
|---|---|---:|---:|---:|
| spp | Read-only `.spp` (HDF5-subset) importer; "import-only FOREVER" (crates/spp/src/lib.rs:1-6) | 6,581 | 48 | **7.29** |
| stylus | Owned stylus/tablet input: Windows Ink + Wintab / Wayland + XI2, platform-free pen-flag decode (crates/stylus/src/lib.rs:1-17) | 1,586 | 31 | 19.55 |
| umber-app | The eframe shell (binary, main.rs) + the paint-session library surface for integration tests (crates/umber-app/src/lib.rs:1-11) | 55,172 | 676 | 12.25 |
| umber-bake | Mesh-map bakers: AO, normal, curvature, position, thickness, ID, cage, dilation (crates/umber-bake/src/lib.rs:1-6) | 9,231 | 150 | 16.25 |
| umber-brush | Brush engine: inputs→curves→dab-params state machine, one-euro/lazy-mouse conditioning, dab batcher (crates/umber-brush/src/lib.rs:1-9) | 9,304 | 159 | 17.09 |
| umber-cli | Headless automation: inspect/bake/batch recipes + the agent JSONL session (crates/umber-cli/src/main.rs:1-11) | 7,930 | 79 | 9.96 |
| umber-color | Color management: OCIO v2 configs, display transforms (CPU reference + GPU LUT), per-channel CM flags (crates/umber-color/src/lib.rs:1-12) | 961 | 26 | 27.06 |
| umber-core | Document model: texture sets, layers, masks, undo journal, `.umber` I/O — no GPU, headless-testable (crates/umber-core/src/lib.rs:1-10) | 8,741 | 126 | 14.41 |
| umber-export | Template/preset export: naming tokens, packing, engine presets, PNG/EXR/TIFF/JPEG (crates/umber-export/src/lib.rs:1-11) | 4,931 | 84 | 17.04 |
| umber-gpu | The single wgpu device context and ALL GPU work submission; UI never touches wgpu directly (crates/umber-gpu/src/lib.rs:1-14) | 26,199 | 220 | 8.40 |
| umber-graph | Procedural node-graph engine: DAG, topo eval, typed eval, MaterialX I/O with declared custom nodedefs (crates/umber-graph/src/lib.rs:1-16) | 13,662 | 141 | 10.32 |
| umber-gui | Design-language presentation layer: theme tokens, fonts, icon atlas, foundational widgets; egui alone (crates/umber-gui/src/lib.rs:1-15) | 8,211 | 136 | 16.56 |
| umber-mesh | Mesh container + import (glTF, OBJ, FBX, USD ASCII + usdc), raycast, seams (crates/umber-mesh/src/lib.rs:1-5, 23) | 9,961 | 146 | 14.66 |
| umber-paint | Headless paint engine: per-layer planes, strokes/retouch, composite-on-read flush, persistence (crates/umber-paint/src/lib.rs:1-20) | 19,908 | 178 | 8.94 |
| umber-plugin-sdk | Guest SDK for wasm32 node plugins: no_std pure-function C-ABI + the AI-provider interface (crates/umber-plugin-sdk/src/lib.rs:1-8, 30) | 1,716 | 15 | 8.74 |
| umber-wasm | WASM plugin host: wasmtime fuel-metered runtime, wire layout, contract checks (crates/umber-wasm/src/lib.rs:1-15) | 2,076 | 36 | 17.34 |
| **Total (16)** | | **186,170** | **2,251** | **12.09** |

**Density floor check (brief item 3).** The floor is 7/kLOC (stated in the
brief; documented at docs/ROADMAP.md:22, "no crate below 7"). **No crate
is below it.** The floor-adjacent band (7–10/kLOC) holds five crates:
spp **7.29** (the closest approach, +4.1% over the floor), umber-gpu
8.40, umber-plugin-sdk 8.74, umber-paint 8.94, umber-cli 9.96. spp's
margin is arithmetic-thin: at its 48 tests, **+277 LOC of test-free
growth (6,858 LOC) breaches the floor** (48/6,858 × 1000 = 6.999).

## 2. Perf state (docs/perf/protocol.md, 461 lines; brief item 4)

The 60-line window carries the budgets and the instrumentation contract:
five §12 budgets (input→photon < 20 ms; 4K stroke @ 200 Hz zero dropped
dabs; viewport 60 fps P95 < 16.7 ms on 1M-tri + 4K; undo RAM 100 steps
@ 4K within ~2 GB; installer < 150 MB — docs/perf/protocol.md:11-17),
the `perf`-feature plan (tracy spans, HUD, nightly soak;
docs/perf/protocol.md:19-32), and the input-to-photon harness invocation
(docs/perf/protocol.md:39-42). The four asked-for items:

- **Baseline.** `perf/baseline.json`, seeded **2026-10-19** after the
  fleet-drift conviction: viewport_mean 85.3739 ms, viewport_p95
  93.43795 ms, input_photon_p50 0.1927 ms, input_photon_p95 10.1799 ms
  (perf/baseline.json:2-8; narrative at docs/perf/protocol.md:424-432).
  Baselines are per-hardware-class AND per-fleet-era
  (perf/baseline.json:4). A real-GPU record exists alongside it
  (2026-10-19, RTX 3070, tip dc4b355): viewport P95 0.2034 ms (~82×
  inside budget), input-to-photon P95 10.08 ms (half the budget), stroke
  soak 2914/2914 dabs, zero drops (docs/perf/protocol.md:445-456).
- **Nightly invocation.** `.github/workflows/perf-nightly.yml` —
  schedule cron `0 3 * * *` 03:00 UTC (perf-nightly.yml:17-18), hosted
  runner pinned `ubuntu-24.04` on 2026-10-24 (perf-nightly.yml:39),
  builds the harnesses (perf-nightly.yml:60-61) and runs
  `scripts/perf_gate.sh`: median-of-3 passes, plus a same-tip
  median-of-5 canary leg before any red may fail the job
  (perf-nightly.yml:8-12, 63-68). The two harness entry points:
  input-to-photon `cargo test -p umber-app --features perf --release
  input_photon` (docs/perf/protocol.md:39-42) and ref-scene
  `cargo test -p umber-gpu --features perf --release ref_scene`
  (docs/perf/protocol.md:74-77).
- **Compare gate.** `python3 scripts/perf_compare.py /tmp/ref-scene.json
  /tmp/i2p.json perf/baseline.json` (perf-nightly.yml:70-72): a >10%
  regression on any tracked metric fails the job, as does a missing
  tracked metric or a failing harness (perf-nightly.yml:5-7;
  docs/perf/protocol.md:28-32). Since dc4b355 (2026-10-19) no single
  sample may carry a REGRESSION verdict (perf-nightly.yml:8-12).
- **Latest triage record.** The 2026-10-19 hunt
  (docs/perf/protocol.md:227-436): the first clean gate read was RED
  (x1.83 viewport mean on 0955ac8, run 39263843229), confirmed x1.92
  (39268968173), then four bisect legs; leg 4 re-benched the baseline's
  OWN tip (5e0d89e) at 89.29 ms today vs 45.54/48.31 three days earlier
  — **"THE RUNNER FLEET IS CONVICTED, THE CODE IS INNOCENT"**
  (docs/perf/protocol.md:386-411): GitHub's ubuntu-24.04 fleet shifted
  CPU throughput ~×0.5 between 10-17 and 10-19 with a byte-identical
  image. The hunt closed **INFRA-DRIFT-ANSWERED** with the reseed
  (docs/perf/protocol.md:424-436). Doc-ordering nit: the reseed section
  calls the drift-hardening a "NAMED FOLLOW-UP"
  (docs/perf/protocol.md:432-435) while it landed the same day as
  dc4b355 — the later section already cites the landed
  `perf_gate.sh` (docs/perf/protocol.md:458-459).

## 3. SECURITY.md verdict (brief item 5)

- **Row count.** 40 lines total; the threat-surface table has **10 data
  rows** (SECURITY.md:26-35; header at 24-25) — matching the 10-17
  audit's "SECURITY.md current (10 rows, 0 stale)" (docs/ROADMAP.md:23).
- **PENDING claims: none.** `grep -n "PENDING" SECURITY.md` returns
  nothing across all 40 lines.
- **HEAD anchors: 3 rows name an old commit.** Rows 27, 28 and 31 read
  "**IN HEAD (ddb59e0, battery-verified 2026-10-16), not yet in a
  release:**". `ddb59e0` is dated 2026-10-16 and sits **144 commits
  behind HEAD** (`git rev-list --count ddb59e0..HEAD` = 144; HEAD =
  0ec64bd, 2026-10-24). The *substance* is still in the tree —
  `validate_set_name` at crates/umber-core/src/project.rs:348, the
  traversal-name rejection test at
  crates/umber-mesh/src/mesh_maps.rs:174, `MAX_PRESET_BYTES = 1 << 20`
  at crates/umber-core/src/smart.rs:148, the inflate bound at
  crates/spp/src/texture_cache.rs:32 — and "not yet in a release" is
  literally true (no tagged release exists; SECURITY.md:5-7). But the
  anchor calls a 144-commit-old commit "HEAD": **stale-by-anchor on 3
  of 10 rows; the claims themselves hold.** When the first release tags,
  all three anchors should flip from `ddb59e0` to the tag.

## 4. Named gaps — what the numbers show that the docs don't admit

1. **The density floor's tightest resident is the hostile-input
   parser.** spp, the crate that reads untrusted HDF5-subset bytes (a
   named threat surface, SECURITY.md:31), has the workspace's thinnest
   test density — 7.29/kLOC, +4.1% over the 7 floor, and only 2 of its
   48 tests are integration/corpus tests
   (crates/spp/tests/corpus.rs:2 `#[test]` occurrences of 48). Its own
   module docs admit an unverified seam inside that thin cover: the
   HDF5 message-type numbers are "best-effort recall … the first corpus
   file must re-pin them" (crates/spp/src/lib.rs:24-28). ROADMAP.md:22's
   "no crate below 7" is true but never names which crate is holding
   the floor up by 4%.
2. **Workspace density is drifting DOWN, not holding.** The 10-17
   census recorded 166.5k LOC / 2,061 tests = 12.4/kLOC
   (docs/ROADMAP.md:21-22); today reads 186,170 / 2,251 = 12.09/kLOC —
   LOC +11.8% in 7 days, tests only +9.2%. No doc records the trend or a
   rule for it; the "12.4/kLOC" headline is now 0.3 stale.
3. **umber-app is 29.6% of the workspace.** 55,172 of 186,170 LOC —
   2.11× umber-gpu, the next largest (26,199). The umber-paint
   extraction (2026-10-14, crates/umber-paint/src/lib.rs:3) moved the
   engine out of the shell, yet the shell remains the dominant mass and
   its `main.rs` is still 5,946 lines. AGENTS.md's workspace map
   carries no size signal, so this concentration is invisible in the
   docs.
4. **paint_state.rs is 11,257 lines** — a single file larger than 12 of
   the 16 whole crates (all of umber-core is 8,741). umber-paint packs
   19,908 LOC into 6 files (~3,318/file, the workspace's highest
   per-file concentration). No design doc or module doc states a
   file-size boundary for the paint engine.
5. **The perf protocol's triage log stops at 2026-10-19, but the
   nightly has been RED since 10-21.** Three consecutive reds on the
   ubuntu-26.04 image rotation (10-21 aac1c4a2, 10-22, 10-23 96edd1e4)
   are recorded only in the workflow comments
   (.github/workflows/perf-nightly.yml:30-38) and commit ec7ccb8
   (2026-10-24); STATE.md:39-40 still says "still RED since 10-21,
   latest run 39923126737". The 24.04 pin's own behavioral gate —
   "tomorrow's nightly on the pinned image" (ec7ccb8) — is unverified
   at HEAD: no green pinned-image nightly exists anywhere in the
   record.
6. **The [M8] stage-4 drag gate is still UNDECIDED even though its
   deciding run happened.** The gate is FAIL on llvmpipe (318.28 ms vs
   the 16.7 ms budget, docs/perf/protocol.md:162) and "stays UNDECIDED
   until the reference-GPU run" (docs/perf/protocol.md:169) — but the
   2026-10-19 RTX 3070 record (docs/perf/protocol.md:438-461) and its
   harness (scripts/perf-3070.sh:32-34 runs `pool_drag`; its row
   template asks for "pool_drag p95 <n> ms", perf-3070.sh:38) omit the
   drag number. The protocol's state of record and its own tooling
   disagree about whether the [M8] drag verdict was ever captured.
7. **The installer budget's tracking table does not exist.** The budget
   says "< 150 MB … tracked in this doc's table" (docs/perf/protocol.md:17),
   but the baselines table (docs/perf/protocol.md:152-153) has no
   installer column and no row has ever carried one. The flatpak
   artifact measured 12,635,392 bytes (STATE.md:28) — 12× under budget,
   yet unrecorded in the protocol.
8. **The nightly compare gates 2 of the 5 budgets.** baseline.json
   carries four metrics (viewport mean/p95, i2p p50/p95;
   perf/baseline.json:5-8). Stroke throughput rides the soak's
   zero-drop *assert* (a harness failure, perf-nightly.yml:7), not the
   >10% compare; undo RAM is measured only by the metadata-only model
   ("the ceiling pins shape, NOT the §12 ~2GB@4K budget",
   docs/perf/protocol.md:154); the installer is untracked (gap 7). The
   workflow's claim ">10% regression on any budget fails the job"
   (perf-nightly.yml:5) is in practice "on any of the two compared
   budgets".

## 5. Limits

- Read-only census: no builds, no tests run — the counts are static
  (`#[test]` occurrences, raw line counts, the same yardstick the
  10-17 census used). Nothing here verifies a test passes.
- The working tree holds only untracked `.dragon/` orchestrator files
  (10 `??` lines, `git status --porcelain`), so the numbers describe
  committed code at HEAD 0ec64bd; concurrent-agent edits landing after
  this audit will shift them.
- SECURITY.md substance-checks (gap-3 greps in §3) confirm the named
  defenses exist in the tree; they do not re-verify the battery claims
  the rows cite.
