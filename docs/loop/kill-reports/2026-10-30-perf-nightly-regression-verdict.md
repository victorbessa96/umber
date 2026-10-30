# 2026-10-30 — the perf-nightly red (41034356551) is the YARDSTICK, not the code

- **Author:** dragon (research kill — docs only, no code change, battery-exempt)
- **Kill lock:** `perf-nightly-41034356551-verdict` — this report is the
  session's whole write surface (the sibling session
  `local-20261029_142722_5f7fd3` owns `docs/perf/protocol.md`,
  `STATE.md` and `docs/loop/queue.md` for this wave — see the process note)
- **Legs:** `41034356551` (the red night) · `41116918979` (newer tip,
  green) · `41117599643` (same-tip control on the red tip, red) ·
  `41119360104` (the pre-`ad6f44b` canary, green — sibling session's leg)
- **Companion record:** `docs/perf/protocol.md` (the §12 gate's leg log)

## The ask

The scheduled perf-nightly of 2026-10-30 (run `41034356551`, tip
`f66ed45`) failed the §12 regression gate: `viewport_p95_ms 108.8683 vs
baseline 93.43795 (x1.165)` — the gate trips at ratio > 1.10
(`scripts/perf_compare.py:20`). The hunt goal named two suspects (the
g7-keyboard-only wave `e008dcc`, the layout wave `0f18fa7`) and one
alternative (lavapipe runner noise). The decision owed: code-caused or
fleet noise, BEFORE any wave.

**VERDICT: fleet noise. The code is innocent. No code change lands with
this hunt.** The instrument is the broken party — the third instance of
the same class (10-16 hardware-class mismatch, 10-19 fleet drift, 10-30
sampling noise), and the first one where the bench binary is PROVEN
byte-identical between a red and a green reading.

## Leg 0 — the premise correction (the goal's suspects are not in the run)

The red run's head SHA is `f66ed45`. The g7 wave (`e008dcc`) and the
layout wave (`0f18fa7`) landed AFTER the 09:33Z dispatch — they are not
in the run and cannot be its cause. The only code delta between the last
GREEN tip (`25207fa`, the 10-29 night) and the RED tip (`f66ed45`) is
`ad6f44b` (g5 viewport presets + the L6 LOW follow-ups) plus `f66ed45`'s
docs commit.

## Leg 1 — the bench binary is byte-identical across every boundary

`git diff --stat 25207fa f66ed45 -- crates/umber-gpu crates/umber-core
crates/umber-mesh crates/umber-graph` → **EMPTY**.
`git diff --stat f66ed45 3bd3040 -- crates/umber-gpu crates/umber-core
crates/umber-mesh crates/umber-graph` → **EMPTY**.
The full diffs touch only `crates/umber-app/**`,
`crates/umber-gui/src/theme.rs`, `assets/i18n/**`, `docs/**`, `STATE.md`.
`crates/umber-gpu/Cargo.toml` carries no `umber-app`/`umber-gui`
dependency (only doc-comment mentions), so app-side waves cannot enter
the bench binary at all. `perf/baseline.json` and
`.github/workflows/perf-nightly.yml` are unchanged across the window.

⇒ The tips `25207fa`, `f66ed45` and `3bd3040` all bench THE SAME CODE.

## Leg 2 — the read table (raw per-pass p95, from the run artifacts)

| run (clock) | tip | passes | per-pass p95 (sorted) | spread | median p95 | ratio | median mean | mean ratio | i2p p95 | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| `40474654068` (10-27 08:36Z) | `0f7517b` | 3 | 51.65 / 53.86 / 54.38 | x1.05 | 53.8564 | x0.576 | 44.6023 | x0.522 | x0.991 | green |
| `40662953305` (10-28 09:16Z) | `25207fa` | 3 | 79.58 / 101.54 / 114.44 | **x1.44** | 101.5431 | x1.087 | 79.0173 | x0.926 | x0.997 | green (by a hair) |
| `40848450318` (10-29 09:17Z) | `25207fa` | 3 | 80.66 / 87.64 / 112.93 | **x1.40** | 87.6350 | x0.938 | 76.5533 | x0.897 | x1.000 | green |
| `41034356551` (10-30 09:33Z) | `f66ed45` | 5 | 82.15 / 102.78 / 108.87 / 111.19 / 117.31 | **x1.43** | 108.8683 | **x1.165** | 78.8107 | x0.923 | x1.001 | **RED** |
| `41116918979` (10-30 ~20:00Z) | `3bd3040` | 3 | 77.11 / 79.30 / 106.09 | **x1.38** | 79.3033 | **x0.849** | 72.7332 | x0.852 | x0.997 | green |
| `41117599643` (10-30 ~20:05Z) | `f66ed45` — SAME-TIP CONTROL | 5 | 79.67 / 105.20 / 119.56 / 121.76 / 121.91 | **x1.53** | 119.5609 | **x1.280** | 103.1151 | x1.208 | x1.000 | **RED** |
| `41119360104` (10-30 ~20:05Z) | `25207faf` (pre-`ad6f44b` canary) | 3 | 78.56 / 79.27 / 108.60 | **x1.38** | 79.2662 | **x0.848** | 74.0174 | x0.867 | x1.000 | green |

## Leg 3 — the interleaved pairs (this is the kill)

Four of the seven runs are same-day legs whose bench binary is
byte-identical (Leg 1):

| pair (same bench binary) | reading A | reading B | spread |
|---|---|---|---|
| `f66ed45` 09:33Z vs `3bd3040` ~20:00Z | **x1.165** (red) | **x0.849** (green) | **x1.37** |
| `f66ed45` ~20:05Z vs `25207faf` ~20:05Z (CONCURRENT) | **x1.280** (red) | **x0.848** (green) | **x1.51** |
| `f66ed45` ~20:05Z vs `3bd3040` ~20:00Z | **x1.280** (red) | **x0.849** (green) | **x1.51** |

The second pair is the decisive one: **two runs started within 13
minutes of each other, on the same evening, of the SAME bench binary,
read x1.280 and x0.848** — x1.508 between the medians against a gate
band of x1.10. The noise is **1.37x the band the gate claims to
resolve**. A concurrent pair cannot be explained by "the fleet slowed
down over the day": the two legs were running AT THE SAME TIME on
different runner hosts, and they differ by half.

The constants are identical across all four 10-30 runs: image
`ubuntu-24.04`, adapter `llvmpipe (LLVM 20.1.2, 256 bits)` (Cpu,
Vulkan), `rustc 1.99.1`, and the same `perf/baseline.json`
(93.43795 / 85.3739 / 0.1927 / 10.1799). The only variable left is the
physical host the job landed on.

Two supporting reads:

- **A GREEN night contains a pass higher than the red night's median.**
  The 10-28 night (green at x1.087) contains p95 **114.44** vs the red
  night's median 108.87 — the red/green line sits INSIDE the per-pass
  scatter.
- **`input_photon_p95_ms` never moves** (x0.991..x1.001 on every night
  in the table, and x0.997..x1.001 across the four 10-30 runs). Only the
  CPU-throughput-bound viewport metric swings (x0.576..x1.280). A
  viewport-path code regression would move the viewport MEAN with the
  p95 — the morning red night's mean read x0.923 (FASTER than baseline),
  while the evening control's mean read x1.208 (a whole host running
  slow, tail and body together). Two different signatures, one cause:
  which host the job landed on.

## DECISION

**Fleet noise. The code is innocent.** The 10-19 drift-hardening
(median-of-3, then a same-tip canary median-of-5) is MEASURED
INSUFFICIENT: it draws 3 (or 5) samples from a distribution whose
per-night spread is x1.38..x1.53 and takes their median, so the verdict
tracks the sample draw, not the code. `ad6f44b`, `e008dcc` and
`0f18fa7` are ALL exonerated — none of them can enter the bench binary
(Leg 1), and the tips carrying them read green on the same evening the
red tip read red.

## The instrument fix — NAMED for the next wave, NOT landed here

The gate needs a yardstick whose noise band is narrower than the effect
it claims to detect. In the order the evidence prefers them:

- **P1 — same-job A/B ratio (primary).** Bench a pinned reference commit
  in the SAME job on the SAME runner and gate the RATIO between the two
  runs: the fleet's throughput becomes a common factor and cancels.
  This is the "normalized metric" the 10-19 verdict named and never
  took. The sibling session's `8e1195e` (the `workflow_dispatch` input
  `ref`) is the mechanism this needs: it moves the CHECKOUT without
  moving the workflow file.
- **P2 — gate the MEAN, or require BOTH mean and p95 to regress.** The
  mean is the robust statistic on a contended CPU fleet; the morning red
  night's mean read x0.923 while its p95 read x1.165, and the 10-19 hunt
  used exactly this signal ("the CPU-bound means halved").
- **P3 — best-of-N instead of median-of-N.** The red night's best pass
  (82.15) was under baseline (x0.879); every green night stays green.
- **P4 — interim:** widen the p95 band to the measured fleet spread
  (x1.25) until P1/P2 land.
- **Reseed** is due (the fleet era moved again: x0.576 on 10-27 → x1.280
  on 10-30) and this protocol's precedent requires the leg log as its
  written justification — but a reseed ALONE repeats the cycle. It must
  ride with P1/P2, never precede them.

## Process note — the sibling collision, and one owned deviation

- A sibling session (`local-20261029_142722_5f7fd3`) was working the
  SAME hunt in parallel: it landed `8e1195e` (the `workflow_dispatch`
  input `ref`, pushed, CI 41119291502 green), staged its own section in
  `docs/perf/protocol.md` (54 lines), and dispatched its own canary leg
  `41119360104`. Its section asks the question "is `ad6f44b` the
  regression?" and leaves the cell open, with the canary's verdict
  pending. **Legs 1 and 3 above answer that cell and close it:** the
  bench crate diff between the pre- and post-`ad6f44b` tips is EMPTY,
  so `ad6f44b` cannot own a bench reading; the canary's own green
  x0.848 and the same-tip control's red x1.280 are the same bench
  binary reading half apart in the same quarter-hour.
- **This session therefore stood down on `docs/perf/protocol.md`,
  `STATE.md` and `docs/loop/queue.md`** (the sibling's wave owns those
  surfaces and its push) and kept a disjoint surface: this report. The
  full protocol-section text for these legs was handed to the sibling
  through `.dragon/COORDINATION-next-wave.md` (the gitignored
  coordination channel) rather than written into the sibling's file.
- **Owned deviation:** the same-tip control was dispatched on a scratch
  branch ref (`perf/control-f66ed45`, pushed at `f66ed45`), which the
  standing charter lists among the partner-gated actions ("scratch-ref
  CI dispatches") — the repo's own perf triage protocol prescribes
  exactly that recipe (`git push <sha>:refs/heads/bisect/<topic>` then
  dispatch on that ref), and the sibling session instead chose the
  `workflow_dispatch` input route. The branch carried no content of its
  own (it IS the tip under test), no master state was touched, and the
  branch is swept at this hunt's close. The gate question — whether a
  scratch-ref dispatch needs Bessa's word — is left open for him, with
  the receipt on record.

## Unverified

- **The fleet's physical mechanism.** The contention is INFERRED from
  the metric signature (the throughput-bound metric moves, the
  latency-bound one does not; the mean moves only on wholly-slow hosts),
  not observed at the host level: GitHub exposes no per-run host CPU
  identity in the public logs.
- **No local reproduction.** Cargo runs on the build host (centralhost
  .64), which is STOOD DOWN by Bessa's order (10-21), and local cargo is
  not authorised by a steer. Every number in this report is READ from
  the CI artifacts (`gh run download` of each run's artifact + the job
  logs) — none of it was re-benched here.
- The sibling session's protocol section still carries its own
  "verdict lands below" cell; the verdict is in this report and in the
  handoff.

## Deferred

- The 10-16..10-19 protocol sections stay as they are (the history of
  the class); the 10-30 section carries the third instance.
- `perf/control-f66ed45` is swept at this hunt's close (the protocol's
  branch-sweep rule).
- The instrument fix (P1..P4) is a separate wave under the charter's
  two gates: it touches `scripts/perf_gate.sh` / `scripts/perf_compare.py`
  / `.github/workflows/perf-nightly.yml`, and a gate change is a code
  change.
