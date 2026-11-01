# Absorption ledger — storytold/texturecraft (2026-11-01)

Bessa's brief: evaluate <https://github.com/storytold/texturecraft>,
compare it against Umber, absorb what is beneficial, and suggest.
Source cloned depth-1 at `e01dac2` (scratch, removed after the pass).
License: **MIT OR Apache-2.0** — absorbable into Umber's
GPL-3.0-or-later with notices retained (THIRD_PARTY.md "Absorbed
source files"). Brand/trademark assets excluded (upstream requires
forks to remove them).

Verdicts: **landed** (this pass) · **row** (queued with evidence) ·
**have** (Umber already has it — do not re-propose) · **no**
(declined, with why).

## What texturecraft is

Pure-Rust Substance-Painter-class 3D texture painter, 9 lib crates +
3 apps + xtask, edition 2024, egui 0.37 (their own fork pins), wgpu 30.
L0 core/texel/geom → L1 compose/bake → L2 paint/formats → L3
engine/render → L4 automation (MCP + JSONL RPC + bridge) → L5
ui-egui. ~93% self-assessed Painter parity; 86 commits, 3 stars,
active. Its distinguishing ideas: **every action is a command**
(`CommandSpec` registry; menus/CLI/control/MCP all dispatch by id),
**agent-native surfaces** (MCP via rmcp, loopback token-auth control
channel, `serve` JSONL), **capability-sandboxed file roots**, COW
sparse 64×64 u16 texel tiles + multi-source BFS UV padding
(`DilationMap`), CPU compositor as the blend oracle, 32 blend modes,
GPU bake + path-traced render, UDIM, export presets/templates with
channel packing, 9-language i18n, full packaging matrix
(MSI/AppImage+zsync/flatpak/deb/rpm/dmg/FreeBSD/web zip + SHA256SUMS).

## Landed in this pass (wave 1, code-only envelope)

| Item | From | What landed |
|---|---|---|
| UV padding engine | `crates/texel/src/dilate.rs` | `crates/umber-core/src/uv_dilation.rs` — `DilationMap` multi-source BFS (Chebyshev, `max_dist`, per-block borders for UDIM atlases), `apply`/`source`, reference-BFS property test ported. Edition-2021 adaptation (let-chains rewritten), attribution header kept. Fills the gap `umber-export/src/presets.rs:393` literally names ("the padding rule before dilation lands"). |
| Control-channel security | `crates/automation/src/security.rs` | `crates/umber-cli/src/security.rs` — token gen (getrandom 0.3, already locked), 0600 token file, `UMBER_CONTROL_TOKEN`, constant-time compare, 8 MiB bounded line reads, connection limiter. Ported tests. |
| File confinement for automation | `crates/automation/src/workspace.rs` | `crates/umber-cli/src/workspace.rs` — `AuthorizedWorkspace` with `--read-root`/`--write-root`; upstream used cap-std handles, umber's loaders are path-based so confinement is std-only canonicalize+prefix (symlink/junction escapes fail; TOCTOU window documented as accepted for a loopback token-auth tool). `relative_path` shape validation ported verbatim (backslash/absolute/drive/device/control-char rejects). `confine_step` rewrites path fields of any agent step. Ported tests incl. symlink escape. |
| MCP server | `crates/automation/src/server.rs` + `docs/control-protocol.md` | `crates/umber-cli/src/mcp.rs` — `umber-cli mcp [--read-root DIR] [--write-root DIR]`: hand-rolled MCP over stdio (newline-delimited JSON-RPC 2.0; `initialize`/`ping`/`tools/list`/`tools/call`; protocol 2025-06-18) with **zero new protocol deps** (no rmcp/schemars/tokio). 17 tools over the existing agent session: version, session_inspect, run_step, run_steps (≤256, stop-at-first-error), load_mesh, project_open/save, bake, bake_status, export, paint_stroke/read/snapshot, set_active, undo/redo, paint_undo/redo. Unit tests cover initialize/tools/version/run_steps failure/escape rejection. |
| Loopback JSONL server | `crates/automation/src/rpc.rs` | `crates/umber-cli/src/serve.rs` — `umber-cli serve --port N`: 127.0.0.1-only TCP, auth-first token handshake, `ping`/`methods`/`step`/`steps`, one session per connection, bounded framing + connection cap from security.rs. Unit tests cover auth-first, bad token close, methods, step, confinement, malformed line. |
| Reuse patch | — | `agent.rs`: `Session` + `reply` are now `pub(crate)` (behavior unchanged; the 2026-10-14 protocol tests still pin it). `main.rs` wires `mcp`/`serve`. `umber-cli/Cargo.toml` gains `getrandom = "0.3"` (0.3.4 already in Cargo.lock; promote to workspace deps at wave close). |

## Became queue rows (need a cargo battery to land safely)

| Item | From | Umber today (verified) | Effort |
|---|---|---|---|
| Export padding via DilationMap | texel `dilate.rs` | module landed; consumer wiring pending: export driver computes `covered[]` from UV islands via umber-mesh's `polygon_fill`, then `DilationMap::apply` on each output map; replaces the transparent-black pre-dilation rule in `presets.rs` pack_texel path | S |
| App-side `--control <port>` channel + MCP `--bridge` | apps/texturecraft `--control`, `automation/src/bridge.rs` | landed pieces: security.rs + serve.rs give the server half over umber-cli; the app half (loopback listener inside umber-app, token file, `viewport.screenshot`/`ui.inspect`, bridge forwarding in mcp.rs) is umber-app work | M |
| rmcp-based MCP upgrade | server.rs | hand-rolled wire landed (zero-dep); swap internals to rmcp 3.5 + schemars + tokio once a battery verifies; keep the tool catalog identical so clients don't notice | S |
| Unified command registry (every menu action = `CommandSpec{id,label,menu,shortcut,params,enabled,run}`; agent/CLI/menus dispatch by id; `command.list`/`session.inspect` for agents) | engine `commands.rs` | umber-app has an action registry (g5 viewport presets, propmenu catalogs 829/829) but CLI/agent use the separate step vocabulary; unifying gives agents `command.list` and menus-as-commands | L |
| Camera-visibility stroke mask (`EyeDepth`, backface culling, angle fade) | paint `visibility.rs`, `raster.rs` | umber paints UV-view strokes with the seam graph (`umber_mesh::seam`); OccluderScope exists for thickness bake only; no per-stroke eye-depth buffer in 3D strokes | M |
| UV G-buffer (texel→triangle barycentrics; one source of truth for paint/composite/masks/bake/padding) | geom `gbuffer.rs` | umber-mesh has bvh/raycast/seam/fill but no texel→surface G-buffer; would unify stroke rasterization, masks, padding | L |
| COW snapshot undo for pixels (clone = Arc bump per tile) | texel `tile.rs` + engine Session::edit | umber undoes via per-op journal commands (`undo.rs`, PaintStrokeCommand) — memory grows with ops, not pixels; evaluate COW for 8K sets | M |
| Tracer abstraction (CPU oracle vs GPU baker behind one trait) | bake `rays.rs` | umber-bake has a `gpu` feature + umber-gpu bake shaders; check the trait seam is as clean | S |
| Profile tuning: dev deps opt-level 2, release hot-crate opt 3, wasm-release profile | root Cargo.toml profiles | umber has release lto-thin + dev-deps opt 1 (debug usability) and no wasm app build (umber-wasm is the plugin runtime, not a web app) — measure with the perf-nightly A/B ratio gate before touching | S |
| Blend menu catalog helpers (`MENU` order, `label`/`short`/`parse`, `is_non_separable`) | core `blend.rs` | umber's `BlendMode` (~28 variants incl. NormalCombine/NormalDetail) + `blend_channel`/`blend_rgb` already match tc's 32-mode set; only the UI helper fns are missing | S |
| Generated parity table (`docs/parity.md` --check in CI) | `cargo xtask parity` | umber has scorecard.py --check (counts) + requirements.md rows; a generated Painter-menu live/missing table is the next step | S |
| i18n beyond en-US/pt-BR (de/es/fr/it/ja/zh/ko) | ui-egui localization + `localization-parity.md` | umber has ftl catalogs + parity test (en-US/pt-BR); tc itself flags "no native-speaker review" — extend after native review | M |
| Theme matrix (4 brightness × 7 accents) | ui-egui | umber has g0 theme live-reload (`UMBER_THEME_FILE`) + token overrides; a fixed brightness/accent matrix is a panel-level add | S |
| Packaging legs: AppImage zsync self-update, deb/rpm, FreeBSD, windows-arm64, packaging-lint.yml, SHA256SUMS on artifacts | packaging/, .github/workflows | umber ships via cargo-dist (dist-workspace.toml: installers + checksums) + appimage/ + flatpak/ (archive-type fix landed); zsync update metadata + arm64/FreeBSD legs + lint job are the deltas | M |
| Mesh exporters (obj/glb/usda/usdc/usdz with textures) | formats `mesh_export` | umber-export writes usd/mtlx + textures; verify glb/obj round-trip coverage against tc's list | S |
| Hostile-input test sweep for mesh importers in panic_hunt | AGENTS.md golden rules | `umber-app/tests/panic_hunt.rs` covers mtlx/PSD/.spp/projects but NOT fbx/gltf/usdc parsers (same gap photocraft ledger named 10-14) | S |
| Decompression-bomb preflights on every decoder | texel/formats limits | EXR/PNG/PSD limits landed 10-14; extend to the mesh importers + .umber project reader | S |

## Already have (do not re-propose)

| Item | Where in Umber |
|---|---|
| Crate layering gate (cargo-free, CI) | `scripts/check-layers.py` (absorbed 2026-10-14 from photocraft's xtask layers) — 16 crates GREEN 2026-11-01 |
| Never-crash lint gate (deny unwrap/expect/panic, engines) | crate-root `cfg_attr(not(test), deny(...))` + clippy.toml; new modules scanned clean 2026-11-01 |
| JSONL agent session (the MCP-adjacent core) | `umber-cli agent` — `crates/umber-cli/src/agent.rs` (load/state/bake/export/project_*/layer_*/paint_* steps, undo journals, paint sidecar) |
| Offscreen snapshot harness | `paint_snapshot` step + `umber-paint/src/snapshot.rs` |
| Bake map set (AO, curvature, thickness, world normal, ID, normal, position, skew, transfer) + margin/dilation | `crates/umber-bake/` — superset of tc's bake list |
| UDIM tiles | `umber-core/src/tiles.rs`, `umber-mesh/src/udim.rs`, `umber-export` `$udim` token |
| Export presets + filename templates + channel packing | `crates/umber-export/src/{presets,driver}.rs` |
| Blend modes + non-separable handling | `umber-core/src/layers.rs` BlendMode + blend_channel/blend_rgb |
| i18n with parity test | `crates/umber-app/src/i18n.rs` + en-US/pt-BR.ftl |
| Nightly perf with A/B ratio gate | `docs/perf/protocol.md`, `.github/workflows/perf-nightly.yml`, `scripts/perf_compare_ab.py` (tc has nothing like it — umber advantage) |
| SECURITY.md threat table, CONTRIBUTING, scorecard --check, docs-only PR classifier | landed 2026-10-14 pass |
| Lossless crash-safe project format | `.umber` dir + sidecars (hash-named, blake3) — arguably stronger than tc's `.tcraft` zip+lz4 |
| Mesh import breadth (glTF/FBX via ufbx/USD/USDC/OBJ) + read-only .spp | `umber-mesh`, `umber-spp` (clean-room, hard-walled) |
| GPU viewport + compositor + paint compute | `umber-gpu` (wgpu 30, tile pool, compositor), `umber-paint` (PaintThread mpsc, compute splat) |

## Declined

| Item | Why |
|---|---|
| `[patch.crates-io]` to storytold's egui/winit forks + craft-libs git deps | pins umber's UI stack to a third party's fork; umber runs egui 0.36 + crates.io winit deliberately |
| vendored `egui-phosphor` fork | built for egui 0.37; umber is on 0.36 — pattern only, revisit at an egui bump |
| rmcp/schemars/tokio for MCP now | hand-rolled wire landed with zero deps; swap internals at a battery-verified wave, keeping the catalog stable |
| cap-std Dir handles | std canonicalize+prefix confinement covers umber's path-based loaders; cap-std is the hardened upgrade if the surface ever faces a hostile local user |
| `.tcraft` zip+lz4 container | `.umber` + sidecars is the format of record (crash-safe, hash-named); no migration value |
| ArtCraft brand assets / app icon / screenshots | trademark; forks must remove them |
| Workspace-wide `unsafe_code = "forbid"` | `crates/stylus` needs unsafe for X11/XI2 FFI — umber's per-crate never-crash pins are the right shape |
| tc's 9-language catalogs verbatim | no native-speaker review upstream either; extend umber's ftl catalogs deliberately |
| One CARGO_TARGET_DIR per agent | umber serializes cargo through the flock by Bessa's order (photocraft ledger, stands) |

## Suggestions for Bessa

1. **Drive umber from Hermes over MCP now**: `umber-cli mcp --read-root <dir> --write-root <dir>` speaks MCP on stdio; Hermes' native MCP client can register it and paint/bake/export through tools. The app-side `--control` + `--bridge` row closes the loop for driving the live GUI later.
2. **Land the export-padding wiring next** — `uv_dilation` is in core; the consumer is a small driver change, and it kills the last transparent-black padding wart in `presets.rs`.
3. **Keep the hand-rolled MCP wire until rmcp earns it** — zero-dep means the agent surface can never break on a protocol SDK bump; the rmcp swap is queued and client-invisible.
4. **The command-registry unification is the biggest structural prize** (tc's `CommandSpec`): it would make menus, shortcuts, CLI, agent and MCP one dispatch table. It's an L-effort umber-app refactor — schedule it as its own wave, not inside a feature kill.
5. **Perf profile tuning belongs behind the A/B ratio gate** — the perf-nightly instrument wave (landed 10-30) can now measure tc-style hot-crate opt-levels safely.
6. **Watch the repo**: 3 stars but 86 commits and 93% parity claim in weeks — if ArtCraft keeps this pace, re-sweep at each umber wave close; the research/absorption ledger pattern is now proven twice (photocraft 10-14, texturecraft 11-01).
