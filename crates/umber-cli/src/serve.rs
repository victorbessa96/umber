//! `serve` — a loopback TCP JSON-lines server over the agent session
//! ([`crate::agent`]), for scripts and agents that keep one headless
//! session open over a socket instead of stdio.
//!
//! Pattern absorbed 2026-11-01 from storytold/texturecraft
//! `crates/automation` (rpc.rs + security.rs + docs/control-protocol.md
//! — MIT OR Apache-2.0, Copyright (c) 2026 ArtCraft Team and the
//! TextureCraft contributors; see THIRD_PARTY.md "Absorbed source
//! files"). Umber's version is std-only (no tokio): one thread per
//! connection, each with its OWN session (a shared session would need a
//! mutex around every step and would interleave undo journals — per-
//! connection sessions are the honest shape for v1).
//!
//! Wire: one JSON object per line each way. Requests
//! `{"id", "method", "params"}`; replies `{"id", "ok": true, "result"}`
//! or `{"id", "ok": false, "error"}`. Loopback only (127.0.0.1). The
//! first request on every connection must be
//! `{"id": 0, "method": "auth", "params": {"token": "<token>"}}` — the
//! token is `UMBER_CONTROL_TOKEN` when set, else a fresh one written to
//! `<temp>/umber-control-<port>.token` (owner-only, 0600). Methods:
//! `ping`, `methods`, `step` ({step}), `steps` ({steps: [...]}).
//! Path fields inside steps are confined to the `--read-root` /
//! `--write-root` roots ([`crate::workspace`]); without roots there is
//! no file access.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::agent::Session;
use crate::security::{self, ConnectionLimiter, LineRead, MAX_CONNECTIONS};
use crate::workspace::{Access, AuthorizedWorkspace};

/// The methods `serve` answers (besides `auth`).
pub const METHODS: &[&str] = &["ping", "methods", "step", "steps"];

pub(crate) fn serve_cmd(args: &[String]) -> Result<()> {
    let mut port: Option<u16> = None;
    let mut read_root = None;
    let mut write_root = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                let p = args.get(i).ok_or_else(|| anyhow!("--port needs a number"))?;
                port = Some(p.parse().map_err(|_| anyhow!("--port: `{p}` is not a port number"))?);
            }
            "--read-root" => {
                i += 1;
                read_root = Some(PathBuf::from(args.get(i).ok_or_else(|| anyhow!("--read-root needs a dir"))?));
            }
            "--write-root" => {
                i += 1;
                write_root = Some(PathBuf::from(args.get(i).ok_or_else(|| anyhow!("--write-root needs a dir"))?));
            }
            other => return Err(anyhow!("unknown argument: {other}\nusage: umber-cli serve --port N [--read-root DIR] [--write-root DIR]")),
        }
        i += 1;
    }
    let port = port.ok_or_else(|| {
        anyhow!("usage: umber-cli serve --port N [--read-root DIR] [--write-root DIR]")
    })?;
    let (token, token_path) = security::server_token(port)?;
    match token_path {
        Some(p) => eprintln!("umber-cli: control token written to {}", p.display()),
        None => eprintln!("umber-cli: control token from {}", security::TOKEN_ENV),
    }
    let ws = AuthorizedWorkspace::new(read_root.as_deref(), write_root.as_deref())?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("umber-cli: serving on 127.0.0.1:{port}");
    let limiter = ConnectionLimiter::default();
    for stream in listener.incoming() {
        let stream = stream?;
        let permit = match limiter.try_acquire(MAX_CONNECTIONS) {
            Some(p) => p,
            None => continue, // over the connection limit: drop, keep serving
        };
        let ws = ws.clone();
        let token = token.clone();
        std::thread::spawn(move || {
            let _permit = permit;
            handle_conn(stream, &token, ws);
        });
    }
    Ok(())
}

fn handle_conn(stream: TcpStream, token: &str, ws: AuthorizedWorkspace) {
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut sess = Session::default();
    let mut authed = false;
    let mut index = 0usize;
    let mut line = String::new();
    loop {
        match security::read_bounded_line(&mut reader, &mut line) {
            Ok(LineRead::Eof) => return,
            Ok(LineRead::TooLong) => {
                let _ = write_json(
                    &mut writer,
                    &json!({"id": Value::Null, "ok": false, "error": format!("request over {}-byte limit", security::MAX_REQUEST_BYTES)}),
                );
                continue;
            }
            Ok(LineRead::Line) => {}
            Err(_) => return,
        }
        let (reply, close) = handle_line(&mut sess, &ws, &mut authed, &mut index, token, &line);
        let stop = !write_json(&mut writer, &reply);
        if stop || close {
            return;
        }
    }
}

fn write_json(w: &mut impl Write, v: &Value) -> bool {
    writeln!(w, "{v}").and_then(|()| w.flush()).is_ok()
}

/// Handle one request line. Pure apart from the session — unit-tested.
fn handle_line(
    sess: &mut Session,
    ws: &AuthorizedWorkspace,
    authed: &mut bool,
    index: &mut usize,
    token: &str,
    line: &str,
) -> (Value, bool) {
    let msg: Value = match serde_json::from_str(line) {
        Ok(m) => m,
        Err(e) => {
            return (
                json!({"id": Value::Null, "ok": false, "error": format!("not JSON: {e}")}),
                false,
            )
        }
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let ok = |result: Value| (json!({"id": id, "ok": true, "result": result}), false);
    let err = |e: String, close: bool| (json!({"id": id, "ok": false, "error": e}), close);

    if !*authed {
        if method == "auth" {
            let supplied = params.get("token").and_then(Value::as_str).unwrap_or("");
            if security::token_matches(token, supplied) {
                *authed = true;
                return ok(Value::Null);
            }
            return err("bad token".to_string(), true);
        }
        return err(
            format!(
                "the first request must be auth (token via ${} or {})",
                security::TOKEN_ENV,
                security::token_file(0).display()
            ),
            true,
        );
    }
    match method {
        "ping" => ok(json!("pong")),
        "methods" => ok(json!(METHODS)),
        "step" => {
            let Some(mut step) = params.get("step").cloned() else {
                return err("`step` must be an object".to_string(), false);
            };
            let mode = step_mode(&step);
            if let Err(e) = ws.confine_step(&mut step, mode) {
                return err(format!("{e:#}"), false);
            }
            let (reply, _quit) = sess.reply(*index, &format!("{step}\n"));
            *index += 1;
            ok(reply)
        }
        "steps" => {
            let steps = match params.get("steps").and_then(Value::as_array) {
                Some(s) if s.len() <= security::MAX_BATCH_STEPS => s.clone(),
                Some(s) => {
                    return err(
                        format!(
                            "`steps` has {} entries (limit {})",
                            s.len(),
                            security::MAX_BATCH_STEPS
                        ),
                        false,
                    )
                }
                None => return err("`steps` must be an array".to_string(), false),
            };
            let mut results = Vec::new();
            for (i, s) in steps.iter().enumerate() {
                let mut s = s.clone();
                let mode = step_mode(&s);
                if let Err(e) = ws.confine_step(&mut s, mode) {
                    return err(format!("steps[{i}]: {e:#}"), false);
                }
                let (reply, _quit) = sess.reply(*index, &format!("{s}\n"));
                *index += 1;
                let step_ok = reply["ok"].as_bool().unwrap_or(false);
                results.push(reply.clone());
                if !step_ok {
                    return ok(
                        json!({"completed": i, "results": results, "failed": {"index": i, "error": reply.get("error").cloned().unwrap_or(Value::Null)}}),
                    );
                }
            }
            ok(json!({"completed": results.len(), "results": results, "failed": Value::Null}))
        }
        _ => err(format!("unknown method: {method} (see `methods`)"), false),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (Session, AuthorizedWorkspace, bool, usize) {
        (Session::default(), AuthorizedWorkspace::default(), false, 0)
    }

    #[test]
    fn auth_is_required_first_and_bad_token_closes() {
        let (mut sess, ws, mut authed, mut idx) = fresh();
        let token = "0123456789abcdef0123456789abcdef";
        let (r, close) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            r#"{"id":1,"method":"ping"}"#,
        );
        assert_eq!(r["ok"], false);
        assert!(close);
        assert!(!authed);
        let (r, close) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            r#"{"id":0,"method":"auth","params":{"token":"wrong-wrong-wrong-wrong"}}"#,
        );
        assert_eq!(r["ok"], false);
        assert!(close);
        let (r, close) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            &json!({"id":0,"method":"auth","params":{"token":token}}).to_string(),
        );
        assert_eq!(r["ok"], true);
        assert!(!close);
        assert!(authed);
    }

    #[test]
    fn after_auth_ping_and_methods_and_version_step_work() {
        let (mut sess, ws, mut authed, mut idx) = fresh();
        let token = "0123456789abcdef0123456789abcdef";
        handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            &json!({"id":0,"method":"auth","params":{"token":token}}).to_string(),
        );
        let (r, _) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            r#"{"id":1,"method":"ping"}"#,
        );
        assert_eq!(r["result"], "pong");
        let (r, _) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            r#"{"id":2,"method":"methods"}"#,
        );
        assert!(r["result"].as_array().unwrap().contains(&json!("step")));
        let (r, _) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            r#"{"id":3,"method":"step","params":{"step":{"version":{}}}}"#,
        );
        assert_eq!(r["ok"], true);
        assert_eq!(r["result"]["protocol_version"], 1);
    }

    #[test]
    fn step_paths_fail_closed_without_roots() {
        let (mut sess, ws, mut authed, mut idx) = fresh();
        let token = "0123456789abcdef0123456789abcdef";
        handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            &json!({"id":0,"method":"auth","params":{"token":token}}).to_string(),
        );
        let (r, _) = handle_line(
            &mut sess,
            &ws,
            &mut authed,
            &mut idx,
            token,
            r#"{"id":4,"method":"step","params":{"step":{"load":{"mesh":"/etc/passwd"}}}}"#,
        );
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("rejected"));
    }

    #[test]
    fn malformed_line_gets_an_error_envelope_and_the_loop_continues() {
        let (mut sess, ws, mut authed, mut idx) = fresh();
        let (r, close) = handle_line(&mut sess, &ws, &mut authed, &mut idx, "tok", "not json");
        assert_eq!(r["ok"], false);
        assert!(!close);
    }
}
