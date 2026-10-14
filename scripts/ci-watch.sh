#!/usr/bin/env bash
# ci-watch.sh <run-id> — poll a GitHub Actions run until it completes,
# then print the verdict line. Always run in the BACKGROUND with a
# notify pattern on CI_VERDICT= so the agent's turn never blocks:
#
#   terminal(background=true, notify=["CI_VERDICT="],
#     command="bash scripts/ci-watch.sh 38499753829")
#
# The point: no foreground sleep-polling. The watcher pings between
# turns; the dragon keeps hunting in parallel.
set -u
RUN="${1:?usage: ci-watch.sh <run-id>}"
while true; do
    V=$(gh run view "$RUN" --json status,conclusion \
        --jq '.status + " " + (.conclusion // "pending")' 2>/dev/null || echo "query-failed")
    case "$V" in
        "completed "*) echo "CI_VERDICT=$V"; exit 0 ;;
    esac
    sleep 60
done
