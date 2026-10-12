#!/usr/bin/env bash
# ci-consume.sh — pull the failed CI run's error CLASS (not instances) for a head.
# Usage: bash scripts/ci-consume.sh <short-sha>
# Prints: the deduped lint/error lines + their file:line sites, ready to fix.
set -u
SHA="${1:?usage: ci-consume.sh <short-sha>}"
cd "$(git rev-parse --show-toplevel 2>/dev/null || echo $HOME/Projects/umber)" || exit 2

RID=$(gh run list --limit 10 --json databaseId,headSha \
  --jq ".[] | select(.headSha | startswith(\"$SHA\")) | .databaseId" | head -1)
[ -n "$RID" ] || { echo "no run for $SHA"; exit 1; }

gh run view "$RID" --repo victorbessa96/umber --log-failed 2>/dev/null > /tmp/ci-$$.txt
echo "=== run $RID for $SHA — the error classes (deduped) ==="
grep -E "error(:|\[)" /tmp/ci-$$.txt | grep -vE "implied|override|could not compile|Process completed|For more information" | sort -u
echo "=== the failing tests (FAILED + panicked classes) ==="
# Test failures never say "error" — they say FAILED and panicked at.
# Strip the job/step/timestamp prefix and thread pids so sort -u dedups
# across platforms and reruns; the class is the test name + site.
sed -e 's/^[^\t]*\t[^\t]*\t[^ ]* //' -e 's/ ([0-9]*) panicked/ panicked/' /tmp/ci-$$.txt \
  | grep -E "panicked at|^test .*FAILED" \
  | sort -u
echo "=== the sites ==="
grep -E "\-\->" /tmp/ci-$$.txt | sed 's/.*--> //' | sort -u
grep -oE "panicked at [^ ]+:[0-9]+:[0-9]+" /tmp/ci-$$.txt | sed -e 's/panicked at //' -e 's|\\|/|g' | sort -u
rm -f /tmp/ci-$$.txt
