# umber — State

> Updated every wave. This file is the bridge across context windows. Read this first on any new session before deciding what to do.
>
> ARCHITECTURE NOTE: this file was rewritten 2026-10-12 (A1 of the core-loop
> queue) from CURRENT TRUTH after the strategy review found it
> self-contradictory (status line said "wave 3 of 6" while the handoff said
> everything through wave 6 was built; Oct-9 overnight-grant language was
> still live). The wave-history narrative lives in CHANGELOG.md and
> DECISIONS.md; this file carries ONE current picture.

## Status line

Current phase: **CORE-LOOP (post-strategy-review)** — the product spine
(the six criticals on the one user path that matters) until Gate 0 passes.
State: CORE-LOOP-IN-FLIGHT
GUI chrome + all P2 work: FROZEN pending Gate 0 (Bessa lifts the freeze).
SPEC.md: v1.0 DRAFT — not ratified; Bessa's ratification is a release gate.
No tag, no release, no v0.1 claim until Gate 0 passes.

## The core-loop queue (the ONLY active hunt)

The strategy review (docs/specs/2026-10-12-strategy-review.md, accepted in
full by the dragon 2026-10-12, d3516c6) found six critical gaps on the user
path. Queue truth and per-item state live in docs/loop/queue.md (single
source of loop truth). Summary:

- B1 viewport texture binding (critical 1) — DONE (e802f92 + 9455885)
- B2 4K live target + per-layer/per-channel routing (criticals 2+6) —
  IN FLIGHT (tile pool + compositor landed; the live paint target is still
  TARGET_SIZE=512, paint_state.rs:233 — the 4K wiring is the open gate)
- B3 stroke undo journal (critical 3) — DONE (f78e6c1; smudge test
  stacking contract pinned bed9e0a)
- B4 pixel save/load (critical 4) — DONE (d834d77 + c999da6)
- B5 pen pressure end-to-end (critical 5) — TODO (begin_stroke hardcodes
  1.0; stylus crate has zero uses in umber-app; Linux first)
- B6 Gate 0 journey test — TODO (FBX → 4K pressure paint → lit 3D →
  undo/redo → save/reopen → export decode-verified)

Phase A (truth pass) is partial: A0's row corrections landed (d3516c6);
A1 (this rewrite), A2 (roadmap reconciliation), A3 (docs sweep) close
with the phase.

## Machine-load directive (standing)

One cargo command at a time, no gate stacking, never chained after other
commands in the same shell line — stacked full-parallelism gates crashed
Hermes (2026-10-12, Bessa's order). Local cargo budget: `cargo fmt --all`,
`cargo clippy -p <crate> --all-targets -- -D warnings` per touched crate,
ONE at a time. No local cargo build/test — CI (ubuntu+windows) is the
runtime gate. Remote gate (centralhost 192.168.15.64, 16-core/31GB, RTX
3070) is the sanctioned heavy-build path when reachable.

## Claw dispatch recipe (proven)

Vendor dependency sources into docs/claw-artifacts/<crate>/ + verified-API
facts README before dispatch. opencode: brief points at the artifacts.
claude: --allowedTools with cargo read/write allowlist. Cross-review by the
OTHER claw. Never trust claw self-reports: verify from artifacts (fmt/
clippy/test + behavioral probes) before committing. Claws never commit.

**Claw lessons (both claws hit sandbox walls, both rescued the same way):**
headless opencode cannot read the cargo registry (auto-rejects
external_directory); headless claude cannot WebFetch/cargo-doc without
pre-approved allowlists. Vendor the dep sources and pass --allowedTools.

## Blocked / waiting

- B5's Windows Ink path: Windows-first CI gate applies; Linux path is
  the queue item.
- Remote build gate (centralhost) unreachable as of 2026-10-12 ~02:50
  (ping + ssh dead). CI is the runtime gate until it returns.
- Push-gate approval for wave pushes rides Bessa's word (Telegram or
  in-session); the loop never flips it for itself.

## Handoff note (what the next session must know)

1. **Read docs/loop/loop-prompt.md first** — it is the loop contract
   (rules, queue, iteration protocol, failure modes). Read
   docs/loop/queue.md second — per-item state lives there.
2. The four criticals landed as a commit chain in one evening
   (e802f92 → bed9e0a). The CI red streak on their wake (5 reds:
   compositor pin d1aeeb3-fixed, then the smudge test's stale one-entry
   premise f78e6c1→9e4fb9e→bed9e0a) is closed: bed9e0a pushed, the
   stacking contract (entries stack, one per stroke — sibling test
   two_paint_strokes_undo_reverts_only_the_latest pins it) is the truth.
3. B2 is the open kill: tile pool (1MiB/tile, 64MiB/layer-channel at 4K)
   + compositor exist in umber-gpu, but the live paint target is still
   512² (paint_state.rs:233 TARGET_SIZE). Wire 4K as the live target +
   per-layer channel routing; the memory model is already prototyped in
   the tilepool kill report (/tmp/umber-tilepool-*.json).
4. The smudge-test lesson generalizes: when a feature changes a
   cross-cutting invariant (every stroke = one history entry), grep the
   test suite for every consumer of the OLD invariant before marking done.
5. Kill-locks before dispatch (scripts/kill-locks.sh add|check|remove|list)
   — no live locks; zero claws running as of this write.
6. 599+ tests green on both platforms pre-core-loop-phase; expect the
   count to move as B2/B5/B6 land.

## History pointers (audit trail, preserved deliberately)

- docs/specs/2026-10-09-full-project-audit.md — the full-project audit
- docs/specs/2026-10-12-strategy-review.md — the adversarial review that
  re-cut the roadmap around the six criticals (claude-opus, preserved
  verbatim + the dragon's disposition)
- DECISIONS.md — append-only, all-time decision log
- CHANGELOG.md — the wave-by-wave build history (waves 0-6 + frontier)
- Public repo: https://github.com/victorbessa96/umber (old patina URL 301s)
- Rename trail: Patina → Umber (2026-10-09, DECISIONS.md [02:41]; Bessa
  can revert — full trail there)
