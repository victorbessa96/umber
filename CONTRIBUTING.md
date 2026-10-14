# Contributing to Umber

Thanks for helping build the community's Substance Painter alternative.

**Current stage:** the core loop is real and CI-proven (Gate 0: FBX in →
4K pressured paint → lit 3D viewport → undo/redo → save/reopen → engine
export, decode-verified on Linux and Windows). Code contributions are
open. Where things stand right now: [`STATE.md`](STATE.md). What comes
next: [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Ways to help

1. **Paint with it and report what hurts.** A bug report with a mesh and
   the exact steps is worth more than a feature idea.
2. **Pick a roadmap row.** Every open row in `docs/ROADMAP.md` names what
   it closes; the design docs it depends on live in `docs/specs/`.
3. **Correct the research.** `docs/research/` carries cited claims about
   Substance Painter, competitors and the Rust ecosystem. A correction
   with a better source is welcome (use the research-correction template).
4. **Docs.** Always welcome, no setup needed.

## Ground rules

- **Rust only** for the core. FFI only where no viable crate exists (OCIO,
  ufbx) and the license is compatible.
- **GPL-3.0-or-later.** By contributing you agree your work is licensed
  under GPL-3.0-or-later. Standalone library crates may be released MIT.
- **Original implementations only.** No Adobe SDKs, no `.sbsar`
  execution, no writing `.spp`. Reading user-provided `.spp` files is
  allowed (clean-room, from published community layout docs — see
  `crates/spp`). No decompilation of Painter. Ideas are not
  copyrightable; code is.
- **Never crash in the engine crates.** `umber-core`, `umber-color` and
  `umber-spp` deny `unwrap`/`expect`/`panic` outside tests. Return typed
  errors.
- **Decisions are written down.** Architectural calls land in
  `DECISIONS.md` (append-only). Disagree with one? Open an issue rather
  than editing the log.
- **Be excellent to each other** — see
  [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## Development setup

```bash
git clone https://github.com/victorbessa96/umber.git
cd umber
cargo build --workspace          # toolchain is pinned in rust-toolchain.toml
cargo run -p umber-app           # the app
cargo run -p umber-cli -- --help # the headless CLI (bake / export / batch / agent)
```

GPU tests need a Vulkan (Linux) or DX12 (Windows) adapter. Without a
GPU, install a software one — CI uses the same:

```bash
sudo apt-get install -y mesa-vulkan-drivers libvulkan1   # lavapipe
```

Windows CI uses WARP.

## Before you open a PR

CI runs these exact gates on Linux and Windows; run them first:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p umber-gpu --features gpu   # the GPU golden family
```

Then:

- **Tests must be able to fail.** Pin exact values derived from the
  arithmetic; use a range only for implementation-defined behavior (GPU
  rounding, dab-edge coverage) and say why in a comment.
- **Strings the user sees go through `tr("group.key")`** with an entry in
  both `assets/i18n/en-US.ftl` and `pt-BR.ftl` (a test pins key parity).
- **Docs move with the code.** If your change makes a doc wrong, fix the
  doc in the same PR. A user-visible change gets a `CHANGELOG.md` line.
- **Read the trap list.** [`CLAUDE.md`](CLAUDE.md) (same content as
  `AGENTS.md`) lists the Rust and clippy traps that have each cost a CI
  round on this codebase, plus the workspace map.

## Commit messages

```
<area>: <what changed, imperative, under ~70 chars>

Why the change exists and what it does, wrapped at ~72 columns.
Name what you verified and what you could not.
```

`<area>` is the crate or surface (`umber-paint`, `agent`, `docs`,
`viewport`, `ci`). One logical change per commit.

## Where things live

| Path | What |
|---|---|
| `SPEC.md` | the contract — what Umber is and is not |
| `STATE.md` | current state, read first |
| `docs/ROADMAP.md` | the plan of record |
| `docs/specs/` | design docs per subsystem (read the one for your surface) |
| `docs/research/` | cited research behind the requirements |
| `DECISIONS.md` | the append-only decision log |
| `CLAUDE.md` / `AGENTS.md` | workspace map, trap list, contributor + agent rules |
| `crates/` | the Rust workspace |
