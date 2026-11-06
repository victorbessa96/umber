#!/usr/bin/env bash
# release-ci.sh: the version-bump CI gate (Bessa 2026-11-06: CI runs when
# preparing a version bump, not on every push).
#
#   bash scripts/release-ci.sh            # gate origin/master's tip
#   bash scripts/release-ci.sh <sha>      # gate a specific pushed commit
#
# Dispatches .github/workflows/ci.yml on master, confirms the run it
# started is for the SHA being gated (a push landing between the check
# and the dispatch would otherwise test the wrong commit), waits for it,
# and prints the verdict per job. Exit 0 = every job green on that SHA;
# only then cut the tag:
#
#   git tag -a vX.Y.Z <sha> -m "Umber X.Y.Z"
#   git push origin vX.Y.Z          # release.yml packages the tag
#
# The bump commit itself goes through dcommit + safe-push like any other.
# Nothing here tags or pushes: tagging is the human's step.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
git fetch -q origin master
tip=$(git rev-parse origin/master)
sha=$(git rev-parse "${1:-$tip}")

if [ "$sha" != "$tip" ]; then
  echo "release-ci: $sha is not origin/master's tip ($tip)." >&2
  echo "  workflow_dispatch runs the branch tip; gate the tip, or push the bump first." >&2
  exit 2
fi

since=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh workflow run ci.yml --ref master
echo "release-ci: dispatched ci.yml on master @ ${sha:0:8}; finding the run"

run=""
for _ in $(seq 1 30); do
  run=$(gh run list --workflow ci.yml --event workflow_dispatch --limit 5 \
    --json databaseId,headSha,createdAt \
    -q "[.[] | select(.headSha == \"$sha\" and .createdAt >= \"$since\")][0].databaseId // empty")
  [ -n "$run" ] && break
  sleep 5
done
if [ -z "$run" ]; then
  echo "release-ci: no dispatched run for ${sha:0:8} appeared (a newer push moved master?)" >&2
  exit 3
fi

echo "release-ci: run $run (https://github.com/$(gh repo view --json nameWithOwner -q .nameWithOwner)/actions/runs/$run)"
status=0
gh run watch "$run" --exit-status --interval 60 > /dev/null || status=$?
gh run view "$run" --json headSha,conclusion,jobs \
  -q '"\(.headSha[0:8]) \(.conclusion)", (.jobs[] | "  \(.name): \(.conclusion)")'
if [ "$status" -ne 0 ]; then
  echo "release-ci: RED, do not tag. Logs: gh run view $run --log-failed" >&2
  exit 1
fi
echo "release-ci: GREEN on ${sha:0:8}; safe to tag."
