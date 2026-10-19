# Kill report: per-set masked bakes — the ray half BUILT (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, under Bessa's live steer
"do all the work that you can locally … and do not stop"). Kill-lock
`masked-bakes-ray-half` (the brief's lock — this kill is the brief's
build), holding `crates/umber-app/src/bakes_panel.rs`,
`crates/umber-app/src/bake_sources.rs`, and
`docs/loop/briefs/2026-10-18-masked-bakes-ray-half.md`.

Context: the masked-bakes design's closure tail named the ray half
as the next code work — AO/thickness/bent kept the whole-mesh bake
while the raster maps already baked per-set. The scout (the brief,
parked an hour earlier the same evening) found all three bakers'
`_with_occluder` variants and the panel's submesh plumbing already
live; the build is the brief's six call-site changes, executed
exactly.

## Built (per the brief, item for item)

1. `bake_sources::bake_ao_with_occluder(raster, occluder, size, rays)`
   — mirrors `bake_ao` through `bake_ao_mesh_with_occluder`; the
   Export dialog's shared `bake_ao` call site untouched (its
   whole-mesh semantics preserved).
2. `bake_sources::bake_ao_tile_with_occluder(raster, occluder, …,
   tile)` — the tile filter applied to the RASTER only
   (`filter_mesh_for_tile` + `bake_tile_window`, both public), the
   occluder passed whole.
3. The Ao arm: `match (job.source, raster_is_submesh)` — mapped
   WholeMesh/Tile jobs bake (submesh, whole); unmapped arms keep the
   exact prior calls. Margins: mapped → `raster_margin` (per-submesh
   seams); the tile cache key widened to `(u16, bool)` so the
   submesh-filtered and whole-filtered tile meshes never collide;
   unmapped paths byte-identical.
4. The Thickness arm: mapped →
   `bake_thickness_mesh_with_occluder(submesh, whole)` +
   `raster_margin`; unmapped untouched.
5. `bake_extra_map` gains `occluder: Option<&MeshData>`: `None` =
   today's single-mesh calls byte-identical (both test call sites
   updated to `None`); `Some(mesh)` = BentNormal bakes through
   `bake_ao_and_bent_mesh_with_occluder`. The extra-map arm's mapped
   condition extends to BentNormal for BOTH the bake and the margin
   (a half-edit the build caught before gating: mapped Bent would
   have padded against the whole mesh's seams).
6. Both stale comments retired to the landed truth (transfer stays
   whole-mesh by design — its rays come from the high-poly group).

## The load-bearing test (GPU, at the battery)

`mapped_ray_maps_raster_the_submesh_but_occlude_cross_material` — a
ground quad (material 0) with a floating slab (material 1) over
half of it: the ground's AO rasterized from its OWN submesh darkens
under the slab when the occluder is the whole mesh (rays cross
materials — the property a submesh-only bake would LOSE), while a
ground-only occluder stays fully lit (the control, min covered texel
= 255), with identical coverage in both (same raster). The API
assumption corrected during the build: `submesh_for_material` is a
`&self` method returning `MeshData` directly (the never-silent-empty
contract), not an associated Option.

## Judgment calls

1. The tile-seams cache key change `(u16, bool)` is structural but
   output-neutral: unmapped tiles hit the same filtered mesh + seams
   under the new key, byte-identical.
2. The Export dialog's raw-bake path is untouched by design — its
   per-set story is a named follow-up, not this kill's.
3. The panel-level per-set divergence gate (Body ≠ Hilt outputs) is
   structurally guaranteed by the raster trio's existing per-set
   tests plus the new occlusion pin; the real-corpus assertion rides
   the battery's full suite.

## Unverified (the honest boundary)

- No compile/test run (host down since 10-17 21:48; the outage
  contract parks the battery). Compile risk swept statically: every
  signature matched against the live tree this session (the three
  `_with_occluder` fns, `filter_mesh_for_tile`/`bake_tile_window`
  pub-ness, `submesh_for_material`'s `&self` shape, `try_request_device`'s
  Option, the existing test literals' field-complete MeshData); the
  two test-module call sites the census caught are patched; no other
  `bake_extra_map`/`bake_ao` callers exist in umber-app (grep-clean).
- The new GPU test runs at the battery (scope: umber-app; the
  existing GPU suite's device-harness idiom followed exactly).

## Gates run (host-free, this session)

- `rustfmt --check --edition 2021`: CLEAN on both files.
- `scripts/verify-kill.sh`: PASS on both files (brace census, trap
  greps, doc-attachment, lock audit).
- Focused verifier: the brief's anchors re-checked + the build's
  diff census (every mapped arm calls the with_occluder variants
  with (raster, whole); every unmapped arm byte-preserved; the
  occlusion test present with its control) + manifest pins.

State: STATICALLY VERIFIED, IN FLIGHT — battery (scope: umber-app +
umber-bake) + commit + push parked on the build host's return, its
own commit row; the lock releases at that commit. The brief's test
gates 1/3/4 (real-corpus divergence, unmapped no-regression, the
per-set margin) ride the battery's existing suite + this test.
