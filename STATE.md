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

Current phase: **CORE-LOOP — CLOSED (Gate 0 PASSED 2026-10-12, run 38255484123,
both platforms green).** The user path is whole end to end; the next phase's
queue comes from the C1 audit's refreshed worklist. GUI/P2 unfreeze is
Bessa's explicit call.
State: GATE-0-PASSED-PHASE-CLOSED
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
  DONE v1 (133dd12: DEFAULT_TARGET_SIZE=4096, Texture Sets size combo,
  target_size.txt sidecar; per-set/per-channel sizing = named follow-up)
- B3 stroke undo journal (critical 3) — DONE (f78e6c1; smudge test
  stacking contract pinned bed9e0a)
- B4 pixel save/load (critical 4) — DONE (d834d77 + c999da6)
- B5 pen pressure end-to-end (critical 5) — DONE (f867215 pressured API +
  shaping + can-fail test; 6430623 the source fix: raw `Event::Touch`
  force, single pens included — the multi_touch 2+ gate never carried a
  pen, cross-review finding fixed same-night; stylus-crate backends =
  named follow-up)
- B6 Gate 0 journey test — DONE IN CODE (15c072e: the six-leg journey
  test landed dragon-verified; umber-app gains a lib target for it; the
  compositor's WGSL isNan corpse fixed — no such builtin in naga 30's
  front, verified). Runtime verdict = CI at push; the phase closes on
  that green.

Phase A (truth pass): COMPLETE — A0 d3516c6, A1 e116729 (this rewrite),
A2+A3 3e0278f; queue.md records the evidence per item.

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
