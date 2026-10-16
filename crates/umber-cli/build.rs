//! The Windows version resource on `umber-cli.exe` (see
//! `crates/umber-app/build.rs` for the why: SignPath requires product
//! metadata on every signed binary). Targeting Windows only; a missing
//! resource compiler warns instead of failing the build.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "Umber")
        .set(
            "FileDescription",
            "Umber headless CLI (bake, export, agent)",
        )
        .set("InternalName", "umber-cli")
        .set("OriginalFilename", "umber-cli.exe")
        .set("LegalCopyright", "GPL-3.0-or-later; see LICENSE");
    if let Err(e) = res.compile() {
        println!("cargo:warning=no Windows version resource on umber-cli.exe: {e}");
    }
}
