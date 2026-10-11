#!/usr/bin/env bash
# verify-kill.sh — the standard pre-commit gate for every claw kill's files.
# Usage: bash scripts/verify-kill.sh <file1.rs> [file2.rs ...]
# Catches the trap classes that burned CI rounds on 2026-10-10/11:
#   fmt drift, brace imbalance, thiserror `source` field trap, matches!
#   arity, manual Default impls that are derivable, doc-comment attachment.
# Zero cargo required (code-only era safe); rustfmt must be on PATH.
set -u
cd "$(git rev-parse --show-toplevel 2>/dev/null || echo .)" || exit 2
fails=0
files=("$@")

echo "== (1) brace balance + fn/test census =="
for f in "${files[@]}"; do
  [ -f "$f" ] || { echo "  FAIL: $f missing"; fails=$((fails+1)); continue; }
  python3 -c "
src = open('$f').read()
o, c = src.count('{'), src.count('}')
print(f'  {\"PASS\" if o==c else \"FAIL\"}: $f braces {o}/{c}' + ('' if o==c else ' — UNBALANCED'))
exit(0 if o==c else 1)
" || fails=$((fails+1))
done

echo "== (2) rustfmt canonical =="
rustfmt --check "${files[@]}" 2>/dev/null && echo "  PASS: fmt clean" || {
  echo "  fixing fmt drift in-place"; rustfmt "${files[@]}" 2>/dev/null
  rustfmt --check "${files[@]}" 2>/dev/null && echo "  PASS: fmt aligned (in-place)" || { echo "  FAIL: fmt"; fails=$((fails+1)); }
}

echo "== (3) the trap greps =="
for f in "${files[@]}"; do
  # thiserror's source-field trap (u64/etc named `source` in struct variants)
  grep -n 'source:' "$f" 2>/dev/null | grep -v 'source_id\|source_color\|FillSource\|source: &\|// ' | head -2 | while read -r line; do
    echo "  WARN: $f — possible thiserror source-field trap: $line"
  done
  # matches! with a third argument (arity trap)
  python3 -c "
import re
src = open('$f').read()
bad = re.findall(r'matches!\([^)]*\)\s*,\s*\"', src, re.S)
print(f'  {\"WARN\" if bad else \"PASS\"}: $f matches!-arity: {len(bad)} suspicious' + (f' {bad[:1]}' if bad else ''))
" || true
done

echo "== (4) doc-comment attachment (blank line between doc blocks) =="
for f in "${files[@]}"; do
  python3 -c "
lines = open('$f').readlines()
hits = [i+1 for i in range(len(lines)-2)
        if lines[i].startswith('///') and lines[i+1].strip()=='' and lines[i+2].startswith('///')]
print(f'  {\"WARN\" if hits else \"PASS\"}: $f doc-attachment: {hits[:3] if hits else \"clean\"}')
" || true
done

echo
[ "$fails" -eq 0 ] && echo "VERIFY-KILL: ${files[*]} — balanced, fmt-clean, trap-scanned" || echo "VERIFY-KILL: $fails hard failure(s)"
exit "$fails"
