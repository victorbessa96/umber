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
//! * `brush` — extension `umberbrush` (`preset_library`'s scan filter) +
//!   the per-file pipeline of `preset_library::load_one_file` (bytes ->
//!   UTF-8 -> `BrushPreset::from_json` -> `validate()`)
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

use std::fmt;
use std::path::{Path, PathBuf};

/// The four importable asset kinds — K5's `--kind` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    SmartMaterial,
    SmartMask,
    Brush,
    Env,
}

impl AssetKind {
    /// Parses the CLI `--kind` value.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "smart-material" => Some(Self::SmartMaterial),
            "smart-mask" => Some(Self::SmartMask),
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
            Self::Brush => "brush",
            Self::Env => "env",
        }
    }

    /// The extension the GUI's library listers scan for, case-sensitive
    /// (`document::list_smart_*`, `preset_library::sorted_preset_files`).
    /// `Env` has none — it is gated by [`env_format_for_path`] instead.
    fn listed_extension(self) -> Option<&'static str> {
        match self {
            Self::SmartMaterial => Some("umbersmart"),
            Self::SmartMask => Some("umbermask"),
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
        AssetKind::SmartMaterial | AssetKind::SmartMask => Some(user.parent()?.join("smart")),
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
    let dir = match library {
        Some(dir) => dir.to_path_buf(),
        None => default_library_dir(kind).ok_or(ImportError::LibraryDirUnavailable { kind })?,
    };
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
        assert_eq!(default_library_dir(AssetKind::Env), Some(state.join("env")));
    }

    #[test]
    fn kind_names_round_trip() {
        for name in ["smart-material", "smart-mask", "brush", "env"] {
            let kind = AssetKind::from_name(name).unwrap();
            assert_eq!(kind.name(), name);
            assert_eq!(kind.to_string(), name);
        }
        assert!(AssetKind::from_name("smart-thing").is_none());
    }
}
