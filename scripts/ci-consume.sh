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
echo "=== the sites ==="
grep -E "\-\->" /tmp/ci-$$.txt | sed 's/.*--> //' | sort -u
rm -f /tmp/ci-$$.txt
