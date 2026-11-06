//! `mcp` — an MCP server over stdio, exposing the agent session
//! ([`crate::agent`]) as tools for AI agents (Claude Code, Hermes, any
//! MCP client).
//!
//! Pattern absorbed 2026-11-01 from storytold/texturecraft
//! `crates/automation` (server.rs + control-protocol.md — MIT OR
//! Apache-2.0, Copyright (c) 2026 ArtCraft Team and the TextureCraft
//! contributors; see THIRD_PARTY.md "Absorbed source files").
//! Upstream serves MCP through the `rmcp` SDK; queue row 155 moved umber
//! onto the same SDK: the stdio server is an `rmcp` [`ServerHandler`]
//! served on a tokio runtime (`transport-io`), and the exposed tool
//! catalog is pinned byte-identical by the tests at the bottom of this
//! file. MCP stdio transport = one JSON-RPC 2.0 message per line on
//! stdin/stdout; stdout carries only protocol messages, logs go to
//! stderr.
//!
//! Supported methods: `initialize`, `ping`, `tools/list`, `tools/call`.
//! Notifications get no response. File access is confined to the
//! `--read-root` / `--write-root` roots ([`crate::workspace`]); without
//! roots there is no file access. The `command_list` / `command_run`
//! tools are different: they carry no paths, they forward to a LIVE
//! umber-app's `--control` channel when the session has one
//! (`UMBER_CONTROL_PORT` + `UMBER_CONTROL_TOKEN_FILE` in the env) and
//! answer a typed error when it does not (see [`crate::agent`]).
//! `library_list` (assets wave K6) likewise carries no arguments: it
//! scans the bundled starter `assets/` plus the per-user library dirs
//! and replies rows `{path, kind, name, bytes, source}`; `import_asset`
//! rides `run_step` / `run_steps` with its `file` (read root) and
//! `library` (write root) paths confined like every file tool.

use std::borrow::Cow;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig,
    Tool, ToolsCapability,
};
use rmcp::service::{MaybeSendFuture, RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServiceExt};

use crate::agent::Session;
use crate::workspace::{Access, AuthorizedWorkspace};

/// The MCP protocol revision this server speaks (pinned: the
/// `supported_protocol_versions` list holds exactly this one).
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// The one protocol revision this server negotiates.
static SUPPORTED_PROTOCOL_VERSIONS: [ProtocolVersion; 1] = [ProtocolVersion::V_2025_06_18];

pub(crate) fn mcp_cmd(args: &[String]) -> Result<()> {
    let (read_root, write_root) = parse_roots(args)?;
    let ws = AuthorizedWorkspace::new(read_root.as_deref(), write_root.as_deref())?;
    let server = McpServer::new(ws);
    // One stdio client at a time; a current-thread runtime is enough
    // because tool dispatch is synchronous behind the state mutex.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let service = server.serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok(())
    })
}

fn parse_roots(args: &[String]) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    let mut read_root = None;
    let mut write_root = None;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        i += 1;
        match flag.as_str() {
            "--read-root" | "--write-root" => {
                let dir = args
                    .get(i)
                    .cloned()
                    .map(PathBuf::from)
                    .ok_or_else(|| anyhow!("{flag} needs a directory"))?;
                i += 1;
                if flag == "--read-root" {
                    read_root = Some(dir);
                } else {
                    write_root = Some(dir);
                }
            }
            other => {
                return Err(anyhow!(
                    "unknown argument: {other}\nusage: umber-cli mcp [--read-root DIR] [--write-root DIR]"
                ));
            }
        }
    }
    Ok((read_root, write_root))
}

/// The rmcp server: the confined workspace plus the mutable session
/// state (session + step index) behind one mutex — `ServerHandler` is
/// `&self`, and a poisoned mutex degrades to the inner value, never a
/// panic.
struct McpServer {
    ws: AuthorizedWorkspace,
    state: Mutex<(SendableSession, usize)>,
}

/// `Session` is `!Send` only because the paint undo stack carries a
/// `Box<dyn FnMut>` callback. The MCP server touches the session solely
/// through `McpServer::state`'s mutex, and the stdio service runs on a
/// current-thread tokio runtime, so the guard never crosses threads in
/// practice; the wrapper makes that explicit for the `ServerHandler:
/// Send + Sync` bound.
struct SendableSession(Session);
// SAFETY: all access goes through the `state` mutex; the runtime is
// single-threaded, so no `&Session` is ever shared across threads.
unsafe impl Send for SendableSession {}

impl McpServer {
    fn new(ws: AuthorizedWorkspace) -> Self {
        Self {
            ws,
            state: Mutex::new((SendableSession(Session::default()), 0)),
        }
    }

    #[cfg(test)]
    fn with_session(ws: AuthorizedWorkspace, sess: Session) -> Self {
        Self {
            ws,
            state: Mutex::new((SendableSession(sess), 0)),
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, (SendableSession, usize)> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        // `{"tools": {"listChanged": false}}` — the same capabilities
        // object the hand-rolled server sent.
        let mut tools = ToolsCapability::default();
        tools.list_changed = Some(false);
        let mut caps = ServerCapabilities::default();
        caps.tools = Some(tools);
        ServerConfig::new(caps)
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_server_info(Implementation::new("umber-cli", env!("CARGO_PKG_VERSION")))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        debug_assert_eq!(
            SUPPORTED_PROTOCOL_VERSIONS[0].as_str(),
            MCP_PROTOCOL_VERSION
        );
        Cow::Borrowed(&SUPPORTED_PROTOCOL_VERSIONS)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(tools_catalog())))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, McpError>> + MaybeSendFuture + '_ {
        let name = request.name.into_owned();
        let arguments = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| json!({}));
        // Tool-level failures (unknown tool, step error, missing roots,
        // no control channel) are `isError` results, not JSON-RPC
        // errors — the same shape the hand-rolled server replied with.
        let response = {
            let mut state = self.lock_state();
            let (sess, index) = &mut *state;
            match run_tool(&mut sess.0, &self.ws, &name, &arguments, index) {
                Ok(reply) => {
                    let ok = reply["ok"].as_bool().unwrap_or(false);
                    let text = serde_json::to_string(&reply).unwrap_or_default();
                    let content = vec![ContentBlock::text(text)];
                    if ok {
                        CallToolResult::success(content)
                    } else {
                        CallToolResult::error(content)
                    }
                }
                Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("{e:#}"))]),
            }
        };
        std::future::ready(Ok(CallToolResponse::from(response)))
    }
}

/// Run one tool: build the agent step, confine its paths, dispatch.
fn run_tool(
    sess: &mut Session,
    ws: &AuthorizedWorkspace,
    name: &str,
    args: &Value,
    index: &mut usize,
) -> Result<Value> {
    let step = match name {
        "version" => json!({ "version": {} }),
        "session_inspect" => json!({ "project_state": {} }),
        "run_step" => {
            let mut step = args.get("step").cloned().ok_or_else(|| {
                anyhow!("`step` must be an object, e.g. {{\"bake_status\": {{}}}}")
            })?;
            let mode = step_mode(&step);
            ws.confine_step(&mut step, mode)?;
            confine_import_asset(ws, &mut step)?;
            step
        }
        "run_steps" => {
            let steps = args
                .get("steps")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("`steps` must be an array of step objects"))?;
            if steps.len() > crate::security::MAX_BATCH_STEPS {
                return Err(anyhow!(
                    "`steps` has {} entries (limit {})",
                    steps.len(),
                    crate::security::MAX_BATCH_STEPS
                ));
            }
            let mut results = Vec::new();
            for (i, s) in steps.iter().enumerate() {
                let mut s = s.clone();
                let mode = step_mode(&s);
                ws.confine_step(&mut s, mode)?;
                confine_import_asset(ws, &mut s)?;
                let reply = dispatch(sess, s, index);
                let ok = reply["ok"].as_bool().unwrap_or(false);
                results.push(reply.clone());
                if !ok {
                    return Ok(json!({
                        "ok": false,
                        "completed": i,
                        "results": results,
                        "failed": { "index": i, "error": reply.get("error").cloned().unwrap_or(Value::Null) },
                    }));
                }
            }
            return Ok(
                json!({ "ok": true, "completed": results.len(), "results": results, "failed": Value::Null }),
            );
        }
        "load_mesh" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("`path` (read-root-relative) is required"))?;
            let host = ws.host_read_path(path)?;
            json!({ "load": { "mesh": host } })
        }
        "project_open" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("`path` (read-root-relative) is required"))?;
            let host = ws.host_read_path(path)?;
            json!({ "project_open": { "dir": host } })
        }
        "project_save" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("`path` (write-root-relative) is required"))?;
            let host = ws.host_write_path(path)?;
            json!({ "project_save": { "dir": host } })
        }
        "bake" | "export" | "paint_stroke" | "paint_snapshot" | "set_active" | "paint_read" => {
            // Passthrough: the arguments ARE the step body; confine the
            // path-bearing fields (out/out_dir write; dir/path by step mode).
            let mut wrapped = json!({ name: args.clone() });
            let mode = step_mode(&wrapped);
            ws.confine_step(&mut wrapped, mode)?;
            wrapped
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow!("internal: step body lost"))?
        }
        "bake_status" => json!({ "bake_status": {} }),
        "command_list" => json!({ "command_list": {} }),
        "library_list" => json!({ "library_list": {} }),
        "command_run" => {
            let id = args.get("id").and_then(Value::as_str).ok_or_else(|| {
                anyhow!("`id` (dotted command id, e.g. \"view.toggle_grid\") is required")
            })?;
            json!({ "command_run": { "id": id } })
        }
        "undo" => json!({ "undo": {} }),
        "redo" => json!({ "redo": {} }),
        "paint_undo" => json!({ "paint_undo": {} }),
        "paint_redo" => json!({ "paint_redo": {} }),
        other => return Err(anyhow!("unknown tool: {other} (see tools/list)")),
    };
    Ok(dispatch(sess, step, index))
}

/// Dispatch one agent step through the session and return its reply envelope.
fn dispatch(sess: &mut Session, step: Value, index: &mut usize) -> Value {
    let line = format!("{step}\n");
    let (reply, _quit) = sess.reply(*index, &line);
    *index += 1;
    reply
}

/// Which workspace side a step's ambiguous path keys (`dir`, `path`) mean.
fn step_mode(step: &Value) -> Access {
    let name = step
        .as_object()
        .and_then(|o| o.keys().next())
        .cloned()
        .unwrap_or_default();
    match name.as_str() {
        "project_save" | "export" | "bake" | "paint_snapshot" => Access::Write,
        _ => Access::Read,
    }
}

/// Confine an `import_asset` step's path fields — `file` reads, an
/// explicit `library` writes (`confine_step` only knows the shared key
/// set). Without roots the step fails closed like every file tool;
/// an absent `library` keeps K5's per-user library dir default.
fn confine_import_asset(ws: &AuthorizedWorkspace, step: &mut Value) -> Result<()> {
    let Some(body) = step
        .as_object_mut()
        .and_then(|o| o.get_mut("import_asset"))
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    for (key, access) in [("file", Access::Read), ("library", Access::Write)] {
        if let Some(v) = body.get_mut(key) {
            if let Some(s) = v.as_str() {
                let host = match access {
                    Access::Read => ws.host_read_path(s)?,
                    Access::Write => ws.host_write_path(s)?,
                };
                *v = Value::String(host.display().to_string());
            }
        }
    }
    Ok(())
}

/// The static tool catalog as the exact JSON the pre-rmcp server sent
/// on `tools/list` (row 155 pin: names, descriptions, and inputSchema
/// shapes are byte-identical to that wire array). Assets wave K6
/// (2026-11-05) appended exactly one tool, `library_list`, after
/// `command_run`; the pre-K6 entries are untouched.
fn catalog_json() -> Value {
    json!([
        { "name": "version", "description": "Protocol and build versions: {\"protocol_version\", \"umber_version\"}.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "session_inspect", "description": "The session as JSON: the open project's census (sets, layers, journal seq) or the held mesh summary.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "run_step", "description": "Run one agent-protocol step, e.g. {\"step\": {\"layer_add\": {...}}}. Path fields are confined to the roots. Stops at the step's error.", "inputSchema": { "type": "object", "properties": { "step": { "type": "object" } }, "required": ["step"] } },
        { "name": "run_steps", "description": "Run several steps in order (max 256); stops at the first error and reports {completed, results, failed}.", "inputSchema": { "type": "object", "properties": { "steps": { "type": "array", "items": { "type": "object" } } }, "required": ["steps"] } },
        { "name": "load_mesh", "description": "Load a mesh file (path relative to the read root) and hold it for the session.", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] } },
        { "name": "project_open", "description": "Open a .umber project directory (path relative to the read root).", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] } },
        { "name": "project_save", "description": "Write the open project to a directory (path relative to the write root).", "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] } },
        { "name": "bake", "description": "Bake mesh maps headless: {maps, out_dir, size, rays, dilate, margin, tiles} — same fields as the agent `bake` step; out_dir is write-root-relative.", "inputSchema": { "type": "object" } },
        { "name": "bake_status", "description": "Poll the running bake job.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "export", "description": "Export textures with a preset: {preset, out_dir, size, rays, tiles, usda}; out_dir is write-root-relative.", "inputSchema": { "type": "object" } },
        { "name": "paint_stroke", "description": "Paint one stroke through the real headless engine: {points: [[x,y]..], color: [r,g,b,a], radius_scale, radius?, radius_texels?, layer?}.", "inputSchema": { "type": "object" } },
        { "name": "paint_read", "description": "Read texels of the selected plane: {texels: [[x,y]..], channel, tile?, layer?, composite?}.", "inputSchema": { "type": "object" } },
        { "name": "paint_snapshot", "description": "Write the selected plane as PNG: exactly one of {out, out_dir} (write-root-relative).", "inputSchema": { "type": "object" } },
        { "name": "set_active", "description": "Choose the working texture set: {set}.", "inputSchema": { "type": "object", "properties": { "set": { "type": "string" } }, "required": ["set"] } },
        { "name": "undo", "description": "Undo one project edit.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "redo", "description": "Redo one project edit.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "paint_undo", "description": "Undo one paint stroke.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "paint_redo", "description": "Redo one paint stroke.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "command_list", "description": "List the live app's command registry (id, label, menu, shortcut, params, enabled) over its --control channel. Typed error when no channel is configured (UMBER_CONTROL_PORT/UMBER_CONTROL_TOKEN_FILE).", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "command_run", "description": "Run one GUI command by dotted id on the live app's --control channel (same dispatch as a menu click): {id}. Unknown/disabled ids are the app's typed errors.", "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] } },
        { "name": "library_list", "description": "List the asset libraries headless — the bundled starter assets/ plus the per-user library dirs (state-dir smart/, brushes/, env/) — with the assets panel's own scan rules. Replies rows {path, kind, name, bytes, source: starter|user} for smart materials, smart masks, smart filters, brushes and env maps. No arguments; no control channel needed.", "inputSchema": { "type": "object", "properties": {} } }
    ])
}

/// The catalog as rmcp `Tool`s. Every optional `Tool` field stays
/// `None`, so each tool serializes as exactly
/// `{"name": ..., "description": ..., "inputSchema": {...}}` — the
/// pre-rmcp wire shape (pinned by the tests).
fn tools_catalog() -> Vec<Tool> {
    catalog_json()
        .as_array()
        .expect("catalog_json is an array")
        .iter()
        .map(|entry| {
            let name = entry["name"].as_str().expect("tool name").to_string();
            let description = entry["description"]
                .as_str()
                .expect("tool description")
                .to_string();
            let input_schema = entry["inputSchema"]
                .as_object()
                .expect("tool inputSchema")
                .clone();
            Tool::new(name, description, Arc::new(input_schema))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// The client end of the wire: the server is served over a tokio
    /// duplex pair, so tests speak real newline-delimited JSON-RPC.
    struct Wire {
        rd: BufReader<tokio::io::ReadHalf<DuplexStream>>,
        wr: tokio::io::WriteHalf<DuplexStream>,
    }

    impl Wire {
        async fn send(&mut self, msg: Value) {
            let mut line = serde_json::to_string(&msg).unwrap();
            line.push('\n');
            self.wr.write_all(line.as_bytes()).await.unwrap();
            self.wr.flush().await.unwrap();
        }

        async fn recv(&mut self) -> Value {
            let mut line = String::new();
            let n = self.rd.read_line(&mut line).await.unwrap();
            assert!(n > 0, "server closed the wire");
            serde_json::from_str(&line).unwrap()
        }

        async fn call(&mut self, id: i64, method: &str, params: Value) -> Value {
            self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .await;
            self.recv().await
        }
    }

    /// Serve one server on a duplex pair; returns the client wire. The
    /// spawned task runs `serve` (which waits for `initialize`) and then
    /// the message loop until the client end drops.
    async fn start(ws: AuthorizedWorkspace, sess: Session) -> Wire {
        let (client, server_io) = tokio::io::duplex(1 << 16);
        let (rd, wr) = tokio::io::split(client);
        let server = McpServer::with_session(ws, sess);
        tokio::spawn(async move {
            let svc = server.serve(server_io).await.expect("serve");
            let _ = svc.waiting().await;
        });
        Wire {
            rd: BufReader::new(rd),
            wr,
        }
    }

    fn default_ws() -> AuthorizedWorkspace {
        AuthorizedWorkspace::default()
    }

    fn no_control() -> Session {
        Session::with_control(None)
    }

    /// initialize + notifications/initialized, asserting the pinned
    /// handshake fields.
    async fn handshake(w: &mut Wire) {
        let init = w
            .call(
                1,
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "test-client", "version": "0.0.0" },
                }),
            )
            .await;
        assert_eq!(init["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(init["result"]["serverInfo"]["name"], "umber-cli");
        assert_eq!(
            init["result"]["capabilities"],
            json!({ "tools": { "listChanged": false } })
        );
        // Notifications get no response: the next reply is the ping's.
        w.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
    }

    fn call_tool<'a>(
        w: &'a mut Wire,
        id: i64,
        name: &'a str,
        arguments: Value,
    ) -> impl Future<Output = Value> + 'a {
        w.call(
            id,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    fn result_text(r: &Value) -> (bool, Value) {
        let is_error = r["result"]["isError"].as_bool().unwrap();
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let reply: Value = serde_json::from_str(text).unwrap_or(Value::String(text.to_string()));
        (is_error, reply)
    }

    #[test]
    fn catalog_pin_is_byte_identical_to_the_pre_rmcp_wire_array() {
        let tools = tools_catalog();
        // Count pin: 21 tools (the 20 pre-K6 pins plus library_list).
        assert_eq!(tools.len(), 21, "{tools:?}");
        // Value equality per tool (name, description, inputSchema).
        let wire = serde_json::to_value(&tools).unwrap();
        assert_eq!(wire, catalog_json());
        // Byte equality of the canonical (sorted-key) serialization: the
        // pre-rmcp server put the catalog on the wire as a
        // serde_json::Value (BTreeMap, keys sorted); rmcp serializes the
        // Tool structs in field order. Same JSON, same bytes once
        // canonicalized — that is the byte-identical pin.
        assert_eq!(
            serde_json::to_string(&wire).unwrap(),
            serde_json::to_string(&catalog_json()).unwrap()
        );
        // Name order pin: the pre-K6 20 stay byte-identical in their
        // positions; K6's ONE intentional addition is library_list last.
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                "version",
                "session_inspect",
                "run_step",
                "run_steps",
                "load_mesh",
                "project_open",
                "project_save",
                "bake",
                "bake_status",
                "export",
                "paint_stroke",
                "paint_read",
                "paint_snapshot",
                "set_active",
                "undo",
                "redo",
                "paint_undo",
                "paint_redo",
                "command_list",
                "command_run",
                "library_list",
            ],
            "{names:?}"
        );
        let run = tools.iter().find(|t| t.name == "command_run").unwrap();
        assert_eq!(run.input_schema["required"], json!(["id"]));
        // The one addition, pinned explicitly: name + schema.
        let list = tools.iter().find(|t| t.name == "library_list").unwrap();
        assert_eq!(
            list.description.as_deref(),
            Some("List the asset libraries headless — the bundled starter assets/ plus the per-user library dirs (state-dir smart/, brushes/, env/) — with the assets panel's own scan rules. Replies rows {path, kind, name, bytes, source: starter|user} for smart materials, smart masks, smart filters, brushes and env maps. No arguments; no control channel needed.")
        );
        assert_eq!(
            serde_json::to_value(&list.input_schema).unwrap(),
            json!({ "type": "object", "properties": {} })
        );
    }

    #[test]
    fn initialize_and_ping_and_unknown_method_over_the_wire() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let ping = w.call(2, "ping", json!({})).await;
            assert_eq!(ping["result"], json!({}));
            let bad = w.call(3, "nope", json!({})).await;
            assert_eq!(bad["error"]["code"], -32601);
        });
    }

    #[test]
    fn tools_list_over_the_wire_matches_the_pinned_catalog() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = w.call(4, "tools/list", json!({})).await;
            let tools = r["result"]["tools"].clone();
            assert_eq!(tools.as_array().unwrap().len(), 21, "{tools}");
            // Canonical (sorted-key) byte pin against the pre-rmcp wire
            // array — see the catalog_pin test for why canonicalized.
            assert_eq!(
                serde_json::to_string(&tools).unwrap(),
                serde_json::to_string(&catalog_json()).unwrap()
            );
        });
    }

    #[test]
    fn tools_call_version_needs_no_roots_and_no_mesh() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(&mut w, 5, "version", json!({})).await;
            let (is_error, reply) = result_text(&r);
            assert!(!is_error);
            assert_eq!(reply["ok"], true);
            assert_eq!(reply["step"], "version");
            assert_eq!(reply["protocol_version"], 1);
        });
    }

    #[test]
    fn file_tools_fail_closed_without_roots() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            for name in ["load_mesh", "project_open", "project_save"] {
                let r = call_tool(&mut w, 6, name, json!({"path": "x"})).await;
                let (is_error, reply) = result_text(&r);
                assert!(is_error, "{name}");
                let text = serde_json::to_string(&reply).unwrap();
                assert!(text.contains("root"), "{name}: {text}");
            }
        });
    }

    #[test]
    fn run_steps_stops_at_the_first_error() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(
                &mut w,
                7,
                "run_steps",
                json!({"steps": [{"version": {}}, {"state": {}}]}),
            )
            .await;
            let (is_error, reply) = result_text(&r);
            assert!(is_error);
            assert_eq!(reply["ok"], false);
            assert_eq!(reply["completed"], 1);
            assert_eq!(reply["failed"]["index"], 1);
            // `state` without a mesh is the honest error, not a crash.
            assert!(reply["failed"]["error"]
                .as_str()
                .unwrap()
                .contains("no mesh loaded"));
        });
    }

    #[test]
    fn paths_are_confined_even_inside_run_step() {
        runtime().block_on(async {
            let base = std::env::temp_dir().join(format!("umber-mcp-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&base).unwrap();
            let ws = AuthorizedWorkspace::new(Some(&base), Some(&base)).unwrap();
            let mut w = start(ws, no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(
                &mut w,
                8,
                "run_step",
                json!({"step": {"load": {"mesh": "../../etc/passwd"}}}),
            )
            .await;
            let (is_error, reply) = result_text(&r);
            assert!(is_error);
            assert!(
                serde_json::to_string(&reply).unwrap().contains("rejected"),
                "{reply}"
            );
            let _ = std::fs::remove_dir_all(&base);
        });
    }

    #[test]
    fn unknown_tool_is_a_tool_level_error_not_a_protocol_error() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(&mut w, 9, "nope", json!({})).await;
            assert!(r.get("error").is_none(), "{r}");
            let (is_error, reply) = result_text(&r);
            assert!(is_error);
            assert!(
                serde_json::to_string(&reply)
                    .unwrap()
                    .contains("unknown tool: nope"),
                "{reply}"
            );
        });
    }

    #[test]
    fn command_run_requires_an_id_argument() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(&mut w, 10, "command_run", json!({})).await;
            let (is_error, reply) = result_text(&r);
            assert!(is_error);
            assert!(
                serde_json::to_string(&reply).unwrap().contains("`id`"),
                "{reply}"
            );
        });
    }

    #[test]
    fn command_tools_fail_closed_without_a_control_channel() {
        runtime().block_on(async {
            // Pin the endpoint off: the env may name a channel in some
            // environments, this test must not depend on it.
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            for (name, arguments) in [
                ("command_list", json!({})),
                ("command_run", json!({"id": "view.toggle_grid"})),
            ] {
                let r = call_tool(&mut w, 11, name, arguments).await;
                let (is_error, reply) = result_text(&r);
                assert!(is_error, "{name}");
                assert_eq!(reply["ok"], false, "{name}: {reply}");
                assert!(
                    reply["error"]
                        .as_str()
                        .unwrap()
                        .contains("no live app control channel (set UMBER_CONTROL_PORT/UMBER_CONTROL_TOKEN_FILE)"),
                    "{name}: {reply}"
                );
            }
        });
    }

    /// A fresh per-test scratch dir under the system temp dir.
    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("umber-mcp-library-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn library_list_over_the_wire_populated_then_empty_scratch_library() {
        // UMBER_ASSETS_DIR points the starter root at scratch trees; the
        // lock (shared with agent.rs's env-sensitive test) keeps the
        // parallel tests from racing the process-global env.
        let _guard = crate::agent::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let old = std::env::var_os(crate::agent::ASSETS_DIR_ENV);
        runtime().block_on(async {
            // Populated scratch starter: one row per kind, .txt skipped.
            let assets = scratch("populated");
            for sub in ["smart", "brushes", "env"] {
                std::fs::create_dir_all(assets.join(sub)).unwrap();
            }
            std::fs::write(assets.join("smart/mat.umbersmart"), b"{}").unwrap();
            std::fs::write(assets.join("smart/mask.umbermask"), b"{}").unwrap();
            std::fs::write(assets.join("smart/filter.umberfilter"), b"{}").unwrap();
            std::fs::write(assets.join("brushes/b.umberbrush"), b"{}").unwrap();
            std::fs::write(assets.join("env/studio.png"), b"12345").unwrap();
            std::fs::write(assets.join("env/skip.txt"), b"no").unwrap();
            std::env::set_var(crate::agent::ASSETS_DIR_ENV, &assets);
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(&mut w, 12, "library_list", json!({})).await;
            let (is_error, reply) = result_text(&r);
            assert!(!is_error, "{reply}");
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(reply["step"], "library_list", "{reply}");
            let rows = reply["rows"].as_array().unwrap();
            assert_eq!(reply["count"], 5, "{reply}");
            assert_eq!(rows.len(), 5, "{reply}");
            let mut kinds: Vec<&str> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
            kinds.sort();
            assert_eq!(
                kinds,
                [
                    "brush",
                    "env",
                    "smart-filter",
                    "smart-mask",
                    "smart-material"
                ],
                "{reply}"
            );
            for row in rows {
                assert_eq!(row["source"], "starter", "{row}");
                assert!(
                    row["path"]
                        .as_str()
                        .unwrap()
                        .starts_with(assets.to_str().unwrap()),
                    "{row}"
                );
                assert!(row["name"].is_string() && row["bytes"].is_u64(), "{row}");
            }
            let env_row = rows.iter().find(|r| r["kind"] == "env").unwrap();
            assert_eq!(env_row["name"], "studio", "{env_row}");
            assert_eq!(env_row["bytes"], 5, "{env_row}");
            // Empty scratch library: zero rows, still ok — never an error.
            let empty = scratch("empty");
            std::env::set_var(crate::agent::ASSETS_DIR_ENV, &empty);
            let r = call_tool(&mut w, 13, "library_list", json!({})).await;
            let (is_error, reply) = result_text(&r);
            assert!(!is_error, "{reply}");
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(reply["rows"], json!([]), "{reply}");
            assert_eq!(reply["count"], 0, "{reply}");
        });
        match old {
            Some(v) => std::env::set_var(crate::agent::ASSETS_DIR_ENV, v),
            None => std::env::remove_var(crate::agent::ASSETS_DIR_ENV),
        }
    }

    #[test]
    fn import_asset_via_run_step_is_confined_and_delegates_to_k5() {
        runtime().block_on(async {
            let base = scratch("import");
            let read = base.join("read");
            let write = base.join("write");
            std::fs::create_dir_all(&read).unwrap();
            std::fs::create_dir_all(&write).unwrap();
            // A valid brush inside the read root (a committed repo asset).
            let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/brushes/hard-round.umberbrush");
            std::fs::copy(&src, read.join("hard-round.umberbrush")).unwrap();
            let ws = AuthorizedWorkspace::new(Some(&read), Some(&write)).unwrap();
            let mut w = start(ws, no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(
                &mut w,
                14,
                "run_step",
                json!({"step": {"import_asset": {
                    "file": "hard-round.umberbrush", "kind": "brush", "library": "lib"
                }}}),
            )
            .await;
            let (is_error, reply) = result_text(&r);
            assert!(!is_error, "{reply}");
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(reply["step"], "import_asset", "{reply}");
            assert_eq!(reply["kind"], "brush", "{reply}");
            let target = write.join("lib/hard-round.umberbrush");
            // Compare canonical paths: the confined workspace reports the
            // canonical form, which on Windows is the `\\?\` verbatim
            // path with long names, while the temp dir can come back in
            // 8.3 short form (`RUNNER~1`), so the strings differ for the
            // same file.
            let reported = PathBuf::from(reply["target"].as_str().expect("target string"));
            assert_eq!(
                reported.canonicalize().unwrap(),
                target.canonicalize().unwrap(),
                "{reply}"
            );
            assert_eq!(
                reply["bytes"],
                std::fs::metadata(&src).unwrap().len(),
                "{reply}"
            );
            assert!(target.is_file(), "the bytes landed inside the write root");
            assert_eq!(
                std::fs::read(&target).unwrap(),
                std::fs::read(&src).unwrap()
            );
            // An escape out of the read root is rejected.
            let r = call_tool(
                &mut w,
                15,
                "run_step",
                json!({"step": {"import_asset": {"file": "../../etc/passwd", "kind": "env"}}}),
            )
            .await;
            let (is_error, reply) = result_text(&r);
            assert!(is_error, "{reply}");
            assert!(
                serde_json::to_string(&reply).unwrap().contains("rejected"),
                "{reply}"
            );
            let _ = std::fs::remove_dir_all(&base);
        });
    }

    #[test]
    fn import_asset_via_run_step_fails_closed_without_roots() {
        runtime().block_on(async {
            let mut w = start(default_ws(), no_control()).await;
            handshake(&mut w).await;
            let r = call_tool(
                &mut w,
                16,
                "run_step",
                json!({"step": {"import_asset": {"file": "x.png", "kind": "env"}}}),
            )
            .await;
            let (is_error, reply) = result_text(&r);
            assert!(is_error, "{reply}");
            assert!(
                serde_json::to_string(&reply).unwrap().contains("root"),
                "{reply}"
            );
        });
    }
}
