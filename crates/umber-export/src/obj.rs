//! Wavefront OBJ geometry and material export — deterministic triangle meshes.
//!
//! The public API mirrors the borrowed [`crate::usd::UsdMeshView`] shape so
//! the export crate remains mesh-free. Parallel attributes use the same
//! one-based index at each corner, matching `tobj`'s `single_index=true`
//! import contract. The material entry points consume the driver's existing
//! [`crate::usd::UsdTexture`] walk and emit one MTL material with passthrough
//! texture references; packed channels are refused rather than reinterpreted.

use std::path::{Path, PathBuf};

use umber_graph::mtlx_doc::UDIM_TOKEN;

use crate::driver::{ExportError, TokenSources};
use crate::presets::OutputSpec;
use crate::tiled_template;
use crate::usd::{UsdMeshView, UsdTexture};

/// Borrowed geometry accepted by the OBJ writer.
pub type ObjMeshView<'a> = UsdMeshView<'a>;

/// Upper bound on one exported OBJ document.
pub const MAX_OBJ_EXPORT_BYTES: u64 = 1 << 28;

/// Upper bound on one sanitized object/file identifier. Filesystem components
/// and the OBJ object declaration stay bounded before any allocation happens.
pub(crate) const MAX_OBJ_NAME_BYTES: usize = 240;

/// Byte length of [`obj_name`]'s sanitized output, computed without allocating
/// the name. Every input character maps to at most one ASCII byte.
pub(crate) fn obj_name_len(asset: &str) -> usize {
    let mut len = asset.chars().count();
    if len == 0 || asset.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        len += 2;
    }
    len
}

/// One bounded ASCII object/file identifier: no path separators or whitespace
/// can escape the output directory or split the OBJ object declaration.
pub(crate) fn obj_name(asset: &str) -> String {
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

/// Maximum MTL document size, matched to the OBJ text budget.
pub const MAX_MTL_EXPORT_BYTES: u64 = MAX_OBJ_EXPORT_BYTES;

fn write_error(message: impl Into<String>) -> ExportError {
    ExportError::Write(format!("obj: {}", message.into()))
}

fn object_stem(asset: &str, tile: Option<u16>) -> Result<String, ExportError> {
    let mut name = asset.to_owned();
    if let Some(tile) = tile {
        name.push('_');
        name.push_str(&tile.to_string());
    }
    if obj_name_len(&name) > MAX_OBJ_NAME_BYTES {
        return Err(write_error(format!(
            "asset name exceeds {MAX_OBJ_NAME_BYTES} bytes after sanitization"
        )));
    }
    Ok(obj_name(&name))
}

fn validate_texture_reference(file: &str) -> Result<(), ExportError> {
    if file.is_empty() {
        return Err(write_error("texture reference is empty"));
    }
    if file.contains(['\n', '\r', '\0']) {
        return Err(write_error(format!(
            "texture file {file:?} contains a line break or NUL"
        )));
    }
    if file.contains(UDIM_TOKEN) {
        return Err(write_error(
            "OBJ/MTL texture references must use concrete tile names",
        ));
    }
    if file.contains("://") {
        return Err(write_error(format!(
            "texture file {file:?} must be a relative path, not a URI"
        )));
    }
    if let Some((prefix, _)) = file.split_once(':') {
        if !prefix.is_empty()
            && prefix
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            return Err(write_error(format!(
                "texture file {file:?} has a URI scheme or Windows drive prefix"
            )));
        }
    }
    if file.contains('\\') {
        return Err(write_error(format!(
            "texture file {file:?} must use forward slashes"
        )));
    }
    let path = Path::new(file);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(write_error(format!(
            "texture file {file:?} escapes the export directory"
        )));
    }
    Ok(())
}

fn mtl_key(input: &str) -> Option<&'static str> {
    match input {
        "base_color" => Some("map_Kd"),
        "geometry_normal" => Some("map_Bump"),
        "specular_roughness" => Some("map_Pr"),
        "base_metalness" => Some("map_Pm"),
        "emission_color" => Some("map_Ke"),
        "geometry_opacity" => Some("map_d"),
        _ => None,
    }
}

/// Serialize one MTL material from the driver's existing texture walk.
///
/// Only passthrough references are representable honestly: `rgb` for a
/// color/vector image and `r` for a scalar image. Packed channels require a
/// separate-map preset and are refused rather than assigned invented syntax.
pub fn to_mtl_document(
    asset: &str,
    textures: &[UsdTexture],
    tile: Option<u16>,
) -> Result<String, ExportError> {
    let object = object_stem(asset, tile)?;
    let mut bound = 1024u64 + 4 * object.len() as u64;
    for texture in textures {
        validate_texture_reference(&texture.file)?;
        bound = bound.saturating_add(64 + texture.file.len() as u64);
    }
    if bound > MAX_MTL_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: bound,
            cap: MAX_MTL_EXPORT_BYTES,
        });
    }

    let mut out = String::new();
    out.push_str("# Umber MTL export\n");
    out.push_str(&format!("# asset {object}\n"));
    out.push_str(&format!("newmtl {object}\nKd 1 1 1\nillum 2\n"));
    for texture in textures {
        let Some(key) = mtl_key(texture.input) else {
            return Err(write_error(format!(
                "unsupported MTL input {:?}",
                texture.input
            )));
        };
        if texture.output != "rgb" && texture.output != "r" {
            return Err(write_error(format!(
                "packed channel {:?} for {} requires a separate-map preset",
                texture.output, texture.file
            )));
        }
        out.push_str(&format!("{key} {}\n", texture.file));
    }
    if out.len() as u64 > MAX_MTL_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: out.len() as u64,
            cap: MAX_MTL_EXPORT_BYTES,
        });
    }
    Ok(out)
}

/// Serialize OBJ geometry with its matching `mtllib` and `usemtl` binding.
pub fn to_obj_document_with_material(
    asset: &str,
    mesh: &ObjMeshView<'_>,
    textures: &[UsdTexture],
    tile: Option<u16>,
) -> Result<String, ExportError> {
    let object = object_stem(asset, tile)?;
    let _ = to_mtl_document(asset, textures, tile)?;
    let mut doc = to_obj_document(asset, mesh)?;
    let marker = format!("o {}\n", obj_name(asset));
    let binding = format!("mtllib {object}.mtl\n{marker}usemtl {object}\n");
    doc = doc.replacen(&marker, &binding, 1);
    if doc.len() as u64 > MAX_OBJ_EXPORT_BYTES {
        return Err(ExportError::TooLarge {
            size: doc.len() as u64,
            cap: MAX_OBJ_EXPORT_BYTES,
        });
    }
    Ok(doc)
}

/// Resolve one output to the concrete filename the tiled driver wrote.
pub fn obj_file_for_output(
    output: &OutputSpec,
    sources: &TokenSources<'_>,
    tile: Option<u16>,
) -> String {
    if let Some(tile) = tile {
        let udim = tile.to_string();
        let sources = TokenSources {
            udim: &udim,
            ..*sources
        };
        sources.expand_output_from(output, &tiled_template(&output.filename))
    } else {
        sources.expand_output(output)
    }
}

/// Write an OBJ/MTL pair after validating every texture reference exists.
/// MTL is written first; failures restore its previous bytes or remove a
/// newly-created file, so an OBJ failure never leaves a changed MTL behind.
pub fn write_obj_with_mtl(
    out_dir: &Path,
    asset: &str,
    mesh: &ObjMeshView<'_>,
    textures: &[UsdTexture],
    tile: Option<u16>,
) -> Result<(PathBuf, PathBuf), ExportError> {
    let object = object_stem(asset, tile)?;
    let mtl_doc = to_mtl_document(asset, textures, tile)?;
    let obj_doc = to_obj_document_with_material(asset, mesh, textures, tile)?;
    let root = if textures.is_empty() {
        None
    } else {
        Some(std::fs::canonicalize(out_dir).map_err(|e| write_error(e.to_string()))?)
    };
    for texture in textures {
        validate_texture_reference(&texture.file)?;
        let path = root
            .as_ref()
            .expect("non-empty texture walk has a root")
            .join(&texture.file);
        let resolved = std::fs::canonicalize(&path).map_err(|_| {
            write_error(format!(
                "referenced texture {} does not exist",
                path.display()
            ))
        })?;
        let root = root.as_ref().expect("non-empty texture walk has a root");
        if !resolved.starts_with(root) || !resolved.is_file() {
            return Err(write_error(format!(
                "referenced texture {} escapes the export directory",
                path.display()
            )));
        }
    }
    std::fs::create_dir_all(out_dir).map_err(|e| write_error(e.to_string()))?;
    let obj_path = out_dir.join(format!("{object}.obj"));
    let mtl_path = out_dir.join(format!("{object}.mtl"));
    if obj_path.is_dir() || mtl_path.is_dir() {
        return Err(write_error(
            "OBJ/MTL output path is an existing directory".to_owned(),
        ));
    }
    let previous_mtl = match std::fs::read(&mtl_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(write_error(error.to_string())),
    };
    let restore_mtl = |path: &Path| {
        if let Some(bytes) = &previous_mtl {
            let _ = std::fs::write(path, bytes);
        } else {
            let _ = std::fs::remove_file(path);
        }
    };
    if let Err(error) = std::fs::write(&mtl_path, mtl_doc) {
        restore_mtl(&mtl_path);
        return Err(write_error(error.to_string()));
    }
    let temp_obj = out_dir.join(format!(".{object}.obj.tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&temp_obj);
    if let Err(error) = std::fs::write(&temp_obj, obj_doc) {
        let _ = std::fs::remove_file(&temp_obj);
        restore_mtl(&mtl_path);
        return Err(write_error(error.to_string()));
    }
    if obj_path.exists() {
        let backup = out_dir.join(format!(".{object}.obj.bak-{}", std::process::id()));
        let _ = std::fs::remove_file(&backup);
        if let Err(error) = std::fs::rename(&obj_path, &backup) {
            let _ = std::fs::remove_file(&temp_obj);
            restore_mtl(&mtl_path);
            return Err(write_error(error.to_string()));
        }
        if let Err(error) = std::fs::rename(&temp_obj, &obj_path) {
            let _ = std::fs::rename(&backup, &obj_path);
            let _ = std::fs::remove_file(&temp_obj);
            restore_mtl(&mtl_path);
            return Err(write_error(error.to_string()));
        }
        let _ = std::fs::remove_file(backup);
    } else if let Err(error) = std::fs::rename(&temp_obj, &obj_path) {
        let _ = std::fs::remove_file(&temp_obj);
        restore_mtl(&mtl_path);
        return Err(write_error(error.to_string()));
    }
    Ok((obj_path, mtl_path))
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
        let assert_writer_rejected = |case: &str, mesh: ObjMeshView<'_>| {
            let dir = std::env::temp_dir().join(format!(
                "umber-obj-ragged-writer-{case}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            assert!(matches!(
                write_obj(&dir, "S", &mesh),
                Err(ExportError::Write(_))
            ));
            assert!(!dir.exists(), "{case}: writer created a partial output dir");
        };
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &uvs, &[])),
            Err(ExportError::Write(_))
        ));
        assert_writer_rejected("empty", view(&positions, &normals, &uvs, &[]));
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &uvs, &[0, 1])),
            Err(ExportError::Write(_))
        ));
        assert_writer_rejected("ragged", view(&positions, &normals, &uvs, &[0, 1]));
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &uvs, &[0, 1, 99])),
            Err(ExportError::Write(_))
        ));
        assert_writer_rejected(
            "out-of-range",
            view(&positions, &normals, &uvs, &[0, 1, 99]),
        );
        let short_normals = vec![[0.0f32, 0.0, 1.0]; 2];
        assert!(matches!(
            to_obj_document("S", &view(&positions, &short_normals, &uvs, &[0, 1, 2])),
            Err(ExportError::Write(_))
        ));
        assert_writer_rejected(
            "short-normals",
            view(&positions, &short_normals, &uvs, &[0, 1, 2]),
        );
        let short_uvs = vec![[0.0f32, 0.0]; 2];
        assert!(matches!(
            to_obj_document("S", &view(&positions, &normals, &short_uvs, &[0, 1, 2])),
            Err(ExportError::Write(_))
        ));
        assert_writer_rejected(
            "short-uvs",
            view(&positions, &normals, &short_uvs, &[0, 1, 2]),
        );

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

        // `create_dir_all(out_dir)` itself fails when out_dir is a regular
        // file. The error is typed, not a panic or a success report.
        let dir = std::env::temp_dir().join(format!(
            "umber-obj-create-dir-failure-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&dir);
        std::fs::write(&dir, b"file, not a directory").unwrap();
        assert!(matches!(
            write_obj(&dir, "Sword", &mesh),
            Err(ExportError::Write(_))
        ));
        let _ = std::fs::remove_file(dir);
    }

    fn texture(input: &'static str, output: &'static str, file: &str) -> UsdTexture {
        UsdTexture {
            file: file.to_owned(),
            input,
            input_type: if output == "rgb" { "color3" } else { "float" },
            output,
            source_colorspace: "raw",
        }
    }

    fn write_texture(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), b"texture").unwrap();
    }

    #[test]
    fn mtl_document_maps_every_supported_passthrough_input() {
        let textures = vec![
            texture("base_color", "rgb", "base.png"),
            texture("geometry_normal", "rgb", "normal.png"),
            texture("specular_roughness", "r", "rough.png"),
            texture("base_metalness", "r", "metal.png"),
            texture("emission_color", "rgb", "emission.png"),
            texture("geometry_opacity", "r", "opacity.png"),
        ];
        let doc = to_mtl_document("Sword", &textures, None).unwrap();
        for expected in [
            "newmtl Sword",
            "map_Kd base.png",
            "map_Bump normal.png",
            "map_Pr rough.png",
            "map_Pm metal.png",
            "map_Ke emission.png",
            "map_d opacity.png",
        ] {
            assert!(doc.contains(expected), "missing {expected:?}: {doc}");
        }
    }

    #[test]
    fn material_pair_uses_the_real_driver_and_reimports() {
        use crate::driver::{run_preset, MapSet};
        use crate::presets::{ExportPreset, MapKind};

        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let dir = std::env::temp_dir().join(format!("umber-obj-mtl-driver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut set = MapSet::new(2);
        for kind in [
            MapKind::BaseColor,
            MapKind::Roughness,
            MapKind::Metallic,
            MapKind::Normal,
        ] {
            set.set(kind, vec![128; 16]);
        }
        let preset = ExportPreset::blender_principled();
        let sources = crate::driver::TokenSources::new("Sword");
        run_preset(&preset, &set, &sources, &dir).expect("real texture run");
        let textures = crate::usd::usd_textures(&preset, |output| sources.expand_output(output));
        let (obj, mtl) = write_obj_with_mtl(&dir, "Sword", &mesh, &textures, None).unwrap();
        let obj_a = std::fs::read(&obj).unwrap();
        let mtl_a = std::fs::read(&mtl).unwrap();
        let (obj_b, mtl_b) = write_obj_with_mtl(&dir, "Sword", &mesh, &textures, None).unwrap();
        assert_eq!(obj_a, std::fs::read(obj_b).unwrap());
        assert_eq!(mtl_a, std::fs::read(mtl_b).unwrap());
        for texture in &textures {
            assert!(
                dir.join(&texture.file).is_file(),
                "missing {}",
                texture.file
            );
        }
        let mtl_text = std::fs::read_to_string(&mtl).unwrap();
        for key in ["map_Kd ", "map_Pr ", "map_Pm ", "map_Bump "] {
            assert!(mtl_text.contains(key), "missing {key}: {mtl_text}");
        }
        for load in [umber_mesh::load(&obj), umber_mesh::load_obj(&obj)] {
            let back = load.expect("OBJ/MTL re-import");
            assert_eq!(back.triangle_count(), 2);
            assert_eq!(back.material_names, vec!["Sword"]);
            assert!(back.positions.iter().flatten().all(|v| v.is_finite()));
            assert!(back.normals.iter().flatten().all(|v| v.is_finite()));
            assert!(back.uvs.iter().flatten().all(|v| v.is_finite()));
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn packed_and_unsafe_references_fail_before_creating_documents() {
        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let dir = std::env::temp_dir().join(format!("umber-obj-mtl-refuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let packed = [texture(
            "specular_roughness",
            "b",
            "Sword_metallicRoughness.png",
        )];
        assert!(matches!(
            write_obj_with_mtl(&dir, "Sword", &mesh, &packed, None),
            Err(ExportError::Write(_))
        ));
        assert!(!dir.exists(), "packed refusal created an output directory");
        for file in [
            "/tmp/base.png",
            "../base.png",
            "https://example.invalid/base.png",
            "file:base.png",
            "C:/base.png",
            "C:base.png",
            "bad\\base.png",
            "bad\nbase.png",
            "base_<UDIM>.png",
        ] {
            let textures = [texture("base_color", "rgb", file)];
            assert!(
                to_mtl_document("Sword", &textures, None).is_err(),
                "unsafe reference accepted: {file:?}"
            );
        }
    }

    #[test]
    fn missing_texture_and_directory_target_leave_no_pair() {
        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let dir =
            std::env::temp_dir().join(format!("umber-obj-mtl-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let textures = [texture("base_color", "rgb", "Sword_baseColor.png")];
        assert!(matches!(
            write_obj_with_mtl(&dir, "Sword", &mesh, &textures, None),
            Err(ExportError::Write(_))
        ));
        assert!(!dir.join("Sword.obj").exists());
        assert!(!dir.join("Sword.mtl").exists());

        write_texture(&dir, "Sword_baseColor.png");
        std::fs::create_dir_all(dir.join("Sword.obj")).unwrap();
        assert!(matches!(
            write_obj_with_mtl(&dir, "Sword", &mesh, &textures, None),
            Err(ExportError::Write(_))
        ));
        assert!(!dir.join("Sword.mtl").exists());

        #[cfg(unix)]
        {
            std::fs::remove_file(dir.join("Sword_baseColor.png")).unwrap();
            std::os::unix::fs::symlink("/etc/passwd", dir.join("Sword_baseColor.png")).unwrap();
            assert!(matches!(
                write_obj_with_mtl(&dir, "Sword", &mesh, &textures, None),
                Err(ExportError::Write(_))
            ));
            assert!(!dir.join("Sword.mtl").exists());
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tiled_pair_uses_concrete_names_and_reimports() {
        use crate::presets::ExportPreset;

        let (positions, normals, uvs, indices) = quad();
        let mesh = view(&positions, &normals, &uvs, &indices);
        let dir = std::env::temp_dir().join(format!("umber-obj-mtl-tile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let preset = ExportPreset::blender_principled();
        let sources = crate::driver::TokenSources::new("Sword");
        let file = obj_file_for_output(&preset.outputs[0], &sources, Some(1002));
        assert_eq!(file, "Sword_basecolor_1002.png");
        write_texture(&dir, &file);
        let textures = [texture("base_color", "rgb", &file)];
        let (obj, mtl) = write_obj_with_mtl(&dir, "Sword", &mesh, &textures, Some(1002)).unwrap();
        assert!(obj.ends_with("Sword_1002.obj"));
        assert!(mtl.ends_with("Sword_1002.mtl"));
        let obj_text = std::fs::read_to_string(&obj).unwrap();
        assert!(obj_text.contains("mtllib Sword_1002.mtl"));
        assert!(obj_text.contains("usemtl Sword_1002"));
        let mtl_text = std::fs::read_to_string(&mtl).unwrap();
        assert!(mtl_text.contains("newmtl Sword_1002"));
        assert!(mtl_text.contains("map_Kd Sword_basecolor_1002.png"));
        assert!(!mtl_text.contains("<UDIM>"));
        let back = umber_mesh::load_obj(&obj).expect("tiled OBJ/MTL re-import");
        assert_eq!(back.material_names, vec!["Sword_1002"]);
        assert_eq!(back.triangle_count(), 2);
        let _ = std::fs::remove_dir_all(dir);
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
