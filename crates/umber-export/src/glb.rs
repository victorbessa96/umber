//! GLB 2.0 geometry export — RED acceptance scaffold.
//!
//! The public API mirrors the borrowed [`crate::usd::UsdMeshView`] shape so
//! the export crate remains mesh-free. The implementation lands after the
//! failing binary round-trip is captured.

use std::path::{Path, PathBuf};

use crate::driver::ExportError;
use crate::usd::UsdMeshView;

/// Borrowed geometry accepted by the GLB writer.
pub type GlbMeshView<'a> = UsdMeshView<'a>;

/// Upper bound on one exported GLB document.
pub const MAX_GLB_EXPORT_BYTES: u64 = 1 << 28;

/// Byte length of the little-endian geometry payload: 12 bytes per position,
/// 12 per normal, 8 per UV, and 4 per u32 index.
fn bin_len(mesh: &GlbMeshView<'_>) -> u64 {
    (mesh.positions.len() as u64)
        .saturating_mul(12)
        .saturating_add((mesh.normals.len() as u64).saturating_mul(12))
        .saturating_add((mesh.uvs.len() as u64).saturating_mul(8))
        .saturating_add((mesh.indices.len() as u64).saturating_mul(4))
}

/// Conservative container-size upper bound, evaluated before any document or
/// JSON allocation. The JSON envelope is bounded independently of mesh count.
fn doc_size_upper_bound(asset: &str, mesh: &GlbMeshView<'_>) -> u64 {
    let object = crate::obj::obj_name_len(asset) as u64;
    12u64
        .saturating_add(8)
        .saturating_add(4096)
        .saturating_add(4 * object)
        .saturating_add(8)
        .saturating_add(bin_len(mesh))
}

/// Serialize a mesh as deterministic GLB 2.0: JSON chunk + little-endian BIN
/// chunk, with parallel attributes sharing the same accessor index.
pub fn to_glb_document(asset: &str, mesh: &GlbMeshView<'_>) -> Result<Vec<u8>, ExportError> {
    let write = |message: String| ExportError::Write(format!("glb: {message}"));
    if crate::obj::obj_name_len(asset) > crate::obj::MAX_OBJ_NAME_BYTES {
        return Err(write(format!(
            "asset name exceeds {} bytes after sanitization",
            crate::obj::MAX_OBJ_NAME_BYTES
        )));
    }
    let triangles = mesh.triangle_count();
    if triangles == 0 {
        return Err(write("mesh has no triangles".into()));
    }
    if !mesh.indices.len().is_multiple_of(3) {
        return Err(write(format!(
            "mesh has {} indices, not a multiple of 3",
            mesh.indices.len()
        )));
    }
    if mesh.normals.len() != mesh.positions.len() || mesh.uvs.len() != mesh.positions.len() {
        return Err(write(format!(
            "mesh attributes are not parallel ({} positions, {} normals, {} uvs)",
            mesh.positions.len(),
            mesh.normals.len(),
            mesh.uvs.len()
        )));
    }
    for &index in mesh.indices {
        if index as u64 >= mesh.positions.len() as u64 {
            return Err(write(format!(
                "index {index} is out of range for {} positions",
                mesh.positions.len()
            )));
        }
    }
    let bound = doc_size_upper_bound(asset, mesh);
    if bound > MAX_GLB_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: bound,
            cap: MAX_GLB_EXPORT_BYTES,
        });
    }

    let object = crate::obj::obj_name(asset);
    let position_bytes = mesh.positions.len() as u64 * 12;
    let normal_bytes = mesh.normals.len() as u64 * 12;
    let uv_bytes = mesh.uvs.len() as u64 * 8;
    let index_bytes = mesh.indices.len() as u64 * 4;
    let total_bin = bin_len(mesh);
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for position in mesh.positions {
        for axis in 0..3 {
            let value = finite(position[axis]);
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    let document = serde_json::json!({
        "asset": {"version": "2.0", "generator": "Umber"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": object}],
        "meshes": [{
            "name": object,
            "primitives": [{
                "attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
                "indices": 3,
                "mode": 4
            }]
        }],
        "buffers": [{"byteLength": total_bin}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": position_bytes},
            {"buffer": 0, "byteOffset": position_bytes, "byteLength": normal_bytes},
            {"buffer": 0, "byteOffset": position_bytes + normal_bytes, "byteLength": uv_bytes},
            {"buffer": 0, "byteOffset": position_bytes + normal_bytes + uv_bytes, "byteLength": index_bytes}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": mesh.positions.len(), "type": "VEC3", "min": min, "max": max},
            {"bufferView": 1, "componentType": 5126, "count": mesh.positions.len(), "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": mesh.positions.len(), "type": "VEC2"},
            {"bufferView": 3, "componentType": 5125, "count": mesh.indices.len(), "type": "SCALAR"}
        ]
    });
    let json = serde_json::to_vec(&document).map_err(|e| write(format!("json: {e}")))?;
    let json_padded = json.len().div_ceil(4) * 4;
    let total_len = 12usize
        .saturating_add(8)
        .saturating_add(json_padded)
        .saturating_add(8)
        .saturating_add(total_bin as usize);
    if total_len as u64 > MAX_GLB_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: total_len as u64,
            cap: MAX_GLB_EXPORT_BYTES,
        });
    }

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&(json_padded as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out.resize(out.len() + (json_padded - json.len()), b' ');
    out.extend_from_slice(&(total_bin as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    for position in mesh.positions {
        for value in position {
            out.extend_from_slice(&finite(*value).to_le_bytes());
        }
    }
    for normal in mesh.normals {
        for value in normal {
            out.extend_from_slice(&finite(*value).to_le_bytes());
        }
    }
    for uv in mesh.uvs {
        for value in uv {
            out.extend_from_slice(&finite(*value).to_le_bytes());
        }
    }
    for index in mesh.indices {
        out.extend_from_slice(&index.to_le_bytes());
    }
    debug_assert_eq!(out.len(), total_len);
    Ok(out)
}

/// Writes `<asset>.glb` under `out_dir`, returning its path. The document is
/// validated and size-checked before the directory or file is created.
pub fn write_glb(
    out_dir: &Path,
    asset: &str,
    mesh: &GlbMeshView<'_>,
) -> Result<PathBuf, ExportError> {
    let doc = to_glb_document(asset, mesh)?;
    if doc.len() as u64 > MAX_GLB_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: doc.len() as u64,
            cap: MAX_GLB_EXPORT_BYTES,
        });
    }
    std::fs::create_dir_all(out_dir).map_err(|e| ExportError::Write(e.to_string()))?;
    let path = out_dir.join(format!("{}.glb", crate::obj::obj_name(asset)));
    std::fs::write(&path, doc).map_err(|e| ExportError::Write(e.to_string()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    type QuadFixture = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>);

    fn quad() -> QuadFixture {
        (
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![[0.0, 0.0, 1.0]; 4],
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            vec![0, 1, 2, 0, 2, 3],
        )
    }

    fn view<'a>(
        positions: &'a [[f32; 3]],
        normals: &'a [[f32; 3]],
        uvs: &'a [[f32; 2]],
        indices: &'a [u32],
    ) -> GlbMeshView<'a> {
        GlbMeshView {
            positions,
            normals,
            uvs,
            indices,
        }
    }

    #[test]
    fn deterministic_binary_layout_and_real_roundtrip() {
        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let doc = to_glb_document("Sword", &mesh).expect("GLB document");
        assert_eq!(doc, to_glb_document("Sword", &mesh).unwrap());
        assert_eq!(&doc[..4], b"glTF");
        assert_eq!(u32::from_le_bytes(doc[4..8].try_into().unwrap()), 2);
        let total = u32::from_le_bytes(doc[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, doc.len());
        let json_len = u32::from_le_bytes(doc[12..16].try_into().unwrap()) as usize;
        assert_eq!(json_len % 4, 0);
        assert_eq!(&doc[16..20], b"JSON");
        let bin_header = 20 + json_len;
        let bin_len =
            u32::from_le_bytes(doc[bin_header..bin_header + 4].try_into().unwrap()) as usize;
        assert_eq!(&doc[bin_header + 4..bin_header + 8], b"BIN\0");
        assert_eq!(bin_len, 4 * 12 + 4 * 12 + 4 * 8 + 6 * 4);
        let json: serde_json::Value = serde_json::from_slice(&doc[20..20 + json_len]).unwrap();
        assert_eq!(
            json["meshes"][0]["primitives"][0]["attributes"]["POSITION"],
            0
        );
        assert_eq!(
            json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"],
            1
        );
        assert_eq!(
            json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"],
            2
        );
        assert_eq!(json["meshes"][0]["primitives"][0]["indices"], 3);
        assert_eq!(json["accessors"][3]["componentType"], 5125);
        assert_eq!(json["accessors"][0]["componentType"], 5126);
        assert_eq!(json["accessors"][1]["componentType"], 5126);
        assert_eq!(json["accessors"][2]["componentType"], 5126);
        assert_eq!(
            json["accessors"][0]["min"],
            serde_json::json!([0.0, 0.0, 0.0])
        );
        assert_eq!(
            json["accessors"][0]["max"],
            serde_json::json!([1.0, 1.0, 0.0])
        );
        for (index, offset, length) in [(0, 0, 48), (1, 48, 48), (2, 96, 32), (3, 128, 24)] {
            assert_eq!(json["bufferViews"][index]["byteOffset"], offset);
            assert_eq!(json["bufferViews"][index]["byteLength"], length);
        }
        assert_eq!(json["accessors"][0]["count"], 4);
        assert_eq!(json["accessors"][1]["count"], 4);
        assert_eq!(json["accessors"][2]["count"], 4);
        assert_eq!(json["accessors"][3]["count"], 6);

        let dir = std::env::temp_dir().join(format!("umber-glb-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_glb(&dir, "Sword", &mesh).expect("GLB write");
        let bytes_a = std::fs::read(&path).unwrap();
        let path_b = write_glb(&dir, "Sword", &mesh).expect("second GLB write");
        assert_eq!(bytes_a, std::fs::read(path_b).unwrap());
        for load in [umber_mesh::load(&path), umber_mesh::load_gltf(&path)] {
            let back = load.expect("GLB re-import");
            assert_eq!(back.triangle_count(), 2);
            assert!(back.positions.iter().flatten().all(|v| v.is_finite()));
            assert!(back.normals.iter().flatten().all(|v| v.is_finite()));
            assert!(back.uvs.iter().flatten().all(|v| v.is_finite()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ragged_and_over_cap_are_typed_failures() {
        let (positions, normals, uvs, _) = quad();
        assert!(matches!(
            to_glb_document("S", &view(&positions, &normals, &uvs, &[])),
            Err(ExportError::Write(_))
        ));
        assert!(matches!(
            to_glb_document("S", &view(&positions, &normals, &uvs, &[0, 1])),
            Err(ExportError::Write(_))
        ));
        // Nonzero triangle count with a non-multiple index count reaches the
        // divisibility branch, unlike `[0, 1]`, which exits as empty.
        assert!(matches!(
            to_glb_document("S", &view(&positions, &normals, &uvs, &[0, 1, 2, 0])),
            Err(ExportError::Write(_))
        ));
        assert!(matches!(
            to_glb_document("S", &view(&positions, &normals, &uvs, &[0, 1, 99])),
            Err(ExportError::Write(_))
        ));
        let short_normals = vec![[0.0f32, 0.0, 1.0]; 2];
        assert!(matches!(
            to_glb_document("S", &view(&positions, &short_normals, &uvs, &[0, 1, 2])),
            Err(ExportError::Write(_))
        ));
        let short_uvs = vec![[0.0f32, 0.0]; 2];
        assert!(matches!(
            to_glb_document("S", &view(&positions, &normals, &short_uvs, &[0, 1, 2])),
            Err(ExportError::Write(_))
        ));

        // Writer-level failures also leave no partial file when the output
        // directory already exists: empty, non-multiple, out-of-range, and
        // ragged attribute cases all stop before any file is created.
        for (label, indices, normals, uvs) in [
            ("empty", Vec::new(), normals.clone(), uvs.clone()),
            ("nonmultiple", vec![0, 1], normals.clone(), uvs.clone()),
            ("out_of_range", vec![0, 1, 99], normals.clone(), uvs.clone()),
            (
                "short_normals",
                vec![0, 1, 2],
                short_normals.clone(),
                uvs.clone(),
            ),
            (
                "short_uvs",
                vec![0, 1, 2],
                normals.clone(),
                short_uvs.clone(),
            ),
        ] {
            let dir = std::env::temp_dir()
                .join(format!("umber-glb-invalid-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let mesh = view(&positions, &normals, &uvs, &indices);
            assert!(
                matches!(write_glb(&dir, label, &mesh), Err(ExportError::Write(_))),
                "{label}"
            );
            assert!(
                !dir.join(format!("{label}.glb")).exists(),
                "{label} left a partial file"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        // Fresh nonexistent directories pin writer-level no-output for every
        // geometry-validation failure, including the true divisibility case.
        for (label, indices, normals, uvs) in [
            ("empty", Vec::new(), normals.clone(), uvs.clone()),
            (
                "nonmultiple",
                vec![0, 1, 2, 0],
                normals.clone(),
                uvs.clone(),
            ),
            ("out_of_range", vec![0, 1, 99], normals.clone(), uvs.clone()),
            (
                "short_normals",
                vec![0, 1, 2],
                short_normals.clone(),
                uvs.clone(),
            ),
            (
                "short_uvs",
                vec![0, 1, 2],
                normals.clone(),
                short_uvs.clone(),
            ),
        ] {
            let dir = std::env::temp_dir().join(format!(
                "umber-glb-writer-nonexistent-{label}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            let mesh = view(&positions, &normals, &uvs, &indices);
            assert!(
                matches!(write_glb(&dir, label, &mesh), Err(ExportError::Write(_))),
                "{label}"
            );
            assert!(!dir.exists(), "{label} created a partial output dir");
            assert!(
                !dir.join(format!("{label}.glb")).exists(),
                "{label} left a partial file"
            );
        }

        let n = 8_000_001;
        let positions = vec![[0.0f32; 3]; n];
        let normals = vec![[0.0f32, 0.0, 1.0]; n];
        let uvs = vec![[0.0f32, 0.0]; n];
        let indices: Vec<u32> = (0..n as u32).collect();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let actual = to_glb_document("bomb", &mesh);
        assert!(
            matches!(actual, Err(ExportError::TooLarge { cap, .. }) if cap == MAX_GLB_EXPORT_BYTES),
            "actual: {:?}",
            actual
                .as_ref()
                .map(|doc| doc.len())
                .map_err(|err| err.to_string())
        );
        let dir = std::env::temp_dir().join(format!("umber-glb-overcap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(
            write_glb(&dir, "bomb", &mesh),
            Err(ExportError::TooLarge { .. })
        ));
        assert!(
            !dir.join("bomb.glb").exists(),
            "over-cap writer left a partial file"
        );
    }

    #[test]
    fn huge_names_and_write_failures_are_typed_without_partial_files() {
        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let huge = "A".repeat(40_000_000);
        assert!(matches!(
            to_glb_document(&huge, &mesh),
            Err(ExportError::Write(_))
        ));
        let dir = std::env::temp_dir().join(format!("umber-glb-huge-name-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(
            write_glb(&dir, &huge, &mesh),
            Err(ExportError::Write(_))
        ));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "huge-name writer left a partial file"
        );

        let dir =
            std::env::temp_dir().join(format!("umber-glb-write-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // `create_dir_all(out_dir)` succeeds, but the actual output path is
        // already a directory, so `std::fs::write` itself must fail.
        std::fs::create_dir_all(dir.join("Sword.glb")).unwrap();
        assert!(matches!(
            write_glb(&dir, "Sword", &mesh),
            Err(ExportError::Write(_))
        ));
        assert!(dir.join("Sword.glb").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_finite_metadata_sanitizes_without_non_finite_tokens() {
        let (mut positions, normals, mut uvs, indices) = quad();
        positions[0] = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        uvs[0] = [f32::NAN, f32::INFINITY];
        let doc = to_glb_document("Sword", &view(&positions, &normals, &uvs, &indices)).unwrap();
        for token in ["NaN", "nan", "inf", "-inf", "infinity", "-infinity"] {
            assert!(
                !doc.windows(token.len()).any(|w| w == token.as_bytes()),
                "{token} leaked into GLB"
            );
        }
        let dir = std::env::temp_dir().join(format!("umber-glb-nonfinite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_glb(&dir, "Sword", &view(&positions, &normals, &uvs, &indices)).unwrap();
        let back = umber_mesh::load_gltf(&path).expect("sanitized GLB re-import");
        assert_eq!(back.triangle_count(), 2);
        assert!(back.positions.iter().flatten().all(|v| v.is_finite()));
        assert!(back.uvs.iter().flatten().all(|v| v.is_finite()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
