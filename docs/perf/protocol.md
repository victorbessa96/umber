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
