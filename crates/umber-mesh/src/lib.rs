//! umber-mesh — mesh container + import (glTF, OBJ, FBX, USD ASCII).
//!
//! Wave 1 scope: the `MeshData` container + format detection + OBJ import
//! (tobj). glTF and FBX loaders are the Wave-1 claw work; the half-edge
//! DCEL topology structure lands with seam-aware work (Wave 2+).

use glam::Vec3;

pub mod bake_support;
pub mod bvh;
pub mod fbx;
pub mod fill;
pub mod gltf;
pub mod mesh_maps;
pub mod overlay;
pub mod polygon_fill;
pub mod raycast;
pub mod seam;
pub mod seam_mirror;
pub mod tangents;
pub mod udim;
pub mod usd;
pub mod usdc;

pub use fbx::load_fbx;
pub use fill::{
    cylindrical_coords, evaluate_fill_tile, projection_sample_2d, rasterize_uv_geometry,
    sample_source, sample_triplanar, spherical_coords, triplanar_planar_uvs, triplanar_weights,
    wrap01, FillParams, FillProjection, FillSource, FillTile, ProjAxis, TexelGeom,
};
pub use gltf::load_gltf;
pub use mesh_maps::{
    format_mesh_map, parse_mesh_map, parse_mesh_map_stem, texture_set_name, MeshMapKind,
    MeshMapName,
};
pub use overlay::{wire_vertices_from_indices, WireVertex};
pub use polygon_fill::{
    flood_connected_triangles, flood_uv_island, pick_from_ray, point_in_polygon,
    rasterize_polygon_mask, uv_triangle_at, FillMask, LASSO_MAX_POINTS,
};
pub use raycast::{
    face_normal, front_facing_weight, ray_intersect, uv_at, walk_surface_path, RayHit,
};
pub use udim::{
    filter_mesh_for_triangles, present_tiles, tile_of_triangle, tile_of_uv, triangles_for_tile,
    FIRST_TILE,
};
pub use usd::load_usda;

/// Interleaved mesh data as imported, before any GPU upload.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Triangle list indices into the vertex attributes.
    pub indices: Vec<u32>,
    /// Materials present on the mesh (one texture set per material in the app).
    pub material_names: Vec<String>,
    /// Which material each triangle uses: one index into
    /// `material_names` per triangle (`triangle_count()` long).
    /// Empty means "one material" — every existing mesh and test
    /// literal predates the field, so `..Default::default()` keeps
    /// them valid and consumers treat empty as the whole mesh
    /// (per-set masked bakes, `docs/specs/per-set-masked-bakes.md`
    /// §1).
    pub triangle_materials: Vec<u16>,
}

impl MeshData {
    /// Number of vertices (length of the parallel attribute vectors).
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles (one third of the index count).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The triangles that use material `material` (an index into
    /// `material_names`), as a submesh for per-set bakes
    /// (`docs/specs/per-set-masked-bakes.md` §2): the FULL vertex
    /// arrays ride along, `indices` holds the kept triangles'
    /// original vertex ids — the exact shape
    /// [`filter_mesh_for_triangles`] already proven for per-tile AO.
    /// Material index out of range, or a mesh with no
    /// `triangle_materials`, yields an empty submesh — never a silent
    /// whole-mesh bake.
    pub fn submesh_for_material(&self, material: usize) -> MeshData {
        let empty = MeshData {
            positions: self.positions.clone(),
            normals: self.normals.clone(),
            uvs: self.uvs.clone(),
            indices: Vec::new(),
            material_names: self.material_names.clone(),
            triangle_materials: Vec::new(),
        };
        let Some(tris) = self.triangles_of_material(material) else {
            return empty;
        };
        filter_mesh_for_triangles(self, &tris)
    }

    /// The triangle ids (0-based, into `indices`) that use `material`.
    /// `None` when the mesh carries no triangle→material mapping.
    pub fn triangles_of_material(&self, material: usize) -> Option<Vec<u32>> {
        if self.triangle_materials.is_empty() {
            return None;
        }
        let indices: Vec<u32> = self
            .triangle_materials
            .iter()
            .enumerate()
            .filter(|&(_, &m)| usize::from(m) == material)
            .map(|(t, _)| u32::try_from(t).unwrap_or(u32::MAX))
            .collect();
        Some(indices)
    }

    /// Axis-aligned bounds of the vertex positions.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        if self.positions.is_empty() {
            return None;
        }
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for p in &self.positions {
            let p = Vec3::new(p[0], p[1], p[2]);
            min = min.min(p);
            max = max.max(p);
        }
        Some((min, max))
    }
}

/// Import error, uniform across formats.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("obj: {0}")]
    Obj(String),
    #[error("gltf: {0}")]
    Gltf(String),
    #[error("fbx: {0}")]
    Fbx(String),
    #[error("usd: {0}")]
    Usd(String),
    /// Binary crate files (`.usdc`, or a `.usd` holding crate bytes).
    #[error("usd: usdc (binary) arrives when the pure-Rust reader stabilizes")]
    UsdBinary,
    /// An importer panicked on this file and [`load`]'s boundary caught
    /// it: the file is malformed in a way a dependency does not handle.
    #[error("import failed on a malformed file ({0})")]
    Panicked(String),
}

/// Detect the import format from a file extension.
pub fn detect_format(path: &std::path::Path) -> Result<Format, ImportError> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("obj") => Ok(Format::Obj),
        Some("gltf" | "glb") => Ok(Format::Gltf),
        Some("fbx") => Ok(Format::Fbx),
        Some("usda" | "usd") => Ok(Format::Usd),
        Some("usdc") => Ok(Format::Usdc),
        Some("usdz") => Ok(Format::Usdz),
        Some(other) => Err(ImportError::Unsupported(other.to_string())),
        None => Err(ImportError::Unsupported("(no extension)".into())),
    }
}

/// Supported import formats (SPEC.md deliverable 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Obj,
    Gltf,
    Fbx,
    /// `.usda`, or `.usd` (sniffed: binary crate content errors).
    Usd,
    /// `.usdc` — the binary crate, via openusd (the deferral voided).
    Usdc,
    /// `.usdz` — the package format, its first layer.
    Usdz,
}

/// Load a mesh from disk, dispatching on file extension.
///
/// Supports `.obj` ([`load_obj`]), `.gltf`/`.glb` ([`load_gltf`]), `.fbx`
/// ([`load_fbx`]), `.usda`/`.usd` ([`load_usda`]), `.usdc`
/// ([`usdc::load_usdc`]) and `.usdz` ([`usdc::load_usdz`]);
/// anything else [`ImportError::Unsupported`].
///
/// # The panic boundary
///
/// The dispatch runs inside [`std::panic::catch_unwind`]: a panic in an
/// importer DEPENDENCY on a crafted file (the 2026-10-14 panic hunt pinned
/// gltf 1.4.1, tobj 4.0.5 and openusd 0.7.0 panics that umber-mesh cannot
/// pre-validate away) becomes [`ImportError::Panicked`] instead of taking
/// the app down on File → Open. It catches unwinding panics only: a stack
/// overflow or an allocation-failure ABORT still kills the process, which
/// is why the recursive parsers carry depth caps. Umber's OWN parse code
/// validates before it indexes; this boundary is for code it does not own.
pub fn load(path: &std::path::Path) -> Result<MeshData, ImportError> {
    let format = detect_format(path)?;
    let dispatch = || match format {
        Format::Obj => load_obj(path),
        Format::Gltf => load_gltf(path),
        Format::Fbx => load_fbx(path),
        Format::Usd => load_usda(path),
        Format::Usdc => crate::usdc::load_usdc(path),
        Format::Usdz => crate::usdc::load_usdz(path),
    };
    // AssertUnwindSafe: the closure borrows only `path` and `format`
    // (read-only), and a caught panic returns an error — no partially
    // built state escapes.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(dispatch)).unwrap_or_else(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        Err(ImportError::Panicked(format!(
            "{format:?} importer: {detail}"
        )))
    })
}

/// OBJ import via tobj (Wave 1 baseline; claw hardening in flight).
///
/// Missing normals/UVs are left as whatever tobj produced (possibly short of
/// `positions`); only the glTF/FBX loaders zero-fill. Returns
/// [`ImportError::Obj`] when the file cannot be parsed.
pub fn load_obj(path: &std::path::Path) -> Result<MeshData, ImportError> {
    let (models, materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            single_index: true,
            triangulate: true,
            ..Default::default()
        },
    )
    .map_err(|e| ImportError::Obj(e.to_string()))?;

    let material_names = match materials {
        Ok(mats) => mats.into_iter().map(|m| m.name).collect(),
        Err(_) => vec!["DefaultMaterial".to_string()],
    };

    let mut data = MeshData {
        material_names,
        ..Default::default()
    };
    // Per-set masked bakes §1: each OBJ model carries tobj's
    // `material_id` — one material per model's triangles. All-or-nothing
    // at file scope: any model without a material keeps the whole-mesh
    // fallback for the whole file.
    let mut pending_triangle_materials: Vec<u16> = Vec::new();
    let mut complete = true;
    for model in models {
        let mesh = model.mesh;
        let model_material = mesh
            .material_id
            .and_then(|id| u16::try_from(id).ok())
            .filter(|&id| usize::from(id) < data.material_names.len());
        // Validate attribute lengths before chunking: tobj does not guarantee
        // exact multiples for malformed files (review #1 — silent-drop risk).
        if mesh.positions.len() % 3 != 0
            || mesh.normals.len() % 3 != 0
            || mesh.texcoords.len() % 2 != 0
        {
            return Err(ImportError::Obj(format!(
                "malformed OBJ '{}': non-multiple attribute lengths (pos {}, nrm {}, uv {})",
                model.name,
                mesh.positions.len(),
                mesh.normals.len(),
                mesh.texcoords.len()
            )));
        }
        data.positions
            .extend(mesh.positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]));
        data.normals
            .extend(mesh.normals.chunks_exact(3).map(|c| [c[0], c[1], c[2]]));
        data.uvs
            .extend(mesh.texcoords.chunks_exact(2).map(|c| [c[0], c[1]]));
        data.indices.extend(mesh.indices.iter().copied());
        match model_material {
            Some(slot) => {
                let triangles = mesh.indices.len() / 3;
                pending_triangle_materials.extend(std::iter::repeat_n(slot, triangles));
            }
            None => complete = false,
        }
    }
    if complete && pending_triangle_materials.len() == data.triangle_count() {
        data.triangle_materials = pending_triangle_materials;
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_materials_empty_means_whole_mesh() {
        // §1's contract: a mesh without the map (every pre-field mesh)
        // has no triangles_of_material — the caller's whole-mesh
        // fallback — and an out-of-range material on a MAPPED mesh
        // yields an empty submesh, never a silent whole-mesh bake.
        let mesh = MeshData {
            positions: vec![[0.0; 3], [1.0; 3], [0.0; 3], [1.0; 3]],
            normals: vec![[0.0; 3]; 4],
            uvs: vec![[0.0; 2]; 4],
            indices: vec![0, 1, 2, 1, 3, 2],
            material_names: vec!["Body".into()],
            triangle_materials: Vec::new(),
        };
        assert!(mesh.triangles_of_material(0).is_none());
        let unmapped = mesh.submesh_for_material(0);
        assert!(unmapped.indices.is_empty());
        assert_eq!(unmapped.positions.len(), 4, "vertex arrays ride along");
    }

    #[test]
    fn submesh_for_material_keeps_the_sets_triangles() {
        // Two materials, two triangles: material 1 owns triangle 1.
        // The submesh keeps the FULL vertex arrays (the bake cores
        // resolve through `indices` — the subset-mesh kill's contract)
        // and only the kept triangle's original ids.
        let mesh = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            indices: vec![0, 1, 2, 1, 3, 2],
            material_names: vec!["Body".into(), "Hilt".into()],
            triangle_materials: vec![0, 1],
        };
        let body = mesh.submesh_for_material(0);
        assert_eq!(body.indices, vec![0, 1, 2]);
        assert_eq!(body.positions.len(), 4, "full vertex arrays ride along");
        assert_eq!(body.triangle_count(), 1);

        let hilt = mesh.submesh_for_material(1);
        assert_eq!(hilt.indices, vec![1, 3, 2]);

        // Out-of-range material: empty submesh, not a whole-mesh bake.
        assert!(mesh.submesh_for_material(2).indices.is_empty());
        // And the triangle census matches the map's length exactly.
        assert_eq!(
            mesh.triangles_of_material(1),
            Some(vec![1]),
            "the map is exactly triangle_count long"
        );
    }

    #[test]
    fn detect_format_routes_by_extension() {
        assert_eq!(
            detect_format(std::path::Path::new("a.OBJ")).unwrap(),
            Format::Obj
        );
        assert_eq!(
            detect_format(std::path::Path::new("a.glb")).unwrap(),
            Format::Gltf
        );
        assert_eq!(
            detect_format(std::path::Path::new("a.fbx")).unwrap(),
            Format::Fbx
        );
        assert_eq!(
            detect_format(std::path::Path::new("a.usda")).unwrap(),
            Format::Usd
        );
        assert_eq!(
            detect_format(std::path::Path::new("a.USD")).unwrap(),
            Format::Usd
        );
        assert!(matches!(
            detect_format(std::path::Path::new("a.usdc")),
            Ok(Format::Usdc)
        ));
        assert!(matches!(
            detect_format(std::path::Path::new("a.usdz")),
            Ok(Format::Usdz)
        ));
        assert!(detect_format(std::path::Path::new("a.stl")).is_err());
    }

    #[test]
    fn bounds_and_counts_are_consistent() {
        let data = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0]],
            normals: vec![[0.0; 3]; 2],
            uvs: vec![[0.0; 2]; 2],
            indices: vec![0, 1, 0],
            material_names: vec!["m".into()],
            triangle_materials: Vec::new(),
        };
        assert_eq!(data.vertex_count(), 2);
        assert_eq!(data.triangle_count(), 1);
        let (min, max) = data.bounds().unwrap();
        assert_eq!(min, Vec3::ZERO);
        assert_eq!(max, Vec3::new(1.0, 2.0, 3.0));
        assert!(MeshData::default().bounds().is_none());
    }
}
