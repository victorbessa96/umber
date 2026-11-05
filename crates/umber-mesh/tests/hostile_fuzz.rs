//! Long mutational fuzz lane over the hostile corpus — queue row 156,
//! sub-item A. **`#[ignore]`d**: not part of the always-on battery; run
//! explicitly in a battery session:
//!
//! ```text
//! UMBER_FUZZ_ITERS=200000 cargo test -p umber-mesh -- --ignored hostile_fuzz --nocapture
//! ```
//!
//! Iterations come from `UMBER_FUZZ_ITERS` (default 100_000). Each
//! iteration picks a corpus seed (`tests/corpus/`, via `gen.py`), applies
//! seeded-RNG mutations (byte flips, byte inserts, truncation, range
//! delete/duplicate), writes the mutant to a temp file keeping the seed's
//! extension, and runs it through every pub importer entry point
//! enumerated from `lib.rs` — the same matrix as `hostile_corpus.rs` —
//! each call inside `catch_unwind`. Any panic is a named finding and
//! fails the test; a finding becomes a new corpus file + a fix in the
//! same kill, so `hostile_corpus.rs` covers it permanently.
//!
//! No new dependencies: the RNG is a xorshift64* and the mutations are
//! hand-rolled. Deterministic per run: the seed is fixed, so a finding
//! reproduces byte-for-byte.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use umber_mesh::{ImportError, MeshData};

thread_local! {
    static LAST_PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

fn install_quiet_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            LAST_PANIC.with(|p| *p.borrow_mut() = format!("{info}"));
            default(info);
        }));
    });
}

/// xorshift64* — small, fast, deterministic; no external crates.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// One importer entry point: name + the pub loader fn.
type Entry = (&'static str, fn(&Path) -> Result<MeshData, ImportError>);

fn importer_entries() -> Vec<Entry> {
    vec![
        ("load", umber_mesh::load),
        ("load_obj", umber_mesh::load_obj),
        ("gltf::load_gltf", umber_mesh::gltf::load_gltf),
        ("fbx::load_fbx", umber_mesh::fbx::load_fbx),
        ("usd::load_usda", umber_mesh::usd::load_usda),
        ("usdc::load_usdc", umber_mesh::usdc::load_usdc),
        ("usdc::load_usdz", umber_mesh::usdc::load_usdz),
    ]
}

const CORPUS_SUBDIRS: [&str; 5] = ["fbx", "gltf", "usda", "usdc", "usdz"];

fn corpus_seeds() -> Vec<(String, Vec<u8>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut seeds = Vec::new();
    for sub in CORPUS_SUBDIRS {
        let dir = root.join(sub);
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .expect("corpus dir must exist")
            .map(|e| e.expect("corpus entry readable").path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for p in entries {
            let bytes = std::fs::read(&p).expect("seed readable");
            seeds.push((p.file_name().unwrap().to_string_lossy().into_owned(), bytes));
        }
    }
    assert!(seeds.len() >= 33, "corpus seeds missing");
    seeds
}

/// One mutation step: flip / insert / truncate / delete-range /
/// duplicate-range, chosen by the RNG.
fn mutate(rng: &mut Rng, bytes: &mut Vec<u8>) {
    if bytes.is_empty() {
        bytes.push(rng.next() as u8);
        return;
    }
    match rng.below(5) {
        0 => {
            // Byte flip at a random offset.
            let off = rng.below(bytes.len() as u64) as usize;
            bytes[off] ^= (rng.next() % 255 + 1) as u8;
        }
        1 => {
            // Insert 1-8 random bytes at a random offset.
            let off = rng.below(bytes.len() as u64 + 1) as usize;
            let n = 1 + rng.below(8) as usize;
            let ins: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
            bytes.splice(off..off, ins);
        }
        2 => {
            // Truncate to a random length.
            let cut = rng.below(bytes.len() as u64 + 1) as usize;
            bytes.truncate(cut);
        }
        3 => {
            // Delete a random range.
            let start = rng.below(bytes.len() as u64) as usize;
            let len = 1 + rng.below((bytes.len() - start).min(64) as u64) as usize;
            bytes.drain(start..(start + len).min(bytes.len()));
        }
        _ => {
            // Duplicate a random range.
            let start = rng.below(bytes.len() as u64) as usize;
            let len = 1 + rng.below((bytes.len() - start).min(64) as u64) as usize;
            let chunk = bytes[start..(start + len).min(bytes.len())].to_vec();
            bytes.splice(start..start, chunk);
        }
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
#[ignore = "long fuzz lane: UMBER_FUZZ_ITERS=200000 cargo test -p umber-mesh -- --ignored hostile_fuzz --nocapture"]
fn hostile_fuzz_mutational_loop() {
    install_quiet_hook();
    let iters: u64 = std::env::var("UMBER_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let seeds = corpus_seeds();
    let entries = importer_entries();
    let mut rng = Rng::new(0x156A_0001_2026_1105);
    let mut findings: Vec<String> = Vec::new();
    let mut oks = 0usize;
    let tmp = std::env::temp_dir().join(format!("umber-hostile-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("temp dir");

    for iter in 0..iters {
        let (name, seed) = &seeds[rng.below(seeds.len() as u64) as usize];
        let mut bytes = seed.clone();
        let rounds = 1 + rng.below(4);
        for _ in 0..rounds {
            mutate(&mut rng, &mut bytes);
        }
        // Keep the seed's extension so `load` dispatches like the original.
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = tmp.join(format!("fuzz-{iter}-{id}-{name}"));
        std::fs::write(&path, &bytes).expect("mutant write");
        for (entry_name, f) in &entries {
            let outcome = catch_unwind(AssertUnwindSafe(|| f(&path)));
            match outcome {
                Ok(Ok(_)) => oks += 1,
                Ok(Err(_)) => {}
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                        .unwrap_or_else(|| "<non-string panic payload>".to_string());
                    let site = LAST_PANIC.with(|p| std::mem::take(&mut *p.borrow_mut()));
                    let at = site
                        .lines()
                        .find(|l| {
                            l.contains("umber_mesh::")
                                || l.contains("openusd::")
                                || l.contains("ufbx::")
                                || l.contains("gltf::")
                                || l.contains("tobj::")
                        })
                        .unwrap_or("?")
                        .trim()
                        .to_string();
                    findings.push(format!(
                        "iter {iter} seed {name} x {entry_name}: PANIC: {msg} at {at} (mutant {} bytes at {path:?})",
                        bytes.len()
                    ));
                }
            }
        }
        let _ = std::fs::remove_file(&path);
        if findings.len() >= 10 {
            break; // enough to triage; the always-on suite is the gate
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    eprintln!(
        "hostile_fuzz: {iters} iters over {} seeds x {} entries, {oks} accepted mutants, {} panic findings",
        seeds.len(),
        entries.len(),
        findings.len()
    );
    assert!(
        findings.is_empty(),
        "fuzz findings (each becomes a corpus file + fix):\n{}",
        findings.join("\n")
    );
}
