//! cargo-fuzz target over the umber-mesh importer seams — queue row 386,
//! wave-fuzz-corpus (b), seeded from the row 156-A hostile corpus
//! (`crates/umber-mesh/tests/corpus/`, copied into `fuzz/corpus/mesh_load/`
//! with a one-byte lane selector prepended).
//!
//! Byte 0 of the input selects the lane — the `load` dispatcher once per
//! format extension, plus each direct pub entry, the same matrix
//! `crates/umber-mesh/tests/hostile_corpus.rs` runs — and the rest of the
//! input is written to a temp file with that lane's extension, then fed to
//! the entry.
//!
//! There is deliberately NO `catch_unwind` here: the contract under test is
//! hostile_corpus.rs's "every entry returns a typed Err, never a panic",
//! so a panic in an importer or a dependency must surface as a libFuzzer
//! crash artifact. (`load` itself carries the boundary and returns
//! `Err(ImportError::Panicked)`; the direct entries do not, and are the
//! seam this target exists to fuzz.) An `Ok` on a mutant is legitimate —
//! mutants may parse as valid small meshes; only the pinned corpus files
//! are required to be typed errors.
#![no_main]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;
use umber_mesh::{ImportError, MeshData};

/// libfuzzer-sys installs a panic hook that ABORTS on any panic, which
/// would neuter umber-mesh's own `catch_unwind` boundaries (`load`,
/// `load_gltf_with_parts`, ...). Replace it with a printing hook: panics
/// inside a boundary unwind normally and become typed `Err`s; only a
/// panic NO boundary catches reaches the `extern "C"` callback edge,
/// aborts there, and libFuzzer records the input as a crash artifact.
/// So a crash artifact == a pub entry that panics on hostile input.
fn install_passthrough_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("PANIC: {info}");
        }));
    });
}

type Entry = fn(&Path) -> Result<MeshData, ImportError>;

/// (lane name, temp-file extension, entry). Lanes 0..=6 drive the `load`
/// dispatcher with each format's extension; 7..=12 drive the direct pub
/// entries the way hostile_corpus.rs does.
const LANES: &[(&str, &str, Entry)] = &[
    ("load.glb", "glb", umber_mesh::load),
    ("load.gltf", "gltf", umber_mesh::load),
    ("load.obj", "obj", umber_mesh::load),
    ("load.fbx", "fbx", umber_mesh::load),
    ("load.usda", "usda", umber_mesh::load),
    ("load.usdc", "usdc", umber_mesh::load),
    ("load.usdz", "usdz", umber_mesh::load),
    ("gltf::load_gltf", "gltf", umber_mesh::gltf::load_gltf),
    ("fbx::load_fbx", "fbx", umber_mesh::fbx::load_fbx),
    ("usd::load_usda", "usda", umber_mesh::usd::load_usda),
    ("usdc::load_usdc", "usdc", umber_mesh::usdc::load_usdc),
    ("usdc::load_usdz", "usdz", umber_mesh::usdc::load_usdz),
    ("load_obj", "obj", umber_mesh::load_obj),
];

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(ext: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umber-fuzz-mesh-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("case-{id}.{ext}"))
}

fuzz_target!(|data: &[u8]| {
    install_passthrough_hook();
    if data.len() < 2 {
        return;
    }
    let (name, ext, entry) = LANES[(data[0] as usize) % LANES.len()];
    let path = temp_path(ext);
    if std::fs::write(&path, &data[1..]).is_err() {
        return;
    }
    // A panic unwinds out of fuzz_target and libFuzzer records the input as
    // a crash artifact; `name` is only used for diagnostics.
    let _ = name;
    let _ = entry(&path);
    let _ = std::fs::remove_file(&path);
});
