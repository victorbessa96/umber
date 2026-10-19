# Per-Layer Pool — Design

Date: 2026-10-14 (doc-only kill, lock `perlayer-pool-doc`; r2 rewrite
under lock `pool-r2`).
Status: LANDED + CLOSED 2026-10-14 — all five stages landed
(1a+1b+2 `b1ceb2c`/`f2aff9f`, 3 selection + stroke gate `ed64a58`,
4 per-layer pixels `dd6e1cd`, 5 anchors + instances `ec6d552`), the
arc closed in `cfb205c`; the binding review artifact:
`docs/loop/artifacts/2026-10-14-perlayer-pool-review.json`. Stage
1a's kill report: `docs/loop/kill-reports/2026-10-14-pool-stage1a.md`.
(Status line corrected 2026-10-19 by the docs audit — it still read
READY TO IMPLEMENT five days after the landing.)

**Revision history**
- **r1 (DRAFT, pre-review).** The original design.
- **r2 (post-review, this text).** Folds the binding adversarial review
  (`docs/loop/artifacts/2026-10-14-perlayer-pool-review.json`: verdict
  NOT-READY, 3 BLOCKERs, 12 MAJORs, 13 MINORs) INTO the normative
  sections, following the dynmat r2 precedent: the corrections are
  applied in place, not appendixed. Each corrected passage carries its
  finding id in brackets (e.g. **[B1]**), so the diff reads as
  corrections. Where r2 departs from a binding fix's literal wording, the
  departure and its reason are in the deviation log at the end.

Closes, when built: the four v1 boundaries of dynamic material layering
(`docs/specs/dynamic-materials-design.md:414-437`, restated in
`crates/umber-app/src/material_layer.rs:20-28`), the stroke-gate
follow-up (`crates/umber-app/src/layers_panel.rs:42-49`), and the
per-layer-storage follow-up both prior pool docs deferred
(`dynamic-materials-design.md:147-153,795-807`;
`per-channel-tile-pool-design.md:134-139`).
Requirement rows touched: see the traceability table at the end.

## The problem, precisely

`PaintState` holds ONE flattened target per `(tile, channel)`:
`targets: BTreeMap<u16, TileTargets>` (`paint_state.rs:531`), where
`TileTargets { base_color, channels }` (`paint_state.rs:465-468`) never
carries a layer dimension. Every pixel producer writes into that one
surface:

- **Strokes** stage dabs into `route_tile(uv)`'s active-channel target
  (`paint_state.rs:2432-2435`). No layer is consulted: the stroke paints
  "the canvas", whatever layer the user thinks is selected.
- **Retouch** (smudge/clone/eraser) writes back into the tile's baseColor
  target (`paint_state.rs:2524-2536`).
- **Fills** read the flattened tile, composite over it and publish
  (`fill_layer.rs:383-431`, the read at `:414`, the publish at `:426`).
- **Materials** do the same per declared channel
  (`material_layer.rs:12-18`, wired at `main.rs:2958-3041`).
- **Mesh-map imports** **[M9]** push a new `LayerKind::Paint` layer named
  `"Imported <map>"` and store the PNG in the `AssetStore`, but never
  publish the bytes to the pool (`mesh_map_import.rs:296-345`). The
  imported map renders nothing today. (`spp_import.rs` and
  `psd_import.rs` make no `PaintState` call, so they are not pool
  writers.)

The dynamic-materials r2 review chose option (b): ship materials on the
fill precedent's destructive publish and defer per-layer storage
(`dynamic-materials-design.md:439-444`: "per-layer storage is a pool
redesign touching every paint, fill, save and binding path"). That
decision left four warts, each "fixed only by per-layer pool storage"
(`material_layer.rs:20`):

1. **Stack order is not preserved.** A material composites over
   whatever the tile holds when it refreshes, including layers above it.
2. **Re-eval compounds.** Every refresh composites again over its own
   previous output, so Multiply darkens each time (pinned at
   `dynamic-materials-design.md:664-667`: 170 → 144).
3. **Nothing un-publishes.** Hiding, disabling, deleting or a failed
   re-eval stops future publishes; the pixels already published stay.
4. **Anchors read the flattened channel**, which includes the material's
   own last output: self-feedback on every re-eval.

The same shape blocks the stroke gate. `check_paintable` exists and is
armed for materials (`crates/umber-core/src/layers.rs:1366-1376`), but
its own docs say no stroke path calls it (`layers.rs:1356-1360`).
Kill B shipped it as a row badge plus a helper (`paint_gate_allows`,
`layers_panel.rs:82-84`) because "wiring the gate INTO stroke begin waits
on a selected-layer seam that does not exist in v1"
(`layers_panel.rs:44-49`). There is nothing to select: a stroke has no
layer to land in.

### Three facts the design is built on (verified live, 2026-10-14)

**F1 — The compositor already exists and is unused by the app.**
`StackCompositor::composite_stack` (`umber-gpu/src/compositor.rs`) and
its CPU oracle `composite_stack_cpu` fold a `[GpuStackLayer]` /
`[CpuStackLayer]` bottom-to-top with the full 32-mode set and layer
masks. Outside umber-gpu the only consumers are
`tests/gate0_journey.rs:242-247`, which composites one layer, and
`material_layer.rs:76`, which uses the per-pixel primitive. Its module
docs already assume per-layer storage ("the tile pool owns one GPU target
per (layer, channel, tile)", `compositor.rs:3-14`). This design decides
WHEN it runs and WHAT it reads; it does not invent a compositor. (Line
numbers inside `compositor.rs` below `:90` shifted when stage 1a landed;
cite its module-docs judgment numbers, which are stable.)

**F2 — `umber_gpu::TilePool` is NOT the storage this design uses.**
It is keyed `(layer, channel: u8, tile: TileId)` (`tile_pool.rs:129-139`).
The `u8` channel is the `ChannelKind` tag (Color/Data, `:142-145`), so
`roughness`, `metallic` and `height` (all Data) collide on one key. Its
`TileId` is a 512² grid cell, not a UDIM tile (`tile_pool.rs:34-44,85`).
It has no consumer outside its own tests. Its memory model (`:46-70`,
"10-layer × 5-channel × 4K = 3.2 GiB") and its advisory cap
(`RECOMMENDED_MAX_RESIDENT_BYTES`, `:95`) ARE reused in §1.3's budgeting.

**F3 — The flat pool mixes three alpha conventions today.**
- Strokes are **premultiplied**: the dab splat computes `src =
  dab.color * coverage; out = src + dst * (1 - src.a)`
  (`umber-gpu/src/shaders.rs:584-587`), and `Dab.color` is premultiplied
  (`paint.rs:43`).
- Fills are **straight**: `evaluate_fill_tile` writes the sample with its
  own alpha and carries coverage as a separate plane
  (`umber-mesh/src/fill.rs:690-694`). The non-fast path divides by
  `oa` (`fill_layer.rs:364`). The fast path publishes the sampled bytes
  and DROPS fractional coverage (`fill_layer.rs:413-426`).
- Materials go through `composite_pixel_masked`'s straight-alpha lerp
  (`material_layer.rs:496-540`).
- The **eraser** **[M1]** defaults to alpha-only: `EraseChannels::default()`
  is `a: true`, rgb off, and an enabled channel is scaled by `k = 1 −
  cov` (`umber-brush/src/retouch.rs:75-108,530-545`). On a premultiplied
  texel that scales `a` and leaves `p`, producing `p > a`.

The compositor's legacy path treats every source as STRAIGHT alpha:
`cov = src_a · opacity · mask; out_rgb = base + (blend − base)·cov`.
It starts from a transparent accumulator, and `blend_channel` clamps its
source to 0..1 (`layers.rs:343`). Feeding a premultiplied stroke plane
through it unchanged double-multiplies soft edges. For a texel `P = 100,
A = 128` over transparent: `(100/255)·(128/255) = 0.196847 → 50`, where
today's flat target shows 100. Stage 1a resolved this in the compositor
(§4.3); the alpha invariant every plane WRITER must keep is §3's.

## 1. Storage: planes keyed (owner, tile, channel)

### 1.1 The key

```rust
/// Who owns a plane. Layer ids start at 0 (`LayerStack::new`,
/// layers.rs:905-911), so no id value can double as "no layer":
/// the ownerless plane is its own variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaneOwner {
    /// The session's ownerless bottom plane (§1.4). Selectable (§2).
    Base,
    /// A document layer's own pixels.
    Layer(u64),
}

/// NEW surface in `paint_state.rs`, beside `TileTargets`.
struct PlaneKey {
    owner: PlaneOwner,
    tile: u16,
    channel: String,
}

/// One GPU plane per (owner, tile, channel), materialized lazily.
planes: BTreeMap<(PlaneOwner, u16, String), Plane>,

struct Plane {
    thread: PaintThread,
    /// [B1] `Some((dir, hash))` = this plane's bytes were last committed to
    /// (saved into, or loaded from) the project at `dir`, whose sidecar
    /// file hashed to `hash` (blake3 of the PNG bytes). EVERY write
    /// (stage, WriteRegion, publish, Resize) sets it to `None`. The save
    /// may skip this plane's readback only under §6.2's two-part test.
    committed: Option<(PathBuf, blake3::Hash)>,
}
```

Each plane is a `PaintThread` (`umber_gpu::paint_thread`), the same
type every target is today. That keeps strokes, `WriteRegion`,
`WriteMask` and `Resize` unchanged per plane.

**[M4] One shared compositor.** Every `PaintThread` builds its own
`PaintCompositor` today (`paint_thread.rs:175`; `paint.rs:627-640`:
shader module, bind-group layout, pipeline and a 1×1 fallback). Per plane
that is a shader compile on a stroke's FIRST batch on a new layer/tile:
a mid-stroke hitch. NEW surface in umber-gpu (stage 1b):
`PaintThread::with_compositor(.., Arc<PaintCompositor>)`. `PaintState`
builds ONE compositor at `new` and hands its `Arc` to every plane and
cache thread, so materializing a plane never compiles a shader.

Today's `TileTargets` threads MOVE into `planes` as the `Base` owner's
planes (§1.4). Tile 1001's Base baseColor is still created in
`PaintState::new`, so `paint_target()`'s invariant
(`paint_state.rs:1873-1880`) holds. `TileTargets` keeps its
`base_color` + `channels` shape with a new role: the **composite cache**
(§4), populated LAZILY (§4.2's lazy cache rule).

**[B3] The reader census.** The accessors `tile_target`,
`channel_target`, `paint_target`, `read_tile` and `read_channel_tile`
become COMPOSITE accessors: they resolve "cache if materialized, else
the Base plane". The NEW `read_plane(owner, tile, channel)` is the only
PLANE accessor. Every reader in the tree is in exactly one class, and a
plane reader NEVER calls a composite accessor:

| Class | Reader | Reads |
|---|---|---|
| COMPOSITE | `viewport.rs:256` (eyedropper) | cache-else-Base, flush-first (§4.1 part 3) |
| COMPOSITE | `viewport.rs:550,558` (`bind_paint_target`, baseColor + height) | cache-else-Base texture, bound for render |
| COMPOSITE | `uv_view.rs:351` (UV canvas) | cache-else-Base texture, bound for render |
| COMPOSITE | `bake_sources.rs:82,99` (`painted_base_color` / `_tile`) | cache-else-Base, flush-first (signature moves to `&mut PaintState`, stage 4) |
| COMPOSITE | `main.rs:1006` (painted-map export) | cache-else-Base, flush-first |
| COMPOSITE | retouch's SAMPLE source for smudge/clone (§3, NEW at stage 4) **[M3]** | cache-else-Base, flush-first |
| PLANE | `main.rs:1948-1952` (the `save_paint_tiles` loop) | `read_plane(owner, …)` for EVERY owner, Base included; lands on `read_plane` at stage 2, day one |
| PLANE | `paint_state.rs:2473` (`capture_paint_event` via `read_target`) | the stroke owner's plane |
| PLANE | `paint_state.rs:2510` (retouch's CPU mirror) | the stroke owner's plane |
| PLANE | `main.rs:652` (`PaintMaterialIo::read_channel`) | RETIRED for publish at stage 4 (§5.2); the anchor path is §5.3's prefix composite over `read_plane` |
| PLANE (accounting) | `main.rs:3120` (Perf HUD) | `plane_bytes()` (§1.3), not `paint_target().byte_len()` (stage 4) |
| RETIRED | `fill_layer.rs:414` (the refresh's `read_tile`) | removed with `composite_fill_over` (§5.1) |

Why this matters: if the save loop stayed on `read_tile`, then once a
cache exists for `(1001, baseColor)` the save would write the COMPOSITE
into `paint/tile-1001.png` AND the layer planes under `paint/layers/…`.
Every reopen would composite every layer over a Base that already
contains it, adding one copy per save/reopen cycle. §8 T21 pins this.

JUDGMENT CALL — flat `(owner, tile, channel)` key over nesting planes
inside `TileTargets`. The per-channel design nested channels under tiles
so `targets.get(&tile)` stayed one hop (`per-channel-tile-pool-design.md:
56-60`). Planes are walked by owner just as often as by tile:
- delete or retire a layer (§5.4);
- census for save (§6);
- build composite entries for one `(tile, channel)` (§4).

A `BTreeMap` keyed owner-first makes `range((Layer(id), ..))` the
per-layer walk. The per-`(tile, channel)` walk is a filter over a
handful of entries (layers × routed channels per tile, §1.3's numbers),
which is cheap.

JUDGMENT CALL — no texture-set component in the key. The document holds
ONE layer stack (`document.rs:20-24`, "one texture-set layer stack"), so
layer ids are unique in the session, and a tile's set is already
`set_of_tile(tile)` (`paint_state.rs:905`). `layer_masks` makes the same
assumption (`paint_state.rs:660`, keyed `(layer, tile)`). Ids are NOT
unique across documents (every `LayerStack` starts at 0), which is why
§6.3's session lifecycle clears every layer-owned plane on open/new
**[M10]**. When per-set stacks land (multi-set documents), the set joins
BOTH keys in one change. That is recorded as a v2 boundary (§9), not
designed here.

### 1.2 Lazy materialization

The per-channel pool's rule extends one level
(`per-channel-tile-pool-design.md:72-80`): **a plane exists only once
something writes it.**

- A stroke materializes `(stroke_owner, tile, stroke_channel)` on its
  first staged batch on that tile. This is `ensure_channel`'s shape
  (`paint_state.rs:2154`), keyed by owner.
- A fill materializes `(Layer(id), tile, "baseColor")` per present tile
  on its first refresh.
- A material materializes `(Layer(id), tile, channel)` per present tile
  and per declared, enabled channel on its first refresh.
- A mesh-map import materializes `(Layer(id), tile, channel)` for its
  one routed channel (§5.6).
- The load path materializes exactly the planes the sidecar lists (§6),
  AFTER §6.3's session-lifecycle clear. Each loaded plane starts
  `committed = Some((opened_dir, blake3(png_bytes)))` **[B1, U3]**, so an
  in-place save right after open re-stages it without a readback.
- A layer that was never written has NO planes. The composite treats a
  missing plane as `None`, i.e. a transparent no-op under every mode
  (compositor judgment 2), so "reads composite around it" holds by
  construction.
- Size: a plane is created at `tile_channel_size(tile, channel)`
  (`paint_state.rs:899`), so an override applies per plane exactly as it
  does to targets today.

**[M4] Materialization is the one budget checkpoint.** Every path above
goes through one NEW `PaintState::materialize_plane(key) ->
Result<&mut Plane, PaintRefusal>`. It refuses when the new plane would
push `plane_bytes()` past the hard cap (§1.3), or when wgpu reports
`OutOfMemory` while creating it (§1.3). An existing plane never goes
through the check again, so it keeps painting.

Reads never materialize, as the existing rule requires: `read_channel_tile`
never side-effect-creates (`paint_state.rs:2620-2627`). The new plane
reader `read_plane(owner, tile, channel) -> Option<RgbaBuffer>` keeps
that contract.

### 1.3 The memory story

**[M4] Recounted.** One RGBA8 plane at the 4096 default is `4096² × 4 =
67,108,864 B = 64 MiB` (2048: 16 MiB; 8192: 256 MiB). Today one tile
with 5 routed channels holds `5 × 64 = 320 MiB` of targets
(`paint_state.rs:113-118`). Under this design those targets BECOME the
Base planes (§1.4), and the cache is ADDITIONAL. Under the lazy cache
rule (§4.2) there is one cache texture per `(tile, channel)` that has at
least one layer-owned plane. Worked cases at 4K, 5 Base channels:

| Session | Base | Layer planes | Cache | Total |
|---|---|---|---|---|
| Today (flat only) | 320 MiB | 0 | 0 | 320 MiB |
| 6 paint layers × 2 channels, 1 tile | 320 MiB | 12 × 64 = 768 MiB | 2 × 64 = 128 MiB | 1,216 MiB |
| + 2 materials × 4 declared channels | 320 MiB | 20 × 64 = 1,280 MiB | ≤ 5 × 64 = 320 MiB | ≤ 1,920 MiB |
| Same, 4 UDIM tiles (every layer on every tile) | 1,280 MiB | 80 × 64 = 5,120 MiB | 4 × 320 = 1,280 MiB | ≤ 7,680 MiB |

Line items on top of the table:
- **Quick-mask copies.** Every `PaintTarget` lazily owns a full-size
  `R8` quick-mask texture on its first `WriteMask`
  (`paint.rs:247-250,529-545`): +16 MiB per quick-mask-gated plane at
  4K. Under §3 that is per plane a gated stroke touches.
- **Anchor scratch** (§5.3): transient CPU RAM during a refresh,
  `L_prefix + 1` read-back planes per `(anchor, tile)` (64 MiB each at
  4K), freed when the material's eval returns.
- **Retired CPU RAM** (§5.4): up to `RETIRED_BUDGET_BYTES = 512 MiB`,
  paint-owned planes only. One newest tombstone may exceed it.

The dominant cost is fills and materials, not strokes. Eval-owned planes
are DENSE: every present tile × every declared channel, because
`refresh_dirty_materials` walks `tiles()` (`material_layer.rs:740-741`).
A stroke plane appears only on tiles a stroke touched. This is the same
residency arithmetic `tile_pool.rs:50-58` already wrote down ("3.2 GiB"),
now attached to a real consumer.

**Accounting.** `PaintState::plane_bytes()` returns, exactly:
`Σ w·h·4` over materialized planes and cache textures, plus `Σ w·h` over
the planes holding a GPU quick-mask copy. The Perf HUD reports it
(`main.rs:3120`, stage 4).

**Budgeting: two constants.**
- `PLANE_BUDGET_BYTES = 2 GiB` (advisory): 32 planes at 4K or 128 at
  2K. Crossing it logs once and sets a status-line flag; the Texture
  Sets panel names it beside the size combo.
- `PLANE_HARD_CAP_BYTES = 4 GiB` (enforced, 2× the advisory). It is a
  `PaintState` field defaulting to the constant, so tests can lower it.
  A materialization that would cross it is REFUSED with a typed
  `PaintRefusal::PlaneBudget { bytes }`, where `bytes` is the total it
  would have reached.
- Plane creation is also wrapped in a wgpu `OutOfMemory` error scope.
  A captured error is the same refusal. The wgpu 30 error-scope API
  shape is unverified (§ Open verifications).

Where a refusal surfaces:
- **Stroke.** `stroke_refusal = Some(PaintRefusal::PlaneBudget { .. })`,
  shown on the status line. The refused event's dabs on that tile are
  dropped. The stroke continues on its already-materialized planes, so
  existing planes keep painting.
- **Fill/material refresh.** The layer's state records
  `budget_blocked: Some(bytes)`. The row badge shows it (the material
  badge slot; fills gain the same slot at stage 4), and the status line
  logs once. The layer stays dirty, and it retries only when
  `plane_bytes()` falls (a plane freed) or its params change. Retrying
  every frame would spam.
- **Load / import.** The plane is skipped, with a warn log and the
  status line naming it. This is the sidecar load's existing loud-skip
  shape (`main.rs:2280-2290`).

JUDGMENT CALL (reversed in r2, **[M4]**) — enforce at MATERIALIZATION, not
at stroke time and not "never". The r1 text made the budget advisory
because "refusing is a silent data-path failure the never-crash doctrine
forbids". The real alternative was worse. wgpu texture creation does not
return `Result`, the tree installs no uncaptured-error handler (`grep
on_uncaptured_error|push_error_scope` over `crates/`: zero hits), and the
4-tile row (7.5 GiB) exceeds the 8 GB RX 6600 reference card
(`requirements.md:166`) once the framebuffer, mesh and env maps are
counted. That is a crash, which the doctrine forbids most. A typed, loud
refusal of a NEW plane is not a silent drop: it names the bytes and
leaves every existing plane writable. The 4-tile row cannot fully
materialize under the default cap. The refusal names it, and virtual
texturing (§9.1) is the real fix. Revisit both numbers against
`docs/perf/protocol.md` measurements at stage 4, not before.

`LayerStack::memory_bytes` stays metadata-only (`layers.rs:1378-1389`,
the honesty contract). Plane bytes are reported by `PaintState`, never
by `Layer`, as the material eval cache already is
(`material_layer.rs:39-41`).

### 1.4 The Base plane

`PlaneOwner::Base` is the session's ownerless plane. It composites at the
very bottom: Normal, opacity 1, unmasked, visible unless the user hides
its row (§2.2). It holds:

1. **Strokes whose active target is Base.** That means the empty-stack
   document (today's fresh-session behavior), a stack with no paintable
   layer (§2.3, **[M12]**), an explicit selection of the Base row
   (§2.2, **[M3]**), and the headless callers that drive `PaintState`
   with no document at all: `bake_sources.rs:637,827`,
   `input_photon.rs:269` and `perf_soak.rs:82`. Each keeps painting
   byte-identically. (`main.rs:3386` is test code under `cfg(test)` at
   `main.rs:3273`, not a production caller **[m13]**.)
2. **Migrated v1 paint** (§6.3): a v1 project's flattened tiles load
   into Base, unchanged.

Base is what makes stage 1 a pure refactor. Before any layer-owned plane
exists, Base IS today's `TileTargets` content, and readers read it
directly (the lazy cache rule, §4.2).

**Base is exempt from §3's `p ≤ a` invariant** (advisor ruling on
**[M1]**). It holds v1's mixed-convention bytes (F3), which is why the
bottom-copy rule exists (§4.3 item 2). Every write into Base keeps
TODAY's semantics. That includes the eraser's `EraseParams` as defined
(alpha-only by default), so "Base is today's path" stays literally
true, and a v1 project erased in Base produces exactly today's bytes.

**Base's composite entry carries `premultiplied: true`.** The exemption
constrains Base's WRITERS, not its entry flag. Base composites as the
bottom, Normal, opacity 1, unmasked, premultiplied entry, so the
bottom-copy rule (§4.3 item 2) seeds it verbatim. A `premultiplied:
false` Base entry would send every Base texel through the straight path
the moment a cache materializes, which is F3's 50-vs-100 error on all
legacy paint.

## 2. The active-layer seam

### 2.1 Where the selection lives

```rust
// NEW fields on PaintState, beside mask_edit_layer (paint_state.rs:657-667)
/// [M3] The target new strokes paint. Base is first-class and selectable.
/// The Layers panel edits this through the shell; `begin_stroke`
/// snapshots it.
active_target: PlaneOwner,
/// [M12] `true` when the user chose `active_target` (row click, add
/// button, mask-row selection); `false` when the shell's revalidation
/// resolved it. Only an EXPLICIT unpaintable selection refuses (§2.3).
active_explicit: bool,
/// [m6] `active_target`'s stack index at the last sync, so a vanished id
/// can resolve to its former neighbour (§2.3).
active_index: usize,
/// The shell's last paintability verdict for `active_target`
/// (`LayerStack::check_paintable`; `Ok(())` for Base), pushed between
/// strokes.
active_verdict: Result<(), InstanceError>,
/// The live stroke's owner, snapshotted at `begin_stroke`.
stroke_owner: PlaneOwner,
/// Why the last `begin_stroke` (or a mid-stroke materialization) refused.
/// `None` after an accepted stroke.
stroke_refusal: Option<PaintRefusal>,
```

```rust
/// [M4] Every reason a stroke can be refused. One enum, so the status
/// line has one surface. No field is named `source` (thiserror trap).
pub enum PaintRefusal {
    /// The explicitly selected layer is unpaintable (`check_paintable`).
    Layer(InstanceError),
    /// Materializing a new plane would cross `PLANE_HARD_CAP_BYTES`, or
    /// wgpu reported `OutOfMemory` creating it (§1.3).
    PlaneBudget { bytes: u64 },
}
```

JUDGMENT CALL — `PaintState`, not `Document`. This generalizes
`mask_edit_layer` (`paint_state.rs:657-667`): selection is session state
that the Layers panel edits and `begin_stroke` snapshots
(`paint_state.rs:1008`). Three reasons rule out `Document`:
- `Document::run` would make it undoable and journaled, but a selection
  is not an edit.
- `JournalOp` (`journal.rs:48`) has no selection op, and should not get
  one.
- The stroke path that consumes it lives in `PaintState`, which has no
  `LayerStack`.

The selection is not persisted in v2. On open the shell auto-selects
(§2.3): the topmost paintable layer, else Base.

### 2.2 How the Layers panel sets it

`layers_ui(ui, doc, mats)` (`document.rs:277-281`) takes no `PaintState`,
and the material context borrows the shell's state mutably, so the
selection cannot be written from inside it. NEW surface:

```rust
/// What the Layers rows asked for this frame.
pub struct LayersUiOutcome {
    /// `Some(target)` = a row's name was clicked (select it). The Base
    /// pseudo-row yields `Some(PlaneOwner::Base)`.
    pub select: Option<PlaneOwner>,
    /// `Some(visible)` = the Base row's visibility was toggled (stage 4).
    pub base_visible: Option<bool>,
}
pub fn layers_ui(ui, doc, mats, active: PlaneOwner, base_visible: bool) -> LayersUiOutcome
```

- The row name becomes a `selectable_label(active == Layer(id), name)`.
  It is the same widget the visibility toggle already uses
  (`document.rs:355-357`).
- **[M3] The Base pseudo-row** is pinned at the bottom of the list,
  labelled "Base paint", and is selectable from stage 3. Its visibility
  toggle lands at stage 4, when there is a live composite for it to
  affect. It is session state: never in the layer JSON, never journaled.
  It is shown whenever the stack is non-empty or Base holds any plane.
  NEW strings `layers.base-paint` (label) and `layers.base-paint-hint`
  (tooltip: "paint that belongs to no layer: v1 projects, and strokes
  made with no layer selected"), in both ftl files at stage 3.
- The add buttons (`document.rs:285-302`) select the new layer
  explicitly, matching Painter's add-then-paint behavior. The new id is
  the stack top after `doc.run(LayerCommand::add(..))`.
- **[m5] Selection and mask editing reconcile.**
  `set_active_target(target)` clears `mask_edit_layer` unless it equals
  `target`'s layer id. Selecting a row's mask in `layer_mask_ui`
  (`document.rs:521`) also sets `active_target = Layer(that id)`,
  explicitly. Without this, a user editing L2's mask who adds a layer
  and paints would paint L2's MASK, because a set `mask_edit_layer` wins
  over everything (`paint_state.rs:2345-2352`).
- The shell applies the outcome through `paint.set_active_target(..)`,
  the `layer_mask_ui` precedent of mutating the paint session from a
  panel action (`document.rs:521`).
- A mesh-map import does not change an existing selection: it is not a
  row click (§5.6).

### 2.3 The verdict and the stroke gate

`check_paintable` (`layers.rs:1366-1376`) is the single paintability
truth. It gains two arms, keeping the match exhaustive by hand per its
own NOTE (`:1356-1360`):

```rust
LayerKind::Fill => Err(InstanceError::PaintOnFill { layer: id }),
LayerKind::Folder { .. } => Err(InstanceError::PaintOnFolder { layer: id }),
```

JUDGMENT CALL — fills and folders become unpaintable.
- A fill's plane is EVAL-owned: the next refresh overwrites it whole
  (§5.1), so a stroke there would be silently destroyed.
- A folder carries no pixels (compositor judgment 4).
- Painter refuses both. You paint a fill's MASK, which stays allowed
  (below).
- Field names are `layer`, never `source` (the thiserror trap, CLAUDE.md).

**Mechanism** (`PaintState` cannot call `check_paintable` itself):

1. Every frame, before panels run, the shell revalidates the selection
   (the fill-adoption precedent, `main.rs:2906-2924`):
   - **An explicit Base selection never auto-moves** **[M3]**. Base is
     always paintable.
   - **Auto-resolution** applies on open, on new, and whenever the
     target is a vanished `Layer(id)`. It picks the topmost PAINTABLE
     layer, else Base **[M12]**. It never picks a refusing layer, and its
     result has `active_explicit = false`. A stack holding only
     Fill/Material/Instance/Folder layers therefore paints into Base,
     exactly as it does today.
   - **A vanished `Layer(id)`** **[m6]** resolves against
     `active_index` (the id's index at the last sync). Let `i =
     min(active_index, len)`. It walks DOWN from `stack[i − 1]` (the
     layer that sat directly below) to the nearest paintable layer.
     The start is `i.checked_sub(1)`, and `None` (the vanished layer was
     the bottom) skips the down-walk, so no `usize` underflow is
     possible. Failing that, it walks UP from `stack[i]`, else Base. The shell holds no index once
     the id is gone, which is why `PaintState` keeps `active_index`.
     `prune_layer_masks` (`paint_state.rs:1370-1374`) just sets `None`
     and is not the precedent.
   - It then pushes `sync_active_target(target, verdict, index)`, where
     `verdict` is `doc.stack.check_paintable(id)`, or `Ok(())` for
     Base. `sync_tool`/`sync_symmetry` are the precedents
     (`paint_state.rs:1067,1097`).
2. `begin_stroke_styled` (`paint_state.rs:976`) decides in this order:
   - `mask_edit_layer.is_some()`: a layer-MASK stroke. The gate is
     bypassed, so painting a fill's or material's mask is legitimate.
     This matches the existing hook order (`paint_state.rs:2340-2352`).
   - Quick-mask edit mode: paints the session mask, gate bypassed.
   - Otherwise, `active_verdict` is `Err(e)`, which happens only on an
     EXPLICIT selection of an unpaintable layer **[M12]**: **refuse**.
     `stroking` stays `false`, `stroke_refusal =
     Some(PaintRefusal::Layer(e))`, nothing is staged, no capture opens.
     The shell renders `stroke_refusal` on the status line through the
     existing `layers.*` unpaintable hints. The user's way out is one
     click: select a paintable row or Base.
   - Otherwise accept, and `stroke_owner = active_target`. (Stage 3
     lands the gate with `stroke_owner` forced to `Base` for every
     accepted stroke; stage 4 switches on `Layer(id)` routing, §7.)
3. `replay_path`/`replay_filled_path` (`paint_state.rs:1764,1811`) begin
   strokes through the same function, so they inherit the gate.

The live stroke snapshots `stroke_owner` like `stroke_channel`
(`paint_state.rs:1012`): a mid-stroke selection change never splits a
stroke across two planes.

**Boundary (v2): owner deleted while stroking** **[m7]**. The verdict is
one frame old. Kinds never change, as no command mutates `kind`
(`dynamic-materials-design.md:243-249`). So the stale cases are a layer
deleted in the same frame a stroke begins on it, and a layer deleted
mid-stroke. Both are resolved the same way:
- The per-frame retire census (§5.4) SKIPS the live `stroke_owner`
  while `is_stroking()`, so the stroke's planes are never retired and
  then re-materialized under it.
- `end_stroke` on an owner that is no longer in the last snapshot
  retires that owner's planes immediately, and discards the stroke's
  undo capture with a log. There is no live layer to undo into.

Nothing crashes and no live layer is touched.

`paint_gate_allows` (`layers_panel.rs:82-84`) stays the row-badge helper.
This closes the follow-up Kill B recorded at `layers_panel.rs:44-49`.

## 3. Stroke routing: per-event (owner, tile, channel)

`mask_edit_layer` already routes strokes per layer:
`stroke_mask_layer` is snapshotted at begin (`paint_state.rs:1008`),
and each event stamps `(layer, tile)`'s plane (`paint_state.rs:2345-2352`
→ `layer_mask_event`, `:1381`). This design generalizes that to pixels:

| Dimension | Decided | Source |
|---|---|---|
| owner | once per stroke (`stroke_owner`) | §2.3 |
| tile | per event (`route_tile(uv)`; mirrors by their own UV) | `paint_state.rs:2251-2316`, unchanged |
| channel | once per stroke (`stroke_channel`) | `paint_state.rs:1012`, unchanged |

**[M1] The plane alpha invariant.** Every write into a LAYER-owned plane
preserves `p ≤ a` per channel (valid premultiplied data). Base is exempt
(§1.4). Each writer's obligation:
- the dab splat is premultiplied over by construction (F3);
- fills and materials write `(c·α, α)` (§5.1, §5.2);
- smudge (per-channel lerp of two valid texels, `retouch.rs:408-411`)
  preserves it;
- clone/smudge SAMPLES come from the cache (below). The cache can carry
  bottom-copied legacy Base texels with `p > a`, so a retouch write into
  a layer plane clamps `p` to `a` per channel. That is the same clamp
  stage 1a's `unpremultiply_texel` applies at composite time, so writing
  the clamped value changes no composited byte;
- the eraser follows the rule below.

Concretely, every place that resolves a target by `(tile, channel)`
takes the owner too:

- **Stage** (`paint_state.rs:2432-2435`):
  `plane_of_mut(stroke_owner, tile, &channel)` replaces
  `target_of_mut(tile, &channel)`. A first batch on a new
  `(owner, tile)` materializes through `materialize_plane` and may be
  refused (§1.2).
- **Undo capture** (`capture_paint_event`, `:2459-2499`): the snapshot
  reads the PLANE (a plane reader, §1.1). `PaintStrokeCapture.tiles` is
  keyed `(owner, tile)`. `StrokeTile` and `PaintWrites`
  (`retouch.rs:331-337,490-493`) gain an `owner` field. Undo/redo write
  into the plane, never the cache. The history envelope (64 entries /
  256 MiB, `paint_state.rs:455-456`) is unchanged: regions are rects of
  one plane, the same bytes as today.
- **Retouch** (`retouch_event`, `:2504-2539`) has two reads **[M3]**:
  - the CPU **mirror** is read from and written back to
    `(stroke_owner, tile, stroke_channel)`, a PLANE reader;
  - smudge and clone **sample** the COMPOSITE (cache-else-Base,
    flush-first), a COMPOSITE reader. That is one extra full-tile
    readback per stroke per tile, beside the mirror read retouch already
    pays (`paint_state.rs:2510`). It is not a new class of cost.

  This lifts today's baseColor-only retouch boundary (`:2525-2527`).

  JUDGMENT CALL (reversed in r2, **[M3]**) — smudge and clone sample the
  COMPOSITE and write the active plane. r1 sampled the layer's own plane,
  which regresses three P0 tools. Clone-to-new-layer, the standard
  workflow, would sample an empty plane and do nothing. Today both tools
  sample the visible canvas, and that stays the default. "Sample current
  layer only" is the follow-up toggle (§9). r1's cost argument ("needs a
  composite readback per stroke") is struck: the mirror read already
  pays one.
- **Eraser** **[M1]**: erases the active target's plane. On a
  premultiplied (layer-owned) plane:
  - `EraseChannels.a` scales ALL FOUR channels by `k = 1 − cov`: the
    premultiplied erase.
  - `EraseChannels.r/g/b` mean straight-space color removal. The texel
    is unpremultiplied, the enabled color channels are scaled by `k`, and
    the result is re-premultiplied. All of it happens inside
    `retouch_event`, on the mirror, before the `WriteRegion`.
  - On Base the eraser keeps today's `EraseParams` semantics verbatim
    (§1.4).
  - Requirement row 19 ("sets layer alpha to zero") becomes literally
    true without breaking the invariant: alpha goes to zero and so does
    the premultiplied color. §8 T22 pins `(200,0,0,255)` at 50% →
    `(100,0,0,128)`.
- **Quick-mask gate** (`sync_mask_tile`/`publish_mask`, `:2567-2607`):
  the GPU mask copy rides the thread the dab stages into, which is now
  the plane's. `mask_synced` is keyed `(owner, tile, channel)`. This
  closes the "non-baseColor channel's OWN GPU mask copy" follow-up noted
  at `paint_state.rs:2588-2591`. **[m9]** Every existing `mask_synced`
  reset is a stage-4 touch point:
  - `select_id_mask`'s `mask_synced.remove(&tile)`
    (`paint_state.rs:1435`) becomes
    `mask_synced.retain(|(_, t, _)| *t != tile)`;
  - the invert/clear/resize paths that empty it
    (`paint_state.rs:649-652`) stay `.clear()`.
- **Seam mirrors and symmetry** recurse through the same
  `push_event_styled`, so mirrors land in the SAME owner's plane on the
  mirrored tile and materialize it lazily.

**UDIM interaction** (`docs/specs/udim-design.md:74-82`). Per-event tile
routing is unchanged: a stroke dragging across a tile seam paints each
event into the event's tile, now inside the stroke's one layer. A layer
touched only on 1002 has planes only on 1002. Its composite on 1001
simply has no entry, which is the transparent no-op. Cross-tile stroke
continuity stays the UDIM design's named follow-up; per-layer storage
neither helps nor hurts it.

**Every stroke write marks the cache dirty** (§4.2): each dab footprint
of the event, the same `footprint` the undo capture already computes
(`umber_brush::retouch::footprint`), per `(tile, channel)`, merged into
the multi-rect `DirtyRegion` **[M8]**. A mirrored pair on opposite sides
of a tile stays two rects.

## 4. Composite strategy

### 4.1 The two options, costed

**Composite-on-write (CoW, today's shape).** The flat target is the
truth, and every writer folds its contribution into it at write time
(strokes splat in; fills/materials read-modify-publish). With per-layer
planes added, CoW would mean dual-writing each producer into its plane
AND into the flat target. That is only correct for the TOPMOST layer:
painting a lower layer cannot be folded into a flat result that already
contains the layers above it, because blending does not commute. CoW is
therefore not an option that preserves order. Its cost is what you pay
today, plus the warts.

**Composite-on-read (CoR, recommended).** Planes are the truth, and the
flat `TileTargets` become a CACHE. Writers mark `(tile, channel, rect)`
dirty. Before any reader can see the cache, the dirty regions are
recomposited from planes through `StackCompositor`.

| Event | CoW (today) | CoR (this design) |
|---|---|---|
| Stroke event | dab splat into the flat target (footprint texels) | dab splat into the plane + region recomposite: `L_c × Σ|R_i|` texel-ops (`L_c` = contributing entries, `R_i` = the ≤ 8 dirty rects) |
| Stroke event with symmetry / seam mirrors **[M8]** | splat at each mirror | each mirror's footprint is its OWN rect (≤ 8 per `(tile, channel)`, §4.2), so a pair at x≈100 and x≈3996 costs `2 × L_c × |R|`, not a near-full-width union. Past 8 rects the closest pair merges, so the cost degrades smoothly toward `Full`. |
| Fill/material refresh | BLOCKING full readback (64 MiB) + CPU composite of 16.8M texels + 64 MiB upload, per tile×channel (`fill_layer.rs:414-426`, `material_layer.rs:12-18`) | eval → 64 MiB upload into the OWN plane (no readback) + GPU recomposite `L_c × 16.8M` |
| Hide / opacity / blend / reorder / delete (one edit) | nothing happens (warts 1, 3) | `Full` for the changed layer's `(tile, channel)`s |
| Metadata DRAG **[M8]** (egui sliders emit one change per frame) | nothing happens | uncoalesced: `L_c × 16.8M × channels × tiles` per frame, plus every masked entry's mask upload. Coalesced (§4.2): at most ONE `Full` per DISPLAYED `(tile, channel)` per frame, i.e. `≤ 3 × L_c × 16.8M`; everything else stays dirty and composites on read |
| Masked layer in a recomposite | n/a (the app never composites, F1) | `composite_stack` uploads each masked entry's WHOLE mask per dispatch (by design: "nothing to sync", `paint_state.rs:286-290`), i.e. 16 MiB per masked layer at 4K. The region path (landed, §4.3 item 3) uploads only each rect's mask bytes: `|R|` bytes per masked layer per stroke frame |
| Save | 1 readback per cache target | 1 readback per plane failing §6.2's clean-skip test; the cache is never saved (§6) |
| Viewport / export / bake read | reads the flat target | reads the cache (composite readers, §1.1), fresh per §4.1's read contract |

At 4K a stroke frame's rect is small. A brush of r = 24 texels has a
49×49 footprint (`paint_state.rs:91-92`), and a 4-dab event union is on
the order of 150×50 = 7,500 texels. That puts a 6-layer recomposite at
about 45k texel-ops, against a full-tile 4K recomposite of `6 × 16.8M =
100M`. The region path is what keeps CoR inside the input-to-photon
budget (`requirements.md:164`, < 20 ms). **[M8]** The drag and mirror
rows are NOT proven inside the 16.7 ms frame by arithmetic. Stage 4's
perf gate measures them (§7), with the frame budget as pass/fail.

CoR also removes the composite-time readback. Today a non-Normal or
partial-opacity fill (`fill_layer.rs:413-414`; the Normal-at-1.0 fast
path already skips it) and every material channel
(`material_layer.rs:12-18`) block on a 64 MiB readback of the flattened
tile per tile × channel. Under CoR their planes never read the cache.
The readbacks that remain are an anchored material's prefix reads
(§5.3) and the retouch sample (§3).

JUDGMENT CALL — CoR, with an EAGER per-frame flush plus flush-on-read
for CPU readers. Three parts:

1. **The flush** **[M7]**. It runs at the END of the shell's update,
   AFTER the fill AND material refresh blocks (`main.rs:2906-3041`),
   not after the early drain (`main.rs:2894-2899`). The producers
   publish `WriteRegion`s into per-plane mpsc queues
   (`paint_thread.rs:150-160`), and the composite reads plane TEXTURES,
   so a queued-but-unprocessed command is invisible to it. The flush
   therefore:
   - first DRAINS (`process_pending`) every plane in the composite set
     of each `(tile, channel)` it is about to composite;
   - then composites.

   A stroke still shows the same frame it splats. Fills and materials
   now show the same frame they refresh, where r1's placement showed
   them one frame late. The flush and the plane threads share the
   session's queue (`paint_state.rs:527-528`), so queue submission order
   sequences "drained writes, then composite".
2. **GPU binders.** The viewport's `bind_paint_target`
   (`viewport.rs:533-564`) and the UV canvas (`uv_view.rs:351`) bind
   textures that the GPU samples at render time, after update returns.
   So they see the end-of-update flush.
3. **CPU readers that run DURING the frame flush first.** These are the
   eyedropper (`viewport.rs:256`), the painted-map export
   (`main.rs:1006`), the bake sources (`bake_sources.rs:82,99`), the
   retouch sample (§3) and `read_tile`/`read_channel_tile`. Following
   `read_target`'s drain-first precedent (`paint_state.rs:2637-2651`),
   the `&mut self` readers drain and composite the requested
   `(tile, channel)`'s pending dirt, `Full` included, against the LAST
   snapshot before reading back. This is also where §4.2's deferred
   (non-displayed) `Full` marks get paid. NEW surface: `paint_target()`
   and `tile_target()` are `&self` today (`paint_state.rs:1873,1885`),
   so the export and bake call sites move to a `&mut` flushing accessor.
   That changes `bake_sources.rs`'s `painted_base_color` /
   `painted_base_color_tile` from `Option<&PaintState>` to
   `Option<&mut PaintState>` (`bake_sources.rs:77-104`, **[m13]**).

**Boundary (v2).** Metadata edits are discovered when the shell diffs the
snapshot at the next frame's sync, so a reader running in the SAME frame
as a visibility, opacity, blend or reorder edit sees the cache as of the
previous snapshot. The read is one edit stale, never torn, and the next
frame is exact.

### 4.2 The cache and its dirt

- `cache_dirty: BTreeMap<(u16, String), DirtyRegion>`, where
  **[M8]** `DirtyRegion` is `Rects(Vec<DirtyRect>) | Full`, with the
  `Vec` capped at 8 (umber-app has no `smallvec` dependency; a plain
  capped `Vec` needs none).
  - A new rect that overlaps or touches an existing one unions into it.
  - Past 8 rects, the CLOSEST pair merges into its bounding rect.
    "Closest" means the pair whose union adds the fewest texels.
  - `Full` absorbs everything.
- **Writers of dirt** (exhaustive):
  - **Plane writes** (stage, retouch, undo/redo, publish) mark their
    rects on `(tile, channel)`.
  - **Layer-mask strokes** **[M6]**. `layer_mask_event`
    (`paint_state.rs:1381`) mutates a CPU mask Vec and returns before
    any staging, so no plane write and no snapshot diff can see it. It
    marks a `Rect` itself: the stamped footprint union, in SET-size
    texels. The rect goes onto EVERY `(tile, channel)` where that layer
    has a plane, AND onto every `(tile, channel)` where an Instance whose
    source is that layer contributes. A material's mask edit therefore
    dirties all its declared channels, not `stroke_channel`. On a
    channel whose size is overridden, the rect is scaled to the channel
    size, rounding outward.
  - **Metadata changes** mark `Full` for every `(tile, channel)` the
    layer has a plane on (a reorder marks both layers'). The metadata is
    order, visibility, opacity, blend, mask ENABLE / attach / detach,
    material channel params, and the Base row's visibility.
- **[M8] `Full` coalescing.** A metadata `Full` is composited by the
  per-frame flush only for the `(tile, channel)`s a GPU binder shows
  this frame:
  - the ACTIVE tile's baseColor and height (`viewport.rs:550-563`);
  - the UV canvas's bound channel when that panel is open
    (`uv_view.rs:351`).

  At most one `Full` recomposite runs per such `(tile, channel)` per
  frame, however many edits landed. Every other `(tile, channel)` stays
  `Full`-dirty and composites lazily when it is read (§4.1 part 3) or
  when it becomes displayed (an active-tile or channel switch). A
  slider drag on a 5-channel × 4-tile material therefore costs ≤ 3
  recomposites per frame, not 20.
- **The lazy cache rule.** A `(tile, channel)` whose only plane is Base
  needs no cache texture: readers get Base directly, which is today's
  exact path at zero extra VRAM. The cache texture materializes on the
  first layer-owned plane for that `(tile, channel)`, born `Full`-dirty
  so it can never be read uncomposited. Hiding the Base row (stage 4)
  also materializes the cache for Base's `(tile, channel)`s, since a
  hidden Base cannot be read "directly". In stage 1 nothing changes for
  any reader.
- The shell builds the composite entries (`PaintState` holds no stack).
  NEW surface `CompositeSnapshot`: per frame, the stack's per-layer
  metadata bottom-to-top, built from `doc.stack.layers` plus the Base
  row's visibility and handed to `paint.flush_composite(&snapshot)`.
  The shell diffs it against last frame's to raise `Full` marks.
  Material layers expand per channel (§5.2).

### 4.3 The compositor surface (LANDED, stage 1a, `b1ceb2c`)

Three additions, all default-off, so every existing caller and oracle
test stayed byte-identical. The landed text is the compositor's module
docs, judgments 10–12. This section records what landed, with r1's
stale prose corrected.

1. **`premultiplied: bool` on `CpuStackLayer`/`GpuStackLayer`.** When it
   is set, a contributing texel un-premultiplies before the blend:
   - `s = p / a` for `a > 0`, CLAMPED to `0..=1`. That is exactly "clamp
     `p` to `a`", since `min(p, a)/a == min(p/a, 1)` (judgment 10,
     `unpremultiply_texel`).
   - `a == 0` gives `cov == 0`, a no-op.
   - Then `composite_pixel_masked` runs unchanged. For Normal over the
     transparent accumulator, `out = (p/a)·a = p`. For Normal over any
     accumulator, `out = p + base·(1 − a)`, which is exactly the paint
     shader's premultiplied over (`shaders.rs:586`).

   Every LAYER-owned v2 plane is premultiplied (§3's invariant), and
   every writer converts (§3, §5.1, §5.2). The `from_layer` constructors
   default the flag to `false`.

   On WGSL the flag rides `CompositeParams.flags` bit 0. The old
   `pad0`/`pad1`/`pad2` are renamed `flags`/`origin_x`/`origin_y`, and
   the 32-byte assert holds.

   Invalid premultiplied data (`p > a`) composites as "clamp `p` to
   `a`". That is precisely the 28-LSB eraser error §3's invariant
   prevents (§8 T22's sentinel 255).

   The struct literals in the compositor's own tests and in
   `crates/umber-app/tests/gate0_journey.rs:247` gained `premultiplied:
   false` in the same commit **[m1]**.
2. **The bottom-copy identity rule** (judgment 11). When no entry has
   contributed yet, and the first contributing entry is premultiplied,
   Normal, `clamped_opacity == 1.0` and unmasked, its bytes SEED the
   accumulator verbatim instead of compositing.
   - "Contributed" means the entry passed the skip filters (visible,
     pixel-bearing, non-`Passthrough`). A fully transparent bottom entry
     counts, so the seed fires at most once, at the true bottom.
   - On valid premultiplied data (`p ≤ a`) the seed equals the composite
     exactly (item 1's identity).
   - On anything else it is still verbatim. That matters for Base, which
     holds v1's mixed-convention bytes (F3): a legacy straight texel
     `(200, 200, 200, 100)` would un-premultiply to `s = 2.0 → 1.0`,
     giving `out_rgb = 100/255 → 100`. The rule keeps 200.
   - CPU: `acc = byte/255`, and `f32_to_u8` returns the same byte.
   - GPU: a `copy_texture_to_texture` of the entry's tile overlap. It is
     exact by construction, not ±1 LSB. Both textures are `Rgba8Unorm`
     `PaintTarget`s carrying `COPY_SRC | COPY_DST` (`paint.rs:276-280`).
     r1's alternative seed dispatch was not built, so no flag bit 1
     exists.
3. **Region compositing** (judgment 12).
   `StackCompositor::composite_stack_region(encoder, layers, output,
   rect: CompositeRect)` clears and recomposites only `rect`, and leaves
   every texel outside it untouched.
   - The clear is a transparent `PaintTarget::write_region` (the retouch
     path's proven shape).
   - The dispatch extent rides `CompositeParams.width`/`height`, with the
     origin in `origin_x`/`origin_y`. Full-tile dispatches carry origin
     `(0, 0)`, so one shader serves both scopes.
   - Out-of-bounds rect edges clamp to the tile, and a fully-outside rect
     is a no-op.
   - A masked entry uploads ONLY `rect`'s mask bytes into a rect-sized
     `R8Unorm` texture, sampled at `texel − origin`.

   JUDGMENT CALL — rect upload over persistent per-layer GPU mask
   textures. It keeps the existing "nothing to sync" property
   (`paint_state.rs:286-290`) and adds no mask-sync state. The cost is
   `|R|` bytes per masked layer per stroke frame. Persistent GPU masks
   are the follow-up if measurement shows the upload matters.

   The CPU oracle `composite_stack_region_cpu` returns `rect`'s bytes.
   Under the house rule (judgment 8) the GPU must match it within §8's
   tolerance.

The compositor precondition "output must not alias a layer texture"
(judgment 5) holds by construction: the cache is a separate
`PaintThread` from every plane.

### 4.4 What stays flat

- **The cache itself.** It is derived, never saved (§6), and is what
  every composite reader binds.
- **The quick mask.** It is per SET, session state, never persisted or
  undoable (`paint_state.rs:186-190,233-254`). It is a stroke GATE, not
  a layer, and gates dab coverage into whichever plane the stroke
  targets (§3).
- **Layer-mask planes.** Already per layer (`layer_masks`, keyed
  `(layer, tile)`, set-sized, `paint_state.rs:264-290`). Their storage
  is unchanged. What changes is that they finally gate something live:
  the app never runs the stack compositor today (F1), so a PAINT layer's
  mask plane gates nothing visible; only material composites read masks.
  Under CoR every entry carries its enabled mask, and mask strokes raise
  dirt (§4.2, **[M6]**). When a channel is overridden, the mask is
  resampled nearest-neighbour to the plane size with the existing
  `resample_mask_nearest` (`material_layer.rs:461`).
- **The Base plane.** It is a plane, but owned by no layer (§1.4).
- **The fill preview.** No fill-preview surface exists in the tree today
  (`grep -ri "fill.preview"` over `crates/` and `docs/specs` finds
  nothing), so there is nothing to keep flat. A fill is previewed by
  being live (§5.1).

## 5. Fills and materials write their own planes

### 5.1 Fills

`refresh_fill_layer` (`fill_layer.rs:383-431`) keeps steps 1–3
(evaluate → symmetry → quick-mask gate, `fill_layer.rs:12-28`). Step 4
changes: the result is written PREMULTIPLIED into
`(Layer(id), tile, "baseColor")` through a new
`PaintState::publish_plane(owner, tile, channel, w, h, rgba) ->
Result<(), PaintRefusal>`. That is `publish_fill_channel`'s shape with
an owner (`paint_state.rs:1625-1655`), plus the materialization check
(§1.2).

- Texel = `(c · α, α)`, with `α = sample_a · coverage` quantized by
  `f32_to_u8`. Coverage is FOLDED IN. That fixes the fast path's silent
  coverage drop (`fill_layer.rs:413-426` publishes the sampled bytes and
  discards fractional quick-mask coverage).
- `composite_fill_over` (`fill_layer.rs:334-371`) and the `read_tile`
  inside the refresh (`:414`) are RETIRED. The layer's blend/opacity are
  applied by the compositor at read. That also retires the
  `blend_channel(..).unwrap_or(s)` degradation of triplet and
  normal-map modes (`fill_layer.rs:363`, dynmat review #17). A fill
  carrying `Passthrough` keeps today's documented behavior (composite as
  Normal, `fill_layer.rs:326-328`): the entry builder maps it to Normal.
- **[m8] Size is per tile.** The plane is
  `tile_channel_size(tile, "baseColor")` (`paint_state.rs:899`),
  evaluated per tile. Tiles belong to different sets
  (`paint_state.rs:556-560`), and `refresh_fill_layer` loops over every
  tile (`fill_layer.rs:398`), so its single `size` parameter becomes
  per-tile. The literal `1024` at `main.rs:2945` is RETIRED. On a 4096
  set it would make the CPU oracle fail loudly (`InvalidPixelData`) and
  the GPU path zero-fill (compositor judgment 7).
- **[B2] Open-path seeding.** NEW `FillLayerStates::ensure_open`, the
  shape of `MaterialLayerStates::ensure_open`
  (`material_layer.rs:672`). `open_project` calls it for every Fill
  layer, as it does for materials. Its rules:
  - It inserts the state CLEAN: it does not push onto `dirty`. Today's
    `ensure` (`fill_layer.rs:843-859`) pushes every FRESH id (`if fresh
    || state.params() != params`), which is the "fill compounds on open"
    bug (`dynamic-materials-design.md:469-475`). Under per-layer planes
    the same bug would double-count every v1 fill.
  - The per-frame `ensure` keeps its semantics for runtime-added layers
    and param edits.
  - §6.3's dormancy and §6.3's "v2 planeless ⇒ dirty" rule decide which
    open-seeded fills become dirty.
- Fills stay baseColor-only (unchanged v1 boundary). Multi-channel
  procedural content is what materials are for.

### 5.2 Materials

`refresh_dirty_materials` (`material_layer.rs:776`) keeps discovery,
validation, overrides, eval, the Uniform/blend per-channel checks and
resampling (`material_layer.rs:255-453`). The per-channel step changes
from "read flattened → `composite_material_channel` → publish" to:
write the resampled output, premultiplied, into `(Layer(id), tile,
channel)` via `publish_plane`.

- `MaterialTileIo` (`material_layer.rs:735-766`) loses `read_channel`
  for the publish path (`main.rs:652` is retired for publish, §1.1).
  `publish_channel` targets the layer's plane.
- The PER-CHANNEL blend/opacity/enable apply at COMPOSITE time. The
  entry builder emits one entry per `(material layer, channel)` with:
  - `blend = channel.blend`
  - `opacity = layer.opacity × channel.opacity` (formed first, clamped
    once, `dynamic-materials-design.md:376-378`)
  - `mask` = the layer mask
  - `premultiplied = true`
  - `pixels = None` when the channel is disabled, undeclared, or carries
    a forbidden blend. That mirrors `ChannelStatus::SkippedDisabled`/
    `SkippedBlend` (`material_layer.rs:175-187`).
  - The layer's own `blend_mode` stays inert (`material_layer.rs:29-31`).
- A failed eval writes nothing (whole-material no-op, unchanged), so the
  layer's planes keep the last good output. A budget refusal (§1.3)
  sets `budget_blocked` and writes nothing.
- Stage 4 rewrites `material_layer.rs`'s v1-boundaries module docs
  (`:20-28`) in the same kill, and retires the destructive-publish
  driver tests (§8, **[m4]**).

### 5.3 The four warts, closed

| Wart (`material_layer.rs:20-28`) | Why it closes |
|---|---|
| 1. Stack order not preserved | The composite folds entries in stack order on every flush. A material's plane holds only ITS output, so where it sits in the stack is where it composites. Reorder raises `Full` (§4.2). |
| 2. Re-eval compounds | A refresh REPLACES the plane (`WriteRegion` of the whole plane). Composite of identical plane bytes gives identical cache bytes. Refreshing twice is idempotent (§8 T6: 170, 170, never 144). |
| 3. Nothing un-publishes | Hide → the entry is skipped (`visible: false`). Disable a channel → `pixels: None`. Delete → the layer's planes are dropped (§5.4: eval-owned planes take no tombstone) and leave the composite. Each raises `Full`, and the next flush recomposites without it. Undo of the delete re-evaluates from the params the `Insert` carries. Undo of `SetMaterial` restores params, which re-dirty, which re-eval REPLACES the plane, so pixel-exact undo of material edits is automatic. |
| 4. Anchors self-feed | See below. |

**Anchors: downward partial flatten.** `AnchorRef` is a typed
`(layer, channel)`. Today `PaintMaterialIo::anchor_image` uses the layer
only as a liveness check and returns the FULL flattened channel
(`main.rs:675-687`).

NEW rule: an anchor on layer `A`, channel `c`, resolves to the composite
of the stack's entries for `c` from the bottom (Base included) THROUGH
`A` (inclusive). It is "the flattened cache", truncated at the anchor.

**[m11] The prefix composite runs on the CPU oracle, deterministically.**
For each `(anchor, tile)`:
1. `read_plane` every prefix plane. Each read drains its queue first
   (the `read_target` precedent).
2. Fold them through `composite_stack_cpu` with the same entry builder
   the flush uses.

The resulting bytes are bit-exact across machines and GPUs, so a saved
material plane depends only on its inputs (§6.1's determinism claim,
§6.5's disjoint-PNG claim).

Cost: `L_prefix` readbacks of 64 MiB at 4K plus one CPU composite of
16.8M texels, per `(anchor, tile)` per refresh. Today's anchor read is
one readback (`main.rs:675-687` → `read_channel_tile`).

JUDGMENT CALL — determinism over the GPU path's single readback. A GPU
prefix composite is ±n LSB, implementation-defined (judgment 8). Fed into
CPU graph eval and saved, it would make two artists re-saving an
untouched anchored material produce different PNGs: a binary conflict
§6.5 claims away. Anchored refreshes run only when the material is
dirty, never per frame, so the extra readbacks are a refresh-time cost.
A GPU fast path, with GPU-dependent anchor bytes recorded as a §6.5
boundary, is a measured follow-up (§9).

**[M7] Ordering.** The prefix reads drain every prefix plane. A material
whose prefix contains a layer refreshed THIS frame refreshes AFTER that
layer's publish. Anchor sources sit below their consumers (the forward
rule below), and the refresh order is already bottom-to-top
(`main.rs:2979`), so the fill refresh block running before the material
block plus the drain-first read is sufficient. Nothing stale can be
read, and a refreshed material does not go clean on stale input.

A material `M` may anchor only layers strictly BELOW it:

- `A == M` → NEW `MaterialError::SelfAnchor { layer }`.
- `A` above `M` → NEW `MaterialError::ForwardAnchor { layer, anchor }`.

Both are whole-material errors with the badge, never silent.
Self-feedback is impossible by construction.

JUDGMENT CALL — partial flatten THROUGH the anchored layer, rather than
the anchored layer's own plane. The brief's wording ("anchors read the
FLATTENED cache") and requirement row 42 ("reference any
layer/channel/mask as generator input") both point at accumulated stack
content. Edge-wear and height-detail generators want the accumulated
result: everything painted up to that point, not one layer's isolated
strokes. The layer-local plane is one `read_plane` away if a node ever
needs it (follow-up, §9).

**Instances render.** `LayerKind::Instance` entries pass `None` today
(compositor judgment 4), and the dynmat doc names "no per-layer result
to reference" (`dynamic-materials-design.md:163-166`). With planes, an
instance's entry reads its SOURCE's planes (transitively, via
`instance_eval_order`, `layers.rs:1255`), with the instance's own
`visible`/`opacity`/mask. An instance's entries are part of the source
layer's dirt set (§4.2's mask writer, and source plane writes mark the
instance's `(tile, channel)`s).

JUDGMENT CALL — for a Material source, the instance reuses the source's
per-channel blends and multiplies its own opacity in. For Paint/Fill
sources, the instance's own `blend_mode` applies. This is
"renders its source layer's composited pixels" (`layers.rs:40`) taken
literally per channel.

### 5.4 Layer delete, undo, and tombstones

**[M5] Rewritten.** `LayerCommand::Remove` is undoable (`journal.rs`
`Insert`, `:59-66`, which holds the full `Layer`, params included). If
delete freed a PAINT layer's planes, undoing the delete would revive an
EMPTY layer, because strokes are not re-derivable.

1. **Only PAINT-owned planes get tombstones.** When the shell's
   per-frame census sees an id with planes that is no longer in the
   stack:
   - a **Paint** layer's planes are retired to a CPU `RetiredPlanes` map
     (owner → bytes per tile × channel, plus each plane's size);
   - a **Fill or Material** layer's planes (eval-owned, §5.1/§5.2) are
     DROPPED with no tombstone. Its revive re-dirties the layer from the
     params the `Insert` carries, and the next refresh regenerates the
     planes. r1's "1 GiB to delete one 4-tile material" example retires
     0 bytes.
   - The census skips the live `stroke_owner` while `is_stroking()`
     (§2.3, **[m7]**).
2. **Amortized readback.** Retirement reads back AT MOST ONE plane per
   frame. Each GPU thread stays alive, out of the composite (its owner
   is not in the snapshot), until its bytes land in `RetiredPlanes`.
   Then it drops. Deleting a 4-tile, 2-channel paint layer is 8 frames
   of one 64 MiB readback each, not one 8-readback hitch. An undo that
   arrives mid-retirement simply cancels the pending readbacks: the
   still-live threads rejoin the composite.
3. **Revive.** When a layer reappears (undo of Remove, `Insert` replay),
   its retired planes upload back unchanged and re-materialize through
   `materialize_plane` (a budget refusal there is a loud revive-empty,
   as in item 4).
4. **The cap.** `RETIRED_BUDGET_BYTES = 512 MiB` of CPU RAM. Past it, the
   OLDEST tombstone is freed first, EXCLUDING the newest entry. One
   tombstone larger than the cap on its own is retained while it is the
   newest, so the last delete is always undoable. Undoing the delete of
   a freed tombstone revives the layer EMPTY, with a log and a status
   line naming the layer (string `layers.tombstone-lost`, stage 4).

   JUDGMENT CALL — a bounded, honest pixel loss on a deep undo-of-delete
   beats unbounded RAM. 512 MiB is the stroke history's 256 MiB budget
   (`paint_state.rs:456`) doubled, because a retired plane is a whole
   plane rather than a dirty rect. That is eight 4K planes. The document
   history holds 256 entries with no byte budget (`document.rs:58-60`:
   "the memory budget system (bytes) arrives with GPU-tile snapshots",
   and this design is that arrival).
5. **Freeing.** A retired entry is freed when no command left in
   `doc.history` can revive its id. NEW surface: `UndoStack::iter()`
   (or a `referenced_layer_ids`) for that census, because no `on_evict`
   hook is wired today (`paint_state.rs:94-96`). Retired planes are not
   saved: the layer is not in the model.
6. **Resize drops tombstones.** `resize_set`/`resize_channel` (§5.5)
   drop the tombstones of the resized set's tiles (the resized channels
   only, for `resize_channel`). Contents are dropped by the resize
   anyway, and old-size bytes would be rejected by the revive's publish.
7. **Session lifecycle** clears `RetiredPlanes` (§6.3, **[M10]**). Ids
   are not unique across documents, so a tombstone from document A must
   never revive into document B.
8. Stroke-history regions (§3) whose owner is retired apply to the
   retired bytes. If the owner is gone and freed, the region is dropped
   with a log.
9. `remove_layer_masks`/`prune_layer_masks` (`paint_state.rs:1360-1375`)
   keep their current semantics for MASK planes.

   JUDGMENT CALL — mask planes do not get tombstones in v2. Detaching a
   mask already drops its planes by design ("a re-attach starts white",
   `:1357-1359`).

### 5.5 Resize

`resize_set`/`resize_channel` (`paint_state.rs:1985-2075`) publish
`Resize` to every PLANE of the set's tiles (overrides honored per
channel, as for targets today) and to the cache. They also mark `Full`
on every resized `(tile, channel)` and drop the matching tombstones
(§5.4 item 6). Contents drop, the same destructive contract as today
(`:1965-1970`), now for every layer. Fills and materials re-dirty
through the existing size census (`main.rs:3000-3017`) and refill their
planes. Painted layers lose their pixels exactly as the flat target
does today. The UI's existing per-set hint already names the loss.

### 5.6 Imported maps (stage 4) **[M9]**

`import_bytes` (`mesh_map_import.rs:312-345`) stores the PNG and pushes a
`LayerKind::Paint` layer named `"Imported <map>"` on top. It returns
`ImportApplied { channel, layer_id, asset, .. }` (`:282-293`), but the
bytes never reach the pool. Under CoR such a layer renders nothing,
although per-layer storage is exactly what lets it render.

At stage 4:
- `apply_session_set` (`mesh_map_import.rs:542`) returns the
  `ImportApplied` list instead of a bare count, or takes a publish
  callback. The shell then decodes each stored PNG and publishes it via
  `publish_plane(Layer(layer_id), tile, channel, w, h, rgba)`,
  premultiplied (straight PNG `(c, α)` → `(c·α, α)`, `f32_to_u8`).
- The tile comes from the file's UDIM token when present, else
  `FIRST_TILE`.
- A size mismatch against `tile_channel_size(tile, channel)` is refused
  loudly (warn log + status line), exactly as the sidecar load refuses
  (`main.rs:2280-2290`). The layer still exists, and its row badges the
  refusal.
- Import does not move an existing selection (§2.2). On open,
  auto-resolution may pick an import layer as the topmost paintable one.
  It now owns real pixels, so a stroke there paints that layer, which is
  the honest behavior.

If the stage-4 kill cannot carry §5.6, it is recorded as a named v2
boundary in §9, AND §2.3's auto-resolution must skip import-created
layers until it lands. That needs a marker the shell can see; the layer
carries no asset ref today. The kill states its choice in
`mesh_map_import.rs`'s module docs.

## 6. Save / load

### 6.1 The sidecar layout

```
paint/
  tile-1001.png                       <- Base, baseColor    (the v1/v2 path, unchanged)
  height/tile-1001.png                <- Base, height       (the per-channel path, unchanged)
  layers/7/baseColor/tile-1001.png    <- Layer 7, baseColor (NEW)
  layers/7/roughness/tile-1001.png    <- Layer 7, roughness (NEW)
  layers/12/normal/tile-1002.png      <- Layer 12, normal on UDIM 1002
  mask-7-tile-1001.png                <- unchanged (paint_mask_name, main.rs:1154)
  target_size.txt, target_sizes.txt   <- unchanged
  manifest.txt                        <- one line per artifact, arity 5 unchanged
```

- **Base keeps the legacy paths byte-for-byte** (`paint_tile_path` /
  `paint_channel_tile_path`, `main.rs:1057-1072`). The bytes are Base's
  PLANE bytes, read via `read_plane(Base, …)`, never the cache
  (**[B3]**, §1.1). Layer planes nest under `layers/<id>/<channel>/`.
- The manifest `name` field encodes the plane, exactly as the
  per-channel pool encoded the channel instead of adding a 6th column
  (`main.rs:1074-1089`, `parse_paint_commit`'s exact 5-field destructure
  at `:1574`).
- **The manifest header** **[B2, M11]** is the paint commit's
  provenance marker.
  - Stages 1b–3 keep writing `# Umber paint commit v1` (`main.rs:1553`).
  - From stage 4 on, every save writes `# Umber paint commit v2`,
    REGARDLESS of `project.json`'s version, followed by one
    `# dormant <id>` line per layer dormant at save time (§6.3).
  - `parse_paint_commit` still skips `#` lines for the artifact table
    (`main.rs:1570`). NEW at stage 4: the open path reads the header
    lines, the one load-path read of the manifest (`main.rs:1503`: the
    load "never reads it" today).
  - An absent or unreadable manifest counts as legacy. That errs toward
    dormancy, which is the byte-identical side.
  - Writing the v2 header before stage 4 would be a bug. A stage-2/3
    build still bakes fill output into Base, and a stage-4 build
    trusting a v2 header would double-count it.
- **Parse rule (the trust boundary):**
  - 1 component, `tile-<N>.png` → `(Base, baseColor, N)`.
  - 2 components, `<channel>/tile-<N>.png` → `(Base, channel, N)`.
  - EXACTLY 4 components, `layers/<u64>/<channel>/tile-<N>.png` →
    `(Layer(id), channel, N)`.
  - Anything else is rejected. That includes `..`, a backslash, an empty
    channel, a non-numeric id, and 3 components.
  - Depth disambiguates a texture-set channel literally named `layers`:
    `layers/tile-1001.png` is that channel's Base plane.
  - This extends `parse_paint_sidecar_name`/`is_paint_sidecar_name`
    (`main.rs:1099-1109,1515-1520`). `list_paint_tile_names` and the
    load collector walk ONE directory level today
    (`main.rs:1732-1749`, `collect_paint_tiles` at `:2061`); both learn
    the `layers/` depth. `sweep_paint_temps` already recurses fully
    (`main.rs:1843-1860`).
- **[m10] Pruning empty directories.** The apply step prunes unlisted
  FILES (`main.rs:1750-1756`). After pruning files it removes the
  now-empty directories under `paint/layers/` bottom-up (`<channel>/`,
  then `<id>/`), so a deleted layer leaves no empty tree behind. Only
  directories under `paint/layers/` are candidates.
- **Do NOT put planes under `layers/<set>/`** (the layer-JSON tree).
  `prune_stale_layer_files` deletes every entry there that is not a live
  `<id>.json`, DIRECTORIES included (`project.rs:436-480`, the
  `remove_dir_all` at `:467-469`). A plane directory beside the layer
  JSON would be deleted by every project save. The pixel tree stays
  under `paint/`, inside the paint commit's own atomic protocol
  (`main.rs:33-60`).

JUDGMENT CALL — the cache is NEVER saved. It is derived (one flush
recomputes it), and saving it would make every paint stroke on any
layer rewrite the shared `paint/tile-<N>.png`. That is the merge
conflict this layout exists to remove (§6.5). It is also the B3
double-count. Export and bake read the cache after the flush, never the
sidecar. A GPU-composited cache's ±n LSB therefore never reaches a
saved file.

JUDGMENT CALL — eval-owned planes (fills, materials) ARE saved, though
re-evaluable. Opening must not depend on the mesh being present (a fill
cannot evaluate without one, `main.rs:2952-2954`), and opening must be
deterministic without re-running graphs. This is the project-open seeds
CLEAN precedent (`material_layer.rs:34-38`), now for fills too
(`FillLayerStates::ensure_open`, §5.1). It also retires the fill
compound-on-open bug (`dynamic-materials-design.md:469-475`). Anchored
material planes are deterministic because their prefix runs on the CPU
oracle (§5.3).

### 6.2 Save cost and the clean-plane skip **[B1]**

Per-plane readback replaces per-target readback. A clean plane may skip
its readback ONLY when BOTH hold:

1. **Same directory.** The save's target project dir equals the
   `committed` dir. Compare after `std::fs::canonicalize`; a
   canonicalize failure counts as different.
2. **Same bytes on disk.** The target dir's sidecar file for that plane
   exists, and its blake3 hash equals the recorded `committed` hash. The
   check re-reads and re-hashes the file. That is a file read plus
   blake3, far cheaper than a GPU readback plus PNG encode.

Only then are the file's bytes re-staged, and the plane's `committed`
is re-recorded for the new commit. In EVERY other case the plane is
read back and encoded. That covers `committed == None`, a different
dir, a missing file and a hash mismatch.

- **Never re-stage from a dir the plane was not committed to.** Every
  save goes through `rfd::FileDialog::save_file()` (`main.rs:2397-2404`;
  Ctrl+S and the menu call the same fn, `main.rs:2662,2712`), so every
  save can target a NEW directory. r1's "re-stage its existing sidecar
  bytes" read from the TARGET dir, which a save-as does not have: every
  clean plane was silently dropped, Base included, from stage 2 on.
- The failed-readback re-stage (`main.rs:1955-1962`) keeps its current
  semantics only for an in-place save, i.e. when condition 1 holds. In
  a save-as, a failed readback is a loud per-plane failure: warn log
  plus the save's status line. It is never a silent omission.
- A clean skip that finds its source file missing or mismatched is
  never silent either. It logs at warn level and falls through to the
  readback.
- After a successful commit, every staged plane records
  `committed = Some((target_dir, blake3(staged_png)))`. The load path
  records the same (§1.2), so an in-place save right after open reads
  back nothing.
- The content-addressed store dedupes the blob anyway (`main.rs:24-32`,
  `AssetStore` hashes with blake3, `umber-core/src/assets.rs:97`). A
  save after painting one layer reads back that layer's touched planes
  only. PNG encoding of identical bytes is identical, so untouched
  layers produce no file churn.
- Stage 2's surface adds `blake3 = { workspace = true }` to
  `crates/umber-app/Cargo.toml`. Today it is a dependency of
  `umber-core` only (the E0432 trap).

### 6.3 Migration: a version bump AND a dual-read loader

- **Dual read, keyed on the sidecar's CONTENT, not on `project.json`.**
  Legacy paths always load into Base. `layers/…` paths load into
  layer planes. The paint loader needs no version at all.
  - A v1 project: every tile and channel lands in Base, verbatim.
  - The first frame is byte-identical: Base-only `(tile, channel)`s
    have no cache and read Base directly (§4.2). Once a layer plane
    exists, the bottom-copy rule seeds Base verbatim (§4.3 item 2).
  - The crash window `main.rs:74-80` documents ("new metadata with old
    paint") can now produce a v2 `project.json` beside a v1-layout
    sidecar. It loads as a migration, the same result the window gives
    today.
- **[M2] The version: one default, one opt-in.**
  - `CURRENT_PROJECT_VERSION = 1` STAYS, as the DEFAULT WRITE version.
    `ProjectModel::new` (`project.rs:230-240`) keeps stamping it, so
    the app's model build (`main.rs:2419`), every umber-cli batch/agent
    session (`umber-cli/src/agent.rs:1585`) and the agent's wire
    `version` field (`agent.rs:662`) are unchanged.
  - NEW `pub const LAYER_PLANES_PROJECT_VERSION: u32 = 2` and
    `pub const MAX_SUPPORTED_PROJECT_VERSION: u32 = 2` in `project.rs`,
    re-exported from `umber-core/src/lib.rs` beside
    `CURRENT_PROJECT_VERSION` (`lib.rs:42`).
  - `validate_project_structure` (`project.rs:368`) and the load probe
    (`:570`) change from `!= CURRENT_PROJECT_VERSION` to
    `1..=MAX_SUPPORTED_PROJECT_VERSION`. `UnsupportedVersion`'s message
    (`project.rs:52-54`) names the supported range.
  - The app's `save_project` sets `model.version =
    LAYER_PLANES_PROJECT_VERSION` iff the save will stage at least one
    plane whose owner is a `Layer(id)` LIVE in the stack. Orphans and
    tombstones don't count. Otherwise it leaves 1. The decision is
    computable from the session before either file is written.
  - A loaded model is NOT re-stamped. r1's "a loaded model is stamped
    2" contradicted "write 2 iff a layer plane exists" and is struck.
    The save recomputes the version every time.

JUDGMENT CALL — bump, superseding `dynamic-materials-design.md:548-552`'s
no-bump argument for THIS change. That argument was that a bump makes
the new build refuse every old project. Dual-read removes it. The
opposite failure is worse here: an OLD build opening a v2 project would
see only Base and show the canvas missing every layer's paint. Its save
would leave the `layers/` tree in place (its walker is one level deep,
`main.rs:1732-1749`) beside its own new flat edits, a silent fork. A
loud `UnsupportedVersion` on the old build is the safer failure.

The minimal bump (2 only when a live layer plane is saved) keeps
layer-less and headless projects, such as umber-cli batch sessions,
readable by old builds. Stage 2 must re-grep `CURRENT_PROJECT_VERSION`
across `crates/` (umber-cli and `LANDING_NOTES.md` included) before
landing.

- **[B2, M11] Dormant fills and materials, keyed on PROVENANCE.** A
  legacy flat plane already contains every fill's and material's
  destructive output. It cannot be decomposed: Multiply and compounded
  publishes are not invertible. On the OPEN path a Fill or Material
  layer is **dormant** iff one of these holds:
  - the paint manifest's header is NOT `# Umber paint commit v2`
    (absent, unreadable, or v1: a pre-stage-4 commit), AND the layer has
    no loaded planes, AND Base is non-empty; or
  - the header IS v2 and lists the layer's id on a `# dormant <id>`
    line.

  A dormant layer is seeded CLEAN (`ensure_open`, §5.1). It does not
  re-evaluate on open, even with a mesh present, so the first frame
  stays byte-identical. Its row badges "its pre-v2 result lives in the
  Base paint" (string `layers.dormant-badge`, stage 4).

  The first param edit or explicit refresh activates it. It evaluates
  into its own plane, its old contribution in Base remains, and the
  layer is composited twice. The badge says so beforehand. Every save
  lists the still-dormant ids, so dormancy survives a save/reopen of a
  project the user never touched. Without the list, the v2 header would
  make the next reopen evaluate the fill over its own baked output.

  A non-dormant planeless Fill/Material on open (a v2 header, id not
  listed) is a layer that never evaluated: a fill first seen without a
  mesh (`main.rs:2952-2954`), or a material whose every eval failed. It
  is seeded DIRTY, and it evaluates normally once a mesh is present. Its
  result lives nowhere, so badging it as dormant would be false.

  JUDGMENT CALL — a byte-identical open, with any double-count only
  after an explicit user action, beats a silent double-count on every
  open. Dormancy is keyed on provenance, not on absence: "planeless"
  alone cannot tell legacy output baked into Base from never evaluated.
  The paint commit is the right carrier: it is the pixel tree's own
  provenance, and it's written atomically with the planes it describes.
  `project.json`'s version is NOT a valid key. Under the minimal bump, a
  session whose fill never evaluated saves version 1 with no planes, so
  "version 1" and "legacy" are not equivalent. A layer added at runtime
  is never dormant.

- **Base stays Base.** It is shown as a pinned, SELECTABLE bottom
  pseudo-row ("Base paint", §2.2) from stage 3, with its visibility
  toggle at stage 4. A "convert Base to a layer" action is a follow-up
  (§9).

  JUDGMENT CALL — opening a v1 project must not rewrite the layer JSON
  tree. Injecting a synthetic layer would allocate an id
  (`next_layer_id`), and the next save would add a new
  `layers/<set>/<id>.json` the user never made.

- **[M10] Session lifecycle.** Layer ids restart at 0 in every
  `LayerStack` (`layers.rs:905-911`), so session pixel state keyed by id
  must not outlive its document. `open_project` and `reset_for_new`
  call NEW `PaintState::clear_layer_planes()` BEFORE the sidecar load.
  - It drops every `Layer`-owned plane, every cache texture,
    `cache_dirty`, `RetiredPlanes`, `mask_synced` entries for layer
    owners, and the active/stroke owner. The selection resets to Base
    with `active_explicit = false`, and the next sync auto-resolves it.
  - Base planes follow the existing resize/load path: they are replaced
    by the load, or cleared by new.
  - `reset_for_new` also resets `fill_states` (`main.rs:502`).
    `reset_for_new` currently omits it (`main.rs:603-615`). Without the
    reset, B's Fill 3 inherits A's Fill 3 state.
  - Lands at stage 2, the first stage that can load a layer plane.

### 6.4 The autosave journal

`JournalOp` records layer METADATA only, with no pixel op
(`main.rs:74-77`). That stays true. Plane writes ride neither the
journal nor `project.json`. Pixel crash-safety is the paint commit's
own atomic protocol, unchanged (`main.rs:33-60`). The journal's
interactions with planes:

- Replaying `Remove` for a layer whose planes were saved: the planes
  are orphans on load (id not in the stack). They are ignored with a
  log, do not count toward the version decision (§6.3), and are pruned
  by the next save's manifest (the apply step prunes unlisted names,
  `main.rs:1754-1756`, and now their empty directories, §6.1).
- Replaying `Add` for a layer created after the last save: the layer
  exists and has no planes. It is empty, exactly as unsaved paint is
  lost today. A replayed Fill/Material `Add` is a runtime-added layer
  (never dormant) and evaluates normally.
- Selection is not journaled (§2.1).

### 6.5 The merge story gets stronger

`docs/merge-story.md` Property 1 says different layers never conflict,
and that is true for layer JSON. Property 3 claims painted pixels are
"content-addressed and binary-neutral … the layer file's asset reference
is the single point of contention" (`merge-story.md:33-44`). The code
does not do that: pixels live in ONE flat `paint/tile-<N>.png` per tile
and channel, and no layer file references them (`main.rs:18-32`).
Today two artists painting DIFFERENT layers on tile 1001 both rewrite
`paint/tile-1001.png`, a binary conflict on every concurrent paint
session.

Under this layout each layer's pixels are its own files
(`paint/layers/<id>/…`). Two artists painting different layers touch
disjoint PNGs, so Property 1 becomes true for PIXELS, not just metadata.
That holds for eval-owned planes too: saved fill/material bytes are
CPU-deterministic (§5.3's CPU anchor prefix, **[m11]**), so re-saving an
untouched material on another machine reproduces its PNG. The same-layer
case is still a binary conflict on that layer's PNG, as it should be.

One shared file remains: `manifest.txt`. It is provenance, not authority
for the artifact table (the sidecar wins on open, `main.rs:59-60`), so
a conflict there resolves by taking either side, and the next save
rewrites it. Its header lines carry dormancy (§6.3). A conflict that
drops a `# dormant` line makes that layer evaluate on the next open:
a visible double-count the user can undo by hiding the layer, never a
lost pixel. The `merge-story.md` correction is a doc delta in the
kill report (outside this lock).

## 7. Migration path: staged kills

Every stage lands green and leaves the app whole. The ordering rule,
from the advisor review and the adversarial review:
- **Save must round-trip layer planes BEFORE any stroke can create one.**
  Otherwise a save/reopen between stages would flatten every layer.
- **Strokes and fills/materials move to planes TOGETHER.** Moving either
  alone reorders destructive publishes against strokes.
- **[B2] No producer moves to its own plane before the open path stops
  re-evaluating it over legacy Base output.** Dormancy, the fill
  open-seeding (`FillLayerStates::ensure_open`) and the v2 manifest
  header therefore land IN stage 4, in the same kill that moves fills
  and materials onto planes. Stage 4 must not land without them.

| Stage | Surface | Lands | Behavior after |
|---|---|---|---|
| **1a — compositor** ✓ LANDED (`b1ceb2c`) | `umber-gpu/src/compositor.rs`, `umber-app/tests/gate0_journey.rs` (struct literal) | `premultiplied` flag (`flags` bit 0), bottom-copy identity rule (GPU texture copy), `composite_stack_region` + `composite_stack_region_cpu` + `CompositeRect` (§4.3); §8 T1–T3 + 3 GPU twins | Byte-identical: every new field is default-off; existing goldens pass unchanged. |
| **1b — storage** | `paint_state.rs`; `umber-gpu/src/paint_thread.rs` + `paint.rs` (`PaintThread::with_compositor`, shared `Arc<PaintCompositor>`) **[M4]** | `PlaneOwner`, `planes`, `Plane` (with `committed`), `materialize_plane` + `PaintRefusal::PlaneBudget` + both budget constants + the OOM error scope, `plane_bytes`, `read_plane`, `publish_plane`, the lazy cache rule, `cache_dirty` (multi-rect `DirtyRegion`), `flush_composite` (a no-op while only Base exists) | Byte-identical: everything is Base, and readers read Base. Every existing paint_state test passes unchanged; that IS the regression contract. |
| **2 — save v2** | `main.rs` sidecar codec + `save_paint_tiles`/`load_paint_tiles` + `save_project` (version decision) + `open_project`/`reset_for_new` (`clear_layer_planes`, `fill_states` reset); `crates/umber-app/Cargo.toml` (`blake3`); `umber-core/src/project.rs` (version range, two new constants) + `umber-core/src/lib.rs` (re-export) | the §6.1 layout + parse rule + empty-dir prune, the save loop on `read_plane` **[B3]**, `committed` + §6.2's clean-skip test **[B1]**, dual-read, the minimal bump via `LAYER_PLANES_PROJECT_VERSION` **[M2]**, the session lifecycle **[M10]**. The manifest header STAYS v1. | Layer planes round-trip (published via `publish_plane` in tests). No UI creates one yet, so saves still write version 1, and the CLI still writes version 1. Save-as into a new dir keeps every plane. |
| **3 — selection + gate** | `paint_state.rs` (`active_target`, `active_explicit`, `active_index`, verdict, `PaintRefusal::Layer`, mask-edit reconcile), `layers.rs` (Fill/Folder arms + tests), `document.rs` (`LayersUiOutcome`, selectable rows, add-selects, the SELECTABLE Base pseudo-row **[M3]**), `main.rs` (per-frame revalidate + sync), `layers_panel.rs` (gate test update), `assets/i18n/en-US.ftl` + `pt-BR.ftl` (refusal hints, `layers.base-paint`, `layers.base-paint-hint`) | §2 entirely, except the Base row's visibility toggle | Strokes on an EXPLICITLY selected Material/Instance/Fill/Folder are REFUSED with a typed reason. Auto-selection never picks a refusing layer, so a stack of only fills/materials still paints (into Base) **[M12]**. Accepted strokes still land in Base. Closes the stroke-gate follow-up. Coherent: only the gate and the selection are new. |
| **4 — per-layer pixels** | `paint_state.rs` (routing, capture, retouch mirror + composite sample, premultiplied eraser, quick-mask per plane + the `mask_synced` resets, mask-stroke dirt, resize + tombstone drop, `Full` coalescing), `umber-brush/src/retouch.rs` (owner keys; the premultiplied erase), `fill_layer.rs` (`publish_plane`, per-tile size, `ensure_open`), `material_layer.rs` (plane publish, module docs v1 block rewrite, retired driver tests), `mesh_map_import.rs` (§5.6), `bake_sources.rs` (`&mut PaintState` signatures), `main.rs` (`PaintMaterialIo`, refresh drivers, the END-of-update flush **[M7]**, `CompositeSnapshot` + Base visibility, tombstone census, budget lines, Perf HUD → `plane_bytes()` at `:3120`, dormancy + the v2 manifest header + `# dormant` lines **[B2, M11]**), `document.rs` (Base row visibility toggle), `assets/i18n/en-US.ftl` + `pt-BR.ftl` (budget, tombstone-loss, dormant badge, import-refusal strings), `docs/perf/protocol.md` (new scenarios) | §3, §4.1's flush + read contract, §4.2's dirt + coalescing, §5.1–5.2, §5.4–5.6, §6.1's v2 header, §6.3's dormancy | Strokes, fills, materials and imports each write their own planes, and the cache is composited every frame. Warts 1–3 closed; the first version-2 saves. A v1 project with a Fill opens byte-identical (dormant). **Gate:** §8's stage-4 tests including **T18**, plus the perf gate: input-to-photon re-measured under `docs/perf/protocol.md`, AND "drag opacity of a 5-channel masked material at 4K" and "X-symmetry stroke near the tile edge" against the 16.7 ms frame budget, pass/fail **[M8]**. |
| **5 — anchors, instances** | `material_layer.rs` (`SelfAnchor`/`ForwardAnchor`, CPU prefix flatten), `main.rs` (`PaintMaterialIo::anchor_image`), the entry builder (instances), `layers_panel.rs` (dormant-badge polish only, if stage 4 split it) | §5.3's anchor + instance rules | Wart 4 closed and instances render. Until 5 lands, anchors keep v1's documented full-flatten read and instances keep rendering nothing (both already-documented v1 states, not regressions). |

**Version skew (dev-only window).** From stage 2, builds accept
version 2. So a stage-2/3 build can open a stage-4 project that has real
layer planes. It has no `CompositeSnapshot` and still publishes fills
destructively, so it shows Base only and may compound fills into Base.
No release ships between stages 2 and 4, so this is a developer-build
hazard, not a user one. Don't round-trip stage-4 projects through
stage-2/3 builds.

Stage 4 is the dynamic-materials Kill-B-class landing: it touches the
live publish path, and r2 made it larger by pulling dormancy and imports
into it (B2, M9). It should be dragon-authored, or a claw with
`material_layer.rs`'s module docs and this doc as required reading (the
per-channel-pool rollout precedent, `per-channel-tile-pool-design.md:
161-165`). If it must split, the split line is §7's third ordering bullet:
fills and materials move to planes in the same sub-kill as dormancy +
`ensure_open` + the v2 header. Stages 1b and 2 are headless-testable and
claw-shaped. Stage 3 is UI and needs the GPU-free headless tests named in
§8 plus a runtime check.

## 8. Test plan

House rule: exact bytes, hand-derived, each ≥ 0.1 from a rounding
boundary unless exact. Quantizer `f32_to_u8 = (v·255).round()`. The CPU
oracle accumulates in f32 and quantizes ONCE at the end, so the
intermediate values below are unrounded. Test numbers T1–T19 are stable
(committed code cites them: `b1ceb2c` names "§8 T1/T2/T3"). r2's
additions are T20 and up.

Goldens are pinned on `composite_stack_cpu`/`composite_stack_region_cpu`.
**[m12] GPU twins** assert `|gpu − cpu| ≤ n_contributing` LSB, and each
twin states its contributing-entry count. Reason, in the test comment:
the GPU accumulator is an in-place `Rgba8Unorm` storage texture
quantized after EVERY dispatch (judgment 5), the CPU oracle quantizes
once, and UNORM rounding and FMA contraction are implementation-defined
(judgment 8). A bottom-copy seed contributes 0, because the copy is
exact.

**Compositor (stage 1a) — LANDED in `b1ceb2c`**
1. ✓ **Premultiplied identity.** One premultiplied entry, texel
   `(100,100,100,128)`, Normal, 1.0, over transparent. It carries an
   all-255 mask plane, so the entry is not "unmasked" and the bottom-copy
   rule does NOT fire. The mask's `m = 1.0` leaves coverage unchanged,
   so this exercises the general un-premultiply path:
   `s = 100/128 = 0.78125`, `cov = 128/255`,
   `out = 0.78125·0.501961 = 0.392157 → 100`; alpha `0.501961 → 128`.
   Sentinel: the straight path gives `0.392157·0.501961 = 0.196847 →
   50`. Assert 100 and ≠ 50.
2. ✓ **Bottom-copy verbatim.** Legacy straight texel `(200,200,200,100)`
   as the bottom premultiplied Normal/1.0/unmasked entry →
   `(200,200,200,100)`. Sentinel via the general path (the T1 mask
   trick): `s = 2.0 → clamp 1.0`, `out = 100/255 → 100`, giving
   `(100,100,100,100)`. Assert 200 and ≠ 100.
3. ✓ **Region equals full.** A 4×4 two-layer stack; the region
   `(1,1)-(3,3)` bytes equal the full composite's same rect exactly, and
   texels outside the rect are untouched in the output.
4. ✓ **Flag off is byte-identical** **[m1]**. Every pre-existing
   compositor test passes with `premultiplied: false` added
   mechanically to its struct literals (the compositor's own test
   literals + `gate0_journey.rs:247`), and no expected byte changed.

**Order, compounding, un-publish (stage 4).** Stack bottom→top:
- `L0`: paint, opaque byte 200.
- `M`: material, Multiply, src 100 opaque, layer 1.0 × channel 0.25 →
  cov 0.25.
- `L2`: paint, Normal, opacity 0.5, opaque byte **60** **[m3]**.

The Multiply factor is `1 − 0.25·(1 − 100/255) = 1 − 0.25·0.607843 =
0.848039`.

5. **Order preserved (wart 1).**
   - Stack order: `200·0.848039 = 169.608`, then
     `169.608 + (60 − 169.608)·0.5 = 114.804 → 115`.
   - With `M` reordered to the top: `L2` first gives
     `(200 + 60)/2 = 130.0`, then `130·0.848039 = 110.245 → 110`.
   - Sentinel: v1's destructive path. In v1, strokes ignore layer
     opacity (layers are metadata, F1), so an opaque 60 stroke REPLACES
     the flat 200, and M's refresh gives `60·0.848039 = 50.882 → 51`,
     whatever the stack order. (r1's sentinel, `106` with `L2 = 50`, was
     wrong: v1 gives `42.402`, which is also 0.098 from a boundary.)
   - Assert 115 for the stack order, 110 after the reorder, ≠ 51.
6. **Re-eval idempotent (wart 2).** `L0` + `M` only:
   `169.608 → 170`. Refresh `M` again with identical inputs → **170**.
   Sentinel: v1's compounding gives `170·0.848039 = 144.167 → 144`
   (`dynamic-materials-design.md:664-667`). Assert 170, 170, ≠ 144.
7. **Un-publish (wart 3)** **[M5]**. From test 6:
   - hide `M` → 200;
   - show `M` → 170;
   - disable its channel → 200;
   - delete `M` → 200. Its planes are DROPPED (eval-owned): the GPU
     plane count goes −1, and `RetiredPlanes` holds NOTHING for `M`;
   - undo the delete → `M` is re-dirtied from the `Insert`'s params, and
     the next refresh → 170 (revive by re-eval, not by tombstone).
   - **Retired** (stage 4, **[m4]**): the `material_layer.rs` driver
     tests pinning dynmat goldens 2b (171/187, "order matters under
     destructive publish") and 2c (144, "pins the v1 boundary so a future
     non-destructive fix flips this test on purpose") are deleted with
     the destructive path. Their composite-level replacements are T5
     (order) and T6 (170, 170). The kill names both retired tests in its
     report, and the rewritten module docs say why.

**Anchors (stage 5).**
8. **Downward partial flatten** (CPU prefix, §5.3). `L0` opaque 200;
   `L1` paint Normal opacity 0.5, opaque 100; `M` above both.
   - `anchor(L0) = 200`.
   - `anchor(L1) = 200 + (100 − 200)·0.5 = 150.0 → 150`.
   - `anchor(M)` → `Err(SelfAnchor { layer: M })`.
   - An anchor on a layer above `M` → `Err(ForwardAnchor)`.
   - The prefix bytes are bit-exact (CPU), with no GPU tolerance.
   - Sentinel: v1's full-flatten read returns the composite INCLUDING
     `M`'s own output.

**Routing + gate (stages 3–4).**
9. **Per-layer routing** (stage 4) **[m2]**.
   - A stroke with `active_target = Layer(L1)` changes
     `(L1, 1001, baseColor)` only.
   - On a 512² session, that one stroke adds exactly
     `(L1, 1001, baseColor)` plus that `(tile, channel)`'s cache:
     `plane_bytes() == 3 · 1,048,576 = 3,145,728` (Base + L1 + cache).
     This is T14's stroke-routed variant.
   - `(L0, …)` and Base are byte-identical before and after.
   - The cache equals `composite_stack_cpu` of the planes. The GPU twin
     has 3 contributing entries (Base seed + L0 + L1), so its tolerance
     is ≤ 2 LSB (the seed is exact).
   - Undo restores `L1`'s plane byte-exact and leaves `L0` untouched.
10. **The gate** (stage 3).
    - Material, Instance, Fill and Folder layers, each EXPLICITLY
      selected, refuse. `staged_batch_count() == 0`,
      `is_stroking() == false`, and `stroke_refusal ==
      Some(PaintRefusal::Layer(e))`, where `e` is the typed variant
      naming the layer.
    - **[M12]** A stack holding only a Fill, with no explicit
      selection: auto-resolution selects Base, and a stroke is ACCEPTED
      into Base. Explicitly selecting the Fill → refused, `PaintOnFill`.
    - A layer-MASK stroke with a Fill layer selected for mask editing is
      ACCEPTED: the mask plane changes and no pixel plane does.
    - Unknown id → `UnknownLayer`.
11. **Selection lifecycle** (stage 3).
    - A mid-stroke `set_active_target` does not split the stroke: every
      staged batch lands in the begin-time owner.
    - **[m6]** Stack `[P0, P1, F2, P3]` with `P3` active
      (`active_index = 3`). Removing `P3` resolves to `P1`: start at
      `stack[min(3, 3) − 1] = F2`, unpaintable, walk down to `P1`.
      Removing `P0` while it is active (`active_index = 0`) walks up to
      the nearest paintable layer.
    - An explicit Base selection survives adds, removes and reorders.
    - An empty stack → Base → strokes land in Base. This is the headless
      callers' contract: `perf_soak.rs:82`'s stroke stages exactly as
      before.

**Writers (stage 4).**
12. **Fill premultiply with coverage.** Sample `(255,0,0,255)` at
    quick-mask coverage 0.25 → plane texel `(64,0,0,64)`
    (`255·0.25 = 63.75 → 64`). Sentinel: the v1 fast path published
    `(255,0,0,255)`, dropping coverage.
13. **Material per-channel entry.** One channel, Multiply, layer 0.5 ×
    channel 0.5 over `L0 = 200` gives 170, the dynmat 2a golden
    (`dynamic-materials-design.md:649-655`) reproduced through the
    stack composite instead of the destructive publish.

**Storage + save (stages 1b, 2).**
14. **Lazy + accounting** (stage 1b) **[m2]**. A fresh 512² session
    holds exactly Base's 1001 baseColor plane: `plane_bytes() ==
    512·512·4 = 1,048,576`. `publish_plane(PlaneOwner::Layer(1), 1001,
    "baseColor", 512, 512, bytes)` adds exactly `(Layer(1), 1001,
    baseColor)` plus that `(tile, channel)`'s cache target, giving
    `3 · 1,048,576 = 3,145,728`. A layer never published holds 0 planes,
    and the cache for a Base-only `(tile, channel)` is never allocated.
    (The stroke-routed variant is T9, stage 4.)
15. **Sidecar names** (stage 2).
    - `parse("layers/7/baseColor/tile-1001.png") == (Layer(7),
      "baseColor", 1001)`.
    - `parse("layers/tile-1001.png") == (Base, "layers", 1001)`.
    - Each of these is rejected: `layers/7/tile-1001.png`,
      `layers/x/baseColor/tile-1001.png`,
      `layers/7/../tile-1001.png`, `layers/7/a\b/tile-1001.png`.
16. **Round trips** (stage 2).
    - A v2 save → load gives byte-identical planes per owner, and the
      cache after the first flush equals the pre-save cache.
    - A v1 project (flat `tile-1001.png` + `height/tile-1001.png`) loads
      all into Base, and the first frame's reader bytes equal the files
      exactly.
    - A version-3 `project.json` is rejected with `UnsupportedVersion(3)`,
      and its message names the range `1..=2`.
    - A session with no live layer plane saves version 1. One with a
      live layer plane saves 2. One whose only layer planes are orphans
      (owner not in the stack) saves 1.
    - A v2 `project.json` beside a v1-layout sidecar loads as migration.
17. **Planes never under `layers/<set>/`** (stage 2). A save → save
    cycle keeps every `paint/layers/…` file: the regression guard
    against `prune_stale_layer_files` (`project.rs:436-480`).
    **[m10]** Delete a layer that has planes, then save: no
    `paint/layers/<id>/` directory survives, empty or not, and every
    other layer's directory does.
18. **Dormancy** (stage 4, IN THE STAGE-4 GATE) **[B2]**. A v1 project
    (v1 manifest header) with a Fill layer, a mesh and non-empty Base:
    - it opens with that layer clean, dormant and planeless;
    - the first frame AND the frames after the mesh arrives are
      byte-identical to the files (no re-eval, no double-count);
    - an explicit refresh materializes its plane;
    - saving while it is still dormant writes the v2 header plus
      `# dormant <id>`, and reopening that save is dormant again and
      byte-identical.

    Sentinel: with `ensure` instead of `ensure_open`, the fill evaluates
    on the first mesh frame and the composite double-counts.
19. **Resize** (stage 4). `resize_set` drops every plane's contents on
    the set's tiles, and other sets' planes are byte-identical. **[M5]**
    The resized set's tombstones are dropped too, and other sets'
    tombstones survive.

**r2 additions (the review's binding tests).**
20. **Save-as keeps clean planes** (stage 2) **[B1]**. Paint L1 + L2
    (via `publish_plane`) and Base, then save to dir A. Change L1 only,
    then save to a NEW dir B, and reopen B. L2's and Base's planes are
    byte-identical to their pre-save readback, and B's manifest lists
    them. Variants:
    - An in-place save to A right after opening A reads back NOTHING:
      the readback count is 0.
    - Corrupt A's L2 sidecar on disk, then save in place: L2 is read
      back (hash mismatch), not re-staged.
    - Sentinel: r1's skip gives an empty L2 and Base in B.
21. **Save writes Base, not the cache** (stage 2) **[B3]**. Publish a
    non-Base `Layer(1)` plane on `(1001, baseColor)` so a cache
    materializes, then flush and save. The bytes of
    `paint/tile-1001.png` decode to Base's plane exactly, ≠ the cache's
    bytes. Reopen, and the cache equals the pre-save cache (no
    double-count).
22. **Premultiplied eraser golden** (stage 4) **[M1]**. Layer plane
    texel `(200,0,0,255)`, alpha-only erase (`EraseChannels::default()`)
    at coverage 0.5 (`k = 0.5`, exact in f32) → plane `(100,0,0,128)`.
    The test drives the erase kernel at a texel whose
    `gated_coverage` (`retouch.rs:315-316`) is exactly 0.5. The kill
    picks a dab construction that yields it, e.g. a 0.5 quick-mask
    value over a full-coverage dab center, and asserts the coverage
    first. The exact-0.5 dab shape is unverified until the kill reads
    `coverage`.
    `200·0.5 = 100.0` exactly. `255·0.5 = 127.5` is a TIE, which
    `f32::round` rounds half away from zero, a defined (not
    implementation-defined) rule → 128. Composite over opaque white:
    - `r = 1 + (100/128 − 1)·128/255 = 1 − 28/255 = 227/255 → 227`
      (exact);
    - `g = b = 1 + (0 − 1)·128/255 = 127/255 → 127` (exact).

    Sentinel: the unscaled-rgb plane `(200,0,0,128)` clamps
    `s = 1.5625 → 1.0`, giving `r = 255`. Assert 227 and ≠ 255.
    Companion: the same erase on a BASE plane gives today's
    `(200,0,0,128)` byte-for-byte (§1.4's exemption).
23. **Base is reachable, retouch samples the composite** (stage 4)
    **[M3]**. Stage 3 forces every accepted stroke into Base, so a
    Base-erase assertion could not fail there. T11 covers stage 3's
    selection half.
    - A v1 project with one Paint layer: select Base, then erase. Base's
      plane alpha changes, and the layer plane is absent or unchanged.
      Sentinel: with the Paint layer selected instead, the same erase
      leaves Base byte-identical.
    - Clone onto a NEWLY added empty layer whose source region shows
      Base paint: the new layer's plane becomes non-empty and equals the
      cloned composite bytes. Its texels satisfy `p ≤ a`, with legacy
      texels clamped.
    - Sentinel: the layer-only sample leaves the plane empty.
24. **Version write sites** (stage 2) **[M2]**. A umber-cli agent
    session save writes `version: 1`, and the wire `version` field
    reports 1. `ProjectModel::new().version == CURRENT_PROJECT_VERSION
    == 1`. The app save of a session with a live layer plane writes 2.
    Re-saving a loaded version-2 project after deleting its only planed
    layer writes 1.
25. **Tombstones** (stage 4) **[M5]**.
    - A paint layer's delete → undo revives byte-identical planes.
    - With the cap lowered below one tombstone's size, a single
      oversize tombstone is retained while it is the newest, and its
      undo revives byte-identical. A second delete then frees the first
      (oldest), and undoing the first revives it EMPTY with the status
      line.
    - Retirement reads back at most one plane per frame: a 3-plane
      delete needs 3 frames.
26. **Mask strokes dirty the composite** (stage 4) **[M6]**. A mask
    stroke on a 2-channel material with planes on tile 1001 leaves
    `cache_dirty` holding exactly 2 entries, `(1001, ch_a)` and
    `(1001, ch_b)`, each a `Rects` of the stamped footprint (no
    `Full`). The next flush makes the cache equal `composite_stack_cpu`
    with the edited mask.
27. **Same-frame producer ordering** (stage 5) **[M7]**. A fill `F` and
    a material `M` anchored on `F` are both dirtied in one frame. `M`'s
    anchor bytes equal the composite through `F`'s NEW plane, not its
    previous one, and the end-of-update flush shows both in that frame's
    cache. Sentinel: the pre-drain composite reads `F`'s old bytes. The
    stage-4 half (the flush shows a fill refreshed this frame, not next
    frame) gates stage 4.
28. **Dirt shape** (stage 4) **[M8]**.
    - Two footprints at x≈100 and x≈3996 on one 4K `(tile, channel)`
      give 2 rects, not one near-full-width union.
    - A 9th disjoint rect merges the pair with the smallest union.
    - A metadata `Full` on a non-displayed channel is NOT composited by
      the frame flush: its dirt is still `Full` after the flush, and a
      `read_channel_tile` on it composites first and returns
      `composite_stack_cpu`'s bytes.
29. **Imported map renders** (stage 4) **[M9]**. Importing a 512² AO
    PNG for tile 1001 materializes `(Layer(id), 1001, "ao")` holding the
    decoded bytes, premultiplied. A mismatched size is refused with a
    status line and no plane. The prior selection is unchanged.
30. **No cross-document bleed** (stage 2) **[M10]**. Open A, where
    `Layer(3)` has planes. Then open B, where `Layer(3)` is a Fill with
    no planes. After B's load, no `(Layer(3), …)` plane exists and
    `RetiredPlanes` is empty. B saves version 1. `reset_for_new` also
    leaves `fill_states` empty.
31. **v2 planeless fill is not dormant** (stage 4) **[M11]**. A project
    with the v2 manifest header and a planeless Fill (not listed
    dormant) opens NON-dormant and dirty, and it refreshes into its own
    plane on mesh arrival. Its row shows no dormant badge.
32. **Mask edit and selection reconcile** (stage 3) **[m5]**. While
    editing L2's mask, add a layer, then stroke. The new layer is
    selected and `mask_edit_layer` is cleared; at stage 4 the new
    layer's plane changes. L2's mask is byte-identical.
33. **Budget refusal** (stage 1b, stroke half at stage 4) **[M4]**.
    With `PLANE_HARD_CAP` lowered to exactly
    `plane_bytes() + 512·512·4 − 1`, `publish_plane(PlaneOwner::Base,
    1001, "height", 512, 512, bytes)` returns
    `Err(PaintRefusal::PlaneBudget { bytes })`, where `bytes ==
    plane_bytes() + 1,048,576`, and no plane is created. The test uses
    a Base plane on purpose: a new LAYER plane would also materialize a
    cache texture (§4.2), and both count toward the cap. A publish into
    an EXISTING plane still succeeds. Stage 4: a stroke
    whose next tile would cross the cap sets `stroke_refusal` and keeps
    painting its already-materialized tile.
34. **Owner deleted mid-stroke** (stage 4) **[m7]**. Begin a stroke on
    `L1`, remove `L1` while stroking, and run 3 frames. The census does
    not retire `L1`'s planes while the stroke lives. `end_stroke`
    retires them, discards the capture (history length unchanged), and
    logs.

## 9. Non-goals (what this design does NOT do)

1. **No virtual texturing or sparse pages.** Planes are full-size
   `PaintThread`s. The atlas, page table and 512² paging that
   `tile_pool.rs:3-11` names as Wave-3 work stay deferred. That is the
   real fix for §1.3's 7.5 GiB UDIM row, which the hard cap refuses
   loudly in the meantime. It is a v3 surface.
2. **No plane residency or eviction.** Planes stay GPU-resident while
   their layer lives, and only retired PAINT planes move to CPU.
   Evicting cold planes needs a re-upload-for-composite story. §1.3's
   budget warns at 2 GiB and refuses NEW planes at the hard cap; it
   never evicts.
3. **No per-layer CPU composite in the live path.** The CPU oracle is the
   test truth and the anchor partial-flatten engine (§5.3); the viewport
   cache is GPU-composited and never saved. A GPU anchor fast path is a
   measured follow-up.
4. **No folder membership or isolation.** Folders keep passing `None`
   (compositor judgment 4). Non-passthrough folder flattening waits on
   membership in umber-core.
5. **No multi-stack (per-set) keys.** Plane and mask keys omit the set
   (§1.1). Per-set stacks add it to both at once.
6. **No "sample current layer only" retouch toggle.** Smudge and clone
   sample the composite by default (§3, reversed in r2); the
   layer-only toggle is the follow-up.
7. **No pixel ops in the autosave journal, and no selection
   persistence.** These are the unchanged v1 boundaries (§6.4, §2.1).
8. **No Base → layer conversion action, and no multi-entry material
   sub-stacks.** The first is a follow-up; the second is the dynmat
   doc's follow-up (`dynamic-materials-design.md:795-798`), unblocked by
   this design but not built.
9. **No layered PSD export of real layers.** It becomes possible (planes
   exist) but stays the export row's follow-up (`requirements.md:100`).
10. **No per-channel masks, no cross-tile stroke continuity, no
    per-channel dab density.** These are unchanged named follow-ups
    (`dynamic-materials-design.md:804`, `udim-design.md:74-82`,
    `paint_state.rs:159-162`).
11. **No tombstones for eval-owned planes, no mask tombstones.** Fills
    and materials revive by re-eval (§5.4). Masks restart white on
    re-attach, as today.
12. **No persistent per-layer GPU mask textures.** The rect upload
    stands until measurement says otherwise (§4.3 item 3).

## Open verifications (carried into the stage kills)

Each one is a pre-landing read, per CLAUDE.md hard rule 4. The doc's
design doesn't depend on the outcome, only the code shape does.
- **U1 (stage 1b).** Does eframe/egui-wgpu 0.36 install a device
  uncaptured-error handler? Read `~/.cargo/registry/src/*/egui-wgpu-0.36*/`.
  It changes M4's failure framing, not the binding cap.
- **wgpu 30 error scopes (stage 1b).** The exact shape of
  `push_error_scope`/`pop_error_scope` in wgpu 30 (whether it is a pair
  or a guard, and whether `pop` is a future). Read
  `docs/claw-artifacts/wgpu/` and then the registry source before
  writing §1.3's OOM scope.
- **Shared compositor (stage 1b).** Whether `PaintCompositor` is
  `Send + Sync` as built (`paint.rs:627-640`), so one `Arc` can serve
  every `PaintThread`. If it is not, the kill shares the pipeline and
  layout objects inside it instead, with the same no-mid-stroke-compile
  outcome.
- **U2 (stage 4).** WGSL un-premultiply exactness per vendor. Covered by
  the twins' `≤ n_contributing` tolerance.
- **U4 (stage 4).** egui's per-frame change cadence for the material
  channel opacity widgets during a drag. M8's coalescing bounds the cost
  either way; the perf gate measures it.

## Requirements traceability

| Row (`docs/specs/requirements.md`) | Today | After this design | Stage |
|---|---|---|---|
| :46 Dynamic material layering (P2) | v1 on destructive publish, four warts | warts 1–3 closed (§5.3), wart 4 closed, Instance-of-material renders | 4, 5 |
| :37 Layer types: paint/fill/folders | layers are metadata; strokes paint the canvas | paint and fill layers own pixels; strokes target the selected layer or Base | 3, 4 |
| :19 Eraser ("sets layer alpha to zero") | erases the flat baseColor target | erases the active target's plane: premultiplied erase on layer planes, today's semantics on Base | 4 |
| :40 Masks | planes exist; gate nothing live for paint layers (F1) | gate every layer in the live composite; mask strokes recomposite | 4 |
| :41 Layer instancing | instances render nothing (compositor judgment 4) | instances render their source's planes | 5 |
| :42 Anchors | full flattened read, self-feedback | downward partial flatten (CPU, deterministic), typed self/forward errors | 5 |
| :120 Project = directory + per-layer files | per-layer JSON; pixels flat | per-layer pixel files (`paint/layers/<id>/…`), version 2 only when a live layer plane is saved | 2 |
| :124 Merge story | Property 3 misdescribes pixel storage (§6.5) | different layers never conflict on PIXELS either | 2, 4 (+ doc delta) |
| :164 Input-to-photon < 20 ms | stroke = splat only | splat + region recomposite; re-measured before stage 4 lands, with drag and mirror scenarios | 1a, 4 |
| :167 Undo RAM | regions keyed (tile, channel) | regions keyed (owner, tile, channel); envelope unchanged; tombstones capped at 512 MiB | 4 |

No row is marked ✓ by this kill: it is a design doc. Each stage's kill
marks its rows.

## Deviation log (doc vs brief)

- **Brief: "LayerPixels keyed (layer_id, tile, channel)".** The key is
  `(PlaneOwner, tile, channel)`. Layer ids start at 0
  (`layers.rs:905-911`), so no id can mean "no layer". The headless
  callers, the empty-stack document, and (r2) the selectable Base row
  need an ownerless target (§1.4).
- **Brief: "check_paintable (… Material/Instance arms)".** It also gains
  Fill and Folder arms (§2.3, judgment call).
- **Brief: "anchors read the FLATTENED cache".** Specified as the
  flatten TRUNCATED at the anchored layer (§5.3). Reading the full cache
  is exactly wart 4, so "never self-feedback" requires the truncation.
- **Brief: "a version bump or a dual-read loader?".** Both, plus a
  minimal bump (2 only when a live layer plane is saved), implemented
  as an opt-in constant so the default write version stays 1 (§6.3).
  The dual read keys on sidecar content, not on the version.
- **Brief's grounding: "the composite machinery ALREADY EXISTS".** True
  for blending (F1). It lacked three things this design needs:
  premultiplied sources (F3), a verbatim seed, and region dispatch
  (§4.3). Stage 1a landed all three.
- **Not in the brief: `umber_gpu::TilePool`.** It already exists keyed
  by layer. It is rejected as the store (F2), and its memory model is
  reused.

### r2 deviations from the review's binding fixes (each with its reason)

- **M11's key.** The review keys dormancy on "the LOADED project.json
  version is 1 (equivalently, the manifest lacks the v2 header)". The
  two are NOT equivalent under M2's minimal bump. A session whose fill
  never evaluated saves version 1 with no planes, which recreates M11's
  false badge. r2 keys on the manifest header ALONE, writes that header
  from stage 4 only (a stage-2/3 build still bakes fills into Base), and
  persists still-dormant ids as `# dormant <id>` lines. This supersedes
  r1's "dormancy is never persisted". Without the list, the first save
  of an untouched v1 project would write the v2 header, and the next
  reopen would double-count.
- **B2's split.** The review allows stage 5 to keep "the dormant badge's
  UI polish". r2 lands the badge itself in stage 4 (a dormant layer
  without its badge is an unexplained non-rendering layer) and leaves
  only polish to stage 5.
- **M3's Base row.** Its SELECTION moves into stage 3 as bound. Its
  VISIBILITY toggle lands at stage 4: before stage 4 there is no live
  composite, so hiding Base would have nothing coherent to show.
- **M8's coalescing set.** The review names "baseColor + height" of the
  active tile. r2 adds the UV canvas's bound channel when that panel is
  open (`uv_view.rs:351` binds the cache). Otherwise the UV view would
  show a stale composite during a drag.
- **m11.** The review offered two options. r2 picks the CPU-oracle
  prefix (deterministic), so §5.3, §6.1, §6.5 and §9.3 agree. The GPU
  fast path is a follow-up.
- **m3.** T5's L2 is 60, the review's suggested derivable sentinel. All
  three values recompute ≥ 0.1 from a boundary (114.804, 110.245,
  50.882).
- **m1.** The fix landed with stage 1a (the literals were edited in
  `b1ceb2c`), so T4 records the landed form instead of prescribing a
  constructor.
- **Test numbering.** r2 adds T20–T34 rather than renumbering, because
  committed code cites T1–T3.
