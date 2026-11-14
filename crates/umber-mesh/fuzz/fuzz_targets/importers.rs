//! wave-fuzz-corpus (b): every importer entry point over hostile bytes.
//! The first byte picks the extension so one target covers the whole
//! dispatch table; the input is written to a temp file because the
//! importers are path-based. A panic anywhere fails the run; a typed
//! `Err` (or even an `Ok`) is fine — the hostile-corpus suite's
//! Err-requirement is for the curated files, not fuzz mutants.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    const EXTS: [&str; 7] = ["obj", "gltf", "glb", "fbx", "usda", "usdc", "usdz"];
    let ext = EXTS[usize::from(data[0]) % EXTS.len()];
    let dir = std::env::temp_dir().join(format!("umber-fuzz-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("input.{ext}"));
    if std::fs::write(&path, data).is_ok() {
        let _ = umber_mesh::load(&path);
        let _ = umber_mesh::load_with_parts(&path);
    }
    let _ = std::fs::remove_file(&path);
});
