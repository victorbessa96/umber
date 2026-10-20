# M1 stale-row sweep — first pass (2026-10-20)

Meta-lane M1 of the 23:52 steer (local autonomy under FREEZE-4): the
queue/design-doc open-marker sweep as a RECURRING lane, not burst-on-ask.
This is the first standing pass. Claimed 07:56 local, local-2866bb4a
(wake-13449). Doc-only, battery-exempt per the envelope — the census IS
the verification. Lock: m1-stale-row-sweep (queue.md, STATE.md, this
report). Committed via dcommit; rides the recovery push.

## The headline finding

The stencil-chain + perf-instrument wave WAS PUSHED 2026-10-19 15:54:24
-03 (origin/master `aac1c4a`) — while queue.md's push-log row still said
"push pending" and STATE.md still said "thirteen rows local, push
pending". The push-log row's own named verdict gate — the first HARDENED
nightly on the pushed tip — has run GREEN twice since.

Evidence (per the M2 standing rule: the run objects' JOB LISTS, never the
conclusion alone):

- `git reflog show origin/master`: "update by push" at 2026-10-19
  15:54:24 -0300, aac1c4a; `git ls-remote origin master` →
  aac1c4a25242f2b318f99a427efccc74d2d0e33b (live, 10-20 07:5x).
- CI run 39306460278 (push, headSha aac1c4a…): layering gate SUCCESS,
  change classifier SUCCESS, "fmt + clippy + test (ubuntu-latest)"
  SUCCESS, "fmt + clippy + test (windows-latest)" SUCCESS.
- Packaging run 39306460702 (push, aac1c4a): build AppImage (x86_64)
  SUCCESS, lint packaging files SUCCESS, build Flatpak bundle SKIPPED
  (trigger design — the exact shape the M2 rule was born from).
- perf-nightly 39306559332 (workflow_dispatch, aac1c4a, 10-19 18:55Z):
  perf job SUCCESS.
- perf-nightly 39391362339 (schedule, aac1c4a, 10-20 08:09Z = 05:09
  local): perf job SUCCESS.
- The kill-ledger already recorded the push (its Landed-2026-10-19
  heading: "wave 1 of the day PUSHED, origin aac1c4a"); queue.md and
  STATE.md were the stale surfaces.

Post-push local tail: 12 commits (4d69808..86fe4a4: strip fix, docs
audit, queue sweeps, soft edges, packaging hint, port annotation,
checklist-truth, kill-ledger, dcommit M3, M4 STATE block + fixup),
riding the ONE recovery push at HOST_BACK per the freeze-4 envelope.

## Ticks (each re-derived at its landing site before the edit)

1. queue.md header — "GUI/P2 FROZEN pending Gate 0" superseded the same
   week (Gate 0 PASS run 38255484123; freeze LIFTED per the standing
   directive — both recorded further down the same file).
2. queue.md row 2 — the "CI reseed = C2-14" pointer died: C2-14 closed
   (row C2-13..19) and the reseed landed (`c53a08d` + `b1d3660`, both
   touch perf/baseline.json + docs/perf/protocol.md — git-verified).
3. queue.md row 16 — "ARMED, awaiting the first real .spp" stale: the
   3 .spp exist at ~/corpora/umber/spp/ (ls-verified 10-20: BessaCreatureV2
   84.7MB, Bessa Synth 437MB, HydrageaBessa 777MB, dropped 10-14), the
   host rsync completed 10-16 08:31 hash-matched (W-C2g row), the corpus
   gate ran green; crates/spp/tests/corpus.rs:27-29 live.
4. queue.md row 23 — "in flight — the active worklist" stale: every
   G0-G7 row in gui-plan-2026-10-15.md carries ✓ + SHA (g6-stencil-overlay
   via 0955ac8 + 2bb8c60); the per-row "(… open)" parentheticals stay
   named follow-ups, honestly open.
5. queue.md push-log row — tip "push pending" → `aac1c4a`; CI column
   "pending" → the green job-list citation above.
6. queue.md Scratch section — rewritten (dated pointer preserved per the
   10-17 precedent): the old contents described the closed 10-16 outage
   world; the new ones carry the freeze-4 envelope + the 12-commit tail.
7. STATE.md status line — "thirteen rows local, push pending" → the
   pushed truth (pre-push wording preserved in history at aac1c4a).
8. STATE.md Next — the wave push + hardened nightly are DONE; C2-8's
   drag reorder CLOSED (f2c87d4); the Next is now HOST_BACK → parked
   fleet → ONE push → packaging re-dispatch.
9. STATE.md Blocked/waiting — B5/winink, flatpak vendored-registry, and
   perf x15.3 lines all described the pre-10-19 world; rewritten to the
   current truth, plus the freeze-4 line STATE.md was missing entirely.
10. kill-ledger Waiting row 1 — blocked-on "the sibling's close motion"
    → HOST_BACK (worktree-side edit per the single-owner rule; rides the
    next ledger commit).

## Markers verified CURRENT (no tick — honest non-coverage)

- Row 15's "--script control-call arm still owed": absent from
  crates/umber-cli/src (grep 10-20) — still owed.
- C2-1 "GUI smoke pending", C2-3 "desktop kill-9 smoke pending", C2-9
  "a desktop check open": runtime acts; no report records them — still
  pending. (kittest coverage is not a desktop smoke; not conflated.)
- C2-22 Windows signing, C2-23..25 release lane: Bessa-gated; SPEC.md
  line 1 still "DRAFT — awaiting Bessa's ratification", the ratification
  checklist box at :125 unticked.
- C2-12's "sliders / eyedropper / persisted swatches deferred": the G1
  and eyedropper kills sit UNCOMMITTED in the parked fleet; the
  committed tree keeps the deferral true. Queue rows describe committed
  state — worktree bytes are not landed state.
- The packaging re-dispatch verdict gate: correctly waiting on the NEXT
  push (PUSH FIRST — the manifest clones master at build time; the strip
  fix 4d69808 must be on master first).
- Design-doc status lines (docs/specs/*.md): all LANDED/CLOSED/BUILT or
  plan-of-record — clean, nothing stale (grep census 10-20).

## The recurring-lane protocol (for future passes)

Scope: queue.md rows, STATE.md status/Next/Blocked, design-doc status
lines, the kill-ledger's Waiting table. Method: grep for open/pending/
deferred/owed/in-flight markers; verify EACH against the live tree with
file:line, commit, or run-object evidence BEFORE ticking; never tick on
prose — the tree wins. Runtime acts (smokes, desktop checks) stay
pending until a report records them. Worktree-side bytes are not landed
state. CI claims cite job lists (M2). Cadence: every session that lands
a kill ticks what it touches (updated-at-landing); a full pass runs at
every wave close and whenever the STATE block's Recent-events line is
more than one wave old. Precedent passes: the 10-19 burst sweep
(f0ae70d), the docs audit (907c393), the C2-8 closure census (f2c87d4);
this report makes the lane standing.

Census: 10 ticks, 6 held-current verdicts, 0 unverified claims left in
the touched rows. Doc-only; no code touched; no battery owed.
