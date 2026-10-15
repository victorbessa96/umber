//! `bake-all --margin` and the agent `bake` step's `margin` field end to
//! end through the built binary: the Adjacent Faces bake margin
//! (umber-bake's `margin` module) wired into the headless bake paths.
//!
//! GPU-gated: a run that fails because this host has no usable wgpu
//! adapter prints `skipping:` and returns (the CLI tests' house rule,
//! see `agent_paint_ops.rs`'s `is_gpu_skip`). Comparisons pin RELATIONS
//! between runs on the same adapter, never golden bytes (GPU output is
//! adapter-flaky across lavapipe/WARP/hardware).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Writes the seamed fixture into `dir`: margin.rs's `seamed_quad` as an
/// OBJ. 3D quad A(0,0,0) B(1,0,0) C(1,1,0) D(0,1,0), triangles (A,B,C)
/// and (A,C,D) sharing 3D edge A-C, which is a UV cut: island A's seam on
/// u = 0.375 (body to the left), island B's on u = 0.625 (body to the
/// right), with an empty gap between. At 64² the gap is texel columns
/// 24..40, so `--dilate 4` gives Adjacent Faces a 4-texel strip on each
/// side to fill from the OTHER island's interior, where Extend repeats
/// the island's own border. The position map varies across the quad, so
/// the two fills differ there.
///
/// Not `umber-mesh/tests/assets/cube_b7400.fbx`: that fixture bakes to
/// zero coverage (every texel alpha 0 in all six maps at 64², observed
/// while writing this test), so no margin has anything to fill.
fn seamed_quad(dir: &Path) -> PathBuf {
    let path = dir.join("seamed_quad.obj");
    std::fs::write(
        &path,
        "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
         vt 0.375 0.875\nvt 0 0.125\nvt 0.375 0.125\n\
         vt 0.625 0.875\nvt 0.625 0.125\nvt 1 0.875\n\
         vn 0 0 1\n\
         f 1/1/1 2/2/1 3/3/1\nf 1/4/1 3/5/1 4/6/1\n",
    )
    .expect("write seamed_quad.obj");
    path
}

/// A fresh scratch dir per test tag (tests share one process).
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umber-cli-margin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The no-adapter failure shapes (`bake_context`'s message, and the
/// storage-texture capability gap some software adapters report).
fn is_gpu_skip(stderr: &str) -> bool {
    stderr.contains("no suitable GPU adapter")
        || stderr.contains("lacks read-write storage textures")
}

/// Runs `bake-all <mesh> <out> <extra...>`. `None` = GPU skip; panics on
/// any other failure.
fn bake_all(mesh: &Path, out: &Path, extra: &[&str]) -> Option<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_umber-cli"))
        .arg("bake-all")
        .arg(mesh)
        .arg(out)
        .args(extra)
        .output()
        .expect("spawn umber-cli");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        if is_gpu_skip(&stderr) {
            eprintln!("skipping: {}", stderr.trim());
            return None;
        }
        panic!("bake-all {extra:?} failed: {stderr}");
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The written PNGs, sorted by file name, with their bytes.
fn pngs(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("read out dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("png"))
        .map(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            let bytes = std::fs::read(&p).expect("read png");
            (name, bytes)
        })
        .collect();
    files.sort();
    files
}

#[test]
fn adjacent_margin_bakes_every_map_and_differs_from_extend() {
    let flags = ["--dilate", "4", "--size", "64", "--rays", "4"];
    let mesh_dir = scratch("mesh");
    let mesh = seamed_quad(&mesh_dir);
    let adj_dir = scratch("adjacent");
    let Some(stdout) = bake_all(
        &mesh,
        &adj_dir,
        &[&flags[..], &["--margin", "adjacent"]].concat(),
    ) else {
        return;
    };
    assert!(stdout.contains("adjacent-faces margin"), "{stdout}");
    assert!(stdout.contains("bake-all complete"), "{stdout}");
    let adjacent = pngs(&adj_dir);
    // bake-all's six maps (BakeMap::ALL), each a real PNG.
    assert_eq!(adjacent.len(), 6, "{adjacent:?}");
    for (name, bytes) in &adjacent {
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{name} is not a PNG"
        );
    }

    let ext_dir = scratch("extend");
    let Some(stdout) = bake_all(
        &mesh,
        &ext_dir,
        &[&flags[..], &["--margin", "extend"]].concat(),
    ) else {
        return;
    };
    assert!(!stdout.contains("adjacent-faces margin"), "{stdout}");
    let extend = pngs(&ext_dir);
    let names = |v: &[(String, Vec<u8>)]| v.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    assert_eq!(names(&adjacent), names(&extend));

    // Absent --margin is Extend: byte-identical files on the same adapter
    // (the bake is deterministic per adapter, and the PNG encoder is a
    // pure function of the pixels).
    let def_dir = scratch("default");
    bake_all(&mesh, &def_dir, &flags).expect("the adapter was there a moment ago");
    assert_eq!(pngs(&def_dir), extend, "absent --margin must equal extend");

    // Adjacent Faces fills the seam strips from the neighbouring face's
    // interior, where Extend repeats the island border: at least one map
    // must differ. Identical pixels encode to identical bytes, so a byte
    // difference is a pixel difference.
    let differing: Vec<&str> = adjacent
        .iter()
        .zip(&extend)
        .filter(|((_, a), (_, e))| a != e)
        .map(|((n, _), _)| n.as_str())
        .collect();
    assert!(
        !differing.is_empty(),
        "adjacent and extend wrote identical maps for a seamed quad"
    );
    eprintln!("maps differing adjacent vs extend: {differing:?}");

    // The position map varies across the seam: it must be among them.
    assert!(
        differing.iter().any(|n| n.ends_with("_position.png")),
        "{differing:?}"
    );

    for dir in [mesh_dir, adj_dir, ext_dir, def_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn bad_margin_value_fails_before_any_bake() {
    let out = scratch("bad");
    let mesh = seamed_quad(&out);
    let output = Command::new(env!("CARGO_BIN_EXE_umber-cli"))
        .arg("bake-all")
        .arg(&mesh)
        .arg(&out)
        .args(["--margin", "bilinear"])
        .output()
        .expect("spawn umber-cli");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--margin") && stderr.contains("bilinear"),
        "{stderr}"
    );
    // Parsing fails before run_bake creates the out dir's contents.
    assert_eq!(pngs(&out).len(), 0);
    let _ = std::fs::remove_dir_all(out);
}

/// Runs one `umber-cli agent` session: `load` the mesh, `bake` with
/// `bake` (the step's fields; `out_dir` is added here), then poll
/// `bake_status` to a final state and `quit`. `None` = GPU skip (the
/// bake failed on a missing adapter); panics on any other failure.
fn agent_bake(mesh: &Path, out: &Path, mut bake: serde_json::Value) -> Option<()> {
    bake["out_dir"] = serde_json::json!(out);
    let mut child = Command::new(env!("CARGO_BIN_EXE_umber-cli"))
        .arg("agent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn umber-cli agent");
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut call = |step: serde_json::Value| -> serde_json::Value {
        writeln!(stdin, "{step}").expect("write step");
        stdin.flush().expect("flush step");
        let mut line = String::new();
        stdout.read_line(&mut line).expect("read reply");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("reply is not JSON ({e}): {line}"))
    };
    let load = call(serde_json::json!({ "load": { "mesh": mesh } }));
    assert_eq!(load["ok"], true, "{load}");
    let started = call(serde_json::json!({ "bake": bake }));
    assert_eq!(started["ok"], true, "{started}");
    let deadline = Instant::now() + Duration::from_secs(600);
    let last = loop {
        let status = call(serde_json::json!("bake_status"));
        assert_eq!(status["ok"], true, "{status}");
        if status["status"] == "done" || status["status"] == "failed" {
            break status;
        }
        assert!(Instant::now() < deadline, "bake never finished: {status}");
        std::thread::sleep(Duration::from_millis(100));
    };
    let quit = call(serde_json::json!("quit"));
    assert_eq!(quit["ok"], true, "{quit}");
    assert!(child.wait().expect("agent exits").success());
    if last["status"] == "failed" {
        let error = last["error"].as_str().unwrap_or_default();
        if is_gpu_skip(error) {
            eprintln!("skipping: {error}");
            return None;
        }
        panic!("agent bake failed: {last}");
    }
    Some(())
}

#[test]
fn agent_bake_margin_adjacent_differs_and_absent_is_extend() {
    let base = serde_json::json!({ "dilate": 4, "size": 64, "rays": 4 });
    let with = |margin: &str| {
        let mut step = base.clone();
        step["margin"] = serde_json::json!(margin);
        step
    };
    let mesh_dir = scratch("agent-mesh");
    let mesh = seamed_quad(&mesh_dir);
    let adj_dir = scratch("agent-adjacent");
    if agent_bake(&mesh, &adj_dir, with("adjacent")).is_none() {
        return;
    }
    let adjacent = pngs(&adj_dir);
    assert_eq!(adjacent.len(), 6, "{adjacent:?}");

    let ext_dir = scratch("agent-extend");
    agent_bake(&mesh, &ext_dir, with("extend")).expect("the adapter was there a moment ago");
    let extend = pngs(&ext_dir);
    let def_dir = scratch("agent-default");
    agent_bake(&mesh, &def_dir, base.clone()).expect("the adapter was there a moment ago");
    // Absent `margin` is Extend, byte for byte (same adapter, same path).
    assert_eq!(pngs(&def_dir), extend, "absent margin must equal extend");

    let differing: Vec<&str> = adjacent
        .iter()
        .zip(&extend)
        .filter(|((_, a), (_, e))| a != e)
        .map(|((n, _), _)| n.as_str())
        .collect();
    eprintln!("agent maps differing adjacent vs extend: {differing:?}");
    assert!(
        differing.iter().any(|n| n.ends_with("_position.png")),
        "adjacent and extend must differ on the position map: {differing:?}"
    );

    // The agent writes the same files `bake-all` does for the same
    // settings: one bake path, two front ends.
    let cli_dir = scratch("agent-cli-adjacent");
    bake_all(
        &mesh,
        &cli_dir,
        &[
            "--dilate", "4", "--size", "64", "--rays", "4", "--margin", "adjacent",
        ],
    )
    .expect("the adapter was there a moment ago");
    assert_eq!(pngs(&cli_dir), adjacent, "agent and bake-all must agree");

    for dir in [mesh_dir, adj_dir, ext_dir, def_dir, cli_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
