# Kill report — per-layer pool stage 1b: the storage primitives

Date: 2026-10-14. Lock `pool-stage1b` (pre-registered; `kill-locks.sh`
not called per the brief). MAIN tree. NO cargo, NO commits. Surface: the
4 locked files only — `crates/umber-app/src/paint_state.rs`,
`crates/umber-gpu/src/paint_thread.rs`, `crates/umber-gpu/src/paint.rs`,
this report.

Read before writing: the design doc §1 in full (§1.1–§1.5), §4.1's read
contract, §8's stage-1b rows (T14, T33), `paint_state.rs` in full, both
`paint_thread.rs` and `paint.rs`, and `compositor.rs` (stage 1a's landed
`composite_stack_region` + `CompositeRect`). Open verifications resolved
by source reads (below); the wgpu 30.0.1 + egui-wgpu 0.36.2 registry
sources were read, not assumed.

## 1. Built

**`crates/umber-app/src/paint_state.rs`** (only app file):

- `PlaneOwner` (`Base | Layer(u64)`, §1.1, with the exact derives incl.
  `Ord`/`Hash` for the map key), `Plane { thread, committed }` with a
  `committed()` reader, `PaintRefusal::PlaneBudget { bytes }` ([M4], the
  `Layer` arm lands in stage 3), `PLANE_BUDGET_BYTES = 2 GiB` (advisory)
  + `PLANE_HARD_CAP_BYTES = 4 GiB` (enforced default), and `DirtyRegion`
  (`Rects(Vec<DirtyRect>) | Full`, cap 8, touch-union + closest-pair
  merge, `Full` absorbs — [M8], no `smallvec`).
- Storage beside `TileTargets`: `planes: BTreeMap<(PlaneOwner, u16,
  String), Plane>`, `cache: BTreeMap<(u16, String), PaintThread>`,
  `cache_dirty: BTreeMap<(u16, String), DirtyRegion>`, `compositor:
  Arc<PaintCompositor>` (built once in `with_target_size`, handed to
  every thread), `plane_hard_cap_bytes: u64` (defaults to the hard-cap
  constant; `set_plane_hard_cap_bytes` lowers it for tests).
- `materialize_plane(owner, tile, channel)` — the one budget checkpoint:
  exists → `Ok(false)` (never re-checked); else prospective bytes (plane
  + cache texture when a layer-owned plane would also birth its cache) vs
  the cap → `Err(PlaneBudget { bytes })`; else OOM-scoped creation
  (`push_oom_scope` / `await_oom_scope`), OOM → the same refusal.
- `plane_bytes()` — exactly §1.3's formula: Σ `w·h·4` over materialized
  planes (`targets` Base threads + `planes` + `cache`) plus Σ `w·h` over
  threads holding a GPU quick-mask copy.
- `read_plane(owner, tile, channel)` — the only PLANE accessor: Base
  delegates to the drain-first `read_target`; layer planes drain + read
  back the same way. Reads never materialize (`None` for never-written).
- `publish_plane(owner, tile, channel, w, h, rgba)` — §5.1's fill shape
  with an owner: dims/byte-count validated against
  `tile_channel_size` (torn uploads dropped with a log, the `publish_fill`
  precedent), `materialize_plane` (refusals propagate), one full-plane
  `WriteRegion`, `committed = None`, `(tile, channel)` marked `Full`.
- `flush_composite(&mut self)` — a no-op: early-returns on empty dirt
  (the stage-4 fire condition); the snapshot argument, end-of-update
  placement and region dispatches land in stage 4, NOT here.
- `ensure_tile` / `ensure_channel` now build through
  `PaintThread::with_compositor` with the shared `Arc` (infallible —
  docs updated); every other stroke/read/resize path is byte-identical by
  construction (additive diff, no existing line re-routed).
- Tests added: 7 (4 CPU-only + 2 GPU-gated + 1 constants).
  - CPU `plane_budget_constants_are_pinned`: 2 GiB / 4 GiB exact.
  - CPU `dirty_region_unions_touching_rects`: edge-adjacent and
    overlapping unions, exact rects.
  - CPU `dirty_region_keeps_disjoint_rects`: 2 rects stay 2, not `Full`.
  - CPU `dirty_region_merges_closest_pair_past_eight`: 8× 10×10 at
    pitch 20 + 1 far rect → 8 entries, merged `(0,0)-(30,10)` present
    (first-minimal-pair tie-break, strict `<`), far rect intact.
  - §8 T14 `lazy_planes_and_accounting`: fresh 512² →
    `plane_bytes() == 1,048,576`, empty cache, `Layer(2)` reads `None`;
    `publish_plane(Layer(1), 1001, baseColor, …)` → `3,145,728`, cache
    born with exactly `(1001, baseColor)`, dirt `Full`, flush is a no-op
    (bytes + dirt unchanged), plane reads back the published bytes
    exactly, Base plane == composite reader byte-for-byte, Base all
    zeros.
  - §8 T33 `budget_refusal_names_bytes`: cap at
    `plane_bytes() + 1,048,576 − 1`, `publish_plane(Base, 1001, height,
    …)` → `Err(PlaneBudget { bytes: plane_bytes() + 1,048,576 })`
    exactly, nothing created (`channels_present` empty, bytes
    unchanged); a second state proves writes into an EXISTING plane
    still succeed under a lowered cap.

**`crates/umber-gpu/src/paint_thread.rs`** ([M4] seam):

- `PaintThread::with_compositor(device, queue, w, h,
  Arc<PaintCompositor>) -> Self` (infallible); `new()` wraps its own
  fresh compositor in an `Arc` and delegates, so every existing caller
  and test is untouched. The `compositor` field is now the shared `Arc`.
- `push_oom_scope` / `await_oom_scope`: the §1.3 OOM scope. Verified
  shape (wgpu 30.0.1 source): `push_error_scope(OutOfMemory)` returns a
  `#[must_use]` `!Send + !Sync` guard; `pop` is a future. The driver is
  std-only (`std::task::Wake` over an `Arc` — no unsafe, no executor;
  `pollster` is optional in umber-gpu and dev-only in umber-app, so
  neither can run here): each `Pending` pumps the device with a blocking
  poll. Pump failure logs and reports no-error (refusing on no
  observation would block painting on a transient).

**`crates/umber-gpu/src/paint.rs`**: 1 test,
`paint_compositor_is_send_sync_for_pool_sharing`, pinning the open
verification's outcome (`Device` is asserted `Send + Sync` in the wgpu
source; the rest of the fields are the same Arc-backed handle family).

**This report**: `docs/loop/kill-reports/2026-10-14-pool-stage1b.md`.

## 2. Judgment calls

Every call below is also written into the module docs of the file that
carries it.

- **Base stays in `TileTargets` for this stage (additive, not the §1.4
  move).** Moving every stroke/read/resize path onto `planes`-Base is ~20
  touch points in a 5000-line file with no compiler available under this
  kill's NO-cargo protocol; the regression contract ("existing tests pass
  UNCHANGED") is then held by construction instead of by luck. `planes`
  holds layer-owned planes, `cache`/`cache_dirty` sit beside, and the
  move lands with stage 4's routing — the same kill that first stages
  into a non-Base plane. Consequence, stated in the docs: `read_plane`
  resolves Base through `targets`, and Base has no `committed` slot until
  the move (stage 2's save loop reads `committed` on layer planes only).
- **`committed` is `[u8; 32]`, not `blake3::Hash`.** `blake3` sits in
  `umber-core` only; naming the type in umber-app without the dep is the
  E0432 trap, and both the lock and §6.2's own dependency note put that
  dep in stage 2's surface. The bytes are identical (`Hash` is 32 bytes);
  stage 2 swaps the alias. This is the one reconciling deviation, per the
  brief's "reconcile to the doc" rule.
- **`materialize_plane` returns `Result<bool, PaintRefusal>`**, not the
  doc's `Result<&mut Plane, PaintRefusal>`: no single reference type
  covers both owners while Base lives outside `planes`. Stage 4 restores
  the doc shape with the move.
- **Torn `publish_plane` uploads return `Ok(())`, dropped with a log.**
  There is no refusal variant for caller error — only for budget — so
  the `publish_fill` precedent (warn + drop) applies; a size mismatch
  never materializes.
- **Cache dirt is marked `Full` on every `publish_plane`, including
  Base-only entries.** §4.2's writers-of-dirt list is unconditional, and
  the stage-1b flush consumes nothing, so the mark is pure record.
- **OOM pump failure reports no-error.** See the `await_oom_scope` docs:
  a failed observation must not refuse painting.
- **U1 answered for the framework path:** neither egui-wgpu 0.36.2 nor
  eframe 0.36 calls `on_uncaptured_error` (source-read). The scope is
  therefore load-bearing, not belt-and-braces.

## 3. Unverified

- **No compiler ran (brief forbids cargo).** Every signature, borrow and
  derive above is hand-checked against the surrounding code (ergonomics
  in `mark_rect`'s let-else, `Box::pin(scope.pop())` + `Pin<&mut F>`
  polling without `Unpin`, `is_none_or` availability, `too_many_arguments`
  count of 7 on `publish_plane`), but CI is the first real gate. Highest
  risks, in order: (a) the `await_oom_scope` future/poll shape against
  wgpu 30.0.1's exact `WasmNotSend` bound; (b) `PaintCompositor: Send +
  Sync` if any field (pipeline, layout, fallback mask view) is not.
- **No GPU ran.** T14/T33 skip without an adapter by design; the OOM
  refusal arm (real device OOM) is untestable here and untested. The
  `device.poll(Wait)` inside `await_oom_scope` blocks to idle on every
  plane materialization — rare (once per plane), but its latency is
  unmeasured; stage 4's perf gate (`docs/perf/protocol.md`) should watch
  it.
- **Existing-test byte-identity** is by construction (no live path
  re-routed; `with_compositor` shares the identical pipeline), not by
  execution.

## 4. Deferred

- The §1.4 move (`TileTargets` threads into `planes`-Base), the
  `CompositeSnapshot` argument + drain-then-composite flush, stroke
  routing into planes, and `PaintRefusal::Layer` — stages 3–4, each
  recorded at its hook (`flush_composite`, `materialize_plane`,
  `plane_exists` docs).
- `committed`'s `blake3::Hash` alias + the `blake3` dep — stage 2's
  surface per §6.2's note (§2 above).
- Base planes' `committed` slots — arrive with the move; stage 2's
  clean-skip test covers layer planes first.
- No requirements row is marked ✓ by this kill (storage lands no user
  row; §7 row 1b is the "Behavior after" proof, `PENDING` CI).

## 5. Gates run

- `bash scripts/verify-kill.sh crates/umber-app/src/paint_state.rs
  crates/umber-gpu/src/paint_thread.rs crates/umber-gpu/src/paint.rs` —
  **PASS** (balanced, fmt-clean — one fmt drift fixed in place by the
  script; the three thiserror `source`-field WARNs are named false
  positives: `clone_source` fields and wgpu `source:` struct literals).
  Lock audit confirms all three files under `pool-stage1b.lock`.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D
  warnings` — **NOT RUN**: the brief's standing protocol forbids cargo
  ("NO cargo. NO commits."). CI owns the first compile; §3 names the
  highest-risk shapes for its reader.
