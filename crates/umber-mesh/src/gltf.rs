//! glTF 2.0 / GLB mesh import via the [`gltf`] crate.
//!
//! All meshes and primitives in the document are concatenated into a single
//! [`MeshData`](crate::MeshData); per-primitive indices are offset by the
//! vertex base accumulated so far. Missing normals or `TEXCOORD_0` are filled
//! with zeros so the attribute vectors always stay parallel to `positions`.
//! Non-indexed primitives are treated as sequential triangle lists.

use std::path::Path;

use super::{ImportError, MeshData};

/// Load a `.gltf` or `.glb` file into a single concatenated [`MeshData`].
///
/// Returns [`ImportError::Gltf`] when the document cannot be imported, when a
/// buffer view cannot be resolved, or when the document contains no position
/// data at all.
pub fn load_gltf(path: &Path) -> Result<MeshData, ImportError> {
    let (document, buffers, _) =
        gltf::import(path).map_err(|e| ImportError::Gltf(e.to_string()))?;
    validate_accessors(&document)?;
    let mut data = MeshData::default();
    // Per-set masked bakes §1: the triangle→material map attaches only
    // when every primitive that emitted triangles named its material.
    let mut pending_triangle_materials: Vec<u16> = Vec::new();
    let mut complete = true;
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            append_primitive(
                &mut data,
                &primitive,
                &buffers,
                &mut pending_triangle_materials,
                &mut complete,
            )?;
        }
    }
    if complete && pending_triangle_materials.len() == data.triangle_count() {
        data.triangle_materials = pending_triangle_materials;
    }
    if data.positions.is_empty() {
        return Err(ImportError::Gltf("no mesh geometry found".into()));
    }
    Ok(data)
}

/// Pre-validates every accessor a primitive will read, BEFORE the `gltf`
/// crate's reader touches it. gltf 1.4.1 panics (or `debug_assert`s) on
/// shapes a crafted file can declare — the panic hunt's findings #9-#15:
/// `count: 0` underflows `count - 1`, a huge count overflows
/// `stride * (count - 1)`, a short `byteStride` or a wrong component type
/// / dimensions trips an assert, a float/signed index type or a u32
/// TEXCOORD hits `unreachable!()`. Each shape is refused here with a
/// typed error naming the accessor.
fn validate_accessors(document: &gltf::Document) -> Result<(), ImportError> {
    use gltf::accessor::{DataType, Dimensions};
    use gltf::mesh::Semantic;
    // A cap far past any real mesh (2^28 elements ≈ 268M) and far below
    // where `stride * count` could overflow a usize.
    const MAX_COUNT: usize = 1 << 28;
    let check = |accessor: &gltf::Accessor<'_>, role: &str| -> Result<(), ImportError> {
        let idx = accessor.index();
        let count = accessor.count();
        if count == 0 || count > MAX_COUNT {
            return Err(ImportError::Gltf(format!(
                "accessor {idx} ({role}): count {count} outside 1..={MAX_COUNT}"
            )));
        }
        let elem = accessor.size();
        if let Some(view) = accessor.view() {
            if let Some(stride) = view.stride() {
                if stride < elem {
                    return Err(ImportError::Gltf(format!(
                        "accessor {idx} ({role}): byteStride {stride} < element size {elem}"
                    )));
                }
            }
        }
        Ok(())
    };
    let want = |accessor: &gltf::Accessor<'_>,
                role: &str,
                dims: &[Dimensions],
                types: &[DataType]|
     -> Result<(), ImportError> {
        check(accessor, role)?;
        if !dims.contains(&accessor.dimensions()) || !types.contains(&accessor.data_type()) {
            return Err(ImportError::Gltf(format!(
                "accessor {} ({role}): {:?} {:?} is not a valid {role} layout",
                accessor.index(),
                accessor.data_type(),
                accessor.dimensions()
            )));
        }
        Ok(())
    };
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            for (semantic, accessor) in primitive.attributes() {
                match semantic {
                    Semantic::Positions => {
                        want(&accessor, "POSITION", &[Dimensions::Vec3], &[DataType::F32])?
                    }
                    Semantic::Normals => {
                        want(&accessor, "NORMAL", &[Dimensions::Vec3], &[DataType::F32])?
                    }
                    Semantic::TexCoords(_) => want(
                        &accessor,
                        "TEXCOORD",
                        &[Dimensions::Vec2],
                        &[DataType::F32, DataType::U8, DataType::U16],
                    )?,
                    // Attributes Umber never reads are still count/stride
                    // checked: the reader is not called on them, but the
                    // shape is cheap to refuse.
                    _ => check(&accessor, "attribute")?,
                }
            }
            if let Some(indices) = primitive.indices() {
                want(
                    &indices,
                    "indices",
                    &[Dimensions::Scalar],
                    &[DataType::U8, DataType::U16, DataType::U32],
                )?;
            }
        }
    }
    Ok(())
}

/// Append one glTF primitive to the accumulated [`MeshData`], offsetting its
/// indices by the current vertex base.
///
/// Primitives without `POSITION` data are skipped: there is nothing to keep
/// parallel with the rest of the shape.
fn append_primitive(
    data: &mut MeshData,
    primitive: &gltf::Primitive<'_>,
    buffers: &[gltf::buffer::Data],
    pending_triangle_materials: &mut Vec<u16>,
    complete: &mut bool,
) -> Result<(), ImportError> {
    let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| &data.0[..]));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .map(|values| values.collect())
        .unwrap_or_default();
    if positions.is_empty() {
        return Ok(());
    }
    let indices_before = data.indices.len();
    let base = u32::try_from(data.positions.len())
        .map_err(|_| ImportError::Gltf("more than u32::MAX vertices".into()))?;
    let vertex_count = u32::try_from(positions.len())
        .map_err(|_| ImportError::Gltf("primitive has more than u32::MAX vertices".into()))?;
    data.positions.extend(positions.iter().copied());

    match reader.read_normals() {
        Some(normals) => data.normals.extend(normals),
        None => data
            .normals
            .extend(std::iter::repeat_n([0.0; 3], positions.len())),
    }

    match reader.read_tex_coords(0) {
        Some(tex_coords) => data.uvs.extend(tex_coords.into_f32()),
        None => data
            .uvs
            .extend(std::iter::repeat_n([0.0; 2], positions.len())),
    }

    match reader.read_indices() {
        Some(indices) => {
            // Range-check against THIS primitive's vertex count, and offset
            // with checked arithmetic: an index past the primitive was
            // accepted and crashed the first consumer (finding #7), and
            // `i + base` overflowed on a u32::MAX index (finding #6).
            for i in indices.into_u32() {
                if i >= vertex_count {
                    return Err(ImportError::Gltf(format!(
                        "index {i} out of range ({vertex_count} vertices in the primitive)"
                    )));
                }
                let global = i.checked_add(base).ok_or_else(|| {
                    ImportError::Gltf("vertex index overflows u32 across primitives".into())
                })?;
                data.indices.push(global);
            }
        }
        None => {
            let end = base.checked_add(vertex_count).ok_or_else(|| {
                ImportError::Gltf("vertex count overflows u32 across primitives".into())
            })?;
            data.indices.extend(base..end);
        }
    }

    let material_slot = if let Some(name) = primitive.material().name() {
        match data.material_names.iter().position(|n| n == name) {
            Some(slot) => u16::try_from(slot).ok(),
            None => {
                if u16::try_from(data.material_names.len()).is_ok() {
                    data.material_names.push(name.to_string());
                    u16::try_from(data.material_names.len() - 1).ok()
                } else {
                    None
                }
            }
        }
    } else {
        None
    };

    // Per-set masked bakes §1: one material per triangle — a glTF
    // primitive is single-material, so every triangle this primitive
    // emits carries the same slot. Collect per-primitive, attach
    // only when EVERY primitive in the file mapped (all-or-nothing:
    // the map is either empty or exactly `triangle_count()` long; a
    // mixed named/unnamed file keeps the whole-mesh fallback).
    if let Some(slot) = material_slot {
        // indices are 3-per-triangle: the count is the emitted corners / 3.
        let triangles = (data.indices.len() - indices_before) / 3;
        if triangles > 0 {
            pending_triangle_materials.extend(std::iter::repeat_n(slot, triangles));
        }
    } else if indices_before != data.indices.len() {
        // This primitive emitted triangles but has no named material:
        // the whole-mesh fallback fires for the entire file.
        *complete = false;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Encode bytes as standard base64 (RFC 4648 §4) for data-URI buffers.
    fn base64_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let mut block: u32 = 0;
            for (i, byte) in chunk.iter().enumerate() {
                block |= (*byte as u32) << (16 - 8 * i);
            }
            let quantum = match chunk.len() {
                1 => {
                    out.push(ALPHABET[(block >> 18) as usize & 63] as char);
                    out.push(ALPHABET[(block >> 12) as usize & 63] as char);
                    "=="
                }
                2 => {
                    out.push(ALPHABET[(block >> 18) as usize & 63] as char);
                    out.push(ALPHABET[(block >> 12) as usize & 63] as char);
                    out.push(ALPHABET[(block >> 6) as usize & 63] as char);
                    "="
                }
                _ => {
                    for shift in [18, 12, 6, 0] {
                        out.push(ALPHABET[(block >> shift) as usize & 63] as char);
                    }
                    ""
                }
            };
            out.push_str(quantum);
        }
        out
    }

    /// Raw little-endian buffer for the triangle fixture: 3×VEC3 positions,
    /// 3×VEC2 uvs, 3×u16 indices (66 bytes total).
    fn triangle_buffer_bytes() -> Vec<u8> {
        let mut bytes = Vec::with_capacity(66);
        for v in [[0.0f32, 0.0, 0.0], [1.0f32, 0.0, 0.0], [0.0f32, 1.0, 0.0]] {
            for c in v {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        for v in [[0.0f32, 0.0], [1.0f32, 0.0], [0.0f32, 1.0]] {
            for c in v {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bytes.extend_from_slice(&i.to_le_bytes());
        }
        bytes
    }

    /// Minimal valid `.gltf` JSON for one triangle with `POSITION` +
    /// `TEXCOORD_0` (no normals: exercises the zero-fill path), an embedded
    /// data-URI buffer, and one named material.
    fn triangle_gltf_json() -> String {
        let uri = base64_encode(&triangle_buffer_bytes());
        format!(
            r#"{{
  "asset": {{"version": "2.0"}},
  "scene": 0,
  "scenes": [{{"nodes": [0]}}],
  "nodes": [{{"mesh": 0}}],
  "meshes": [{{
    "primitives": [{{
      "attributes": {{"POSITION": 0, "TEXCOORD_0": 1}},
      "indices": 2,
      "material": 0
    }}]
  }}],
  "materials": [{{"name": "TestMat"}}],
  "buffers": [{{
    "byteLength": 66,
    "uri": "data:application/octet-stream;base64,{uri}"
  }}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 36, "byteLength": 24}},
    {{"buffer": 0, "byteOffset": 60, "byteLength": 6}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
      "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]}},
    {{"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"}},
    {{"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}}
  ]
}}"#
        )
    }

    /// Write `contents` to a uniquely named file under the system temp dir and
    /// return its path. Unique per call so tests can run in parallel.
    fn write_fixture(file_name: &str, contents: &[u8]) -> std::path::PathBuf {
        let id = FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "umber-mesh-test-{}-{id}-{file_name}",
            std::process::id()
        ));
        std::fs::write(&path, contents).expect("fixture write must succeed");
        path
    }

    #[test]
    fn loads_embedded_gltf_triangle() {
        let path = write_fixture("triangle.gltf", triangle_gltf_json().as_bytes());
        let data = crate::load(&path).expect("triangle gltf must load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(data.vertex_count(), 3);
        assert_eq!(data.triangle_count(), 1);
        assert_eq!(
            data.positions,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        );
        assert_eq!(data.normals, vec![[0.0; 3]; 3]);
        assert_eq!(data.uvs, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(data.indices, vec![0, 1, 2]);
        assert_eq!(data.material_names, vec!["TestMat".to_string()]);
        // Per-set masked bakes §1: the primitive named its material, so the
        // triangle→material map is complete — one slot per triangle.
        assert_eq!(data.triangle_materials, vec![0u16]);
        assert_eq!(data.triangle_materials.len(), data.triangle_count());
    }

    /// Minimal valid `.glb`: JSON chunk (padded to 4 bytes) plus a BIN chunk
    /// carrying the same triangle buffer as the `.gltf` fixture.
    fn triangle_glb_bytes() -> Vec<u8> {
        let json = triangle_gltf_json_for_glb();
        let bin = triangle_buffer_bytes();
        let json_len = json.len().div_ceil(4) * 4;
        let total = 12 + 8 + json_len + 8 + bin.len();
        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_len as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(json.as_bytes());
        glb.extend_from_slice(&vec![b' '; json_len - json.len()]);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\x00");
        glb.extend_from_slice(&bin);
        glb
    }

    /// Same triangle document as [`triangle_gltf_json`], but with the buffer
    /// referencing the GLB BIN chunk (no `uri`).
    fn triangle_gltf_json_for_glb() -> String {
        r#"{
  "asset": {"version": "2.0"},
  "scene": 0,
  "scenes": [{"nodes": [0]}],
  "nodes": [{"mesh": 0}],
  "meshes": [{
    "primitives": [{
      "attributes": {"POSITION": 0, "TEXCOORD_0": 1},
      "indices": 2,
      "material": 0
    }]
  }],
  "materials": [{"name": "TestMat"}],
  "buffers": [{
    "byteLength": 66
  }],
  "bufferViews": [
    {"buffer": 0, "byteOffset": 0, "byteLength": 36},
    {"buffer": 0, "byteOffset": 36, "byteLength": 24},
    {"buffer": 0, "byteOffset": 60, "byteLength": 6}
  ],
  "accessors": [
    {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
      "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
    {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
    {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
  ]
}"#
        .to_string()
    }

    #[test]
    fn loads_glb_triangle() {
        let path = write_fixture("triangle.glb", &triangle_glb_bytes());
        let data = crate::load(&path).expect("triangle glb must load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(data.vertex_count(), 3);
        assert_eq!(data.triangle_count(), 1);
        assert_eq!(data.uvs, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(data.indices, vec![0, 1, 2]);
        assert_eq!(data.material_names, vec!["TestMat".to_string()]);
        assert_eq!(data.triangle_materials, vec![0u16]);
        assert_eq!(data.triangle_materials.len(), data.triangle_count());
    }
}
