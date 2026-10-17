//! FBX mesh import via the [`ufbx`] crate (official Rust bindings).
//!
//! Every node carrying a mesh contributes its triangulated faces. Vertices
//! are expanded per corner (no index de-duplication): this keeps UV seams
//! intact at the cost of duplicated positions, and is the documented
//! simplification to revisit once a weld pass exists. Missing normals or UVs
//! are filled with zeros so the attribute vectors always stay parallel to
//! `positions`; positions always come from the real vertex data. Node
//! transforms and skinning are ignored (bind pose).

use std::path::Path;

use super::{ImportError, MeshData};

/// Load an `.fbx` file into a single concatenated [`MeshData`].
///
/// Walks every scene node, triangulates each attached mesh, and appends the
/// result with per-primitive index offsets. Returns [`ImportError::Fbx`] when
/// the file cannot be parsed, when index data is internally inconsistent, or
/// when the scene contains no mesh geometry at all.
pub fn load_fbx(path: &Path) -> Result<MeshData, ImportError> {
    let path_str = path.to_string_lossy();
    let scene = ufbx::load_file(&path_str, ufbx::LoadOpts::default())
        .map_err(|e| ImportError::Fbx(format!("{e:?}")))?;
    let mut data = MeshData::default();
    for node in &scene.nodes {
        let Some(mesh) = node.mesh.as_ref() else {
            continue;
        };
        let mesh: &ufbx::Mesh = mesh;
        append_mesh(&mut data, mesh)?;
    }
    if data.positions.is_empty() {
        return Err(ImportError::Fbx("no mesh geometry found".into()));
    }
    Ok(data)
}

/// Append one triangulated [`ufbx::Mesh`] to the accumulated [`MeshData`].
///
/// `corner` values produced by [`ufbx::triangulate_face_vec`] are absolute
/// mesh corner indices, so they index `vertex_indices` and the attribute
/// index buffers directly.
fn append_mesh(data: &mut MeshData, mesh: &ufbx::Mesh) -> Result<(), ImportError> {
    for material in &mesh.materials {
        let name = material.element.name.to_string();
        if !data.material_names.iter().any(|n| n == &name) {
            data.material_names.push(name);
        }
    }

    // Face -> index into `data.material_names` (per-set masked bakes
    // §1): ufbx exposes per-material parts, each carrying the face
    // ids it covers. Faces no part claims get u16::MAX — the caller's
    // whole-mesh fallback, never a wrong-set attribution.
    let mut face_material = vec![u16::MAX; mesh.faces.len()];
    for part in &mesh.material_parts {
        let part_index = mesh
            .materials
            .get(part.index as usize)
            .map(|material| material.element.name.to_string());
        let Some(name) = part_index else { continue };
        let Some(slot) = data
            .material_names
            .iter()
            .position(|n| n == &name)
            .and_then(|i| u16::try_from(i).ok())
        else {
            continue;
        };
        for face in &part.face_indices {
            if let Some(slot_ref) = face_material.get_mut(*face as usize) {
                *slot_ref = slot;
            }
        }
    }

    let positions: &[ufbx::Vec3] = &mesh.vertices;
    let position_indices: &[u32] = &mesh.vertex_indices;
    let uv = &mesh.vertex_uv;
    let normal = &mesh.vertex_normal;
    let has_uv = uv.exists && !uv.values.is_empty() && !uv.indices.is_empty();
    let has_normal = normal.exists && !normal.values.is_empty() && !normal.indices.is_empty();

    // All-or-nothing (§1's contract): the map is either empty or exactly
    // `triangle_count()` long. A mesh with any unmapped face keeps the
    // whole-mesh fallback rather than a ragged half-map.
    let all_faces_mapped =
        !face_material.is_empty() && face_material.iter().all(|&slot| slot != u16::MAX);
    let mut mesh_triangle_materials: Vec<u16> = Vec::new();

    let mut corners = Vec::new();
    for (face_id, face) in mesh.faces.iter().enumerate() {
        corners.clear();
        ufbx::triangulate_face_vec(&mut corners, mesh, *face);
        let face_material_slot = face_material[face_id];
        for corner in &corners {
            let corner = *corner as usize;
            let position_index =
                position_indices.get(corner).copied().ok_or_else(|| {
                    ImportError::Fbx(format!("corner {corner} has no position index"))
                })? as usize;
            let position = positions.get(position_index).ok_or_else(|| {
                ImportError::Fbx(format!("position index {position_index} out of range"))
            })?;
            data.positions
                .push([position.x as f32, position.y as f32, position.z as f32]);

            if has_uv {
                let uv_index =
                    uv.indices.get(corner).copied().ok_or_else(|| {
                        ImportError::Fbx(format!("corner {corner} has no UV index"))
                    })? as usize;
                let tex_coord = uv
                    .values
                    .get(uv_index)
                    .ok_or_else(|| ImportError::Fbx(format!("UV index {uv_index} out of range")))?;
                data.uvs.push([tex_coord.x as f32, tex_coord.y as f32]);
            } else {
                data.uvs.push([0.0; 2]);
            }

            if has_normal {
                let normal_index = normal.indices.get(corner).copied().ok_or_else(|| {
                    ImportError::Fbx(format!("corner {corner} has no normal index"))
                })? as usize;
                let axis = normal.values.get(normal_index).ok_or_else(|| {
                    ImportError::Fbx(format!("normal index {normal_index} out of range"))
                })?;
                data.normals
                    .push([axis.x as f32, axis.y as f32, axis.z as f32]);
            } else {
                data.normals.push([0.0; 3]);
            }

            data.indices.push(data.positions.len() as u32 - 1);
        }
        // One material per emitted triangle: `corners.len() / 3`
        // triangles came out of this face (a quad yields two).
        if all_faces_mapped {
            let triangles = corners.len() / 3;
            if triangles > 0 {
                mesh_triangle_materials.extend(std::iter::repeat_n(face_material_slot, triangles));
            }
        }
    }
    if all_faces_mapped {
        data.triangle_materials.extend(mesh_triangle_materials);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real binary FBX fixture (Kaydara FBX 7.4 binary, a Blender-exported
    // cube) from ufbx's own upstream test corpus
    // (data/blender_272_cube_7400_binary.fbx). Binary FBX cannot be built by
    // hand in-test; this is the honest fixture.
    #[test]
    fn loads_fbx_cube_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("assets")
            .join("cube_b7400.fbx");
        let data = crate::load(&path).expect("cube fbx fixture must load");
        assert!(data.vertex_count() >= 8, "cube needs >= 8 verts");
        assert!(data.triangle_count() >= 12, "cube needs >= 12 tris");
        assert_eq!(data.positions.len(), data.normals.len());
        assert_eq!(data.positions.len(), data.uvs.len());
        // Blender 2.72 cube exports carry no material; material presence is
        // not asserted for this fixture. A material-carrying fixture lands
        // with the Wave-2 texture-set work.
        // This fixture carries NO UV layer (no `LayerElementUV` in the
        // file), so every UV loads as the zero-fill [0, 0] — pinned, so a
        // test that needs UV coverage cannot silently pick this cube
        // (2026-10-15: a bake test on it baked all-empty maps). A UV'd FBX
        // fixture is the follow-up.
        assert!(data.uvs.iter().all(|uv| *uv == [0.0, 0.0]));
    }

    // The UV'd fixture (C2-17): a cube with a real 3x2-atlas unwrap,
    // generated by `scripts/gen-fbx-uv-cube.py` (ASCII FBX 7.4; no FBX
    // authoring tool runs on the build hosts, so the script is the
    // fixture's source and reproduces it byte for byte).
    #[test]
    fn loads_the_uv_unwrapped_fbx_cube() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("assets")
            .join("cube_uv_ascii7400.fbx");
        let data = crate::load(&path).expect("the UV'd cube loads");
        // 6 quads -> 12 triangles -> 36 corners (the loader de-indexes).
        assert_eq!(data.triangle_count(), 12);
        assert_eq!(data.uvs.len(), data.positions.len());
        // Every corner's UV sits strictly inside one of the six atlas
        // cells (the 1/64 inset), and every cell is used: a real unwrap,
        // not the zero-fill the binary cube produces.
        let margin = 1.0 / 64.0 - 1e-4;
        let mut cells = std::collections::BTreeSet::new();
        for uv in &data.uvs {
            assert!(
                (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]),
                "{uv:?}"
            );
            let col = (uv[0] * 3.0).floor().min(2.0);
            let row = (uv[1] * 2.0).floor().min(1.0);
            let (du, dv) = (uv[0] - col / 3.0, uv[1] - row / 2.0);
            assert!(
                du >= margin && dv >= margin,
                "inset from the cell edge: {uv:?}"
            );
            assert!(1.0 / 3.0 - du >= margin && 0.5 - dv >= margin, "{uv:?}");
            cells.insert((col as u8, row as u8));
        }
        assert_eq!(cells.len(), 6, "one atlas cell per face: {cells:?}");
        // Normals are the six axis directions, unit length.
        for n in &data.normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "{n:?}");
        }
        // Positions are the ±1 cube.
        assert!(data.positions.iter().flatten().all(|c| c.abs() == 1.0));
    }
}
