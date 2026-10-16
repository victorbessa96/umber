//! Records the commit the binary is built from as `UMBER_GIT_HASH`, read
//! by Help ▸ About (`shell_menu::AboutInfo`).
//!
//! Order: `GITHUB_SHA` (CI and release builds), then `git rev-parse
//! --short=12 HEAD`. Any failure (no git on PATH, a source tarball with no
//! `.git`) sets nothing and the build carries on: About then says
//! "commit: unknown". The build never fails because of this script.
//!
//! Cost: the script reruns when HEAD moves, so every new commit, docs-only
//! ones included, recompiles the `umber` binary crate once.

use std::path::Path;
use std::process::Command;

fn main() {
    windows_version_resource();
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    let git_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.git");
    // Only watch paths that exist: cargo reruns a script on EVERY build
    // when a watched path is missing.
    for watched in ["HEAD", "refs/heads", "packed-refs"] {
        let path = git_dir.join(watched);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let hash = std::env::var("GITHUB_SHA")
        .ok()
        .filter(|s| is_hex(s))
        .map(|s| s.chars().take(12).collect::<String>())
        .or_else(|| {
            let out = Command::new("git")
                .args(["rev-parse", "--short=12", "HEAD"])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .ok()?;
            let text = String::from_utf8(out.stdout).ok()?;
            let text = text.trim();
            (out.status.success() && is_hex(text)).then(|| text.to_owned())
        });
    if let Some(hash) = hash {
        println!("cargo:rustc-env=UMBER_GIT_HASH={hash}");
    }
}

/// A plausible commit id: 7 to 64 hex digits. Anything else (an error
/// message on stdout, an empty variable) is not recorded.
fn is_hex(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The Windows version resource (ProductName, versions, description) on
/// `umber.exe`: SignPath requires product metadata on every signed binary
/// (`docs/CODE_SIGNING.md`), and Explorer's Properties ▸ Details shows it.
/// Only when TARGETING Windows (`CARGO_CFG_TARGET_OS`, not the host), so
/// Linux builds never look for a resource compiler. A compile failure
/// (no Windows SDK `rc.exe`) is a warning, not a broken build: the binary
/// is still correct, it just carries no metadata, and the release job's
/// check catches that.
fn windows_version_resource() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "Umber")
        .set("FileDescription", "Umber texture painter")
        .set("InternalName", "umber")
        .set("OriginalFilename", "umber.exe")
        .set("LegalCopyright", "GPL-3.0-or-later; see LICENSE");
    // `compile_for`: this package also builds a library target (the
    // integration tests link it), and the resource belongs to the
    // `umber` binary alone.
    if let Err(e) = res.compile_for("umber") {
        println!("cargo:warning=no Windows version resource on umber.exe: {e}");
    }
}
