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

Current phase: **THE STENCIL ENGINE LANDED END TO END + THE PERF
INSTRUMENT REBUILT (2026-10-19) — PUSHED 2026-10-19 15:54 local, origin
`aac1c4a`: CI 39306460278 green both platforms, the first hardened
nightlies green ×2 on the pushed tip (39306559332 + 39391362339). The
post-push doc/tooling tail (12 commits, 4d69808..86fe4a4) rides the
recovery push under the freeze-4 envelope.** (Ticked in the 2026-10-20
M1 sweep — the pre-push "thirteen rows local, push pending" wording is
preserved in git history at aac1c4a.)
The morning closed the freeze-3 fleet (push `ff9ac02..0955ac8`, CI
39263817670 GREEN both platforms; one clippy fix-forward, two
recovery-script defects fixed live — addenda 1+2 in the script's
report); the afternoon completed the stencil engine's whole chain
on top: the gate_plane paint hook (`3a8b4cb`, Amendment A), the app
wiring (`6c95e26`), the viewport shell path + the N toggle
(`2bb8c60`), the set-switch re-stage (`dcc5e89`), the cross-tile
observer (`45ad4e4` — coded by the opencode claw on
deepseek-v4.1-flash, host battery 822 tests green), and the design
doc's closure tick (`450b3e1`) — the lane's step-4 CLOSED, soft
edges answered by design, the mid-stroke window BY DESIGN. The
flatpak kill's verdict gate: fixed same-day by the
flatpak-archive-type kill (archive-type: tar-gzip per source) — the
post-push re-dispatch is the verdict gate. The perf instrument was
rebuilt the same day: the four-leg bisect CONVICTED THE RUNNER
FLEET, not the code (the baseline's own tip reads 89.29 today vs
45.54 on 10-16; every commit reads 78-91 on today's fleet — the
full leg log in docs/perf/protocol.md), the baseline reseeded to
today's fleet era (`c53a08d`+`b1d3660`), the nightly drift-hardened
(`dc4b355`: median-of-3 + the same-tip canary — no single sample
carries a verdict), and C2-16 closed with the §12 budgets' first
real-hardware record (`dc0c599`: the RTX 3070 at 82x/2x/zero-drop
margins) — the C2-13..19 honest-claims row is ALL SEVEN CLOSED.

**Next:** HOST_BACK (freeze-4, day 4 — Bessa's power-cycle, then boot
STRAIGHT into memtest86+ per the standing protocol) → the recovery
runbook: `.dragon/runbooks/host-back-recovery.md` (the parked fleet's
per-kill batteries + commits, then the ONE push of the doc/tooling
tail + ci-watch, then the packaging re-dispatch — PUSH FIRST; the
fleet's scopes live in its table). C2-8's drag reorder is CLOSED
(f2c87d4); the cached-bake/multi-tile follow-ups ride the parked
fleet. The release lane is Bessa-gated: SPEC ratification (SPEC.md
still DRAFT), Windows signing, the alpha tag
(`docs/specs/release-checklist-0.1.0.md`). (Ticked in the 2026-10-20
M5 runbook pass — the recovery-order prose moved to the runbook.)

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

- FREEZE-4 (since 2026-10-17 ~18:14): centralhost wedged, day 4 at this
  write — NO cargo, NO batteries, NO pushes until HOST_BACK
  (build-host-outage protocol + Bessa's 23:52 steer). Live state: the
  STATE block in `.dragon/COORDINATION-next-wave.md`; memtest86+ owed at
  Bessa's next physical access (boot straight into it, skip the OS).
- Windows Ink: winink stays default-off and hook-less (C2-30 closed —
  winit's WM_POINTER path carries pen pressure on Windows); the
  pen-flags half (S1) sits in the parked fleet, HOST_BACK-gated.
- Flatpak bundle build: the hermetic manifest landed (621 vendored
  sources; archive-type + strip-components fixes); extraction VERIFIED
  live by dispatch 39306564270 (621 crates past the old wall; its
  vendor-checksums death root-caused + fixed by 4d69808, riding the
  recovery push) — the verdict gate is the post-push packaging
  re-dispatch.
- Perf-nightly: RESOLVED — the four-leg bisect convicted the runner
  FLEET, not the code (docs/perf/protocol.md); baseline reseeded
  (c53a08d/b1d3660), nightly drift-hardened (dc4b355), and the first
  hardened nightlies ran GREEN on the pushed tip (39306559332 +
  39391362339). (Rows re-derived in the 2026-10-20 M1 sweep.)
- Pushes are autonomous: push-gate.json is STANDING approval (Bessa's
  directive: keep it true, push when needed) — for WAVE pushes (Gate 2,
  loop-prompt v1.2 §5). Current envelope (freeze-4) pauses pushes until
  HOST_BACK; the recovery session owns the ONE push.

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
   — no live locks (all released at the 2026-10-16 wave close); zero claws
   running as of this write. Dragon-loop wake
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
