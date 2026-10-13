# Umber — agent contract

Rust 3D texture-painting DCC. 14-crate workspace, wgpu 30 + egui/eframe 0.36, toolchain pinned 1.97.0.
Applies to every coding agent (Claude Code, OpenCode, …). Hard rules first, then traps, tooling, pointers.

## Hard rules

1. **No `cargo build`, no `cargo test`.** CI (ubuntu + windows) is the runtime gate.
   Before finishing you MUST run, one cargo command at a time (never parallel — the machine has crashed under stacked builds):
   - `cargo fmt --all`
   - `cargo clippy -p <crate> --all-targets -- -D warnings` for every crate you touched (`-D warnings` is what CI uses; without it you pass locally and fail CI).
2. **Never commit.** No `git add/commit/push/stash/checkout/reset`. The orchestrator ("the dragon") reviews, gates and commits. The exception is a brief that explicitly gives you the tree.
3. **Extend, don't revert.** Other agents edit the tree concurrently. If a file you didn't write changed, build on it, and never "clean up" someone else's hunk. Concurrent kills own DISJOINT file surfaces: the dragon registers each kill's files via `bash scripts/kill-locks.sh add <name> <files…>` and refuses any dispatch whose surface intersects a live lock.
4. **Don't guess dependency APIs.** Read the source first: `docs/claw-artifacts/<crate>/` (vendored, with a verified-facts README), then `~/.cargo/registry/src/*/`.
   Shapes that already burned rounds:
   - wgpu 30 `PipelineLayoutDescriptor { bind_group_layouts: &[Option<&Bgl>], immediate_size }`
   - egui 0.36 `DroppedFile::path()` is a **method**
   - eframe 0.36 has no surface format/colorspace hook
   - `umber-gpu` never owns a `wgpu::Surface` (eframe does).
5. **Tests must be able to fail.** Pin exact values derived from the arithmetic. Use range checks only for implementation-defined behavior, and give the reason in a comment.
6. **Dead code is resolved by wiring the consumer, not by deleting it** (or `#[allow(dead_code)]`). If you can't wire it, say so in the report.
7. **User-facing strings go through `tr("group.key")`** (`crates/umber-app/src/i18n.rs`).
   - Add the key to `assets/i18n/en-US.ftl`; the completeness test fails if it's missing.
   - Also add it to `pt-BR.ftl`. This is house convention; pt-BR otherwise falls back to en-US.
   - Fluent ids can't contain dots, so `tr("menu.open-mesh")` maps to message `menu`, attribute `.open-mesh`. Use kebab-case attributes under an existing group header.
8. **State judgment calls in the module docs**: constants, v1 boundaries, deliberate deferrals. The next agent reads those before extending.
9. **Mark your requirements row.** If your task completes a row in `docs/specs/requirements.md`, add a 4th cell: `` | ✓ built (`PENDING`) — <v1 boundary, if any> | ``. The dragon fills in the sha.
10. **Hard walls**: no .sbsar execution, no Adobe SDKs, no WRITING .spp. `.spp` IMPORT is ratified (binding ruling 2026-10-12): read-only through `crates/spp` (umber-spp), clean-room per `docs/research/spp-hdf5-layout.md` — never a write path. License stays GPL-3.0-or-later.
11. **Never crash in the engine crates** (the never-crash gate, 2026-10-13): `umber-core`, `umber-color`, `umber-spp` deny `unwrap`/`expect`/`panic` in non-test code (`cfg_attr(not(test), deny(...))` at the crate root + `clippy.toml`). Return typed errors; a provably-infallible case needs `#[allow(clippy::expect_used)]` + the reason. `umber-gpu`/`umber-app` are exempt (harness + UI code), but the discipline is the same there.

## Rust traps (each one cost a CI round; check your diff for all of them)

Compile errors:
- **thiserror `source` field.** A struct-variant field named `source` becomes the error source, and non-`Error` types break the derive. Rename it (`source_id`).
- **`matches!` takes exactly 2 args.** A trailing message string is a syntax error.
- **No `struct`/`enum` inside `impl` blocks.** Module scope only.
- **`Option<&mut T>` params need `mut` bindings** for `as_deref_mut()`/`as_mut()` (E0596).
- **Fields of a `&T` are already reachable.** `&dispatch.field` gives `&&`. Use `dispatch.field`, or clone out of a shared ref instead of moving (E0507).
- **No field access in format strings.** `"{ask.name}"` doesn't compile. Bind `let name = ask.name;` first.
- **New module or dep:** exactly one `mod x;` line (a duplicate is E0428), and add the dep to that crate's `Cargo.toml` (E0432).
- **Borrow splits (E0502):** take `&mut self.a` and `&self.b` separately. Don't call `&mut self` methods while holding a borrow.
- **`Path::ends_with` matches whole components**, not string suffixes: `Path::from("Sword_baseColor.png").ends_with("baseColor.png")` is FALSE. Suffix-match via `file_name().and_then(|n| n.to_str())`. (Cost: an hour of probe archaeology on a working export.)
- **A closure returning `&str` borrowed from its arg** can't feed `.iter().find()` — the borrow can't outlive the call. Return an owned `String`.
- **`std::env::temp_dir()` under the agent harness is `~/.hermes/cache/scratch`**, not `/tmp` — CI-style `/tmp` assumptions don't hold locally; failed-run leftovers accumulate there and are evidence.

Clippy `-D warnings`:
- **Crate-root deny lints beat clippy.toml's allowances.** A plain `#![deny(clippy::unwrap_used)]` also binds TEST code (clippy.toml's `allow-unwrap-in-tests` can't override a crate-level deny). The working shape is `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]` — see umber-core/umber-color/umber-spp's lib.rs.
- **Too many arguments**: >7 params fails. Bundle them into a context struct and state the call.
- **Derivable `Default`**: use `#[derive(Default)]`, with `#[default]` on the enum variant. Tuned defaults (non-zero, non-first) stay manual.
- **`neg_cmp_op_on_partial_ord`**:
  - The exact rewrite of `!(x > 0.0)` is `x.is_nan() || x <= 0.0`.
  - The repo also uses `!x.is_finite() || x <= 0.0`, which additionally skips +∞. Use it only when that's intended.
- **Doc-comment attachment**: no blank line between `///` blocks. Keep each doc block directly above its item.
- **Doc lazy continuation**: indent continuation lines of a `///` list item to the item's text.
- **`map_or(true, …)` → `is_none_or(…)`**, NOT `is_some_and`: an is_some_and rewrite flips the absent-arm default. If the absent arm is `true`, the semantic is "absent ⇒ enabled" — preserve it. (Cost: a silent behavior flip that starved every auto-rebake fire.)
- **Redundant field shorthand**: `Foo { x }`, not `Foo { x: x }`.
- **Needless or elided lifetimes**, and the `?` rewrite (`question_mark`).
- **Unused `#[must_use]` results**: acknowledge them with `let _ =`, or use them.
- **`new()` without `Default`**: add the `Default` impl.
- **Redundant match guards.**
- **Platform-gated code** (`stylus` `winink` feature, `cfg(windows)`) is only compiled on Windows CI. Linux clippy won't see it, so read it twice.

## Tooling

| Command | When | Notes |
|---|---|---|
| `cargo fmt --all --check` | before every push | CI runs this exact gate; a miss is a 4-minute round-trip for nothing. |
| `bash scripts/verify-kill.sh <files…>` | before finishing, on every file you touched | **Rewrites files in place** (rustfmt). Hard-fails on brace imbalance and fmt only. The thiserror, `matches!` and doc-attachment checks are WARN; treat WARN as fail unless it's a false positive you can name. Safe on `.ftl` (fmt is skipped). |
| `cargo clippy -p <crate> --all-targets -- -D warnings` | before finishing, per touched crate | Matches CI's clippy flags. Use `cargo check -p <crate>` for faster iteration. |
| `bash scripts/pre-push.sh` | dragon only | Gates HEAD~1 plus staged files; useless on an uncommitted tree. It's the only gate with the neg-cmp and derivable-Default greps, so check those by hand. |

Other scripts (`brief-template.sh`, `ci-consume.sh`, `loop-status.sh`, `perf_compare.py`) are orchestrator tools. Don't run them.

## Workspace map

| Crate | Owns |
|---|---|
| `umber-core` | document model: layers, undo, journal, `.umber` project I/O. No GPU or graph deps. |
| `umber-brush` | stroke pipeline: conditioning, spacing, smoothing, adapters, retouch |
| `umber-gpu` | wgpu: renderer, paint passes, bakers, `graph_eval` (WGSL arm), postfx, display LUT |
| `umber-graph` | node graph: topo, CPU reference eval, nodes, MaterialX I/O |
| `umber-mesh` | loaders (glTF/FBX/OBJ/usda/usdc), raycast, fill, polygon fill |
| `umber-bake` / `umber-export` / `umber-color` | mesh-map bakers + cage + dilation / naming tokens, packing, engine presets, writers / OCIO, display LUT, per-channel CM flags |
| `stylus` | owned tablet-input crate (Windows Ink behind the `winink` feature) |
| `umber-gui` | design-language widgets (Phase 0a). Has no `tr()` calls today; translate in umber-app and pass strings in. |
| `umber-app` | the eframe app: panels, viewport, i18n |
| `umber-cli` | headless CLI, batch recipes, agent JSONL session |
| `umber-wasm` / `umber-plugin-sdk` | wasmtime plugin host / guest SDK. Example plugins live in `examples/` (a separate workspace, excluded). |

**Tests:**
- Inline `#[cfg(test)] mod tests` in the source file is the norm.
- Integration tests exist only in `crates/umber-cli/tests/` and `crates/umber-wasm/tests/` (fixtures under `tests/fixtures/`).
- Mesh test assets are in `crates/umber-mesh/tests/assets/`.

## Pointers (read the one for your surface before writing code)

- **Requirements:** `docs/specs/requirements.md`, the P0/P1/P2 tier table.
  - STATE.md records P0 and P1 as closed, but the ✓ marks lag (early-wave rows are unmarked). Don't treat an unmarked row as open work without checking the code.
  - The remaining **P2** rows:
    - §2 dynamic material layering
    - §5 displacement/tessellation preview
    - §5 custom WGSL user shaders
    - §6 PSD export
    - §10 AI-texturing provider interface
- **Design docs:** `docs/specs/<surface>-design.md`. The filename names the surface (brush-presets, seam-aware-stamping, smudge-clone, udim, i18n, wasm-plugins, cli-batch, …). Non-obvious mappings:
  - node graph → `node-graph-design.md` + `graph-canvas-design.md` + `mtlx-export-design.md`
  - versions and pinning policy → `tech-stack.md`
  - threading and data flow → `architecture.md`
  - perf budgets → `docs/perf/protocol.md`
  - Fill, postfx, HDR and path tools have no design doc; their spec is in the module docs.
- **GUI:** `docs/specs/gui-audit-and-roadmap.md`.
  - Phase 0a is done (`umber-gui` exists; the Brush panel migrated).
  - Next is **0b**: shell chrome, meaning `toolbar.rs` and `status_bar.rs` in umber-app built from umber-gui parts, plus the empty state and the 9 shortcuts.
- **Session handoff:** the "Handoff note" section of `STATE.md`. Its status line at the top is stale; ignore it.
- **Why decisions were made:** `DECISIONS.md` (append-only, don't edit).

## Report (end every task with exactly this)

1. **Built:** files touched, tests added (count each).
2. **Judgment calls:** every one, including the ones already written into module docs.
3. **Unverified:** API signatures you couldn't confirm, and behavior that needs a GPU or runtime run. Name the gap; don't paper over it.
4. **Deferred:** what you deliberately left out, and where it's recorded.
5. **Gates run:** verify-kill, fmt and clippy results, verbatim pass/fail.
