//! The crash reporter (docs/loop/audit-2026-10-15-c2.md item C2-2;
//! ROADMAP `wave-crash-reporter`): a panic anywhere on any thread writes a
//! report file before the process dies, and the next launch can show it.
//!
//! - [`install`] chains a `std::panic::set_hook` in front of the default
//!   hook (which still prints to stderr). The hook writes one report per
//!   panic into `<state dir>/crashes/` ([`crate::gpu_startup::state_dir`],
//!   the same per-user root as the GPU marker and the plugins dir).
//! - A report holds the panic message, the location, the thread name, a
//!   forced backtrace, and the build facts (version, commit), the same
//!   facts Help ▸ About copies, so a bug report carries the build.
//! - [`pending`] lists reports a previous run left. The shell offers them
//!   once ("Umber closed unexpectedly last time…"), and
//!   [`acknowledge`] renames a shown report to `.seen` so it is offered
//!   only once, while staying on disk for the user to attach.
//!
//! The hook must never panic or block. Every step is fallible and
//! best-effort: a failed write is printed to stderr and dropped. The hook
//! also writes nothing user-supplied except the panic text itself (no
//! paths from the document, no environment), so a report is safe to
//! paste into a public issue.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The reports directory under `state_dir`.
pub fn crash_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("crashes")
}

/// The report body for one panic: plain text, one fact per line, then
/// the backtrace.
pub fn format_report(
    message: &str,
    location: Option<&str>,
    thread: &str,
    unix_secs: u64,
    backtrace: &str,
) -> String {
    let commit = option_env!("UMBER_GIT_HASH").unwrap_or("unknown");
    format!(
        "Umber crash report\n\
         version: {}\n\
         commit: {commit}\n\
         time (unix): {unix_secs}\n\
         thread: {thread}\n\
         location: {}\n\
         message: {message}\n\
         \n\
         backtrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        location.unwrap_or("unknown"),
    )
}

/// The panic payload as text (`&str` / `String` payloads; anything else
/// reads as a fixed placeholder).
pub fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-text panic payload)".to_owned()
    }
}

/// Writes `body` as `crash-<unix>-<pid>.txt` in `dir` (created if absent).
/// Returns the path written.
pub fn write_report(dir: &Path, unix_secs: u64, body: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("crash-{unix_secs}-{}.txt", std::process::id()));
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Installs the hook. `state_dir` is resolved now, not inside the hook,
/// so the hook does no environment lookups while the process is failing.
pub fn install(state_dir: PathBuf) {
    let dir = crash_dir(&state_dir);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_owned();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let body = format_report(
            &payload_text(info.payload()),
            location.as_deref(),
            &thread,
            unix,
            &backtrace,
        );
        match write_report(&dir, unix, &body) {
            Ok(path) => eprintln!("umber: crash report written to {}", path.display()),
            Err(e) => eprintln!("umber: crash report not written: {e}"),
        }
        previous(info);
    }));
}

/// Reports a previous run left and nobody has seen yet, oldest first.
pub fn pending(state_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(crash_dir(state_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|x| x == "txt")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("crash-"))
        })
        .collect();
    out.sort();
    out
}

/// Marks a shown report as seen (`.txt` → `.seen`), so it is offered only
/// once but stays on disk.
pub fn acknowledge(report: &Path) -> std::io::Result<()> {
    std::fs::rename(report, report.with_extension("seen"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("umber-crash-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn report_carries_the_facts() {
        let body = format_report("boom", Some("src/x.rs:1:2"), "main", 42, "frame 0");
        assert!(body.starts_with("Umber crash report\n"));
        assert!(body.contains(&format!("version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(body.contains("commit: "));
        assert!(body.contains("time (unix): 42"));
        assert!(body.contains("thread: main"));
        assert!(body.contains("location: src/x.rs:1:2"));
        assert!(body.contains("message: boom"));
        assert!(body.ends_with("backtrace:\nframe 0\n"));
        let no_loc = format_report("boom", None, "main", 0, "");
        assert!(no_loc.contains("location: unknown"));
    }

    #[test]
    fn payloads_read_as_text() {
        let s: Box<dyn std::any::Any + Send> = Box::new("static str");
        assert_eq!(payload_text(s.as_ref()), "static str");
        let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("owned"));
        assert_eq!(payload_text(owned.as_ref()), "owned");
        let other: Box<dyn std::any::Any + Send> = Box::new(7u32);
        assert_eq!(payload_text(other.as_ref()), "(non-text panic payload)");
    }

    #[test]
    fn write_pending_acknowledge_round_trip() {
        let state = temp_state("rt");
        assert!(pending(&state).is_empty(), "no dir = nothing pending");
        let dir = crash_dir(&state);
        let a = write_report(&dir, 100, "first").unwrap();
        let b = write_report(&dir, 200, "second").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a report").unwrap();
        assert_eq!(
            pending(&state),
            vec![a.clone(), b.clone()],
            "oldest first, reports only"
        );
        acknowledge(&a).unwrap();
        assert_eq!(pending(&state), vec![b]);
        assert!(a.with_extension("seen").exists(), "kept on disk");
        let _ = std::fs::remove_dir_all(&state);
    }

    /// The hook test, run as a CHILD PROCESS: the panic hook is
    /// process-global, and libtest runs sibling tests in parallel threads,
    /// so installing it in the test process would also catch their
    /// panics (`bakes_panel`'s deliberate worker panic) and could restore
    /// the default hook under them. The child runs this same test binary
    /// filtered to `crash_hook_child`, which installs, panics, and exits.
    #[test]
    fn a_real_panic_writes_a_report_through_the_hook() {
        let state = temp_state("hook");
        let exe = std::env::current_exe().expect("test binary path");
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "crash::tests::crash_hook_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("UMBER_CRASH_CHILD_STATE", &state)
            .output()
            .expect("spawn the child test");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "the child must die of its panic: {stderr}"
        );
        let reports = pending(&state);
        assert_eq!(reports.len(), 1, "{reports:?}\n{stderr}");
        let body = std::fs::read_to_string(&reports[0]).unwrap();
        assert!(body.contains("message: deliberate test panic"), "{body}");
        assert!(body.contains("thread: crash-test"), "{body}");
        assert!(body.contains("crash.rs"), "the location names this file");
        assert!(stderr.contains("crash report written to"), "{stderr}");
        let _ = std::fs::remove_dir_all(&state);
    }

    /// The child half: inert unless the parent set the env var, so a plain
    /// `cargo test` run passes it trivially.
    #[test]
    fn crash_hook_child() {
        let Some(state) = std::env::var_os("UMBER_CRASH_CHILD_STATE") else {
            return;
        };
        install(PathBuf::from(state));
        std::thread::Builder::new()
            .name("crash-test".into())
            .spawn(|| panic!("deliberate test panic"))
            .unwrap()
            .join()
            .ok();
        // The thread's panic already wrote the report; fail this child so
        // the parent can tell it ran to here.
        std::process::exit(3);
    }
}
