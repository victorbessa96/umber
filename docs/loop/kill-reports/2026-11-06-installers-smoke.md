# Installers smoke: cargo-dist shell installer (worklist #9), 2026-11-06

**Verdict: the build passed and the install passed. The `--version` assertion FAILED because neither binary has a `--version` flag. The installed GUI is missing its runtime assets on any machine without a source tree.**

This is the first time the cargo-dist shell installer has been built and
installed. The config has existed since 9f875b3. Flatpak and AppImage were
**not** built in this pass (the scope was the linux x86_64 shell installer only), so
they still have never been produced.

## Setup

- Source: a detached worktree of `7e64237` at
  `~/.hermes/cache/scratch/umber-dist`. The main worktree had uncommitted
  sibling edits in `crates/`, so the build used a clean checkout.
- dist: pinned `cargo-dist-version = "0.32.0"`, installed the same way as in
  `release.yml`:
  `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/axodotdev/cargo-dist/releases/download/v0.32.0/cargo-dist-installer.sh | sh`
  → `~/.cargo/bin/dist`, `dist --version` = `cargo-dist 0.32.0`.
- Host: Arch Linux, kernel 7.2.14, glibc 2.44, 8 cores.

## Commands and results

| step | command | exit |
|---|---|---|
| config drift | `dist generate --check` | 0 (release.yml matches config) |
| plan | `dist plan` | 0 |
| build | `dist build --artifacts=all --target=x86_64-unknown-linux-gnu` | 0 (`Finished dist profile [optimized] in 16m 12s`) |

`dist plan` announced v0.1.0 for two packages, `umber-app` (bin `umber`) and
`umber-cli` (bin `umber-cli`). Each package gets a `.tar.xz` (linux), a `.zip`
(windows), a shell installer, a PowerShell installer, `source.tar.gz` and sha256 files.
The build log has 0 cargo warnings. dist printed `WARN skipping powershell installer: not
building any supported platforms (use --artifacts=global)` four times. That
skip is expected because Windows was not built.

Network was available, so the `--artifacts local` fallback was not needed.

### Artifacts (`target/distrib/`)

| artifact | bytes |
|---|---|
| `umber-app-installer.sh` | 49,765 |
| `umber-cli-installer.sh` | 49,773 |
| `umber-app-x86_64-unknown-linux-gnu.tar.xz` | 12,974,836 |
| `umber-cli-x86_64-unknown-linux-gnu.tar.xz` | 4,579,260 |
| `source.tar.gz` | 8,390,508 |
| installed `umber` binary | 46,522,176 |
| installed `umber-cli` binary | 15,385,856 |

sha256:
- `umber-app-x86_64-unknown-linux-gnu.tar.xz`: `89d30c17d4ff1140fd246db40e27e23d8b0627a81251507e9bf0c122ecc4474f`. The installer embeds the same value.
- `umber-cli-x86_64-unknown-linux-gnu.tar.xz`: `a64a681d163b90d5b1666484eafc839a4deb503cabedaf6c7bc3f8f4317398b9`.
- `umber-app-installer.sh`: `49e9c512ad311f57587f032eb71ee693fbf82b99e64acd4da46b76b575fdc87f`.

The archive contains `umber` plus README.md, LICENSE, CHANGELOG.md, THIRD_PARTY.md
and THIRD_PARTY_FONTS.txt. The font-licence `include` works. Dynamic deps are
libgcc_s, libm, libc and ld-linux only.

## Prefix smoke

The run used a scratch root `S=~/.hermes/cache/scratch/smoke-1793989269` and a
clean environment (`env -i PATH=/usr/bin:/bin HOME=$S/home TMPDIR=$S/tmp`).
The installer has no `--yes` or `--prefix` flags (see gap 5), so the install
location came from the installer's documented env vars:

```
UMBER_APP_DOWNLOAD_URL=file://$D UMBER_APP_UNMANAGED_INSTALL=$S/prefix/bin sh $D/umber-app-installer.sh
UMBER_CLI_DOWNLOAD_URL=file://$D UMBER_CLI_UNMANAGED_INSTALL=$S/prefix/bin sh $D/umber-cli-installer.sh
```

| check | result | exit |
|---|---|---|
| umber-app install | `installing to …/prefix/bin  umber  everything's installed!` | **0** |
| umber-cli install | `installing to …/prefix/bin  umber-cli  everything's installed!` | **0** |
| binaries land | `prefix/bin/umber` (46.5 MB, +x) and `prefix/bin/umber-cli` (15.4 MB, +x) | — |
| no profile/PATH edits | `$S/home` empty after install (unmanaged mode) | — |
| checksum enforcement | tarball with 1 appended byte → `ERROR: checksum mismatch want: 89d30c… got: 67b23e…`, nothing installed | **1** (correct refusal) |
| `umber --version` (headless) | flag ignored, GUI starts, `Error: WinitEventLoop(… neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.)` | **1** |
| `umber-cli --version` | `Error: unknown command: --version` + usage | **1** |
| `umber-cli` (no args) | prints `umber-cli v0.1.0 (wave-6)` + command list | **0** |
| `umber-cli inspect tri.obj` (1-tri OBJ) | `vertices: 3 triangles: 1 …` | **0** |
| `umber-cli bake-ao tri.obj ao.png --size 64 --rays 8` | `wrote ao.png (64x64, 8 rays)`, valid 64×64 RGBA PNG | **0** |
| `umber-cli bench` (defaults, and `--size 64 --rays 4`) | `Error: position map: mesh has 49152 triangles, over the 4096 budget for this bake path` | **1** |
| `timeout 8 umber` on Wayland, build tree hidden, CWD=scratch | window up, alive at 8 s (killed by timeout) | **124** (expected) |

The installed binaries run. The version check the task asked for does not pass.
`umber-cli` with no arguments is the only way to get the version string.

## Gaps found

1. **No `--version` on either binary.** `umber` ignores unknown `--` flags
   and launches the GUI. `umber-cli` rejects the flag as an unknown command.
   Installers, packagers (Flatpak/AppStream checks, distro scripts) and users
   expect `--version` to print and exit 0. The fix belongs in `crates/`, which
   is out of scope for this pass.
2. **Runtime assets do not ship, and the binary looks for them in the build
   tree.** The archive holds only the binary and docs. The asset paths are
   CWD-relative (`assets/env/studio-neutral.png`) or compile-time
   `env!("CARGO_MANIFEST_DIR")` joins. Two absolute builder paths are baked
   into `umber`. In `env.rs`, `assets_panel.rs` (starter library), `plugins.rs`
   (`../../examples`), `actions.rs` (`docs/user/keyboard.md`) and the
   umber-gui icon atlas, the lookup only works on the build machine. On a
   clean machine, observed with the build tree moved away and CWD set to
   scratch:
   - `WARN icon atlas unavailable (atlas io …/crates/umber-gui/../../assets/icons/atlas/manifest.json: No such file or directory)`
   - `no default environment found; procedural fallback`
   - The smart-material library, examples and keyboard docs are missing for the same reason. They were not individually exercised.

   Adding `assets/` to the dist `include` would **not** fix this, because no
   lookup checks paths relative to the executable. `dist-workspace.toml` was
   left unchanged for that reason. The fix needs exe-relative (or
   XDG data-dir) asset resolution in `crates/`, and then `include`
   entries.
3. **The installer only works once a GitHub release exists.**
   `ARTIFACT_DOWNLOAD_URLS` is hard-wired to
   `https://github.com/victorbessa96/umber/releases/download/v0.1.0`, which
   currently returns `HTTP/2 404`. The `curl | sh` path cannot work until a `v0.1.0` tag
   runs `release.yml`. This smoke used `UMBER_APP_DOWNLOAD_URL=file://…`.
4. **The local build needs glibc 2.44.** The highest required symbol is
   `GLIBC_2.44`, because the build ran on Arch. Release artifacts come from
   `ubuntu-22.04` runners (glibc 2.35), so CI output is more portable. The
   binary tested here is **not** the binary a release would ship, and the CI
   build's glibc floor is unmeasured.
5. **The task's installer flags do not exist.** The dist 0.32 shell installer
   accepts only `-v`, `-q`, `--no-modify-path` (deprecated) and `-h`. The install
   location is set through `UMBER_APP_INSTALL_DIR` / `UMBER_APP_UNMANAGED_INSTALL`.
   The default location is `$CARGO_HOME/bin`, and the installer edits
   shell profiles. Users without Rust will get a new `~/.cargo/bin` directory,
   which is an odd default for a GUI app. dist's `install-path` setting
   can change it.
6. **There are two installers.** The workspace has two dist-able packages, so a
   user needs two `curl | sh` runs to get the GUI and the CLI. If one
   installer is intended, set `dist = false` on one package or merge them.
7. **`umber-cli bench` fails with its own defaults.** Its built-in mesh has
   49,152 triangles, over the 4,096-triangle budget of the position-map
   bake path. Exit 1 occurs regardless of `--size` / `--rays`. This is a bug in the CLI
   itself, separate from packaging.
8. **A launch that fails for a non-GPU reason makes the next launch fall back to GL.**
   The headless `umber --version` run wrote the GPU startup marker and then
   failed at winit event-loop creation (no display). The next launch, using the
   same `$HOME`, logged `ERROR gpu_startup: the previous launch died inside the
   GPU driver on auto; falling back to gl` and saved that choice to
   `~/.local/share/umber/gpu-backend`. A missing display gets recorded as a driver crash.
9. **Not covered:** Windows zip and PowerShell installer (needs
   `--artifacts=global` or a Windows runner), Flatpak, AppImage, macOS
   (not a target), and a real `curl | sh` from a GitHub release (blocked by
   gap 3).

## Reproduce

```
dist --version                     # cargo-dist 0.32.0
git worktree add --detach <wt> 7e64237 && cd <wt>
dist generate --check && dist plan
dist build --artifacts=all --target=x86_64-unknown-linux-gnu
D=$PWD/target/distrib; S=$(mktemp -d)
env -i PATH=/usr/bin:/bin HOME=$S TMPDIR=$S \
  UMBER_APP_DOWNLOAD_URL=file://$D UMBER_APP_UNMANAGED_INSTALL=$S/bin \
  sh $D/umber-app-installer.sh; echo $?
```

Artifacts and the full build log are kept at
`~/.hermes/cache/scratch/umber-dist-artifacts/`, outside git.
