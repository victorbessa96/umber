# Third-Party Licenses — Dependency Manifest

> Wave-1 merge gate (SPEC.md Constraints): every dependency's license is
> recorded here before first claw-authored merge to master. Updated
> whenever `Cargo.lock` changes materially. Generated + verified with
> `cargo license` / `cargo about`; hand-audited for the load-bearing ones.

## Application license

Umber is GPL-3.0-or-later (LICENSE). Standalone library crates may be MIT
where upstream licenses permit relicensing.

## Fonts embedded in every binary

egui's default fonts are compiled in (`epaint_default_fonts` 0.36.2, via
eframe's `default_fonts`): Ubuntu Light (Ubuntu Font Licence 1.0), Hack
(MIT + Bitstream Vera + DejaVu public domain), Noto Emoji (OFL-1.1) and
emoji-icon-font (MIT). These licences require their texts to accompany
every copy: `LICENSES/THIRD_PARTY_FONTS.txt` carries them verbatim, and
every release archive includes it (`dist-workspace.toml` `include`).

## The whole tree, by licence (2026-10-15)

`Cargo.lock` holds 636 packages, 620 from crates.io, read from each
crate's own manifest in the local registry (enumn 0.1.14 was not
downloaded locally; MIT OR Apache-2.0 per crates.io). All are
permissive or offer a permissive choice. The ones worth naming:

| Crate | Licence | Why it is fine |
|---|---|---|
| colored 2.2.0 | MPL-2.0 | test-only: reached via `dify` ← egui_kittest's `snapshot` feature, a dev-dependency, so it is never in a shipped binary (MPL-2.0 is GPL-3.0 compatible regardless) |
| self_cell 1.3.0 | Apache-2.0 OR GPL-2.0-only | Umber takes Apache-2.0 |
| ittapi / ittapi-sys 0.4.0 | GPL-2.0-only OR BSD-3-Clause | Umber takes BSD-3-Clause |
| r-efi 5.3.0 / 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | Umber takes MIT |
| epaint_default_fonts 0.36.2 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | notices shipped (section above) |

`cargo-audit` (advisory DB 2026-10-14): 0 vulnerabilities; 2
unmaintained warnings, both transitive (`paste` via egui_dock + pulp,
`ttf-parser` via owned_ttf_parser), with no action on our side short of
upstream.

## Direct dependencies (Wave 1)

| Crate | Version | License | GPL-3.0 compatible | Notes |
|---|---|---|---|---|
| wgpu | 30.x | MIT OR Apache-2.0 | ✅ | GPU abstraction |
| naga | 30.x | MIT OR Apache-2.0 | ✅ | shader IR |
| eframe / egui | 0.36 | MIT OR Apache-2.0 | ✅ | app shell (bundles rfd: MIT/Apache) |
| egui_dock | 0.21 | MIT OR Apache-2.0 | ✅ | dockable panels |
| glam | 0.34 | MIT OR Apache-2.0 | ✅ | math |
| gltf | 1.x | MIT OR Apache-2.0 | ✅ | glTF import |
| tobj | 4.x | MIT | ✅ | OBJ import |
| ufbx | 0.11 | MIT | ✅ | FBX import (C lib, official bindings) |
| thiserror / anyhow | 2 / 1 | MIT OR Apache-2.0 | ✅ | errors |
| rfd | 0.15.4 | MIT | ✅ | native file dialogs (umber-app; added Wave 1 — recorded after the fact, gate audit caught it) |
| egui-wgpu / epaint | 0.36 | MIT OR Apache-2.0 | ✅ | egui wgpu bindings (umber-gpu, claw pass) |
| windows | 0.62 (optional, target-gated to Windows) | MIT OR Apache-2.0 | ✅ | Win32 WM_POINTER/pen API for the stylus Windows Ink backend |
| bytemuck | 1.x | MIT OR Zlib OR Apache-2.0 | ✅ | pod casting for GPU buffers |
| pollster | 1.x | MIT OR Apache-2.0 | ✅ | adapter blocking in gpu-feature tests (optional dep) |
| log / env_logger | 0.4 / 0.11 | MIT OR Apache-2.0 | ✅ | logging |
| rayon / parking_lot / crossbeam | (W2+) | MIT OR Apache-2.0 | ✅ | concurrency |
| fluent (fluent-bundle, fluent-syntax, intl-memoizer, unic-langid) | 0.17 | Apache-2.0 OR MIT | ✅ | UI string catalogs (umber-app; wave-6 i18n slice) |

## Reimplementation (not derivative) sources

| Upstream | License | Relationship |
|---|---|---|
| libmypaint | LGPL-2.1-or-later | **Documented-semantics reimplementation only** — we port the published brush model (inputs/curves/dab spacing) from its documentation and papers; no code is copied. SPEC.md Constraints. |
| OpenPBR spec / openpbr-bsdf | Apache-2.0 (ASWF) | Reference used for the WGSL über-shader port. |

## Deferred dependencies (later waves — pre-audited)

| Crate | Wave | License | Risk note |
|---|---|---|---|
| ocio-rs / ocio-sys (vendored OCIO 2.5) | 2 | BSD-3-Clause (OCIO) | single-maintainer bindings; LUT fallback pre-designed |
| image / exr / half | 2 | MIT OR Apache-2.0 | imaging |
| wasmtime + WASI 0.3 | 6 | Apache-2.0 WITH LLVM-exception | plugin runtime |
| openusd | 5 | MIT (crate) — verify against Pixar USD (Apache-2.0) at adoption | pre-1.0 |
| octotablet (fork/absorb) | 2 | MIT OR Apache-2.0 — **verify at absorption; relicensing path requires it** | stylus layer |
| quick-xml | 4 | MIT | .mtlx parsing |

## Hard-walled (never)

- Adobe .sbsar / Materials SDK — closed terms, no FOSS path
- Autodesk FBX SDK — proprietary, non-redistributable (ufbx is the route)
