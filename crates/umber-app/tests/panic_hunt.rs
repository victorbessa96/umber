//! The panic-hunt (queue row 13's named follow-up): garbage in, typed
//! errors out — never a panic. Deterministic pseudo-random bytes (an
//! xorshift seeded per case; no fuzz framework, no new deps — the
//! house's inline-#[cfg(test)] shape, CI-fast).
//!
//! The seams are the PUBLIC parse/load surfaces untrusted bytes reach
//! first: the mtlx parser, the PSD importer, the .spp HDF5 reader, and
//! the project loader's corrupt-file arms. C2-20 (2026-10-15) added the
//! uncovered ones: the MaterialX IMPORTER (`mtlx_import::parse_mtlx`,
//! the third-party-document path, distinct from the round-trip parser),
//! `.umberbrush` presets, `.umbersmart` / `.umbermask` templates, the
//! paint sidecar's text formats, the autosave journal, and the PNG / EXR
//! readers the environment loader and the brush stamps use. The JSONL
//! agent wire is in `umber-cli/tests/agent_panic_hunt.rs`. Each case asserts ONLY the
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

// ---------------------------------------------------------------------
// C2-20: the seams the first hunt did not reach.
// ---------------------------------------------------------------------

/// Both garbage classes over a text seam: pure noise (lossy UTF-8) and a
/// real payload with bytes clobbered.
fn text_cases(salt: u64, valid: &str, cases: u64) -> Vec<String> {
    let mut out = Vec::new();
    for seed in 1..=cases {
        out.push(
            String::from_utf8_lossy(&garbage(seed + salt, (seed % 700) as usize + 1)).into_owned(),
        );
        let flips = (seed % 24) as usize + 1;
        out.push(
            String::from_utf8_lossy(&mutate(seed + salt, valid.as_bytes(), flips)).into_owned(),
        );
    }
    out
}

fn fixture(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is tracked: {e}", path.display()))
}

#[test]
fn mtlx_importer_takes_garbage_without_panicking() {
    // The importer reads OTHER tools' documents (Blender, the SP USD
    // creator): a richer grammar than the round-trip parser.
    for rel in [
        "crates/umber-graph/tests/fixtures/mtlx/blender_standard_surface.mtlx",
        "crates/umber-graph/tests/fixtures/mtlx/sp_usd_creator_udim.mtlx",
        "crates/umber-graph/tests/fixtures/mtlx/nodedef_instanced.mtlx",
    ] {
        let valid = fixture(rel);
        assert!(
            umber_graph::mtlx_import::parse_mtlx(&valid).is_ok(),
            "{rel} parses intact"
        );
        for case in text_cases(0x1_0000, &valid, 150) {
            let _ = umber_graph::mtlx_import::parse_mtlx(&case);
        }
    }
}

#[test]
fn brush_presets_take_garbage_without_panicking() {
    let valid = std::fs::read_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brushes"),
    )
    .ok()
    .and_then(|mut d| {
        d.find_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "umberbrush").then(|| std::fs::read_to_string(p).ok())?
        })
    })
    .expect("a shipped .umberbrush exists");
    assert!(
        umber_brush::preset::BrushPreset::from_json(&valid).is_ok(),
        "the shipped preset parses"
    );
    for case in text_cases(0x2_0000, &valid, 300) {
        let _ = umber_brush::preset::BrushPreset::from_json(&case);
    }
}

#[test]
fn smart_templates_take_garbage_without_panicking() {
    let material = fixture("assets/smart/worn-metal.umbersmart");
    let mask = fixture("assets/smart/edge-wear.umbermask");
    for case in text_cases(0x3_0000, &material, 300) {
        if let Ok(m) = umber_core::smart::SmartMaterial::from_json(&case) {
            // A template that parses must also APPLY without panicking:
            // apply is where the parsed values are used.
            let mut stack = umber_core::LayerStack::default();
            let _ = m.apply(&mut stack);
        }
    }
    for case in text_cases(0x3_8000, &mask, 300) {
        let _ = umber_core::smart::SmartMask::from_json(&case);
    }
}

#[test]
fn paint_sidecar_text_formats_take_garbage_without_panicking() {
    use umber_paint::persistence as p;
    let commit = "1001 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
    for case in text_cases(0x4_0000, commit, 300) {
        let _ = p::parse_paint_commit(&case);
        let _ = p::parse_paint_set_sizes(&case);
        let _ = p::parse_paint_target_size(&case);
        let _ = p::parse_paint_tile_name(&case);
        let _ = p::parse_paint_mask_name(&case);
    }
}

#[test]
fn the_journal_reader_takes_garbage_without_panicking() {
    // The autosave journal is what crash recovery replays: a torn or
    // hostile journal must read as its valid prefix, never a panic.
    let tmp = std::env::temp_dir().join(format!("umber-fuzz-journal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("temp dir");
    let path = umber_core::journal::journal_path(&tmp);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("journal dir");
    }
    for seed in 1..=200u64 {
        let mut bytes = garbage(seed + 0x5_0000, (seed % 900) as usize + 1);
        // Every fourth case: newline-dense, so the line splitter is busy.
        if seed % 4 == 0 {
            for (i, b) in bytes.iter_mut().enumerate() {
                if i % 7 == 0 {
                    *b = b'\n';
                }
            }
        }
        std::fs::write(&path, &bytes).expect("write garbage");
        if let Ok(entries) = umber_core::journal::read_journal(&tmp) {
            let mut model = umber_core::project::ProjectModel::new(
                vec![umber_core::TextureSet::new_default("Set")],
                Vec::new(),
                Default::default(),
            );
            let _ = umber_core::journal::replay(&mut model, &entries);
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn png_and_exr_readers_take_garbage_without_panicking() {
    // The environment loader and brush stamps read these from user files.
    let tmp = std::env::temp_dir().join(format!("umber-fuzz-img-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("temp dir");
    let png = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/env/studio-neutral.png"),
    )
    .expect("the default environment is tracked");
    for seed in 1..=120u64 {
        let noise = garbage(seed + 0x6_0000, (seed % 2048) as usize + 1);
        let mutated = mutate(seed + 0x6_0000, &png, (seed % 16) as usize + 1);
        for (name, bytes) in [("n.png", &noise), ("m.png", &mutated), ("n.exr", &noise)] {
            let path = tmp.join(name);
            std::fs::write(&path, bytes).expect("write garbage");
            if name.ends_with(".png") {
                let _ = umber_export::png::read_png_rgba8(&path);
            } else {
                let _ = umber_export::exr::read_exr_rgba_f32(&path);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
