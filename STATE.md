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

Current phase: **POST-POOL MARCH (2026-10-15).** The per-layer pool arc
is complete (stages 1a-5, 2026-10-14). Since then: the paint engine is a
headless crate (`umber-paint`, PR #8), and the JSONL agent session
paints through it (`paint_stroke` / `paint_read` / `paint_snapshot` /
undo, a `version` step, exact wire strokes); the photocraft + Blender
absorption pass (`docs/research/absorption-2026-10-14-photocraft-blender.md`)
landed falloff curves + front-face falloff (with brush-panel controls),
the Adjacent Faces bake margin (CLI + the Bakes panel combo), crash-safe GPU startup, save-format
completeness, the crate layering gate and the docs-only CI path filter;
the mesh-importer panic hunt found 24 crash bugs and hardening closed
23 (`.usdz` loads for the first time; the open one is an openusd
allocation abort, upstream). Stroke methods and the cavity mask now
reach the brush (51d9047, 4bebc72). The GUI plan
(`docs/specs/gui-plan-2026-10-15.md`) is the active worklist (queue item
23): G0 foundations, most of G1-G3, the G4 ramp / ValueField grammar /
curve widget, the G4 property menu on every value field, the G5 menus
(Recent, Revert, Window, a real About), Hide UI / Maximize and the
workspaces, the G6 display-mode keys, and the shared kittest support
module landed 2026-10-15, two sessions in parallel under kill locks, with
per-kill state in the plan's tables. Waves are large and push once
(Bessa 2026-10-15). Next: the rest of the GUI plan (G3 keymap editor, G4
color picker, G5 home screen / first run / tab overflow, G6 overlays /
contextual toolbar / UV parity, G7), ROADMAP 8b's
remaining rows (stencil, bitmap stamps → angle/jitter), the Win32 tablet
path, and the 0.1.0 gates.
State: POST-POOL-MARCH
GUI/P2 freeze: LIFTED (Bessa's standing directive 2026-10-12 — nothing in this project waits on him).
SPEC.md: v1.0 DRAFT — not ratified; Bessa's ratification is a release gate (v1.1 amendment drafted: docs/specs/spec-v1.1-amendment-draft.md).
No tag, no release, no v0.1 claim until the 0.1.0 checklist's gates pass.
Workspace: 16 crates, 1,600+ tests (the `#[test]` census), CI on Linux + Windows.

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

## Machine-load + wave model (standing)

One cargo command at a time, no gate stacking, never chained after
other commands in the same shell line — stacked full-parallelism gates
crashed Hermes (2026-10-12, Bessa's order). Local cargo now runs under
the flock at /tmp/umber-cargo.lock.

TWO-GATE WAVES (Bessa's ruling 2026-10-14, replacing per-push CI):
Gate 1 = scripts/wave-battery.sh per kill (fmt + clippy -D warnings +
full suites, local, minutes — this gate catches what CI catches);
Gate 2 = ONE push per wave + ONE CI run (scripts/ci-watch.sh
backgrounded with a notify pattern; the verdict pings between turns).
Kills batch as commits unpushed between gates; nothing idles on a
foreground sleep. Proven Waves 1-4: every real bug caught by Gate 1.

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
- Pushes are autonomous: push-gate.json is STANDING approval (Bessa's
  directive: keep it true, push when needed).

## Handoff note (what the next session must know)

1. **Read docs/loop/loop-prompt.md first** — it is the loop contract
   (rules, queue, iteration protocol, failure modes). Read
   docs/loop/queue.md second — per-item state lives there.
2. The criticals landed as a commit chain in one evening and the CI red
   streak on their wake is closed: bed9e0a (the smudge stacking-contract
   fix — the sibling test two_paint_strokes_undo_reverts_only_the_latest
   pins the contract) went green on both platforms, and the wave
   continued green through f867215 (pressure API), 6430623 (the raw
   Event::Touch source fix), and 15c072e (Gate 0 itself).
3. B2 landed v1 (133dd12): DEFAULT_TARGET_SIZE=4096,
   with_target_size for pinned test geometry, the Texture Sets size
   combo, target_size.txt sidecar persistence — per-set/per-channel
   sizing remains the named follow-up (C1 worklist #4).
4. The smudge-test lesson generalizes: when a feature changes a
   cross-cutting invariant (every stroke = one history entry), grep the
   test suite for every consumer of the OLD invariant before marking done.
5. Kill-locks before dispatch (scripts/kill-locks.sh add|check|remove|list)
   — no live locks; zero claws running as of this write. Dragon-loop wake
   and leisure sessions write into the shared main tree too: a hold set
   in a live session (e.g. "no cargo") must be written into any queued
   hunt's goal, or a wake session will not see it.
6. Test counts: 1,600+ `#[test]`s across 16 crates (2026-10-15 census;
   599+ at the core-loop close). The C1 audit's Pass 2 verified zero
   stale-premise tests then; the 2026-10-15 truth pass found two stale
   requirements MARKS instead (brush angle / alignment / jitter were
   marked built and are not — rows 15 and 154 corrected).

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
