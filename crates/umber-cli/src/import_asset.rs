//! `umber-cli import-asset` (assets wave K5): validated import of one
//! asset file into a library directory.
//!
//! Validation reuses the SAME loaders the GUI's assets panel drives, so a
//! file the CLI accepts is a file the panel will list:
//!
//! * `smart-material` — extension `umbersmart` (the `document` listers'
//!   filter) + `umber_core::smart::SmartMaterial::load_file`
//! * `smart-mask` — extension `umbermask` +
//!   `umber_core::smart::SmartMask::load_file`
//! * `smart-filter` — extension `umberfilter` +
//!   `umber_core::smart::SmartFilter::load_file`
//! * `brush` — extension `umberbrush` (`preset_library`'s scan filter) +
//!   the per-file pipeline of `preset_library::load_one_file` (bytes ->
//!   UTF-8 -> `BrushPreset::from_json` -> `validate()`)
//! * `brush` with an `.abr` file (row 284, case-insensitive extension) —
//!   a Photoshop brush LIBRARY, not one asset: validated by
//!   `umber_brush::abr::read_abr` (parse level), then FANNED OUT by
//!   [`import_abr_asset`] into one `<slug>.umberbrush` per kept brush plus
//!   a `<slug>.stamp.png` sidecar (`preset.rs`'s convention) for each
//!   stamped one, via `umber_brush::abr_import::import_abr` (row 283)
//! * `env` — the `env::format_for_path` gate (`.png`/`.exr`,
//!   case-insensitive -> `umber_gpu::EnvFormat`); pixel decode happens at
//!   GPU load time in the GUI, so the CLI's gate is the format gate.
//!
//! An invalid file is a typed refusal ([`ImportError`]) — never a silent
//! copy. A valid file is copied bytes-untouched (`fs::copy`) into the
//! per-user library dir for its kind, or `--library DIR`.
//!
//! Per-user library dirs (the same roots the GUI reads): brushes ->
//! `preset_library::user_preset_dir()`; smart assets and env maps -> the
//! state dir (the parent of the user preset dir, `gpu_startup::state_dir`'s
//! rule) joined with `smart` / `env`.
//!
//! ## The `.abr` fan-out (row 284)
//!
//! Scope per `docs/loop/briefs/2026-11-07-wave-abr-design.md`: one `.abr`
//! is a library, so [`import_abr_asset`] writes N files, not one copy.
//! Judgment calls:
//!
//! * slug = the mapped preset name, ASCII-lowercased; every run of
//!   non-`[a-z0-9]` chars (non-ASCII letters included — portable file
//!   names) becomes one `-`; leading/trailing dashes trimmed; capped at
//!   [`ABR_SLUG_MAX`] chars; an empty result is `abr-brush-{brush_index}`;
//!   a Windows reserved device name (`con`, `nul`, `com1`, …) gets
//!   `-brush` appended so the library stays portable. Collisions within
//!   one run get `-2`, `-3`, … in entry order.
//! * ALL targets are planned before anything is written; one existing
//!   target refuses the whole import ([`ImportError::TargetExists`]) —
//!   the single-file lane's no-overwrite rule, applied to the set.
//! * the stamp sidecar is written `Transfer::Linear` (a coverage data
//!   plane, the stencil converter's rule), rows in file order (top row
//!   first, as the `.abr` stores them).
//! * a dry run reports the PLANNED counts (`presets` / `stamps_written`
//!   = what would be written) with `dry_run: true`.
//! * a mid-write IO failure can leave the files already written in
//!   place (no rollback); the targets were all fresh, so nothing the
//!   user owned was overwritten.
//! * [`import_asset`] on a brush `.abr` delegates to the fan-out and
//!   summarizes it as an [`ImportOutcome`] whose `target` is the
//!   library DIR and whose `bytes` is the total written; the CLI and the
//!   agent step call [`import_abr_asset`] directly for the full outcome.
//!
//! Non-goals (v1): cross-library stamp dedup (blake3 of a new stamp vs
//! the library's existing sidecars — follow-up), `patt` patterns, v8+
//! files, dynamics → engine wiring, and `.abr` WRITING (never).

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// The five importable asset kinds — K5's `--kind` vocabulary plus the
/// smart-filter kind (3b1f4ae's `.umberfilter` tier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    SmartMaterial,
    SmartMask,
    SmartFilter,
    Brush,
    Env,
}

impl AssetKind {
    /// Parses the CLI `--kind` value.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "smart-material" => Some(Self::SmartMaterial),
            "smart-mask" => Some(Self::SmartMask),
            "smart-filter" => Some(Self::SmartFilter),
            "brush" => Some(Self::Brush),
            "env" => Some(Self::Env),
            _ => None,
        }
    }

    /// The CLI spelling of this kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::SmartMaterial => "smart-material",
            Self::SmartMask => "smart-mask",
            Self::SmartFilter => "smart-filter",
            Self::Brush => "brush",
            Self::Env => "env",
        }
    }

    /// The extension the GUI's library listers scan for, case-sensitive
    /// (`document::list_smart_*`, `preset_library::sorted_preset_files`;
    /// the smart tier's extensions are the core
    /// [`umber_core::smart::SMART_*_EXT`] constants).
    /// `Env` has none — it is gated by [`env_format_for_path`] instead.
    fn listed_extension(self) -> Option<&'static str> {
        match self {
            Self::SmartMaterial => Some(umber_core::smart::SMART_MATERIAL_EXT),
            Self::SmartMask => Some(umber_core::smart::SMART_MASK_EXT),
            Self::SmartFilter => Some(umber_core::smart::SMART_FILTER_EXT),
            Self::Brush => Some("umberbrush"),
            Self::Env => None,
        }
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Serializes as the CLI spelling ([`AssetKind::name`]) — the field
/// [`AbrImportOutcome`] prints.
impl serde::Serialize for AssetKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// Whether `path` has the `.abr` extension (case-insensitive — Photoshop
/// libraries ship as both `.abr` and `.ABR`).
pub fn is_abr_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("abr"))
}

/// The per-user library dir for `kind`, or `None` when no home is
/// discoverable (the CLI then refuses typed — never a temp-dir surprise).
///
/// Mirrors the GUI: brushes live in `preset_library::user_preset_dir()`;
/// smart assets and env maps live in the state dir (the parent of the
/// user preset dir — `gpu_startup::state_dir`'s rule) under `smart/` /
/// `env/`.
pub fn default_library_dir(kind: AssetKind) -> Option<PathBuf> {
    let user = umber_brush::preset_library::user_preset_dir()?;
    match kind {
        AssetKind::Brush => Some(user),
        AssetKind::SmartMaterial | AssetKind::SmartMask | AssetKind::SmartFilter => {
            Some(user.parent()?.join("smart"))
        }
        AssetKind::Env => Some(user.parent()?.join("env")),
    }
}

/// The environment format gate — the exact rule of the GUI's
/// `env::format_for_path` (umber-app/src/env.rs): the extension,
/// case-insensitive, `.png` -> LDR RGBA8, `.exr` -> HDR float; anything
/// else is not an environment map. Re-implemented here over
/// `umber_gpu::EnvFormat` because umber-cli must not depend on the egui
/// app crate; the match arms are kept identical to env.rs.
fn env_format_for_path(path: &Path) -> Option<umber_gpu::EnvFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some(umber_gpu::EnvFormat::Rgba8Unorm),
        "exr" => Some(umber_gpu::EnvFormat::Rgba32Float),
        _ => None,
    }
}

/// A typed refusal — [`import_asset`] never copies an invalid file.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The source path is not an existing file.
    #[error("source is not a file: {}", .0.display())]
    SourceNotFound(PathBuf),
    /// The extension is not one the GUI's listers / format gate accept
    /// for this kind (`expected` is `.umbersmart`-style, or `.png|.exr`
    /// for env).
    #[error("{kind}: {} has extension {found}, expected {expected}", .path.display())]
    UnsupportedExtension {
        kind: AssetKind,
        path: PathBuf,
        found: String,
        expected: String,
    },
    /// The GUI's own loader rejected the content.
    #[error("{kind}: {} failed validation: {detail}", .path.display())]
    Invalid {
        kind: AssetKind,
        path: PathBuf,
        detail: String,
    },
    /// No `--library` given and the per-user library dir is not
    /// discoverable (no HOME/XDG_DATA_HOME).
    #[error("{kind}: no --library given and the per-user library dir is not discoverable")]
    LibraryDirUnavailable { kind: AssetKind },
    /// The resolved target already exists — imports never overwrite.
    #[error("target already exists: {}", .0.display())]
    TargetExists(PathBuf),
    /// The source path has no file name to name the target after.
    #[error("source path has no file name: {}", .0.display())]
    NoFileName(PathBuf),
    /// Filesystem failure (create dir / copy).
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

/// What [`import_asset`] did — or, on `--dry-run`, would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    pub kind: AssetKind,
    pub source: PathBuf,
    /// The resolved copy destination.
    pub target: PathBuf,
    pub dry_run: bool,
    /// Bytes copied (bytes-untouched); 0 on a dry run.
    pub bytes: u64,
}

/// Runs the GUI's own validation for `kind` on `file`: the extension
/// gate first (the listers never read other extensions), then the
/// content loader. `Ok(())` means the assets panel would list the file.
fn validate(file: &Path, kind: AssetKind) -> Result<(), ImportError> {
    // Row 284: a brush `.abr` is a library — parse-level gate here, the
    // fan-out ([`import_abr_asset`]) does the mapping. Branching on the
    // actual extension keeps `listed_extension` (the lister's filter)
    // untouched.
    if kind == AssetKind::Brush && is_abr_path(file) {
        return validate_abr(file).map_err(|detail| ImportError::Invalid {
            kind,
            path: file.to_path_buf(),
            detail,
        });
    }
    let found = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("<none>")
        .to_owned();
    match kind.listed_extension() {
        Some(want) => {
            if !file.extension().is_some_and(|e| e == want) {
                return Err(ImportError::UnsupportedExtension {
                    kind,
                    path: file.to_path_buf(),
                    found,
                    expected: format!(".{want}"),
                });
            }
        }
        None => {
            if env_format_for_path(file).is_none() {
                return Err(ImportError::UnsupportedExtension {
                    kind,
                    path: file.to_path_buf(),
                    found,
                    expected: ".png|.exr".to_owned(),
                });
            }
        }
    }
    let invalid = |detail: String| ImportError::Invalid {
        kind,
        path: file.to_path_buf(),
        detail,
    };
    match kind {
        AssetKind::SmartMaterial => umber_core::smart::SmartMaterial::load_file(file)
            .map(|_| ())
            .map_err(|e| invalid(e.to_string())),
        AssetKind::SmartMask => umber_core::smart::SmartMask::load_file(file)
            .map(|_| ())
            .map_err(|e| invalid(e.to_string())),
        AssetKind::SmartFilter => umber_core::smart::SmartFilter::load_file(file)
            .map(|_| ())
            .map_err(|e| invalid(e.to_string())),
        AssetKind::Brush => validate_brush(file).map_err(invalid),
        // The format gate above is the whole env gate: pixel decode
        // happens at GPU load time in the GUI, not in the CLI.
        AssetKind::Env => Ok(()),
    }
}

/// The `preset_library::load_one_file` pipeline over the public API:
/// bytes -> UTF-8 -> `BrushPreset::from_json` -> `validate()`.
fn validate_brush(file: &Path) -> Result<(), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("cannot read: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("not valid UTF-8: {e}"))?;
    let preset = umber_brush::preset::BrushPreset::from_json(&text).map_err(|e| e.to_string())?;
    preset.validate().map_err(|e| e.to_string())
}

/// The `.abr` parse-level gate: `umber_brush::abr::read_abr` succeeds.
fn validate_abr(file: &Path) -> Result<(), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("cannot read: {e}"))?;
    umber_brush::abr::read_abr(&bytes)
        .map(|_| ())
        .map_err(|e| format!("not a readable .abr: {e}"))
}

/// Validates `file` with the GUI's own loaders for `kind`, resolves the
/// target (`library`, or the kind's per-user library dir), and — unless
/// `dry_run` — copies the bytes untouched into place.
///
/// # Errors
///
/// A typed [`ImportError`] on every refusal: missing source, wrong
/// extension, loader rejection, undiscoverable library dir, existing
/// target, IO failure. An invalid file is never copied.
pub fn import_asset(
    file: &Path,
    kind: AssetKind,
    library: Option<&Path>,
    dry_run: bool,
) -> Result<ImportOutcome, ImportError> {
    if !file.is_file() {
        return Err(ImportError::SourceNotFound(file.to_path_buf()));
    }
    validate(file, kind)?;
    if kind == AssetKind::Brush && is_abr_path(file) {
        // Row 284: an `.abr` fans out; summarize for this single-file
        // signature (target = the library dir, bytes = total written).
        let multi = import_abr_asset(file, library, dry_run)?;
        let dir = resolve_library(AssetKind::Brush, library)?;
        let mut bytes = 0u64;
        if !multi.dry_run {
            for target in &multi.targets {
                bytes += std::fs::metadata(target)
                    .map_err(|e| ImportError::Io {
                        context: format!("reading back {}", target.display()),
                        source: e,
                    })?
                    .len();
            }
        }
        return Ok(ImportOutcome {
            kind,
            source: multi.source,
            target: dir,
            dry_run: multi.dry_run,
            bytes,
        });
    }
    let dir = resolve_library(kind, library)?;
    let name = file
        .file_name()
        .ok_or_else(|| ImportError::NoFileName(file.to_path_buf()))?;
    let target = dir.join(name);
    if target.exists() {
        return Err(ImportError::TargetExists(target.clone()));
    }
    let source = file.to_path_buf();
    if dry_run {
        return Ok(ImportOutcome {
            kind,
            source,
            target,
            dry_run: true,
            bytes: 0,
        });
    }
    std::fs::create_dir_all(&dir).map_err(|e| ImportError::Io {
        context: format!("creating library dir {}", dir.display()),
        source: e,
    })?;
    let bytes = std::fs::copy(file, &target).map_err(|e| ImportError::Io {
        context: format!("copying to {}", target.display()),
        source: e,
    })?;
    Ok(ImportOutcome {
        kind,
        source,
        target,
        dry_run: false,
        bytes,
    })
}

/// The library dir for `kind`: the explicit `--library`, else the kind's
/// per-user dir ([`default_library_dir`]).
fn resolve_library(kind: AssetKind, library: Option<&Path>) -> Result<PathBuf, ImportError> {
    match library {
        Some(dir) => Ok(dir.to_path_buf()),
        None => default_library_dir(kind).ok_or(ImportError::LibraryDirUnavailable { kind }),
    }
}

/// What [`import_abr_asset`] did — or, on `--dry-run`, would do (row 284).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AbrImportOutcome {
    /// Always [`AssetKind::Brush`].
    pub kind: AssetKind,
    pub source: PathBuf,
    /// Every file written (or planned), in entry order: each
    /// `<slug>.umberbrush`, followed by its `<slug>.stamp.png` when
    /// stamped.
    pub targets: Vec<PathBuf>,
    pub dry_run: bool,
    /// `.umberbrush` files written (planned, on a dry run).
    pub presets: u32,
    /// `.stamp.png` sidecars written (planned, on a dry run).
    pub stamps_written: u32,
    /// Brushes dropped because an earlier brush in the file has the
    /// byte-identical stamp (`AbrImport::duplicate_stamps_skipped`).
    pub duplicate_stamps_skipped: u32,
    /// `samp` tips no brush references (`AbrImport::orphan_samples`).
    pub orphan_samples: u32,
    /// Brushes emitted radial — computed, or tip missing
    /// (`AbrImport::brushes_without_sample`).
    pub brushes_without_sample: u32,
}

/// The slug length cap, in chars (ASCII, so also bytes).
pub const ABR_SLUG_MAX: usize = 48;

/// Windows reserved device names — a file stem equal to one of these is
/// unopenable there, so the slug gets `-brush` appended.
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The file slug for a preset `name` (rules in the module docs):
/// `"My Brush!!"` -> `my-brush`; an empty result -> `abr-brush-{index}`.
fn abr_slug(name: &str, index: usize) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    // ASCII-only, so a byte truncation is a char truncation.
    slug.truncate(ABR_SLUG_MAX);
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        return format!("abr-brush-{index}");
    }
    if WINDOWS_RESERVED.contains(&slug.as_str()) {
        slug.push_str("-brush");
    }
    slug
}

/// `base`, or the first free `base-2`, `base-3`, … not yet in `taken`;
/// the returned slug is recorded in `taken`.
fn unique_slug(base: String, taken: &mut HashSet<String>) -> String {
    if taken.insert(base.clone()) {
        return base;
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}-{n}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// One planned preset file and its optional stamp sidecar.
struct AbrPlanItem {
    preset_path: PathBuf,
    json: String,
    stamp: Option<(PathBuf, u32, u32, Vec<u8>)>,
}

/// Row 284: fans an `.abr` brush library out into the brush library dir
/// (`library`, or the user preset dir) — one `<slug>.umberbrush` per kept
/// brush plus a `<slug>.stamp.png` per stamped one (module docs: the
/// slug rules, the all-or-nothing refusal, the dry-run counts).
///
/// # Errors
///
/// [`ImportError::SourceNotFound`]; [`ImportError::UnsupportedExtension`]
/// for a non-`.abr` file; [`ImportError::Invalid`] when the file does
/// not parse (or a mapped preset fails to serialize / validate);
/// [`ImportError::LibraryDirUnavailable`]; [`ImportError::TargetExists`]
/// when ANY planned target exists (nothing is written);
/// [`ImportError::Io`] on a write failure.
pub fn import_abr_asset(
    file: &Path,
    library: Option<&Path>,
    dry_run: bool,
) -> Result<AbrImportOutcome, ImportError> {
    let kind = AssetKind::Brush;
    if !file.is_file() {
        return Err(ImportError::SourceNotFound(file.to_path_buf()));
    }
    if !is_abr_path(file) {
        return Err(ImportError::UnsupportedExtension {
            kind,
            path: file.to_path_buf(),
            found: file
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("<none>")
                .to_owned(),
            expected: ".abr".to_owned(),
        });
    }
    let invalid = |detail: String| ImportError::Invalid {
        kind,
        path: file.to_path_buf(),
        detail,
    };
    let bytes = std::fs::read(file).map_err(|e| ImportError::Io {
        context: format!("reading {}", file.display()),
        source: e,
    })?;
    let imported = umber_brush::abr_import::import_abr(&bytes)
        .map_err(|e| invalid(format!("not a readable .abr: {e}")))?;
    let dir = resolve_library(kind, library)?;

    // Plan EVERY target before touching the filesystem.
    let mut taken = HashSet::new();
    let mut plan = Vec::with_capacity(imported.entries.len());
    for entry in imported.entries {
        let index = entry.brush_index;
        entry
            .preset
            .validate()
            .map_err(|e| invalid(format!("brush {index} maps to an invalid preset: {e}")))?;
        let json = entry
            .preset
            .to_json()
            .map_err(|e| invalid(format!("brush {index} does not serialize: {e}")))?;
        let slug = unique_slug(abr_slug(&entry.preset.name, entry.brush_index), &mut taken);
        let stamp = entry
            .stamp
            .map(|(w, h, rgba)| (dir.join(format!("{slug}.stamp.png")), w, h, rgba));
        plan.push(AbrPlanItem {
            preset_path: dir.join(format!("{slug}.umberbrush")),
            json,
            stamp,
        });
    }
    let mut targets = Vec::new();
    for item in &plan {
        targets.push(item.preset_path.clone());
        if let Some((path, ..)) = &item.stamp {
            targets.push(path.clone());
        }
    }
    if let Some(existing) = targets.iter().find(|t| t.exists()) {
        return Err(ImportError::TargetExists(existing.clone()));
    }

    let presets = plan.len() as u32;
    let stamps_written = plan.iter().filter(|i| i.stamp.is_some()).count() as u32;
    let outcome = AbrImportOutcome {
        kind,
        source: file.to_path_buf(),
        targets,
        dry_run,
        presets,
        stamps_written,
        duplicate_stamps_skipped: imported.duplicate_stamps_skipped,
        orphan_samples: imported.orphan_samples,
        brushes_without_sample: imported.brushes_without_sample,
    };
    if dry_run {
        return Ok(outcome);
    }

    std::fs::create_dir_all(&dir).map_err(|e| ImportError::Io {
        context: format!("creating library dir {}", dir.display()),
        source: e,
    })?;
    for item in &plan {
        std::fs::write(&item.preset_path, &item.json).map_err(|e| ImportError::Io {
            context: format!("writing {}", item.preset_path.display()),
            source: e,
        })?;
        if let Some((path, w, h, rgba)) = &item.stamp {
            umber_export::png::write_png(path, *w, *h, rgba, umber_export::png::Transfer::Linear)
                .map_err(|e| ImportError::Io {
                context: format!("writing {}", path.display()),
                source: std::io::Error::other(e.to_string()),
            })?;
        }
    }
    Ok(outcome)
}

/// A synthetic v6 minor-1 `.abr` (row 284's fixture, the byte layout of
/// umber-brush's abr tests): one raw 4x4 `samp` tip `tip1`
/// (`abr_fixture::TIP_ALPHA`) shared by two sampled brushes (`"My
/// Brush!!"`, `"Second"`) plus one computed brush (`"Round"`). Maps to 2
/// presets (`my-brush`, `round`), 1 stamp, 1 duplicate, 0 orphans, 1
/// brush without a sample. Shared with agent.rs's step test.
#[cfg(test)]
pub(crate) mod abr_fixture {
    /// The tip's alpha plane, row-major, top row first.
    pub(crate) const TIP_ALPHA: [u8; 16] = [
        0, 17, 34, 51, 68, 85, 102, 119, 136, 153, 170, 187, 204, 221, 238, 255,
    ];

    struct Enc(Vec<u8>);

    impl Enc {
        fn raw(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
        fn u8(&mut self, v: u8) {
            self.0.push(v);
        }
        fn u16(&mut self, v: u16) {
            self.raw(&v.to_be_bytes());
        }
        fn i16(&mut self, v: i16) {
            self.raw(&v.to_be_bytes());
        }
        fn u32(&mut self, v: u32) {
            self.raw(&v.to_be_bytes());
        }
        fn i32(&mut self, v: i32) {
            self.raw(&v.to_be_bytes());
        }
        fn f64(&mut self, v: f64) {
            self.raw(&v.to_be_bytes());
        }
        /// `u32` UTF-16 unit count + big-endian units.
        fn unicode(&mut self, text: &str) {
            let units: Vec<u16> = text.encode_utf16().collect();
            self.u32(units.len() as u32);
            for unit in units {
                self.u16(unit);
            }
        }
        /// A four-char id as `0` + bytes, else `i32` length + bytes.
        fn id(&mut self, id: &str) {
            let len = if id.len() == 4 { 0 } else { id.len() as i32 };
            self.i32(len);
            self.raw(id.as_bytes());
        }
        /// Class structure: empty name, class id, item count.
        fn obj_header(&mut self, class_id: &str, count: u32) {
            self.unicode("");
            self.id(class_id);
            self.u32(count);
        }
        fn text(&mut self, key: &str, value: &str) {
            self.id(key);
            self.raw(b"TEXT");
            self.unicode(value);
        }
        fn units(&mut self, key: &str, code: &[u8; 4], value: f64) {
            self.id(key);
            self.raw(b"UntF");
            self.raw(code);
            self.f64(value);
        }
        fn boolean(&mut self, key: &str, value: bool) {
            self.id(key);
            self.raw(b"bool");
            self.u8(u8::from(value));
        }
        /// One `Brsh` list entry: an `Objc` of class `_` whose `Brsh`
        /// item is the tip-shape descriptor (name inside the shape).
        fn brush(&mut self, name: &str, tip: Option<&str>) {
            self.raw(b"Objc");
            self.obj_header("_", 1);
            self.id("Brsh");
            self.raw(b"Objc");
            let class = if tip.is_some() {
                "sampledBrush"
            } else {
                "computedBrush"
            };
            self.obj_header(class, 9 + u32::from(tip.is_some()));
            self.text("Nm  ", name);
            self.units("Dmtr", b"#Pxl", 32.0);
            self.units("Angl", b"#Ang", 15.0);
            self.units("Rndn", b"#Prc", 100.0);
            self.boolean("Intr", true);
            self.units("Spcn", b"#Prc", 25.0);
            self.units("Hrdn", b"#Prc", 80.0);
            self.boolean("flipX", false);
            self.boolean("flipY", false);
            if let Some(tip) = tip {
                self.text("sampledData", tip);
            }
        }
        /// `8BIM` + kind + size + payload + 4-byte pad.
        fn section(&mut self, kind: &[u8; 4], payload: &[u8]) {
            self.raw(b"8BIM");
            self.raw(kind);
            self.u32(payload.len() as u32);
            self.raw(payload);
            for _ in 0..(4 - payload.len() % 4) % 4 {
                self.u8(0);
            }
        }
    }

    /// The fixture bytes (see the module doc).
    pub(crate) fn bytes() -> Vec<u8> {
        // samp: one minor-1 record — length prefix, pascal name, the
        // 10-byte parameter block, bounds, depth 8, raw, alpha.
        let mut body = Enc(Vec::new());
        body.u8(4);
        body.raw(b"tip1");
        for _ in 0..5 {
            body.u16(0);
        }
        for v in [0i32, 0, 4, 4] {
            body.i32(v); // y, x, bottom (height), right (width)
        }
        body.i16(8);
        body.u8(0);
        body.raw(&TIP_ALPHA);
        let declared = body.0.len() as u32;
        while !body.0.len().is_multiple_of(4) {
            body.u8(0);
        }
        let mut samp = Enc(Vec::new());
        samp.u32(declared);
        samp.raw(&body.0);

        // desc: versioned (16) root `null` descriptor with a `Brsh` list.
        let mut desc = Enc(Vec::new());
        desc.u32(16);
        desc.obj_header("null", 1);
        desc.id("Brsh");
        desc.raw(b"VlLs");
        desc.i32(3);
        desc.brush("My Brush!!", Some("tip1"));
        desc.brush("Second", Some("tip1"));
        desc.brush("Round", None);

        let mut file = Enc(Vec::new());
        file.i16(6);
        file.i16(1);
        file.section(b"samp", &samp.0);
        file.section(b"desc", &desc.0);
        file.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh per-test scratch dir under the system temp dir.
    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "umber-cli-import-asset-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A committed repo asset as a valid fixture (the same trick as
    /// umber-brush's shipped-library test).
    fn repo_asset(rel: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(rel)
    }

    const MATERIAL: &str = "assets/smart/rusted-iron.umbersmart";
    const MASK: &str = "assets/smart/ao-cavity.umbermask";
    const BRUSH: &str = "assets/brushes/hard-round.umberbrush";

    /// A minimal VALID `.umberfilter` document (3b1f4ae's schema: name +
    /// an ops list; posterize levels clamp into 2..=256). No starter
    /// filter ships yet, so tests write their own fixture.
    const FILTER_JSON: &str =
        r#"{"name":"Test Filter","version":1,"ops":[{"type":"posterize","levels":3}]}"#;

    /// Writes `FILTER_JSON` to `<dir>/<file_name>` and returns the path.
    fn filter_fixture(dir: &Path, file_name: &str) -> PathBuf {
        let path = dir.join(file_name);
        std::fs::write(&path, FILTER_JSON).unwrap();
        path
    }

    fn assert_bytes_identical(a: &Path, b: &Path) {
        assert_eq!(
            std::fs::read(a).unwrap(),
            std::fs::read(b).unwrap(),
            "imported bytes must be identical to the source"
        );
    }

    #[test]
    fn valid_smart_material_imports_bytes_untouched() {
        let lib = tmp_dir("material");
        let src = repo_asset(MATERIAL);
        let out = import_asset(&src, AssetKind::SmartMaterial, Some(&lib), false).unwrap();
        assert_eq!(out.kind, AssetKind::SmartMaterial);
        assert!(!out.dry_run);
        assert_eq!(out.target, lib.join("rusted-iron.umbersmart"));
        assert_eq!(out.bytes, std::fs::metadata(&src).unwrap().len());
        assert_bytes_identical(&src, &out.target);
    }

    #[test]
    fn valid_smart_mask_imports_bytes_untouched() {
        let lib = tmp_dir("mask");
        let src = repo_asset(MASK);
        let out = import_asset(&src, AssetKind::SmartMask, Some(&lib), false).unwrap();
        assert_eq!(out.target, lib.join("ao-cavity.umbermask"));
        assert_bytes_identical(&src, &out.target);
    }

    #[test]
    fn valid_brush_imports_bytes_untouched() {
        let lib = tmp_dir("brush");
        let src = repo_asset(BRUSH);
        let out = import_asset(&src, AssetKind::Brush, Some(&lib), false).unwrap();
        assert_eq!(out.target, lib.join("hard-round.umberbrush"));
        assert_bytes_identical(&src, &out.target);
    }

    #[test]
    fn valid_smart_filter_imports_bytes_untouched() {
        let src_dir = tmp_dir("filter-src");
        let lib = tmp_dir("filter-lib");
        let src = filter_fixture(&src_dir, "crunch.umberfilter");
        let out = import_asset(&src, AssetKind::SmartFilter, Some(&lib), false).unwrap();
        assert_eq!(out.kind, AssetKind::SmartFilter);
        assert_eq!(out.target, lib.join("crunch.umberfilter"));
        assert_eq!(out.bytes, std::fs::metadata(&src).unwrap().len());
        assert_bytes_identical(&src, &out.target);
        // And it really parses through the core loader.
        let loaded = umber_core::smart::SmartFilter::load_file(&out.target).unwrap();
        assert_eq!(loaded.name, "Test Filter");
    }

    #[test]
    fn invalid_smart_filter_is_a_typed_refusal() {
        let dir = tmp_dir("bad-filter");
        let lib = tmp_dir("bad-filter-lib");
        let bad = dir.join("bad.umberfilter");
        std::fs::write(&bad, b"{ not json").unwrap();
        let err = import_asset(&bad, AssetKind::SmartFilter, Some(&lib), false).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Invalid {
                    kind: AssetKind::SmartFilter,
                    ..
                }
            ),
            "got {err:?}"
        );
        assert!(!lib.join("bad.umberfilter").exists(), "never a silent copy");
        // A valid-JSON but schema-invalid filter (unknown op) is refused too.
        let unknown = dir.join("unknown-op.umberfilter");
        std::fs::write(&unknown, r#"{"name":"X","ops":[{"type":"magic"}]}"#).unwrap();
        assert!(matches!(
            import_asset(&unknown, AssetKind::SmartFilter, Some(&lib), false),
            Err(ImportError::Invalid {
                kind: AssetKind::SmartFilter,
                ..
            })
        ));
        assert!(!lib.join("unknown-op.umberfilter").exists());
    }

    #[test]
    fn smart_filter_with_the_wrong_extension_is_refused_before_content() {
        let src_dir = tmp_dir("filter-wrong-ext");
        let lib = tmp_dir("filter-wrong-ext-lib");
        // A VALID .umberfilter offered as smart-material: the listers'
        // extension gate rejects it before the loader runs.
        let src = filter_fixture(&src_dir, "crunch.umberfilter");
        let err = import_asset(&src, AssetKind::SmartMaterial, Some(&lib), false).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::UnsupportedExtension {
                    kind: AssetKind::SmartMaterial,
                    ..
                }
            ),
            "got {err:?}"
        );
        assert!(!lib.join("crunch.umberfilter").exists());
    }

    #[test]
    fn valid_env_passes_the_format_gate_case_insensitively() {
        let dir = tmp_dir("env-src");
        let lib = tmp_dir("env-lib");
        // The gate is the extension (env.rs's format_for_path); content
        // decode happens at GPU load time, so bytes here are irrelevant.
        let png = dir.join("studio.PNG");
        std::fs::write(&png, b"placeholder").unwrap();
        let out = import_asset(&png, AssetKind::Env, Some(&lib), false).unwrap();
        assert_eq!(out.target, lib.join("studio.PNG"));
        assert_bytes_identical(&png, &out.target);
        let exr = dir.join("probe.exr");
        std::fs::write(&exr, b"").unwrap();
        assert!(import_asset(&exr, AssetKind::Env, Some(&lib), false).is_ok());
    }

    #[test]
    fn dry_run_resolves_target_and_copies_nothing() {
        let lib = tmp_dir("dry");
        let src = repo_asset(MATERIAL);
        let out = import_asset(&src, AssetKind::SmartMaterial, Some(&lib), true).unwrap();
        assert!(out.dry_run);
        assert_eq!(out.bytes, 0);
        assert_eq!(out.target, lib.join("rusted-iron.umbersmart"));
        assert!(!out.target.exists(), "dry run must not copy");
    }

    #[test]
    fn invalid_smart_material_is_a_typed_refusal() {
        let dir = tmp_dir("bad-material");
        let lib = tmp_dir("bad-material-lib");
        let bad = dir.join("bad.umbersmart");
        std::fs::write(&bad, b"this is not json").unwrap();
        let err = import_asset(&bad, AssetKind::SmartMaterial, Some(&lib), false).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Invalid {
                    kind: AssetKind::SmartMaterial,
                    ..
                }
            ),
            "got {err:?}"
        );
        assert!(!lib.join("bad.umbersmart").exists(), "never a silent copy");
    }

    #[test]
    fn invalid_smart_mask_is_a_typed_refusal() {
        let dir = tmp_dir("bad-mask");
        let lib = tmp_dir("bad-mask-lib");
        let bad = dir.join("bad.umbermask");
        std::fs::write(&bad, b"{ not json").unwrap();
        let err = import_asset(&bad, AssetKind::SmartMask, Some(&lib), false).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Invalid {
                    kind: AssetKind::SmartMask,
                    ..
                }
            ),
            "got {err:?}"
        );
        assert!(!lib.join("bad.umbermask").exists());
    }

    #[test]
    fn invalid_brush_is_a_typed_refusal() {
        let dir = tmp_dir("bad-brush");
        let lib = tmp_dir("bad-brush-lib");
        // Parses as JSON but fails `validate()`: empty preset name.
        let empty_name = dir.join("empty-name.umberbrush");
        let text = std::fs::read_to_string(repo_asset(BRUSH)).unwrap();
        std::fs::write(
            &empty_name,
            text.replace("\"name\": \"Hard Round\"", "\"name\": \"\""),
        )
        .unwrap();
        let err = import_asset(&empty_name, AssetKind::Brush, Some(&lib), false).unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::Invalid {
                    kind: AssetKind::Brush,
                    ..
                }
            ),
            "got {err:?}"
        );
        // Not even UTF-8.
        let raw = dir.join("raw.umberbrush");
        std::fs::write(&raw, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(
            import_asset(&raw, AssetKind::Brush, Some(&lib), false),
            Err(ImportError::Invalid {
                kind: AssetKind::Brush,
                ..
            })
        ));
        assert!(!lib.join("empty-name.umberbrush").exists());
        assert!(!lib.join("raw.umberbrush").exists());
    }

    #[test]
    fn invalid_env_extension_is_a_typed_refusal() {
        let dir = tmp_dir("bad-env");
        let lib = tmp_dir("bad-env-lib");
        let jpg = dir.join("albedo.jpg");
        std::fs::write(&jpg, b"jpeg bytes").unwrap();
        let err = import_asset(&jpg, AssetKind::Env, Some(&lib), false).unwrap_err();
        match err {
            ImportError::UnsupportedExtension {
                kind,
                found,
                expected,
                ..
            } => {
                assert_eq!(kind, AssetKind::Env);
                assert_eq!(found, "jpg");
                assert_eq!(expected, ".png|.exr");
            }
            other => panic!("expected UnsupportedExtension, got {other:?}"),
        }
        let noext = dir.join("noext");
        std::fs::write(&noext, b"x").unwrap();
        assert!(matches!(
            import_asset(&noext, AssetKind::Env, Some(&lib), false),
            Err(ImportError::UnsupportedExtension { .. })
        ));
        assert!(!lib.join("albedo.jpg").exists());
        assert!(!lib.join("noext").exists());
    }

    #[test]
    fn wrong_extension_for_the_kind_is_refused_before_content() {
        let lib = tmp_dir("wrong-ext");
        // A VALID .umbersmart offered as smart-mask: the listers' filter
        // rejects it before the loader ever runs.
        let err = import_asset(
            &repo_asset(MATERIAL),
            AssetKind::SmartMask,
            Some(&lib),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                ImportError::UnsupportedExtension {
                    kind: AssetKind::SmartMask,
                    ..
                }
            ),
            "got {err:?}"
        );
        assert!(!lib.join("rusted-iron.umbersmart").exists());
    }

    #[test]
    fn existing_target_is_refused_and_bytes_kept() {
        let lib = tmp_dir("exists");
        let src = repo_asset(BRUSH);
        import_asset(&src, AssetKind::Brush, Some(&lib), false).unwrap();
        std::fs::write(lib.join("hard-round.umberbrush"), b"tampered").unwrap();
        let err = import_asset(&src, AssetKind::Brush, Some(&lib), false).unwrap_err();
        assert!(matches!(err, ImportError::TargetExists(_)), "got {err:?}");
        assert_eq!(
            std::fs::read(lib.join("hard-round.umberbrush")).unwrap(),
            b"tampered",
            "an existing target is never overwritten"
        );
    }

    #[test]
    fn missing_source_is_a_typed_refusal() {
        let lib = tmp_dir("missing");
        let err = import_asset(
            Path::new("/nonexistent/nope.umbersmart"),
            AssetKind::SmartMaterial,
            Some(&lib),
            false,
        )
        .unwrap_err();
        assert!(matches!(err, ImportError::SourceNotFound(_)), "got {err:?}");
    }

    #[test]
    fn default_library_dirs_follow_the_gui_roots() {
        let Some(user) = umber_brush::preset_library::user_preset_dir() else {
            return; // no home in this environment — nothing to assert against
        };
        assert_eq!(default_library_dir(AssetKind::Brush), Some(user.clone()));
        let state = user.parent().unwrap().to_path_buf();
        assert_eq!(
            default_library_dir(AssetKind::SmartMaterial),
            Some(state.join("smart"))
        );
        assert_eq!(
            default_library_dir(AssetKind::SmartMask),
            Some(state.join("smart"))
        );
        assert_eq!(
            default_library_dir(AssetKind::SmartFilter),
            Some(state.join("smart")),
            "smart filters share the state-dir smart/ library the listers scan"
        );
        assert_eq!(default_library_dir(AssetKind::Env), Some(state.join("env")));
    }

    #[test]
    fn kind_names_round_trip() {
        for name in [
            "smart-material",
            "smart-mask",
            "smart-filter",
            "brush",
            "env",
        ] {
            let kind = AssetKind::from_name(name).unwrap();
            assert_eq!(kind.name(), name);
            assert_eq!(kind.to_string(), name);
        }
        assert!(AssetKind::from_name("smart-thing").is_none());
    }

    // --- row 284: the `.abr` fan-out -------------------------------------

    /// Writes the synthetic `.abr` fixture to `<dir>/<file_name>`.
    fn abr_file(dir: &Path, file_name: &str) -> PathBuf {
        let path = dir.join(file_name);
        std::fs::write(&path, abr_fixture::bytes()).unwrap();
        path
    }

    fn dir_entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn load_preset(path: &Path) -> umber_brush::preset::BrushPreset {
        let text = std::fs::read_to_string(path).unwrap();
        let preset = umber_brush::preset::BrushPreset::from_json(&text).unwrap();
        preset.validate().unwrap();
        preset
    }

    #[test]
    fn abr_fans_out_presets_and_one_deduped_stamp() {
        let src_dir = tmp_dir("abr-src");
        let lib = tmp_dir("abr-lib");
        let src = abr_file(&src_dir, "set.abr");
        let out = import_abr_asset(&src, Some(&lib), false).unwrap();
        assert_eq!(out.kind, AssetKind::Brush);
        assert!(!out.dry_run);
        assert_eq!(
            out.targets,
            vec![
                lib.join("my-brush.umberbrush"),
                lib.join("my-brush.stamp.png"),
                lib.join("round.umberbrush"),
            ]
        );
        assert_eq!(
            dir_entries(&lib),
            vec![
                "my-brush.stamp.png",
                "my-brush.umberbrush",
                "round.umberbrush"
            ],
            "2 presets + 1 stamp: the second brush's shared tip is deduped"
        );

        // Every written preset loads through from_json + validate().
        let stamped = load_preset(&lib.join("my-brush.umberbrush"));
        assert_eq!(stamped.name, "My Brush!!");
        assert!(stamped.params.stamped);
        // 32 px diameter -> 16 px radius; 25% spacing -> 2 dabs/radius.
        assert_eq!(stamped.lazy_mouse.radius_px, 16.0);
        assert_eq!(stamped.spacing.dabs_per_radius, 2.0);
        let radial = load_preset(&lib.join("round.umberbrush"));
        assert_eq!(radial.name, "Round");
        assert!(!radial.params.stamped);

        // The sidecar reads back 4x4 with the tip's alpha byte-for-byte
        // (read_png_rgba8 returns GPU row order — last stored row first —
        // so the expected plane is the fixture's rows reversed).
        let (w, h, rgba) =
            umber_export::png::read_png_rgba8(&lib.join("my-brush.stamp.png")).unwrap();
        assert_eq!((w, h), (4, 4));
        let mut expected = Vec::new();
        for row in (0..4).rev() {
            for col in 0..4 {
                let alpha = abr_fixture::TIP_ALPHA[row * 4 + col];
                expected.extend_from_slice(&[255, 255, 255, alpha]);
            }
        }
        assert_eq!(rgba, expected);
    }

    #[test]
    fn abr_outcome_counts_match_the_fixture() {
        let src_dir = tmp_dir("abr-counts-src");
        let lib = tmp_dir("abr-counts-lib");
        let src = abr_file(&src_dir, "set.abr");
        let out = import_abr_asset(&src, Some(&lib), false).unwrap();
        assert_eq!(out.presets, 2);
        assert_eq!(out.stamps_written, 1);
        assert_eq!(out.duplicate_stamps_skipped, 1);
        assert_eq!(out.orphan_samples, 0);
        assert_eq!(out.brushes_without_sample, 1);
        // And it serializes to the one-line JSON the CLI prints.
        let json = serde_json::to_value(&out).unwrap();
        assert_eq!(json["kind"], "brush");
        assert_eq!(json["targets"].as_array().unwrap().len(), 3);
        assert_eq!(json["presets"], 2);
    }

    #[test]
    fn abr_existing_target_refuses_the_whole_import() {
        let src_dir = tmp_dir("abr-exists-src");
        let lib = tmp_dir("abr-exists-lib");
        let src = abr_file(&src_dir, "set.abr");
        // Pre-create the LAST planned target: the refusal must still
        // fire before the first one is written.
        std::fs::write(lib.join("round.umberbrush"), b"mine").unwrap();
        let err = import_abr_asset(&src, Some(&lib), false).unwrap_err();
        match err {
            ImportError::TargetExists(path) => assert_eq!(path, lib.join("round.umberbrush")),
            other => panic!("expected TargetExists, got {other:?}"),
        }
        assert_eq!(
            dir_entries(&lib),
            vec!["round.umberbrush"],
            "nothing written"
        );
        assert_eq!(
            std::fs::read(lib.join("round.umberbrush")).unwrap(),
            b"mine"
        );
    }

    #[test]
    fn abr_dry_run_lists_targets_and_writes_nothing() {
        let src_dir = tmp_dir("abr-dry-src");
        let lib = tmp_dir("abr-dry-lib");
        let src = abr_file(&src_dir, "set.abr");
        let out = import_abr_asset(&src, Some(&lib), true).unwrap();
        assert!(out.dry_run);
        assert_eq!(out.targets.len(), 3);
        assert_eq!(out.targets[0], lib.join("my-brush.umberbrush"));
        assert_eq!(out.presets, 2, "dry run reports the planned counts");
        assert_eq!(out.stamps_written, 1);
        assert!(dir_entries(&lib).is_empty(), "dry run must not write");
    }

    #[test]
    fn abr_garbage_is_a_typed_parse_refusal_not_a_copy() {
        let src_dir = tmp_dir("abr-bad-src");
        let lib = tmp_dir("abr-bad-lib");
        let bad = src_dir.join("bad.abr");
        std::fs::write(&bad, b"definitely not a brush library").unwrap();
        for err in [
            import_abr_asset(&bad, Some(&lib), false).unwrap_err(),
            import_asset(&bad, AssetKind::Brush, Some(&lib), false).unwrap_err(),
        ] {
            assert!(
                matches!(
                    err,
                    ImportError::Invalid {
                        kind: AssetKind::Brush,
                        ..
                    }
                ),
                "got {err:?}"
            );
        }
        assert!(dir_entries(&lib).is_empty(), "never a silent copy");
        // A non-.abr file is refused by the fan-out's extension gate.
        assert!(matches!(
            import_abr_asset(&repo_asset(BRUSH), Some(&lib), false),
            Err(ImportError::UnsupportedExtension { .. })
        ));
    }

    #[test]
    fn import_asset_on_an_abr_delegates_to_the_fan_out() {
        let src_dir = tmp_dir("abr-delegate-src");
        let lib = tmp_dir("abr-delegate-lib");
        // Upper-case extension: the .abr gate is case-insensitive.
        let src = abr_file(&src_dir, "SET.ABR");
        let out = import_asset(&src, AssetKind::Brush, Some(&lib), false).unwrap();
        assert_eq!(out.target, lib, "the summary target is the library dir");
        let total: u64 = dir_entries(&lib)
            .iter()
            .map(|n| std::fs::metadata(lib.join(n)).unwrap().len())
            .sum();
        assert_eq!(dir_entries(&lib).len(), 3);
        assert_eq!(out.bytes, total);
        assert!(!lib.join("SET.ABR").exists(), "an .abr is never copied raw");
    }

    #[test]
    fn abr_slug_rules() {
        assert_eq!(abr_slug("My Brush!!", 0), "my-brush");
        assert_eq!(abr_slug("", 0), "abr-brush-0");
        assert_eq!(abr_slug("!!!", 7), "abr-brush-7");
        assert_eq!(abr_slug("  Chalk -- Dry  ", 0), "chalk-dry");
        assert_eq!(abr_slug("Pinceau Sèche", 0), "pinceau-s-che");
        assert_eq!(abr_slug("CON", 0), "con-brush");
        let long = "a".repeat(60);
        assert_eq!(abr_slug(&long, 0), "a".repeat(ABR_SLUG_MAX));
        // A cut landing on a separator does not leave a trailing dash.
        let edge = format!("{}-b", "a".repeat(ABR_SLUG_MAX - 1));
        assert_eq!(abr_slug(&edge, 0), "a".repeat(ABR_SLUG_MAX - 1));

        let mut taken = HashSet::new();
        assert_eq!(unique_slug("my-brush".into(), &mut taken), "my-brush");
        assert_eq!(unique_slug("my-brush".into(), &mut taken), "my-brush-2");
        assert_eq!(unique_slug("my-brush".into(), &mut taken), "my-brush-3");
        assert_eq!(unique_slug("my-brush-2".into(), &mut taken), "my-brush-2-2");
    }
}
