//! The `agent` control-protocol v0 surface (project + paint reads + the
//! save seam) over the process boundary. Each test spawns the real
//! binary with piped stdio, the way a harness does — the same shape as
//! `tests/agent.rs`, with its own slim driver (one harness per
//! integration target).
//!
//! Every stdout line must parse as JSON, and nothing may follow the
//! last reply: that is the stdout-discipline check. No test here needs
//! a GPU adapter (the project steps are CPU-only), so the waits are the
//! same generous deadlines as the loop tests.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use umber_core::{
    BlendMode, Channel, ChannelKind, LayerKind, LayerStack, ProjectModel, ProjectSettings,
    TextureSet, TextureSetLayers,
};

/// A non-GPU step's reply deadline.
const STEP_TIMEOUT: Duration = Duration::from_secs(60);
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
    let dir = std::env::temp_dir().join(format!("umber-cli-project-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn assert_ok(reply: &Value, step: &str) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["step"], step, "{reply}");
}

/// A unit quad OBJ (UVs over [0,1], +Z normal), the same fixture the
/// batch tests write.
fn write_quad_obj(dir: &Path) -> PathBuf {
    let path = dir.join("quad.obj");
    std::fs::write(
        &path,
        "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
         vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
         vn 0 0 1\n\
         f 1/1/1 2/2/1 3/3/1\nf 1/1/1 3/3/1 4/4/1\n",
    )
    .expect("write quad.obj");
    path
}

/// Two sets, three layers, exact opacity/blend/visibility (0.5 is exact
/// in binary, so the wire round trip is bit-exact, not approximate).
fn fixture_project() -> ProjectModel {
    let body = TextureSet::new_default("Body");
    let helmet = TextureSet {
        name: "Helmet".to_string(),
        resolution: 4096,
        channels: vec![Channel {
            name: "baseColor".into(),
            kind: ChannelKind::Color,
        }],
        tiles: Default::default(),
    };
    let mut stack = LayerStack::new();
    let base = stack.add_layer("Base", LayerKind::Paint);
    stack.set_opacity(base, 0.5);
    stack.set_blend_mode(base, BlendMode::Multiply);
    let top = stack.add_layer("Top", LayerKind::Fill);
    stack.set_visible(top, false);
    let mut helmet_stack = LayerStack::new();
    helmet_stack.add_layer("Metal", LayerKind::Paint);
    ProjectModel::new(
        vec![body, helmet],
        vec![
            TextureSetLayers {
                texture_set: "Body".into(),
                stack,
            },
            TextureSetLayers {
                texture_set: "Helmet".into(),
                stack: helmet_stack,
            },
        ],
        ProjectSettings {
            active_texture_set: Some("Body".into()),
            ..ProjectSettings::default()
        },
    )
}

/// A temp-dir project built via `save_to_dir`, plus the paint sidecar's
/// size model (a set size plus a per-channel override).
fn write_fixture_project(tag: &str) -> PathBuf {
    let dir = scratch(&format!("proj-{tag}"));
    umber_core::save_to_dir(&fixture_project(), &dir).expect("fixture saves");
    std::fs::create_dir_all(dir.join("paint")).expect("paint dir");
    std::fs::write(
        dir.join("paint").join("target_sizes.txt"),
        "# Umber paint-set sizes v1\nBody=1024x1024\nchannel/Body/baseColor=512x512\n",
    )
    .expect("sidecar");
    dir
}

/// Test 1: `project_open` reads back the saved census exactly, and
/// `project_state` carries the same sets.
#[test]
fn project_open_reads_back_census_exact() {
    let dir = write_fixture_project("open");
    let mut agent = Agent::spawn();

    let open = agent.call(&json!({ "project_open": { "dir": dir } }));
    assert_ok(&open, "project_open");
    assert_eq!(open["index"], 0);
    assert_eq!(open["version"], 1, "{open}");
    assert_eq!(open["active_set"], "Body", "{open}");
    assert_eq!(open["journal_seq"], 0, "{open}");
    let sets = open["texture_sets"].as_array().expect("sets array");
    assert_eq!(sets.len(), 2, "{open}");
    assert_eq!(sets[0]["name"], "Body", "{open}");
    assert_eq!(sets[0]["tiles"], json!([1001]), "{open}");
    assert_eq!(sets[0]["channels"][0]["name"], "baseColor", "{open}");
    assert_eq!(sets[0]["channels"][0]["width"], 512, "{open}");
    assert_eq!(sets[0]["channels"][1]["width"], 1024, "{open}");
    assert_eq!(sets[0]["layers"][0]["name"], "Base", "{open}");
    assert_eq!(sets[0]["layers"][0]["blend"], "Multiply", "{open}");
    assert_eq!(sets[1]["name"], "Helmet", "{open}");
    assert_eq!(sets[1]["layers"][0]["name"], "Metal", "{open}");

    let state = agent.call(&json!("project_state"));
    assert_ok(&state, "project_state");
    assert_eq!(
        state["project"]["texture_sets"], open["texture_sets"],
        "{state}"
    );
    assert_eq!(state["project"]["active_set"], "Body", "{state}");

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 2: an empty session is a state, not an error (`project: null`),
/// and opening afterwards transitions to the census.
#[test]
fn project_state_empty_session_is_null() {
    let dir = write_fixture_project("null");
    let mut agent = Agent::spawn();

    let empty = agent.call(&json!("project_state"));
    assert_ok(&empty, "project_state");
    assert_eq!(empty["project"], Value::Null, "{empty}");

    let open = agent.call(&json!({ "project_open": { "dir": dir } }));
    assert_ok(&open, "project_open");
    let state = agent.call(&json!({ "project_state": {} }));
    assert_ok(&state, "project_state");
    assert_eq!(state["project"]["active_set"], "Body", "{state}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 3: the typed load failure verbatim (an unknown directory), and
/// the session survives it with no project held.
#[test]
fn unknown_project_dir_is_a_typed_error() {
    let dir = scratch("missing");
    let mut agent = Agent::spawn();

    let missing = agent.call(&json!({ "project_open": { "dir": dir.join("nope.umber") } }));
    assert_eq!(missing["ok"], false, "{missing}");
    assert_eq!(missing["step"], "project_open", "{missing}");
    assert!(
        missing["error"].as_str().unwrap().contains("I/O error"),
        "{missing}"
    );

    // Still alive, and still holding nothing.
    let state = agent.call(&json!("project_state"));
    assert_ok(&state, "project_state");
    assert_eq!(state["project"], Value::Null, "{state}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 4: `set_active` switches the working set; an unknown name is a
/// typed error that switches nothing.
#[test]
fn set_active_ok_and_unknown_names() {
    let dir = write_fixture_project("active");
    let mut agent = Agent::spawn();
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": dir } })),
        "project_open",
    );

    let switch = agent.call(&json!({ "set_active": { "set": "Helmet" } }));
    assert_ok(&switch, "set_active");
    assert_eq!(switch["active_set"], "Helmet", "{switch}");

    let bad = agent.call(&json!({ "set_active": { "set": "Boots" } }));
    assert_eq!(bad["ok"], false, "{bad}");
    assert!(
        bad["error"]
            .as_str()
            .unwrap()
            .contains("unknown texture set"),
        "{bad}"
    );
    assert!(
        bad["error"].as_str().unwrap().contains("\"Boots\""),
        "{bad}"
    );

    let state = agent.call(&json!("project_state"));
    assert_eq!(state["project"]["active_set"], "Helmet", "{state}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 5: the photocraft rule — an unknown field runs nothing.
#[test]
fn project_steps_deny_unknown_fields() {
    let dir = write_fixture_project("fields");
    let mut agent = Agent::spawn();

    let bad = agent.call(&json!({ "project_open": { "dir": dir, "bogus": 1 } }));
    assert_eq!(bad["ok"], false, "{bad}");
    assert!(
        bad["error"].as_str().unwrap().contains("unknown field"),
        "{bad}"
    );

    // Nothing ran: still holding nothing, and a clean open works after.
    let state = agent.call(&json!("project_state"));
    assert_eq!(state["project"], Value::Null, "{state}");
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": dir } })),
        "project_open",
    );

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 6: `project_save` writes `project.json` + layers, reports zero
/// paint, and a fresh session reopens the save census-exact.
#[test]
fn project_save_writes_layers_and_reports_zero_paint() {
    let dir = write_fixture_project("save");
    let save_dir = scratch("saved").join("copy.umber");
    let mut agent = Agent::spawn();
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": dir } })),
        "project_open",
    );

    let saved = agent.call(&json!({ "project_save": { "dir": save_dir } }));
    assert_ok(&saved, "project_save");
    assert_eq!(saved["painted"], 0, "{saved}");
    assert_eq!(saved["journal_seq"], 0, "{saved}");
    assert!(save_dir.join("project.json").is_file());
    assert!(save_dir
        .join("layers")
        .join("Body")
        .join("1.json")
        .is_file());
    assert!(!save_dir.join("paint").exists());

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());

    let mut again = Agent::spawn();
    let open = again.call(&json!({ "project_open": { "dir": save_dir } }));
    assert_ok(&open, "project_open");
    assert_eq!(open["active_set"], "Body", "{open}");
    assert_eq!(
        open["texture_sets"][0]["layers"][0]["name"], "Base",
        "{open}"
    );
    assert_eq!(
        open["texture_sets"][0]["layers"][0]["blend"], "Multiply",
        "{open}"
    );

    assert_ok(&again.call(&json!("quit")), "quit");
    assert!(again.wait_exit().success());
}

/// Test 7: with no journaled edits, `undo`/`redo` report false with the
/// capabilities — absence is state, never an error.
#[test]
fn undo_redo_empty_reports_false_with_capabilities() {
    let dir = write_fixture_project("undo");
    let mut agent = Agent::spawn();
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": dir } })),
        "project_open",
    );

    let undo = agent.call(&json!("undo"));
    assert_ok(&undo, "undo");
    assert_eq!(undo["undone"], false, "{undo}");
    assert_eq!(undo["can_undo"], false, "{undo}");
    assert_eq!(undo["can_redo"], false, "{undo}");
    assert_eq!(undo["journal_seq"], 0, "{undo}");

    let redo = agent.call(&json!({ "redo": {} }));
    assert_ok(&redo, "redo");
    assert_eq!(redo["redone"], false, "{redo}");
    assert_eq!(redo["can_undo"], false, "{redo}");
    assert_eq!(redo["can_redo"], false, "{redo}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 8: the ruling — the mesh stays whatever `load` set, and the
/// project stays whatever `project_open` set. Neither step clears the
/// other.
#[test]
fn mesh_and_project_sessions_stay_independent() {
    let dir = scratch("ruling");
    let mesh = write_quad_obj(&dir);
    let project = write_fixture_project("ruling");
    let mut agent = Agent::spawn();

    let load = agent.call(&json!({ "load": { "mesh": mesh } }));
    assert_ok(&load, "load");
    let open = agent.call(&json!({ "project_open": { "dir": project } }));
    assert_ok(&open, "project_open");

    // Opening the project did not clear the mesh ...
    let state = agent.call(&json!("state"));
    assert_ok(&state, "state");
    assert_eq!(state["summary"], load["summary"], "{state}");
    // ... and the mesh did not clear the project.
    let pstate = agent.call(&json!("project_state"));
    assert_ok(&pstate, "project_state");
    assert_eq!(pstate["project"]["active_set"], "Body", "{pstate}");

    // Reloading the mesh still leaves the project held.
    assert_ok(&agent.call(&json!({ "load": { "mesh": mesh } })), "load");
    let again = agent.call(&json!("project_state"));
    assert_eq!(again["project"]["active_set"], "Body", "{again}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}
