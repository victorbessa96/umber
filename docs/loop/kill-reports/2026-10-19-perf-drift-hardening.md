# Kill report: perf drift-hardening — median-of-3 + the same-tip canary (2026-10-19)

Dragon-authored by the Razul main session (the perf-hunt's
lane-owning sibling), under Bessa's "continue the long horizon, do
not stop". Kill-lock `drift-hardening`, holding three files:
`.github/workflows/perf-nightly.yml`, `scripts/perf_gate.sh` (new),
`scripts/perf_median.py` (new).

## Context

The 2026-10-19 runner-fleet drift (docs/perf/protocol.md's leg log)
read REGRESSION on five different commits in one day — the baseline's
own tip included — and cost a four-leg CI bisect to prove the code
innocent. A single sample can no longer carry a verdict: GitHub's
hosted fleet shifted CPU throughput ~x0.5 between 10-17 and 10-19
with the image byte-identical, and it will drift again.

## Built

- `scripts/perf_median.py`: median-of-N harness JSONs into the
  canonical report paths. Per-key medians over the passes that
  produced each key (a flaky metric is not a veto); a key absent
  from every pass stays absent and perf_compare's MISSING check
  fires exactly as before; non-numeric keys ride from the first
  input; `_median_of: N` records provenance (inert to the compare).
  Reuses perf_compare's log-noise-tolerant loader — no second parser.
- `scripts/perf_gate.sh`: the two-gate flow. The stroke soak runs
  once, first, as before (assert harness — its failure fails the job
  before any comparison). Gate 1: three passes of ref_scene_bench +
  input_photon_session, medians, compare — green exits 0. On red,
  the same-tip canary: two more passes (same runner, same tip,
  minutes later), median-of-5, compare again — gate 2 green means
  gate 1's red was drift; gate 2 red is the real regression verdict.
- The workflow: the harness step collapses to one gate call (the
  script owns the env vars), `timeout-minutes: 40 -> 90` (holds
  build + soak + five passes + two compares with margin), the
  artifact upload keeps every per-pass JSON as evidence, and the
  unchanged `if: always()` compare step still prints the honest
  verdict if the gate dies mid-canary.

## Gates run (host-free, this session)

Ad-hoc verification 6/6 on the shipped bytes: perf_median compiles
and produces the exact median-of-3 on the three RECORDED same-era
reads (83.4674 / 87.2804 / 85.9958 → 85.9958), the p95 median over
the two present passes (91.2494 / 95.6265 → 93.43795 — their mean,
which self-consistently equals the reseeded baseline's p95, both
being means of the same two runs), noise-LINE tolerance, the merged
JSON flattens through perf_compare.metrics() identically to a raw
harness report, perf_gate.sh syntax, the workflow YAML parses, and
the end-to-end compare fires honest verdicts on the merged fixture
(viewport_mean OK x1.007; a fixture input_photon_p95 reads
REGRESSION x9.3). Host-gated unknowns: the gate's real runtime
inside 90 min, and the first hardened nightly as the live verdict
gate — the same class as every workflow change.

State: VERIFIED host-free, battery-exempt (no crate bytes touched);
commits at this wave, rides the next push; the first nightly on the
pushed tip is the end-to-end proof.
