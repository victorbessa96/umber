//! File confinement for the untrusted automation surfaces (`mcp`, `serve`).
//!
//! Absorbed 2026-11-01 from storytold/texturecraft
//! `crates/automation/src/workspace.rs` — MIT OR Apache-2.0, Copyright
//! (c) 2026 ArtCraft Team and the TextureCraft contributors (see
//! THIRD_PARTY.md "Absorbed source files"). Upstream confines with
//! cap-std `Dir` handles; the 2026-11-01 port used std canonicalize +
//! prefix checks with an accepted TOCTOU window. Queue row 155 restores
//! the cap-std confinement: every filesystem touch below a root goes
//! through a cap-std `Dir` handle, so resolution happens with
//! openat-style semantics and a symlink that escapes the root is refused
//! by the capability itself, not by a prefix test raced against the
//! filesystem.
//!
//! The public API is unchanged: [`AuthorizedWorkspace::host_read_path`] /
//! [`AuthorizedWorkspace::host_write_path`] still mint HOST paths, because
//! umber's loaders are path-based (`umber_mesh::load(path)`,
//! `umber_core::load_from_dir`). The host path is minted only after the
//! cap-std handle has resolved the same relative path inside the sandbox;
//! the residual window is the loader's own path-based open (the surfaces
//! are loopback-only and token-authenticated local tools, not a sandbox
//! against a hostile local user).
//!
//! Roots are trusted launch-time configuration (`--read-root`,
//! `--write-root`); request paths are untrusted forward-slash relative
//! paths. A missing authority fails closed: no root, no file access.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use cap_std::fs::{Dir, OpenOptions};
use serde_json::Value;

/// Largest file read through automation (the mesh importer's own limit still applies downstream).
#[cfg_attr(not(test), allow(dead_code))]
pub const MAX_READ_BYTES: u64 = 512 << 20;

/// Separate read and write root capabilities. A missing authority fails closed.
#[derive(Clone, Debug, Default)]
pub struct AuthorizedWorkspace {
    /// The cap-std directory handles: all confined I/O goes through these.
    read: Option<Arc<Dir>>,
    write: Option<Arc<Dir>>,
    /// Canonicalized host roots (symlinks resolved once, at open), used
    /// only to mint host paths for the path-based loaders after the `Dir`
    /// has resolved the same relative path inside the sandbox.
    read_root: Option<PathBuf>,
    write_root: Option<PathBuf>,
}

/// Which side of the workspace a path-carrying step touches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Read,
    Write,
}

impl AuthorizedWorkspace {
    pub fn new(read_root: Option<&Path>, write_root: Option<&Path>) -> Result<Self> {
        let read = open_root(read_root, "read")?;
        let write = open_root(write_root, "write")?;
        Ok(Self {
            read: read.as_ref().map(|(dir, _)| dir.clone()),
            write: write.as_ref().map(|(dir, _)| dir.clone()),
            read_root: read.map(|(_, root)| root),
            write_root: write.map(|(_, root)| root),
        })
    }

    /// The cap-std handle + canonical host root for one side, or the
    /// fail-closed "no root" error.
    fn handle(&self, access: Access) -> Result<(&Arc<Dir>, &Path)> {
        let (dir, root, what) = match access {
            Access::Read => (self.read.as_ref(), self.read_root.as_ref(), "read"),
            Access::Write => (self.write.as_ref(), self.write_root.as_ref(), "write"),
        };
        match (dir, root) {
            (Some(d), Some(r)) => Ok((d, r)),
            _ => Err(anyhow!(
                "file access is not granted: no {what} root (start the server with --{what}-root <dir>)"
            )),
        }
    }

    /// Read one regular file below the read root.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let rel = relative_path(path)?;
        let (dir, _) = self.handle(Access::Read)?;
        let meta = dir
            .metadata(&rel)
            .map_err(|e| file_error("read", path, e))?;
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
        dir.open(&rel)
            .map_err(|e| file_error("read", path, e))?
            .take(MAX_READ_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| file_error("read", path, e))?;
        Ok(bytes)
    }

    /// Create or replace one file below the write root: a temporary file beside the target,
    /// synced and renamed over it, all through the cap-std handle. The parent directory must exist.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let rel = relative_path(path)?;
        let (dir, _) = self.handle(Access::Write)?;
        self.check_write(&rel, path, dir)?;
        let leaf = rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = rel.with_file_name(format!(".{leaf}.{}.tmp", std::process::id()));
        let res = dir
            .open_with(&tmp, OpenOptions::new().write(true).create_new(true))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, bytes)?;
                f.sync_all()
            })
            .and_then(|()| dir.rename(&tmp, dir, &rel));
        if let Err(e) = res {
            let _ = dir.remove_file(&tmp);
            return Err(file_error("write", path, e));
        }
        Ok(())
    }

    /// Create a directory (and its parents) below the write root.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn create_dir_all(&self, path: &str) -> Result<()> {
        let rel = relative_path(path)?;
        let (dir, _) = self.handle(Access::Write)?;
        // cap-std's create_dir_all resolves every existing component
        // inside the sandbox, so a symlink escape anywhere in the chain
        // fails here, at the capability, not at a later prefix test.
        dir.create_dir_all(&rel)
            .map_err(|e| file_error("create", path, e))
    }

    /// The entry names of one directory below the read root.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn list_dir(&self, path: &str) -> Result<Vec<String>> {
        let rel = relative_path(path)?;
        let (dir, _) = self.handle(Access::Read)?;
        let mut names = Vec::new();
        for entry in dir
            .read_dir(&rel)
            .map_err(|e| file_error("list", path, e))?
        {
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

    /// Verify a write target stays inside the sandbox through the `Dir`
    /// handle: the parent chain must resolve inside (cap-std errors on an
    /// escaping symlink), and an existing leaf must resolve inside too.
    fn check_write(&self, rel: &Path, raw: &str, dir: &Dir) -> Result<()> {
        let parent = rel.parent().unwrap_or_else(|| Path::new(""));
        if !parent.as_os_str().is_empty() {
            let cp = dir
                .canonicalize(parent)
                .map_err(|e| file_error("write", raw, e))?;
            if escapes(&cp) {
                return Err(escape(raw));
            }
        }
        if dir.symlink_metadata(rel).is_ok() {
            let c = dir
                .canonicalize(rel)
                .map_err(|e| file_error("write", raw, e))?;
            if escapes(&c) {
                return Err(escape(raw));
            }
        }
        Ok(())
    }

    /// Resolve one untrusted path below the given root and mint its
    /// confined HOST path: validate the shape ([`relative_path`]), then
    /// resolve through the cap-std `Dir` (openat-style; symlink escapes
    /// are refused by the capability) and join onto the canonical root.
    fn confine(&self, raw: &str, access: Access) -> Result<PathBuf> {
        let rel = relative_path(raw)?;
        let (dir, root) = self.handle(access)?;
        let what = match access {
            Access::Read => "read",
            Access::Write => "write",
        };
        match access {
            Access::Read => {
                // Must exist: the sandbox canonicalize resolves every
                // component, symlinks included, and errors on escape.
                let c = dir
                    .canonicalize(&rel)
                    .map_err(|e| file_error(what, raw, e))?;
                if escapes(&c) {
                    return Err(escape(raw));
                }
                // cap-std canonicalize returns a sandbox-RELATIVE path;
                // the host path is the canonical root joined with it.
                Ok(root.join(c))
            }
            Access::Write => {
                // The leaf is created new; the parent chain must exist and stay inside.
                self.check_write(&rel, raw, dir)?;
                if rel.file_name().is_none() {
                    return Err(anyhow!("path rejected (no leaf): `{raw}`"));
                }
                Ok(root.join(&rel))
            }
        }
    }
}

/// A sandbox-relative canonical path that nonetheless escaped (absolute
/// or carrying `..`): cap-std already refused it; this is the belt-and-
/// braces guard that keeps the typed escape error.
fn escapes(canonical: &Path) -> bool {
    canonical.is_absolute()
        || canonical.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn open_root(path: Option<&Path>, authority: &str) -> Result<Option<(Arc<Dir>, PathBuf)>> {
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
    // The root itself is trusted launch-time configuration, so ambient
    // open is correct here; everything BELOW the root goes through the
    // returned confined handle.
    let dir = Dir::open_ambient_dir(&c, cap_std::ambient_authority())
        .map_err(|e| anyhow!("cannot open {authority} root `{}`: {e}", path.display()))?;
    Ok(Some((Arc::new(dir), c)))
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
        // Row 155 pin: a symlink in the scratch workspace pointing outside
        // is refused on EVERY surface — the cap-std handle refuses the
        // escape at resolution time.
        assert!(ws.read("link/secret.txt").is_err());
        assert!(ws.write("link/new.txt", b"x").is_err());
        assert!(ws.host_read_path("link/secret.txt").is_err());
        assert!(ws.host_write_path("link/new.txt").is_err());
        assert!(ws.list_dir("link").is_err());
        assert!(ws.create_dir_all("link/deeper").is_err());
        assert!(!outside.join("new.txt").exists());
        assert!(!outside.join("deeper").exists());
        // A symlink that stays INSIDE the root still works (unchanged).
        std::fs::write(inside.join("real.txt"), b"r").unwrap();
        std::os::unix::fs::symlink("real.txt", inside.join("alias.txt")).unwrap();
        assert_eq!(ws.read("alias.txt").unwrap(), b"r");
        let host = ws.host_read_path("alias.txt").unwrap();
        assert!(
            host.starts_with(&*inside.canonicalize().unwrap()),
            "{host:?}"
        );
        assert!(host.ends_with("real.txt"), "resolved inside: {host:?}");
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
