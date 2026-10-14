# Kill report — Per-Layer Pool design doc

Date: 2026-10-14. Lock `perlayer-pool-doc` (pre-registered; `kill-locks.sh`
not called per the brief). Doc-only kill: NO cargo, NO commits. Every
grounding file in the brief was read before writing:
- `dynamic-materials-design.md`, `per-channel-tile-pool-design.md`,
  `udim-design.md`, `docs/loop/decisions.md`;
- `paint_state.rs` (module docs, storage, routing, the mask hook, the
  read/publish seams), `compositor.rs`, `material_layer.rs`,
  `fill_layer.rs`, `project.rs`.

The census followed the code into `tile_pool.rs`, `shaders.rs` (paint
splat), `umber-mesh/src/fill.rs`, `layers.rs` (`check_paintable`,
`blend_channel`, id allocation), `document.rs`, `layers_panel.rs`,
`main.rs` (refresh drivers, `PaintMaterialIo`, the paint sidecar codec
and commit protocol), `viewport.rs`, `uv_view.rs`, `retouch.rs`,
`docs/merge-story.md` and `requirements.md`.

An advisor review ran on the outline before writing. Its eleven points
are folded into the doc; the ones that changed the design are marked
with † in §2 below.

## 1. Built

- `docs/specs/per-layer-pool-design.md` (NEW). Contents:
  - the problem, and three verified ground facts (F1–F3);
  - §1–§9 numbered per the brief;
  - a requirements traceability table and a deviation log.
- `docs/loop/kill-reports/2026-10-14-perlayer-pool-doc.md` (this file).
- Tests added: 0 (doc kill). The doc's §8 specifies 19 test cases with
  hand-derived pinned bytes for the implementation kills.

## 2. Judgment calls

Every call below is also written into the doc as a JUDGMENT CALL paragraph.

**Three ground facts that shaped everything**
- **F1.** `StackCompositor`/`composite_stack_cpu` exist and the app never
  runs them. The design specifies when they run, not a new compositor.
- **F2.** `umber_gpu::TilePool` already keys `(layer, channel, tile)`,
  but REJECTED as the store. Its `u8` channel is the Color/Data kind:
  roughness, metallic and height collide. Its `TileId` is a 512² grid
  cell, not a UDIM tile. Its memory model and advisory cap are reused.
- **F3 †.** The flat pool mixes three alpha conventions:
  - strokes premultiplied (`shaders.rs:584-587`);
  - fills straight (`fill.rs:690-694`);
  - materials through the compositor's straight lerp.

  The compositor assumes straight alpha and clamps the source. A
  premultiplied plane composited naively turns P=100/A=128 into 50.
  Hence the compositor's new `premultiplied` flag plus the bottom-copy
  identity rule.

**Storage and selection**
- **Key = `(PlaneOwner, tile, channel)` with `PlaneOwner::{Base,
  Layer(u64)}`.** Ids start at 0. The empty-stack document and the five
  headless `begin_stroke` callers † need an ownerless target.
- **Flat BTreeMap key, owner-first.** The per-layer walks (retire, save,
  census) are as common as the per-tile ones.
- **No texture-set component in the key** (one document stack). The
  same latent assumption `layer_masks` already makes. Per-set stacks
  add the set to both keys in one change.
- **Lazy materialization extended per owner.** A missing plane is a
  transparent no-op entry.
- **Advisory `PLANE_BUDGET_BYTES = 2 GiB`, warn-only.** That is twice
  `TilePool`'s 1 GiB, because cache + planes double-count by design.
  Refusal would be a silent data-path failure. To be revisited at
  stage 4 against the perf protocol.
- **Selection lives on `PaintState`** (the `mask_edit_layer`
  precedent). Not journaled, not undoable, not persisted.
- **The verdict is pushed by the shell each frame †**, because
  `PaintState` has no `LayerStack`. Mask and quick-mask strokes bypass
  the gate † (hook order). A refused begin stays non-stroking with a
  typed `stroke_refusal`.
- **`check_paintable` gains Fill and Folder arms.** A fill's plane is
  eval-owned, and a folder has no pixels. Painter refuses both.
- **Selection is `None` only while the stack is empty †.** A non-empty
  stack always names a layer, so a refusal explains itself instead of
  silently painting Base.
- **`layers_ui` returns `LayersUiOutcome` †.** It takes no `PaintState`
  today. The add buttons select the new layer.

**Composite**
- **Composite-on-read with an eager per-frame flush** after the existing
  drain. Composite-on-write cannot preserve order, since only the
  topmost layer can be folded into a flat result. CoR also removes the
  two blocking 64 MiB readbacks fills and materials pay today.
- **The region composite is new surface †.** `composite_stack` clears
  the whole output, and the region path is what keeps CoR inside the
  < 20 ms input-to-photon row. Flags ride `CompositeParams.pad0`; the
  rect origin rides `pad1`/`pad2`. The 32-byte assert holds.
- **The lazy cache.** A Base-only `(tile, channel)` has no cache, and
  readers read Base directly. That makes stage 1 a zero-VRAM,
  byte-identical refactor.
- **Retouch samples the layer plane, not the composite.** "Sample
  merged" is a follow-up.
- **The read contract (final review).** An eager end-of-frame flush, plus
  flush-on-read for CPU readers that run mid-frame (eyedropper, export,
  bake sources), following `read_target`'s drain-first precedent. The
  export and bake sites move to a `&mut` flushing accessor (new
  surface). Boundary: a metadata edit is visible to readers from the
  next frame.
- **The region path uploads only the rect's mask bytes (final
  review).** Today's whole-plane upload per dispatch would cost 16 MiB
  per masked layer per 4K stroke frame. Rect upload was chosen over
  persistent GPU masks to keep the "nothing to sync" property.

**Fills, materials, anchors, instances**
- **All v2 planes are premultiplied; writers convert.** Fills fold
  coverage into alpha, fixing the fast path's coverage drop. The fill
  literal `1024` is retired for `size_of_channel` †.
- **Material per-channel blend/opacity apply at composite time.** One
  entry per `(material, channel)`.
- **Anchors = partial flatten THROUGH the anchored layer †.**
  Self-anchor and forward-anchor are new typed errors. This honors the
  brief's "flattened cache" wording and row 42, and makes self-feedback
  impossible by construction. It is computed as a GPU prefix composite
  plus ONE readback (final review), the same single read as today's
  anchor, not one readback per plane.
- **Instances render their source's planes.** A Material source keeps
  its per-channel blends.
- **Delete → planes retired to CPU (tombstones); undo-of-delete revives
  them †.** Retired planes are freed by a history census, since no
  `on_evict` hook is wired. Mask planes get no tombstones (existing
  detach semantics).
- **`RETIRED_BUDGET_BYTES = 512 MiB` (final review).** The oldest
  tombstone is freed first, and undoing that delete revives an empty
  layer with a log and a status line. The document history has 256
  entries and no byte budget (`document.rs:58-60`).

**Save**
- **Layout `paint/layers/<id>/<channel>/tile-<N>.png`, Base at the
  legacy paths.** A depth-exact parse rule disambiguates a channel
  named `layers`. Planes are NEVER under the layer-JSON tree, because
  `prune_stale_layer_files` deletes directories there
  (`project.rs:467-469`). That is a trap this doc names explicitly.
- **The cache is never saved.** That is what makes the merge story
  true for pixels. Eval-owned planes ARE saved: deterministic open,
  no mesh needed at open.
- **Version bump + dual-read, minimal bump †.**
  - The loader accepts {1, 2}.
  - The paint loader keys on sidecar CONTENT, not version, so the
    "new project.json + old paint" crash window loads as migration.
  - Saves write 2 only when a layer-owned plane exists.
  - This explicitly supersedes the dynmat no-bump argument: an old
    build would otherwise show a blank canvas and fork the files.
- **Migration loads v1 paint into Base, verbatim; no synthetic layer.**
  Opening never rewrites the layer-JSON tree. Pre-v2 fills and
  materials open **dormant** † (derived, not persisted): byte-identical
  first frame, with any double-count only after an explicit user action
  that the badge warns about.
- **Staging †.**
  - Save round-trips layer planes (stage 2) BEFORE strokes can create
    them (stage 4).
  - Strokes and fills/materials move to planes together (stage 4), so
    no stage reorders destructive publishes against strokes.
  - The gate (stage 3) lands while strokes still paint Base.

## 3. Unverified

- **Composite and save timings.** No GPU timings are claimed. §4.1 costs
  composite and save in texel-ops and bytes only. Whether region
  recomposite holds the < 20 ms budget needs the
  `docs/perf/protocol.md` measurement the doc gates stage 4 on.
- **The GPU bottom-copy seed.** Is unorm8 → f32 → unorm8 exact through
  `textureLoad`/`textureStore`, or does it need
  `copy_texture_to_texture`? If the latter, do `PaintTarget` textures
  carry `COPY_SRC`/`COPY_DST` usage? Not read in `paint.rs`. The doc
  leaves it to the implementation and pins CPU exactness only.
- **Queue ordering.** That plane-thread `process_pending` submissions and
  the flush's composite share one queue and order by submission is
  inferred from `PaintState` cloning one device/queue pair
  (`paint_state.rs:527-528`). The internals of `PaintThread`'s submit
  were not read.
- **Retouch ops on premultiplied planes.** The alpha handling of
  `umber_brush::retouch` (smudge mixing, the eraser's alpha write) on
  premultiplied planes was not audited. Stage 4 must, especially the
  eraser: zeroing alpha must also zero rgb.
- **The rect-mask upload.** A rect-sized `R8Unorm` sampled at
  `texel − origin` is specified, not prototyped. The WGSL change to
  `COMPOSITE_WGSL`'s mask fetch is the implementation's.
- **Verified, not a gap.** `InstanceError` derives `Clone`
  (`layers.rs:76`), so storing the verdict and copying it into
  `stroke_refusal` needs no new derive.
- **Version consumers outside the app.** umber-cli's and other crates'
  use of `CURRENT_PROJECT_VERSION` and the paint sidecar was not
  censused. The doc instructs stage 2 to re-grep.
- **Line numbers.** `main.rs` line numbers are as read on 2026-10-14. The
  tree is concurrently edited (`agent.rs` is dirty in another lock).

## 4. Deferred

All recorded in the doc's §9 or inline:
- sparse pages / virtual texturing, and plane residency / eviction;
- folder membership;
- per-set plane keys;
- "sample merged" retouch;
- pixel journal ops and selection persistence;
- a Base → layer conversion action;
- multi-entry material sub-stacks;
- layered PSD export of real layers;
- per-channel masks, cross-tile stroke continuity, per-channel dab
  density.

## 5. Gates run

- `bash scripts/verify-kill.sh <the two .md files>` — NOT run. The
  script is zero-cargo (its fmt leg touches `.rs` only,
  `scripts/verify-kill.sh:24-28`). It was attempted, but the harness
  requires approval for the command, which this non-interactive session
  cannot grant. The dragon should run it at consumption. Any
  brace-balance WARN on the doc's Rust code fences (`PlaneOwner`,
  `LayersUiOutcome`, the `check_paintable` arms) is the nameable false
  positive.
- `cargo fmt --all` / `cargo clippy` — NOT run (NO cargo per the brief;
  no Rust touched).
- Manual: every `path:line` cite was read in-session against the live
  tree before it was written.

## Doc deltas (dragon applies; outside this lock)

- `docs/merge-story.md` Property 3 (`:33-44`): pixels do not currently
  live as layer-referenced content-addressed assets. They are one flat
  `paint/tile-<N>.png` per (tile, channel), with the `AssetStore` as
  provenance behind it (`main.rs:18-32`). Proposed replacement text
  says so honestly and points at `per-layer-pool-design.md` §6.5, under
  which different-layer edits stop conflicting on pixels.
- `docs/specs/requirements.md`: no ✓ marks (a design doc completes no
  row). Optionally append to row 46's text: "v2 path:
  `docs/specs/per-layer-pool-design.md` (per-layer planes close the four
  v1 boundaries)".
- `docs/specs/dynamic-materials-design.md:147-153`: optionally add a
  pointer saying the named per-layer follow-up is now designed in
  `per-layer-pool-design.md`.
