#!/usr/bin/env python3
"""perf_median.py — median-of-N harness JSONs into one canonical report.

The drift-hardening half of the 2026-10-19 prescription
(docs/perf/protocol.md): a runner-fleet CPU shift moved single-run
numbers by x2 in one day (image byte-identical), so a single sample
can no longer carry a REGRESSION verdict. Each metric harness runs N
times; this script medians every numeric key the passes report and
writes one JSON the unchanged perf_compare.py reads.

Semantics: a key missing from SOME passes medians over the passes
that produced it (a flaky metric is not a veto); a key absent from
EVERY pass is absent from the output, and perf_compare's MISSING
check fires exactly as before. Non-numeric keys ride from the first
input verbatim. The merged report carries "_median_of": N so the
artifact shows its own provenance (inert to perf_compare, which reads
only the flat harness keys through metrics()).

Usage: perf_median.py <out.json> <in1.json> [<in2.json> ...]
"""

import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from perf_compare import load  # the same log-noise-tolerant loader


def median_merge(inputs):
    merged = {}
    numeric = {}
    for data in inputs:
        for key, value in data.items():
            if isinstance(value, (int, float)) and not isinstance(value, bool):
                numeric.setdefault(key, []).append(value)
            elif key not in merged:
                merged[key] = value
    for key, values in numeric.items():
        merged[key] = statistics.median(values)
    return merged


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    out = Path(sys.argv[1])
    inputs = [d for d in (load(p) for p in sys.argv[2:]) if d]
    if not inputs:
        print("FAIL: no parsable harness JSONs among the inputs")
        return 1
    merged = median_merge(inputs)
    merged["_median_of"] = len(inputs)
    out.write_text(json.dumps(merged) + "\n")
    print(f"median-of-{len(inputs)} -> {out}")
    for key in sorted(merged):
        print(f"  {key}: {merged[key]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
