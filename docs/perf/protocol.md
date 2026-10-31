# Umber — Performance Protocol (§12)

The measurement contract for the requirements §12 budgets. Written
2026-10-09 (full-project audit finding: the budget targets existed
with NOTHING measuring them). This doc defines what we measure, how,
and where the numbers land — instrumentation lands with the wave-4
painting work so every new perf-relevant change ships with a number.

## Budgets (from requirements §12 — unchanged, now enforced)

| Budget | Target | How measured |
|---|---|---|
| Input-to-photon | < 20 ms @ 60Hz | `frame_timestamps` tracing span: stylus/pointer event receive → frame present callback. P50/P95/P99 per session, reported in the perf HUD + tracy. v1 headless lower bound: see *The input-to-photon session* below. |
| Stroke throughput | 4K set, no dropped dabs @ 200Hz input | Dab-count instrumentation: dabs staged vs dabs rendered per frame. A dropped dab is a staged dab absent from the frame's upload. Assert zero in the stroke soak test. |
| Viewport | 60 fps sustained, P95 frame-time < 16.7ms, 1M-tri + 4K set | The reference-scene benchmark: a 1M-tri procedural mesh (subdivided cube grid, checked in) rendered headless via lavapipe in CI + live on dev GPUs. |
| Undo RAM | 100 steps @ 4K within ~2GB | TilePool accounting: `total_bytes()` sampled after each undo push in the soak test; assert monotonic-under-cap. |
| Installer | < 150 MB tracked per release | `cargo dist` artifact size logged at release CI; tracked in this doc's table, not gated. |

## Instrumentation plan (wave-4, lands with the painting work)

1. **`profiling` facade + tracy-client** behind a `perf` feature
   (default off — zero cost when off, the crate is already the §10
   P0 plan). Spans: `frame`, `stroke_eval`, `dab_upload`, `bake_pass`,
   `display_pass`, `undo_push`.
2. **The perf HUD** (egui overlay, `perf` feature): frame ms, dab
   throughput, tile-pool bytes, undo depth. Live numbers beat after-
   the-fact reports for the artist-facing floor.
3. **CI soak tests** (`perf` feature, nightly job — NOT per-commit;
   the load directive): the 200Hz stroke soak, the undo soak, the
   reference-scene render. Numbers land in the CI summary + a
   `perf/baseline.json` checked in; a regression >10% on any budget
   fails the job.

## The input-to-photon session (input→photon row)

`crates/umber-app/src/input_photon.rs`, behind umber-app's `perf` feature
(design: `docs/specs/input-photon-design.md`):

```
UMBER_I2P_EVENTS=5000 UMBER_I2P_JSON=i2p.json \
  cargo test -p umber-app --features perf --release input_photon -- --nocapture
```

- Session: N synthetic pointer events (default 5000) on the stroke
  soak's 200Hz logical clock, one diagonal begin/extend/end stroke across
  a 4096² target, each pushed through `PaintState`'s real event path and
  drained with `process_pending` (the drain main.rs's `frame` span wraps).
  No wall-clock sleeping.
- Per-event delta: logical receive mark → the first drain at or after it
  whose dispatch counter advanced (the frame that composited its paint),
  i.e. `(j − i) × 5ms` + that drain's measured wall span (push → GPU
  completion via `device.poll(Wait)`). Events after the last dispatching
  drain are reported as `unpresented`, never counted.
- **Present proxy: the PaintThread's GPU completion**, not the winit
  present callback (unreachable headless). egui layout, the compositor
  pass and the swapchain/vsync wait are NOT included, so the number is a
  **lower bound** on true input-to-photon. The live tracy session is the
  follow-up.
- The test asserts only that the harness is correct: dabs staged, at
  least one dispatching drain, events presented, p50 ≤ p95 ≤ p99. It
  prints the P95 < 20ms verdict without failing on it. The nightly
  job's >10% regression comparison is the gate.
- Output: one `input-to-photon baseline:` line (events presented,
  synthesized, unpresented, p50/p95/p99, verdict), plus one
  `input-to-photon json:` line (`events, p50_ms, p95_ms, p99_ms,
  budget_ms`, where `events` is the presented count the percentiles
  cover). Set `UMBER_I2P_JSON` to also write the JSON to a file. The
  harness doesn't print the adapter, so note the hardware class by hand
  with every row (lavapipe and the dev GPU are separate baselines).

## The reference-scene bench (viewport row)
`crates/umber-gpu/src/ref_scene.rs`, behind umber-gpu's `perf` feature:

```
UMBER_REF_SCENE_FRAMES=1000 UMBER_REF_SCENE_JSON=ref-scene.json \
  cargo test -p umber-gpu --features perf --release ref_scene -- --nocapture
```

- Scene: 289-subdivision cube grid (1,002,252 tris, generator checked
  in, deterministic) + four solid 4096² maps, drawn through the
  viewport's own mesh-pass `draw` into a 1920×1080 sRGB + Depth32Float
  offscreen target. 3 warm-up frames are not counted; the default is 120
  measured frames, and recorded baselines use ≥ 1000 (the rule below).
- The 4K set is **resident, unsampled**: the mesh pass has no texture
  binding yet. Until it gains one, the number covers the 1M-tri half of
  the budget plus the set's VRAM pressure, not texture sampling.
- The test asserts only that the harness is correct: N frames timed, all
  times non-zero, p50 ≤ p95 ≤ p99, and the mesh covers more than 2% of
  the target (an empty render covers about 0%). It prints the P95 < 16.7ms verdict without failing on it.
  The nightly job's >10% regression comparison is the gate.
- Output: one `ref-scene baseline:` line, plus one `ref-scene json:` line
  (`frames, mean_ms, p50_ms, p95_ms, p99_ms, tris` + adapter, device
  type, backend). Set `UMBER_REF_SCENE_JSON` to also write the JSON to a
  file. Record the adapter with every row, since lavapipe and the dev GPU
  are separate baselines.

## The per-layer-pool stage-4 gate ([M8], composite-on-read)

Stage 4 replaces the flat-target splat with splat-plus-region-recomposite.
Two scenarios gate the landing (both pass/fail against the 16.7 ms frame
budget — the arithmetic does NOT prove them inside budget, so measurement
does):

1. **Metadata drag**: drag the opacity slider of a 5-channel MASKED
   material at 4K (all channels enabled, every entry masked). egui emits
   one change per frame; uncoalesced that is `L_c × 16.8M × channels ×
   tiles` per frame plus every masked entry's mask upload. Coalesced
   (§4.2: at most ONE `Full` per DISPLAYED `(tile, channel)` per frame)
   it costs ≤ 3 full-tile recomposites per frame; everything else stays
   dirty and composites on read. Procedure: `cargo test -p umber-app
   --features perf --release --test pool_perf pool_drag` (built —
   synthetic slider values through the shell's snapshot-diff + flush
   path, `device.poll(Wait)` per frame; `UMBER_POOL_DRAG_NOMASK=1` isolates
   the mask cost).
   Gate: P95 frame < 16.7 ms.
2. **X-symmetry stroke near the tile edge**: a mirrored pair at x≈100 and
   x≈3996 on one 4K `(tile, channel)` must stay TWO rects (≤ 8 per
   `(tile, channel)`), costing `2 × L_c × |R|`, not a near-full-width
   union. Past 8 rects the closest pair merges (smooth degrade toward
   `Full`). Procedure: same harness family (`pool_mirror`), one stroke
   with X symmetry straddling the edge, per-frame flush wall time.
   Gate: P95 frame < 16.7 ms, and the dirt census asserts 2 rects (unit:
   `distant_footprints_stay_two_rects` in `paint_state.rs`).

Both harnesses are nightly-gated like the soak tests (NOT per-commit);
their rows land in the baselines table below with the hardware class
(lavapipe and the dev GPU are separate baselines). Until the harnesses
land, the gate is the two unit pins (rect count + coalescing) plus the
input-to-photon re-measurement under composite-on-read (the stroke frame
is now splat + region recomposite — re-run the i2p session before stage
4 lands and record the row).

## The real-GPU row (C2-16): `scripts/perf-3070.sh`

The baselines below were all measured on llvmpipe or lavapipe (CPU
rasterizers) apart from the 2026-10-10 stroke soak on the RTX 3070. That
row predates the ref-scene bench, so **no real-GPU viewport number
exists**. On the 3070 host:

```bash
bash scripts/perf-3070.sh      # ~10-20 min: release build + three benches
```

It runs the ref scene (1M tris + 4K set, 1000 frames), input-to-photon
(5000 events) and the composite-on-read drag. It writes the three JSONs
to `perf-3070-<commit>/` and prints the row template for the table
below. The 3070 is above SPEC's 3060 class (amendment A9), so a pass
there shows the budget is reachable; it does not prove the class floor.

## Baselines (fill as instrumentation lands)

| Date | Commit | Input→photon P95 | Dab drop @200Hz | Viewport P95 (ref scene) | Undo RAM @100 steps | Notes |
|---|---|---|---|---|---|---|
| 2026-10-10 | 19e71c5 | — (tracy session pending) | **0** (2914/2914 staged=composited; inter-dispatch mean 8.56ms, p50 10ms, p95 15ms; 4096² target, 3070) | — (ref-scene bench not built) | 19016 B final, sub-linear (first-10 Δ 1746B > last-10 Δ 980B), empirical ceiling 38032B | First real row. Undo numbers are the metadata-only model (no GPU-tile bytes in Document yet — the ceiling pins shape, NOT the §12 ~2GB@4K budget). Stroke soak ran the `perf` feature suite remotely (single-threaded, ICD pinned). |

| 2026-10-14 | cfb205c (stage 5) | **10.18 ms** (p50 0.22, p99 10.24; 4998/5000 presented) | — | — | — | Stage-3 baseline was p95 10.19 ms: composite-on-read + stage 5 cost nothing measurable on the stroke path. **Adapter: llvmpipe (CPU rasterizer, LLVM 20, 8 threads)** — this host's iGPU (HD 530) is not reachable (`/dev/dri/renderD128` is `root:render`, user not in `render`). Present proxy = PaintThread GPU completion (lower bound). |

### Stage-4 [M8] gate rows (2026-10-14, cfb205c, llvmpipe)

| Scenario | P95 | p50 / p99 | Budget | Verdict on this adapter |
|---|---|---|---|---|
| Metadata drag, 5-ch MASKED material @4K, 120 frames | **318.28 ms** | 223.87 / 374.76 | 16.7 ms | FAIL on llvmpipe |
| Same, masks off (`UMBER_POOL_DRAG_NOMASK=1`, 30 frames) | 241.13 ms | 191.54 / 262.22 | 16.7 ms | FAIL on llvmpipe — the mask path adds ~30% |
| X-symmetry stroke near the tile edge @4K, 120 frames | **2.67 ms** | 1.58 / 3.39 | 16.7 ms | PASS |

Read these as the CI-floor class, not the gate: the budget is a [HW]
criterion on the reference GPU class (the 2026-10-10 row's 3070), and a
CPU rasterizer recompositing two full 4K tiles per frame was always going
to miss it. The drag gate stays UNDECIDED until the reference-GPU run.

Harness correction: the stage-4 kill's `pool_drag` published no planes and
marked no dirt, so it flushed nothing and printed p95 0.003 ms — a false
PASS. It now builds the real scenario (5 material planes + a real mask,
the shell's `full_owners` → `mark_owner_full` → flush cycle, the two
displayed caches materialized) and ends with a sanity pin that fails if
the session composited nothing. `pool_mirror` gained the same kind of pin
(stroke + its X mirror must both land).

## Rules

- No perf-relevant PR merges without a number next to the claim.
- Baselines are per-hardware-class: lavapipe (CI floor), the named
  mid-range reference GPU (dev machine), recorded separately.
- "Feels fine" is not a measurement. Neither is a single run — every
  budget number is P95 over ≥ 1000 frames or ≥ 1000 dabs.

## Nightly verdict 2026-10-16 (run 38869909446, head `06c1711`) — RESOLVED by the triage below

The first COMPLETED perf-nightly in a week (all prior runs cancelled)
was RED on a PRE-wave head:

- `viewport_mean_ms` 84.07 vs the then-baseline 5.49 (**x15.3**)
- `viewport_p95_ms` 91.87 vs 6.24 (**x14.7**)
- input-photon both OK (p50 x0.33, p95 x0.97)

## Triage verdict 2026-10-16 (run 38957356307, head `5e0d89e`) — a baseline-class mismatch, closed by reseed

A fresh nightly dispatched against CURRENT master (`5e0d89e`, the
W-GUI-1 wave) to decide whether the x15 lived past the pre-wave head:

- `viewport_mean_ms` 45.54 vs the then-baseline 5.49 (**x8.3**)
- `viewport_p95_ms` 57.24 vs 6.24 (**x9.2**)
- input-photon both OK (p50 x0.15, p95 x0.96)
- stroke soak green: 2914/2914 dabs composited, zero drops, @200Hz on
  the 4K target

Diagnosis: the regression is NOT a master regression — it is the
hardware-class mismatch the 2026-10-13 seed's own note prescribed
reseeding away. That seed was measured on the dev machine's Intel HD
530 over GL; the nightly measures on lavapipe, GitHub's SOFTWARE
Vulkan rasterizer, which is ~8x slower on this bench by nature. The
note read: "reseed from a CI run once the nightly produces authentic
runner numbers" — the first completed nightly produced them, and the
reseed landed 2026-10-16 (`perf/baseline.json`, seeded from run
38957356307's own metrics).

What the two completed runs ALSO measure, against the same yardstick:
the pre-wave head `06c1711` at x15.3 → the W-GUI-1 head `5e0d89e` at
x8.3 — the host-return wave HALVED the absolute viewport frame time
on lavapipe (84.07ms → 45.54ms mean). The protocol keeps the red
recorded here: the seed change is a re-baseline with written
justification (this section), exactly the remedy the original triage
row named. The nightly's compare gate is now apples-to-apples
(lavapipe baseline vs lavapipe runs); the next nightly is the first
clean read of the gate.

## Triage verdict 2026-10-19 (run 39263843229, head `0955ac8`) — e0063 VERIFIED; the first clean gate read is RED on real numbers — the live-regression hunt is NAMED

The E0063 fix (`b942d64`, cube_grid's `triangle_materials` default)
unblocked the nightly's build step — the first run to reach the
compare gate since the reseed:

- BUILD GREEN: the perf-harness compiles; the 39097675689 /
  39108802121 / 39222850046 build-step red class is closed.
- COMPARE RED on real numbers, against the AUTHENTIC runner-lavapipe
  baseline (9acdc1c, seeded from 38957356307's own metrics on
  `5e0d89e`):
  - `viewport_mean_ms` 83.47 vs 45.54 (**x1.83**)
  - `viewport_p95_ms` 91.25 vs 57.24 (**x1.59**)
  - `input_photon_p50_ms` 0.219 vs 0.094 (**x2.32**)
  - `input_photon_p95_ms` x1.01 OK

This is NOT a replay of the x8/x15 hardware-class mismatch (that
closed with the reseed). The same-tip noise floor measured so far is
TIGHT: 45.54 (the seed run) vs 48.31 (the 10-17 nightly,
39013831785, one day and a handful of commits later) = x1.06. A x1.83
viewport reading against a x1.06 floor is a LIVE regression signal,
not runner noise — the viewport frame time is back to nearly the
pre-wave `06c1711` level (84.07ms).

Suspect window: `5e0d89e..0955ac8`. Lead suspects: the stencil
wave's per-frame viewport/app surface (viewport.rs's overlays_header
+ stencil_owns gating in the eyedropper/id-mask paths,
display_panel.rs's stencil rows, the app's stencil state) and the
masked-bakes ray half's bake_sources helpers — the first bisect leg.

Discriminator before the bisect: confirmation re-run 39268968173 on
the SAME tip (`0955ac8`) — a second ~83 read excludes runner
variance and opens the bisect; a ~48 read closes this red as
variance. Verdict updates this note.

## CONFIRMED 2026-10-19 ~14:50Z — runner variance excluded, the bisect opens

Confirmation run 39268968173 (same tip `0955ac8`): viewport_mean
87.28 (**x1.92**), viewport_p95 95.63 (**x1.67**), input_photon_p50
0.167 (x1.77), input_photon_p95 x1.01 OK. Two independent reads —
83.47 and 87.28 — agree within x1.05 of each other and both nearly
double the baseline. The live regression is REAL: the viewport frame
time nearly doubled somewhere in `5e0d89e..0955ac8`, back to the
pre-wave `06c1711` level (84.07).

Correction to the triage above: the 10-17 nightly (39013831785) ran
on head `5e0d89e` itself — the same tip that seeded the baseline —
so the 45.54-vs-48.31 pair is the same-tip noise floor (x1.06), and
NOTHING past `5e0d89e` is exonerated: the suspect window includes
W-GUI-3/4's GPU-side work (the masked-bakes end-to-end chain
`a23bcac`..`73a57cd`) as much as the stencil wave and the fleet.

Bisect leg 1: run 39273743336 on `bisect/perf-2026-10-19` (`b942d64`
— post-e0063, pre-stencil-wave; the boundary that splits the
W-GUI-3/4+i18n era from the stencil-wave+fleet era). A ~48 read
puts the guilt in `caa21e1..0955ac8`; a ~84 read puts it in
`5e0d89e..b942d64`. Each leg is a ~20-min nightly dispatch on a
temporary branch ref (workflow_dispatch needs a ref, not a raw SHA);
the branch is deleted at hunt close. The next session reads
39273743336 live and fires leg 2 from its verdict.

## Bisect leg 1 verdict (2026-10-19 ~15:2xZ) — the guilt PREDATES the stencil wave

Run 39273743336 on `bisect/perf-2026-10-19` (`b942d64`):
viewport_mean 85.99 (**x1.89**), viewport_p95 94.58 (x1.65),
input_photon_p50 0.211 (x2.24). The regression is FULLY PRESENT
before the stencil wave — the stencil-fleet era (`caa21e1..0955ac8`)
is EXONERATED (86.0 at b942d64 → 83.5/87.3 at 0955ac8 is flat
within the x1.05 read-to-read noise).

The guilty window is `5e0d89e..b942d64` (32 commits; the
code-relevant set): the W-GUI-2 wave (`d9fa931` — 19 kills incl.
camera frame/zoom, context rail, density, held-key poll, the
overlays popover), W-GUI-3's shell halves (`7b5b33e`), falloff
Custom (`4a2e3c2`, the paint pass's binding-5 LUT), the CurveEdit
consumer (`7208a7b`), the triangle_materials / masked-bakes chain
(`a23bcac`/`5aae0ec`/`c59034b`/`59c679a`/`73a57cd`), the row-label
registry (`8eb02c9` — its own commit reads "the shell re-registers
per frame"), the E0063 tail (`a394876`, test-gated), the FBX fixture
(`7f3d529`, test), and e0063 itself (`b942d64`, one harness-gated
line).

Bisect leg 2: run 39278581016 on `bisect/perf-leg2` (`7208a7b` —
post-falloff+CurveEdit, pre-masked-bakes; the W-GUI-2/3 | W-GUI-4
boundary). A ~48 read puts the guilt in `a23bcac..b942d64` (the
masked-bakes chain, the registry, e0063); a ~84 read puts it in
`5e0d89e..7208a7b` (W-GUI-2's giant `d9fa931` the heaviest single
suspect). One fresh branch per leg — force-moving a bisect cursor
trips the destructive-command guard, and per-leg branches sweep
clean at hunt close either way.

## Bisect leg 2 verdict (2026-10-19 ~15:53Z) — the guilt is in the W-GUI-2/3 + falloff half; the dark zone is exonerated

Run 39278581016 on `bisect/perf-leg2` (`7208a7b`): viewport_mean
90.82 (**x1.99**), viewport_p95 99.45 (x1.74), input_photon_p50 0.187
(x1.98); the stroke soak stayed green (2914/2914, zero drops). The
regression is FULLY FORMED at 7208a7b — everything after it (the
masked-bakes chain, the row-label registry, the E0063 class, the
stencil wave, the fleet) is exonerated, and the a23bcac..b942d64
dark zone needs no cherry-pick bridge after all.

Within `5e0d89e..7208a7b`, the static narrowing holds: exactly TWO
commits touched umber-gpu (the bench's own crate) — `d9fa931` (the
W-GUI-2 wave's umber-gpu half: camera frame/zoom, subset-mesh,
occluder-scope) and `4a2e3c2` (falloff Custom: the paint pass's
binding-5 LUT + the pipeline change). Leg 3 at `7b5b33e`
(post-shell-halves, PRE-falloff) decides between them: a normal
(~48) read convicts 4a2e3c2; a ~84+ read convicts d9fa931 and the
hunt goes inside its umber-gpu diff.

Bisect leg 3: run 39285259123 on `bisect/perf-leg3` (`7b5b33e`),
dispatched 16:25Z.

## Bisect leg 3, first attempt (16:29Z) — a build-failure non-verdict; re-dispatched as 3b

Run 39285259123 on `bisect/perf-leg3` (`7b5b33e`) died at the BUILD
step: E0425, `dialog_buttons` not found in `umber_gui` — the known
missing re-export that `efd0cbf` (7b5b33e's immediate child) fixed;
the wave commit's own battery had run on the rsynced host tree and
CI caught the pushed-tree gap. NOT a data point. Re-dispatched at
the buildable boundary: run 39288080877 on `bisect/perf-leg3b`
(`efd0cbf`), same tree shape — post-W-GUI-3, pre-falloff. Decision
rule unchanged: a ~48 read convicts 4a2e3c2 (falloff — the fix hunt
opens on paint_callback's per-frame bind-group/LUT path, provable
with a local Lavapipe run on the host); a ~84+ read convicts
d9fa931 and the hunt goes inside its udim.rs/assets.rs/snapshot.rs
(camera.rs analytically cleared; the falloff WGSL fetch is
case-10-gated and the LUT texture is created once at compositor
construction).

## Bisect leg 3b verdict (2026-10-19 17:06Z) — the W-GUI-2 window is CONVICTED, and the paradox is live

Run 39288080877 on `bisect/perf-leg3b` (`efd0cbf`, buildable,
post-W-GUI-3, PRE-falloff): viewport_mean 78.07 (**x1.72**),
viewport_p95 87.04 (x1.52), input_photon_p50 0.169 (x1.80). The
regression PREDATES falloff — `4a2e3c2` is exonerated; the guilt
entered in `5e0d89e..efd0cbf`, whose only code payload is the
W-GUI-2 wave (`d9fa931`) plus off-path fix tails.

**The paradox**: the completed static map cleared EVERY file
d9fa931 touched — camera.rs (event-driven adds, never in the bench
loop), udim.rs (`filter_mesh_for_triangles`, a bake-side helper),
assets.rs, snapshot.rs (the bench passes None brush). A conviction
with no on-path code change means either the per-frame path model
is incomplete, or the comparison's YARDSTICK moved: the conviction
compares today's reads (78-91 across five runs, all red) against a
baseline measured 10-16/17 (~45-48). The runner image is IDENTICAL
both eras (ubuntu-24.04, Version 20261012.334.1) — the remaining
confound is the fleet's CPU mix (lavapipe is CPU-bound; hosted
runners span Intel/AMD generations at different clocks).

**Leg 4 — the decisive infra-vs-code discriminator**: run
39292605800 on `bisect/perf-leg4` (`5e0d89e`, the baseline's OWN
tip), dispatched 17:15Z. A ~84 read TODAY convicts the runner
fleet (the code is innocent; the nightly needs a fresh same-era
reseed + a drift-hardened yardstick — the hunt reframes to
instrument). A ~48 read keeps the code guilty and the hunt goes
inside paint_callback's internals with the local Lavapipe repro.

## Bisect leg 4 verdict (2026-10-19 17:36Z) — THE RUNNER FLEET IS CONVICTED, THE CODE IS INNOCENT

Run 39292605800 on `bisect/perf-leg4` (`5e0d89e` — the baseline's
own tip, the commit whose CI run seeded the 45.54 baseline on
10-16 and re-read 48.31 on 10-17):

- **viewport_mean 89.29 benched TODAY.** The baseline's own tip,
  with byte-identical bench code and scene, reads ~2x its
  three-day-old measurement on today's fleet.
- The compare's x16.26 line is this checkout's OWN baseline file —
  5e0d89e predates the 9acdc1c reseed, so its compare ran against
  the 10-13 Intel-GL numbers (5.49); the hardware-class mismatch
  redux, not a datum. The RAW MEAN is the evidence.
- input_photon_p95 stable (10.15, x0.97 vs even the GL baseline's
  10.53) — the metric dominated by fixed-latency work barely moved;
  the CPU-bound means halved.

**The full read table**: 5e0d89e 89.29 (today) vs 45.54/48.31
(10-16/17); efd0cbf 78.07; b942d64 85.99; 7208a7b 90.82; 0955ac8
83.47/87.28. Every commit reads 78-91 today, every era-of-baseline
read sat at 45-48. **No code regression exists.** GitHub's
ubuntu-24.04 fleet (image v20261012.334.1, identical both eras —
so the physical host population, its CPU mix, is the mover)
shifted CPU throughput for this llvmpipe-bound bench by ~x0.5
between 10-17 and 10-19. The static map was right: every
"conviction" was the yardstick moving.

**The hunt's verdict**: the nightly's ABSOLUTE-ms baseline on a
mutable shared fleet is a broken instrument. The prescription:
(1) reseed the baseline from today's fleet on the current tip,
with the written justification this note carries; (2) harden the
yardstick against the NEXT drift — multi-run medians (N>=3) before
a REGRESSION verdict, a same-tip canary leg when a red lands
exactly like this one, or a normalized metric. The bisect branches
sweep at close. The x1.06 "noise floor" reading of 10-17 was two
samples of a fleet in transition — the floor is wider than
measured.

## The reseed lands (2026-10-19 ~17:5xZ)

`perf/baseline.json` reseeded per this note's prescription and the
`9acdc1c` precedent: same-tip means of the two `0955ac8` master
runs measured today (39263843229 + 39268968173) — viewport_mean
85.3739, viewport_p95 93.438, input_photon p50 0.1927 / p95
10.1799. The nightly's gate is apples-to-apples with today's fleet
again; the baseline file's own note carries the conviction record
and the fleet-era stamp. NAMED FOLLOW-UP (the drift-hardening
kill): N>=3-run medians before any REGRESSION verdict + the
same-tip canary leg on any red — this hunt cost four legs because
neither existed. The hunt closes as INFRA-DRIFT-ANSWERED: no code
regression, the instrument reset, the prescription on file.

## The real-GPU budget record (2026-10-19 ~15:5xZ) — C2-16 closed

The §12 budgets' first real-hardware record — centralhost's RTX
3070 (Vulkan, DiscreteGpu; at/above the §12 reference class), tip
`dc4b355` exported clean via git archive into an isolated host dir
(a sibling's in-flight bytes were never measured):

- **Viewport** (ref_scene_bench, 1000 frames, 1,002,252 tris, the
  4K set at 268,435,456 bytes resident, 1080p target): mean
  0.1931ms / p50 0.1900 / p95 0.2034 / p99 0.2081 — the bench's own
  verdict field `within_budget: true` against `budget_p95_ms: 16.7`
  (**~82x inside** the 60fps-sustained budget).
- **Input-to-photon** (4,998 events): p50 0.0877ms / p95
  10.0825ms / p99 10.0894ms against `budget_ms: 20.0` — half the
  budget; the ~10ms tail is the 60Hz present cadence of the
  drain-events-then-acquire loop.
- **Stroke throughput** (the 200Hz soak, 4096x4096 target):
  `staged=2914 composited=2914 drops=0` (inter-dispatch mean
  8.56ms), `test result: ok` — zero dropped dabs.

The lavapipe CI gate stays the regression instrument (fleet-era
baselined, drift-hardened by `perf_gate.sh`); this record is the
honest-claims data point the C2-16 row wanted — the budgets hold on
real hardware with 82x, 2x, and zero-drop margins.

## Nightly verdict 2026-10-30 (run 41034356551) — FLEET, NOT CODE: the bench binary is byte-identical across a red and a green reading

The 10-30 scheduled nightly (`41034356551`, tip `f66ed45`, ubuntu-24.04
+ lavapipe, adapter `llvmpipe (LLVM 20.1.2, 256 bits)`, rustc 1.99.1)
read `REGRESSION viewport_p95_ms: 108.8683 vs baseline 93.43795
(x1.165)` — gate 1 (median-of-3) red, the same-tip canary (median-of-5)
red, job failed at 10:03Z. Seven legs land here (the leg log), all read
from the CI artifacts, none re-benched locally (the build host is STOOD
DOWN; local cargo is not authorised).

### The leg log

| run (clock) | tip | passes | per-pass viewport p95 (ms) | median p95 | x vs baseline | median mean | x vs baseline | i2p p95 x | verdict |
|---|---|---|---|---|---|---|---|---|---|
| `40474654068` (10-27 08:36Z) | `0f7517b` | 3 | 51.65 / 53.86 / 54.38 | 53.8564 | x0.576 | 44.6023 | x0.522 | x0.991 | green |
| `40662953305` (10-28 09:16Z) | `25207fa` | 3 | 79.58 / 101.54 / 114.44 | 101.5431 | x1.087 | 79.0173 | x0.926 | x0.997 | green (by a hair) |
| `40848450318` (10-29 09:17Z) | `25207fa` | 3 | 80.66 / 87.64 / 112.93 | 87.6350 | x0.938 | 76.5533 | x0.897 | x1.000 | green |
| `41034356551` (10-30 09:33Z) | `f66ed45` | 5 | 82.15 / 102.78 / 108.87 / 111.19 / 117.31 | 108.8683 | **x1.165** | 78.8107 | x0.923 | x1.001 | **RED** |
| `41116918979` (10-30 19:38Z) | `3bd3040` | 3 | 77.11 / 79.30 / 106.09 | 79.3033 | **x0.849** | 72.7332 | x0.852 | x0.997 | green |
| `41117599643` (10-30 19:43Z) | `f66ed45` (same-tip control) | 5 | 79.67 / 105.20 / 119.56 / 121.76 / 121.91 | 119.5609 | **x1.280** | 103.1151 | x1.208 | x1.000 | **RED** |
| `41119360104` (10-30 19:56Z) | `25207faf` (pre-`ad6f44b` canary) | 3 | 78.56 / 79.27 / 108.60 | 79.2662 | **x0.848** | 74.0174 | x0.867 | x1.000 | green |

### What the log proves

1. **The bench binary is byte-identical across every boundary.**
   `git diff --stat 25207fa f66ed45 -- crates/umber-gpu crates/umber-core
   crates/umber-mesh crates/umber-graph` → EMPTY; same for
   `f66ed45..3bd3040`. The full diffs touch only `crates/umber-app/**`,
   `crates/umber-gui/src/theme.rs`, `assets/i18n/**`, `docs/**`,
   `STATE.md`. `crates/umber-gpu/Cargo.toml` carries no `umber-app` /
   `umber-gui` dependency, so app-side waves cannot enter the bench
   binary at all. `perf/baseline.json` and the workflow's compare step
   are unchanged across the window.
2. **Two runs of the SAME bench binary, started 13 minutes apart on the
   same evening, read x1.280 and x0.848** (`41117599643` vs
   `41119360104`): **x1.508 between the medians against a gate band of
   x1.10** — the noise is 1.37x the band the gate claims to resolve. A
   concurrent pair cannot be "the fleet slowed during the day": the legs
   ran AT THE SAME TIME on different runner hosts and differ by half.
   The same rustc, the same image, the same adapter string and the same
   baseline file sit behind all four 10-30 runs; the only variable is
   the physical host.
3. **A GREEN night contains a pass higher than the red night's median.**
   The 10-28 night (green at x1.087) contains p95 114.44 against the
   red night's median 108.87 — the red/green line sits INSIDE the
   per-pass scatter.
4. **`input_photon_p95_ms` never moves** (x0.991..x1.001 across the
   table): only the CPU-throughput-bound viewport metric swings
   (x0.576..x1.280). A viewport-path regression would move the viewport
   MEAN with the p95 — and the two signatures differ: the morning red
   night's mean read x0.923 (faster than baseline, a tail-only event)
   while the evening control's mean read x1.208 (a wholly-slow host).
5. The hunt's two named suspects (`e008dcc` g7-keyboard-only, `0f18fa7`
   the layout wave) were NOT in the red run at all: its head SHA is
   `f66ed45` and both landed after the 09:33Z dispatch. `ad6f44b` is the
   only code delta in the window and cannot reach the bench.

### DECISION

**Fleet. The code is innocent.** Third instance of this class (the 10-16
hardware-class mismatch, the 10-19 fleet drift, now the 10-30 per-runner
heterogeneity), and the first one where the bench binary is PROVEN
identical between a red and a green reading. The 10-19 drift-hardening
(median-of-3, then a same-tip canary median-of-5) is MEASURED
INSUFFICIENT: it draws 3 (or 5) samples from a distribution whose
per-night spread is x1.38..x1.53 and takes their median, so the verdict
tracks the sample draw, not the code.

### The instrument fix — named, not landed

The gate needs a yardstick whose noise band is narrower than the effect
it claims to detect: **P1** a same-job A/B RATIO against a pinned
reference commit (the fleet's throughput cancels — `8e1195e`'s
`workflow_dispatch` input `ref` is the mechanism); **P2** gate the MEAN,
or require both mean and p95 to regress; **P3** best-of-N instead of
median-of-N (the red night's best pass, 82.15, was UNDER baseline);
**P4** interim: widen the p95 band to the measured fleet spread (x1.25).
The reseed is due (the fleet era moved again: x0.576 on 10-27 → x1.280
on 10-30) and this protocol's precedent requires the leg log as its
written justification — but a reseed ALONE repeats the cycle; it must
ride with P1/P2, never precede them. Queued as its own two-gate wave.

### The full record

- `docs/loop/kill-reports/2026-10-30-perf-nightly-regression-verdict.md`
  (`55bcefa`) — the proof, the bench-crate diffs, the stand-down and the
  owned scratch-ref deviation.
- `docs/loop/kill-reports/2026-10-30-perf-fleet-canary-cells.md`
  (`e9001d2`) — the three-cell canary dissection and `8e1195e`.

The raw evidence (seven job logs + every run's scene JSONs) is staged in
the vault at `projects/umber/assets/2026-10-30-perf-gate-noise/`.

## The instrument wave (2026-10-30 ~18:3xZ) — P1 lands: the same-job A/B ratio

The 10-30 red's dissection (above, and the two kill reports of
2026-10-30) measured the gate's noise band at **x1.51 on identical
code** against its **x1.10 band** — the yardstick cannot resolve what
it claims. The named fix P1 landed as `0981d2f`:

- `scripts/perf_gate.sh` now fetches the pinned reference
  `PERF_AB_REF` (default `0955ac8467ce7670b3fb7dd24d15c835dd013ffc`,
  the tip `perf/baseline.json` was seeded from — move the pin with a
  reseed), archives it to /tmp/ab-ref with its own target dir, builds
  it, and runs its passes INTERLEAVED with the current tree's (pass i:
  current, then reference) so time-correlated fleet drift hits both
  legs equally.
- The verdict is the RATIO current/reference per metric
  (`scripts/perf_compare_ab.py`, band `PERF_AB_BAND` default x1.10),
  still two-gated: median-of-3 pairs, canary median-of-5 on a red.
  The fleet's throughput is a common factor and cancels; the ratio
  carries the code delta only.
- The absolute compare vs `perf/baseline.json` is now REPORT-ONLY in
  both the gate and the workflow step (`|| echo ::warning::`), kept in
  the log and the artifacts for this leg log.
- Artifacts carry the `/tmp/ab-*.json` files; job timeout 90 -> 120
  min for the second build.

Gate 1 for the wave (tooling scope, no Rust changed): bash -n +
py_compile + workflow YAML assertions + `perf_compare_ab.py` unit
cases (absolute-2x-but-ratio-1.0 is green, proving report-only;
ratio x1.20 fails; MISSING fails; reference-absent prints U; band
argv override) + a stubbed-cargo end-to-end of `perf_gate.sh` in both
directions (ratio 1.0 -> GATE 1 green exit 0; ratio ~1.95 -> gate 1
red -> canary -> GATE 2 red exit 1). No local bench (centralhost stood
down; the harness needs the CI lavapipe).

**Open:** the ratio's own noise floor is re-derived from the first
instrumented nightlies; if it also proves wider than x1.10, P2 (gate
the mean / require both mean+p95) and P3 (best-of-N) remain. No reseed
rides this wave — with the ratio gate the absolute baseline is
report-only, so reseeding is no longer urgent.
