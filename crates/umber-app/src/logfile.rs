//! The session log file (G5 Help ▸ Show log): every log line goes to
//! stderr as before AND to `<state>/logs/umber.log`, truncated at launch
//! (one session per file; the previous session's is kept as
//! `umber.prev.log`, so a crash's log survives the relaunch that reports
//! it). No new dependency: `env_logger`'s `Target::Pipe` takes the tee.
//!
//! A log file that cannot be opened is not an error: logging falls back to
//! stderr alone, and Show log says there is no file.

use std::io::Write;
use std::path::{Path, PathBuf};

/// `<state>/logs`.
pub fn log_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("logs")
}

/// This session's log file.
pub fn log_path(state_dir: &Path) -> PathBuf {
    log_dir(state_dir).join("umber.log")
}

/// The previous session's log file.
pub fn prev_log_path(state_dir: &Path) -> PathBuf {
    log_dir(state_dir).join("umber.prev.log")
}

/// Rotates the last session's log to `umber.prev.log` and opens a fresh
/// `umber.log`. `None` when the directory or file cannot be created.
pub fn open_session_log(state_dir: &Path) -> Option<std::fs::File> {
    std::fs::create_dir_all(log_dir(state_dir)).ok()?;
    let path = log_path(state_dir);
    if path.exists() {
        let _ = std::fs::rename(&path, prev_log_path(state_dir));
    }
    std::fs::File::create(path).ok()
}

/// Writes every byte to both sinks. A failing file write never fails the
/// log call: stderr is the sink of record, the file a convenience.
pub struct Tee<A: Write, B: Write> {
    pub primary: A,
    pub secondary: Option<B>,
}

impl<A: Write, B: Write> Write for Tee<A, B> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.primary.write(buf)?;
        if let Some(file) = self.secondary.as_mut() {
            if file.write_all(&buf[..n]).is_err() {
                // A full disk or a removed file: stop teeing, keep stderr.
                self.secondary = None;
            }
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = self.secondary.as_mut() {
            let _ = file.flush();
        }
        self.primary.flush()
    }
}

/// Opens `path` with the platform's default handler (a text editor for
/// `.log`). Spawned detached; the error is the caller's status line.
///
/// No shell anywhere: each opener is a normal executable taking the path
/// as ONE argument. On Windows that rules out `cmd /C start`, because
/// cmd.exe re-parses its command line, so a path holding `&`, `^` or `%`
/// (a user name, a redirected profile) would split into a second command.
/// `explorer.exe <file>` opens it with its associated program instead.
pub fn open_in_system(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tee_writes_both_and_survives_a_failing_file() {
        let mut tee = Tee {
            primary: Vec::new(),
            secondary: Some(Vec::new()),
        };
        tee.write_all(b"hello\n").unwrap();
        assert_eq!(tee.primary, b"hello\n");
        assert_eq!(tee.secondary.as_deref(), Some(&b"hello\n"[..]));

        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk full"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut tee = Tee {
            primary: Vec::new(),
            secondary: Some(Broken),
        };
        tee.write_all(b"still logged").unwrap();
        assert_eq!(tee.primary, b"still logged");
        assert!(tee.secondary.is_none(), "a failing file stops teeing");
    }

    #[test]
    fn a_new_session_rotates_the_last_log() {
        let state = std::env::temp_dir().join(format!("umber-logfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        let mut first = open_session_log(&state).expect("opens");
        first.write_all(b"session one").unwrap();
        drop(first);
        let _second = open_session_log(&state).expect("opens again");
        assert_eq!(
            std::fs::read(prev_log_path(&state)).unwrap(),
            b"session one"
        );
        assert_eq!(std::fs::read(log_path(&state)).unwrap(), b"");
        let _ = std::fs::remove_dir_all(&state);
    }
}
