//! Hostile-OUTPUT round-trip battery — queue row 156-B export lane (the
//! open flank after row 156-A hardened the import side).
//!
//! umber-export's PUBLIC mesh-export surface, enumerated from
//! `src/lib.rs`, is exactly one format: `.usda` via
//! [`umber_export::usd::to_usda_document`] / [`umber_export::usd::write_usda`].
//! The fbx/gltf/usdc/usdz lanes are import-only in this workspace — no
//! exporter for them exists anywhere under `crates/` (the "glb/obj
//! mesh-export round-trip" ledger row is unimplemented) — so `.usda` is
//! the only format there is to round-trip through.
//!
//! Every fixture under `umber-mesh/tests/corpus/` + `tests/assets/` is:
//!
//! 1. imported with [`umber_mesh::load`] inside `catch_unwind` — a panic
//!    is a named failure, never a killed test process;
//! 2. if the import is a typed `Err` (the hostile-corpus contract), it is
//!    recorded as refused — there is no mesh to export;
//! 3. if it imports `Ok`, exported through every supported mesh format,
//!    again inside `catch_unwind`, and the exported file is RE-IMPORTED
//!    with [`umber_mesh::load`] and [`umber_mesh::usd::load_usda`]: same
//!    triangle count, finite positions/normals/uvs, and the output file
//!    under the [`umber_export::usd::MAX_USDA_EXPORT_BYTES`] cap.
//!
//! Plus determinism goldens (same mesh exported twice = byte-identical
//! output) and the bomb guard (oversized mesh / huge asset name / NaN
//! positions → typed `Err` or a sanitized, re-importable document, never
//! a panic, never a multi-GB file).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use umber_export::driver::ExportError;
use umber_export::usd::{to_usda_document, write_usda, UsdMeshView, MAX_USDA_EXPORT_BYTES};
use umber_mesh::MeshData;

/// The mesh-export formats umber-export's public API supports,
/// enumerated from `src/lib.rs` (see the module doc).
const MESH_EXPORT_FORMATS: [&str; 1] = ["usda"];

const CORPUS_SUBDIRS: [&str; 5] = ["fbx", "gltf", "usda", "usdc", "usdz"];

fn mesh_tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../umber-mesh/tests")
}

/// Every fixture: all hostile-corpus files plus the valid test assets.
fn fixture_files() -> Vec<PathBuf> {
    let root = mesh_tests_root();
    let mut files = Vec::new();
    for sub in CORPUS_SUBDIRS {
        let dir = root.join("corpus").join(sub);
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("corpus dir {dir:?} must exist: {e}"))
            .map(|e| e.expect("corpus entry readable").path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        files.extend(entries);
    }
    let assets = root.join("assets");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&assets)
        .unwrap_or_else(|e| panic!("assets dir {assets:?} must exist: {e}"))
        .map(|e| e.expect("asset entry readable").path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    files.extend(entries);
    files
}

/// Run `f` under a panic boundary: a panic becomes a named `Err`, like
/// `hostile_corpus.rs`'s `run_case` — never a killed test process.
fn run_catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        format!("PANIC: {msg}")
    })
}

fn view_of(mesh: &MeshData) -> UsdMeshView<'_> {
    UsdMeshView {
        positions: &mesh.positions,
        normals: &mesh.normals,
        uvs: &mesh.uvs,
        indices: &mesh.indices,
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "umber-export-roundtrip-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir creatable");
    dir
}

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

/// Export `mesh` through one supported format and re-import the output.
/// Every step is panic-guarded and typed; `Ok(())` means the round-trip
/// verified (same triangle count, finite attributes, under the size cap).
fn export_and_reimport(
    format: &str,
    asset: &str,
    mesh: &MeshData,
    dir: &Path,
) -> Result<(), String> {
    assert_eq!(
        format, "usda",
        "the only supported mesh format (see module doc)"
    );
    let view = view_of(mesh);
    let path = run_catch(|| write_usda(dir, asset, &view, &[]))
        .map_err(|p| format!("{asset}: export {p}"))?
        .map_err(|e| format!("{asset}: export returned a typed error: {e}"))?;
    let len = std::fs::metadata(&path)
        .map_err(|e| format!("{asset}: stat exported file: {e}"))?
        .len();
    if len > MAX_USDA_EXPORT_BYTES {
        return Err(format!(
            "{asset}: exported file is {len} bytes, over the cap {MAX_USDA_EXPORT_BYTES}"
        ));
    }
    let back_load = run_catch(|| umber_mesh::load(&path))
        .map_err(|p| format!("{asset}: re-import via load {p}"))?
        .map_err(|e| format!("{asset}: exported file must re-import, load gave: {e}"))?;
    let back_usda = run_catch(|| umber_mesh::load_usda(&path))
        .map_err(|p| format!("{asset}: re-import via load_usda {p}"))?
        .map_err(|e| format!("{asset}: exported file must re-import, load_usda gave: {e}"))?;
    for (entry, back) in [("load", &back_load), ("load_usda", &back_usda)] {
        if back.triangle_count() != mesh.triangle_count() {
            return Err(format!(
                "{asset}: {entry} triangle count {} != source {}",
                back.triangle_count(),
                mesh.triangle_count()
            ));
        }
        if !back.positions.iter().flatten().all(|v| v.is_finite()) {
            return Err(format!(
                "{asset}: {entry} re-imported positions not all finite"
            ));
        }
        if !back.normals.iter().flatten().all(|v| v.is_finite()) {
            return Err(format!(
                "{asset}: {entry} re-imported normals not all finite"
            ));
        }
        if !back.uvs.iter().flatten().all(|v| v.is_finite()) {
            return Err(format!("{asset}: {entry} re-imported uvs not all finite"));
        }
    }
    Ok(())
}

#[test]
fn fixtures_are_present() {
    let files = fixture_files();
    let corpus = files
        .iter()
        .filter(|p| p.starts_with(mesh_tests_root().join("corpus")))
        .count();
    let assets = files.len() - corpus;
    assert!(corpus >= 33, "hostile corpus shrank: {corpus} files");
    assert!(assets >= 4, "test assets shrank: {assets} files");
}

#[test]
fn every_fixture_roundtrips_through_every_export_format_or_is_a_typed_err() {
    let files = fixture_files();
    let root = mesh_tests_root();
    let mut failures: Vec<String> = Vec::new();
    let mut refused = 0usize;
    let mut roundtripped = 0usize;
    for (n, path) in files.iter().enumerate() {
        let rel = path.strip_prefix(&root).unwrap().display();
        // 1. Import, panic-guarded: a panic is a named failure.
        let imported = match run_catch(|| umber_mesh::load(path)) {
            Err(panic) => {
                failures.push(format!("{rel}: import {panic}"));
                continue;
            }
            Ok(r) => r,
        };
        let mesh = match imported {
            // 2. Hostile input: the contract is a typed Err; nothing to export.
            Err(_) => {
                refused += 1;
                continue;
            }
            Ok(m) => m,
        };
        // 3. Export through every supported mesh format + re-import.
        let asset = path.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = scratch(&format!("case{n}"));
        for format in MESH_EXPORT_FORMATS {
            match export_and_reimport(format, &asset, &mesh, &dir) {
                Ok(()) => roundtripped += 1,
                Err(e) => failures.push(format!("{rel} x {format}: {e}")),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(
        roundtripped >= 4,
        "the 4 test assets must round-trip, only {roundtripped} did"
    );
    assert!(
        failures.is_empty(),
        "{} fixtures: {refused} refused (typed Err), {roundtripped} round-trips, {} failures:\n{}",
        files.len(),
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn determinism_golden_same_mesh_exports_byte_identical() {
    let assets_dir = mesh_tests_root().join("assets");
    let assets: Vec<PathBuf> = fixture_files()
        .into_iter()
        .filter(|p| p.starts_with(&assets_dir))
        .collect();
    assert!(
        !assets.is_empty(),
        "need the valid assets as export sources"
    );
    for path in &assets {
        let asset = path.file_stem().unwrap().to_string_lossy().into_owned();
        let mesh = run_catch(|| umber_mesh::load(path))
            .expect("asset import must not panic")
            .expect("asset must import");
        let view = view_of(&mesh);
        // Document level: two serializations are identical.
        let d1 = run_catch(|| to_usda_document(&asset, &view, &[]))
            .expect("export must not panic")
            .expect("asset must export");
        let d2 = run_catch(|| to_usda_document(&asset, &view, &[]))
            .expect("export must not panic")
            .expect("asset must export");
        assert_eq!(d1, d2, "{asset}: document not deterministic");
        // File level: two writes are byte-identical and under the cap.
        let dir_a = scratch("det-a");
        let dir_b = scratch("det-b");
        let pa = write_usda(&dir_a, &asset, &view, &[]).expect("asset must export");
        let pb = write_usda(&dir_b, &asset, &view, &[]).expect("asset must export");
        let ba = std::fs::read(&pa).expect("read export a");
        let bb = std::fs::read(&pb).expect("read export b");
        assert_eq!(ba, bb, "{asset}: exported bytes differ across runs");
        assert!(
            ba.len() as u64 <= MAX_USDA_EXPORT_BYTES,
            "{asset}: golden over the cap"
        );
        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }
}

#[test]
fn bomb_guard_oversized_mesh_is_a_typed_err_and_writes_nothing() {
    // 600k vertices bound the document above the cap, so the export is
    // refused on the estimate before the multi-hundred-MB string exists.
    let n = 600_000;
    let positions = vec![[0.0f32; 3]; n];
    let normals = vec![[0.0f32, 0.0, 1.0]; n];
    let uvs = vec![[0.0f32, 0.0]; n];
    let indices: Vec<u32> = (0..n as u32).collect();
    let view = UsdMeshView {
        positions: &positions,
        normals: &normals,
        uvs: &uvs,
        indices: &indices,
    };
    let r = run_catch(|| to_usda_document("bomb", &view, &[])).expect("export must not panic");
    assert!(
        matches!(&r, Err(ExportError::TooLarge { cap, .. }) if *cap == MAX_USDA_EXPORT_BYTES),
        "expected TooLarge, got {}",
        match &r {
            Ok(s) => format!("Ok({} bytes)", s.len()),
            Err(e) => e.to_string(),
        }
    );
    let dir = scratch("bomb");
    let r = run_catch(|| write_usda(&dir, "bomb", &view, &[])).expect("write must not panic");
    assert!(
        matches!(&r, Err(ExportError::TooLarge { .. })),
        "write_usda must refuse over-cap: {r:?}"
    );
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "an over-cap export must not write any file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bomb_guard_huge_asset_name_is_a_typed_err() {
    let (p, n, u, i) = quad();
    let view = UsdMeshView {
        positions: &p,
        normals: &n,
        uvs: &u,
        indices: &i,
    };
    // 40M-char name: the sanitized prim segment rides in every path, so
    // the estimate exceeds the cap and the export is refused.
    let huge = "A".repeat(40_000_000);
    let r = run_catch(|| to_usda_document(&huge, &view, &[])).expect("export must not panic");
    assert!(
        matches!(&r, Err(ExportError::TooLarge { .. })),
        "huge name must be TooLarge, got {}",
        match &r {
            Ok(s) => format!("Ok({} bytes)", s.len()),
            Err(e) => e.to_string(),
        }
    );
}

#[test]
fn nan_inf_metadata_sanitizes_and_the_export_reimports_finite() {
    let path = mesh_tests_root().join("assets/cube_uv_ascii7400.fbx");
    let mut mesh = run_catch(|| umber_mesh::load(&path))
        .expect("import must not panic")
        .expect("asset must import");
    assert!(mesh.triangle_count() > 0);
    mesh.positions[0] = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
    mesh.uvs[0] = [f32::NAN, f32::INFINITY];
    let dir = scratch("nan");
    let out = run_catch(|| write_usda(&dir, "nan_cube", &view_of(&mesh), &[]))
        .expect("export must not panic")
        .expect("NaN/inf sanitize to lexable 0.0, not an error");
    let doc = std::fs::read_to_string(&out).expect("read export");
    let non_finite_token = doc
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| matches!(w, "NaN" | "nan" | "inf" | "-inf" | "infinity" | "-infinity"));
    assert!(
        !non_finite_token,
        "no non-finite number may lex into the exported document"
    );
    let back = run_catch(|| umber_mesh::load(&out))
        .expect("re-import must not panic")
        .expect("sanitized export must re-import");
    assert_eq!(back.triangle_count(), mesh.triangle_count());
    assert!(back
        .positions
        .iter()
        .all(|p| p.iter().all(|v| v.is_finite())));
    assert!(back.uvs.iter().all(|p| p.iter().all(|v| v.is_finite())));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ragged_and_out_of_range_meshes_are_typed_errs_not_panics() {
    let (p, n, u, _) = quad();
    for (label, indices) in [
        ("not-a-multiple-of-3", vec![0u32, 1]),
        ("out-of-range", vec![0, 1, 99]),
    ] {
        let view = UsdMeshView {
            positions: &p,
            normals: &n,
            uvs: &u,
            indices: &indices,
        };
        let r = run_catch(|| to_usda_document("S", &view, &[])).expect("export must not panic");
        assert!(
            matches!(&r, Err(ExportError::Write(_))),
            "{label}: expected typed Write error, got {r:?}"
        );
    }
    let short_normals = vec![[0.0f32, 0.0, 1.0]; 2];
    let view = UsdMeshView {
        positions: &p,
        normals: &short_normals,
        uvs: &u,
        indices: &[0, 1, 2],
    };
    let r = run_catch(|| to_usda_document("S", &view, &[])).expect("export must not panic");
    assert!(
        matches!(&r, Err(ExportError::Write(_))),
        "ragged attributes: expected typed Write error, got {r:?}"
    );
}
