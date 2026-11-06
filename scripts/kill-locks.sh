#!/usr/bin/env bash
# kill-locks.sh — the parallel-kill file-surface lock manager. The dragon
# registers each dispatched kill's exclusive file list; the gate refuses a
# dispatch whose surface intersects a live lock. Locks live in
# /tmp/umber-kill-locks/<name>.lock (one file list per line).
# Usage:
#   bash scripts/kill-locks.sh add <kill-name> <file> [<file> ...]
#   bash scripts/kill-locks.sh check <file> [<file> ...]   -> exit 1 on conflict
#   bash scripts/kill-locks.sh remove <kill-name>
#   bash scripts/kill-locks.sh list
set -u
DIR=/tmp/umber-kill-locks
mkdir -p "$DIR"

cmd="${1:?add|check|remove|list}"
shift

case "$cmd" in
add)
  name="${1:?kill name}"; shift
  [ $# -ge 1 ] || { echo "kill-locks: add needs files" >&2; exit 2; }
  # refuse to add a surface that already intersects a live lock
  for f in "$@"; do
    bash "$0" check "$f" || exit 1
  done
  # Write to a temp file and rename, refusing on ANY write error: a full
  # /tmp (tmpfs, 2026-11-06) once left an EMPTY lock that still printed
  # "locked", so the files it claimed were silently unprotected.
  tmp="$DIR/.$name.lock.$$"
  if ! printf '%s\n' "$@" > "$tmp" || [ "$(wc -l < "$tmp")" -ne $# ]; then
    rm -f "$tmp"
    echo "kill-locks: REFUSED: could not write the lock for $name (disk full? df -h $DIR)" >&2
    exit 3
  fi
  mv -f "$tmp" "$DIR/$name.lock" || { rm -f "$tmp"; echo "kill-locks: REFUSED: rename failed" >&2; exit 3; }
  echo "locked: $name -> $(wc -l < "$DIR/$name.lock") files"
  ;;
check)
  for f in "$@"; do
    for lock in "$DIR"/*.lock; do
      [ -e "$lock" ] || continue
      if grep -qxF "$f" "$lock"; then
        echo "CONFLICT: $f already locked by ${lock##*/}" >&2
        exit 1
      fi
    done
  done
  echo "clear"
  ;;
remove)
  name="${1:?kill name}"
  rm -f "$DIR/$name.lock"
  echo "unlocked: $name"
  ;;
list)
  for lock in "$DIR"/*.lock; do
    [ -e "$lock" ] || continue
    echo "${lock##*/}: $(tr '\n' ' ' < "$lock")"
  done
  ;;
esac
