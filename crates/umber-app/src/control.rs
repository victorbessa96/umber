//! `umber-app --control <port>` — the app-side loopback control channel
//! (queue row 153). v1 answers `auth` + `ping`/`version`/`methods`;
//! `viewport.screenshot` / `ui.inspect` land with the UI-thread command
//! queue in the follow-up kill (design brief
//! docs/loop/briefs/2026-11-02-app-control-channel-design.md).
//!
//! Security model: bind 127.0.0.1 ONLY — there is no interface argument
//! to get wrong. Token auth before any method dispatch, one token from
//! `UMBER_CONTROL_TOKEN` or a fresh 0600 file at
//! `<tmp>/umber-app-control-<port>.token`. Concurrency: the listener runs
//! on a dedicated thread owning its sockets; v1 answers every request
//! in-thread, so no egui state ever crosses a thread boundary.
//!
//! Wire: one JSON object per line each way. Requests
//! `{"id", "method", "params"}`; replies `{"id", "ok": true, "result"}` or
//! `{"id", "ok": false, "error"}`. The first request on every connection
//! must be `auth`.

use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread::JoinHandle;

use serde_json::{json, Value};

use umber_core::control::{self, ConnectionLimiter, LineRead};

pub use umber_core::control::TOKEN_ENV;

/// The methods v1 answers besides `auth`.
pub const METHODS: &[&str] = &["ping", "version", "methods"];

/// A running control listener. Dropping it does NOT stop the thread (the
/// app process owns it); the field on `UmberApp` keeps it alive for the
/// app's lifetime.
pub struct ControlServer {
    /// The bound port (0 at startup means an ephemeral port was picked).
    pub port: u16,
    /// The accepted token (also in `UMBER_CONTROL_TOKEN` or the token file).
    pub token: String,
    /// Where a generated token was written, when the env gave none.
    pub token_file: Option<PathBuf>,
    _thread: JoinHandle<()>,
}

/// `--control` takes a bare port number only. Anything with an interface
/// (`0.0.0.0:8377`, `[::1]:8377`) or non-numeric is refused: the channel
/// is loopback-only, ever.
pub fn parse_control_arg(v: &str) -> Result<u16, String> {
    v.parse::<u16>().map_err(|_| {
        format!("--control: `{v}` is not a bare port number (loopback-only; there is no interface argument)")
    })
}

/// Start the control listener on 127.0.0.1:`port` (0 = ephemeral, for tests).
pub fn start(port: u16) -> std::io::Result<ControlServer> {
    let listener = control::bind_loopback(port)?;
    let port = listener.local_addr()?.port();
    let (token, token_file) = control::server_token("umber-app-control", port)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let thread = std::thread::Builder::new()
        .name("umber-control".into())
        .spawn(move || accept_loop(listener, port, token))?;
    Ok(ControlServer {
        port,
        token,
        token_file,
        _thread: thread,
    })
}

fn accept_loop(listener: TcpListener, port: u16, token: String) {
    let limiter = ConnectionLimiter::default();
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Some(permit) = limiter.try_acquire(control::MAX_CONNECTIONS) {
                    let token = token.clone();
                    let _ = std::thread::Builder::new()
                        .name("umber-control-conn".into())
                        .spawn(move || {
                            let _permit = permit;
                            handle_conn(s, port, &token);
                        });
                }
            }
            Err(_) => continue,
        }
    }
}

fn handle_conn(stream: TcpStream, port: u16, token: &str) {
    let _ = stream.set_nodelay(true);
    let mut reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(_) => return,
    };
    let mut writer = stream;
    let mut line = String::new();

    // The first request must be auth.
    if !matches!(
        control::read_bounded_line(&mut reader, &mut line),
        Ok(LineRead::Line)
    ) {
        return;
    }
    let req = match serde_json::from_str::<Value>(&line) {
        Ok(v) => v,
        Err(_) => {
            let _ = control::write_json_line(
                &mut writer,
                &json!({"id": Value::Null, "ok": false, "error": "malformed json"}),
            );
            return;
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
            &mut writer,
            &json!({"id": id, "ok": false, "error": "unauthorized"}),
        );
        return;
    }
    let _ = control::write_json_line(
        &mut writer,
        &json!({"id": id, "ok": true, "result": {"app": "umber-app", "version": env!("CARGO_PKG_VERSION")}}),
    );

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    fn roundtrip(server: &ControlServer, lines: &[String]) -> Vec<Value> {
        let mut s = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, server.port)).unwrap();
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
    fn bad_token_is_rejected_and_closes() {
        let server = start(0).unwrap();
        let replies = roundtrip(
            &server,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": "wrong-wrong-wrong"}})
                    .to_string(),
            ],
        );
        assert_eq!(replies[0]["ok"], false);
        assert_eq!(replies[0]["error"], "unauthorized");
        // The connection is closed after a failed auth: a further write/read errors or EOFs.
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
    fn auth_then_ping_version_methods_unknown() {
        let server = start(0).unwrap();
        let token = server.token.clone();
        let replies = roundtrip(
            &server,
            &[
                json!({"id": 0, "method": "auth", "params": {"token": token}}).to_string(),
                json!({"id": 1, "method": "ping"}).to_string(),
                json!({"id": 2, "method": "version"}).to_string(),
                json!({"id": 3, "method": "methods"}).to_string(),
                json!({"id": 4, "method": "viewport.screenshot"}).to_string(),
            ],
        );
        assert_eq!(replies[0]["ok"], true);
        assert_eq!(replies[0]["result"]["app"], "umber-app");
        assert_eq!(replies[1]["result"]["pong"], true);
        assert_eq!(replies[1]["result"]["port"], server.port);
        assert_eq!(replies[2]["result"]["app"], "umber-app");
        assert!(replies[2]["result"]["version"].is_string());
        assert_eq!(replies[3]["result"]["methods"][0], "ping");
        // v1: screenshot is not implemented yet — honest unknown_method.
        assert_eq!(replies[4]["ok"], false);
        assert!(replies[4]["error"]
            .as_str()
            .unwrap()
            .starts_with("unknown_method"));
    }

    #[test]
    fn first_request_must_be_auth() {
        let server = start(0).unwrap();
        let token = server.token.clone();
        let replies = roundtrip(
            &server,
            &[json!({"id": 1, "method": "ping", "params": {"token": token}}).to_string()],
        );
        assert_eq!(replies[0]["ok"], false);
        assert_eq!(replies[0]["error"], "unauthorized");
    }
}
