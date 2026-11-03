# Kill report: app-side control channel v1 (queue row 153, steps 1-2) — 2026-11-03

Implements rollout steps 1-2 of
docs/loop/briefs/2026-11-02-app-control-channel-design.md under the
no-cargo envelope (rustfmt/docs/git/pure-python only; tests pinned UNRUN).

## What landed (`b67fbf8`)

- `umber_core::control` (new, L1, std + thiserror/blake3/serde_json only):
  `TOKEN_ENV`, `MAX_REQUEST_BYTES`/`MAX_CONNECTIONS`/`MAX_BATCH_STEPS`,
  `ControlError`, `validate_token`, `generate_token` (`/dev/urandom` on
  Unix, blake3 process-entropy mix elsewhere), `token_file(prefix, port)`,
  `write_token_file` (0600 Unix), `server_token`, `client_token`,
  `token_matches` (constant-time), `LineRead`/`read_bounded_line`,
  `ConnectionLimiter`/`Permit`, `bind_loopback` (127.0.0.1 ONLY, no
  interface argument exists), `write_json_line`. 7 pinned tests.
  Deliberately NO getrandom edge in core: Cargo.lock untouched (verified
  empty diff), which is why generation is urandom+blake3 rather than the
  CLI's getrandom path.
- `umber-cli/src/security.rs`: now thin anyhow wrappers over
  `umber_core::control` with prefix `umber-control`; public API identical
  (`TOKEN_ENV`, MAX_*, `token_file`, `generate_token`, `client_token`,
  `server_token`, `token_matches`, `LineRead`, `read_bounded_line`,
  `ConnectionLimiter`, `Permit`), so serve.rs/mcp.rs are untouched. The
  CLI's getrandom dep stays declared (removing it would stale Cargo.lock
  under --locked); prune at the next battery. CLI's 3 pinned tests
  unchanged, now exercising the wrappers.
- `umber-app/src/control.rs` (new): `parse_control_arg` (bare port only;
  `0.0.0.0:8377` / `[::1]:8377` / non-numeric refused), `ControlServer`,
  `start(port)` — bind_loopback, server_token with prefix
  `umber-app-control`, named listener thread, per-connection threads under
  the limiter. Wire: `{"id","method","params"}` in, `{"id","ok","result"|"error"}`
  out; first request MUST be auth (constant-time compare); then
  ping/version/methods; anything else (incl. the future
  viewport.screenshot) gets an honest `unknown_method`. 4 pinned tests
  over a real loopback socket (bad-token reject + close, auth-first,
  full method sweep, bare-port parsing).
- `umber-app/src/main.rs`: `mod control;`, `UmberApp.control:
  Option<ControlServer>` (keeps the server alive for the app lifetime),
  `--control <port>` parsed next to `--safe-gpu` (bad value exits 2),
  server started in the run_native creator (now `move`); explicit
  `--control` start failure exits 1 with the reason on stderr; startup
  line prints the bound port and where the token lives.

## Verification (envelope: no cargo, no tests)

- rustfmt --edition 2021 --check CLEAN x5 (core control.rs, core lib.rs,
  cli security.rs, app control.rs, app main.rs).
- scripts/check-layers.py GREEN (16 crates) — core gained no workspace
  edges, so no layer movement.
- `git diff` on Cargo.lock + every Cargo.toml: EMPTY (no new deps, lock
  not stale under --locked).
- Static reference audit: every `security::*` use in serve.rs/mcp.rs
  resolves to a re-exported/wrapped item (grep-verified, list in commit).
- 14 pinned tests written, UNRUN until the next battery-capable wave
  (Bessa's no-cargo envelope 2026-11-01, reaffirmed 2026-11-02/03).

## Security posture

Loopback-only by construction (no interface argument; bind is hardcoded
127.0.0.1), token auth before any dispatch, 0600 token file, bounded
request lines (8 MiB cap), connection cap 16, constant-time token
compare, unknown methods rejected. v1 is read-only: ping/version/methods
touch no app state.

## Named follow-ups

- Step 3 viewport.screenshot: egui frame pixels at end_pass ->
  umber_export::png::write_png -> base64, captured on the UI thread and
  ferried through an mpsc queue to the listener thread (the queue + drain
  in `fn ui` is step 3's core). PNG golden regenerates at the battery.
- Step 4 ui.inspect (panel list, active tool, brush params, layer census
  as JSON; kittest-pinnable).
- Step 5 --bridge: spawn `umber-cli mcp`, relay MCP frames verbatim.
- Row 152: `hermes mcp add umber ...` after the next fresh build.
- Battery: cargo test (core control 7 + cli security 3 + app control 4
  + full suites) + clippy -D warnings + CI on the pushed tip.
