# Design brief: geometry masks (R-B2)

- Status: DESIGN ONLY. Written 2026-11-12 by a Razul-dispatched subagent
  under kill-lock `razul-geomask-brief` (docs only, no code touched).
- Requirement: wave plan `docs/loop/waves/2026-11-09-wave-plan.md:182`
  and `docs/ROADMAP.md:239`, "Geometry masks (R-B2) | per-mesh/per-tile
  include/exclude on layers/effects | rides UDIM". Census
  `docs/research/spp-parity-census-2026-10-16.md:35` (row B2: per-mesh-name
  and per-UV-tile include/exclude, hide-excluded, copy/paste; ABSENT; mask
  predicate = mesh names x UV-tile sets; rides UDIM tiles which exist; P1)
  and `:114` (tier-1 item 6).

## What exists (measured on 403d54ad)

Layers and masks (umber-core):

- `Layer` (`crates/umber-core/src/layers.rs:920-950`) carries `mask:
  Option<LayerMask>` (935). `LayerMask` (798-803) is metadata only: a name
  and `enabled`. The mask pixels live in the paint session.
- The additive-field precedent is `Layer::material` (944-949):
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, so files
  without the key load and re-save without it. `Layer::fill` (942) writes
  `null` and is the precedent NOT to repeat.
- `LayerCommand` (`layers.rs:1534`) has 13 variants, ending with
  `AttachMask` / `DetachMask` / `SetMaskEnabled` (1633-1650). Exhaustive
  matches over it: `crates/umber-core/src/journal.rs:162` and `:258`
  (forward and revert journal ops), `crates/umber-app/src/history_panel.rs:222-242`
  (one `HistoryKind` per variant, i18n'd label). A new variant therefore
  breaks umber-app's build unless the history panel lands in the same kill.
- `JournalOp` (`journal.rs:48`) mirrors the commands (`SetLayerMask` 134,
  `SetMaskEnabled` 141). A journal line with an unknown op stops replay at
  that line (`journal.rs:20-23`), never a hard error.
- Folder membership is not modeled (`layers.rs:4-7`): the stack is flat.
- Layer construction is local: struct literals of `Layer` exist only in
  `layers.rs` (7 sites); a new field touches no other crate.
- No struct on the project path sets `deny_unknown_fields` (grep over
  umber-core and umber-paint: only a comment in `smart.rs:60`), so an older
  build loads a file that carries a new key and silently drops it.
- Project versions (`crates/umber-core/src/project.rs:39-56`):
  `CURRENT_PROJECT_VERSION = 1`, `LAYER_PLANES_PROJECT_VERSION = 2`
  (bumped because an old build would show a canvas missing paint),
  `MAX_SUPPORTED_PROJECT_VERSION = 2`. Layer files are
  `layers/<set>/<id>.json`, each capped at `MAX_LAYER_FILE_BYTES = 64 MiB`
  (64).
- Name safety rules: `WINDOWS_RESERVED_STEMS` (`project.rs:549`),
  `validate_set_name` (562) and `is_valid_channel_name`
  (`crates/umber-core/src/lib.rs:235`) refuse separators, reserved
  characters, control characters, dot-only names, trailing dot/space and
  device stems by stem. These guard names that become directory names.

UDIM tiles (umber-mesh, umber-app):

- `crates/umber-mesh/src/udim.rs`: `tile_of_uv` (48, `1001 + floor(u) +
  10*floor(v)`, clamped to the grid), `tile_of_triangle` (62, the FIRST
  vertex's tile wins), `filter_mesh_for_triangles` (95),
  `triangles_for_tile` (122), `present_tiles` (135). Grid is
  `1001..=LAST_TILE` with `LAST_TILE = 1100` (41). Integration tests:
  `crates/umber-mesh/tests/udim_tiles.rs` (476 lines).
- `crates/umber-app/src/tile_selection.rs:27` `TileSelection`: the
  Bakes/Export tile selector (all present tiles selected by default,
  deselections survive list changes). It is the UI precedent for a tile
  checklist.

Mesh names (umber-mesh):

- `MeshData` (`crates/umber-mesh/src/lib.rs:60-76`) has `material_names`
  plus `triangle_materials: Vec<u16>` (one material index per triangle,
  empty = "one material"; all-or-nothing at file scope). There is NO
  per-object or per-sub-mesh name. Every importer drops it today: OBJ reads
  `model.name` only for an error message (`lib.rs:344-358`), glTF walks
  `document.meshes()` without reading the mesh name (`gltf.rs:93`, names
  only materials at 286), FBX reads material names only (`fbx.rs:45-62`),
  USD keeps the prim name in `raw.name` for errors (`usd.rs:959`).
- `MeshData` has 122 struct-literal construction sites; 107 of them list
  every field with no `..Default::default()`, across 31 files (counted by
  script over `crates/**/*.rs`). A new `MeshData` field means editing 107
  literals in 31 files.
- Texel walks: `for_each_uv_geometry` (`crates/umber-mesh/src/uv_gbuffer.rs:485`,
  takes a per-triangle `filter`) and `UvGBuffer::build` (358). UV gutter
  padding: `DilationMap` (`crates/umber-core/src/uv_dilation.rs:35`).

Composite path (umber-paint):

- `SnapshotLayer` (`crates/umber-paint/src/paint_state.rs:981-1003`) is the
  per-row composite metadata, with `mask_enabled` (1000). 31 struct
  literals: 21 in `paint_state.rs`, 6 in `snapshot.rs`, 4 in
  `crates/umber-app/tests/pool_perf.rs`, none using `..Default`.
- `CompositeSnapshot::rows_for(set)` (1055) picks a tile's rows by set;
  `hidden_sets` (1050) already drops a whole set's rows.
- The snapshot diff decides which rows re-composite (1126-1137): any field
  change in a row marks it changed.
- `resolve_composite_entries` (3973) resolves each row through
  `resolve_row` (4015). The layer-mask gate is there (4057-4065): the
  `(layer, tile)` plane from `layer_masks` (1412) is resampled
  (`resample_mask_nearest`, `crates/umber-paint/src/material_layer.rs:521`)
  into `owned_masks` and multiplied in by the compositor.
- `read_composite` (4294, used by export `crates/umber-app/src/main.rs:799`
  and `crates/umber-cli/src/agent.rs:1919`) and `bake_layers` (4407, the
  flatten, resolving rows at 4449) both go through the same resolve path.
  A gate added in `resolve_row` therefore reaches viewport, export and
  flatten together.
- `build_composite_snapshot` (`crates/umber-paint/src/snapshot.rs:25`)
  turns the document stack into rows; it sets `mask_enabled` from
  `layer.mask` (46).
- `PaintState::set_mesh` (1912) has NO production caller: the only calls
  are in test modules (`main.rs:7673` sits in `mod pool_stage4_tests`
  opened at 7644; `bake_sources.rs:1429` in `mod tests` at 681). The paint
  session holds no mesh in production; mesh-dependent work takes the mesh
  as a parameter (`bake_and_select_id`, 2754). The shell holds the mesh in
  `state.mesh`, set by `load_mesh_path` (`main.rs:3068-3087`).
- The ID-bake cache precedent: a FIFO ring capped at
  `ID_BAKE_CACHE_CAP = 3` (`paint_state.rs:548`).
- The quick mask / ID-mask path (`crates/umber-brush/src/id_mask.rs`) is a
  per-set stroke gate, not a per-layer composite gate; unrelated.

Clipboard and multi-layer edits (umber-app):

- Session clipboard precedent: `BakeSync`'s clipboard
  (`crates/umber-app/src/bakes_panel.rs:741`, "never persisted").
- Multi-selection edits issue one undoable command per layer
  (`crates/umber-app/src/layers_panel.rs:1133-1140`).

"Effects": umber has no per-layer effect stack today (census row B3:
per-channel effect enable and folder effects unverified, paint-effect input
absent). The only per-layer gate is the layer mask.

## Target

### The mask predicate

```rust
// umber-core/src/layers.rs
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometryMask {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub excluded_meshes: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub excluded_tiles: BTreeSet<u16>,
}
// on Layer:
#[serde(default, skip_serializing_if = "GeometryMask::is_empty")]
pub geometry_mask: GeometryMask,
```

A triangle `t` is INCLUDED for a layer iff `mesh_of(t) ∉ excluded_meshes`
and `tile_of_triangle(t) ∉ excluded_tiles`. The model stores exclusions,
not inclusions, for three reasons: the empty mask is the identity (every
pre-feature layer is unmasked with no key on disk); a mesh or tile that
appears later (reimport, painting into a new tile) is included by default,
which matches `TileSelection`'s "all present" default; and a mask naming a
mesh or tile the current mesh lacks stays inert data instead of an error,
so reloading a different mesh revision loses nothing. The UI shows include
checkboxes; unchecking writes an exclusion.

Per-texel semantics:

- Tile exclusion is exact per tile: a row whose layer excludes the tile
  contributes NO entry for that tile in `resolve_row` (no plane needed).
- Mesh exclusion is a coverage plane per `(layer, tile)`: 255 on texels
  covered by included triangles of that tile, 0 on texels covered only by
  excluded ones. Overlapping UVs (a texel covered by both): included wins,
  because hiding paint the user can see on an included mesh is the worse
  error. Uncovered gutter texels take the nearest covered texel's value
  through `DilationMap`, so island borders do not seam. The plane
  multiplies with the layer mask (both gates apply, the existing
  `cavity x quick` multiplication rule).
- Applies to every row kind that composites: Paint, Fill, Material (all
  its channel entries), Instance (the instance's OWN geometry mask, like
  its own mask overlay; the source's mask does not travel). Folder: stored
  and editable, inert until folder membership exists (non-goal below).
- "Effects": in today's model the layer mask is the only effect-like gate
  on a row, and the geometry gate multiplies after it, so the requirement's
  "layers/effects" is met for every gate the tree has. Per-effect geometry
  masks belong to the B3 effect-stack lane.
- Paint planes are never edited by the mask. A stroke over excluded
  geometry writes into the plane and composites hidden; removing the
  exclusion shows it. Non-destructive, matching the layer mask.

### Mesh names

`MeshData` gains no field (107 literals in 31 files would churn). A new
side table carries the parts:

```rust
// umber-mesh/src/lib.rs
pub struct MeshParts {
    pub names: Vec<String>,        // unique, importer order
    pub triangle_parts: Vec<u16>,  // one per triangle, or empty
}
pub fn load_with_parts(path: &Path) -> Result<(MeshData, MeshParts), ImportError>;
```

`load()` stays byte-identical (it calls `load_with_parts` and drops the
parts). Part name per importer: OBJ `model.name`, glTF mesh name (else the
node name), FBX the owning node name, USD the Mesh prim path's last
component. An empty or missing name becomes `mesh <n>` by first-seen order
(deterministic across reloads of the same file). Same-name objects fold
into one part. All-or-nothing like `triangle_materials`: any triangle
without a part leaves `triangle_parts` empty, and then mesh exclusions are
inert (tile exclusions still apply). The panel lists only parts with at
least one triangle on the active set's tiles.

### Coverage function (GPU-free)

`umber_mesh::geometry_coverage(mesh, parts, excluded: &BTreeSet<String>,
tile, width, height) -> Option<Vec<u8>>`, in a new
`crates/umber-mesh/src/geometry_mask.rs`. `None` when no triangle on the
tile is excluded (fully open; the gate adds no work). It walks triangles
with `for_each_uv_geometry`'s filter shape (triangle-driven, never a
per-texel scan of all triangles, per the perf-kill rule), then dilates.
umber-mesh sits below umber-core, so it takes std types, not
`GeometryMask`.

### Paint session gate

- `SnapshotLayer` gains `geometry: GeometryMask` (cloned from the layer
  by `build_composite_snapshot`). The snapshot diff compares it, so an edit
  re-composites exactly the edited row.
- `PaintState::set_geometry_source(Option<Arc<(MeshData, MeshParts)>>)`,
  called by the shell's `load_mesh_path` (and cleared on mesh unload).
  This is new production state: today the session holds no mesh.
- `resolve_row` order: excluded tile → return `None`; else if the row has
  excluded meshes and a source is set, fetch the coverage plane from a
  cache keyed `(source generation, tile, width, height, excluded set)`,
  FIFO-capped (start at 8, the `ID_BAKE_CACHE_CAP` precedent), and
  multiply it into the row's owned mask (creating one at 255 when the row
  has no layer mask).
- Cost claim owed by the K3 kill: the coverage build on a 4096² tile with
  a 100k-triangle fixture, measured, written into the doc comment. The
  fixture must carry the cost class (not a 2-triangle quad).

### Hide excluded

A session-only viewport toggle, off by default, never persisted, never
read by export or bake. While on, the viewport draws only the triangles the
ACTIVE layer includes (both predicates, triangle-level via
`tile_of_triangle` and `triangle_parts`), and picking skips hidden
triangles so a stroke reaches geometry the hidden part occluded. It
re-derives the triangle list when the active layer, its mask, or the mesh
changes. If the viewport's pick path has no per-triangle filter seam, the
K5 kill lands draw-only and names pick-through as its follow-up row
instead of widening into the BVH.

### Copy / paste

- Copy reads the active layer's `GeometryMask` into a session clipboard
  (never persisted, the `BakeSync` precedent).
- Paste REPLACES the mask on every selected layer, one
  `LayerCommand::SetGeometryMask { id, new, old }` per layer (the
  layers-panel multi-selection precedent), each undoable. Empty clipboard
  → no-op with a notice.
- Panel bulk actions over the visible checklist: include all, exclude
  all, invert. Each is one command on the active layer.

### Command, journal, history

- `LayerCommand::SetGeometryMask { id, new: GeometryMask, old:
  Option<GeometryMask> }`, apply/revert like `SetMaskEnabled`.
- `JournalOp::SetGeometryMask { id, mask: GeometryMask }` both directions.
- `HistoryKind::SetGeometryMask` with `history.op-set-geometry-mask` in
  both catalogs.
- `Layer::memory_bytes` counts the mask's heap strings.

## Concurrency model

- Runtime: `PaintState` and the document are owned by the UI thread; the
  coverage build runs synchronously inside the composite flush, cached.
  No new threads, no new locks. The cache is invalidated by key (source
  generation bumps on every `set_geometry_source`), never by a shared
  flag.
- A geometry edit during a stroke is allowed: it changes composite
  metadata only (like opacity), not the stroke's plane, so the stroke
  snapshot stays valid.
- Crash model: the mask rides the layer JSON and the journal; it is
  covered by the existing project commit and journal replay. No new
  on-disk file, no new commit protocol.
- Agent lanes: each implementation kill below takes a kill lock over its
  full file list at dispatch, and the surfaces are disjoint where kills
  run in parallel. Kills that share a dependency are sequenced, not
  co-locked.

## Security model

- Mesh part names come from untrusted import files. They are never path
  segments: they live inside `layers/<set>/<id>.json` and in memory only.
  The Windows-reserved-stems lesson from the channel-manage cross-review
  applies the moment a name becomes a file or directory name; this design
  has none. Any later kill that writes per-mesh sidecars must reuse
  `is_valid_channel_name` / `validate_set_name`, not a new rule.
- Bounds at load (umber-core, the layer deserialize path): at most 4096
  excluded mesh names per layer, each at most 1024 bytes; at most 100
  excluded tiles (the grid size). Over the cap is a `ProjectError` naming
  the layer, never a silent truncation (a truncating load would break
  byte-identical re-save). The 64 MiB layer-file cap already bounds the
  worst case; the per-mask caps keep the coverage cache key small.
- Out-of-grid tile ids (outside `1001..=1100`) are kept as inert data:
  they cannot match a triangle, and keeping them preserves re-save bytes.
- Importer-side: part names pass through the importers' existing size
  caps (`read_capped`, `lib.rs:209`); names longer than 1024 bytes are
  truncated at a char boundary AT IMPORT (before any mask can store them),
  so a stored exclusion always matches the imported name.
- Display: control and bidi-override characters in part names render
  escaped in the panel (the name is shown, never interpreted).

## Migration / rollout

- `project.json` and layer files: the field is
  `#[serde(default, skip_serializing_if = "GeometryMask::is_empty")]`, the
  `Layer::material` precedent. Old files load with an empty mask and
  re-save byte-identically (pinned by a golden test).
- No version bump: admission does not change (this build loads every
  file it loaded before, and old files never carry the key). The
  downgrade behavior is named, not hidden: an older build loading a file
  with masks ignores the key (no `deny_unknown_fields`), composites the
  layer unmasked and drops the mask on its next save; an older build
  replaying a journal line with `SetGeometryMask` stops replay at that
  line. Both match the `material` field's precedent, which also shipped
  without a bump. If the partner wants old builds to refuse such files,
  that is a separate decision (bump to 3 only when a live layer carries a
  non-empty mask, the `LAYER_PLANES_PROJECT_VERSION` pattern).
- Rollout order: K1 + K2 (inert data, no visible change), K5 (viewport
  seam, unused), K3 (gate live; empty masks byte-identical), K4 (UI turns
  it on), K6 (CLI parity), K7 (docs). Each kill is shippable alone; until
  K4 no user can create a non-empty mask.

## Verification plan

K1 (core): `layers::tests::geometry_mask_default_is_empty`,
`set_geometry_mask_undo_restores_old`, `geometry_mask_memory_bytes_counts_names`;
`project::tests::pre_geomask_layer_resaves_byte_identical` (golden: an
inline pre-feature layer JSON literal, load, save, compare bytes),
`geometry_mask_round_trips`, `geometry_mask_over_cap_is_refused`;
`journal::tests::set_geometry_mask_replays_after_crash`;
`history_panel` kind/label test; the existing i18n catalog-completeness
test covers both `.ftl` files.

K2 (mesh): `obj_objects_become_parts`, `gltf_meshes_become_parts`,
`fbx_nodes_become_parts`, `usd_prims_become_parts`,
`missing_part_leaves_parts_empty` (all-or-nothing), `unnamed_part_gets_ordinal_name`,
`load_matches_load_with_parts_mesh` (byte-equal `MeshData`);
`geometry_coverage_none_when_nothing_excluded`,
`geometry_coverage_zeroes_excluded_part_texels`,
`geometry_coverage_overlap_included_wins`,
`geometry_coverage_dilates_gutters`; the 4096² cost measurement recorded.

K3 (paint, headless-device pattern of the crate): `excluded_tile_row_contributes_nothing`,
`excluded_mesh_multiplies_with_layer_mask`, `empty_geometry_mask_composite_byte_identical`
(golden against the pre-change composite), `read_composite_honors_geometry_gate`,
`flatten_honors_geometry_gate`, `geometry_edit_marks_only_that_row`,
`instance_uses_own_geometry_mask`, `no_source_makes_mesh_exclusion_inert`.

K4 (app, kittest): `layers_panel_geometry_section_snapshot`,
`geometry_uncheck_writes_one_command`, `paste_geometry_mask_one_command_per_selected`,
`paste_empty_clipboard_is_noop`, `invert_flips_visible_checklist_only`.

K5 (viewport): `hidden_triangles_skip_draw_list`,
`hidden_triangles_skip_pick` (or the named follow-up), `hide_toggle_is_not_persisted`.

K6 (CLI): `agent_export_honors_mesh_exclusion`.

Battery per kill: rustfmt --check on touched `.rs`, `scripts/check-layers.py`,
`scripts/scorecard.py --check`, clippy + tests for the touched crates
(`UMBER_ASSETS_DIR=<repo root>` in the main tree).

## Kill breakdown (disjoint surfaces)

| Kill | Surface (the lock's full file list) | Depends on |
| --- | --- | --- |
| K1 `geomask-core` | `crates/umber-core/src/layers.rs`, `crates/umber-core/src/journal.rs`, `crates/umber-core/src/project.rs`, `crates/umber-core/src/lib.rs`, `crates/umber-app/src/history_panel.rs`, `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl` (the golden is an inline byte literal in `project.rs`'s tests; umber-core has no `tests/` dir) | none |
| K2 `geomask-mesh-parts` | `crates/umber-mesh/src/lib.rs`, `gltf.rs`, `fbx.rs`, `usd.rs`, `usdc.rs`, new `crates/umber-mesh/src/geometry_mask.rs`, new `crates/umber-mesh/tests/geometry_parts.rs` | none (parallel with K1) |
| K5 `geomask-viewport-hide` | `crates/umber-app/src/viewport.rs` (plus `crates/umber-gpu/src/renderer.rs` only if the draw needs a second index buffer) | none (takes a triangle list; parallel with K1/K2) |
| K3 `geomask-paint-gate` | `crates/umber-paint/src/paint_state.rs`, `crates/umber-paint/src/snapshot.rs`, `crates/umber-app/tests/pool_perf.rs` | K1, K2 |
| K4 `geomask-panel` | `crates/umber-app/src/layers_panel.rs`, `crates/umber-app/src/main.rs`, `assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl` | K1, K3, K5 (`.ftl` overlaps K1: sequenced after it) |
| K6 `geomask-cli-parity` | `crates/umber-cli/src/agent.rs` | K3 |
| K7 `geomask-docs` | `docs/ROADMAP.md`, `docs/research/spp-parity-census-2026-10-16.md` (B2 row to LANDED), `docs/loop/queue.md` | K4 |

K1, K2 and K5 share no file and can run in parallel. `main.rs` is only in
K4 (the `load_mesh_path` source handoff and the hide-excluded wiring both
ride it), so no two kills co-own it.

## Non-goals

- Folder geometry masks applying to children (needs folder membership,
  `layers.rs:4-7`).
- Per-effect geometry masks (needs the B3 effect stack).
- Geometry masks on bakes (the Bakes panel already has per-set and
  per-tile scoping via `TileSelection`).
- A smooth or feathered geometry edge (the gate is binary per texel, plus
  dilation).
- Splitting triangles that straddle tiles (the v1 first-vertex rule of
  `udim.rs:9-12` stands).
- Persisting hide-excluded or the clipboard.
- A project version bump (see Migration; partner decision if wanted).
