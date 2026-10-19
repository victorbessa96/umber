# Flatpak packaging — Umber (`org.umber.Umber`)

v1 manifest track for the requirements §10 distribution row
("cargo-dist (MSI + GH releases), Flatpak + AppImage on Linux").
cargo-dist 0.32.0 has no Flatpak/AppImage targets, so this directory is
NEW packaging, not a dist-config extension. Status: config lands here;
the build is proven by CI (`.github/workflows/packaging.yml`, gated —
see below), not by this kill (no cargo / no downloads in the claw).

## Files

| File | Role |
|---|---|
| `org.umber.Umber.json.in` | the hand-maintained manifest TEMPLATE: app metadata, module, build-commands. The `@@CARGO_SOURCES@@` slot is where the generator splices the crate sources |
| `org.umber.Umber.json` | the GENERATED manifest (committed artifact — `gen-cargo-sources.py` renders it from the template + Cargo.lock; CI's lint job re-runs the generator and fails on drift) |
| `gen-cargo-sources.py` | Cargo.lock → one `type: archive` source per registry crate (url = `static.crates.io/crates/<n>/<n>-<v>.crate`, sha256 = the lock's own checksum — verified live: a fetched `.crate`'s sha256 equals the lock entry byte-for-byte), spliced into the template |
| `vendor-checksums.py` | build-command #1 inside the sandbox: writes each staged `vendor/<crate>/.cargo-checksum.json` exactly like `cargo vendor` (full files map + the lock's package checksum), so cargo's source-replacement consumer accepts the tree |
| `org.umber.Umber.desktop` | desktop entry, installed to `/app/share/applications/` |
| `org.umber.Umber.metainfo.xml` | AppStream metadata, installed to `/app/share/metainfo/` |
| `LICENSE`, `LICENSES/THIRD_PARTY_FONTS.txt`, `THIRD_PARTY.md` (repo root, from the git source) | installed to `/app/share/licenses/org.umber.Umber/`: the GPL text plus the embedded egui fonts' OFL/UFL/MIT notices, which must accompany every copy |
| `org.umber.Umber.svg` | v1 icon composition (NOT a final logo — see below) |

## Judgment calls (v1 boundaries)

- **Buildsystem `simple`, cargo run directly.** No meson/cmake wrapper:
  flatpak-builder executes `build-commands` in the SDK shell, and the
  workspace builds with `cargo --offline --locked build --release -p
  umber-app`. Hermetic since 2026-10-18: every registry crate is a
  manifest SOURCE (flatpak-builder downloads them in its network-legal
  source phase), staged into `vendor/` + consumed through cargo's
  documented source-replacement — no network at build time, Flathub-
  submission-shaped. The hermetic closure is what run 38883955783's
  red demanded (the sandbox has no DNS: `index.crates.io`
  unresolvable, exit 101).
- **Runtime `org.freedesktop.Platform` 24.08, SDK `org.freedesktop.Sdk`.**
  24.08 is the brief-pinned branch (verified live on Flathub at kill time;
  25.08/26.08 exist — bumping the branch is a one-line follow-up, not v1).
- **No `org.freedesktop.Platform.Vulkan` extension — it does not exist.**
  The brief named it; the flatpak-docs runtime/extension list (verified
  2026-10-12) names Mesa/Vulkan drivers only under
  `org.freedesktop.Platform.GL.default`, which `--device=dri` pulls in
  automatically. The manifest therefore requests `--device=dri` and no
  Vulkan extension. This correction is the load-bearing fact of this kill.
- **Rust via the `org.freedesktop.Sdk.Extension.rust-stable` SDK extension**
  (`append-path /usr/lib/sdk/rust-stable/bin`, the Flathub-documented
  shape). Consequence, stated honestly: the Flatpak build uses the
  extension's stable toolchain, NOT the repo-pinned 1.97.0
  (`rust-toolchain.toml`). `Cargo.toml`'s `rust-version = "1.85"` floor
  keeps this sound (newer stable builds it); dependency reproducibility
  still holds via `--locked`. Re-pinning to an exact toolchain inside the
  sandbox (rustup) is the named follow-up if Flathub demands it.
- **No extra SDK modules v1.** winit 0.30.13 links against X11/Wayland
  client libs at build time and `x11-dl` 2.21.0 dlopens libX11/libXi at
  runtime (see `umber-app/Cargo.toml`); wayland-client 0.31.15 likewise.
  The 24.08 SDK ships all of these dev libraries, so the manifest has a
  single module. If the CI flatpak job fails on a missing `-dev` package,
  that failure names the module to add — recorded, not guessed.
- **finish-args, each with a reason:** `--share=ipc` (X11/Wayland
  requirement); `--socket=wayland` + `--socket=fallback-x11` (winit backends
  the app actually exercises); `--device=dri` (wgpu/Vulkan — the Mesa
  correction above); `--talk-name=org.freedesktop.portal.*` (file-chooser
  portals used by rfd/ashpd); `--talk-name=org.a11y.Bus` (accessibility via
  accesskit); `--filesystem=xdg-documents` + `--filesystem=xdg-pictures`
  (project/mesh homes; one-off picks go through portals). No
  `--filesystem=host`, no PulseAudio (the app produces no audio).
- **Git source tracks `master`.** No release tag exists yet (releases are
  Bessa's call per the installers design doc); at ship time this pins to
  the release tag. The CI job builds the working tree as-is.
- **Metainfo has no `<screenshot>`.** Screenshots must be hosted URLs;
  inventing one would fail `appstreamcli validate` and lie to users.
  Adding the screenshot URL is queued behind the first public release.
  The single `<release>` entry is honestly typed `development`.
- **No `MimeType=` in the desktop file.** Claiming
  `application/x-umber-project` without shipping a shared-mime-info
  definition would be a broken claim; the mime definition ships with a
  later file-association slice.
- **Icon is a placeholder composition.** `org.umber.Umber.svg` traces the
  lucide `paintbrush.svg` glyph (from `assets/icons/`, ISC set) in amber
  (`#e8a33d`) on a dark pro-theme tile (`#1a1d24`). It exists so the
  desktop entry, AppStream, and AppDir validate — it is NOT the Umber
  brand mark, and the metainfo/desktop files must not be read as claiming
  otherwise.

## Runtime dependencies (for the manifest record)

At runtime the Flatpak carries: the `umber` binary (statically links
wasmtime/Cranelift — no runtime `.so` concern), the freedesktop 24.08
runtime (X11, Wayland, Mesa incl. Vulkan drivers via `--device=dri`),
and the host GPU stack. The Vulkan loader is system-provided per the
installers design doc; nothing driver-shaped is bundled.

## Build locally (human run — the claw cannot)

```sh
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.freedesktop.Platform//24.08 org.freedesktop.Sdk//24.08
flatpak-builder --force-clean build-dir org.umber.Umber.json
flatpak-builder --run build-dir org.umber.Umber.json umber
```

CI runs the build on release tags and `workflow_dispatch` only: the
runtime + SDK download is ~1 GB and too heavy for every push. The
per-push proof is manifest/desktop/metainfo lint (see
`.github/workflows/packaging.yml`).
