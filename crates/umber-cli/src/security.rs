//! Tokens and bounded framing for the loopback JSON-lines transports
//! (`umber-cli serve`'s TCP channel; the app-side `--control` channel
//! shares these conventions).
//!
//! Absorbed 2026-11-01 from storytold/texturecraft
//! `crates/automation/src/security.rs` — MIT OR Apache-2.0, Copyright
//! (c) 2026 ArtCraft Team and the TextureCraft contributors (see
//! THIRD_PARTY.md "Absorbed source files"). Adapted to Umber: anyhow
//! errors, `UMBER_CONTROL_TOKEN` / `umber-control-<port>.token` naming.
//!
//! 2026-11-03 (queue row 153, app control channel): the primitives moved
//! to `umber_core::control` so `umber-app --control` shares ONE
//! implementation (std-only there, so no getrandom edge in core and
//! Cargo.lock does not move). This module keeps the anyhow-shaped API
//! `serve`/`mcp` already call; the getrandom dep stays declared until the
//! next battery prunes it (removing it now would stale Cargo.lock inside
//! the no-cargo envelope).

use std::io::BufRead;
use std::path::PathBuf;

use anyhow::Result;

pub use umber_core::control::{
    ConnectionLimiter, LineRead, MAX_BATCH_STEPS, MAX_CONNECTIONS, MAX_REQUEST_BYTES, TOKEN_ENV,
};

/// The token file `serve --port` writes when the environment gives no token.
pub fn token_file(port: u16) -> PathBuf {
    umber_core::control::token_file("umber-control", port)
}

/// A fresh 256-bit token as 64 hex digits, from the OS random generator.
// Client-side helper exercised by the security tests; the app bridge reads
// the token via umber_core::control::client_token directly.
#[cfg_attr(not(test), allow(dead_code))]
pub fn generate_token() -> Result<String> {
    Ok(umber_core::control::generate_token()?)
}

/// Token for a client of `port`: explicit, else `UMBER_CONTROL_TOKEN`, else the token file.
#[cfg_attr(not(test), allow(dead_code))]
pub fn client_token(port: u16, supplied: Option<String>) -> Result<String> {
    Ok(umber_core::control::client_token(
        "umber-control",
        port,
        supplied,
    )?)
}

/// Token for a server on `port`: `UMBER_CONTROL_TOKEN`, else a fresh one written to
/// [`token_file`] (owner-only on Unix). Returns the token and where it was written.
pub fn server_token(port: u16) -> Result<(String, Option<PathBuf>)> {
    Ok(umber_core::control::server_token("umber-control", port)?)
}

/// Compare equal-length tokens without an early exit.
pub fn token_matches(expected: &str, supplied: &str) -> bool {
    umber_core::control::token_matches(expected, supplied)
}

/// Read one line, never buffering more than [`MAX_REQUEST_BYTES`] + 1 bytes.
pub fn read_bounded_line(r: &mut impl BufRead, line: &mut String) -> std::io::Result<LineRead> {
    umber_core::control::read_bounded_line(r, line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let a = generate_token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, generate_token().unwrap());
        assert!(token_matches(&a, &a.clone()));
        assert!(!token_matches(&a, "short"));
        assert!(client_token(1, Some("too short".into())).is_err());
        assert!(client_token(1, Some(format!(" {a}\n"))).is_ok());
    }

    #[test]
    fn oversized_lines_are_skipped() {
        let input = format!("{}\n{{\"ok\":1}}\n", "x".repeat(MAX_REQUEST_BYTES + 10));
        let mut r = std::io::Cursor::new(input);
        let mut line = String::new();
        assert!(matches!(
            read_bounded_line(&mut r, &mut line).unwrap(),
            LineRead::TooLong
        ));
        assert!(matches!(
            read_bounded_line(&mut r, &mut line).unwrap(),
            LineRead::Line
        ));
        assert_eq!(line, "{\"ok\":1}\n");
        assert!(matches!(
            read_bounded_line(&mut r, &mut line).unwrap(),
            LineRead::Eof
        ));
    }

    #[test]
    fn limiter_releases() {
        let l = ConnectionLimiter::default();
        let p = l.try_acquire(1).unwrap();
        assert!(l.try_acquire(1).is_none());
        drop(p);
        assert!(l.try_acquire(1).is_some());
    }
}
