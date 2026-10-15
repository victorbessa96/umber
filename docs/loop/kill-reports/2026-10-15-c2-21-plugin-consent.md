# Kill report: C2-21 plugin consent + module-size cap

Date: 2026-10-15. Author: dragon. Lock: `c2-21-plugin-consent` over
`crates/umber-app/src/{plugins,graph_panel}.rs`, `crates/umber-app/Cargo.toml`,
`crates/umber-wasm/src/{runtime,lib}.rs`, `crates/umber-wasm/tests/size_cap.rs`,
`SECURITY.md` and this report. Source: `docs/loop/audit-2026-10-15-c2.md`
row 21.

## The gap

Every launch compiled and registered every `*.wasm` in the user plugins
dir (`~/.local/share/umber/plugins`), with no consent. The sandbox is
strong: no host imports, fuel, a 64 MiB memory cap, a fresh store per
call. But "a file someone dropped in a dir runs on next launch" is the
classic shape of an unwanted-code foothold. Nothing bounded the module
size either, and compilation is the one host-side cost a module controls
before any fuel is metered.

## Built

- **Consent bound to bytes** (`plugins.rs`):
  - `TrustStore` reads `plugins-trusted.txt`, a sibling of the plugins
    dir, with one `<blake3 hex> <file name>` line per trust. Only the hash
    is matched.
  - The scan hashes each file from the SAME buffer it would compile, so
    there is no gap for the file to be swapped between check and use.
  - An untrusted file is reported in `PluginLoadReport::pending` and is
    never compiled.
  - Changing one byte revokes trust.
  - `load_trusted_file` re-reads the file and refuses it if the hash
    moved since it was listed.
  - Trust is written with write-then-rename. Control characters are
    stripped from the display name, so a crafted file name cannot forge
    a second line. Uppercase or non-64-char hashes are not accepted.
  - The in-memory trust is added only AFTER the file is saved, so a
    failed save never trusts silently.
  - The debug-build `examples/` dir (the repo's own reference plugins)
    is implicitly trusted.
- **Graph panel**: one row per pending plugin showing its path, the first
  12 hex digits of its hash, and **Trust & load**. Loading registers the
  plugin and its `.mtlx` declaration immediately. The status line gains
  `· N awaiting trust`. `GraphPanel::load_plugins(dirs)` keeps its
  signature (main.rs is under a sibling lock), and the panel opens the
  trust store itself.
- **`MODULE_SIZE_CAP` = 8 MiB** (`umber-wasm`, re-exported). The runtime
  refuses an over-cap buffer before `Module::new`. The app refuses an
  over-cap FILE by its metadata before reading it, so a multi-gigabyte
  `.wasm` never lands in memory. The reference plugins are under 4 KiB.
- `SECURITY.md`'s plugin row now states consent and the cap.
- `blake3` (already in the workspace) is added to umber-app's
  dependencies. The Cargo.lock delta is one line, produced by cargo on the
  build host and verified to be only that line.

## Tests (5 new)

- `an_untrusted_user_plugin_is_pending_and_never_registers`:
  1. Pending with the exact-bytes hash, not registered, status line says so.
  2. Trust through a real file, then load.
  3. A reopened store trusts it on a fresh scan.
  4. One appended byte puts it back to pending, and the stale "trust &
     load" refuses the swapped file.
- `the_trust_file_rejects_junk_and_keeps_one_line_per_entry`: junk,
  uppercase and short hashes are rejected; a newline in the name is
  stripped; trusting again is idempotent.
- `an_oversized_module_is_refused_before_it_is_read`: a sparse file over
  the cap fails and is never offered for trust.
- `size_cap.rs` (umber-wasm): over the cap is refused by size; exactly at
  the cap reaches wasmtime's own validation (the cap is inclusive).
- Mutation: removing the trust check fails the consent test at its
  `loaded.is_empty()` assert.
- The existing loader tests pass through an explicit trust-everything
  store, so their tolerance and collision assertions are unchanged.
- Gate 1: umber-app 477, umber-wasm 33 + 2. fmt and clippy clean.

## Judgment calls

- **Hash, not path.** A path allow-list would trust whatever is later
  written to that path. A hash binds the reviewed bytes.
- **No signature scheme.** Signing needs a key distribution story (the
  plugin SDK guide, C2-27). Consent plus the sandbox is the v1 bar, and
  `SECURITY.md` claims no more.
- **Strings are untranslated** ("Trust & load", "awaiting trust") until
  the catalogs, under the sibling's lock, take consent keys.
- **No revoke UI.** To revoke, delete the line from
  `plugins-trusted.txt`. A Preferences list of trusted plugins is the
  follow-up once g3-preferences lands.
