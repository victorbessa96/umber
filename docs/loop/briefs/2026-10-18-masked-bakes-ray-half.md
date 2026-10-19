# Brief: per-set masked bakes — the ray half (§3's AO/thickness/bent occluder split)

Written 2026-10-18 (host-free scouting pass, the design's
`per-set-masked-bakes.md` closure tail's named work item (a)).
One code kill, fully scouted against the live tree: every symbol,
line anchor, and call site below was verified this date.

## The contract

A mapped mesh (`triangle_materials` non-empty) bakes each enabled
set's RAY maps from the set's SUBMESH as raster, against the WHOLE
mesh as occluder — rays cross materials (the design's rule), UV
coverage is the set's. An UNMAPPED mesh keeps today's byte-for-byte
output: every existing call shape is preserved on the unmapped path.
No UI, no i18n (the OccluderScope enum is the separately-gated
follow-up (b) — NOT this kill).

## The machinery that already exists (verified live)

- Baker variants, all three ray bakers:
  `umber_bake::ao::bake_ao_mesh_with_occluder` (ao.rs:581),
  `bake_ao_and_bent_mesh_with_occluder` (ao.rs:604),
  `thickness::bake_thickness_mesh_with_occluder` (thickness.rs:231).
  The plain fns delegate with `(mesh, mesh)` — the split is already
  the baker-level truth.
- The panel's job loop already carries the per-job submesh:
  `raster_mesh: Option<&MeshData>`, `raster_source`,
  `raster_is_submesh` (bakes_panel.rs:2061-2072).
- The margin machinery for submesh rasters: `raster_margin`
  (bakes_panel.rs:1974-1989) — per-submesh seam graphs cached in
  `set_seams: HashMap<*const MeshData, _>`, whole-mesh seams in
  `whole_seams`; the raster maps (curvature :2156, position :2198,
  the extra-map raster trio :2232) already route through it.
- `bake_sources::bake_ao` (:202) / `bake_ao_tile` (:218): the
  AO helpers SHARED with the Export dialog (the panel dilates
  afterward; the export path uses the raw bake — bake_sources docs).

## The changes (bakes_panel.rs + bake_sources.rs only)

1. **`bake_sources::bake_ao_with_occluder(device, queue, raster,
   occluder, size, rays)`** — a sibling to `bake_ao` calling
   `bake_ao_mesh_with_occluder`. The Export dialog's call site is
   UNTOUCHED (whole-mesh semantics preserved there); the panel's
   mapped arm uses the new one.
2. **`bake_sources::bake_ao_tile_with_occluder(device, queue, raster,
   occluder, size, rays, tile)`** — mirrors `bake_ao_tile`'s internal
   tile filter (`ao::filter_mesh_for_tile`) applied to the RASTER
   argument only; the occluder passes whole. The panel's
   `BakeSource::Tile` + mapped arm uses it.
3. **bakes_panel.rs, the `Ao` arm (:2102-2145)**: when
   `raster_is_submesh`, WholeMesh jobs call the new
   `bake_ao_with_occluder(raster_source, mesh, ...)`, Tile jobs call
   `bake_ao_tile_with_occluder(raster_source, mesh, ..., job.tile)`;
   margin via `raster_margin(..., raster_source, true, mesh, raw)`.
   Unmapped: the exact current calls (`bake_ao(mesh)` /
   `bake_ao_tile(mesh, tile)`, `whole_seams`/`tile_seams` margins).
4. **The `Thickness` arm (:2169-2184)**: mapped →
   `bake_thickness_mesh_with_occluder(device, queue, raster_source,
   mesh, size, size, &params)` + `raster_margin`; unmapped → today's
   `bake_thickness_mesh(mesh)` + `whole_seams` path, untouched.
5. **The `BentNormal` branch of the extra-map arm (:2211-2226)**:
   settled by scout — `bake_extra_map` (:2276) routes BentNormal to
   `umber_bake::ao::bake_ao_and_bent_mesh(...)` directly. Give
   `bake_extra_map` an `occluder: Option<&MeshData>` argument: `None`
   = today's semantics exactly (the unmapped call stays
   byte-identical); `Some(occluder)` = the BentNormal arm calls
   `bake_ao_and_bent_mesh_with_occluder(raster, occluder, ...)` (the
   raster trio ignores the arg — they already take their mesh
   explicitly). The mapped BentNormal branch passes
   `Some(mesh)` with `raster_source` as the bake arg; margin via
   `raster_margin`.
6. **The comment at :2065-2068 updates** — "RAY maps keep the whole
   mesh … the with_occluder split is the named follow-up" becomes
   the landed truth ("RAY maps raster the set's submesh against the
   whole-mesh occluder; transfer keeps whole" — transfer is
   high-mesh rays by design, out of scope).

## Test gates (at the battery; GPU)

1. Two-material fixture (the FBX corpus's; §2's importer tests
   already load it): Body's and Hilt's AO outputs DIFFER, each
   covering only its own islands — the design's gap example closes
   for AO/thickness/bent.
2. Cross-material occlusion PINS: a Body island geometrically under
   a Hilt overhang darkens (rays hit the whole mesh) — this is the
   property a naive submesh-only bake would LOSE; assert it
   explicitly.
3. Unmapped mesh: every ray map byte-identical to the pre-kill
   output (the no-regression contract; the raster maps' existing
   tests already pin their half).
4. The margin: a mapped set's ray map dilates from its own submesh
   seams (no bleed across sets).

## Rollout

One kill (lock: `masked-bakes-ray-half` over bakes_panel.rs +
bake_sources.rs + this brief), parked host-free, battery at the
recovery (umber-app + umber-bake scopes; the GPU gates above). The
OccluderScope enum + panel row (follow-up (b)) stays double-gated on
the i18n catalogs; §5's auto-mapping (follow-up (c)) is separate.
