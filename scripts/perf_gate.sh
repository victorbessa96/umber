#!/usr/bin/env bash
# perf_gate.sh — the drift-hardened §12 nightly gate (the 2026-10-19
# prescription, docs/perf/protocol.md): no single sample carries a
# REGRESSION verdict. The two metric harnesses run THREE passes; the
# per-key medians (perf_median.py) feed the unchanged perf_compare.py.
# On a red, the same-tip CANARY leg runs two more passes — same runner,
# same tip, minutes later — and the median-of-5 must ALSO be red
# before the job fails. The runner-fleet CPU drift of 2026-10-19 (x2
# in one day, image byte-identical) read red on five different
# commits and cost a four-leg CI bisect; this gate answers the same
# event inside one job.
#
# The stroke soak runs once, before any comparison — it is an assert
# harness (zero drops), not a metrics source; its failure fails the
# job outright, matching the pre-hardening contract.
#
# Usage: perf_gate.sh  (the nightly workflow calls it from the
# workspace root; the harness env vars live here, not in the YAML).
set -u
cd "$(dirname "$0")/.." || exit 126

run_pass() { # $1 = pass index (1..5)
    local i=$1
    UMBER_REF_SCENE_FRAMES=1000 UMBER_REF_SCENE_JSON=/tmp/ref-r$i.json \
    UMBER_I2P_EVENTS=5000 UMBER_I2P_JSON=/tmp/i2p-r$i.json \
        bash -c 'set +e; f=0
                 cargo test -p umber-gpu --features perf --release ref_scene_bench -- --nocapture || f=1
                 cargo test -p umber-app --features perf --release input_photon_session -- --nocapture || f=1
                 exit $f'
}

median_into() { # $1 = canonical out; rest = the pass JSONs
    local out=$1; shift
    python3 scripts/perf_median.py "$out" "$@" || exit 1
}

echo "=== stroke soak (assert harness, once) ==="
cargo test -p umber-app --features perf --release stroke_soak -- --nocapture || exit 1

echo "=== gate 1: three passes, median-of-3 ==="
for i in 1 2 3; do
    run_pass "$i" || { echo "harness failure in pass $i"; exit 1; }
done
median_into /tmp/ref-scene.json /tmp/ref-r1.json /tmp/ref-r2.json /tmp/ref-r3.json
median_into /tmp/i2p.json /tmp/i2p-r1.json /tmp/i2p-r2.json /tmp/i2p-r3.json

if python3 scripts/perf_compare.py /tmp/ref-scene.json /tmp/i2p.json perf/baseline.json; then
    echo "GATE 1 green (median-of-3). No canary owed."
    exit 0
fi

echo "=== gate 1 red — the same-tip canary leg: two more passes, median-of-5 ==="
for i in 4 5; do
    run_pass "$i" || { echo "harness failure in canary pass $i"; exit 1; }
done
median_into /tmp/ref-scene.json /tmp/ref-r1.json /tmp/ref-r2.json /tmp/ref-r3.json /tmp/ref-r4.json /tmp/ref-r5.json
median_into /tmp/i2p.json /tmp/i2p-r1.json /tmp/i2p-r2.json /tmp/i2p-r3.json /tmp/i2p-r4.json /tmp/i2p-r5.json

if python3 scripts/perf_compare.py /tmp/ref-scene.json /tmp/i2p.json perf/baseline.json; then
    echo "GATE 2 green (median-of-5) — gate 1's red was fleet drift, not a regression."
    exit 0
fi
echo "GATE 2 red (median-of-5) — the regression is real: same tip, same runner, five passes."
exit 1
