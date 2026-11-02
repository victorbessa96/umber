//! `mcp` — an MCP server over stdio, exposing the agent session
//! ([`crate::agent`]) as tools for AI agents (Claude Code, Hermes, any
//! MCP client).
//!
//! Pattern absorbed 2026-11-01 from storytold/texturecraft
//! `crates/automation` (server.rs + control-protocol.md — MIT OR
//! Apache-2.0, Copyright (c) 2026 ArtCraft Team and the TextureCraft
//! contributors; see THIRD_PARTY.md "Absorbed source files").
//! Upstream speaks MCP through the `rmcp` SDK; umber hand-rolls the
//! wire so the agent surface adds ZERO new protocol dependencies beyond
//! serde_json (rmcp + schemars + tokio stay a queued upgrade for a
//! battery-verified wave). MCP stdio transport = one JSON-RPC 2.0
//! message per line on stdin/stdout; stdout carries only protocol
//! messages, logs go to stderr.
//!
//! Supported methods: `initialize`, `ping`, `tools/list`, `tools/call`.
//! Notifications get no response. File access is confined to the
//! `--read-root` / `--write-root` roots ([`crate::workspace`]); without
//! roots there is no file access.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::agent::Session;
use crate::workspace::{Access, AuthorizedWorkspace};

/// The MCP protocol revision this server speaks.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) fn mcp_cmd(args: &[String]) -> Result<()> {
    let (read_root, write_root) = parse_roots(args)?;
    let ws = AuthorizedWorkspace::new(read_root.as_deref(), write_root.as_deref())?;
    let mut sess = Session::default();
    let mut index = 0usize;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle_message(&mut sess, &ws, &msg, &mut index),
            Err(e) => Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            )),
        };
        if let Some(r) = resp {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
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

/// Handle one JSON-RPC message. Returns `None` for notifications.
fn handle_message(
    sess: &mut Session,
    ws: &AuthorizedWorkspace,
    msg: &Value,
    index: &mut usize,
) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let Some(id) = id else {
        // Notifications (notifications/initialized, notifications/cancelled, …): no response.
        return None;
    };
    match method {
        "initialize" => Some(result_response(
            id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "umber-cli", "version": env!("CARGO_PKG_VERSION") },
            }),
        )),
        "ping" => Some(result_response(id, json!({}))),
        "tools/list" => Some(result_response(id, json!({ "tools": tools_catalog() }))),
        "tools/call" => {
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            Some(call_tool(sess, ws, id, params, index))
        }
        _ => Some(error_response(
            id,
            -32601,
            &format!("method not found: {method}"),
        )),
    }
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// An MCP `tools/call` result.
fn tool_result(id: Value, ok: bool, text: String) -> Value {
    result_response(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "isError": !ok,
        }),
    )
}

fn call_tool(
    sess: &mut Session,
    ws: &AuthorizedWorkspace,
    id: Value,
    params: Value,
    index: &mut usize,
) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match run_tool(sess, ws, name, &arguments, index) {
        Ok(reply) => tool_result(
            id,
            reply["ok"].as_bool().unwrap_or(false),
            serde_json::to_string(&reply).unwrap_or_default(),
        ),
        Err(e) => tool_result(id, false, format!("{e:#}")),
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

/// The static tool catalog (inputSchema hand-written; no schemars dep).
fn tools_catalog() -> Value {
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
        { "name": "paint_stroke", "description": "Paint one stroke through the real headless engine: {points: [[x,y]..], color: [r,g,b,a], radius_scale, radius?, layer?}.", "inputSchema": { "type": "object" } },
        { "name": "paint_read", "description": "Read texels of the selected plane: {texels: [[x,y]..], channel, tile?, layer?, composite?}.", "inputSchema": { "type": "object" } },
        { "name": "paint_snapshot", "description": "Write the selected plane as PNG: exactly one of {out, out_dir} (write-root-relative).", "inputSchema": { "type": "object" } },
        { "name": "set_active", "description": "Choose the working texture set: {set}.", "inputSchema": { "type": "object", "properties": { "set": { "type": "string" } }, "required": ["set"] } },
        { "name": "undo", "description": "Undo one project edit.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "redo", "description": "Redo one project edit.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "paint_undo", "description": "Undo one paint stroke.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "paint_redo", "description": "Redo one paint stroke.", "inputSchema": { "type": "object", "properties": {} } }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> (Session, AuthorizedWorkspace, usize) {
        (Session::default(), AuthorizedWorkspace::default(), 0)
    }

    #[test]
    fn initialize_and_ping_and_unknown_method() {
        let (mut sess, ws, mut idx) = make();
        let init = handle_message(
            &mut sess,
            &ws,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            &mut idx,
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(init["result"]["serverInfo"]["name"], "umber-cli");
        let ping = handle_message(
            &mut sess,
            &ws,
            &json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
            &mut idx,
        )
        .unwrap();
        assert_eq!(ping["result"], json!({}));
        let bad = handle_message(
            &mut sess,
            &ws,
            &json!({"jsonrpc":"2.0","id":3,"method":"nope"}),
            &mut idx,
        )
        .unwrap();
        assert_eq!(bad["error"]["code"], -32601);
        // Notifications get no response.
        assert!(handle_message(
            &mut sess,
            &ws,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            &mut idx
        )
        .is_none());
    }

    #[test]
    fn tools_list_carries_the_catalog() {
        let (mut sess, ws, mut idx) = make();
        let r = handle_message(
            &mut sess,
            &ws,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            &mut idx,
        )
        .unwrap();
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        for want in [
            "version",
            "run_step",
            "run_steps",
            "load_mesh",
            "project_open",
            "bake",
            "export",
            "paint_stroke",
        ] {
            assert!(names.contains(&want), "missing tool {want}: {names:?}");
        }
    }

    #[test]
    fn tools_call_version_needs_no_roots_and_no_mesh() {
        let (mut sess, ws, mut idx) = make();
        let r = handle_message(&mut sess, &ws, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"version","arguments":{}}}), &mut idx).unwrap();
        assert_eq!(r["result"]["isError"], false);
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let reply: Value = serde_json::from_str(text).unwrap();
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["step"], "version");
        assert_eq!(reply["protocol_version"], 1);
    }

    #[test]
    fn file_tools_fail_closed_without_roots() {
        let (mut sess, ws, mut idx) = make();
        for name in ["load_mesh", "project_open", "project_save"] {
            let r = handle_message(&mut sess, &ws, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":{"path":"x"}}}), &mut idx).unwrap();
            assert_eq!(r["result"]["isError"], true, "{name}");
            assert!(
                r["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("root"),
                "{name}: {:?}",
                r["result"]["content"][0]["text"]
            );
        }
    }

    #[test]
    fn run_steps_stops_at_the_first_error() {
        let (mut sess, ws, mut idx) = make();
        let r = handle_message(&mut sess, &ws, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_steps","arguments":{"steps":[{"version":{}},{"state":{}}]}}}), &mut idx).unwrap();
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        let reply: Value = serde_json::from_str(text).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["completed"], 1);
        assert_eq!(reply["failed"]["index"], 1);
        // `state` without a mesh is the honest error, not a crash.
        assert!(reply["failed"]["error"]
            .as_str()
            .unwrap()
            .contains("no mesh loaded"));
    }

    #[test]
    fn paths_are_confined_even_inside_run_step() {
        let base = std::env::temp_dir().join(format!("umber-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let ws = AuthorizedWorkspace::new(Some(&base), Some(&base)).unwrap();
        let (mut sess, _, mut idx) = (Session::default(), (), 0usize);
        let r = handle_message(&mut sess, &ws, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_step","arguments":{"step":{"load":{"mesh":"../../etc/passwd"}}}}}), &mut idx).unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("rejected"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
