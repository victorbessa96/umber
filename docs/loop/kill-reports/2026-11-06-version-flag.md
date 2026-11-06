# Kill report — `--version` on both binaries (installers gap #1, row 247)

- Date: 2026-11-06 ~17:1x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-version-flag` (core/cli/app main files, queue.md, COORDINATION,
  this report).
- Gap closed: installers smoke (row 240, kill report
  `2026-11-06-installers-smoke.md`) gap #1 — "the `--version` assertion
  FAILED because neither binary has a `--version` flag".

## The change

- `umber_core::control::is_version_arg(a)` — one predicate
  (`--version` | `-V`), shared by both binaries; pin
  `control::tests::version_flag_predicate` (accepts both flags, rejects
  `--verbose`, a subcommand, empty).
- `crates/umber-cli/src/main.rs`: match arm
  `Some(other) if umber_core::control::is_version_arg(other)` placed
  BEFORE the unknown-command fallback, printing
  `umber-cli v{CARGO_PKG_VERSION}`; the bare-invocation usage text
  gains a `--version | -V` line.
- `crates/umber-app/src/main.rs`: right after the `gc` headless lane,
  `args().nth(1)` checked with the predicate → print `umber v{...}` and
  `exit(0)` before any window/GPU work (headless-safe).

## Verification (all live, this session)

- Built dev binaries, run directly:
  - `./target/debug/umber-cli --version` → `umber-cli v0.1.0`
  - `./target/debug/umber-cli -V` → `umber-cli v0.1.0`
  - `./target/debug/umber --version` → `umber v0.1.0` (no window opened;
    the two INFO lines are env_logger/i18n init, which precede the check)
  - `./target/debug/umber -V` → `umber v0.1.0`
  - `./target/debug/umber-cli --bogus` → still
    `Error: unknown command: --bogus` + usage (predicate does not
    swallow real flags).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p umber-core -p umber-cli -p umber-app --all-targets -- -D warnings`: 0 errors.
- `cargo test -p umber-core -p umber-cli -p umber-app --all-targets`
  (flock, nice 19, -j3, log /tmp/umber-version-battery.log): 19 suites
  all `test result: ok`, 0 failed — core 185/0 (incl. the new pin), app
  lib 253/0 + bin 753/0 + integration suites, cli suites green.

## Not done / next

- Remaining installers gaps from row 240: #2 exe-relative runtime assets
  (icon atlas/env/starter missing on a clean install), #4 cli bench
  49k-tri-over-4096 always fails, #5 no-display launch poisons the GPU
  fallback to GL permanently. #3 (v0.1.0 tag so the release workflow
  runs) stays partner-gated.
