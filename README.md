# Umber

<p align="center">
  <b>Open-source, Rust-native 3D texture painting — the community's answer to Adobe Substance 3D Painter.</b><br>
  Paint PBR materials directly on your meshes, build non-destructive layer stacks, bake mesh maps,
  and export engine-ready texture sets. Fast, local-first, and free forever. Written entirely in Rust.
</p>

<p align="center">
  <img alt="Status: pre-release" src="https://img.shields.io/badge/status-pre--release-d69e2e?style=flat-square">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/pure-Rust-b7410e?style=flat-square&logo=rust">
  <img alt="Linux · Windows" src="https://img.shields.io/badge/Linux%20%C2%B7%20Windows-2f7bf5?style=flat-square">
  <img alt="Agents: JSONL control + CLI" src="https://img.shields.io/badge/agents-JSONL%20%2B%20CLI-555555?style=flat-square">
  <img alt="License: GPL-3.0-or-later" src="https://img.shields.io/badge/license-GPL--3.0--or--later-3a3a3a?style=flat-square">
</p>

> [!IMPORTANT]
> **Status: pre-release, core loop complete.** The whole path works end to end and is tested on Linux and Windows CI: import a mesh → paint at 4K with real pen pressure → see it lit in the 3D viewport → undo/redo byte-exact → save and reopen with nothing lost → export engine-ready textures (decode-verified). Every layer owns its own pixels, materials and fills are live, and a headless JSONL agent protocol drives the same paint engine the app uses. <!-- scorecard:crates -->16<!-- /scorecard --> crates, <!-- scorecard:tests -->2,900+<!-- /scorecard --> tests.
>
> **Not yet:** no release tag (SPEC ratification and the first drift-hardened nightly green lead the 0.1.0 checklist, `docs/specs/release-checklist-0.1.0.md`); the open feature waves — the Win32 tablet path, the a11y pass, the power-user tail and the 0.1.0 release gates — are tracked in [`docs/ROADMAP.md`](docs/ROADMAP.md) and the current wave plan [`docs/loop/waves/2026-11-09-wave-plan.md`](docs/loop/waves/2026-11-09-wave-plan.md) (the decal projector lane closed 2026-11-09; brush angle/jitter and bitmap stamps landed C2-11, 2026-10-19).

<p align="center">
  <a href="#why">Why</a> ·
  <a href="#why-umber">Why Umber</a> ·
  <a href="#what-works-today">What works today</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#repository-layout">Repository layout</a> ·
  <a href="#contributing">Contributing</a> ·
  <a href="#license">License</a>
</p>

## Why

Substance Painter is the industry standard for game/film texturing — and it is subscription-only, closed, and cloud-pressured. The open-source alternatives each miss something: ArmorPaint is close but single-maintainer with a thin material ecosystem; Blender's texture paint is a module of a larger tool, not a focused painter; everything else is either dead, film-priced (Mari ~$2k/yr), or discontinued (Quixel Mixer). The world deserves a Rust-native, GPL, community-owned painter with a professional workflow.

## Principles

- **Free forever, GPL-3.0-or-later.** No subscriptions, no license servers, no telemetry. Community-owned, community-extended.
- **Local-first.** Your machine, your files. No account. No cloud requirement — ever.
- **Fast by construction.** Rust + wgpu (Vulkan/DX12). Compute-first painting and baking. 60fps viewport floor.
- **Original procedural engine.** Umber ships its own node-graph engine for generators, filters, and smart materials. It never executes Adobe's formats: `.sbsar` is out, and `.spp` is import-only (read-only, clean-room, `crates/spp` — ratified 2026-10-12), never written.
- **Git-friendly project format.** Diffable, mergeable projects — the anti-.spp.
- **Windows + Linux first-class.** One codebase, no second-class platform.

## Why Umber

- **Familiar.** Texture sets, a layer stack with per-channel blending, masks, smart materials, the tool shelf, 3D and 2D views — the layout texture artists already know.
- **Fast.** A tiled, byte-exact paint engine with GPU viewport; mesh maps bake on the GPU, and the undo journal is bounded and coalescing.
- **Seamless.** Strokes flow across UV seams (UDIM-aware, seam-clipped at tile boundaries), and front-face falloff keeps paint off the back of the model.
- **Robust.** Crash-safe GPU startup with automatic backend fallback, crash-safe saves with a commit protocol, hostile-input-tested commands.
- **Open.** glTF/GLB, FBX, OBJ, USDA/USDC/USDZ in; PNG, JPEG, TIFF, EXR, PSD, USD out at up to 8K with engine presets.
- **Agent-native.** Every panel action is a registered command: drive the same paint engine from the UI, the `--control` JSON channel, the headless CLI, or the JSONL agent protocol.

## What works today

| Area | |
|---|---|
| **Import** | FBX, glTF/GLB, OBJ, USDA, USDC, USDZ; PSD import; `.spp` opens read-only and reports every dataset, but real Painter projects keep their paint inside an undocumented graph format, so no channels import from them yet |
| **Painting** | MyPaint-style dynamics + stabilizer, 9 falloff curves, smudge / clone / erase, mirror + radial symmetry, path and polygon tools, quick mask, seam-aware strokes, front-face falloff for 3D strokes, UDIM |
| **Layers** | per-layer pixel planes, paint / fill / material / folder / instance layers, masks, anchors, 32 blend modes, click-to-time-travel history |
| **Materials** | an original node-graph engine (generators, filters, smart materials); MaterialX export + import |
| **Baking** | AO, normal (mesh + high→low with cage), curvature, position, world-space normal, thickness, ID, bent normals; dilation and an Adjacent Faces seam margin |
| **Viewport** | OpenPBR shading, IBL, view modes, post-effects, orthographic camera, OCIO / ACES color management |
| **Export** | engine presets (Unreal, Unity, glTF, Blender) with DX/GL normal conventions, EXR / PNG / TIFF / PSD / USD / MaterialX, up to 8K |
| **Automation** | headless CLI (bake, export, batch recipes) and a versioned JSONL agent protocol: projects, layers, undo, paint strokes, plane snapshots |
| **Extensibility** | sandboxed WASM plugins (wasmtime, fuel-metered, memory-capped) |
| **Platform** | Linux + Windows; crash-safe GPU startup with backend fallback; i18n (en-US, pt-BR) |

Still open is tracked in [`docs/ROADMAP.md`](docs/ROADMAP.md) — the Win32 tablet path, the a11y pass, channel manage, the four post nodes, and the 0.1.0 release gates among them.

## Quick start

```bash
# the app: viewport, layer stack, Bakes panel, Export dialog
git clone https://github.com/victorbessa96/umber.git && cd umber
cargo run -p umber-app

# the app plus a loopback JSON control channel (drive panels by command id)
cargo run -p umber-app -- --control 7878

# headless: bake every P0 mesh map from a mesh, seam-padded
cargo run -p umber-cli -- bake-all sword.obj out/ --size 1024 --rays 16 --dilate 16

# headless: pack through an engine preset (DirectX normals for Unreal)
cargo run -p umber-cli -- export sword.obj out/ --preset unreal

# drive the app's paint engine over JSONL (one step per line, one reply per step)
printf '%s\n' '"version"' '"quit"' | cargo run -p umber-cli -- agent
```

GPU work needs a Vulkan (Linux) or DX12 (Windows) adapter; `mesa-vulkan-drivers` (lavapipe) works for headless use. If a driver crashes at startup, the next launch steps down to a safer backend automatically; `--safe-gpu` or `WGPU_BACKEND` force one.

**First time?** Follow [`docs/user/getting-started.md`](docs/user/getting-started.md): open a mesh → bake → paint → save → export, every step naming the real menu. Shortcuts live in [`docs/user/keyboard.md`](docs/user/keyboard.md) — note the two viewport-local keys, `P` (toggle orthographic) and `F` (frame the mesh), which fire only while the viewport is hovered.

## Code signing policy

Windows builds are not signed yet; Umber is applying for free open-source
signing (SignPath Foundation). Until then every release ships SHA-256
checksums. Roles, privacy and uninstall details: [docs/CODE_SIGNING.md](docs/CODE_SIGNING.md).

## License

GPL-3.0-or-later for the application. Standalone library crates extracted from Umber may be released under MIT where it helps the ecosystem.

## Repository layout

```
umber/
├── SPEC.md            # the contract — what Umber is and is not
├── STATE.md           # current state (read first)
├── DECISIONS.md       # append-only architecture decision log
├── CHANGELOG.md       # what landed, by wave
├── docs/
│   ├── ROADMAP.md     # the plan of record
│   ├── references/    # reference READMEs from sibling open-source apps
│   ├── specs/         # design docs per subsystem
│   ├── user/          # the newcomer path: getting-started.md, keyboard.md
│   ├── loop/          # the autonomous-build records: queue, kill-ledger,
│   │                  #   kill-reports/ (per-kill evidence), briefs, coordination notes
│   ├── research/      # cited research (Painter inventory, competitors, absorption ledgers)
│   └── claw-artifacts/ # vendored third-party source dumps from claw research
│                      #   (large; historical, kept for provenance)
├── scripts/           # the local battery, CI helpers, the crate layering gate
└── crates/            # Rust workspace — <!-- scorecard:crates -->16<!-- /scorecard --> crates in a layered dependency graph
```

## Contributing

Code contributions are open. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) (setup, the exact CI gates, commit format), then [`STATE.md`](STATE.md) and [`docs/ROADMAP.md`](docs/ROADMAP.md) for what to pick up. Security reports go through [`SECURITY.md`](SECURITY.md).

## Credit

Built by [Bessa](https://github.com/victorbessa96) and Razul, with research claws.
