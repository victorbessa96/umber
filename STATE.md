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

Current phase: **THE FRESH CHARTER CYCLE — L6 THE UNIFIED TIMELINE
LANDED (2026-10-27, `75c9594`).** Bessa's launch word opened the
cycle ("continue working autonomously... all the claws"); the W-C4
runway's L6 — the graph journal joining the History window as the
timeline's third arm (layers + strokes + graph, interleaved by
op_id, one Ctrl+Z across all three) — landed through the opencode
claw (deepseek-v4.1-flash-free; the claude lane is auth-dead pending
Bessa's re-login), dragon-reviewed across three fix-forward rounds
(the `&mut dyn` elided-object-lifetime trap; the queue test's
fixture lie), battery GREEN: umber-app 873 tests + clippy + the gpu
leg. The 10-24/25 storm before it — the six-lens audit + ROADMAP
refresh, R-B1 flatten, R-C1 Assets browser v1, symmetry persistence,
the fill channel target, the material picker v1, the OccluderScope
row, the pressure curve, L2b, the L3b ring, the pick's toast — all
both-gates-green (the coordination body + kill reports carry the
ledger). The W-C3b sweep and the flatpak five-wall chain stay closed
(history: the 10-23 status era + CHANGELOG).

**2026-10-29 wave (local-4e9bde4c, `ad6f44b`):** g5-viewport-presets —
the g5-workspaces open tail, F1/F2/F4/F5 registry actions over the 3D/2D
views, applied to the active workspace's live dock and persisted — plus
l6-low-followups (the pi review's two LOWs: the History-arm same-frame
graph drain; the load test's undrained-pending pin). One commit for both
kills (they share main.rs). Battery GREEN: umber-app 879 tests 0 failed
+ clippy + fmt + layers. Desktop checks 1-3 deferred — Bessa at the
desktop, no screenshots per his word.

**Next:** the open tail — g5-workspaces CLOSED 10-29 (`ad6f44b`) ·
g7-keyboard-only CLOSED 10-30 (`e008dcc`) · the
Painter-faithful default layout CLOSED 10-30 (`0f18fa7`, the viewport
dominates the centre; LAYOUTS_VERSION 2 resets saved v1 layouts) ·
the 10-30 perf-nightly red VERDICTED FLEET, not code (the bench binary
is byte-identical across a red and a green reading; the §12 gate's
measured noise band is x1.51 against a x1.10 band — the instrument fix
P1..P4 plus the reseed is queued as its own two-gate wave; leg log in
docs/perf/protocol.md) · the perf-gate instrument wave CLOSED 10-30
(`0981d2f` — P1 landed: the nightly's verdict is now the same-job A/B
RATIO current/reference, band x1.10, not the absolute band; the
absolute compare is report-only) · the quick-mask design question DECIDED 10-31 (persist, 8-bit sidecar — brief in the 10-31 design kill report; undo = follow-up) · the follow-up
queue (the flatten's compound one-step; the browser's drag-drop +
thumbnails) · the desktop-checks Result lines x4 (need the live desktop) ·
queue.md's push-log row for this wave (rides the next docs pass) · the
CLAUDE.md trap-line addition (the `&mut dyn` object-lifetime trap —
a protected-file write, Bessa's hands) · the different-family
cross-review debt if the pi attempts stay dead (the claude lane's
revival takes it). centralhost: HOST_BACK 2026-10-21 ~13:32,
unreachable since — FRAGILE, memtest86+ owed at Bessa's next
physical access (boot straight into it, skip the OS), checking STOOD
DOWN by his order. The release lane stays Bessa-gated: SPEC
ratification (SPEC.md still DRAFT), the dogfood asset, Windows
signing, the alpha tag.

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

- centralhost: unreachable since 2026-10-21 ~13:32 (HOST_BACK that day,
  plain boot, FRAGILE); memtest86+ owed at Bessa's next physical access
  (boot straight into it, skip the OS); host checking STOOD DOWN by his
  order. The cargo envelope (Bessa 2026-10-22 ~13:1x): cargo LIFTED,
  run slow — ONE cargo stack machine-wide, heavy legs -j1..-j3 under
  the flock, the 8G/2G scope shield, heavy jobs serialized one at a
  time. The W-C3a close ran fully LOCAL; the W-C3b close does the same.
- Windows Ink: winink stays default-off and hook-less (C2-30 closed);
  the pen-flags half (S1) LANDED with the W-C3a fleet (`8d7bf8a`).
- Flatpak bundle build: GREEN END TO END (2026-10-23, verdict run
  40040567126 on `3f1f307`) — the five-wall chain closed (vendor
  parser, runtime 26.08, install trio, rofiles hardening, svg
  loader); Umber.flatpak (12,635,392 bytes) uploads as the CI
  artifact; the job stays pinned to ubuntu-24.04 (`f62d112`) against
  the runner rotation.
- Perf-nightly: RED since 10-21 (the drift gate flipped on an unchanged
  tip) — its own hunt; the segv hunt's evidence names the opening
  hypothesis (perf-nightly.yml:30 rides ubuntu-latest — the same
  rotation); pin vs re-baseline is that hunt's call.
- Pushes are autonomous: push-gate.json is STANDING approval (Bessa's
  directive: keep it true, push when needed) — for WAVE pushes (Gate 2,
  charter + umber-wave-ops skill). The release lane is Bessa-gated:
  SPEC ratification, the dogfood asset, Windows signing, the version
  bump + tag (`docs/specs/release-checklist-0.1.0.md`).

## Handoff note (what the next session must know)

1. **Read the standing charter first** — vault
   `projects/umber/loop-charter.md` (v1, 2026-10-22; skill
   `umber-wave-ops` + the STATE block in
   `.dragon/COORDINATION-next-wave.md` beside it). docs/loop/loop-prompt.md
   carries the supersession banner (ff92379) and stays as the
   public-facing historical contract. Read docs/loop/queue.md second —
   per-item state lives there.
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
   — locks live per-kill again (the L6 lock held 18:22 to
   commit-time on 10-27); claw lanes 10-27: opencode live
   (deepseek-v4.1-flash-free pinned by canary), pi live (glm-5.3),
   claude AUTH-DEAD pending Bessa's re-login. Dragon-loop wake
   and leisure sessions write into the shared main tree too: a hold set
   in a live session (e.g. "no cargo") must be written into any queued
   hunt's goal, or a wake session will not see it.
6. Test counts: 1,700+ `#[test]`s across 16 crates (2026-10-16: the wave
   battery ran 967 suite tests green across the five app/engine crates;
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
