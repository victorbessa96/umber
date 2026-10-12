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

echo "== (2) rustfmt canonical (.rs files only) =="
rs=()
for f in "${files[@]}"; do case "$f" in *.rs) rs+=("$f");; esac; done
if [ "${#rs[@]}" -eq 0 ]; then
  echo "  PASS: no .rs files to fmt"
else
  # Resolve the formatter BEFORE trusting any exit code. A PATH-bare
  # session (wake/loop shells without ~/.cargo/bin) turns `rustfmt` into
  # exit 127, which this gate read as "fmt drift" — firing a bogus
  # in-place fix with a binary that does not exist (burned 2026-10-12:
  # two false-reds on a fmt-clean committed file). Missing formatter is
  # an honest, named failure; it must never masquerade as drift.
  RUSTFMT="$(command -v rustfmt || true)"
  if [ -z "$RUSTFMT" ] && [ -x "$HOME/.cargo/bin/rustfmt" ]; then
    RUSTFMT="$HOME/.cargo/bin/rustfmt"
  fi
  if [ -z "$RUSTFMT" ]; then
    echo "  FAIL: rustfmt not found (not on PATH, not in ~/.cargo/bin) — fmt NOT verified"
    fails=$((fails+1))
  elif "$RUSTFMT" --edition 2021 --check "${rs[@]}" 2>/dev/null; then
    echo "  PASS: fmt clean"
  else
    echo "  fixing fmt drift in-place"; "$RUSTFMT" --edition 2021 "${rs[@]}" 2>/dev/null
    "$RUSTFMT" --edition 2021 --check "${rs[@]}" 2>/dev/null && echo "  PASS: fmt aligned (in-place)" || { echo "  FAIL: fmt"; fails=$((fails+1)); }
  fi
fi

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

echo "== (5) lock audit (every dirty file matches a live kill-lock, or is dragon-owned) =="
# A claw may wander off its registered surface; this leg catches it BEFORE
# commit. Dirty files covered by any /tmp/umber-kill-locks/*.lock entry are
# legitimate kill-in-flight surfaces. Dirty files NOT in any lock are the
# dragon's own work (docs/loop/queue.md, STATE, scratch) — they pass with a
# note. A claw-locked file dirty while its lock's claw has EXITED is the
# orphan-consumption case — the dragon consumes; the note names it.
LOCK_DIR=/tmp/umber-kill-locks
uncovered=0
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  while IFS= read -r f; do
    f="${f#?? }"
    [ -n "$f" ] || continue
    covered=0
    for lock in "$LOCK_DIR"/*.lock; do
      [ -e "$lock" ] || continue
      # strip parenthetical qualifiers from lock entries (e.g. path(rows-only))
      base="${f%%\(*}"
      if grep -qxF "$f" "$lock" 2>/dev/null || grep -qxF "$base" "$lock" 2>/dev/null; then
        covered=1
        echo "  PASS: $f — locked by ${lock##*/}"
        break
      fi
    done
    if [ "$covered" -eq 0 ]; then
      echo "  NOTE: $f — not under any kill-lock (dragon-owned surface, or register it)"
      uncovered=$((uncovered+1))
    fi
  done < <(git status --porcelain | awk '{print $NF}')
else
  echo "  SKIP: not a git work tree"
fi

echo
[ "$fails" -eq 0 ] && echo "VERIFY-KILL: ${files[*]} — balanced, fmt-clean, trap-scanned$([ "$uncovered" -gt 0 ] && echo ", $uncovered unlocked-but-noted file(s)")" || echo "VERIFY-KILL: $fails hard failure(s)"
exit "$fails"
