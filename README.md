# Umber

**Open-source, Rust-native 3D texture painting — the community's answer to Adobe Substance 3D Painter.**

Umber is a ground-up reimagining of the 3D texture-painting DCC: paint PBR materials directly on your meshes, build non-destructive layer stacks, bake mesh maps, and export engine-ready texture sets — fast, local-first, and free forever. Built completely in Rust for **Windows and Linux**.

> **Status: pre-release, core loop complete.** The whole path works end to end and is tested on Linux and Windows CI: import a mesh → paint at 4K with real pen pressure → see it lit in the 3D viewport → undo/redo byte-exact → save and reopen with nothing lost → export engine-ready textures (decode-verified). Every layer owns its own pixels, materials and fills are live, and a headless JSONL agent protocol drives the same paint engine the app uses. 16 crates, 1,600+ tests. No release tag yet: SPEC ratification and the perf baseline lead the 0.1.0 checklist (`docs/specs/release-checklist-0.1.0.md`).

## Why

Substance Painter is the industry standard for game/film texturing — and it is subscription-only, closed, and cloud-pressured. The open-source alternatives each miss something: ArmorPaint is close but single-maintainer with a thin material ecosystem; Blender's texture paint is a module of a larger tool, not a focused painter; everything else is either dead, film-priced (Mari ~$2k/yr), or discontinued (Quixel Mixer). The world deserves a Rust-native, GPL, community-owned painter with a professional workflow.

## Principles

- **Free forever, GPL-3.0-or-later.** No subscriptions, no license servers, no telemetry. Community-owned, community-extended.
- **Local-first.** Your machine, your files. No account. No cloud requirement — ever.
- **Fast by construction.** Rust + wgpu (Vulkan/DX12). Compute-first painting and baking. 60fps viewport floor.
- **Original procedural engine.** We do not touch Adobe's .sbsar/.spp formats — Umber ships its own node-graph engine for generators, filters, and smart materials.
- **Git-friendly project format.** Diffable, mergeable projects — the anti-.spp.
- **Windows + Linux first-class.** One codebase, no second-class platform.

## What works today

| Area | |
|---|---|
| **Import** | FBX, glTF/GLB, OBJ, USDA, USDC, USDZ; `.spp` (read-only, clean-room) and PSD import |
| **Painting** | MyPaint-style dynamics + stabilizer, 9 falloff curves, smudge / clone / erase, mirror + radial symmetry, path and polygon tools, quick mask, seam-aware strokes, front-face falloff for 3D strokes, UDIM |
| **Layers** | per-layer pixel planes, paint / fill / material / folder / instance layers, masks, anchors, 32 blend modes, click-to-time-travel history |
| **Materials** | an original node-graph engine (generators, filters, smart materials); MaterialX export + import |
| **Baking** | AO, normal (mesh + high→low with cage), curvature, position, world-space normal, thickness, ID, bent normals; dilation and an Adjacent Faces seam margin |
| **Viewport** | OpenPBR shading, IBL, view modes, post-effects, orthographic camera, OCIO / ACES color management |
| **Export** | engine presets (Unreal, Unity, glTF, Blender) with DX/GL normal conventions, EXR / PNG / TIFF / PSD / USD / MaterialX, up to 8K |
| **Automation** | headless CLI (bake, export, batch recipes) and a versioned JSONL agent protocol: projects, layers, undo, paint strokes, plane snapshots |
| **Extensibility** | sandboxed WASM plugins (wasmtime, fuel-metered, memory-capped) |
| **Platform** | Linux + Windows; crash-safe GPU startup with backend fallback; i18n (en-US, pt-BR) |

Still open is tracked in [`docs/ROADMAP.md`](docs/ROADMAP.md) — brush angle / jitter, bitmap stamps, the Win32 tablet path, and the 0.1.0 release gates among them.

## Try it

```bash
# the app: viewport, layer stack, Bakes panel, Export dialog
cargo run -p umber-app

# headless: bake every P0 mesh map from a mesh, seam-padded
cargo run -p umber-cli -- bake-all sword.obj out/ --size 1024 --rays 16 --dilate 16

# headless: pack through an engine preset (DirectX normals for Unreal)
cargo run -p umber-cli -- export sword.obj out/ --preset unreal

# drive the app's paint engine over JSONL (one step per line, one reply per step)
printf '%s\n' '"version"' '"quit"' | cargo run -p umber-cli -- agent
```

GPU work needs a Vulkan (Linux) or DX12 (Windows) adapter; `mesa-vulkan-drivers` (lavapipe) works for headless use. If a driver crashes at startup, the next launch steps down to a safer backend automatically; `--safe-gpu` or `WGPU_BACKEND` force one.

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
│   ├── specs/         # design docs per subsystem
│   └── research/      # cited research (Painter inventory, competitors, absorption ledgers)
├── scripts/           # the local battery, CI helpers, the crate layering gate
└── crates/            # Rust workspace — 16 crates in a layered dependency graph
```

## Contributing

Code contributions are open. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) (setup, the exact CI gates, commit format), then [`STATE.md`](STATE.md) and [`docs/ROADMAP.md`](docs/ROADMAP.md) for what to pick up. Security reports go through [`SECURITY.md`](SECURITY.md).

## Credit

Built by [Bessa](https://github.com/victorbessa96) and Razul, with research claws.
