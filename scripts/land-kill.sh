#!/usr/bin/env bash
# land-kill.sh — the one-call landing ceremony for a finished kill.
# Removes the four-call ritual (durable report copy, queue row, CHANGELOG
# entry, docs commit+push) that the 2026-10-12 misses proved fragile: the
# docs commit forgot the CHANGELOG staging twice.
#
# Usage:
#   bash scripts/land-kill.sh <kill-name> <commit-sha> <queue-row-id> <queue-done-text> <changelog-line> [report-src]
# Example:
#   bash scripts/land-kill.sh stylusxi2 d2fd7b7 7 \
#     "done (d2fd7b7) — XI2 backend + StylusPoller wiring" \
#     "- The XI2 stylus backend — pen pressure reaches the brush on Linux (\`d2fd7b7\`)" \
#     /tmp/umber-stylusxi2-out.jsonl
#
# The queue row is matched by its leading "| <id> |" cell; the CHANGELOG line
# is appended under the newest "### Wave" heading's bullet list (matching the
# file's established shape). Reports land in docs/loop/kill-reports/.
set -eu
cd "$(dirname "$0")/.."

[ $# -ge 5 ] || { echo "land-kill: needs <name> <sha> <row> <row-text> <changelog-line> [report-src]" >&2; exit 2; }
name="$1"; sha="$2"; row="$3"; row_text="$4"; changelog="$5"; report="${6:-}"

echo "== land-kill: $name ($sha) =="

# (1) durable report copy
if [ -n "$report" ] && [ -e "$report" ]; then
  mkdir -p docs/loop/kill-reports
  dest="docs/loop/kill-reports/$(date +%Y-%m-%d)-${name}.json"
  case "$report" in
    *.jsonl) dest="docs/loop/kill-reports/$(date +%Y-%m-%d)-${name}.jsonl" ;;
  esac
  cp "$report" "$dest"
  echo "  report: $dest"
fi

# (2) queue row mark
q=docs/loop/queue.md
python3 - "$q" "$row" "$row_text" << 'EOF'
import re, sys
q, row, text = sys.argv[1], sys.argv[2], sys.argv[3]
s = open(q).read()
# Row shape: "| 7 | Task text | STATUS | notes |". Match the leading id cell
# and the FIRST status cell, whatever bold/format it carries; idempotent on
# re-run (the status is replaced, not duplicated).
pat = re.compile(r"^(\|\s*" + re.escape(row) + r"\s*\|[^|]+\|)([^|]*)(\|.*)$", re.M)
def repl(m):
    status = m.group(2).strip()
    if status and text.startswith(status.strip("*")):
        return m.group(0)  # already marked identically
    return m.group(1) + " **" + text + "** " + m.group(3)
new, n = pat.subn(repl, s, count=1)
if n != 1:
    sys.exit(f"land-kill: queue row |{row}| not matched once")
open(q, "w").write(new)
EOF
echo "  queue row $row marked"

# (3) CHANGELOG append: under the FIRST ### heading inside ## [Unreleased]
# (the current phase's section — its bullet list is where kill lines go)
c=CHANGELOG.md
python3 - "$c" "$changelog" << 'EOF'
import sys
c, line = sys.argv[1], sys.argv[2]
s = open(c).read()
u = s.find("\n## [Unreleased]")
if u == -1:
    sys.exit("land-kill: no [Unreleased] section")
i = s.find("\n### ", u)
if i == -1:
    sys.exit("land-kill: no ### section under [Unreleased]")
j = s.find("\n### ", i + 1)
if j == -1:
    j = s.find("\n## ", i + 1)
if j == -1:
    j = len(s)
# insert after the LAST bullet before j (or at j if the section is text-only)
k = s.rfind("\n- ", i, j)
if k == -1:
    insert_at = j
else:
    # k+1 is the bullet's LINE START; insert after the bullet's line END
    eol = s.find("\n", k + 1)
    insert_at = (eol + 1) if eol != -1 else len(s)
s = s[:insert_at] + line + "\n" + s[insert_at:]
open(c, "w").write(s)
EOF
echo "  CHANGELOG appended"

# (4) commit + push
git add CHANGELOG.md docs/loop/queue.md
[ -n "$report" ] && [ -e "docs/loop/kill-reports/$(date +%Y-%m-%d)-${name}.json" ] && git add "docs/loop/kill-reports/$(date +%Y-%m-%d)-${name}.json"
[ -n "$report" ] && [ -e "docs/loop/kill-reports/$(date +%Y-%m-%d)-${name}.jsonl" ] && git add "docs/loop/kill-reports/$(date +%Y-%m-%d)-${name}.jsonl"
git commit -q -m "docs(loop): worklist #$row marked done — the queue row, the durable kill report, the CHANGELOG entry ($name, $sha)"
git push origin master
echo "  pushed"
echo "land-kill: done"
