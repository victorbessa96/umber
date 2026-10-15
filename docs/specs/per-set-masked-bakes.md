# Per-set masked bakes (design, C2-9 follow-up)

Status: design, 2026-10-15. Written while the build host was down; no code.

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
