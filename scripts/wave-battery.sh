#!/usr/bin/env bash
# wave-battery.sh — Gate 1 of the two-gate wave model (Bessa 2026-10-14):
# the per-kill LOCAL verification battery. Fast (minutes, warm cache),
# catches what CI catches, keeps every commit individually honest.
#
# Usage: bash scripts/wave-battery.sh [crates...]  (default: all engine+app)
# Gate 2 (the single wave-end CI run) rides the normal push + ci-watch.sh.
set -u
cd "$(dirname "$0")/.."
CRATES=("${@:-umber-core umber-color umber-graph umber-spp umber-cli umber-gpu umber-app}")
FAIL=0

echo "=== fmt ==="
if flock /tmp/umber-cargo.lock cargo fmt --all; then
    echo "PASS fmt (applied)"
else
    echo "FAIL fmt"; FAIL=1
fi

echo "=== clippy -D warnings ==="
for c in "${CRATES[@]}"; do
    N=$(flock /tmp/umber-cargo.lock cargo clippy -p "$c" --all-targets -- -D warnings 2>&1 | grep -cE '^error')
    if [ "$N" = "0" ]; then echo "PASS clippy $c"; else echo "FAIL clippy $c: $N errors"; FAIL=1; fi
done

echo "=== test suites ==="
for c in "${CRATES[@]}"; do
    R=$(flock /tmp/umber-cargo.lock cargo test -p "$c" 2>&1 | grep -E 'test result' | grep -c ' FAILED\.')
    if [ "$R" = "0" ]; then
        T=$(flock /tmp/umber-cargo.lock cargo test -p "$c" 2>&1 | grep -E 'test result: ok' | awk '{s+=$4} END {print s+0}')
        echo "PASS test $c (${T:-?} tests)"
    else
        echo "FAIL test $c"; FAIL=1
    fi
done

echo "=== battery: $([ $FAIL = 0 ] && echo GREEN || echo RED) ==="
exit "$FAIL"
