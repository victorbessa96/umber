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

Current phase: **THE W-C3a FLEET CLOSED THROUGH THE LOCAL PORT + THE
RUNNER-ROTATION REDS PINNED (2026-10-23) — eleven rows pushed
e610ed7..96edd1e (02:52 local), on top of the 28-commit doc/tooling tail
pushed 02:00 on Bessa's explicit order (aac1c4a..e610ed7).** Local
battery GREEN, 1,304 tests / 6 crates, under the slow envelope (one
cargo stack machine-wide, -j3/nice-19, the 8G/2G oomd scope shield —
nine fires, two reboots, five scope kills to land; the G1
sRGB-endpoint ULP red fixed production-side mid-close). The pushed
tip's CI 39908194347: windows + change classifier green; the ubuntu
leg red = the runner-image SIGSEGV (ubuntu-latest rotated 24.04→26.04
on 10-21 — root-caused, the check matrix pinned to ubuntu-24.04,
`c697bab`, rides the W-C3b push); the layering gate red = the fleet's
stale scorecard (README regenerated 2,100+→2,200+ this pass); the
flatpak dispatch red = the same rotation at the bwrap/rofiles layer
(the packaging job pinned this pass). The host stays unreachable,
checking STOOD DOWN; the W-C3b close runs LOCALLY via
scripts/w-c3b-local-close.sh (the port authored 10-23).
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

**Next:** the W-C3b close — `scripts/w-c3b-local-close.sh` (the local
port, same shape as the W-C3a one): the W-C3B claim, four sweep rows
(usdz-sublayer-walk, blend-replace-opacity, solo-height, tangent-frame)
+ the docs row, local throttled battery (umber-mesh umber-core
umber-gpu umber-app), the post-rows scorecard verify, ONE push carrying
`c697bab` + the flatpak pin + the scorecard + this docs pass, explicit
run-id ci-watch, then the post-push packaging re-dispatch (the flatpak
verdict gate, now on the pinned runner). Launch gated on the
kitten-voicesweep bake finishing — heavy jobs serialize one at a time
(the 10-22/23 reboot lesson). After the close: the W-C3b docs pass
(consume the four manifests + the sentinel-c3b, the
release-checklist/requirements rows 39/79/111, ledger sweep→LANDED, M1
pass), then the briefed runway (tangent wiring → L1 → L2 → L3 → L5-L7 +
K1, one dispatch each from docs/loop/briefs/), then the perf-nightly
red hunt (opening hypothesis: the 10-21 drift-gate flip is the same
ubuntu rotation — pin vs re-baseline is that hunt's call).
centralhost: HOST_BACK 2026-10-21 ~13:32, unreachable since — FRAGILE,
memtest86+ owed at Bessa's next physical access (boot straight into it,
skip the OS), checking STOOD DOWN by his order. The release lane stays
Bessa-gated: SPEC ratification (SPEC.md still DRAFT), the dogfood
asset, Windows signing, the alpha tag.

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
- Flatpak bundle build: the chain has never been green end to end — the
  offline-deps + archive-type + strip-components fixes all landed; the
  10-23 dispatch died at the runner's bwrap/rofiles layer (the
  ubuntu-26.04 rotation), the job now pinned to ubuntu-24.04; the
  W-C3b push's post-push re-dispatch is the verdict gate.
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
   — no live locks (the locks dir was emptied 10-22 01:47 with no
   owner; re-add per-kill locks at close time); zero claws running at
   the pass-3 sweep. Dragon-loop wake
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
