//! Binary crate (`.usdc`) and package (`.usdz`) mesh import via
//! `openusd` 0.7.0 — the pure-Rust USD implementation.
//!
//! The 2026-10-09 audit's deferral ("usdc when a pure-Rust reader
//! stabilizes") is void. This arm MIRRORS the `.usda` hand parser's
//! geometry rules (`usd.rs`'s doc block is the contract): per-corner
//! vertex expansion, fan triangulation of n-gons, flat Newell normals
//! when none authored, `leftHanded` winding flip, zero-filled missing
//! UVs, holes rejected. The two parsers stay structurally separate
//! (the text parser is token-stream shaped; the crate walk is
//! path-keyed) — the RULES are mirrored, not the code shared; a
//! shared attribute layer is the named follow-up when a third USD
//! format forces it.
//!
//! Error contract: `safe` mode (structural validation before decode)
//! and every failure maps to `ImportError::Usd`/`UsdBinary`; a corrupt
//! file never panics.

use std::path::Path;

use openusd::sdf::{AbstractData, Path as SdfPath, SpecType, Value};
use openusd::usdc;

use super::{ImportError, MeshData};
/// Loads a `.usdc` (binary crate) file: the first Mesh prim becomes a
/// fan-triangulated [`MeshData`], mirroring [`super::usd::load_usda`].
pub fn load_usdc(path: &Path) -> Result<MeshData, ImportError> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"PXR-USDC") {
        return Err(ImportError::UsdBinary);
    }
    let reader = std::io::Cursor::new(bytes);
    let data = usdc::CrateData::open(reader, true)
        .map_err(|e| ImportError::Usd(format!("usdc: open failed: {e}")))?;
    extract_first_mesh(&data)
}

/// Loads a `.usdz` (package) file: its first layer's first Mesh prim.
pub fn load_usdz(path: &Path) -> Result<MeshData, ImportError> {
    let layer =
        usdc::read_file(path).map_err(|e| ImportError::Usd(format!("usdz: open failed: {e}")))?;
    extract_first_mesh(layer.as_ref())
}

/// Depth-first, authoring-order walk: the first prim whose `typeName`
/// is `Mesh` — the same first-Mesh rule as the .usda parser.
fn extract_first_mesh(data: &dyn AbstractData) -> Result<MeshData, ImportError> {
    let root = SdfPath::abs_root();
    let mesh_path = find_first_mesh(data, &root)
        .ok_or_else(|| ImportError::Usd("usd: no Mesh prim in document".to_string()))?;
    build_mesh_from_spec(data, &mesh_path)
}

fn find_first_mesh(data: &dyn AbstractData, path: &SdfPath) -> Option<SdfPath> {
    if data.spec_type(path) == Some(SpecType::Prim) {
        if let Ok(Some(v)) = data.try_field(path, "typeName") {
            if let Value::Token(ty) = v.as_ref() {
                if ty.as_str() == "Mesh" {
                    return Some(path.clone());
                }
            }
        }
    }
    for child in child_prim_paths(data, path) {
        if let Some(found) = find_first_mesh(data, &child) {
            return Some(found);
        }
    }
    None
}

/// A prim's children, in `primChildren` order (the authoring order the
/// crate index preserves). Child paths build by parsing — the Path
/// API has no `append_child`, and `format!("{parent}/{name}")` is the
/// canonical composition the parser itself uses.
fn child_prim_paths(data: &dyn AbstractData, path: &SdfPath) -> Vec<SdfPath> {
    let mut out = Vec::new();
    if let Ok(Some(v)) = data.try_field(path, "primChildren") {
        if let Value::TokenVec(names) = v.as_ref() {
            for name in names {
                let composed = format!("{}/{}", path.as_str().trim_end_matches('/'), name.as_str());
                if let Ok(child) = SdfPath::new(&composed) {
                    out.push(child);
                }
            }
        }
    }
    out
}

/// One field lookup: decode failures become `ImportError::Usd` with the
/// spec path (the crate's error context — the .usda parser's `line N:`
/// equivalent). Returns the CLONED value (try_field hands back a borrow;
/// cloning here keeps every caller's match simple).
fn field(
    data: &dyn AbstractData,
    path: &SdfPath,
    name: &str,
) -> Result<Option<Value>, ImportError> {
    match data.try_field(path, name) {
        Ok(opt) => Ok(opt.map(|cow| cow.into_owned())),
        Err(e) => Err(ImportError::Usd(format!(
            "usd: {}.{} decode failed: {e}",
            path.as_str(),
            name
        ))),
    }
}

/// Extracts the Mesh's geometry, mirroring the .usda parser's rules.
fn build_mesh_from_spec(data: &dyn AbstractData, path: &SdfPath) -> Result<MeshData, ImportError> {
    // --- topology ---
    let points: Vec<f32> = match field(data, path, "points")? {
        Some(Value::FloatVec(v)) if !v.is_empty() => v,
        Some(Value::Vec3fVec(v)) if !v.is_empty() => {
            v.into_iter().flat_map(<[f32; 3]>::from).collect()
        }
        Some(_) => {
            return Err(ImportError::Usd(format!(
                "usd: {path}: points has wrong type"
            )))
        }
        None => return Err(ImportError::Usd(format!("usd: {path}: missing points"))),
    };
    let counts = match field(data, path, "faceVertexCounts")? {
        Some(Value::IntVec(v)) => v,
        _ => {
            return Err(ImportError::Usd(format!(
                "usd: {path}: missing faceVertexCounts"
            )))
        }
    };
    let indices = match field(data, path, "faceVertexIndices")? {
        Some(Value::IntVec(v)) => v,
        _ => {
            return Err(ImportError::Usd(format!(
                "usd: {path}: missing faceVertexIndices"
            )))
        }
    };
    let total: i64 = counts.iter().map(|&c| c as i64).sum();
    if indices.len() as i64 != total {
        return Err(ImportError::Usd(format!(
            "usd: {path}: faceVertexIndices ({}) does not match faceVertexCounts total ({total})",
            indices.len()
        )));
    }
    // holeIndices: rejected, like the .usda parser.
    if let Some(Value::IntVec(holes)) = field(data, path, "holeIndices")? {
        if !holes.is_empty() {
            return Err(ImportError::Usd(format!(
                "usd: {path}: holeIndices not supported in v1 (rejected, not ignored)"
            )));
        }
    }

    // --- orientation ---
    let left_handed = matches!(field(data, path, "orientation")?,
        Some(Value::Token(t)) if t.as_str() == "leftHanded");

    // --- normals: primvars:normals else normals; flat Newell when absent ---
    let mut normals: Option<Vec<f32>> = None;
    for name in ["primvars:normals", "normals"] {
        if let Some(Value::FloatVec(v)) = field(data, path, name)? {
            normals = Some(v);
            break;
        }
    }

    // --- UVs: primvars:st; zero-filled when absent (the .usda rule) ---
    let uvs: Vec<f32> = match field(data, path, "primvars:st")? {
        Some(Value::FloatVec(v)) => v,
        _ => Vec::new(),
    };

    // --- the fan triangulation + per-corner expansion (the .usda rules).
    // MeshData's attribute vecs are per-vertex ([f32;3] / [f32;2] entries),
    // so the builder accumulates triples, not flat floats. ---
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut out_indices: Vec<u32> = Vec::new();
    let mut out_normals: Vec<[f32; 3]> = Vec::new();
    let mut out_uvs: Vec<[f32; 2]> = Vec::new();

    let newell = |face: &[i32]| -> [f32; 3] {
        let mut n = [0f32; 3];
        for i in 0..face.len() {
            let a = face[i] as usize * 3;
            let b = face[(i + 1) % face.len()] as usize * 3;
            n[0] += (points[a + 1] - points[b + 1]) * (points[a + 2] + points[b + 2]);
            n[1] += (points[a + 2] - points[b + 2]) * (points[a] + points[b]);
            n[2] += (points[a] - points[b]) * (points[a + 1] + points[b + 1]);
        }
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 0.0 {
            [n[0] / len, n[1] / len, n[2] / len]
        } else {
            [0.0, 0.0, 1.0]
        }
    };

    let mut corner: usize = 0;
    for (fi, &count) in counts.iter().enumerate() {
        if count < 3 {
            return Err(ImportError::Usd(format!(
                "usd: {path}: face {fi} has {count} corners (< 3)"
            )));
        }
        let face: Vec<i32> = indices[corner..corner + count as usize].to_vec();
        corner += count as usize;
        let flat = newell(&face);

        for t in 0..(count - 2) {
            let t = t as usize;
            let tri = if left_handed {
                [face[0], face[t + 2], face[t + 1]]
            } else {
                [face[0], face[t + 1], face[t + 2]]
            };
            for &vi in &tri {
                let vi = usize::try_from(vi)
                    .map_err(|_| ImportError::Usd(format!("usd: {path}: negative vertex index")))?;
                if vi * 3 + 3 > points.len() {
                    return Err(ImportError::Usd(format!(
                        "usd: {path}: vertex index {vi} out of range ({} points)",
                        points.len() / 3
                    )));
                }
                let p: [f32; 3] = [points[vi * 3], points[vi * 3 + 1], points[vi * 3 + 2]];
                positions.push(p);
                match &normals {
                    Some(n) if vi * 3 + 3 <= n.len() => {
                        out_normals.push([n[vi * 3], n[vi * 3 + 1], n[vi * 3 + 2]])
                    }
                    _ => out_normals.push(flat),
                }
                if !uvs.is_empty() && vi * 2 + 2 <= uvs.len() {
                    out_uvs.push([uvs[vi * 2], uvs[vi * 2 + 1]]);
                } else {
                    out_uvs.push([0.0, 0.0]);
                }
                out_indices.push(out_indices.len() as u32);
            }
        }
    }

    Ok(MeshData {
        positions,
        normals: out_normals,
        uvs: out_uvs,
        indices: out_indices,
        material_names: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round-trip fixture built programmatically: a quad authored as
    /// .usda text, parsed by openusd's own parser, serialized to .usdc
    /// bytes with `CrateWriter::write` — never shipped as a binary.
    fn quad_usdc_bytes() -> Result<Vec<u8>, String> {
        let usda_text = r#"
#usda 1.0
(
    defaultPrim = "Mesh"
)

def Mesh "Mesh"
{
    float3[] points = [(-1, -1, 0), (1, -1, 0), (1, 1, 0), (-1, 1, 0)]
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
    texCoord2f[] primvars:st = [(0, 0), (1, 0), (1, 1), (0, 1)] (
        interpolation = "vertex"
    )
}
"#;
        let data = openusd::usda::parse(usda_text).map_err(|e| format!("fixture parse: {e}"))?;
        let mut out = std::io::Cursor::new(Vec::<u8>::new());
        usdc::CrateWriter::write(&data, &mut out).map_err(|e| format!("fixture write: {e}"))?;
        Ok(out.into_inner())
    }

    #[test]
    fn usdc_round_trip_quad() {
        let bytes = quad_usdc_bytes().expect("fixture builds");
        let dir = std::env::temp_dir().join("umber-usdc-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quad.usdc");
        std::fs::write(&path, &bytes).unwrap();
        let mesh = load_usdc(&path).expect("usdc loads");
        // 4 corners x 2 triangles x 3 floats, per-corner expansion.
        assert_eq!(
            mesh.positions.len(),
            4 * 2,
            "4 corners x 2 tris (per-vertex [f32;3] entries)"
        );
        assert_eq!(mesh.indices.len(), 6, "two triangles");
        // RightHanded default keeps winding: v0, v1, v2.
        assert_eq!(&mesh.indices[..3], &[0, 1, 2]);
        // UVs carried per corner (4 corners x 2 tris).
        assert_eq!(mesh.uvs.len(), 4 * 2);
        assert!(
            (mesh.uvs[0][0] - 0.0).abs() < 1e-6 && (mesh.uvs[1][0] - 1.0).abs() < 1e-6,
            "uv[0] == (0,0), uv[1] == (1,0): {:?}",
            &mesh.uvs[..2]
        );
    }

    #[test]
    fn usdc_truncated_fails_cleanly() {
        let bytes = quad_usdc_bytes().expect("fixture builds");
        let truncated = &bytes[..bytes.len() / 2];
        let dir = std::env::temp_dir().join("umber-usdc-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("truncated.usdc");
        std::fs::write(&path, truncated).unwrap();
        assert!(
            load_usdc(&path).is_err(),
            "a truncated crate must fail, not panic"
        );
    }

    #[test]
    fn usdc_wrong_magic_is_usdbinary_error() {
        let dir = std::env::temp_dir().join("umber-usdc-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notusdc.usdc");
        std::fs::write(&path, b"garbage that is not a crate").unwrap();
        assert!(matches!(load_usdc(&path), Err(ImportError::UsdBinary)));
    }

    #[test]
    fn usdc_3m_mesh_fans_and_normals() {
        // A cube's 6 faces through the crate path: the flat normals are
        // Newell-computed (none authored) and the winding is CCW.
        let usda_text = r#"
#usda 1.0

def Mesh "Cube"
{
    float3[] points = [(-1,-1,-1), (1,-1,-1), (1,1,-1), (-1,1,-1), (-1,-1,1), (1,-1,1), (1,1,1), (-1,1,1)]
    int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
    int[] faceVertexIndices = [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 5, 4, 2, 3, 7, 6, 0, 3, 7, 4, 1, 2, 6, 5]
}
"#;
        let data = openusd::usda::parse(usda_text).expect("fixture parses");
        let mut out = std::io::Cursor::new(Vec::<u8>::new());
        usdc::CrateWriter::write(&data, &mut out).expect("fixture writes");
        let dir = std::env::temp_dir().join("umber-usdc-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cube.usdc");
        std::fs::write(&path, out.into_inner()).unwrap();
        let mesh = load_usdc(&path).expect("cube loads");
        assert_eq!(
            mesh.indices.len(),
            6 * 2 * 3,
            "6 faces x 2 tris x 3 corners"
        );
        assert_eq!(
            mesh.normals.len(),
            mesh.positions.len(),
            "normals per corner"
        );
        // The flat normals are unit length (Newell normalized).
        for n in &mesh.normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "normal not unit: {n:?}");
        }
        // UVs zero-filled (none authored).
        assert!(mesh.uvs.iter().all(|u| u[0] == 0.0 && u[1] == 0.0));
    }
}
