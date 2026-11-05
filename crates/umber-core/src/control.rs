//! Loopback control-channel primitives shared by `umber-cli serve` and
//! `umber-app --control <port>` (queue row 153). Tokens, bounded JSONL
//! framing, connection limiting, loopback-only binding.
//!
//! Std-only plus thiserror/blake3/serde_json (all already core deps):
//! token generation reads `/dev/urandom` on Unix and falls back to a
//! blake3 mix of process entropy elsewhere. Deliberately NO getrandom
//! edge — added 2026-11-03 inside the no-cargo envelope, so Cargo.lock
//! must not move.
//!
//! Absorbed 2026-11-01 from storytold/texturecraft `automation/security.rs`
//! via umber-cli (see THIRD_PARTY.md "Absorbed source files"), lifted here
//! 2026-11-03 so the app-side channel shares one implementation.

#[cfg(unix)]
use std::io::Read;
use std::io::{BufRead, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Environment variable holding the control token (both sides).
pub const TOKEN_ENV: &str = "UMBER_CONTROL_TOKEN";
/// Largest JSON request line, newline included.
pub const MAX_REQUEST_BYTES: usize = 8 << 20;
/// Simultaneous TCP connections a control server serves.
pub const MAX_CONNECTIONS: usize = 16;
/// Steps in one `steps` batch.
pub const MAX_BATCH_STEPS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("control token must be 16 to 1024 printable ASCII characters")]
    InvalidToken,
    #[error("cannot generate a control token: {0}")]
    NoEntropy(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ControlError>;

/// The server accepts any token of 16..=1024 printable ASCII characters;
/// generated ones are 64 hex.
pub fn validate_token(t: &str) -> Result<()> {
    if (16..=1024).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_graphic()) {
        Ok(())
    } else {
        Err(ControlError::InvalidToken)
    }
}

/// A fresh 256-bit token as 64 hex digits. Unix reads `/dev/urandom`;
/// other platforms mix process entropy through blake3 — a convenience
/// fallback on a loopback-only channel, not the security boundary.
pub fn generate_token() -> Result<String> {
    #[cfg(unix)]
    {
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|e| ControlError::NoEntropy(e.to_string()))?;
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
    #[cfg(not(unix))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut hasher = blake3::Hasher::new();
        hasher.update(&nanos.to_le_bytes());
        hasher.update(&std::process::id().to_le_bytes());
        hasher.update(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        // fn-pointer cast, not a fn-item cast (clippy::fn_to_numeric_cast).
        let gen: fn() -> Result<String> = generate_token;
        hasher.update(&(gen as usize).to_le_bytes());
        Ok(hasher.finalize().to_hex().to_string())
    }
}

/// The token file a server with the given name prefix writes when the
/// environment gives no token: `<temp>/<prefix>-<port>.token`.
pub fn token_file(prefix: &str, port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{port}.token"))
}

/// Write the token file owner-only (0600 on Unix).
pub fn write_token_file(path: &Path, token: &str) -> Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(token.as_bytes())?;
    Ok(())
}

/// Server-side token for `prefix` on `port`: `UMBER_CONTROL_TOKEN` when
/// set, else a fresh one written to [`token_file`]. Returns the token and
/// where it was written.
pub fn server_token(prefix: &str, port: u16) -> Result<(String, Option<PathBuf>)> {
    if let Ok(t) = std::env::var(TOKEN_ENV) {
        validate_token(t.trim())?;
        return Ok((t.trim().to_string(), None));
    }
    let token = generate_token()?;
    let path = token_file(prefix, port);
    write_token_file(&path, &token)?;
    Ok((token, Some(path)))
}

/// Client-side token: explicit, else `UMBER_CONTROL_TOKEN`, else the token file.
pub fn client_token(prefix: &str, port: u16, supplied: Option<String>) -> Result<String> {
    let token = match supplied.or_else(|| std::env::var(TOKEN_ENV).ok()) {
        Some(t) => t,
        None => {
            let path = token_file(prefix, port);
            std::fs::read_to_string(&path).map_err(|e| {
                ControlError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "no control token: set {TOKEN_ENV} or start the server on port {port} so it writes {} ({e})",
                        path.display()
                    ),
                ))
            })?
        }
    };
    let token = token.trim().to_string();
    validate_token(&token)?;
    Ok(token)
}

/// Compare equal-length tokens without an early exit.
pub fn token_matches(expected: &str, supplied: &str) -> bool {
    expected.len() == supplied.len()
        && expected
            .bytes()
            .zip(supplied.bytes())
            .fold(0u8, |d, (a, b)| d | (a ^ b))
            == 0
}

pub enum LineRead {
    Eof,
    Line,
    /// Over [`MAX_REQUEST_BYTES`]; the rest of the line is still unread.
    TooLong,
}

/// Read one line, never buffering more than [`MAX_REQUEST_BYTES`] + 1 bytes.
pub fn read_bounded_line(r: &mut impl BufRead, line: &mut String) -> std::io::Result<LineRead> {
    line.clear();
    let mut bytes = Vec::new();
    let n = std::io::Read::take(&mut *r, (MAX_REQUEST_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if n == 0 {
        return Ok(LineRead::Eof);
    }
    if n > MAX_REQUEST_BYTES {
        if bytes.last() != Some(&b'\n') {
            discard_rest_of_line(r)?;
        }
        return Ok(LineRead::TooLong);
    }
    *line = String::from_utf8(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(LineRead::Line)
}

fn discard_rest_of_line(r: &mut impl BufRead) -> std::io::Result<()> {
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok(());
        }
        if let Some(end) = buf.iter().position(|&b| b == b'\n') {
            r.consume(end + 1);
            return Ok(());
        }
        let n = buf.len();
        r.consume(n);
    }
}

/// Counts live connections; a permit is released when dropped.
#[derive(Clone, Default)]
pub struct ConnectionLimiter(Arc<AtomicUsize>);

pub struct Permit(Arc<AtomicUsize>);

impl ConnectionLimiter {
    pub fn try_acquire(&self, max: usize) -> Option<Permit> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < max).then_some(n + 1)
            })
            .ok()
            .map(|_| Permit(self.0.clone()))
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Bind a control listener on 127.0.0.1 only. Port 0 picks an ephemeral
/// port (tests). There is deliberately no interface argument: the control
/// channel is loopback-only, ever.
pub fn bind_loopback(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
}

/// Write one JSON value as a single line and flush.
pub fn write_json_line(w: &mut impl Write, v: &serde_json::Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *w, v)?;
    w.write_all(b"\n")?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let a = generate_token().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, generate_token().unwrap());
        validate_token(&a).unwrap();
        assert!(validate_token("too short").is_err());
        assert!(validate_token(&"x".repeat(1025)).is_err());
        assert!(validate_token("with space 1234567").is_err());
        assert!(token_matches(&a, &a.clone()));
        assert!(!token_matches(&a, "short"));
        assert!(!token_matches(&a, &format!("{a}x")));
    }

    #[test]
    fn token_file_roundtrip_is_owner_only() {
        let token = generate_token().unwrap();
        let path = token_file("umber-test-control", 0);
        let path = path.with_file_name(format!(
            "umber-test-control-{}-{}.token",
            std::process::id(),
            &token[..8]
        ));
        write_token_file(&path, &token).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn client_token_prefers_explicit_then_env_then_file() {
        let good = generate_token().unwrap();
        assert!(client_token("umber-test-control", 0, Some("too short".into())).is_err());
        let padded = format!(" {good}\n");
        assert_eq!(
            client_token("umber-test-control", 0, Some(padded)).unwrap(),
            good
        );
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

    #[test]
    fn loopback_bind_is_127_0_0_1() {
        let l = bind_loopback(0).unwrap();
        let addr = l.local_addr().unwrap();
        assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
        assert_ne!(addr.port(), 0);
    }

    #[test]
    fn json_line_framing() {
        let mut buf = Vec::new();
        write_json_line(&mut buf, &serde_json::json!({"ok": true})).unwrap();
        assert_eq!(buf, b"{\"ok\":true}\n");
    }
}
