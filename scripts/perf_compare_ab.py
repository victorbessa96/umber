#!/usr/bin/env python3
"""perf_compare_ab.py — the same-job A/B ratio gate (P1 of the 2026-10-30
instrument wave).

The absolute gate (perf_compare.py vs perf/baseline.json) measures
fleet + code together: on the hosted lavapipe fleet the runner's CPU
throughput is a common factor that moved x1.16..x1.51 on IDENTICAL
code within a day (docs/perf/protocol.md, the 2026-10-30 leg log), so
a x1.10 absolute band cannot resolve a code regression. This gate
compares against a pinned reference commit benched IN THE SAME JOB on
the SAME runner (perf_gate.sh builds PERF_AB_REF and interleaves its
passes with the current tree's) and gates the RATIO current/reference
per metric. The fleet factor cancels; the ratio carries the code delta.

Band: argv[5] or PERF_AB_BAND, default 1.10. The ratio's own noise floor
is re-derived from the first weeks of runs; if it also proves wider
than the band, P2/P3 (mean-gate / best-of-N) stay on the table.

Semantics mirror perf_compare.py: a tracked metric missing from the
current run is a MISSING failure; one missing from the pinned reference
prints U and is skipped (the reference is pinned and carries all four).
The absolute compare against perf/baseline.json, when given, is printed
REPORT-ONLY — it no longer fails the job.

Usage: perf_compare_ab.py <cur-ref.json> <cur-i2p.json> <ab-ref.json> <ab-i2p.json> [band] [baseline.json]
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from perf_compare import TRACKED, load, metrics  # the same tolerant loader


def main() -> int:
    if len(sys.argv) < 5:
        print(__doc__)
        return 2

    cur = metrics(load(sys.argv[1]), load(sys.argv[2]))
    ab = metrics(load(sys.argv[3]), load(sys.argv[4]))
    band = float(sys.argv[5]) if len(sys.argv) > 5 else 1.10
    baseline_path = Path(sys.argv[6]) if len(sys.argv) > 6 else None
    baseline = (
        json.loads(baseline_path.read_text())
        if baseline_path and baseline_path.exists()
        else {}
    )

    if not cur:
        print("FAIL: no current metrics parsed — harness output missing/unparsable")
        return 1
    if not ab:
        print("FAIL: no A/B reference metrics parsed — the reference leg is broken")
        return 1

    if baseline:
        print("--- absolute vs perf/baseline.json (REPORT-ONLY, does not fail) ---")
        tracked_abs = {k for k in TRACKED if k in baseline}
        for key in sorted(tracked_abs & set(cur)):
            base = baseline[key]
            print(f"  {key}: {cur[key]} vs baseline {base} (x{cur[key] / base:.3f})")

    tracked = {k for k in TRACKED if k in baseline} if baseline else set(cur)
    failures = []
    for key in sorted(tracked - cur.keys()):
        print(f"MISSING      {key}: tracked but absent from the current run")
        failures.append(key)

    print(f"--- A/B ratio vs the pinned reference (same job, same runner; band x{band:.2f}) ---")
    for key in sorted(cur):
        if key not in ab:
            print(f"U  {key}: {cur[key]} (absent from the pinned reference — skipped)")
            continue
        ratio = cur[key] / ab[key] if ab[key] else float("inf")
        verdict = "OK" if ratio <= band else "REGRESSION"
        print(
            f"{verdict:<12} {key}: current {cur[key]} / reference {ab[key]} "
            f"= x{ratio:.3f}"
        )
        if ratio > band:
            failures.append(key)

    if failures:
        print(
            f"FAIL: {len(failures)} metric(s) over the A/B band or missing: "
            + ", ".join(failures)
        )
        return 1
    print(f"PASS: all A/B ratios within x{band:.2f} of the pinned reference")
    return 0


if __name__ == "__main__":
    sys.exit(main())
