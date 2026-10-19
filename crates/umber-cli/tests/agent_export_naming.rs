//! The agent `export` step's project naming rule (the deferred
//! unification, landed 2026-10-18): with a project open, outputs are
//! named after the ACTIVE set — bake's v0 contract, unified — instead
//! of the mesh-derived set name; with none open, the mesh's name
//! stands (the batch path's behavior, byte-identical through
//! `ExportAsk::set_override: None`).
//!
//! Over the process boundary, the suite's pattern: spawn the real
//! binary, one JSONL line in, one JSON reply out. Export runs the
//! bake pipeline (AO + tangent normal per tile), so its deadline is
//! the bake class and the no-adapter skip is the accepted skip.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use umber_core::{
    LayerKind, LayerStack, ProjectModel, ProjectSettings, TextureSet, TextureSetLayers,
};

/// A non-GPU step's reply deadline.
const STEP_TIMEOUT: Duration = Duration::from_secs(60);
/// The export step's deadline (it bakes; lavapipe is slow).
const EXPORT_TIMEOUT: Duration = Duration::from_secs(600);
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

    /// Sends one non-bake step and returns its reply.
    fn call(&mut self, step: &Value) -> Value {
        self.send_raw(&step.to_string());
        self.recv(STEP_TIMEOUT)
    }

    /// Sends one `export` step and returns its reply (the bake-class
    /// deadline — the pipeline runs inside the step).
    fn call_export(&mut self, step: &Value) -> Value {
        self.send_raw(&step.to_string());
        self.recv(EXPORT_TIMEOUT)
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
    let dir = std::env::temp_dir().join(format!(
        "umber-cli-export-naming-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn assert_ok(reply: &Value, step: &str) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["step"], step, "{reply}");
}

/// Whether `reply` is the accepted no-GPU skip (see the module docs).
fn is_gpu_skip(reply: &Value) -> bool {
    reply["ok"] == false
        && reply["error"].as_str().is_some_and(|e| {
            e.contains("no suitable GPU adapter") || e.contains("lacks read-write storage textures")
        })
}

/// The quad fixture (tests/agent.rs's shape): UV'd, no materials — so
/// `texture_set_name` reads the file stem, not a material name.
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

/// A 512² set named `Body` (the paint-ops fixture's shape): the name a
/// project-open export must take over from the mesh's `quad`.
fn write_fixture_project() -> PathBuf {
    let mut body = TextureSet::new_default("Body");
    body.resolution = 512;
    let mut stack = LayerStack::new();
    stack.add_layer("Paint", LayerKind::Paint);
    let dir = scratch("proj");
    let model = ProjectModel::new(
        vec![body],
        vec![TextureSetLayers {
            texture_set: "Body".into(),
            stack,
        }],
        ProjectSettings {
            active_texture_set: Some("Body".into()),
            ..ProjectSettings::default()
        },
    );
    umber_core::save_to_dir(&model, &dir).expect("fixture saves");
    dir
}

/// One `export` step body for a given out dir (preset `unity`: its
/// Normal output survives the headless satisfiability filter — AO +
/// baked tangent normal are the maps the pipeline bakes).
fn export_step(out: PathBuf) -> Value {
    json!({ "export": { "preset": "unity", "out_dir": out } })
}

/// Every written file's name, asserted against one prefix per leg.
fn written_names(reply: &Value) -> Vec<String> {
    let written = reply["written"].as_array().expect("written array");
    assert!(
        !written.is_empty(),
        "the satisfiable outputs wrote: {reply}"
    );
    written
        .iter()
        .map(|w| {
            Path::new(w.as_str().expect("written path is a string"))
                .file_name()
                .expect("written path has a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// The naming rule: with no project open the mesh-derived name (the
/// OBJ's stem — no materials) stands; once a project opens, the ACTIVE
/// set REPLACES it in every written filename (never appends to it).
#[test]
fn export_names_after_the_active_set_when_a_project_is_open() {
    let dir = scratch("mesh");
    let mesh = write_quad_obj(&dir);
    let mut agent = Agent::spawn();
    assert_ok(&agent.call(&json!({ "load": { "mesh": mesh } })), "load");

    // No project: the mesh-derived name. The batch path shares this
    // behavior byte-identically (its `set_override` is `None`).
    let bare = agent.call_export(&export_step(scratch("out-bare")));
    if is_gpu_skip(&bare) {
        eprintln!("skipping: {bare}");
        return;
    }
    assert_ok(&bare, "export");
    for name in written_names(&bare) {
        assert!(
            name.starts_with("quad_"),
            "mesh-derived set name expected: {name} ({bare})"
        );
    }

    // Project open: the active set takes over the naming — bake's v0
    // rule, unified.
    let proj = write_fixture_project();
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": proj } })),
        "project_open",
    );
    let named = agent.call_export(&export_step(scratch("out-proj")));
    assert_ok(&named, "export");
    for name in written_names(&named) {
        assert!(
            name.starts_with("Body_"),
            "active-set name expected: {name} ({named})"
        );
        assert!(
            !name.starts_with("quad"),
            "the override replaces the name, never appends: {name}"
        );
    }

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}
