//! Whole-run export rollback: a write-ahead artifact guard.
//!
//! The driver's writers each overwrite their destination directly, so a
//! failure late in a run (a document writer, a second tile, a second
//! texture set) would otherwise leave earlier outputs replaced and later
//! outputs absent. [`ExportArtifactGuard`] makes the run transactional at
//! the file level:
//!
//! - [`ExportArtifactGuard::new`] creates a unique backup directory INSIDE
//!   `out_dir` (so the backup lives on the same filesystem as the outputs
//!   and can be removed with the run);
//! - [`ExportArtifactGuard::backup_all`] rejects duplicate destinations
//!   first (lexical comparison — `a/../b` and `b` are the same destination
//!   even before either exists), then copies every existing destination
//!   into the backup directory and records absent destinations as absent;
//! - [`ExportArtifactGuard::commit`] discards the backups and leaves the
//!   new outputs in place;
//! - dropping the guard without committing restores every original file
//!   and removes every newly created output, best effort.
//!
//! Deliberate boundaries:
//!
//! - Backups are file copies, never in-memory images: an 8K RGBA8 output
//!   is 256 MiB and a run can write dozens, so holding them in RAM would
//!   defeat the purpose of the guard.
//! - Destination comparison is LEXICAL, not canonicalized: the guard must
//!   work before any destination exists, and canonicalization would also
//!   follow symlinks. A path that climbs above a filesystem root cannot
//!   be represented distinctly, so those extra `..` components collapse.
//! - [`ExportArtifactGuard::backup`] refuses symlinks and non-regular
//!   destinations; the guard never follows a planted link to copy or
//!   overwrite something outside the output tree.
//! - Restoration is best effort (the Drop path cannot return an error). A
//!   read-only filesystem or a directory removed mid-run can still leave a
//!   partial export; the run's typed error names the original failure.
//! - Empty directories created by a failed run are left behind; the guard
//!   owns files, not directory lifetimes.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::driver::ExportError;

/// Distinguishes guard instances within one process; combined with the
/// process id and an existence probe, it makes the backup directory unique
/// even when several exports run concurrently in one output directory.
static BACKUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One destination the guard is responsible for. `backup` is `Some` when
/// the destination existed before the run (its bytes live in the backup
/// directory) and `None` when the run is expected to create it.
struct GuardEntry {
    path: PathBuf,
    backup: Option<PathBuf>,
}

/// A whole-run rollback guard for the files an export writes.
///
/// Create it after every path is known but before the first writer runs:
///
/// ```ignore
/// let paths = expanded_output_paths_tiled(&preset, &tiles, &sources, out_dir)?;
/// let mut guard = ExportArtifactGuard::new(out_dir)?;
/// guard.backup_all(&paths)?;
/// write_everything()?;
/// guard.commit();
/// ```
///
/// If `write_everything` returns early (or panics), dropping the guard
/// restores the directory to its pre-run state.
pub struct ExportArtifactGuard {
    backup_dir: PathBuf,
    entries: Vec<GuardEntry>,
    /// Lexical keys already backed up (or recorded absent) — makes
    /// [`Self::backup`] idempotent.
    backed_up: HashSet<PathBuf>,
    committed: bool,
}

impl ExportArtifactGuard {
    /// Creates the guard and its unique backup directory inside `out_dir`.
    ///
    /// # Errors
    ///
    /// [`ExportError::Write`] when `out_dir` cannot be created or a unique
    /// backup directory cannot be reserved.
    pub fn new(out_dir: &Path) -> Result<Self, ExportError> {
        std::fs::create_dir_all(out_dir)
            .map_err(|error| write_error(format!("creating {}: {error}", out_dir.display())))?;
        let pid = std::process::id();
        for _ in 0..1024 {
            let sequence = BACKUP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let backup_dir = out_dir.join(format!(".umber-export-backup-{pid}-{sequence}"));
            match std::fs::create_dir(&backup_dir) {
                Ok(()) => {
                    return Ok(Self {
                        backup_dir,
                        entries: Vec::new(),
                        backed_up: HashSet::new(),
                        committed: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(write_error(format!(
                        "creating backup directory {}: {error}",
                        backup_dir.display()
                    )));
                }
            }
        }
        Err(write_error(
            "could not reserve a unique export backup directory",
        ))
    }

    /// Backs up every destination in `paths`, rejecting duplicate
    /// destinations before touching any of them.
    ///
    /// # Errors
    ///
    /// [`ExportError::DuplicateOutput`] when two paths name the same
    /// destination (lexically: `a/../b` aliases `b`), and
    /// [`ExportError::Write`] when an existing destination is a symlink or
    /// non-regular file, or cannot be copied into the backup directory.
    pub fn backup_all<P: AsRef<Path>>(&mut self, paths: &[P]) -> Result<(), ExportError> {
        let mut seen = HashSet::with_capacity(paths.len());
        for path in paths {
            let key = lexical_key(path.as_ref());
            if !seen.insert(key.clone()) {
                return Err(ExportError::DuplicateOutput(key));
            }
        }
        for path in paths {
            self.backup(path.as_ref())?;
        }
        Ok(())
    }

    /// Backs up one destination (idempotent for a path already handled).
    ///
    /// An existing regular file is copied into the backup directory; an
    /// absent destination is recorded so Drop removes it if a writer
    /// creates it. Symlinks and non-regular destinations are refused.
    ///
    /// # Errors
    ///
    /// [`ExportError::Write`] for a symlink/non-regular destination or a
    /// copy/metadata failure.
    pub fn backup<P: AsRef<Path>>(&mut self, path: P) -> Result<(), ExportError> {
        let key = lexical_key(path.as_ref());
        if self.backed_up.contains(&key) {
            return Ok(());
        }
        match std::fs::symlink_metadata(&key) {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                if file_type.is_symlink() {
                    return Err(write_error(format!(
                        "refusing symlink export destination: {}",
                        key.display()
                    )));
                }
                if !file_type.is_file() {
                    return Err(write_error(format!(
                        "refusing non-regular export destination: {}",
                        key.display()
                    )));
                }
                let backup = self
                    .backup_dir
                    .join(format!("artifact-{}", self.entries.len()));
                std::fs::copy(&key, &backup).map_err(|error| {
                    write_error(format!(
                        "backing up {} to {}: {error}",
                        key.display(),
                        backup.display()
                    ))
                })?;
                self.entries.push(GuardEntry {
                    path: key.clone(),
                    backup: Some(backup),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.entries.push(GuardEntry {
                    path: key.clone(),
                    backup: None,
                });
            }
            Err(error) => {
                return Err(write_error(format!(
                    "inspecting {}: {error}",
                    key.display()
                )));
            }
        }
        self.backed_up.insert(key);
        Ok(())
    }

    /// Finishes the run: discards the backups and leaves every new output
    /// in place. Backup removal is best effort — a failed cleanup must not
    /// turn a successful export into a failed one.
    pub fn commit(mut self) {
        self.committed = true;
        let _ = std::fs::remove_dir_all(&self.backup_dir);
    }
}

impl Drop for ExportArtifactGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // Restore in reverse write order so overlapping aliases (should a
        // caller back up both) end at the oldest recorded bytes.
        for entry in self.entries.iter().rev() {
            match &entry.backup {
                Some(backup) => {
                    if let Some(parent) = entry.path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::copy(backup, &entry.path);
                }
                None => {
                    let _ = std::fs::remove_file(&entry.path);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.backup_dir);
    }
}

fn write_error(message: impl Into<String>) -> ExportError {
    ExportError::Write(format!("export artifacts: {}", message.into()))
}

/// A lexical destination key: `.` components are dropped, `..` pops the
/// preceding normal component, and surplus `..` above a root/prefix are
/// ignored. This is deliberately not `canonicalize` — destinations need
/// not exist yet, and a symlink must not be followed.
fn lexical_key(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                Some(Component::ParentDir) | Some(Component::CurDir) | None => out.push(".."),
            },
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("umber-export-guard-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn dropped_guard_restores_an_existing_destination() {
        let dir = probe_dir("restore");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("existing.png");
        std::fs::write(&path, b"original bytes").unwrap();
        {
            let mut guard = ExportArtifactGuard::new(&dir).unwrap();
            guard.backup_all(std::slice::from_ref(&path)).unwrap();
            // The simulated writer fails after replacing the file.
            std::fs::write(&path, b"partial bytes").unwrap();
            // No commit: Drop restores the original.
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropped_guard_removes_a_newly_created_destination() {
        let dir = probe_dir("new");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("new.png");
        {
            let mut guard = ExportArtifactGuard::new(&dir).unwrap();
            guard.backup_all(std::slice::from_ref(&path)).unwrap();
            std::fs::write(&path, b"new bytes").unwrap();
        }
        assert!(!path.exists(), "a failed run must remove its new output");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_keeps_the_new_bytes_and_removes_the_backup() {
        let dir = probe_dir("commit");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("existing.png");
        std::fs::write(&path, b"original bytes").unwrap();
        {
            let mut guard = ExportArtifactGuard::new(&dir).unwrap();
            guard.backup_all(std::slice::from_ref(&path)).unwrap();
            std::fs::write(&path, b"committed bytes").unwrap();
            guard.commit();
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"committed bytes");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("backup"))
            .collect();
        assert!(leftovers.is_empty(), "commit removes the backup directory");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_all_rejects_duplicate_and_lexically_aliased_destinations() {
        let dir = probe_dir("duplicate");
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("Body_baseColor.png");
        let alias = dir.join("sub/../Body_baseColor.png");
        let mut guard = ExportArtifactGuard::new(&dir).unwrap();
        let error = guard
            .backup_all(&[first.clone(), alias.clone()])
            .unwrap_err();
        assert!(
            matches!(error, ExportError::DuplicateOutput(_)),
            "expected DuplicateOutput, got {error:?}"
        );
        // Rejection happens before the first backup: no entry was made.
        assert!(guard.entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_is_idempotent_for_the_same_destination() {
        let dir = probe_dir("idempotent");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Body_baseColor.png");
        std::fs::write(&path, b"original").unwrap();
        let mut guard = ExportArtifactGuard::new(&dir).unwrap();
        guard.backup(&path).unwrap();
        guard.backup(&path).unwrap();
        assert_eq!(guard.entries.len(), 1, "one backup entry per destination");
        std::fs::write(&path, b"replacement").unwrap();
        drop(guard);
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn backup_refuses_a_symlink_destination() {
        use std::os::unix::fs::symlink;

        let dir = probe_dir("symlink");
        std::fs::create_dir_all(&dir).unwrap();
        let outside = dir.parent().unwrap().join(format!(
            "umber-export-guard-outside-{}.txt",
            std::process::id()
        ));
        std::fs::write(&outside, b"keep").unwrap();
        let link = dir.join("Body_baseColor.png");
        symlink(&outside, &link).unwrap();
        let mut guard = ExportArtifactGuard::new(&dir).unwrap();
        assert!(matches!(guard.backup(&link), Err(ExportError::Write(_))));
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
