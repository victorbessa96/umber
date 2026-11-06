//! Environment-map loading (Wave-4 item 6): File > Load Environment…
//! plus the committed studio default.
//!
//! GPU access follows the `viewport`/`paint_state`/`bakes_panel`
//! pattern: this module never names a `wgpu` type — the device and
//! queue travel inside `&GpuContext`, and image decoding goes through
//! `umber-export`'s read APIs (no new dependencies: PNG via the `png`
//! crate umber-export already uses, EXR via the workspace `exr` crate).

use std::path::{Path, PathBuf};

use umber_gpu::{EnvFormat, EnvIrradiance, GpuContext};

/// The committed neutral-studio default (see
/// `assets/gen/make_studio_env.py`).
pub const DEFAULT_ENV_ASSET: &str = "assets/env/studio-neutral.png";

/// Picks the pixel encoding from the file extension (case-insensitive).
/// Returns `None` for anything that isn't an LDR PNG or HDR EXR equirect.
pub fn format_for_path(path: &Path) -> Option<EnvFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some(EnvFormat::Rgba8Unorm),
        "exr" => Some(EnvFormat::Rgba32Float),
        _ => None,
    }
}

/// Loads an environment file (`.png` or `.exr`) and convolves it into an
/// [`EnvIrradiance`] on `gpu` — the File > Load Environment… driver.
/// Both readers return GPU row order (row 0 = -Y; see their doc
/// comments), which is what `EnvIrradiance::from_equirect` expects.
pub fn load_environment_file(gpu: &GpuContext, path: &Path) -> anyhow::Result<EnvIrradiance> {
    let format = format_for_path(path).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported environment format: {} (want .png or .exr)",
            path.display()
        )
    })?;
    let (width, height, bytes) = match format {
        EnvFormat::Rgba8Unorm => {
            let (w, h, rgba) = umber_export::png::read_png_rgba8(path)
                .map_err(|e| anyhow::anyhow!("reading {}: {e:#}", path.display()))?;
            (w, h, rgba)
        }
        EnvFormat::Rgba32Float => {
            let (w, h, floats) = umber_export::exr::read_exr_rgba_f32(path)
                .map_err(|e| anyhow::anyhow!("reading {}: {e:#}", path.display()))?;
            // Native-endian float bytes for `from_equirect`'s
            // reinterpretation (x86-64/aarch64 — umber's targets — are
            // little-endian). Manual loop, not bytemuck: umber-app
            // doesn't depend on it, and this keeps Cargo.toml untouched.
            let bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
            (w, h, bytes)
        }
    };
    EnvIrradiance::from_equirect(&gpu.device, &gpu.queue, &bytes, width, height, format)
        .map_err(|e| anyhow::anyhow!("convolving {}: {e:#}", path.display()))
}

/// Candidate locations for the studio default, in order: the
/// repository-relative path (correct when the binary runs with the repo
/// root as CWD — `cargo run`, the usual dev loop), then the
/// compile-time manifest path (correct when CWD is elsewhere but the
/// binary was built from this source tree).
pub fn default_env_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from(DEFAULT_ENV_ASSET),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/env/studio-neutral.png"),
    ]
}

/// Loads the committed studio default if present, else `None`
/// (procedural fallback — never a startup failure). Logs which path
/// won so a missing asset is diagnosable, not silent.
pub fn load_default_environment(gpu: &GpuContext) -> Option<EnvIrradiance> {
    for candidate in default_env_candidates() {
        if !candidate.is_file() {
            continue;
        }
        match load_environment_file(gpu, &candidate) {
            Ok(env) => {
                log::info!("default environment: {}", candidate.display());
                return Some(env);
            }
            Err(err) => {
                log::warn!(
                    "default environment {} failed: {err:#}",
                    candidate.display()
                );
                return None;
            }
        }
    }
    log::info!("no default environment found; procedural fallback");
    None
}

/// The env-map library search path: the bundled `assets/env`, then the
/// exe-dir `env/` (installed builds), then the persisted extra library
/// folders — the same first-wins order as [`crate::document::smart_dirs`].
pub fn env_dirs() -> Vec<PathBuf> {
    env_dirs_with(&crate::document::load_extra_dirs(
        crate::document::extra_dirs_path().as_deref(),
    ))
}

/// [`env_dirs`] with explicit extras (the testable core, mirroring
/// [`crate::document::smart_dirs_with`]).
pub fn env_dirs_with(extras: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/env")];
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join("env"));
    }
    for e in extras {
        if !dirs.contains(e) {
            dirs.push(e.clone());
        }
    }
    dirs
}

/// A cheap magic-byte gate — the listers' skip-bad-file rule without a
/// full decode (EXR files can be huge): `.png` must start with the PNG
/// signature, `.exr` with the EXR magic `0x762f3101`.
fn has_env_magic(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 8];
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let n = f.read(&mut buf).unwrap_or(0);
    match format_for_path(path) {
        Some(EnvFormat::Rgba8Unorm) => {
            n >= 8 && buf[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        }
        Some(EnvFormat::Rgba32Float) => n >= 4 && buf[..4] == [0x76, 0x2F, 0x31, 0x01],
        None => false,
    }
}

/// Every loadable environment map in `dirs` — `.png` / `.exr` equirects —
/// as `(display name, path)`, sorted by name, deduplicated by file name
/// (first directory wins), bad files skipped with a log line: the
/// [`crate::document::list_smart_materials`] rules. The display name is
/// the file stem (`studio-neutral.png` → `studio-neutral`).
pub fn list_env_maps(dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && format_for_path(p).is_some())
            .collect();
        paths.sort();
        for path in paths {
            let Some(file) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            if !seen.insert(file.clone()) {
                continue;
            }
            if !has_env_magic(&path) {
                log::warn!("env map {} skipped: wrong magic bytes", path.display());
                continue;
            }
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or(file);
            out.push((name, path));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_dispatch_is_case_insensitive() {
        assert_eq!(
            format_for_path(Path::new("studio.PNG")),
            Some(EnvFormat::Rgba8Unorm)
        );
        assert_eq!(
            format_for_path(Path::new("probe.ExR")),
            Some(EnvFormat::Rgba32Float)
        );
        assert_eq!(format_for_path(Path::new("albedo.jpg")), None);
        assert_eq!(format_for_path(Path::new("no-extension")), None);
    }

    #[test]
    fn default_candidates_end_at_the_committed_asset() {
        let candidates = default_env_candidates();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0], PathBuf::from(DEFAULT_ENV_ASSET));
        assert!(
            candidates[1].ends_with("assets/env/studio-neutral.png"),
            "manifest fallback must land on the asset: {}",
            candidates[1].display()
        );
    }

    #[test]
    fn loading_an_unsupported_format_is_an_error_not_a_panic() {
        // No GPU needed: the format gate runs before any device touch.
        assert!(format_for_path(Path::new("x.tiff")).is_none());
    }

    /// Requests the default device (plain, no features — the convolve
    /// pass needs none). Returns `None` where no wgpu adapter exists,
    /// mirroring `paint_state`'s test convention. NOTE: umber-app has no
    /// `gpu` feature gate (unlike umber-gpu/umber-bake) — GPU tests here
    /// run under plain `cargo test -p umber-app` and skip without an
    /// adapter.
    fn try_request_adapter() -> Option<wgpu::Adapter> {
        let instance = wgpu::Instance::default();
        match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        {
            Ok(adapter) => Some(adapter),
            Err(_) => {
                eprintln!("skipping: no wgpu adapter available");
                None
            }
        }
    }

    #[test]
    fn committed_studio_default_loads_and_convolves() {
        let Some(adapter) = try_request_adapter() else {
            return;
        };
        // The committed asset must exist from the crate dir (tests run
        // with CWD = crates/umber-app, so the manifest fallback wins).
        let manifest_asset =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/env/studio-neutral.png");
        assert!(
            manifest_asset.is_file(),
            "committed default missing: {}",
            manifest_asset.display()
        );
        let (w, h, rgba) =
            umber_export::png::read_png_rgba8(&manifest_asset).expect("studio PNG must decode");
        assert_eq!((w, h), (1024, 512), "generator output dimensions");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("device");
        let gpu = umber_gpu::GpuContext::new(
            adapter,
            device,
            queue,
            wgpu::TextureFormat::Rgba8Unorm,
            None,
        );
        let env = EnvIrradiance::from_equirect(
            &gpu.device,
            &gpu.queue,
            &rgba,
            w,
            h,
            EnvFormat::Rgba8Unorm,
        )
        .expect("studio default must convolve");
        assert_eq!(env.dimensions(), (32, 16));
    }

    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    const EXR_MAGIC: &[u8] = &[0x76, 0x2F, 0x31, 0x01];

    fn write_env(dir: &Path, name: &str, magic: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, magic).unwrap();
        p
    }

    #[test]
    fn env_dirs_with_puts_bundled_first_then_extras_deduped() {
        let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/env");
        let extra = std::env::temp_dir().join("umber-env-dirs-extra");
        let dirs = env_dirs_with(&[bundled.clone(), extra.clone(), extra.clone()]);
        assert_eq!(dirs[0], bundled, "bundled assets/env first (wins dedup)");
        assert_eq!(
            dirs.iter().filter(|d| **d == extra).count(),
            1,
            "extras deduped: {dirs:?}"
        );
        assert!(dirs.len() <= 3, "bundled + exe-dir + one extra: {dirs:?}");
    }

    #[test]
    fn list_env_maps_gates_formats_dedups_first_wins_and_skips_bad_files() {
        let root = std::env::temp_dir().join(format!("umber-env-list-{}", std::process::id()));
        let a = root.join("a");
        let b = root.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        write_env(&a, "studio-neutral.png", PNG_MAGIC);
        write_env(&a, "probe.exr", EXR_MAGIC);
        write_env(&a, "notes.txt", b"not an env map");
        write_env(&a, "fake.png", b"no magic at all");
        // Same file name in b: the first dir must win.
        write_env(&b, "studio-neutral.png", EXR_MAGIC);
        write_env(&b, "extra.exr", EXR_MAGIC);
        let listed = list_env_maps(&[a.clone(), b.clone()]);
        let names: Vec<&str> = listed.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["extra", "probe", "studio-neutral"],
            "sorted by name; .txt and bad-magic skipped: {names:?}"
        );
        let neutral = listed.iter().find(|(n, _)| n == "studio-neutral").unwrap();
        assert!(
            neutral.1.starts_with(&a),
            "first dir wins the same-name dedup: {}",
            neutral.1.display()
        );
        // A missing dir is not an error.
        let with_missing = list_env_maps(&[root.join("nope"), a.clone()]);
        assert_eq!(with_missing.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_bundled_env_dir_lists_the_committed_studio_maps() {
        let listed = list_env_maps(&env_dirs_with(&[]));
        let names: Vec<&str> = listed.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"studio-neutral"),
            "the committed default lists: {names:?}"
        );
        assert!(names.len() >= 3, "three studio maps ship: {names:?}");
    }
}
