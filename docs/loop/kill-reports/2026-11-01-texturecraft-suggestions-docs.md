# Kill report — texturecraft suggestions write-down (2026-11-01, docs-only)

Session: local-20261101_tcabsorb. Lock: `tc-absorb-suggestions`.
Docs-only pass per the code-only envelope — no cargo, no push (rides
the next wave push with the wave-1 battery).

## Written down

- `docs/loop/queue.md` rows 151-157: export-padding DilationMap wiring ·
  Hermes MCP registration of `umber-cli mcp` (exact `hermes mcp add`
  stanza; debug binary is 2026-10-30 and predates `mcp`/`serve`, so
  registration waits for the next build) · app-side `--control <port>` +
  MCP `--bridge` · CommandSpec command-registry unification (own design
  wave) · rmcp/tokio + cap-std upgrades (client-invisible) · engine rows
  (EyeDepth, UV G-buffer, COW undo eval, blend helpers, parity table,
  theme matrix, packaging legs, mesh-importer hostile sweep) · standing
  re-sweep of storytold/texturecraft at each wave close.
- `docs/ROADMAP.md` Scripting/automation row now notes the MCP server +
  control channel landed (`3331bcb`, unpushed).
- `.dragon/COORDINATION-next-wave.md` STATE block: new Updated line.
- Vault: `projects/umber/2026-11-01-texturecraft-absorption.md` +
  index link in `umber-project-INDEX.md`.
- Constraints file: status note appended.

## Not done (by envelope, not by choice)

- No cargo: wave-1's 17 pinned tests + battery + clippy + CI remain
  owed at the next sanctioned wave push.
- No Hermes MCP registration yet: requires a fresh umber-cli binary
  carrying `mcp`/`serve` (queued as row 152 with the stanza).

## Pointers

Ledger: `docs/research/absorption-2026-11-01-texturecraft.md`.
Wave-1 kill report: `docs/loop/kill-reports/2026-11-01-texturecraft-absorb-wave1.md`.
