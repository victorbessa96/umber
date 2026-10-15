//! The mesh panic-hunt (ROADMAP wave 8 `wave-fuzz-corpus` item (a)):
//! garbage in, typed `ImportError` out — never a panic. The umber-app
//! `tests/panic_hunt.rs` shape extended to the six mesh importers and the
//! `load` dispatcher: deterministic xorshift noise plus mutated-valid
//! inputs (truncation, strided byte flips, zeroed ranges, targeted text
//! edits — huge counts, negative indices, NaN).
//!
//! Contract asserted: every case returns (never panics, never aborts).
//! A success on pure noise is a worse bug than an error — such cases are
//! COUNTED and printed (`FLAGGED`), not celebrated; they do not fail the
//! test because a mutated-valid input can legitimately still be valid.
//!
//! Judgment calls (the kill report has the rest):
//! - `catch_unwind` wraps each case so a failure names the SEED and the
//!   mutation — a panic message from inside a dependency never carries
//!   it. Every caught panic still fails the test.
//! - Every bug found is pinned below as an `#[ignore = "BUG: …"]` minimal
//!   reproducer (exact bytes/text). Un-ignore once the importer is fixed:
//!   the test then asserts the typed `Err`.
//! - The fuzz loops steer AROUND the pinned bug classes only where a
//!   mutation would hit them on nearly every case (noted inline); the
//!   reproducers carry the coverage for those classes.
//! - Files land in a per-test temp dir
//!   (`temp_dir()/umber-mesh-fuzz-<name>-<pid>`), removed on drop —
//!   never a shared dir (parallel tests race).

use std::cell::{Cell, RefCell};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{Duration, Instant};

use umber_mesh::{ImportError, MeshData};

// ---------------------------------------------------------------- harness

fn xorshift(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

/// Deterministic garbage: xorshift64 over `seed`, `len` bytes.
fn garbage(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len).map(|_| (xorshift(&mut x) & 0xff) as u8).collect()
}

/// Mutated-valid: `flips` pseudo-random bytes of `base` inverted.
fn mutate(seed: u64, base: &[u8], flips: usize) -> Vec<u8> {
    let mut x = seed | 1;
    let mut out = base.to_vec();
    for _ in 0..flips {
        let idx = (xorshift(&mut x) as usize) % out.len();
        out[idx] ^= 0xff;
    }
    out
}

/// Per-test temp dir, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("umber-mesh-fuzz-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        Dir(p)
    }

    fn write(&self, file: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(file);
        std::fs::write(&p, bytes).expect("write fuzz case");
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

thread_local! {
    /// Set while a fuzz case runs: the hook then records instead of printing.
    static IN_CASE: Cell<bool> = const { Cell::new(false) };
    /// `file:line` of the last panic on this thread (+ a backtrace when the
    /// site is inside `alloc`, where the location alone names nothing).
    static LAST_PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Chain a panic hook that records each in-case panic's SITE. Panic
/// payloads carry no location, and the known-bug triage below matches
/// on sites, never on messages ("attempt to multiply with overflow" is
/// every overflow everywhere). Outside a case the default hook runs, so
/// ordinary test failures still print.
fn install_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !IN_CASE.with(Cell::get) {
                default(info);
                return;
            }
            // Normalized to '/' separators: on Windows `Location::file()`
            // is `crates\umber-mesh\src\...` and the registry path is
            // `C:\Users\...\gltf-json-1.4.1\src\mesh.rs`, so the '/'-
            // written needles in the KNOWN tables matched nothing and every
            // known bug read as a NEW panic (CI 38672191735, windows leg).
            let mut site = info
                .location()
                .map(|l| format!("{}:{}", l.file().replace('\\', "/"), l.line()))
                .unwrap_or_default();
            if site.contains("/alloc/") {
                site.push_str(
                    &std::backtrace::Backtrace::force_capture()
                        .to_string()
                        .replace('\\', "/"),
                );
            }
            LAST_PANIC.with(|p| *p.borrow_mut() = site);
        }));
    });
}

/// A panic site already pinned by a `bug_*` reproducer: `(needle, bug)`.
/// A caught panic whose site contains `needle` is tallied as KNOWN (and
/// printed) instead of failing the class, so the loops keep their power
/// to catch NEW sites. Each needle is a `file:line` (or, for `alloc`
/// sites, the dependency frame that drove the allocation).
type Known = &'static [(&'static str, &'static str)];

const GLTF_KNOWN: Known = &[(
    "gltf-json-1.4.1/src/mesh.rs:151",
    "bug_gltf_position_dangling_accessor_panics",
)];

const USDC_KNOWN: Known = &[
    (
        "umber-mesh/src/usdc.rs:218",
        "bug_usdc_negative_index_panics",
    ),
    (
        "umber-mesh/src/usdc.rs:219",
        "bug_usdc_negative_index_panics",
    ),
    (
        "umber-mesh/src/usdc.rs:220",
        "bug_usdc_index_out_of_range_panics",
    ),
    (
        "umber-mesh/src/usdc.rs:239",
        "bug_usdc_negative_count_panics",
    ),
    // The alloc-site needles name the openusd frame that drove the failing
    // allocation. Symbol syntax differs by platform: Linux backtraces print
    // `<R as openusd::usdc::reader::ReadExt>::read_vec` and
    // `read_compressed::<..>`; MSVC prints `openusd::usdc::reader::impl$2::
    // read_vec<..>` and `...::read_compressed<..>` (no turbofish), and on a
    // Vec3f read the innermost named frame is `RawVec<openusd::gf::vec::
    // Vec3f>::with_capacity_in` (CI 38674793562). Each pair matches both.
    (
        "ReadExt>::read_vec",
        "bug_openusd_read_vec_capacity_overflow",
    ),
    (
        "openusd::usdc::reader::impl$",
        "bug_openusd_read_vec_capacity_overflow",
    ),
    (
        "RawVec<openusd::gf::vec::",
        "bug_openusd_read_vec_capacity_overflow",
    ),
    (
        "read_compressed::<",
        "bug_openusd_read_compressed_capacity_overflow",
    ),
    (
        ">::read_compressed<",
        "bug_openusd_read_compressed_capacity_overflow",
    ),
    ("usdc/coding.rs:52", "bug_openusd_decode_ints_slice_oob"),
    (
        "usdc/coding.rs:22",
        "bug_openusd_encoded_buffer_size_overflow",
    ),
    ("usdc/reader.rs:1562", "bug_openusd_lz4_chunked_todo"),
    (
        "usdc/reader.rs:656",
        "bug_openusd_compressed_value_debug_assert",
    ),
];

/// usdc mutations that ABORTED openusd 0.7.0 on a failed multi-TiB
/// allocation (`ReadExt::read_vec` / `read_compressed` sized from
/// unchecked crate counts) — 25 usdc cases, 15 usdz. EMPTY since openusd
/// 0.8.0 (2026-10-15, mxpv/openusd#138): every one of them now runs, on
/// the frozen 0.7.0 bytes, and must come back typed.
/// `bug_usdc_huge_count_allocation_aborts` keeps the child-process pin.
const USDC_ABORTS: &[&str] = &[];
const USDZ_ABORTS: &[&str] = &[];

/// A per-case wall-clock ceiling: a case over this is an allocation bomb
/// or a hang and is reported as a finding.
const CASE_BUDGET: Duration = Duration::from_secs(5);

/// Tally of one fuzz class. `run` never lets a panic escape; `finish`
/// fails the test if any case panicked or blew the time budget.
struct Hunt {
    name: &'static str,
    known: Known,
    /// Case names that ABORT the process (failed allocation, stack
    /// overflow): not catchable, so they cannot run in-loop. Each is
    /// pinned by a `bug_*` reproducer.
    skip: &'static [&'static str],
    known_hits: Vec<String>,
    skipped: usize,
    ok: usize,
    err: usize,
    flagged: Vec<String>,
    panics: Vec<String>,
    slow: Vec<String>,
}

impl Hunt {
    fn new(name: &'static str) -> Self {
        Self::triaged(name, &[], &[])
    }

    fn triaged(name: &'static str, known: Known, skip: &'static [&'static str]) -> Self {
        install_hook();
        Hunt {
            name,
            known,
            skip,
            known_hits: Vec::new(),
            skipped: 0,
            ok: 0,
            err: 0,
            flagged: Vec::new(),
            panics: Vec::new(),
            slow: Vec::new(),
        }
    }

    /// Run one case. `noise` = the input is pure garbage, so an `Ok` is
    /// flagged as a dishonest success.
    fn run(
        &mut self,
        case: String,
        noise: bool,
        f: impl FnOnce() -> Result<MeshData, ImportError>,
    ) {
        // An abort (stack overflow, failed allocation) kills the binary
        // before `finish` can name the case; UMBER_FUZZ_TRACE=1 prints
        // each case first so the last line names the killer.
        // UMBER_FUZZ_SKIP=a|b adds abort cases at runtime (triage aid).
        let extra = std::env::var("UMBER_FUZZ_SKIP").unwrap_or_default();
        if self.skip.contains(&case.as_str()) || extra.split('|').any(|c| c == case) {
            self.skipped += 1;
            return;
        }
        if std::env::var_os("UMBER_FUZZ_TRACE").is_some() {
            eprintln!("[{}] case: {case}", self.name);
        }
        let start = Instant::now();
        IN_CASE.with(|c| c.set(true));
        let outcome = catch_unwind(AssertUnwindSafe(f));
        IN_CASE.with(|c| c.set(false));
        let took = start.elapsed();
        if took > CASE_BUDGET {
            self.slow.push(format!("{case} took {took:?}"));
        }
        match outcome {
            Ok(Ok(mesh)) => {
                self.ok += 1;
                if noise {
                    self.flagged.push(format!(
                        "{case}: Ok({} verts, {} indices)",
                        mesh.positions.len(),
                        mesh.indices.len()
                    ));
                }
            }
            Ok(Err(_)) => self.err += 1,
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<non-string panic>".into());
                let site = LAST_PANIC.with(|p| std::mem::take(&mut *p.borrow_mut()));
                match self.known.iter().find(|(needle, _)| site.contains(needle)) {
                    Some((_, bug)) => self.known_hits.push(format!("{case} -> {bug}")),
                    None => {
                        // For an `alloc` site, name the first frame that
                        // drove the allocation instead.
                        let at = site
                            .lines()
                            .find(|l| l.contains("openusd::") || l.contains("umber_mesh::"))
                            .or_else(|| site.lines().next())
                            .unwrap_or("?")
                            .trim();
                        self.panics.push(format!("{case}: {msg} at {at}"));
                    }
                }
            }
        }
    }

    fn finish(self) {
        let name = self.name;
        eprintln!(
            "[{name}] ok={} err={} flagged={} known-bug={} skipped-abort={} panics={} slow={}",
            self.ok,
            self.err,
            self.flagged.len(),
            self.known_hits.len(),
            self.skipped,
            self.panics.len(),
            self.slow.len()
        );
        for k in &self.known_hits {
            eprintln!("[{name}] known bug: {k}");
        }
        for f in &self.flagged {
            eprintln!("[{name}] FLAGGED success on garbage: {f}");
        }
        assert!(
            self.panics.is_empty() && self.slow.is_empty(),
            "[{name}] {} panicking / {} over-budget cases:\n{}\n{}",
            self.panics.len(),
            self.slow.len(),
            self.panics.join("\n"),
            self.slow.join("\n")
        );
    }
}

/// Byte-level mutation classes over a valid payload.
fn byte_mutations(base: &[u8], cases: u64) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let n = base.len();
    // Truncation at many lengths (dense near the head, strided after).
    let mut cut = 0usize;
    while cut < n {
        out.push((format!("truncate@{cut}"), base[..cut].to_vec()));
        cut += if cut < 64 { 1 } else { (n / 97).max(1) };
    }
    // Single-byte flips at strided offsets.
    let stride = (n / 64).max(1);
    for off in (0..n).step_by(stride) {
        let mut m = base.to_vec();
        m[off] ^= 0xff;
        out.push((format!("flip@{off}"), m));
    }
    // Zeroed ranges.
    for (i, start) in (0..n).step_by((n / 24).max(1)).enumerate() {
        let len = 1 + (i * 7) % 64;
        let mut m = base.to_vec();
        for b in m.iter_mut().skip(start).take(len) {
            *b = 0;
        }
        out.push((format!("zero@{start}+{len}"), m));
    }
    // Random multi-flips.
    for seed in 1..=cases {
        out.push((
            format!("mutate seed={seed}"),
            mutate(seed, base, (seed % 24) as usize + 1),
        ));
    }
    out
}

/// Text-level mutations: every occurrence of each `(from, to)` edit
/// applied one at a time (each case differs from the valid text by one
/// token).
fn text_edits(base: &str, edits: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for (from, to) in edits {
        for (pos, _) in base.match_indices(from) {
            let mut s = base.to_string();
            s.replace_range(pos..pos + from.len(), to);
            out.push((format!("edit {from:?}->{to:?} @{pos}"), s.into_bytes()));
        }
    }
    out
}

/// The numeric/structural edits shared by the text formats.
const NUMERIC_EDITS: &[(&str, &str)] = &[
    ("1", "999999999"),
    ("1", "-1"),
    ("1", "NaN"),
    ("1", "inf"),
    ("0", "-2147483648"),
    ("0", "18446744073709551615"),
    ("2", "1e308"),
    ("3", "4294967295"),
    ("[", ""),
    ("]", ""),
    (",", ""),
    ("(", "["),
];

// ---------------------------------------------------------------- fixtures

fn fbx_cube() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("assets")
        .join("cube_b7400.fbx");
    std::fs::read(path).expect("cube fbx fixture")
}

/// RFC 4648 base64 (data-URI buffers).
fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let mut b = 0u32;
        for (i, x) in c.iter().enumerate() {
            b |= (*x as u32) << (16 - 8 * i);
        }
        for k in 0..=c.len() {
            out.push(A[(b >> (18 - 6 * k)) as usize & 63] as char);
        }
        for _ in c.len()..3 {
            out.push('=');
        }
    }
    out
}

/// The knobs a glTF reproducer turns; `Default` is the valid triangle.
struct GltfTri {
    /// Extra bufferView-0 JSON, e.g. `, "byteStride": 4`.
    stride: &'static str,
    count: &'static str,
    pos_component: &'static str,
    pos_type: &'static str,
    index_component: &'static str,
    indices: [u32; 3],
    /// Extra attributes on the indexed primitive, e.g. `, "NORMAL": 1`.
    extra_attr: &'static str,
    /// Prepend a non-indexed primitive (shifts the second one's base).
    two_prims: bool,
}

impl Default for GltfTri {
    fn default() -> Self {
        GltfTri {
            stride: "",
            count: "3",
            pos_component: "5126",
            pos_type: "VEC3",
            index_component: "5125",
            indices: [0, 1, 2],
            extra_attr: "",
            two_prims: false,
        }
    }
}

impl GltfTri {
    /// Minimal glTF: one triangle, f32 VEC3 positions + u32 indices in a
    /// 48-byte embedded base64 buffer.
    fn json(&self) -> String {
        let mut buf = Vec::new();
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in v {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in self.indices {
            buf.extend_from_slice(&i.to_le_bytes());
        }
        let extra = self.extra_attr;
        let indexed = format!(r#"{{"attributes": {{"POSITION": 0{extra}}}, "indices": 1}}"#);
        let prims = if self.two_prims {
            format!(r#"{{"attributes": {{"POSITION": 0}}}}, {indexed}"#)
        } else {
            indexed
        };
        let (stride, count, pc, pt, ic) = (
            self.stride,
            self.count,
            self.pos_component,
            self.pos_type,
            self.index_component,
        );
        let uri = b64(&buf);
        format!(
            r#"{{"asset": {{"version": "2.0"}},
"meshes": [{{"primitives": [{prims}]}}],
"buffers": [{{"byteLength": 48, "uri": "data:application/octet-stream;base64,{uri}"}}],
"bufferViews": [
 {{"buffer": 0, "byteOffset": 0, "byteLength": 36{stride}}},
 {{"buffer": 0, "byteOffset": 36, "byteLength": 12}}],
"accessors": [
 {{"bufferView": 0, "componentType": {pc}, "count": {count}, "type": "{pt}", "min": [0, 0, 0], "max": [1, 1, 0]}},
 {{"bufferView": 1, "componentType": {ic}, "count": 3, "type": "SCALAR"}}]}}"#
        )
    }
}

/// Minimal glTF→GLB wrapper (12-byte header, one JSON chunk, no BIN).
fn glb(json: &str) -> Vec<u8> {
    let mut j = json.as_bytes().to_vec();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let total = 12 + 8 + j.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out
}

const USDA_QUAD: &str = r#"#usda 1.0
(
    defaultPrim = "Root"
)

def Xform "Root"
{
    def Mesh "Quad"
    {
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
        normal3f[] normals = [(0, 0, 1), (0, 0, 1), (0, 0, 1), (0, 0, 1)] (
            interpolation = "vertex"
        )
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
        texCoord2f[] primvars:st = [(0, 0), (1, 0), (1, 1), (0, 1)] (
            interpolation = "faceVarying"
        )
        int[] primvars:st:indices = [0, 1, 2, 3]
        uniform token orientation = "leftHanded"
    }
}
"#;

const OBJ_QUAD: &str = "o quad\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\nf 1/1/1 2/2/1 3/3/1 4/4/1\n";

/// A `.usdc` crate built by openusd's own writer from `.usda` text (the
/// usdc.rs unit tests' fixture route — no shipped binary).
fn usdc_bytes(text: &str) -> Vec<u8> {
    let data = openusd::usda::parse(text).expect("fixture parses");
    let mut out = std::io::Cursor::new(Vec::<u8>::new());
    openusd::usdc::CrateWriter::write(&data, &mut out).expect("fixture writes");
    out.into_inner()
}

/// A usdc mesh with the given topology (points fixed: one triangle).
fn usdc_mesh(counts: &str, indices: &str) -> Vec<u8> {
    usdc_bytes(&format!(
        "#usda 1.0\ndef Mesh \"M\"\n{{\n    float3[] points = [(0,0,0),(1,0,0),(0,1,0)]\n    \
         int[] faceVertexCounts = [{counts}]\n    int[] faceVertexIndices = [{indices}]\n}}\n"
    ))
}

const USDC_TRI_TEXT: &str = "#usda 1.0\ndef Mesh \"M\"\n{\n    float3[] points = [(0,0,0),(1,0,0),(0,1,0)]\n    int[] faceVertexCounts = [3]\n    int[] faceVertexIndices = [0, 1, 2]\n    texCoord2f[] primvars:st = [(0, 0), (1, 0), (0, 1)]\n}\n";

fn usdz_package(layer: &[u8]) -> Vec<u8> {
    let mut zip = openusd::usdz::ArchiveWriter::new(std::io::Cursor::new(Vec::new()));
    zip.add_layer("scene.usdc", layer).expect("add layer");
    zip.finish().expect("finish zip").into_inner()
}

// ---------------------------------------------------------------- garbage

#[test]
fn every_importer_takes_pure_noise_without_panicking() {
    // 200 noise payloads x 7 extensions through the dispatcher, plus the
    // same noise behind each binary/text magic (past the first gate).
    // Noise never parses as a mesh, so any success here is FLAGGED.
    let dir = Dir::new("noise");
    let mut hunt = Hunt::new("noise");
    for seed in 1..=200u64 {
        let noise = garbage(seed, (seed as usize * 37) % 4096 + 1);
        for ext in ["obj", "gltf", "glb", "fbx", "usda", "usdc", "usdz"] {
            let p = dir.write(&format!("n{seed}.{ext}"), &noise);
            hunt.run(format!("noise seed={seed} .{ext}"), true, || {
                umber_mesh::load(&p)
            });
        }
        for (ext, magic) in [
            ("usda", &b"#usda 1.0\n"[..]),
            ("usdc", &b"PXR-USDC"[..]),
            ("fbx", &b"Kaydara FBX Binary  \x00\x1a\x00"[..]),
            ("glb", &b"glTF\x02\x00\x00\x00"[..]),
            ("usdz", &b"PK\x03\x04"[..]),
        ] {
            let mut bytes = magic.to_vec();
            bytes.extend_from_slice(&noise);
            let p = dir.write(&format!("m{seed}.{ext}"), &bytes);
            hunt.run(format!("magic+noise seed={seed} .{ext}"), true, || {
                umber_mesh::load(&p)
            });
        }
    }
    // Empty file, missing file, no extension, unknown extension.
    let empty = dir.write("empty.fbx", b"");
    hunt.run("empty .fbx".into(), true, || umber_mesh::load_fbx(&empty));
    let missing = dir.0.join("absent.usda");
    hunt.run("missing .usda".into(), true, || umber_mesh::load(&missing));
    let bare = dir.write("noext", b"v 0 0 0");
    hunt.run("no extension".into(), true, || umber_mesh::load(&bare));
    let stl = dir.write("x.stl", b"solid x");
    hunt.run(".stl".into(), true, || umber_mesh::load(&stl));
    hunt.finish();
}

// ---------------------------------------------------------------- FBX

#[test]
fn fbx_mutated_cube_never_panics() {
    let base = fbx_cube();
    let dir = Dir::new("fbx");
    let mut hunt = Hunt::new("fbx");
    let valid = dir.write("valid.fbx", &base);
    assert!(umber_mesh::load_fbx(&valid).is_ok(), "valid fixture loads");
    for (i, (case, bytes)) in byte_mutations(&base, 150).into_iter().enumerate() {
        let p = dir.write(&format!("c{i}.fbx"), &bytes);
        hunt.run(case, false, || umber_mesh::load_fbx(&p));
    }
    hunt.finish();
}

// ---------------------------------------------------------------- glTF

#[test]
fn gltf_mutated_triangle_never_panics() {
    let json = GltfTri::default().json();
    let dir = Dir::new("gltf");
    let mut hunt = Hunt::triaged("gltf", GLTF_KNOWN, &[]);
    let valid = dir.write("valid.gltf", json.as_bytes());
    let mesh = umber_mesh::load_gltf(&valid).expect("valid fixture loads");
    assert_eq!(mesh.indices, vec![0, 1, 2]);
    // Byte-level mutations of the JSON text and of its GLB wrapping.
    for (i, (case, bytes)) in byte_mutations(json.as_bytes(), 150).into_iter().enumerate() {
        let p = dir.write(&format!("t{i}.gltf"), &bytes);
        hunt.run(format!("gltf {case}"), false, || umber_mesh::load_gltf(&p));
    }
    for (i, (case, bytes)) in byte_mutations(&glb(&json), 60).into_iter().enumerate() {
        let p = dir.write(&format!("b{i}.glb"), &bytes);
        hunt.run(format!("glb {case}"), false, || umber_mesh::load_gltf(&p));
    }
    // Structural edits. Kept off the pinned bug classes (accessor
    // `count` 0/huge, short `byteStride`, componentType/type swaps): each
    // panics in gltf 1.4.1 on EVERY case and has its own reproducer.
    let edits: &[(&str, &str)] = &[
        ("\"byteLength\": 48", "\"byteLength\": 4800"),
        ("\"byteLength\": 36", "\"byteLength\": 3"),
        ("\"byteLength\": 12", "\"byteLength\": 0"),
        ("\"byteOffset\": 36", "\"byteOffset\": 4000"),
        ("\"byteOffset\": 0", "\"byteOffset\": 47"),
        ("\"bufferView\": 1", "\"bufferView\": 9"),
        ("\"bufferView\": 0", "\"bufferView\": -1"),
        ("\"buffer\": 0", "\"buffer\": 3"),
        ("\"POSITION\": 0", "\"POSITION\": 7"),
        ("\"indices\": 1", "\"indices\": 5"),
        ("\"min\": [0, 0, 0]", "\"min\": [0]"),
        ("\"max\": [1, 1, 0]", "\"max\": \"x\""),
        ("base64,", "base64,!!"),
        ("data:application", "file:///nonexistent#"),
        ("\"version\": \"2.0\"", "\"version\": \"9.9\""),
        (
            "\"count\": 3, \"type\": \"SCALAR\"",
            "\"count\": 2, \"type\": \"SCALAR\"",
        ),
        (
            "\"count\": 3, \"type\": \"SCALAR\"",
            "\"count\": 4, \"type\": \"SCALAR\"",
        ),
    ];
    for (i, (case, bytes)) in text_edits(&json, edits).into_iter().enumerate() {
        let p = dir.write(&format!("e{i}.gltf"), &bytes);
        hunt.run(case, false, || umber_mesh::load_gltf(&p));
    }
    hunt.finish();
}

// ---------------------------------------------------------------- USDA

#[test]
fn usda_mutated_quad_never_panics() {
    let dir = Dir::new("usda");
    let mut hunt = Hunt::new("usda");
    let valid = dir.write("valid.usda", USDA_QUAD.as_bytes());
    let mesh = umber_mesh::load_usda(&valid).expect("valid fixture loads");
    assert_eq!(mesh.indices.len(), 6);
    for (i, (case, bytes)) in byte_mutations(USDA_QUAD.as_bytes(), 200)
        .into_iter()
        .enumerate()
    {
        let p = dir.write(&format!("b{i}.usda"), &bytes);
        hunt.run(case, false, || umber_mesh::load_usda(&p));
    }
    let mut edits = NUMERIC_EDITS.to_vec();
    edits.extend_from_slice(&[
        ("[4]", "[999999999]"),
        ("[4]", "[2147483647, 2147483647]"),
        ("[4]", "[-4]"),
        ("[4]", "[3, 3]"),
        ("[0, 1, 2, 3]", "[]"),
        ("\"vertex\"", "\"bogus\""),
        ("\"faceVarying\"", "\"uniform\""),
        ("\"leftHanded\"", "\"\""),
        ("point3f[] points", "point3f[] points.timeSamples"),
        ("def Mesh", "over Mesh"),
        ("\"Quad\"", "\"Q\\"),
        ("#usda", "#usdc"),
    ]);
    for (i, (case, bytes)) in text_edits(USDA_QUAD, &edits).into_iter().enumerate() {
        let p = dir.write(&format!("e{i}.usda"), &bytes);
        hunt.run(case, false, || umber_mesh::load_usda(&p));
    }
    // Bracket depth inside the parser's safe range (the overflow at
    // ~10k+ is pinned by `bug_usda_deep_nesting_overflows_stack`).
    for depth in [1usize, 64, 512] {
        let s = format!(
            "#usda 1.0\ndef Mesh \"M\"\n{{\n    int[] faceVertexCounts = {}\n}}\n",
            "[".repeat(depth)
        );
        let p = dir.write(&format!("deep{depth}.usda"), s.as_bytes());
        hunt.run(format!("nesting depth={depth}"), false, || {
            umber_mesh::load_usda(&p)
        });
    }
    hunt.finish();
}

// ---------------------------------------------------------------- USDC / USDZ

#[test]
fn usdc_mutated_crate_never_panics() {
    let dir = Dir::new("usdc");
    let mut hunt = Hunt::triaged("usdc", USDC_KNOWN, USDC_ABORTS);
    let base = usdc_frozen_0_7_0();
    let valid = dir.write("valid.usdc", &base);
    let mesh = umber_mesh::usdc::load_usdc(&valid).expect("valid fixture loads");
    assert_eq!(mesh.indices, vec![0, 1, 2]);
    for (i, (case, bytes)) in byte_mutations(&base, 200).into_iter().enumerate() {
        let p = dir.write(&format!("b{i}.usdc"), &bytes);
        hunt.run(case, false, || umber_mesh::usdc::load_usdc(&p));
    }
    // Topology edits that stay typed (the out-of-range / negative
    // classes panic and are pinned by the `bug_usdc_*` reproducers).
    for (i, (counts, indices)) in [
        ("3", "0, 1"),
        ("2", "0, 1"),
        ("0", ""),
        ("3, 3", "0, 1, 2"),
        ("2147483647", "0, 1, 2"),
    ]
    .iter()
    .enumerate()
    {
        let p = dir.write(&format!("t{i}.usdc"), &usdc_mesh(counts, indices));
        hunt.run(
            format!("topology counts=[{counts}] indices=[{indices}]"),
            false,
            || umber_mesh::usdc::load_usdc(&p),
        );
    }
    hunt.finish();
}

#[test]
fn usdz_garbage_and_mutated_packages_never_panic() {
    let dir = Dir::new("usdz");
    let mut hunt = Hunt::triaged("usdz", USDC_KNOWN, USDZ_ABORTS);
    let layer = usdc_frozen_0_7_0();
    let package = usdz_package(&layer);
    for (i, (case, bytes)) in byte_mutations(&package, 100).into_iter().enumerate() {
        let p = dir.write(&format!("b{i}.usdz"), &bytes);
        hunt.run(case, false, || umber_mesh::usdc::load_usdz(&p));
    }
    // A bare crate under a .usdz name: the path load_usdz actually reads
    // today (see `bug_usdz_valid_package_does_not_load`).
    for (i, (case, bytes)) in byte_mutations(&layer, 60).into_iter().enumerate() {
        let p = dir.write(&format!("c{i}.usdz"), &bytes);
        hunt.run(format!("crate-as-usdz {case}"), false, || {
            umber_mesh::usdc::load_usdz(&p)
        });
    }
    hunt.finish();
}

// ---------------------------------------------------------------- OBJ

#[test]
fn obj_mutated_quad_never_panics() {
    let dir = Dir::new("obj");
    let mut hunt = Hunt::new("obj");
    let valid = dir.write("valid.obj", OBJ_QUAD.as_bytes());
    let mesh = umber_mesh::load_obj(&valid).expect("valid fixture loads");
    assert_eq!(mesh.indices.len(), 6);
    for (i, (case, bytes)) in byte_mutations(OBJ_QUAD.as_bytes(), 200)
        .into_iter()
        .enumerate()
    {
        let p = dir.write(&format!("b{i}.obj"), &bytes);
        hunt.run(case, false, || umber_mesh::load_obj(&p));
    }
    // Text edits. `1 -> -1` is dropped: it lands in the vt/vn slots and
    // hits the relative-index underflow pinned by `bug_obj_relative_*`.
    let mut edits: Vec<(&str, &str)> = NUMERIC_EDITS
        .iter()
        .copied()
        .filter(|&(from, to)| !(from == "1" && to == "-1"))
        .collect();
    edits.extend_from_slice(&[
        ("f 1/", "f -1/"),
        ("f 1/", "f -99/"),
        ("f 1/", "f 0/"),
        ("4/4/1", "4/4/1 4/4/1 4/4/1"),
        ("f ", "f\n"),
        ("o quad", "mtllib /nonexistent/x.mtl\nusemtl m\no quad"),
        ("vn 0 0 1", "vn 0 0"),
        ("v 0 0 0", "v 0 0 0 1 1 1 1"),
    ]);
    for (i, (case, bytes)) in text_edits(OBJ_QUAD, &edits).into_iter().enumerate() {
        let p = dir.write(&format!("e{i}.obj"), &bytes);
        hunt.run(case, false, || umber_mesh::load_obj(&p));
    }
    hunt.finish();
}

// ---------------------------------------------------------------- triage

/// Triage aid, not a check: `UMBER_FUZZ_DUMP=<path>` writes the usdc
/// fixture's bytes; `UMBER_FUZZ_FILE=<path>` loads one file through the
/// dispatcher and prints the outcome (run under RUST_BACKTRACE=1 to get
/// a panic site). Used to reduce the usdc findings below.
#[test]
#[ignore = "triage tool: run with UMBER_FUZZ_DUMP / UMBER_FUZZ_FILE"]
fn triage_one_file() {
    if let Some(dump) = std::env::var_os("UMBER_FUZZ_DUMP") {
        std::fs::write(dump, usdc_bytes(USDC_TRI_TEXT)).expect("dump fixture");
    }
    if let Some(file) = std::env::var_os("UMBER_FUZZ_FILE") {
        let r = umber_mesh::load(Path::new(&file));
        eprintln!("TRIAGE {:?}", r.map(|m| m.indices.len()));
    }
}

// ---------------------------------------------------------------- reproducers
//
// Each pins ONE finding with its minimal input. Un-ignore after the fix;
// the assertion is then the typed-Err contract.

fn expect_typed_err(dir: &Dir, file: &str, bytes: &[u8]) {
    let p = dir.write(file, bytes);
    let r = umber_mesh::load(&p);
    assert!(r.is_err(), "{file}: expected a typed Err, got {r:?}");
}

/// Load `bytes` in a CHILD process (this test binary re-run on
/// `triage_one_file`): stack overflows and failed allocations ABORT, which
/// in-process would kill every other test. Asserts the child exited
/// cleanly with a typed Err.
fn expect_typed_err_in_child(dir: &Dir, file: &str, bytes: &[u8]) {
    let p = dir.write(file, bytes);
    let out = std::process::Command::new(std::env::current_exe().expect("test exe"))
        .args(["--ignored", "--exact", "triage_one_file", "--nocapture"])
        .env("UMBER_FUZZ_FILE", &p)
        .output()
        .expect("spawn child");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stderr.contains("TRIAGE Err"),
        "{file}: child {:?}, expected a typed Err:\n{}",
        out.status,
        stderr.lines().rev().take(6).collect::<Vec<_>>().join("\n")
    );
}

/// The usdc fixture with one byte inverted — the reduced shape of every
/// openusd finding (the exhaustive single-flip map in the kill report).
///
/// The bytes are FROZEN: `tests/assets/usdc_tri_openusd_0_7_0.usdc` is
/// exactly what openusd 0.7.0's `CrateWriter` wrote for [`USDC_TRI_TEXT`]
/// (869 bytes), committed as a file. The offsets below were mapped on those
/// bytes, so they must not follow the writer: openusd 0.8.0 writes the same
/// layer in 866 bytes, which would move every offset. Reading the frozen
/// 0.7.0 bytes with the 0.8.0 reader is the regression test — the same
/// corruptions that panicked or aborted 0.7.0 must now come back typed.
fn usdc_flip(offset: usize) -> Vec<u8> {
    let mut b = usdc_frozen_0_7_0();
    b[offset] ^= 0xff;
    b
}

/// The frozen 0.7.0-written crate bytes (see [`usdc_flip`]). The mutation
/// hunts seed from these too, so their case names (`flip@104`, `mutate
/// seed=10`) keep naming the same bytes across openusd writer changes.
fn usdc_frozen_0_7_0() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/assets/usdc_tri_openusd_0_7_0.usdc");
    let b = std::fs::read(&path).expect("frozen 0.7.0 usdc fixture");
    assert_eq!(b.len(), 869, "the frozen 0.7.0 fixture changed");
    b
}

#[test]
// FIXED 2026-10-15 (usdc topology pre-validation); was: BUG: usdc faceVertexIndices out of range index points unchecked (usdc.rs:220, Newell closure)
fn bug_usdc_index_out_of_range_panics() {
    let d = Dir::new("bug-usdc-oob");
    expect_typed_err(&d, "a.usdc", &usdc_mesh("3", "0, 1, 99"));
}

#[test]
// FIXED 2026-10-15 (usdc topology pre-validation); was: BUG: usdc negative faceVertexIndices overflow `as usize * 3` (usdc.rs:219, Newell closure)
fn bug_usdc_negative_index_panics() {
    let d = Dir::new("bug-usdc-neg");
    expect_typed_err(&d, "a.usdc", &usdc_mesh("3", "0, 1, -1"));
}

#[test]
// FIXED 2026-10-15 (usdc topology pre-validation); was: BUG: usdc negative faceVertexCounts entry passes the sum check, then slices OOB (usdc.rs:239)
fn bug_usdc_negative_count_panics() {
    let d = Dir::new("bug-usdc-negcount");
    expect_typed_err(&d, "a.usdc", &usdc_mesh("5, -2", "0, 1, 2"));
}

#[test]
// FIXED 2026-10-15 (usd.rs MAX_NESTING); was: BUG: usda value recursion unbounded; 20k nested '[' aborts on stack overflow (usd.rs parse_value/parse_sequence)
fn bug_usda_deep_nesting_overflows_stack() {
    let d = Dir::new("bug-usda-deep");
    let s = format!(
        "#usda 1.0\ndef Mesh \"M\"\n{{\n    int[] faceVertexCounts = {}\n}}\n",
        "[".repeat(20_000)
    );
    expect_typed_err_in_child(&d, "a.usda", s.as_bytes());
}

#[test]
// FIXED 2026-10-15 (usd.rs MAX_NESTING); was: BUG: usda prim recursion unbounded; 5k well-formed nested prims abort on stack overflow (usd.rs parse_prim)
fn bug_usda_deep_prims_overflow_stack() {
    let d = Dir::new("bug-usda-prims");
    // Well-formed nesting (no Mesh): depth alone is the trigger.
    let s = format!(
        "#usda 1.0\n{}{}",
        "def X \"A\" {\n".repeat(5_000),
        "}\n".repeat(5_000)
    );
    expect_typed_err_in_child(&d, "a.usda", s.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF accessor count 0 underflows `count - 1` (gltf 1.4.1 accessor/util.rs:384)
fn bug_gltf_accessor_count_zero_panics() {
    let d = Dir::new("bug-gltf-c0");
    let json = GltfTri {
        count: "0",
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF accessor count 2^62 overflows `stride * (count - 1)` (gltf 1.4.1 accessor/util.rs:384)
fn bug_gltf_accessor_huge_count_panics() {
    let d = Dir::new("bug-gltf-huge");
    let json = GltfTri {
        count: "4611686018427387904",
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF byteStride below element size trips a debug_assert (gltf 1.4.1 accessor/util.rs:376)
fn bug_gltf_short_stride_panics() {
    let d = Dir::new("bug-gltf-stride");
    let json = GltfTri {
        stride: r#", "byteStride": 4"#,
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF POSITION accessor not f32 VEC3 trips debug_assert_eq (gltf 1.4.1 accessor/util.rs:371)
fn bug_gltf_position_wrong_type_panics() {
    let d = Dir::new("bug-gltf-postype");
    for (i, (pc, pt)) in [("5121", "SCALAR"), ("5126", "VEC2")]
        .into_iter()
        .enumerate()
    {
        let json = GltfTri {
            pos_component: pc,
            pos_type: pt,
            ..Default::default()
        }
        .json();
        expect_typed_err(&d, &format!("a{i}.gltf"), json.as_bytes());
    }
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF index componentType FLOAT/BYTE hits unreachable!() (gltf 1.4.1 mesh/mod.rs:396)
fn bug_gltf_index_float_component_panics() {
    let d = Dir::new("bug-gltf-idxtype");
    for (i, ic) in ["5126", "5120"].into_iter().enumerate() {
        let json = GltfTri {
            index_component: ic,
            ..Default::default()
        }
        .json();
        expect_typed_err(&d, &format!("a{i}.gltf"), json.as_bytes());
    }
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF TEXCOORD_0 on a u32 accessor hits unreachable!() (gltf 1.4.1 mesh/mod.rs:430)
fn bug_gltf_texcoord_u32_panics() {
    let d = Dir::new("bug-gltf-uvtype");
    let json = GltfTri {
        extra_attr: r#", "TEXCOORD_0": 1"#,
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf validate_accessors); was: BUG: glTF NORMAL on a u32 SCALAR accessor trips debug_assert_eq (gltf 1.4.1 accessor/util.rs:371)
fn bug_gltf_normal_wrong_type_panics() {
    let d = Dir::new("bug-gltf-nrmtype");
    let json = GltfTri {
        extra_attr: r#", "NORMAL": 1"#,
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf checked index offset); was: BUG: glTF index u32::MAX plus a primitive base overflows `i + base` (gltf.rs:70)
fn bug_gltf_index_plus_base_overflows() {
    let d = Dir::new("bug-gltf-base");
    let json = GltfTri {
        indices: [0, 1, u32::MAX],
        two_prims: true,
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (gltf index range check); was: BUG: glTF indices never range-checked; index 7777 on 3 verts returns Ok (gltf.rs:70)
fn bug_gltf_index_out_of_range_is_accepted() {
    // Not a panic in the importer: a dishonest Ok that defers the crash
    // to the first consumer indexing `positions[7777]`.
    let d = Dir::new("bug-gltf-ioob");
    let json = GltfTri {
        indices: [0, 1, 7777],
        ..Default::default()
    }
    .json();
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (load() panic boundary); was: BUG: OBJ relative vt index past the start underflows, `vt * 2` overflows (tobj 4.0.5 lib.rs:852)
fn bug_obj_relative_texcoord_underflow_panics() {
    let d = Dir::new("bug-obj-vt");
    expect_typed_err(
        &d,
        "a.obj",
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf 1/-5 2/1 3/1\n",
    );
}

#[test]
// FIXED 2026-10-15 (load() panic boundary); was: BUG: OBJ relative vn index past the start underflows, `vn * 3` overflows (tobj 4.0.5 lib.rs:860)
fn bug_obj_relative_normal_underflow_panics() {
    let d = Dir::new("bug-obj-vn");
    expect_typed_err(
        &d,
        "a.obj",
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1//-5 2//1 3//1\n",
    );
}

#[test]
// FIXED 2026-10-15 (load_usdz via usdz::Archive); was: BUG: load_usdz opens the zip as a raw crate (usdc::read_file), so a VALID .usdz never loads (usdc.rs:41)
fn bug_usdz_valid_package_does_not_load() {
    let d = Dir::new("bug-usdz-valid");
    let p = d.write("a.usdz", &usdz_package(&usdc_bytes(USDC_TRI_TEXT)));
    let mesh = umber_mesh::usdc::load_usdz(&p).expect("a valid usdz package loads");
    assert_eq!(mesh.indices, vec![0, 1, 2]);
}

#[test]
// FIXED 2026-10-15 (load() panic boundary); was: BUG: glTF POSITION naming a missing accessor indexes root.accessors unchecked (gltf-json 1.4.1 mesh.rs:151)
fn bug_gltf_position_dangling_accessor_panics() {
    let d = Dir::new("bug-gltf-dangling");
    let json = GltfTri::default()
        .json()
        .replace(r#""POSITION": 0"#, r#""POSITION": 7"#);
    expect_typed_err(&d, "a.gltf", json.as_bytes());
}

#[test]
// FIXED 2026-10-15 (openusd 0.8.0, mxpv/openusd#138: read_vec checks the
// count against the remaining bytes before allocating); was: BUG: openusd 0.7.0 sizes Vec allocs from unchecked crate counts; multi-TiB alloc ABORTS (usdc/reader.rs read_vec, read_compressed)
fn bug_usdc_huge_count_allocation_aborts() {
    // 36: read_vec::<u32>; 104: read_vec::<Vec3f> (points);
    // 188: read_compressed::<u8>; 389: read_compressed::<u64>.
    let d = Dir::new("bug-usdc-abort");
    for off in [36, 104, 188, 389] {
        expect_typed_err_in_child(&d, &format!("flip{off}.usdc"), &usdc_flip(off));
    }
}

#[test]
// FIXED in openusd 0.8.0 (mxpv/openusd#138; was also caught by load()'s panic boundary); was: BUG: openusd 0.7.0 read_vec count*size overflows: capacity overflow panic (usdc/reader.rs ReadExt::read_vec)
fn bug_openusd_read_vec_capacity_overflow() {
    let d = Dir::new("bug-usdc-readvec");
    for off in [39, 75, 107] {
        expect_typed_err(&d, &format!("flip{off}.usdc"), &usdc_flip(off));
    }
}

#[test]
// FIXED in openusd 0.8.0 (mxpv/openusd#138; was also caught by load()'s panic boundary); was: BUG: openusd 0.7.0 read_compressed estimated size overflows: capacity overflow panic (usdc/reader.rs read_compressed)
fn bug_openusd_read_compressed_capacity_overflow() {
    let d = Dir::new("bug-usdc-readcomp");
    for off in [191, 392] {
        expect_typed_err(&d, &format!("flip{off}.usdc"), &usdc_flip(off));
    }
}

#[test]
// FIXED in openusd 0.8.0 (mxpv/openusd#138; was also caught by load()'s panic boundary); was: BUG: openusd 0.7.0 integer decoding slices &data[offset..] unchecked (usdc/coding.rs:52)
fn bug_openusd_decode_ints_slice_oob() {
    let d = Dir::new("bug-usdc-coding52");
    expect_typed_err(&d, "a.usdc", &usdc_flip(351));
}

#[test]
// FIXED in openusd 0.8.0 (mxpv/openusd#138; was also caught by load()'s panic boundary); was: BUG: openusd 0.7.0 encoded_buffer_size `count * 2` overflows (usdc/coding.rs:22)
fn bug_openusd_encoded_buffer_size_overflow() {
    let d = Dir::new("bug-usdc-coding22");
    expect_typed_err(&d, "a.usdc", &usdc_flip(358));
}

#[test]
// FIXED in openusd 0.8.0 (mxpv/openusd#138; was also caught by load()'s panic boundary); was: BUG: openusd 0.7.0 lz4 chunk count != 0 hits todo!() (usdc/reader.rs:1562)
fn bug_openusd_lz4_chunked_todo() {
    let d = Dir::new("bug-usdc-lz4");
    expect_typed_err(&d, "a.usdc", &usdc_flip(200));
}

#[test]
// FIXED in openusd 0.8.0 (mxpv/openusd#138; was also caught by load()'s panic boundary); was: BUG: openusd 0.7.0 compressed-flag value trips debug_assert!(!is_compressed) (usdc/reader.rs:656)
fn bug_openusd_compressed_value_debug_assert() {
    let d = Dir::new("bug-usdc-656");
    expect_typed_err(&d, "a.usdc", &usdc_flip(408));
}
