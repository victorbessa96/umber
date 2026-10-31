# Kill report: 2026-10-30 perf-gate instrument wave — the same-job A/B ratio (P1)

**Status: Gate 1 (tooling checks) green — committing on the two-gate model.**

- **Kill**: `perf-ab-instrument`. The 2026-10-30 nightly red
  (41034356551, tip `f66ed452`, viewport_p95 x1.165 vs baseline) was
  verdicted FLEET, NOT CODE by the six-cell dissection (see
  `2026-10-30-perf-fleet-canary-cells.md` and the sibling's
  `2026-10-30-perf-nightly-regression-verdict.md`): the hosted lavapipe
  fleet's CPU throughput moves x1.16..x1.51 on IDENTICAL code within a
  day (same tip read x1.165 at 10:03Z and x1.280 at 19:43Z, the mean
  metric moving x0.923 -> x1.208), so the absolute x1.10 band sits
  under the yardstick's own noise band.
- **Lock**: `perf-ab-instrument` — scripts/perf_gate.sh,
  scripts/perf_compare_ab.py, .github/workflows/perf-nightly.yml,
  this report.

## What landed

1. `scripts/perf_compare_ab.py` (new): the RATIO gate. Compares the
   current tree's median metrics against the pinned reference's,
   per metric, band `PERF_AB_BAND`/argv default **x1.10**. Semantics
   mirror perf_compare.py: a tracked metric missing from the current
   run is a MISSING failure; one missing from the pinned reference
   prints U and is skipped. The absolute compare vs
   `perf/baseline.json` is printed REPORT-ONLY (never fails).
2. `scripts/perf_gate.sh`: now builds the pinned reference
   `PERF_AB_REF` (default `0955ac846...` — the tip
   `perf/baseline.json` was seeded from; move the pin with a reseed)
   via `git fetch --depth=1` + `git archive` into /tmp/ab-ref with its
   own CARGO_TARGET_DIR, and runs its passes INTERLEAVED with the
   current tree's (pass i: current then reference), so any
   time-correlated fleet drift hits both legs equally. The two-gate
   structure is unchanged: median-of-3 pairs, and on a red a canary
   leg of two more pairs (median-of-5 each) before the job may fail.
   The stroke-soak assert still fails the job outright.
3. `.github/workflows/perf-nightly.yml`: the absolute compare step is
   now `|| echo "::warning::..."` (report-only; the ratio gate owns
   the verdict), the artifact upload gains the `/tmp/ab-*.json`
   files, `timeout-minutes` 90 -> 120 (two builds + ten passes), and
   the header comment records why.

## Gate 1 (this wave's battery, tooling scope)

No Rust code changed, so the cargo battery is out of scope for this
wave; what ran, all green:

- `bash -n scripts/perf_gate.sh`; `py_compile` on all three perf
  scripts; workflow YAML parse + assertions (timeout 120, report-only
  compare with `|| echo`, ab-* artifact paths, gate step present).
- `perf_compare_ab.py` unit cases against synthetic JSONs: absolute
  x2.14 red but ratio 1.0 -> PASS (proves the absolute leg is
  report-only); ratio x1.20 -> REGRESSION exit 1; tracked-but-missing
  -> MISSING exit 1; reference-absent metric -> U, skipped; band argv
  override works.
- End-to-end dry run of `perf_gate.sh` with a stubbed `cargo`:
  green path (both legs equal) -> medians -> ratio 1.0 -> "GATE 1
  green", exit 0, no canary owed; red path (reference half-speed) ->
  gate 1 red -> canary pairs 4/5 -> median-of-5 -> "GATE 2 red",
  exit 1. The A/B reference fetch+archive leg ran for real against
  origin (`0955ac846...`).

## What this does NOT claim

- The ratio's own noise floor is unknown until the first instrumented
  nightlies land; x1.10 on the ratio is the starting band (the fleet
  factor cancels, so it should be far tighter than the absolute band).
  The first weeks of ratio numbers re-derive the floor; if it also
  proves wide, P2 (gate the mean / require both) and P3 (best-of-N)
  stay on the table, and the leg log in docs/perf/protocol.md gets
  the numbers.
- No baseline reseed rides this wave: with the ratio gate, the
  absolute baseline is report-only, so reseeding is no longer urgent.
- No local bench ran (centralhost stood down; the perf harness needs
  the CI lavapipe). Everything above is script logic + synthetic
  data, plus the real git fetch/archive of the reference.
