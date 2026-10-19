# Per-set masked bakes (design, C2-9 follow-up)

Status: IMPLEMENTED in stages (verified 2026-10-18 — see the
"Verified truth, 2026-10-18" tail below: §1-§2 complete, §3's raster
half + §4 landed, the ray-map occluder split and the OccluderScope
UI remain the named follow-ups).

## The gap

The Bakes panel's per-set checklist writes the SAME whole-mesh bake under
every enabled set's name (`bakes_panel.rs` `bake_jobs`: "per-material
masked bakes are the named follow-up"). On a two-material mesh, Body's
`Body_ambient_occlusion.png` and Hilt's `Hilt_ambient_occlusion.png` are
byte-identical, and each carries the other material's islands.

Export (`35b0a2f`) and the composite now treat sets separately, so the
bake output is the last whole-mesh artifact in a per-set pipeline.

## The missing fact

`umber_mesh::MeshData` knows WHICH materials exist (`material_names`) but
not which triangle uses which. Every importer drops that mapping:

Checked in the registry sources (2026-10-15): `ufbx 0.11.5`
`Mesh::face_material: List<u32>`, `tobj` `Mesh::material_id:
Option<usize>`, and `gltf` `Material::index() -> Option<usize>`.

| importer | where the material is known | today |
|---|---|---|
| FBX (`fbx.rs` `append_mesh`) | `mesh.materials` + per-face `face_material` | names only |
| glTF (`gltf.rs`) | one material per primitive | names only |
| OBJ (`lib.rs`, tobj) | `mesh.material_id` per model | names only |
| USD (`usd.rs`, `usdc.rs`) | `GeomSubset` / `material:binding` | nothing |

## Design

1. **`MeshData.triangle_materials: Vec<u16>`**: one index into
   `material_names` per triangle. Empty means "one material" (every
   existing mesh and test literal stays valid with `..Default::default()`).
   It is filled by:
   - FBX: `face_material[face]`, repeated once per triangle the face emits;
   - glTF: the primitive's material index, repeated per triangle;
   - OBJ: each model's `material_id`;
   - USD: later. Empty is honest until subsets parse.
2. **`MeshData::submesh_for_material(i) -> MeshData`**: the triangles of
   material `i`, with the full vertex arrays kept so UVs stay identical.
   This is the same shape as `triangles_for_tile`, already used for the
   per-tile AO bake.
3. **Bakes:** `bake_jobs` bakes each enabled set from its own submesh.
   Rays still hit the WHOLE mesh, because AO/thickness occluders cross
   materials. Only the rasterized UV coverage is the set's. The bakers
   take `(raster mesh, occluder mesh)`; today they are the same argument.
   - Curvature, position, normal and ID need only the raster submesh.
   - AO, thickness, bent normal and transfer need the occluder split.
4. **Paint tiles**: `assign_tile_to_set` already exists. A multi-material
   mesh's sets map to the tiles their UVs occupy (`triangles_for_tile` ∩
   material triangles), so the composite's per-set rows line up without
   manual assignment.

## The occluder scope (researched 2026-10-15)

The reference tools agree on the default, and every one of them also
exposes a scope:

- **Substance 3D Painter** (Mesh map settings, AO/thickness/bent
  normals): "Self occlusion — select which meshes should affect ambient
  occlusion for the current mesh", with *Always* and *Only Same Mesh
  Name*. The default is Always, i.e. the whole mesh, which is Umber's
  rule above.
- **Marmoset Toolbag** bake groups: "Ignore Groups" isolates each
  group's AO/thickness/lighting rays, and a per-mesh "Exclude When
  Ignoring Groups" opts single meshes back in.
- **The user case, a real request** (Adobe community, 2023): "AO for the
  gauges, but the glass texture set will darken the result". Adobe's PM
  answered that no per-set exclude exists: rename the mesh and use *Only
  Same Mesh Name*, which the asker rejected with more than two sets.

So Umber's design takes the default plus the scope Painter lacks:

```
enum OccluderScope { WholeMesh /* default */, OwnSet, Sets(BTreeSet<u32>) }
```

- `OwnSet` is Marmoset's Ignore Groups.
- `Sets` names the occluding materials: the glass case is "everything
  but Glass", which Painter cannot express.
- It lives in the per-set bake settings and changes only the occluder
  argument from §3. The raster submesh is untouched.
- The parameter applies to AO, thickness and bent normal. Transfer takes
  rays from the HIGH mesh, so its scope is the high-poly group, not
  this.

Ship order: `WholeMesh` lands in step 3 (no UI, matching today's
output byte for byte); the enum and the panel row are step 4's. This
keeps step 3's acceptance a pure no-regression check.

## Order (each a kill, all host-gated)

1. **Core:** the field, `..Default::default()` at the 138 struct-literal
   sites that list fields explicitly (a mechanical pass, compiler-checked
   per crate), and `submesh_for_material` with tests.
2. **Importers:** FBX + glTF + OBJ fill it. A two-material fixture per
   format (the FBX corpus has one; glTF can be generated).
3. **Bakers:** the raster/occluder split, starting with AO, which needs
   it most.
4. **Panel:** per-set bake jobs.
5. **Auto-assign:** tiles to sets.

## Why not now

The field touches 138 struct literals across 7 crates. Without a
compiler, one missed site breaks the build and the next wave's gate. A
compiler-guided pass on the host is minutes, while a blind edit here
risks the gate. This doc sets the order so the first host-up hour starts
on code.

## Verified truth, 2026-10-18 (the closure tail — every claim
grep-verified against the live tree this date)

The "no code" line above went stale; the implementation landed in
stages. What the tree carries today, by the doc's own order:

1. **§1 — LANDED.** `MeshData.triangle_materials: Vec<u16>` is a
   live field (`umber-mesh/src/lib.rs:66`), empty = whole-mesh by
   default (`:96`), and `submesh_for_material` exists with the
   never-silent-empty contract (`lib.rs:87`).
2. **§2 — LANDED, all three importers.** glTF: the all-or-nothing
   per-primitive attach (`umber-mesh/src/gltf.rs:23-40`, the
   mixed-named/unnamed file keeps the whole-mesh fallback). FBX:
   per-face `face_material` slots repeated per emitted triangle
   (`fbx.rs:56-143`). OBJ: tobj's per-model material pass
   (`lib.rs:240-260`). USD stays honestly empty, per the design.
3. **§3 — the RASTER half LANDED, the ray half deliberately not.**
   Raster maps (curvature/position/normal/ID) bake each enabled set
   from its own submesh — the job tuples carry it
   (`bakes_panel.rs:638-640`, built at `:1781-1815`), and the
   margin path knows the submesh seams (`:1974-1989`). RAY maps
   (AO/thickness/bent/transfer) keep the WHOLE mesh, in the tree's
   own words: "occluders cross materials, and the baker's
   with_occluder split is the named host-gated follow-up"
   (`bakes_panel.rs:2061-2068`). So the doc's gap example is HALF
   closed today: Body's and Hilt's curvature/position/normal/ID
   differ per set, but their AO/thickness/bent remain the shared
   whole-mesh bake under each name until the occluder split lands.
4. **§4 — LANDED.** `bake_jobs` fans out per enabled set, each set's
   raster from `dispatch.mesh.submesh_for_material(slot)`
   (`bakes_panel.rs:1797-1800`), the `None` arm keeping today's
   byte-for-byte whole-mesh behavior for unmapped meshes.
5. **§5 — the tooling exists.** `PaintState::assign_tile_to_set`
   is live with heavy usage (the persistence restore path included);
   the design's full triangles∩materials AUTO-mapping remains the
   last named step.

**The named remaining work, in order:** (a) the `with_occluder`
split in `umber-bake` (AO/thickness/bent gain the (raster,
occluder) arguments — the doc's §3 ray half, host-gated by its own
compiler rule), (b) the `OccluderScope` enum (`WholeMesh`/`OwnSet`/
`Sets`) + its per-set panel settings row — the row needs i18n keys,
and both `.ftl` catalogs are stencil-locked until the freeze-3
recovery lands, so this slice is double-gated, (c) §5's auto-
mapping. None of these are staleness; all three are named, ordered
follow-ups — the doc's contract with the future is honest as of
this date.
