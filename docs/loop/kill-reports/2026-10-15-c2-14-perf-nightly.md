# Kill report: C2-14 + C2-15 (the perf nightly actually runs, and stops leaking)

Date: 2026-10-15. Author: dragon. Lock: `c2-14-perf-nightly` over
`.github/workflows/perf-nightly.yml`, `scripts/perf_compare.py` and this
report. Source: `docs/loop/audit-2026-10-15-c2.md` C2-14, C2-15.

## Found

- **The nightly never ran.** `runs-on: [self-hosted]` with zero runners
  registered (`gh api .../actions/runners` total_count = 0). The three
  runs on record (10-13, 10-14, 10-15) all sat queued, and the first two
  were cancelled. The comment's "falls back to hosted with lavapipe" had
  no mechanism.
- **Its lavapipe path was wrong anyway.** `VK_DRIVER_FILES` pointed at
  `/usr/share/vulkan/icd.d/lxd/lavapipe.icd.json`. The hosted runner's
  real file is `lvp_icd.json`, read from the main CI log of run
  38760269560 (its "Software GPU presence probe" step).
- **The gate leaked three ways.** Every harness ended in `|| true`, so
  the stroke soak's zero-drop assert could never fail the job. A metric
  missing from the fresh run read as U and passed. The compare step was
  skipped whenever an earlier step failed.

## Built

- `runs-on: ${{ vars.PERF_RUNNER || 'ubuntu-latest' }}`. It is hosted by
  default. The 3070 runner opts in by setting the repo variable once it
  is online, so nothing waits on a runner that does not exist.
- The ICD path is discovered (`lvp_icd*.json` / `*lavapipe*.json`) and
  exported, and the step fails loudly if none is found.
- The harnesses run under `set +e` with an exit tally. All three run,
  their numbers are kept, and any failure fails the job.
- The compare step runs `if: always()`, so numbers are compared even
  when a harness failed.
- `perf_compare.py` adds a `TRACKED` metric set. A tracked metric that is
  in the baseline but absent from the fresh run now FAILS as MISSING. A
  new metric the baseline lacks stays U. Annotation keys (`note`,
  `adapter`, `seeded`) stay inert.

## Verification

- `perf_compare.py` against six fixture cases (local Python, not cargo).
  All six give the intended exit code: within 10% passes; a regression
  fails; a harness that produced nothing fails as MISSING (the case that
  used to pass); no fresh metrics fails; first run with no baseline
  passes and prints the candidate; annotation-only keys pass.
- The workflow YAML parses (PyYAML).
- **Not verified: an actual nightly run.** The first one fires at 03:00
  UTC after this lands, or by `workflow_dispatch`. Its numbers become the
  reseed for `perf/baseline.json`, which is still the Intel HD 530 / GL
  seed. That reseed is the follow-up and closes C2-14 fully.

## Deferred

- The baseline reseed from the first green run (C2-14's second half).
- The stroke soak emitting JSON (a dab-throughput metric). Today it is
  a pass/fail assert, and the tally now makes that assert count.
