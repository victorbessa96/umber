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
//!
//! Confinement is VARIANT-AWARE: [`AuthorizedWorkspace::confine_step`]
//! dispatches on the step name and confines exactly the fields that
//! variant carries ([`step_path_fields`]), including the reads
//! (`bake_transfer`'s `low`/`high`, `inspect`'s `mesh`, `import_asset`'s
//! `file`) and writes (`bake_transfer`/`bake`/`export`/`paint_snapshot`'s
//! `out_dir`, `import_asset`'s `library`) the earlier shared-key confiner
//! missed. A new path-bearing step variant MUST be added to that table;
//! the negative test enumerates every entry.
//!
//! `import_asset` without an explicit `library` would write to the
//! per-user asset library, which is outside every granted root. It is
//! therefore refused unless that directory resolves inside the write
//! root ([`AuthorizedWorkspace::authorize_host_write`]); there is no
//! silent unconfined write mode. A caller that wants the per-user library
//! grants it as (or below) `--write-root`.

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
    ///
    /// The step name selects the field table ([`step_path_fields`]), so a
    /// variant's reads and writes are confined by identity, never by a
    /// guessed shared key set. An `import_asset` with no `library` resolves
    /// the per-user library and requires it to be inside the write root
    /// (see [`Self::authorize_host_write`]).
    pub fn confine_step(&self, step: &mut Value) -> Result<()> {
        let Some(obj) = step.as_object_mut() else {
            return Ok(());
        };
        let Some((name, body)) = obj.iter_mut().next() else {
            return Ok(());
        };
        let name = name.clone();
        let Some(body) = body.as_object_mut() else {
            return Ok(());
        };
        let Some(fields) = step_path_fields(&name) else {
            return Ok(());
        };
        for (key, access) in fields {
            if let Some(v) = body.get_mut(*key) {
                if let Some(s) = v.as_str() {
                    let host = self.confine(s, *access)?;
                    *v = Value::String(host.display().to_string());
                }
            }
        }
        if name == "import_asset" {
            self.confine_import_library(body)?;
        }
        Ok(())
    }

    /// `import_asset`'s implicit write target: an absent `library` means
    /// the kind's per-user library dir. That directory is not request
    /// input, so it is authorized as an absolute HOST write target and
    /// must resolve inside the write root; the resolved host path is
    /// written back into the step so the import uses exactly it.
    fn confine_import_library(&self, body: &mut serde_json::Map<String, Value>) -> Result<()> {
        if body.get("library").is_some_and(|v| !v.is_null()) {
            return Ok(());
        }
        let Some(kind) = body
            .get("kind")
            .and_then(Value::as_str)
            .and_then(crate::import_asset::AssetKind::from_name)
        else {
            // An absent/unknown kind is refused by the step parser before
            // any filesystem write, so there is nothing to confine.
            return Ok(());
        };
        let dir = crate::import_asset::default_library_dir(kind).ok_or_else(|| {
            anyhow!(
                "import_asset: no per-user library dir for `{kind}`; \
                 pass `library` inside the write root"
            )
        })?;
        let host = self.authorize_host_write(&dir)?;
        body.insert("library".into(), Value::String(host.display().to_string()));
        Ok(())
    }

    /// Authorize an absolute HOST write target that did not come through
    /// [`relative_path`] (the per-user asset library). The path must
    /// resolve inside the write root: existing components are
    /// canonicalized (so an escaping symlink is refused) and any
    /// not-yet-created trailing components are appended to the nearest
    /// existing ancestor. A missing write root fails closed.
    pub fn authorize_host_write(&self, host: &Path) -> Result<PathBuf> {
        let Some(root) = self.write_root.as_ref() else {
            return Err(anyhow!(
                "file access is not granted: no write root (start the server with --write-root <dir>)"
            ));
        };
        let resolved = canonicalize_existing_prefix(host)?;
        if !resolved.starts_with(root) {
            return Err(anyhow!(
                "path rejected (resolves outside the write root): `{}`",
                host.display()
            ));
        }
        Ok(resolved)
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

/// The path-bearing fields of one agent step variant and the authority
/// each needs. Every variant that carries a filesystem path is listed;
/// a new path-bearing variant MUST be added here or its paths reach the
/// session unconfined (the negative test enumerates this table).
fn step_path_fields(name: &str) -> Option<&'static [(&'static str, Access)]> {
    match name {
        "load" => Some(&[("mesh", Access::Read)]),
        "inspect" => Some(&[("mesh", Access::Read)]),
        "bake" => Some(&[("out_dir", Access::Write)]),
        "bake_transfer" => Some(&[
            ("low", Access::Read),
            ("high", Access::Read),
            ("out_dir", Access::Write),
        ]),
        "export" => Some(&[("out_dir", Access::Write)]),
        "project_open" => Some(&[("dir", Access::Read)]),
        "project_save" => Some(&[("dir", Access::Write)]),
        "paint_snapshot" => Some(&[("out", Access::Write), ("out_dir", Access::Write)]),
        "import_asset" => Some(&[("file", Access::Read), ("library", Access::Write)]),
        _ => None,
    }
}

/// Canonicalize the longest existing prefix of `path` and append the
/// components that do not exist yet. Symlinks in the existing prefix are
/// resolved, so an escaping link is visible to the caller's root check.
fn canonicalize_existing_prefix(path: &Path) -> Result<PathBuf> {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut current = path;
    loop {
        match current.canonicalize() {
            Ok(canonical) => {
                let mut out = canonical;
                for part in tail.iter().rev() {
                    out.push(part);
                }
                return Ok(out);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = current.file_name() else {
                    return Err(anyhow!(
                        "cannot resolve `{}`: no existing ancestor",
                        path.display()
                    ));
                };
                tail.push(name.to_os_string());
                let Some(parent) = current.parent() else {
                    return Err(anyhow!(
                        "cannot resolve `{}`: no existing ancestor",
                        path.display()
                    ));
                };
                current = parent;
            }
            Err(e) => return Err(anyhow!("cannot resolve `{}`: {e}", path.display())),
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
        assert!(ws
            .authorize_host_write(&inside.join("link/new.txt"))
            .is_err());
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

    /// Build a one-variant step object with the given path fields.
    fn step_value(name: &str, fields: &[(&str, &str)]) -> Value {
        let body: serde_json::Map<String, Value> = fields
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::String((*v).to_string())))
            .collect();
        let mut step = serde_json::Map::new();
        step.insert(name.to_string(), Value::Object(body));
        Value::Object(step)
    }

    #[test]
    fn confine_step_covers_every_path_bearing_variant() {
        let (inside, outside, ws) = roots("variants");
        std::fs::write(inside.join("m.glb"), b"x").unwrap();
        std::fs::create_dir_all(inside.join("out")).unwrap();
        let canonical_inside = inside.canonicalize().unwrap();

        let variants: Vec<(&str, Vec<(&str, Access)>)> = vec![
            ("load", vec![("mesh", Access::Read)]),
            ("inspect", vec![("mesh", Access::Read)]),
            ("bake", vec![("out_dir", Access::Write)]),
            (
                "bake_transfer",
                vec![
                    ("low", Access::Read),
                    ("high", Access::Read),
                    ("out_dir", Access::Write),
                ],
            ),
            ("export", vec![("out_dir", Access::Write)]),
            ("project_open", vec![("dir", Access::Read)]),
            ("project_save", vec![("dir", Access::Write)]),
            (
                "paint_snapshot",
                vec![("out", Access::Write), ("out_dir", Access::Write)],
            ),
            (
                "import_asset",
                vec![("file", Access::Read), ("library", Access::Write)],
            ),
        ];
        for (name, fields) in &variants {
            let registered = step_path_fields(name).expect("variant registered in the confiner");
            assert_eq!(registered.len(), fields.len(), "{name}");
            // Every path field refuses an escape on its own.
            for (key, _) in fields {
                let mut step = step_value(name, &[(*key, "../outside/x")]);
                assert!(
                    ws.confine_step(&mut step).is_err(),
                    "{name}.{key} escape accepted"
                );
            }
            // Every path field is rewritten to a confined host path.
            let good: Vec<(&str, &str)> = fields
                .iter()
                .map(|(key, access)| {
                    let value = match access {
                        Access::Read => "m.glb",
                        Access::Write => "out/x",
                    };
                    (*key, value)
                })
                .collect();
            let mut step = step_value(name, &good);
            ws.confine_step(&mut step)
                .unwrap_or_else(|e| panic!("{name}: {e:#}"));
            for (key, _) in fields {
                let rewritten = step[*name][*key].as_str().expect("rewritten string");
                assert!(
                    Path::new(rewritten).starts_with(&canonical_inside),
                    "{name}.{key}: {rewritten}"
                );
            }
        }
        let _ = outside;
    }

    #[test]
    fn host_write_authorization_requires_inside_the_root() {
        let (inside, outside, ws) = roots("hostwrite");
        std::fs::create_dir_all(inside.join("lib")).unwrap();
        let ok = ws
            .authorize_host_write(&inside.join("lib/new.umberbrush"))
            .unwrap();
        assert!(ok.starts_with(inside.canonicalize().unwrap()), "{ok:?}");
        assert!(ws.authorize_host_write(&outside.join("lib")).is_err());
        let none = AuthorizedWorkspace::default();
        assert!(none.authorize_host_write(&inside.join("lib")).is_err());
    }

    #[test]
    fn import_asset_without_library_requires_write_authority() {
        let _guard = crate::agent::ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_xdg = std::env::var_os("XDG_DATA_HOME");
        let old_appdata = std::env::var_os("APPDATA");
        let (inside, outside, ws) = roots("defaultlib");
        std::fs::write(inside.join("m.umberbrush"), b"brush").unwrap();
        let step = || {
            step_value(
                "import_asset",
                &[("file", "m.umberbrush"), ("kind", "brush")],
            )
        };

        std::env::set_var("XDG_DATA_HOME", &outside);
        std::env::set_var("APPDATA", &outside);
        let error = ws.confine_step(&mut step()).unwrap_err().to_string();
        assert!(
            error.contains("outside the write root"),
            "expected default-library write refusal, got: {error}"
        );

        std::env::set_var("XDG_DATA_HOME", &inside);
        std::env::set_var("APPDATA", &inside);
        let mut allowed = step();
        ws.confine_step(&mut allowed).unwrap();
        let library = allowed["import_asset"]["library"].as_str().unwrap();
        assert!(
            Path::new(library).starts_with(inside.canonicalize().unwrap()),
            "resolved default library escaped the write root: {library}"
        );
        assert!(library.ends_with("brushes"), "{library}");

        match old_xdg {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_appdata {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
    }

    #[test]
    fn confine_step_rewrites_path_fields_to_confined_host_paths() {
        let (inside, outside, ws) = roots("step");
        std::fs::write(inside.join("m.glb"), b"x").unwrap();
        let mut step = serde_json::json!({"load": {"mesh": "m.glb"}});
        ws.confine_step(&mut step).unwrap();
        let mesh = step["load"]["mesh"].as_str().unwrap();
        assert!(
            mesh.starts_with(&*inside.canonicalize().unwrap().to_string_lossy()),
            "{mesh}"
        );
        let mut bad = serde_json::json!({"load": {"mesh": "../outside/m.glb"}});
        assert!(ws.confine_step(&mut bad).is_err());
        let mut out = serde_json::json!({"export": {"out_dir": "../outside"}});
        assert!(ws.confine_step(&mut out).is_err());
        let _ = outside;
    }
}
