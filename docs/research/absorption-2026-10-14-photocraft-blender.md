# Absorption ledger — photocraft + Blender (2026-10-14)

Bessa's brief: "absorb everything that you see as a positive for this
project, be it in features, capacities or documentation/flow/process".
Two research subagents surveyed
[storytold/photocraft](https://github.com/storytold/photocraft) and
Blender (its manual and developer handbook). Every candidate below was
then checked against Umber's own code; the **Umber today** column
records that check. Candidates Umber already has are recorded too (as
`have`), so nobody re-proposes them.

Already absorbed from photocraft before this pass (excluded): the
never-crash lint gate, the control protocol / agent session, the
offscreen snapshot harness, the importer corpus pattern.

Verdicts: **landed** (in this pass) · **row** (became a ROADMAP row) ·
**have** (Umber already has it) · **no** (declined, with why).

## Landed in this pass

| Item | From | What landed |
|---|---|---|
| Crate layering gate | photocraft `xtask/src/layers.rs` | `scripts/check-layers.py` — every crate in a declared layer L0–L6, deps point down, UI crates (egui/eframe/winit/rfd/egui_dock) only in umber-gpu (egui-wgpu callback trait) / umber-gui / umber-app; an unregistered new crate fails. Pure Python (no cargo): a CI `gates` job + the battery's first step. Negative-tested: upward edge, UI leak into umber-paint, unregistered crate, dev-dep reaching up — all four caught. |
| Decompression-bomb limit on EXR read | photocraft `crates/codecs/tests/limits.rs` | `read_exr_rgba_f32` refused nothing: a crafted data window reached `vec![…; w]; h]` directly (the HDR env-map path, `umber-app/src/env.rs`). Now a header preflight (`exr::meta::MetaData`) refuses > 16K per side or > 2 GiB decoded, before any pixel allocation, with a typed `ExrError::TooLarge`. PNG (`DECODE_LIMIT_BYTES` 1 GiB) and PSD (`MAX_PSD_SIDE`/`MAX_PSD_BYTES`) already had limits. |
| Threat surface in SECURITY.md | photocraft `book/src/security/threat-model.md` | SECURITY.md was "Wave 0"; it now carries the threat table (meshes, PSD, images, .spp, MaterialX, projects, WASM plugins, agent protocol) with each surface's verified defence — and names the mesh importers' panic-hunt gap. |
| Contributor docs | photocraft `AGENTS.md`; Blender handbook (commit messages, bug reports) | CONTRIBUTING.md rewritten (was "Wave 0 — code PRs parked"): setup incl. lavapipe, the exact CI gates, test honesty rules, i18n rule, a commit-message format (Blender's subject/body/72-col shape). Bug template asks for build, GPU+driver, `RUST_LOG`/`RUST_BACKTRACE` output, a minimal repro mesh/project (Blender's "simple .blend" rule). Stale "Patina" removed from the templates. |

## Became ROADMAP rows

| Item | From | Umber today (verified) | Effort |
|---|---|---|---|
| **Bake margin: Adjacent Faces** | Blender baking manual (Margin Type: Extend / Adjacent Faces) | `umber-bake/src/dilation.rs` extends border pixels only (`dilate_map`) — no cross-seam fill; the seam graph (`umber_mesh::seam`) the brush already uses is the data an adjacent-faces margin needs | M |
| **Brush falloff curve presets + custom curve** | Blender brush falloff (Smooth/Sphere/Root/Sharp/Linear/Constant/Custom…) | `BrushParams` carries a single `hardness: f32` (`umber-brush/src/dab_adapter.rs`) — one radial profile | S |
| **Front-face / normal falloff** | Blender brush falloff (Normal Falloff + angle) | no view-angle attenuation in the 3D stroke path — grazing faces take full-strength, stretched dabs | S |
| **Stroke methods: airbrush, anchored, drag-dot, dash** | Blender stroke manual | spacing, line constraint, path/polygon exist; no airbrush (rate while held), no anchored/drag-dot (the decal-placement idioms), no dash ratio/length (stitches) | M |
| **Stamp angle (fixed / rake / random) + size/opacity/position jitter; stencil overlay** | Blender brush texture manual | `BrushParams` is color/alpha/hardness/pressure-gamma/wash-mode only: no per-dab rotation and **no jitter** (`brush_panel.rs` module docs confirm a kill brief named jitter/angle and found neither — an earlier summary claiming jitter was wrong); no screen-space stencil to paint through | M |
| **Brush-level cavity mask** | Blender texture-paint mask settings | curvature bakes (`umber-bake`) and layer masks exist; no per-brush "mask by cavity" toggle reusing the baked curvature | S |
| **Object-space normal bake** | Blender baking (Normal Space: Object/Tangent) | tangent-space only (`umber-bake/src/normal_map.rs`); DX/GL Y-flip at export already **have** (`NormalConvention`, `a1490ad`) | S |
| **GPU-startup crash marker + backend fallback** | photocraft `apps/photocraft/src/gpu_startup.rs` | nothing — a driver segfault at device creation repeats every launch; the marker (written before device creation, cleared after first frames) steps Vulkan→GL/DX12 next launch; `--safe-gpu` override | M |
| **Mesh importers in the panic-hunt** | photocraft codec limits + this pass's threat table | FBX / glTF / USD/USDC parsers have no garbage coverage (`umber-app/tests/panic_hunt.rs` covers mtlx/PSD/.spp/projects) | S |
| **Save-format completeness by exhaustive destructuring** | photocraft `crates/format/src/convert.rs` | to verify per-converter; the pattern (no `..` in the doc→disk destructure) makes a new field a compile error until saved | S |
| **Generated, CI-checked scorecard** | photocraft `xtask/src/scorecard.rs` | README/CONTRIBUTING/bug-template drift found in this very pass (crate + test counts, stage, name) — the headline numbers should be generated + `--check`ed | M |
| **Path-filtered Windows leg** | photocraft `ci.yml` `platforms` job | every PR runs both legs; docs-only PRs could skip the Windows build (or a `full-ci` label forces it) | S |
| **Release phases (alpha → beta protocol freeze → RC)** | Blender release cycle | the 0.1.0 checklist exists (`docs/specs/release-checklist-0.1.0.md`); a beta that freezes the agent protocol + file format gives integrators the Blender-API-freeze guarantee | S |

## Already have (do not re-propose)

| Item | From | Where in Umber |
|---|---|---|
| Autosave + crash recovery | photocraft Autosaver | the journal (`umber_core::journal`, requirements §5 `015fbf0`) |
| DX/GL normal Y-flip | Blender swizzle | `NormalConvention` per export preset |
| Nightly perf with a baseline | photocraft perf-nightly; Blender benchmark framework | `docs/perf/protocol.md` + the i2p/pool harnesses; budget breach blocks, slowdown records |
| i18n key coverage | photocraft `i18n_coverage.rs` | `i18n::missing_keys` + the en-US/pt-BR parity test |
| Test data in-repo, filtered runs | Blender test setup | fixtures under `crates/*/tests/fixtures`; `cargo test -p` |

## Declined

| Item | From | Why |
|---|---|---|
| Quick Edit (external-editor round trip) | Blender texture-paint options | real value, but it needs camera-projection painting first (not built); revisit after projection lands |
| One CARGO_TARGET_DIR per agent | photocraft AGENTS.md §5 | Umber serializes cargo through `flock /tmp/umber-cargo.lock` by Bessa's order — separate target dirs would multiply disk and defeat the deliberate one-cargo-at-a-time rule |
| `@bot build` CI-on-demand | Blender PR conventions | Umber's two-gate model already runs one CI per wave |
| PR 3-day reply SLA, WIP prefix | Blender PR conventions | single-maintainer repo today; revisit with outside contributors |
| Packaging lint + sha256-verified tool downloads | photocraft packaging-lint | worth it at the first release build; logged here, not a row until packaging CI runs on tags |
