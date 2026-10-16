#!/usr/bin/env bash
# loop-status.sh — ONE call answers the loop wakeup's entire state question.
# Prints: time, tree state, the last commits, CI verdicts, kill reports, live
# claws, and recent file movement. Designed to be the FIRST call of every
# /loop wakeup (fresh state, batched readout).
set -u
cd "$(git rev-parse --show-toplevel 2>/dev/null || echo $HOME/Projects/umber)" || exit 2

echo "=== $(date '+%a %H:%M:%S') ==="
echo "--- tree ---"
echo "HEAD: $(git log --oneline -1)"
dirty=$(git status --porcelain | wc -l)
echo "dirty files: $dirty"
[ "$dirty" -gt 0 ] && git status --porcelain | head -8

echo "--- ci (origin/victorbessa96/umber) ---"
gh run list --limit 3 --json status,conclusion,headSha \
  --jq '.[] | .headSha[:8] + " " + .status + "/" + (.conclusion // "pending")' 2>/dev/null \
  | head -3

echo "--- kills (the /tmp reports) ---"
for f in /tmp/umber-*-oc-out.json /tmp/umber-*-claude-out.json; do
  [ -f "$f" ] || continue
  printf "%s %s\n" "$(basename $f | sed 's/umber-//;s/-oc-out.json//;s/-claude-out.json//')" "$(stat -c '%s bytes, %y' $f | cut -c1-30)"
done | sort

echo "--- claws ---"
echo "opencode kills: $(pgrep -fa 'opencode run' | grep -c task 2>/dev/null || echo 0)"
echo "claude procs:   $(pgrep -c claude 2>/dev/null || echo 0)"

echo "--- movement (files touched in the last 4 min) ---"
find crates assets -newermt "4 minutes ago" \( -name '*.rs' -o -name '*.ftl' -o -name '*.toml' \) 2>/dev/null | head -6

echo "--- build host (192.168.15.64) ---"
if timeout 4 bash -c '</dev/tcp/192.168.15.64/22' 2>/dev/null; then
    echo "host: UP"
    ssh -o BatchMode=yes -o ConnectTimeout=5 -n bessa@192.168.15.64 \
        'pgrep -af "wave-battery|cargo" | grep -v pgrep | head -3; \
         ls -t /tmp/*battery*.log 2>/dev/null | head -2' \
        2>/dev/null | sed 's/^/  /'
else
    echo "host: DOWN (cargo parked — loop-prompt: host-free work only)"
fi
