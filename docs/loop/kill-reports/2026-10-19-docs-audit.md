# docs-audit-2026-10-19 — the full docs sweep (stale claims, structure, secrets)

2026-10-19, session local-3d2151f3, Bessa's direct steer ("check
the entirety of the docs... any stale claims, roadmap, work
items... files that need to be removed or deleted, created or
rewritten/reorganized?" + "make sure there's no secret information
being spread in the docs"). Lock: docs-audit-2026-10-19 (the seven
touched files + this report). Doc-only, battery-exempt.

## The secrets verdict (the security-mandated half)

CLEAN, on every axis, across ALL tracked files (git grep at HEAD,
working tree included):

- private key blocks, github/openai/aws/slack token shapes, generic
  `key|secret|password|token = value` assignments, Bearer headers,
  basic-auth URLs, env-file value shapes: ZERO hits;
- the root-password literal: absent from the tree;
- DDNS hostnames (mooo.com / vpn.net.br), the Telegram id: absent;
- `bessa@192.168.15.64` appears in 6 operational files (build-host
  doc, loop-prompt, queue, 4 scripts) — a private RFC1918 LAN
  address plus username, NOT a credential and unreachable from
  outside the LAN. FLAG for Bessa only: if this repo ever goes
  public, that pattern mildly exposes his LAN layout; removal
  would be cosmetic and is his call, not a security defect.

## Changes made

1. **per-layer-pool-design.md — the stale status line** (the
   audit's headline finding): it still read "READY TO IMPLEMENT
   (stages 1b-5). Stage 1a is LANDED" five days after queue row 22
   recorded "done — all five stages landed 2026-10-14, the arc
   closed in cfb205c". Corrected to LANDED + CLOSED with the five
   stage commits, the review artifact, and an audit-provenance
   note — the same closure shape as pcpool/dynmat/C2-9 (this was
   the FOURTH stale-status survivor the sweeps have caught).
2. **LANDING_NOTES_DISPLAY_PANEL.md (root) — removed** as a stale
   diverged duplicate: the canonical note lives in
   crates/umber-app/ beside its two sibling landing notes (the
   code references in display_panel.rs and display_lut.rs resolve
   there); the root copy was the pre-evolution §9-era record.
   A provenance NOTE added to the canonical note names the removal,
   the reason, and the retrievable history commit (146d7a0).
3. **docs/agent-contract-draft.md — removed**: superseded by
   agent-contract-optimized.md (its own header said "draft ->
   CLAUDE.md / AGENTS.md", both of which exist at root), referenced
   nowhere in the tree. History keeps it.
4. **DECISIONS.md <-> docs/loop/decisions.md — cross-referenced**:
   NOT duplicates (two named-scope registries: alignment/mid-
   execution decisions vs the loop's rulings) but a reader of one
   could miss the other. One scope line added to each header
   pointing at the other.

## Deliberate non-changes (checked and kept)

- **docs/claw-artifacts/ (11.7 MB, 390 files)**: vendored claw
  research dumps (wasmtime 5.1M, openusd 4.3M, ag-psd 1.4M...).
  Already in PUSHED history (7a684a8 era) — removing now does not
  shrink the repo; a history rewrite is nuclear and Bessa's call.
  Flagged for a future `git filter-repo` decision if the repo ever
  needs a diet.
- **docs/loop/COORDINATION-*** (3 dated notes), **wave-0-closeout.md,
  merge-story.md, the audit/kill-report history, the stylusxi2
  session-log JSONL** (a deliberate evidence artifact, secrets-clean):
  dated historical records, append-only discipline keeps them.
- **push-gate.json**: current and explicit (Bessa's standing
  approval, quoted).
- **The "pending" greps in specs**: false positives (protocol state
  machines — bake_status pending/running/done, pending edges in
  graph-canvas — not stale claims).
- **stylus LANDING_NOTES.md**: exists, its code reference resolves.

## The sweep's method (reproducible)

Status markers across all specs (grep "^Status:"), stale-phrase
sweeps ("not wired yet", pending, TODO), the full tracked-file
secret-pattern sweep, filename-duplication checks (root vs docs/
vs crates/), reference-resolution checks for every moved-or-
suspect file, and a keep/remove verdict per finding with the
history-preserving git-rm (never rm) for removals.
