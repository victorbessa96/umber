//! File confinement for the untrusted automation surfaces (`mcp`, `serve`).
//!
//! Absorbed 2026-11-01 from storytold/texturecraft
//! `crates/automation/src/workspace.rs` — MIT OR Apache-2.0, Copyright
//! (c) 2026 ArtCraft Team and the TextureCraft contributors (see
//! THIRD_PARTY.md "Absorbed source files"). Upstream confines with
//! cap-std `Dir` handles; umber's loaders are path-based
//! (`umber_mesh::load(path)`, `umber_core::load_from_dir`), so this
//! port confines with std only: every EXISTING path component is fully
//! canonicalized (symlinks resolved) before a prefix test against the
//! canonicalized root, so `..`, absolute paths, drive prefixes, device
//! names and symlink/junction escapes all fail. Writes canonicalize the
//! parent and go through a create-new temp file + fsync + rename. The
//! residual TOCTOU window between check and use is accepted: the
//! surfaces are loopback-only and token-authenticated local tools, not
//! a sandbox against a hostile local user (that is what the cap-std
//! upgrade row is for).
//!
//! Roots are trusted launch-time configuration (`--read-root`,
//! `--write-root`); request paths are untrusted forward-slash relative
//! paths. A missing authority fails closed: no root, no file access.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde_json::Value;

/// Largest file read through automation (the mesh importer's own limit still applies downstream).
pub const MAX_READ_BYTES: u64 = 512 << 20;

/// Separate read and write root capabilities. A missing authority fails closed.
#[derive(Clone, Debug, Default)]
pub struct AuthorizedWorkspace {
    /// Canonicalized roots (symlinks resolved once, at open).
    read: Option<PathBuf>,
    write: Option<PathBuf>,
}

/// Which side of the workspace a path-carrying step touches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Read,
    Write,
}

impl AuthorizedWorkspace {
    pub fn new(read_root: Option<&Path>, write_root: Option<&Path>) -> Result<Self> {
        Ok(Self {
            read: open_root(read_root, "read")?,
            write: open_root(write_root, "write")?,
        })
    }

    /// Read one regular file below the read root.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let target = self.confine(path, Access::Read)?;
        let meta = std::fs::metadata(&target).map_err(|e| file_error("read", path, e))?;
        if !meta.is_file() {
            return Err(anyhow!("not a regular file: `{path}`"));
        }
        if meta.len() > MAX_READ_BYTES {
            return Err(anyhow!(
                "`{path}` is {} bytes (limit {MAX_READ_BYTES})",
                meta.len()
            ));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&target)
            .map_err(|e| file_error("read", path, e))?
            .take(MAX_READ_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| file_error("read", path, e))?;
        Ok(bytes)
    }

    /// Create or replace one file below the write root: a temporary file beside the target,
    /// synced and renamed over it. The parent directory must exist.
    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let target = self.confine(path, Access::Write)?;
        let leaf = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = target.with_file_name(format!(".{leaf}.{}.tmp", std::process::id()));
        let res = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, bytes)?;
                f.sync_all()
            })
            .and_then(|()| std::fs::rename(&tmp, &target));
        if let Err(e) = res {
            let _ = std::fs::remove_file(&tmp);
            return Err(file_error("write", path, e));
        }
        Ok(())
    }

    /// Create a directory (and its parents) below the write root.
    pub fn create_dir_all(&self, path: &str) -> Result<()> {
        let target = self.confine(path, Access::Write)?;
        std::fs::create_dir_all(&target).map_err(|e| file_error("create", path, e))
    }

    /// The entry names of one directory below the read root.
    pub fn list_dir(&self, path: &str) -> Result<Vec<String>> {
        let target = self.confine(path, Access::Read)?;
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&target).map_err(|e| file_error("list", path, e))? {
            let entry = entry.map_err(|e| file_error("list", path, e))?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    /// The confined HOST path of a read — for umber's path-based loaders
    /// (`umber_mesh::load`, `project_open`). The path must exist.
    pub fn host_read_path(&self, raw: &str) -> Result<PathBuf> {
        self.confine(raw, Access::Read)
    }

    /// The confined HOST path of a write — for `export`'s `out_dir`,
    /// `project_save`'s `dir`, `paint_snapshot`'s `out`/`out_dir`.
    pub fn host_write_path(&self, raw: &str) -> Result<PathBuf> {
        self.confine(raw, Access::Write)
    }

    /// Rewrite the path-carrying fields of one agent step to confined host
    /// paths, so a `run_step`/`step` client cannot reach outside the roots.
    /// `mode` governs the ambiguous keys (`dir`, `path`); the unambiguous
    /// ones are fixed: `mesh`/`in` read, `out`/`out_dir` write.
    pub fn confine_step(&self, step: &mut Value, mode: Access) -> Result<()> {
        let Some(obj) = step.as_object_mut() else {
            return Ok(());
        };
        let Some((_name, body)) = obj.iter_mut().next() else {
            return Ok(());
        };
        let Some(body) = body.as_object_mut() else {
            return Ok(());
        };
        let confine = |v: &mut Value, access: Access, ws: &Self| -> Result<()> {
            if let Some(s) = v.as_str() {
                let host = ws.confine(s, access)?;
                *v = Value::String(host.display().to_string());
            }
            Ok(())
        };
        for key in ["mesh", "in", "input"] {
            if let Some(v) = body.get_mut(key) {
                confine(v, Access::Read, self)?;
            }
        }
        for key in ["out", "out_dir", "output"] {
            if let Some(v) = body.get_mut(key) {
                confine(v, Access::Write, self)?;
            }
        }
        for key in ["dir", "path"] {
            if let Some(v) = body.get_mut(key) {
                confine(v, mode, self)?;
            }
        }
        Ok(())
    }

    /// Resolve one untrusted path below the given root: validate the shape
    /// ([`relative_path`]), then canonicalize and prefix-test.
    fn confine(&self, raw: &str, access: Access) -> Result<PathBuf> {
        let rel = relative_path(raw)?;
        let (root, what) = match access {
            Access::Read => (self.read.as_ref(), "read"),
            Access::Write => (self.write.as_ref(), "write"),
        };
        let Some(root) = root else {
            return Err(anyhow!("file access is not granted: no {what} root (start the server with --{what}-root <dir>)"));
        };
        match access {
            Access::Read => {
                // Must exist: canonicalize resolves every component, symlinks included.
                let c = root
                    .join(&rel)
                    .canonicalize()
                    .map_err(|e| file_error(what, raw, e))?;
                if !c.starts_with(root) {
                    return Err(escape(raw));
                }
                Ok(c)
            }
            Access::Write => {
                // The leaf is created new; the parent chain must exist and stay inside.
                let parent = rel.parent().unwrap_or_else(|| Path::new(""));
                let canon_parent = if parent.as_os_str().is_empty() {
                    root.clone()
                } else {
                    let cp = root
                        .join(parent)
                        .canonicalize()
                        .map_err(|e| file_error(what, raw, e))?;
                    if !cp.starts_with(root) {
                        return Err(escape(raw));
                    }
                    cp
                };
                let leaf = rel
                    .file_name()
                    .ok_or_else(|| anyhow!("path rejected (no leaf): `{raw}`"))?;
                let target = canon_parent.join(leaf);
                // An existing target must itself resolve inside the root (not a symlink out).
                if target.symlink_metadata().is_ok() {
                    let c = target
                        .canonicalize()
                        .map_err(|e| file_error(what, raw, e))?;
                    if !c.starts_with(root) {
                        return Err(escape(raw));
                    }
                }
                Ok(target)
            }
        }
    }
}

fn open_root(path: Option<&Path>, authority: &str) -> Result<Option<PathBuf>> {
    let Some(path) = path else { return Ok(None) };
    if path.as_os_str().is_empty() {
        return Err(anyhow!("the automation {authority} root is empty"));
    }
    let c = path
        .canonicalize()
        .map_err(|e| anyhow!("cannot open {authority} root `{}`: {e}", path.display()))?;
    if !c.is_dir() {
        return Err(anyhow!(
            "the automation {authority} root `{}` is not a directory",
            path.display()
        ));
    }
    Ok(Some(c))
}

/// Validate an untrusted path before any I/O (shape check; confinement happens after).
pub fn relative_path(raw: &str) -> Result<PathBuf> {
    let reject = |why: &str| {
        anyhow!("path rejected ({why}): `{raw}`; use a forward-slash path relative to the workspace root")
    };
    if raw.is_empty() {
        return Err(reject("empty"));
    }
    if raw.contains('\\') {
        return Err(reject("backslash"));
    }
    if raw.starts_with('/') {
        return Err(reject("absolute"));
    }
    if raw.contains(':') {
        return Err(reject("drive, scheme or stream prefix"));
    }
    for c in raw.split('/') {
        if c.is_empty() || c == "." || c == ".." {
            return Err(reject("empty, `.` or `..` component"));
        }
        if c.ends_with(['.', ' ']) || c.chars().any(char::is_control) {
            return Err(reject(
                "component ends in a dot or space, or has control characters",
            ));
        }
        let stem = c.split('.').next().unwrap_or(c).to_ascii_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|ch: char| ch.is_ascii_digit()));
        if device {
            return Err(reject("reserved device name"));
        }
    }
    Ok(PathBuf::from(raw))
}

fn escape(raw: &str) -> anyhow::Error {
    anyhow!("path rejected (resolves outside the workspace root): `{raw}`")
}

fn file_error(op: &str, path: &str, e: std::io::Error) -> anyhow::Error {
    anyhow!("{op} `{path}` failed: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(name: &str) -> (PathBuf, PathBuf, AuthorizedWorkspace) {
        let base = std::env::temp_dir().join(format!("umber-ws-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (inside, outside) = (base.join("inside"), base.join("outside"));
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let ws = AuthorizedWorkspace::new(Some(&inside), Some(&inside)).unwrap();
        (inside, outside, ws)
    }

    #[test]
    fn reads_and_writes_stay_in_the_root() {
        let (inside, _, ws) = roots("valid");
        std::fs::write(inside.join("a.txt"), b"canary").unwrap();
        assert_eq!(ws.read("a.txt").unwrap(), b"canary");
        ws.create_dir_all("out/deep").unwrap();
        ws.write("out/deep/b.txt", b"one").unwrap();
        ws.write("out/deep/b.txt", b"two").unwrap();
        assert_eq!(
            std::fs::read(inside.join("out/deep/b.txt")).unwrap(),
            b"two"
        );
        let leftovers = std::fs::read_dir(inside.join("out/deep")).unwrap().count();
        assert_eq!(leftovers, 1, "no temporary files left behind");
        assert!(
            ws.write("missing/c.txt", b"x").is_err(),
            "parent must exist"
        );
        assert_eq!(ws.list_dir("out").unwrap(), vec!["deep".to_string()]);
    }

    #[test]
    fn escapes_and_malformed_paths_are_rejected() {
        let (_, outside, ws) = roots("reject");
        let abs = outside.join("x.txt").display().to_string();
        for p in [
            "",
            "/abs",
            "../x",
            "a/../../x",
            "a\\b",
            "C:/x",
            "a//b",
            "./a",
            "a:b",
            "NUL",
            "con.txt",
            "x/COM1.log",
            "dot.",
            abs.as_str(),
        ] {
            assert!(ws.read(p).is_err(), "read accepted {p:?}");
            assert!(ws.write(p, b"x").is_err(), "write accepted {p:?}");
            assert!(ws.create_dir_all(p).is_err(), "mkdir accepted {p:?}");
        }
    }

    #[test]
    fn authorities_are_separate_and_fail_closed() {
        let (inside, _, _) = roots("separate");
        std::fs::write(inside.join("r.txt"), b"r").unwrap();
        let ro = AuthorizedWorkspace::new(Some(&inside), None).unwrap();
        assert!(ro.read("r.txt").is_ok());
        assert!(ro.write("w.txt", b"x").is_err());
        let none = AuthorizedWorkspace::default();
        assert!(none.read("r.txt").is_err());
        assert!(none.host_read_path("r.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        let (inside, outside, ws) = roots("symlink");
        std::fs::write(outside.join("secret.txt"), b"s").unwrap();
        std::os::unix::fs::symlink(&outside, inside.join("link")).unwrap();
        assert!(ws.read("link/secret.txt").is_err());
        assert!(ws.write("link/new.txt", b"x").is_err());
        assert!(!outside.join("new.txt").exists());
    }

    #[test]
    fn confine_step_rewrites_path_fields_to_confined_host_paths() {
        let (inside, outside, ws) = roots("step");
        std::fs::write(inside.join("m.glb"), b"x").unwrap();
        let mut step = serde_json::json!({"load": {"mesh": "m.glb"}});
        ws.confine_step(&mut step, Access::Read).unwrap();
        let mesh = step["load"]["mesh"].as_str().unwrap();
        assert!(
            mesh.starts_with(&*inside.canonicalize().unwrap().to_string_lossy()),
            "{mesh}"
        );
        let mut bad = serde_json::json!({"load": {"mesh": "../outside/m.glb"}});
        assert!(ws.confine_step(&mut bad, Access::Read).is_err());
        let mut out = serde_json::json!({"export": {"out_dir": "../outside"}});
        assert!(ws.confine_step(&mut out, Access::Write).is_err());
        let _ = outside;
    }
}
