# Lens E — engine, architecture, performance (2026-10-24, live-verified)

> Provenance note (2026-10-24, the orchestrator): this report is the
> pi claw's own work (glm-5.3, the lens-E brief) — its dispatch
> wedged mid-run, the dragon authored a solo version to unblock the
> audit (committed in d3e5538), and THEN the zombie process flushed
> its finished report minutes after its "death" (the shared-scratch
> lesson, live again: a claw's writes can land after its exit —
> verify the invariant, judge the tip's full state). The pi version
> is the richer (the roles column, the cross-checked method, the
> fleet-halving catch) and REPLACES the solo version here with
> attribution. The code state both measured is identical (HEAD
> 0ec64bd's tree — only docs commits since).


Read-only audit of the live tree at HEAD `0ec64bd` (W-C4 L5c, "paint tiles auto-map by material").
No cargo was run (claw rule 1); every count below is a direct python/grep tally over the
sources, and every claim carries a file:line from the tree as it stands today. The 10-17
full-project audit and the 10-24 lens-tools audit are the comparison anchors.

---

## 1. Crate map + health

**16 crates, 186,296 LOC (src + tests dirs), 2,251 `#[test]`s, 12.1 tests/kLOC overall.**
The 10-17 baseline (166,540 LOC, 2,061 tests) is a week stale: the tree grew ~19.8k LOC
(+12%) through the W-C3b sweep and W-C4 L1/L2/L3/L5c, and the test count grew with it.

| Crate | Layer | Owns | LOC | tests | /kLOC | Notes |
|---|---|---|---:|---:|---:|---|
| umber-color | L0 | OCIO, display LUT, per-channel CM flags | 964 | 26 | 27.0 | densest crate in the workspace |
| umber-mesh | L0 | glTF/FBX/OBJ/USD loaders, raycast, fills | 9,978 | 146 | 14.6 | + 33 panic-hunt tests (§5) |
| umber-graph | L0 | node graph, topo, CPU reference eval, MaterialX | 13,681 | 141 | 10.3 | CPU oracle for the GPU arm |
| umber-spp | L0 | clean-room read-only .spp import (HDF5 subset) | 6,587 | 48 | 7.3 | corpus-gated hex archaeology |
| umber-plugin-sdk | L0 | guest SDK (no_std, zero-dep) | 1,719 | 15 | 8.7 | |
| stylus | L0 | owned tablet input (winink behind a feature) | 1,590 | 31 | 19.5 | winink passive today (§6) |
| umber-core | L1 | document model, layers, undo, journal, .umber I/O | 8,750 | 126 | 14.4 | never-crash deny (lib.rs:20) |
| umber-export | L1 | tokens, packing, presets, writers (PNG/EXR/PSD/USD) | 4,941 | 84 | 17.0 | psd.rs = the P2 surprise (§7) |
| umber-wasm | L1 | wasmtime plugin host | 2,086 | 36 | 17.3 | runtime.rs covered by 26 integration tests |
| umber-gpu | L2 | renderer, paint, compositor, graph_eval, postfx, LUT | 26,221 | 220 | 8.4 | biggest crate; see density note |
| umber-brush | L3 | stroke conditioning, spacing, symmetry, presets | 9,321 | 159 | 17.1 | |
| umber-bake | L3 | mesh-map bakers, cage, dilation | 9,242 | 150 | 16.2 | |
| umber-paint | L4 | headless paint engine, per-layer planes, undo capture | 19,914 | 178 | 8.9 | paint_state.rs is 56% of the crate (§2) |
| umber-cli | L5 | headless CLI, batch recipes, agent JSONL | 7,920 | 79 | 10.0 | |
| umber-gui | L5 | design-language widgets (Phase 0a) | 8,228 | 136 | 16.5 | no `tr()` calls by contract |
| umber-app | L6 | eframe shell: panels, viewport, i18n | 55,154 | 676 | 12.3 | 53.2k src + 1.9k tests-dir |

**Density shape.** Nothing is below 7/kLOC. The low three all have named extenuations:
umber-gpu 8.4 carries 3,063 lines of zero-test WGSL string constants (`shaders.rs` 1,038 +
`bake_shaders.rs` 2,025 — behavioral coverage through render/golden tests; excluding them
the crate sits at 9.5); umber-paint 8.9 concentrates in `paint_state.rs` (11,257 lines /
99 tests, 8.8/kLOC for that one file); umber-spp 7.3 is the clean-room reader whose
coverage is corpus-driven (48 tests over the format decoding). The GUI/brush/bake/export
band holds 15–17/kLOC, and umber-color (pure math) leads at 27.

**Layering gate: GREEN** (`scripts/check-layers.py`, run 2026-10-24). L0 umber-color,
umber-mesh, umber-graph, umber-spp, umber-plugin-sdk, stylus · L1 umber-core, umber-export,
umber-wasm · L2 umber-gpu · L3 umber-brush, umber-bake · L4 umber-paint · L5 umber-cli,
umber-gui · L6 umber-app. Dependencies point DOWN only; intra-layer dependencies: none;
UI deps confined exactly where the rules allow (umber-gpu{egui-wgpu}, umber-gui{egui},
umber-app{all}). No upward deps, no violations, no unregistered crates.

**No-test >80-line files — the 10-17 audit's five, re-derived.** Eight files match the
scan today; of the 10-17 five, **four still stand** and one graduated:

- Still standing: `umber-gpu/src/bake_shaders.rs` (2,025), `umber-gpu/src/shaders.rs`
  (1,038) — WGSL sources, covered behaviorally through the render/golden tests;
  `spp/src/builder.rs` (619) — the synthetic .spp fixture writer, exercised by every
  round-trip in lib.rs; `umber-wasm/src/runtime.rs` (417) — 26 integration tests cover it.
- **Graduated:** `umber-paint/src/snapshot.rs` — 301 lines carrying **7 tests** today.
- New entrants, none a product gap: `umber-app/src/lib.rs` (83 lines of launch wiring),
  `umber-app/src/set_switch.rs` (131 — the W-C4 L1 kill that landed 2026-10-24; its tests
  live in `umber-app/tests/set_switch.rs`), and two test-support mods
  (`umber-app/tests/support/mod.rs`, `umber-wasm/tests/common/mod.rs` — harness code).

Verdict: the honest remainder is unchanged in kind — two WGSL constants files plus a
test-fixture builder. The W-C4 week opened no new product-code test gap.

---

## 2. Paint engine architecture — umber-paint + umber-brush

**Verdict: SOUND.** The stroke path is a documented, ordered pipeline with typed
refusals at every budget, an oracle discipline on the CPU side, and a hook ladder whose
order is written down in-module. The debts are named, bounded, and none of them is a
correctness hole.

### The stroke path, station by station

1. **Entry + routing** — `PaintState::push_event_styled` (`paint_state.rs:4909`). Every
   event routes per-tile through `route_tile(uv)` (`:4783`); crossing tiles resets the
   conditioner (per-event UDIM routing — cross-tile stroke continuity is the stated v1
   boundary, and `texels_per_uv_for_channel` at `:1671` is the W-C4 L2 density fix that
   just landed in `306a502`: dabs and the cursor ring now read the same channel-density
   function, so a 256-channel under a 512 set spaces and displays at 256).
2. **Seam awareness** — the stroke-level `stroke_near_seam` flag (`:1187`, set at
   `:1810`–`1813`) plus a per-event `mirror_positions` re-gate on UV-segment distance
   (`:4941`–`4964`, the two-deep gate; the math lives in `umber_mesh::seam_mirror`).
   Mirrors recurse through `push_event_styled` with `expand_seams = false`, so each
   mirror runs the same conditioning + staging for its own tile.
3. **Synthetic clock** — `EVENT_STEP_NS` (8 ms, `:579`) + event sequence numbers, so
   spacing and one-euro are deterministic and replayable.
4. **Conditioning** — `umber_brush::StrokeConditioner` (`wiring.rs:58`), pipeline order
   fixed in the module doc (`wiring.rs:1`): **one-euro filter → lazy mouse → line
   constraint → spacing**. Exact mode (`wiring.rs:68`–`73`) skips the one-euro/lazy
   filters but keeps spacing — the agent-stroke fix from 2026-10-15 (a 2-point wire
   stroke painted nothing at 1 Hz one-euro smoothing; the bug hunt is in ROADMAP's
   `wave-agent-stroke-conditioning` row, landed).
5. **The hook ladder** — `stage_stamps` (`paint_state.rs:5027`), in order:
   - **layer-mask hook first**: a mask-edit stroke routes to `mask_event` (`:5326`) —
     one gate per stroke; it paints neither the layer nor the quick mask.
   - **retouch hook**: `is_retouch()` dabs (smudge/clone/eraser, same position/radius/
     pressure-shaped alpha/hardness as paint dabs) divert to the per-stroke CPU mirror
     via `retouch_event` (`:5246`) — the GPU splat never sees them; the retouch stroke
     stamps its own symmetry siblings (it owns per-sibling smudge chains and clone
     sources).
   - **wash mode** — `apply_wash_mode` (`:5091`, defined `:5872`): scales dabs so
     overlap accumulates to stroke opacity; deliberately AFTER the retouch hook (their
     build-up alphas stay untouched) and BEFORE symmetry (siblings inherit the
     normalized alpha). One scalar per event batch, no shader arm.
   - **symmetry** — `expand_dabs` (`:5101`, from `umber_brush::symmetry`): every
     original dab followed by its siblings in the same batch; seam mirrors are not
     symmetrized (v1).
   - **quick-mask gate** — after expansion (`:5113`): every dab carries the shader's
     gate flag; `sync_mask_tile` (`:5511`) uploads the tile's mask first.
   - **undo capture** — `capture_paint_event` (`:5183`) on the FINAL dabs: first-touch
     tiles snapshot before-bytes, dabs' shader-identical footprints union into the dirty
     rect. "A dropped readback must never drop a stamp" — a tile whose readback fails
     records nothing but still paints.
   - **staging** — `PaintThreadCommand::Stage { dabs }` (`:5149`): one command per tile
     per event; the footprint unions into the dirty-rect; a first batch on a new
   `(owner, tile)` may be REFUSED (`§1.2`) — the refusal lands in `stroke_refusal`,
   the dabs on that tile are dropped, the stroke continues on its materialized planes.
   Typed refusal, never a panic.
6. **GPU side** — the PaintThread splits batches into capacity segments and
   overlap-safe dispatches; `PaintCompositor::splat_dabs` is one compute dispatch, one
   workgroup per dab, **no atomics** — overlapping dabs are split across calls and
   wgpu's hazard tracking orders them (the overlap contract, `umber-gu/src/paint.rs`
   module docs). `process_pending` (`paint_state.rs:4356`) drains every thread and
   returns `FrameStats` (dabs composited, dispatches, clears, segments).
7. **Composite-on-read** — `composite_target` resolves cache-else-Base and **reads
   never materialize** (`§1.1 [B3]`); `flush_composite(&CompositeSnapshot)`
   (`:3608`) is the end-of-update flush binders sample after; the snapshot builder is
   `umber-paint/src/snapshot.rs` (301/7 — the graduated file). A poisoned composite
   entry clears after one `warn` and drops its dirt — no per-frame spam
   (`:369`, `:3606`).

### Per-layer pool: planes and budgets (all `paint_state.rs`)

Planes are keyed `(PlaneOwner, tile, channel)` — every layer's every channel owns an
independent target set; the GPU-side keying (`umber-gpu/src/tile_pool.rs`, `PoolKey`)
keeps the channel a `u8` so a new channel kind breaks the exhaustive match at compile
time. The budget ladder:

| Budget | Value | Site | Behavior |
|---|---|---|---|
| `PLANE_BUDGET_BYTES` | 2 GiB | `:682` | soft budget — retire oldest plane |
| `PLANE_HARD_CAP_BYTES` | 4 GiB | `:688` | hard cap — typed refusal (`§1.2`) |
| `RETIRED_BUDGET_BYTES` | 512 MiB | `:1050` | tombstone store for retired planes, oldest frees first |
| stroke history | 256 MiB | module docs | per-stroke session history |
| undo envelope | 64 entries / 256 MiB | `:591`–`592` | the shared `UndoStack` cap (requirements.md:167: a full-tile 4K entry is 128 MiB — the third evicts the first) |

Retirement is read-back-then-tombstone, so the pool never loses paint the way
`umber-gpu`'s raw `TilePool::evict` can (that Wave-2 data-loss eviction semantics doc in
`tile_pool.rs` is real, but its consumers today are umber-gpu-internal — no
umber-paint/umber-app path calls it).

### Undo history

- `umber-core/src/undo.rs` (629/11): command-pattern `UndoStack`, generic over `Command`
  with exact-inverse apply/revert and `memory_bytes` feeding the RAM budget.
- Paint strokes capture dirty-rect before/after (`capture_paint_event`); retouch
  records one `PaintStrokeCommand` per stroke covering every touched tile
  (`umber-paint/src/retouch.rs` module docs).
- **Cumulative merge** (audit 2026-10-12-c1 Pass 4 item 5, the §12 undo-RAM row): a
  stroke overlapping the TOP entry coalesces into it at push time — oldest before +
  newest after over the bounding box, never the intermediate pair; it needs the
  stroke's full-tile start image because the bounding box's corners lie in neither
  rect. Older entries are never rewritten.
- **Journal** (`umber-core/src/journal.rs`, 943/6): append-only `journal.jsonl`, fsync
  per entry, replay past the high-water mark on load; corruption rule — stop at the
  first unterminated/malformed/unknown-op/non-increasing line, everything before it
  replays, never a hard error.

### Known debts (each with its seam)

1. **Split redo** — a merged entry undoes/redoes as one unit; split redo is the named
   follow-up (requirements.md:167, the §12 undo row).
2. **Merge-below-top** — merges happen against the TOP entry only (`retouch.rs` module
   docs: "older entries are never rewritten"); a stroke overlapping an older entry
   stacks a second snapshot. RAM absorbed by the 64/256 MiB envelope; correctness
   unaffected.
3. **L2b same-class arms** — symmetry snapshot, clone offset, fill raster, and seam
   mirror radius still read set-scale density; mapped as the named L2b follow-up
   (commit `306a502`; the debt register in `docs/loop/audit-2026-10-24-full-project.md:41,115`).
4. **Unified timeline (W-C4 L6)** — the procedural graph's undo journal is
   panel-local (`umber-app/src/graph_panel/history.rs:28`–`32`: "Kill 1 is
   panel-local… the unified timeline (Ctrl+Z, the History panel's rows, click-to-
   time-travel) is the named follow-up kill"); needs `main.rs` key dispatch +
   `history_panel.rs`. Graph edits were the last non-undoable edit class until this
   kill — the wiring is new and the seam is exactly where the docs say.
5. **L3b multi-entry ID bake cache** — multi-tile meshes no-op on the bake pre-warm by
   design (commit `6430374`).
6. **Cross-tile stroke continuity** — `route_tile` (`:4783`) resets the conditioner on
   tile crossing; per-event routing is the v1 shape.
7. **`paint_state.rs` gravity** — 11,257 lines / 99 tests, 56% of umber-paint in one
   file. The hook ladder is documented, but every stroke feature lands here; the next
   decomposition pass will need a plan, not a drive-by.
8. **Angle/alignment/jitter quartet** — not in the engine (`BrushParams` carries no
   such fields; requirements §1 row is honestly marked PARTIAL); the paint shader
   stamps a radially symmetric disc.

---

## 3. GPU stack — umber-gpu (26,221 / 220)

**Pins** (workspace `Cargo.toml`): wgpu 30 + naga 30 + naga_oil 0.23, eframe/egui 0.36,
egui_dock 0.21, openusd 0.8.0, wasmtime `=50.0.0-rc.1`. umber-gpu never constructs a
`wgpu::Instance` outside tests — eframe owns the surface and the device arrives through
`CreationContext::wgpu_render_state` (lib.rs docs; hard rule 4).

**Safe-start** (`umber-app/src/gpu_startup.rs`): a staged marker machine
(`gpu-starting.marker`) through planned → adapter → device, with `note_adapter` /
`note_frame` and `FRAMES_TO_CONFIRM` good frames before the working backend persists to
`gpu-backend`. The 2026-10-15 Wayland/GL false-positive incident (a confirmed-good GL
run left un-persisted) is documented in-module — the staging is the fix for exactly
that class.

**Shader surfaces**: every WGSL string lives inline as a `const &str` — the established
pattern (`compositor.rs` module docs name `TEXTURE_DISPLAY_SHADER`, `VIEW_MODE_WGSL`,
`POSTFX_SHADER` as precedent). Inventory: `shaders.rs` 1,038 (`PAINT_COMPUTE_SHADER`,
view modes, height-perturbed normals), `bake_shaders.rs` 2,025,
`graph_shaders.rs` 896 (3 tests), the compositor's inline `COMPOSITE_WGSL`, postfx,
texture display. The discipline that matters: WGSL is **transliterated from the Rust
oracle, not invented** — compositor.rs: "every match arm, clamp, and divide-by-zero
guard has a one-to-one counterpart, in the same order, under the same ids." The
compositor's load-bearing judgment calls are on-record: iterative per-layer dispatches
rather than a `binding_array` single pass (portability risk with CI as the only runtime
gate), and coverage-gated compositing `out = dst + (blend − dst) × cov` so a
transparent texel or an evicted tile passes through instead of erasing the stack — with
Replace the one documented exception (the opacity-bypass that landed with W-C3b).

**Error handling**: typed errors per module — `GpuError` (renderer.rs:23), `PaintError`
(paint.rs:185), `CompositeError` (compositor.rs:677), `GraphGpuError` (graph_eval.rs:326),
`IblError` (ibl.rs:344), `GoldenError` (golden.rs:238) — plus budget refusals
(`PaintRefusal` → `stroke_refusal`) and **OOM scopes**: `push_oom_scope` /
`await_oom_scope` (`paint_thread.rs:369/:382`) wrap dispatch drains with wgpu 30's
`ErrorScopeGuard` (this is the ROADMAP "wgpu 30 error-scope API shape" probe, landed
in-tree). The poisoned-entry rule (one warn, drop the dirt, next write re-marks) keeps
composite failures from spamming frames (`paint_state.rs:369`, `:3606`).

**Readback**: `PaintTarget::read_back_rgba8` — three consumers: the retouch first-touch
CPU mirror, the high-to-low transfer, PNG export; the undo capture tolerates a dropped
readback by recording nothing (never dropping the stamp).

**Probe/capability model**: `HdrProbe` (`hdr.rs`, 423/7) probes eframe's offered
surface formats; requirements §5 records the honest boundary — eframe 0.36 exposes no
format/colorspace hook (`preferred_framebuffer_format` hardcodes 8-bit SDR), so the HDR
surface switch is built and gated on an eframe upgrade. `gpu_startup`'s adapter notes
are the startup-side probe.

**Upgrade posture**: two named gates, both tracked in ROADMAP — `wave-hdr-surface`
(tracks eframe 0.36→next; capability model + probe ✓ built) and `wave-ray-query`
(tracks wgpu stability; the compute-shader raycast path is built and is the shipped
route). The post-pool probe kills: wgpu 30 error-scope shape ✓ landed; eframe 0.36 GPU
error-handler behavior, `PaintCompositor` Arc threading, and un-premultiply exactness
on GPU vendors remain runtime-probe-shaped (no vendor hardware in CI).

**graph_eval** (2,458/9): the WGSL arm over `umber-graph`'s CPU reference; hybrid
routing — per-pixel families on GPU, globals/assets/guests on CPU through the same
registry.

---

## 4. Performance

**Protocol** (`docs/perf/protocol.md`; budgets at requirements.md:164–168):
input-to-photon < 20 ms @ 60 Hz · 4K stroke, zero dropped dabs @ 200 Hz input ·
viewport 60 fps sustained, P95 < 16.7 ms, on a named mid-range GPU class (RTX 3060 /
RX 6600), 1M-tri mesh, 4K set · undo RAM 100 steps @ 4K within ~2 GB · installer < 150 MB
(tracked, not gated).

**Harnesses, all checked in**:
- `umber-app/src/input_photon.rs` — the input-to-photon timestamp instrumentation,
  `BUDGET_MS = 20.0` (`:72`).
- `umber-gpu/src/ref_scene.rs` (759/7, `perf` feature) — the §12 viewport harness:
  a 1,002,252-tri reference mesh (289-subdivision cube grid, pure arithmetic, layout
  pinned by spot tests) + four 4096² PaintTargets; `BUDGET_P95_MS = 16.7` (`:63`).
- `umber-app/tests/pool_perf.rs` — `pool_drag_session` (`:181`) and
  `pool_mirror_session` (`:287`), 16.7 ms per-frame budgets through the real adapter.
- The stroke soak — a once-only zero-drop assert that fails the nightly before any
  comparison runs.

**Nightly invocation** (`.github/workflows/perf-nightly.yml`): cron 03:00 UTC, hosted
lavapipe by default, `PERF_RUNNER=self-hosted` opt-in for the 3070 (no runners
registered yet — "every nightly 10-13..15 sat queued then cancelled"), and explicitly
NOT per-commit: failures surface drift, they don't block merges. The gate
(`scripts/perf_gate.sh` + `perf_median.py` + `perf_compare.py`): metric harnesses run
as **median-of-3**; > 10% regression on any budget fails; a tracked metric the run
didn't produce fails; on a red, a same-tip canary leg runs two more passes and the
**median-of-5 must also be red** before the job fails; artifacts keep every per-pass
JSON.

**Triage record** (protocol.md, the model of how this project handles perf incidents):
- **2026-10-16** (`:196`): baseline-class mismatch — the first baseline was seeded on
  the dev machine's Intel HD, not CI; closed by reseeding from a CI run (`:214`).
- **2026-10-19 ~14:50Z** (`:262`): the runner-fleet drift conviction. Identical tip
  `5e0d89e` read 45.54/48.31 on 10-16/17 and **89.29** benched 10-19; five tips spanned
  78–91 on the new fleet with a byte-identical image; a four-leg bisect
  (`:279/:309/:337`) proved the code innocent; "the CPU-bound means halved" (`:401`) —
  GitHub's ubuntu-24.04 fleet shifted CPU throughput ~×0.5 in a day. The remedy was
  twofold: the drift-hardened median gate (commit `dc4b355`) and the reseeded baseline
  (`c53a08d`/`b1d3660` → `perf/baseline.json`, seeded 2026-10-19 on lavapipe from
  same-tip master runs).
- **2026-10-21 → now**: a third, separate cause — `ubuntu-latest` rotated to 26.04 on
  10-21 and every nightly since dies with the panic_hunt suite hanging on the new
  image's adapter stack (three consecutive reds: 10-21 `aac1c4a2` the unchanged-tip
  first flip, 10-22, 10-23 `96edd1e4`, all `Image: ubuntu-26.04`). Remedy committed
  `ec7ccb8` (2026-10-24): pin the hosted default to 24.04, mirroring the CI matrix pin
  (`c697bab`, whose pinned 24.04 leg stayed green in run 40020097585). **Behavioral
  gate = the first nightly on the pinned image — pending at audit time.**

**The 60fps @ 1M-tri + 4K floor**: the harness is complete and checked in, the gate
compares drift against a reseeded baseline — but no authenticated 3060-class number
exists anywhere (see below), and `ref_scene.rs`'s own module doc says the 4K set is
held **RESIDENT, UNSAMPLED** (written before the mesh-pass tile binding landed — see
surprises).

**The honest unmeasured list**:

1. **The named reference-GPU-class floor itself** (RTX 3060 / RX 6600, current driver,
   1M-tri, 4K): zero authenticated samples. The nightly gate is drift-comparison against
   a lavapipe baseline, not an absolute-floor check, and the PERF_RUNNER 3070 opt-in
   has never run.
2. **Input-to-photon on real stylus hardware**: instrumented (20 ms budget), but every
   recorded number is CI/lavapipe; no digitizer-hardware sample exists.
3. **Windows/WARP perf leg**: the nightly is Linux/lavapipe-only; WARP appears in
   ci.yml's test matrix, but no perf harness runs there.
4. **The floor's 4K leg as sampled**: `ref_scene.rs:10`–`14` holds the 4K set
   resident-unsampled per its module docs; whether the harness binds paint tiles since
   the viewport-texture kill needs a runtime read. As written, the "4K texture set"
   budget may not include 4K paint-tile sampling.
5. **Vendor-GPU probe rows** (un-premultiply exactness, eframe 0.36 error-handler
   behavior): probe-shaped, no vendor hardware in CI.
6. **True multi-tile UDIM stroke throughput at 4K×N tiles**: pool_perf covers drag and
   mirror sessions; the soak and ref_scene are single-tile-shaped.

---

## 5. Robustness

**Never-crash doctrine (rule 11), verified at the deny sites**: `umber-core/src/lib.rs:20`,
`umber-color/src/lib.rs:22`, `spp/src/lib.rs:53` — all three carry
`cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))`.
The engine crates' discipline holds in the exempt crates too (e.g. crash.rs itself uses
`lock().unwrap_or_else(|p| p.into_inner())` — a poisoned lock still holds a valid Vec).
The queue's row 13 records the deny-gate as probe-proven: a planted unwrap lints.

**Crash reporter** (`umber-app/src/crash.rs`): `install` chains `std::panic::set_hook`
in front of the default hook; one report per panic into `<state dir>/crashes/`
(message, location, thread, forced backtrace, build facts — the same facts
Help ▸ About copies). `pending`/`acknowledge` offer a report once (`.seen` rename)
and keep it on disk for attaching. `note_op` keeps a `RECENT_OPS = 10` ring (`:32`) of
fixed code labels — "it is `&'static str` so user text cannot reach it" — so a report
is safe to paste publicly. The hook must never panic or block; every step is fallible
best-effort with a stderr fallback.

**Fuzz corpus**:
- **Mesh panic hunt** (`umber-mesh/tests/panic_hunt_mesh.rs`, 33 test fns): deterministic
  xorshift noise plus mutated-valid inputs (truncation, strided byte flips, zeroed
  ranges, targeted text edits — huge counts, negative indices, NaN) through the six
  importers and the `load()` dispatcher (which catches importer panics). Contract:
  every case returns, never panics — `catch_unwind` names the seed and mutation so a
  failure is reproducible; a success on pure noise is COUNTED and printed (`FLAGGED`),
  not celebrated; discovered bugs are pinned as `#[ignore = "BUG: …"]` reproducers;
  per-test temp dirs, never shared (parallel tests race). SECURITY.md's "5,400+ cases"
  is structurally consistent with the count-parameterized loops but not independently
  recounted here. The hunt has real yield on record: it found the usdz arm had never
  loaded a valid file until `6c68ce7` (requirements §7 row).
- **Four-seam hunt** (`umber-app/tests/panic_hunt.rs`, 10 test fns): 850 deterministic
  garbage + mutated-valid cases through the four public seams — mtlx, psd, spp, the
  project loader (`docs/loop/queue.md:71`), landed `fc4ba4c`.

**Memory caps** (the full inventory, all live):

| Cap | Value | Site |
|---|---|---|
| `MAX_SPP_FILE_BYTES` | 4 GiB | spp/src/lib.rs:99 |
| `MAX_DATASET_BYTES` | 256 MiB | spp/src/hdf5.rs:39 |
| `MAX_CHANNEL_BYTES` | 256 MiB | spp/src/texture_cache.rs:47 |
| `MAX_PSD_BYTES` / `PSD_DECODE_BUDGET` / `MAX_PSD_FILE_BYTES` | 256 MiB / 1 GiB / 2 GiB | umber-app/src/psd_import.rs:120–126 |
| `MAX_EXR_BYTES` | 2 GiB | umber-export/src/exr.rs:108 |
| `MAX_PRESET_BYTES` / `MAX_PRESET_LAYERS` | 1 MiB / 256 | umber-core/src/smart.rs:148/150 |
| `MAX_NESTING` (USD prim/value depth) | 256 | umber-mesh/src/usd.rs:351 |
| plane pool soft/hard/retired/history | 2 GiB / 4 GiB / 512 MiB / 64-entries-256 MiB | umber-paint/src/paint_state.rs:682/688/1050/591–592 |
| session/agent history windows | 256 entries | umber-app/src/document.rs:72, umber-cli/src/agent.rs:533 |
| input-to-photon budget | 20 ms | umber-app/src/input_photon.rs:72 |
| viewport P95 budget | 16.7 ms | umber-gpu/src/ref_scene.rs:63 |
| crash-report op ring | 10 | umber-app/src/crash.rs:32 |

**SECURITY.md currency**: pre-release posture, and the threat-surface rows carry
"IN HEAD (ddb59e0)" provenance markers that are ~a wave stale (HEAD is `0ec64bd`) —
the markers lag, but the row claims sampled live: the USD nesting cap is real at
`usd.rs:351` (MAX_NESTING = 256 with `enter_or_refuse` at `:409`–`:414`), the panic
hunt is real and growing (33 mesh test fns + the 10-fn four-seam hunt). Treat the
markers as provenance, not currency; the rows' content spot-checks current.

**Structural**: layering gate GREEN, no upward deps (§1); typed errors at every GPU
and paint boundary (§3); refusals instead of panics at every budget (§2).

---

## 6. The named gaps

Collected, with status as of this audit:

1. **Split redo** — requirements.md:167 (open follow-up).
2. **Merge-below-top** — retouch.rs module docs; top-entry-only is the v1 boundary.
3. **L2b same-class arms** — four set-scale density readers (symmetry snapshot, clone
   offset, fill raster, seam mirror radius); mapped in `306a502`, not yet migrated.
4. **Unified timeline (W-C4 L6)** — graph_panel/history.rs:28–32; needs main.rs + the
   History panel. Open.
5. **L3b multi-entry ID bake cache** — `6430374`; multi-tile meshes skip the pre-warm.
6. **L4 winink** — `stylus/src/lib.rs:95`–`118` returns None on Windows today
   (passive); research + vendored artifacts landed
   (`docs/claw-artifacts/eframe-0.36-rwh-surface.md`,
   `windows-0.62-subclass-surface.md`); the bytes are not written.
7. **L5 remainder** — the OccluderScope panel row + i18n keys: 0 references in
   umber-app/src, 0 keys in `assets/i18n/en-US.ftl` (verified 10-24).
8. **Cross-tile stroke continuity** — route_tile resets the conditioner per tile (v1).
9. **Angle/alignment/jitter quartet** — no engine params, no panel rows (requirements
   §1/§11, both honestly marked).
10. **The perf unmeasured six** — §4's list; headlined by the absent 3060-class floor
    sample and the resident-unsampled 4K harness leg.
11. **The P2 moonshots still actually open** — dynamic material layering (§2),
    displacement/tessellation preview (§5), custom WGSL user shaders (§5; the wasm
    host exists, the shader-replacement hook does not). PSD export and the AI provider
    *interface* are built — see surprises.

---

## 7. The surprises (docs-claims the live tree refutes)

1. **requirements.md:87 (§5 perf floor) still reads "NOT REACHABLE: perf/baseline.json
   absent"** — but `perf/baseline.json` exists, reseeded 2026-10-19 (`c53a08d`/
   `b1d3660`), and the nightly consumes it nightly. The row was never re-marked after
   the reseed; the doc now understates the tree.
2. **AGENTS.md's remaining-P2 list names "§6 PSD export"** — the tree has it:
   `umber-export/src/psd.rs` with `ag-psd = "0.3.0"`, and requirements §6 carries
   "✓ built (`40909ea` ag-psd round trip)". The same list's "§10 AI-texturing provider
   interface" is also built to its stated v1 boundary (the interface; delivery is
   post-0.1 by that row's own text). The agent contract's pointer list is a wave stale
   on two of its five rows.
3. **tech-stack.md version rows drifted**: openusd "**0.7**" (`:27`) vs live 0.8.0;
   wasmtime "**49**" (`:42`) vs live `=50.0.0-rc.1`; the workspace-shape sketch lists
   `xtask/` (`:75`) which does not exist (scripts/ + .github/ carry those duties).
4. **The 10-17 audit's numbers are a full wave stale**: 166,540 LOC → 186,296 (+12% in
   seven days), and its five no-test files are four (snapshot.rs graduated to 7 tests).
   The growth is the W-C3b sweep + W-C4, not drift.
5. **`ref_scene.rs`'s module doc claims "the viewport mesh pass has no texture binding
   yet (`Vertex` drops UVs…)"** — stale since the viewport-texture kill: the mesh pass
   samples the bound paint tile today (requirements §5 records the closure). The
   harness's 4K set is resident-unsampled as written, which quietly weakens what the
   floor harness measures (§4, honest list item 4).
6. **The "viewport mean halved" leg** (protocol.md:401) is fleet drift, not code:
   GitHub's ubuntu-24.04 fleet halved CPU throughput between 10-17 and 10-19 on a
   byte-identical image, and the four-leg bisect convicted the fleet, not the tree. A
   reader skimming the leg log could misread it as a regression hunt that found code
   guilt — it found innocence, and the median-of-3/5 gate now exists so a single
   fleet hiccup can never carry a verdict again.

---

*Method: read-only. Counts via python/grep over the live tree at HEAD `0ec64bd`;
`scripts/check-layers.py` (pure python, cargo-free) run once; no builds, no tests
executed, no file modified except this report. Every file:line cite was re-derived
against the tree on 2026-10-24.*
