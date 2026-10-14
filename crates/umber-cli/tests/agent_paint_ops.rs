//! The agent paint steps (docs/specs/agent-paint-ops-design.md) over the
//! process boundary: `paint_stroke`, `paint_read`, `paint_undo`,
//! `paint_redo`, and `project_save` / `project_open` carrying the paint
//! sidecar — the same engine and persistence the GUI uses.
//!
//! GPU rule (the bake test's): a machine with no paint-capable adapter
//! answers the first paint step with a typed error naming it — that is
//! the ONLY accepted skip; any other failure fails the test. The
//! no-paint tests need no GPU at all.
//!
//! Exactness rule: dab-edge coverage is implementation-defined (the
//! Gate-0 journey test's rule), so painted texels pin COLOR (R = B = 0
//! under a pure-green brush — 0 × any coverage is exact) and presence
//! (A > 0), never an exact alpha. Untouched texels and undo restores are
//! byte-exact.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use umber_core::{
    LayerKind, LayerStack, ProjectModel, ProjectSettings, TextureSet, TextureSetLayers,
};

/// A step's reply deadline: generous — the first paint step requests
/// the device (cold lavapipe/driver init) and 4K readbacks on a CPU
/// rasterizer take seconds.
const STEP_TIMEOUT: Duration = Duration::from_secs(180);
/// The exit deadline after quit/EOF.
const EXIT_TIMEOUT: Duration = Duration::from_secs(60);

/// A spawned `umber-cli agent`. Stdout is read on a thread into a
/// channel so every wait has a deadline.
struct Agent {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Agent {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_umber-cli"))
            .arg("agent")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn umber-cli agent");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Self {
            child,
            stdin,
            lines,
        }
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{line}").expect("write step");
        stdin.flush().expect("flush step");
    }

    /// The next reply line, parsed. It must be JSON.
    fn recv(&self, timeout: Duration) -> Value {
        let line = self
            .lines
            .recv_timeout(timeout)
            .unwrap_or_else(|e| panic!("no reply within {timeout:?}: {e}"));
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"))
    }

    /// Sends one step and returns its reply.
    fn call(&mut self, step: &Value) -> Value {
        self.send_raw(&step.to_string());
        self.recv(STEP_TIMEOUT)
    }

    /// Closes stdin (EOF).
    fn close_stdin(&mut self) {
        drop(self.stdin.take());
    }

    /// Waits for the exit (killing the child past the deadline), then
    /// asserts stdout carried nothing after the last reply.
    fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + EXIT_TIMEOUT;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                break status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("agent did not exit within {EXIT_TIMEOUT:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        match self.lines.recv_timeout(STEP_TIMEOUT) {
            Err(RecvTimeoutError::Disconnected) => {}
            Ok(line) => panic!("stray stdout after the last reply: {line}"),
            Err(RecvTimeoutError::Timeout) => panic!("stdout never closed"),
        }
        status
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A fresh scratch dir per test.
fn scratch(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("umber-cli-paint-ops-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn assert_ok(reply: &Value, step: &str) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["step"], step, "{reply}");
}

/// A 512² set (small planes keep CPU-rasterizer readbacks fast): one
/// paint layer `Paint` (id 0) and one fill `Fill` (id 1).
fn fixture_project() -> ProjectModel {
    let mut body = TextureSet::new_default("Body");
    body.resolution = 512;
    let mut stack = LayerStack::new();
    stack.add_layer("Paint", LayerKind::Paint);
    stack.add_layer("Fill", LayerKind::Fill);
    ProjectModel::new(
        vec![body],
        vec![TextureSetLayers {
            texture_set: "Body".into(),
            stack,
        }],
        ProjectSettings {
            active_texture_set: Some("Body".into()),
            ..ProjectSettings::default()
        },
    )
}

fn write_fixture_project(tag: &str) -> PathBuf {
    let dir = scratch(&format!("proj-{tag}"));
    umber_core::save_to_dir(&fixture_project(), &dir).expect("fixture saves");
    dir
}

/// Whether `reply` is the accepted no-GPU skip (see the module docs).
fn is_gpu_skip(reply: &Value) -> bool {
    reply["ok"] == false
        && reply["error"].as_str().is_some_and(|e| {
            e.contains("no suitable GPU adapter") || e.contains("lacks read-write storage textures")
        })
}

/// A short diagonal stroke through UV (0.5, 0.5): texel (256, 255) on a
/// 512² plane (v flips: row = (1 − 0.5)·512 − ½ → the centre rows).
fn green_stroke(layer: Option<u64>) -> Value {
    let points: Vec<[f32; 2]> = (0..=8).map(|i| {
        let t = i as f32 * 0.005;
        [0.48 + t, 0.48 + t]
    }).collect();
    let mut body = json!({ "points": points, "color": [0.0, 1.0, 0.0, 1.0] });
    if let Some(id) = layer {
        body["layer"] = json!(id);
    }
    json!({ "paint_stroke": body })
}

/// The centre texel and a far corner texel (the stroke never reaches it).
const CENTRE: [u32; 2] = [256, 256];
const FAR: [u32; 2] = [8, 8];

fn read(agent: &mut Agent, layer: Option<u64>, composite: bool) -> Value {
    let mut body = json!({ "texels": [CENTRE, FAR], "composite": composite });
    if let Some(id) = layer {
        body["layer"] = json!(id);
    }
    agent.call(&json!({ "paint_read": body }))
}

/// Tests 1 + 3: a green stroke on Base lands (color-exact, present), a
/// far texel stays exactly transparent; undo restores the sampled texels
/// byte-exact and redo brings the stroke back byte-exact.
#[test]
fn stroke_reads_back_and_undo_redo_are_exact() {
    let dir = write_fixture_project("base");
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "project_open": { "dir": dir } })), "project_open");

    let stroke = agent.call(&green_stroke(None));
    if is_gpu_skip(&stroke) {
        eprintln!("skipping: {stroke}");
        return;
    }
    assert_ok(&stroke, "paint_stroke");
    assert_eq!(stroke["tiles"], json!([1001]), "{stroke}");
    assert_eq!(stroke["can_undo_stroke"], true, "{stroke}");

    let after = read(&mut agent, None, false);
    assert_ok(&after, "paint_read");
    assert_eq!((after["width"].clone(), after["height"].clone()), (json!(512), json!(512)));
    let centre = &after["texels"][0];
    assert_eq!(centre[0], 0, "R stays exactly 0 under green: {after}");
    assert_eq!(centre[2], 0, "B stays exactly 0 under green: {after}");
    assert!(centre[1].as_u64().unwrap_or(0) > 0, "G painted: {after}");
    assert!(centre[3].as_u64().unwrap_or(0) > 0, "A painted: {after}");
    assert_eq!(after["texels"][1], json!([0, 0, 0, 0]), "far texel untouched: {after}");

    let undo = agent.call(&json!("paint_undo"));
    assert_ok(&undo, "paint_undo");
    assert_eq!(undo["undone"], true, "{undo}");
    let undone = read(&mut agent, None, false);
    assert_eq!(undone["texels"], json!([[0, 0, 0, 0], [0, 0, 0, 0]]), "{undone}");

    let redo = agent.call(&json!("paint_redo"));
    assert_ok(&redo, "paint_redo");
    assert_eq!(redo["redone"], true, "{redo}");
    let redone = read(&mut agent, None, false);
    assert_eq!(redone["texels"], after["texels"], "redo is byte-exact: {redone}");

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 2: a stroke on the paint layer lands in THAT plane only; Base is
/// untouched, and the composite shows it.
#[test]
fn stroke_on_a_layer_routes_to_its_own_plane() {
    let dir = write_fixture_project("layer");
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "project_open": { "dir": dir } })), "project_open");

    let stroke = agent.call(&green_stroke(Some(0)));
    if is_gpu_skip(&stroke) {
        eprintln!("skipping: {stroke}");
        return;
    }
    assert_ok(&stroke, "paint_stroke");

    let layer = read(&mut agent, Some(0), false);
    assert_ok(&layer, "paint_read");
    assert_eq!(layer["exists"], true, "{layer}");
    assert!(layer["texels"][0][3].as_u64().unwrap_or(0) > 0, "{layer}");

    let base = read(&mut agent, None, false);
    assert_ok(&base, "paint_read");
    assert_eq!(base["texels"][0], json!([0, 0, 0, 0]), "Base untouched: {base}");

    let composite = read(&mut agent, None, true);
    assert_ok(&composite, "paint_read");
    assert_eq!(composite["composite"], true, "{composite}");
    assert!(composite["texels"][0][3].as_u64().unwrap_or(0) > 0, "{composite}");

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 4: save → a FRESH process opens it → the stroke reads back
/// byte-identical (the sidecar round trip through the shared
/// persistence), and the reopen reports the restored planes.
#[test]
fn saved_paint_survives_a_fresh_session() {
    let dir = write_fixture_project("save");
    let saved_dir = scratch("save-out");
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "project_open": { "dir": dir } })), "project_open");
    let stroke = agent.call(&green_stroke(Some(0)));
    if is_gpu_skip(&stroke) {
        eprintln!("skipping: {stroke}");
        return;
    }
    assert_ok(&stroke, "paint_stroke");
    let before = read(&mut agent, Some(0), false);
    let save = agent.call(&json!({ "project_save": { "dir": saved_dir } }));
    assert_ok(&save, "project_save");
    assert!(save["painted"].as_u64().unwrap_or(0) >= 1, "{save}");
    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());

    let mut again = Agent::spawn();
    let open = again.call(&json!({ "project_open": { "dir": saved_dir } }));
    assert_ok(&open, "project_open");
    assert!(open["painted"].as_u64().unwrap_or(0) >= 1, "{open}");
    // A paint layer owns a plane: the save wrote the per-layer format.
    assert_eq!(open["version"], 2, "{open}");
    let after = read(&mut again, Some(0), false);
    assert_eq!(after["texels"], before["texels"], "byte-identical reopen: {after}");
    assert_ok(&again.call(&json!("quit")), "quit");
    assert!(again.wait_exit().success());
}

/// Test 5: a stroke on a Fill layer is a typed refusal naming it; the
/// session survives (the next step still answers).
#[test]
fn stroke_on_a_fill_layer_refuses_typed() {
    let dir = write_fixture_project("fill");
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "project_open": { "dir": dir } })), "project_open");
    let refused = agent.call(&green_stroke(Some(1)));
    if is_gpu_skip(&refused) {
        eprintln!("skipping: {refused}");
        return;
    }
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["step"], "paint_stroke", "{refused}");
    let error = refused["error"].as_str().unwrap_or_default();
    assert!(error.contains("paint refused"), "{refused}");
    // Nothing recorded, and the session lives.
    let undo = agent.call(&json!("paint_undo"));
    assert_ok(&undo, "paint_undo");
    assert_eq!(undo["undone"], false, "{undo}");
    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 6: input validation is typed and GPU-free — too many texels, an
/// empty stroke, an out-of-range color, an unknown layer.
#[test]
fn paint_inputs_are_validated_before_any_gpu_work() {
    let dir = write_fixture_project("validate");
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "project_open": { "dir": dir } })), "project_open");

    let many: Vec<[u32; 2]> = (0..65).map(|i| [i, 0]).collect();
    let too_many = agent.call(&json!({ "paint_read": { "texels": many } }));
    assert_eq!(too_many["ok"], false, "{too_many}");
    assert!(too_many["error"].as_str().unwrap_or_default().contains("1..=64"), "{too_many}");

    let empty = agent.call(&json!({ "paint_stroke": { "points": [], "color": [0, 1, 0, 1] } }));
    assert_eq!(empty["ok"], false, "{empty}");
    assert!(empty["error"].as_str().unwrap_or_default().contains("at least one"), "{empty}");

    let hot = agent.call(&json!({ "paint_stroke": { "points": [[0.5, 0.5]], "color": [0, 2, 0, 1] } }));
    assert_eq!(hot["ok"], false, "{hot}");
    assert!(hot["error"].as_str().unwrap_or_default().contains("0..=1"), "{hot}");

    let ghost = agent.call(&json!({ "paint_stroke": {
        "layer": 99, "points": [[0.5, 0.5]], "color": [0, 1, 0, 1] } }));
    assert_eq!(ghost["ok"], false, "{ghost}");
    assert!(ghost["error"].as_str().unwrap_or_default().contains("no layer 99"), "{ghost}");

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 7: a session that never paints saves exactly as before —
/// `painted: 0`, no `paint/` sidecar — and its `paint_undo` is a no-op
/// that never requests a device.
#[test]
fn a_session_that_never_paints_writes_no_sidecar() {
    let dir = write_fixture_project("nopaint");
    let out = scratch("nopaint-out");
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "project_open": { "dir": dir } })), "project_open");
    let undo = agent.call(&json!("paint_undo"));
    assert_ok(&undo, "paint_undo");
    assert_eq!(undo["undone"], false, "{undo}");
    let save = agent.call(&json!({ "project_save": { "dir": out } }));
    assert_ok(&save, "project_save");
    assert_eq!(save["painted"], 0, "{save}");
    assert!(!out.join("paint").exists(), "no sidecar for a paint-free session");
    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}
