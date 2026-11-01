# Kill report — texturecraft absorption wave 1 (2026-11-01)

Session: local-20261101_tcabsorb. Lock: `tc-absorb-wave1` (15 files,
disjoint from the live sibling `g4-assign-shortcut` lane). Envelope:
code-only per the STATE block (wakeup #7/#8 answers stand) — NO cargo;
static verification only; battery + clippy + pinned tests + CI owed at
the next sanctioned wave push.

## Landed

- `crates/umber-core/src/uv_dilation.rs` (NEW) + `lib.rs` wiring —
  `DilationMap` ported from storytold/texturecraft `crates/texel/src/dilate.rs`
  @ e01dac2 (MIT OR Apache-2.0, © 2026 ArtCraft Team — header retained,
  THIRD_PARTY.md row added). Edition-2021 adaptation: `apply`'s let-chains
  rewritten. 5 tests ported (incl. reference-BFS property test).
- `crates/umber-cli/src/security.rs` (NEW) — token gen/validation/file,
  constant-time compare, bounded line reads, connection limiter. Ported
  from `crates/automation/src/security.rs`. 3 tests. getrandom 0.3
  (0.3.4 already in Cargo.lock — no lock change needed).
- `crates/umber-cli/src/workspace.rs` (NEW) — `AuthorizedWorkspace`
  read/write roots, std-only canonicalize+prefix confinement (upstream's
  cap-std replaced; TOCTOU note in module docs), `relative_path` shape
  validation verbatim, `confine_step` path-field rewriter. 5 tests.
- `crates/umber-cli/src/mcp.rs` (NEW) — `umber-cli mcp`: hand-rolled MCP
  over stdio (JSON-RPC 2.0 newline-delimited; initialize/ping/tools/list/
  tools/call; protocol 2025-06-18), 17 tools over the agent Session,
  root-confined paths. Zero new protocol deps. 5 tests.
- `crates/umber-cli/src/serve.rs` (NEW) — `umber-cli serve --port N`:
  loopback-only token-auth TCP JSONL, one session per connection,
  `ping`/`methods`/`step`/`steps`. 4 tests.
- `crates/umber-cli/src/agent.rs` — `Session` and `Session::reply` are
  `pub(crate)` now (two-line visibility change; protocol behavior
  unchanged, existing tests still pin it).
- `crates/umber-cli/src/main.rs` — `mod` decls + `mcp`/`serve` arms +
  usage text.
- `crates/umber-cli/Cargo.toml` — `getrandom = "0.3"` direct dep (promote
  to workspace deps at wave close).
- `THIRD_PARTY.md` — "Absorbed source files" section + deliberate
  non-absorptions list.
- `docs/research/absorption-2026-11-01-texturecraft.md` — full
  evaluation ledger (landed / row / have / no + suggestions).

## Verification (static, per envelope)

- rustfmt --edition 2021 --check: FMT-OK on all 8 touched .rs files
  (5 new files formatted in place; pre-existing files untouched by the
  formatter).
- `python3 scripts/check-layers.py`: GREEN (16 crates) — new umber-cli
  edges are external-only (getrandom), no layer violation.
- Never-crash scan: zero `unwrap()`/`expect(`/`panic!` outside
  `#[cfg(test)]` in uv_dilation.rs (umber-core deny holds) and in the
  four new umber-cli modules.
- rustfmt's full parse = syntax-valid on every new file.
- NOT run (envelope): cargo check/clippy/test, CI. The 17 ported/new
  unit tests are pinned and UNRUN until the next sanctioned battery.
  Cargo.lock needs no update (getrandom 0.3.4 already locked).

## Open tails (queued, see the ledger's "row" table)

- Wire `DilationMap` into the export driver's padding step (replaces the
  transparent-black pre-dilation rule in `umber-export/src/presets.rs`).
- App-side `--control <port>` + MCP `--bridge` (umber-app lane).
- rmcp swap for the MCP wire (client-invisible), command-registry
  unification, EyeDepth visibility, UvGBuffer, profile tuning behind the
  perf A/B gate, packaging legs (zsync/deb/rpm/arm64/FreeBSD/lint).

## Not absorbed (deliberate)

ArtCraft brand/trademark assets, craft-libs git pins, storytold
egui/winit fork patches, vendored egui-phosphor (egui 0.37), rmcp/
schemars/tokio now, `.tcraft` container format, workspace-wide
unsafe-forbid (stylus needs unsafe), tc's unreviewed translations.
