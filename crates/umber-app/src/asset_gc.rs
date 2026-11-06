//! Project-level `AssetStore` garbage collection (requirements row 121's
//! named v1 boundary, "no store GC"). Every paint save stores each
//! sidecar artifact as a content-hash blob under `<project>/assets/` and
//! names it in a commit manifest; superseded blobs used to pile up there
//! forever. [`collect`] removes them.
//!
//! The live set is exactly the union of the two committed manifests:
//!
//! * `paint/manifest.txt` (`<name> <hash> <ext> <size> <temp>` lines,
//!   parsed by `umber_paint::persistence::parse_paint_commit`);
//! * `paint/quick_mask/manifest.txt` (`<stem>.<json|bin> <blake3>` lines,
//!   whose blob is `<blake3>.<json|bin>` in the same store).
//!
//! Safety rules, in the order [`collect`] applies them:
//!
//! 1. **Only after a successful commit.** A `manifest.pending` (a commit
//!    that has not rolled forward) or any staged `*.tmp.<pid>` under
//!    `paint/` (a save in flight, or a crash the next open sweeps) makes
//!    the pass refuse with [`GcError::CommitPending`] /
//!    [`GcError::SaveInProgress`]: the blobs such a save stored are not
//!    in any committed manifest yet.
//! 2. **A hostile manifest refuses the whole pass.** Any malformed line
//!    is [`GcError::MalformedManifest`] (path, line number, text). A
//!    manifest the parser cannot read never shrinks the live set.
//! 3. **No manifest, no GC.** A non-empty store with neither manifest is
//!    [`GcError::NoCommittedManifest`]: an empty live set would condemn
//!    every blob, and that is never what a first GC should do.
//! 4. **Re-check at delete time.** Before each delete the manifests are
//!    re-read from disk (and rule 1 re-checked); a blob that became
//!    referenced since the scan is kept.
//! 5. **Rename, then remove.** A doomed blob is first renamed to
//!    `<hash>.<ext>.gc-trash.<pid>` in its shard, then unlinked. A GC
//!    killed between the two leaves a trash file; the next pass removes
//!    it, or renames it back when its blob is referenced again and the
//!    real name is missing, so a killed GC converges and never loses a
//!    referenced blob.
//! 6. **Grace period.** Orphans younger than [`GcOptions::min_age`] are
//!    kept (git's `gc.pruneExpire` rule): it covers a blob a concurrent
//!    save or mesh-map import stored but has not committed yet.
//!
//! Files the store does not own (not `<64 hex>.<alnum ext>` under a
//! matching 2-hex shard, symlinks, nested dirs) are counted as foreign
//! and never touched.
//!
//! v1 boundaries: the mesh-map import's blobs are session-referenced
//! only (the applied layer's plane is what the next save commits), so
//! the grace period is their only protection before that save;
//! `project.json` and the layer JSON hold no `AssetRef`s today, so they
//! are not scanned.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use thiserror::Error;
use umber_paint::persistence::{
    is_paint_temp_name, paint_commit_pending_path, paint_manifest_path, paint_sidecar_dir,
    parse_paint_commit, parse_quick_mask_commit, quick_mask_manifest_path, quick_mask_sidecar_dir,
};

/// Manifests above this size are refused unread (a hostile project
/// directory must not make the GC allocate without bound).
pub const MAX_MANIFEST_BYTES: u64 = 64 << 20;

/// The infix of a blob a GC pass renamed away but has not unlinked yet.
pub const TRASH_INFIX: &str = ".gc-trash.";

/// Everything that stops a GC pass. Every variant leaves the store as
/// it was at the moment of the refusal.
#[derive(Debug, Error)]
pub enum GcError {
    /// A filesystem call failed.
    #[error("asset GC: I/O error at {path}: {source}")]
    Io {
        /// The path the failing call touched.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A manifest line does not parse (tampering or corruption).
    #[error("asset GC refused: malformed manifest {path} line {line}: {text:?}")]
    MalformedManifest {
        /// The manifest file.
        path: PathBuf,
        /// 1-based line number (0: the file is not UTF-8).
        line: usize,
        /// The offending line (truncated to 120 chars).
        text: String,
    },
    /// A manifest is larger than [`MAX_MANIFEST_BYTES`].
    #[error("asset GC refused: manifest {path} is {bytes} bytes (cap {MAX_MANIFEST_BYTES})")]
    ManifestTooLarge {
        /// The manifest file.
        path: PathBuf,
        /// Its size.
        bytes: u64,
    },
    /// A commit is pending roll-forward; open or save the project first.
    #[error("asset GC refused: commit pending at {path} (open or save the project first)")]
    CommitPending {
        /// The pending manifest.
        path: PathBuf,
    },
    /// A staged save temp exists; the save has not committed.
    #[error("asset GC refused: staged save temp {path} (a save is in flight or crashed)")]
    SaveInProgress {
        /// The staged temp.
        path: PathBuf,
    },
    /// The store holds blobs but no committed manifest exists.
    #[error("asset GC refused: the store holds blobs but no committed manifest names any")]
    NoCommittedManifest,
    /// The pass's mutation budget ran out (the fault-injection seam the
    /// killed-GC tests drive; production passes no budget).
    #[error("asset GC interrupted after its step budget")]
    Interrupted,
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> GcError + '_ {
    move |source| GcError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// How a pass runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcOptions {
    /// List what would go and the bytes reclaimable; touch nothing.
    pub dry_run: bool,
    /// Orphans whose mtime is younger than this are kept (rule 6).
    pub min_age: Duration,
}

/// The default grace period: an hour, so an uncommitted blob from a
/// save or import in flight is never condemned.
pub const DEFAULT_MIN_AGE: Duration = Duration::from_secs(60 * 60);

impl Default for GcOptions {
    /// Dry run with the default grace period: deleting is opt-in.
    fn default() -> Self {
        Self {
            dry_run: true,
            min_age: DEFAULT_MIN_AGE,
        }
    }
}

/// One pass's tally. In a dry run `deleted_*` stay zero and
/// `orphan_bytes` is the bytes reclaimable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GcReport {
    /// Whether the pass was a dry run.
    pub dry_run: bool,
    /// Blobs some committed manifest names.
    pub live_blobs: u64,
    /// Bytes the live blobs occupy.
    pub live_bytes: u64,
    /// Orphans eligible for deletion (past the grace period).
    pub orphan_blobs: u64,
    /// Bytes the eligible orphans occupy (reclaimable).
    pub orphan_bytes: u64,
    /// The eligible orphans' paths, sorted.
    pub orphans: Vec<PathBuf>,
    /// Orphans kept because they are younger than the grace period.
    pub kept_young: u64,
    /// Orphans kept because the delete-time re-read found them referenced.
    pub kept_rereferenced: u64,
    /// Blobs actually deleted.
    pub deleted_blobs: u64,
    /// Bytes actually deleted.
    pub deleted_bytes: u64,
    /// Leftover trash files (from a killed pass) removed.
    pub trash_removed: u64,
    /// Leftover trash files renamed back because their blob is referenced.
    pub trash_restored: u64,
    /// Entries the store does not own, left untouched.
    pub foreign: u64,
}

impl GcReport {
    /// JSON shape for the control channel.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "dry_run": self.dry_run,
            "live_blobs": self.live_blobs,
            "live_bytes": self.live_bytes,
            "orphan_blobs": self.orphan_blobs,
            "orphan_bytes": self.orphan_bytes,
            "orphans": self.orphans.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "kept_young": self.kept_young,
            "kept_rereferenced": self.kept_rereferenced,
            "deleted_blobs": self.deleted_blobs,
            "deleted_bytes": self.deleted_bytes,
            "trash_removed": self.trash_removed,
            "trash_restored": self.trash_restored,
            "foreign": self.foreign,
        })
    }
}

/// A blob's identity in the store: `(hash, ext)`, its filename's halves.
type BlobKey = (String, String);

fn read_manifest(path: &Path) -> Result<Option<String>, GcError> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(path)(e)),
    };
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(GcError::ManifestTooLarge {
            path: path.to_path_buf(),
            bytes: meta.len(),
        });
    }
    let bytes = fs::read(path).map_err(io_err(path))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| GcError::MalformedManifest {
            path: path.to_path_buf(),
            line: 0,
            text: "<not UTF-8>".into(),
        })
}

/// The first line `parse` refuses, as a typed error (the parsers say
/// only "some line is bad"; re-parsing line by line names which).
fn locate_bad_line<T>(path: &Path, text: &str, parse: impl Fn(&str) -> Option<T>) -> GcError {
    let bad = text
        .lines()
        .enumerate()
        .find(|(_, line)| parse(line).is_none());
    let (line, text) = bad.map_or((0, String::new()), |(i, l)| (i + 1, l.to_string()));
    GcError::MalformedManifest {
        path: path.to_path_buf(),
        line,
        text: text.chars().take(120).collect(),
    }
}

/// The live set: every blob a committed manifest names. Returns `None`
/// when neither manifest exists.
pub fn read_live_set(project: &Path) -> Result<Option<HashSet<BlobKey>>, GcError> {
    let mut live = HashSet::new();
    let mut found = false;
    let paint = paint_manifest_path(project);
    if let Some(text) = read_manifest(&paint)? {
        found = true;
        let entries = parse_paint_commit(&text)
            .ok_or_else(|| locate_bad_line(&paint, &text, parse_paint_commit))?;
        live.extend(entries.into_iter().map(|e| (e.asset.hash, e.asset.ext)));
    }
    let masks = quick_mask_manifest_path(project);
    if let Some(text) = read_manifest(&masks)? {
        found = true;
        let entries = parse_quick_mask_commit(&text)
            .ok_or_else(|| locate_bad_line(&masks, &text, parse_quick_mask_commit))?;
        for (name, hash) in entries {
            // `parse_quick_mask_commit` admits only `.json` / `.bin`.
            let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
            live.insert((hash.to_hex().to_string(), ext.to_string()));
        }
    }
    Ok(found.then_some(live))
}

/// Rule 1: refuses while a commit is pending or a save is staged.
fn check_committed(project: &Path) -> Result<(), GcError> {
    for pending in [
        paint_commit_pending_path(project),
        quick_mask_sidecar_dir(project).join("manifest.pending"),
    ] {
        if fs::symlink_metadata(&pending).is_ok() {
            return Err(GcError::CommitPending { path: pending });
        }
    }
    let mut stack = vec![paint_sidecar_dir(project)];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read {
            let entry = entry.map_err(io_err(&dir))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(io_err(&path))?;
            if kind.is_dir() {
                stack.push(path);
            } else if is_paint_temp_name(&entry.file_name().to_string_lossy()) {
                return Err(GcError::SaveInProgress { path });
            }
        }
    }
    Ok(())
}

/// A store-owned name: `<64 lowercase hex>.<alnum ext>`.
fn parse_blob_name(name: &str) -> Option<BlobKey> {
    let (hash, ext) = name.split_once('.')?;
    let hex = hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let ext_ok = !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    (hex && ext_ok).then(|| (hash.to_string(), ext.to_string()))
}

/// A trash name: `<blob name>.gc-trash.<digits>` → the blob's key.
fn parse_trash_name(name: &str) -> Option<BlobKey> {
    let (blob, pid) = name.rsplit_once(TRASH_INFIX)?;
    let pid_ok = !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit());
    pid_ok.then(|| parse_blob_name(blob)).flatten()
}

/// What the scan found in one shard entry.
enum Found {
    Blob {
        key: BlobKey,
        path: PathBuf,
        bytes: u64,
        modified: SystemTime,
    },
    Trash {
        key: BlobKey,
        path: PathBuf,
    },
}

fn scan_store(root: &Path, report: &mut GcReport) -> Result<Vec<Found>, GcError> {
    let mut found = Vec::new();
    let read = match fs::read_dir(root) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(io_err(root)(e)),
    };
    for shard in read {
        let shard = shard.map_err(io_err(root))?;
        let shard_path = shard.path();
        let shard_name = shard.file_name().to_string_lossy().into_owned();
        let is_dir = shard.file_type().map_err(io_err(&shard_path))?.is_dir();
        let shard_ok = shard_name.len() == 2
            && shard_name
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !is_dir || !shard_ok {
            report.foreign += 1;
            continue;
        }
        for entry in fs::read_dir(&shard_path).map_err(io_err(&shard_path))? {
            let entry = entry.map_err(io_err(&shard_path))?;
            let path = entry.path();
            // symlink_metadata: a symlink is foreign, never followed.
            let meta = fs::symlink_metadata(&path).map_err(io_err(&path))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let in_shard = |key: &BlobKey| key.0.starts_with(&shard_name);
            if !meta.is_file() {
                report.foreign += 1;
            } else if let Some(key) = parse_trash_name(&name).filter(in_shard) {
                found.push(Found::Trash { key, path });
            } else if let Some(key) = parse_blob_name(&name).filter(in_shard) {
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                found.push(Found::Blob {
                    key,
                    path,
                    bytes: meta.len(),
                    modified,
                });
            } else {
                report.foreign += 1;
            }
        }
    }
    // Deterministic order (reports, tests, the kill simulation).
    found.sort_by(|a, b| found_path(a).cmp(found_path(b)));
    Ok(found)
}

fn found_path(found: &Found) -> &Path {
    match found {
        Found::Blob { path, .. } | Found::Trash { path, .. } => path,
    }
}

fn blob_path_for(root: &Path, key: &BlobKey) -> PathBuf {
    root.join(&key.0[..2]).join(format!("{}.{}", key.0, key.1))
}

/// The fault-injection seam: each filesystem mutation spends one step;
/// `None` is unlimited (production).
struct Budget(Option<usize>);

impl Budget {
    fn spend(&mut self) -> Result<(), GcError> {
        match &mut self.0 {
            Some(0) => Err(GcError::Interrupted),
            Some(n) => {
                *n -= 1;
                Ok(())
            }
            None => Ok(()),
        }
    }
}

/// Whether `key` is referenced right now, re-read from disk (rule 4).
/// Re-checks rule 1 too: a save that started since the scan refuses.
fn referenced_now(project: &Path, key: &BlobKey) -> Result<bool, GcError> {
    check_committed(project)?;
    Ok(read_live_set(project)?.is_some_and(|live| live.contains(key)))
}

/// Runs one GC pass over `<project>/assets/` (see the module docs for
/// the rules). Call it after a successful save commit; it refuses
/// otherwise.
pub fn collect(project: &Path, options: GcOptions) -> Result<GcReport, GcError> {
    collect_with_budget(project, options, None)
}

fn collect_with_budget(
    project: &Path,
    options: GcOptions,
    budget: Option<usize>,
) -> Result<GcReport, GcError> {
    let mut budget = Budget(budget);
    let mut report = GcReport {
        dry_run: options.dry_run,
        ..GcReport::default()
    };
    check_committed(project)?;
    let live = read_live_set(project)?;
    let root = project.join("assets");
    let found = scan_store(&root, &mut report)?;
    let any_blob = found.iter().any(|f| matches!(f, Found::Blob { .. }));
    let Some(live) = live else {
        if any_blob {
            return Err(GcError::NoCommittedManifest);
        }
        return Ok(report);
    };
    let now = SystemTime::now();
    let mut doomed = Vec::new();
    for item in found {
        match item {
            Found::Trash { key, path } => {
                if options.dry_run {
                    continue;
                }
                let blob = blob_path_for(&root, &key);
                if referenced_now(project, &key)? && fs::symlink_metadata(&blob).is_err() {
                    budget.spend()?;
                    fs::rename(&path, &blob).map_err(io_err(&path))?;
                    report.trash_restored += 1;
                } else {
                    budget.spend()?;
                    remove_if_present(&path)?;
                    report.trash_removed += 1;
                }
            }
            Found::Blob {
                key,
                path,
                bytes,
                modified,
            } => {
                if live.contains(&key) {
                    report.live_blobs += 1;
                    report.live_bytes += bytes;
                    continue;
                }
                let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
                if age < options.min_age {
                    report.kept_young += 1;
                    continue;
                }
                report.orphan_blobs += 1;
                report.orphan_bytes += bytes;
                report.orphans.push(path.clone());
                doomed.push((key, path, bytes));
            }
        }
    }
    if options.dry_run {
        return Ok(report);
    }
    for (key, path, bytes) in doomed {
        if referenced_now(project, &key)? {
            report.kept_rereferenced += 1;
            continue;
        }
        // Rule 5: rename into the shard's trash name, then unlink.
        let mut trash = path.as_os_str().to_owned();
        trash.push(format!("{TRASH_INFIX}{}", std::process::id()));
        let trash = PathBuf::from(trash);
        budget.spend()?;
        match fs::rename(&path, &trash) {
            Ok(()) => {}
            // Gone since the scan (another pass): nothing to delete.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io_err(&path)(e)),
        }
        // Referenced between the check and the rename: put it back.
        if referenced_now(project, &key)? && fs::symlink_metadata(&path).is_err() {
            budget.spend()?;
            fs::rename(&trash, &path).map_err(io_err(&trash))?;
            report.kept_rereferenced += 1;
            continue;
        }
        budget.spend()?;
        remove_if_present(&trash)?;
        report.deleted_blobs += 1;
        report.deleted_bytes += bytes;
    }
    Ok(report)
}

fn remove_if_present(path: &Path) -> Result<(), GcError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(path)(e)),
    }
}

/// `umber gc <project> [--delete] [--min-age-secs N]`: the shell
/// subcommand. Dry run unless `--delete`. Prints the report as JSON on
/// stdout; a refusal goes to stderr with exit code 1, a usage error
/// with exit code 2. `args` excludes the program name and the `gc` word.
pub fn run_cli(
    args: &[String],
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> i32 {
    const USAGE: &str = "usage: umber gc <project-dir> [--delete] [--min-age-secs N]";
    let mut project = None;
    let mut options = GcOptions::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--delete" => options.dry_run = false,
            "--min-age-secs" => match it.next().and_then(|v| v.parse().ok()) {
                Some(secs) => options.min_age = Duration::from_secs(secs),
                None => {
                    let _ = writeln!(err, "umber gc: --min-age-secs needs a number\n{USAGE}");
                    return 2;
                }
            },
            flag if flag.starts_with("--") || project.is_some() => {
                let _ = writeln!(err, "umber gc: unexpected argument {flag:?}\n{USAGE}");
                return 2;
            }
            path => project = Some(PathBuf::from(path)),
        }
    }
    let Some(project) = project.filter(|p| p.is_dir()) else {
        let _ = writeln!(err, "umber gc: needs an existing project folder\n{USAGE}");
        return 2;
    };
    match collect(&project, options) {
        Ok(report) => {
            let _ = writeln!(out, "{}", report.to_json());
            0
        }
        Err(e) => {
            let _ = writeln!(err, "umber gc: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_core::{AssetRef, AssetStore};
    use umber_paint::persistence::{
        format_paint_commit, format_quick_mask_commit, PaintCommitEntry,
    };

    /// A project dir with `paint/` and an empty store.
    fn project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("umber-asset-gc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(paint_sidecar_dir(&dir)).unwrap();
        dir
    }

    fn store(dir: &Path, bytes: &[u8], ext: &str) -> AssetRef {
        AssetStore::new(dir).store(bytes, ext).unwrap()
    }

    fn blob_path(dir: &Path, r: &AssetRef) -> PathBuf {
        blob_path_for(&dir.join("assets"), &(r.hash.clone(), r.ext.clone()))
    }

    /// Commits `refs` as the paint manifest (`tile-<1001+i>.png`).
    fn commit_paint(dir: &Path, refs: &[&AssetRef]) {
        let entries: Vec<PaintCommitEntry> = refs
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let name = format!("tile-{}.png", 1001 + i);
                PaintCommitEntry {
                    temp: format!("{name}.tmp.1"),
                    name,
                    asset: (*r).clone(),
                }
            })
            .collect();
        fs::write(paint_manifest_path(dir), format_paint_commit(&entries, &[])).unwrap();
    }

    fn delete_now() -> GcOptions {
        GcOptions {
            dry_run: false,
            min_age: Duration::ZERO,
        }
    }

    fn dry_now() -> GcOptions {
        GcOptions {
            dry_run: true,
            min_age: Duration::ZERO,
        }
    }

    /// Every file under `assets/`, sorted.
    fn store_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.join("assets")];
        while let Some(d) = stack.pop() {
            let Ok(read) = fs::read_dir(&d) else { continue };
            for e in read.flatten() {
                if e.path().is_dir() {
                    stack.push(e.path());
                } else {
                    out.push(e.path());
                }
            }
        }
        out.sort();
        out
    }

    /// The fixture: 2 paint-referenced blobs, 1 quick-mask-referenced
    /// blob, 3 orphans (5 + 7 + 11 bytes = 23 reclaimable).
    struct Fixture {
        dir: PathBuf,
        live: Vec<AssetRef>,
        orphans: Vec<AssetRef>,
    }

    fn fixture(tag: &str) -> Fixture {
        let dir = project(tag);
        let a = store(&dir, b"live tile A bytes", "png");
        let b = store(&dir, b"live tile B", "png");
        let mask_bytes = b"quick mask carry";
        let mask = store(&dir, mask_bytes, "bin");
        commit_paint(&dir, &[&a, &b]);
        fs::create_dir_all(quick_mask_sidecar_dir(&dir)).unwrap();
        let stem = "0".repeat(64);
        fs::write(
            quick_mask_manifest_path(&dir),
            format_quick_mask_commit(&[(format!("{stem}.bin"), blake3::hash(mask_bytes))]),
        )
        .unwrap();
        let orphans = vec![
            store(&dir, b"old-1", "png"),
            store(&dir, b"old-two", "png"),
            store(&dir, b"old-three!!", "bin"),
        ];
        Fixture {
            dir,
            live: vec![a, b, mask],
            orphans,
        }
    }

    #[test]
    fn gc_removes_exactly_the_orphans_with_byte_counts() {
        let f = fixture("orphans");
        let live_bytes: u64 = f.live.iter().map(|r| r.size).sum();
        let report = collect(&f.dir, delete_now()).unwrap();
        assert_eq!(report.live_blobs, 3);
        assert_eq!(report.live_bytes, live_bytes);
        assert_eq!(report.orphan_blobs, 3);
        assert_eq!(report.orphan_bytes, 5 + 7 + 11);
        assert_eq!(report.deleted_blobs, 3);
        assert_eq!(report.deleted_bytes, 23);
        for r in &f.live {
            assert_eq!(
                AssetStore::new(&f.dir).load(r).unwrap().len() as u64,
                r.size
            );
        }
        for r in &f.orphans {
            assert!(!blob_path(&f.dir, r).exists(), "orphan {} survived", r.hash);
        }
        assert_eq!(store_files(&f.dir).len(), 3, "only the live blobs remain");
        // A second pass is a no-op.
        let again = collect(&f.dir, delete_now()).unwrap();
        assert_eq!(
            (again.orphan_blobs, again.deleted_blobs, again.live_blobs),
            (0, 0, 3)
        );
        fs::remove_dir_all(&f.dir).unwrap();
    }

    #[test]
    fn dry_run_lists_reclaimable_bytes_and_changes_nothing() {
        let f = fixture("dry");
        let before: Vec<(PathBuf, Vec<u8>)> = store_files(&f.dir)
            .into_iter()
            .map(|p| (p.clone(), fs::read(&p).unwrap()))
            .collect();
        let report = collect(&f.dir, dry_now()).unwrap();
        assert!(report.dry_run);
        assert_eq!((report.orphan_blobs, report.orphan_bytes), (3, 23));
        assert_eq!((report.deleted_blobs, report.deleted_bytes), (0, 0));
        let mut listed = report.orphans.clone();
        listed.sort();
        let mut expected: Vec<PathBuf> = f.orphans.iter().map(|r| blob_path(&f.dir, r)).collect();
        expected.sort();
        assert_eq!(listed, expected);
        let after: Vec<(PathBuf, Vec<u8>)> = store_files(&f.dir)
            .into_iter()
            .map(|p| (p.clone(), fs::read(&p).unwrap()))
            .collect();
        assert_eq!(before, after, "a dry run touched the store");
        // The default options are a dry run too.
        assert!(collect(&f.dir, GcOptions::default()).unwrap().dry_run);
        fs::remove_dir_all(&f.dir).unwrap();
    }

    #[test]
    fn killed_mid_gc_converges_without_losing_a_referenced_blob() {
        // Every kill point: a pass interrupted after `k` mutations, then a
        // clean re-run. The 3 orphans cost 2 mutations each (rename to
        // trash, unlink), so k = 0..=6 covers every boundary.
        let mut torn_seen = false;
        for k in 0..=6 {
            let f = fixture(&format!("kill-{k}"));
            let first = collect_with_budget(&f.dir, delete_now(), Some(k));
            if k < 6 {
                assert!(
                    matches!(first, Err(GcError::Interrupted)),
                    "k={k}: {first:?}"
                );
            }
            for r in &f.live {
                assert!(
                    blob_path(&f.dir, r).is_file(),
                    "k={k}: live blob lost after kill"
                );
            }
            torn_seen |= store_files(&f.dir)
                .iter()
                .any(|p| p.to_string_lossy().contains(TRASH_INFIX));
            let rerun = collect(&f.dir, delete_now()).unwrap();
            assert_eq!(rerun.live_blobs, 3, "k={k}");
            let left = store_files(&f.dir);
            let mut expected: Vec<PathBuf> = f.live.iter().map(|r| blob_path(&f.dir, r)).collect();
            expected.sort();
            assert_eq!(left, expected, "k={k}: the re-run did not converge");
            fs::remove_dir_all(&f.dir).unwrap();
        }
        assert!(
            torn_seen,
            "no kill point left a trash file: the sweep tests nothing"
        );
    }

    #[test]
    fn half_deleted_store_converges_and_a_referenced_trash_is_restored() {
        let f = fixture("half");
        // A killed pass's debris: one orphan unlinked, one renamed to trash
        // but not unlinked, and a LIVE blob caught in trash with its real
        // name missing (a pass killed after the manifest re-referenced it).
        fs::remove_file(blob_path(&f.dir, &f.orphans[0])).unwrap();
        let trash = |p: PathBuf| PathBuf::from(format!("{}{TRASH_INFIX}4242", p.display()));
        let o1 = blob_path(&f.dir, &f.orphans[1]);
        fs::rename(&o1, trash(o1.clone())).unwrap();
        let l0 = blob_path(&f.dir, &f.live[0]);
        fs::rename(&l0, trash(l0.clone())).unwrap();
        let report = collect(&f.dir, delete_now()).unwrap();
        assert_eq!((report.trash_removed, report.trash_restored), (1, 1));
        assert_eq!((report.deleted_blobs, report.deleted_bytes), (1, 11));
        for r in &f.live {
            assert_eq!(
                AssetStore::new(&f.dir).load(r).unwrap().len() as u64,
                r.size
            );
        }
        assert_eq!(store_files(&f.dir).len(), 3);
        fs::remove_dir_all(&f.dir).unwrap();
    }

    #[test]
    fn hostile_manifest_refuses_with_a_typed_error_and_deletes_nothing() {
        let f = fixture("hostile");
        let path = paint_manifest_path(&f.dir);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("../../etc/passwd deadbeef png 1 x\n");
        fs::write(&path, &text).unwrap();
        let before = store_files(&f.dir);
        match collect(&f.dir, delete_now()) {
            Err(GcError::MalformedManifest {
                path: p,
                line,
                text,
            }) => {
                assert_eq!(p, path);
                assert_eq!(line, 4, "header + 2 entries, then the bad line");
                assert!(text.starts_with("../../etc/passwd"));
            }
            other => panic!("expected MalformedManifest, got {other:?}"),
        }
        assert_eq!(store_files(&f.dir), before);
        // A malformed quick-mask manifest refuses the same way.
        commit_paint(&f.dir, &[&f.live[0], &f.live[1]]);
        fs::write(
            quick_mask_manifest_path(&f.dir),
            "# Umber quick-mask commit v1\nnot a line\n",
        )
        .unwrap();
        assert!(matches!(
            collect(&f.dir, delete_now()),
            Err(GcError::MalformedManifest { line: 2, .. })
        ));
        // Non-UTF-8 bytes are typed too.
        fs::write(&path, [0xff, 0xfe, b'\n']).unwrap();
        assert!(matches!(
            collect(&f.dir, dry_now()),
            Err(GcError::MalformedManifest { line: 0, .. })
        ));
        assert_eq!(store_files(&f.dir), before);
        fs::remove_dir_all(&f.dir).unwrap();
    }

    #[test]
    fn uncommitted_state_refuses_the_pass() {
        let f = fixture("pending");
        let before = store_files(&f.dir);
        let pending = paint_commit_pending_path(&f.dir);
        fs::write(&pending, "# Umber paint commit v2\n").unwrap();
        assert!(matches!(
            collect(&f.dir, delete_now()),
            Err(GcError::CommitPending { .. })
        ));
        fs::remove_file(&pending).unwrap();
        let staged = paint_sidecar_dir(&f.dir).join("tile-1001.png.tmp.77");
        fs::write(&staged, b"x").unwrap();
        assert!(matches!(
            collect(&f.dir, delete_now()),
            Err(GcError::SaveInProgress { .. })
        ));
        fs::remove_file(&staged).unwrap();
        // No manifest at all, but blobs present: refuse, never "all orphans".
        fs::remove_file(paint_manifest_path(&f.dir)).unwrap();
        fs::remove_file(quick_mask_manifest_path(&f.dir)).unwrap();
        assert!(matches!(
            collect(&f.dir, delete_now()),
            Err(GcError::NoCommittedManifest)
        ));
        assert_eq!(store_files(&f.dir), before);
        // No manifest and no store: a clean empty report.
        let empty = project("empty");
        assert_eq!(
            collect(&empty, delete_now()).unwrap(),
            GcReport {
                dry_run: false,
                ..GcReport::default()
            }
        );
        fs::remove_dir_all(&f.dir).unwrap();
        fs::remove_dir_all(&empty).unwrap();
    }

    #[test]
    fn young_orphans_and_foreign_files_are_kept() {
        let f = fixture("young");
        let foreign = f
            .dir
            .join("assets")
            .join(&f.live[0].hash[..2])
            .join("README.txt");
        fs::write(&foreign, b"not ours").unwrap();
        let options = GcOptions {
            dry_run: false,
            min_age: Duration::from_secs(3600),
        };
        let report = collect(&f.dir, options).unwrap();
        assert_eq!(
            (report.kept_young, report.deleted_blobs, report.foreign),
            (3, 0, 1)
        );
        assert!(foreign.is_file());
        assert_eq!(store_files(&f.dir).len(), 7);
        fs::remove_dir_all(&f.dir).unwrap();
    }

    #[test]
    fn cli_is_dry_by_default_and_types_its_failures() {
        let f = fixture("cli");
        let run = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let code = run_cli(&args, &mut out, &mut err);
            (
                code,
                String::from_utf8(out).unwrap(),
                String::from_utf8(err).unwrap(),
            )
        };
        let dir = f.dir.display().to_string();
        let (code, out, _) = run(&[&dir, "--min-age-secs", "0"]);
        assert_eq!(code, 0);
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            (json["dry_run"].as_bool(), json["orphan_bytes"].as_u64()),
            (Some(true), Some(23))
        );
        assert_eq!(store_files(&f.dir).len(), 6);
        assert_eq!(run(&[]).0, 2);
        assert_eq!(run(&[&dir, "--bogus"]).0, 2);
        assert_eq!(run(&[&dir, "--min-age-secs"]).0, 2);
        let (code, out, _) = run(&[&dir, "--delete", "--min-age-secs", "0"]);
        assert_eq!(code, 0, "{out}");
        assert_eq!(store_files(&f.dir).len(), 3);
        fs::write(
            paint_manifest_path(&f.dir),
            "# Umber paint commit v2\ngarbage\n",
        )
        .unwrap();
        let (code, _, err) = run(&[&dir, "--delete"]);
        assert_eq!(code, 1);
        assert!(err.contains("malformed manifest"), "{err}");
        fs::remove_dir_all(&f.dir).unwrap();
    }
}
