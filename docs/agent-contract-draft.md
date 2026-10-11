# The Agent Working Contract (draft → CLAUDE.md / AGENTS.md)

Umber is a Rust 3D texture-painting DCC (11 crates, wgpu 30 + egui/eframe 0.36).
This file is the STANDING BRIEF for every coding agent working in this repo —
Claude Code, OpenCode, or any other claw. Read it once at session start; it
replaces the per-task boilerplate.

## The workspace

- `crates/umber-core` — the document model (layers, undo, journal, project I/O). Dependency-minimal: no GPU, no graph crate.
- `crates/umber-brush` — the stroke pipeline (conditioning, spacing, adapters, retouch).
- `crates/umber-gpu` — wgpu: renderer, paint passes, bakers, graph eval, postfx, display LUT. Never holds a `wgpu::Surface` (eframe owns it).
- `crates/umber-graph` — the node graph: topo, eval (CPU reference), nodes, mtlx. `umber-gpu::graph_eval` is the WGSL arm.
- `crates/umber-mesh` — loaders (glTF/FBX/OBJ/usda/usdc), raycast, fill, polygon fill.
- `crates/umber-bake`, `umber-export`, `umber-color`, `umber-cli`, `umber-app`, `umber-gui` (the GUI foundation crate), `umber-wasm`/`umber-plugin-sdk`.
- Assets: `assets/i18n/{en-US,pt-BR}.ftl` (Fluent catalogs — EVERY user-facing string goes through `tr()` in BOTH catalogs).

## Hard rules

1. **Code-only era**: NO `cargo build`, NO `cargo test` runs from agents. `cargo clippy --workspace --all-targets`, `cargo fmt`, and `cargo check` ARE allowed and EXPECTED before you finish (they're the local gate — run them on YOUR files' crates). CI (GitHub Actions, both platforms) is the canonical runtime gate.
2. **Never commit.** You write files; the dragon (the orchestrator session) reviews, gates, and commits. Never `git add`/`commit`/`push`/`stash`/`checkout` unless your brief says the tree is yours alone.
3. **No fabrication.** If you can't verify something, say so in your report. A named gap beats a plausible claim.
4. **Read the vendored source for the API you call** (`~/.cargo/registry/src/*/`), especially wgpu 30 and egui 0.36 — both changed shapes recently (e.g. `PipelineLayoutDescriptor { bind_group_layouts: &[Option<&Bgl>], immediate_size }`, `DroppedFile::path()` is a trait method). Do not guess signatures.
5. **Tests must be able to FAIL** (Pitfall 15): exact-value pins derived from the arithmetic where possible; range checks only for genuinely implementation-defined behavior, with the why in a comment.
6. **Every judgment call is stated in the module docs** — the constants, the v1 boundaries, the deliberately-deferred. The next agent reads those before extending.
7. **Extend, don't revert**: the tree may carry concurrent agents' work. If a file you didn't write changed, work WITH it.
8. **Mark your requirements row** in the same change: if your task completes a row in `docs/specs/requirements.md`, append ` ✓ built (<will-be-commit>)` to that row.

## The recurring Rust traps (each burned a CI round — check before finishing)

- **thiserror's `source` field trap**: a struct-variant field named `source` becomes the error SOURCE; non-`Error` types break the derive. Name it `source_id`.
- **`matches!` takes exactly 2 args** — a third "message" is a macro syntax error.
- **Doc-comment attachment**: a blank line between `///` blocks can make clippy attach the first doc to the second item. Keep doc blocks adjacent to their item.
- **Negated float comparisons** (`!(x > 0.0)`): use `!x.is_finite() || x <= 0.0` (preserves NaN-skip semantics).
- **Structs cannot live inside `impl` blocks** — module scope only.
- **`Option<&mut T>` params need `mut` bindings** for `as_deref_mut`/`as_mut`.
- **A reference's fields are already borrowed** — `&dispatch.field` on a `&T` makes `&&`. Use `dispatch.field` directly.
- **8-arg functions fail clippy** — bundle params into a context struct (state your judgment call).
- **Derivable `Default`s fail clippy** — but TUNED defaults (non-zero/non-first values) stay manual; `#[default]` on the variant.

## The tooling (run these, they're the gate)

- `bash scripts/verify-kill.sh <files...>` — the trap gate: brace balance, fmt auto-align, the trap greps. Run on your files before finishing.
- `bash scripts/pre-push.sh` — the dragon's commit gate (same classes, on the commit's own files).
- `cargo clippy -p <your-crate> --all-targets` — THE gate. One local pass saves a 5-minute CI round-trip.
- `cargo fmt` before finishing. Always.

## The requirements + specs

- `docs/specs/requirements.md` — the tier table (P0/P1/P2). P0 and P1 are COMPLETE (built + marked or honestly boundary'd). P2 is the remaining tier.
- Design docs live in `docs/specs/*.md` — read the one covering your surface before writing code.
- `docs/specs/gui-audit-and-roadmap.md` — the GUI plan (Phase 0a done: `umber-gui` exists; 0b = wire it into the app).
- `STATE.md` — the handoff note between sessions. Read it first.

## Report shape (when you finish)

End your report with: what you built (files + test counts), every judgment call you made, what you could NOT verify (egui signatures you couldn't confirm, behavior that needs a GPU run), and what you deliberately deferred. The dragon reads exactly this.
