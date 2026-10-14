# Per-Layer Pool — Design

Date: 2026-10-14 (doc-only kill, lock `perlayer-pool-doc`).
Status: DRAFT (pre-review; the staged implementation kills of §7 derive
from this doc).
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
`StackCompositor::composite_stack` (`umber-gpu/src/compositor.rs:1037`)
and its CPU oracle `composite_stack_cpu` (`compositor.rs:740-797`) fold a
`[GpuStackLayer]`/`[CpuStackLayer]` bottom-to-top with the full 32-mode
set and layer masks. Outside umber-gpu the only consumers are
`tests/gate0_journey.rs:242-247`, which composites one layer, and
`material_layer.rs:76`, which uses the per-pixel primitive. Its module
docs already assume per-layer storage ("the tile pool owns one GPU target
per (layer, channel, tile)", `compositor.rs:3-14`). This design decides
WHEN it runs and WHAT it reads; it does not invent a compositor.

**F2 — `umber_gpu::TilePool` is NOT the storage this design uses.**
It is keyed `(layer, channel: u8, tile: TileId)` (`tile_pool.rs:129-139`).
The `u8` channel is the `ChannelKind` tag (Color/Data, `:142-145`), so
`roughness`, `metallic` and `height` (all Data) collide on one key. Its
`TileId` is a 512² grid cell, not a UDIM tile (`tile_pool.rs:34-44,85`).
It has no consumer outside its own tests. Its memory model (`:46-70`,
"10-layer × 5-channel × 4K = 3.2 GiB") and its advisory cap
(`RECOMMENDED_MAX_RESIDENT_BYTES`, `:95`) ARE reused in §1's budgeting.

**F3 — The flat pool mixes three alpha conventions today.**
- Strokes are **premultiplied**: the dab splat computes `src =
  dab.color * coverage; out = src + dst * (1 - src.a)`
  (`umber-gpu/src/shaders.rs:584-587`).
- Fills are **straight**: `evaluate_fill_tile` writes the sample with its
  own alpha and carries coverage as a separate plane
  (`umber-mesh/src/fill.rs:690-694`). The non-fast path divides by
  `oa` (`fill_layer.rs:364`). The fast path publishes the sampled bytes
  and DROPS fractional coverage (`fill_layer.rs:413-426`).
- Materials go through `composite_pixel_masked`'s straight-alpha lerp
  (`material_layer.rs:496-540`).

The compositor treats every source as STRAIGHT alpha: `cov = src_a ·
opacity · mask; out_rgb = base + (blend − base)·cov`
(`compositor.rs:693-729`). It starts from a transparent accumulator
(`compositor.rs:765`), and `blend_channel` clamps its source to 0..1
(`layers.rs:343`). Feeding a premultiplied stroke plane through it
unchanged double-multiplies soft edges. For a texel `P = 100, A = 128`
over transparent: `(100/255)·(128/255) = 0.196847 → 50`, where today's
flat target shows 100. Any per-layer design has to resolve this before
it can promise a byte-identical migration (§4.3).

## 1. Storage: planes keyed (owner, tile, channel)

### 1.1 The key

```rust
/// Who owns a plane. Layer ids start at 0 (`LayerStack::new`,
/// layers.rs:905-911), so no id value can double as "no layer":
/// the ownerless plane is its own variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaneOwner {
    /// The session's ownerless bottom plane (§1.4).
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
    /// Set by every write (stage, WriteRegion, publish); cleared by a
    /// committed save. The save skips the readback of a clean plane (§6.2).
    dirty_since_save: bool,
}
```

Each plane is a `PaintThread` (`umber_gpu::paint_thread`), the same
type every target is today. That keeps strokes, `WriteRegion`,
`WriteMask` and `Resize` unchanged per plane.

Today's `TileTargets` threads MOVE into `planes` as the `Base` owner's
planes (§1.4). Tile 1001's Base baseColor is still created in
`PaintState::new`, so `paint_target()`'s invariant
(`paint_state.rs:1873-1880`) holds. `TileTargets` keeps its
`base_color` + `channels` shape with a new role: the **composite cache**
(§4), populated LAZILY (§4.2's lazy cache rule).

The reader accessors (`tile_target`, `channel_target`, `paint_target`,
`read_tile`, `read_channel_tile`) resolve "cache if materialized, else
the Base plane". Every reader of them is therefore untouched:
`viewport.rs:256,550,558`, `uv_view.rs:351`, `bake_sources.rs:82,99`
and `main.rs:1006`.

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
assumption (`paint_state.rs:660`, keyed `(layer, tile)`). When per-set
stacks land (multi-set documents), the set joins BOTH keys in one change.
That is recorded as a v2 boundary (§9), not designed here.

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
- The load path materializes exactly the planes the sidecar lists (§6).
- A layer that was never written has NO planes. The composite treats a
  missing plane as `None`, i.e. a transparent no-op under every mode
  (compositor judgment 2, `compositor.rs:34-43`), so "reads composite
  around it" holds by construction.
- Size: a plane is created at `tile_channel_size(tile, channel)`
  (`paint_state.rs:899`), so an override applies per plane exactly as it
  does to targets today.

Reads never materialize, as the existing rule requires: `read_channel_tile`
never side-effect-creates (`paint_state.rs:2620-2627`). The new plane
reader `read_plane(owner, tile, channel) -> Option<RgbaBuffer>` keeps
that contract.

### 1.3 The memory story

One RGBA8 plane at the 4096 default is `4096² × 4 = 67,108,864 B =
64 MiB` (2048: 16 MiB; 8192: 256 MiB). Today one tile with 5 routed
channels holds `5 × 64 = 320 MiB` of targets (`paint_state.rs:113-118`).
Under this design the cache stays and planes are added. Worked cases at
4K, one tile:

| Session | Planes | Cache | Total |
|---|---|---|---|
| Today (flat only) | 0 | 5 × 64 MiB | 320 MiB |
| 6 paint layers, 2 channels each | 12 × 64 = 768 MiB | 320 MiB | 1,088 MiB |
| + 2 materials × 4 declared channels | + 8 × 64 = 512 MiB | 320 MiB | 1,600 MiB |
| Same, 4 UDIM tiles (every layer on every tile) | 80 × 64 MiB | 4 × 320 MiB | 6,400 MiB |

The dominant cost is fills and materials, not strokes. Eval-owned planes
are DENSE: every present tile × every declared channel, because
`refresh_dirty_materials` walks `tiles()` (`material_layer.rs:740-741`).
A stroke plane appears only on tiles a stroke touched. This is the same
residency arithmetic `tile_pool.rs:50-58` already wrote down ("3.2 GiB"),
now attached to a real consumer.

**Budgeting.** NEW constant `PLANE_BUDGET_BYTES = 2 GiB`, which is 32
planes at 4K or 128 at 2K. `PaintState::plane_bytes()` returns the exact
sum of `w·h·4` over materialized planes and the cache. Crossing the
budget logs once and sets a status-line flag; the Texture Sets panel
names it beside the size combo.

JUDGMENT CALL — advisory, never enforced, exactly like
`RECOMMENDED_MAX_RESIDENT_BYTES` (`tile_pool.rs:60-70`). Refusing a stroke
or a fill refresh at a byte line is a silent data-path failure the
never-crash doctrine forbids. Evicting needs a residency story that §9
defers. The number is twice the tile pool's 1 GiB, because cache + planes
double-count by design (the cache is derived). Revisit it against
`docs/perf/protocol.md` measurements at stage 4, not before.

`LayerStack::memory_bytes` stays metadata-only (`layers.rs:1378-1389`,
the honesty contract). Plane bytes are reported by `PaintState`, never
by `Layer`, as the material eval cache already is
(`material_layer.rs:39-41`).

### 1.4 The Base plane

`PlaneOwner::Base` is the session's ownerless plane. It composites at the
very bottom: Normal, opacity 1, visible, unmasked. It holds:

1. **Strokes with no selected layer.** That means the empty-stack
   document (today's fresh-session behavior) and the headless callers
   that drive `PaintState` with no document at all:
   `bake_sources.rs:637,827`, `input_photon.rs:269`, `perf_soak.rs:82`,
   `main.rs:3386`. Each keeps painting byte-identically.
2. **Migrated v1 paint** (§6.3): a v1 project's flattened tiles load
   into Base, unchanged.

Base is what makes stage 1 a pure refactor. Before any layer-owned plane
exists, Base IS today's `TileTargets` content, and readers read it
directly (the lazy cache rule, §4.2).

## 2. The active-layer seam

### 2.1 Where the selection lives

```rust
// NEW fields on PaintState, beside mask_edit_layer (paint_state.rs:657-667)
/// The layer new strokes paint (`None` = the Base plane — only while the
/// document stack is empty, or for headless callers; see §2.3). The
/// Layers panel edits this through the shell; `begin_stroke` snapshots it.
active_layer: Option<u64>,
/// The shell's last paintability verdict for `active_layer`
/// (`LayerStack::check_paintable`), pushed between strokes.
active_verdict: Result<(), InstanceError>,
/// The live stroke's owner, snapshotted at `begin_stroke`.
stroke_owner: PlaneOwner,
/// Why the last `begin_stroke` refused (None after an accepted stroke).
stroke_refusal: Option<InstanceError>,
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

The selection is not persisted in v2. On open the shell selects the
topmost paintable layer (§2.3).

### 2.2 How the Layers panel sets it

`layers_ui(ui, doc, mats)` (`document.rs:277-281`) takes no `PaintState`,
and the material context borrows the shell's state mutably, so the
selection cannot be written from inside it. NEW surface:

```rust
/// What the Layers rows asked for this frame.
pub struct LayersUiOutcome {
    /// `Some(id)` = a row's name was clicked (select it).
    pub select: Option<u64>,
}
pub fn layers_ui(ui, doc, mats, active: Option<u64>) -> LayersUiOutcome
```

- The row name becomes a `selectable_label(active == Some(id), name)`.
  It is the same widget the visibility toggle already uses
  (`document.rs:355-357`).
- The add buttons (`document.rs:285-302`) select the new layer, matching
  Painter's add-then-paint behavior. The new id is the stack top after
  `doc.run(LayerCommand::add(..))`.
- The shell applies the outcome through `paint.set_active_layer(id)`,
  the `layer_mask_ui` precedent of mutating the paint session from a
  panel action (`document.rs:521`).
- No new user-facing strings: the row name is the label.

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
- A folder carries no pixels (compositor judgment 4, `compositor.rs:
  50-56`).
- Painter refuses both. You paint a fill's MASK, which stays allowed
  (below).
- Field names are `layer`, never `source` (the thiserror trap, CLAUDE.md).

**Mechanism** (`PaintState` cannot call `check_paintable` itself):

1. Every frame, before panels run, the shell revalidates the selection
   (the fill-adoption precedent, `main.rs:2906-2924`):
   - `None` while the stack is non-empty becomes the topmost paintable
     layer. If none is paintable, the topmost layer is selected anyway,
     so the refusal names a reason instead of falling back to Base.
   - An id no longer in the stack becomes the nearest layer below it,
     else the one above, else `None`. This is the `prune_layer_masks`
     shape (`paint_state.rs:1370-1374`).
   - It then pushes `sync_active_layer(active, verdict)`, where `verdict`
     is `doc.stack.check_paintable(id)` (or `Ok(())` for `None`).
     `sync_tool`/`sync_symmetry` are the precedents (`paint_state.rs:
     1067,1097`).
2. `begin_stroke_styled` (`paint_state.rs:976`) decides in this order:
   - `mask_edit_layer.is_some()`: a layer-MASK stroke. The gate is
     bypassed, so painting a fill's or material's mask is legitimate.
     This matches the existing hook order (`paint_state.rs:2340-2352`).
   - Quick-mask edit mode: paints the session mask, gate bypassed.
   - Otherwise, `active_verdict` is `Err(e)`: **refuse**. `stroking`
     stays `false`, `stroke_refusal = Some(e)`, nothing is staged, no
     capture opens. The shell renders `stroke_refusal` on the status
     line through the existing `layers.*` unpaintable hints.
   - Otherwise accept, and `stroke_owner` is `Layer(id)`, or `Base` when
     `active_layer` is `None`. (Stage 3 lands the gate with
     `stroke_owner` forced to `Base` for every accepted stroke; stage 4
     switches on `Layer(id)` routing, §7.)
3. `replay_path`/`replay_filled_path` (`paint_state.rs:1764,1811`) begin
   strokes through the same function, so they inherit the gate.

The live stroke snapshots `stroke_owner` like `stroke_channel`
(`paint_state.rs:1012`): a mid-stroke selection change never splits a
stroke across two planes.

**Boundary (v2).** The verdict is one frame old. Kinds never change, as
no command mutates `kind` (`dynamic-materials-design.md:243-249`), so
the only stale case is a layer deleted earlier in the same frame that a
stroke then begins on. That stroke lands in a plane whose owner is gone.
The next frame's sync retires it through the tombstone path (§5.4).
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

Concretely, every place that resolves a target by `(tile, channel)`
takes the owner too:

- **Stage** (`paint_state.rs:2432-2435`):
  `plane_of_mut(stroke_owner, tile, &channel)` replaces
  `target_of_mut(tile, &channel)`.
- **Undo capture** (`capture_paint_event`, `:2459-2499`): the snapshot
  reads the PLANE. `PaintStrokeCapture.tiles` is keyed `(owner, tile)`.
  `StrokeTile` and `PaintWrites` (`retouch.rs:331-337,490-493`) gain an
  `owner` field. Undo/redo write into the plane, never the cache. The
  history envelope (64 entries / 256 MiB, `paint_state.rs:455-456`) is
  unchanged: regions are rects of one plane, the same bytes as today.
- **Retouch** (`retouch_event`, `:2504-2539`): the CPU mirror is read
  from and written back to `(stroke_owner, tile, stroke_channel)`. This
  lifts today's baseColor-only retouch boundary (`:2525-2527`) with no
  extra cost.

  JUDGMENT CALL — smudge and clone sample the LAYER's plane, not the
  composite. "Sample merged" needs a composite readback per stroke, which
  is a follow-up (§9).
- **Eraser**: erases the active layer's plane alpha. Requirement row 19
  ("sets layer alpha to zero") becomes literally true.
- **Quick-mask gate** (`sync_mask_tile`/`publish_mask`, `:2567-2607`):
  the GPU mask copy rides the thread the dab stages into, which is now
  the plane's. `mask_synced` is keyed `(owner, tile, channel)`. This
  closes the "non-baseColor channel's OWN GPU mask copy" follow-up noted
  at `paint_state.rs:2588-2591`.
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

**Every stroke write marks the cache dirty** (§4.2): the union of the
event's dab footprints, the same `footprint` the undo capture already
computes (`umber_brush::retouch::footprint`), per `(tile, channel)`.

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
| Stroke event | dab splat into the flat target (footprint texels) | dab splat into the plane + region recomposite: `L_c × |R|` texel-ops (`L_c` = contributing entries, `R` = dirty rect) |
| Fill/material refresh | BLOCKING full readback (64 MiB) + CPU composite of 16.8M texels + 64 MiB upload, per tile×channel (`fill_layer.rs:414-426`, `material_layer.rs:12-18`) | eval → 64 MiB upload into the OWN plane (no readback) + GPU recomposite `L_c × 16.8M` |
| Hide / opacity / blend / reorder / delete | nothing happens (warts 1, 3) | full recomposite of the changed layer's `(tile, channel)`s |
| Masked layer in a recomposite | n/a (the app never composites, F1) | today's `composite_stack` uploads each masked entry's WHOLE plane per dispatch (`compositor.rs:1029-1036`, by design: "nothing to sync", `paint_state.rs:286-290`), i.e. 16 MiB per masked layer at 4K. The region path uploads only the rect's mask bytes (§4.3 item 3): `|R|` bytes per masked layer per stroke frame |
| Save | 1 readback per cache target | 1 readback per DIRTY plane; the cache is never saved (§6) |
| Viewport / export / bake read | reads the flat target | reads the cache (unchanged readers), fresh per §4.1's read contract |

At 4K a stroke frame's rect is small. A brush of r = 24 texels has a
49×49 footprint (`paint_state.rs:91-92`), and a 4-dab event union is on
the order of 150×50 = 7,500 texels. That puts a 6-layer recomposite at
about 45k texel-ops, against a full-tile 4K recomposite of `6 × 16.8M =
100M`. The region path is what keeps CoR inside the input-to-photon
budget (`requirements.md:164`, < 20 ms).

CoR also removes the composite-time readback. Today a non-Normal or
partial-opacity fill (`fill_layer.rs:413-414`; the Normal-at-1.0 fast
path already skips it) and every material channel
(`material_layer.rs:12-18`) block on a 64 MiB readback of the flattened
tile per tile × channel. Under CoR their planes never read the cache.
The one readback that remains is an anchored material's
partial-flatten read, at one per `(anchor, tile)` (§5.3).

JUDGMENT CALL — CoR, with an EAGER per-frame flush plus flush-on-read
for CPU readers. Three parts:

1. **The flush.** It runs right after the existing per-frame drain
   (`main.rs:2894-2899`, after `process_pending`), so a stroke shows the
   same frame it splats, as today. The flush and the plane threads share
   the session's queue (`paint_state.rs:527-528`), so queue submission
   order sequences "splat, then composite".
2. **GPU binders.** The viewport's `bind_paint_target`
   (`viewport.rs:533-564`) and the UV canvas (`uv_view.rs:351`) bind
   textures that the GPU samples at render time, after update returns.
   So they see the end-of-frame flush.
3. **CPU readers that run DURING the frame flush first.** These are the
   eyedropper (`viewport.rs:256`), the painted-map export
   (`main.rs:1006`), the bake sources (`bake_sources.rs:82,99`) and
   `read_tile`/`read_channel_tile`. Following `read_target`'s
   drain-first precedent (`paint_state.rs:2637-2651`), the `&mut self`
   readers flush the requested `(tile, channel)`'s pending dirt against
   the LAST snapshot before reading back. NEW surface: `paint_target()`
   and `tile_target()` are `&self` today (`paint_state.rs:1873,1885`),
   so the export and bake call sites move to a `&mut` flushing accessor.

**Boundary (v2).** Metadata edits are discovered when the shell diffs the
snapshot at the next frame's sync, so a reader running in the SAME frame
as a visibility, opacity, blend or reorder edit sees the cache as of the
previous snapshot. The read is one edit stale, never torn, and the next
frame is exact.

### 4.2 The cache and its dirt

- `cache_dirty: BTreeMap<(u16, String), DirtyRegion>`, where
  `DirtyRegion` is `Rect(DirtyRect) | Full`. Rects union, and `Full`
  absorbs everything. Plane writes mark rects. Any change to a layer's
  composite METADATA marks `Full` for every `(tile, channel)` that
  layer has a plane on, and a reorder marks both layers'. The metadata
  is order, visibility, opacity, blend, mask enable or plane, and
  material channel params.
- **The lazy cache rule.** A `(tile, channel)` whose only plane is Base
  needs no cache texture: readers get Base directly, which is today's
  exact path at zero extra VRAM. The cache texture materializes on the
  first layer-owned plane for that `(tile, channel)`, born `Full`-dirty
  so it can never be read uncomposited. In stage 1 nothing changes for
  any reader.
- The shell builds the composite entries (`PaintState` holds no stack).
  NEW surface `CompositeSnapshot`: per frame, the stack's per-layer
  metadata bottom-to-top, built from `doc.stack.layers` and handed to
  `paint.flush_composite(&snapshot)`. The shell diffs it against last
  frame's to raise `Full` marks. Material layers expand per channel
  (§5.2).

### 4.3 The compositor surface this needs (NEW, umber-gpu)

Three additions, all default-off so every existing caller and oracle
test is byte-identical:

1. **`premultiplied: bool` on `CpuStackLayer`/`GpuStackLayer`**
   (`compositor.rs:505-574`). When it is set, a contributing texel
   un-premultiplies before the blend: `s_rgb = p_rgb / a` for `a > 0`
   (`a == 0` gives `cov == 0`, a no-op). Then
   `composite_pixel_masked` runs unchanged. For Normal over the
   transparent accumulator, `out = (p/a)·a = p`. For Normal over any
   accumulator, `out = p + base·(1 − a)`, which is exactly the paint
   shader's premultiplied over (`shaders.rs:586`).

   Every v2 plane is premultiplied, and every writer converts (§5.1,
   §5.2). Existing constructors (`from_layer`) default to `false`.

   On WGSL the flag rides `CompositeParams.pad0`
   (`compositor.rs:805-814`) as bit 0, so the 32-byte assert at
   `compositor.rs:816-819` holds.

   Adding the field breaks the struct literal at
   `crates/umber-app/tests/gate0_journey.rs:247` (E0063). Stage 1's
   surface includes that file.
2. **The bottom-copy identity rule.** When no entry has contributed yet
   and the first contributing entry is premultiplied, Normal,
   `clamped_opacity == 1.0` and unmasked, its bytes SEED the accumulator
   verbatim instead of compositing.
   - On valid premultiplied data (`p ≤ a`) this equals the composite
     exactly (item 1's identity).
   - On anything else it is still verbatim. That matters for Base, which
     holds v1's mixed-convention bytes (F3): a legacy straight texel
     `(200, 200, 200, 100)` un-premultiplies to `s = 2.0`, which
     `blend_channel` clamps to 1.0 (`layers.rs:343`), giving
     `out_rgb = 100/255 → 100`. The rule keeps 200.
   - CPU: `acc = byte/255`, and `f32_to_u8` (`compositor.rs:644-653`)
     returns the same byte.
   - GPU: a seed dispatch (bit 1 of `pad0`) or a texture copy of the
     rect, whichever the implementation proves (§Unverified in the
     kill report).
3. **Region compositing.**
   `composite_stack_region(encoder, layers, output, rect)` clears and
   recomposites only `rect`. `composite_stack` clears the WHOLE output
   (`compositor.rs:1043`), which is what makes it unusable for the
   per-event path. The rect origin rides `pad1`/`pad2`, and the dispatch
   covers `rect` only.

   A masked entry uploads ONLY `rect`'s mask bytes into a rect-sized
   `R8Unorm` texture, sampled at `texel − origin`. Today's
   `upload_mask` uploads the whole plane per dispatch
   (`compositor.rs:1029-1036`), 16 MiB per masked layer per 4K frame.

   JUDGMENT CALL — rect upload over persistent per-layer GPU mask
   textures. It keeps the existing "nothing to sync" property
   (`paint_state.rs:286-290`) and adds no mask-sync state. The cost is
   `|R|` bytes per masked layer per stroke frame. Persistent GPU masks
   are the follow-up if measurement shows the upload matters.

   The CPU oracle gains `composite_stack_region_cpu`, which returns
   `rect`'s bytes. Under the house rule (judgment 8, `compositor.rs:79-83`)
   the GPU must match it to ±1 LSB.

The compositor precondition "output must not alias a layer texture"
(judgment 5, `compositor.rs:57-64`) holds by construction: the cache is
a separate `PaintThread` from every plane.

### 4.4 What stays flat

- **The cache itself.** It is derived, never saved (§6), and is what
  every reader binds.
- **The quick mask.** It is per SET, session state, never persisted or
  undoable (`paint_state.rs:186-190,233-254`). It is a stroke GATE, not
  a layer, and gates dab coverage into whichever plane the stroke
  targets (§3).
- **Layer-mask planes.** Already per layer (`layer_masks`, keyed
  `(layer, tile)`, set-sized, `paint_state.rs:264-290`). Unchanged. What
  changes is that they finally gate something live: the app never runs
  the stack compositor today (F1), so a PAINT layer's mask plane gates
  nothing visible; only material composites read masks. Under CoR every
  entry carries its enabled mask. When a channel is overridden, the mask
  is resampled nearest-neighbour to the plane size with the existing
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
`PaintState::publish_plane(owner, tile, channel, w, h, rgba)`. That is
`publish_fill_channel`'s shape with an owner
(`paint_state.rs:1625-1655`).

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
- Size: the plane is `size_of_channel(set, "baseColor")`. The literal
  `1024` at `main.rs:2945` is RETIRED. On a 4096 set it would make the
  CPU oracle fail loudly (`InvalidPixelData`) and the GPU path zero-fill
  (compositor judgment 7, `compositor.rs:73-78`).
- Fills stay baseColor-only (unchanged v1 boundary). Multi-channel
  procedural content is what materials are for.

### 5.2 Materials

`refresh_dirty_materials` (`material_layer.rs:776`) keeps discovery,
validation, overrides, eval, the Uniform/blend per-channel checks and
resampling (`material_layer.rs:255-453`). The per-channel step changes
from "read flattened → `composite_material_channel` → publish" to:
write the resampled output, premultiplied, into `(Layer(id), tile,
channel)`.

- `MaterialTileIo` (`material_layer.rs:735-766`) loses `read_channel`
  for the publish path. `publish_channel` targets the layer's plane.
- The PER-CHANNEL blend/opacity/enable apply at COMPOSITE time. The
  entry builder emits one entry per `(material layer, channel)` with:
  - `blend = channel.blend`
  - `opacity = layer.opacity × channel.opacity` (formed first, clamped
    once, `dynamic-materials-design.md:376-378`)
  - `mask` = the layer mask
  - `pixels = None` when the channel is disabled, undeclared, or carries
    a forbidden blend. That mirrors `ChannelStatus::SkippedDisabled`/
    `SkippedBlend` (`material_layer.rs:175-187`).
  - The layer's own `blend_mode` stays inert (`material_layer.rs:29-31`).
- A failed eval writes nothing (whole-material no-op, unchanged), so the
  layer's planes keep the last good output.

### 5.3 The four warts, closed

| Wart (`material_layer.rs:20-28`) | Why it closes |
|---|---|
| 1. Stack order not preserved | The composite folds entries in stack order on every flush. A material's plane holds only ITS output, so where it sits in the stack is where it composites. Reorder raises `Full` (§4.2). |
| 2. Re-eval compounds | A refresh REPLACES the plane (`WriteRegion` of the whole plane). Composite of identical plane bytes gives identical cache bytes. Refreshing twice is idempotent (§8 T4: 170, 170, never 144). |
| 3. Nothing un-publishes | Hide → the entry is skipped (`visible: false`). Disable a channel → `pixels: None`. Delete → the layer's planes are retired (§5.4) and leave the composite. Each raises `Full` and the next flush recomposites without it. Undo of `SetMaterial` restores params, which re-dirty, which re-eval REPLACES the plane, so pixel-exact undo of material edits is now automatic. |
| 4. Anchors self-feed | See below. |

**Anchors: downward partial flatten.** `AnchorRef` is a typed
`(layer, channel)`. Today `PaintMaterialIo::anchor_image` uses the layer
only as a liveness check and returns the FULL flattened channel
(`main.rs:675-687`).

NEW rule: an anchor on layer `A`, channel `c`, resolves to the composite
of the stack's entries for `c` from the bottom THROUGH `A` (inclusive).
It is "the flattened cache", truncated at the anchor.

Planes are GPU-resident, so the partial flatten runs on the GPU:
`composite_stack` over the prefix entries into a scratch `PaintTarget`
at the channel's size, then ONE readback for the CPU graph eval. That is
the same single 64 MiB read `anchor_image` costs today
(`main.rs:675-687` → `read_channel_tile`), not one readback per plane.
`composite_stack_cpu` over the same prefix is the test oracle (±1 on GPU). A material
`M` may anchor only layers strictly BELOW it:

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
needs it (follow-up, §9). Cost: one GPU prefix composite plus one
readback per `(anchor, tile)` per refresh, i.e. today's anchor read plus
`L_prefix` dispatches.

**Instances render.** `LayerKind::Instance` entries pass `None` today
(`compositor.rs:50-56,525-526`), and the dynmat doc names "no per-layer
result to reference" (`dynamic-materials-design.md:163-166`). With
planes, an instance's entry reads its SOURCE's planes (transitively,
via `instance_eval_order`, `layers.rs:1255`), with the instance's own
`visible`/`opacity`/mask.

JUDGMENT CALL — for a Material source, the instance reuses the source's
per-channel blends and multiplies its own opacity in. For Paint/Fill
sources, the instance's own `blend_mode` applies. This is
"renders its source layer's composited pixels" (`layers.rs:40`) taken
literally per channel.

### 5.4 Layer delete, undo, and tombstones

`LayerCommand::Remove` is undoable (`journal.rs` `Insert`, `:59-66`). If
delete freed the planes, undoing the delete would revive an EMPTY layer.

- When the shell's per-frame census sees an id with planes that is no
  longer in the stack, that layer's planes are **retired**: read back
  to a CPU `RetiredPlanes` map (owner → bytes per tile×channel), and the
  GPU threads dropped. VRAM is the scarcer pool, and retired planes are
  rarely revived.
- When a layer reappears (undo of Remove, `Insert` replay), its retired
  planes upload back unchanged.
- A retired entry is freed when no command left in `doc.history` can
  revive its id. NEW surface: `UndoStack::iter()` (or a
  `referenced_layer_ids`) for that census, because no `on_evict` hook
  is wired today (`paint_state.rs:94-96`). Retired planes are not
  saved: the layer is not in the model.
- **Byte cap.** NEW constant `RETIRED_BUDGET_BYTES = 512 MiB` of CPU
  RAM. Deleting one 4-channel material across 4 tiles retires 16 planes
  = 1 GiB at 4K, and the document history holds 256 entries with no
  byte budget (`document.rs:58-60`: "the memory budget system (bytes)
  arrives with GPU-tile snapshots", and this design is that arrival).
  Past the cap, the OLDEST tombstone is freed first. Undoing that
  layer's delete then revives it EMPTY, with a log and a status line
  naming the layer.

  JUDGMENT CALL — a bounded, honest pixel loss on a deep undo-of-delete
  beats unbounded RAM. 512 MiB is the stroke history's 256 MiB budget
  (`paint_state.rs:456`) doubled, because a retired plane is a whole
  plane rather than a dirty rect. That is eight 4K planes.
- Stroke-history regions (§3) whose owner is retired apply to the
  retired bytes. If the owner is gone and freed, the region is dropped
  with a log.
- `remove_layer_masks`/`prune_layer_masks` (`paint_state.rs:1360-1375`)
  keep their current semantics for MASK planes.

  JUDGMENT CALL — mask planes do not get tombstones in v2. Detaching a
  mask already drops its planes by design ("a re-attach starts white",
  `:1357-1359`).

### 5.5 Resize

`resize_set`/`resize_channel` (`paint_state.rs:1985-2075`) publish
`Resize` to every PLANE of the set's tiles (overrides honored per
channel, as for targets today) and to the cache. Contents drop, the same
destructive contract as today (`:1965-1970`), now for every layer.
Fills and materials re-dirty through the existing size census
(`main.rs:3000-3017`) and refill their planes. Painted layers lose their
pixels exactly as the flat target does today. The UI's existing
per-set hint already names the loss.

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
  `paint_channel_tile_path`, `main.rs:1057-1072`). Layer planes nest
  under `layers/<id>/<channel>/`.
- The manifest `name` field encodes the plane, exactly as the
  per-channel pool encoded the channel instead of adding a 6th column
  (`main.rs:1074-1089`, `parse_paint_commit`'s exact 5-field destructure
  at `:1574`). The manifest header becomes `# Umber paint commit v2`;
  `#` lines are skipped by the parser (`main.rs:1570`), so this is
  informational.
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
conflict this layout exists to remove (below). Export and bake read the
cache after the flush, never the sidecar.

JUDGMENT CALL — eval-owned planes (fills, materials) ARE saved, though
re-evaluable. Opening must not depend on the mesh being present (a fill
cannot evaluate without one, `main.rs:2952-2954`), and opening must be
deterministic without re-running graphs. This is the project-open seeds
CLEAN precedent (`material_layer.rs:34-38`), now for fills too. It also
retires the fill compound-on-open bug
(`dynamic-materials-design.md:469-475`): re-eval no longer compounds
anyway (§5.3).

### 6.2 Save cost

Per-plane readback replaces per-target readback. The `dirty_since_save`
flag (§1.1) skips the readback of a clean plane and re-stages its
existing sidecar bytes. That is the path the save already takes for a
failed readback (`main.rs:1949-1962`). The content-addressed store
dedupes the blob anyway (`main.rs:24-32`). A save after painting one
layer reads back that layer's touched planes only. PNG encoding of
identical bytes is identical, so untouched layers produce no file churn.

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
- **The bump: `CURRENT_PROJECT_VERSION` 1 → 2, written only when it must
  be.** The loader accepts `{1, 2}`. `project.rs:368-370` and `:570-572`
  change from `!=` to a supported-range check, the
  `UnsupportedVersion` message (`project.rs:52-54`) names both, and a
  loaded model is stamped 2 so `validate_project_structure` passes on
  re-save (it checks `model.version`, `project.rs:368`). The save writes
  version **2 iff the session holds at least one layer-owned plane**,
  else 1. The decision is computable from the session before either
  file is written.

JUDGMENT CALL — bump, superseding `dynamic-materials-design.md:548-552`'s
no-bump argument for THIS change. That argument was that a bump makes
the new build refuse every old project. Dual-read removes it. The
opposite failure is worse here: an OLD build opening a v2 project would
see only Base and show the canvas missing every layer's paint. Its save
would leave the `layers/` tree in place (its walker is one level deep,
`main.rs:1732-1749`) beside its own new flat edits, a silent fork. A
loud `UnsupportedVersion` on the old build is the safer failure.

The minimal bump (2 only when layer planes exist) keeps layer-less and
headless projects, such as umber-cli batch sessions, readable by old
builds. Stage 2 must re-grep `CURRENT_PROJECT_VERSION` across `crates/`
(umber-cli included) before landing.

- **Dormant fills and materials on migration.** A v1 flat plane already
  contains every fill's and material's destructive output, and it cannot
  be decomposed: Multiply and compounded publishes are not invertible.
  On the OPEN path, a Fill or Material layer with no loaded planes while
  Base is non-empty is seeded CLEAN and **dormant**. It does not
  re-evaluate on open, so the first frame stays byte-identical. Its row
  badges "its pre-v2 result lives in the Base paint".

  The first param edit or explicit refresh activates it. It evaluates
  into its own plane, and its old contribution in Base remains,
  composited twice. The badge says so beforehand.

  JUDGMENT CALL — a byte-identical open, with any double-count only
  after an explicit user action, beats a silent double-count on every
  open. Dormancy is DERIVED ("Base non-empty AND no planes AND
  open-path state"), never persisted. A layer added at runtime is never
  dormant.

- **Base stays Base.** It is shown as a pinned bottom pseudo-row
  ("Base paint", visibility only, session state) when it holds planes.
  A "convert Base to a layer" action is a follow-up (§9).

  JUDGMENT CALL — opening a v1 project must not rewrite the layer JSON
  tree. Injecting a synthetic layer would allocate an id
  (`next_layer_id`), and the next save would add a new
  `layers/<set>/<id>.json` the user never made.

### 6.4 The autosave journal

`JournalOp` records layer METADATA only, with no pixel op
(`main.rs:74-77`). That stays true. Plane writes ride neither the
journal nor `project.json`. Pixel crash-safety is the paint commit's
own atomic protocol, unchanged (`main.rs:33-60`). The journal's
interactions with planes:

- Replaying `Remove` for a layer whose planes were saved: the planes
  are orphans on load (id not in the stack). They are ignored with a
  log and pruned by the next save's manifest (the apply step prunes
  unlisted names, `main.rs:1754-1756`).
- Replaying `Add` for a layer created after the last save: the layer
  exists and has no planes. It is empty, exactly as unsaved paint is
  lost today.
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
The same-layer case is still a binary conflict on that layer's PNG, as
it should be. One shared file remains: `manifest.txt`. It is provenance,
not authority (the sidecar wins on open, `main.rs:59-60`), so a conflict
there resolves by taking either side, and the next save rewrites it.
The `merge-story.md` correction is a doc delta in the kill report
(outside this lock).

## 7. Migration path: staged kills

Every stage lands green and leaves the app whole. The ordering rule,
from the advisor review:
- **Save must round-trip layer planes BEFORE any stroke can create one.**
  Otherwise a save/reopen between stages would flatten every layer.
- **Strokes and fills/materials move to planes TOGETHER.** Moving either
  alone reorders destructive publishes against strokes.

| Stage | Surface | Lands | Behavior after |
|---|---|---|---|
| **1a — compositor** | `umber-gpu/src/compositor.rs`, `umber-app/tests/gate0_journey.rs` (struct literal) | `premultiplied` flag, bottom-copy identity rule, `composite_stack_region` + CPU oracle (§4.3) | Byte-identical: every new field is default-off. |
| **1b — storage** | `paint_state.rs` | `PlaneOwner`, `planes`, `Plane`, `plane_bytes`, `read_plane`, `publish_plane`, the lazy cache rule, `flush_composite` (a no-op while only Base exists) | Byte-identical: everything is Base, and readers read Base. Every existing paint_state test passes unchanged; that IS the regression contract. |
| **2 — save v2** | `main.rs` sidecar codec + `save_paint_tiles`/`load_paint_tiles`; `umber-core/src/project.rs` (version range + stamp) | the §6.1 layout + parse rule, `dirty_since_save`, dual-read, the minimal bump | Layer planes round-trip (published via `publish_plane` in tests). No UI creates one yet, so saves still write version 1. |
| **3 — selection + gate** | `paint_state.rs` (`active_layer`, verdict, refusal), `layers.rs` (Fill/Folder arms + tests), `document.rs` (`LayersUiOutcome`, selectable rows, add-selects), `main.rs` (per-frame revalidate + sync), `layers_panel.rs` (gate test update), ftl (refusal hints) | §2 entirely | Strokes on Material/Instance/Fill/Folder are REFUSED with a typed reason. Accepted strokes still land in Base. Closes the stroke-gate follow-up. Coherent: only the gate is new. |
| **4 — per-layer pixels** | `paint_state.rs` (routing, capture, retouch, quick-mask per plane, resize), `retouch.rs` (owner keys), `fill_layer.rs`, `material_layer.rs`, `main.rs` (`PaintMaterialIo`, refresh drivers, `CompositeSnapshot`, tombstone census, budget line), the flushing `&mut` reader accessors + their export/bake call sites (§4.1), `assets/i18n/en-US.ftl` + `pt-BR.ftl` (budget + tombstone-loss status lines) | §3, §5.1–5.2, §5.4–5.5, §4.2's dirt, §4.1's read contract | Strokes, fills and materials each write their own planes, and the cache is composited every frame. Warts 1–3 closed; the first version-2 saves. Perf gate: input-to-photon re-measured under `docs/perf/protocol.md` before landing. |
| **5 — anchors, instances, dormancy** | `material_layer.rs` (`SelfAnchor`/`ForwardAnchor`, partial flatten), the entry builder (instances), `layers_panel.rs` (dormant badge, Base pseudo-row), `assets/i18n/en-US.ftl` + `pt-BR.ftl` (badge + row strings) | §5.3's anchor + instance rules, §6.3's dormancy | Wart 4 closed and instances render. Until 5 lands, anchors keep v1's documented full-flatten read and instances keep rendering nothing (both already-documented v1 states, not regressions). |

Stage 4 is the dynamic-materials Kill-B-class landing: it touches the
live publish path. It should be dragon-authored, or a claw with
`material_layer.rs`'s module docs and this doc as required reading (the
per-channel-pool rollout precedent, `per-channel-tile-pool-design.md:
161-165`). Stages 1a, 1b and 2 are headless-testable and claw-shaped.

## 8. Test plan

House rule: exact bytes, hand-derived, each ≥ 0.1 from a rounding
boundary unless exact. Quantizer `f32_to_u8 = (v·255).round()`
(`compositor.rs:644-653`). The CPU oracle accumulates in f32 and
quantizes ONCE at the end (`compositor.rs:765,796`), so the
intermediate values below are unrounded.

Goldens are pinned on `composite_stack_cpu`/`composite_stack_region_cpu`.
GPU twins assert ±1 LSB with the reason in a comment: UNORM rounding and
FMA contraction are implementation-defined (judgment 8,
`compositor.rs:79-83`).

**Compositor (stage 1a)**
1. **Premultiplied identity.** One premultiplied entry, texel
   `(100,100,100,128)`, Normal, 1.0, over transparent. It carries an
   all-255 mask plane, so the entry is not "unmasked" and the bottom-copy
   rule does NOT fire. The mask's `m = 1.0` leaves coverage unchanged,
   so this exercises the general un-premultiply path:
   `s = 100/128 = 0.78125`, `cov = 128/255`,
   `out = 0.78125·0.501961 = 0.392157 → 100`; alpha `0.501961 → 128`.
   Sentinel: the straight path gives `0.392157·0.501961 = 0.196847 →
   50`. Assert 100 and ≠ 50.
2. **Bottom-copy verbatim.** Legacy straight texel `(200,200,200,100)`
   as the bottom premultiplied Normal/1.0/unmasked entry →
   `(200,200,200,100)`. Sentinel via the general path (the T1 mask
   trick): `s = 2.0 → clamp 1.0`, `out = 100/255 → 100`, giving
   `(100,100,100,100)`. Assert 200 and ≠ 100.
3. **Region equals full.** A 4×4 two-layer stack; the region `(1,1)-(3,3)`
   bytes equal the full composite's same rect exactly, and texels
   outside the rect are untouched in the output.
4. **Flag off is byte-identical.** Every existing compositor test passes
   unchanged with `premultiplied: false` (the regression contract,
   asserted by not editing them).

**Order, compounding, un-publish (stage 4).** Stack bottom→top:
- `L0`: paint, opaque byte 200.
- `M`: material, Multiply, src 100 opaque, layer 1.0 × channel 0.25 →
  cov 0.25.
- `L2`: paint, Normal, opacity 0.5, opaque byte 50.

The Multiply factor is `1 − 0.25·(1 − 100/255) = 1 − 0.25·0.607843 =
0.848039`.

5. **Order preserved (wart 1).**
   - Stack order: `200·0.848039 = 169.608`, then
     `169.608 + (50 − 169.608)·0.5 = 109.804 → 110`.
   - With `M` reordered to the top: `L2` first gives
     `(200 + 50)/2 = 125.0`, then `125·0.848039 = 106.005 → 106`.
   - v1's destructive refresh after painting `L2` yields 106 WHATEVER the
     stack order (the sentinel).
   - Assert 110 for the stack order and 106 after the reorder.
6. **Re-eval idempotent (wart 2).** `L0` + `M` only:
   `169.608 → 170`. Refresh `M` again with identical inputs → **170**.
   Sentinel: v1's compounding gives `170·0.848039 = 144.167 → 144`
   (`dynamic-materials-design.md:664-667`). Assert 170, 170, ≠ 144.
7. **Un-publish (wart 3).** From test 6:
   - hide `M` → 200;
   - show `M` → 170;
   - disable its channel → 200;
   - delete `M` → 200 and its planes are retired (CPU map holds them,
     GPU count −1);
   - undo the delete → 170 (tombstone revive, byte-identical planes).

**Anchors (stage 5).**
8. **Downward partial flatten.** `L0` opaque 200; `L1` paint Normal
   opacity 0.5, opaque 100; `M` above both.
   - `anchor(L0) = 200`.
   - `anchor(L1) = 200 + (100 − 200)·0.5 = 150.0 → 150`.
   - `anchor(M)` → `Err(SelfAnchor { layer: M })`.
   - An anchor on a layer above `M` → `Err(ForwardAnchor)`.
   - Sentinel: v1's full-flatten read returns the composite INCLUDING
     `M`'s own output.

**Routing + gate (stages 3–4).**
9. **Per-layer routing.**
   - A stroke with `active_layer = L1` changes `(L1, 1001, baseColor)`
     only.
   - `(L0, …)` and Base are byte-identical before and after.
   - The cache equals `composite_stack_cpu` of the planes; the GPU path
     is ±1.
   - Undo restores `L1`'s plane byte-exact and leaves `L0` untouched.
10. **The gate.**
    - Material, Instance, Fill and Folder active layers each refuse.
      `staged_batch_count() == 0`, `is_stroking() == false`, and
      `stroke_refusal` matches the typed variant naming the layer.
    - A layer-MASK stroke with a Fill layer selected for mask editing is
      ACCEPTED: the mask plane changes and no pixel plane does.
    - Unknown id → `UnknownLayer`.
11. **Selection lifecycle.**
    - A mid-stroke `set_active_layer` does not split the stroke: every
      staged batch lands in the begin-time owner.
    - Removing the active layer revalidates to the nearest layer below.
    - An empty stack → `None` → strokes land in Base. This is the
      headless callers' contract: `perf_soak.rs:82`'s stroke stages
      exactly as before.

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
14. **Lazy + accounting.** A fresh 512² session holds exactly Base's
    1001 baseColor plane: `plane_bytes() == 512·512·4 = 1,048,576`.
    One stroke on `L1` adds exactly `(L1, 1001, baseColor)` plus that
    `(tile, channel)`'s cache target, giving
    `3 · 1,048,576 = 3,145,728`. A layer with no stroke holds 0 planes,
    and the cache for a Base-only `(tile, channel)` is never allocated.
15. **Sidecar names.**
    - `parse("layers/7/baseColor/tile-1001.png") == (Layer(7),
      "baseColor", 1001)`.
    - `parse("layers/tile-1001.png") == (Base, "layers", 1001)`.
    - Each of these is rejected: `layers/7/tile-1001.png`,
      `layers/x/baseColor/tile-1001.png`,
      `layers/7/../tile-1001.png`, `layers/7/a\b/tile-1001.png`.
16. **Round trips.**
    - A v2 save → load gives byte-identical planes per owner, and the
      cache after the first flush equals the pre-save cache.
    - A v1 project (flat `tile-1001.png` + `height/tile-1001.png`) loads
      all into Base, and the first frame's reader bytes equal the files
      exactly.
    - A version-3 `project.json` is rejected with `UnsupportedVersion(3)`.
    - A session with no layer planes saves version 1, and one with a
      layer plane saves 2.
    - A v2 `project.json` beside a v1-layout sidecar loads as migration.
17. **Planes never under `layers/<set>/`.** A save → save cycle keeps
    every `paint/layers/…` file: the regression guard against
    `prune_stale_layer_files` (`project.rs:436-480`).
18. **Dormancy.** A v1 project with a Fill layer opens with that layer
    clean, dormant and planeless. The first frame is byte-identical. An
    explicit refresh materializes its plane.
19. **Resize.** `resize_set` drops every plane on the set's tiles, and
    other sets' planes are byte-identical.

## 9. Non-goals (what this design does NOT do)

1. **No virtual texturing or sparse pages.** Planes are full-size
   `PaintThread`s. The atlas, page table and 512² paging that
   `tile_pool.rs:3-11` names as Wave-3 work stay deferred. That is the
   real fix for §1.3's 6.4 GiB UDIM row, and it is a v3 surface.
2. **No plane residency or eviction.** Planes stay GPU-resident while
   their layer lives, and only retired planes move to CPU. Evicting
   cold planes needs a re-upload-for-composite story. §1.3's budget only
   warns.
3. **No per-layer CPU composite in the live path.** The CPU oracle is the
   test truth and the anchor partial-flatten engine; the viewport cache
   is GPU-composited.
4. **No folder membership or isolation.** Folders keep passing `None`
   (compositor judgment 4). Non-passthrough folder flattening waits on
   membership in umber-core.
5. **No multi-stack (per-set) keys.** Plane and mask keys omit the set
   (§1.1). Per-set stacks add it to both at once.
6. **No "sample merged" smudge/clone.** Retouch samples the active
   layer's plane (§3).
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

## Requirements traceability

| Row (`docs/specs/requirements.md`) | Today | After this design | Stage |
|---|---|---|---|
| :46 Dynamic material layering (P2) | v1 on destructive publish, four warts | warts 1–3 closed (§5.3), wart 4 closed, Instance-of-material renders | 4, 5 |
| :37 Layer types: paint/fill/folders | layers are metadata; strokes paint the canvas | paint and fill layers own pixels; strokes target the selected layer | 3, 4 |
| :19 Eraser ("sets layer alpha to zero") | erases the flat baseColor target | erases the active layer's plane | 4 |
| :40 Masks | planes exist; gate nothing live for paint layers (F1) | gate every layer in the live composite | 4 |
| :41 Layer instancing | instances render nothing (`compositor.rs:525-526`) | instances render their source's planes | 5 |
| :42 Anchors | full flattened read, self-feedback | downward partial flatten, typed self/forward errors | 5 |
| :120 Project = directory + per-layer files | per-layer JSON; pixels flat | per-layer pixel files (`paint/layers/<id>/…`), version 2 only when needed | 2 |
| :124 Merge story | Property 3 misdescribes pixel storage (§6.5) | different layers never conflict on PIXELS either | 2, 4 (+ doc delta) |
| :164 Input-to-photon < 20 ms | stroke = splat only | splat + region recomposite; re-measured before stage 4 lands | 1a, 4 |
| :167 Undo RAM | regions keyed (tile, channel) | regions keyed (owner, tile, channel); envelope unchanged | 4 |

No row is marked ✓ by this kill: it is a design doc. Each stage's kill
marks its rows.

## Deviation log (doc vs brief)

- **Brief: "LayerPixels keyed (layer_id, tile, channel)".** The key is
  `(PlaneOwner, tile, channel)`. Layer ids start at 0
  (`layers.rs:905-911`), so no id can mean "no layer", and five
  headless callers plus the empty-stack document need an ownerless
  target (§1.4).
- **Brief: "check_paintable (… Material/Instance arms)".** It also gains
  Fill and Folder arms (§2.3, judgment call).
- **Brief: "anchors read the FLATTENED cache".** Specified as the
  flatten TRUNCATED at the anchored layer (§5.3). Reading the full cache
  is exactly wart 4, so "never self-feedback" requires the truncation.
- **Brief: "a version bump or a dual-read loader?".** Both, plus a
  minimal bump (2 only when layer planes exist). The dual read keys on
  sidecar content, not on the version (§6.3).
- **Brief's grounding: "the composite machinery ALREADY EXISTS".** True
  for blending (F1). It lacks three things this design needs: premultiplied
  sources (F3), a verbatim seed, and region dispatch (§4.3). All three
  are named as new surface, with stage 1a carrying them.
- **Not in the brief: `umber_gpu::TilePool`.** It already exists keyed
  by layer. It is rejected as the store (F2), and its memory model is
  reused.
