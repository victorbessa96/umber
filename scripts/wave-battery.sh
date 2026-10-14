#!/usr/bin/env bash
# wave-battery.sh — Gate 1 of the two-gate wave model (Bessa 2026-10-14):
# the per-kill LOCAL verification battery. Fast (minutes, warm cache),
# catches what CI catches, keeps every commit individually honest.
#
# Usage: bash scripts/wave-battery.sh [crates...]  (default: all engine+app)
# Gate 2 (the single wave-end CI run) rides the normal push + ci-watch.sh.
set -u
cd "$(dirname "$0")/.."

# Gate 1 must not lie: if cargo cannot execute, every check below would
# silently "pass" (no error lines, no test-result lines). Hard-fail instead.
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
    echo "BATTERY: FAIL — cargo not found on PATH (looked in \$HOME/.cargo/bin too)"
    exit 1
fi

CRATES=("${@:-umber-core umber-color umber-graph umber-spp umber-cli umber-gpu umber-app}")
LOGDIR=$(mktemp -d /tmp/wave-battery.XXXXXX)
# Logs survive a RED battery (the FAIL lines point into them); a GREEN
# battery cleans up after itself.
trap '[ "$FAIL" = 0 ] && rm -rf "$LOGDIR"' EXIT
FAIL=0

echo "=== layers ==="
if python3 scripts/check-layers.py; then
    echo "PASS layers"
else
    echo "FAIL layers (scripts/check-layers.py)"
    FAIL=1
fi

echo "=== fmt ==="
if flock /tmp/umber-cargo.lock cargo fmt --all; then
    echo "PASS fmt (applied)"
else
    echo "FAIL fmt"; FAIL=1
fi

echo "=== clippy -D warnings ==="
for c in "${CRATES[@]}"; do
    LOG="$LOGDIR/clippy-$c.log"
    if flock /tmp/umber-cargo.lock cargo clippy -p "$c" --all-targets -- -D warnings >"$LOG" 2>&1; then
        echo "PASS clippy $c"
    else
        echo "FAIL clippy $c ($(grep -cE '^error' "$LOG") errors — $LOG)"; FAIL=1
    fi
done

echo "=== test suites ==="
for c in "${CRATES[@]}"; do
    LOG="$LOGDIR/test-$c.log"
    flock /tmp/umber-cargo.lock cargo test -p "$c" >"$LOG" 2>&1
    STATUS=$?
    R=$(grep -cE 'test result: .* FAILED\.' "$LOG")
    LINES=$(grep -cE '^test result:' "$LOG")
    if [ "$STATUS" -eq 0 ] && [ "$R" -eq 0 ] && [ "$LINES" -gt 0 ]; then
        T=$(grep -E '^test result: ok' "$LOG" | awk '{s+=$4} END {print s+0}')
        echo "PASS test $c (${T:-?} tests)"
    else
        echo "FAIL test $c (cargo exit $STATUS, $LINES result lines, $R failed lines — $LOG)"; FAIL=1
    fi
done

echo "=== battery: $([ $FAIL = 0 ] && echo GREEN || echo RED) ==="
exit "$FAIL"
