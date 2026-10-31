#!/usr/bin/env bash
# perf_gate.sh — the §12 nightly gate, A/B-instrumented (the 2026-10-30
# instrument wave).
#
# History: the 2026-10-19 drift-hardening made no single sample carry a
# verdict (median-of-3, then a same-tip canary median-of-5 on a red). The
# 2026-10-30 red (run 41034356551) proved the next layer: the hosted
# lavapipe fleet's CPU throughput itself moves x1.16..x1.51 on IDENTICAL
# code within a day, so even a 5-sample median of ABSOLUTE numbers tracks
# the sample draw, not the code (docs/perf/protocol.md, the 10-30 leg log:
# measured noise band x1.51 against the x1.10 band it claims to resolve).
#
# The instrument fix (P1 — same-job A/B ratio): the pinned reference
# commit PERF_AB_REF (the tip perf/baseline.json was seeded from) is
# fetched, archived, built and benched IN THE SAME JOB on the SAME
# runner, its passes INTERLEAVED with the current tree's, and the
# verdict is the RATIO current/reference per metric
# (scripts/perf_compare_ab.py, band PERF_AB_BAND, default 1.10). The
# fleet's throughput is a common factor and cancels; the ratio carries
# the code delta only. The absolute compare vs perf/baseline.json stays
# in the log REPORT-ONLY (the workflow's compare step is non-fatal).
#
# The stroke soak runs once, before any comparison — it is an assert
# harness (zero drops), not a metrics source; its failure fails the job
# outright, as before.
#
# Usage: perf_gate.sh  (the nightly workflow calls it from the workspace
# root; the harness env vars live here, not in the YAML).
set -u
cd "$(dirname "$0")/.." || exit 126

# The A/B reference: the commit perf/baseline.json was seeded from
# (2026-10-19, same-tip means of runs 39263843229 + 39268968173).
# On a baseline reseed, move this pin with it.
PERF_AB_REF="${PERF_AB_REF:-0955ac8467ce7670b3fb7dd24d15c835dd013ffc}"
PERF_AB_BAND="${PERF_AB_BAND:-1.10}"
AB_DIR=/tmp/ab-ref
AB_TARGET=/tmp/ab-target

run_pass() { # $1 = pass index (current tree)
    local i=$1
    UMBER_REF_SCENE_FRAMES=1000 UMBER_REF_SCENE_JSON=/tmp/ref-r$i.json \
    UMBER_I2P_EVENTS=5000 UMBER_I2P_JSON=/tmp/i2p-r$i.json \
        bash -c 'set +e; f=0
                 cargo test -p umber-gpu --features perf --release ref_scene_bench -- --nocapture || f=1
                 cargo test -p umber-app --features perf --release input_photon_session -- --nocapture || f=1
                 exit $f'
}

run_pass_ab() { # $1 = pass index (pinned reference tree)
    local i=$1
    (cd "$AB_DIR" && \
     CARGO_TARGET_DIR="$AB_TARGET" \
     UMBER_REF_SCENE_FRAMES=1000 UMBER_REF_SCENE_JSON=/tmp/ab-ref-r$i.json \
     UMBER_I2P_EVENTS=5000 UMBER_I2P_JSON=/tmp/ab-i2p-r$i.json \
        bash -c 'set +e; f=0
                 cargo test -p umber-gpu --features perf --release ref_scene_bench -- --nocapture || f=1
                 cargo test -p umber-app --features perf --release input_photon_session -- --nocapture || f=1
                 exit $f')
}

median_into() { # $1 = canonical out; rest = the pass JSONs
    local out=$1; shift
    python3 scripts/perf_median.py "$out" "$@" || exit 1
}

setup_ab_ref() {
    echo "=== A/B reference: $PERF_AB_REF (the perf/baseline.json seed tip) ==="
    git fetch --depth=1 origin "$PERF_AB_REF" \
        || { echo "FAIL: cannot fetch the A/B reference $PERF_AB_REF"; exit 1; }
    rm -rf "$AB_DIR"
    mkdir -p "$AB_DIR"
    git archive "$PERF_AB_REF" | tar -x -C "$AB_DIR" \
        || { echo "FAIL: git archive of the A/B reference"; exit 1; }
    (cd "$AB_DIR" && CARGO_TARGET_DIR="$AB_TARGET" \
        cargo build --release -p umber-app -p umber-gpu --features perf) \
        || { echo "FAIL: the A/B reference build"; exit 1; }
}

ab_compare() { # medians must already sit at the canonical paths
    python3 scripts/perf_compare_ab.py \
        /tmp/ref-scene.json /tmp/i2p.json \
        /tmp/ab-ref-scene.json /tmp/ab-i2p.json \
        "$PERF_AB_BAND" perf/baseline.json
}

echo "=== stroke soak (assert harness, once) ==="
cargo test -p umber-app --features perf --release stroke_soak -- --nocapture || exit 1

setup_ab_ref

echo "=== gate 1: three interleaved pass pairs, median-of-3 each, A/B ratio ==="
for i in 1 2 3; do
    run_pass "$i" || { echo "harness failure in current pass $i"; exit 1; }
    run_pass_ab "$i" || { echo "harness failure in reference pass $i"; exit 1; }
done
median_into /tmp/ref-scene.json /tmp/ref-r1.json /tmp/ref-r2.json /tmp/ref-r3.json
median_into /tmp/i2p.json /tmp/i2p-r1.json /tmp/i2p-r2.json /tmp/i2p-r3.json
median_into /tmp/ab-ref-scene.json /tmp/ab-ref-r1.json /tmp/ab-ref-r2.json /tmp/ab-ref-r3.json
median_into /tmp/ab-i2p.json /tmp/ab-i2p-r1.json /tmp/ab-i2p-r2.json /tmp/ab-i2p-r3.json

if ab_compare; then
    echo "GATE 1 green (median-of-3 A/B ratio). No canary owed."
    exit 0
fi

echo "=== gate 1 red — the canary leg: two more interleaved pairs, median-of-5 each ==="
for i in 4 5; do
    run_pass "$i" || { echo "harness failure in current pass $i"; exit 1; }
    run_pass_ab "$i" || { echo "harness failure in reference pass $i"; exit 1; }
done
median_into /tmp/ref-scene.json /tmp/ref-r1.json /tmp/ref-r2.json /tmp/ref-r3.json /tmp/ref-r4.json /tmp/ref-r5.json
median_into /tmp/i2p.json /tmp/i2p-r1.json /tmp/i2p-r2.json /tmp/i2p-r3.json /tmp/i2p-r4.json /tmp/i2p-r5.json
median_into /tmp/ab-ref-scene.json /tmp/ab-ref-r1.json /tmp/ab-ref-r2.json /tmp/ab-ref-r3.json /tmp/ab-ref-r4.json /tmp/ab-ref-r5.json
median_into /tmp/ab-i2p.json /tmp/ab-i2p-r1.json /tmp/ab-i2p-r2.json /tmp/ab-i2p-r3.json /tmp/ab-i2p-r4.json /tmp/ab-i2p-r5.json

if ab_compare; then
    echo "GATE 2 green (median-of-5 A/B ratio) — gate 1's red was the sample draw, not a regression."
    exit 0
fi
echo "GATE 2 red (median-of-5 A/B ratio) — the current tree is slower than the pinned reference on the same runner, five passes: a code regression."
exit 1
