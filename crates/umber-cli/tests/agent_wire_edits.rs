//! The `agent` edit-step family (queue row 14's follow-up: `layer_add`,
//! `layer_remove`, `layer_set_opacity`, `layer_reorder`,
//! `layer_set_material`) over the process boundary. Each test spawns the
//! real binary with piped stdio, the way a harness does — the same shape
//! as `tests/project.rs`, with its own slim driver (one harness per
//! integration target).
//!
//! Every stdout line must parse as JSON, and nothing may follow the
//! last reply: that is the stdout-discipline check. No test here needs
//! a GPU adapter (the edit steps are CPU-only), so the waits are the
//! same generous deadlines as the loop tests.
//!
//! There is deliberately no `layer_rename` test: the step does not exist
//! (no `LayerCommand` rename op, no journal entry — see the `agent.rs`
//! module docs), so an unknown `layer_rename` line fails as an unknown
//! variant, exactly like any other unknown step.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use umber_core::{
    LayerKind, LayerStack, ProjectModel, ProjectSettings, TextureSet, TextureSetLayers,
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
    let dir =
        std::env::temp_dir().join(format!("umber-cli-wire-edits-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn assert_ok(reply: &Value, step: &str) {
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["step"], step, "{reply}");
}

/// One set, two layers, one exact opacity (0.5 is exact in binary, so the
/// wire round trip is bit-exact, not approximate). Ids 0 and 1, so the
/// first added layer is id 2.
fn fixture_project() -> ProjectModel {
    let body = TextureSet::new_default("Body");
    let mut stack = LayerStack::new();
    let base = stack.add_layer("Base", LayerKind::Paint);
    stack.set_opacity(base, 0.5);
    stack.add_layer("Top", LayerKind::Fill);
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

/// A temp-dir project built via `save_to_dir` (journal mark 0).
fn write_fixture_project(tag: &str) -> PathBuf {
    let dir = scratch(&format!("proj-{tag}"));
    umber_core::save_to_dir(&fixture_project(), &dir).expect("fixture saves");
    dir
}

/// Opens the fixture and returns the driving agent.
fn open_fixture(tag: &str) -> (Agent, PathBuf) {
    let dir = write_fixture_project(tag);
    let mut agent = Agent::spawn();
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": dir } })),
        "project_open",
    );
    (agent, dir)
}

/// The Body set's layer rows from a `project_state` reply (owned: the
/// reply is borrowed by the caller).
fn body_layers(state: &Value) -> Vec<Value> {
    state["project"]["texture_sets"][0]["layers"]
        .as_array()
        .expect("layers array")
        .clone()
}

/// The Body set's layer names from a `project_state` reply, bottom first.
fn body_names(state: &Value) -> Vec<String> {
    body_layers(state)
        .iter()
        .map(|l| l["name"].as_str().expect("name string").to_string())
        .collect()
}

/// Test 1: `layer_add` pushes a layer with the next id, the census delta,
/// and the undo capabilities — with `opacity` as a second undo entry.
#[test]
fn layer_add_pushes_a_layer_with_id_and_census_delta() {
    let (mut agent, _dir) = open_fixture("add");

    let added =
        agent.call(&json!({ "layer_add": { "name": "Shade", "kind": "paint", "opacity": 0.25 } }));
    assert_ok(&added, "layer_add");
    assert_eq!(added["id"], 2, "{added}");
    assert_eq!(added["layer"]["name"], "Shade", "{added}");
    assert_eq!(added["layer"]["kind"], "Paint", "{added}");
    assert_eq!(added["layer"]["opacity"].as_f64().unwrap(), 0.25, "{added}");
    assert_eq!(added["layers"], 3, "{added}");
    assert_eq!(added["can_undo"], true, "{added}");
    assert_eq!(added["can_redo"], false, "{added}");
    // The add plus the opacity are two undo entries through one wire step
    // (a fresh fixture opens at mark 0).
    assert_eq!(added["journal_seq"], 2, "{added}");

    let state = agent.call(&json!("project_state"));
    let layers = body_layers(&state);
    assert_eq!(layers.len(), 3, "{state}");
    assert_eq!(layers[2]["name"], "Shade", "{state}");
    assert_eq!(layers[2]["opacity"].as_f64().unwrap(), 0.25, "{state}");

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 2: `layer_add` accepts the fill and material kinds, defaulting an
/// absent opacity to the stack default 1.0.
#[test]
fn layer_add_accepts_fill_and_material_kinds() {
    let (mut agent, _dir) = open_fixture("kinds");

    let fill = agent.call(&json!({ "layer_add": { "name": "Fill 1", "kind": "fill" } }));
    assert_ok(&fill, "layer_add");
    assert_eq!(fill["id"], 2, "{fill}");
    assert_eq!(fill["layer"]["kind"], "Fill", "{fill}");
    assert_eq!(fill["layer"]["opacity"].as_f64().unwrap(), 1.0, "{fill}");

    let mat = agent.call(&json!({ "layer_add": { "name": "Steel", "kind": "material" } }));
    assert_ok(&mat, "layer_add");
    assert_eq!(mat["id"], 3, "{mat}");
    assert_eq!(mat["layer"]["kind"], "Material", "{mat}");

    let state = agent.call(&json!("project_state"));
    assert_eq!(
        body_names(&state),
        vec!["Base", "Top", "Fill 1", "Steel"],
        "{state}"
    );

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 3: strict typing — an unknown field, an unknown kind, and a
/// non-finite opacity are all errors, and the session survives each one.
#[test]
fn layer_add_rejects_bad_inputs_and_survives() {
    let (mut agent, _dir) = open_fixture("add-strict");

    let bad_field =
        agent.call(&json!({ "layer_add": { "name": "X", "kind": "paint", "bogus": 1 } }));
    assert_eq!(bad_field["ok"], false, "{bad_field}");
    assert!(
        bad_field["error"]
            .as_str()
            .unwrap()
            .contains("unknown field"),
        "{bad_field}"
    );

    let bad_kind = agent.call(&json!({ "layer_add": { "name": "X", "kind": "folder" } }));
    assert_eq!(bad_kind["ok"], false, "{bad_kind}");
    assert!(
        bad_kind["error"]
            .as_str()
            .unwrap()
            .contains("unknown variant"),
        "{bad_kind}"
    );

    // 1e39 overflows f32 (max ≈ 3.4e38) to +inf: the non-finite
    // rejection, not a stored poison. (1e38 would be a VALID finite
    // f32 — the claw's draft had the exponent one short; the battery
    // caught it.)
    let bad_opacity =
        agent.call(&json!({ "layer_add": { "name": "X", "kind": "paint", "opacity": 1e39 } }));
    assert_eq!(bad_opacity["ok"], false, "{bad_opacity}");
    assert!(
        bad_opacity["error"]
            .as_str()
            .unwrap()
            .contains("not finite"),
        "{bad_opacity}"
    );

    // Still alive, and nothing ran: the next id is still 2.
    let added = agent.call(&json!({ "layer_add": { "name": "X", "kind": "paint" } }));
    assert_ok(&added, "layer_add");
    assert_eq!(added["id"], 2, "{added}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 4: `layer_remove` names the removed row; unknown ids are typed
/// errors that remove nothing.
#[test]
fn layer_remove_reports_the_row_and_unknown_ids_error() {
    let (mut agent, _dir) = open_fixture("remove");

    let removed = agent.call(&json!({ "layer_remove": { "id": 1 } }));
    assert_ok(&removed, "layer_remove");
    assert_eq!(removed["name"], "Top", "{removed}");
    assert_eq!(removed["index"], 1, "{removed}");
    assert_eq!(removed["layers"], 1, "{removed}");

    let state = agent.call(&json!("project_state"));
    assert_eq!(body_names(&state), vec!["Base"], "{state}");

    let unknown = agent.call(&json!({ "layer_remove": { "id": 99 } }));
    assert_eq!(unknown["ok"], false, "{unknown}");
    assert!(
        unknown["error"].as_str().unwrap().contains("unknown layer"),
        "{unknown}"
    );
    assert!(
        unknown["error"].as_str().unwrap().contains("99"),
        "{unknown}"
    );

    // The failed remove held nothing back: the census still reads one.
    let again = agent.call(&json!("project_state"));
    assert_eq!(body_names(&again).len(), 1, "{again}");

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 5: `layer_set_opacity` stores, clamps past-the-end input to 1.0,
/// and rejects non-finite input and unknown ids.
#[test]
fn layer_set_opacity_stores_clamps_and_rejects() {
    let (mut agent, _dir) = open_fixture("opacity");

    let set = agent.call(&json!({ "layer_set_opacity": { "id": 1, "opacity": 0.25 } }));
    assert_ok(&set, "layer_set_opacity");
    assert_eq!(set["id"], 1, "{set}");
    assert_eq!(set["opacity"].as_f64().unwrap(), 0.25, "{set}");

    // 7.0 is finite but out of range: the stack clamps, and the reply
    // carries what it holds, not what was asked.
    let clamped = agent.call(&json!({ "layer_set_opacity": { "id": 1, "opacity": 7.0 } }));
    assert_ok(&clamped, "layer_set_opacity");
    assert_eq!(clamped["opacity"].as_f64().unwrap(), 1.0, "{clamped}");

    let state = agent.call(&json!("project_state"));
    assert_eq!(
        body_layers(&state)[1]["opacity"].as_f64().unwrap(),
        1.0,
        "{state}"
    );

    let infinite = agent.call(&json!({ "layer_set_opacity": { "id": 1, "opacity": 1e39 } }));
    assert_eq!(infinite["ok"], false, "{infinite}");
    assert!(
        infinite["error"].as_str().unwrap().contains("not finite"),
        "{infinite}"
    );

    let unknown = agent.call(&json!({ "layer_set_opacity": { "id": 77, "opacity": 0.5 } }));
    assert_eq!(unknown["ok"], false, "{unknown}");
    assert!(
        unknown["error"].as_str().unwrap().contains("unknown layer"),
        "{unknown}"
    );

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 6: `layer_reorder` moves by id (index 0 = bottom), reports the
/// landed index, and clamps past-the-end targets onto the top.
#[test]
fn layer_reorder_moves_by_id_with_landed_index() {
    let (mut agent, _dir) = open_fixture("reorder");
    assert_ok(
        &agent.call(&json!({ "layer_add": { "name": "Third", "kind": "paint" } })),
        "layer_add",
    );

    let moved = agent.call(&json!({ "layer_reorder": { "id": 0, "to": 2 } }));
    assert_ok(&moved, "layer_reorder");
    assert_eq!(moved["from"], 0, "{moved}");
    assert_eq!(moved["to"], 2, "{moved}");

    let state = agent.call(&json!("project_state"));
    assert_eq!(body_names(&state), vec!["Top", "Third", "Base"], "{state}");

    // Past-the-end clamps onto the top (the `LayerStack::reorder` rule).
    let clamped = agent.call(&json!({ "layer_reorder": { "id": 0, "to": 99 } }));
    assert_ok(&clamped, "layer_reorder");
    assert_eq!(clamped["from"], 2, "{clamped}");
    assert_eq!(clamped["to"], 2, "{clamped}");

    let unknown = agent.call(&json!({ "layer_reorder": { "id": 42, "to": 0 } }));
    assert_eq!(unknown["ok"], false, "{unknown}");
    assert!(
        unknown["error"].as_str().unwrap().contains("unknown layer"),
        "{unknown}"
    );

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 7: `layer_set_material` stores the sparse form when `channels` is
/// absent and the full explicit list when present (absent channel fields
/// read as the `MaterialChannel::named` defaults; opacity clamps). An
/// unknown definition id still points — the app badges it, never a refused
/// edit — while unknown layer ids and unknown blends are typed errors.
#[test]
fn layer_set_material_sparse_and_explicit_channels() {
    let (mut agent, _dir) = open_fixture("material");

    let sparse = agent
        .call(&json!({ "layer_set_material": { "id": 0, "material_id": "umber.test-steel" } }));
    assert_ok(&sparse, "layer_set_material");
    assert_eq!(sparse["material_id"], "umber.test-steel", "{sparse}");
    assert_eq!(sparse["channels"], json!([]), "{sparse}");

    let explicit = agent.call(&json!({ "layer_set_material": {
        "id": 0,
        "material_id": "umber.test-steel",
        "channels": [
            { "name": "baseColor", "blend": "Multiply", "opacity": 0.5 },
            { "name": "roughness", "opacity": 3.0 },
        ] } }));
    assert_ok(&explicit, "layer_set_material");
    let channels = explicit["channels"].as_array().expect("channels array");
    assert_eq!(channels.len(), 2, "{explicit}");
    assert_eq!(channels[0]["name"], "baseColor", "{explicit}");
    assert_eq!(channels[0]["blend"], "Multiply", "{explicit}");
    assert_eq!(channels[0]["opacity"].as_f64().unwrap(), 0.5, "{explicit}");
    assert_eq!(channels[0]["enabled"], true, "{explicit}");
    // The roughness entry omits blend/enabled (the named defaults) and
    // overflows opacity (clamped to 1.0).
    assert_eq!(channels[1]["blend"], "Normal", "{explicit}");
    assert_eq!(channels[1]["opacity"].as_f64().unwrap(), 1.0, "{explicit}");
    assert_eq!(channels[1]["enabled"], true, "{explicit}");

    let bad_blend = agent.call(&json!({ "layer_set_material": {
        "id": 0, "material_id": "umber.test-steel",
        "channels": [{ "name": "baseColor", "blend": "Nope" }] } }));
    assert_eq!(bad_blend["ok"], false, "{bad_blend}");
    assert!(
        bad_blend["error"]
            .as_str()
            .unwrap()
            .contains("unknown variant"),
        "{bad_blend}"
    );

    let unknown = agent
        .call(&json!({ "layer_set_material": { "id": 9, "material_id": "umber.test-steel" } }));
    assert_eq!(unknown["ok"], false, "{unknown}");
    assert!(
        unknown["error"].as_str().unwrap().contains("unknown layer"),
        "{unknown}"
    );

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}

/// Test 8: the undo/redo round trip over the wire — an edit, an undo back
/// to the pre-edit census, a redo forward to the post-edit census — for a
/// property edit and a structural edit alike.
#[test]
fn edit_undo_redo_round_trips_through_census() {
    let (mut agent, _dir) = open_fixture("round-trip");

    // Property round trip: Top starts at 1.0.
    assert_ok(
        &agent.call(&json!({ "layer_set_opacity": { "id": 1, "opacity": 0.75 } })),
        "layer_set_opacity",
    );
    let undo = agent.call(&json!("undo"));
    assert_ok(&undo, "undo");
    assert_eq!(undo["undone"], true, "{undo}");
    let state = agent.call(&json!("project_state"));
    assert_eq!(
        body_layers(&state)[1]["opacity"].as_f64().unwrap(),
        1.0,
        "{state}"
    );
    let redo = agent.call(&json!("redo"));
    assert_ok(&redo, "redo");
    assert_eq!(redo["redone"], true, "{redo}");
    let state = agent.call(&json!("project_state"));
    assert_eq!(
        body_layers(&state)[1]["opacity"].as_f64().unwrap(),
        0.75,
        "{state}"
    );

    // Structural round trip: the added layer leaves and returns.
    assert_ok(
        &agent.call(&json!({ "layer_add": { "name": "Sketched", "kind": "paint" } })),
        "layer_add",
    );
    let undo = agent.call(&json!("undo"));
    assert_eq!(undo["undone"], true, "{undo}");
    let state = agent.call(&json!("project_state"));
    assert_eq!(body_names(&state), vec!["Base", "Top"], "{state}");
    let redo = agent.call(&json!("redo"));
    assert_eq!(redo["redone"], true, "{redo}");
    let state = agent.call(&json!("project_state"));
    assert_eq!(
        body_names(&state),
        vec!["Base", "Top", "Sketched"],
        "{state}"
    );

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());
}

/// Test 9: the journal records every edit — `project_save` stamps the live
/// high-water mark, and a fresh session reopens the edited state.
#[test]
fn journal_records_edits_and_save_stamps_high_water() {
    let (mut agent, _dir) = open_fixture("journal");
    let save_dir = scratch("wiresaved").join("copy.umber");

    // Two recorded edits on a fixture that opens at mark 0.
    assert_ok(
        &agent.call(&json!({ "layer_add": { "name": "Extra", "kind": "paint" } })),
        "layer_add",
    );
    assert_ok(
        &agent.call(&json!({ "layer_set_opacity": { "id": 0, "opacity": 0.75 } })),
        "layer_set_opacity",
    );

    let saved = agent.call(&json!({ "project_save": { "dir": save_dir } }));
    assert_ok(&saved, "project_save");
    assert_eq!(saved["painted"], 0, "{saved}");
    assert_eq!(saved["journal_seq"], 2, "{saved}");

    assert_ok(&agent.call(&json!("quit")), "quit");
    assert!(agent.wait_exit().success());

    // A fresh session reopens the edited state with the stamped mark.
    let mut again = Agent::spawn();
    let open = again.call(&json!({ "project_open": { "dir": save_dir } }));
    assert_ok(&open, "project_open");
    assert_eq!(open["journal_seq"], 2, "{open}");
    assert_eq!(open["active_set"], "Body", "{open}");
    let state = again.call(&json!("project_state"));
    assert_eq!(body_names(&state), vec!["Base", "Top", "Extra"], "{state}");
    assert_eq!(
        body_layers(&state)[0]["opacity"].as_f64().unwrap(),
        0.75,
        "{state}"
    );

    assert_ok(&again.call(&json!("quit")), "quit");
    assert!(again.wait_exit().success());
}

/// Test 10: with no project open every edit step is the same typed error
/// as the v0 model steps (the census stays the `project: null` state),
/// and the session survives to open afterwards.
#[test]
fn edits_without_an_open_project_are_typed_errors() {
    let dir = write_fixture_project("no-project");
    let mut agent = Agent::spawn();

    let steps = [
        json!({ "layer_add": { "name": "X", "kind": "paint" } }),
        json!({ "layer_remove": { "id": 0 } }),
        json!({ "layer_set_opacity": { "id": 0, "opacity": 0.5 } }),
        json!({ "layer_reorder": { "id": 0, "to": 0 } }),
        json!({ "layer_set_material": { "id": 0, "material_id": "umber.test-steel" } }),
    ];
    for step in &steps {
        let reply = agent.call(step);
        assert_eq!(reply["ok"], false, "{reply}");
        assert!(
            reply["error"].as_str().unwrap().contains("no project open"),
            "{reply}"
        );
    }

    // Still alive: the census is empty state, and opening works after.
    let state = agent.call(&json!("project_state"));
    assert_ok(&state, "project_state");
    assert_eq!(state["project"], Value::Null, "{state}");
    assert_ok(
        &agent.call(&json!({ "project_open": { "dir": dir } })),
        "project_open",
    );

    agent.close_stdin();
    assert!(agent.wait_exit().success());
}
