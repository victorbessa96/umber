//! `umber-app --control <port>` — the app-side loopback control channel
//! (queue row 153). v2 adds the UI-thread command queue: `ui.inspect` and
//! `viewport.screenshot` execute on the UI thread (the app drains
//! [`ControlServer::requests`] at the top of `fn ui`); listener threads
//! block on a per-request reply channel for at most [`UI_REPLY_TIMEOUT`]
//! (job commands: [`JOB_REPLY_TIMEOUT`]).
//! `umber-app --bridge <port>` (step 5) relays MCP stdio frames verbatim
//! to an `umber-cli mcp` child process — one authed client at a time; the
//! app never interprets MCP frames.
//!
//! Security model: bind 127.0.0.1 ONLY — there is no interface argument
//! to get wrong. Token auth before any method dispatch, one token from
//! `UMBER_CONTROL_TOKEN` or a fresh 0600 file at
//! `<tmp>/umber-app-control-<port>.token` (bridge: `...-bridge-<port>.token`).
//! Concurrency: listener + per-connection threads own only sockets and the
//! request queue; no egui state ever crosses a thread boundary — replies
//! for UI-thread methods flow back through a per-request channel.
//!
//! Wire: one JSON object per line each way. Requests
//! `{"id", "method", "params"}`; replies `{"id", "ok": true, "result"}` or
//! `{"id", "ok": false, "error"}`. The first request on every connection
//! must be `auth`.

use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};

pub use umber_core::control::TOKEN_ENV;
use umber_core::control::{self, ConnectionLimiter, LineRead};

/// The methods v2 answers besides `auth`. `ui.inspect` and
/// `viewport.screenshot` execute on the UI thread (see [`ControlRequest`]).
pub const METHODS: &[&str] = &[
    "ping",
    "version",
    "methods",
    "ui.inspect",
    "viewport.screenshot",
    "command.list",
    "command.run",
    "mesh.load",
    "project.open",
    "job.status",
    "assets.gc",
];

/// How long a connection thread waits for the UI thread to answer a queued
/// request before replying `ui_timeout`.
pub const UI_REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Commands that start a long-running job when run by id (A2-4 tail):
/// `bake.run` hands off to a worker, `export.run` exports synchronously on
/// the UI thread, so their reply can legitimately outlast
/// [`UI_REPLY_TIMEOUT`]. The ROADMAP standing debt of 2026-11-06 ("an
/// export longer than 30 s times out over --control although it
/// finishes") is closed by waiting [`JOB_REPLY_TIMEOUT`] for exactly
/// these ids; every other request keeps the 30 s UI timeout.
pub const JOB_COMMANDS: &[&str] = &["bake.run", "export.run"];
/// The reply ceiling for [`JOB_COMMANDS`]: long enough for a big export,
/// still bounded so a stuck UI thread cannot hang a client forever.
pub const JOB_REPLY_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The reply ceiling for one request: [`JOB_REPLY_TIMEOUT`] for a
/// `command.run` of a [`JOB_COMMANDS`] id, [`UI_REPLY_TIMEOUT`] otherwise.
pub fn reply_timeout_for(req: &Value) -> Duration {
    let is_command_run = req.get("method").and_then(Value::as_str) == Some("command.run");
    let id = req.pointer("/params/id").and_then(Value::as_str);
    if is_command_run && id.is_some_and(|i| JOB_COMMANDS.contains(&i)) {
        JOB_REPLY_TIMEOUT
    } else {
        UI_REPLY_TIMEOUT
    }
}

/// A request forwarded to the UI thread (queue row 153 steps 3-4). The app
/// drains the queue at the top of `fn ui` and sends the result back; `Err`
/// is the wire error string.
pub enum ControlRequest {
    /// Dump the app's live state as JSON (panels, tool, brush, layers).
    Inspect {
        reply: Sender<Result<Value, String>>,
    },
    /// Capture the last painted frame as PNG (base64) + dims.
    Screenshot {
        reply: Sender<Result<Value, String>>,
    },
    /// List the command registry (queue row 154 wave 2c): id + enabled per
    /// command, evaluated on the UI thread.
    ListCommands {
        reply: Sender<Result<Value, String>>,
    },
    /// Run one command by dotted id through the same dispatch as a menu
    /// click; typed `CommandError` on the wire, never a panic.
    RunCommand {
        id: String,
        reply: Sender<Result<Value, String>>,
    },
    /// Load a mesh by path (UI/UX pass U1-7, R-AGENT-1): the agent's way
    /// past the empty state. `file.open_mesh` would open a native dialog
    /// on the UI thread, which no remote caller can answer.
    LoadMesh {
        path: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    /// Open a project folder by path (same reason as [`Self::LoadMesh`]).
    OpenProject {
        path: PathBuf,
        reply: Sender<Result<Value, String>>,
    },
    /// Read the bake/export job state (A2-4 tail): what a client polls
    /// after `command.run bake.run` / `export.run`. Read-only.
    JobStatus {
        reply: Sender<Result<Value, String>>,
    },
}

/// Commands that open a native file dialog on the UI thread. Over the
/// control channel they would block the UI thread until a human answered
/// it, and every later request would time out; they refuse with
/// [`dialog_refusal`] instead (R-AGENT-1).
pub const DIALOG_COMMANDS: &[&str] = &[
    "file.open_mesh",
    "file.open_project",
    "file.save_project_as",
];

/// The wire error for a [`DIALOG_COMMANDS`] id: names the path-taking
/// method to use instead.
pub fn dialog_refusal(id: &str) -> String {
    let instead = match id {
        "file.open_mesh" => "mesh.load {path}",
        "file.open_project" => "project.open {path}",
        _ => "a path-taking method",
    };
    format!("command opens a native dialog (blocked over control): {id}; use {instead}")
}

/// A running control listener. Dropping it does NOT stop the thread (the
/// app process owns it); the field on `UmberApp` keeps it alive for the
/// app's lifetime.
pub struct ControlServer {
    /// The bound port (0 at startup means an ephemeral port was picked).
    pub port: u16,
    /// The accepted token (also in `UMBER_CONTROL_TOKEN` or the token file).
    /// Read by the control tests; the app itself only needs `token_file`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub token: String,
    /// Where a generated token was written, when the env gave none.
    pub token_file: Option<PathBuf>,
    /// Drained by the app on the UI thread, top of `fn ui`.
    pub requests: Receiver<ControlRequest>,
    _thread: JoinHandle<()>,
}

/// `--control` / `--bridge` take a bare port number only. Anything with an
/// interface (`0.0.0.0:8377`, `[::1]:8377`) or non-numeric is refused: the
/// channels are loopback-only, ever.
pub fn parse_control_arg(v: &str) -> Result<u16, String> {
    v.parse::<u16>().map_err(|_| {
        format!("`{v}` is not a bare port number (loopback-only; there is no interface argument)")
    })
}

/// Start the control listener on 127.0.0.1:`port` (0 = ephemeral, for tests).
pub fn start(port: u16, repaint: egui::Context) -> std::io::Result<ControlServer> {
    let listener = control::bind_loopback(port)?;
    let port = listener.local_addr()?.port();
    let (token, token_file) =
        control::server_token("umber-app-control", port).map_err(std::io::Error::other)?;
    let (tx, requests) = mpsc::channel();
    let token_loop = token.clone();
    let thread = std::thread::Builder::new()
        .name("umber-control".into())
        .spawn(move || accept_loop(listener, port, token_loop, tx, repaint))?;
    Ok(ControlServer {
        port,
        token,
        token_file,
        requests,
        _thread: thread,
    })
}

fn accept_loop(
    listener: TcpListener,
    port: u16,
    token: String,
    requests: Sender<ControlRequest>,
    repaint: egui::Context,
) {
    let limiter = ConnectionLimiter::default();
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Some(permit) = limiter.try_acquire(control::MAX_CONNECTIONS) {
                    let token = token.clone();
                    let requests = requests.clone();
                    let repaint = repaint.clone();
                    let _ = std::thread::Builder::new()
                        .name("umber-control-conn".into())
                        .spawn(move || {
                            let _permit = permit;
                            handle_conn(s, port, &token, &requests, &repaint);
                        });
                }
            }
            Err(_) => continue,
        }
    }
}

/// Read + check the first request on a connection: it must be `auth` with
/// the right token. Replies on the wire either way; returns false when the
/// connection is closed (bad token, malformed first line, EOF).
fn auth_first(reader: &mut BufReader<TcpStream>, writer: &mut TcpStream, token: &str) -> bool {
    let mut line = String::new();
    if !matches!(
        control::read_bounded_line(reader, &mut line),
        Ok(LineRead::Line)
    ) {
        return false;
    }
    let req = match serde_json::from_str::<Value>(&line) {
        Ok(v) => v,
        Err(_) => {
            let _ = control::write_json_line(
                writer,
                &json!({"id": Value::Null, "ok": false, "error": "malformed json"}),
            );
            return false;
        }
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let supplied = req
        .pointer("/params/token")
        .and_then(Value::as_str)
        .unwrap_or("");
    if method != "auth" || !control::token_matches(token, supplied) {
        let _ = control::write_json_line(
            writer,
            &json!({"id": id, "ok": false, "error": "unauthorized"}),
        );
        return false;
    }
    let _ = control::write_json_line(
        writer,
        &json!({"id": id, "ok": true, "result": {"app": "umber-app", "version": env!("CARGO_PKG_VERSION")}}),
    );
    true
}

fn handle_conn(
    stream: TcpStream,
    port: u16,
    token: &str,
    requests: &Sender<ControlRequest>,
    repaint: &egui::Context,
) {
    let _ = stream.set_nodelay(true);
    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(_) => return,
    };
    let mut writer = stream;
    if !auth_first(&mut reader, &mut writer, token) {
        return;
    }
    let mut line = String::new();

    loop {
        match control::read_bounded_line(&mut reader, &mut line) {
            Ok(LineRead::Line) => {}
            Ok(LineRead::TooLong) => {
                let _ = control::write_json_line(
                    &mut writer,
                    &json!({"id": Value::Null, "ok": false, "error": format!("request over {}-byte limit", control::MAX_REQUEST_BYTES)}),
                );
                continue;
            }
            Ok(LineRead::Eof) | Err(_) => return,
        }
        let req = match serde_json::from_str::<Value>(&line) {
            Ok(v) => v,
            Err(_) => {
                let _ = control::write_json_line(
                    &mut writer,
                    &json!({"id": Value::Null, "ok": false, "error": "malformed json"}),
                );
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let result = match method {
            "ping" => json!({"pong": true, "port": port}),
            "version" => json!({"app": "umber-app", "version": env!("CARGO_PKG_VERSION")}),
            "methods" => json!({"methods": METHODS}),
            "ui.inspect" => {
                match forward_ui(requests, repaint, |reply| ControlRequest::Inspect { reply }) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = control::write_json_line(
                            &mut writer,
                            &json!({"id": id, "ok": false, "error": e}),
                        );
                        continue;
                    }
                }
            }
            "viewport.screenshot" => {
                match forward_ui(requests, repaint, |reply| ControlRequest::Screenshot {
                    reply,
                }) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = control::write_json_line(
                            &mut writer,
                            &json!({"id": id, "ok": false, "error": e}),
                        );
                        continue;
                    }
                }
            }
            "job.status" => {
                match forward_ui(requests, repaint, |reply| ControlRequest::JobStatus {
                    reply,
                }) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = control::write_json_line(
                            &mut writer,
                            &json!({"id": id, "ok": false, "error": e}),
                        );
                        continue;
                    }
                }
            }
            "command.list" => {
                match forward_ui(requests, repaint, |reply| ControlRequest::ListCommands {
                    reply,
                }) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = control::write_json_line(
                            &mut writer,
                            &json!({"id": id, "ok": false, "error": e}),
                        );
                        continue;
                    }
                }
            }
            "mesh.load" | "project.open" => {
                let path = req
                    .pointer("/params/path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
                let Some(path) = path else {
                    let _ = control::write_json_line(
                        &mut writer,
                        &json!({"id": id, "ok": false, "error": "missing params.path"}),
                    );
                    continue;
                };
                let mesh = method == "mesh.load";
                match forward_ui(requests, repaint, |reply| {
                    if mesh {
                        ControlRequest::LoadMesh { path, reply }
                    } else {
                        ControlRequest::OpenProject { path, reply }
                    }
                }) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = control::write_json_line(
                            &mut writer,
                            &json!({"id": id, "ok": false, "error": e}),
                        );
                        continue;
                    }
                }
            }
            "assets.gc" => match assets_gc_reply(&req) {
                Ok(v) => v,
                Err(e) => {
                    let _ = control::write_json_line(
                        &mut writer,
                        &json!({"id": id, "ok": false, "error": e}),
                    );
                    continue;
                }
            },
            "command.run" => {
                let cmd_id = req
                    .pointer("/params/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(cmd) = cmd_id.as_deref().filter(|c| DIALOG_COMMANDS.contains(c)) {
                    let _ = control::write_json_line(
                        &mut writer,
                        &json!({"id": id, "ok": false, "error": dialog_refusal(cmd)}),
                    );
                    continue;
                }
                let reply_timeout = reply_timeout_for(&req);
                match cmd_id {
                    Some(cmd_id) => {
                        match forward_ui_with(requests, repaint, reply_timeout, |reply| {
                            ControlRequest::RunCommand { id: cmd_id, reply }
                        }) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = control::write_json_line(
                                    &mut writer,
                                    &json!({"id": id, "ok": false, "error": e}),
                                );
                                continue;
                            }
                        }
                    }
                    None => {
                        let _ = control::write_json_line(
                            &mut writer,
                            &json!({"id": id, "ok": false, "error": "missing params.id (dotted command id)"}),
                        );
                        continue;
                    }
                }
            }
            other => {
                let _ = control::write_json_line(
                    &mut writer,
                    &json!({"id": id, "ok": false, "error": format!("unknown_method: {other}")}),
                );
                continue;
            }
        };
        let _ = control::write_json_line(
            &mut writer,
            &json!({"id": id, "ok": true, "result": result}),
        );
    }
}

/// `assets.gc {path, dry_run?, min_age_secs?}`: one AssetStore GC pass
/// over the project folder at `path` (requirements row 121). Runs on the
/// connection thread: it reads only the project's files, never egui
/// state, and refuses on its own while a save is staged or pending
/// (`umber_app::asset_gc`'s rules). `dry_run` defaults to TRUE, so a
/// bare call only lists what would go and the bytes reclaimable.
pub fn assets_gc_reply(req: &Value) -> Result<Value, String> {
    let path = req
        .pointer("/params/path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "missing params.path (the project folder)".to_string())?;
    if !path.is_dir() {
        return Err(format!("no such folder: {}", path.display()));
    }
    let mut options = umber_app::asset_gc::GcOptions::default();
    match req.pointer("/params/dry_run") {
        None | Some(Value::Null) => {}
        Some(Value::Bool(b)) => options.dry_run = *b,
        Some(_) => return Err("params.dry_run must be a boolean".to_string()),
    }
    match req.pointer("/params/min_age_secs") {
        None | Some(Value::Null) => {}
        Some(v) => {
            let secs = v
                .as_u64()
                .ok_or_else(|| "params.min_age_secs must be a non-negative integer".to_string())?;
            options.min_age = Duration::from_secs(secs);
        }
    }
    umber_app::asset_gc::collect(&path, options)
        .map(|report| report.to_json())
        .map_err(|e| e.to_string())
}

/// Forward a request to the UI thread's queue and wait for the reply.
fn forward_ui(
    requests: &Sender<ControlRequest>,
    repaint: &egui::Context,
    make: impl FnOnce(Sender<Result<Value, String>>) -> ControlRequest,
) -> Result<Value, String> {
    forward_ui_with(requests, repaint, UI_REPLY_TIMEOUT, make)
}

/// Same as [`forward_ui`] with an explicit reply ceiling (job commands get
/// [`JOB_REPLY_TIMEOUT`]; see [`reply_timeout_for`]).
fn forward_ui_with(
    requests: &Sender<ControlRequest>,
    repaint: &egui::Context,
    timeout: Duration,
    make: impl FnOnce(Sender<Result<Value, String>>) -> ControlRequest,
) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    requests
        .send(make(tx))
        .map_err(|_| "control queue closed (app is shutting down)".to_string())?;
    // The UI thread drains the queue only on frames; on an idle window no
    // frame is coming, so wake it explicitly or every UI-forwarded method
    // dies on ui_timeout (found by the 2026-11-05 headless smoke).
    repaint.request_repaint();
    match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(format!("ui_timeout after {}s", timeout.as_secs()))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("ui thread dropped the reply channel".to_string())
        }
    }
}

/// The `viewport.screenshot` result: the frame's pixels as PNG, base64.
/// Runs on the UI thread inside egui's screenshot callback, so the encode
/// never crosses a thread boundary.
pub fn screenshot_reply(img: &egui::ColorImage) -> Result<Value, String> {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in &img.pixels {
        rgba.extend_from_slice(&px.to_array());
    }
    let png = umber_export::png::encode_png(
        w as u32,
        h as u32,
        &rgba,
        umber_export::png::Transfer::Linear,
    )
    .map_err(|e| format!("png encode: {e}"))?;
    use base64::Engine as _;
    Ok(json!({
        "width": w,
        "height": h,
        "format": "png",
        "png_base64": base64::engine::general_purpose::STANDARD.encode(&png),
    }))
}

// ---------------------------------------------------------------------------
// `--bridge <port>`: verbatim MCP stdio relay to an `umber-cli mcp` child
// (queue row 153 step 5; design brief
// docs/loop/briefs/2026-11-02-app-control-channel-design.md).
// ---------------------------------------------------------------------------

/// A running bridge listener. Same lifetime rule as [`ControlServer`]: the
/// field on `UmberApp` keeps it alive for the app's lifetime.
pub struct BridgeServer {
    pub port: u16,
    /// Read by the bridge tests; the app itself only needs `token_file`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub token: String,
    pub token_file: Option<PathBuf>,
    _thread: JoinHandle<()>,
}

/// The app's own `--control` endpoint, handed to the bridge so the
/// spawned `umber-cli mcp` child can reach it (`UMBER_CONTROL_PORT` +
/// `UMBER_CONTROL_TOKEN_FILE`, or `UMBER_CONTROL_TOKEN` when the app's
/// token came from the env and no file was written).
#[derive(Debug, Clone)]
pub struct BridgeControl {
    pub port: u16,
    pub token_file: Option<PathBuf>,
    pub token: String,
}

/// Start the bridge listener on 127.0.0.1:`port` (0 = ephemeral, tests).
/// The `umber-cli mcp` child spawns lazily on the first authed client;
/// `control` wires that child to the app's `--control` channel.
pub fn start_bridge(port: u16, control: Option<BridgeControl>) -> std::io::Result<BridgeServer> {
    let listener = control::bind_loopback(port)?;
    let port = listener.local_addr()?.port();
    let (token, token_file) =
        control::server_token("umber-app-bridge", port).map_err(std::io::Error::other)?;
    let state = Arc::new(Mutex::new(BridgeState {
        control,
        ..BridgeState::default()
    }));
    let token_loop = token.clone();
    let thread = std::thread::Builder::new()
        .name("umber-bridge".into())
        .spawn(move || bridge_accept_loop(listener, token_loop, state))?;
    Ok(BridgeServer {
        port,
        token,
        token_file,
        _thread: thread,
    })
}

#[derive(Default)]
struct BridgeState {
    /// The app's `--control` endpoint for the MCP child, when the app
    /// runs with `--control` (None = the child gets no channel and the
    /// `command_*` tools answer their typed error).
    control: Option<BridgeControl>,
    /// The spawned `umber-cli mcp` child, when a client owns the bridge.
    child: Option<std::process::Child>,
    /// The live client connection the child's stdout pumps into.
    out: Option<TcpStream>,
}

fn bridge_accept_loop(listener: TcpListener, token: String, state: Arc<Mutex<BridgeState>>) {
    let limiter = ConnectionLimiter::default();
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Some(permit) = limiter.try_acquire(control::MAX_CONNECTIONS) {
                    let token = token.clone();
                    let state = Arc::clone(&state);
                    let _ = std::thread::Builder::new()
                        .name("umber-bridge-conn".into())
                        .spawn(move || {
                            let _permit = permit;
                            bridge_conn(s, &token, state);
                        });
                }
            }
            Err(_) => continue,
        }
    }
}

/// The `umber-cli` binary: next to the running app executable first, then
/// on `PATH`.
fn mcp_child_program() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("umber-cli");
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("umber-cli")
}

/// Build the `umber-cli mcp` child command, wiring the app's `--control`
/// endpoint into its env so the `command_*` tools reach the live app.
/// Split from `spawn_mcp_child` so tests can assert the env without
/// spawning.
fn mcp_child_command(control: Option<&BridgeControl>) -> std::process::Command {
    let mut cmd = std::process::Command::new(mcp_child_program());
    cmd.arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(control) = control {
        cmd.env("UMBER_CONTROL_PORT", control.port.to_string());
        match &control.token_file {
            Some(path) => {
                cmd.env("UMBER_CONTROL_TOKEN_FILE", path);
            }
            None => {
                cmd.env(TOKEN_ENV, &control.token);
            }
        }
    }
    cmd
}

fn spawn_mcp_child(control: Option<&BridgeControl>) -> std::io::Result<std::process::Child> {
    mcp_child_command(control).spawn()
}

fn bridge_conn(stream: TcpStream, token: &str, state: Arc<Mutex<BridgeState>>) {
    let state_control = state.lock().unwrap().control.clone();
    let _ = stream.set_nodelay(true);
    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(_) => return,
    };
    let mut writer = stream;
    if !auth_first(&mut reader, &mut writer, token) {
        return;
    }
    // One authed client owns the bridge at a time (MCP stdio is a
    // single-client stream).
    let mut guard = state.lock().unwrap();
    if guard.out.is_some() {
        drop(guard);
        let _ = control::write_json_line(
            &mut writer,
            &json!({"id": Value::Null, "ok": false, "error": "bridge busy: one authed client at a time"}),
        );
        return;
    }
    if guard.child.is_none() {
        match spawn_mcp_child(state_control.as_ref()) {
            Ok(child) => guard.child = Some(child),
            Err(e) => {
                drop(guard);
                let _ = control::write_json_line(
                    &mut writer,
                    &json!({"id": Value::Null, "ok": false, "error": format!("bridge child failed to spawn: {e}")}),
                );
                return;
            }
        }
    }
    let mut child = guard.child.take().expect("child just ensured");
    let pipes = (child.stdin.take(), child.stdout.take());
    let (mut child_stdin, child_stdout) = match pipes {
        (Some(a), Some(b)) => (a, b),
        _ => {
            // No pipes: kill + wait now so the child cannot zombie on this
            // early path (the pump thread owns reaping on the normal path).
            let _ = child.kill();
            let _ = child.wait();
            drop(guard);
            return;
        }
    };
    guard.out = Some(writer.try_clone().expect("writer clones"));
    drop(guard);

    // Pump client -> child stdin, verbatim bytes (MCP frames are
    // newline-delimited JSON; the app never interprets them).
    let state_in = Arc::clone(&state);
    let pump_in = std::thread::Builder::new()
        .name("umber-bridge-in".into())
        .spawn(move || {
            let mut line = String::new();
            loop {
                match control::read_bounded_line(&mut reader, &mut line) {
                    Ok(LineRead::Line) => {
                        if child_stdin
                            .write_all(line.as_bytes())
                            .and_then(|_| child_stdin.flush())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(LineRead::TooLong) => continue,
                    Ok(LineRead::Eof) | Err(_) => break,
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            // The bridge is free again: the next client respawns the child.
            let mut g = state_in.lock().unwrap();
            g.child = None;
            g.out = None;
        });
    // Pump child stdout -> client, verbatim bytes.
    let state_out = Arc::clone(&state);
    let pump_out = std::thread::Builder::new()
        .name("umber-bridge-out".into())
        .spawn(move || {
            let mut reader = BufReader::new(child_stdout);
            let mut line = String::new();
            loop {
                match control::read_bounded_line(&mut reader, &mut line) {
                    Ok(LineRead::Line) => {
                        let mut g = state_out.lock().unwrap();
                        match g.out.as_mut() {
                            Some(out) => {
                                if out
                                    .write_all(line.as_bytes())
                                    .and_then(|_| out.flush())
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    Ok(LineRead::TooLong) => continue,
                    Ok(LineRead::Eof) | Err(_) => break,
                }
            }
        });
    // The pumps own their halves; dropping the JoinHandles detaches them.
    drop(pump_in);
    drop(pump_out);
    drop(writer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead as _;
    use std::time::Duration;

    fn roundtrip(port: u16, token: &str, lines: &[String]) -> Vec<Value> {
        let mut s = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut out = Vec::new();
        for l in lines {
            s.write_all(l.as_bytes()).unwrap();
            s.write_all(b"\n").unwrap();
            let mut buf = String::new();
            r.read_line(&mut buf).unwrap();
            out.push(serde_json::from_str(&buf).unwrap());
        }
        let _ = token;
        out
    }

    #[test]
    fn control_arg_takes_a_bare_port_only() {
        assert_eq!(parse_control_arg("8377").unwrap(), 8377);
        assert_eq!(parse_control_arg("0").unwrap(), 0);
        assert!(parse_control_arg("0.0.0.0:8377").is_err());
        assert!(parse_control_arg("[::1]:8377").is_err());
        assert!(parse_control_arg("8377x").is_err());
        assert!(parse_control_arg("99999").is_err());
    }

    #[test]
    fn job_commands_get_the_job_reply_timeout() {
        for id in JOB_COMMANDS {
            let req = json!({"method": "command.run", "params": {"id": id}});
            assert_eq!(reply_timeout_for(&req), JOB_REPLY_TIMEOUT, "{id}");
        }
        for req in [
            json!({"method": "command.run", "params": {"id": "view.toggle_grid"}}),
            json!({"method": "command.run"}),
            json!({"method": "job.status"}),
            json!({"method": "ui.inspect"}),
        ] {
            assert_eq!(reply_timeout_for(&req), UI_REPLY_TIMEOUT, "{req}");
        }
        // A job id on another method keeps the short UI timeout.
        assert_eq!(
            reply_timeout_for(&json!({"method": "job.status", "params": {"id": "export.run"}})),
            UI_REPLY_TIMEOUT
        );
    }

    #[test]
    fn bad_token_is_rejected_and_closes() {
        let server = start(0, egui::Context::default()).unwrap();
        let replies = roundtrip(
            server.port,
            &server.token,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": "wrong-wrong-wrong"}})
                    .to_string(),
            ],
        );
        assert_eq!(replies[0]["ok"], false);
        assert_eq!(replies[0]["error"], "unauthorized");
        // The connection is closed after a failed auth: a further read EOFs.
        let mut s = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, server.port)).unwrap();
        s.write_all(
            b"{\"id\":0,\"method\":\"auth\",\"params\":{\"token\":\"wrong-wrong-wrong\"}}\n",
        )
        .unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut buf = String::new();
        r.read_line(&mut buf).unwrap();
        let mut buf2 = String::new();
        assert_eq!(r.read_line(&mut buf2).unwrap(), 0); // EOF: closed
    }

    #[test]
    fn auth_then_ping_version_methods() {
        let server = start(0, egui::Context::default()).unwrap();
        let token = server.token.clone();
        let replies = roundtrip(
            server.port,
            &token,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": token}}).to_string(),
                json!({"id": 1, "method": "ping"}).to_string(),
                json!({"id": 2, "method": "version"}).to_string(),
                json!({"id": 3, "method": "methods"}).to_string(),
                json!({"id": 4, "method": "nope"}).to_string(),
            ],
        );
        assert_eq!(replies[0]["ok"], true);
        assert_eq!(replies[0]["result"]["app"], "umber-app");
        assert_eq!(replies[1]["result"]["pong"], true);
        assert_eq!(replies[1]["result"]["port"], server.port);
        assert_eq!(replies[2]["result"]["app"], "umber-app");
        assert!(replies[2]["result"]["version"].is_string());
        let methods = replies[3]["result"]["methods"].as_array().unwrap();
        let names: Vec<&str> = methods.iter().map(|m| m.as_str().unwrap()).collect();
        assert!(names.contains(&"ping"));
        assert!(names.contains(&"ui.inspect"));
        assert!(names.contains(&"viewport.screenshot"));
        assert_eq!(replies[4]["ok"], false);
        assert!(replies[4]["error"]
            .as_str()
            .unwrap()
            .starts_with("unknown_method"));
    }

    #[test]
    fn ui_requests_forward_to_the_drain_thread() {
        let ctx = egui::Context::default();
        let server = start(0, ctx.clone()).unwrap();
        let port = server.port;
        let token = server.token.clone();
        // A stand-in UI thread: drains the queue and answers canned results.
        let drain = std::thread::spawn(move || {
            let requests = server.requests;
            while let Ok(req) = requests.recv() {
                match req {
                    ControlRequest::Inspect { reply } => {
                        let _ = reply.send(Ok(json!({"inspected": true})));
                    }
                    ControlRequest::Screenshot { reply } => {
                        let _ = reply.send(Ok(
                            json!({"width": 1, "height": 1, "format": "png", "png_base64": ""}),
                        ));
                    }
                    ControlRequest::ListCommands { reply } => {
                        let _ = reply.send(Ok(json!({"commands": [], "count": 0})));
                    }
                    ControlRequest::RunCommand { id, reply } => {
                        let _ = reply.send(Ok(json!({"ran": id})));
                    }
                    ControlRequest::LoadMesh { path, reply } => {
                        let _ = reply.send(Ok(json!({"loaded": path.display().to_string()})));
                    }
                    ControlRequest::OpenProject { path, reply } => {
                        let _ = reply.send(Ok(json!({"opened": path.display().to_string()})));
                    }
                    ControlRequest::JobStatus { reply } => {
                        let _ = reply.send(Ok(json!({"bake": {"running": false}})));
                    }
                }
            }
        });
        let replies = roundtrip(
            port,
            &token,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": token}}).to_string(),
                json!({"id": 1, "method": "ui.inspect"}).to_string(),
                json!({"id": 2, "method": "viewport.screenshot"}).to_string(),
                json!({"id": 3, "method": "command.list"}).to_string(),
                json!({"id": 4, "method": "command.run", "params": {"id": "view.toggle_grid"}})
                    .to_string(),
                json!({"id": 5, "method": "command.run"}).to_string(),
                json!({"id": 6, "method": "command.run", "params": {"id": "file.open_mesh"}})
                    .to_string(),
                json!({"id": 7, "method": "mesh.load", "params": {"path": "/m/a.glb"}}).to_string(),
                json!({"id": 8, "method": "mesh.load"}).to_string(),
                json!({"id": 9, "method": "project.open", "params": {"path": "/p"}}).to_string(),
                json!({"id": 10, "method": "job.status"}).to_string(),
            ],
        );
        assert_eq!(replies[10]["ok"], true);
        assert_eq!(replies[10]["result"]["bake"]["running"], false);
        // R-AGENT-1: a dialog command refuses instead of hanging the UI
        // thread, and names the path-taking method.
        assert_eq!(replies[6]["ok"], false);
        assert!(replies[6]["error"].as_str().unwrap().contains("mesh.load"));
        assert_eq!(replies[7]["ok"], true);
        assert_eq!(replies[7]["result"]["loaded"], "/m/a.glb");
        assert_eq!(replies[8]["error"], "missing params.path");
        assert_eq!(replies[9]["result"]["opened"], "/p");
        assert_eq!(replies[1]["ok"], true);
        assert_eq!(replies[1]["result"]["inspected"], true);
        assert_eq!(replies[2]["ok"], true);
        assert_eq!(replies[2]["result"]["width"], 1);
        assert_eq!(replies[3]["ok"], true);
        assert_eq!(replies[3]["result"]["count"], 0);
        assert_eq!(replies[4]["ok"], true);
        assert_eq!(replies[4]["result"]["ran"], "view.toggle_grid");
        assert_eq!(replies[5]["ok"], false);
        assert_eq!(replies[5]["error"], "missing params.id (dotted command id)");
        // The accept thread must wake the UI thread on every queued request,
        // or an idle window never drains the queue (2026-11-05 headless smoke).
        assert!(ctx.has_requested_repaint());
        drop(drain);
    }

    #[test]
    fn assets_gc_runs_on_the_connection_thread_dry_by_default() {
        let dir = std::env::temp_dir().join(format!("umber-control-gc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("paint")).unwrap();
        let store = umber_core::AssetStore::new(&dir);
        let live = store.store(b"live", "png").unwrap();
        let orphan = store.store(b"orphan!", "png").unwrap();
        let line = format!("tile-1001.png {} png 4 tile-1001.png.tmp.1", live.hash);
        std::fs::write(
            dir.join("paint/manifest.txt"),
            format!("# Umber paint commit v2\n{line}\n"),
        )
        .unwrap();
        // No drain thread: a UI-forwarded method would time out here.
        let server = start(0, egui::Context::default()).unwrap();
        let token = server.token.clone();
        let path = dir.display().to_string();
        let replies = roundtrip(
            server.port,
            &token,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": token}}).to_string(),
                json!({"id": 1, "method": "assets.gc", "params": {"path": path, "min_age_secs": 0}}).to_string(),
                json!({"id": 2, "method": "assets.gc", "params": {"path": path, "dry_run": false, "min_age_secs": 0}}).to_string(),
                json!({"id": 3, "method": "assets.gc", "params": {}}).to_string(),
                json!({"id": 4, "method": "assets.gc", "params": {"path": path, "dry_run": "yes"}}).to_string(),
            ],
        );
        assert_eq!(replies[1]["ok"], true, "{}", replies[1]);
        assert_eq!(replies[1]["result"]["dry_run"], true);
        assert_eq!(replies[1]["result"]["orphan_bytes"], 7);
        assert_eq!(replies[1]["result"]["deleted_blobs"], 0);
        assert_eq!(replies[2]["result"]["deleted_bytes"], 7);
        assert_eq!(replies[3]["ok"], false);
        assert_eq!(replies[4]["ok"], false);
        assert!(store.contains(&live));
        assert!(!store.contains(&orphan));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn first_request_must_be_auth() {
        let server = start(0, egui::Context::default()).unwrap();
        let token = server.token.clone();
        let replies = roundtrip(
            server.port,
            &token,
            &[json!({"id": 1, "method": "ping", "params": {"token": token}}).to_string()],
        );
        assert_eq!(replies[0]["ok"], false);
        assert_eq!(replies[0]["error"], "unauthorized");
    }

    #[test]
    fn screenshot_reply_encodes_png_base64() {
        let img = egui::ColorImage::filled([2, 2], egui::Color32::RED);
        let v = screenshot_reply(&img).unwrap();
        assert_eq!(v["width"], 2);
        assert_eq!(v["height"], 2);
        assert_eq!(v["format"], "png");
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(v["png_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn bridge_requires_auth() {
        let bridge = start_bridge(0, None).unwrap();

        let replies = roundtrip(
            bridge.port,
            &bridge.token,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": "wrong-wrong-wrong"}})
                    .to_string(),
            ],
        );
        assert_eq!(replies[0]["ok"], false);
        assert_eq!(replies[0]["error"], "unauthorized");
    }

    #[test]
    fn mcp_child_env_carries_the_control_endpoint() {
        let with_file = BridgeControl {
            port: 8391,
            token_file: Some(PathBuf::from("/tmp/umber-app-control-8391.token")),
            token: "secret".into(),
        };
        let cmd = mcp_child_command(Some(&with_file));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&("UMBER_CONTROL_PORT".as_ref(), Some("8391".as_ref()))));
        assert!(envs.contains(&(
            "UMBER_CONTROL_TOKEN_FILE".as_ref(),
            Some("/tmp/umber-app-control-8391.token".as_ref())
        )));
        assert!(!envs.contains(&("UMBER_CONTROL_TOKEN".as_ref(), Some("secret".as_ref()))));

        // Token from env (no file): the child gets the direct-token var.
        let no_file = BridgeControl {
            port: 8391,
            token_file: None,
            token: "secret".into(),
        };
        let cmd = mcp_child_command(Some(&no_file));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&("UMBER_CONTROL_TOKEN".as_ref(), Some("secret".as_ref()))));

        // No control server: no channel env at all (tools fail closed typed).
        let cmd = mcp_child_command(None);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(!envs
            .iter()
            .any(|(k, _)| *k == std::ffi::OsStr::new("UMBER_CONTROL_PORT")));
    }
}
