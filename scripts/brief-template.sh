#!/usr/bin/env bash
# brief-template.sh — the standard kill-brief generator. Writes the JSON brief
# to /tmp from a heredoc body, so dispatching a kill is 2 lines, not a 20-line
# python block. Usage:
#   bash scripts/brief-template.sh <name> <<'EOF'
#   ...the task body (READ:/DELIVERABLE:/State every judgment call.)...
#   EOF
# Then dispatch with the standard one-liner (printed on success).
set -u
NAME="${1:?usage: brief-template.sh <kill-name> (task on stdin)}"
BODY="$(cat)"
OUT="/tmp/umber-${NAME}-oc.json"

python3 - "$OUT" "$BODY" <<'PY'
import json, sys
out, body = sys.argv[1], sys.argv[2]
brief = {
    "task": body,
    "context": "Umber Rust DCC. Code-only: no cargo build/test. The dragon gates your files through scripts/verify-kill.sh at consumption.",
    "allowedTools": ["read", "edit", "write", "bash", "grep", "glob"],
    "disallowedTools": ["webfetch", "notebook"],
}
with open(out, "w") as f:
    json.dump(brief, f, indent=2)
PY

echo "brief written: $OUT"
echo "dispatch with:"
echo "  cd ~/Projects/umber && opencode run \"\$(cat $OUT | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"task\"])')\" > /tmp/umber-${NAME}-oc-out.json 2>&1; echo ${NAME^^}_OC_EXIT=\$?"
