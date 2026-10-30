# 2026-10-30 — the perf-nightly red: the six-cell dissection (fleet, not code)

- **Author:** dragon (research kill — docs only, no bench code change, battery-exempt)
- **Companion report (sibling lane, owns `docs/perf/protocol.md`):**
  `docs/loop/kill-reports/2026-10-30-perf-nightly-regression-verdict.md`
  — kill lock `perf-nightly-41034356551-verdict` (STATE.md,
  docs/perf/protocol.md, docs/loop/queue.md, that report). **This file
  deliberately does not touch those paths**: the §12 leg log and the
  STATE/queue rows are the sibling's to write.
- **Shipped by this hunt:** `8e1195e` — the same-tip fleet canary
  became dispatchable (`workflow_dispatch` input `ref` in
  `.github/workflows/perf-nightly.yml`).

## The ask

The scheduled nightly `41034356551` (tip `f66ed452`) failed the §12
gate: `viewport_p95_ms 108.8683 vs baseline 93.43795 (x1.165)`. The
decision owed: code-caused or fleet noise.

**VERDICT: FLEET. The code is innocent.** The yardstick moved; the
binary did not. Three new cells land here, and one of them is
decisive on its own.

## The three new cells (all read from the CI artifacts, none re-benched locally)

| run | dispatched | checkout | per-pass viewport_mean (ms) | median mean | median p95 | x p95 |
|---|---|---|---|---|---|---|
| 41116918979 | 10-30 19:38Z | `3bd3040` | 71.3 / 92.3 / 72.7 | 72.73 (x0.852) | 79.30 | **x0.849** |
| 41117599643 | 10-30 19:43Z | `f66ed452` | 76.4 / 103.1 / 73.9 / 103.7 / 103.7 | 103.12 (**x1.208**) | 119.56 | **x1.280** |
| 41119360104 (CANARY) | 10-30 19:56Z | `25207faf` (pre-`ad6f44b`) | 74.0 / 79.8 / 73.1 | 74.02 (x0.867) | 79.27 | **x0.848** |

Every cell compares against the SAME `perf/baseline.json` (unchanged
since the 2026-10-19 reseed) on the SAME hosted lavapipe fleet.

### Leg A — identical code moved between runs (the decisive leg)

Tip `f66ed452` read p95 **x1.165** at 10:03Z (the failing nightly) and
p95 **x1.280** at 19:43Z (the re-dispatch, 41117599643) — and the MEAN
metric moved with it: **x0.923 → x1.208**. A property of a binary
cannot move between two runs of the same commit, on the same image,
1h40 apart. The same tip also read x1.165 while its best single pass
was x0.879 of baseline (82.1481 vs 93.43795).

### Leg B — the code cannot own it

`f66ed452` contains `ad6f44b` (the g5 viewport-presets + L6-lows
wave). Its DESCENDANT `3bd3040` — carrying that same code — measured
**x0.849 p95 / x0.852 mean**, the fastest cell of the six, three
minutes before the slow `f66ed452` dispatch. Verified ancestry:
`git merge-base --is-ancestor f66ed452 3bd3040` → true.

### Leg C — the fleet is not globally slow, so the slow readings are per-runner

The canary cell pins the checkout to `25207faf` — the tip BEFORE
`ad6f44b`, i.e. the code the red run did NOT have — and reads
**x0.848 p95 / x0.867 mean** on today's fleet. The old code is fast
today; the code under suspicion is fast today (Leg B); only the
*instances* that read slow are slow. This is the per-runner-instance
signature (noisy neighbours on the hosted pool), not an era shift and
not a code property.

### Corroboration

`input_photon_p95_ms` stayed at 10.178-10.186ms across every run
(baseline 10.1799, x1.0005) — the latency-bound stroke→display path
never moved; only the throughput-bound ref-scene bench does. The
failing night's own red was TAIL-ONLY (p95 x1.165 with mean x0.923),
the classic contention signature.

### Why the fleet swings

Per-pass means cluster into two modes — 73-79ms ("fast") and 92-104ms
("slow") — and they alternate WITHIN a single job on a single machine
(41117599643: 76.4 / 103.1 / 73.9 / 103.7 / 103.7). The gate draws 3
(or 5) samples from an x1.4-wide distribution and takes their median,
so the verdict tracks the sample draw. The sibling's leg log adds the
day-scale half: `25207fa` read x1.087 (10-28) and x0.938 (10-29) —
byte-identical code, x1.159 apart — and the 10-27 night read x0.576.

## The instrument change that shipped (`8e1195e`)

The protocol's named canary ("a same-tip canary leg on any red")
could only measure the dispatched ref, so a canary at the baseline tip
needed a scratch branch — the partner-gated action. The workflow now
takes a `ref` input that pins the CHECKOUT to any commit while the
workflow file stays on the dispatched ref:

```
gh workflow run perf-nightly.yml --ref master -f ref=<sha>
```

No scratch ref, no push, and the canary compares against that tree's
own baseline file. The cell that closed this hunt (41119360104) was
dispatched exactly that way, at the pre-`ad6f44b` tip.

## The prescription (unchanged in substance, now with a command)

The gate's 10% band is narrower than the yardstick's own noise (the
fleet moves x1.16-1.28 on identical code within a day; within-run
spread is x1.40-1.44). The evidence prefers, in order:

- **P1 — same-job A/B ratio (primary).** Bench a pinned reference
  commit in the SAME job on the SAME runner and gate the RATIO; the
  fleet's throughput cancels as a common factor.
- **P2 — gate the mean, or require BOTH mean and p95 to regress.**
  The mean is the robust statistic on a contended CPU fleet (the red
  night's mean was x0.923 while its p95 was x1.165).
- **P3 — best-of-N instead of median-of-N** (the red night's best
  pass, 82.15, was under baseline).
- **P4 — interim:** widen the p95 band to the measured spread (x1.25).

A reseed alone repeats the cycle; it must ride with P1/P2. The
threshold itself is a standards call for the partner — the data to
make it is in this report and in the sibling's.

## Unverified / not done

- The fleet's physical mechanism is INFERRED from the metric signature
  (tail-only movement on the throughput-bound metric, flat on the
  latency-bound one); GitHub exposes no per-run host identity.
- No local re-bench: centralhost is stood down by Bessa's order and
  local cargo is not authorised.
- No bench-code change lands here — every instrument change is its own
  wave under the charter's two-gate model.
