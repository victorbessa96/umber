//! Wavefront OBJ geometry export — deterministic triangle meshes.
//!
//! The public API mirrors the borrowed [`crate::usd::UsdMeshView`] shape so
//! the export crate remains mesh-free. Parallel attributes use the same
//! one-based index at each corner, matching `tobj`'s `single_index=true`
//! import contract. GLB, material/MTL, and texture references are out of scope.

use std::path::{Path, PathBuf};

use crate::driver::ExportError;
use crate::usd::UsdMeshView;

/// Borrowed geometry accepted by the OBJ writer.
pub type ObjMeshView<'a> = UsdMeshView<'a>;

/// Upper bound on one exported OBJ document.
pub const MAX_OBJ_EXPORT_BYTES: u64 = 1 << 28;

/// Upper bound on one sanitized object/file identifier. Filesystem components
/// and the OBJ object declaration stay bounded before any allocation happens.
const MAX_OBJ_NAME_BYTES: usize = 240;

/// Byte length of [`obj_name`]'s sanitized output, computed without allocating
/// the name. Every input character maps to at most one ASCII byte.
fn obj_name_len(asset: &str) -> usize {
    let mut len = asset.chars().count();
    if len == 0 || asset.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        len += 2;
    }
    len
}

/// One bounded ASCII object/file identifier: no path separators or whitespace
/// can escape the output directory or split the OBJ object declaration.
fn obj_name(asset: &str) -> String {
    let mut name: String = asset
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert_str(0, "M_");
    }
    name
}

/// One lexable finite number. Non-finite metadata becomes `0.0` rather than
/// leaking NaN/Inf tokens into a document another parser must read.
fn fmt_num(value: f32) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        "0".to_owned()
    }
}

/// Conservative document-size upper bound, evaluated before any document
/// allocation. Eight f32 slots per vertex and 36 bytes per index cover the
/// longest finite `Display` forms and the `f v/vt/vn` corner syntax.
fn doc_size_upper_bound(asset: &str, mesh: &ObjMeshView<'_>) -> u64 {
    let object = obj_name_len(asset) as u64;
    let vertices = mesh.positions.len() as u64;
    let indices = mesh.indices.len() as u64;
    1024u64
        .saturating_add(4 * object)
        .saturating_add(vertices.saturating_mul(8 * 64 + 16))
        .saturating_add(indices.saturating_mul(36))
}

/// Serialize a mesh as deterministic Wavefront OBJ: parallel attributes use
/// the same one-based index at every corner, matching `tobj`'s
/// `single_index=true` import contract.
pub fn to_obj_document(asset: &str, mesh: &ObjMeshView<'_>) -> Result<String, ExportError> {
    let write = |message: String| ExportError::Write(format!("obj: {message}"));
    if obj_name_len(asset) > MAX_OBJ_NAME_BYTES {
        return Err(write(format!(
            "asset name exceeds {MAX_OBJ_NAME_BYTES} bytes after sanitization"
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
    if bound > MAX_OBJ_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: bound,
            cap: MAX_OBJ_EXPORT_BYTES,
        });
    }

    let object = obj_name(asset);
    let mut out = String::new();
    out.push_str("# Umber OBJ export\n");
    out.push_str(&format!("# asset {object}\no {object}\n"));
    for position in mesh.positions {
        out.push_str(&format!(
            "v {} {} {}\n",
            fmt_num(position[0]),
            fmt_num(position[1]),
            fmt_num(position[2])
        ));
    }
    for uv in mesh.uvs {
        out.push_str(&format!("vt {} {}\n", fmt_num(uv[0]), fmt_num(uv[1])));
    }
    for normal in mesh.normals {
        out.push_str(&format!(
            "vn {} {} {}\n",
            fmt_num(normal[0]),
            fmt_num(normal[1]),
            fmt_num(normal[2])
        ));
    }
    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0] + 1, triangle[1] + 1, triangle[2] + 1];
        out.push_str(&format!("f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}\n"));
    }
    Ok(out)
}

/// Writes `<asset>.obj` under `out_dir`, returning its path. The document is
/// validated and size-checked before the directory or file is created.
pub fn write_obj(
    out_dir: &Path,
    asset: &str,
    mesh: &ObjMeshView<'_>,
) -> Result<PathBuf, ExportError> {
    let doc = to_obj_document(asset, mesh)?;
    if doc.len() as u64 > MAX_OBJ_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: doc.len() as u64,
            cap: MAX_OBJ_EXPORT_BYTES,
        });
    }
    std::fs::create_dir_all(out_dir).map_err(|e| ExportError::Write(e.to_string()))?;
    let path = out_dir.join(format!("{}.obj", obj_name(asset)));
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
    ) -> ObjMeshView<'a> {
        ObjMeshView {
            positions,
            normals,
            uvs,
            indices,
        }
    }

    #[test]
    fn deterministic_document_and_real_roundtrip() {
        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let doc = to_obj_document("Sword", &mesh).expect("OBJ document");
        assert_eq!(doc, to_obj_document("Sword", &mesh).unwrap());
        assert!(!doc.contains('\r'));
        assert!(doc.contains("v 0 0 0"));
        assert!(doc.contains("vt 0 0"));
        assert!(doc.contains("vn 0 0 1"));
        assert!(doc.contains("f 1/1/1 2/2/2 3/3/3"));
        assert!(doc.contains("f 1/1/1 3/3/3 4/4/4"));

        let dir = std::env::temp_dir().join(format!("umber-obj-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_obj(&dir, "Sword", &mesh).expect("OBJ write");
        let bytes_a = std::fs::read(&path).unwrap();
        let path_b = write_obj(&dir, "Sword", &mesh).expect("second OBJ write");
        assert_eq!(bytes_a, std::fs::read(path_b).unwrap());
        for load in [umber_mesh::load(&path), umber_mesh::load_obj(&path)] {
            let back = load.expect("OBJ re-import");
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
            to_obj_document("S", &view(&positions, &normals, &uvs, &[])),
            Err(ExportError::Write(_))
        ));
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &uvs, &[0, 1])),
            Err(ExportError::Write(_))
        ));
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &uvs, &[0, 1, 99])),
            Err(ExportError::Write(_))
        ));
        let short_normals = vec![[0.0f32, 0.0, 1.0]; 2];
        assert!(matches!(
            to_obj_document("S", &view(&positions, &short_normals, &uvs, &[0, 1, 2])),
            Err(ExportError::Write(_))
        ));
        let short_uvs = vec![[0.0f32, 0.0]; 2];
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &short_uvs, &[0, 1, 2])),
            Err(ExportError::Write(_))
        ));

        let n = 600_000;
        let positions = vec![[0.0f32; 3]; n];
        let normals = vec![[0.0f32, 0.0, 1.0]; n];
        let uvs = vec![[0.0f32, 0.0]; n];
        let indices: Vec<u32> = (0..n as u32).collect();
        let mesh = view(&positions, &normals, &uvs, &indices);
        assert!(matches!(
            to_obj_document("bomb", &mesh),
            Err(ExportError::TooLarge { cap, .. }) if cap == MAX_OBJ_EXPORT_BYTES
        ));
        let dir = std::env::temp_dir().join(format!("umber-obj-overcap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(
            write_obj(&dir, "bomb", &mesh),
            Err(ExportError::TooLarge { .. })
        ));
        assert!(
            !dir.exists(),
            "over-cap writer created a partial output dir"
        );
    }

    #[test]
    fn huge_names_and_write_failures_are_typed_without_partial_files() {
        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let huge = "A".repeat(40_000_000);
        assert!(matches!(
            to_obj_document(&huge, &mesh),
            Err(ExportError::Write(_))
        ));
        let dir = std::env::temp_dir().join(format!("umber-obj-huge-name-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(
            write_obj(&dir, &huge, &mesh),
            Err(ExportError::Write(_))
        ));
        assert!(
            !dir.exists(),
            "huge-name writer created a partial output dir"
        );

        let dir =
            std::env::temp_dir().join(format!("umber-obj-write-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // `create_dir_all(out_dir)` succeeds, but the actual output path is
        // already a directory, so `std::fs::write` itself must fail.
        std::fs::create_dir_all(dir.join("Sword.obj")).unwrap();
        assert!(matches!(
            write_obj(&dir, "Sword", &mesh),
            Err(ExportError::Write(_))
        ));
        assert!(dir.join("Sword.obj").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_finite_metadata_sanitizes_without_non_finite_tokens() {
        let (mut positions, normals, mut uvs, indices) = quad();
        positions[0] = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        uvs[0] = [f32::NAN, f32::INFINITY];
        let doc = to_obj_document("Sword", &view(&positions, &normals, &uvs, &indices)).unwrap();
        for token in ["NaN", "nan", "inf", "-inf", "infinity", "-infinity"] {
            assert!(!doc.contains(token), "{token} leaked into OBJ");
        }
        let dir = std::env::temp_dir().join(format!("umber-obj-nonfinite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_obj(&dir, "Sword", &view(&positions, &normals, &uvs, &indices)).unwrap();
        let back = umber_mesh::load_obj(&path).expect("sanitized OBJ re-import");
        assert_eq!(back.triangle_count(), 2);
        assert!(back.positions.iter().flatten().all(|v| v.is_finite()));
        assert!(back.uvs.iter().flatten().all(|v| v.is_finite()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
