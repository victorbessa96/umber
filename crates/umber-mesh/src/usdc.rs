//! Binary crate (`.usdc`) and package (`.usdz`) mesh import via
//! `openusd` 0.8.0 — the pure-Rust USD implementation.
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
//! Error contract: every failure maps to `ImportError::Usd`/`UsdBinary`;
//! topology (counts, indices) is validated before any of it is used. The
//! crate-reader panics and allocation aborts the 2026-10-14 panic hunt
//! found in openusd 0.7.0 (findings #19-#24, including a multi-TB
//! allocation that aborted the process) are fixed upstream in 0.8.0
//! (mxpv/openusd#138); the hunt's `bug_usdc_*` reproducers are live tests
//! of that. `umber_mesh::load`'s panic boundary stays as defence in depth.

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
    // openusd 0.8: the crate reader takes owned bytes (`impl
    // Into<AssetBuffer>`) instead of a `Read + Seek`.
    let data = usdc::CrateData::open(bytes, true)
        .map_err(|e| ImportError::Usd(format!("usdc: open failed: {e}")))?;
    extract_first_mesh(&data)
}

/// Loads a `.usdz` (package) file: the root layer's first Mesh prim,
/// then — when the root authors none — its sublayer stack.
///
/// The package is a zip; its root layer is the first `.usdc`/`.usda`/
/// `.usd` entry (the USDZ spec's rule), opened through
/// `openusd::usdz::Archive`. This used to call `usdc::read_file` on the
/// zip itself — which reads a bare crate — so no valid `.usdz` had ever
/// loaded (panic-hunt finding #8, `bug_usdz_valid_package_does_not_load`).
///
/// Beyond the first layer (the release checklist's
/// `.usdz-beyond-first-layer` deferral, closed here): real packages
/// compose — the root layer often sublayers the geometry while holding
/// only materials or metadata. The walk keeps first-layer precedence (a
/// root that authors a Mesh never reads a sublayer), then follows the
/// root's `subLayers` in authored order (USD opinion strength),
/// depth-first, resolving each path INSIDE the package. A sublayer the
/// package does not carry, or that fails to open, is skipped — USD's
/// own layer-registry tolerance (a missing sublayer must not abort the
/// load; a later one may hold the mesh). The walk is cycle-guarded and
/// depth-bounded: a sublayer graph is untrusted input. A nested
/// `.usdz` entry composes one level deep through `Archive::read`'s own
/// first-layer rule; a nested package's sublayers are not walked (the
/// named boundary).
pub fn load_usdz(path: &Path) -> Result<MeshData, ImportError> {
    let mut archive = openusd::usdz::Archive::open(path)
        .map_err(|e| ImportError::Usd(format!("usdz: open failed: {e}")))?;
    let root_layer = archive
        .read_first_layer()
        .map_err(|e| ImportError::Usd(format!("usdz: no readable root layer: {e}")))?;
    let root = SdfPath::abs_root();
    if let Some(mesh_path) = find_first_mesh(root_layer.as_ref(), &root) {
        return build_mesh_from_spec(root_layer.as_ref(), &mesh_path);
    }
    let mut visited = vec![archive.first_layer_name().unwrap_or_default()];
    for authored in sublayer_names(root_layer.as_ref(), &root)? {
        if let Some(mesh) = walk_sublayers(&mut archive, &authored, &mut visited, 0)? {
            return Ok(mesh);
        }
    }
    Err(ImportError::Usd(
        "usd: no Mesh prim in document".to_string(),
    ))
}

/// The pseudo-root's `subLayers` list, authored order. The usda parser
/// stores it as `Value::StringVec` (its own test pins
/// `["./someAnimation.usd", ...]`); a crate layer that carries
/// pseudo-root metadata may spell the same list as asset paths. A layer
/// that authors none yields an empty vec — the walk simply ends.
fn sublayer_names(data: &dyn AbstractData, root: &SdfPath) -> Result<Vec<String>, ImportError> {
    match data.try_field(root, "subLayers") {
        Ok(Some(v)) => match v.as_ref() {
            Value::StringVec(names) => Ok(names.to_vec()),
            Value::AssetPathVec(assets) => {
                Ok(assets.iter().map(|a| a.as_str().to_string()).collect())
            }
            _ => Ok(Vec::new()),
        },
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(ImportError::Usd(format!(
            "usd: subLayers decode failed: {e}"
        ))),
    }
}

/// Normalizes one authored sublayer ref to a package entry name:
/// `@./geo.usda@` → `geo.usda`. USDZ entries are package-relative (the
/// spec forbids escaping the package), so this is purely lexical:
/// strip the `@…@` quoting and any leading `./` runs; a path that
/// still climbs (`..`) or goes absolute names nothing inside the
/// package and yields `None`.
fn normalize_sublayer(authored: &str) -> Option<String> {
    let mut name = authored.trim().trim_matches('@').trim();
    while let Some(rest) = name.strip_prefix("./") {
        name = rest.trim_start_matches('/');
    }
    if name.is_empty() || name.starts_with('/') || name.split('/').any(|seg| seg == "..") {
        return None;
    }
    Some(name.to_string())
}

/// Depth-first walk of one sublayer: open it inside the package, look
/// for a Mesh, then recurse into its own sublayers. `visited` holds the
/// normalized entry names already walked (the cycle guard — the root's
/// own name seeds it); `depth` bounds the chain. A sublayer that is
/// absent, unreadable, or self-escaping is skipped, not fatal — but a
/// Mesh that IS found and malformed is a hard error: the document's
/// first Mesh is broken, and the error contract says so.
fn walk_sublayers(
    archive: &mut openusd::usdz::Archive,
    authored: &str,
    visited: &mut Vec<String>,
    depth: usize,
) -> Result<Option<MeshData>, ImportError> {
    const MAX_SUB_LAYER_DEPTH: usize = 16;
    if depth >= MAX_SUB_LAYER_DEPTH {
        return Ok(None);
    }
    let Some(name) = normalize_sublayer(authored) else {
        return Ok(None);
    };
    if visited.iter().any(|v| v == &name) {
        return Ok(None);
    }
    if !archive.contains(&name) {
        return Ok(None);
    }
    let layer = match archive.read(&name) {
        Ok(layer) => layer,
        Err(_) => return Ok(None),
    };
    visited.push(name);
    let root = SdfPath::abs_root();
    if let Some(mesh_path) = find_first_mesh(layer.as_ref(), &root) {
        return build_mesh_from_spec(layer.as_ref(), &mesh_path).map(Some);
    }
    for sub in sublayer_names(layer.as_ref(), &root)? {
        if let Some(mesh) = walk_sublayers(archive, &sub, visited, depth + 1)? {
            return Ok(Some(mesh));
        }
    }
    Ok(None)
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

/// One field lookup. openusd is path-keyed: an attribute's VALUE lives on
/// the attribute spec at `<prim>.<attr>` in its `default` field, while
/// prim-level fields (`typeName`, `orientation`) sit on the prim spec
/// itself. This helper resolves both: if `<path>.<name>` is an attribute
/// spec, its `default` is the value; otherwise the field reads from the
/// prim spec directly.
fn field(
    data: &dyn AbstractData,
    path: &SdfPath,
    name: &str,
) -> Result<Option<Value>, ImportError> {
    // The property spec path, if it parses. USD namespaced identifiers
    // (colons) are VALID property-name segments in Sdf paths — the parser
    // registers `primvars:st` at `/Mesh.primvars:st` verbatim
    // (is_valid_namespace_identifier accepts ':' segments; the round-6
    // split into dot segments pointed at a path that never exists).
    if let Ok(prop) = path.append_property(name) {
        if data.spec_type(&prop) == Some(SpecType::Attribute) {
            return match data.try_field(&prop, "default") {
                Ok(opt) => Ok(opt.map(|cow| cow.into_owned())),
                Err(e) => Err(ImportError::Usd(format!(
                    "usd: {}.{} decode failed: {e}",
                    prop.as_str(),
                    "default"
                ))),
            };
        }
    }
    // Prim-level field (typeName, orientation, ...) — read on the prim.
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
    // Validate the topology ONCE, up front, before any closure indexes
    // `points`: every count >= 3 (a negative count once passed the sum
    // check and sliced out of bounds), every index in `0..point_count` (a
    // negative or past-the-end index once crashed the Newell closure,
    // which ran before the per-corner range check). After this block every
    // index below is a valid point index — the panic-hunt findings #1-#3.
    let point_count = points.len() / 3;
    for (fi, &count) in counts.iter().enumerate() {
        if count < 3 {
            return Err(ImportError::Usd(format!(
                "usd: {path}: face {fi} has {count} corners (< 3)"
            )));
        }
    }
    for (k, &vi) in indices.iter().enumerate() {
        if vi < 0 || vi as usize >= point_count {
            return Err(ImportError::Usd(format!(
                "usd: {path}: faceVertexIndices[{k}] = {vi} out of range ({point_count} points)"
            )));
        }
    }
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
    // --- Normals: primvars:normals else normals; typed-vector aware
    // (`float3[]` parses to Vec3fVec, not FloatVec); flat Newell when absent.
    for name in ["primvars:normals", "normals"] {
        match field(data, path, name)? {
            Some(Value::FloatVec(v)) => {
                normals = Some(v);
                break;
            }
            Some(Value::Vec3fVec(v)) => {
                normals = Some(v.into_iter().flat_map(<[f32; 3]>::from).collect());
                break;
            }
            _ => continue,
        }
    }

    // --- UVs: primvars:st; typed-vector aware (`texCoord2f[]` parses to
    // Vec2fVec); zero-filled when absent (the .usda rule) ---
    let uvs: Vec<f32> = match field(data, path, "primvars:st")? {
        Some(Value::FloatVec(v)) => v,
        Some(Value::Vec2fVec(v)) => v.into_iter().flat_map(<[f32; 2]>::from).collect(),
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
    for &count in &counts {
        // Counts are >= 3 and sum to indices.len() (validated above), so
        // this slice is in bounds.
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
        // Per-set masked bakes §1: USD's GeomSubset binding is the named
        // follow-up; empty is the honest whole-mesh state.
        triangle_materials: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use openusd::usdz::ArchiveWriter;

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
            2 * 3,
            "a quad fans into 2 triangles x 3 corners (per-vertex [f32;3] entries)"
        );
        assert_eq!(mesh.indices.len(), 6, "two triangles");
        // RightHanded default keeps winding: v0, v1, v2.
        assert_eq!(&mesh.indices[..3], &[0, 1, 2]);
        // UVs carried per corner (2 tris x 3 corners).
        assert_eq!(mesh.uvs.len(), 2 * 3);
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

    // --- .usdz packages: the sublayer walk (the release checklist's
    // `.usdz-beyond-first-layer` deferral, closed). Fixtures are
    // packages built programmatically through `usdz::ArchiveWriter` —
    // raw .usda layer texts, stored entries, never shipped binaries.

    /// A quad Mesh layer whose first corner is `first` — the tell that
    /// distinguishes one quad from another in the precedence tests.
    fn quad_mesh_body(first: &str) -> String {
        format!("\ndef Mesh \"Mesh\"\n{{\n    float3[] points = [({first}), (1, -1, 0), (1, 1, 0), (-1, 1, 0)]\n    int[] faceVertexCounts = [4]\n    int[] faceVertexIndices = [0, 1, 2, 3]\n}}\n")
    }

    fn quad_layer(first: &str) -> String {
        format!("#usda 1.0\n{}", quad_mesh_body(first))
    }

    /// A layer that sublayers `names` (authored order) and optionally
    /// carries its own quad Mesh.
    fn sublayering_layer(names: &[&str], mesh_first_corner: Option<&str>) -> String {
        let mut text = String::from("#usda 1.0\n(\n");
        if !names.is_empty() {
            let refs: Vec<String> = names.iter().map(|n| format!("@./{n}@")).collect();
            text.push_str(&format!("    subLayers = [{}]\n", refs.join(", ")));
        }
        text.push_str(")\n");
        if let Some(first) = mesh_first_corner {
            text.push_str(&quad_mesh_body(first));
        }
        text
    }

    fn write_usdz(dir_name: &str, file_name: &str, layers: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file_name);
        let mut zip = ArchiveWriter::new(std::fs::File::create(&path).unwrap());
        for (name, text) in layers {
            zip.add_layer(name, text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn usdz_root_mesh_precedes_sublayers() {
        // First-layer precedence: the walk only fires when the root
        // layer authors no Mesh. A root WITH a mesh never reads one.
        let path = write_usdz(
            "umber-usdz-tests",
            "root_mesh.usdz",
            &[
                (
                    "root.usda",
                    &sublayering_layer(&["geo.usda"], Some("-1, -1, 0")),
                ),
                ("geo.usda", &quad_layer("9, 9, 9")),
            ],
        );
        let mesh = load_usdz(&path).expect("root mesh loads");
        assert_eq!(
            mesh.positions[0],
            [-1.0, -1.0, 0.0],
            "the ROOT layer's mesh wins over the sublayer's"
        );
    }

    #[test]
    fn usdz_sublayer_mesh_loads() {
        let path = write_usdz(
            "umber-usdz-tests",
            "sublayer.usdz",
            &[
                ("root.usda", &sublayering_layer(&["geo.usda"], None)),
                ("geo.usda", &quad_layer("-1, -1, 0")),
            ],
        );
        let mesh = load_usdz(&path).expect("the sublayer's mesh loads");
        assert_eq!(mesh.indices.len(), 6, "the quad fans into 2 tris");
        assert_eq!(mesh.positions[0], [-1.0, -1.0, 0.0]);
    }

    #[test]
    fn usdz_nested_sublayers_load() {
        let path = write_usdz(
            "umber-usdz-tests",
            "nested.usdz",
            &[
                ("root.usda", &sublayering_layer(&["mid.usda"], None)),
                ("mid.usda", &sublayering_layer(&["leaf.usda"], None)),
                ("leaf.usda", &quad_layer("2, 2, 2")),
            ],
        );
        let mesh = load_usdz(&path).expect("the leaf's mesh loads through the chain");
        assert_eq!(mesh.positions[0], [2.0, 2.0, 2.0]);
    }

    #[test]
    fn usdz_missing_sublayer_tolerated() {
        // USD's tolerance rule: a sublayer the package does not carry is
        // skipped; a later one still holds the mesh.
        let path = write_usdz(
            "umber-usdz-tests",
            "missing_sub.usdz",
            &[
                (
                    "root.usda",
                    &sublayering_layer(&["gone.usda", "geo.usda"], None),
                ),
                ("geo.usda", &quad_layer("-1, -1, 0")),
            ],
        );
        let mesh = load_usdz(&path).expect("the mesh behind the missing sublayer loads");
        assert_eq!(mesh.positions[0], [-1.0, -1.0, 0.0]);
    }

    #[test]
    fn usdz_sublayer_cycle_terminates() {
        // a -> b -> a with no Mesh anywhere: the cycle guard ends the
        // walk instead of recursing forever.
        let path = write_usdz(
            "umber-usdz-tests",
            "cycle.usdz",
            &[
                ("root.usda", &sublayering_layer(&["a.usda"], None)),
                ("a.usda", &sublayering_layer(&["b.usda"], None)),
                ("b.usda", &sublayering_layer(&["a.usda"], None)),
            ],
        );
        let err = load_usdz(&path).expect_err("no mesh anywhere in the cycle");
        assert!(
            err.to_string().contains("no Mesh prim"),
            "the walk terminates with the honest error: {err}"
        );
    }

    #[test]
    fn usdz_escaping_sublayer_rejected() {
        // Package-relative only: `../outside.usda` and `/abs.usda`
        // name nothing inside the package and are skipped.
        let root = "#usda 1.0\n(\n    subLayers = [@../outside.usda@, @/abs.usda@]\n)\n";
        let path = write_usdz("umber-usdz-tests", "escape.usdz", &[("root.usda", root)]);
        let err = load_usdz(&path).expect_err("nothing inside the package to load");
        assert!(err.to_string().contains("no Mesh prim"), "{err}");
    }

    #[test]
    fn usdz_sublayer_depth_bound_enforced() {
        // A 20-deep chain with the mesh only at the far end: the walk
        // stops at MAX_SUB_LAYER_DEPTH (16) and reports no mesh —
        // bounded, not overflowed.
        let mut layers: Vec<(String, String)> = vec![(
            "root.usda".to_string(),
            sublayering_layer(&["l1.usda"], None),
        )];
        for i in 1..20 {
            layers.push((
                format!("l{i}.usda"),
                sublayering_layer(&[&format!("l{}.usda", i + 1)], None),
            ));
        }
        layers.push(("l20.usda".to_string(), quad_layer("3, 3, 3")));
        let borrowed: Vec<(&str, &str)> = layers
            .iter()
            .map(|(n, t)| (n.as_str(), t.as_str()))
            .collect();
        let path = write_usdz("umber-usdz-tests", "deep.usdz", &borrowed);
        let err = load_usdz(&path).expect_err("beyond the depth bound");
        assert!(err.to_string().contains("no Mesh prim"), "{err}");
    }

    #[test]
    fn usdz_sublayer_depth_bound_reachable() {
        // The positive half: a 15-deep chain IS within the bound and
        // loads — the bound bounds hostile chains, not real ones.
        let mut layers: Vec<(String, String)> = vec![(
            "root.usda".to_string(),
            sublayering_layer(&["l1.usda"], None),
        )];
        for i in 1..15 {
            layers.push((
                format!("l{i}.usda"),
                sublayering_layer(&[&format!("l{}.usda", i + 1)], None),
            ));
        }
        layers.push(("l15.usda".to_string(), quad_layer("4, 4, 4")));
        let borrowed: Vec<(&str, &str)> = layers
            .iter()
            .map(|(n, t)| (n.as_str(), t.as_str()))
            .collect();
        let path = write_usdz("umber-usdz-tests", "deep_ok.usdz", &borrowed);
        let mesh = load_usdz(&path).expect("a 15-deep chain loads");
        assert_eq!(mesh.positions[0], [4.0, 4.0, 4.0]);
    }
}
