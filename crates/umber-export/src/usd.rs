//! USD ASCII (`.usda`) mesh + material export — the import mirror of
//! `umber-mesh`'s hand parser (`usd.rs`'s doc block is the contract).
//!
//! [`to_usda_document`] serializes a triangle mesh plus a UsdShade
//! Material/Shader network to `.usda` text that `umber_mesh::load_usda`
//! parses back: `point3f[] points`, `int[] faceVertexCounts` /
//! `faceVertexIndices`, vertex-interpolated `primvars:normals` and
//! `primvars:st`, and `rel material:binding`. The export joins the driver
//! pipeline ([`crate::driver`]) the way the `.mtlx` side does: the CLI and
//! the Export dialog write the texture files first, then resolve the
//! actually-written names through [`usd_textures`] and append
//! [`write_usda`]'s `<textureSet>.usda` to the run. `ExportPreset::usda`
//! is the toggle, mirroring `ExportPreset::materialx`.
//!
//! # Judgment calls (requirements §6 P1 row)
//!
//! - The mesh is written triangulated (`faceVertexCounts` all 3s): the
//!   export source (`umber-mesh`'s `MeshData`) only carries triangles, so
//!   there are no n-gons to preserve. The importer's fan triangulation of
//!   a 3-vertex face is the identity — the triangulation round-trips
//!   exactly, and re-importing always yields the per-corner expansion
//!   (`3 × triangles` vertices).
//! - Right-handed only: no `orientation` attribute is emitted, so the
//!   importer keeps the winding as authored. `leftHanded` is never
//!   emitted.
//! - Normals and UVs are required, parallel to positions, and carried
//!   with `vertex` interpolation (the `usdc.rs` fixture's convention for
//!   `primvars:st`). A ragged mesh is an [`ExportError::Write`], not a
//!   silent zero-fill.
//! - The prim structure is `/Asset/{Mesh, Looks/Material}`: `def Xform`
//!   carrying `def Mesh "Mesh"` and `def Scope "Looks"` carrying
//!   `def Material "Material"`. The importer takes the first `def Mesh`
//!   in pre-order, so this nests exactly like its `Root/Quad`,
//!   `World/Tri` and `World/Mat` fixtures — the export imports back.
//! - The Shader network uses `info:id = "open_pbr_surface"` — the same
//!   node name [`umber_graph::mtlx_doc::to_mtlx_document`] emits — with
//!   the vendored-nodedef input names (`base_color`, `base_metalness`,
//!   `specular_roughness`, `geometry_normal`). Only textured inputs are
//!   emitted; untextured inputs stay at the shader's defaults (the
//!   `.mtlx` side carries the app's `SurfaceParams` constants instead).
//! - One `UsdUVTexture` reader per distinct file (first-use order,
//!   `Texture_0…`), so a packed file (ORM, metallicRoughness) is read
//!   once and fanned out through its `r`/`g`/`b`/`a` outputs — the same
//!   first-wins-per-input rule as `materialx_document`. The channel pick
//!   reuses the driver's own [`packed_channel`] analysis.
//! - A `DirectX`-convention normal is NOT wired (there is no green-flip
//!   node in the UsdShade core vocabulary) and AO/height have no OpenPBR
//!   input — both rules mirrored from `materialx_document`.
//! - Texture references are relative `@…@` asset paths (the `.usda`
//!   convention); tiled runs reference the `<UDIM>`-suffixed names, one
//!   per tile substitution naming a file the run wrote — the same rule
//!   as the tiled `.mtlx`. `@@@…@@@` delimiters when a name holds `@`.
//! - The `.usda` always carries the WHOLE mesh: per-tile geometry splits
//!   stay a bake-window concern; only the texture references are
//!   tile-aware.
//! - The asset segment of every prim path is sanitized to
//!   `[A-Za-z0-9_]` (`M_`-prefixed when empty or digit-led — the
//!   `element_name` rule from `mtlx_doc`), because `.` separates
//!   properties in Sdf paths and `/` would escape the prim.
//! - Non-finite floats sanitize to `0.0`: `NaN`/`inf` are not lexable
//!   `.usda` numbers.

use std::path::{Path, PathBuf};

use umber_graph::mtlx_doc::{MtlxColorSpace, TextureMap, UDIM_TOKEN};

use crate::driver::{is_passthrough, packed_channel, texture_map, ExportError, TokenSources};
use crate::presets::{ExportPreset, NormalConvention, OutputSpec};
use crate::tiled_template;

/// The mesh half of a `.usda` export: parallel attribute slices over
/// `u32` triangle indices — the `umber-mesh` `MeshData` fields, borrowed
/// so this crate stays mesh-free (callers map the four fields over).
#[derive(Debug, Clone, Copy)]
pub struct UsdMeshView<'a> {
    /// One `[x, y, z]` per vertex (`points` verbatim).
    pub positions: &'a [[f32; 3]],
    /// One per vertex, parallel to `positions` (`primvars:normals`).
    pub normals: &'a [[f32; 3]],
    /// One per vertex, parallel to `positions` (`primvars:st`).
    pub uvs: &'a [[f32; 2]],
    /// Triangle list (`faceVertexIndices` verbatim).
    pub indices: &'a [u32],
}

impl<'a> UsdMeshView<'a> {
    /// Number of triangles (`indices.len() / 3`).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// One textured OpenPBR input: which shader input it feeds, of which
/// type, from which texture-reader output channel.
#[derive(Debug, Clone, PartialEq)]
pub struct UsdTexture {
    /// The file string, relative to the `.usda` (may carry [`UDIM_TOKEN`]).
    pub file: String,
    /// The OpenPBR input name (`base_color`, `specular_roughness`, …).
    pub input: &'static str,
    /// The OpenPBR input type (`color3`, `float`, `vector3`).
    pub input_type: &'static str,
    /// The `UsdUVTexture` output channel (`rgb`, `r`, `g`, `b`, `a`).
    pub output: &'static str,
    /// The reader's `sourceColorSpace` (`sRGB` for color-managed maps,
    /// `raw` for data — the §9 rule).
    pub source_colorspace: &'static str,
}

/// The `.usda` side of a run: one [`UsdTexture`] per OpenPBR-wirable map
/// of every output, `file_name` resolving each output's file name
/// (relative to the `.usda`, where the document is written).
///
/// The walk mirrors `materialx_document` (`driver.rs`): passthrough
/// outputs reference the whole image, packed outputs (ORM,
/// metallicRoughness) reference one channel, first texture per OpenPBR
/// input wins, AO/height and the DirectX normal are skipped.
pub fn usd_textures(
    preset: &ExportPreset,
    file_name: impl Fn(&OutputSpec) -> String,
) -> Vec<UsdTexture> {
    let mut textures = Vec::new();
    for output in &preset.outputs {
        let file = file_name(output);
        for (index, (kind, _)) in output.maps.iter().enumerate() {
            let map = texture_map(*kind);
            let Some((input, ty)) = map.openpbr_input() else {
                continue; // AO / height: no OpenPBR input.
            };
            if textures.iter().any(|t: &UsdTexture| t.input == input) {
                continue;
            }
            if map == TextureMap::Normal && output.normal_convention != NormalConvention::Opengl {
                continue;
            }
            let output = if is_passthrough(output, index) {
                if ty == "float" {
                    "r"
                } else {
                    "rgb"
                }
            } else if ty == "float" {
                match packed_channel(output, index) {
                    Some(c) => channel_suffix(c),
                    None => continue, // wired in no channel.
                }
            } else {
                continue; // a color/vector map can't be one channel.
            };
            textures.push(UsdTexture {
                file: file.clone(),
                input,
                input_type: ty,
                output,
                source_colorspace: match map.default_colorspace() {
                    MtlxColorSpace::SrgbTexture => "sRGB",
                    MtlxColorSpace::LinRec709 => "raw",
                },
            });
        }
    }
    textures
}

/// The `UsdUVTexture` output channel carrying packed channel `index`.
fn channel_suffix(index: u8) -> &'static str {
    match index {
        0 => "r",
        1 => "g",
        2 => "b",
        _ => "a",
    }
}

/// The file string one preset output is referenced by in the `.usda`:
/// the output's expanded name, or — when `tile_names` (more than one
/// tile, or a lone tile other than 1001) — its [`tiled_template`] form
/// with the driver's `$udim` expanded to [`UDIM_TOKEN`], the same rule
/// the tiled `.mtlx` uses.
pub fn usda_file_for_output(
    output: &OutputSpec,
    sources: &TokenSources<'_>,
    tile_names: bool,
) -> String {
    if tile_names {
        let template = tiled_template(&output.filename);
        TokenSources {
            udim: UDIM_TOKEN,
            ..*sources
        }
        .expand_output_from(output, &template)
    } else {
        sources.expand_output(output)
    }
}

/// A valid Sdf prim-path segment from a texture-set name: anything
/// outside `[A-Za-z0-9_]` becomes `_`; an empty or digit-led name gets an
/// `M_` prefix (the `element_name` rule from `mtlx_doc`).
fn prim_segment(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "M_");
    }
    s
}

/// One lexable number: non-finite sanitizes to `0.0` (`NaN`/`inf` are not
/// `.usda` numbers); Rust's `Display` never emits exponents, which keeps
/// every value inside the lexer's word bytes.
fn fmt_num(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    v.to_string()
}

fn fmt_triple(v: [f32; 3]) -> String {
    format!("({}, {}, {})", fmt_num(v[0]), fmt_num(v[1]), fmt_num(v[2]))
}

fn fmt_pair(v: [f32; 2]) -> String {
    format!("({}, {})", fmt_num(v[0]), fmt_num(v[1]))
}

/// An `@…@` asset path for a relative texture reference (`@@@…@@@` when
/// the name itself holds `@`; a newline can never lex, so it is an
/// error at the [`to_usda_document`] boundary instead).
fn asset_path(file: &str) -> String {
    if file.contains('@') {
        format!("@@@{file}@@@")
    } else {
        format!("@{file}@")
    }
}

/// Serialize the `.usda` document for `asset`'s mesh + textures: the
/// `/Asset/{Mesh, Looks/Material}` prim structure with the UsdShade
/// network binding the texture files to the OpenPBR surface.
///
/// # Errors
///
/// [`ExportError::Write`] when the mesh is ragged (no triangles, a
/// non-multiple index count, an out-of-range or unrepresentable index,
/// attribute vectors not parallel to positions) or a texture file string
/// holds a newline.
pub fn to_usda_document(
    asset: &str,
    mesh: &UsdMeshView<'_>,
    textures: &[UsdTexture],
) -> Result<String, ExportError> {
    let write = |message: String| ExportError::Write(format!("usda: {message}"));
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
    for &i in mesh.indices {
        if i as u64 >= mesh.positions.len() as u64 {
            return Err(write(format!(
                "index {i} is out of range for {} positions",
                mesh.positions.len()
            )));
        }
    }
    for texture in textures {
        if texture.file.contains('\n') {
            return Err(write(format!(
                "texture file {:?} holds a newline",
                texture.file
            )));
        }
    }

    let prim = prim_segment(asset);
    let looks_path = format!("/{prim}/Looks");
    let material_path = format!("{looks_path}/Material");

    let mut out = String::new();
    out.push_str("#usda 1.0\n");
    out.push_str("(\n");
    out.push_str(&format!("    defaultPrim = \"{prim}\"\n"));
    out.push_str("    upAxis = \"Y\"\n");
    out.push_str(")\n");
    out.push('\n');
    out.push_str(&format!("def Xform \"{prim}\"\n"));
    out.push_str("{\n");
    out.push_str("    def Mesh \"Mesh\"\n");
    out.push_str("    {\n");
    out.push_str("        int[] faceVertexCounts = [");
    for i in 0..triangles {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('3');
    }
    out.push_str("]\n");
    out.push_str("        int[] faceVertexIndices = [");
    for (i, index) in mesh.indices.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&index.to_string());
    }
    out.push_str("]\n");
    out.push_str("        point3f[] points = [");
    for (i, position) in mesh.positions.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&fmt_triple(*position));
    }
    out.push_str("]\n");
    out.push_str("        normal3f[] primvars:normals = [");
    for (i, normal) in mesh.normals.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&fmt_triple(*normal));
    }
    out.push_str("] (\n");
    out.push_str("            interpolation = \"vertex\"\n");
    out.push_str("        )\n");
    out.push_str("        texCoord2f[] primvars:st = [");
    for (i, uv) in mesh.uvs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&fmt_pair(*uv));
    }
    out.push_str("] (\n");
    out.push_str("            interpolation = \"vertex\"\n");
    out.push_str("        )\n");
    out.push_str("        uniform token subdivisionScheme = \"none\"\n");
    out.push_str(&format!(
        "        rel material:binding = <{material_path}>\n"
    ));
    out.push_str("    }\n");
    out.push('\n');
    out.push_str("    def Scope \"Looks\"\n");
    out.push_str("    {\n");
    out.push_str("        def Material \"Material\"\n");
    out.push_str("        {\n");
    out.push_str(&format!(
        "            token outputs:surface.connect = <{material_path}/Shader.outputs:surface>\n"
    ));
    out.push('\n');
    out.push_str("            def Shader \"Shader\"\n");
    out.push_str("            {\n");
    out.push_str("                uniform token info:id = \"open_pbr_surface\"\n");
    for texture in textures {
        let reader = reader_name(textures, texture);
        let value_type = match texture.input_type {
            "float" => "float",
            "vector3" => "vector3",
            _ => "color3f",
        };
        out.push_str(&format!(
            "                {value_type} inputs:{}.connect = <{material_path}/{reader}.outputs:{}>\n",
            texture.input, texture.output
        ));
    }
    out.push_str("            }\n");
    // One reader per distinct file, in first-use order (deterministic).
    let mut readers: Vec<&str> = Vec::new();
    for texture in textures {
        if !readers.contains(&texture.file.as_str()) {
            readers.push(texture.file.as_str());
        }
    }
    for (n, file) in readers.iter().enumerate() {
        let file: &str = file;
        let colorspace = textures
            .iter()
            .find(|t| t.file.as_str() == file)
            .map(|t| t.source_colorspace)
            .unwrap_or("raw");
        out.push('\n');
        out.push_str(&format!("            def Shader \"Texture_{n}\"\n"));
        out.push_str("            {\n");
        out.push_str("                uniform token info:id = \"UsdUVTexture\"\n");
        out.push_str(&format!(
            "                asset inputs:file = {}\n",
            asset_path(file)
        ));
        out.push_str(&format!(
            "                token inputs:sourceColorSpace = \"{colorspace}\"\n"
        ));
        if !textures.is_empty() {
            out.push_str(&format!(
                "                float2 inputs:st.connect = <{material_path}/PrimvarSt.outputs:result>\n"
            ));
        }
        out.push_str("            }\n");
    }
    if !textures.is_empty() {
        out.push('\n');
        out.push_str("            def Shader \"PrimvarSt\"\n");
        out.push_str("            {\n");
        out.push_str("                uniform token info:id = \"UsdPrimvarReader_float2\"\n");
        out.push_str("                token inputs:varname = \"st\"\n");
        out.push_str("            }\n");
    }
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    Ok(out)
}

/// The reader a texture's connection points at: `Texture_{n}` for the
/// index of its file in first-use order (matches [`to_usda_document`]).
fn reader_name<'a>(textures: &'a [UsdTexture], texture: &UsdTexture) -> String {
    let mut readers: Vec<&'a str> = Vec::new();
    for t in textures {
        if !readers.contains(&t.file.as_str()) {
            readers.push(t.file.as_str());
        }
    }
    let n = readers
        .iter()
        .position(|file| *file == texture.file.as_str())
        .unwrap_or(0);
    format!("Texture_{n}")
}

/// Writes `<asset>.usda` under `out_dir`, returning its path. The
/// document references the texture files by relative path; the caller
/// passes the names the run actually wrote (see [`usd_textures`] +
/// [`usda_file_for_output`]).
pub fn write_usda(
    out_dir: &Path,
    asset: &str,
    mesh: &UsdMeshView<'_>,
    textures: &[UsdTexture],
) -> Result<PathBuf, ExportError> {
    std::fs::create_dir_all(out_dir).map_err(|e| ExportError::Write(e.to_string()))?;
    let doc = to_usda_document(asset, mesh, textures)?;
    let path = out_dir.join(format!("{asset}.usda"));
    std::fs::write(&path, doc).map_err(|e| ExportError::Write(e.to_string()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{TokenSources, SINGLE_TILE_UDIM};
    use crate::presets::ExportPreset;

    /// A unit quad in z=0: 4 corners, 2 triangles, +Z normals, unit UVs.
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

    fn quad_view<'a>(
        positions: &'a [[f32; 3]],
        normals: &'a [[f32; 3]],
        uvs: &'a [[f32; 2]],
        indices: &'a [u32],
    ) -> UsdMeshView<'a> {
        UsdMeshView {
            positions,
            normals,
            uvs,
            indices,
        }
    }

    fn gltf_textures() -> Vec<UsdTexture> {
        let preset = ExportPreset::gltf_metal_rough();
        usd_textures(&preset, |output| {
            TokenSources::new("Sword").expand_output(output)
        })
    }

    #[test]
    fn prim_structure_binds_the_mesh_to_the_material() {
        let (positions, normals, uvs, indices) = quad();
        let doc = to_usda_document(
            "Sword",
            &quad_view(&positions, &normals, &uvs, &indices),
            &gltf_textures(),
        )
        .unwrap();
        assert!(
            doc.starts_with("#usda 1.0\n"),
            "the header the parser requires"
        );
        assert!(doc.contains("defaultPrim = \"Sword\""));
        assert!(doc.contains("def Xform \"Sword\""));
        assert!(doc.contains("def Mesh \"Mesh\""));
        assert!(doc.contains("def Scope \"Looks\""));
        assert!(doc.contains("def Material \"Material\""));
        assert!(doc.contains("def Shader \"Shader\""));
        assert!(
            doc.contains("rel material:binding = </Sword/Looks/Material>"),
            "the mesh binds the material"
        );
        assert!(
            doc.contains(
                "token outputs:surface.connect = </Sword/Looks/Material/Shader.outputs:surface>"
            ),
            "the material drives the surface"
        );
        assert!(doc.contains("uniform token info:id = \"open_pbr_surface\""));
    }

    #[test]
    fn texture_references_are_relative_asset_paths() {
        let (positions, normals, uvs, indices) = quad();
        let doc = to_usda_document(
            "Sword",
            &quad_view(&positions, &normals, &uvs, &indices),
            &gltf_textures(),
        )
        .unwrap();
        assert!(doc.contains("asset inputs:file = @Sword_baseColor.png@"));
        assert!(doc.contains("asset inputs:file = @Sword_metallicRoughness.png@"));
        assert!(doc.contains("asset inputs:file = @Sword_normal.png@"));
        for line in doc.lines().filter(|l| l.contains("inputs:file")) {
            let path = line.split('@').nth(1).expect("asset path delimiters");
            assert!(
                !path.starts_with('/'),
                "references stay relative, got {path:?}"
            );
            assert!(!path.contains("://"), "no URI scheme, got {path:?}");
        }
    }

    #[test]
    fn packed_outputs_share_one_reader_through_channel_outputs() {
        // glTF: baseColor passthrough (rgb) + metallicRoughness packed
        // (G = metal, B = rough) + normal passthrough (rgb).
        let textures = gltf_textures();
        assert_eq!(textures.len(), 4);
        let input = |name: &str| {
            textures
                .iter()
                .find(|t| t.input == name)
                .unwrap_or_else(|| panic!("no {name} input"))
        };
        assert_eq!(
            (
                input("base_color").output,
                input("base_color").file.as_str()
            ),
            ("rgb", "Sword_baseColor.png")
        );
        assert_eq!(
            (
                input("base_metalness").output,
                input("base_metalness").file.as_str()
            ),
            ("g", "Sword_metallicRoughness.png")
        );
        assert_eq!(
            (
                input("specular_roughness").output,
                input("specular_roughness").file.as_str()
            ),
            ("b", "Sword_metallicRoughness.png")
        );
        assert_eq!(input("geometry_normal").output, "rgb");
        let (positions, normals, uvs, indices) = quad();
        let doc = to_usda_document(
            "Sword",
            &quad_view(&positions, &normals, &uvs, &indices),
            &textures,
        )
        .unwrap();
        // One reader for the shared packed file, fanned out by channel.
        assert_eq!(doc.matches("def Shader \"Texture_").count(), 3);
        assert!(doc.contains(
            "inputs:base_metalness.connect = </Sword/Looks/Material/Texture_1.outputs:g>"
        ));
        assert!(doc.contains(
            "inputs:specular_roughness.connect = </Sword/Looks/Material/Texture_1.outputs:b>"
        ));
        assert!(
            doc.contains("token inputs:sourceColorSpace = \"sRGB\""),
            "base color is color-managed"
        );
        assert!(
            doc.contains("token inputs:sourceColorSpace = \"raw\""),
            "data maps are raw"
        );
    }

    #[test]
    fn unreal_skips_ao_and_the_directx_normal() {
        // Unreal: AO has no OpenPBR input; ORM's G/B carry rough/metal;
        // the DirectX normal is not wired (no green-flip node in UsdShade).
        let preset = ExportPreset::unreal_orm();
        let textures = usd_textures(&preset, |output| {
            TokenSources::new("Hull").expand_output(output)
        });
        let inputs: Vec<&str> = textures.iter().map(|t| t.input).collect();
        assert!(inputs.contains(&"base_color"));
        assert!(inputs.contains(&"base_metalness"));
        assert!(inputs.contains(&"specular_roughness"));
        assert!(
            !inputs.contains(&"geometry_normal"),
            "DirectX normal must not wire: {inputs:?}"
        );
        assert!(
            textures
                .iter()
                .all(|t| !t.file.contains("ORM") || t.input != "occlusion"),
            "AO has no OpenPBR input"
        );
    }

    #[test]
    fn tiled_runs_reference_the_udim_token() {
        let preset = ExportPreset::gltf_metal_rough();
        let sources = TokenSources {
            texture_set: "Sword",
            mesh: "",
            layer_name: "",
            udim: SINGLE_TILE_UDIM,
        };
        let textures = usd_textures(&preset, |output| {
            usda_file_for_output(output, &sources, true)
        });
        assert!(
            textures.iter().all(|t| t.file.contains(UDIM_TOKEN)),
            "every reference is tile-aware: {textures:?}"
        );
        let (positions, normals, uvs, indices) = quad();
        let doc = to_usda_document(
            "Sword",
            &quad_view(&positions, &normals, &uvs, &indices),
            &textures,
        )
        .unwrap();
        assert!(doc.contains("@Sword_baseColor_<UDIM>.png@"));
    }

    #[test]
    fn asset_names_sanitize_to_prim_path_segments() {
        let (positions, normals, uvs, indices) = quad();
        let doc = to_usda_document(
            "my set.v2",
            &quad_view(&positions, &normals, &uvs, &indices),
            &[],
        )
        .unwrap();
        assert!(doc.contains("def Xform \"my_set_v2\""));
        assert!(doc.contains("rel material:binding = </my_set_v2/Looks/Material>"));
        assert!(
            !doc.contains("my set.v2"),
            "no raw segment leaks into paths"
        );
    }

    #[test]
    fn ragged_meshes_error_instead_of_writing() {
        let (positions, normals, uvs, _) = quad();
        let empty: Vec<u32> = Vec::new();
        // Empty index buffer = no triangles.
        assert!(matches!(
            to_usda_document("S", &quad_view(&positions, &normals, &uvs, &empty), &[]),
            Err(ExportError::Write(_))
        ));
        // A 2-index buffer is not a multiple of 3.
        assert!(matches!(
            to_usda_document("S", &quad_view(&positions, &normals, &uvs, &[0, 1]), &[]),
            Err(ExportError::Write(_))
        ));
        // Index 99 is out of range for the quad.
        assert!(matches!(
            to_usda_document(
                "S",
                &quad_view(&positions, &normals, &uvs, &[0, 1, 99]),
                &[]
            ),
            Err(ExportError::Write(_))
        ));
        let short_normals = vec![[0.0, 0.0, 1.0]; 2];
        assert!(matches!(
            to_usda_document(
                "S",
                &quad_view(&positions, &short_normals, &uvs, &[0, 1, 2]),
                &[]
            ),
            // Short normals violate the parallel-attributes contract.
            Err(ExportError::Write(_))
        ));
        let bad_file = vec![UsdTexture {
            file: "a\nb.png".into(),
            input: "base_color",
            input_type: "color3",
            output: "rgb",
            source_colorspace: "sRGB",
        }];
        assert!(matches!(
            to_usda_document(
                "S",
                &quad_view(&positions, &normals, &uvs, &[0, 1, 2]),
                &bad_file
            ),
            // A newline can never lex as an asset path.
            Err(ExportError::Write(_))
        ));
    }

    #[test]
    fn export_imports_back_with_the_same_triangles_and_binding() {
        // The round-trip: the exported `.usda` parses with the real
        // importer — same triangle count, per-corner expansion, first
        // triangle's attributes intact, and the binding still in the text
        // (the importer skips material prims by design, so the binding is
        // asserted textually, like the RICH fixture's skipped Material).
        let (positions, normals, uvs, indices) = quad();
        let doc = to_usda_document(
            "Sword",
            &quad_view(&positions, &normals, &uvs, &indices),
            &gltf_textures(),
        )
        .unwrap();
        assert!(doc.contains("rel material:binding = </Sword/Looks/Material>"));
        assert!(doc.contains("uniform token info:id = \"open_pbr_surface\""));
        let dir = std::env::temp_dir().join(format!("umber-export-usd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_usda(
            &dir,
            "Sword",
            &quad_view(&positions, &normals, &uvs, &indices),
            &gltf_textures(),
        )
        .unwrap();
        assert!(path.ends_with("Sword.usda"));
        let back = umber_mesh::load_usda(&path).expect("exported .usda must parse");
        assert_eq!(back.triangle_count(), 2, "same mesh counts");
        assert_eq!(
            back.vertex_count(),
            6,
            "the importer's per-corner expansion"
        );
        assert_eq!(
            back.positions,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ]
        );
        assert_eq!(
            back.uvs,
            vec![
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.0, 0.0],
                [1.0, 1.0],
                [0.0, 1.0],
            ]
        );
        assert_eq!(back.normals, vec![[0.0, 0.0, 1.0]; 6]);
        assert_eq!(back.indices, vec![0, 1, 2, 3, 4, 5]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
