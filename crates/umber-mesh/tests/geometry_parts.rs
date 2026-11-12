//! Geometry masks (R-B2) K2: per-part triangle lists + mesh names per
//! format loader, and the per-tile coverage plane.
//!
//! Design brief: `docs/loop/briefs/2026-11-12-geomask-design.md`
//! (verification plan, K2). Every fixture is a tiny synthetic mesh
//! written to a unique temp file at test time; the per-part triangle
//! counts are pinned exactly.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use umber_mesh::{
    geometry_coverage, load, load_with_parts, MeshData, MeshParts, MeshPartsBuilder,
    MAX_PART_NAME_BYTES,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temp file holding `contents`, removed on drop.
struct Fixture(PathBuf);

impl Fixture {
    fn new(file_name: &str, contents: &[u8]) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("umber-geomask-parts-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let path = dir.join(file_name);
        std::fs::write(&path, contents).expect("fixture write");
        Fixture(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn names(parts: &MeshParts) -> Vec<&str> {
    parts.names.iter().map(String::as_str).collect()
}

/// Loads `fixture` both ways and pins `load` == `load_with_parts().0`.
fn load_both(fixture: &Fixture) -> (MeshData, MeshParts) {
    let plain = load(&fixture.0).expect("load");
    let (data, parts) = load_with_parts(&fixture.0).expect("load_with_parts");
    assert_eq!(plain, data, "load() must equal load_with_parts().0");
    assert!(
        parts.triangle_parts.is_empty() || parts.triangle_parts.len() == data.triangle_count(),
        "all-or-nothing: the map is empty or triangle_count long"
    );
    (data, parts)
}

// ------------------------------------------------------------------ OBJ

/// Two objects: `Body` is a quad (2 triangles), `Hilt` a triangle.
const TWO_OBJECT_OBJ: &str = "\
o Body
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
vt 0 0
vt 1 0
vt 1 1
vt 0 1
f 1/1 2/2 3/3 4/4
o Hilt
v 0 0 1
v 1 0 1
v 0 1 1
vt 0 0
vt 1 0
vt 0 1
f 5/5 6/6 7/7
";

#[test]
fn obj_objects_become_parts() {
    let fx = Fixture::new("two.obj", TWO_OBJECT_OBJ.as_bytes());
    let (data, parts) = load_both(&fx);
    assert_eq!(data.triangle_count(), 3);
    assert_eq!(names(&parts), ["Body", "Hilt"]);
    assert_eq!(parts.triangle_parts, [0, 0, 1]);
    assert_eq!(parts.triangle_counts(), [2, 1]);
    assert_eq!(parts.triangles_of_part("Hilt"), Some(vec![2]));
    assert_eq!(parts.triangles_of_part("Nope"), None);
}

#[test]
fn same_name_objects_fold_into_one_part() {
    let obj = format!("{TWO_OBJECT_OBJ}o Body\nv 2 0 0\nv 3 0 0\nv 2 1 0\nf 8 9 10\n");
    let fx = Fixture::new("fold.obj", obj.as_bytes());
    let (data, parts) = load_both(&fx);
    assert_eq!(data.triangle_count(), 4);
    assert_eq!(names(&parts), ["Body", "Hilt"]);
    assert_eq!(parts.triangle_parts, [0, 0, 1, 0]);
    assert_eq!(parts.triangle_counts(), [3, 1]);
}

#[test]
fn unnamed_part_gets_ordinal_name() {
    // Geometry before any `o` line: tobj calls it "unnamed_object", the
    // builder names it `mesh 1`; a later named object keeps its name.
    let obj = "\
v 0 0 0
v 1 0 0
v 0 1 0
f 1 2 3
o Named
v 0 0 1
v 1 0 1
v 0 1 1
f 4 5 6
";
    let fx = Fixture::new("unnamed.obj", obj.as_bytes());
    let (_, parts) = load_both(&fx);
    assert_eq!(names(&parts), ["mesh 1", "Named"]);
    assert_eq!(parts.triangle_parts, [0, 1]);

    // The builder rule itself: ordinals count unnamed parts only, in
    // first-seen order, and blank names count as unnamed.
    let mesh = MeshData {
        indices: vec![0; 3 * 4],
        ..Default::default()
    };
    let mut b = MeshPartsBuilder::new();
    b.push(None, 1);
    b.push(Some("A"), 1);
    b.push(Some("   "), 1);
    b.push(Some(""), 1);
    let parts = b.finish(&mesh);
    assert_eq!(names(&parts), ["mesh 1", "A", "mesh 2", "mesh 3"]);
    assert_eq!(parts.triangle_parts, [0, 1, 2, 3]);
}

#[test]
fn missing_part_leaves_parts_empty() {
    // All-or-nothing: one triangle without a part (marked incomplete, or
    // a count that does not match the mesh) leaves the whole map empty.
    let mesh = MeshData {
        indices: vec![0; 3 * 3],
        ..Default::default()
    };
    let mut b = MeshPartsBuilder::new();
    b.push(Some("A"), 2);
    b.mark_incomplete();
    b.push(Some("B"), 1);
    assert_eq!(b.finish(&mesh), MeshParts::default());

    let mut short = MeshPartsBuilder::new();
    short.push(Some("A"), 2);
    let parts = short.finish(&mesh);
    assert!(parts.names.is_empty() && parts.triangle_parts.is_empty());

    // And an empty map makes every mesh exclusion inert.
    let excluded: BTreeSet<String> = ["A".to_string()].into();
    assert_eq!(
        geometry_coverage(&mesh, &parts, &excluded, 1001, 4, 4),
        None
    );
}

#[test]
fn long_part_names_truncate_at_a_char_boundary() {
    let long = "é".repeat(MAX_PART_NAME_BYTES); // 2 bytes per char
    let mesh = MeshData {
        indices: vec![0; 3],
        ..Default::default()
    };
    let mut b = MeshPartsBuilder::new();
    b.push(Some(&long), 1);
    let parts = b.finish(&mesh);
    assert_eq!(parts.names[0].len(), MAX_PART_NAME_BYTES);
    assert!(parts.names[0].chars().all(|c| c == 'é'));
}

// ----------------------------------------------------------------- glTF

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut block = 0u32;
        for (i, b) in chunk.iter().enumerate() {
            block |= u32::from(*b) << (16 - 8 * i);
        }
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(A[(block >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// glTF with three meshes: mesh 0 named "Blade" (a quad, 2 triangles),
/// mesh 1 unnamed but instanced by node "Guard" (1 triangle), mesh 2
/// unnamed on an unnamed node (1 triangle).
fn three_mesh_gltf() -> String {
    let mut buf = Vec::new();
    // quad positions (4), tri positions (3)
    for p in [
        [0f32, 0., 0.],
        [1., 0., 0.],
        [1., 1., 0.],
        [0., 1., 0.],
        [0., 0., 1.],
        [1., 0., 1.],
        [0., 1., 1.],
    ] {
        for c in p {
            buf.extend_from_slice(&c.to_le_bytes());
        }
    }
    // 84 bytes so far; quad indices (6 u16) then tri indices (3 u16 + pad)
    for i in [0u16, 1, 2, 0, 2, 3, 0, 1, 2, 0] {
        buf.extend_from_slice(&i.to_le_bytes());
    }
    let len = buf.len(); // 104
    let uri = base64(&buf);
    format!(
        r#"{{
  "asset": {{"version": "2.0"}},
  "scene": 0,
  "scenes": [{{"nodes": [0, 1, 2]}}],
  "nodes": [{{"mesh": 0, "name": "BladeNode"}}, {{"mesh": 1, "name": "Guard"}}, {{"mesh": 2}}],
  "meshes": [
    {{"name": "Blade", "primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 2}}]}},
    {{"primitives": [{{"attributes": {{"POSITION": 1}}, "indices": 3}}]}},
    {{"primitives": [{{"attributes": {{"POSITION": 1}}, "indices": 3}}]}}
  ],
  "buffers": [{{"byteLength": {len}, "uri": "data:application/octet-stream;base64,{uri}"}}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 48}},
    {{"buffer": 0, "byteOffset": 48, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 84, "byteLength": 12}},
    {{"buffer": 0, "byteOffset": 96, "byteLength": 6}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
      "min": [0, 0, 0], "max": [1, 1, 0]}},
    {{"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3",
      "min": [0, 0, 1], "max": [1, 1, 1]}},
    {{"bufferView": 2, "componentType": 5123, "count": 6, "type": "SCALAR"}},
    {{"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"}}
  ]
}}"#
    )
}

#[test]
fn gltf_meshes_become_parts() {
    let fx = Fixture::new("three.gltf", three_mesh_gltf().as_bytes());
    let (data, parts) = load_both(&fx);
    assert_eq!(data.triangle_count(), 4);
    // Mesh name wins over the node name; an unnamed mesh takes its
    // node's name; unnamed on both sides gets the ordinal.
    assert_eq!(names(&parts), ["Blade", "Guard", "mesh 1"]);
    assert_eq!(parts.triangle_parts, [0, 0, 1, 2]);
    assert_eq!(parts.triangle_counts(), [2, 1, 1]);
}

// ------------------------------------------------------------------ FBX

/// ASCII FBX 7.4 with two Model nodes, each owning its own Geometry:
/// "Body" a quad (2 triangles), "Hilt" a triangle (1). Same header
/// shape as `scripts/gen-fbx-uv-cube.py`'s fixture.
fn two_node_fbx() -> String {
    let geometry = |id: u32, name: &str, verts: &str, pvi: &str, nv: usize, ni: usize| {
        format!(
            "\tGeometry: {id}, \"Geometry::{name}\", \"Mesh\" {{\n\
             \t\tVertices: *{nv} {{\n\t\t\ta: {verts}\n\t\t}}\n\
             \t\tPolygonVertexIndex: *{ni} {{\n\t\t\ta: {pvi}\n\t\t}}\n\
             \t\tGeometryVersion: 124\n\t}}\n"
        )
    };
    let model = |id: u32, name: &str| {
        format!(
            "\tModel: {id}, \"Model::{name}\", \"Mesh\" {{\n\t\tVersion: 232\n\
             \t\tProperties70:  {{\n\t\t}}\n\t\tShading: T\n\t\tCulling: \"CullingOff\"\n\t}}\n"
        )
    };
    format!(
        "; FBX 7.4.0 project file\n\
FBXHeaderExtension:  {{\n\tFBXHeaderVersion: 1003\n\tFBXVersion: 7400\n}}\n\
GlobalSettings:  {{\n\tVersion: 1000\n}}\n\
Definitions:  {{\n\tVersion: 100\n\tCount: 4\n\
\tObjectType: \"Geometry\" {{\n\t\tCount: 2\n\t}}\n\
\tObjectType: \"Model\" {{\n\t\tCount: 2\n\t}}\n}}\n\
Objects:  {{\n{}{}{}{}}}\n\
Connections:  {{\n\tC: \"OO\",2000,0\n\tC: \"OO\",1000,2000\n\
\tC: \"OO\",2001,0\n\tC: \"OO\",1001,2001\n}}\n",
        geometry(
            1000,
            "BodyGeo",
            "0.0,0.0,0.0,1.0,0.0,0.0,1.0,1.0,0.0,0.0,1.0,0.0",
            "0,1,2,-4",
            12,
            4
        ),
        geometry(
            1001,
            "HiltGeo",
            "0.0,0.0,1.0,1.0,0.0,1.0,0.0,1.0,1.0",
            "0,1,-3",
            9,
            3
        ),
        model(2000, "Body"),
        model(2001, "Hilt"),
    )
}

#[test]
fn fbx_nodes_become_parts() {
    let fx = Fixture::new("two.fbx", two_node_fbx().as_bytes());
    let (data, parts) = load_both(&fx);
    assert_eq!(data.triangle_count(), 3);
    assert_eq!(names(&parts), ["Body", "Hilt"]);
    assert_eq!(parts.triangle_parts, [0, 0, 1]);
    assert_eq!(parts.triangle_counts(), [2, 1]);
}

#[test]
fn fbx_committed_cube_is_one_part_named_by_its_node() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("assets")
        .join("cube_2mat_ascii7400.fbx");
    let (data, parts) = load_with_parts(&path).expect("cube loads");
    assert_eq!(load(&path).expect("cube loads"), data);
    assert_eq!(names(&parts), ["Cube2Mat"]);
    assert_eq!(parts.triangle_parts, vec![0u16; 12]);
}

// ------------------------------------------------------------------ USD

/// The loaders import the FIRST Mesh prim; nested in an Xform, its prim
/// path is `/Root/Sword`, so the part is `Sword`.
const NESTED_USDA: &str = r#"#usda 1.0
def Xform "Root"
{
    def Mesh "Sword"
    {
        float3[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0), (2, 0, 0)]
        int[] faceVertexCounts = [4, 3]
        int[] faceVertexIndices = [0, 1, 2, 3, 1, 4, 2]
    }
}
"#;

#[test]
fn usd_prims_become_parts() {
    let fx = Fixture::new("nested.usda", NESTED_USDA.as_bytes());
    let (data, parts) = load_both(&fx);
    // quad fans to 2 + triangle 1.
    assert_eq!(data.triangle_count(), 3);
    assert_eq!(names(&parts), ["Sword"]);
    assert_eq!(parts.triangle_parts, [0, 0, 0]);
}

#[test]
fn usdc_prims_become_parts() {
    let layer = openusd::usda::parse(NESTED_USDA).expect("fixture parses");
    let mut out = std::io::Cursor::new(Vec::<u8>::new());
    openusd::usdc::CrateWriter::write(&layer, &mut out).expect("crate writes");
    let fx = Fixture::new("nested.usdc", &out.into_inner());
    let (data, parts) = load_both(&fx);
    assert_eq!(data.triangle_count(), 3);
    assert_eq!(names(&parts), ["Sword"]);
    assert_eq!(parts.triangle_parts, [0, 0, 0]);
}

#[test]
fn load_matches_load_with_parts_mesh() {
    // Byte-equal MeshData on every format's fixture (load_both asserts
    // it), plus the committed assets.
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("assets");
    for name in [
        "cube_b7400.fbx",
        "cube_uv_ascii7400.fbx",
        "cube_2mat_ascii7400.fbx",
        "usdc_tri_openusd_0_7_0.usdc",
    ] {
        let path = assets.join(name);
        let plain = load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (data, parts) = load_with_parts(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(plain, data, "{name}");
        assert_eq!(parts.triangle_parts.len(), data.triangle_count(), "{name}");
        assert_eq!(parts.names.len(), 1, "{name}: single-object asset");
    }
    let fixtures = [
        Fixture::new("a.obj", TWO_OBJECT_OBJ.as_bytes()),
        Fixture::new("a.gltf", three_mesh_gltf().as_bytes()),
        Fixture::new("a.fbx", two_node_fbx().as_bytes()),
        Fixture::new("a.usda", NESTED_USDA.as_bytes()),
    ];
    for fx in &fixtures {
        load_both(fx);
    }
}

// ------------------------------------------------------------- coverage

/// Two UV-disjoint quads in tile 1001: part "Left" covers u in
/// [0, 0.5], part "Right" covers u in [0.5, 1]; both span v in [0, 1].
fn left_right() -> (MeshData, MeshParts) {
    let quad = |u0: f32, u1: f32| [[u0, 0.0], [u1, 0.0], [u1, 1.0], [u0, 1.0]];
    let mut uvs = Vec::new();
    uvs.extend(quad(0.0, 0.5));
    uvs.extend(quad(0.5, 1.0));
    let positions = uvs.iter().map(|uv| [uv[0], uv[1], 0.0]).collect();
    let mesh = MeshData {
        positions,
        normals: vec![[0.0, 0.0, 1.0]; 8],
        uvs,
        indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
        ..Default::default()
    };
    let parts = MeshParts {
        names: vec!["Left".into(), "Right".into()],
        triangle_parts: vec![0, 0, 1, 1],
    };
    (mesh, parts)
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn geometry_coverage_none_when_nothing_excluded() {
    let (mesh, parts) = left_right();
    assert_eq!(
        geometry_coverage(&mesh, &parts, &set(&[]), 1001, 8, 8),
        None
    );
    // An exclusion naming no part of this mesh is inert data.
    assert_eq!(
        geometry_coverage(&mesh, &parts, &set(&["Gone"]), 1001, 8, 8),
        None
    );
    // The excluded part has no footprint on another tile.
    assert_eq!(
        geometry_coverage(&mesh, &parts, &set(&["Left"]), 1002, 8, 8),
        None
    );
    // Zero-sized target.
    assert_eq!(
        geometry_coverage(&mesh, &parts, &set(&["Left"]), 1001, 0, 8),
        None
    );
}

#[test]
fn geometry_coverage_zeroes_excluded_part_texels() {
    let (mesh, parts) = left_right();
    let plane = geometry_coverage(&mesh, &parts, &set(&["Left"]), 1001, 8, 8)
        .expect("Left is excluded on 1001");
    assert_eq!(plane.len(), 64);
    for y in 0..8 {
        for x in 0..8 {
            let want = if x < 4 { 0 } else { 255 };
            assert_eq!(plane[y * 8 + x], want, "texel ({x}, {y})");
        }
    }
    // Both excluded: nothing composites.
    let plane = geometry_coverage(&mesh, &parts, &set(&["Left", "Right"]), 1001, 8, 8)
        .expect("both excluded");
    assert!(plane.iter().all(|&v| v == 0));
}

#[test]
fn geometry_coverage_overlap_included_wins() {
    // Both parts cover the SAME full-tile quad: the overlap is included,
    // so the gate is fully open, whichever order the triangles come in.
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = MeshData {
        positions: uvs
            .iter()
            .map(|uv: &[f32; 2]| [uv[0], uv[1], 0.0])
            .collect(),
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs,
        indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
        ..Default::default()
    };
    for triangle_parts in [vec![0, 0, 1, 1], vec![1, 1, 0, 0]] {
        let parts = MeshParts {
            names: vec!["Shell".into(), "Inner".into()],
            triangle_parts,
        };
        assert_eq!(
            geometry_coverage(&mesh, &parts, &set(&["Inner"]), 1001, 8, 8),
            None,
            "included wins on every overlapping texel"
        );
    }
}

#[test]
fn geometry_coverage_dilates_gutters() {
    // Two small islands with an uncovered gutter column between them:
    // excluded "Left" island at u in [0, 0.25], included "Right" island
    // at u in [0.75, 1]. Gutter texels take their nearest covered
    // texel's value, so the plane is 0 on the left half and 255 on the
    // right half, with no uncovered hole left.
    let quad = |u0: f32, u1: f32| [[u0, 0.0], [u1, 0.0], [u1, 1.0], [u0, 1.0]];
    let mut uvs = Vec::new();
    uvs.extend(quad(0.0, 0.25));
    uvs.extend(quad(0.75, 1.0));
    let mesh = MeshData {
        positions: uvs.iter().map(|uv| [uv[0], uv[1], 0.0]).collect(),
        normals: vec![[0.0, 0.0, 1.0]; 8],
        uvs,
        indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
        ..Default::default()
    };
    let parts = MeshParts {
        names: vec!["Left".into(), "Right".into()],
        triangle_parts: vec![0, 0, 1, 1],
    };
    let plane = geometry_coverage(&mesh, &parts, &set(&["Left"]), 1001, 8, 8).expect("gate");
    for y in 0..8 {
        let row = &plane[y * 8..y * 8 + 8];
        assert_eq!(row, &[0, 0, 0, 0, 255, 255, 255, 255], "row {y}");
    }
}

/// The K2 cost measurement (brief: 4096² tile, ~100k-triangle fixture
/// carrying the cost class, not a 2-triangle quad). A 256x256 cell grid
/// over tile 1001 = 131 072 triangles; every other row of cells is the
/// excluded part. Ignored by default (it allocates a 16 MiB plane and
/// runs for a while in debug); run in release:
/// `cargo test --release -p umber-mesh --test geometry_parts -- --ignored --nocapture`.
#[test]
#[ignore]
fn geometry_coverage_cost_4096() {
    const N: usize = 256;
    let mut uvs = Vec::with_capacity((N + 1) * (N + 1));
    for j in 0..=N {
        for i in 0..=N {
            uvs.push([i as f32 / N as f32, j as f32 / N as f32]);
        }
    }
    let mut indices = Vec::with_capacity(N * N * 6);
    let mut triangle_parts = Vec::with_capacity(N * N * 2);
    for j in 0..N {
        for i in 0..N {
            let a = (j * (N + 1) + i) as u32;
            let b = a + 1;
            let c = a + (N + 1) as u32 + 1;
            let d = a + (N + 1) as u32;
            indices.extend([a, b, c, a, c, d]);
            let part = (j % 2) as u16;
            triangle_parts.extend([part, part]);
        }
    }
    let mesh = MeshData {
        positions: uvs.iter().map(|uv| [uv[0], uv[1], 0.0]).collect(),
        normals: vec![[0.0, 0.0, 1.0]; uvs.len()],
        uvs,
        indices,
        ..Default::default()
    };
    let parts = MeshParts {
        names: vec!["Even".into(), "Odd".into()],
        triangle_parts,
    };
    assert_eq!(mesh.triangle_count(), 131_072);
    let start = std::time::Instant::now();
    let plane = geometry_coverage(&mesh, &parts, &set(&["Odd"]), 1001, 4096, 4096).expect("gate");
    let elapsed = start.elapsed();
    eprintln!("geometry_coverage 4096^2, 131072 tris: {elapsed:?}");
    assert_eq!(plane.len(), 4096 * 4096);
    let open = plane.iter().filter(|&&v| v == 255).count();
    assert_eq!(open, 4096 * 4096 / 2, "half the rows composite");
}
