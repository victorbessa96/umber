//! cargo-fuzz target over the umber-spp opener — queue row 386,
//! wave-fuzz-corpus (b). The input bytes are written to a temp `.spp`
//! file and fed to `umber_spp::open` (the HDF5-subset reader seam). The
//! `hdf5` module is private, so `open(path)` is the only pub entry; a
//! panic inside it unwinds out of this target and libFuzzer records the
//! input as a crash artifact.
#![no_main]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;

use libfuzzer_sys::fuzz_target;

/// See mesh_load.rs: replace libfuzzer-sys's aborting panic hook with a
/// printing one, so only panics no `catch_unwind` boundary catches abort
/// at the extern "C" edge and become crash artifacts.
fn install_passthrough_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("PANIC: {info}");
        }));
    });
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fuzz_target!(|data: &[u8]| {
    install_passthrough_hook();
    if data.is_empty() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("umber-fuzz-spp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path: PathBuf = dir.join(format!("case-{id}.spp"));
    if std::fs::write(&path, data).is_err() {
        return;
    }
    let _ = umber_spp::open(&path);
    let _ = std::fs::remove_file(&path);
});
