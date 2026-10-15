# Kill report (UNRUN, host-gated): Windows version resource on both binaries

Date: 2026-10-15. Author: dragon. Lock: `win-version-resource` over the
two build scripts, `crates/umber-cli/Cargo.toml` and this report. Alpha
step 0 (SignPath needs product metadata on every signed binary).

## Written

- `crates/umber-app/build.rs`: `windows_version_resource()` sets
  ProductName "Umber", FileDescription, InternalName, OriginalFilename
  "umber.exe" and LegalCopyright, then calls `compile_for("umber")`.
  This package also has a lib target, so the resource is bound to the
  binary alone (the crate docs warn against mixing `compile` and
  `compile_for`).
- `crates/umber-cli/build.rs` (new): the same for `umber-cli.exe`,
  using `compile()` (one binary, no lib).
- File/ProductVersion come from `CARGO_PKG_VERSION` through
  `WindowsResource::new()`, so the alpha bump flows in with no edit.
- Both run only when `CARGO_CFG_TARGET_OS == "windows"` (the TARGET,
  not the host), and a resource-compiler failure is a `cargo:warning`,
  not an error.

API checked against docs.rs winresource 0.1.34: `new()`,
`set(&str, &str) -> &mut Self`, `compile() -> io::Result<()>`,
`compile_for(&str) -> io::Result<()>`.

## Still to do, with the host

1. The workspace dependency `winresource = "0.1.34"` in the root
   `[workspace.dependencies]`, plus
   `[build-dependencies] winresource = { workspace = true }` in
   `crates/umber-app/Cargo.toml`. Both files are under
   `g5-workspace-persist`'s lock (its egui_dock serde edit); add them
   when that kill commits, NOT before. umber-cli's Cargo.toml already
   carries the build-dep line.
2. `cargo metadata` on the host resolves it into Cargo.lock: winresource
   (MIT) + `version_check` 0.9.5 (already locked) and `toml` (optional,
   off by default). Add winresource to THIRD_PARTY.md.
3. CI's Windows release smoke builds it. Then check
   `(Get-Item target\release\umber.exe).VersionInfo` on a Windows
   build: ProductName Umber, ProductVersion = the crate version.

Until step 1 lands, umber-cli's build-dep names a workspace dependency
that does not exist yet. These files therefore commit TOGETHER with
steps 1–2, never alone (a lone commit would break every build).
