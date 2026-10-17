#!/usr/bin/env bash
# brief-template.sh — the standard kill-brief generator. Writes the JSON brief
# to /tmp from a heredoc body, so dispatching a kill is 2 lines, not a 20-line
# python block. Usage:
#   bash scripts/brief-template.sh <name> <<'EOF'
#   ...the task body (READ:/DELIVERABLE:/State every judgment call.)...
#   EOF
# Then dispatch with the standard one-liner (printed on success).
# NOTE — the two-step form: the nested $(cat | python3) inside $() dies in some
# shell contexts (empty message to opencode, "You must provide a message").
# ALWAYS extract the task to a plain file first, then feed THAT:
#   cat /tmp/umber-<name>-oc.json | python3 -c 'import json,sys; print(json.load(sys.stdin)["task"])' > /tmp/oc-task-<name>.txt
#   opencode run "$(cat /tmp/oc-task-<name>.txt)" > /tmp/umber-<name>-oc-out.json 2>&1
set -u
NAME="${1:?usage: brief-template.sh <kill-name> (task on stdin)}"
BODY="$(cat)"
OUT="/tmp/umber-${NAME}-oc.json"

python3 - "$OUT" "$BODY" <<'PY'
import json, sys
out, body = sys.argv[1], sys.argv[2]
brief = {
    "task": body,
    "context": "Umber Rust DCC. Code-only: no cargo build/test. The dragon gates your files through scripts/verify-kill.sh at consumption. VERIFICATION LADDER (mandatory before you claim done): (1) ~/.cargo/bin/rustfmt --check --edition 2021 on every .rs you touched; (2) resolve EVERY external API you call against the pinned source — ~/.cargo/registry/src/*/ for registry crates, docs/claw-artifacts/ for vendored ones; a guessed API is a fabricated API; (3) for arithmetic-only logic, port the function line-for-line to Python, run your tests' pinned cases against the port, then MUTATE one branch and require the cases to catch it; (4) for any new struct field or enum variant, sweep the workspace for every construction site and exhaustive match — list them, do not guess. State which rungs you ran in the report's Unverified section.",
    "allowedTools": ["read", "edit", "write", "bash", "grep", "glob"],
    "disallowedTools": ["webfetch", "notebook"],
}
with open(out, "w") as f:
    json.dump(brief, f, indent=2)
PY

echo "brief written: $OUT"
echo "dispatch with:"
echo "  cd ~/Projects/umber && opencode run \"\$(cat $OUT | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"task\"])')\" > /tmp/umber-${NAME}-oc-out.json 2>&1; echo ${NAME^^}_OC_EXIT=\$?"
