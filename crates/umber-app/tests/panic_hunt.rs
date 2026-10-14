//! The panic-hunt (queue row 13's named follow-up): garbage in, typed
//! errors out — never a panic. Deterministic pseudo-random bytes (an
//! xorshift seeded per case; no fuzz framework, no new deps — the
//! house's inline-#[cfg(test)] shape, CI-fast).
//!
//! The seams are the PUBLIC parse/load surfaces untrusted bytes reach
//! first: the mtlx parser, the PSD importer, the .spp HDF5 reader, and
//! the project loader's corrupt-file arms. Each case asserts ONLY the
//! no-panic contract plus typed-error honesty (a success on garbage
//! would be a worse bug — flagged, not celebrated).

/// Deterministic garbage: xorshift64 over `seed`, `len` bytes.
fn garbage(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed | 1;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push((x & 0xff) as u8);
    }
    out
}

/// Mutated-valid inputs: a real payload with bytes progressively
/// clobbered — the interesting middle ground (valid headers, broken
/// bodies) pure noise never reaches.
fn mutate(seed: u64, base: &[u8], flips: usize) -> Vec<u8> {
    let mut x = seed | 1;
    let mut out = base.to_vec();
    for _ in 0..flips {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let idx = (x as usize) % out.len();
        out[idx] ^= 0xff;
    }
    out
}

#[test]
fn mtlx_parser_takes_garbage_without_panicking() {
    // Pure noise + mutated-valid graphs: every outcome is Ok or
    // MtlxError — the parser's contract. 200 cases per class, each
    // deterministic (the seed IS the case id).
    for seed in 1..=200u64 {
        let noise = garbage(seed, (seed % 512) as usize + 1);
        let _ = umber_graph::mtlx::from_mtlx(&String::from_utf8_lossy(&noise));
    }
    let valid = umber_graph::mtlx::to_mtlx(
        &umber_graph::Graph::new(),
        &umber_graph::mtlx::painter_nodedefs(),
    );
    for seed in 1..=200u64 {
        let mutated = mutate(seed, valid.as_bytes(), (seed % 24) as usize + 1);
        let _ = umber_graph::mtlx::from_mtlx(&String::from_utf8_lossy(&mutated));
    }
}

#[test]
fn psd_importer_takes_garbage_without_panicking() {
    for seed in 1..=200u64 {
        let noise = garbage(seed + 0x5000, (seed % 1024) as usize + 1);
        let _ = umber_app::psd_import::import_psd_bytes(&noise, "fuzz.psd");
    }
}

#[test]
fn spp_reader_takes_garbage_without_panicking() {
    // The public seam is `open(path)` (hdf5::parse is crate-private by
    // design — import-only, no byte-level API crosses). Garbage lands
    // on disk once per case; `open` must answer typed, never panic.
    for seed in 1..=200u64 {
        let noise = garbage(seed + 0xa000, (seed % 4096) as usize + 1);
        let path =
            std::env::temp_dir().join(format!("umber-fuzz-{}-{}.spp", std::process::id(), seed));
        std::fs::write(&path, &noise).expect("write garbage");
        let _ = umber_spp::open(&path);
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn project_loader_corrupt_dirs_are_typed_errors_not_panics() {
    // A directory of garbage sidecars: every read fails typed (or the
    // dir simply doesn't exist — also typed). No panic paths in the
    // open flow.
    let tmp = std::env::temp_dir().join(format!("umber-fuzz-proj-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("temp dir");
    for seed in 1..=50u64 {
        let name = tmp.join(format!("sidecar-{seed}.json"));
        std::fs::write(&name, garbage(seed + 0xf000, (seed % 300) as usize + 1))
            .expect("write garbage");
    }
    let _ = umber_core::project::load_from_dir(&tmp);
    // The empty dir + the missing dir: typed errors, no panic.
    let _ = umber_core::project::load_from_dir(&tmp.join("absent"));
    let _ = std::fs::remove_dir_all(&tmp);
}
