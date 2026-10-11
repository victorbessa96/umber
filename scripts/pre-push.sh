#!/usr/bin/env bash
# pre-push.sh — the local gate that catches the CI-failing classes BEFORE the
# push. Runs on the files a commit touches (vs HEAD~1), zero full-build:
#   fmt drift (auto-fixed in place), brace balance, the trap greps, doc-comment
#   attachment, and clippy's KNOWN recurring classes as static patterns.
# This is the tool that would have saved tonight's 15 CI round-trips.
set -u
cd "$(git rev-parse --show-toplevel 2>/dev/null || echo $HOME/Projects/umber)" || exit 2
fails=0
note() { echo "  PASS: $1"; }
bad()  { echo "  FAIL: $1"; fails=$((fails+1)); }

# The files this commit changed (.rs only for the lint classes).
FILES=$(git diff --name-only HEAD~1 HEAD -- '*.rs' 2>/dev/null; git diff --name-only --cached -- '*.rs' 2>/dev/null | sort -u)
[ -z "$FILES" ] && { echo "PRE-PUSH: no .rs changes to gate"; exit 0; }

echo "== fmt (auto-fix in place, then verify) =="
rustfmt $FILES 2>/dev/null
rustfmt --check $FILES 2>/dev/null && note "fmt canonical" || bad "fmt drift remains"

for f in $FILES; do
  [ -f "$f" ] || continue
  echo "== $f =="

  # Brace balance.
  python3 -c "
src = open('$f').read()
o, c = src.count('{'), src.count('}')
assert o == c, f'unbalanced {o}/{c}'
" && note "braces" || bad "brace imbalance"

  # The known clippy classes as static greps (the classes that burned rounds):
  # 1. thiserror source-field trap
  grep -n 'source:' "$f" | grep -v 'source_id\|source_color\|FillSource\|// \|source: &' | head -1 | grep -q . \
    && bad "thiserror source-field trap" || note "no source-field trap"

  # 2. matches! arity (a third string arg)
  python3 -c "
import re
src = open('$f').read()
hits = re.findall(r'matches!\([^)]*\)\s*,\s*\"', src, re.S)
assert not hits, f'matches!-arity: {hits[:1]}'
" && note "no matches!-arity" || bad "matches!-arity"

  # 3. doc-comment attachment (blank line between /// blocks)
  python3 -c "
lines = open('$f').readlines()
hits = [i+1 for i in range(len(lines)-2)
        if lines[i].startswith('///') and lines[i+1].strip()=='' and lines[i+2].startswith('///')]
assert not hits, f'doc-attachment: {hits[:3]}'
" && note "doc attachment" || bad "doc-comment attachment"

  # 4. negated float comparison (the !(x > 0.0) class) — POSIX ERE
  #    bracket classes don't honor \[ escapes, so the class is kept to
  #    identifiers + fields (the index form !(a[i] > 0) is rare enough
  #    to ride on the other checks).
  grep -nE '!\([a-zA-Z_][a-zA-Z0-9_.]* > ' "$f" | head -1 | grep -q . \
    && bad "negated float comparison" || note "no neg-cmp"

  # 5. manual Default on a derive-able shape (impl Default whose body is pure
  #    field defaults of the SAME struct — heuristic: single Self::Variant line)
  python3 -c "
import re
src = open('$f').read()
manuals = re.findall(r'impl Default for (\w+) \{\s*fn default\(\) -> Self \{\s*Self::(\w+)\s*\}\s*\}', src)
suspects = [(s, v) for s, v in manuals if v in ('Off', 'Paint', 'Lit', 'Uv', 'BuildUp', 'Normal')]
assert not suspects, f'derivable-impl suspects: {suspects}'
" && note "no derivable-impl suspects" || bad "derivable Default impl"
done

echo
[ "$fails" -eq 0 ] && echo "PRE-PUSH: clean — push with confidence" || echo "PRE-PUSH: $fails failure(s) — fix BEFORE pushing"
exit "$fails"
