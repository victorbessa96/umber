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

# The default scope is a real array (one element per crate). The old
# "${@:-a b c}" form made ONE element holding every name, so a bare
# `wave-battery.sh` handed cargo the package "umber-core umber-color ..."
# and failed at clippy before a single crate ran.
if [ "$#" -gt 0 ]; then
    CRATES=("$@")
else
    CRATES=(umber-core umber-color umber-graph umber-spp umber-cli umber-gpu umber-app)
fi
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
# GPU-heavy suites deadlock inside the NVIDIA driver when several test
# threads each hold a wgpu device at once (2026-10-15, RTX 3070, NVIDIA
# 615.71, 16-thread host): umber-paint's lib suite hangs at the default 16
# threads and at 8, passed ONCE at 4 then hung at 4 on repeat, and passes
# every time at 1 (154 tests, ~18 s). The same commit, so not a code
# regression; CI's software adapters never hit it. libtest's default of
# one thread per CPU made the verdict depend on the machine. GPU crates run
# single-threaded (UMBER_GPU_TEST_THREADS overrides), the rest keep
# UMBER_TEST_THREADS (default: libtest's own); every suite is bounded by a
# timeout so a hang is a loud FAIL with a cause, never a battery that never
# returns.
GPU_CRATES=" umber-gpu umber-paint umber-app umber-bake umber-cli umber-gui "
GPU_TEST_THREADS="${UMBER_GPU_TEST_THREADS:-1}"
SUITE_TIMEOUT="${UMBER_SUITE_TIMEOUT:-1200}"
for c in "${CRATES[@]}"; do
    LOG="$LOGDIR/test-$c.log"
    THREADS_ARG=()
    case "$GPU_CRATES" in
        *" $c "*) THREADS_ARG=(-- --test-threads="$GPU_TEST_THREADS") ;;
        *) [ -n "${UMBER_TEST_THREADS:-}" ] && THREADS_ARG=(-- --test-threads="$UMBER_TEST_THREADS") ;;
    esac
    flock /tmp/umber-cargo.lock timeout "$SUITE_TIMEOUT" cargo test -p "$c" "${THREADS_ARG[@]}" >"$LOG" 2>&1
    STATUS=$?
    R=$(grep -cE 'test result: .* FAILED\.' "$LOG")
    LINES=$(grep -cE '^test result:' "$LOG")
    if [ "$STATUS" -eq 124 ]; then
        echo "FAIL test $c (TIMEOUT after ${SUITE_TIMEOUT}s, args ${THREADS_ARG[*]:-default} — $LOG)"; FAIL=1
        continue
    fi
    if [ "$STATUS" -eq 0 ] && [ "$R" -eq 0 ] && [ "$LINES" -gt 0 ]; then
        T=$(grep -E '^test result: ok' "$LOG" | awk '{s+=$4} END {print s+0}')
        echo "PASS test $c (${T:-?} tests)"
    else
        echo "FAIL test $c (cargo exit $STATUS, $LINES result lines, $R failed lines — $LOG)"; FAIL=1
    fi
done

echo "=== battery: $([ $FAIL = 0 ] && echo GREEN || echo RED) ==="
exit "$FAIL"
