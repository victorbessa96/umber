//! Resolution of the bundled runtime data (`assets/`, `examples/`) for a
//! packaged binary. Installers-smoke 2026-11-06 gap #2: the lookups were
//! CWD-relative or compile-time `CARGO_MANIFEST_DIR` joins, so an
//! installed GUI on a machine without the source tree found nothing
//! (icon atlas, env maps, starter library, examples all missing).
//!
//! [`data_path`] tries, in priority order: `$UMBER_ASSETS_DIR`,
//! `<exe>/../share/umber` (XDG-style install), `<exe>` (dist archives
//! ship `assets/` next to the binary), the compile-time repo root (the
//! dev loop), and the CWD (running from a checkout). The first candidate
//! that actually holds the file wins; when nothing exists the dev repo
//! join is returned so dev behaviour and error messages are unchanged.

use std::path::{Path, PathBuf};

/// Env override for the whole data root (packagers, tests, debugging).
pub const DATA_ROOT_ENV: &str = "UMBER_ASSETS_DIR";

/// The compile-time repo root (dev fallback; correct for `cargo run`).
fn manifest_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Candidate roots in priority order. Pure — the public wrappers inject
/// the live exe/cwd/env so the order is unit-testable.
fn candidates(exe: Option<&Path>, cwd: Option<&Path>, env_root: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(r) = env_root {
        v.push(r.to_path_buf());
    }
    if let Some(exe) = exe {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("../share/umber"));
            v.push(dir.to_path_buf());
        }
    }
    v.push(manifest_repo_root());
    if let Some(c) = cwd {
        v.push(c.to_path_buf());
    }
    v
}

fn live_candidates() -> Vec<PathBuf> {
    candidates(
        std::env::current_exe().ok().as_deref(),
        std::env::current_dir().ok().as_deref(),
        std::env::var_os(DATA_ROOT_ENV).as_deref().map(Path::new),
    )
}

/// Every existing location of `rel`, best first, one entry per real
/// directory: `cargo run` from the checkout makes the manifest root and
/// the CWD the same dir, and listing it twice would double every scanned
/// starter. Empty when no candidate holds the file.
pub fn data_paths(rel: &str) -> Vec<PathBuf> {
    existing_unique(live_candidates().into_iter().map(|r| r.join(rel)))
}

/// The existing paths of `paths`, in order, dropping any whose canonical
/// form an earlier entry already holds.
fn existing_unique(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for p in paths.filter(|p| p.exists()) {
        let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
        if !seen.contains(&canon) {
            seen.push(canon);
            out.push(p);
        }
    }
    out
}

/// The best location of `rel` (a repo-root-relative path like
/// `assets/env/studio-neutral.png` or `examples`). Falls back to the dev
/// repo join when nothing exists, so callers can always stat the result.
pub fn data_path(rel: &str) -> PathBuf {
    data_paths(rel)
        .into_iter()
        .next()
        .unwrap_or_else(|| manifest_repo_root().join(rel))
}

/// The data root for logging: the first candidate that contains an
/// `assets/` dir, else the dev repo root.
pub fn data_root() -> PathBuf {
    live_candidates()
        .into_iter()
        .find(|r| r.join("assets").is_dir())
        .unwrap_or_else(manifest_repo_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("umber-bundled-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn candidate_order_is_env_exe_share_exe_manifest_cwd() {
        let c = candidates(
            Some(Path::new("/opt/prefix/bin/umber")),
            Some(Path::new("/work")),
            Some(Path::new("/envroot")),
        );
        assert_eq!(c[0], PathBuf::from("/envroot"));
        assert_eq!(c[1], PathBuf::from("/opt/prefix/bin/../share/umber"));
        assert_eq!(c[2], PathBuf::from("/opt/prefix/bin"));
        assert_eq!(c[3], manifest_repo_root());
        assert_eq!(c[4], PathBuf::from("/work"));
    }

    #[test]
    fn exe_relative_wins_over_manifest_and_cwd() {
        // Simulated dist layout: <tmp>/bin/umber + <tmp>/bin/assets/env/x.png,
        // with the manifest root and CWD both (wrongly) holding the file too.
        let root = tmp("dist");
        let bin = root.join("bin");
        fs::create_dir_all(bin.join("assets/env")).unwrap();
        fs::write(bin.join("assets/env/x.png"), b"dist").unwrap();
        let cwd = tmp("cwd");
        fs::create_dir_all(cwd.join("assets/env")).unwrap();
        fs::write(cwd.join("assets/env/x.png"), b"cwd").unwrap();
        let found = candidates(Some(&bin.join("umber")), Some(&cwd), None)
            .into_iter()
            .map(|r| r.join("assets/env/x.png"))
            .find(|p| p.exists())
            .unwrap();
        assert_eq!(found, bin.join("assets/env/x.png"), "exe-relative wins");
        assert_eq!(fs::read(&found).unwrap(), b"dist");
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&cwd).unwrap();
    }

    #[test]
    fn share_layout_beats_flat_exe_dir() {
        let root = tmp("xdg");
        let bin = root.join("bin");
        let share = root.join("share/umber");
        fs::create_dir_all(bin.join("assets")).unwrap();
        fs::write(bin.join("assets/m.bin"), b"flat").unwrap();
        fs::create_dir_all(share.join("assets")).unwrap();
        fs::write(share.join("assets/m.bin"), b"share").unwrap();
        let found = candidates(Some(&bin.join("umber")), None, None)
            .into_iter()
            .map(|r| r.join("assets/m.bin"))
            .find(|p| p.exists())
            .unwrap();
        // The candidate is the lexical `bin/../share/umber`: compare the
        // real file, not the spelling.
        assert_eq!(
            found.canonicalize().unwrap(),
            share.join("assets/m.bin").canonicalize().unwrap()
        );
        assert_eq!(fs::read(&found).unwrap(), b"share");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn env_override_wins_over_everything() {
        // Serialized: mutates process env.
        static M: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = M.lock().unwrap();
        let root = tmp("envroot");
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(root.join("assets/marker.txt"), b"m").unwrap();
        let bin = tmp("envbin");
        fs::create_dir_all(bin.join("assets")).unwrap();
        fs::write(bin.join("assets/marker.txt"), b"exe").unwrap();
        std::env::set_var(DATA_ROOT_ENV, &root);
        let found = candidates(
            Some(&bin.join("umber")),
            None,
            std::env::var_os(DATA_ROOT_ENV).as_deref().map(Path::new),
        )
        .into_iter()
        .map(|r| r.join("assets/marker.txt"))
        .find(|p| p.exists())
        .unwrap();
        std::env::remove_var(DATA_ROOT_ENV);
        assert_eq!(found, root.join("assets/marker.txt"));
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&bin).unwrap();
    }

    #[test]
    fn dev_fallback_finds_the_committed_studio_map() {
        // In the repo the manifest root holds the asset, so the live
        // resolver must land on an existing file.
        let p = data_path("assets/env/studio-neutral.png");
        assert!(
            p.is_file(),
            "committed studio map resolves: {}",
            p.display()
        );
    }

    #[test]
    fn missing_rel_falls_back_to_the_manifest_join() {
        let p = data_path("assets/definitely-not-there.bin");
        assert!(p.starts_with(manifest_repo_root()));
        assert!(p.ends_with("assets/definitely-not-there.bin"));
    }

    #[test]
    fn data_paths_lists_all_existing_best_first() {
        let a = tmp("multi-a");
        let b = tmp("multi-b");
        for d in [&a, &b] {
            fs::create_dir_all(d.join("assets")).unwrap();
            fs::write(d.join("assets/m.txt"), b"x").unwrap();
        }
        // env root = a, cwd = b: both exist, env first.
        let paths = candidates(None, Some(&b), Some(&a))
            .into_iter()
            .map(|r| r.join("assets/m.txt"))
            .filter(|p| p.exists())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec![a.join("assets/m.txt"), b.join("assets/m.txt")]);
        fs::remove_dir_all(&a).unwrap();
        fs::remove_dir_all(&b).unwrap();
    }

    #[test]
    fn same_dir_via_two_roots_is_listed_once() {
        // `cargo run` from the checkout: manifest root == CWD. The second
        // spelling (`<root>/sub/..`) is the same directory and must drop.
        let a = tmp("dedup");
        fs::create_dir_all(a.join("assets")).unwrap();
        fs::create_dir_all(a.join("sub")).unwrap();
        let other = a.join("sub/..").join("assets");
        let paths = existing_unique(
            [a.join("assets"), other, a.join("missing"), a.join("assets")].into_iter(),
        );
        assert_eq!(paths, vec![a.join("assets")]);
        fs::remove_dir_all(&a).unwrap();
    }
}
