# Design: per-texture-set layer stacks (C2-9)

Date: 2026-10-15. Author: dragon. Status: LANDED (all four stages +
the export-sets and parked-materials follow-ups, 2026-10-15 — seven
c2-9 kill reports); CLOSED 2026-10-18 — read the dated closure at this
doc's tail. The audit row C2-9: "`Document` holds one stack per set;
switching sets switches stacks; save writes every set". This doc exists
because the naive change is wrong in two places the code makes easy to
miss.

## Today (verified in the tree)

- `Document` holds ONE `LayerStack` + ONE `UndoStack<LayerCommand>`
  (`crates/umber-app/src/document.rs:24`). A save writes one
  `TextureSetLayers` entry (`main.rs`, `write_project_files`), and an
  open loads only the active set's entry (`open_project_at`).
- The project format already holds many sets: `ProjectModel.layers:
  Vec<TextureSetLayers>` (`umber-core/src/project.rs:200`), validated per
  set. The autosave journal tags every entry with its `texture_set`
  (`journal.rs:434, 563`) and replays per set (`:507`). **The file side
  is ready. The session side is not.**
- The paint session already has sets: tiles map to sets
  (`paint_state.rs:1102`, `set_of_tile`, `tiles_of_set`), sets have sizes,
  and the Texture Sets panel switches `active_set`.

## The two hazards

1. **Layer ids collide.** `LayerStack` ids restart at 0 in every stack
   (`layers.rs:924-937`), but paint planes are keyed
   `(PlaneOwner::Layer(id), tile, channel)` with NO set
   (`paint_state.rs:592`, `planes: BTreeMap<(PlaneOwner, u16, String), _>`).
   Two stacks would both own a layer 0, and set B's layer 0 would paint
   into set A's layer-0 planes. The open path already knows this: it
   clears every layer plane before loading, because "layer ids restart
   at 0 in every LayerStack".
2. **The composite reads ONE stack for every tile.** The shell builds one
   `CompositeSnapshot` from `doc.stack` and `flush_composite` composites
   every dirty `(tile, channel)` against it. Swapping the document's
   stack on a set switch would drop the inactive set's layers out of the
   snapshot: their planes would be retired (`note_removed_layers`) and
   their tiles would show only Base. Today one stack spans all tiles, so
   the viewport is consistent. A naive per-set stack makes it worse.

## Design

**Ids: one id space per document.** Each set keeps its own `LayerStack`,
but every stack allocates from a shared counter. Concretely, a
`Document::next_id` is the max over all stacks. A set's stack is
(re)built with `LayerStack::from_parts(layers, global_next)` on creation
and on every switch, and `add_layer`'s allocation updates the global
counter. Ids stay globally unique, so planes keyed by id are already
correct and need NO set dimension. Saved projects: `from_parts` already
takes the max of the saved counter and `max(id)+1`, so loading N sets
and seeding each with the global max is collision-free. Old
single-set files load unchanged.

**History: one per set.** Commands carry ids and indices valid only
against their stack. `Document` keeps `UndoStack` + `SessionHistory` per
set and swaps them with the stack. The one-timeline undo (C2-7) then
walks the ACTIVE set's timeline, which matches Painter: undo is per
texture set. The stroke journal stays session-wide, but every stroke
lands on a tile of a known set. A follow-up can filter stroke rows by
the active set; v1 leaves strokes global and says so.

**Composite: per-set snapshots.** `CompositeSnapshot` gains the set
dimension: `layers_by_set: BTreeMap<String, Vec<SnapshotLayer>>`, and
`flush_composite` composites `(tile, channel)` against
`layers_by_set[set_of_tile(tile)]`. The shell builds the snapshot from
EVERY set's stack each frame, not only the active one. The retire census
(`CompositeSnapshot::removed`) diffs the union, so switching sets
retires nothing.

**Save / open.** The save writes one `TextureSetLayers` per set, plus a
`TextureSet` per set (the import session currently holds one
`session_set`, which becomes a map with it). The open loads every entry
and activates `settings.active_texture_set`.

**Switching.** The Texture Sets panel's selection (already
`paint.set_active_set`) also calls `Document::activate_set(name)`. That
swaps the stack/history pair in and keeps the global id counter. The
Layers panel then shows the new set's stack. Painting targets the active
set's tiles, which is already how `PaintState` routes.

## Stages (each its own kill, in order)

1. **Global ids** (`umber-core` + `document.rs`): the shared counter and
   `from_parts` seeding. Test: two stacks, interleaved adds, no shared
   id. Invisible on its own, which is why it goes first.
2. **Per-set snapshots** (`paint_state.rs` + the shell's snapshot
   builder): the composite reads each tile's own set. Test (GPU): two
   sets, a layer in each, both tiles composite their own layer.
3. **Per-set document** (`document.rs` + `main.rs`): stack + history
   per set, `activate_set`, save/open of every set. Tests: a switch
   keeps both stacks; save → open round trip of two sets; undo after a
   switch acts on the active set only.
4. **Panel + docs**: the Layers panel header names the active set; the
   user guide and keyboard reference updates.

## Open questions

- **Strokes on the one timeline across sets**: global (v1) or filtered
  to the active set?
- **The Bakes and Export panels already iterate sets.** They read the
  composite through `read_target`, so stage 2 makes them correct with
  no change. Stage 2's GPU test pins that.

---

## Closure (2026-10-18 — the design-review kill, verified against the live tree)

All four stages landed the day after this doc was written, plus two
follow-ups. Every symbol below grep-verified against the tree on
2026-10-18:

- **Hazard 1 resolved as designed — global ids** (stage 1,
  `2026-10-15-c2-9-stage1-ids.md`): `LayerStack::next_id` +
  `from_parts(layers, saved_next_id)`
  (`umber-core/src/layers.rs:913/:937`) — one id space per document,
  planes keyed by id need no set dimension, old single-set files load
  unchanged.
- **Hazard 2 resolved as designed — per-set compositing** (stage 2,
  `2026-10-15-c2-9-stage2-composite.md`): the design's
  `CompositeSnapshot.layers_by_set` landed as `set_layers:
  BTreeMap<String, Vec<SnapshotLayer>>` (the implementation's
  reconciling name, recorded in that report) with `rows_for(set)` +
  `set_of_tile(tile)` routing each tile to its own set's rows, and
  `all_rows()` (the union) feeding the retire census so switching sets
  retires nothing — exactly the doc's stage-2 contract.
- **Stage 3 — the per-set document**
  (`2026-10-15-c2-9-stage3-doc.md` + `-stage3b.md`):
  `Document::activate_set(name)` + `active_set()` live
  (`umber-app/src/document.rs:321/:348`); stack + history swap per
  set; save/open round-trips every set.
- **Stage 4 — panel + docs** (`2026-10-15-c2-9-export-sets.md`,
  `-parked-materials.md`): the Layers panel names the active set and
  shows only its stack (the C2-9 stage-4 comment in `layers_ui`); the
  Bakes/Export panels ride `set_of_tile` with no change of their own,
  as the open questions predicted.
- **Open questions, resolved by the landing**: the stroke timeline
  stayed global in v1 (the stage-3b report's boundary, exactly the
  doc's v1 statement); the panels needed no stage-2 changes — the
  design's prediction held.

No section of this doc was falsified by the implementation; the one
reconciling rename (`layers_by_set` → `set_layers`) is recorded where
it belongs, in the stage-2 report. The landing reports (seven c2-9
entries, 2026-10-15) remain the authoritative implementation record.

Residual authority of this doc: the two-hazard analysis (still the
reference for WHY per-set ids are global and WHY the composite unions
across sets) and the stage ordering rationale.
