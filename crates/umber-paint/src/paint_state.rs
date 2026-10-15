//! App-side paint state: pointer strokes → conditioned dabs → PaintThread.
//!
//! The first painting surface is the 2D UV view (direct UV→texel mapping —
//! no ray-picking needed); 3D-viewport painting (UV projection from the
//! depth buffer) arrives in a later wave. This module owns the per-stroke
//! [`StrokeConditioner`] and the [`PaintThread`] session, and translates
//! pointer events from the UV view into staged paint commands.
//!
//! Seam-aware stamping (docs/specs/seam-aware-stamping-design.md, slice 3):
//! once a mesh is handed over via [`PaintState::set_mesh`], dab positions
//! near a UV seam are mirrored into the neighboring island's UV coordinates
//! through the [`SeamGraph`](umber_mesh::seam::SeamGraph) edge
//! correspondence, so a stroke paints every UV image of the same 3D
//! neighborhood.
//!
//! Per-tile paint routing (docs/specs/udim-design.md, slice 4): the
//! session holds one [`PaintThread`] per UDIM tile, created lazily the
//! first time an event routes there. A pointer event at UV `(u, v)` lands
//! in `tile_of_uv([u, v])`'s target at tile-LOCAL texel coordinates (see
//! [`local_texel`]); the last tile an original (non-mirror) event routed
//! to is the ACTIVE tile, which [`PaintState::paint_target`] returns so
//! every existing single-target caller stays source-compatible. Routing
//! is live only once a multi-tile mesh is handed over via
//! [`PaintState::set_mesh`]; without a mesh, or with a single-tile mesh,
//! everything routes to tile 1001 — the pre-UDIM behavior,
//! byte-identical. V1 boundary (the design's named follow-up): routing is
//! per-event, so a stroke dragging across a tile seam paints each event
//! into the tile under the pointer; there is no cross-tile stroke
//! continuity.
//!
//! Retouch tools (docs/specs/smudge-clone-design.md; the eraser is §2's P0
//! row): with a smudge, clone or eraser [`BrushTool`] active, the
//! conditioned dabs skip the GPU splat and run on a per-stroke CPU mirror
//! instead ([`crate::retouch`]). The hook is in `push_event_styled`, right after `DabAdapter::stamps_to_dabs`: the
//! first event a stroke routes to a tile drains that tile's queue and
//! reads it back (`PaintTarget::read_back_rgba8`), every event's dirty
//! rect is written back as a `PaintThreadCommand::WriteRegion`, and
//! `end_stroke` records the stroke as ONE entry in the retouch history.
//! Retouch strokes do not seam-mirror in v1.
//!
//! Paint-stroke undo (CORE-LOOP CRITICAL 3, the 2026-10-12 strategy
//! review): paint-tool strokes record ONE [`PaintStrokeCommand`] per stroke
//! on the SAME `retouch_history` — the v1 `clear()` on every paint stroke
//! is gone. The capture mirrors the retouch path with one structural
//! difference (paint composites on the GPU, so there is no CPU mirror
//! holding the after-bytes):
//!
//! * Lazy per-tile before snapshot: the first staged batch a stroke routes
//!   to a tile drains that tile's queue and reads it back
//!   (`PaintTarget::read_back_rgba8`), so earlier strokes' dabs are in the
//!   bytes. Tiles the stroke never stages to cost nothing.
//! * Dirty accumulation: the union of the staged dabs' shader-identical
//!   footprint boxes ([`umber_brush::retouch::footprint`]), taken over the
//!   FINAL dabs (after symmetry expansion and mask gating), so symmetric
//!   siblings are covered; seam mirrors recurse through the same staging,
//!   so their tiles accumulate too. Under an active mask gate the union is
//!   conservative (footprint ⊇ written texels) — undo stays byte-exact.
//! * At `end_stroke` each touched tile is drained and read back, and the
//!   AFTER bytes are sliced out of the same rect. A tile whose
//!   before == after (a fully gated-away stroke) contributes no region, and
//!   a stroke with no regions records no entry — the identity rule, same as
//!   retouch. Undo/redo apply through `retouch_writes` (`WriteRegion`), the
//!   same drain as retouch strokes.
//!
//! Judgment calls (v1 boundaries):
//!
//! * Last-stroke undo is byte-exact. Undoing an OLDER stroke under newer
//!   paint overwrites the overlap (linear-undo last-write-wins) — the same
//!   property retouch strokes already had among themselves.
//! * Cumulative merge (audit 2026-10-12-c1 Pass 4 item 5): `end_stroke`
//!   records through [`crate::retouch::record_stroke`], which coalesces a
//!   stroke into the TOP entry when their rects overlap on a shared tile
//!   and the union is no larger than the pair (oldest before + newest
//!   after over the union; rules and judgment calls in `retouch`'s module
//!   docs). The paint path hands it the full-tile before snapshot and the
//!   full-tile after readback it already holds. A merged entry undoes and
//!   redoes as ONE unit: repainting the same spot N times is one undo step
//!   (split redo under merged storage is the named follow-up).
//! * Cost is two full-target readbacks per touched tile per stroke (before
//!   at first touch, after at end); retouch needs one because the after
//!   lives in its CPU mirror. Paint has no CPU composite step to keep an
//!   after in, so the readbacks are inherent, not a shortcut.
//! * Memory: RGBA8 is 4 bytes/texel. At the 4096 default a full-tile
//!   rect is 4096·4096·4 = 64 MiB per side, so a worst-case entry
//!   (before + after) is ~128 MiB: two such entries fill the 256 MiB
//!   budget and the third evicts the oldest, while the 64-entry cap alone
//!   would admit ~8 GiB — so at 4K the BYTE budget is the RAM guard and
//!   the entry cap is the stroke-count UX. At 512² a full-tile side is
//!   1 MiB and a worst-case entry ~2 MiB, so the 64-entry cap binds first
//!   (64 × 2 MiB ≈ 128 MiB, inside the budget). A typical stroke is small
//!   (brush r = 24 texels ⇒ footprint ≈ 49×49×4 ≈ 10 KiB per side; a
//!   16-event union is tens of KiB). V1 keeps BOTH existing caps unchanged
//!   ([`RETOUCH_HISTORY_MAX_ENTRIES`] + [`RETOUCH_HISTORY_MAX_BYTES`])
//!   and wires no `on_evict` hook — evicted entries are plain `Vec<u8>`,
//!   whose drop needs no cleanup. Against the review's §12:167 row
//!   (2 GB / 100-step budget): this 256 MiB / 64-step envelope sits well
//!   inside it; the entry cap must never be raised toward 100 without the
//!   byte budget beside it. NOTE on the caps' shape: the 256 MiB figure is
//!   the WHOLE-history budget, not a per-entry cap — one full-tile 4K
//!   stroke entry is 128 MiB, i.e. half the budget, so the second such
//!   entry fits and the third evicts the first. The budget still holds at
//!   4K (it is what evicts); what does NOT hold is "64 full-tile strokes
//!   of history" — at 4K that would be 8 GiB, and the byte budget caps it
//!   at ~2. Full-tile strokes are the pathological case (a real stroke
//!   covers a small union); the v1 boundary is that no per-entry cap
//!   exists, so one giant stroke can evict the whole history. The
//!   cumulative merge (above) is what raises the EFFECTIVE stroke count
//!   inside this envelope: N overlapping full-tile 4K strokes coalesce into
//!   one 128 MiB entry instead of evicting each other at the third. Both
//!   caps stay exactly 64 entries / 256 MiB — the merge never raises them.
//!   Non-overlapping strokes (and crossings whose bounding box outgrows
//!   the pair) still stack one entry each.
//! * One tile at the default = 64 MiB of GPU target, plus the tile's CPU
//!   mirror ONLY when a retouch stroke touched it (paint composites on the
//!   GPU, so paint strokes hold no CPU composite — their undo capture is
//!   the before/after rect pair above), plus the undo snapshot cost. N
//!   tiles × 64 MiB at 4K × (UDIM tile count): a 10-tile set holds
//!   ~640 MiB of live targets before any undo entry.
//! * Target size (CORE-LOOP CRITICAL 2, the 2026-10-12 strategy review):
//!   every texture SET owns its square paint size, defaulting to
//!   [`DEFAULT_TARGET_SIZE`] (4096 — the review's directive; the old 512
//!   live target survives only as the tests' explicit construction size).
//!   [`Self::set_sizes`] maps set name → `(width, height)` and
//!   [`Self::tile_sets`] maps tile id → set name; [`Self::ensure_tile`]
//!   creates each tile at ITS set's size, so one session can hold a 2048
//!   set beside a 4096 set. Tiles with no assignment resolve to the
//!   ACTIVE set ([`Self::active_set`]); the user-facing setting lives in
//!   the Texture Sets panel (the `SIZE_PRESETS` combo, 512..8K), which
//!   lists the sets, selects the active one, and routes through
//!   [`Self::resize_set`]. `resize_set` rebuilds only that set's tiles at
//!   the new size. The legacy [`Self::resize_target`] resizes the ACTIVE
//!   set (single-set sessions behave byte-identically to v1), and
//!   [`Self::with_target_size`] keeps its signature so the Gate-0 journey
//!   test constructs unchanged.
//! * Per-set sizes persist in the project's `paint/target_sizes.txt`
//!   sidecar (set→size lines plus `tile-<N>=<set>` assignments; the v1
//!   `target_size.txt` global still reads — a v1 project opens with its
//!   global applied to every set — and is still written with the active
//!   set's size for old readers). See `main.rs`'s persistence docs.
//! * Per-CHANNEL sizes (requirements §2's sizing row, the deferred half):
//!   each channel of a set can carry its own resolution in
//!   [`Self::channel_sizes()`], keyed `(set, channel)`; absent = inherit
//!   the set's size ([`Self::size_of_channel`] falls back to
//!   [`Self::size_of_set`]). [`Self::ensure_channel`] builds at the
//!   channel's size when overridden, the set's when not, and
//!   [`Self::resize_channel`] is the channel's `resize_set`: it resizes
//!   ONLY that channel's materialized targets, clears the shared stroke
//!   history (same semantics — snapshots no longer fit), and resizes
//!   nothing else. [`Self::resize_set`] preserves overrides (an
//!   overridden channel keeps its size while baseColor and inheriting
//!   channels rebuild). `resize_set` takes arbitrary `(width, height)`
//!   but the UI only offers squares; non-square sizes arise only from
//!   the project load path's stored dims.
//! * Judgment calls (why the channel rule is shaped this way):
//!   [`BASE_COLOR_CHANNEL`] never carries an override — it IS the set's
//!   canvas, so [`Self::size_of_channel`] short-circuits it to the set
//!   size and [`Self::resize_channel`] on it delegates to
//!   [`Self::resize_set`] (one size cannot both lead and follow).
//!   Strokes still condition at the SET's texel density
//!   ([`Self::texels_per_uv_for`]) whatever channel they land in —
//!   per-channel dab density is the named follow-up, so painting a
//!   smaller channel today lands set-scale dabs on fewer texels. The
//!   quick mask stays per-SET (it gates baseColor's thread only) and
//!   layer-mask planes stay set-sized: neither follows a channel resize.
//!   Deferred beside the sizing: 8K-per-channel VRAM guidance (the set
//!   combo's note covers 8K once) and per-TILE channel sizes (the key
//!   would grow a tile id — no caller needs it while sets stay square).
//! * Judgment calls (why the size is NOT on `umber_core::TextureSet`):
//!   that struct's `resolution` is the DOCUMENT/bake resolution (default
//!   2048), a different quantity from the live paint-target size (default
//!   4096), and its schema (`project.json`) is another kill's surface
//!   (this kill's lock is umber-app only). The paint session owns the
//!   registry keyed by set NAME — names are the app's set identity (the
//!   mesh-derived `texture_set_name`, the import session set's name). A
//!   rename is a new set: previously assigned tiles keep the old name
//!   until reassigned (the open path re-derives assignments from the
//!   sidecar, so renames across save/open do not strand tiles).
//! * `resize_set` still clears the WHOLE stroke-undo history, not just
//!   the resized set's entries (rects recorded at the old size no longer
//!   fit, and one entry can span tiles of several sets — filtering by set
//!   would leave half-entries). The UI names this per set (the Texture
//!   Sets panel's hint string), so a size change is never a silent undo
//!   loss. The clear also drops queued commits and bumps the stroke epoch,
//!   so the History window's next drain drops the rows that described the
//!   evicted entries (see [`StrokeCommit`]).
//! * The quick mask is per SET ([`Self::masks`]), not per tile: planes are
//!   uniform within a set, so a set resize drops only that set's planes.
//!   `umber-brush`'s `QuickMask` (single-size) is untouched — one instance
//!   per set at the set's size. [`Self::quick_mask`] returns the ACTIVE
//!   set's mask (every existing caller stays source-compatible).
//! * `texels_per_uv` is the ACTIVE set's density (single-set sessions read
//!   it exactly as v1 did); per-tile sites (routing, seam radius, clone
//!   offset, fill replay) use [`Self::texels_per_uv_for`]. The symmetry
//!   snapshot is taken at the stroke-START tile's density: a stroke
//!   crossing into a different-sized tile applies the start tile's
//!   transforms there (slightly off-center siblings — named follow-up).
//! * Mask-mode strokes still leave the history alone (they touch the mask,
//!   not the layer).
//!
//! Symmetry (§2's P1 row, `umber_brush::symmetry`): the panel's
//! [`SymmetrySettings`] are adopted between strokes ([`PaintState::sync_symmetry`])
//! and snapshot into tile-local texel transforms at `begin_stroke`. The
//! hook sits right after `DabAdapter::stamps_to_dabs` — AFTER the stroke
//! conditioner, so siblings are exact images of the smoothed path and the
//! one-euro/spacing state only ever sees the pointer (unlike the seam
//! mirrors, which recurse through the conditioner). Paint dabs expand into
//! the SAME `Stage` command (batch counts unchanged, dabs × siblings);
//! retouch dabs hand the primaries to [`RetouchStroke`], which stamps the
//! siblings with per-sibling smudge motion and mirrored clone sources.
//! Symmetry is tile-local (each tile mirrors about its own center line)
//! and v1 applies it to ORIGINAL events only: symmetric siblings do not
//! seam-mirror and seam mirrors do not symmetrize (named follow-up).
//! Symmetry off skips the expansion entirely — byte-identical staging.
//! The settings are session state: not in the project file (follow-up).
//!
//! Straight-line constraint + angle snap (§2's P1 row,
//! `umber_brush::line_constraint`): the panel's [`LineConstraintMode`] is
//! adopted between strokes ([`PaintState::sync_line_constraint`]) and set
//! on the live conditioner, whose hook sits AFTER lazy mouse and BEFORE
//! spacing — every stamp projects onto the stroke-start-anchored line (or
//! the nearest snap-multiple ray) before the dab planner sees it, so
//! spacing and the dab adapter downstream are untouched. Off (the default)
//! is the identity: staged dabs are byte-identical. Like symmetry, the
//! mode is session state persisted in the paint session (not the project
//! file); a mid-stroke change is refused so one stroke never splits.
//!
//! Wash mode (`umber_brush::WashMode`, §2 line 16): the active preset's
//! mode is adopted between strokes ([`PaintState::sync_wash_mode`]) and
//! renormalizes staged paint dabs pre-splat ([`apply_wash_mode`], right
//! after the retouch hook so smudge/clone/eraser and mask strokes keep
//! build-up alphas). CPU-side only: no extra GPU surface, no shader arm.
//!
//! Quick mask (§2's P1 row, `umber_brush::quick_mask` — semantics,
//! convention and the not-persisted/not-undoable rule live there): the
//! session owns one [`QuickMask`]. `begin_stroke` snapshots its two modes
//! for the whole stroke:
//!
//! * Mask mode (`editing`): the stroke paints the MASK. The hook sits
//!   right after symmetry expansion in `push_event_styled` and BEFORE the
//!   retouch hook, so mask mode wins whatever tool is selected: the dabs
//!   (and their symmetry siblings and seam mirrors) stamp the CPU mask,
//!   the touched rect is queued onto the tile's GPU mask
//!   (`PaintThreadCommand::WriteMask`), and nothing is staged. The layer
//!   and the retouch history are untouched.
//! * Gating (`active && !editing`): every layer dab is gated per texel
//!   AFTER symmetry expansion. Paint dabs carry the shader's `mask_gate`
//!   flag, so primaries, siblings and seam mirrors are all multiplied by
//!   their tile's GPU mask. Before the first gated dab on a tile, the
//!   tile's whole plane is uploaded (an invert/clear/resize marks every
//!   tile stale). Smudge/clone/eraser dabs multiply the CPU plane inside
//!   the retouch ops.
//!
//! Mask off (the default) changes nothing: no plane is allocated, no
//! upload is queued, and the staged dabs are byte-identical.
//!
//! Layer masks (requirements §2, the "METADATA ONLY" row):
//! per-LAYER paintable grayscale masks that gate the stack compositor.
//! Unlike the session-scoped quick mask (never persisted, never undoable),
//! layer masks persist through the `paint/` sidecar (`main.rs`) and their
//! METADATA (attach/detach/enable) is undoable through the document's
//! layer journal — but mask STROKES are not undoable (the quick-mask rule,
//! same reason: planes are not journal entries).
//!
//! * Storage ([`PaintState::layer_masks`]): one `u8` plane per
//!   `(layer id, tile)` at the tile's set size, `255` = open. Lazy like
//!   the channel pool: a layer with no stroke costs nothing, and a fresh
//!   plane starts WHITE (attaching a mask must be a no-op gate — a black
//!   start would hide the layer on attach). Planes are keyed by layer id,
//!   not by channel: the mask gates the layer's composited alpha whatever
//!   channel it composites.
//! * Routing ([`PaintState::mask_edit_layer`]): the Layers panel selects
//!   which layer's mask strokes paint (`Some(id)`) or none (paints layers).
//!   `begin_stroke` snapshots the selection into `stroke_mask_layer`, so a
//!   mid-stroke switch never splits one stroke across two planes. A mask
//!   stroke stamps the same conditioned dabs (symmetry siblings and seam
//!   mirrors included) into the `u8` plane via [`stamp_grayscale_mask`]
//!   and stages nothing — the layer pixels and the stroke-undo history
//!   are untouched. Quick-mask mode and layer-mask mode are independent:
//!   a layer-mask stroke paints NEITHER the layer NOR the quick mask, and
//!   it is NOT gated by the quick mask either (one gate per stroke — the
//!   compositor's layer gate is where the two meet; gating a mask stroke
//!   by another mask composes poorly and no tool selects it).
//! * Sizing: [`Self::resize_set`] drops that set's tiles' planes with the
//!   targets (a rect recorded at the old size no longer fits — the same
//!   rule as the quick mask).
//! * The gate itself lives in the compositor (`umber_gpu::compositor`,
//!   `cov = src_alpha × opacity × mask`), not here: this module only owns
//!   the planes and the stroke routing. No GPU mask texture is kept per
//!   layer — the compositor uploads the plane per dispatch from these
//!   bytes, so there is nothing to sync.
//!
//! Paint paths (§2 line 26's P1 row, `umber_brush::path`): a
//! [`PaintPath`](umber_brush::path::PaintPath) replays as ONE stroke
//! through this same pipeline ([`Self::replay_path`] — paint/erase/smudge
//! route to the consumer the op mode already selects), and a closed path
//! fills its interior ([`Self::replay_filled_path`]: the polygon fill's
//! even-odd rasterizer over the active tile, one stroke per covered row
//! run). Per-point size rides a per-event radius scale
//! ([`Self::push_event_styled`], which [`push_event_styled`](Self::push_event_styled)
//! delegates to at scale `1.0` — byte-identical); per-point opacity is
//! folded into the event pressure by the path itself (both are flow
//! multipliers, so spacing interpolates them for free). Replays refuse
//! while a live stroke runs, and restore the session tool afterwards.
//!
//! Per-layer pool storage, stage 1b (docs/specs/per-layer-pool-design.md
//! §1): one GPU plane per `(owner, tile, channel)`, materialized lazily.
//! [`PlaneOwner::Base`] is the session's ownerless bottom plane — today's
//! `TileTargets` content, read directly while no layer-owned plane exists
//! (the lazy cache rule, §4.2) — and [`PlaneOwner::Layer`] is a document
//! layer's own pixels. The PLANE-reader API is [`PaintState::read_plane`];
//! the composite accessors (`paint_target`, `tile_target`,
//! `channel_target`, `read_tile`, `read_channel_tile`) resolve
//! cache-else-Base once a cache exists, which in stage 1b means they read
//! Base exactly as today: every existing test passes unchanged, and that
//! IS the regression proof (§7 row 1b "Behavior after").
//!
//! Judgment calls (stage-1b boundaries — read before extending):
//!
//! * Base stays in `TileTargets` (the §1.4 store unification is deferred
//!   to a later kill — see the stage-4 block below, which records why).
//!   Stage 1b only adds the `planes` map (layer-owned planes), the
//!   `cache` map and `cache_dirty` beside them, so the live stroke path
//!   is byte-identical by construction.
//! * `Plane.committed` is `[u8; 32]`, not `blake3::Hash`: `blake3` is a
//!   dependency of `umber-core` only, and the workspace rule (E0432) plus
//!   this stage's lock forbid adding it to `umber-app/Cargo.toml` here —
//!   §6.2's own dependency note puts that dep in stage 2's surface, which
//!   swaps the alias for the hash type (the bytes are identical either
//!   way). Recorded as a reconciling deviation in the stage-1b report.
//! * `materialize_plane` returns `Result<bool, PaintRefusal>` (created vs
//!   already present), not the doc's `Result<&mut Plane, PaintRefusal>`:
//!   Base planes live outside `planes`, so no single reference type covers
//!   both owners. `plane_of_mut` is the dispatch that restores the doc's
//!   routing shape (§3): Base arms resolve through the targets store,
//!   layer arms through the planes map.
//! * The OOM error scope rides `umber_gpu::paint_thread::push_oom_scope`
//!   / `await_oom_scope` (wgpu 30's guard + async-pop shape, verified in
//!   the registry source). The pop driver is std-only (`std::task::Wake`
//!   over an `Arc`): `pollster` is dev-only in this crate and cannot run
//!   at runtime.
//! * Base stays in `TileTargets` (the §1.4 store unification is
//!   DEFERRED — see the stage-4 block below): every Base reader keeps
//!   today's exact path, and `plane_of_mut` dispatches the two stores.
//!
//! Per-layer pool stage 4 (docs/specs/per-layer-pool-design.md §3–§6):
//! per-layer pixels. Strokes, fills, materials and imports each write
//! their own planes (`plane_of_mut(stroke_owner, tile, &channel)` in the
//! stage/capture/retouch paths); the cache composites every frame
//! (`flush_composite(&snapshot)` at the shell's end-of-update); warts
//! 1–3 close; the first version-2 saves.
//!
//! Judgment calls (stage-4 boundaries — read before extending):
//!
//! * Base planes stay in `TileTargets`; `planes` holds layer-owned planes
//!   only. The doc's §1.4 end state unifies both stores, but unifying
//!   would churn every Base reader (save, export, bake, viewport binds)
//!   for no behavioral gain: `plane_of_mut` / `plane_thread` dispatch
//!   the two stores behind one seam, and the save census already walks
//!   both. A later move fills the seam in without touching callers.
//! * The flush's encoder types ride `umber_gpu`'s re-exports
//!   (`WgpuCommandEncoder`, `WgpuCommandEncoderDescriptor`), never bare
//!   `wgpu::` paths: GPU objects stay behind umber-gpu (the architecture
//!   rule), and bare paths resolve only under the test target's
//!   dev-dependency. One wgpu 30 in `Cargo.lock`, so the types are the
//!   ones eframe hands the shell.
//! * Composite errors drop the dirt loudly (never crash, never spam):
//!   a poisoned entry clears after one `warn`; the next write re-marks.
//! * History-evict tombstone freeing (§5.4 item 5) is deferred (needs an
//!   `UndoStack::iter` surface in umber-core, outside this kill's lock):
//!   the 512 MiB cap bounds RAM regardless; bytes are kept longer, so
//!   revives stay exact, never wrong.
//! * `bake_sources` keeps its `Option<&PaintState>` signatures (its
//!   caller is `export_dialog.rs`, outside this kill's lock): the
//!   composite accessors resolve cache-else-Base, so exports read the
//!   last flush's cache — one frame stale at most, exact once frames
//!   settle.
//! * The committed record clears on EVERY pixel write (stage 2 wired
//!   publish only): a stroke/undo/resize followed by an in-place save
//!   round-trips the new bytes, never the old file.
//!
//!
//! Per-layer pool storage, stage 3 (docs/specs/per-layer-pool-design.md
//! §2): the selection + the stroke gate. WHO a stroke paints is explicit
//! (`active_target`, edited through the shell) and gated (`active_verdict`,
//! pushed by the shell once per frame); stage 4 routes accepted strokes
//! into `stroke_owner`'s plane (the snapshot below is `active_target`).
//!
//! Judgment calls (stage-3 boundaries — read before extending):
//!
//! * `stroke_owner` snapshots `active_target` on every accepted stroke
//!   (stage 4's routing — the stage-3 force-to-Base is retired).
//! * `sync_active_target` keeps `active_explicit` on a same-target push and
//!   clears it when the target moves (see its docs): the shell moves the
//!   target only when it resolved it (open, new, vanished id), and a
//!   resolved target is auto by §2.3.
//! * `active_index` is meaningless for `Base` (no stack index): the sync
//!   preserves whatever it holds, and the vanished-id walk reads it only
//!   for `Layer` targets.
//! * `stroke_refusal` is stored; the shell renders it in the LayerStack
//!   panel (the status bar's snapshot is fixed — `status_bar.rs` stays
//!   outside the lock, so the panel line is the display).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use emath::Pos2;
use umber_bake::id::{bake_id_mesh, IdBakeParams, IdFlavor};
use umber_brush::id_mask::{select_id, SelectIdError};
use umber_brush::path::{sanitize_size, PaintPath, PathOp};
use umber_brush::retouch::{coverage, footprint, DirtyRect, RetouchDab};
use umber_brush::symmetry::{expand_dabs, texel_transforms};
use umber_brush::{
    wash_scale, BrushParams, BrushTool, DabAdapter, LineConstraintMode, MaskPaint, QuickMask,
    RgbaBuffer, StrokeConditioner, StrokeEvent, SymTransform, SymmetrySettings, WashMode,
};
use umber_core::layers::InstanceError;
use umber_core::undo::UndoStack;
use umber_core::BlendMode;
use umber_gpu::compositor::{CompositeRect, GpuStackLayer, StackCompositor};
use umber_gpu::paint::{Dab, PaintCompositor};
use umber_gpu::paint_thread::{FrameStats, PaintThread, PaintThreadCommand};
use umber_mesh::polygon_fill::rasterize_polygon_mask;
use umber_mesh::seam::{build_seam_graph, SeamGraph};
use umber_mesh::udim::{tile_of_triangle, tile_of_uv, FIRST_TILE};

use crate::retouch::{record_stroke, PaintStrokeCommand, PaintWrites, RetouchStroke, StrokeTile};

/// Paint-what-you-see projection (§2's P1 row, Mari-style paint buffer).
///
/// * [`PaintProjection::Uv`]: pointer positions are already UVs (the 2D UV
///   view). The 3D viewport does not paint — Ctrl+drag orbits.
/// * [`PaintProjection::Screen`] (the default: today's behavior, both
///   surfaces paint): a viewport pointer resolves through the camera + mesh
///   raycast to a UV before conditioning, via
///   [`PaintState::begin_screen_point`]/[`PaintState::extend_screen_point`];
///   misses skip the stamp (no orphan dabs) and seam mirrors fan out from
///   the resolved UV exactly like a UV-view stroke.
///
/// V1 judgment — live raycast, not a deferred Mari buffer: a true paint
/// buffer would snapshot screen-space points mid-stroke and resolve them to
/// UVs at stroke end, but that breaks the live preview (the UV view and the
/// export bridge read the tile targets every frame), the per-frame
/// `process_pending` drain the retouch readback depends on, and the per-event
/// staged-batch accounting the seam/symmetry hooks rely on. It is also
/// unnecessary in the current wiring: `Viewport::ui` skips `handle_input`
/// while a paint drag is in flight, so the camera cannot move mid-stroke
/// and there is nothing to tear. The deferred, camera-snapshotting buffer is
/// the named follow-up for when camera animation during strokes lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaintProjection {
    /// 2D UV editing only; the 3D viewport orbits.
    Uv,
    /// Paint what you see (the default): viewport strokes raycast to UVs.
    #[default]
    Screen,
}

impl PaintProjection {
    /// Whether Ctrl+drag in the 3D viewport paints (`Screen`) or orbits
    /// (`Uv`). The UV view paints in both modes.
    pub fn viewport_paints(self) -> bool {
        matches!(self, Self::Screen)
    }
}

/// Errors from constructing or driving the paint session.
#[derive(Debug, thiserror::Error)]
pub enum PaintStateError {
    /// The GPU compositor rejected the device (missing storage-texture
    /// feature) or a dispatch failed.
    #[error(transparent)]
    Paint(#[from] umber_gpu::paint::PaintError),
    /// The stack compositor rejected the device (the same storage-texture
    /// floor as the paint pass — one device story).
    #[error(transparent)]
    Composite(#[from] umber_gpu::compositor::CompositeError),
    /// The stroke conditioner's parameters were invalid.
    #[error(transparent)]
    Conditioning(#[from] umber_brush::ConditionerError),
}

/// Errors from ID-mask selection ([`PaintState::select_id_mask`],
/// [`PaintState::bake_and_select_id`]).
#[derive(Debug, thiserror::Error)]
pub enum IdSelectError {
    /// The click-time ID bake failed.
    #[error(transparent)]
    Bake(#[from] umber_bake::id::IdBakeError),
    /// The clicked bake bytes did not select (uncovered click, size
    /// mismatch — see `umber_brush::id_mask`).
    #[error(transparent)]
    Selection(#[from] SelectIdError),
    /// Refused: the live stroke snapshotted its mask gate at
    /// `begin_stroke`, so a mid-stroke selection would split one stroke
    /// across two masks (the same rule as `invert_mask`/`clear_mask`).
    #[error("id-mask selection refused: a stroke is in flight")]
    StrokeInFlight,
}

/// Default paint-target edge length in texels: 4096 (CORE-LOOP CRITICAL 2,
/// the 2026-10-12 strategy review's directive — the live target is 4K, not
/// 512; the 4K/8K-via-test-hook era ends here). 4096² RGBA8 = 64 MiB per
/// tile-target (see the module docs' memory note); N tiles × that at 4K ×
/// UDIM. The [`Self::new`] default; [`Self::with_target_size`] builds any
/// square size (the wiring tests pin 512 explicitly so their seam/mask
/// geometry stays byte-identical).
pub const DEFAULT_TARGET_SIZE: u32 = 4096;

/// The paint session's default texture-set name: what tiles resolve to
/// before the app names a set (mesh load / project open retarget the
/// active set), and what [`Self::with_target_size`] sizes. Mirrors the
/// `texture_set_name` fallback chain (`umber_mesh::mesh_maps`) and the
/// import session set's initial name, so a fresh session, a fresh import
/// state and a set-less mesh all agree on one set.
pub const DEFAULT_TEXTURE_SET: &str = "TextureSet";

/// The reserved channel name (docs/specs/per-channel-tile-pool-design.md):
/// `umber_core::TextureSet::new_default`'s first channel. Routing to it
/// resolves to [`TileTargets::base_color`] and never allocates a
/// `channels` entry — every pre-channel caller (`targets.get_mut(&tile)`
/// reading `.base_color`) stays byte-identical.
pub const BASE_COLOR_CHANNEL: &str = "baseColor";

/// The height channel's canonical name (audit worklist #12's binding seam):
/// the channel the viewport binds through `MeshBuffers::set_height_texture`
/// for the Lit path's per-frame normal perturbation. It is a CONVENTION,
/// not a reserved word — any channel named "height" materializes and binds
/// like any other; this const is the single spelling the app-side bind
/// reads so a rename is one edit (the document's channel list remains the
/// source of truth for what exists). `pub` (not `pub(crate)`): the lib
/// surface exposes paint_state for integration tests, and the const is part
/// of the channel-name contract a test pins.
pub const HEIGHT_CHANNEL: &str = "height";

/// Brush radius in target texels.
const BRUSH_RADIUS_TEXELS: f32 = 24.0;

/// Synthetic event-clock step: nominal nanoseconds between consecutive
/// pointer events fed to the stroke conditioner.
///
/// The conditioner needs strictly advancing timestamps: `OneEuroFilter`
/// holds its estimate on duplicate/backwards timestamps, so feeding every
/// event `time_ns: 0` (as this module historically did) freezes the filter
/// and no stroke ever emits a dab. Real tablet timestamps are not plumbed
/// through the UV view yet (a later stylus-wave slice); until then each
/// `push_event_styled` call advances a per-stroke logical clock by one step
/// (8 ms, the same regime as the brush crate's own tests). Deterministic:
/// the same event sequence always conditions identically.
const EVENT_STEP_NS: u64 = 8_000_000;

/// Mirror-gate margin over the brush footprint: a dab is mirrored across a
/// seam edge when its UV position lies within
/// `BRUSH_RADIUS_TEXELS * SEAM_MIRROR_MARGIN / texels_per_uv` of the edge's
/// UV segment. Must exceed 1.0 so a dab whose footprint can reach the
/// opposite island is always expanded; start at 1.5× per the design doc.
pub const SEAM_MIRROR_MARGIN: f32 = 1.5;

/// Retouch-history capacity (entries — one per stroke, or one per run of
/// overlapping strokes under the cumulative merge) and memory budget
/// (bytes of before + after dirty-rect snapshots). Unchanged by the merge.
const RETOUCH_HISTORY_MAX_ENTRIES: usize = 64;
const RETOUCH_HISTORY_MAX_BYTES: usize = 256 * 1024 * 1024;

/// One tile's paint targets (docs/specs/per-channel-tile-pool-design.md):
/// `base_color` is the v1-era surface every pre-channel caller reads —
/// created at tile creation exactly as today ([`PaintState::ensure_tile`])
/// — and `channels` holds one additional [`PaintThread`] per non-reserved
/// channel name that has ever routed here, lazily materialized on first
/// route ([`PaintState::ensure_channel`]). A channel with no route costs
/// nothing: `channels` stays empty.
struct TileTargets {
    base_color: PaintThread,
    channels: BTreeMap<String, PaintThread>,
}

impl TileTargets {
    /// Every materialized thread, `base_color` first — `process_pending`'s
    /// per-tile drain walks this so a materialized channel is never left
    /// undrained. `resize_set` does NOT walk this blindly: an overridden
    /// channel keeps its own size (see `PaintState::resize_set`), so the
    /// resize publishes per-thread sizes explicitly.
    fn threads_mut(&mut self) -> impl Iterator<Item = &mut PaintThread> {
        std::iter::once(&mut self.base_color).chain(self.channels.values_mut())
    }
}

/// Who owns a plane (per-layer-pool design §1.1). Layer ids start at 0
/// (`LayerStack::new`), so no id value can double as "no layer": the
/// ownerless plane is its own variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaneOwner {
    /// The session's ownerless bottom plane (§1.4). Selectable (§2).
    Base,
    /// A document layer's own pixels.
    Layer(u64),
}

/// One GPU plane per `(owner, tile, channel)`, materialized lazily
/// (per-layer-pool design §1.1–§1.2): the same [`PaintThread`] type every
/// target is today, so strokes, `WriteRegion`, `WriteMask` and `Resize`
/// are unchanged per plane.
pub struct Plane {
    thread: PaintThread,
    /// `Some((dir, hash))` = this plane's bytes were last committed to
    /// (saved into, or loaded from) the project at `dir`, whose sidecar
    /// file hashed to `hash` (blake3 of the PNG bytes). EVERY write
    /// (stage, `WriteRegion`, publish, `Resize`) sets it to `None`. The
    /// save may skip this plane's readback only under §6.2's two-part
    /// test (lands in stage 2 with the save loop that reads it).
    ///
    /// The hash is `[u8; 32]`, byte-identical to `blake3::Hash` (see the
    /// module docs): stage 2 swaps the alias when the dependency lands.
    committed: Option<(PathBuf, [u8; 32])>,
}

impl Plane {
    /// What the save path's clean-skip test reads (§6.2, stage 2).
    /// The PLANE-side accessor [`PaintState::plane_committed`] serves
    /// the live save loop; this one stays for the §8 clean-skip test.
    #[allow(
        dead_code,
        reason = "the §6.2 clean-skip test's reader; the live save loop uses PaintState::plane_committed"
    )]
    pub fn committed(&self) -> Option<&(PathBuf, [u8; 32])> {
        self.committed.as_ref()
    }
}

/// Every reason a stroke can be refused (per-layer-pool design §2.1,
/// [M4]). One enum, so the status line has one surface. No field is named
/// `source` (the thiserror trap).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaintRefusal {
    /// The explicitly selected layer is unpaintable (`check_paintable`'s
    /// verdict, pushed by the shell — stage 3's stroke gate, §2.3).
    #[error("paint refused: {0}")]
    Layer(InstanceError),
    /// Materializing a new plane would cross the hard cap, or wgpu
    /// reported `OutOfMemory` creating it (§1.3). `bytes` is the total
    /// `plane_bytes()` would have reached.
    #[error("plane budget refused: materializing would reach {bytes} bytes")]
    PlaneBudget {
        /// The total `plane_bytes()` the refused materialization would
        /// have reached (plane plus its cache texture, if new).
        bytes: u64,
    },
}

/// Advisory plane budget (§1.3): 32 planes at 4K or 128 at 2K. Crossing
/// it logs once and sets a status-line flag (stage 4); it never refuses.
#[allow(dead_code, reason = "advisory budget: stage 4's status-line consumer")]
pub const PLANE_BUDGET_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Enforced plane budget (§1.3): 2× the advisory. The default
/// [`PaintState`] hard cap (see its `plane_hard_cap_bytes` field, which
/// tests lower). A materialization that would cross it is refused with
/// [`PaintRefusal::PlaneBudget`].
pub const PLANE_HARD_CAP_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// One `(tile, channel)`'s composite dirt (per-layer-pool design §4.2,
/// [M8]): the multi-rect rule. `umber-app` has no `smallvec` dependency;
/// a plain capped `Vec` needs none.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "stage-1b primitive: stage 4's flush is the consumer; the §8 tests exercise it directly"
)]
pub enum DirtyRegion {
    /// At most 8 rects (see [`DirtyRegion::MAX_RECTS`]).
    Rects(Vec<DirtyRect>),
    /// A metadata change (order, visibility, opacity, blend, mask
    /// attach/detach, channel params): absorbs everything.
    Full,
}

#[allow(
    dead_code,
    reason = "stage-1b primitive: stage 4's flush is the consumer; the §8 tests exercise it directly"
)]
impl DirtyRegion {
    /// Rect cap: past it, the CLOSEST pair merges into its bounding rect,
    /// so the cost degrades smoothly toward `Full`.
    pub const MAX_RECTS: usize = 8;

    /// Whether this entry needs a full-tile recomposite.
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Marks the entry `Full` (the metadata-change writer, §4.2).
    pub fn mark_full(&mut self) {
        *self = Self::Full;
    }

    /// Marks `rect` dirty (the plane-write writer, §4.2): a rect that
    /// overlaps or touches an existing one unions into it; past
    /// [`DirtyRegion::MAX_RECTS`] rects the closest pair (whose union adds
    /// the fewest texels) merges into its bounding rect. A `Full` entry
    /// absorbs the rect.
    pub fn mark_rect(&mut self, rect: DirtyRect) {
        let Self::Rects(rects) = self else {
            return;
        };
        rects.push(rect);
        merge_touching(rects);
        while rects.len() > Self::MAX_RECTS {
            merge_closest_pair(rects);
        }
    }
}

/// Unions every touching-or-overlapping pair in `rects` until none
/// remains (the [`DirtyRegion::mark_rect`] writer rule).
fn merge_touching(rects: &mut Vec<DirtyRect>) {
    loop {
        let mut pair = None;
        for (i, a) in rects.iter().enumerate() {
            if let Some(j) = rects
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, b)| rects_touch(a, b))
                .map(|(j, _)| j)
            {
                pair = Some((i, j));
                break;
            }
        }
        let Some((i, j)) = pair else {
            return;
        };
        let united = rects[i].union(rects[j]);
        rects.remove(j.max(i));
        rects.remove(j.min(i));
        rects.push(united);
    }
}

/// Whether `a` and `b` overlap or touch (share an edge): half-open
/// `[x0, x1) × [y0, y1)` rects whose extents meet on both axes union
/// without a gap.
fn rects_touch(a: &DirtyRect, b: &DirtyRect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// Scales a SET-size texel rect into a channel-size rect, rounding
/// outward so the dirt always covers the stamped texels (§4.2 [M6]).
/// Identical sizes return the rect untouched (the common path —
/// no arithmetic, no drift).
fn scale_rect_outward(
    rect: DirtyRect,
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> DirtyRect {
    if (src_w, src_h) == (dst_w, dst_h) || src_w == 0 || src_h == 0 {
        return rect;
    }
    DirtyRect {
        x0: (rect.x0 as u64 * dst_w as u64 / src_w as u64).min(dst_w as u64) as u32,
        y0: (rect.y0 as u64 * dst_h as u64 / src_h as u64).min(dst_h as u64) as u32,
        x1: ((rect.x1 as u64 * dst_w as u64).div_ceil(src_w as u64)).min(dst_w as u64) as u32,
        y1: ((rect.y1 as u64 * dst_h as u64).div_ceil(src_h as u64)).min(dst_h as u64) as u32,
    }
}

/// Merges the pair of `rects` whose union adds the fewest texels
/// (the over-cap writer rule). `rects` holds more than one entry.
fn merge_closest_pair(rects: &mut Vec<DirtyRect>) {
    let mut best: Option<(usize, usize, u64)> = None;
    for (i, a) in rects.iter().enumerate() {
        for (j, b) in rects.iter().enumerate().skip(i + 1) {
            let united = a.union(*b);
            let added = united.width() as u64 * united.height() as u64
                - a.width() as u64 * a.height() as u64
                - b.width() as u64 * b.height() as u64;
            if best.is_none_or(|(_, _, least)| added < least) {
                best = Some((i, j, added));
            }
        }
    }
    if let Some((i, j, _)) = best {
        let united = rects[i].union(rects[j]);
        rects.remove(j.max(i));
        rects.remove(j.min(i));
        rects.push(united);
    }
}

/// One tile's slice of a live paint stroke's undo capture: the
/// stroke-start snapshot plus the union of every staged dab footprint on
/// that tile (see the module docs). The snapshot is taken lazily on the
/// stroke's first staged batch per tile, so untouched tiles cost nothing.
struct PaintTileCapture {
    /// The tile's texels before the stroke touched it (drained first, so
    /// earlier strokes' dabs are in the bytes).
    before: RgbaBuffer,
    /// Union of the staged dab footprints, or `None` when the tile was
    /// snapshotted but nothing staged yet.
    dirty: Option<DirtyRect>,
}

/// The live paint-tool stroke's undo capture (`Some` between
/// `begin_stroke` and `end_stroke` while the paint tool is active; `None`
/// for retouch and mask-mode strokes, which have their own paths).
#[derive(Default)]
struct PaintStrokeCapture {
    /// Keyed `(owner, tile)` (§3): undo restores the stroke owner's plane,
    /// never the cache, and leaves other owners untouched.
    tiles: BTreeMap<(PlaneOwner, u16), PaintTileCapture>,
}

/// One layer's stack metadata for the composite (§4.2): what the shell
/// hands [`PaintState::flush_composite`] every frame, bottom-to-top. The
/// session holds no stack; the shell builds this from `doc.stack.layers`
/// plus the Base row's visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotLayerKind {
    /// Strokes land here; entry blend/opacity are the layer's own.
    Paint,
    /// Eval-owned baseColor plane; entry blend/opacity are the layer's
    /// own (`Passthrough` maps to `Normal` — the fill precedent).
    Fill,
    /// Eval-owned per-channel planes; the blend/opacity ride
    /// [`SnapshotLayer::material_channels`], never the layer's own.
    Material,
}

/// One material channel's composite entry (§5.2): only DECLARED + enabled
/// + allowed-blend channels reach the snapshot (disabled, undeclared and
///   forbidden-blend channels composite as `pixels: None` — no entry).
///
/// `opacity` is the already-formed `layer.opacity × channel.opacity`
/// product. Built by [`crate::material_layer::material_composite_channels`].
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotMaterialChannel {
    /// The plane channel this entry reads.
    pub channel: String,
    /// The channel's blend mode.
    pub blend: BlendMode,
    /// `layer.opacity × channel.opacity`.
    pub opacity: f32,
}

/// One stack row's composite metadata, bottom-to-top.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotLayer {
    /// Whose plane this row composites. For an instance row, its SOURCE
    /// (resolved through instance chains): instances carry no planes of
    /// their own and render the source's pixels (§5.3, stage 5).
    pub owner: PlaneOwner,
    /// The row's OWN layer id: mask lookup and row identity (`None` =
    /// Base, unmasked). Differs from `owner` only on instance rows.
    pub layer_id: Option<u64>,
    /// Paint/Fill rows use these; Material rows use `material_channels`.
    pub kind: SnapshotLayerKind,
    /// The row's blend mode (Paint/Fill; a Fill `Passthrough` composites
    /// as `Normal`).
    pub blend: BlendMode,
    /// The row's opacity (Paint/Fill).
    pub opacity: f32,
    /// Whether the row contributes (layer `visible`, Base row's toggle).
    pub visible: bool,
    /// Whether an enabled layer mask gates this row (the flush looks the
    /// bytes up in `layer_masks`; an absent plane reads fully white).
    pub mask_enabled: bool,
    /// `Some` for Material rows: one entry per contributing channel.
    pub material_channels: Option<Vec<SnapshotMaterialChannel>>,
}

/// Whether `row` IS `who`'s row: Base by owner, a layer by its
/// `layer_id`. Not by owner — an instance row carries its SOURCE's owner
/// (it composites the source's planes), so owner equality would conflate
/// the two. An anchor on an instance therefore truncates at the instance's
/// row, and an anchor on its source at the source's.
fn row_is(row: &SnapshotLayer, who: PlaneOwner) -> bool {
    match who {
        PlaneOwner::Base => row.owner == PlaneOwner::Base,
        PlaneOwner::Layer(id) => row.layer_id == Some(id),
    }
}

/// One resolved composite entry (the flush's entry builder output — see
/// `PaintState::resolve_composite_entries`).
struct ResolvedEntry {
    owner: PlaneOwner,
    blend: BlendMode,
    opacity: f32,
    visible: bool,
    mask_index: Option<usize>,
}

/// The per-frame composite input (§4.2): the stack's rows bottom-to-top
/// (Base first), the Base row's visibility, and the `(tile, channel)`s a
/// GPU binder shows this frame (the active tile's baseColor + height and
/// the UV canvas's bound channel). A metadata `Full` composites only for
/// those; every other `(tile, channel)` stays dirty and composites
/// lazily on read (§4.1 part 3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompositeSnapshot {
    /// Bottom-to-top rows (index 0 is the stack bottom, matching
    /// `umber_core::LayerStack` and the compositor's entry order).
    pub layers: Vec<SnapshotLayer>,
    /// The Base pseudo-row's visibility toggle (stage 4).
    pub base_visible: bool,
    /// Displayed `(tile, channel)`s: the only `Full` entries the frame
    /// flush composites ([M8]'s coalescing).
    pub displayed: Vec<(u16, String)>,
}

impl CompositeSnapshot {
    /// The ids with planes in `prev` that `next` no longer lists, with
    /// their kind (`true` = Paint, tombstoned; `false` = eval-owned,
    /// dropped): the shell's per-frame retire census (§5.4). Order-only
    /// edits (reorder, opacity, blend, visibility, channel params) are NOT
    /// removals — they surface through [`Self::full_owners`] instead.
    pub fn removed(prev: &Self, next: &Self) -> Vec<(u64, bool)> {
        let live: BTreeSet<u64> = next.layers.iter().filter_map(|l| l.layer_id).collect();
        prev.layers
            .iter()
            .filter_map(|l| {
                let id = l.layer_id?;
                (!live.contains(&id)).then_some((id, matches!(l.kind, SnapshotLayerKind::Paint)))
            })
            .collect()
    }

    /// The owners whose `(tile, channel)`s must recomposite `Full` after
    /// this snapshot replaces `prev`: every owner when the ORDER changed
    /// (a reorder marks both layers' entries — marking all is the same
    /// cost class and never misses), else the owners whose blend, opacity,
    /// visibility, mask enable or material channels changed, plus Base
    /// when its visibility flipped. A layer's entry is compared by
    /// position-independent identity (its `layer_id`), so a pure reorder
    /// still resolves through the order arm.
    pub fn full_owners(prev: &Self, next: &Self) -> Vec<PlaneOwner> {
        let prev_order: Vec<Option<u64>> = prev.layers.iter().map(|l| l.layer_id).collect();
        let next_order: Vec<Option<u64>> = next.layers.iter().map(|l| l.layer_id).collect();
        if prev_order != next_order {
            let mut owners: Vec<PlaneOwner> = next.layers.iter().map(|l| l.owner).collect();
            owners.sort();
            owners.dedup();
            return owners;
        }
        let mut owners = Vec::new();
        for layer in &next.layers {
            // Identity = (owner, layer_id): an instance row shares its
            // source's owner (it composites the source's planes) but keeps
            // its own `layer_id`, so the two rows never cross-compare.
            let same = prev
                .layers
                .iter()
                .find(|l| l.owner == layer.owner && l.layer_id == layer.layer_id);
            let changed = match same {
                None => true,
                Some(old) => {
                    old.blend != layer.blend
                        || old.opacity != layer.opacity
                        || old.visible != layer.visible
                        || old.mask_enabled != layer.mask_enabled
                        || old.material_channels != layer.material_channels
                }
            };
            if changed {
                owners.push(layer.owner);
            }
        }
        if prev.base_visible != next.base_visible && !owners.contains(&PlaneOwner::Base) {
            owners.push(PlaneOwner::Base);
        }
        owners
    }
}

/// One retired (tombstoned) plane (§5.4): a deleted PAINT layer's CPU
/// bytes, waiting for an undo-of-delete to revive them or for the cap to
/// free them. Eval-owned planes never retire (they revive by re-eval).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RetiredPlane {
    tile: u16,
    channel: String,
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// CPU bytes held for deleted paint layers, pending undo (§5.4 item 4):
/// 512 MiB — the stroke history's 256 MiB doubled, because a retired
/// plane is a whole plane rather than a dirty rect (eight 4K planes).
/// Past it, the OLDEST tombstone frees first, EXCLUDING the newest: one
/// oversize tombstone is retained while newest, so the last delete stays
/// undoable. Undoing a freed delete revives EMPTY with the
/// `layers.tombstone-lost` line.
pub const RETIRED_BUDGET_BYTES: u64 = 512 * 1024 * 1024;

/// One recorded stroke awaiting its History-window row: the stroke
/// journal's length before the stroke's push (`cursor`) and after
/// (`depth`). Queued by [`PaintState::end_stroke`] — the one chokepoint
/// that sees the journal on both sides of the push — and drained into
/// session-history rows by the app shell's per-frame stroke drain (the
/// `history_panel` module, which this library target does not include).
/// A fresh entry lands at `depth == cursor + 1`; a stroke merged into the
/// top entry (the cumulative merge) lands at `depth == cursor` — same
/// depth as its predecessor, one shared undo step. A stroke that wrote no
/// texels (the identity rule) or painted only the quick mask queues
/// nothing: there is no entry to travel to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeCommit {
    /// `retouch_history` length before the stroke recorded: the dead-redo
    /// truncation point, mirroring `Document::run`'s cursor.
    pub cursor: usize,
    /// `retouch_history` length after the stroke recorded: the row's jump
    /// target.
    pub depth: usize,
}

/// The paint session: one GPU target per UDIM tile plus the live stroke
/// conditioner.
pub struct PaintState {
    /// Device/queue handles kept for lazy per-tile target creation.
    device: umber_gpu::WgpuDevice,
    queue: umber_gpu::WgpuQueue,
    /// One [`TileTargets`] per UDIM tile. Tile [`FIRST_TILE`] is created in
    /// [`Self::new`] and never removed; other tiles appear on first route.
    /// Stage-1b role: the Base owner's planes (§1.4's move into `planes`
    /// lands with stage 4's routing) AND the composite cache (§4 — today
    /// only the cache half is live, and only once a layer-owned plane
    /// exists for that `(tile, channel)`).
    targets: BTreeMap<u16, TileTargets>,
    /// One [`Plane`] per `(owner, tile, channel)`, materialized lazily
    /// (§1.2). In stage 1b only layer-owned planes live here (via
    /// [`Self::publish_plane`]); Base planes stay in `targets` until the
    /// §1.4 move.
    planes: BTreeMap<(PlaneOwner, u16, String), Plane>,
    /// The committed registry (§6.2 [B1]): every plane's last-committed
    /// `(dir, content hash)`, keyed like `planes` but covering Base's
    /// targets-resident planes too (Base still lives in `targets` this
    /// stage — the save loop's clean-skip reads THIS map, one seam for
    /// both stores). Cleared by [`Self::clear_layer_planes`] and by any
    /// write to the plane it names.
    plane_committed: BTreeMap<(PlaneOwner, u16, String), (PathBuf, [u8; 32])>,
    /// The composite cache (§4.2's lazy cache rule): one thread per
    /// `(tile, channel)` that has at least one layer-owned plane, born
    /// `Full`-dirty so it can never be read uncomposited. A `(tile,
    /// channel)` whose only plane is Base has no cache entry: readers get
    /// Base directly, today's exact path at zero extra VRAM.
    cache: BTreeMap<(u16, String), PaintThread>,
    /// One [`DirtyRegion`] per `(tile, channel)` with pending composite
    /// work (§4.2). Plane writes mark their rects; metadata changes mark
    /// `Full`. The per-frame flush consumes these in stage 4; in stage 1b
    /// [`Self::flush_composite`] only records them.
    cache_dirty: BTreeMap<(u16, String), DirtyRegion>,
    /// `(tile, channel)` keys whose non-displayed `Full` dirt is
    /// coalesced (§4.2 [M8]) — waiting for a read or screen time. The
    /// waiting read composites against the synthesized stack (§8 T28).
    coalesced_fulls: BTreeSet<(u16, String)>,
    /// The ONE paint compositor every plane and cache thread shares
    /// (§1, [M4]): built at [`Self::new`], handed as an [`Arc`] to every
    /// [`PaintThread::with_compositor`] call, so materializing a plane
    /// never compiles a shader.
    compositor: Arc<PaintCompositor>,
    /// The stack compositor the per-frame flush folds planes through
    /// (§4.1): built once at [`Self::new`] (the same device floor as the
    /// paint compositor), so no frame ever builds a pipeline.
    stack_compositor: StackCompositor,
    /// The enforced plane budget (§1.3): a materialization that would push
    /// [`Self::plane_bytes`] past this is refused with
    /// [`PaintRefusal::PlaneBudget`]. Defaults to
    /// [`PLANE_HARD_CAP_BYTES`]; tests lower it through
    /// [`Self::set_plane_hard_cap_bytes`].
    plane_hard_cap_bytes: u64,
    /// The tile the last original (non-mirror) event routed to; always a
    /// key of `targets`.
    active_tile: u16,
    /// The channel new strokes paint (the Texture Sets panel's future
    /// channel combo edits this — [`Self::set_active_channel`], mirroring
    /// [`Self::set_active_set`]). [`BASE_COLOR_CHANNEL`] by default.
    active_channel: String,
    /// The live stroke's channel, snapshotted at `begin_stroke` (like
    /// `stroke_mask_edit`/`stroke_symmetry`) so a mid-stroke
    /// `set_active_channel` never splits one stroke across two targets.
    stroke_channel: String,
    /// Every known texture set's paint-target size (square in practice —
    /// see the module docs; the load path honors stored non-squares).
    /// Unknown sets read as [`DEFAULT_TARGET_SIZE`]² (see
    /// [`Self::size_of_set`]).
    set_sizes: BTreeMap<String, (u32, u32)>,
    /// Every channel's paint-target size override, keyed `(set, channel)`
    /// (square in practice — see the module docs). Absent = inherit the
    /// set's size (see [`Self::size_of_channel`]);
    /// [`BASE_COLOR_CHANNEL`] never has an entry (it IS the set's
    /// canvas). Written to the project's `paint/target_sizes.txt` as
    /// `channel/<set>/<channel>` lines (see `main.rs`'s codec); a file
    /// without them loads every channel at its set's size.
    channel_sizes: BTreeMap<(String, String), (u32, u32)>,
    /// Which set each present tile paints for. Tiles absent here resolve
    /// to the active set (see [`Self::set_of_tile`]); [`Self::ensure_tile`]
    /// records the active set at creation, and the project open path
    /// re-derives the map from the sidecar.
    tile_sets: BTreeMap<u16, String>,
    /// The set new tiles join and the Texture Sets panel edits. Always a
    /// key of `set_sizes` (see [`Self::set_active_set`]).
    active_set: String,
    /// Whether the loaded mesh spans more than one UDIM tile. Routing by
    /// `tile_of_uv` is live only then; otherwise every event lands in
    /// [`FIRST_TILE`] (an edge click at `u == 1.0` must not spawn a blank
    /// tile 1002 on a single-tile mesh).
    multi_tile: bool,
    /// Dab parameters every staged stroke uses (v1: the default brush;
    /// tests override the color to tell tiles' paint apart).
    brush: BrushParams,
    /// The live brush SIZE as a multiplier on [`BRUSH_RADIUS_TEXELS`]
    /// (`[` / `]`, Ctrl+RMB drag — GUI plan G1 g1-brush-resize). Applies to
    /// live strokes only: path replays carry their own per-point scale.
    /// Clamped to the path tool's `PATH_MIN_SIZE..=PATH_MAX_SIZE`.
    brush_size: f32,
    /// How the live stroke's overlapping dabs accumulate, adopted from
    /// the active preset between strokes ([`Self::sync_wash_mode`]).
    /// `BuildUp` (the default) stages adapter dabs unchanged;
    /// `Wash` renormalizes them pre-splat (see `push_event_styled`).
    wash_mode: WashMode,
    conditioner: StrokeConditioner,
    /// Set on pointer-down inside the UV square; cleared on pointer-up.
    stroking: bool,
    /// UV→texel scale (target texels per UV unit) of the ACTIVE set (see
    /// [`Self::texels_per_uv_for`] for the per-tile form the routing
    /// sites use).
    texels_per_uv: f32,
    last_stats: FrameStats,
    /// Seam topology of the loaded mesh (`None` until [`Self::set_mesh`]).
    seam_graph: Option<SeamGraph>,
    /// Armed by [`Self::begin_stroke`] when the stroke starts near a seam;
    /// gates the per-event mirror expansion (seam-blind strokes skip the
    /// mirror call entirely — the zero-cost contract).
    stroke_near_seam: bool,
    /// Per-stroke synthetic event clock (see [`EVENT_STEP_NS`]): advanced
    /// once per [`Self::push_event_styled`] call, reset by
    /// [`Self::begin_stroke`].
    event_seq: u64,
    /// The active tool (synced from the brush panel between strokes).
    tool: BrushTool,
    /// The live retouch stroke (`Some` between `begin_stroke` and
    /// `end_stroke` while a smudge/clone tool is active).
    retouch: Option<RetouchStroke>,
    /// The live paint stroke's undo capture (`Some` between
    /// `begin_stroke` and `end_stroke` while the paint tool is active).
    paint_capture: Option<PaintStrokeCapture>,
    /// The alt-click clone source (UV). Kept across strokes: fixed mode
    /// re-resolves the offset against every stroke's start (each stroke
    /// samples this same point); follow mode resolves it once, on the
    /// first stroke after the alt-click, and keeps that offset.
    clone_source: Option<Pos2>,
    /// Set by an alt-click, cleared by the first clone stroke after it.
    clone_source_fresh: bool,
    /// A clone offset resolved from `clone_source`, waiting to be written
    /// back into the panel's params ([`Self::sync_tool`]).
    resolved_clone_offset: Option<(f32, f32)>,
    /// One entry per paint or retouch stroke (the design's undo-per-stroke
    /// contract), separate from the document's layer journal in v1.
    retouch_history: UndoStack<PaintStrokeCommand>,
    /// Region writes the retouch history's undo/redo produced, drained
    /// into the tiles' queues right after each call.
    retouch_writes: PaintWrites,
    /// Strokes recorded since the last History-window drain (usually zero
    /// or one — the app drains every frame). Queued in [`Self::end_stroke`],
    /// cleared beside the journal in [`Self::resize_set`].
    pending_commits: Vec<StrokeCommit>,
    /// The stroke-history generation: bumped every time the journal is
    /// cleared wholesale ([`Self::resize_set`]), so the History window can
    /// drop rows that describe evicted commands. Starts at 0, matching a
    /// fresh session-history log.
    stroke_epoch: u64,
    /// The symmetry settings (synced from the brush panel between
    /// strokes; the UV view's mirror manipulator edits the offset here).
    symmetry: SymmetrySettings,
    /// The straight-line / angle-snap mode (synced from the brush panel
    /// between strokes via [`Self::sync_line_constraint`]; the live
    /// conditioner is set at the same time).
    line_constraint: LineConstraintMode,
    /// Where strokes may start (§2's P1 row): UV view only, or UV view +
    /// the raycast viewport ([`PaintProjection`]).
    projection: PaintProjection,
    /// Set when the manipulator changed `symmetry`, so the next
    /// [`Self::sync_symmetry`] hands it back to the panel instead of
    /// adopting the panel's (stale) copy.
    symmetry_from_view: bool,
    /// The live stroke's tile-local texel transforms (identity first),
    /// snapshot at `begin_stroke`; `None` = symmetry off for this stroke.
    stroke_symmetry: Option<Vec<SymTransform>>,
    /// The quick mask, one instance per texture set at the set's size
    /// (see the module docs — `umber-brush`'s single-size `QuickMask` is
    /// untouched). [`Self::quick_mask`] returns the active set's.
    masks: BTreeMap<String, QuickMask>,
    /// Tiles whose GPU mask texture matches `quick_mask`'s plane, per
    /// plane a gated stroke touches (§3: the mask copy rides the thread
    /// the dab stages into). Emptied by every whole-mask change (invert,
    /// clear, resize); a gated dab's plane is brought back in sync before
    /// it is staged.
    mask_synced: BTreeSet<(PlaneOwner, u16, String)>,
    /// The live stroke paints the mask (mask mode at `begin_stroke`).
    stroke_mask_edit: bool,
    /// The live stroke is gated by the mask (gating at `begin_stroke`).
    stroke_mask_gate: bool,
    /// The layer-mask planes: one grayscale `u8` plane per
    /// `(layer id, tile)` at the tile's set size, `255` = open (see the
    /// module docs). Lazy — a layer with no mask stroke costs nothing.
    layer_masks: BTreeMap<(u64, u16), Vec<u8>>,
    /// Which layer's mask strokes paint (`None` = strokes paint layers).
    /// The Layers panel edits this; `begin_stroke` snapshots it into
    /// `stroke_mask_layer` so a mid-stroke switch never splits one stroke.
    mask_edit_layer: Option<u64>,
    /// The live stroke's mask target, snapshotted at `begin_stroke` (like
    /// `stroke_mask_edit`/`stroke_channel`).
    stroke_mask_layer: Option<u64>,
    /// [M3] The target new strokes paint (per-layer-pool design §2.1).
    /// Base is first-class and selectable. The Layers panel edits this
    /// through the shell ([`Self::set_active_target`]); `begin_stroke`
    /// snapshots it into [`Self::stroke_owner`].
    active_target: PlaneOwner,
    /// [M12] `true` when the user chose `active_target` (row click, add
    /// button, mask-row selection); `false` when the shell's revalidation
    /// resolved it. Only an EXPLICIT unpaintable selection refuses (§2.3).
    active_explicit: bool,
    /// [m6] `active_target`'s stack index at the last sync, so a vanished
    /// id can resolve to its former neighbour (§2.3). Meaningless for
    /// Base (which has no stack index); preserved untouched.
    active_index: usize,
    /// The shell's last paintability verdict for `active_target`
    /// (`LayerStack::check_paintable`; `Ok(())` for Base), pushed between
    /// strokes via [`Self::sync_active_target`].
    active_verdict: Result<(), InstanceError>,
    /// The live stroke's owner, snapshotted at `begin_stroke`.
    stroke_owner: PlaneOwner,
    /// Why the last `begin_stroke` (or a mid-stroke materialization) refused.
    /// `None` after an accepted stroke.
    stroke_refusal: Option<PaintRefusal>,
    /// The last snapshot the flush composited against (§4.1 part 3):
    /// CPU readers flush-first against THIS (never torn, at most one
    /// metadata edit stale), and `end_stroke` tests it for the
    /// owner-deleted-mid-stroke boundary ([m7]). `None` until the first
    /// flush (readers then composite a synthesized all-visible stack —
    /// the headless fallback; the app always flushes every frame).
    last_snapshot: Option<CompositeSnapshot>,
    /// The `base_visible` the last flush saw: a flip marks every Base
    /// `(tile, channel)` `Full` (and materializes their caches — a hidden
    /// Base can no longer be read "directly", §4.2).
    last_base_visible: bool,
    /// Deleted PAINT layers' CPU bytes by layer id (§5.4): undo of the
    /// delete re-uploads them byte-identical. Each entry's sequence
    /// number orders eviction oldest-first (excluding the newest).
    retired: BTreeMap<u64, (u64, Vec<RetiredPlane>)>,
    /// Tombstone insertion order (the cap's oldest-first victim).
    retired_seq: u64,
    /// The enforced tombstone budget (§5.4 item 4): past it, the OLDEST
    /// tombstone frees first, EXCLUDING the newest. Defaults to
    /// [`RETIRED_BUDGET_BYTES`]; tests lower it through
    /// [`Self::set_retired_cap_bytes`] (the `plane_hard_cap_bytes`
    /// precedent — no panel edits this).
    retired_cap_bytes: u64,
    /// Pending amortized retirements (§5.4 item 2): `(owner, tile,
    /// channel)` triples queued by the shell's retire census, read back
    /// AT MOST ONE plane per flush. Threads stay alive (out of the
    /// composite — their owner is not in the snapshot) until their bytes
    /// land in [`Self::retired`].
    retire_pending: Vec<(u64, u16, String)>,
    /// The advisory-budget warn arm (§1.3): crossing
    /// [`PLANE_BUDGET_BYTES`] logs once and latches
    /// [`Self::over_budget`]; falling back under re-arms it.
    budget_warn_armed: bool,
    /// Latched while [`Self::plane_bytes`] is past the advisory budget
    /// (the shell's budget status line reads this, not the bytes).
    over_budget: bool,
    /// The last tombstone loss waiting for the status line (`take_`
    /// drains it): a freed delete's layer id (undo revives EMPTY) or a
    /// budget-refused revive's. `None` = nothing to report.
    tombstone_notice: Option<u64>,
    /// What layer-mask strokes paint (white opens, black closes — the
    /// quick mask's [`MaskPaint`] semantics on a `u8` plane).
    layer_mask_paint: MaskPaint,
    /// Queued `WriteMask` commands since construction (test
    /// observability for the mask-stroke and zero-cost contracts).
    #[cfg(test)]
    mask_uploads: usize,
    /// Staged `Stage` batches since construction (test observability for
    /// the seam-blind zero-cost contract).
    #[cfg(test)]
    staged_batches: usize,
    /// Per-tile split of `staged_batches` (test observability for the
    /// routing contract).
    #[cfg(test)]
    staged_batches_per_tile: BTreeMap<u16, usize>,
    /// Staged dabs since construction (test observability for the stroke
    /// soak's zero-drop assert — staged here must equal composited at
    /// drain — and the symmetry sibling count).
    #[cfg(any(test, feature = "test-hooks"))]
    staged_dabs: u64,
}

impl PaintState {
    /// Creates the tile-1001 paint target on `device`/`queue` (cloned
    /// inside `PaintThread`) at the [`DEFAULT_TARGET_SIZE`]² default;
    /// further tiles are created lazily on first route (at the live size —
    /// see [`Self::resize_target`]).
    ///
    /// Judgment call (lowest-churn API shape): `new` keeps its
    /// `(device, queue)` signature so the app and the perf harnesses are
    /// untouched, and square non-default sizes go through
    /// [`Self::with_target_size`] (the wiring tests' explicit 512).
    ///
    /// # Errors
    ///
    /// Returns the compositor's error when the device lacks the storage-
    /// texture feature (see `PaintCompositor::new`), or a conditioning
    /// parameter error at construction.
    pub fn new(
        device: umber_gpu::WgpuDevice,
        queue: umber_gpu::WgpuQueue,
    ) -> Result<Self, PaintStateError> {
        Self::with_target_size(device, queue, DEFAULT_TARGET_SIZE)
    }

    /// Creates the tile-1001 paint target at `size`×`size` texels (square —
    /// the UI only offers square presets; [`Self::resize_target`] is the
    /// arbitrary-size path). Same lazily-grown tile pool as [`Self::new`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::new`].
    pub fn with_target_size(
        device: umber_gpu::WgpuDevice,
        queue: umber_gpu::WgpuQueue,
        size: u32,
    ) -> Result<Self, PaintStateError> {
        let compositor = Arc::new(PaintCompositor::new(device.clone())?);
        let stack_compositor = StackCompositor::new(device.clone())?;
        let first = PaintThread::with_compositor(
            device.clone(),
            queue.clone(),
            size,
            size,
            compositor.clone(),
        );
        let default_set = DEFAULT_TEXTURE_SET.to_string();
        Ok(Self {
            device,
            queue,
            targets: BTreeMap::from([(
                FIRST_TILE,
                TileTargets {
                    base_color: first,
                    channels: BTreeMap::new(),
                },
            )]),
            planes: BTreeMap::new(),
            plane_committed: BTreeMap::new(),
            cache: BTreeMap::new(),
            cache_dirty: BTreeMap::new(),
            coalesced_fulls: BTreeSet::new(),
            compositor,
            stack_compositor,
            plane_hard_cap_bytes: PLANE_HARD_CAP_BYTES,
            active_tile: FIRST_TILE,
            active_channel: BASE_COLOR_CHANNEL.to_string(),
            stroke_channel: BASE_COLOR_CHANNEL.to_string(),
            set_sizes: BTreeMap::from([(default_set.clone(), (size, size))]),
            channel_sizes: BTreeMap::new(),
            tile_sets: BTreeMap::from([(FIRST_TILE, default_set.clone())]),
            active_set: default_set.clone(),
            multi_tile: false,
            brush: BrushParams::default(),
            brush_size: 1.0,
            wash_mode: WashMode::BuildUp,
            conditioner: StrokeConditioner::new(
                umber_brush::OneEuroParams::default(),
                umber_brush::OneEuroParams::default(),
                0.0,
                5.0,
            )?,
            stroking: false,
            texels_per_uv: size as f32,
            last_stats: FrameStats::default(),
            seam_graph: None,
            stroke_near_seam: false,
            event_seq: 0,
            tool: BrushTool::Paint,
            retouch: None,
            paint_capture: None,
            clone_source: None,
            clone_source_fresh: false,
            resolved_clone_offset: None,
            retouch_history: UndoStack::new(RETOUCH_HISTORY_MAX_ENTRIES)
                .with_memory_budget(RETOUCH_HISTORY_MAX_BYTES),
            retouch_writes: PaintWrites::default(),
            pending_commits: Vec::new(),
            stroke_epoch: 0,
            symmetry: SymmetrySettings::default(),
            line_constraint: LineConstraintMode::Off,
            projection: PaintProjection::default(),
            symmetry_from_view: false,
            stroke_symmetry: None,
            masks: BTreeMap::from([(default_set, QuickMask::new(size, size))]),
            mask_synced: BTreeSet::new(),
            stroke_mask_edit: false,
            stroke_mask_gate: false,
            layer_masks: BTreeMap::new(),
            mask_edit_layer: None,
            stroke_mask_layer: None,
            active_target: PlaneOwner::Base,
            active_explicit: false,
            active_index: 0,
            active_verdict: Ok(()),
            stroke_owner: PlaneOwner::Base,
            stroke_refusal: None,
            last_snapshot: None,
            last_base_visible: true,
            retired: BTreeMap::new(),
            retired_seq: 0,
            retired_cap_bytes: RETIRED_BUDGET_BYTES,
            retire_pending: Vec::new(),
            budget_warn_armed: true,
            over_budget: false,
            tombstone_notice: None,
            layer_mask_paint: MaskPaint::default(),
            #[cfg(test)]
            mask_uploads: 0,
            #[cfg(test)]
            staged_batches: 0,
            #[cfg(test)]
            staged_batches_per_tile: BTreeMap::new(),
            #[cfg(any(test, feature = "test-hooks"))]
            staged_dabs: 0,
        })
    }

    /// Registers `name` as a texture set at the default size (no-op when
    /// known), with its own quick mask. Mesh load / project open call
    /// this for the session set so the panel always lists it.
    pub fn ensure_set(&mut self, name: &str) {
        if !self.set_sizes.contains_key(name) {
            self.set_sizes
                .insert(name.to_string(), (DEFAULT_TARGET_SIZE, DEFAULT_TARGET_SIZE));
        }
        if !self.masks.contains_key(name) {
            let (width, height) = self.size_of_set(name);
            self.masks
                .insert(name.to_string(), QuickMask::new(width, height));
        }
    }

    /// Makes `name` the active set (see the `active_set` field docs):
    /// newly routed tiles join it, [`Self::target_size`] and
    /// [`Self::quick_mask`] read it, and the panel's size combo edits it.
    /// Registers the set first, so the active set is always a key of
    /// `set_sizes`. Between strokes only (like the `sync_*` adopts — a
    /// mid-stroke switch would move the texel density under the live
    /// conditioner).
    pub fn set_active_set(&mut self, name: &str) {
        self.ensure_set(name);
        self.active_set = name.to_string();
        self.texels_per_uv = self.size_of_set(name).0 as f32;
    }

    /// The active texture set's name.
    pub fn active_set(&self) -> &str {
        &self.active_set
    }

    /// The channel new strokes paint (see the `active_channel` field
    /// docs). [`BASE_COLOR_CHANNEL`] by default.
    pub fn active_channel(&self) -> &str {
        &self.active_channel
    }

    /// Makes `name` the active channel: the next `begin_stroke` snapshots
    /// it for that stroke's routing (`target_of`/`target_of_mut`). A
    /// no-op mid-stroke (like the `sync_*` adopts — a swap under a live
    /// stroke would split one stroke across two targets); the live
    /// stroke keeps its own snapshot.
    pub fn set_active_channel(&mut self, name: &str) {
        if !self.stroking {
            self.active_channel = name.to_string();
        }
    }

    /// Every known texture-set name, ascending.
    pub fn set_names(&self) -> Vec<String> {
        self.set_sizes.keys().cloned().collect()
    }

    /// `name`'s paint-target size; unknown sets read as
    /// [`DEFAULT_TARGET_SIZE`]² (the load path registers mapped sets
    /// before asking, so unknown here means "never seen" — default, never
    /// a panic and never a zero-size target).
    pub fn size_of_set(&self, name: &str) -> (u32, u32) {
        self.set_sizes
            .get(name)
            .copied()
            .unwrap_or((DEFAULT_TARGET_SIZE, DEFAULT_TARGET_SIZE))
    }

    /// `channel`'s paint-target size in `set`: its override when one is
    /// recorded, else the set's size (the inherit rule — see the module
    /// docs). [`BASE_COLOR_CHANNEL`] always reads the set's size (it is
    /// the set's canvas, never overridden). Unknown sets read as
    /// [`DEFAULT_TARGET_SIZE`]² through [`Self::size_of_set`].
    pub fn size_of_channel(&self, set: &str, channel: &str) -> (u32, u32) {
        if channel == BASE_COLOR_CHANNEL {
            return self.size_of_set(set);
        }
        let key = (set.to_string(), channel.to_string());
        self.channel_sizes
            .get(&key)
            .copied()
            .unwrap_or_else(|| self.size_of_set(set))
    }

    /// `channel`'s recorded override in `set`, or `None` when it inherits
    /// the set's size ([`BASE_COLOR_CHANNEL`] is always `None`). The
    /// Texture Sets panel's channel rows read this for their inherit
    /// default; the save path's census walks [`Self::channel_sizes()`].
    pub fn channel_override(&self, set: &str, channel: &str) -> Option<(u32, u32)> {
        if channel == BASE_COLOR_CHANNEL {
            return None;
        }
        let key = (set.to_string(), channel.to_string());
        self.channel_sizes.get(&key).copied()
    }

    /// Every recorded `(set, channel)` size override, sorted (the save
    /// path writes exactly these for the sets it persists — see
    /// `main.rs`'s `commit_paint_tiles`).
    pub fn channel_sizes(&self) -> &BTreeMap<(String, String), (u32, u32)> {
        &self.channel_sizes
    }

    /// The paint-target size of `tile`'s `channel` (the tile's set is
    /// resolved first, then [`Self::size_of_channel`]). The load path's
    /// per-artifact size check reads this — not [`Self::tile_size`]: a
    /// channel PNG must match ITS size, not its set's.
    pub fn tile_channel_size(&self, tile: u16, channel: &str) -> (u32, u32) {
        let set = self.set_of_tile(tile).to_string();
        self.size_of_channel(&set, channel)
    }

    /// The set `tile` paints for: its assignment, else the active set.
    pub fn set_of_tile(&self, tile: u16) -> &str {
        self.tile_sets
            .get(&tile)
            .map(String::as_str)
            .unwrap_or(&self.active_set)
    }

    /// The paint-target size of `tile`'s set.
    pub fn tile_size(&self, tile: u16) -> (u32, u32) {
        let name = self.set_of_tile(tile).to_string();
        self.size_of_set(&name)
    }

    /// The UV→texel density of `tile`'s set (the per-tile form of
    /// `texels_per_uv`, which tracks the active set).
    pub fn texels_per_uv_for(&self, tile: u16) -> f32 {
        self.tile_size(tile).0 as f32
    }

    /// The tiles currently assigned to `set`, ascending.
    pub fn tiles_of_set(&self, set: &str) -> Vec<u16> {
        self.targets
            .keys()
            .copied()
            .filter(|tile| self.set_of_tile(*tile) == set)
            .collect()
    }

    /// Moves `tile` to `set` (registers the set first). Present tiles keep
    /// their pixels — assignment only steers FUTURE `ensure_tile`
    /// creations' sizes and the mask the tile gates against; use
    /// [`Self::resize_set`] to rebuild at the set's size. Drops the tile's
    /// mask-sync flag (its planes now gate against another set's mask).
    pub fn assign_tile_to_set(&mut self, tile: u16, set: &str) {
        self.ensure_set(set);
        self.tile_sets.insert(tile, set.to_string());
        self.mask_synced.retain(|(_, t, _)| *t != tile);
    }

    /// Drops every tile→set assignment (the project open path re-derives
    /// the map from the sidecar, so a previous document's sets never steer
    /// the new one). Present tiles keep their targets; unassigned tiles
    /// resolve to the active set until reassigned.
    pub fn clear_tile_assignments(&mut self) {
        self.tile_sets.clear();
    }

    /// Hands a mesh over for seam-aware stamping: builds its [`SeamGraph`]
    /// once (O(triangles)) and stores it for the stroke path, and arms
    /// per-tile routing when the mesh's triangles span more than one UDIM
    /// tile. Call once per mesh load, alongside the existing mesh handoff.
    pub fn set_mesh(&mut self, mesh: &umber_mesh::MeshData) {
        self.seam_graph = Some(build_seam_graph(mesh));
        let tiles = tile_of_triangle(mesh);
        self.multi_tile = tiles.windows(2).any(|w| w[0] != w[1]);
        self.stroke_near_seam = false;
    }

    /// Whether a stroke is currently active.
    pub fn is_stroking(&self) -> bool {
        self.stroking
    }

    /// Begin a stroke at `uv` (0..1 UV space).
    pub fn begin_stroke(&mut self, uv: Pos2) {
        self.begin_stroke_styled(uv, 1.0, 1.0);
    }

    /// Begin a stroke at `uv` with an explicit pressure and brush-radius
    /// scale (the path replay's per-point style; live strokes use `1.0`
    /// for both through [`Self::begin_stroke`]).
    pub fn begin_stroke_styled(&mut self, uv: Pos2, pressure: f32, radius_scale: f32) {
        // Per-layer-pool stage 3 (§2.3): the stroke gate. A layer-mask
        // stroke paints the mask (never pixels) and a quick-mask-mode
        // stroke paints the session mask, so both bypass the gate — painting
        // a fill's or material's MASK stays legitimate. Otherwise an
        // EXPLICIT unpaintable selection (the only way `active_verdict` is
        // `Err`: auto-resolution never picks a refusing layer) refuses:
        // nothing stages, no capture opens, and the typed reason waits on
        // `stroke_refusal` for the status line. `replay_path` /
        // `replay_filled_path` begin through this function, so they inherit
        // the gate.
        if self.mask_edit_layer.is_none() && !self.quick_mask().is_editing() {
            if let Err(verdict) = self.active_verdict.clone() {
                self.stroking = false;
                self.stroke_refusal = Some(PaintRefusal::Layer(verdict));
                return;
            }
        }
        // Stage-4 routing (§3, §7 row 4): every accepted stroke snapshots
        // `active_target` into `stroke_owner`, and every batch stages
        // into that owner's plane (a mid-stroke selection change never
        // splits the stroke across two planes).
        self.stroke_owner = self.active_target;
        self.stroke_refusal = None;
        self.conditioner.reset();
        self.event_seq = 0;
        self.stroking = true;
        // Stroke-start seam neighborhood check (design slice 3, item 4):
        // arm mirror expansion once per stroke; strokes starting far from
        // any seam skip the per-event mirror call entirely (zero cost).
        //
        // Deviation from the design doc's literal text (which names
        // `seam_edges_near`): that query takes a 3D point, but the app
        // tracks UV positions and no UV→3D inverse mapping exists in
        // `PaintState`. UV-space segment gating is the app-side equivalent:
        // the mirror math itself (`mirror_positions`) gates on UV-segment
        // distance, so checking the same distance up front arms exactly the
        // strokes that can produce mirrors.
        self.stroke_near_seam =
            self.seam_near_uv([uv.x, uv.y], self.seam_radius_uv_for(self.route_tile(uv)));
        // Snapshot the symmetry for the whole stroke (before the retouch
        // stroke opens — it takes the same transforms), at the
        // stroke-start tile's density (a cross-size-tile stroke applies
        // these to its other tiles too — the module docs' boundary).
        let start_tpv = self.texels_per_uv_for(self.route_tile(uv));
        self.stroke_symmetry = self
            .symmetry
            .is_active()
            .then(|| texel_transforms(&self.symmetry, start_tpv));
        // Snapshot the quick-mask modes too: X / I mid-stroke never split
        // one stroke between the mask and the layer.
        self.stroke_mask_edit = self.quick_mask().is_editing();
        self.stroke_mask_gate = self.quick_mask().gates();
        // Snapshot the layer-mask target too: a mid-stroke panel switch
        // never splits one stroke across two planes.
        self.stroke_mask_layer = self.mask_edit_layer;
        // Snapshot the active channel: a mid-stroke `set_active_channel`
        // (refused anyway — see its own docs) must never retarget a live
        // stroke partway through.
        self.stroke_channel = self.active_channel.clone();
        self.retouch = if self.stroke_mask_edit {
            // A mask stroke paints only the mask: no retouch mirror, and
            // the retouch history stays valid (the layer is untouched).
            self.paint_capture = None;
            None
        } else if self.tool.is_retouch() {
            self.paint_capture = None;
            Some(self.begin_retouch(uv))
        } else {
            // A paint-tool stroke opens its own undo capture (one entry
            // per stroke on the shared history — see the module docs); the
            // before snapshot is taken lazily per tile at the first staged
            // batch, so a stroke that stages nothing records nothing.
            self.paint_capture = Some(PaintStrokeCapture::default());
            None
        };
        self.push_event_styled(uv, pressure, radius_scale, true);
    }

    /// Opens the retouch stroke starting at `uv`. Clone: the alt-click
    /// source resolves into the offset here (`source - start`, UV — every
    /// stroke in fixed mode, the first stroke after the click in follow
    /// mode), then the tracker starts in the start tile's local texels —
    /// the UV offset scaled by the texel density with V flipped, matching
    /// [`local_texel`].
    fn begin_retouch(&mut self, uv: Pos2) -> RetouchStroke {
        if let BrushTool::Clone(mut params) = self.tool {
            if let Some(source) = self.clone_source {
                if self.clone_source_fresh || !params.follow {
                    params.source_offset = (source.x - uv.x, source.y - uv.y);
                    self.resolved_clone_offset = Some(params.source_offset);
                    self.tool = BrushTool::Clone(params);
                }
                self.clone_source_fresh = false;
            }
        }
        let mut stroke = RetouchStroke::new(self.tool);
        if let Some(transforms) = &self.stroke_symmetry {
            stroke.set_symmetry(transforms.clone());
        }
        if let BrushTool::Clone(params) = self.tool {
            let tile = self.route_tile(uv);
            let tpv = self.texels_per_uv_for(tile);
            let start = local_texel([uv.x, uv.y], tile, tpv);
            let offset = clone_offset_texels(params.source_offset, tpv);
            stroke.begin_clone(tile, start, offset, params.follow);
        }
        stroke
    }

    /// Adopts the panel's tool (between strokes only — a tool swap
    /// mid-stroke would split one stroke across two ops), first writing a
    /// freshly resolved alt-click clone offset back into the panel's
    /// params so the panel shows the offset the next stroke uses.
    pub fn sync_tool(&mut self, panel_tool: &mut BrushTool) {
        if let (BrushTool::Clone(params), Some(offset)) =
            (&mut *panel_tool, self.resolved_clone_offset.take())
        {
            params.source_offset = offset;
        }
        if !self.stroking {
            self.tool = *panel_tool;
        }
    }

    /// Adopts the active preset's wash mode (between strokes only — a
    /// mode swap mid-stroke would split one stroke across two ops).
    /// Only the mode crosses here: the rest of the preset's params are
    /// still panel-local in v1 (the session paints with its own brush),
    /// and the overlap count comes from the live conditioner density
    /// (the density that actually spaced the stroke's dabs), not the
    /// preset's spacing.
    pub fn sync_wash_mode(&mut self, mode: WashMode) {
        if !self.stroking {
            self.wash_mode = mode;
        }
    }

    /// Syncs the symmetry settings with the panel: a manipulator edit made
    /// in the UV view since the last sync is handed BACK to the panel
    /// (the clone-offset pattern — else the panel's copy would overwrite
    /// it); otherwise the panel's settings are adopted. Adoption takes
    /// effect at the next `begin_stroke` (a live stroke keeps its
    /// snapshot), so a mid-stroke edit never splits one stroke.
    pub fn sync_symmetry(&mut self, panel: &mut SymmetrySettings) {
        if std::mem::take(&mut self.symmetry_from_view) {
            *panel = self.symmetry;
        } else {
            self.symmetry = *panel;
        }
    }

    /// The current symmetry settings (the UV view draws its guides from
    /// these).
    pub fn symmetry(&self) -> &SymmetrySettings {
        &self.symmetry
    }

    /// Adopts the panel's straight-line / angle-snap mode (between strokes
    /// only — a mode swap mid-stroke would split one stroke across two
    /// projections) and sets it on the live conditioner, whose anchor
    /// re-seeds at the next `begin_stroke`.
    pub fn sync_line_constraint(&mut self, mode: LineConstraintMode) {
        if !self.stroking {
            self.line_constraint = mode;
            self.conditioner.set_line_constraint(mode);
        }
    }

    /// The current straight-line / angle-snap mode.
    pub fn line_constraint(&self) -> LineConstraintMode {
        self.line_constraint
    }

    /// Exact stroke input on/off (between strokes only): `true` bypasses
    /// the one-euro + lazy-mouse smoothing so each point lands where it
    /// was sent — for strokes that are already intended geometry (the
    /// agent session's `paint_stroke`). Spacing, seams, symmetry and the
    /// mask/retouch hooks are unchanged. The GUI never sets it (pointer
    /// input wants the smoothing).
    pub fn set_exact_input(&mut self, exact: bool) {
        if !self.stroking {
            self.conditioner.set_exact(exact);
        }
    }

    /// The current paint projection (which surfaces may start strokes).
    pub fn projection(&self) -> PaintProjection {
        self.projection
    }

    /// Adopts the panel's paint projection (between strokes only — a mode
    /// swap mid-stroke would split one stroke across two surfaces).
    pub fn sync_projection(&mut self, mode: PaintProjection) {
        if !self.stroking {
            self.projection = mode;
        }
    }

    /// Begins a Screen-projection stroke at the viewport raycast result:
    /// `Some(uv)` starts at the hit UV, `None` (a miss) starts nothing —
    /// no orphan stroke, no orphan dabs. The viewport calls this on the
    /// opening event of a paint drag instead of [`Self::begin_stroke`].
    pub fn begin_screen_point(&mut self, hit: Option<Pos2>) {
        if let Some(uv) = hit {
            self.begin_stroke(uv);
        }
    }

    pub fn begin_screen_point_pressured(&mut self, hit: Option<Pos2>, pressure: f32) {
        if let Some(uv) = hit {
            self.begin_stroke_styled(uv, pressure, self.brush_size);
        }
    }

    /// Extends a Screen-projection stroke to the viewport raycast result:
    /// `Some(uv)` conditions the hit UV through the existing pipeline
    /// (seam mirrors, symmetry, wash, mask — all inherited, since this
    /// funnels into [`Self::extend_stroke`]); `None` (a miss) skips the
    /// stamp but keeps the stroke alive, so dragging off the mesh and back
    /// on continues the same stroke. A hit with no stroke in flight (the
    /// drag opened on a miss) starts the stroke there.
    pub fn extend_screen_point(&mut self, hit: Option<Pos2>) {
        if let Some(uv) = hit {
            if self.stroking {
                self.extend_stroke(uv);
            } else {
                self.begin_stroke(uv);
            }
        }
    }

    pub fn extend_screen_point_pressured(&mut self, hit: Option<Pos2>, pressure: f32) {
        if let Some(uv) = hit {
            if self.stroking {
                self.extend_stroke_styled(uv, pressure, self.brush_size);
            } else {
                self.begin_stroke_styled(uv, pressure, self.brush_size);
            }
        }
    }

    /// The mirror manipulator: moves the mirror line to `offset`
    /// (tile-local UV, clamped to `0..=1`) and queues the hand-back to
    /// the panel.
    pub fn set_mirror_offset(&mut self, offset: f32) {
        let offset = offset.clamp(0.0, 1.0);
        if self.symmetry.mirror.offset != offset {
            self.symmetry.mirror.offset = offset;
            self.symmetry_from_view = true;
        }
    }

    /// Whether the clone tool is active (the UV view's alt-click gate).
    pub fn clone_tool_active(&self) -> bool {
        matches!(self.tool, BrushTool::Clone(_))
    }

    /// The alt-click affordance: arms `uv` as the clone source. The next
    /// clone stroke resolves it into the offset (`source - stroke start`).
    pub fn set_clone_source(&mut self, uv: Pos2) {
        self.clone_source = Some(uv);
        self.clone_source_fresh = true;
    }

    /// The quick mask (the UV view draws its overlay from it, the panel
    /// its rows): the ACTIVE set's instance (see the `masks` field docs).
    pub fn quick_mask(&self) -> &QuickMask {
        self.masks
            .get(&self.active_set)
            .expect("the active set is always registered (set_active_set ensures it)")
    }

    /// `tile`'s set's mask (what the tile's strokes stamp and gate
    /// against — the per-tile form of [`Self::quick_mask`]).
    pub fn quick_mask_for_tile(&self, tile: u16) -> &QuickMask {
        let name = self.set_of_tile(tile).to_string();
        self.masks
            .get(&name)
            .expect("a tile's set is always registered (assign_tile_to_set ensures it)")
    }

    /// The active set's mask, mutably (the panel rows and shortcuts edit
    /// through here).
    fn active_mask_mut(&mut self) -> &mut QuickMask {
        let active = self.active_set.clone();
        self.masks
            .get_mut(&active)
            .expect("the active set is always registered (set_active_set ensures it)")
    }

    /// `tile`'s set's mask, mutably (registers the set first, so strokes
    /// routing to a never-sized set gate against a correctly sized plane).
    fn mask_of_tile_mut(&mut self, tile: u16) -> &mut QuickMask {
        let name = self.set_of_tile(tile).to_string();
        self.ensure_set(&name);
        self.masks
            .get_mut(&name)
            .expect("ensure_set just registered it")
    }

    /// The X shortcut: enters or leaves mask mode (entering arms gating,
    /// see `umber_brush::quick_mask`); returns the new state. A live
    /// stroke keeps the mode it began with.
    pub fn toggle_mask_mode(&mut self) -> bool {
        self.active_mask_mut().toggle_editing()
    }

    /// Enters (`true`) or leaves mask mode — the panel's toggle.
    pub fn set_mask_mode(&mut self, editing: bool) {
        self.active_mask_mut().set_editing(editing);
    }

    /// Arms or disarms gating. Disarming keeps the mask for the session.
    pub fn set_mask_active(&mut self, active: bool) {
        self.active_mask_mut().set_active(active);
    }

    /// What mask-mode strokes paint (white allows, black blocks).
    pub fn set_mask_paint(&mut self, paint: MaskPaint) {
        self.active_mask_mut().set_paint(paint);
    }

    /// Inverts the mask (the I shortcut in mask mode, or the panel
    /// button). Refused (`false`) mid-stroke: a live gated stroke must
    /// keep reading one mask. Applies to EVERY set's mask (the v1
    /// whole-mask semantics — invert carries no size, so per-set sizes
    /// are irrelevant to it).
    pub fn invert_mask(&mut self) -> bool {
        if self.stroking {
            return false;
        }
        for mask in self.masks.values_mut() {
            mask.invert();
        }
        self.mask_synced.clear();
        true
    }

    /// Zeroes the mask (the panel's Clear). Refused (`false`) mid-stroke,
    /// like [`Self::invert_mask`]. Like invert, session-wide across sets.
    pub fn clear_mask(&mut self) -> bool {
        if self.stroking {
            return false;
        }
        for mask in self.masks.values_mut() {
            mask.clear();
        }
        self.mask_synced.clear();
        true
    }

    /// Which layer's mask strokes paint (`None` = strokes paint layers).
    /// The Layers panel edits this; the live stroke keeps its snapshot.
    pub fn mask_edit_layer(&self) -> Option<u64> {
        self.mask_edit_layer
    }

    /// Selects the layer whose mask strokes paint (`None` leaves mask
    /// mode and paints layers again). Takes effect at the next
    /// `begin_stroke` — a live stroke keeps its snapshot, so a mid-stroke
    /// switch never splits one stroke across two planes.
    pub fn set_mask_edit_layer(&mut self, layer: Option<u64>) {
        self.mask_edit_layer = layer;
    }

    /// The target new strokes paint (per-layer-pool design §2.1, [M3]).
    pub fn active_target(&self) -> PlaneOwner {
        self.active_target
    }

    /// Whether the user chose [`Self::active_target`] explicitly ([M12]).
    pub fn active_explicit(&self) -> bool {
        self.active_explicit
    }

    /// [`Self::active_target`]'s stack index at the last sync ([m6]).
    pub fn active_index(&self) -> usize {
        self.active_index
    }

    /// The shell's last paintability verdict for the active target.
    /// Stage 4's status line reads this beside `stroke_refusal`.
    #[allow(
        dead_code,
        reason = "stage-4 status-line consumer; the gate reads the field"
    )]
    pub fn active_verdict(&self) -> Result<(), InstanceError> {
        self.active_verdict.clone()
    }

    /// The live stroke's owner, snapshotted at `begin_stroke` (like
    /// `stroke_channel`): a mid-stroke selection change never splits a
    /// stroke across two planes. Stage 4's routing reads this; stage 3
    /// forces it to `Base` (see `begin_stroke_styled`).
    #[allow(dead_code, reason = "stage-4 routing consumer; stage-3 tests read it")]
    pub fn stroke_owner(&self) -> PlaneOwner {
        self.stroke_owner
    }

    /// Why the last `begin_stroke` refused (`None` after an accepted
    /// stroke). Stage 4's status line renders this.
    #[allow(
        dead_code,
        reason = "stage-4 status-line consumer; stage-3 tests read it"
    )]
    pub fn stroke_refusal(&self) -> Option<PaintRefusal> {
        self.stroke_refusal.clone()
    }

    /// Selects the target new strokes paint (the Layers panel's row click,
    /// add button or mask-row selection, applied by the shell — §2.2).
    /// Explicit by construction: sets `active_explicit`, so an unpaintable
    /// target refuses at the next `begin_stroke`. Takes effect for the next
    /// stroke — a live stroke keeps its `stroke_owner` snapshot, so a
    /// mid-stroke change never splits one stroke across two planes.
    ///
    /// [m5] Reconciles with mask editing: clears `mask_edit_layer` unless
    /// it equals the target's layer id. Without this, a user editing L2's
    /// mask who adds a layer and paints would paint L2's MASK, because a
    /// set `mask_edit_layer` wins over everything in `push_event_styled`.
    pub fn set_active_target(&mut self, target: PlaneOwner) {
        let keep_mask = matches!(target, PlaneOwner::Layer(id) if self.mask_edit_layer == Some(id));
        if !keep_mask {
            self.mask_edit_layer = None;
        }
        self.active_target = target;
        self.active_explicit = true;
    }

    /// Pushes the shell's per-frame revalidation (§2.3): the target the
    /// shell resolved, its fresh `check_paintable` verdict (`Ok(())` for
    /// Base) and its current stack index. The `sync_tool`/`sync_symmetry`
    /// precedent: the shell owns the stack, this session does not.
    ///
    /// Judgment call — `active_explicit` survives a same-target push and
    /// clears when the target moves. The shell only moves the target when
    /// IT resolved it (open, new, or a vanished id), and a resolved target
    /// is auto (`false`) by §2.3; an untouched target keeps whatever the
    /// user chose. So an explicit Base selection survives adds, removes
    /// and reorders, while a vanished id's neighbour arrives auto.
    pub fn sync_active_target(
        &mut self,
        target: PlaneOwner,
        verdict: Result<(), InstanceError>,
        index: usize,
    ) {
        if target != self.active_target {
            self.active_explicit = false;
        }
        self.active_target = target;
        self.active_verdict = verdict;
        self.active_index = index;
    }

    /// What layer-mask strokes paint (white opens, black closes).
    pub fn set_layer_mask_paint(&mut self, paint: MaskPaint) {
        self.layer_mask_paint = paint;
    }

    /// `(layer, tile)`'s mask plane (`width × height` grayscale bytes at
    /// the tile's set size), or `None` while no stroke has touched it
    /// (it gates as fully open — see the module docs).
    pub fn layer_mask(&self, layer: u64, tile: u16) -> Option<&[u8]> {
        self.layer_masks.get(&(layer, tile)).map(Vec::as_slice)
    }

    /// Every `(layer, tile)` with a mask plane, ascending. The save path's
    /// census (filtered against live layers there).
    pub fn layer_mask_tiles(&self) -> Vec<(u64, u16)> {
        self.layer_masks.keys().copied().collect()
    }

    /// Allocates `(layer, tile)`'s plane white (fully open) at the tile's
    /// set size when it has none yet. White, not black: attaching a mask
    /// must be a no-op gate until painted.
    pub fn ensure_layer_mask(&mut self, layer: u64, tile: u16) {
        if self.layer_masks.contains_key(&(layer, tile)) {
            return;
        }
        let (width, height) = self.tile_size(tile);
        self.layer_masks.insert(
            (layer, tile),
            vec![u8::MAX; width as usize * height as usize],
        );
    }

    /// Restores one mask plane from the load path (the save path's
    /// byte-identical bytes). Rejects (`false`, plane untouched) when
    /// `values` is not the tile's set size — a torn sidecar must never
    /// gate the wrong texels. Materializes the tile first (like the
    /// channel pool's first-route rule); a tile that cannot be created
    /// also rejects.
    pub fn publish_layer_mask(&mut self, layer: u64, tile: u16, values: Vec<u8>) -> bool {
        if !self.ensure_tile(tile) {
            return false;
        }
        let (width, height) = self.tile_size(tile);
        if values.len() != width as usize * height as usize {
            return false;
        }
        self.layer_masks.insert((layer, tile), values);
        true
    }

    /// Drops every plane of `layer` (the Layers panel calls this when its
    /// mask is detached, so a re-attach starts white rather than
    /// resurrecting the old paint).
    pub fn remove_layer_masks(&mut self, layer: u64) {
        self.layer_masks.retain(|(id, _), _| *id != layer);
        if self.mask_edit_layer == Some(layer) {
            self.mask_edit_layer = None;
        }
    }

    /// Drops planes whose layer is not in `live` (the open path calls
    /// this after loading the stack, so a previous document's masks never
    /// gate the new one).
    pub fn prune_layer_masks(&mut self, live: &std::collections::HashSet<u64>) {
        self.layer_masks.retain(|(id, _), _| live.contains(id));
        if self.mask_edit_layer.is_some_and(|id| !live.contains(&id)) {
            self.mask_edit_layer = None;
        }
    }

    /// One layer-mask event on `tile`: stamps every dab into `(layer,
    /// tile)`'s `u8` plane toward the [`MaskPaint`] value. Stages nothing
    /// (the layer pixels are untouched) and records no undo (mask strokes
    /// are not journal entries — the quick-mask rule).
    ///
    /// Marks the composite dirty itself (§4.2 [M6]): `layer_mask_event`
    /// mutates a CPU mask Vec and returns before any staging, so no plane
    /// write and no snapshot diff can see it. The stamped footprint union
    /// (in SET-size texels) lands as a `Rect` on EVERY `(tile, channel)`
    /// where the layer has a plane — a material's mask edit dirties all
    /// its declared channels, not `stroke_channel`. Channels with an
    /// overridden size get the rect scaled, rounding outward. An instance
    /// has no planes of its own: its row in the last frame's snapshot
    /// names the SOURCE owner it composites (stage 5), and the dirt lands
    /// on the source's `(tile, channel)`s — the ones the instance's entry
    /// reads.
    fn layer_mask_event(&mut self, layer: u64, tile: u16, dabs: &[Dab]) {
        self.ensure_layer_mask(layer, tile);
        let target = self.layer_mask_paint.value();
        let target_byte = (target.clamp(0.0, 1.0) * 255.0).round() as u8;
        let (width, height) = self.tile_size(tile);
        let Some(plane) = self.layer_masks.get_mut(&(layer, tile)) else {
            return;
        };
        // A resize between `ensure_layer_mask` and now cannot happen
        // (both run between strokes on the same tile size), but a stale
        // plane must never gate the wrong texels: skip rather than stamp
        // misaligned.
        if plane.len() != width as usize * height as usize {
            return;
        }
        let mut stamped = None;
        for dab in dabs {
            let stamp = RetouchDab {
                pos: dab.pos,
                radius: dab.radius,
                alpha: dab.alpha,
                hardness: dab.hardness,
            };
            if let Some(rect) = footprint(stamp.pos, stamp.radius, width, height) {
                stamped = DirtyRect::union_opt(stamped, Some(rect));
            }
            stamp_grayscale_mask(plane, width, height, &stamp, target_byte);
        }
        // The dirt half ([M6]): every `(tile, channel)` carrying this
        // layer's planes recomposites with the edited mask.
        if let Some(rect) = stamped {
            let plane_owner = self
                .last_snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .layers
                        .iter()
                        .find(|row| row.layer_id == Some(layer))
                        .map(|row| row.owner)
                })
                .unwrap_or(PlaneOwner::Layer(layer));
            let channels: Vec<String> = self
                .planes
                .keys()
                .filter(|(owner, t, _)| *owner == plane_owner && *t == tile)
                .map(|(_, _, channel)| channel.clone())
                .collect();
            for channel in channels {
                let (cw, ch) = self.tile_channel_size(tile, &channel);
                let scaled = scale_rect_outward(rect, width, height, cw, ch);
                self.mark_dirty_rect(tile, &channel, scaled);
            }
        }
    }

    /// Selects the baked ID under `uv` on `tile`'s set mask (C1 worklist
    /// #11): `bake` is that set's ID bake (`width * height * 4` RGBA8,
    /// row-major — what `umber_bake::id::bake_id_mesh` returns, at the
    /// mask's own size). Fills `tile`'s plane with `1.0` on the clicked ID
    /// and `0.0` elsewhere via `umber_brush::id_mask::select_id` (which
    /// arms gating), then drops `tile`'s mask-sync flag so the next gated
    /// dab uploads the fresh plane before staging (the `sync_mask_tile`
    /// path — the upload rides the tile's FIFO with its `Stage` commands,
    /// so no gated dab can read the pre-selection mask). Other tiles'
    /// sync flags are kept: only this tile's plane changed. Returns the
    /// selected region's four bytes.
    ///
    /// # Errors
    ///
    /// Returns [`IdSelectError::StrokeInFlight`] mid-stroke, or forwards
    /// the bake/selection errors; on any error the mask is untouched.
    pub fn select_id_mask(
        &mut self,
        tile: u16,
        bake: &[u8],
        width: u32,
        height: u32,
        uv: [f32; 2],
    ) -> Result<[u8; 4], IdSelectError> {
        if self.stroking {
            return Err(IdSelectError::StrokeInFlight);
        }
        let id = select_id(self.mask_of_tile_mut(tile), tile, bake, width, height, uv)?;
        self.mask_synced.retain(|(_, t, _)| *t != tile);
        Ok(id)
    }

    /// Click-time ID-mask selection: bakes `mesh`'s region IDs at the
    /// clicked tile's set size, then [`Self::select_id_mask`]s the ID under
    /// `uv` (see that method for the fill + sync contract).
    ///
    /// Judgment call — bake on every click, no cached store: no persistent
    /// ID-bake store exists in the app yet (nothing calls `bake_id_mesh`
    /// outside its own tests), and inventing a new global cache for v1
    /// would split bake freshness across the Bakes panel's future ID rows.
    /// The cost is one full-frame compute bake per click at the set size
    /// (the 4K default = 16M texels); baking on tool-entry and re-baking
    /// on mesh change is the named follow-up. Single-tile v1 (see
    /// `umber_brush::id_mask`'s module docs): the tile routes by
    /// [`Self::route_tile`], but the bake spans the mesh's `0..1` UV frame.
    ///
    /// # Errors
    ///
    /// Same as [`Self::select_id_mask`], plus the bake's own errors (empty
    /// mesh, triangle budget — see `umber_bake::id`).
    pub fn bake_and_select_id(
        &mut self,
        mesh: &umber_mesh::MeshData,
        uv: [f32; 2],
    ) -> Result<[u8; 4], IdSelectError> {
        if self.stroking {
            return Err(IdSelectError::StrokeInFlight);
        }
        let tile = self.route_tile(Pos2::new(uv[0], uv[1]));
        let (width, height) = self.tile_size(tile);
        let bytes = bake_id_mesh(
            &self.device,
            &self.queue,
            mesh,
            &IdBakeParams {
                flavor: IdFlavor::Triangle,
                width,
                height,
            },
        )?;
        self.select_id_mask(tile, &bytes, width, height, uv)
    }

    /// Reverts the last stroke (paint or retouch — one entry per stroke);
    /// `false` when there is none.
    pub fn undo_stroke(&mut self) -> bool {
        let undone = self.retouch_history.undo(&mut self.retouch_writes);
        self.flush_retouch_writes();
        undone
    }

    /// Re-applies the last undone stroke (paint or retouch); `false` when
    /// there is none.
    pub fn redo_stroke(&mut self) -> bool {
        let redone = self.retouch_history.redo(&mut self.retouch_writes);
        self.flush_retouch_writes();
        redone
    }

    /// Whether [`Self::undo_stroke`] would do anything.
    pub fn can_undo_stroke(&self) -> bool {
        self.retouch_history.can_undo()
    }

    /// Whether [`Self::redo_stroke`] would do anything.
    pub fn can_redo_stroke(&self) -> bool {
        self.retouch_history.can_redo()
    }

    /// Click-to-time-travel for strokes: drives the single-step
    /// [`Self::undo_stroke`]/[`Self::redo_stroke`] to `target` stroke depth
    /// — the stroke-side twin of `Document::jump_to_depth`, and
    /// deliberately the same shape: a loop of single steps, never a
    /// truncate shortcut, saturating when a step returns `false` (an
    /// evicted depth stops at the journal's bottom instead of failing). A
    /// merged entry travels as one unit: the wall-clock strokes coalesced
    /// into it share one depth, so one step undoes them all (see the
    /// cumulative-merge docs above). Returns how many steps were actually
    /// taken.
    ///
    /// Housed here rather than in the History panel so both crate targets
    /// (the binary shell and the `lib.rs` test surface, which has no
    /// history module) drive and test the same loop: the panel calls this
    /// once per row click, and the body stays exactly the n-single-steps
    /// loop — the stacks are the source of truth, never a snapshot.
    pub fn jump_to_stroke_depth(&mut self, target: usize) -> usize {
        let mut steps = 0;
        while self.stroke_len() > target {
            if !self.undo_stroke() {
                break;
            }
            steps += 1;
        }
        while self.stroke_len() < target {
            if !self.redo_stroke() {
                break;
            }
            steps += 1;
        }
        steps
    }

    /// Number of entries on the stroke-undo journal (the History window's
    /// stroke-depth cursor: stroke rows jump here).
    pub fn stroke_len(&self) -> usize {
        self.retouch_history.len()
    }

    /// The stroke-history generation: bumped every time the journal is
    /// cleared wholesale ([`Self::resize_set`]), so the History window can
    /// drop rows that describe evicted commands.
    pub fn stroke_epoch(&self) -> u64 {
        self.stroke_epoch
    }

    /// Takes the [`StrokeCommit`]s queued since the last drain (usually
    /// zero or one — the app drains every frame).
    pub fn take_stroke_commits(&mut self) -> Vec<StrokeCommit> {
        std::mem::take(&mut self.pending_commits)
    }

    /// Queues the retouch history's pending region writes on their planes
    /// (owner + tile + channel — the undo key, see the module docs).
    /// Regions whose owner retired apply to the RETIRED bytes (§5.4 item
    /// 8: an undo-of-delete then revives the undone pixels too); regions
    /// whose owner is gone and freed drop loudly.
    fn flush_retouch_writes(&mut self) {
        // Drain into a local first: the writes and the target lookup both
        // borrow `self` mutably — the iterator can't hold the borrow while
        // `plane_of_mut` takes it.
        let writes: Vec<_> = self.retouch_writes.writes.drain(..).collect();
        for (owner, tile, channel, rect, rgba) in writes {
            if let PlaneOwner::Layer(id) = owner {
                if !self.planes.contains_key(&(owner, tile, channel.clone())) {
                    if let Some((_, planes)) = self.retired.get_mut(&id) {
                        let applied = planes
                            .iter_mut()
                            .filter(|plane| plane.tile == tile && plane.channel == channel)
                            .any(|plane| {
                                let row = plane.width as usize * 4;
                                let expect = rect.width() as usize * rect.height() as usize * 4;
                                if rgba.len() != expect {
                                    return false;
                                }
                                if rect.x1 > plane.width || rect.y1 > plane.height {
                                    return false;
                                }
                                for (i, y) in (rect.y0..rect.y1).enumerate() {
                                    let dst =
                                        (y as usize * plane.width as usize + rect.x0 as usize) * 4;
                                    plane.data[dst..dst + row]
                                        .copy_from_slice(&rgba[i * row..(i + 1) * row]);
                                }
                                true
                            });
                        if !applied {
                            log::warn!(
                                "undo: region for retired layer {id}/{channel} tile {tile} fits no tombstone; dropping"
                            );
                        }
                    } else {
                        log::warn!(
                            "undo: region for freed layer {id}/{channel} tile {tile}; dropping"
                        );
                    }
                    continue;
                }
            }
            match self.plane_of_mut(owner, tile, &channel) {
                Ok(Some(thread)) => {
                    let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                        x: rect.x0,
                        y: rect.y0,
                        width: rect.width(),
                        height: rect.height(),
                        rgba,
                    }]);
                    self.clear_committed(owner, tile, &channel);
                    self.mark_dirty_rect(tile, &channel, rect);
                }
                Ok(None) => {}
                Err(err) => {
                    log::warn!("undo: region for {owner:?}/{tile}/{channel} refused ({err:?})");
                }
            }
        }
    }

    /// Stages a fill layer's evaluated tile bytes into `tile`'s GPU target
    /// (the fill layer's tile/texture-set plumbing: evaluation is CPU-side
    /// in `fill_layer`, the upload rides the same `WriteRegion` queue as
    /// retouch strokes — one full-tile write, in FIFO order with the
    /// tile's staged dabs).
    ///
    /// Creates the tile target lazily on first use (like pointer routing).
    /// Returns `false` — dropping the write with a log — when the target
    /// cannot be created or `rgba` is not `width`x`height` RGBA8 (the fill
    /// must re-evaluate at the target size first; a torn upload is worse
    /// than a dropped one).
    ///
    /// `pub` (not `pub(crate)`) is the Gate-0 seam: the project open path
    /// restores sidecar PNGs through this call, and the journey test in
    /// `tests/gate0_journey.rs` — outside this crate — drives the same
    /// restore. No other caller needed the width.
    pub fn publish_fill(&mut self, tile: u16, width: u32, height: u32, rgba: Vec<u8>) -> bool {
        if rgba.len() != width as usize * height as usize * 4 {
            log::warn!(
                "fill: dropping {width}x{height} upload for tile {tile} ({} bytes)",
                rgba.len()
            );
            return false;
        }
        if !self.ensure_tile(tile) {
            return false;
        }
        if let Some(thread) = self.target_of_mut(tile, BASE_COLOR_CHANNEL) {
            let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                x: 0,
                y: 0,
                width,
                height,
                rgba,
            }]);
            self.clear_committed(PlaneOwner::Base, tile, BASE_COLOR_CHANNEL);
            self.cache_dirty
                .insert((tile, BASE_COLOR_CHANNEL.to_string()), DirtyRegion::Full);
            true
        } else {
            false
        }
    }

    /// [`Self::publish_fill`]'s channel-aware sibling: uploads `rgba` into
    /// `tile`'s `channel` target, materializing it first via
    /// [`Self::ensure_channel`] (lazy creation on first route). The
    /// project open path restores a per-channel sidecar PNG through this;
    /// `publish_fill` itself stays [`BASE_COLOR_CHANNEL`]-only (the Gate-0
    /// seam's existing signature is untouched).
    pub fn publish_fill_channel(
        &mut self,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> bool {
        if rgba.len() != width as usize * height as usize * 4 {
            log::warn!(
                "fill: dropping {width}x{height} upload for tile {tile}/{channel} ({} bytes)",
                rgba.len()
            );
            return false;
        }
        if !self.ensure_channel(tile, channel) {
            return false;
        }
        if let Some(thread) = self.target_of_mut(tile, channel) {
            let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                x: 0,
                y: 0,
                width,
                height,
                rgba,
            }]);
            self.clear_committed(PlaneOwner::Base, tile, channel);
            self.cache_dirty
                .insert((tile, channel.to_string()), DirtyRegion::Full);
            true
        } else {
            false
        }
    }

    /// Whether `(owner, tile, channel)` has a materialized plane: Base
    /// planes live in `targets` until the §1.4 move, layer-owned planes in
    /// `planes`.
    fn plane_exists(&self, owner: PlaneOwner, tile: u16, channel: &str) -> bool {
        match owner {
            PlaneOwner::Base => {
                if channel == BASE_COLOR_CHANNEL {
                    self.targets.contains_key(&tile)
                } else {
                    self.targets
                        .get(&tile)
                        .is_some_and(|t| t.channels.contains_key(channel))
                }
            }
            PlaneOwner::Layer(_) => self
                .planes
                .contains_key(&(owner, tile, channel.to_string())),
        }
    }

    /// Materializes `(owner, tile, channel)`'s plane at
    /// [`Self::tile_channel_size`] when it has none yet (§1.2): the one
    /// budget checkpoint ([M4]). A first layer-owned plane for its `(tile,
    /// channel)` also materializes that entry's cache thread, born
    /// `Full`-dirty so it can never be read uncomposited (§4.2's lazy
    /// cache rule). An existing plane is returned untouched: it never goes
    /// through the check again, so it keeps painting.
    ///
    /// Returns `Ok(true)` when the plane was created, `Ok(false)` when it
    /// already existed, or `Err(PaintRefusal::PlaneBudget)` when the new
    /// plane (plus its cache texture, if new) would push
    /// [`Self::plane_bytes`] past the hard cap, or when wgpu reported
    /// `OutOfMemory` creating it (§1.3). Nothing is created on refusal.
    pub fn materialize_plane(
        &mut self,
        owner: PlaneOwner,
        tile: u16,
        channel: &str,
    ) -> Result<bool, PaintRefusal> {
        if self.plane_exists(owner, tile, channel) {
            return Ok(false);
        }
        let (width, height) = self.tile_channel_size(tile, channel);
        let plane_len = width as u64 * height as u64 * 4;
        let cache_new = matches!(owner, PlaneOwner::Layer(_))
            && !self.cache.contains_key(&(tile, channel.to_string()));
        let prospective = self.plane_bytes() + plane_len + u64::from(cache_new) * plane_len;
        if prospective > self.plane_hard_cap_bytes {
            return Err(PaintRefusal::PlaneBudget { bytes: prospective });
        }
        let scope = umber_gpu::paint_thread::push_oom_scope(&self.device);
        match owner {
            PlaneOwner::Base => {
                self.ensure_channel(tile, channel);
            }
            PlaneOwner::Layer(_) => {
                let thread = PaintThread::with_compositor(
                    self.device.clone(),
                    self.queue.clone(),
                    width,
                    height,
                    self.compositor.clone(),
                );
                self.planes.insert(
                    (owner, tile, channel.to_string()),
                    Plane {
                        thread,
                        committed: None,
                    },
                );
                if cache_new {
                    self.cache.insert(
                        (tile, channel.to_string()),
                        PaintThread::with_compositor(
                            self.device.clone(),
                            self.queue.clone(),
                            width,
                            height,
                            self.compositor.clone(),
                        ),
                    );
                    self.cache_dirty
                        .insert((tile, channel.to_string()), DirtyRegion::Full);
                }
            }
        }
        if umber_gpu::paint_thread::await_oom_scope(&self.device, scope) {
            log::warn!(
                "plane budget: wgpu reported OutOfMemory materializing {owner:?}/{tile}/{channel}"
            );
            return Err(PaintRefusal::PlaneBudget { bytes: prospective });
        }
        Ok(true)
    }

    /// Resolves `(owner, tile, channel)`'s plane thread, mutably — the
    /// stage-4 routing move (§3): `plane_of_mut(stroke_owner, tile,
    /// &channel)` replaces `target_of_mut(tile, &channel)` in the
    /// stage/capture/retouch paths.
    ///
    /// Base dispatches to the targets store (Base planes stay there —
    /// the §1.4 store unification is deferred: every Base reader keeps
    /// today's exact path, and this dispatch is the seam a later move
    /// fills in). Layer owners materialize through
    /// [`Self::materialize_plane`] (budget refusals propagate — the
    /// stroke drops that tile's dabs and keeps painting its
    /// already-materialized planes).
    fn plane_of_mut(
        &mut self,
        owner: PlaneOwner,
        tile: u16,
        channel: &str,
    ) -> Result<Option<&mut PaintThread>, PaintRefusal> {
        if matches!(owner, PlaneOwner::Base) {
            return Ok(self.target_of_mut(tile, channel));
        }
        self.materialize_plane(owner, tile, channel)?;
        Ok(self
            .planes
            .get_mut(&(owner, tile, channel.to_string()))
            .map(|plane| &mut plane.thread))
    }

    /// Marks one event's dab footprint on `(tile, channel)`'s dirt (§4.2:
    /// every stroke write marks the cache dirty — the same `footprint`
    /// the undo capture computes, merged into the multi-rect
    /// [`DirtyRegion`]). A mirrored pair on opposite sides of a tile
    /// stays two rects (past 8 the closest pair merges).
    fn mark_dirty_rect(&mut self, tile: u16, channel: &str, rect: DirtyRect) {
        self.cache_dirty
            .entry((tile, channel.to_string()))
            .or_insert_with(|| DirtyRegion::Rects(Vec::new()))
            .mark_rect(rect);
    }

    /// Marks `owner`'s every `(tile, channel)` with planes `Full` (§4.2's
    /// metadata writer): hide / opacity / blend / reorder / channel
    /// params / mask attach / detach. The shell raises these from the
    /// per-frame snapshot diff ([`CompositeSnapshot::full_owners`]).
    pub fn mark_owner_full(&mut self, owner: PlaneOwner) {
        let keys: Vec<(u16, String)> = match owner {
            PlaneOwner::Base => self
                .targets
                .iter()
                .flat_map(|(tile, targets)| {
                    std::iter::once((*tile, BASE_COLOR_CHANNEL.to_string())).chain(
                        targets
                            .channels
                            .keys()
                            .map(|channel| (*tile, channel.clone())),
                    )
                })
                .collect(),
            PlaneOwner::Layer(_) => self
                .planes
                .keys()
                .filter(|(id, _, _)| *id == owner)
                .map(|(_, tile, channel)| (*tile, channel.clone()))
                .collect(),
        };
        for key in keys {
            self.cache_dirty.insert(key, DirtyRegion::Full);
        }
    }

    /// Clears `(owner, tile, channel)`'s committed record (§6.2 [B1]):
    /// EVERY pixel write (stage, `WriteRegion`, publish, `Resize`) sets
    /// it to `None`, or the save would clean-skip the plane and re-stage
    /// its pre-write bytes. (Stage 2 wired publish only; stage 4 wires
    /// the stroke, undo and resize writes — a stroke followed by an
    /// in-place save round-trips the stroke, never the old file.)
    fn clear_committed(&mut self, owner: PlaneOwner, tile: u16, channel: &str) {
        self.plane_committed
            .remove(&(owner, tile, channel.to_string()));
        if let Some(plane) = self.planes.get_mut(&(owner, tile, channel.to_string())) {
            plane.committed = None;
        }
    }

    /// The plane's committed record, if any (§6.2's [B1] seam): where its
    /// bytes were last written and the content hash they carried. The
    /// save loop asks, then decides the clean-skip.
    pub fn plane_committed(
        &self,
        owner: PlaneOwner,
        tile: u16,
        channel: &str,
    ) -> Option<(PathBuf, blake3::Hash)> {
        self.plane_committed
            .get(&(owner, tile, channel.to_string()))
            .map(|(dir, hash)| (dir.clone(), blake3::Hash::from_bytes(*hash)))
    }

    /// Marks the plane's bytes committed at `dir` with `hash` (§6.2):
    /// called after a sidecar write lands. Any later plane write
    /// ([`Self::publish_plane`]) clears it.
    pub fn mark_plane_committed(
        &mut self,
        owner: PlaneOwner,
        tile: u16,
        channel: &str,
        dir: PathBuf,
        hash: blake3::Hash,
    ) {
        self.plane_committed
            .insert((owner, tile, channel.to_string()), (dir, hash.into()));
    }

    /// Every materialized plane key `(owner, tile, channel)`, Base
    /// included (§1.1's save census — the save loop stages one sidecar
    /// per materialized plane, and the version decision asks which
    /// layer-owned planes exist). Base still lives in `targets` this
    /// stage (the additive-diff judgment call — it moves in stage 4),
    /// so the census walks BOTH stores: every tile's baseColor thread
    /// as `(Base, tile, "baseColor")`, every explicit channel thread
    /// as `(Base, tile, channel)`, then every layer-owned plane.
    /// Owned: callers iterate without a session borrow.
    pub fn planes_present(&self) -> Vec<(PlaneOwner, u16, String)> {
        let mut keys: Vec<(PlaneOwner, u16, String)> = Vec::new();
        for (tile, targets) in &self.targets {
            // A Base-owned plane in `planes` supersedes the targets
            // thread (stage 4 unifies the stores; until then the
            // published plane is the truth — never both).
            let explicit: Vec<String> = targets.channels.keys().cloned().collect();
            for channel in std::iter::once("baseColor".to_string()).chain(explicit) {
                if !self
                    .planes
                    .contains_key(&(PlaneOwner::Base, *tile, channel.clone()))
                {
                    keys.push((PlaneOwner::Base, *tile, channel));
                }
            }
        }
        keys.extend(self.planes.keys().cloned());
        keys
    }

    /// Drops every layer-owned plane, cache texture and dirty rect, and
    /// every plane's `committed` record (§1.5's session lifecycle — the
    /// open-path and `reset_for_new` call before loading, so a document
    /// never inherits another document's planes). Base's `targets` stay:
    /// the open path re-materializes them from the sidecars it loads.
    /// The selection resets to auto Base (§6.3 [M10]): layer ids restart
    /// at 0 in every `LayerStack`, so a carried selection would point at
    /// another document's layer. The next sync auto-resolves it (§2.3).
    pub fn clear_layer_planes(&mut self) {
        self.planes.clear();
        self.plane_committed.clear();
        self.cache.clear();
        self.cache_dirty.clear();
        self.retired.clear();
        self.retire_pending.clear();
        self.last_snapshot = None;
        self.last_base_visible = true;
        self.budget_warn_armed = true;
        self.over_budget = false;
        self.tombstone_notice = None;
        self.active_target = PlaneOwner::Base;
        self.active_explicit = false;
        self.active_index = 0;
        self.active_verdict = Ok(());
        self.stroke_owner = PlaneOwner::Base;
        self.stroke_refusal = None;
    }

    /// The shell's per-frame retire census (§5.4): `removed` names layer
    /// ids with planes that left the stack, with their kind (`true` =
    /// Paint, `false` = eval-owned Fill/Material — the shell reads the
    /// kind off its previous snapshot, since kinds never mutate).
    ///
    /// Paint planes queue for the amortized readback
    /// ([`Self::retire_pending`], at most one plane per flush); Fill and
    /// Material planes DROP with no tombstone (their revive re-dirties
    /// from the journal's `Insert` params and the next refresh
    /// regenerates them). Both mark their `(tile, channel)`s `Full`.
    /// The census skips the live `stroke_owner` while stroking at PUMP
    /// time ([m7]), so a queued owner still paints until `end_stroke`.
    pub fn note_removed_layers(&mut self, removed: &[(u64, bool)]) {
        for (id, is_paint) in removed {
            let owner = PlaneOwner::Layer(*id);
            let keys: Vec<(u16, String)> = self
                .planes
                .keys()
                .filter(|(own, _, _)| *own == owner)
                .map(|(_, tile, channel)| (*tile, channel.clone()))
                .collect();
            if keys.is_empty() {
                continue;
            }
            if *is_paint {
                self.retire_pending.retain(|entry| entry.0 != *id);
                for (tile, channel) in keys {
                    self.retire_pending.push((*id, tile, channel));
                }
            } else {
                for (tile, channel) in &keys {
                    self.planes.remove(&(owner, *tile, channel.clone()));
                    self.plane_committed
                        .remove(&(owner, *tile, channel.clone()));
                }
                for (tile, channel) in keys {
                    self.cache_dirty.insert((tile, channel), DirtyRegion::Full);
                }
            }
        }
    }

    /// CPU bytes currently held for deleted paint layers (§5.4 item 4).
    pub fn retired_bytes(&self) -> u64 {
        self.retired
            .values()
            .flat_map(|(_, planes)| planes)
            .map(|plane| plane.data.len() as u64)
            .sum()
    }

    /// Whether [`Self::plane_bytes`] crossed the advisory budget (the
    /// shell's budget status line reads this).
    pub fn is_over_budget(&self) -> bool {
        self.over_budget
    }

    /// Lowers (or raises) the enforced tombstone budget (see the
    /// `retired_cap_bytes` field docs). Tests lower it to pin the
    /// oldest-first eviction; production keeps
    /// [`RETIRED_BUDGET_BYTES`].
    pub fn set_retired_cap_bytes(&mut self, bytes: u64) {
        self.retired_cap_bytes = bytes;
    }

    /// Takes the pending tombstone-loss notice for the status line, if
    /// any: a freed delete's layer id (its undo revives EMPTY) or a
    /// budget-refused revive's.
    pub fn take_tombstone_notice(&mut self) -> Option<u64> {
        self.tombstone_notice.take()
    }

    /// Frees the oldest tombstones past the enforced budget,
    /// EXCLUDING the newest entry (§5.4 item 4): one oversize tombstone
    /// is retained while newest, so the last delete stays undoable.
    /// A freed entry arms [`Self::take_tombstone_notice`].
    ///
    /// History-evict freeing ("no command left in `doc.history` can
    /// revive its id", §5.4 item 5) is DEFERRED: it needs an
    /// `UndoStack::iter` surface in umber-core, outside this kill's
    /// lock. The cap bounds RAM regardless; deferral only keeps bytes
    /// longer (revives stay exact, never wrong).
    fn enforce_retired_cap(&mut self) {
        while self.retired_bytes() > self.retired_cap_bytes && self.retired.len() > 1 {
            let newest = self.retired.values().map(|(seq, _)| *seq).max();
            let victim = self
                .retired
                .iter()
                .filter(|(_, (seq, _))| Some(*seq) != newest)
                .min_by_key(|(_, (seq, _))| *seq)
                .map(|(id, _)| *id);
            let Some(id) = victim else {
                break;
            };
            self.retired.remove(&id);
            self.tombstone_notice = Some(id);
            log::warn!(
                "tombstone: freed layer {id}'s retired planes past the cap; undo revives it empty"
            );
        }
    }

    /// Reads back AT MOST ONE queued plane per call into
    /// [`Self::retired`] (§5.4 item 2 — deleting a 4-tile 2-channel
    /// paint layer is 8 flushes of one 64 MiB readback, not one hitch).
    /// The thread drops once its bytes land. A failed readback still
    /// drops the thread (its owner is gone; bytes are lost loudly, the
    /// session never crashes). Queued planes of the live `stroke_owner`
    /// are skipped while stroking ([m7]).
    fn pump_retirements(&mut self) {
        let skip = self.stroking.then_some(self.stroke_owner);
        let Some(index) = self
            .retire_pending
            .iter()
            .position(|(id, _, _)| Some(PlaneOwner::Layer(*id)) != skip)
        else {
            return;
        };
        let (id, tile, channel) = self.retire_pending.remove(index);
        let owner = PlaneOwner::Layer(id);
        match self.read_plane(owner, tile, &channel) {
            Some(image) => {
                let seq = self.retired_seq;
                self.retired_seq += 1;
                self.retired
                    .entry(id)
                    .or_insert((seq, Vec::new()))
                    .1
                    .push(RetiredPlane {
                        tile,
                        channel: channel.clone(),
                        width: image.width,
                        height: image.height,
                        data: image.data,
                    });
            }
            None => {
                log::warn!("tombstone: layer {id}/{channel} tile {tile} unreadable; dropping");
            }
        }
        self.planes.remove(&(owner, tile, channel.clone()));
        self.plane_committed.remove(&(owner, tile, channel));
        self.enforce_retired_cap();
    }

    /// Revives tombstoned bytes for layers back in `snapshot` (undo of
    /// `Remove`) and cancels pending readbacks whose owner rejoined:
    /// their still-live threads rejoin the composite (§5.4 items 2–3).
    /// Re-uploads go through [`Self::publish_plane`] (a budget refusal
    /// is a loud revive-empty naming the layer).
    fn revive_check(&mut self, snapshot: &CompositeSnapshot) {
        let live: BTreeSet<u64> = snapshot
            .layers
            .iter()
            .filter_map(|layer| layer.layer_id)
            .collect();
        self.retire_pending.retain(|entry| !live.contains(&entry.0));
        let revivable: Vec<u64> = self
            .retired
            .keys()
            .copied()
            .filter(|id| live.contains(id))
            .collect();
        for id in revivable {
            let has_planes = self
                .planes
                .keys()
                .any(|(owner, _, _)| *owner == PlaneOwner::Layer(id));
            if has_planes {
                continue;
            }
            let Some((_, planes)) = self.retired.remove(&id) else {
                continue;
            };
            for plane in planes {
                if let Err(err) = self.publish_plane(
                    PlaneOwner::Layer(id),
                    plane.tile,
                    &plane.channel,
                    plane.width,
                    plane.height,
                    plane.data,
                ) {
                    log::warn!("tombstone: revive of layer {id} refused ({err:?}); revives empty");
                    self.tombstone_notice = Some(id);
                    break;
                }
            }
        }
    }

    /// Retires ALL of `id`'s planes synchronously into [`Self::retired`]
    /// ([m7]'s `end_stroke` half): the owner left the stack mid-stroke,
    /// so there is no live layer to undo into — the capture is discarded
    /// and the bytes wait here for an undo-of-delete. Unreadable planes
    /// drop loudly.
    fn retire_owner_now(&mut self, id: u64) {
        let owner = PlaneOwner::Layer(id);
        let keys: Vec<(u16, String)> = self
            .planes
            .keys()
            .filter(|(own, _, _)| *own == owner)
            .map(|(_, tile, channel)| (*tile, channel.clone()))
            .collect();
        self.retire_pending.retain(|entry| entry.0 != id);
        for (tile, channel) in keys {
            match self.read_plane(owner, tile, &channel) {
                Some(image) => {
                    let seq = self.retired_seq;
                    self.retired_seq += 1;
                    self.retired
                        .entry(id)
                        .or_insert((seq, Vec::new()))
                        .1
                        .push(RetiredPlane {
                            tile,
                            channel: channel.clone(),
                            width: image.width,
                            height: image.height,
                            data: image.data,
                        });
                }
                None => {
                    log::warn!("tombstone: layer {id}/{channel} tile {tile} unreadable; dropping");
                }
            }
            self.planes.remove(&(owner, tile, channel.clone()));
            self.plane_committed.remove(&(owner, tile, channel));
        }
        self.enforce_retired_cap();
    }

    /// Backing-store bytes (§1.3's accounting, exactly): Σ `w·h·4` over
    /// materialized planes (Base threads in `targets`, layer-owned planes
    /// in `planes`) and cache textures, plus Σ `w·h` over the threads
    /// holding a GPU quick-mask copy. The Perf HUD reports this (stage 4).
    pub fn plane_bytes(&self) -> u64 {
        let mut bytes: u64 = 0;
        for tile_targets in self.targets.values() {
            bytes += thread_bytes(&tile_targets.base_color);
            for thread in tile_targets.channels.values() {
                bytes += thread_bytes(thread);
            }
        }
        for plane in self.planes.values() {
            bytes += thread_bytes(&plane.thread);
        }
        for thread in self.cache.values() {
            bytes += thread_bytes(thread);
        }
        bytes
    }

    /// Lowers (or raises) the enforced plane budget (see the
    /// `plane_hard_cap_bytes` field docs). Tests lower it to pin
    /// [`PaintRefusal::PlaneBudget`]; the Texture Sets panel will edit it
    /// beside the size combo (stage 4).
    pub fn set_plane_hard_cap_bytes(&mut self, bytes: u64) {
        self.plane_hard_cap_bytes = bytes;
    }

    /// The only PLANE accessor (§1.1, [B3]): `(owner, tile, channel)`'s
    /// texels as a CPU image. Drains the plane's queue first (the
    /// `read_target` precedent), then the blocking full-plane readback.
    /// `None` (logged) when the plane was never materialized, or when the
    /// drain or the readback fails. Reads never materialize: a layer that
    /// was never written holds no planes, and reading it stays `None`.
    pub fn read_plane(
        &mut self,
        owner: PlaneOwner,
        tile: u16,
        channel: &str,
    ) -> Option<RgbaBuffer> {
        if matches!(owner, PlaneOwner::Base) {
            return self.read_target(tile, channel);
        }
        let device = self.device.clone();
        let queue = self.queue.clone();
        let Some(plane) = self.planes.get_mut(&(owner, tile, channel.to_string())) else {
            // A retired owner (§5.4 item 4): the plane was tombstoned —
            // its bytes live in `retired`. Readers get them back so a
            // removed layer stays readable until its revive or drop.
            let layer_id = match owner {
                PlaneOwner::Layer(id) => id,
                PlaneOwner::Base => return None,
            };
            let retired = self
                .retired
                .get(&layer_id)?
                .1
                .iter()
                .find(|plane| plane.tile == tile && plane.channel == channel)?;
            return RgbaBuffer::new(retired.width, retired.height, retired.data.clone());
        };
        if let Err(err) = plane.thread.process_pending() {
            log::warn!("plane: draining {owner:?}/{tile}/{channel} failed: {err:#}");
            return None;
        }
        let target = plane.thread.paint_target();
        let (width, height) = target.dimensions();
        match target.read_back_rgba8(&device, &queue) {
            Ok(bytes) => RgbaBuffer::new(width, height, bytes),
            Err(err) => {
                log::warn!("plane: reading {owner:?}/{tile}/{channel} back failed: {err:#}");
                None
            }
        }
    }

    /// Publishes `rgba` (`width`x`height` RGBA8, row-major) whole-plane
    /// into `(owner, tile, channel)`'s plane (§5.1's fill shape with an
    /// owner, plus the materialization check): materializes through
    /// [`Self::materialize_plane`] (refusals propagate — the layer stays
    /// dirty and retries per §1.3), queues one full-plane `WriteRegion`,
    /// clears the plane's `committed`, and marks the `(tile, channel)`
    /// cache `Full` (the plane-write dirt writer, §4.2).
    ///
    /// A torn upload (byte count or dims not matching the plane's
    /// [`Self::tile_channel_size`]) is dropped with a log — the fill
    /// precedent ([`Self::publish_fill`]) — and still returns `Ok(())`:
    /// there is no refusal variant for caller error, only for budget.
    pub fn publish_plane(
        &mut self,
        owner: PlaneOwner,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<(), PaintRefusal> {
        if rgba.len() != width as usize * height as usize * 4 {
            log::warn!(
                "plane: dropping {width}x{height} upload for {owner:?}/{tile}/{channel} ({} bytes)",
                rgba.len()
            );
            return Ok(());
        }
        if (width, height) != self.tile_channel_size(tile, channel) {
            log::warn!(
                "plane: dropping {width}x{height} upload for {owner:?}/{tile}/{channel} (plane is {}x{})",
                self.tile_channel_size(tile, channel).0,
                self.tile_channel_size(tile, channel).1
            );
            return Ok(());
        }
        self.materialize_plane(owner, tile, channel)?;
        match owner {
            PlaneOwner::Base => {
                if let Some(thread) = self.target_of_mut(tile, channel) {
                    let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                        x: 0,
                        y: 0,
                        width,
                        height,
                        rgba,
                    }]);
                }
            }
            PlaneOwner::Layer(_) => {
                if let Some(plane) = self.planes.get_mut(&(owner, tile, channel.to_string())) {
                    let _ = plane.thread.publish(vec![PaintThreadCommand::WriteRegion {
                        x: 0,
                        y: 0,
                        width,
                        height,
                        rgba,
                    }]);
                    plane.committed = None;
                }
            }
        }
        self.plane_committed
            .remove(&(owner, tile, channel.to_string()));
        self.cache_dirty
            .insert((tile, channel.to_string()), DirtyRegion::Full);
        Ok(())
    }

    /// Recomposites every pending-dirty `(tile, channel)` cache from its
    /// planes (§4.1's flush — the end-of-update placement, AFTER the fill
    /// AND material refresh blocks, drain-first per composite set).
    ///
    /// Per `(tile, channel)` with dirt: `Rects` composite region-by-region
    /// through [`StackCompositor::composite_stack_region`]; a metadata
    /// `Full` composites whole through [`StackCompositor::composite_stack`]
    /// — but ONLY for the snapshot's displayed `(tile, channel)`s ([M8]'s
    /// coalescing: every other `Full` stays dirty and composites lazily
    /// on read or when displayed). Dirt without a cache thread is
    /// dropped (a Base-only `(tile, channel)` reads Base directly).
    ///
    /// The same call pumps one amortized tombstone readback and revives
    /// tombstoned bytes for rejoined layers (§5.4), and latches the
    /// advisory-budget warn (§1.3). Composite errors are loud (`warn`)
    /// and drop the dirt (a poisoned entry must not spam every frame;
    /// the next write re-marks).
    pub fn flush_composite(&mut self, snapshot: &CompositeSnapshot) {
        self.last_snapshot = Some(snapshot.clone());
        if self.last_base_visible != snapshot.base_visible {
            self.last_base_visible = snapshot.base_visible;
            if !snapshot.base_visible {
                let base_keys: Vec<(u16, String)> = self
                    .targets
                    .iter()
                    .flat_map(|(tile, targets)| {
                        std::iter::once((*tile, BASE_COLOR_CHANNEL.to_string())).chain(
                            targets
                                .channels
                                .keys()
                                .map(|channel| (*tile, channel.clone())),
                        )
                    })
                    .collect();
                for (tile, channel) in base_keys {
                    self.ensure_cache(tile, &channel);
                }
            }
            self.mark_owner_full(PlaneOwner::Base);
        }
        self.revive_check(snapshot);
        self.pump_retirements();
        if self.cache_dirty.is_empty() {
            self.check_budget_warn();
            return;
        }
        if let Err(err) = self.process_pending() {
            log::warn!("composite: draining the composite set failed: {err:#}");
            return;
        }
        let keys: Vec<(u16, String)> = self.cache_dirty.keys().cloned().collect();
        let mut encoder =
            self.device
                .create_command_encoder(&umber_gpu::WgpuCommandEncoderDescriptor {
                    label: Some("umber_flush_composite"),
                });
        let mut submitted = false;
        for (tile, channel) in keys {
            let Some(dirt) = self.cache_dirty.remove(&(tile, channel.clone())) else {
                continue;
            };
            if matches!(dirt, DirtyRegion::Full)
                && !snapshot.displayed.contains(&(tile, channel.clone()))
            {
                // [M8]'s coalescing: a non-displayed `Full` waits for a
                // read or for its turn on screen. The waiting read
                // composites against the synthesized stack (§8 T28).
                self.coalesced_fulls.insert((tile, channel.clone()));
                self.cache_dirty.insert((tile, channel), dirt);
                continue;
            }
            if !self.cache.contains_key(&(tile, channel.clone())) {
                // No cache thread YET (the lazy-cache rule): the dirt
                // survives — a later reader materializes the cache and
                // composites on read (§4.1 part 3). Dropping it here
                // would freeze a zeroed cache for the first read.
                self.cache_dirty.insert((tile, channel), dirt);
                continue;
            }
            submitted |= self.composite_dirty(tile, &channel, dirt, snapshot, &mut encoder);
        }
        if submitted {
            self.queue.submit(Some(encoder.finish()));
        }
        self.check_budget_warn();
    }

    /// Ensures `(tile, channel)`'s cache thread exists (the lazy-cache
    /// rule's hiding arm, §4.2): born `Full`-dirty so it can never be
    /// read uncomposited. No-op when present.
    fn ensure_cache(&mut self, tile: u16, channel: &str) {
        if self.cache.contains_key(&(tile, channel.to_string())) {
            return;
        }
        let (width, height) = self.tile_channel_size(tile, channel);
        self.cache.insert(
            (tile, channel.to_string()),
            PaintThread::with_compositor(
                self.device.clone(),
                self.queue.clone(),
                width,
                height,
                self.compositor.clone(),
            ),
        );
        self.cache_dirty
            .insert((tile, channel.to_string()), DirtyRegion::Full);
    }

    /// The flush's entry builder (§4.2), shared with the anchor prefix
    /// (§5.3 [m11]): resolves `snapshot`'s rows for `(tile, channel)` into
    /// per-entry blend/opacity/visibility + resampled mask bytes, in
    /// bottom-to-top order. A row without a plane contributes nothing (the
    /// transparent no-op). `stop_after = Some(owner)` truncates THROUGH
    /// that row (inclusive) — the downward partial flatten; `None` keeps
    /// every row (the flush).
    fn resolve_composite_entries(
        &self,
        snapshot: &CompositeSnapshot,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        stop_after: Option<PlaneOwner>,
    ) -> (Vec<ResolvedEntry>, Vec<Vec<u8>>) {
        let (set_w, set_h) = self.tile_size(tile);
        let mut owned_masks: Vec<Vec<u8>> = Vec::new();
        let mut resolved: Vec<ResolvedEntry> = Vec::new();
        for layer in &snapshot.layers {
            if let Some(entry) = self.resolve_row(
                layer,
                tile,
                channel,
                width,
                height,
                set_w,
                set_h,
                &mut owned_masks,
            ) {
                resolved.push(entry);
            }
            // The truncation is by ROW, not by entry: a stop row without a
            // plane (or with no entry on this channel) still ends the
            // prefix — nothing above the anchor may leak in.
            if stop_after.is_some_and(|stop| row_is(layer, stop)) {
                break;
            }
        }
        (resolved, owned_masks)
    }

    /// One snapshot row → its entry on `(tile, channel)`, or `None` when it
    /// contributes nothing: no plane there (a layer touched only on 1002
    /// has no entry on 1001 by construction), or a Material row with no
    /// contributing entry for this channel. A masked row pushes its
    /// resampled mask into `owned_masks` and records the index.
    #[allow(clippy::too_many_arguments)]
    fn resolve_row(
        &self,
        layer: &SnapshotLayer,
        tile: u16,
        channel: &str,
        width: u32,
        height: u32,
        set_w: u32,
        set_h: u32,
        owned_masks: &mut Vec<Vec<u8>>,
    ) -> Option<ResolvedEntry> {
        self.plane_thread(layer.owner, tile, channel)?;
        if layer.owner == PlaneOwner::Base {
            return Some(ResolvedEntry {
                owner: layer.owner,
                blend: BlendMode::Normal,
                opacity: 1.0,
                visible: layer.visible,
                mask_index: None,
            });
        }
        let layer_id = layer.layer_id?;
        let (blend, opacity) = if matches!(layer.kind, SnapshotLayerKind::Material) {
            // Material rows expand per contributing channel (§5.2); the
            // layer's own blend stays inert. A `None` channel list
            // (disabled, undeclared or forbidden on every channel)
            // contributes NO entry (`pixels: None`).
            let entry = layer
                .material_channels
                .as_ref()?
                .iter()
                .find(|c| c.channel == channel)?;
            (entry.blend, entry.opacity)
        } else if layer.blend == BlendMode::Passthrough
            && matches!(layer.kind, SnapshotLayerKind::Fill)
        {
            // A fill carrying `Passthrough` composites as `Normal`
            // (structural for folders, not a pixel op — §5.1).
            (BlendMode::Normal, layer.opacity)
        } else {
            (layer.blend, layer.opacity)
        };
        let mask_index = if layer.mask_enabled {
            self.layer_masks.get(&(layer_id, tile)).map(|plane| {
                owned_masks.push(crate::material_layer::resample_mask_nearest(
                    plane, set_w, set_h, width, height,
                ));
                owned_masks.len() - 1
            })
        } else {
            None
        };
        Some(ResolvedEntry {
            owner: layer.owner,
            blend,
            opacity,
            visible: layer.visible,
            mask_index,
        })
    }

    /// Records one `(tile, channel)`'s dirt into the encoder against
    /// `snapshot` (the cache thread exists — the caller ensured it).
    /// `true` when dispatches were recorded. On a composite error the
    /// dirt is dropped loudly (see [`Self::flush_composite`]).
    ///
    /// The encoder types ride `umber_gpu`'s re-exports (see the module
    /// docs): non-test code must not name bare `wgpu::` paths.
    fn composite_dirty(
        &mut self,
        tile: u16,
        channel: &str,
        dirt: DirtyRegion,
        snapshot: &CompositeSnapshot,
        encoder: &mut umber_gpu::WgpuCommandEncoder,
    ) -> bool {
        let Some(cache) = self.cache.get(&(tile, channel.to_string())) else {
            return false;
        };
        let (width, height) = cache.paint_target().dimensions();
        // The owned mask bytes every masked entry samples (rect uploads
        // are the compositor's per-dispatch affair — entries borrow these
        // full-size planes, §4.3 item 3).
        let (resolved, owned_masks) =
            self.resolve_composite_entries(snapshot, tile, channel, width, height, None);
        // The dispatch phase reborrows shared (every `&mut` use above
        // ended — NLL): entries borrow the planes, the owned masks and
        // the compositor together, and the encoder records onto the
        // caller's command buffer.
        let this = &*self;
        let entries: Vec<GpuStackLayer<'_>> = resolved
            .iter()
            .filter_map(|meta| {
                let thread = this.plane_thread(meta.owner, tile, channel)?;
                Some(GpuStackLayer {
                    blend_mode: meta.blend,
                    opacity: meta.opacity,
                    visible: meta.visible,
                    texture: Some(thread.paint_target()),
                    mask: meta.mask_index.map(|i| owned_masks[i].as_slice()),
                    premultiplied: true,
                })
            })
            .collect();
        let Some(output) = this.cache.get(&(tile, channel.to_string())) else {
            return false;
        };
        let output = output.paint_target();
        let result = match dirt {
            DirtyRegion::Full => this
                .stack_compositor
                .composite_stack(encoder, &entries, output),
            DirtyRegion::Rects(rects) => {
                let mut result = Ok(());
                for rect in rects {
                    let region = CompositeRect {
                        x: rect.x0,
                        y: rect.y0,
                        width: rect.width(),
                        height: rect.height(),
                    };
                    if let Err(err) = this
                        .stack_compositor
                        .composite_stack_region(encoder, &entries, output, region)
                    {
                        result = Err(err);
                        break;
                    }
                }
                result
            }
        };
        if let Err(err) = result {
            log::warn!("composite: {tile}/{channel} failed ({err:#}); dropping its dirt");
        }
        true
    }

    /// Resolves `(owner, tile, channel)`'s plane thread WITHOUT
    /// materializing (`None` for a never-written plane): the flush's
    /// entry lookup. Base dispatches to the targets store (the §1.4
    /// seam — see [`Self::plane_of_mut`]).
    fn plane_thread(&self, owner: PlaneOwner, tile: u16, channel: &str) -> Option<&PaintThread> {
        if matches!(owner, PlaneOwner::Base) {
            let targets = self.targets.get(&tile)?;
            if channel == BASE_COLOR_CHANNEL {
                Some(&targets.base_color)
            } else {
                targets.channels.get(channel)
            }
        } else {
            self.planes
                .get(&(owner, tile, channel.to_string()))
                .map(|plane| &plane.thread)
        }
    }

    /// Synthesizes an all-visible Normal/1.0 snapshot over the current
    /// plane owners (Base bottom, then layers by id): the headless
    /// fallback when a reader flushes before any frame flush set
    /// [`Self::last_snapshot`]. The app never hits it (it flushes every
    /// frame); tests that publish planes always flush with a real
    /// snapshot first. Order past a reorder is approximate (id order) —
    /// the fallback composites every plane exactly once, never double.
    fn synthesized_snapshot(&self) -> CompositeSnapshot {
        let mut layers = vec![SnapshotLayer {
            owner: PlaneOwner::Base,
            layer_id: None,
            kind: SnapshotLayerKind::Paint,
            blend: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        }];
        let mut owners: Vec<PlaneOwner> = self
            .planes
            .keys()
            .map(|(owner, _, _)| *owner)
            .filter(|owner| !matches!(owner, PlaneOwner::Base))
            .collect();
        owners.sort();
        owners.dedup();
        for owner in owners {
            layers.push(SnapshotLayer {
                owner,
                layer_id: match owner {
                    PlaneOwner::Layer(id) => Some(id),
                    PlaneOwner::Base => None,
                },
                kind: SnapshotLayerKind::Paint,
                blend: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                mask_enabled: false,
                material_channels: None,
            });
        }
        CompositeSnapshot {
            layers,
            base_visible: true,
            displayed: Vec::new(),
        }
    }

    /// Composites `(tile, channel)`'s pending dirt now (the flush-first
    /// half of §4.1 part 3): drains its composite set, dispatches against
    /// the LAST snapshot (never torn, at most one metadata edit stale —
    /// the v2 boundary), and submits. Dirt without a cache thread is
    /// dropped (Base reads directly). Deferred non-displayed `Full`
    /// marks are paid here too — this is where §4.2's coalesced entries
    /// get composited.
    fn ensure_composited(&mut self, tile: u16, channel: &str) {
        if !self.cache_dirty.contains_key(&(tile, channel.to_string())) {
            return;
        }
        if !self.cache.contains_key(&(tile, channel.to_string())) {
            // A dirt mark without a cache thread: a layer plane wrote
            // but no reader ever composited. Materialize the cache NOW
            // (the composite's target must exist for the dispatch) —
            // the §4.1 read contract: every composite reader gets a
            // freshly composited cache, first read included. Base-only
            // reads bypass this (they read Base directly at zero cost).
            self.ensure_cache(tile, channel);
        }
        let snapshot = if self.coalesced_fulls.remove(&(tile, channel.to_string())) {
            // A coalesced non-displayed `Full` (§4.2 [M8]) reads against
            // the synthesized all-visible stack — the headless fallback
            // the §8 T28 third arm pins. Ordinary reads keep the last
            // frame's snapshot, whatever it says.
            self.synthesized_snapshot()
        } else {
            self.last_snapshot
                .clone()
                .unwrap_or_else(|| self.synthesized_snapshot())
        };
        if let Err(err) = self.process_pending() {
            log::warn!("composite: reader drain failed: {err:#}");
            return;
        }
        let Some(dirt) = self.cache_dirty.remove(&(tile, channel.to_string())) else {
            return;
        };
        let mut encoder =
            self.device
                .create_command_encoder(&umber_gpu::WgpuCommandEncoderDescriptor {
                    label: Some("umber_ensure_composited"),
                });
        if self.composite_dirty(tile, channel, dirt, &snapshot, &mut encoder) {
            self.queue.submit(Some(encoder.finish()));
        }
    }

    /// `(tile, channel)`'s composited texels as a CPU image (flush-first
    /// composite reader, §1.1 [B3]): the cache when a layer-owned plane
    /// exists for it, else the Base plane directly (today's exact path
    /// at zero extra VRAM). The eyedropper, the painted-map export, the
    /// bake sources and the retouch sample all read through here.
    pub fn read_composite(&mut self, tile: u16, channel: &str) -> Option<RgbaBuffer> {
        self.ensure_composited(tile, channel);
        if let Some(cache) = self.cache.get_mut(&(tile, channel.to_string())) {
            let device = self.device.clone();
            let queue = self.queue.clone();
            if let Err(err) = cache.process_pending() {
                log::warn!("composite: draining {tile}/{channel} failed: {err:#}");
                return None;
            }
            let target = cache.paint_target();
            let (width, height) = target.dimensions();
            return match target.read_back_rgba8(&device, &queue) {
                Ok(bytes) => RgbaBuffer::new(width, height, bytes),
                Err(err) => {
                    log::warn!("composite: reading {tile}/{channel} back failed: {err:#}");
                    None
                }
            };
        }
        self.read_target(tile, channel)
    }

    /// The anchor read (§5.3, stage 5): the composite of `snapshot`'s
    /// entries for `(tile, channel)` from the bottom (Base included)
    /// THROUGH `anchor`'s row (inclusive) — the downward partial flatten.
    /// Runs on the CPU oracle [m11]: every prefix plane is `read_plane`d
    /// (each drains its queue first, so a layer refreshed earlier this
    /// frame is read post-publish — [M7]) and folded through
    /// `composite_stack_cpu` with the flush's own entry builder, so the
    /// bytes are bit-exact across machines and GPUs.
    ///
    /// `None` when `anchor` has no row in the snapshot (deleted or not a
    /// pixel layer — the caller reports it as an eval error, never a
    /// silent default), or when a prefix read or the composite fails
    /// (logged). An anchor whose prefix holds no planes on this tile
    /// composites to transparent at the channel size — "nothing painted
    /// below yet" is real content, not an error.
    pub fn read_anchor_prefix(
        &mut self,
        snapshot: &CompositeSnapshot,
        tile: u16,
        channel: &str,
        anchor: PlaneOwner,
    ) -> Option<RgbaBuffer> {
        use umber_gpu::compositor::{composite_stack_cpu, CpuStackLayer};
        if !snapshot.layers.iter().any(|row| row_is(row, anchor)) {
            return None;
        }
        let (width, height) = self.tile_channel_size(tile, channel);
        let (resolved, owned_masks) =
            self.resolve_composite_entries(snapshot, tile, channel, width, height, Some(anchor));
        let mut planes: Vec<Vec<u8>> = Vec::with_capacity(resolved.len());
        for entry in &resolved {
            let plane = self.read_plane(entry.owner, tile, channel)?;
            if (plane.width, plane.height) != (width, height) {
                log::warn!(
                    "anchor: {:?}/{tile}/{channel} is {}x{}, expected {width}x{height}; refusing",
                    entry.owner,
                    plane.width,
                    plane.height
                );
                return None;
            }
            planes.push(plane.data);
        }
        let layers: Vec<CpuStackLayer<'_>> = resolved
            .iter()
            .zip(&planes)
            .map(|(entry, bytes)| CpuStackLayer {
                blend_mode: entry.blend,
                opacity: entry.opacity,
                visible: entry.visible,
                rgba: Some(bytes.as_slice()),
                mask: entry.mask_index.map(|i| owned_masks[i].as_slice()),
                premultiplied: true,
            })
            .collect();
        match composite_stack_cpu(&layers, width, height) {
            Ok(bytes) => RgbaBuffer::new(width, height, bytes),
            Err(err) => {
                log::warn!("anchor: prefix composite {tile}/{channel} failed: {err}");
                None
            }
        }
    }

    /// Latches the advisory-budget warn (§1.3): crossing
    /// [`PLANE_BUDGET_BYTES`] logs once and latches
    /// [`Self::is_over_budget`]; falling back under re-arms the log.
    fn check_budget_warn(&mut self) {
        if self.plane_bytes() > PLANE_BUDGET_BYTES {
            self.over_budget = true;
            if self.budget_warn_armed {
                self.budget_warn_armed = false;
                log::warn!(
                    "plane budget: {} bytes past the advisory {} bytes",
                    self.plane_bytes(),
                    PLANE_BUDGET_BYTES
                );
            }
        } else {
            self.over_budget = false;
            self.budget_warn_armed = true;
        }
    }

    /// Extend the stroke to `uv`.
    pub fn extend_stroke(&mut self, uv: Pos2) {
        self.extend_stroke_styled(uv, 1.0, 1.0);
    }

    /// Extend the stroke to `uv` with an explicit pressure and
    /// brush-radius scale (the path replay's per-point style; live strokes
    /// use `1.0` for both through [`Self::extend_stroke`]).
    pub fn extend_stroke_styled(&mut self, uv: Pos2, pressure: f32, radius_scale: f32) {
        if !self.stroking {
            return;
        }
        self.push_event_styled(uv, pressure, radius_scale, true);
    }

    /// End the stroke (flushes nothing extra: `process_pending` drains
    /// all). A retouch stroke that changed texels becomes ONE retouch
    /// history entry, and so does a paint-tool stroke that wrote texels
    /// (its before snapshot plus the drained after-bytes — see the module
    /// docs).
    ///
    /// Each path that records also queues one [`StrokeCommit`] (the
    /// journal length on both sides of the push): the History window's
    /// per-frame drain turns commits into rows. A stroke that recorded
    /// nothing (identity, or mask-mode) queues nothing.
    pub fn end_stroke(&mut self) {
        self.stroking = false;
        // [m7] The owner-deleted-mid-stroke boundary: the verdict is one
        // frame old, and kinds never mutate, so a live stroke's owner can
        // vanish only by delete. The per-frame census skips the live
        // owner, so its planes are still here — retire them NOW into the
        // tombstone (there is no live layer to undo into) and discard the
        // stroke's undo capture with a log. Nothing crashes and no live
        // layer is touched. No snapshot yet (never flushed) reads as live.
        if let PlaneOwner::Layer(id) = self.stroke_owner {
            let gone = self.last_snapshot.as_ref().is_some_and(|snapshot| {
                !snapshot
                    .layers
                    .iter()
                    .any(|layer| layer.owner == PlaneOwner::Layer(id))
            });
            if gone {
                log::warn!("stroke: owner layer {id} deleted mid-stroke; retiring its planes");
                self.retire_owner_now(id);
                self.retouch = None;
                self.paint_capture = None;
                return;
            }
        }
        // Both paths record through `retouch::record_stroke`: merged into
        // the top entry when they overlap (the cumulative merge), and the
        // push's queued re-apply dropped — the stroke already uploaded its
        // bytes (retouch event by event; paint on the GPU, drained to read
        // them back).
        if let Some(stroke) = self.retouch.take() {
            let cursor = self.retouch_history.len();
            if stroke.record(&mut self.retouch_history, &mut self.retouch_writes) {
                self.pending_commits.push(StrokeCommit {
                    cursor,
                    depth: self.retouch_history.len(),
                });
            }
        }
        if let Some(capture) = self.paint_capture.take() {
            let cursor = self.retouch_history.len();
            if self.record_paint_capture(capture) {
                self.pending_commits.push(StrokeCommit {
                    cursor,
                    depth: self.retouch_history.len(),
                });
            }
        }
    }

    /// Ends a paint stroke's undo capture: drains each touched tile, reads
    /// it back, and records the stroke (its BEFORE snapshot and the AFTER
    /// readback, both full-tile so the merge can fill the union's corners).
    /// Tiles with no dirty rect, an unreadable tile, a rect that no longer
    /// fits the target (a resize landed mid-stroke), or byte-identical
    /// before/after over the rect (a fully gated-away stroke) contribute no
    /// region; no regions means no entry. Returns whether an entry was
    /// recorded (a merge into the top entry counts — it rewrote history).
    fn record_paint_capture(&mut self, capture: PaintStrokeCapture) -> bool {
        let channel = self.stroke_channel.clone();
        let owner = self.stroke_owner;
        let mut finished = Vec::new();
        for ((key_owner, tile), entry) in capture.tiles {
            // A mid-stroke selection change never splits the stroke (§2.3):
            // every tile in this capture belongs to the begin-time owner.
            // (A mismatched key would undo into a plane the stroke never
            // painted — skipped loudly, never applied.)
            if key_owner != owner {
                log::warn!("paint capture: owner mismatch, skipping tile {tile}");
                continue;
            }
            let Some(rect) = entry.dirty else {
                continue;
            };
            // Not materializing: `capture.tiles` only holds tiles whose
            // plane `capture_paint_event` already materialized. Reads the
            // PLANE (never the cache — §1.1 [B3]).
            let Some(after_image) = self.read_plane(owner, tile, &channel) else {
                continue;
            };
            if rect.x1 > after_image.width
                || rect.y1 > after_image.height
                || entry.before.width != after_image.width
                || entry.before.height != after_image.height
            {
                continue;
            }
            if entry.before.region(rect) == after_image.region(rect) {
                continue;
            }
            finished.push((tile, rect, entry.before, after_image));
        }
        let tiles: Vec<StrokeTile<'_>> = finished
            .iter()
            .map(|(tile, rect, start, end)| StrokeTile {
                owner,
                tile: *tile,
                channel: channel.clone(),
                rect: *rect,
                start,
                end,
            })
            .collect();
        record_stroke(&mut self.retouch_history, &mut self.retouch_writes, &tiles)
    }

    /// Replays `path` as ONE stroke in the `op` mode: each point becomes a
    /// styled event (its folded pressure, its radius scale) through the
    /// EXISTING pipeline — routing, seam mirrors, symmetry, wash, mask,
    /// then the splat for paint or the CPU mirror for erase/smudge (the
    /// consumer `op` already selects). A closed path appends its sealing
    /// segment, like `umber_brush::path`'s headless replay.
    ///
    /// `false` (nothing staged) with fewer than 2 points or while a live
    /// stroke runs — a replay never splits a stroke. The session tool is
    /// restored afterwards, so replaying never disturbs the panel tool.
    pub fn replay_path(&mut self, path: &PaintPath, op: &PathOp) -> bool {
        if self.stroking || path.len() < 2 {
            return false;
        }
        let saved = self.tool;
        self.tool = op.to_brush_tool();
        let points = path.points();
        let first = &points[0];
        self.begin_stroke_styled(
            Pos2::new(first.uv[0], first.uv[1]),
            first.effective_pressure(),
            first.sanitized_size(),
        );
        for point in &points[1..] {
            self.extend_stroke_styled(
                Pos2::new(point.uv[0], point.uv[1]),
                point.effective_pressure(),
                point.sanitized_size(),
            );
        }
        if path.is_closed() {
            self.extend_stroke_styled(
                Pos2::new(first.uv[0], first.uv[1]),
                first.effective_pressure(),
                first.sanitized_size(),
            );
        }
        self.end_stroke();
        self.tool = saved;
        true
    }

    /// Stamps a closed path's interior on the active tile: the loop runs
    /// through the polygon fill's even-odd rasterizer
    /// ([`rasterize_polygon_mask`], reused — no duplicated parity math),
    /// and each covered row run replays as one stroke at the path's mean
    /// style through the same pipeline as [`Self::replay_path`].
    ///
    /// `false` (nothing staged) unless the path can fill, while a live
    /// stroke runs, or when the loop covers no texel of the active tile.
    /// V1 boundaries: the active tile only (a loop spanning tiles fills
    /// the tile under the last pointer — per-event routing is the named
    /// follow-up), and one stroke per row run. Each run records through
    /// the cumulative merge (`retouch::record_stroke`), so runs whose
    /// footprints overlap the previous run's (adjacent rows at the brush
    /// radius) coalesce into one undo entry; a run disjoint from its
    /// predecessor, or a crossing the size guard rejects, stacks its own.
    pub fn replay_filled_path(&mut self, path: &PaintPath, op: &PathOp) -> bool {
        if self.stroking || !path.can_fill() {
            return false;
        }
        let tile = self.active_tile;
        let (width, height) = self.tile_size(tile);
        if width == 0 || height == 0 {
            return false;
        }
        let tpv = self.texels_per_uv_for(tile);
        let mask = rasterize_polygon_mask(&path.loop_uv(), tile, width, height);
        let runs = fill_runs(&mask, width as usize, height as usize);
        if runs.is_empty() {
            return false;
        }
        let saved = self.tool;
        self.tool = op.to_brush_tool();
        let (pressure, size) = path.mean_style();
        for run in &runs {
            let start = texel_center_uv(tile, run.x0, run.y, tpv);
            self.begin_stroke_styled(Pos2::new(start[0], start[1]), pressure, size);
            for x in run.x0 + 1..=run.x1 {
                let uv = texel_center_uv(tile, x, run.y, tpv);
                self.extend_stroke_styled(Pos2::new(uv[0], uv[1]), pressure, size);
            }
            self.end_stroke();
        }
        self.tool = saved;
        true
    }

    /// Drains pending paint commands on EVERY materialized thread —
    /// `base_color` and every routed channel, on every tile (mirrors and
    /// strokes that left a tile may have queued work off the active tile),
    /// plus every layer-owned plane and every cache texture — and returns
    /// the cumulative stats summed across all of them. With one tile and
    /// no routed channel this is exactly that thread's stats.
    ///
    /// # Errors
    ///
    /// Propagates GPU-processing failures.
    pub fn process_pending(&mut self) -> Result<FrameStats, umber_gpu::paint::PaintError> {
        let mut total = FrameStats::default();
        for tile_targets in self.targets.values_mut() {
            for thread in tile_targets.threads_mut() {
                let stats = thread.process_pending()?;
                total.dabs_composited += stats.dabs_composited;
                total.dispatches += stats.dispatches;
                total.clears += stats.clears;
                total.segments += stats.segments;
            }
        }
        for plane in self.planes.values_mut() {
            let stats = plane.thread.process_pending()?;
            total.dabs_composited += stats.dabs_composited;
            total.dispatches += stats.dispatches;
            total.clears += stats.clears;
            total.segments += stats.segments;
        }
        for thread in self.cache.values_mut() {
            let stats = thread.process_pending()?;
            total.dabs_composited += stats.dabs_composited;
            total.dispatches += stats.dispatches;
            total.clears += stats.clears;
            total.segments += stats.segments;
        }
        self.last_stats = total;
        Ok(self.last_stats)
    }

    /// Cumulative stats of the last processed frame.
    pub fn last_stats(&self) -> FrameStats {
        self.last_stats
    }

    /// Read-only access to the ACTIVE tile's baseColor COMPOSITE texture
    /// (for display callbacks and the single-target export paths): the
    /// cache when a layer-owned plane exists for it (flushed at the last
    /// end-of-update), else the Base plane directly. GPU binders see the
    /// end-of-update flush — they sample at render time, after update
    /// returns (§4.1 part 2).
    pub fn paint_target(&self) -> &umber_gpu::paint::PaintTarget {
        self.composite_target(self.active_tile, BASE_COLOR_CHANNEL)
            .expect("tile 1001 is created in PaintState::new and never removed")
    }

    /// Read-only access to `tile`'s baseColor COMPOSITE texture, or `None`
    /// when nothing has routed to that tile yet (the per-tile export
    /// reads each present tile through this). Cache-else-Base, like
    /// [`Self::paint_target`].
    pub fn tile_target(&self, tile: u16) -> Option<&umber_gpu::paint::PaintTarget> {
        self.composite_target(tile, BASE_COLOR_CHANNEL)
    }

    /// Read-only access to `tile`'s `channel` COMPOSITE texture, or `None`
    /// when the channel has not materialized on that tile yet (lazy — see
    /// the tile-pool design doc). `BASE_COLOR_CHANNEL` routes to the
    /// baseColor field, mirroring the routing rules everywhere else. The
    /// viewport's height bind (audit #12's (c) seam) reads through this:
    /// it must NEVER create a target as a side effect of binding.
    /// Cache-else-Base, like [`Self::paint_target`].
    pub fn channel_target(
        &self,
        tile: u16,
        channel: &str,
    ) -> Option<&umber_gpu::paint::PaintTarget> {
        self.composite_target(tile, channel)
    }

    /// Resolves `(tile, channel)`'s COMPOSITE texture without flushing
    /// (§1.1 [B3]): the cache thread when one is materialized (a
    /// layer-owned plane exists), else the Base plane's thread. Reads
    /// never materialize.
    fn composite_target(&self, tile: u16, channel: &str) -> Option<&umber_gpu::paint::PaintTarget> {
        if let Some(cache) = self.cache.get(&(tile, channel.to_string())) {
            return Some(cache.paint_target());
        }
        let entry = self.targets.get(&tile)?;
        if channel == BASE_COLOR_CHANNEL {
            Some(entry.base_color.paint_target())
        } else {
            entry.channels.get(channel).map(|t| t.paint_target())
        }
    }

    /// The tiles this session holds a paint target for, ascending —
    /// tile 1001 always, plus every tile an event (or seam mirror) has
    /// routed to. The per-tile export iterates exactly these.
    pub fn tiles_present(&self) -> Vec<u16> {
        // BTreeMap keys iterate sorted.
        self.targets.keys().copied().collect()
    }

    /// Every channel materialized on `tile` besides baseColor, ascending
    /// (baseColor itself is implicit — present for every tile `tiles_present`
    /// lists). The per-channel save path's census.
    pub fn channels_present(&self, tile: u16) -> Vec<String> {
        self.targets
            .get(&tile)
            .map(|t| t.channels.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Overrides the stroke color (the per-tile export test paints each
    /// tile a different color; the Gate-0 journey test paints its stroke
    /// pure green so the painted texel is color-identifiable).
    ///
    /// `pub` (not `pub(crate)`) is the Gate-0 seam alongside
    /// [`Self::publish_fill`]: the journey test lives outside this crate.
    pub fn set_brush_color(&mut self, color: [f32; 4]) {
        self.brush.color = color;
    }

    /// The live-stroke brush radius in target texels (the radius every
    /// viewport dab is stamped with, before per-event pressure scaling):
    /// `BRUSH_RADIUS_TEXELS × brush_size`. Read by the viewport's brush
    /// cursor (GUI plan G1).
    pub fn brush_radius_texels(&self) -> f32 {
        BRUSH_RADIUS_TEXELS * self.brush_size
    }

    /// The live brush size multiplier (`1.0` = the historical 24-texel
    /// brush).
    pub fn brush_size(&self) -> f32 {
        self.brush_size
    }

    /// Sets the live brush size multiplier, clamped through the path
    /// tool's `sanitize_size` (`0.05..=8.0`; non-finite resets to `1.0`).
    /// A stroke in flight keeps the size it began with: the change applies
    /// from the next stroke, so a resize gesture never kinks a stroke.
    pub fn set_brush_size(&mut self, size: f32) {
        self.brush_size = sanitize_size(size);
    }

    /// Sets the brush hardness, clamped to `0..=1` (non-finite ignored).
    pub fn set_brush_hardness(&mut self, hardness: f32) {
        if hardness.is_finite() {
            self.brush.hardness = hardness.clamp(0.0, 1.0);
        }
    }

    /// The brush's hardness (`0..=1`): the fraction of the radius painted
    /// at full coverage. Read by the brush cursor's inner ring.
    pub fn brush_hardness(&self) -> f32 {
        self.brush.hardness
    }

    /// The tile the last original pointer event routed to (v1: the tile
    /// the UV view displays, and whose quick mask it overlays).
    pub fn active_tile(&self) -> u16 {
        self.active_tile
    }

    /// Overrides the whole brush (test hook: the quick-mask tests paint
    /// hard full-alpha masks so their values are exactly 0 or 1).
    #[cfg(test)]
    pub(crate) fn set_brush_params(&mut self, params: BrushParams) {
        self.brush = params;
    }

    /// The ACTIVE set's paint-target size (as of the last
    /// [`Self::resize_set`] on it; applied on the next drain). The Export
    /// dialog's painted-source badge gates on this matching the export
    /// size. Per-tile sizes: [`Self::tile_size`].
    pub fn target_size(&self) -> (u32, u32) {
        let active = self.active_set.clone();
        self.size_of_set(&active)
    }

    /// Switches the ACTIVE set's paint targets to `width`x`height`
    /// (contents dropped); that set's later tiles use the same size, other
    /// sets are untouched. The legacy single-size path (every pre-per-set
    /// caller — the perf harnesses, the old panel): single-set sessions
    /// hold every tile in the active set, so they behave byte-identically.
    /// Prefer [`Self::resize_set`] for a named set.
    ///
    /// INTENTIONAL: the resize clears the stroke-undo history (rects
    /// recorded at the old size no longer fit the targets) and resets the
    /// set's quick mask with its targets' contents (the `Resize` command
    /// drops each target's GPU mask too). The UI names this per set (the
    /// Texture Sets panel's hint string), so a size change is never a
    /// silent undo loss.
    pub fn resize_target(&mut self, width: u32, height: u32) {
        let active = self.active_set.clone();
        self.resize_set(&active, width, height);
    }

    /// Switches `set`'s paint targets to `width`x`height` (contents
    /// dropped); the set's later tiles use the same size, other sets are
    /// untouched. Registers the set first. Channel overrides SURVIVE: an
    /// overridden channel keeps ITS size while baseColor and inheriting
    /// channels rebuild at the set's (the per-channel sizing contract —
    /// see the module docs). The history-clear and mask
    /// contracts are the same as [`Self::resize_target`]'s, scoped to the
    /// set's tiles (the history itself is shared, so the clear is
    /// session-wide — see the module docs).
    pub fn resize_set(&mut self, set: &str, width: u32, height: u32) {
        self.ensure_set(set);
        self.set_sizes.insert(set.to_string(), (width, height));
        if self.active_set == set {
            self.texels_per_uv = width as f32;
        }
        // Snapshots recorded at the old size no longer fit the targets.
        self.retouch_history.clear();
        // Nor do their History-window rows (which the per-frame drain
        // drops when it sees the bumped epoch below), nor the queued
        // commits describing them.
        self.pending_commits.clear();
        self.stroke_epoch += 1;
        // Nor does the set's mask: it resets with the targets' contents
        // (the `Resize` command drops each target's GPU mask too). Other
        // sets' masks are untouched.
        if let Some(mask) = self.masks.get_mut(set) {
            mask.resize(width, height);
        }
        let tiles = self.tiles_of_set(set);
        self.mask_synced.retain(|(_, t, _)| !tiles.contains(t));
        // That set's layer-mask planes drop with the targets (a plane
        // recorded at the old size no longer fits — the quick-mask rule).
        // Other sets' tiles' planes are untouched.
        self.layer_masks
            .retain(|(_, tile), _| !tiles.contains(tile));
        // Channel overrides survive the set resize: an overridden
        // channel keeps ITS size while baseColor and inheriting channels
        // rebuild at the set's. Snapshotted first — the loop below holds
        // each tile's targets mutably while the sizes come from the
        // session map (E0502 otherwise).
        let overrides: BTreeMap<String, (u32, u32)> = self
            .channel_sizes
            .iter()
            .filter(|(key, _)| key.0.as_str() == set)
            .map(|(key, size)| (key.1.clone(), *size))
            .collect();
        for tile in &tiles {
            if let Some(tile_targets) = self.targets.get_mut(tile) {
                let _ = tile_targets
                    .base_color
                    .publish(vec![PaintThreadCommand::Resize { width, height }]);
                for (channel, thread) in tile_targets.channels.iter_mut() {
                    let (cw, ch) = overrides.get(channel).copied().unwrap_or((width, height));
                    let _ = thread.publish(vec![PaintThreadCommand::Resize {
                        width: cw,
                        height: ch,
                    }]);
                }
            }
            // The committed record dies with the contents (the save's
            // clean-skip must read the resized plane back, never re-stage
            // the old file). Collected after the loop above — the channel
            // list and the clear both borrow `self` (E0502 otherwise).
            let channels: Vec<String> = self
                .targets
                .get(tile)
                .map(|targets| targets.channels.keys().cloned().collect())
                .unwrap_or_default();
            self.clear_committed(PlaneOwner::Base, *tile, BASE_COLOR_CHANNEL);
            for channel in &channels {
                self.clear_committed(PlaneOwner::Base, *tile, channel);
            }
        }
        // Every layer-owned plane and cache texture on the set's tiles
        // rebuilds too (contents dropped — the same destructive contract
        // as the targets above), and every resized `(tile, channel)`
        // marks `Full` (§5.5). Channel overrides are honored per channel,
        // as for targets today.
        let plane_keys: Vec<(PlaneOwner, u16, String)> = self
            .planes
            .keys()
            .filter(|(_, tile, _)| tiles.contains(tile))
            .cloned()
            .collect();
        for (owner, tile, channel) in &plane_keys {
            let (cw, ch) = self.tile_channel_size(*tile, channel);
            if let Some(plane) = self.planes.get_mut(&(*owner, *tile, channel.clone())) {
                let _ = plane.thread.publish(vec![PaintThreadCommand::Resize {
                    width: cw,
                    height: ch,
                }]);
            }
            self.clear_committed(*owner, *tile, channel);
            self.cache_dirty
                .insert((*tile, channel.clone()), DirtyRegion::Full);
        }
        let cache_keys: Vec<(u16, String)> = self
            .cache
            .keys()
            .filter(|(tile, _)| tiles.contains(tile))
            .cloned()
            .collect();
        for (tile, channel) in &cache_keys {
            let (cw, ch) = self.tile_channel_size(*tile, channel);
            if let Some(thread) = self.cache.get_mut(&(*tile, channel.clone())) {
                let _ = thread.publish(vec![PaintThreadCommand::Resize {
                    width: cw,
                    height: ch,
                }]);
            }
            self.cache_dirty
                .insert((*tile, channel.clone()), DirtyRegion::Full);
        }
        // The resized set's tombstones drop (§5.4 item 6): contents are
        // dropped by the resize anyway, and old-size bytes would fail the
        // revive's publish. Other sets' tombstones survive.
        for (_, planes) in self.retired.values_mut() {
            planes.retain(|plane| !tiles.contains(&plane.tile));
        }
        self.retired.retain(|_, (_, planes)| !planes.is_empty());
        self.retire_pending
            .retain(|entry| !tiles.contains(&entry.1));
    }

    /// Switches `channel`'s paint targets in `set` to `width`x`height`
    /// (contents dropped) and records the override for later tiles: the
    /// channel's `resize_set`. ONLY that channel's materialized targets
    /// on that set's tiles are rebuilt — every other channel (and every
    /// other set) is untouched. Registers the set first. The history
    /// clear is the same session-wide contract as [`Self::resize_set`]'s
    /// (snapshots recorded at the old size no longer fit), and the
    /// stroke-epoch bump drops the History window's rows the same way.
    /// The quick mask and the layer-mask planes are per-SET (see the
    /// module docs) and stay at the set's size. [`BASE_COLOR_CHANNEL`]
    /// delegates to [`Self::resize_set`] (it IS the set's canvas).
    pub fn resize_channel(&mut self, set: &str, channel: &str, width: u32, height: u32) {
        if channel == BASE_COLOR_CHANNEL {
            self.resize_set(set, width, height);
            return;
        }
        self.ensure_set(set);
        self.channel_sizes
            .insert((set.to_string(), channel.to_string()), (width, height));
        // Snapshots recorded at the old size no longer fit the targets.
        self.retouch_history.clear();
        self.pending_commits.clear();
        self.stroke_epoch += 1;
        // ONLY this channel's materialized planes on this set's tiles:
        // a never-materialized channel costs nothing (its size is
        // recorded above for `ensure_channel`), and inheriting channels
        // keep the set's size from `resize_set`.
        let tiles = self.tiles_of_set(set);
        for tile in &tiles {
            if let Some(tile_targets) = self.targets.get_mut(tile) {
                if let Some(thread) = tile_targets.channels.get_mut(channel) {
                    let _ = thread.publish(vec![PaintThreadCommand::Resize { width, height }]);
                }
            }
            self.clear_committed(PlaneOwner::Base, *tile, channel);
            let owners: Vec<PlaneOwner> = self
                .planes
                .keys()
                .filter(|(_, t, c)| *t == *tile && c == channel)
                .map(|(owner, _, _)| *owner)
                .collect();
            for owner in owners {
                if let Some(plane) = self.planes.get_mut(&(owner, *tile, channel.to_string())) {
                    let _ = plane
                        .thread
                        .publish(vec![PaintThreadCommand::Resize { width, height }]);
                }
                self.clear_committed(owner, *tile, channel);
            }
            if let Some(thread) = self.cache.get_mut(&(*tile, channel.to_string())) {
                let _ = thread.publish(vec![PaintThreadCommand::Resize { width, height }]);
            }
            self.cache_dirty
                .insert((*tile, channel.to_string()), DirtyRegion::Full);
        }
        // The resized channel's tombstones drop (see `resize_set`); other
        // channels' and sets' survive.
        for (_, planes) in self.retired.values_mut() {
            planes.retain(|plane| !(tiles.contains(&plane.tile) && plane.channel == channel));
        }
        self.retired.retain(|_, (_, planes)| !planes.is_empty());
        self.retire_pending
            .retain(|entry| !(tiles.contains(&entry.1) && entry.2 == *channel));
    }

    /// Drops `channel`'s override in `set` (it inherits the set's size
    /// again) and rebuilds that channel's materialized targets at the
    /// set's size. A no-op (history kept) when no override was recorded.
    /// Otherwise the same history-clear contract as
    /// [`Self::resize_channel`] (the rebuilt contents no longer fit old
    /// snapshots). [`BASE_COLOR_CHANNEL`] is always a no-op (never
    /// overridden).
    pub fn clear_channel_size(&mut self, set: &str, channel: &str) {
        if channel == BASE_COLOR_CHANNEL {
            return;
        }
        let key = (set.to_string(), channel.to_string());
        if self.channel_sizes.remove(&key).is_none() {
            return;
        }
        self.retouch_history.clear();
        self.pending_commits.clear();
        self.stroke_epoch += 1;
        let (width, height) = self.size_of_set(set);
        let tiles = self.tiles_of_set(set);
        for tile in tiles {
            if let Some(tile_targets) = self.targets.get_mut(&tile) {
                if let Some(thread) = tile_targets.channels.get_mut(channel) {
                    let _ = thread.publish(vec![PaintThreadCommand::Resize { width, height }]);
                }
            }
            // The channel's cache (materialized by an earlier read) is
            // stale at the override's size: rebuild it too, else a read
            // after the clear serves the old-size composite forever.
            if self.cache.contains_key(&(tile, channel.to_string())) {
                if let Some(thread) = self.cache.get_mut(&(tile, channel.to_string())) {
                    let _ = thread.publish(vec![PaintThreadCommand::Resize { width, height }]);
                }
                self.cache_dirty
                    .insert((tile, channel.to_string()), DirtyRegion::Full);
            }
        }
    }

    /// The tile an event at `uv` routes to: `tile_of_uv` when the loaded
    /// mesh spans several tiles, else [`FIRST_TILE`].
    fn route_tile(&self, uv: Pos2) -> u16 {
        if self.multi_tile {
            tile_of_uv([uv.x, uv.y])
        } else {
            FIRST_TILE
        }
    }

    /// Ensures `tile` has a paint thread, creating it at the TILE'S SET
    /// size on first use (and recording the active set when the tile has
    /// no assignment yet). Infallible since stage 1b: the thread shares
    /// the session compositor ([`PaintThread::with_compositor`], [M4]), so
    /// no shader compiles here — creation cannot fail. Always `true`.
    fn ensure_tile(&mut self, tile: u16) -> bool {
        if self.targets.contains_key(&tile) {
            return true;
        }
        let (width, height) = self.tile_size(tile);
        let thread = PaintThread::with_compositor(
            self.device.clone(),
            self.queue.clone(),
            width,
            height,
            self.compositor.clone(),
        );
        self.targets.insert(
            tile,
            TileTargets {
                base_color: thread,
                channels: BTreeMap::new(),
            },
        );
        let active = self.active_set.clone();
        self.tile_sets.entry(tile).or_insert(active);
        true
    }

    /// Ensures `tile` has a paint thread for `channel`, creating it at the
    /// CHANNEL's size on first use — its override when one is recorded,
    /// else the tile's set size (the lazy materialization contract, see
    /// the module docs): a channel's thread appears on first route (a
    /// stroke, a bake write, or this explicit pre-warm call).
    /// [`BASE_COLOR_CHANNEL`] defers to [`Self::ensure_tile`] and never
    /// allocates a `channels` entry. Infallible since stage 1b (the shared
    /// compositor — see [`Self::ensure_tile`]). Always `true`.
    pub fn ensure_channel(&mut self, tile: u16, channel: &str) -> bool {
        if channel == BASE_COLOR_CHANNEL {
            return self.ensure_tile(tile);
        }
        if !self.ensure_tile(tile) {
            return false;
        }
        if self
            .targets
            .get(&tile)
            .is_some_and(|t| t.channels.contains_key(channel))
        {
            return true;
        }
        let (width, height) = self.tile_channel_size(tile, channel);
        let thread = PaintThread::with_compositor(
            self.device.clone(),
            self.queue.clone(),
            width,
            height,
            self.compositor.clone(),
        );
        self.targets
            .get_mut(&tile)
            .expect("ensure_tile just created or kept it")
            .channels
            .insert(channel.to_string(), thread);
        true
    }

    /// Resolves `tile`'s thread for `channel`, mutably, WITHOUT
    /// materializing: `None` when the tile or the channel has never been
    /// created. The read accessors' non-creating counterpart to
    /// `target_of_mut` — a read must never side-effect creation.
    fn existing_target_mut(&mut self, tile: u16, channel: &str) -> Option<&mut PaintThread> {
        let targets = self.targets.get_mut(&tile)?;
        if channel == BASE_COLOR_CHANNEL {
            Some(&mut targets.base_color)
        } else {
            targets.channels.get_mut(channel)
        }
    }

    /// Resolves `tile`'s thread for `channel`, mutably — materializing `channel` first (lazy
    /// creation on first route; a non-reserved channel that has never
    /// routed here appears now). `None` only on a creation failure (same
    /// as `ensure_tile`/`ensure_channel`).
    fn target_of_mut(&mut self, tile: u16, channel: &str) -> Option<&mut PaintThread> {
        if !self.ensure_channel(tile, channel) {
            return None;
        }
        let targets = self.targets.get_mut(&tile)?;
        if channel == BASE_COLOR_CHANNEL {
            Some(&mut targets.base_color)
        } else {
            targets.channels.get_mut(channel)
        }
    }

    /// Brush footprint in UV units for `tile`'s set: the mirror gate
    /// distance (`BRUSH_RADIUS_TEXELS / texels_per_uv`, plus
    /// [`SEAM_MIRROR_MARGIN`).
    fn seam_radius_uv_for(&self, tile: u16) -> f32 {
        mirror_radius_uv(self.texels_per_uv_for(tile))
    }

    /// UV-space seam proximity gate: true iff `uv` lies within `radius_uv`
    /// of any seam edge's `uv_a` or `uv_b` segment. `None` graph (no mesh
    /// loaded) is never near a seam.
    fn seam_near_uv(&self, uv: [f32; 2], radius_uv: f32) -> bool {
        self.seam_graph
            .as_ref()
            .is_some_and(|graph| uv_near_seam(graph, uv, radius_uv))
    }

    /// The stroke-event seam (every caller): position, pressure, an optional
    /// conditioner and the adapter both run at
    /// `BRUSH_RADIUS_TEXELS * sanitize_size(radius_scale)`, so a replayed
    /// path's per-point size varies the dab footprint through the SAME
    /// spacing math live strokes use. Scale `1.0` is byte-identical to
    /// brush-radius scale (`1.0` = the live-stroke default).
    fn push_event_styled(
        &mut self,
        uv: Pos2,
        pressure: f32,
        radius_scale: f32,
        expand_seams: bool,
    ) {
        profiling::scope!("stroke_eval");
        // Per-tile routing (UDIM slice 4): an ORIGINAL event picks the
        // active tile; mirrors route by their own UV below but never move
        // the active tile (the view keeps showing the tile under the
        // pointer). Crossing into another tile mid-stroke resets the
        // conditioner: local coordinates jump at the seam, and smoothing
        // or spacing across that jump would streak the new tile (v1:
        // per-event routing, no cross-tile continuity).
        if expand_seams {
            let tile = self.route_tile(uv);
            if !self.ensure_tile(tile) {
                return;
            }
            if tile != self.active_tile {
                self.conditioner.reset();
                self.active_tile = tile;
            }
        }
        // Seam-aware expansion at the UV layer, before texel conversion
        // (design slice 3, item 2): mirror positions become ADDITIONAL
        // `push_event_styled` calls at the mirrored UVs with
        // `expand_seams = false`, so mirrors recurse through the SAME texel
        // conversion + conditioning path (spacing, one-euro, lazy-mouse)
        // without re-expanding (recursion terminates by construction).
        //
        // The gate is two-deep: the stroke-level `stroke_near_seam` arm
        // (set by `begin_stroke`) skips the mirror call for seam-blind
        // strokes, and `mirror_positions` re-gates on UV-segment distance,
        // so far-from-seam events stage no extra batches.
        // Retouch strokes skip the mirror fan-out (v1): a smudge/clone dab
        // reads its own tile's mirror, and a mirrored clone source has no
        // defined counterpart across the seam.
        // Mask strokes do fan out: the mask covers the same 3D neighborhood
        // in both islands, whatever tool is selected.
        if expand_seams
            && self.stroke_near_seam
            && (self.stroke_mask_edit || !self.tool.is_retouch())
        {
            // Each event (and each seam mirror below) gates on its own
            // tile's density: a mirror landing in a different-sized tile
            // recurses with expand_seams = false and re-resolves there.
            let radius_uv = self.seam_radius_uv_for(self.route_tile(uv));
            // Collect the mirrors first to end the graph borrow before the
            // recursive calls re-borrow `self`.
            let mirrors: Vec<[f32; 2]> = self
                .seam_graph
                .as_ref()
                .map(|graph| {
                    umber_mesh::seam_mirror::mirror_positions(&[[uv.x, uv.y]], graph, radius_uv)
                })
                .unwrap_or_default()
                .into_iter()
                .map(|mapping| mapping.mirrored)
                .collect();
            for mirrored in mirrors {
                self.push_event_styled(
                    Pos2::new(mirrored[0], mirrored[1]),
                    pressure,
                    radius_scale,
                    false,
                );
            }
        }
        // Mirrors across a tile seam land in the other tile: route by the
        // mirrored UV's own tile.
        let tile = if expand_seams {
            self.active_tile
        } else {
            let tile = self.route_tile(uv);
            if !self.ensure_tile(tile) {
                return;
            }
            tile
        };
        let texel = local_texel([uv.x, uv.y], tile, self.texels_per_uv_for(tile));
        // Synthetic event clock (see EVENT_STEP_NS): every event — original
        // or mirror — advances time so the conditioner actually progresses.
        self.event_seq += 1;
        let event = StrokeEvent {
            pos: texel,
            pressure,
            tilt: None,
            time_ns: self.event_seq * EVENT_STEP_NS,
            in_proximity: true,
            contact: true,
        };
        let stamps = self
            .conditioner
            .push(&event, BRUSH_RADIUS_TEXELS * sanitize_size(radius_scale));
        if stamps.is_empty() {
            return;
        }
        let radius = BRUSH_RADIUS_TEXELS * sanitize_size(radius_scale);
        let dabs = DabAdapter::stamps_to_dabs(&stamps, radius, &self.brush);
        if dabs.is_empty() {
            return;
        }
        // The layer-mask hook: a mask-edit stroke stamps the SELECTED
        // layer's mask plane with the same dabs (symmetry siblings
        // included) and stages nothing. It runs before the quick-mask
        // hook, so selecting a layer mask wins over quick-mask mode —
        // one stroke paints exactly one plane.
        if let Some(layer) = self.stroke_mask_layer {
            let dabs = match (&self.stroke_symmetry, expand_seams) {
                (Some(transforms), true) => expand_dabs(&dabs, transforms),
                _ => dabs,
            };
            self.layer_mask_event(layer, tile, &dabs);
            return;
        }
        // The quick-mask edit hook: a mask-mode stroke stamps the MASK
        // with the same dabs a paint stroke would stage (symmetry siblings
        // included) and stages nothing. It runs before the retouch hook, so
        // mask mode wins over a selected smudge/clone tool.
        if self.stroke_mask_edit {
            let dabs = match (&self.stroke_symmetry, expand_seams) {
                (Some(transforms), true) => expand_dabs(&dabs, transforms),
                _ => dabs,
            };
            self.mask_event(tile, &dabs);
            return;
        }
        // The retouch hook: smudge/clone/eraser dabs (same positions,
        // radius, pressure-shaped alpha and hardness as paint dabs) run on
        // the CPU mirror instead of the GPU splat. Only the primaries go
        // in: the retouch stroke stamps their symmetry siblings itself (it
        // owns the per-sibling smudge chains and clone sources; the
        // motion-independent eraser needs no per-sibling state).
        if self.tool.is_retouch() {
            let retouch_dabs: Vec<RetouchDab> = dabs
                .iter()
                .map(|d| RetouchDab {
                    pos: d.pos,
                    radius: d.radius,
                    alpha: d.alpha,
                    hardness: d.hardness,
                })
                .collect();
            self.retouch_event(tile, &retouch_dabs);
            return;
        }
        // Wash mode (MyPaint opaque_linearize, CPU-side pre-splat): the
        // stroke's dabs are scaled so their overlap accumulates to the
        // stroke opacity instead of past it. This lands AFTER the retouch
        // hook, so smudge/clone/eraser and mask-mode strokes keep their
        // own (build-up) dab alphas untouched — and BEFORE symmetry
        // expansion, so siblings inherit the normalized alpha. No new GPU
        // surface and no shader arm: the scale is one scalar per event
        // batch, derived from the live conditioner density (the spacing
        // that actually produced these stamps).
        let dabs = apply_wash_mode(
            dabs,
            self.brush.alpha,
            self.wash_mode,
            self.conditioner.dabs_per_radius(),
        );
        // The symmetry hook: every original event's dab is followed by its
        // siblings (same stamp, transformed position) in the same batch.
        // Seam mirrors (`expand_seams == false`) are not symmetrized (v1).
        let dabs = match (&self.stroke_symmetry, expand_seams) {
            (Some(transforms), true) => expand_dabs(&dabs, transforms),
            _ => dabs,
        };
        // The quick-mask gate, AFTER symmetry expansion: every staged dab
        // (primary, sibling, seam mirror) carries the shader's gate flag,
        // and the shader multiplies its per-texel coverage by this tile's
        // mask texture, which is synced first so it holds the current
        // plane.
        let dabs = if self.stroke_mask_gate {
            let owner = self.stroke_owner;
            let channel = self.stroke_channel.clone();
            self.sync_mask_tile(owner, tile, &channel);
            dabs.into_iter().map(|d| d.with_mask_gate(true)).collect()
        } else {
            dabs
        };
        // The paint-stroke undo capture (see the module docs): the staged
        // dabs' footprints union into the stroke's dirty rect, snapshotting
        // the tile's before-bytes on first touch. Runs on the FINAL dabs
        // (post symmetry expansion and mask gating) so siblings are
        // covered; seam mirrors recurse through this same staging for their
        // own tiles.
        if self.paint_capture.is_some() {
            self.capture_paint_event(tile, &dabs);
        }
        // Stage as one command; the thread splits into capacity segments
        // and overlap-safe dispatches. Routes to the stroke's snapshotted
        // (owner, channel) — §3's routing rule: every event lands in
        // `stroke_owner`'s plane on `route_tile(uv)`'s tile, in the
        // stroke's channel. A first batch on a new `(owner, tile)`
        // materializes through `materialize_plane` and may be refused
        // (§1.2): the refused event's dabs on that tile are dropped and
        // the stroke continues on its already-materialized planes.
        #[cfg(any(test, feature = "test-hooks"))]
        let staged_n = dabs.len() as u64;
        let channel = self.stroke_channel.clone();
        let owner = self.stroke_owner;
        // The dirt footprint unions every staged dab's shader-identical
        // box (§4.2: each dab footprint, merged into the multi-rect).
        let (width, height) = self.tile_channel_size(tile, &channel);
        let mut staged_rect = None;
        for dab in &dabs {
            if let Some(rect) = footprint(dab.pos, dab.radius, width, height) {
                staged_rect = DirtyRect::union_opt(staged_rect, Some(rect));
            }
        }
        match self.plane_of_mut(owner, tile, &channel) {
            Ok(Some(thread)) => {
                let _ = thread.publish(vec![PaintThreadCommand::Stage { dabs }]);
                self.clear_committed(owner, tile, &channel);
                if let Some(rect) = staged_rect {
                    self.mark_dirty_rect(tile, &channel, rect);
                }
            }
            Ok(None) => {}
            Err(refusal) => {
                self.stroke_refusal = Some(refusal);
            }
        }
        // Test observability: count staged batches for the seam-blind
        // zero-cost contract (a publish here means one more `Stage`
        // command queued, whether or not the channel accepted it — the
        // channel is unreachable-closed in practice).
        #[cfg(test)]
        {
            self.staged_batches += 1;
            *self.staged_batches_per_tile.entry(tile).or_default() += 1;
        }
        // Soak/symmetry observability: total staged dabs.
        #[cfg(any(test, feature = "test-hooks"))]
        {
            self.staged_dabs += staged_n;
        }
    }

    /// One event's staged paint dabs for the stroke's undo capture: the
    /// tile's before-bytes are snapshotted on the stroke's first staged
    /// batch there (drained first, so earlier strokes are in the bytes),
    /// then every dab's shader-identical footprint unions into the dirty
    /// rect. A tile whose readback fails records nothing — the stroke still
    /// paints there, but that tile has no before to restore (a dropped
    /// readback must never drop a stamp).
    fn capture_paint_event(&mut self, tile: u16, dabs: &[Dab]) {
        if self.paint_capture.is_none() {
            return;
        }
        let owner = self.stroke_owner;
        let needs_snapshot = self
            .paint_capture
            .as_ref()
            .is_some_and(|capture| !capture.tiles.contains_key(&(owner, tile)));
        if needs_snapshot {
            // Materializing: the FIRST stroke ever routed to this plane
            // has no existing thread to read — its just-created blank
            // bytes are the correct before-state (the same contract
            // `ensure_tile` already gives a brand-new tile). A budget
            // refusal snapshots nothing (the stage drops the dabs too —
            // nothing painted, nothing to capture).
            let channel = self.stroke_channel.clone();
            let image = match self.materialize_plane(owner, tile, &channel) {
                Ok(_) => self.read_plane(owner, tile, &channel),
                Err(refusal) => {
                    self.stroke_refusal = Some(refusal);
                    None
                }
            };
            let (Some(capture), Some(image)) = (self.paint_capture.as_mut(), image) else {
                return;
            };
            capture.tiles.insert(
                (owner, tile),
                PaintTileCapture {
                    before: image,
                    dirty: None,
                },
            );
        }
        let Some(capture) = self.paint_capture.as_mut() else {
            return;
        };
        let Some(entry) = capture.tiles.get_mut(&(owner, tile)) else {
            return;
        };
        // The snapshot's own dims (not the session's): they are the target
        // the rect must fit at `end_stroke`.
        let (width, height) = (entry.before.width, entry.before.height);
        for dab in dabs {
            if let Some(rect) = footprint(dab.pos, dab.radius, width, height) {
                entry.dirty = DirtyRect::union_opt(entry.dirty, Some(rect));
            }
        }
    }

    /// One event's retouch dabs on `tile`: mirror the stroke owner's plane
    /// on the stroke's first touch, run the dabs on the mirror, and queue
    /// the event's dirty rect back onto the plane.
    ///
    /// Two reads (§3 [M3]): the CPU MIRROR reads from and writes back to
    /// `(stroke_owner, tile, stroke_channel)` (a PLANE reader — this lifts
    /// the baseColor-only boundary), while smudge and clone SAMPLE the
    /// COMPOSITE (cache-else-Base, flush-first — one extra full-tile
    /// readback per stroke per tile, beside the mirror read). The eraser
    /// needs no sample. A retouch write into a layer-owned plane keeps
    /// `p ≤ a` (the premultiplied erase + the clamp the composite
    /// applies, so no composited byte changes).
    fn retouch_event(&mut self, tile: u16, dabs: &[RetouchDab]) {
        // Taken for the call: the mirror read needs `&mut self`.
        let Some(mut stroke) = self.retouch.take() else {
            return;
        };
        let owner = self.stroke_owner;
        let channel = self.stroke_channel.clone();
        stroke.set_plane(owner, channel.clone());
        if stroke.accepts(tile) && !stroke.has_tile(tile) {
            // Materialize-then-read: the first touch of a new plane
            // mirrors its just-created blank bytes (the capture's
            // contract). A refusal drops the event (nothing to mirror,
            // nothing to write back).
            let image = match self.materialize_plane(owner, tile, &channel) {
                Ok(_) => self.read_plane(owner, tile, &channel),
                Err(refusal) => {
                    self.stroke_refusal = Some(refusal);
                    None
                }
            };
            if let Some(image) = image {
                stroke.insert_tile(tile, image);
                // Smudge and clone sample the visible canvas: install the
                // flushed composite beside the mirror (the eraser's writes
                // derive from the mirror alone).
                if !matches!(self.tool, BrushTool::Erase(_)) {
                    // The composite sample reads the cache when a layer
                    // plane exists for `(tile, channel)`, else Base
                    // directly — the lazy-cache rule, flushed first.
                    if let Some(sample) = self.read_composite(tile, &channel) {
                        stroke.insert_sample(tile, sample);
                    }
                }
            }
        }
        // The quick-mask gate: the retouch ops multiply their per-texel
        // coverage by the tile's set's CPU plane (allocated empty if
        // untouched).
        let mask = if self.stroke_mask_gate {
            self.mask_of_tile_mut(tile).ensure_plane(tile);
            self.quick_mask_for_tile(tile).view(tile)
        } else {
            None
        };
        if let Some(rect) = stroke.apply_masked(tile, dabs, mask) {
            let rgba = stroke.region(tile, rect);
            match self.plane_of_mut(owner, tile, &channel) {
                Ok(Some(thread)) => {
                    let _ = thread.publish(vec![PaintThreadCommand::WriteRegion {
                        x: rect.x0,
                        y: rect.y0,
                        width: rect.width(),
                        height: rect.height(),
                        rgba,
                    }]);
                    self.clear_committed(owner, tile, &channel);
                    self.mark_dirty_rect(tile, &channel, rect);
                }
                Ok(None) => {}
                Err(refusal) => {
                    self.stroke_refusal = Some(refusal);
                }
            }
        }
        self.retouch = Some(stroke);
    }

    /// One mask-mode event on `tile`: stamps every dab into the quick mask
    /// and queues the touched rect onto the GPU mask (the whole plane if
    /// that copy is stale). The upload rides the Base thread (today's
    /// exact path — a mask-mode stroke stages no plane); every other
    /// plane's copy for the tile is invalidated, so the next gated dab
    /// re-syncs its own plane's copy before staging (§3).
    fn mask_event(&mut self, tile: u16, dabs: &[Dab]) {
        let mut dirty = None;
        for dab in dabs {
            let stamp = RetouchDab {
                pos: dab.pos,
                radius: dab.radius,
                alpha: dab.alpha,
                hardness: dab.hardness,
            };
            dirty = DirtyRect::union_opt(dirty, self.mask_of_tile_mut(tile).stamp(tile, &stamp));
        }
        if dirty.is_some() {
            self.mask_synced.retain(|(_, t, _)| *t != tile);
        }
        if let Some(rect) = dirty {
            if self
                .mask_synced
                .contains(&(PlaneOwner::Base, tile, BASE_COLOR_CHANNEL.to_string()))
            {
                self.publish_mask(PlaneOwner::Base, tile, BASE_COLOR_CHANNEL, rect);
            } else {
                self.sync_mask_tile(PlaneOwner::Base, tile, BASE_COLOR_CHANNEL);
            }
        }
    }

    /// Brings `(owner, tile, channel)`'s GPU mask in line with the CPU
    /// plane before gated dabs read it: a plane never uploaded (or marked
    /// stale by a mask stroke, an invert/clear/resize) gets its whole
    /// plane. A no-op when in sync.
    fn sync_mask_tile(&mut self, owner: PlaneOwner, tile: u16, channel: &str) {
        if self
            .mask_synced
            .contains(&(owner, tile, channel.to_string()))
        {
            return;
        }
        let (width, height) = self.quick_mask_for_tile(tile).dimensions();
        let full = DirtyRect {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        };
        self.publish_mask(owner, tile, channel, full);
    }

    /// Queues `rect` of the quick mask (8-bit) as a `WriteMask` on
    /// `(owner, tile, channel)`'s thread, in FIFO order with its `Stage`
    /// commands — the GPU mask copy rides the thread the dab stages into
    /// (§3). A refused materialization skips the upload (the stage drops
    /// its dabs too — nothing paints ungated).
    fn publish_mask(&mut self, owner: PlaneOwner, tile: u16, channel: &str, rect: DirtyRect) {
        if rect.width() == 0 || rect.height() == 0 {
            return;
        }
        let values = self.quick_mask_for_tile(tile).gpu_bytes(tile, rect);
        // The quick mask is per-SET, not per-channel: each plane holds its
        // OWN GPU copy of its set's plane (this closes the "non-baseColor
        // channel's OWN GPU mask copy" follow-up).
        match self.plane_of_mut(owner, tile, channel) {
            Ok(Some(thread)) => {
                let _ = thread.publish(vec![PaintThreadCommand::WriteMask {
                    x: rect.x0,
                    y: rect.y0,
                    width: rect.width(),
                    height: rect.height(),
                    values,
                }]);
            }
            Ok(None) => return,
            Err(err) => {
                log::warn!("mask: upload for {owner:?}/{tile}/{channel} refused ({err:?})");
                return;
            }
        }
        self.mask_synced.insert((owner, tile, channel.to_string()));
        #[cfg(test)]
        {
            self.mask_uploads += 1;
        }
    }

    /// `tile`'s baseColor COMPOSITE texels as a CPU image (flush-first —
    /// §1.1 [B3]): drains the tile's queue first (so earlier strokes'
    /// dabs are in the texture), composites pending dirt against the last
    /// snapshot, then the blocking readback. `None` (logged; the stroke
    /// skips the tile) when the drain or the readback fails.
    /// Channel-aware reads use [`Self::read_channel_tile`] instead — this
    /// stays baseColor-only (the eyedropper/ID-mask pattern: explicit
    /// channel, no ambient active-channel state).
    pub fn read_tile(&mut self, tile: u16) -> Option<RgbaBuffer> {
        self.read_composite(tile, BASE_COLOR_CHANNEL)
    }

    /// [`Self::read_tile`]'s channel-aware sibling: `tile`'s `channel`
    /// COMPOSITE texels (flush-first). `None` when that channel has never
    /// routed on `tile` — like `read_tile` on a never-created tile, a
    /// read never side-effects creation (the eyedropper/ID-mask pattern:
    /// explicit channel, no ambient active-channel state, and no surprise
    /// allocation either).
    pub fn read_channel_tile(&mut self, tile: u16, channel: &str) -> Option<RgbaBuffer> {
        self.read_composite(tile, channel)
    }

    /// The shared implementation behind the Base-plane reads
    /// ([`Self::read_plane`]'s Base arm and [`Self::read_composite`]'s
    /// cache-miss fallback): drains `tile`'s `channel` target's queue,
    /// then the blocking full-target readback. Never materializes (a
    /// read must never side-effect creation).
    fn read_target(&mut self, tile: u16, channel: &str) -> Option<RgbaBuffer> {
        // Clone the Arc handles out before the mutable target borrow —
        // `thread` borrows `self.targets` while the read-back wants
        // `self.device`/`self.queue` immutably (E0502 otherwise).
        let device = self.device.clone();
        let queue = self.queue.clone();
        let thread = self.existing_target_mut(tile, channel)?;
        if let Err(err) = thread.process_pending() {
            log::warn!("retouch: draining tile {tile}/{channel} failed: {err:#}");
            return None;
        }
        let target = thread.paint_target();
        let (width, height) = target.dimensions();
        match target.read_back_rgba8(&device, &queue) {
            Ok(bytes) => RgbaBuffer::new(width, height, bytes),
            Err(err) => {
                log::warn!("retouch: reading tile {tile}/{channel} back failed: {err:#}");
                None
            }
        }
    }

    /// Staged `Stage`-batch count since construction (test observability;
    /// see the seam-blind zero-cost contract in the wiring tests below).
    #[cfg(test)]
    pub(crate) fn staged_batch_count(&self) -> usize {
        self.staged_batches
    }

    /// Staged `Stage`-batch count routed to `tile` since construction
    /// (test observability for the routing contract).
    #[cfg(test)]
    pub(crate) fn staged_batch_count_for(&self, tile: u16) -> usize {
        self.staged_batches_per_tile
            .get(&tile)
            .copied()
            .unwrap_or(0)
    }

    /// Number of tile targets created so far (test observability for lazy
    /// creation).
    #[cfg(test)]
    pub(crate) fn tile_count(&self) -> usize {
        self.targets.len()
    }

    /// Staged dabs since construction (test observability; see the
    /// stroke soak's zero-drop assert in umber-app's `perf_soak` and the
    /// symmetry staging test). Behind `test-hooks` for the cross-crate
    /// callers (a dependency's `cfg(test)` is never set for its users).
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn staged_dab_count(&self) -> u64 {
        self.staged_dabs
    }

    /// Queued `WriteMask` commands since construction (test
    /// observability; see the quick-mask tests).
    #[cfg(test)]
    pub(crate) fn mask_upload_count(&self) -> usize {
        self.mask_uploads
    }
}

/// One thread's backing-store bytes (§1.3's accounting): `w·h·4` for the
/// RGBA8 plane, plus `w·h` when it holds a GPU quick-mask copy (a
/// full-size `R8` texture — see `PaintTarget`'s mask docs).
fn thread_bytes(thread: &PaintThread) -> u64 {
    let (width, height) = thread.paint_target().dimensions();
    let plane = width as u64 * height as u64 * 4;
    if thread.paint_target().has_mask() {
        plane + width as u64 * height as u64
    } else {
        plane
    }
}

/// Tile-local texel coordinates of `uv` inside `tile` on a target with
/// `texels_per_uv` texels per UV unit: the tile's integer UV offsets
/// (`(tile - 1001) % 10`, `(tile - 1001) / 10`) are subtracted before the
/// usual V-flipped texel mapping. For tile 1001 the offsets are zero and
/// the result is bit-identical to the pre-UDIM `[u * tpv, (1 - v) * tpv]`.
fn local_texel(uv: [f32; 2], tile: u16, texels_per_uv: f32) -> [f32; 2] {
    let index = tile.saturating_sub(FIRST_TILE);
    let tile_u = f32::from(index % 10);
    let tile_v = f32::from(index / 10);
    [
        (uv[0] - tile_u) * texels_per_uv,
        (1.0 - (uv[1] - tile_v)) * texels_per_uv,
    ]
}

/// A UV-space clone offset in tile-local texels: scaled by the texel
/// density, V negated (texel rows grow downward — [`local_texel`]'s flip).
fn clone_offset_texels(offset_uv: (f32, f32), texels_per_uv: f32) -> [f32; 2] {
    [offset_uv.0 * texels_per_uv, -offset_uv.1 * texels_per_uv]
}

/// A tile's integer UV offsets (the inverse of [`local_texel`]'s
/// subtraction): `(tile - 1001) % 10` in U, `(tile - 1001) / 10` in V.
fn tile_uv_offset(tile: u16) -> (f32, f32) {
    let index = tile.saturating_sub(FIRST_TILE);
    (f32::from(index % 10), f32::from(index / 10))
}

/// Global UV of a tile-local texel CENTER (the exact inverse of
/// [`local_texel`], so a filled path's scan rows land on the texels the
/// rasterizer covered).
fn texel_center_uv(tile: u16, x: u32, y: u32, texels_per_uv: f32) -> [f32; 2] {
    let (tile_u, tile_v) = tile_uv_offset(tile);
    [
        tile_u + (x as f32 + 0.5) / texels_per_uv,
        tile_v + 1.0 - (y as f32 + 0.5) / texels_per_uv,
    ]
}

/// One maximal x-contiguous covered span in a tile mask row: the filled
/// path replays each as one stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FillRun {
    y: u32,
    x0: u32,
    x1: u32,
}

/// Row runs of the covered texels in `mask` (row-major, `width`x`height`):
/// maximal x-contiguous `true` spans, rows top-down. A short `mask`
/// degrades to no runs (never a panic — a torn rasterizer output must not
/// crash the fill).
fn fill_runs(mask: &[bool], width: usize, height: usize) -> Vec<FillRun> {
    let mut runs = Vec::new();
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let covered = mask.get(y * width + x).copied().unwrap_or(false);
            if !covered {
                x += 1;
                continue;
            }
            let x0 = x;
            x += 1;
            while x < width && mask.get(y * width + x).copied().unwrap_or(false) {
                x += 1;
            }
            runs.push(FillRun {
                y: y as u32,
                x0: x0 as u32,
                x1: (x - 1) as u32,
            });
        }
    }
    runs
}

/// Brush footprint in UV units for a target with `texels_per_uv` texels
/// per UV unit: `BRUSH_RADIUS_TEXELS * SEAM_MIRROR_MARGIN / texels_per_uv`.
/// Free function so the radius math is unit-testable without a GPU-backed
/// [`PaintState`].
fn mirror_radius_uv(texels_per_uv: f32) -> f32 {
    (BRUSH_RADIUS_TEXELS * SEAM_MIRROR_MARGIN) / texels_per_uv
}

/// Paints one dab into a layer-mask `u8` plane: every covered texel moves
/// toward `target` by the dab's coverage (`alpha * falloff` — the same
/// [`coverage`] the quick mask stamps through), so a hard full-alpha dab
/// sets its disc exactly. Returns the rect it touched, or `None` when the
/// dab covers nothing.
///
/// Free function (like [`mirror_radius_uv`]) so the arithmetic is
/// unit-testable without a GPU-backed [`PaintState`]: the `u8` rounding
/// (`round`, half away from zero) is pinned by the tests below — a
/// truncating cast would bias every soft mask edge shut by half an LSB.
pub(crate) fn stamp_grayscale_mask(
    plane: &mut [u8],
    width: u32,
    height: u32,
    dab: &RetouchDab,
    target: u8,
) -> Option<DirtyRect> {
    if plane.len() != width as usize * height as usize {
        return None;
    }
    let rect = footprint(dab.pos, dab.radius, width, height)?;
    let stride = width as usize;
    let target = f32::from(target);
    let mut wrote = false;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let Some(cov) = coverage(dab, x, y) else {
                continue;
            };
            let cov = cov.clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let m = &mut plane[y as usize * stride + x as usize];
            let next = f32::from(*m) + (target - f32::from(*m)) * cov;
            *m = next.round().clamp(0.0, 255.0) as u8;
            wrote = true;
        }
    }
    wrote.then_some(rect)
}

/// The wash-mode hook in `push_event_styled`, factored out so it is
/// unit-testable without a GPU-backed [`PaintState`]: build-up dabs pass
/// through untouched; wash dabs have every alpha scaled by
/// `wash_scale(stroke_alpha, overlap)` so their overlap accumulates to
/// the stroke opacity. Positions, radii, colors, hardness and mask gates
/// are never touched — only the flow multiplier.
fn apply_wash_mode(dabs: Vec<Dab>, stroke_alpha: f32, mode: WashMode, overlap: f32) -> Vec<Dab> {
    if !matches!(mode, WashMode::Wash) {
        return dabs;
    }
    let scale = wash_scale(stroke_alpha, overlap);
    dabs.into_iter()
        .map(|mut d| {
            d.alpha *= scale;
            d
        })
        .collect()
}

/// UV-space seam proximity: true iff `uv` lies within `radius_uv` of any
/// seam edge's `uv_a` segment OR `uv_b` segment (segment distance, the same
/// gate math as `seam_mirror`'s internal check). Free function so the gate
/// is unit-testable without a GPU-backed [`PaintState`]; the
/// [`PaintState::seam_near_uv`] method wraps it with the stored graph.
fn uv_near_seam(graph: &SeamGraph, uv: [f32; 2], radius_uv: f32) -> bool {
    graph.edges.iter().any(|edge| {
        dist_to_segment(uv, edge.uv_a[0], edge.uv_a[1]) <= radius_uv
            || dist_to_segment(uv, edge.uv_b[0], edge.uv_b[1]) <= radius_uv
    })
}

/// Euclidean distance from `p` to the segment `a--b` (not the infinite
/// line; extrapolation past the endpoints counts only via endpoint
/// closeness). Zero-length segments degrade to distance-to-`a`, never NaN.
fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len_sq = ab[0] * ab[0] + ab[1] * ab[1];
    if len_sq == 0.0 {
        let dx = p[0] - a[0];
        let dy = p[1] - a[1];
        return (dx * dx + dy * dy).sqrt();
    }
    let s = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len_sq;
    let closest = if s <= 0.0 {
        a
    } else if s >= 1.0 {
        b
    } else {
        [a[0] + ab[0] * s, a[1] + ab[1] * s]
    };
    let dx = p[0] - closest[0];
    let dy = p[1] - closest[1];
    (dx * dx + dy * dy).sqrt()
}

/// Seam-free 2x1 strip with continuous UVs spanning [0,2]x[0,1]
/// (position == UV): the left quad's triangles start at u=0 (tile 1001),
/// the right quad's at u=1 (tile 1002). Shared vertices, so no seam
/// edges — tile routing is the only thing under test. Shared by the
/// routing tests here and the per-tile export test in `bake_sources`.
#[cfg(any(test, feature = "test-hooks"))]
pub fn two_tile_strip() -> umber_mesh::MeshData {
    let uvs = vec![
        [0.0, 0.0], // 0
        [1.0, 0.0], // 1
        [2.0, 0.0], // 2
        [0.0, 1.0], // 3
        [1.0, 1.0], // 4
        [2.0, 1.0], // 5
    ];
    umber_mesh::MeshData {
        positions: uvs
            .iter()
            .map(|uv: &[f32; 2]| [uv[0], uv[1], 0.0])
            .collect(),
        normals: vec![[0.0, 0.0, 1.0]; 6],
        uvs,
        indices: vec![0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4],
        material_names: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_mesh::seam::build_seam_graph;
    use umber_mesh::seam_mirror::mirror_positions;

    #[test]
    fn clone_offset_texels_matches_local_texel_deltas() {
        // The tracker's texel offset must land exactly where local_texel
        // maps the alt-clicked source (V flip included).
        let start = [0.25, 0.75];
        let source = [0.5, 0.25];
        let offset_uv = (source[0] - start[0], source[1] - start[1]);
        let a = local_texel(start, FIRST_TILE, 512.0);
        let b = local_texel(source, FIRST_TILE, 512.0);
        let off = clone_offset_texels(offset_uv, 512.0);
        assert_eq!([a[0] + off[0], a[1] + off[1]], b);
    }

    #[test]
    fn brush_size_scales_the_live_radius_and_clamps() {
        // PaintState needs a device; GPU-less hosts skip (the house rule).
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut s = PaintState::with_target_size(device, queue, 64).expect("paint state builds");
        assert_eq!(s.brush_size(), 1.0);
        assert_eq!(s.brush_radius_texels(), BRUSH_RADIUS_TEXELS);
        s.set_brush_size(2.0);
        assert_eq!(s.brush_radius_texels(), BRUSH_RADIUS_TEXELS * 2.0);
        s.set_brush_size(1000.0);
        assert_eq!(s.brush_size(), umber_brush::path::PATH_MAX_SIZE);
        s.set_brush_size(0.0);
        assert_eq!(s.brush_size(), umber_brush::path::PATH_MIN_SIZE);
        s.set_brush_size(f32::NAN);
        assert_eq!(s.brush_size(), 1.0);
        s.set_brush_hardness(2.0);
        assert_eq!(s.brush_hardness(), 1.0);
        s.set_brush_hardness(f32::NAN);
        assert_eq!(s.brush_hardness(), 1.0);
        s.set_brush_hardness(0.25);
        assert_eq!(s.brush_hardness(), 0.25);
    }

    #[test]
    fn default_brush_params_are_sane() {
        let p = BrushParams::default();
        assert!(p.alpha > 0.0 && p.alpha <= 1.0);
        assert!(p.hardness >= 0.0 && p.hardness <= 1.0);
    }

    #[test]
    fn wash_hook_passes_build_up_through_untouched() {
        let dabs = vec![
            Dab::new([1.0, 2.0], 24.0, [1.0, 0.0, 0.0, 1.0], 0.4, 0.5),
            Dab::new([3.0, 4.0], 24.0, [1.0, 0.0, 0.0, 1.0], 0.7, 0.5),
        ];
        let back = apply_wash_mode(dabs.clone(), 0.8, WashMode::BuildUp, 5.0);
        assert_eq!(back.len(), 2);
        for (got, want) in back.iter().zip(dabs.iter()) {
            assert!((got.alpha - want.alpha).abs() < 1e-7);
            assert_eq!(got.pos, want.pos);
            assert_eq!(got.color, want.color);
        }
    }

    #[test]
    fn wash_hook_normalizes_to_stroke_opacity() {
        // Stroke opacity 0.5, overlap 5 (the live conditioner density):
        // every dab alpha scales by wash_scale, so five overlapping dabs
        // fold to 0.5 — while the same dabs unscaled fold past it.
        let dabs = vec![Dab::new([8.0, 8.0], 24.0, [1.0, 0.0, 0.0, 1.0], 0.5, 1.0); 5];
        let washed = apply_wash_mode(dabs.clone(), 0.5, WashMode::Wash, 5.0);
        let scale = wash_scale(0.5, 5.0);
        for dab in &washed {
            assert!((dab.alpha - 0.5 * scale).abs() < 1e-7);
        }
        let folded: f32 = 1.0 - (1.0 - washed[0].alpha).powi(5);
        assert!(
            (folded - 0.5).abs() < 1e-5,
            "five wash dabs must fold to the stroke opacity, got {folded}"
        );
        let buildup: f32 = 1.0 - (1.0f32 - 0.5f32).powi(5);
        assert!(
            (buildup - 0.96875).abs() < 1e-6 && (buildup - folded).abs() > 0.4,
            "build-up must accumulate well past the wash ceiling"
        );
    }

    // --- Per-layer pool storage (stage 1b, §8 T14/T33) ---
    //
    // T14 pins lazy materialization + accounting; T33 pins the budget
    // refusal. Both need a device, so they skip gracefully where no wgpu
    // adapter exists (the `try_request_device` pattern). The `DirtyRegion`
    // shape tests are CPU-only.

    #[test]
    fn plane_budget_constants_are_pinned() {
        // §1.3: advisory 2 GiB (32 planes at 4K), enforced 4 GiB (2×).
        assert_eq!(PLANE_BUDGET_BYTES, 2 * 1024 * 1024 * 1024);
        assert_eq!(PLANE_HARD_CAP_BYTES, 4 * 1024 * 1024 * 1024);
    }

    #[test]
    fn dirty_region_unions_touching_rects() {
        // Edge-adjacent on x unions into one rect …
        let mut dirt = DirtyRegion::Rects(vec![DirtyRect {
            x0: 0,
            y0: 0,
            x1: 10,
            y1: 10,
        }]);
        dirt.mark_rect(DirtyRect {
            x0: 10,
            y0: 0,
            x1: 20,
            y1: 10,
        });
        assert_eq!(
            dirt,
            DirtyRegion::Rects(vec![DirtyRect {
                x0: 0,
                y0: 0,
                x1: 20,
                y1: 10
            }])
        );
        // … and so does overlap.
        let mut dirt = DirtyRegion::Rects(vec![DirtyRect {
            x0: 0,
            y0: 0,
            x1: 10,
            y1: 10,
        }]);
        dirt.mark_rect(DirtyRect {
            x0: 5,
            y0: 5,
            x1: 15,
            y1: 15,
        });
        assert_eq!(
            dirt,
            DirtyRegion::Rects(vec![DirtyRect {
                x0: 0,
                y0: 0,
                x1: 15,
                y1: 15
            }])
        );
    }

    #[test]
    fn dirty_region_keeps_disjoint_rects() {
        let mut dirt = DirtyRegion::Rects(vec![DirtyRect {
            x0: 0,
            y0: 0,
            x1: 10,
            y1: 10,
        }]);
        dirt.mark_rect(DirtyRect {
            x0: 100,
            y0: 0,
            x1: 110,
            y1: 10,
        });
        assert!(!dirt.is_full());
        let DirtyRegion::Rects(rects) = &dirt else {
            panic!("disjoint rects must stay Rects");
        };
        assert_eq!(rects.len(), 2);
    }

    #[test]
    fn dirty_region_merges_closest_pair_past_eight() {
        // Eight 10×10 rects at a 10-texel pitch plus one far rect: the 9th
        // disjoint mark must merge ONE adjacent pair (union adds 100
        // texels: 30×10 − 100 − 100), never anything involving the far
        // rect. Ties break to the first minimal pair (strict `<`), so the
        // merged rect is exactly (0,0)-(30,10).
        let mut dirt = DirtyRegion::Rects(Vec::new());
        for i in 0..8u32 {
            dirt.mark_rect(DirtyRect {
                x0: i * 20,
                y0: 0,
                x1: i * 20 + 10,
                y1: 10,
            });
        }
        dirt.mark_rect(DirtyRect {
            x0: 1000,
            y0: 0,
            x1: 1010,
            y1: 10,
        });
        let DirtyRegion::Rects(rects) = &dirt else {
            panic!("nine disjoint rects must stay Rects, merged to eight");
        };
        assert_eq!(rects.len(), 8);
        assert!(rects.contains(&DirtyRect {
            x0: 0,
            y0: 0,
            x1: 30,
            y1: 10
        }));
        assert!(rects.contains(&DirtyRect {
            x0: 1000,
            y0: 0,
            x1: 1010,
            y1: 10
        }));
    }

    #[test]
    fn dirty_region_full_absorbs_rects() {
        let mut dirt = DirtyRegion::Full;
        dirt.mark_rect(DirtyRect {
            x0: 0,
            y0: 0,
            x1: 10,
            y1: 10,
        });
        assert_eq!(dirt, DirtyRegion::Full);
        let mut dirt = DirtyRegion::Rects(vec![DirtyRect {
            x0: 0,
            y0: 0,
            x1: 10,
            y1: 10,
        }]);
        dirt.mark_full();
        assert_eq!(dirt, DirtyRegion::Full);
    }

    #[test]
    fn lazy_planes_and_accounting() {
        // §8 T14. A fresh 512² session holds exactly Base's 1001 baseColor
        // plane: 512·512·4 = 1,048,576.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        assert_eq!(state.plane_bytes(), 1_048_576);
        assert!(state.cache.is_empty());
        // A layer never published holds 0 planes …
        assert!(state
            .read_plane(PlaneOwner::Layer(2), 1001, BASE_COLOR_CHANNEL)
            .is_none());
        // … while publishing one layer plane adds exactly that plane plus
        // its (tile, channel) cache target: 3 · 1,048,576 = 3,145,728.
        let bytes = vec![42u8; 512 * 512 * 4];
        state
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                bytes.clone(),
            )
            .expect("first layer plane fits the default cap");
        assert_eq!(state.plane_bytes(), 3_145_728);
        assert_eq!(state.cache.len(), 1);
        assert!(state
            .cache
            .contains_key(&(1001, BASE_COLOR_CHANNEL.to_string())));
        // The cache is born Full-dirty, and a flush with nothing
        // displayed leaves the dirt (and the bytes) untouched ([M8]'s
        // coalescing: a non-displayed `Full` waits for a read or for its
        // turn on screen).
        assert_eq!(
            state
                .cache_dirty
                .get(&(1001, BASE_COLOR_CHANNEL.to_string())),
            Some(&DirtyRegion::Full)
        );
        state.flush_composite(&CompositeSnapshot::default());
        assert_eq!(state.plane_bytes(), 3_145_728);
        assert_eq!(
            state
                .cache_dirty
                .get(&(1001, BASE_COLOR_CHANNEL.to_string())),
            Some(&DirtyRegion::Full)
        );
        // The plane reads back exactly what was published (a direct upload
        // round-trip — no composite, no tolerance), and Base is untouched:
        // the plane reader and the composite reader agree byte-for-byte.
        let plane = state
            .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
            .expect("published plane reads");
        assert_eq!(plane.data, bytes);
        let base = state
            .read_plane(PlaneOwner::Base, 1001, BASE_COLOR_CHANNEL)
            .expect("Base plane reads");
        // T14 + T28 (doc §8 27/28): the coalesced read composites first
        // and returns `composite_stack_cpu`'s bytes — the L1 stack over
        // transparent Base, NOT Base alone. Base itself stays untouched
        // (zeros) while the composite shows the published plane.
        let composite = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("composite reader reads");
        assert_eq!(composite.data, bytes);
        assert!(base.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn budget_refusal_names_bytes() {
        // §8 T33. With the hard cap lowered to exactly
        // plane_bytes() + 512·512·4 − 1, a new Base plane is refused with
        // the total it would have reached — and nothing is created. The
        // test uses a Base plane on purpose: a new LAYER plane would also
        // materialize a cache texture, and both count toward the cap.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        let before = state.plane_bytes();
        assert_eq!(before, 1_048_576);
        state.set_plane_hard_cap_bytes(before + 1_048_576 - 1);
        let err = state
            .publish_plane(
                PlaneOwner::Base,
                1001,
                "height",
                512,
                512,
                vec![0u8; 512 * 512 * 4],
            )
            .expect_err("crossing the cap must refuse");
        assert_eq!(
            err,
            PaintRefusal::PlaneBudget {
                bytes: before + 1_048_576
            }
        );
        assert!(state.channels_present(1001).is_empty());
        assert_eq!(state.plane_bytes(), before);
        // A publish into an EXISTING plane still succeeds under the same
        // cap: the check gates materialization, never writes.
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state
            .publish_plane(
                PlaneOwner::Base,
                1001,
                "height",
                512,
                512,
                vec![1u8; 512 * 512 * 4],
            )
            .expect("first publish fits the default cap");
        state.set_plane_hard_cap_bytes(state.plane_bytes() - 1);
        state
            .publish_plane(
                PlaneOwner::Base,
                1001,
                "height",
                512,
                512,
                vec![2u8; 512 * 512 * 4],
            )
            .expect("writes into an existing plane ignore the cap");
    }

    // --- Per-layer pool stage 3: selection + stroke gate (§2, §8 T10/T11/T32) ---
    //
    // Verdicts are pushed by the shell in production (`sync_active_target`,
    // once per frame); these tests perform the shell's dance directly. The
    // stack half of the dance (auto-resolution, the vanished-id neighbour
    // walk) is pinned in `document.rs`'s revalidate tests — here the verdict
    // is crafted, and the gate + snapshot + reconcile behavior is pinned.

    /// Drives a converging diagonal stroke: 1 begin + 24 extends (the
    /// conditioner needs a run of events before it emits its first dab —
    /// the `save_open_round_trip` precedent).
    fn drive_converged_diagonal(state: &mut PaintState) {
        state.begin_stroke(emath::pos2(0.2, 0.2));
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            state.extend_stroke(emath::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        state.end_stroke();
    }

    #[test]
    fn gate_refuses_explicit_unpaintable_selection() {
        // §8 T10. Every EXPLICITLY selected unpaintable kind refuses with
        // its typed verdict: no stroke opens, nothing stages, and the
        // refusal names the layer.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let cases = [
            (7u64, InstanceError::PaintOnMaterial { layer: 7 }),
            (
                9u64,
                InstanceError::PaintOnInstance {
                    instance: 9,
                    source_id: 2,
                },
            ),
            (3u64, InstanceError::PaintOnFill { layer: 3 }),
            (5u64, InstanceError::PaintOnFolder { layer: 5 }),
            (999u64, InstanceError::UnknownLayer(999)),
        ];
        for (id, verdict) in cases {
            // The shell's dance: explicit row click, then the frame's
            // verdict push for the same target (same-target sync keeps the
            // explicit flag — see the lifecycle test below).
            state.set_active_target(PlaneOwner::Layer(id));
            state.sync_active_target(PlaneOwner::Layer(id), Err(verdict.clone()), 0);
            assert!(
                state.active_explicit(),
                "the dance keeps layer {id} explicit"
            );
            state.begin_stroke(emath::pos2(0.5, 0.5));
            assert!(
                !state.is_stroking(),
                "a refused stroke opens nothing on layer {id}"
            );
            assert_eq!(
                state.stroke_refusal(),
                Some(PaintRefusal::Layer(verdict)),
                "the refusal is the typed verdict for layer {id}"
            );
        }
        // Exact zero: the refusal returns before conditioning, so no
        // refused stroke can stage a batch whatever the brush does.
        assert_eq!(
            state.staged_batch_count(),
            0,
            "no refused stroke staged anything"
        );
    }

    #[test]
    fn gate_accepts_auto_base_and_mask_strokes() {
        // §8 T10 [M12]. A fill-only stack with NO explicit selection
        // resolves to Base (the resolution itself is `document.rs`'s
        // revalidate test); the pushed auto verdict is accepted into Base.
        // An explicit fill verdict refuses. A mask stroke with a fill
        // selected for mask editing bypasses the gate: the mask plane
        // changes and no pixel plane does.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // Auto Base: accepted, lands in Base, refusal cleared.
        state.sync_active_target(PlaneOwner::Base, Ok(()), 0);
        assert!(!state.active_explicit());
        drive_converged_diagonal(&mut state);
        assert_eq!(state.stroke_owner(), PlaneOwner::Base);
        assert_eq!(state.stroke_refusal(), None);
        let accepted_batches = state.staged_batch_count();
        assert!(
            accepted_batches > 0,
            "a converged Base stroke must stage batches (got {accepted_batches})"
        );
        // Explicit fill: refused, staging frozen.
        state.set_active_target(PlaneOwner::Layer(3));
        state.sync_active_target(
            PlaneOwner::Layer(3),
            Err(InstanceError::PaintOnFill { layer: 3 }),
            0,
        );
        state.begin_stroke(emath::pos2(0.5, 0.5));
        assert!(!state.is_stroking());
        assert_eq!(
            state.stroke_refusal(),
            Some(PaintRefusal::Layer(InstanceError::PaintOnFill { layer: 3 }))
        );
        assert_eq!(
            state.staged_batch_count(),
            accepted_batches,
            "the refused stroke staged nothing"
        );
        // Mask stroke on the same fill: accepted despite the verdict, the
        // mask plane materializes, no pixel batch stages, refusal clears.
        // Order mirrors `layer_mask_ui`: target first (keeps its own mask),
        // then the mask selection.
        state.set_active_target(PlaneOwner::Layer(3));
        state.set_mask_edit_layer(Some(3));
        state.sync_active_target(
            PlaneOwner::Layer(3),
            Err(InstanceError::PaintOnFill { layer: 3 }),
            0,
        );
        state.begin_stroke(emath::pos2(0.5, 0.5));
        assert!(
            state.is_stroking(),
            "a mask stroke bypasses the gate even on a refusing layer"
        );
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            state.extend_stroke(emath::pos2(0.5 + t, 0.5));
        }
        state.end_stroke();
        assert!(
            state.layer_mask(3, 1001).is_some(),
            "the mask stroke painted the fill's mask plane"
        );
        assert_eq!(
            state.staged_batch_count(),
            accepted_batches,
            "a mask stroke stages no pixel batches"
        );
        assert_eq!(state.stroke_refusal(), None);
    }

    #[test]
    fn selection_survives_same_target_sync_and_flips_on_resolve() {
        // §2.3 + §8 T11. The shell pushes the same target every frame: the
        // explicit flag survives adds, removes and reorders (the stack half
        // is `document.rs`'s revalidate test). A moved target is a shell
        // resolution (open, new, vanished id) and arrives auto.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        assert_eq!(state.active_target(), PlaneOwner::Base);
        assert!(!state.active_explicit());
        // An explicit Base selection survives the frame push.
        state.set_active_target(PlaneOwner::Base);
        assert!(state.active_explicit());
        state.sync_active_target(PlaneOwner::Base, Ok(()), 0);
        assert!(
            state.active_explicit(),
            "same-target sync keeps an explicit Base selection"
        );
        // An explicit layer with a fresh verdict and index stays explicit.
        state.set_active_target(PlaneOwner::Layer(1));
        state.sync_active_target(PlaneOwner::Layer(1), Ok(()), 2);
        assert!(state.active_explicit());
        assert_eq!(state.active_index(), 2);
        // A moved target (the shell resolved elsewhere) arrives auto.
        state.sync_active_target(PlaneOwner::Layer(0), Ok(()), 0);
        assert!(!state.active_explicit());
        assert_eq!(state.active_target(), PlaneOwner::Layer(0));
        assert_eq!(state.active_index(), 0);
    }

    #[test]
    fn mid_stroke_selection_change_does_not_split_the_stroke() {
        // §8 T11. Begin on auto Base, move the selection mid-stroke: the
        // live stroke keeps its begin-time owner and every staged batch
        // lands there.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.sync_active_target(PlaneOwner::Base, Ok(()), 0);
        state.begin_stroke(emath::pos2(0.2, 0.2));
        assert!(state.is_stroking());
        assert_eq!(state.stroke_owner(), PlaneOwner::Base);
        state.set_active_target(PlaneOwner::Layer(4));
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            state.extend_stroke(emath::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        assert_eq!(
            state.stroke_owner(),
            PlaneOwner::Base,
            "the live stroke keeps its begin-time owner"
        );
        state.end_stroke();
        assert!(
            state.staged_batch_count() > 0,
            "the converged stroke must stage batches"
        );
        assert_eq!(
            state.staged_batch_count_for(1001),
            state.staged_batch_count(),
            "every staged batch landed in the begin-time owner's tile"
        );
    }

    #[test]
    fn set_active_target_reconciles_mask_editing() {
        // §8 T32 [m5]. Selecting a new layer (the add button's add-select)
        // clears a stale mask edit; selecting the mask's own layer keeps
        // it; Base clears it.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.set_mask_edit_layer(Some(2));
        state.set_active_target(PlaneOwner::Layer(5));
        assert_eq!(
            state.mask_edit_layer(),
            None,
            "a new layer's selection clears L2's mask edit"
        );
        assert!(state.active_explicit());
        state.set_mask_edit_layer(Some(2));
        state.set_active_target(PlaneOwner::Layer(2));
        assert_eq!(
            state.mask_edit_layer(),
            Some(2),
            "selecting the mask's own layer keeps the edit"
        );
        state.set_active_target(PlaneOwner::Base);
        assert_eq!(state.mask_edit_layer(), None);
    }

    // --- Per-layer pool stage 4: per-layer pixels (§3–§6, §8) ---
    //
    // Strokes route into `stroke_owner`'s plane, the flush composites the
    // cache every frame, and deletes tombstone paint planes. All GPU-gated
    // (lavapipe on CI, silent skip without an adapter).

    /// A Base-only snapshot over `(tile, channel)`, displayed: the test
    /// flush's stack shape.
    fn base_snapshot(tile: u16, channel: &str) -> CompositeSnapshot {
        CompositeSnapshot {
            layers: vec![base_row()],
            base_visible: true,
            displayed: vec![(tile, channel.to_string())],
        }
    }

    /// The Base pseudo-row: Normal, opacity 1, unmasked (visibility rides
    /// the snapshot's `base_visible` in the shell builder; tests show it
    /// directly).
    fn base_row() -> SnapshotLayer {
        SnapshotLayer {
            owner: PlaneOwner::Base,
            layer_id: None,
            kind: SnapshotLayerKind::Paint,
            blend: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        }
    }

    /// The §8 T5–T7 stack: `L0` paint opaque 200 bottom, `M` Multiply
    /// (src 100 opaque, layer 1.0 × channel 0.25) middle, `L2` paint
    /// Normal opacity 0.5 opaque 60 top. Base stays transparent (a layer
    /// touched only on its own planes is the transparent no-op).
    fn order_stack_snapshot() -> CompositeSnapshot {
        let paint_row = |id: u64, blend: BlendMode, opacity: f32| SnapshotLayer {
            owner: PlaneOwner::Layer(id),
            layer_id: Some(id),
            kind: SnapshotLayerKind::Paint,
            blend,
            opacity,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        };
        CompositeSnapshot {
            layers: vec![
                SnapshotLayer {
                    owner: PlaneOwner::Base,
                    layer_id: None,
                    kind: SnapshotLayerKind::Paint,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: None,
                },
                paint_row(10, BlendMode::Normal, 1.0),
                SnapshotLayer {
                    owner: PlaneOwner::Layer(11),
                    layer_id: Some(11),
                    kind: SnapshotLayerKind::Material,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: Some(vec![SnapshotMaterialChannel {
                        channel: BASE_COLOR_CHANNEL.to_string(),
                        blend: BlendMode::Multiply,
                        opacity: 0.25,
                    }]),
                },
                paint_row(12, BlendMode::Normal, 0.5),
            ],
            base_visible: true,
            displayed: vec![(1001, BASE_COLOR_CHANNEL.to_string())],
        }
    }

    /// Publishes the T5–T7 planes (premultiplied writer bytes): L0 opaque
    /// 200, M src 100 opaque, L2 opaque 60.
    fn publish_order_planes(state: &mut PaintState) {
        let flat = |v: u8| {
            let mut bytes = Vec::with_capacity(512 * 512 * 4);
            for _ in 0..512 * 512 {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
            bytes
        };
        state
            .publish_plane(
                PlaneOwner::Layer(10),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                flat(200),
            )
            .expect("L0 publishes");
        state
            .publish_plane(
                PlaneOwner::Layer(11),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                flat(100),
            )
            .expect("M publishes");
        state
            .publish_plane(
                PlaneOwner::Layer(12),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                flat(60),
            )
            .expect("L2 publishes");
    }

    /// The CPU oracle over the T5 planes: one [`CpuStackLayer`] per row
    /// (premultiplied, §4.3 item 1), folded bottom-to-top.
    fn oracle_order(l0: &[u8], m: &[u8], l2: &[u8]) -> Vec<u8> {
        use umber_gpu::compositor::{composite_stack_cpu, CpuStackLayer};
        composite_stack_cpu(
            &[
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(l0),
                    mask: None,
                    premultiplied: true,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Multiply,
                    opacity: 0.25,
                    visible: true,
                    rgba: Some(m),
                    mask: None,
                    premultiplied: true,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 0.5,
                    visible: true,
                    rgba: Some(l2),
                    mask: None,
                    premultiplied: true,
                },
            ],
            512,
            512,
        )
        .expect("oracle composites")
    }

    #[test]
    fn per_layer_routing_stroke_capture_and_undo() {
        // §8 T9. A stroke with `active_target = Layer(1)` changes
        // `(Layer(1), 1001, baseColor)` only: on a 512² session that one
        // stroke adds exactly that plane plus the `(tile, channel)` cache
        // (`plane_bytes() == 3 · 1,048,576`), Base and L0 stay
        // byte-identical, the cache equals `composite_stack_cpu`, and undo
        // restores L1 byte-exact leaving L0 untouched.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // L0 holds paint the stroke must not touch.
        let l0_bytes: Vec<u8> = (0..512 * 512).flat_map(|_| [10, 20, 30, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(0),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                l0_bytes.clone(),
            )
            .expect("L0 publishes");
        state.process_pending().expect("drain succeeds");
        let base_before = state
            .read_plane(PlaneOwner::Base, 1001, BASE_COLOR_CHANNEL)
            .expect("Base reads");
        state.set_active_target(PlaneOwner::Layer(1));
        state.sync_active_target(PlaneOwner::Layer(1), Ok(()), 0);
        drive_converged_diagonal(&mut state);
        assert_eq!(state.stroke_owner(), PlaneOwner::Layer(1));
        assert!(state.stroke_refusal().is_none());
        // Exactly: Base's target + L0 + L1's plane + the stroke's
        // materialized cache = 4 × 512²×4 (the comment's "one layer
        // plane plus the cache beside Base" — Base counts too).
        assert_eq!(state.plane_bytes(), 4 * 1_048_576);
        assert!(state.planes.contains_key(&(
            PlaneOwner::Layer(1),
            1001,
            BASE_COLOR_CHANNEL.to_string()
        )));
        let base_after = state
            .read_plane(PlaneOwner::Base, 1001, BASE_COLOR_CHANNEL)
            .expect("Base reads");
        assert_eq!(base_after.data, base_before.data, "Base is byte-identical");
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(0), 1001, BASE_COLOR_CHANNEL)
                .expect("L0 reads")
                .data,
            l0_bytes,
            "L0 is byte-identical"
        );
        // The cache equals the CPU oracle: Base (transparent) seeded, then
        // L0 Normal/1.0, then L1 Normal/1.0.
        let l1 = state
            .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
            .expect("L1 reads");
        assert_ne!(
            l1.data,
            vec![0u8; 512 * 512 * 4],
            "the stroke painted L1's plane"
        );
        let snapshot = CompositeSnapshot {
            layers: vec![
                base_row(),
                SnapshotLayer {
                    owner: PlaneOwner::Base,
                    layer_id: None,
                    kind: SnapshotLayerKind::Paint,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: None,
                },
                SnapshotLayer {
                    owner: PlaneOwner::Layer(0),
                    layer_id: Some(0),
                    kind: SnapshotLayerKind::Paint,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: None,
                },
                SnapshotLayer {
                    owner: PlaneOwner::Layer(1),
                    layer_id: Some(1),
                    kind: SnapshotLayerKind::Paint,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: None,
                },
            ],
            base_visible: true,
            displayed: vec![(1001, BASE_COLOR_CHANNEL.to_string())],
        };
        state.flush_composite(&snapshot);
        let cache = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        let oracle = {
            use umber_gpu::compositor::{composite_stack_cpu, CpuStackLayer};
            composite_stack_cpu(
                &[
                    CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(&base_before.data),
                        mask: None,
                        premultiplied: true,
                    },
                    CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(&l0_bytes),
                        mask: None,
                        premultiplied: true,
                    },
                    CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(&l1.data),
                        mask: None,
                        premultiplied: true,
                    },
                ],
                512,
                512,
            )
            .expect("oracle composites")
        };
        // The GPU twin folds 3 contributing entries (the transparent Base
        // seed is exact, the two paints quantize per dispatch): ≤ 2 LSB
        // per texel (the §8 twin rule, with the contributing count).
        for (i, (got, want)) in cache.data.iter().zip(oracle.iter()).enumerate() {
            let drift = (*got as i16 - *want as i16).abs();
            assert!(drift <= 2, "texel {i}: got {got}, want {want}");
        }
        // Undo restores L1's plane byte-exact (its stroke-start blank) and
        // leaves L0 untouched.
        assert!(state.undo_stroke());
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
                .expect("L1 reads")
                .data,
            vec![0u8; 512 * 512 * 4]
        );
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(0), 1001, BASE_COLOR_CHANNEL)
                .expect("L0 reads")
                .data,
            l0_bytes
        );
    }

    #[test]
    fn non_displayed_full_composites_on_read() {
        // §8 T28 (third arm): a metadata `Full` on a non-displayed channel
        // is NOT composited by the frame flush (its dirt is still `Full`
        // after), and a `read_channel_tile` on it composites first and
        // returns the oracle's bytes.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let flat200: Vec<u8> = (0..512 * 512).flat_map(|_| [200, 200, 200, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(3),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                flat200.clone(),
            )
            .expect("plane publishes");
        // A frame flush with nothing displayed: the `Full` survives.
        state.flush_composite(&CompositeSnapshot::default());
        assert_eq!(
            state
                .cache_dirty
                .get(&(1001, BASE_COLOR_CHANNEL.to_string())),
            Some(&DirtyRegion::Full)
        );
        // The read composites first (against the synthesized all-visible
        // stack — the headless fallback) and returns the plane's bytes
        // over transparent Base.
        let read = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("read composites");
        assert_eq!(read.data, flat200);
        assert!(
            !state
                .cache_dirty
                .contains_key(&(1001, BASE_COLOR_CHANNEL.to_string())),
            "the read consumed the dirt"
        );
    }

    #[test]
    fn distant_footprints_stay_two_rects() {
        // §8 T28 (first arm, headless): two footprints at x≈100 and
        // x≈3996 on one 4K `(tile, channel)` give 2 rects, not one
        // near-full-width union.
        let mut dirt = DirtyRegion::Rects(Vec::new());
        dirt.mark_rect(DirtyRect {
            x0: 80,
            y0: 100,
            x1: 120,
            y1: 140,
        });
        dirt.mark_rect(DirtyRect {
            x0: 3976,
            y0: 100,
            x1: 4016,
            y1: 140,
        });
        let DirtyRegion::Rects(rects) = &dirt else {
            panic!("two disjoint footprints must not collapse to Full");
        };
        assert_eq!(rects.len(), 2);
        assert!(
            rects.iter().all(|r| r.width() < 1000),
            "no near-full-width union"
        );
    }

    #[test]
    fn erased_texel_composites_227_over_white() {
        // §8 T22 (composite half, headless): the kernel golden
        // `(100,0,0,128)` over opaque white gives `r = 1 + (100/128 −
        // 1)·128/255 = 227/255 → 227` exactly, `g = b = 127`. The kernel
        // bytes come from the real erase op at exactly-0.5 coverage.
        use umber_brush::retouch::{erase_dab_premultiplied, EraseParams, RetouchDab, RgbaBuffer};
        use umber_gpu::compositor::{composite_stack_cpu, CpuStackLayer};
        let dab = RetouchDab {
            pos: [0.5, 0.5],
            radius: 8.0,
            alpha: 0.5,
            hardness: 1.0,
        };
        assert_eq!(
            umber_brush::retouch::coverage(&dab, 0, 0),
            Some(0.5),
            "the golden's coverage must be exactly 0.5 first"
        );
        let mut buf = RgbaBuffer::transparent(1, 1);
        buf.set_texel(0, 0, [200, 0, 0, 255]);
        erase_dab_premultiplied(&mut buf, &dab, EraseParams::default()).expect("erase writes");
        assert_eq!(buf.texel(0, 0), [100, 0, 0, 128]);
        let white = [255, 255, 255, 255];
        let out = composite_stack_cpu(
            &[
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(&white),
                    mask: None,
                    premultiplied: true,
                },
                CpuStackLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    rgba: Some(&buf.data),
                    mask: None,
                    premultiplied: true,
                },
            ],
            1,
            1,
        )
        .expect("oracle composites");
        assert_eq!(out, vec![227, 127, 127, 255]);
        // Sentinel: the unscaled-rgb plane `(200,0,0,128)` clamps
        // `s = 1.5625 → 1.0`, giving `r = 255`. Assert 227 and ≠ 255.
        assert_ne!(out[0], 255);
    }

    #[test]
    fn composite_order_preserved_115_then_110() {
        // §8 T5 (wart 1: stack order is preserved). The Multiply factor is
        // `1 − 0.25·(1 − 100/255) = 0.848039`: stack order gives
        // `200·0.848039 = 169.608`, then `169.608 + (60 − 169.608)·0.5 =
        // 114.804 → 115`. With M reordered to the top: `(200 + 60)/2 =
        // 130`, then `130·0.848039 = 110.245 → 110`. Sentinel: v1's
        // destructive path gives `60·0.848039 = 50.882 → 51` whatever the
        // order (strokes ignore layer opacity there).
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        publish_order_planes(&mut state);
        state.flush_composite(&order_stack_snapshot());
        let cache = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        let flat = |v: u8| {
            let mut bytes = Vec::with_capacity(512 * 512 * 4);
            for _ in 0..512 * 512 {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
            bytes
        };
        let oracle = oracle_order(&flat(200), &flat(100), &flat(60));
        assert_eq!(oracle[0], 115);
        // The GPU twin folds 4 contributing entries (the transparent Base
        // seeds exact — the copy is a no-op — and L0/M/L2 quantize per
        // dispatch): ≤ 3 LSB per texel (the §8 twin rule, with the
        // contributing count). UNORM rounding and FMA contraction are
        // implementation-defined (compositor judgment 8); the oracle pins
        // the exact bytes.
        for (i, (got, want)) in cache.data.iter().zip(oracle.iter()).enumerate() {
            let drift = (*got as i16 - *want as i16).abs();
            assert!(drift <= 3, "texel {i}: got {got}, want {want}");
        }
        assert_ne!(
            cache.data[0], 51,
            "the destructive-publish sentinel must not appear"
        );
        // Reorder M to the top (the shell's snapshot diff marks `Full`;
        // the test applies the mark directly).
        for owner in [
            PlaneOwner::Layer(10),
            PlaneOwner::Layer(11),
            PlaneOwner::Layer(12),
        ] {
            state.mark_owner_full(owner);
        }
        let mut reordered = order_stack_snapshot();
        let m = reordered.layers.remove(2);
        reordered.layers.push(m);
        state.flush_composite(&reordered);
        let cache = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        // Reordered oracle: L2 over L0 gives 130.0, then M gives 110.245
        // → 110 (exact on the CPU side; the GPU twin holds ≤ 3 LSB — the
        // same 4-entry count as above).
        let drift = (cache.data[0] as i16 - 110).abs();
        assert!(
            drift <= 3,
            "reordered composite holds 110 ± twin: {}",
            cache.data[0]
        );
        assert_ne!(cache.data[0], 51);
    }

    #[test]
    fn anchor_prefix_flattens_downward_through_the_anchored_layer() {
        // §8 T8 (stage 5, the downward partial flatten). Stack: Base
        // (transparent), L0 paint opaque 200, L1 paint Normal opacity 0.5
        // opaque 100, M material on top (src 37 — its output must never
        // reach an anchor below it).
        // - anchor(L0) = 200 (Base transparent seeds nothing).
        // - anchor(L1) = 200 + (100 − 200)·0.5 = 150.0 → 150.
        // CPU oracle → bit-exact: assert_eq, no GPU tolerance.
        // Sentinel: v1's full-flatten read (the flush cache) folds M too.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let flat = |v: u8| {
            let mut bytes = Vec::with_capacity(512 * 512 * 4);
            for _ in 0..512 * 512 {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
            bytes
        };
        for (id, v) in [(20u64, 200u8), (21, 100), (22, 37)] {
            state
                .publish_plane(
                    PlaneOwner::Layer(id),
                    1001,
                    BASE_COLOR_CHANNEL,
                    512,
                    512,
                    flat(v),
                )
                .expect("plane publishes");
        }
        let paint_row = |id: u64, opacity: f32| SnapshotLayer {
            owner: PlaneOwner::Layer(id),
            layer_id: Some(id),
            kind: SnapshotLayerKind::Paint,
            blend: BlendMode::Normal,
            opacity,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        };
        let snapshot = CompositeSnapshot {
            layers: vec![
                SnapshotLayer {
                    owner: PlaneOwner::Base,
                    layer_id: None,
                    kind: SnapshotLayerKind::Paint,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: None,
                },
                paint_row(20, 1.0),
                paint_row(21, 0.5),
                SnapshotLayer {
                    owner: PlaneOwner::Layer(22),
                    layer_id: Some(22),
                    kind: SnapshotLayerKind::Material,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: false,
                    material_channels: Some(vec![SnapshotMaterialChannel {
                        channel: BASE_COLOR_CHANNEL.to_string(),
                        blend: BlendMode::Normal,
                        opacity: 1.0,
                    }]),
                },
            ],
            base_visible: true,
            displayed: vec![(1001, BASE_COLOR_CHANNEL.to_string())],
        };
        let l0 = state
            .read_anchor_prefix(&snapshot, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(20))
            .expect("anchor(L0) composites");
        assert_eq!((l0.width, l0.height), (512, 512));
        assert!(l0.data.chunks_exact(4).all(|px| px == [200, 200, 200, 255]));
        let l1 = state
            .read_anchor_prefix(&snapshot, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(21))
            .expect("anchor(L1) composites");
        assert!(l1.data.chunks_exact(4).all(|px| px == [150, 150, 150, 255]));
        // Bit-exact and deterministic: a second read is byte-identical.
        let again = state
            .read_anchor_prefix(&snapshot, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(21))
            .expect("anchor(L1) re-reads");
        assert_eq!(again.data, l1.data);
        // A layer absent from the snapshot (deleted) is `None`, never a
        // silent full flatten.
        assert!(state
            .read_anchor_prefix(&snapshot, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(99))
            .is_none());
        // Sentinel: the full flatten includes M's 37 — the v1 anchor read.
        state.flush_composite(&snapshot);
        let full = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        assert!(
            (full.data[0] as i16 - 37).abs() <= 3,
            "the full flatten is M on top (37 ± twin), got {}",
            full.data[0]
        );
        assert_ne!(full.data[0], l1.data[0], "the prefix must exclude M");
    }

    #[test]
    fn anchor_prefix_stops_at_a_planeless_anchor_row() {
        // The truncation is by ROW: an anchor on a layer with no plane on
        // this tile still ends the prefix, so a layer ABOVE it never leaks
        // in. L0 opaque 200 below, L1 (no plane), L2 opaque 60 above.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let flat = |v: u8| {
            let mut bytes = Vec::with_capacity(512 * 512 * 4);
            for _ in 0..512 * 512 {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
            bytes
        };
        for (id, v) in [(30u64, 200u8), (32, 60)] {
            state
                .publish_plane(
                    PlaneOwner::Layer(id),
                    1001,
                    BASE_COLOR_CHANNEL,
                    512,
                    512,
                    flat(v),
                )
                .expect("plane publishes");
        }
        let row = |id: u64| SnapshotLayer {
            owner: PlaneOwner::Layer(id),
            layer_id: Some(id),
            kind: SnapshotLayerKind::Paint,
            blend: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        };
        let snapshot = CompositeSnapshot {
            layers: vec![row(30), row(31), row(32)],
            base_visible: true,
            displayed: Vec::new(),
        };
        let prefix = state
            .read_anchor_prefix(&snapshot, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(31))
            .expect("anchor(L1) composites");
        assert!(
            prefix
                .data
                .chunks_exact(4)
                .all(|px| px == [200, 200, 200, 255]),
            "L2's 60 above the anchor must not leak in, got {:?}",
            &prefix.data[..4]
        );
    }

    #[test]
    fn instance_row_renders_its_source_planes_with_its_own_opacity() {
        // §5.3 stage 5: an instance row carries its SOURCE's owner and its
        // own opacity. Source L0 opaque 200; instance at opacity 0.5 over
        // it composites the same 200 plane again: 200 + (200 − 200)·0.5 =
        // 200 — so the source sits on a 100 layer first to make the
        // instance visible: L_bottom 100, L0 200 at opacity 0.5 → 150;
        // instance of L0 at 0.5 → 150 + (200 − 150)·0.5 = 175.
        // Sentinel: without the instance row the cache holds 150.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let flat = |v: u8| {
            let mut bytes = Vec::with_capacity(512 * 512 * 4);
            for _ in 0..512 * 512 {
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
            bytes
        };
        for (id, v) in [(40u64, 100u8), (41, 200)] {
            state
                .publish_plane(
                    PlaneOwner::Layer(id),
                    1001,
                    BASE_COLOR_CHANNEL,
                    512,
                    512,
                    flat(v),
                )
                .expect("plane publishes");
        }
        let row = |owner: u64, id: u64, opacity: f32| SnapshotLayer {
            owner: PlaneOwner::Layer(owner),
            layer_id: Some(id),
            kind: SnapshotLayerKind::Paint,
            blend: BlendMode::Normal,
            opacity,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        };
        let without = CompositeSnapshot {
            layers: vec![row(40, 40, 1.0), row(41, 41, 0.5)],
            base_visible: true,
            displayed: vec![(1001, BASE_COLOR_CHANNEL.to_string())],
        };
        let mut with = without.clone();
        with.layers.push(row(41, 42, 0.5));
        // CPU prefix through the instance row (exact) and through the
        // source row (excludes the instance above it).
        let through_instance = state
            .read_anchor_prefix(&with, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(42))
            .expect("prefix through the instance");
        assert_eq!(&through_instance.data[..4], &[175, 175, 175, 255]);
        let through_source = state
            .read_anchor_prefix(&with, 1001, BASE_COLOR_CHANNEL, PlaneOwner::Layer(41))
            .expect("prefix through the source");
        assert_eq!(&through_source.data[..4], &[150, 150, 150, 255]);
        // GPU flush: the instance row renders (≤ 3 LSB twin, 3 entries).
        state.flush_composite(&with);
        let cache = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        assert!(
            (cache.data[0] as i16 - 175).abs() <= 3,
            "instance renders: got {}",
            cache.data[0]
        );
        assert!(
            (cache.data[0] as i16 - 150).abs() > 3,
            "the no-instance sentinel (150) must not appear"
        );
    }

    #[test]
    fn reeval_is_idempotent_170_170_never_144() {
        // §8 T6 (wart 2: re-eval never compounds). L0 + M only:
        // `169.608 → 170`; refreshing M with identical inputs (a whole-plane
        // REPLACE, not a composite over the last output) gives 170 again.
        // Sentinel: v1's compounding gives `170·0.848039 = 144.167 → 144`.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        publish_order_planes(&mut state);
        let pair = || {
            let mut snapshot = order_stack_snapshot();
            snapshot
                .layers
                .retain(|layer| !matches!(layer.owner, PlaneOwner::Layer(12)));
            snapshot
        };
        state.flush_composite(&pair());
        let first = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        assert_eq!(first.data[0], 170);
        // The refresh: identical bytes REPLACE the plane whole.
        let flat100: Vec<u8> = (0..512 * 512).flat_map(|_| [100, 100, 100, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(11),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                flat100,
            )
            .expect("M re-publishes");
        state.flush_composite(&pair());
        let second = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads");
        assert_eq!(second.data[0], 170);
        assert_eq!(second.data, first.data);
        assert_ne!(
            second.data[0], 144,
            "the compounding sentinel must not appear"
        );
    }

    #[test]
    fn unpublish_hides_disables_deletes_and_revives_by_reeval() {
        // §8 T7 (wart 3: hiding/deleting un-publishes). From T6's L0 + M:
        // hide → 200; show → 170; disable the channel → 200; delete → 200
        // with eval-owned planes DROPPED (no tombstone); revive by re-eval
        // (the `Insert` params' refresh) → 170.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        publish_order_planes(&mut state);
        let pair = || {
            let mut snapshot = order_stack_snapshot();
            snapshot
                .layers
                .retain(|layer| !matches!(layer.owner, PlaneOwner::Layer(12)));
            snapshot
        };
        let shown = pair();
        state.flush_composite(&shown);
        let shown_bytes = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads")
            .data;
        assert_eq!(shown_bytes[0], 170);
        // Hide M → 200.
        let mut hidden = pair();
        hidden
            .layers
            .retain(|layer| !matches!(layer.owner, PlaneOwner::Layer(11)));
        state.mark_owner_full(PlaneOwner::Layer(11));
        state.flush_composite(&hidden);
        let hidden_bytes = state
            .read_composite(1001, BASE_COLOR_CHANNEL)
            .expect("cache reads")
            .data;
        assert_eq!(hidden_bytes[0], 200);
        // Show M → 170.
        state.mark_owner_full(PlaneOwner::Layer(11));
        state.flush_composite(&shown);
        assert_eq!(
            state
                .read_composite(1001, BASE_COLOR_CHANNEL)
                .expect("cache reads")
                .data,
            shown_bytes
        );
        // Disable its channel (no entry) → 200.
        let mut disabled = pair();
        if let Some(m) = disabled
            .layers
            .iter_mut()
            .find(|layer| matches!(layer.owner, PlaneOwner::Layer(11)))
        {
            m.material_channels = None;
        }
        state.mark_owner_full(PlaneOwner::Layer(11));
        state.flush_composite(&disabled);
        assert_eq!(
            state
                .read_composite(1001, BASE_COLOR_CHANNEL)
                .expect("cache reads")
                .data[0],
            200
        );
        // Delete M: eval-owned planes DROP (no tombstone), cache → 200.
        state.note_removed_layers(&[(11, false)]);
        assert!(
            state.retire_pending.is_empty(),
            "eval-owned planes never queue a readback"
        );
        state.flush_composite(&hidden);
        assert!(
            !state
                .planes
                .keys()
                .any(|(owner, _, _)| *owner == PlaneOwner::Layer(11)),
            "M's planes are dropped"
        );
        assert!(!state.retired.contains_key(&11), "M leaves no tombstone");
        assert_eq!(
            state
                .read_composite(1001, BASE_COLOR_CHANNEL)
                .expect("cache reads")
                .data[0],
            200
        );
        // Undo of the delete re-dirties from the `Insert` params; the next
        // refresh REPLACES the plane (revive by re-eval, not by tombstone).
        let flat100: Vec<u8> = (0..512 * 512).flat_map(|_| [100, 100, 100, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(11),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                flat100,
            )
            .expect("M re-evaluates");
        state.flush_composite(&shown);
        assert_eq!(
            state
                .read_composite(1001, BASE_COLOR_CHANNEL)
                .expect("cache reads")
                .data,
            shown_bytes
        );
    }

    #[test]
    fn resize_drops_planes_and_their_tombstones() {
        // §8 T19. `resize_set` drops every plane's contents on the set's
        // tiles (other sets' planes are byte-identical); the resized set's
        // tombstones drop too, and other sets' survive.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let set = state.active_set().to_string();
        let pattern: Vec<u8> = (0..512 * 512).flat_map(|_| [11, 22, 33, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                pattern,
            )
            .expect("L1 publishes");
        // A second set with its own tile, plane and tombstone.
        state.publish_fill(1002, 512, 512, vec![0u8; 512 * 512 * 4]);
        state.assign_tile_to_set(1002, "other");
        state.resize_set("other", 512, 512);
        let other_pattern: Vec<u8> = (0..512 * 512).flat_map(|_| [44, 55, 66, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(2),
                1002,
                BASE_COLOR_CHANNEL,
                512,
                512,
                other_pattern.clone(),
            )
            .expect("L2 publishes");
        state.note_removed_layers(&[(2, true)]);
        state.flush_composite(&CompositeSnapshot::default());
        assert!(
            state.retired.contains_key(&2),
            "L2's plane tombstones (one plane, one pump)"
        );
        // Resize the first set: L1's contents drop, L2 is byte-identical.
        state.resize_set(&set, 256, 256);
        let l1 = state
            .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
            .expect("L1 reads");
        assert_eq!((l1.width, l1.height), (256, 256));
        assert!(
            l1.data.iter().all(|&b| b == 0),
            "resized contents are dropped"
        );
        assert_eq!(
            state
                .cache_dirty
                .get(&(1001, BASE_COLOR_CHANNEL.to_string())),
            Some(&DirtyRegion::Full)
        );
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(2), 1002, BASE_COLOR_CHANNEL)
                .expect("L2 reads")
                .data,
            other_pattern,
            "other sets' planes are byte-identical"
        );
        assert!(
            state.retired.contains_key(&2),
            "other sets' tombstones survive"
        );
        // Tombstone L1, then resize its set: the tombstone drops.
        state.note_removed_layers(&[(1, true)]);
        state.flush_composite(&CompositeSnapshot::default());
        assert!(state.retired.contains_key(&1));
        state.resize_set(&set, 128, 128);
        assert!(
            !state.retired.contains_key(&1),
            "the resized set's tombstones drop"
        );
        assert!(
            state.retired.contains_key(&2),
            "other sets' tombstones still survive"
        );
    }

    #[test]
    fn base_stays_reachable_and_clone_samples_the_composite() {
        // §8 T23 [M3]. A v1 project (Base paint) with one Paint layer:
        // selecting Base and erasing changes Base's plane while the layer
        // plane is absent-or-unchanged (and vice versa); cloning onto a
        // newly added EMPTY layer lands the cloned composite bytes with
        // `p ≤ a` (the layer-only sample would leave it empty).
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let white: Vec<u8> = (0..512 * 512).flat_map(|_| [255, 255, 255, 255]).collect();
        assert!(
            state.publish_fill(1001, 512, 512, white.clone()),
            "Base holds the v1 paint"
        );
        let red: Vec<u8> = (0..512 * 512).flat_map(|_| [255, 0, 0, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                red.clone(),
            )
            .expect("L1 publishes");
        state.process_pending().expect("drain succeeds");
        // Select Base, erase: Base's alpha changes, L1 is untouched.
        state.set_active_target(PlaneOwner::Base);
        state.sync_active_target(PlaneOwner::Base, Ok(()), 0);
        let mut erase_tool = BrushTool::Erase(umber_brush::EraseParams::default());
        state.sync_tool(&mut erase_tool);
        drive_converged_diagonal(&mut state);
        state.process_pending().expect("drain succeeds");
        let base_erased = state
            .read_plane(PlaneOwner::Base, 1001, BASE_COLOR_CHANNEL)
            .expect("Base reads");
        assert!(
            base_erased.data.chunks_exact(4).any(|px| px[3] != 255),
            "the Base erase changed alpha somewhere on the diagonal"
        );
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
                .expect("L1 reads")
                .data,
            red,
            "L1 is byte-identical after a Base erase"
        );
        // Sentinel: with the Paint layer selected instead, the same erase
        // leaves Base byte-identical.
        let base_before_reselect = base_erased.data.clone();
        state.set_active_target(PlaneOwner::Layer(1));
        state.sync_active_target(PlaneOwner::Layer(1), Ok(()), 0);
        let mut erase_tool = BrushTool::Erase(umber_brush::EraseParams::default());
        state.sync_tool(&mut erase_tool);
        drive_converged_diagonal(&mut state);
        state.process_pending().expect("drain succeeds");
        assert_eq!(
            state
                .read_plane(PlaneOwner::Base, 1001, BASE_COLOR_CHANNEL)
                .expect("Base reads")
                .data,
            base_before_reselect,
            "a layer erase leaves Base byte-identical"
        );
    }

    #[test]
    fn clone_onto_empty_layer_lands_composite_bytes() {
        // §8 T23 (clone half): onto a newly added empty layer whose source
        // region shows Base paint, the plane becomes non-empty with the
        // cloned composite bytes (`p ≤ a` throughout).
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let white: Vec<u8> = (0..512 * 512).flat_map(|_| [255, 255, 255, 255]).collect();
        assert!(state.publish_fill(1001, 512, 512, white));
        state.set_active_target(PlaneOwner::Layer(7));
        state.sync_active_target(PlaneOwner::Layer(7), Ok(()), 0);
        let mut clone_tool = BrushTool::Clone(umber_brush::CloneParams::default());
        state.sync_tool(&mut clone_tool);
        drive_converged_diagonal(&mut state);
        state.process_pending().expect("drain succeeds");
        let plane = state
            .read_plane(PlaneOwner::Layer(7), 1001, BASE_COLOR_CHANNEL)
            .expect("L7 reads");
        assert_ne!(
            plane.data,
            vec![0u8; 512 * 512 * 4],
            "the clone landed composite bytes (the layer-only sample would stay empty)"
        );
        assert!(
            plane
                .data
                .chunks_exact(4)
                .any(|px| px == [255, 255, 255, 255]),
            "covered texels equal the cloned white composite"
        );
        for px in plane.data.chunks_exact(4) {
            assert!(
                px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3],
                "p ≤ a holds: {px:?}"
            );
        }
    }

    #[test]
    fn tombstones_revive_identical_evict_oldest_and_read_one_per_frame() {
        // §8 T25 [M5]. A paint delete → undo revives byte-identical
        // planes; an oversize tombstone is retained while newest (and its
        // undo revives identical); a second delete frees the first, whose
        // undo revives EMPTY with the status line; retirement reads back
        // at most one plane per frame.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let paint_row = |id: u64| SnapshotLayer {
            owner: PlaneOwner::Layer(id),
            layer_id: Some(id),
            kind: SnapshotLayerKind::Paint,
            blend: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            mask_enabled: false,
            material_channels: None,
        };
        let with_layer = |id: u64| CompositeSnapshot {
            layers: vec![base_row(), paint_row(id)],
            base_visible: true,
            displayed: vec![(1001, BASE_COLOR_CHANNEL.to_string())],
        };
        // Three planes on L1 (amortization needs 3 frames).
        let pat_a: Vec<u8> = (0..512 * 512).flat_map(|_| [10, 20, 30, 255]).collect();
        let pat_b: Vec<u8> = (0..512 * 512).flat_map(|_| [40, 50, 60, 255]).collect();
        let pat_c: Vec<u8> = (0..512 * 512).flat_map(|_| [70, 80, 90, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                pat_a.clone(),
            )
            .expect("publishes");
        state
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                "height",
                512,
                512,
                pat_b.clone(),
            )
            .expect("publishes");
        state
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                "roughness",
                512,
                512,
                pat_c.clone(),
            )
            .expect("publishes");
        state.note_removed_layers(&[(1, true)]);
        assert_eq!(state.retire_pending.len(), 3);
        state.flush_composite(&CompositeSnapshot::default());
        assert_eq!(state.retire_pending.len(), 2, "at most one plane per frame");
        state.flush_composite(&CompositeSnapshot::default());
        assert_eq!(state.retire_pending.len(), 1);
        state.flush_composite(&CompositeSnapshot::default());
        assert!(state.retire_pending.is_empty());
        assert_eq!(
            state.retired.get(&1).map(|(_, planes)| planes.len()),
            Some(3)
        );
        // Undo of the delete (the layer rejoins): bytes revive identical.
        state.flush_composite(&with_layer(1));
        assert!(
            !state.retired.contains_key(&1),
            "the revive consumed the tombstone"
        );
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
                .expect("revived reads")
                .data,
            pat_a
        );
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(1), 1001, "height")
                .expect("revived reads")
                .data,
            pat_b
        );
        assert_eq!(
            state
                .read_plane(PlaneOwner::Layer(1), 1001, "roughness")
                .expect("revived reads")
                .data,
            pat_c
        );
        // Oversize tombstones: the cap below one plane keeps the newest.
        state.set_retired_cap_bytes(1);
        state.note_removed_layers(&[(1, true)]);
        state.flush_composite(&CompositeSnapshot::default());
        state.flush_composite(&CompositeSnapshot::default());
        state.flush_composite(&CompositeSnapshot::default());
        assert!(
            state.retired.contains_key(&1),
            "one oversize tombstone is retained while newest"
        );
        // A second delete frees the first (oldest); undoing the first
        // revives it EMPTY with the status line.
        let pat_d: Vec<u8> = (0..512 * 512).flat_map(|_| [1, 2, 3, 255]).collect();
        state
            .publish_plane(
                PlaneOwner::Layer(2),
                1001,
                BASE_COLOR_CHANNEL,
                512,
                512,
                pat_d,
            )
            .expect("publishes");
        state.note_removed_layers(&[(2, true)]);
        state.flush_composite(&CompositeSnapshot::default());
        assert!(!state.retired.contains_key(&1), "the oldest frees first");
        assert!(state.retired.contains_key(&2), "the newest is retained");
        assert_eq!(state.take_tombstone_notice(), Some(1));
        state.flush_composite(&with_layer(1));
        assert!(
            !state
                .planes
                .keys()
                .any(|(owner, _, _)| *owner == PlaneOwner::Layer(1)),
            "the freed delete revives empty"
        );
    }

    #[test]
    fn mask_strokes_dirty_every_planed_channel() {
        // §8 T26 [M6]. A mask stroke on a 2-channel material with planes
        // on tile 1001 leaves `cache_dirty` holding exactly 2 entries,
        // `(1001, ch)` each, each a `Rects` of the stamped footprint (no
        // `Full`) — and the next flush makes the cache equal the oracle
        // with the edited mask.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        for channel in ["roughness", "metallic"] {
            let bytes: Vec<u8> = (0..512 * 512).flat_map(|_| [200, 200, 200, 255]).collect();
            state
                .publish_plane(PlaneOwner::Layer(5), 1001, channel, 512, 512, bytes)
                .expect("publishes");
        }
        let snapshot = CompositeSnapshot {
            layers: vec![
                base_row(),
                SnapshotLayer {
                    owner: PlaneOwner::Layer(5),
                    layer_id: Some(5),
                    kind: SnapshotLayerKind::Material,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                    visible: true,
                    mask_enabled: true,
                    material_channels: Some(vec![
                        SnapshotMaterialChannel {
                            channel: "roughness".to_string(),
                            blend: BlendMode::Normal,
                            opacity: 1.0,
                        },
                        SnapshotMaterialChannel {
                            channel: "metallic".to_string(),
                            blend: BlendMode::Normal,
                            opacity: 1.0,
                        },
                    ]),
                },
            ],
            base_visible: true,
            displayed: vec![
                (1001, "roughness".to_string()),
                (1001, "metallic".to_string()),
            ],
        };
        // Publish dirt flushes away first (the mask stroke must leave
        // `Rects`, not ride a stale `Full`).
        state.flush_composite(&snapshot);
        assert!(state.cache_dirty.is_empty());
        // The mask stroke (a fill's mask is legitimate to paint while the
        // fill itself refuses strokes — the gate bypass).
        state.set_mask_edit_layer(Some(5));
        drive_converged_diagonal(&mut state);
        state.set_mask_edit_layer(None);
        let mut keys: Vec<(u16, String)> = state.cache_dirty.keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                (1001, "metallic".to_string()),
                (1001, "roughness".to_string())
            ]
        );
        for key in &keys {
            assert!(
                matches!(state.cache_dirty.get(key), Some(DirtyRegion::Rects(_))),
                "{key:?} must be a footprint Rects, never Full"
            );
        }
        // The next flush folds the edited mask: cache equals the oracle
        // within the twin tolerance (one masked entry → no seed → one
        // dispatch → ≤ 1 LSB; fractional mask coverage can land
        // arbitrarily close to a rounding boundary, so exact equality
        // would pin implementation-defined UNORM rounding). A
        // mask-ignoring flush would drift up to 200 LSB — this still
        // proves the mask folded.
        state.flush_composite(&snapshot);
        let mask = state
            .layer_mask(5, 1001)
            .expect("mask plane exists")
            .to_vec();
        for channel in ["roughness", "metallic"] {
            let cached = state.read_composite(1001, channel).expect("cache reads");
            let plane_bytes: Vec<u8> = (0..512 * 512).flat_map(|_| [200, 200, 200, 255]).collect();
            let oracle = {
                use umber_gpu::compositor::{composite_stack_cpu, CpuStackLayer};
                composite_stack_cpu(
                    &[CpuStackLayer {
                        blend_mode: BlendMode::Normal,
                        opacity: 1.0,
                        visible: true,
                        rgba: Some(&plane_bytes),
                        mask: Some(&mask),
                        premultiplied: true,
                    }],
                    512,
                    512,
                )
                .expect("oracle composites")
            };
            for (i, (got, want)) in cached.data.iter().zip(oracle.iter()).enumerate() {
                let drift = (*got as i16 - *want as i16).abs();
                assert!(drift <= 1, "{channel} texel {i}: got {got}, want {want}");
            }
        }
    }

    #[test]
    fn budget_refused_stroke_keeps_painting_materialized_planes() {
        // §8 T33 (stroke half). With the hard cap lowered so one more
        // `(plane + cache)` fits but two do not, a stroke paints its
        // already-materialized tile and refuses the next tile with a typed
        // `PlaneBudget` — never a silent drop, never a crash.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // Two UDIM tiles: the diagonal crosses from 1001 into 1002.
        // Two triangles, one per UDIM tile (v1's `tile_of_triangle`
        // keys on each triangle's FIRST vertex, so spanning requires
        // two tris — see `udim.rs`): first verts at u 0.2 and u 1.2
        // put them in 1001 and 1002, making `multi_tile` true.
        state.set_mesh(&umber_mesh::MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.2, 0.2], [1.2, 0.2], [0.2, 0.8], [1.2, 0.8]],
            indices: vec![0, 1, 3, 1, 2, 3],
            material_names: vec![],
        });
        state.set_active_target(PlaneOwner::Layer(1));
        state.sync_active_target(PlaneOwner::Layer(1), Ok(()), 0);
        // Base (1 MiB) + L1's plane + cache (2 MiB) fit; one more pair does
        // not: the cap sits 1 byte under the next materialization.
        state.set_plane_hard_cap_bytes(4 * 1_048_576 - 1);
        state.begin_stroke(emath::pos2(0.2, 0.2));
        for i in 1..=24 {
            let t = i as f32 * 0.05;
            state.extend_stroke(emath::pos2(0.2 + t, 0.2));
        }
        state.end_stroke();
        assert!(
            state.planes.contains_key(&(
                PlaneOwner::Layer(1),
                1001,
                BASE_COLOR_CHANNEL.to_string()
            )),
            "the materialized tile keeps painting"
        );
        assert!(
            !state.planes.contains_key(&(
                PlaneOwner::Layer(1),
                1002,
                BASE_COLOR_CHANNEL.to_string()
            )),
            "the next tile is refused, never half-created"
        );

        assert!(
            matches!(
                state.stroke_refusal(),
                Some(PaintRefusal::PlaneBudget { .. })
            ),
            "the refusal is typed and loud"
        );
        assert!(
            state.staged_batch_count() > 0,
            "the stroke staged on its live tile"
        );
        let l1 = state
            .read_plane(PlaneOwner::Layer(1), 1001, BASE_COLOR_CHANNEL)
            .expect("L1 reads");
        assert_ne!(l1.data, vec![0u8; 512 * 512 * 4]);
    }

    #[test]
    fn owner_deleted_mid_stroke_retires_on_end_and_discards_capture() {
        // §8 T34 [m7]. Begin on L1, remove L1 while stroking, run frames:
        // the census never retires the live owner's planes; `end_stroke`
        // retires them, discards the capture (history length unchanged),
        // and logs.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.set_active_target(PlaneOwner::Layer(1));
        state.sync_active_target(PlaneOwner::Layer(1), Ok(()), 0);
        state.begin_stroke(emath::pos2(0.2, 0.2));
        // A run of extends (no `end_stroke`): the conditioner converges
        // and stages, so the live stroke owns planes before the delete.
        for i in 1..=10 {
            let t = i as f32 * 0.02;
            state.extend_stroke(emath::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        // The shell's census sees L1 vanish — queued, never pumped while
        // the stroke lives.
        state.note_removed_layers(&[(1, true)]);
        for _ in 0..3 {
            state.flush_composite(&base_snapshot(1001, BASE_COLOR_CHANNEL));
        }
        assert!(
            state.planes.contains_key(&(
                PlaneOwner::Layer(1),
                1001,
                BASE_COLOR_CHANNEL.to_string()
            )),
            "the live stroke's planes are never retired under it"
        );
        // Queued, not yet retired (the pump skips the live owner):
        // the tombstone map stays empty until end_stroke retires.
        assert!(!state.retired.contains_key(&1));
        let depth = state.stroke_len();
        state.end_stroke();
        assert!(
            state.retired.contains_key(&1),
            "end_stroke retires the gone owner's planes"
        );
        assert!(!state
            .planes
            .keys()
            .any(|(owner, _, _)| *owner == PlaneOwner::Layer(1)));
        assert_eq!(state.stroke_len(), depth, "the capture is discarded");
    }

    // --- Paint-what-you-see projection (§2's P1 row) ---
    //
    // The viewport raycasts each pointer to a hit UV (or a miss); these
    // tests pin the session side of that contract. The ray side
    // (`Viewport::pick_uv`: a known ray hits the known UV, corners miss)
    // is pinned in `viewport.rs`.

    #[test]
    fn projection_default_is_screen_and_routes_the_viewport() {
        // Screen is today's behavior (both surfaces paint), so existing
        // sessions are untouched; Uv parks viewport painting (orbit).
        assert_eq!(PaintProjection::default(), PaintProjection::Screen);
        assert!(PaintProjection::Screen.viewport_paints());
        assert!(!PaintProjection::Uv.viewport_paints());
    }

    /// Drives `points` through the Screen entry points: the first through
    /// `begin_screen_point`, the rest through `extend_screen_point`.
    fn drive_screen(state: &mut PaintState, points: &[Option<[f32; 2]>]) {
        let mut first = true;
        for point in points {
            let hit = point.map(|[x, y]| emath::pos2(x, y));
            if first {
                state.begin_screen_point(hit);
                first = false;
            } else {
                state.extend_screen_point(hit);
            }
        }
    }

    #[test]
    fn screen_miss_opens_no_stroke_and_stages_nothing() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.begin_screen_point(None);
        assert!(!state.is_stroking(), "a miss must not open a stroke");
        state.extend_screen_point(None);
        assert!(!state.is_stroking());
        assert_eq!(state.staged_batch_count(), 0, "no hits, no stages");
        assert_eq!(state.staged_dab_count(), 0);
        state.end_stroke();
    }

    #[test]
    fn screen_hit_after_opening_miss_starts_the_stroke_there() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // The drag opens off the mesh, then moves onto it: the stroke
        // starts at the first hit, with no orphan state from the misses.
        drive_screen(&mut state, &[None, None, Some([0.3, 0.3])]);
        assert!(state.is_stroking());
        state.end_stroke();
    }

    #[test]
    fn screen_misses_leave_no_trace_in_the_conditioned_stream() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Misses never touch the conditioner, so a screen stroke with
        // interleaved misses stages EXACTLY what the hits-only UV stroke
        // stages (same batches, same dabs, same order).
        let hits = [[0.3, 0.3], [0.32, 0.32], [0.34, 0.34], [0.36, 0.36]];
        let mut direct = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        direct.begin_stroke(emath::pos2(hits[0][0], hits[0][1]));
        for [x, y] in &hits[1..] {
            direct.extend_stroke(emath::pos2(*x, *y));
        }
        direct.end_stroke();

        let mut screen =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        drive_screen(
            &mut screen,
            &[
                Some(hits[0]),
                None,
                Some(hits[1]),
                None,
                None,
                Some(hits[2]),
                Some(hits[3]),
            ],
        );
        assert!(
            screen.is_stroking(),
            "hits keep the stroke alive across misses"
        );
        screen.end_stroke();
        assert_eq!(
            screen.staged_batch_count(),
            direct.staged_batch_count(),
            "misses stage nothing"
        );
        assert_eq!(
            screen.staged_dab_count(),
            direct.staged_dab_count(),
            "the hit stream conditions identically"
        );
    }

    #[test]
    fn projection_sync_applies_between_strokes_only() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        assert_eq!(state.projection(), PaintProjection::Screen);
        state.sync_projection(PaintProjection::Uv);
        assert_eq!(state.projection(), PaintProjection::Uv);
        // A mid-stroke swap would split one stroke across two surfaces.
        state.begin_stroke(emath::pos2(0.5, 0.5));
        state.sync_projection(PaintProjection::Screen);
        assert_eq!(state.projection(), PaintProjection::Uv);
        state.end_stroke();
        state.sync_projection(PaintProjection::Screen);
        assert_eq!(state.projection(), PaintProjection::Screen);
    }

    #[test]
    fn screen_stroke_inherits_seam_mirrors() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Same march as `seam_stroke_stages_original_plus_mirror_near_seam`
        // but through the Screen entry points: the resolved hit UVs fan
        // out through the shared `push_event_styled` seam path, so a
        // near-seam screen stroke stages original + mirror per event.
        let start = NEAR_SEAM_UV;
        let step = [0.02, 0.02];
        let march: Vec<Option<[f32; 2]>> = (0..=15)
            .map(|i| Some([start[0] + step[0] * i as f32, start[1] + step[1] * i as f32]))
            .collect();

        let mut base = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        base.set_mesh(&single_island_quad());
        drive_screen(&mut base, &march);
        let base_converged = base.staged_batch_count();
        assert!(base_converged >= 1);
        base.extend_screen_point(Some(emath::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        )));
        assert_eq!(
            base.staged_batch_count() - base_converged,
            1,
            "seam-free screen extend stages exactly the original"
        );
        base.end_stroke();

        let mut seam =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        seam.set_mesh(&two_island_quad());
        drive_screen(&mut seam, &march);
        let seam_converged = seam.staged_batch_count();
        assert!(
            seam_converged > base_converged,
            "near-seam screen stroke must stage MORE than the baseline"
        );
        seam.extend_screen_point(Some(emath::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        )));
        assert_eq!(
            seam.staged_batch_count() - seam_converged,
            2,
            "near-seam screen extend stages original + mirror"
        );
        seam.end_stroke();
    }

    // --- Seam-aware wiring tests (slice 3) ---
    //
    // Fixtures mirror `umber_mesh::seam`'s split-quad layout exactly: quad
    // corners A=(0,0,0) B=(1,0,0) C=(1,1,0) D=(0,1,0), diagonal A-C. The
    // two-island variant duplicates the diagonal's vertices with
    // island-offset UVs, so the seam's `uv_a` segment is (0,0)→(1,1) and
    // `uv_b` is (10,10)→(11,11).

    /// Quad with continuous UVs: no seam edges.
    fn single_island_quad() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            material_names: vec![],
        }
    }

    /// Same quad with the diagonal A-C cut across two UV islands: exactly
    /// one seam edge (`uv_a` (0,0)→(1,1), `uv_b` (10,10)→(11,11)).
    fn two_island_quad() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [0.0, 0.0, 0.0], // 0: A (tri 0)
                [1.0, 0.0, 0.0], // 1: B
                [1.0, 1.0, 0.0], // 2: C (tri 0)
                [0.0, 0.0, 0.0], // 3: A (tri 1, duplicated)
                [1.0, 1.0, 0.0], // 4: C (tri 1, duplicated)
                [0.0, 1.0, 0.0], // 5: D
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uvs: vec![
                [0.0, 0.0],   // A in tri 0
                [1.0, 0.0],   // B
                [1.0, 1.0],   // C in tri 0
                [10.0, 10.0], // A in tri 1 (other island)
                [11.0, 11.0], // C in tri 1 (other island)
                [11.0, 10.0], // D
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec![],
        }
    }

    /// A UV on the `uv_a` diagonal, ~0.021 UV units off the seam line:
    /// inside the mirror gate (radius 36/512 ≈ 0.070).
    const NEAR_SEAM_UV: [f32; 2] = [0.5, 0.53];
    /// A UV ~0.57 UV units from either island's segment: outside the gate.
    const FAR_FROM_SEAM_UV: [f32; 2] = [0.9, 0.1];

    /// The seam fixtures' gate radius: the mirror-gate math at an explicit
    /// 512 texels-per-UV (the wiring tests construct at 512 via
    /// [`PaintState::with_target_size`] so `NEAR_SEAM_UV`'s ~0.021-UV seam
    /// offset stays inside the gate — at the 4096 default the gate would
    /// be 8× tighter in UV and the same fixtures would read as seam-free).
    fn gate_radius() -> f32 {
        mirror_radius_uv(512.0)
    }

    #[test]
    fn mirror_radius_is_footprint_times_margin() {
        assert_eq!(SEAM_MIRROR_MARGIN, 1.5);
        assert!((gate_radius() - 36.0 / 512.0).abs() < 1e-6);
    }

    #[test]
    fn seam_gate_fires_near_seam_and_not_far() {
        let graph = build_seam_graph(&two_island_quad());
        assert_eq!(
            graph.edges.len(),
            1,
            "fixture must carry exactly one seam edge, got {:?}",
            graph.edges
        );
        let r = gate_radius();
        // The gate must fire near the seam's uv_a segment ...
        assert!(
            uv_near_seam(&graph, NEAR_SEAM_UV, r),
            "UV {NEAR_SEAM_UV:?} ~0.021 from the seam must be inside radius {r}"
        );
        // ... and stay silent far from both islands' segments. BOTH
        // directions are asserted: a gate stuck true/false fails here.
        assert!(
            !uv_near_seam(&graph, FAR_FROM_SEAM_UV, r),
            "UV {FAR_FROM_SEAM_UV:?} ~0.57 from the seam must be outside radius {r}"
        );
        // Seam-free mesh: the gate never fires (empty edge list).
        let plain = build_seam_graph(&single_island_quad());
        assert!(
            plain.edges.is_empty(),
            "single-island quad must yield zero seam edges, got {:?}",
            plain.edges
        );
        assert!(!uv_near_seam(&plain, NEAR_SEAM_UV, r));
        assert!(!uv_near_seam(&plain, FAR_FROM_SEAM_UV, r));
    }

    #[test]
    fn mirror_positions_fans_out_near_seam_only() {
        // Pins the exact call `push_event_styled` makes: one position, the
        // wiring radius, the two-island graph.
        let graph = build_seam_graph(&two_island_quad());
        let r = gate_radius();
        let near = mirror_positions(&[NEAR_SEAM_UV], &graph, r);
        assert_eq!(
            near.len(),
            1,
            "near-seam position must yield exactly one mirror, got {near:?}"
        );
        assert_eq!(near[0].source, 0);
        // Mirror lands in the OTHER island (`uv_b` lives at UV ~10–11).
        assert!(
            near[0].mirrored[0] > 9.0 && near[0].mirrored[1] > 9.0,
            "mirror must land in the neighbor island, got {:?}",
            near[0].mirrored
        );
        // Offset magnitude preserved across the map (La == Lb here): the
        // mirror's distance from the `uv_b` segment equals the source's
        // distance from `uv_a`.
        let d_src = dist_to_segment(NEAR_SEAM_UV, [0.0, 0.0], [1.0, 1.0]);
        let d_mir = dist_to_segment(near[0].mirrored, [10.0, 10.0], [11.0, 11.0]);
        assert!(
            (d_src - d_mir).abs() < 1e-4,
            "mirror offset {d_mir} must match source offset {d_src}"
        );
        // Far position: no mirrors (the zero-cost contract at the math
        // layer — a far event fans out to nothing).
        let far = mirror_positions(&[FAR_FROM_SEAM_UV], &graph, r);
        assert!(
            far.is_empty(),
            "far position must yield zero mirrors, got {far:?}"
        );
    }

    /// Requests a paint-capable device (graceful skip where no wgpu
    /// adapter exists — the same pattern as `umber_gpu::paint`'s GPU
    /// tests). Returns `None` when the test must be skipped.
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return None;
        };
        if !adapter
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ..Default::default()
        }))
        .ok()
    }

    /// Drives a stroke that stays inside the mirror gate the whole time:
    /// `begin` at `start`, then `steps` extends marching `step` per event.
    fn drive_stroke(state: &mut PaintState, start: [f32; 2], step: [f32; 2], steps: usize) {
        state.begin_stroke(emath::pos2(start[0], start[1]));
        for i in 1..=steps {
            state.extend_stroke(emath::pos2(
                start[0] + step[0] * i as f32,
                start[1] + step[1] * i as f32,
            ));
        }
    }

    #[test]
    fn seam_stroke_stages_original_plus_mirror_near_seam() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // March along the seam diagonal (stays ~0.021 off the line, gate
        // radius ~0.070); 15 events converge the conditioner, then ONE
        // measured extend pins the steady-state per-event cost.
        let start = NEAR_SEAM_UV;
        let step = [0.02, 0.02];

        // Baseline: seam-free mesh. The measured extend stages exactly the
        // original — 1 batch.
        let mut base = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        base.set_mesh(&single_island_quad());
        drive_stroke(&mut base, start, step, 15);
        let base_converged = base.staged_batch_count();
        assert!(
            base_converged >= 1,
            "converged baseline stroke must stage something (got {base_converged})"
        );
        base.extend_stroke(emath::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        assert_eq!(
            base.staged_batch_count() - base_converged,
            1,
            "seam-free extend stages exactly the original (1 batch)"
        );

        // Seam mesh, identical stroke: the measured extend stages original
        // + mirror — 2 batches (the mirror recurses with expand=false, so
        // no further fan-out: recursion terminates).
        let mut seam =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        seam.set_mesh(&two_island_quad());
        drive_stroke(&mut seam, start, step, 15);
        let seam_converged = seam.staged_batch_count();
        assert!(
            seam_converged > base_converged,
            "near-seam stroke must stage MORE than the seam-free baseline \
             (seam {seam_converged} vs base {base_converged})"
        );
        seam.extend_stroke(emath::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        assert_eq!(
            seam.staged_batch_count() - seam_converged,
            2,
            "near-seam extend stages original + mirror (2 batches)"
        );
    }

    #[test]
    fn stroke_far_from_seam_pays_nothing() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // March through (0.9,0.1)→(0.58,0.42): closest approach ~0.11 UV,
        // still outside the ~0.070 gate the whole way.
        let start = FAR_FROM_SEAM_UV;
        let step = [-0.02, 0.02];

        let mut base = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        base.set_mesh(&single_island_quad());
        drive_stroke(&mut base, start, step, 15);
        let base_converged = base.staged_batch_count();
        base.extend_stroke(emath::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        let base_total = base.staged_batch_count();

        // Same stroke on the seam mesh: the stroke-start arm stays off, the
        // mirror call never runs, so the event stream — and every staged
        // batch — is IDENTICAL to the seam-free baseline (structural
        // equality, not just determinism: `push_event_styled` never consults
        // the graph when `stroke_near_seam` is false).
        let mut far = PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        far.set_mesh(&two_island_quad());
        drive_stroke(&mut far, start, step, 15);
        assert_eq!(
            far.staged_batch_count(),
            base_converged,
            "far-from-seam convergence must match the baseline exactly"
        );
        far.extend_stroke(emath::pos2(
            start[0] + step[0] * 16.0,
            start[1] + step[1] * 16.0,
        ));
        assert_eq!(
            far.staged_batch_count(),
            base_total,
            "far-from-seam totals must match the baseline exactly (zero-cost contract)"
        );
        assert_eq!(
            far.staged_batch_count() - base_converged,
            base_total - base_converged,
            "far-from-seam extend stages exactly what the baseline stages"
        );
    }

    // --- Retouch tools (smudge + clone) ---

    #[test]
    fn smudge_stroke_writes_back_and_undoes_as_one_entry() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        // A vertical paint stroke gives the smudge an edge to drag. Since the
        // stroke-undo work (critical 3) this setup stroke records its own
        // history entry, and it STAYS: entries stack, one per stroke — the
        // same contract two_paint_strokes_undo_reverts_only_the_latest pins
        // for plain paint. Undoing it here would blank the canvas and leave
        // the smudge nothing to drag (the 9e4fb9e lesson: the smudge then
        // moves nothing and the write-back assert dies).
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        state.process_pending().expect("drain succeeds");
        assert!(
            state.can_undo_stroke(),
            "the setup stroke records its entry (one per stroke)"
        );
        let painted_batches = state.staged_batch_count();
        let before = state
            .paint_target()
            .read_back_rgba8(&device, &queue)
            .expect("readback");

        let mut tool = BrushTool::Smudge(umber_brush::SmudgeParams {
            strength: 1.0,
            smear: 0.5,
        });
        state.sync_tool(&mut tool);
        // A horizontal smudge across the painted line.
        drive_stroke(&mut state, [0.2, 0.5], [0.01, 0.0], 20);
        state.end_stroke();
        assert_eq!(
            state.staged_batch_count(),
            painted_batches,
            "retouch dabs never stage GPU splats"
        );
        assert!(state.can_undo_stroke(), "the smudge stroke is one entry");
        state.process_pending().expect("drain succeeds");
        // [M7] The shell's END-of-update flush, with the shell's real
        // snapshot shape: the Base row present, Base visible, and
        // `(1001, baseColor)` displayed. A row-less snapshot composites a
        // TRANSPARENT stack over the dirty rect (the readback would differ
        // for the wrong reason); a default one also hides Base.
        state.flush_composite(&base_snapshot(1001, BASE_COLOR_CHANNEL));
        state.process_pending().expect("drain succeeds");
        let after = state
            .paint_target()
            .read_back_rgba8(&device, &queue)
            .expect("readback");
        assert_ne!(after, before, "the smudge must reach the GPU target");

        // The smudge is the SECOND entry: one per stroke, entries stack
        // (critical 3's contract — see the sibling test at :2360). One undo
        // reverts the smudge only; the setup stroke's entry remains.
        // The strokes DO intersect (a cross), but the cumulative merge's
        // size guard keeps them apart: at 512, r = 24, the vertical rect is
        // ~50×194 and the horizontal ~146×48, so their bounding box
        // (~146×194 ≈ 28k texels) is ~1.7× the pair (~16.7k) — a cross
        // never coalesces (`retouch`'s module docs).
        assert!(state.undo_stroke(), "the smudge entry undoes");
        assert!(
            state.can_undo_stroke(),
            "the setup stroke's entry remains beneath the smudge's"
        );
        // [M7] again: the undo's writes land on the plane; the flush
        // refreshes the cache before the readback compares.
        state.flush_composite(&base_snapshot(1001, BASE_COLOR_CHANNEL));
        state.process_pending().expect("drain succeeds");
        let undone = state
            .paint_target()
            .read_back_rgba8(&device, &queue)
            .expect("readback");
        assert_eq!(undone, before, "undo restores the pre-smudge texels");
    }

    // --- Paint-stroke undo (CORE-LOOP CRITICAL 3) ---

    /// A paint stroke is ONE history entry: undo restores the exact
    /// pre-stroke bytes, redo the exact post-stroke bytes.
    #[test]
    fn paint_stroke_undoes_and_redoes_byte_exact() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        let blank = read_back(&mut state, &device, &queue);
        assert!(!state.can_undo_stroke(), "a fresh session has no entries");

        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(
            state.staged_dab_count() > 0,
            "the stroke must stage dabs (got {})",
            state.staged_dab_count()
        );
        assert!(state.can_undo_stroke(), "the paint stroke is one entry");
        let painted = read_back(&mut state, &device, &queue);
        assert_ne!(painted, blank, "the stroke must reach the GPU target");

        assert!(state.undo_stroke());
        assert!(!state.can_undo_stroke(), "one stroke, one entry");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "undo restores the pre-stroke texels"
        );

        assert!(state.redo_stroke());
        assert_eq!(
            read_back(&mut state, &device, &queue),
            painted,
            "redo restores the post-stroke texels"
        );
        assert!(!state.can_redo_stroke());
    }

    /// Two paint strokes are two entries: one undo reverts only the
    /// latest, leaving the first stroke's paint byte-identical in place.
    #[test]
    fn two_paint_strokes_undo_reverts_only_the_latest() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        let blank = read_back(&mut state, &device, &queue);

        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        let first = read_back(&mut state, &device, &queue);
        assert_ne!(first, blank, "the first stroke must paint");

        // Far from the first stroke: ~200 texels apart at r = 24, so the
        // second stroke's dirty rect cannot cover the first stroke's paint.
        drive_stroke(&mut state, [0.7, 0.7], [0.0, 0.02], 15);
        state.end_stroke();
        let second = read_back(&mut state, &device, &queue);
        assert_ne!(second, first, "the second stroke must paint");

        assert!(state.undo_stroke());
        assert!(state.can_undo_stroke(), "the first stroke's entry remains");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            first,
            "one undo reverts only the latest stroke"
        );

        assert!(state.undo_stroke());
        assert!(!state.can_undo_stroke(), "two strokes, two entries");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "the second undo reaches the blank target"
        );

        assert!(state.redo_stroke());
        assert_eq!(read_back(&mut state, &device, &queue), first);
        assert!(state.redo_stroke());
        assert_eq!(read_back(&mut state, &device, &queue), second);
        assert!(!state.can_redo_stroke());
    }

    /// Stroke time-travel end to end: two far-apart paint strokes queue
    /// exactly two commits (`cursor`/`depth` pin the journal on both sides
    /// of each push), and jumping the stroke journal travels exactly n
    /// single steps — byte-identical to stepwise undo/redo. (The commits'
    /// mapping into History-window rows lives in `history_panel`'s
    /// headless feed tests; this crate's library target has no history
    /// module, so the row half cannot be driven from here.)
    #[test]
    fn stroke_commits_queue_exact_pairs_and_jump_travels_exactly() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        assert!(state.take_stroke_commits().is_empty());
        let blank = read_back(&mut state, &device, &queue);

        // The `two_paint_strokes` spacing (~200 texels apart at r = 24):
        // far enough that the second cannot merge into the first, so each
        // stroke is its own entry — depths 1 then 2.
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        let first = read_back(&mut state, &device, &queue);
        assert_ne!(first, blank, "the first stroke must paint");
        drive_stroke(&mut state, [0.7, 0.7], [0.0, 0.02], 15);
        state.end_stroke();
        let second = read_back(&mut state, &device, &queue);
        assert_ne!(second, first, "the second stroke must paint");
        assert_eq!(state.stroke_len(), 2);

        assert_eq!(
            state.take_stroke_commits(),
            vec![
                StrokeCommit {
                    cursor: 0,
                    depth: 1
                },
                StrokeCommit {
                    cursor: 1,
                    depth: 2
                },
            ]
        );
        assert!(state.take_stroke_commits().is_empty());

        // The jump is n single steps: depth 2 -> 0 is 2 undos, restoring
        // the exact blank bytes; back up is 2 redos.
        assert_eq!(state.jump_to_stroke_depth(0), 2);
        assert_eq!(read_back(&mut state, &device, &queue), blank);
        assert!(!state.can_undo_stroke());
        assert_eq!(state.jump_to_stroke_depth(2), 2);
        assert_eq!(read_back(&mut state, &device, &queue), second);
        assert!(!state.can_redo_stroke());
        // Already there: no steps.
        assert_eq!(state.jump_to_stroke_depth(2), 0);
    }

    /// The commit cursor tracks the live journal, not the wall clock: two
    /// strokes, one undone, then a fresh third stroke commits at cursor 1
    /// (the pre-push depth) back to depth 2 — the pair the drain's
    /// redo-future truncation reads.
    #[test]
    fn fresh_stroke_over_undone_state_commits_at_the_live_cursor() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        let blank = read_back(&mut state, &device, &queue);

        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        drive_stroke(&mut state, [0.7, 0.7], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(state.undo_stroke());
        assert_eq!(state.stroke_len(), 1);
        // Far from both predecessors, so no merge: the commit's cursor is
        // the pre-push depth (1), its depth the post-push depth (2).
        drive_stroke(&mut state, [0.75, 0.25], [0.0, 0.02], 15);
        state.end_stroke();
        let third = read_back(&mut state, &device, &queue);
        assert_ne!(third, blank, "the third stroke must paint");

        assert_eq!(
            state.take_stroke_commits(),
            vec![
                StrokeCommit {
                    cursor: 0,
                    depth: 1
                },
                StrokeCommit {
                    cursor: 1,
                    depth: 2
                },
                StrokeCommit {
                    cursor: 1,
                    depth: 2
                },
            ]
        );
        // Both live entries undo to the exact blank canvas.
        assert_eq!(state.jump_to_stroke_depth(0), 2);
        assert_eq!(read_back(&mut state, &device, &queue), blank);
    }

    /// A resize clears the queued commits with the entries and bumps the
    /// epoch, so the next drain can neither record evicted strokes nor
    /// alias the new history's depths. (The epoch's row-clearing half is
    /// pinned in `history_panel`'s headless tests.)
    #[test]
    fn resize_clears_pending_commits_and_bumps_epoch() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        assert_eq!(state.stroke_epoch(), 0);

        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert_eq!(state.stroke_len(), 1);

        let active = state.active_set().to_string();
        state.resize_set(&active, 256, 256);
        assert_eq!(state.stroke_len(), 0);
        assert_eq!(state.stroke_epoch(), 1);
        assert!(state.take_stroke_commits().is_empty());

        // New strokes record against the new epoch at depth 1 again.
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert_eq!(
            state.take_stroke_commits(),
            vec![StrokeCommit {
                cursor: 0,
                depth: 1
            }]
        );
    }

    // --- Paint-stroke pressure (CORE-LOOP CRITICAL 5) ---

    /// Pressure shapes dabs: a half-pressure stroke paints lighter than a
    /// full-pressure one at the same UVs. The chain is real end to end —
    /// `begin/extend_screen_point_pressured` → `begin_stroke_styled` →
    /// the conditioner → `DabPlan.alpha_multiplier` → the dab alpha
    /// (`pressure_gamma` shaping) → the GPU splat → the readback. The
    /// test drives the SAME UV path twice on fresh states, once at 1.0
    /// pressure and once at 0.3, and pins that the light stroke's texels
    /// carry smaller alpha than the heavy stroke's at the same texel.
    #[test]
    fn pressured_stroke_paints_lighter_than_full_pressure() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let paint_at = |pressure: f32| -> Vec<u8> {
            let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint state builds");
            state.begin_screen_point_pressured(Some(emath::pos2(0.3, 0.3)), pressure);
            for i in 1..=15 {
                state.extend_screen_point_pressured(
                    Some(emath::pos2(0.3, 0.3 + 0.02 * i as f32)),
                    pressure,
                );
            }
            state.end_stroke();
            read_back(&mut state, &device, &queue)
        };
        let heavy = paint_at(1.0);
        let light = paint_at(0.3);
        // The stroke's core texel: on the driven line, at the surface
        // center of the first dab cluster. The heavy stroke's alpha
        // channel must exceed the light stroke's there.
        let w = 512usize;
        let texel = |bytes: &[u8]| -> u8 { bytes[(256 * w + 154) * 4 + 3] };
        assert!(texel(&heavy) > 0, "the heavy stroke must paint alpha");
        assert!(
            texel(&heavy) > texel(&light),
            "half pressure must paint lighter: heavy alpha {} vs light {}",
            texel(&heavy),
            texel(&light)
        );
    }

    // --- Symmetry ---

    /// Test (b), end to end: with the mirror on, the same stroke stages
    /// the same `Stage` batches (one per event — siblings ride in the
    /// original's batch) carrying exactly twice the dabs (the stroke stays
    /// clear of the axis, so no sibling dedups); with it off, the staging
    /// is unchanged. The conditioner sees only the pointer either way.
    #[test]
    fn mirrored_stroke_stages_twice_the_dabs_in_the_same_batches() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let start = [0.2, 0.3];
        let step = [0.0, 0.02];

        let mut off = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        drive_stroke(&mut off, start, step, 15);
        off.end_stroke();
        assert!(off.staged_dab_count() > 0);

        let mut on = PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let mut settings = SymmetrySettings::default();
        settings.mirror.enabled = true;
        on.sync_symmetry(&mut settings);
        assert!(on.symmetry().mirror.enabled);
        drive_stroke(&mut on, start, step, 15);
        on.end_stroke();
        assert_eq!(on.staged_batch_count(), off.staged_batch_count());
        assert_eq!(on.staged_dab_count(), 2 * off.staged_dab_count());

        // Turning it back off restores single staging for the next stroke.
        settings.mirror.enabled = false;
        on.sync_symmetry(&mut settings);
        let (batches, dabs) = (on.staged_batch_count(), on.staged_dab_count());
        drive_stroke(&mut on, start, step, 15);
        on.end_stroke();
        assert_eq!(on.staged_batch_count() - batches, off.staged_batch_count());
        assert_eq!(on.staged_dab_count() - dabs, off.staged_dab_count());
    }

    #[test]
    fn manipulator_offset_is_handed_back_to_the_panel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let mut panel = SymmetrySettings::default();
        panel.mirror.enabled = true;
        state.sync_symmetry(&mut panel);
        // The UV view drags the line: the panel adopts it next sync...
        state.set_mirror_offset(0.3);
        state.sync_symmetry(&mut panel);
        assert_eq!(panel.mirror.offset, 0.3);
        // ... and afterwards the panel leads again.
        panel.mirror.offset = 0.6;
        state.sync_symmetry(&mut panel);
        assert_eq!(state.symmetry().mirror.offset, 0.6);
        // Out-of-tile drags clamp to the tile.
        state.set_mirror_offset(1.7);
        assert_eq!(state.symmetry().mirror.offset, 1.0);
    }

    // --- Quick mask ---
    //
    // Every mask here is painted with a hard, full-alpha brush, so its
    // values are exactly 0 or 1 and "blocked" (mask == 0.0, uploaded as
    // byte 0) is a crisp texel set the before/after compares can use.
    // Probe texels sit on row v = 0.5 (y = 256): x = 102 is u ≈ 0.2,
    // x = 358 is u ≈ 0.7, x = 409 is u = 0.2's mirror about u = 0.5.

    fn hard_brush() -> BrushParams {
        BrushParams {
            alpha: 1.0,
            hardness: 1.0,
            ..BrushParams::default()
        }
    }

    /// Drains every queue, then reads tile 1001 back.
    fn read_back(state: &mut PaintState, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<u8> {
        state.process_pending().expect("drain succeeds");
        state
            .paint_target()
            .read_back_rgba8(device, queue)
            .expect("readback")
    }

    fn texel_at(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let i = (y * width + x) as usize * 4;
        [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
    }

    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    /// Texels of tile 1001 that differ between `a` and `b`, split by the
    /// quick mask there: `(blocked, allowed)`.
    fn changed_by_mask(state: &PaintState, a: &[u8], b: &[u8]) -> (usize, usize) {
        let mask = state.quick_mask();
        let width = state.target_size().0;
        let mut out = (0, 0);
        for (i, (ta, tb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
            if ta == tb {
                continue;
            }
            let (x, y) = (i as u32 % width, i as u32 / width);
            if mask.sample(FIRST_TILE, x, y) == 0.0 {
                out.0 += 1;
            } else {
                out.1 += 1;
            }
        }
        out
    }

    /// Mask mode on, one hard white stroke allowing u ≈ 0.6..0.9 along
    /// v = 0.5, mask mode off. Everything else stays blocked (empty).
    fn mask_right_window(state: &mut PaintState) {
        assert!(state.toggle_mask_mode());
        drive_stroke(state, [0.6, 0.5], [0.02, 0.0], 15);
        state.end_stroke();
        assert!(!state.toggle_mask_mode());
    }

    /// Tests (a), (c), (e), (f) end to end:
    /// * (c) a mask-mode stroke stages nothing and leaves the layer
    ///   byte-identical; it edits the mask and uploads it;
    /// * (a) after leaving mask mode, a paint stroke across the whole row
    ///   leaves every blocked texel byte-identical and paints the window;
    /// * (e) deactivating stops gating (the same stroke paints the blocked
    ///   probe) and the mask survives exit and re-entry;
    /// * (f) clear zeroes it.
    #[test]
    fn quick_mask_gates_paint_and_mask_mode_paints_only_the_mask() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        assert!(state.quick_mask().coverage(FIRST_TILE).is_none());
        assert_eq!(state.mask_upload_count(), 0, "no mask, no upload");

        // (c)
        mask_right_window(&mut state);
        assert_eq!(
            state.staged_batch_count(),
            0,
            "a mask stroke stages nothing"
        );
        assert_eq!(state.staged_dab_count(), 0);
        assert!(state.mask_upload_count() > 0, "the mask reached the GPU");
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "the layer is untouched by a mask stroke"
        );
        let mask = state.quick_mask();
        assert_eq!(
            mask.sample(FIRST_TILE, 358, 256),
            1.0,
            "white allowed the window"
        );
        assert_eq!(
            mask.sample(FIRST_TILE, 102, 256),
            0.0,
            "the rest stays blocked"
        );
        let painted_mask = mask.coverage(FIRST_TILE).expect("plane").to_vec();

        // (a)
        assert!(state.quick_mask().gates());
        drive_stroke(&mut state, [0.1, 0.5], [0.05, 0.0], 16);
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let (blocked, allowed) = changed_by_mask(&state, &blank, &gated);
        assert_eq!(blocked, 0, "blocked texels must be byte-identical");
        assert!(allowed > 0, "the window paints");
        assert_eq!(texel_at(&gated, state.target_size().0, 102, 256), CLEAR);
        assert_ne!(texel_at(&gated, state.target_size().0, 358, 256), CLEAR);

        // (e)
        state.set_mask_active(false);
        assert!(!state.quick_mask().gates());
        drive_stroke(&mut state, [0.1, 0.5], [0.05, 0.0], 16);
        state.end_stroke();
        let ungated = read_back(&mut state, &device, &queue);
        assert_ne!(
            texel_at(&ungated, state.target_size().0, 102, 256),
            CLEAR,
            "ungated paints everywhere"
        );
        let survived = |state: &PaintState| {
            state.quick_mask().coverage(FIRST_TILE) == Some(painted_mask.as_slice())
        };
        assert!(survived(&state), "deactivating keeps the mask");
        state.set_mask_mode(true);
        assert!(
            survived(&state),
            "re-entering mask mode shows the same mask"
        );
        state.set_mask_mode(false);
        assert!(state.quick_mask().gates(), "and re-arms gating");

        // (f)
        assert!(state.clear_mask());
        let plane = state.quick_mask().coverage(FIRST_TILE).expect("plane");
        assert!(plane.iter().all(|&v| v == 0.0), "clear zeroes");
    }

    /// Layer masks (§2, the "METADATA ONLY" row), headless half: the
    /// `u8` stamp arithmetic pins exact values without a GPU-backed
    /// `PaintState` (the free function takes the plane directly).
    #[test]
    fn stamp_grayscale_mask_pins_exact_rounding() {
        use umber_brush::retouch::RetouchDab;
        const SIZE: u32 = 64;
        fn hard_dab(pos: [f32; 2], radius: f32) -> RetouchDab {
            RetouchDab {
                pos,
                radius,
                alpha: 1.0,
                hardness: 1.0,
            }
        }
        // A misaligned plane stamps nothing (never a panic, never a torn
        // write — the torn-sidecar rule).
        let mut short = vec![0u8; 15];
        assert_eq!(
            stamp_grayscale_mask(&mut short, 4, 4, &hard_dab([8.0, 8.0], 4.0), 255),
            None
        );
        // A hard full-alpha white dab on a black plane sets its disc
        // exactly (coverage 1.0 at the center, QuickMask's stamp rule).
        let mut plane = vec![0u8; SIZE as usize * SIZE as usize];
        let rect = stamp_grayscale_mask(&mut plane, SIZE, SIZE, &hard_dab([32.0, 32.0], 8.0), 255)
            .expect("covers texels");
        assert!(rect.x0 <= 24 && rect.x1 >= 40, "{rect:?}");
        assert_eq!(plane[32 * SIZE as usize + 32], 255, "white sets exactly");
        assert_eq!(plane[0], 0, "outside the disc never moves");
        assert_eq!(
            plane[32 * SIZE as usize + 45],
            0,
            "just past the radius stays"
        );
        // A hard black dab takes the same disc back to exactly 0.
        stamp_grayscale_mask(&mut plane, SIZE, SIZE, &hard_dab([32.0, 32.0], 8.0), 0);
        assert_eq!(plane[32 * SIZE as usize + 32], 0, "black closes exactly");
        // A half-alpha white dab on black moves halfway with HALF-AWAY
        // rounding: 0.5 * 255 = 127.5 -> 128. Can-fail: a truncating `as`
        // cast pins 127 and biases every soft mask edge shut by half an
        // LSB (the byte the compositor's half-mask golden reads).
        let half = RetouchDab {
            alpha: 0.5,
            ..hard_dab([10.0, 10.0], 2.0)
        };
        stamp_grayscale_mask(&mut plane, SIZE, SIZE, &half, 255);
        assert_eq!(
            plane[10 * SIZE as usize + 10],
            128,
            "half coverage must round half-away, not truncate"
        );
        // A dab entirely off the plane touches nothing.
        assert_eq!(
            stamp_grayscale_mask(&mut plane, SIZE, SIZE, &hard_dab([-50.0, 32.0], 4.0), 255),
            None
        );
    }

    /// Layer masks, plane-pool half: fresh planes are white and lazy,
    /// publishes are size-checked, and resize/remove/prune drop planes.
    #[test]
    fn layer_mask_planes_are_white_lazy_sized_and_pruned() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        assert_eq!(state.mask_edit_layer(), None);
        assert_eq!(
            state.layer_mask(7, FIRST_TILE),
            None,
            "no plane until touched"
        );
        assert!(state.layer_mask_tiles().is_empty());

        // Fresh planes start WHITE (attaching is a no-op gate).
        state.ensure_layer_mask(7, FIRST_TILE);
        let plane = state.layer_mask(7, FIRST_TILE).expect("plane");
        assert_eq!(plane.len(), 512 * 512);
        assert!(plane.iter().all(|&v| v == 255), "fresh planes gate open");
        assert_eq!(state.layer_mask_tiles(), vec![(7, FIRST_TILE)]);
        // Re-ensuring keeps painted data (never re-whites).
        state.publish_layer_mask(7, FIRST_TILE, vec![0u8; 512 * 512]);
        state.ensure_layer_mask(7, FIRST_TILE);
        assert_eq!(state.layer_mask(7, FIRST_TILE).unwrap()[0], 0);

        // Wrong-size publishes are refused, plane untouched.
        assert!(!state.publish_layer_mask(7, FIRST_TILE, vec![0u8; 10]));
        assert_eq!(state.layer_mask(7, FIRST_TILE).unwrap().len(), 512 * 512);
        // Exact bytes round-trip byte-identical.
        let bytes: Vec<u8> = (0..512 * 512).map(|i| (i % 251) as u8).collect();
        assert!(state.publish_layer_mask(7, FIRST_TILE, bytes.clone()));
        assert_eq!(state.layer_mask(7, FIRST_TILE).unwrap(), bytes.as_slice());

        // Removing the layer's masks clears the edit selection too.
        state.set_mask_edit_layer(Some(7));
        state.remove_layer_masks(7);
        assert_eq!(state.layer_mask(7, FIRST_TILE), None);
        assert_eq!(state.mask_edit_layer(), None, "editing a removed mask ends");
        assert!(state.layer_mask_tiles().is_empty());

        // Pruning keeps only live layers' planes.
        state.ensure_layer_mask(7, FIRST_TILE);
        state.ensure_layer_mask(8, FIRST_TILE);
        state.set_mask_edit_layer(Some(7));
        let mut live = std::collections::HashSet::new();
        live.insert(8u64);
        state.prune_layer_masks(&live);
        assert_eq!(state.layer_mask(7, FIRST_TILE), None);
        assert!(state.layer_mask(8, FIRST_TILE).is_some());
        assert_eq!(state.mask_edit_layer(), None, "pruned selection ends");

        // Resizing the set drops its tiles' planes with the targets.
        state.ensure_layer_mask(8, FIRST_TILE);
        let active = state.active_set().to_string();
        state.resize_set(&active, 256, 256);
        assert_eq!(
            state.layer_mask(8, FIRST_TILE),
            None,
            "a resized set's planes drop with its targets"
        );
    }

    /// Layer masks, routing half: a mask-edit stroke paints ONLY the
    /// selected layer's plane (the layer pixels stay byte-identical, the
    /// quick mask stays unallocated), and wins over quick-mask mode.
    #[test]
    fn layer_mask_stroke_paints_only_the_mask_plane() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        state.set_brush_params(hard_brush());
        state.set_layer_mask_paint(MaskPaint::Block);
        let blank = read_back(&mut state, &device, &queue);

        // Mask mode AND a layer selection: the stroke paints the layer
        // mask, neither the quick mask nor the layer.
        state.set_mask_mode(true);
        state.set_mask_edit_layer(Some(7));
        drive_stroke(&mut state, [0.3, 0.5], [0.02, 0.0], 15);
        state.end_stroke();
        assert_eq!(
            state.staged_batch_count(),
            0,
            "a mask stroke stages nothing"
        );
        assert_eq!(state.staged_dab_count(), 0);
        assert_eq!(
            read_back(&mut state, &device, &queue),
            blank,
            "the layer is untouched by a mask stroke"
        );
        assert!(
            state.quick_mask().coverage(FIRST_TILE).is_none(),
            "the quick mask is untouched by a layer-mask stroke"
        );
        let plane = state
            .layer_mask(7, FIRST_TILE)
            .expect("the stroke materialized the plane");
        let shut = plane.iter().filter(|&&v| v == 0).count();
        assert!(shut > 0, "the stroke closed texels in the plane");
        assert!(
            shut < plane.len(),
            "only the stroke's discs closed ({} of {})",
            shut,
            plane.len()
        );
        assert_eq!(plane[0], 255, "far from the stroke stays open");
        // Snapshot BEFORE the mutable calls below end the plane borrow
        // (E0502: `plane` borrows `state` immutably; set_mask_mode and the
        // stroke driver take it mutably).
        let painted_mask = plane.to_vec();
        // Leaving mask MODE is not leaving the mask: `set_mask_mode(false)`
        // clears `editing` but keeps the quick mask ARMED (`gates()` still
        // true — `set_editing` never clears `active`, by design: the X
        // toggle owns arming). An armed-but-never-painted quick mask gates
        // every staged dab to zero (its plane is all-black until painted
        // open), so this stroke MUST also disarm the gate — exactly what the
        // panel's flow does (the artist toggles X off, or paints the mask
        // white first).
        state.set_mask_mode(false);
        state.set_mask_active(false);

        // Back to layers: a paint stroke changes the layer but leaves the
        // mask plane byte-identical.
        state.set_mask_edit_layer(None);
        drive_stroke(&mut state, [0.3, 0.5], [0.02, 0.0], 15);
        state.end_stroke();
        assert_ne!(
            read_back(&mut state, &device, &queue),
            blank,
            "a layer stroke paints the layer"
        );
        assert_eq!(
            state.layer_mask(7, FIRST_TILE).unwrap(),
            painted_mask.as_slice(),
            "a layer stroke leaves the mask plane alone"
        );
    }

    /// Test (b), end to end: inverting flips the gating — the window the
    /// mask allowed is the one place the same stroke now leaves alone.
    #[test]
    fn inverted_quick_mask_flips_paint_gating() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        mask_right_window(&mut state);
        assert!(state.invert_mask());
        assert_eq!(state.quick_mask().sample(FIRST_TILE, 358, 256), 0.0);

        drive_stroke(&mut state, [0.1, 0.5], [0.05, 0.0], 16);
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let (blocked, allowed) = changed_by_mask(&state, &blank, &gated);
        assert_eq!(blocked, 0, "the inverted window is byte-identical");
        assert!(allowed > 0);
        assert_ne!(
            texel_at(&gated, state.target_size().0, 102, 256),
            CLEAR,
            "what was blocked paints"
        );
        assert_eq!(
            texel_at(&gated, state.target_size().0, 358, 256),
            CLEAR,
            "what was allowed is blocked"
        );
        // Invert/clear are refused mid-stroke (one stroke, one mask).
        state.begin_stroke(emath::pos2(0.5, 0.5));
        assert!(!state.invert_mask() && !state.clear_mask());
        state.end_stroke();
    }

    /// Test (d), end to end: the gate applies AFTER symmetry expansion.
    /// With only the left side allowed, a mirrored stroke there paints its
    /// primary but its sibling (on the blocked right side) writes nothing;
    /// with the mask deactivated, the same sibling paints.
    #[test]
    fn quick_mask_gates_symmetry_siblings_on_the_gpu() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        // Allow u ≈ 0.1..0.4 along v = 0.5 (symmetry still off).
        assert!(state.toggle_mask_mode());
        drive_stroke(&mut state, [0.1, 0.5], [0.02, 0.0], 15);
        state.end_stroke();
        assert!(!state.toggle_mask_mode());
        assert!((256..512).all(|x| state.quick_mask().sample(FIRST_TILE, x, 256) == 0.0));

        let mut settings = SymmetrySettings::default();
        settings.mirror.enabled = true;
        state.sync_symmetry(&mut settings);
        drive_stroke(&mut state, [0.15, 0.5], [0.01, 0.0], 15);
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let (blocked, allowed) = changed_by_mask(&state, &blank, &gated);
        assert_eq!(blocked, 0, "no sibling texel lands on the blocked side");
        assert!(allowed > 0);
        assert_ne!(
            texel_at(&gated, state.target_size().0, 102, 256),
            CLEAR,
            "the primary paints"
        );
        assert_eq!(
            texel_at(&gated, state.target_size().0, 409, 256),
            CLEAR,
            "its sibling is gated"
        );

        // Control: ungated, the sibling paints.
        state.set_mask_active(false);
        drive_stroke(&mut state, [0.15, 0.5], [0.01, 0.0], 15);
        state.end_stroke();
        let ungated = read_back(&mut state, &device, &queue);
        assert_ne!(texel_at(&ungated, state.target_size().0, 409, 256), CLEAR);
    }

    // --- ID-mask selection tests (C1 worklist #11) ---

    /// Bake edge length for the ID-selection tests: matches the mask size
    /// (the selection contract needs exact texel correspondence), small so
    /// the click-time bake stays fast.
    const ID_SIZE: u32 = 32;

    /// Two triangles with clearly separated UV halves (the same geometry
    /// as `umber_bake::id`'s GPU `split_halves_mesh` fixture, rebuilt here
    /// because that fixture is test-private): triangle 0 owns `u` in
    /// `[0, 0.5]` (UVs `(0,0)`, `(0.5,0)`, `(0,1)`), triangle 1 owns `u`
    /// in `[0.5, 1]`.
    fn split_halves_mesh() -> umber_mesh::MeshData {
        umber_mesh::MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uvs: vec![
                [0.0, 0.0],
                [0.5, 0.0],
                [0.0, 1.0],
                [0.5, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            material_names: vec!["m".into()],
        }
    }

    /// The audit gate, model half: on a two-ID bake, selecting at a UV in
    /// region A fills the coverage plane with exactly A-in / B-out (byte
    /// equality against the bake, `1.0`/`0.0` with no falloff) and arms
    /// gating. The click-time bake path returns the same ID.
    #[test]
    fn id_mask_selects_only_the_clicked_region() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mesh = split_halves_mesh();
        let bytes = bake_id_mesh(
            &device,
            &queue,
            &mesh,
            &IdBakeParams {
                flavor: IdFlavor::Triangle,
                width: ID_SIZE,
                height: ID_SIZE,
            },
        )
        .expect("the two-region bake succeeds");
        // Can-fail guards: the bake itself produced two distinct covered
        // IDs at the probe texels BEFORE the mask is asserted. The goldens
        // are FNV-1a over the triangle index's LE bytes (pinned against the
        // bake's own mirror in `umber_bake::id`'s tests).
        let id_a = texel_at(&bytes, ID_SIZE, 3, 16);
        let id_b = texel_at(&bytes, ID_SIZE, 28, 16);
        assert_eq!(id_a, [0x15, 0xF5, 0x95, 255], "texel (3, 16) is triangle 0");
        assert_eq!(
            id_b,
            [0x04, 0xB6, 0x69, 255],
            "texel (28, 16) is triangle 1"
        );
        assert_ne!(id_a, id_b, "test premise: two distinct regions");

        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), ID_SIZE)
            .expect("paint state builds");
        // u = 3.5/32 -> x = round(3.5 - 0.5) = 3; v = 0.5 -> y =
        // round(16 - 0.5) = 16: the bake's texel-center, V-flipped mapping.
        let click = [3.5 / 32.0, 0.5];
        let clicked = state
            .select_id_mask(FIRST_TILE, &bytes, ID_SIZE, ID_SIZE, click)
            .expect("a covered click selects");
        assert_eq!(clicked, id_a);
        assert!(state.quick_mask().gates(), "selection arms gating");
        let plane = state
            .quick_mask()
            .coverage(FIRST_TILE)
            .expect("plane")
            .to_vec();
        assert_eq!(plane.len(), bytes.len() / 4);
        for (i, px) in bytes.chunks_exact(4).enumerate() {
            let same = px[0] == id_a[0] && px[1] == id_a[1] && px[2] == id_a[2] && px[3] == id_a[3];
            assert_eq!(plane[i], f32::from(same), "texel {i}: A-in / B-out exactly");
        }
        assert!(
            plane.contains(&1.0) && plane.contains(&0.0),
            "both regions present — a one-region bake cannot pass"
        );

        // The click-time bake path (no stored bytes): same ID, same gate.
        let mut fresh =
            PaintState::with_target_size(device, queue, ID_SIZE).expect("paint state builds");
        let baked = fresh
            .bake_and_select_id(&mesh, click)
            .expect("click-time bake");
        assert_eq!(baked, id_a);
        assert!(fresh.quick_mask().gates());
    }

    /// The audit gate, routing half: after an ID selection the live stroke
    /// snapshots the gate (`stroke_mask_gate`), its dabs carry the shader's
    /// `mask_gate` flag, and only the selected ID's texels change — the
    /// non-selected region stays byte-identical. Selection is refused
    /// mid-stroke.
    #[test]
    fn id_mask_gates_paint_to_the_selected_region() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mesh = split_halves_mesh();
        let params = IdBakeParams {
            flavor: IdFlavor::Triangle,
            width: ID_SIZE,
            height: ID_SIZE,
        };
        let bytes = bake_id_mesh(&device, &queue, &mesh, &params).expect("bake succeeds");
        let clicked = texel_at(&bytes, ID_SIZE, 3, 16);
        assert_eq!(clicked, [0x15, 0xF5, 0x95, 255], "guard: region A golden");

        let mut state = PaintState::with_target_size(device.clone(), queue.clone(), ID_SIZE)
            .expect("paint state builds");
        state.set_brush_params(hard_brush());
        let blank = read_back(&mut state, &device, &queue);
        let click = [3.5 / 32.0, 0.5];
        assert_eq!(
            state
                .select_id_mask(FIRST_TILE, &bytes, ID_SIZE, ID_SIZE, click)
                .expect("select"),
            clicked
        );

        state.begin_stroke(emath::pos2(0.05, 0.5));
        assert!(state.stroke_mask_gate, "the live stroke snapshots the gate");
        for i in 1..=16 {
            state.extend_stroke(emath::pos2(0.05 + 0.02 * i as f32, 0.5));
        }
        state.end_stroke();
        let gated = read_back(&mut state, &device, &queue);
        let mut changed = 0;
        for (i, (a, b)) in blank.chunks_exact(4).zip(gated.chunks_exact(4)).enumerate() {
            if a == b {
                continue;
            }
            changed += 1;
            let base = i * 4;
            assert_eq!(
                [
                    bytes[base],
                    bytes[base + 1],
                    bytes[base + 2],
                    bytes[base + 3]
                ],
                clicked,
                "texel {i} changed but bakes to the non-selected ID"
            );
        }
        assert!(changed > 0, "the selected region paints");
        assert_eq!(
            texel_at(&gated, ID_SIZE, 28, 16),
            texel_at(&blank, ID_SIZE, 28, 16),
            "a region-B texel is byte-identical"
        );

        // Refused mid-stroke (one stroke, one mask — like invert/clear).
        state.begin_stroke(emath::pos2(0.5, 0.5));
        assert!(matches!(
            state.select_id_mask(FIRST_TILE, &bytes, ID_SIZE, ID_SIZE, click),
            Err(IdSelectError::StrokeInFlight)
        ));
        assert!(matches!(
            state.bake_and_select_id(&mesh, click),
            Err(IdSelectError::StrokeInFlight)
        ));
        state.end_stroke();
    }

    // --- Per-tile routing tests (UDIM slice 4) ---

    #[test]
    fn tiles_present_lists_routed_tiles_sorted() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Unrouted session: tile 1001 only.
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.set_mesh(&two_tile_strip());
        assert_eq!(state.tiles_present(), vec![1001]);
        // A routed 1002 event adds 1002; listing is ascending (1002 was
        // created after 1001 but the order is by tile, not creation).
        state.begin_stroke(emath::pos2(1.2, 0.5));
        state.end_stroke();
        assert_eq!(state.tiles_present(), vec![1001, 1002]);
        // Re-routing to an existing tile adds nothing.
        state.begin_stroke(emath::pos2(0.5, 0.5));
        state.end_stroke();
        assert_eq!(state.tiles_present(), vec![1001, 1002]);
    }

    #[test]
    fn two_tile_strip_spans_tiles_1001_and_1002() {
        // Fixture sanity: the routing tests rely on these exact tiles and
        // on the strip being seam-free.
        let mesh = two_tile_strip();
        assert_eq!(tile_of_triangle(&mesh), vec![1001, 1001, 1002, 1002]);
        assert!(build_seam_graph(&mesh).edges.is_empty());
    }

    #[test]
    fn local_texel_subtracts_tile_offsets() {
        // (1.2, 0.5) in tile 1002 at 512 tpv: tile_u = 1, tile_v = 0, so
        // x = (1.2 - 1.0) * 512 and y = (1 - 0.5) * 512 = 256 exactly.
        // NOTE: 1.2_f32 - 1.0 is 0.20000005 in f32 (exact subtraction of
        // the rounded 1.2), so x is 102.400024, NOT the literal 102.4_f32 —
        // assert the derived f32 expression exactly.
        let [x, y] = local_texel([1.2, 0.5], 1002, 512.0);
        assert_eq!(x, (1.2_f32 - 1.0) * 512.0);
        assert_ne!(x, 102.4_f32);
        assert!((x - 102.4).abs() < 1e-3);
        assert_eq!(y, 256.0);

        // V offset adds tens: (0.5, 1.2) in tile 1011 → tile_v = 1, so
        // y = (1 - (1.2 - 1.0)) * 512.
        let [x, y] = local_texel([0.5, 1.2], 1011, 512.0);
        assert_eq!(x, 256.0);
        assert_eq!(y, (1.0 - (1.2_f32 - 1.0)) * 512.0);

        // Tile 1001: bit-identical to the pre-UDIM mapping.
        for uv in [[0.0, 0.0], [0.37, 0.81], [1.0, 1.0], [0.123_456, 0.999]] {
            let old = [uv[0] * 512.0, (1.0 - uv[1]) * 512.0];
            let new = local_texel(uv, 1001, 512.0);
            assert_eq!(new[0].to_bits(), old[0].to_bits(), "x at {uv:?}");
            assert_eq!(new[1].to_bits(), old[1].to_bits(), "y at {uv:?}");
        }
    }

    #[test]
    fn stroke_at_tile_1002_lands_in_tile_1002_only() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.set_mesh(&two_tile_strip());
        assert_eq!(state.staged_batch_count_for(1001), 0);
        assert_eq!(state.staged_batch_count_for(1002), 0);

        // Stays inside [1,2)x[0,1]: (1.2,0.5) → (1.35,0.5).
        drive_stroke(&mut state, [1.2, 0.5], [0.01, 0.0], 15);
        state.end_stroke();

        let in_1002 = state.staged_batch_count_for(1002);
        assert!(
            in_1002 > 0,
            "stroke at UV (1.2,0.5) must stage into tile 1002"
        );
        assert_eq!(
            state.staged_batch_count_for(1001),
            0,
            "tile 1001 must stay untouched by a tile-1002 stroke"
        );
        assert_eq!(state.staged_batch_count(), in_1002);
        assert_eq!(state.active_tile(), 1002);
        assert!(state.tile_target(1002).is_some());
        assert!(state.tile_target(1003).is_none());
        // Every tile's queue drains (and the active-tile accessor serves
        // the routed tile).
        state.process_pending().expect("drain succeeds");
        assert!(std::ptr::eq(
            state.paint_target(),
            state.tile_target(1002).expect("tile 1002 exists"),
        ));
    }

    #[test]
    fn tile_targets_are_created_lazily_and_reused() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.set_mesh(&two_tile_strip());
        assert_eq!(state.tile_count(), 1, "only tile 1001 exists up front");

        state.begin_stroke(emath::pos2(1.2, 0.5));
        state.end_stroke();
        assert_eq!(state.tile_count(), 2, "first route to 1002 creates it");

        state.begin_stroke(emath::pos2(1.7, 0.3));
        state.end_stroke();
        assert_eq!(state.tile_count(), 2, "re-routing to 1002 reuses it");

        state.begin_stroke(emath::pos2(0.5, 0.5));
        state.end_stroke();
        assert_eq!(state.tile_count(), 2, "1001 already existed");
        assert_eq!(state.active_tile(), 1001);
    }

    /// Design gate 1 (docs/specs/per-channel-tile-pool-design.md): a
    /// height write lands in height's target ONLY — baseColor stays
    /// all-transparent — and the stroke-undo journal restores per
    /// channel (the undo key is (tile, channel), not just tile).
    #[test]
    fn height_channel_is_isolated_from_base_color_with_per_channel_undo() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // Deterministic pattern: texel (x, y) channel c holds
        // (x*31 + y*17 + c*71 + seed) mod 251 (251 is prime, so the
        // values spread; the same formula main.rs's persistence tests
        // use). Seed 5 pins texel (0,0) to [5, 76, 147, 218] and
        // (1,0)'s red to 36 — exact bytes, not ranges, so a swapped
        // formula fails here.
        fn channel_pattern(seed: u8) -> Vec<u8> {
            let mut out = Vec::with_capacity(512 * 512 * 4);
            for y in 0..512u32 {
                for x in 0..512u32 {
                    for c in 0..4u32 {
                        out.push(((x * 31 + y * 17 + c * 71 + u32::from(seed)) % 251) as u8);
                    }
                }
            }
            out
        }
        let height_bytes = channel_pattern(5);
        assert_eq!(&height_bytes[..4], &[5u8, 76, 147, 218]);
        assert_eq!(height_bytes[4], 36);
        assert!(height_bytes.iter().any(|&b| b != 0));

        assert!(state.ensure_channel(1001, "height"));
        assert_eq!(state.channels_present(1001), vec!["height".to_string()]);
        assert!(state.publish_fill_channel(1001, "height", 512, 512, height_bytes.clone()));
        state.process_pending().expect("upload lands");

        // Height holds the bytes; baseColor is untouched (a fresh
        // target is zero-initialized transparent black — all-zero).
        let height = state
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((height.width, height.height), (512, 512));
        assert_eq!(height.data, height_bytes);
        let base = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("baseColor reads back");
        assert_eq!((base.width, base.height), (512, 512));
        assert!(
            base.data.iter().all(|&b| b == 0),
            "baseColor must stay all-transparent"
        );
        // A never-routed channel reads None — reads never materialize
        // (the eyedropper/ID-mask contract on read_target).
        assert!(state.read_channel_tile(1001, "roughness").is_none());
        assert_eq!(state.channels_present(1001), vec!["height".to_string()]);

        // The undo half needs a real stroke: fill publishes ride the
        // WriteRegion queue with no history entry (publish_fill's own
        // contract), while a paint-tool stroke records one entry keyed
        // by (tile, channel) — see record_paint_capture.
        assert!(!state.can_undo_stroke(), "fills record no undo entry");
        state.set_active_channel("height");
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(state.can_undo_stroke(), "the height stroke is one entry");
        state.process_pending().expect("drain succeeds");
        let stroked = state
            .read_channel_tile(1001, "height")
            .expect("height re-reads");
        // Can-fail guard with a derivation, not a hope: the pattern's
        // bytes are all < 251 (mod 251), while the default brush
        // ([0.85, 0.2, 0.1, 1.0]) stamps full-coverage texels at alpha
        // 255 down the stroke's center — a value the pattern can never
        // hold — so at least one byte must differ.
        assert_ne!(
            stroked.data, height_bytes,
            "the stroke must reach the height target"
        );
        // Still isolated after the stroke: baseColor never saw a dab.
        let base_after = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("baseColor re-reads");
        assert!(base_after.data.iter().all(|&b| b == 0));

        // Undo restores the height fill byte-exact WITHOUT touching
        // baseColor; redo replays the stroke's bytes.
        assert!(state.undo_stroke());
        state.process_pending().expect("drain succeeds");
        let undone = state
            .read_channel_tile(1001, "height")
            .expect("height re-reads");
        assert_eq!(
            undone.data, height_bytes,
            "undo restores the pre-stroke height bytes"
        );
        let base_undone = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("baseColor re-reads");
        assert!(base_undone.data.iter().all(|&b| b == 0));
        assert!(state.redo_stroke());
        state.process_pending().expect("drain succeeds");
        let redone = state
            .read_channel_tile(1001, "height")
            .expect("height re-reads");
        assert_eq!(
            redone.data, stroked.data,
            "redo restores the post-stroke height bytes"
        );
        assert!(!state.can_redo_stroke());
    }

    /// Design gate 2: channel targets materialize lazily — a session
    /// that KNOWS five channel names but routes to one holds exactly
    /// two targets (baseColor + the routed one).
    #[test]
    fn channel_targets_materialize_lazily_per_routed_channel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // The census: baseColor is implicit (present for every tile
        // tiles_present lists), so targets = channels_present + 1.
        let census = |state: &PaintState| state.channels_present(1001).len() + 1;
        assert_eq!(census(&state), 1, "a fresh tile holds baseColor only");
        assert_eq!(state.tile_count(), 1);

        // Five known channel names (the document's TextureSet.channels
        // is the source of truth; the session mirrors it as plain
        // strings) — routing to ONE materializes exactly one entry.
        assert!(state.ensure_channel(1001, "height"));
        assert_eq!(state.channels_present(1001), vec!["height".to_string()]);
        assert_eq!(census(&state), 2, "baseColor + the one routed channel");
        assert_eq!(
            state.tile_count(),
            1,
            "routing a channel never spawns a tile"
        );
        // Re-routing the same channel reuses its entry.
        assert!(state.ensure_channel(1001, "height"));
        assert_eq!(census(&state), 2);
        // Routing baseColor never allocates a channels entry (the
        // reserved-name contract on BASE_COLOR_CHANNEL).
        assert!(state.ensure_channel(1001, BASE_COLOR_CHANNEL));
        assert_eq!(census(&state), 2);
        // The four unrouted names cost nothing: no entry, and reads
        // report absence WITHOUT creating one (read_target with
        // materialize=false).
        for channel in ["roughness", "metallic", "normal", "ao"] {
            assert!(
                state.read_channel_tile(1001, channel).is_none(),
                "{channel} was never routed"
            );
        }
        assert_eq!(census(&state), 2, "reads must not materialize targets");
        assert_eq!(state.tile_count(), 1);
    }

    #[test]
    fn single_tile_meshes_route_everything_to_1001() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // No mesh: even an out-of-[0,1] UV stays in 1001.
        let mut bare = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        bare.begin_stroke(emath::pos2(1.5, 0.5));
        bare.end_stroke();
        assert_eq!(bare.tile_count(), 1);
        assert_eq!(bare.active_tile(), 1001);

        // Single-tile mesh: the UV view's u == 1.0 edge (tile_of_uv gives
        // 1002) must not spawn a blank tile.
        let mut single =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        single.set_mesh(&single_island_quad());
        drive_stroke(&mut single, [0.9, 0.5], [0.01, 0.0], 15);
        single.end_stroke();
        assert_eq!(single.tile_count(), 1);
        assert_eq!(single.active_tile(), 1001);
        assert!(single.staged_batch_count() > 0);
        assert_eq!(
            single.staged_batch_count_for(1001),
            single.staged_batch_count(),
            "every batch lands in tile 1001"
        );
    }

    #[test]
    fn mirrors_route_by_their_own_tile_without_moving_active() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // two_island_quad's second island lives at UV ~10–11, which
        // tile_of_uv clamps to tile 1100: the mesh is multi-tile, and the
        // seam mirrors route there while the pointer stays in 1001.
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.set_mesh(&two_island_quad());
        drive_stroke(&mut state, NEAR_SEAM_UV, [0.02, 0.02], 16);
        state.end_stroke();
        assert!(state.staged_batch_count_for(1001) > 0);
        assert!(
            state.staged_batch_count_for(1100) > 0,
            "mirrors must land in their own tile"
        );
        assert_eq!(
            state.staged_batch_count(),
            state.staged_batch_count_for(1001) + state.staged_batch_count_for(1100)
        );
        assert_eq!(
            state.active_tile(),
            1001,
            "mirrors never move the active tile"
        );
        state.process_pending().expect("drain succeeds");
    }

    // --- Paint paths (§2 line 26's P1 row) ---

    #[test]
    fn fill_runs_cover_maximal_spans() {
        // Row 0 full, row 1 two spans, row 2 empty.
        let mask = vec![
            true, true, true, true, //
            true, false, false, true, //
            false, false, false, false,
        ];
        assert_eq!(
            fill_runs(&mask, 4, 3),
            vec![
                FillRun { y: 0, x0: 0, x1: 3 },
                FillRun { y: 1, x0: 0, x1: 0 },
                FillRun { y: 1, x0: 3, x1: 3 },
            ]
        );
        assert!(fill_runs(&[], 4, 4).is_empty());
        // A torn (short) rasterizer output degrades to its present prefix,
        // never a panic.
        assert_eq!(
            fill_runs(&[true], 4, 4),
            vec![FillRun { y: 0, x0: 0, x1: 0 }]
        );
    }

    #[test]
    fn texel_center_uv_inverts_local_texel() {
        for tile in [FIRST_TILE, 1002, 1011] {
            for (x, y) in [(0, 0), (17, 42), (511, 511)] {
                let uv = texel_center_uv(tile, x, y, 512.0);
                let back = local_texel(uv, tile, 512.0);
                assert!(
                    (back[0] - (x as f32 + 0.5)).abs() < 1e-3
                        && (back[1] - (y as f32 + 0.5)).abs() < 1e-3,
                    "tile {tile} texel ({x},{y}) must round-trip"
                );
            }
        }
    }

    /// The replay's event stream from path points (16 vertical steps, full
    /// style — the same UVs `drive_stroke` feeds).
    fn straight_path(start: [f32; 2], step: [f32; 2], steps: usize) -> PaintPath {
        let mut path = PaintPath::new();
        path.push_point(start, 1.0, 1.0, 1.0);
        for i in 1..=steps {
            path.push_point(
                [start[0] + step[0] * i as f32, start[1] + step[1] * i as f32],
                1.0,
                1.0,
                1.0,
            );
        }
        path
    }

    #[test]
    fn replayed_path_stages_exactly_like_the_manual_stroke() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let (start, step) = ([0.2, 0.3], [0.0, 0.02]);

        let mut manual = PaintState::with_target_size(device.clone(), queue.clone(), 512)
            .expect("paint state builds");
        drive_stroke(&mut manual, start, step, 15);
        manual.end_stroke();

        let mut replayed =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // Park a non-paint tool: the replay must restore it afterwards.
        let mut smudge_tool = BrushTool::Smudge(umber_brush::SmudgeParams::default());
        replayed.sync_tool(&mut smudge_tool);
        let path = straight_path(start, step, 15);
        assert!(replayed.replay_path(&path, &PathOp::Paint));
        assert!(!replayed.is_stroking(), "replay ends its stroke");
        assert_eq!(
            replayed.staged_batch_count(),
            manual.staged_batch_count(),
            "a full-style replay is the manual event stream"
        );
        assert_eq!(
            replayed.staged_dab_count(),
            manual.staged_dab_count(),
            "dab for dab, through the same conditioner"
        );
        assert_eq!(
            replayed.tool, smudge_tool,
            "the session tool survives the replay"
        );
    }

    #[test]
    fn replay_refuses_live_strokes_and_short_paths() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let mut short = PaintPath::new();
        short.push_point([0.5, 0.5], 1.0, 1.0, 1.0);
        assert!(!state.replay_path(&short, &PathOp::Paint));
        assert!(!state.replay_filled_path(&short, &PathOp::Paint));
        assert!(!state.replay_filled_path(&PaintPath::new(), &PathOp::Paint));
        // A replay never splits a live stroke.
        let path = straight_path([0.2, 0.3], [0.0, 0.02], 15);
        state.begin_stroke(emath::pos2(0.5, 0.5));
        assert!(!state.replay_path(&path, &PathOp::Paint));
        assert!(!state.replay_filled_path(&path, &PathOp::Paint));
        state.end_stroke();
        assert!(state.replay_path(&path, &PathOp::Paint));
    }

    #[test]
    fn retouch_replay_routes_to_the_cpu_mirror() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        let path = straight_path([0.2, 0.5], [0.01, 0.0], 15);
        let batches = state.staged_batch_count();
        let op = PathOp::Smudge(umber_brush::SmudgeParams {
            strength: 1.0,
            smear: 0.5,
        });
        assert!(state.replay_path(&path, &op));
        assert_eq!(
            state.staged_batch_count(),
            batches,
            "smudge dabs never stage GPU splats"
        );
        assert!(
            state.can_undo_stroke(),
            "the replayed smudge is one undo entry"
        );
        // The eraser on a blank target is the exact identity (no writes,
        // no history) — while still replaying through its own consumer.
        let mut blank =
            PaintState::with_target_size(state.device.clone(), state.queue.clone(), 512)
                .expect("paint state builds");
        assert!(blank.replay_path(&path, &PathOp::Erase(umber_brush::EraseParams::default())));
        assert_eq!(blank.staged_batch_count(), 0);
        assert!(
            !blank.can_undo_stroke(),
            "an identity erase records nothing"
        );
    }

    // --- Paint-target size (CORE-LOOP CRITICAL 2) ---

    /// `new` builds the 4096 default (the review's directive), and
    /// `resize_target` round-trips 512→4096→512 exactly (each resize also
    /// clears the stroke history — asserted here so a silent keep would
    /// fail).
    #[test]
    fn new_defaults_to_4k_and_resize_round_trips() {
        assert_eq!(
            DEFAULT_TARGET_SIZE, 4096,
            "the live paint target is 4K, not 512"
        );
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        assert_eq!(state.target_size(), (4096, 4096));
        assert_eq!(state.paint_target().dimensions(), (4096, 4096));

        state.resize_target(512, 512);
        assert_eq!(state.target_size(), (512, 512));
        state.process_pending().expect("resize applies");
        assert_eq!(state.paint_target().dimensions(), (512, 512));

        state.resize_target(4096, 4096);
        assert_eq!(state.target_size(), (4096, 4096));
        state.process_pending().expect("resize applies");
        assert_eq!(state.paint_target().dimensions(), (4096, 4096));

        // The resize-clears-undo contract, on the record: a painted stroke
        // followed by a resize leaves no entry behind.
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(
            state.can_undo_stroke(),
            "the setup stroke must record an entry (else the clear below is vacuous)"
        );
        state.resize_target(512, 512);
        assert!(
            !state.can_undo_stroke(),
            "a resize must clear the stroke history (rects no longer fit)"
        );
        assert_eq!(state.target_size(), (512, 512));
    }

    // --- Per-texture-set paint-target sizes (audit Pass 4 item 4) ---

    /// Each set owns its size: tiles are created at their set's size, the
    /// active set drives `target_size()`, and unknown sets read the
    /// default (never a panic, never zero).
    #[test]
    fn tiles_are_created_at_their_own_sets_size() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        assert_eq!(state.active_set(), DEFAULT_TEXTURE_SET);
        assert_eq!(state.set_names(), vec![DEFAULT_TEXTURE_SET.to_string()]);
        assert_eq!(state.set_of_tile(1001), DEFAULT_TEXTURE_SET);
        assert_eq!(state.tile_size(1001), (512, 512));
        assert_eq!(state.texels_per_uv_for(1001), 512.0);

        state.resize_set("Cloak", 256, 256);
        assert_eq!(state.size_of_set("Cloak"), (256, 256));
        assert_eq!(
            state.set_names(),
            vec!["Cloak".to_string(), DEFAULT_TEXTURE_SET.to_string()]
        );
        // Unknown sets read the default (the load path registers mapped
        // sets before asking — unseen means default).
        assert_eq!(state.size_of_set("NeverSeen"), (4096, 4096));
        assert_eq!(
            state.tile_size(1002),
            (512, 512),
            "unassigned tiles resolve to the active set"
        );

        state.assign_tile_to_set(1002, "Cloak");
        assert_eq!(state.set_of_tile(1002), "Cloak");
        assert_eq!(state.tile_size(1002), (256, 256));
        assert_eq!(state.texels_per_uv_for(1002), 256.0);
        assert_eq!(
            state.tiles_of_set("Cloak"),
            Vec::<u16>::new(),
            "assignment alone creates no target (lazy pool)"
        );

        // Creation pins the dims exactly (not a range check — a tile at
        // the wrong size would fail).
        assert!(state.ensure_tile(1002));
        assert_eq!(
            state.tile_target(1002).expect("created").dimensions(),
            (256, 256)
        );
        assert_eq!(
            state.tile_target(1001).expect("kept").dimensions(),
            (512, 512)
        );

        // The active set drives the legacy single-size surface.
        assert_eq!(state.target_size(), (512, 512));
        state.set_active_set("Cloak");
        assert_eq!(state.active_set(), "Cloak");
        assert_eq!(state.target_size(), (256, 256));
    }

    /// A set resize rebuilds only that set's tiles, drops only that set's
    /// mask planes, and still clears the shared history (the per-set
    /// clears-undo contract — see the module docs for why the clear is
    /// session-wide).
    #[test]
    fn resize_set_is_scoped_to_its_tiles_and_masks() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        state.resize_set("Cloak", 256, 256);
        state.assign_tile_to_set(1002, "Cloak");
        assert!(state.ensure_tile(1002));
        state.process_pending().expect("tiles apply");

        // A mask plane on the default set, so the scoped-drop below is
        // non-vacuous (a resize that dropped nothing would pass anyway).
        state.set_mask_mode(true);
        state.begin_stroke(emath::pos2(0.25, 0.25));
        for i in 1..=14 {
            let t = i as f32 * 0.02;
            state.extend_stroke(emath::pos2(0.25 + t, 0.25));
        }
        state.end_stroke();
        state.set_mask_mode(false);
        // Exiting edit mode does NOT disarm the quick mask (it stays armed
        // and gating — see QuickMask::gates: active && !editing; the app's
        // X-key flow deactivates it separately). The paint stroke below
        // must land, so deactivate the way the app does after a mask
        // session — otherwise the mask gates every dab, the stroke stages
        // nothing, and the identity rule records no history entry (the
        // CI red on 2564c17: can_undo_stroke() false at the next assert).
        state.set_mask_active(false);
        assert!(
            state.quick_mask().coverage(1001).is_some(),
            "the mask stroke must allocate the default set's plane"
        );

        // A paint stroke, so the history clear below is non-vacuous too.
        drive_stroke(&mut state, [0.6, 0.6], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(state.can_undo_stroke());

        state.resize_set("Cloak", 128, 128);
        assert!(
            !state.can_undo_stroke(),
            "a set resize clears the shared stroke history"
        );
        assert_eq!(state.size_of_set("Cloak"), (128, 128));
        assert_eq!(state.size_of_set(DEFAULT_TEXTURE_SET), (512, 512));
        state.process_pending().expect("resize applies");
        assert_eq!(
            state.tile_target(1002).expect("resized").dimensions(),
            (128, 128)
        );
        assert_eq!(
            state.tile_target(1001).expect("untouched").dimensions(),
            (512, 512)
        );
        assert!(
            state.quick_mask().coverage(1001).is_some(),
            "the other set's mask planes survive a set resize"
        );
        // ...while the resized set's own mask follows its new size.
        assert_eq!(state.quick_mask_for_tile(1002).dimensions(), (128, 128));
    }

    /// Per-channel sizing: an overridden channel materializes at ITS size
    /// while baseColor keeps the set's, and a channel without an override
    /// inherits the set's size (the channel → set fallback rule).
    #[test]
    fn overridden_channel_materializes_at_its_size_while_base_color_keeps_the_set() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // The default session paints 4096 (DEFAULT_TARGET_SIZE); the
        // height override pins 512 — a swapped fallback fails here.
        let mut state = PaintState::new(device, queue).expect("paint state builds");
        assert_eq!(state.size_of_set(DEFAULT_TEXTURE_SET), (4096, 4096));
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, "height"),
            (4096, 4096)
        );
        assert_eq!(state.channel_override(DEFAULT_TEXTURE_SET, "height"), None);
        assert_eq!(
            state.channel_override(DEFAULT_TEXTURE_SET, BASE_COLOR_CHANNEL),
            None,
            "baseColor never carries an override"
        );

        state.resize_channel(DEFAULT_TEXTURE_SET, "height", 512, 512);
        assert_eq!(
            state.channel_override(DEFAULT_TEXTURE_SET, "height"),
            Some((512, 512))
        );
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, "height"),
            (512, 512)
        );
        // Inherit still reads the set size, and baseColor never overrides.
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, "roughness"),
            (4096, 4096)
        );
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, BASE_COLOR_CHANNEL),
            (4096, 4096)
        );

        assert!(state.ensure_channel(1001, "height"));
        state.process_pending().expect("creation lands");
        assert_eq!(state.channels_present(1001), vec!["height".to_string()]);
        let height = state
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((height.width, height.height), (512, 512));
        let base = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("baseColor reads back");
        assert_eq!((base.width, base.height), (4096, 4096));
        // A never-routed channel still reads None (reads never
        // materialize, even beside an override).
        assert!(state.read_channel_tile(1001, "roughness").is_none());
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, "roughness"),
            (4096, 4096)
        );
    }

    /// `resize_channel` rebuilds ONLY that channel's targets: baseColor
    /// keeps its dims AND its bytes, the set size is unchanged, the
    /// shared history still clears (the channel's resize_set semantics),
    /// a later set resize preserves the override, and clearing the
    /// override rebuilds at the set's size.
    #[test]
    fn resize_channel_touches_only_its_channel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // Deterministic patterns (the same x*31+y*17+c*71+seed mod 251
        // formula the channel isolation test pins): seed 5 pins texel
        // (0,0) to [5, 76, 147, 218], seed 9 to [9, 80, 151, 222].
        fn channel_pattern(seed: u8) -> Vec<u8> {
            let mut out = Vec::with_capacity(512 * 512 * 4);
            for y in 0..512u32 {
                for x in 0..512u32 {
                    for c in 0..4u32 {
                        out.push(((x * 31 + y * 17 + c * 71 + u32::from(seed)) % 251) as u8);
                    }
                }
            }
            out
        }
        let base_bytes = channel_pattern(5);
        let height_bytes = channel_pattern(9);
        assert_eq!(&base_bytes[..4], &[5u8, 76, 147, 218]);
        assert_eq!(&height_bytes[..4], &[9u8, 80, 151, 222]);
        assert_ne!(base_bytes, height_bytes);
        assert!(state.publish_fill(1001, 512, 512, base_bytes.clone()));
        assert!(state.ensure_channel(1001, "height"));
        assert!(state.publish_fill_channel(1001, "height", 512, 512, height_bytes.clone()));
        state.process_pending().expect("uploads land");
        // A height stroke (baseColor stays exactly the fill pattern, so
        // the isolation assert below is byte-exact), so the history clear
        // is non-vacuous too.
        state.set_active_channel("height");
        drive_stroke(&mut state, [0.3, 0.3], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(state.can_undo_stroke());

        state.resize_channel(DEFAULT_TEXTURE_SET, "height", 256, 256);
        assert!(
            !state.can_undo_stroke(),
            "a channel resize clears the shared stroke history"
        );
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, "height"),
            (256, 256)
        );
        assert_eq!(state.size_of_set(DEFAULT_TEXTURE_SET), (512, 512));
        state.process_pending().expect("resize applies");

        let height = state
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((height.width, height.height), (256, 256));
        let base = state
            .read_channel_tile(1001, BASE_COLOR_CHANNEL)
            .expect("baseColor reads back");
        assert_eq!((base.width, base.height), (512, 512));
        assert_eq!(
            base.data, base_bytes,
            "baseColor bytes survive a channel resize"
        );

        // A set resize preserves the override: baseColor rebuilds at the
        // new set size while height keeps ITS size.
        state.resize_set(DEFAULT_TEXTURE_SET, 1024, 1024);
        state.process_pending().expect("set resize applies");
        assert_eq!(
            state.tile_target(1001).expect("base rebuilt").dimensions(),
            (1024, 1024)
        );
        let height_kept = state
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!(
            (height_kept.width, height_kept.height),
            (256, 256),
            "a set resize preserves the channel override"
        );

        // Clearing the override rebuilds at the set's size; clearing what
        // was never overridden keeps the history (no-op, not a resize).
        state.clear_channel_size(DEFAULT_TEXTURE_SET, "height");
        assert_eq!(state.channel_override(DEFAULT_TEXTURE_SET, "height"), None);
        assert_eq!(
            state.size_of_channel(DEFAULT_TEXTURE_SET, "height"),
            (1024, 1024)
        );
        state.process_pending().expect("clear applies");
        let height_inherited = state
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!(
            (height_inherited.width, height_inherited.height),
            (1024, 1024)
        );
        state.set_active_channel(BASE_COLOR_CHANNEL);
        drive_stroke(&mut state, [0.6, 0.6], [0.0, 0.02], 15);
        state.end_stroke();
        assert!(state.can_undo_stroke());
        state.clear_channel_size(DEFAULT_TEXTURE_SET, "roughness");
        assert!(
            state.can_undo_stroke(),
            "clearing a never-overridden channel must not clear history"
        );
    }

    /// The binding seam's routing contract (audit #12's (c) landing): the
    /// read-only `channel_target` resolves exactly as the write paths do —
    /// the reserved baseColor name to the field, a materialized channel to
    /// its map entry, an unmaterialized channel to `None` (binding must
    /// never materialize). The viewport's height arm rides this: bind when
    /// `Some`, clear when `None`, so the lazy-pool contract holds at the
    /// GPU boundary too.
    #[test]
    fn channel_target_routes_read_only_never_materializes() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut state =
            PaintState::with_target_size(device, queue, 512).expect("paint state builds");
        // baseColor routes to the field (always materialized on 1001).
        assert!(
            state.channel_target(1001, BASE_COLOR_CHANNEL).is_some(),
            "baseColor is present from construction"
        );
        // height: absent until routed.
        assert!(
            state.channel_target(1001, HEIGHT_CHANNEL).is_none(),
            "an unmaterialized channel must read None"
        );
        assert!(
            state.channels_present(1001).is_empty(),
            "and the read must not have materialized it"
        );
        // Route + fill height; the read now resolves, and a read of an
        // UNKNOWN channel still reads None without materializing.
        assert!(state.ensure_channel(1001, HEIGHT_CHANNEL));
        assert!(state.publish_fill_channel(
            1001,
            HEIGHT_CHANNEL,
            512,
            512,
            vec![128u8; 512 * 512 * 4]
        ));
        assert!(state.channel_target(1001, HEIGHT_CHANNEL).is_some());
        assert!(
            state.channel_target(1001, "roughness").is_none(),
            "unknown channels read None"
        );
        assert_eq!(
            state.channels_present(1001),
            vec![HEIGHT_CHANNEL.to_string()]
        );
    }
}
