#!/usr/bin/env bash
# kill-worktree.sh — the per-kill git worktree manager (the claw-seat review's
# #6 structural change, 2026-10-12). Each dispatched kill runs in its own
# worktree: the claw gets the whole tree (gates run freely), its files can
# never collide with a sibling's, and landing is a branch the dragon
# fast-forwards or cherry-picks. Kill-locks stay ADVISORY under worktrees —
# used for merge planning, not collision prevention.
#
# Usage:
#   bash scripts/kill-worktree.sh add <kill-name>       # create ../umber-kill-<name> on a branch
#   bash scripts/kill-worktree.sh where <kill-name>     # print the worktree path
#   bash scripts/kill-worktree.sh list                  # live worktrees + branches
#   bash scripts/kill-worktree.sh land <kill-name>      # fast-forward master to the branch + clean up
#   bash scripts/kill-worktree.sh abort <kill-name>     # discard the branch + worktree
#
# The cargo discipline under worktrees: builds share ONE target dir
# (../../umber-shared-target relative to the repo — actually the MAIN repo's
# target/, via CARGO_TARGET_DIR) and EVERY cargo invocation goes through the
# flock mutex at /tmp/umber-cargo.lock so two worktrees never stack builds:
#   flock /tmp/umber-cargo.lock cargo <cmd>
# The dragon's own cargo calls use the same lock.
set -eu
cd "$(dirname "$0")/.."

cmd="${1:?add|where|list|land|abort}"
shift || true

case "$cmd" in
add)
  name="${1:?kill name}"
  path="../umber-kill-$name"
  branch="kill/$name"
  if [ -e "$path" ]; then
    echo "kill-worktree: $path already exists" >&2
    exit 1
  fi
  git worktree add "$path" -b "$branch" master >/dev/null
  echo "worktree: $path (branch $branch, off master $(git rev-parse --short master))"
  echo "dispatch there with: (cd $path && <claw> ...)"
  echo "cargo there must use: flock /tmp/umber-cargo.lock env CARGO_TARGET_DIR=$(pwd)/target cargo <cmd>"
  ;;
where)
  name="${1:?kill name}"
  echo "$(pwd)/../umber-kill-$name"
  ;;
list)
  git worktree list --porcelain | awk '
    /^worktree / { path = $2 }
    /^branch /   { print path "  " $2 }
  '
  ;;
land)
  name="${1:?kill name}"
  path="../umber-kill-$name"
  branch="kill/$name"
  if [ ! -e "$path" ]; then
    echo "kill-worktree: no worktree at $path" >&2
    exit 1
  fi
  # Refuse to land a branch with no commits beyond master.
  ahead=$(git rev-list --count "master..$branch")
  if [ "$ahead" -eq 0 ]; then
    echo "kill-worktree: $branch has no commits — nothing to land (abort instead?)" >&2
    exit 1
  fi
  echo "landing $branch ($ahead commit(s)):"
  git log --oneline "master..$branch" | cat
  # Fast-forward master if the branch is a clean descendant; else refuse
  # (the dragon cherry-picks or rebases by hand — a forced ff hides conflicts).
  if git merge-base --is-ancestor master "$branch"; then
    git checkout master >/dev/null 2>&1 || true
    git merge --ff-only "$branch"
    echo "fast-forwarded master -> $(git rev-parse --short master)"
  else
    echo "kill-worktree: $branch diverged from master — ff refused; land by hand:" >&2
    echo "  git rebase master $branch && git merge --ff-only $branch   # or cherry-pick" >&2
    exit 1
  fi
  git worktree remove "$path"
  git branch -d "$branch"
  echo "worktree removed, branch deleted, master at $(git rev-parse --short master)"
  ;;
abort)
  name="${1:?kill name}"
  path="../umber-kill-$name"
  branch="kill/$name"
  git worktree remove --force "$path" 2>/dev/null || true
  git branch -D "$branch" 2>/dev/null || true
  echo "aborted: worktree + branch gone"
  ;;
esac
